//! Lark-app onboarding API.
//!
//! Lark credentials are registered in the console config under
//! `[lark-apps.<name>]` and referenced by Apps via `lark_app = "<name>"`. These
//! handlers let the console manage that registry: list it (secrets redacted),
//! upsert an entry (gated on a **live** credential test against Lark), and
//! delete one. Edits go through `toml_edit` so the rest of the config — comments
//! and all — survives untouched, then broadcast via the control plane so every
//! App referencing the changed entry restarts.

use std::sync::Arc;

use axum::extract::{Path as AxPath, State};
use axum::http::StatusCode;
use axum::{Json, response::IntoResponse};
use larkstack_core::{LarkRegistry, default_base_url};
use serde::Deserialize;
use serde_json::{Value, json};
use toml_edit::{DocumentMut, Item, Table, value};
use tracing::info;

use crate::HostState;

/// `GET /api/lark-apps` — the registry, `app_secret` redacted to a boolean.
pub(crate) async fn list(State(s): State<HostState>) -> impl IntoResponse {
    let registry = parse_registry(&s.control.config());
    let mut items: Vec<Value> = registry
        .iter()
        .map(|(name, app)| {
            json!({
                "name": name,
                "app_id": app.app_id,
                "base_url": app.base_url,
                "has_secret": !app.app_secret.is_empty(),
            })
        })
        .collect();
    items.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    (StatusCode::OK, Json(json!({ "lark_apps": items })))
}

#[derive(Deserialize)]
pub(crate) struct UpsertReq {
    name: String,
    app_id: String,
    app_secret: String,
    base_url: Option<String>,
}

/// `POST /api/lark-apps` — live-test the credentials, then write the entry.
/// Nothing is persisted unless the test passes.
pub(crate) async fn upsert(
    State(s): State<HostState>,
    Json(req): Json<UpsertReq>,
) -> impl IntoResponse {
    if !valid_name(&req.name) {
        return bad("name must be non-empty and use only [A-Za-z0-9_-]");
    }
    if req.app_id.is_empty() || req.app_secret.is_empty() {
        return bad("app_id and app_secret are required");
    }
    let base_url = normalize_base_url(req.base_url);

    if let Err(e) = verify(&s.http, &req.app_id, &req.app_secret, &base_url).await {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("credential test failed: {e}") })),
        );
    }

    let mut doc = match parse_doc(&s.control.config()) {
        Ok(d) => d,
        Err(e) => return server_error(e),
    };
    upsert_entry(&mut doc, &req.name, &req.app_id, &req.app_secret, &base_url);
    match write_config(&s, doc) {
        Ok(()) => {
            info!(lark_app = %req.name, "lark-app credentials saved");
            (
                StatusCode::OK,
                Json(json!({ "ok": true, "name": req.name })),
            )
        }
        Err(e) => server_error(e),
    }
}

/// `DELETE /api/lark-apps/{name}` — drop an entry. Apps still referencing it
/// will fail their next build (surfaced as Errored in the console).
pub(crate) async fn delete(
    State(s): State<HostState>,
    AxPath(name): AxPath<String>,
) -> impl IntoResponse {
    let mut doc = match parse_doc(&s.control.config()) {
        Ok(d) => d,
        Err(e) => return server_error(e),
    };
    if !remove_entry(&mut doc, &name) {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("no lark-app '{name}'") })),
        );
    }
    match write_config(&s, doc) {
        Ok(()) => {
            info!(lark_app = %name, "lark-app deleted");
            (StatusCode::OK, Json(json!({ "ok": true })))
        }
        Err(e) => server_error(e),
    }
}

#[derive(Deserialize)]
pub(crate) struct TestReq {
    app_id: String,
    app_secret: String,
    base_url: Option<String>,
}

/// `POST /api/lark-apps/test` — dry-run a credential check without saving.
/// Always `200`; the body's `ok` flag carries the verdict.
pub(crate) async fn test(
    State(s): State<HostState>,
    Json(req): Json<TestReq>,
) -> impl IntoResponse {
    if req.app_id.is_empty() || req.app_secret.is_empty() {
        return Json(json!({ "ok": false, "error": "app_id and app_secret are required" }));
    }
    let base_url = normalize_base_url(req.base_url);
    match verify(&s.http, &req.app_id, &req.app_secret, &base_url).await {
        Ok(expire) => Json(json!({ "ok": true, "expire": expire })),
        Err(e) => Json(json!({ "ok": false, "error": e })),
    }
}

/// Mint a `tenant_access_token` to prove the credentials work. Returns the
/// token's lifetime in seconds on success, or Lark's error message.
async fn verify(
    http: &reqwest::Client,
    app_id: &str,
    app_secret: &str,
    base_url: &str,
) -> Result<u64, String> {
    #[derive(Deserialize)]
    struct TokenResp {
        code: i64,
        #[serde(default)]
        msg: String,
        #[serde(default)]
        expire: u64,
    }

    let url = format!(
        "{}/open-apis/auth/v3/tenant_access_token/internal",
        base_url.trim_end_matches('/')
    );
    let resp = http
        .post(&url)
        .json(&json!({ "app_id": app_id, "app_secret": app_secret }))
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;
    let body: TokenResp = resp
        .json()
        .await
        .map_err(|e| format!("unexpected response: {e}"))?;
    if body.code != 0 {
        return Err(if body.msg.is_empty() {
            format!("Lark error code {}", body.code)
        } else {
            format!("{} (code {})", body.msg, body.code)
        });
    }
    Ok(body.expire)
}

/// Deserialize just the `[lark-apps]` table from the config; absent or malformed
/// → an empty registry (the GET handler must never fail on a stray edit).
fn parse_registry(toml_str: &str) -> LarkRegistry {
    #[derive(Deserialize, Default)]
    struct View {
        #[serde(rename = "lark-apps", default)]
        lark_apps: LarkRegistry,
    }
    toml::from_str::<View>(toml_str)
        .map(|v| v.lark_apps)
        .unwrap_or_default()
}

fn parse_doc(toml_str: &str) -> Result<DocumentMut, String> {
    toml_str
        .parse::<DocumentMut>()
        .map_err(|e| format!("config is not valid TOML: {e}"))
}

fn upsert_entry(doc: &mut DocumentMut, name: &str, app_id: &str, app_secret: &str, base_url: &str) {
    let apps = doc
        .entry("lark-apps")
        .or_insert(Item::Table(Table::new()))
        .as_table_mut()
        .expect("lark-apps is a table");
    // Render only the `[lark-apps.<name>]` sub-headers, not a bare `[lark-apps]`.
    apps.set_implicit(true);

    let mut entry = Table::new();
    entry["app_id"] = value(app_id);
    entry["app_secret"] = value(app_secret);
    entry["base_url"] = value(base_url);
    apps.insert(name, Item::Table(entry));
}

/// Remove `[lark-apps.<name>]`, dropping the parent table if it becomes empty.
/// Returns whether the entry existed.
fn remove_entry(doc: &mut DocumentMut, name: &str) -> bool {
    let Some(apps) = doc.get_mut("lark-apps").and_then(Item::as_table_mut) else {
        return false;
    };
    let existed = apps.remove(name).is_some();
    if apps.is_empty() {
        doc.remove("lark-apps");
    }
    existed
}

fn write_config(s: &HostState, doc: DocumentMut) -> Result<(), String> {
    let body = doc.to_string();
    std::fs::write(s.config_path.as_path(), &body)
        .map_err(|e| format!("write {}: {e}", s.config_path.display()))?;
    s.control.set_config(Arc::new(body));
    Ok(())
}

fn normalize_base_url(raw: Option<String>) -> String {
    raw.filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(default_base_url)
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn bad(msg: &str) -> (StatusCode, Json<Value>) {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": msg })))
}

fn server_error(msg: String) -> (StatusCode, Json<Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": msg })),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(s: &str) -> DocumentMut {
        s.parse().expect("valid toml")
    }

    #[test]
    fn upsert_writes_nested_header_and_keeps_comments() {
        let mut d = doc("# keep me\n[standup-bot]\nenabled = true\n");
        upsert_entry(&mut d, "main", "cli_x", "sec", "https://open.larksuite.com");
        let out = d.to_string();
        assert!(out.contains("# keep me"), "comment preserved: {out}");
        assert!(out.contains("[lark-apps.main]"), "nested header: {out}");
        assert!(
            !out.contains("\n[lark-apps]\n"),
            "no bare parent header: {out}"
        );
        // Round-trips through the serde reader the GET handler / apps use.
        let app = parse_registry(&out).get("main").cloned().expect("present");
        assert_eq!(app.app_id, "cli_x");
        assert_eq!(app.app_secret, "sec");
    }

    #[test]
    fn upsert_replaces_existing_entry_in_place() {
        let mut d = doc("[lark-apps.main]\napp_id = \"old\"\napp_secret = \"o\"\n");
        upsert_entry(&mut d, "main", "new", "n", "https://open.feishu.cn");
        let app = parse_registry(&d.to_string()).get("main").cloned().unwrap();
        assert_eq!(app.app_id, "new");
        assert_eq!(app.base_url, "https://open.feishu.cn");
    }

    #[test]
    fn remove_drops_empty_parent_table() {
        let mut d = doc("[lark-apps.main]\napp_id = \"x\"\napp_secret = \"y\"\n");
        assert!(remove_entry(&mut d, "main"));
        assert!(!d.to_string().contains("lark-apps"));
        // Removing again reports "not found".
        assert!(!remove_entry(&mut d, "main"));
    }

    #[test]
    fn remove_keeps_sibling_entries() {
        let mut d = doc("[lark-apps.a]\napp_id = \"1\"\napp_secret = \"x\"\n\n\
             [lark-apps.b]\napp_id = \"2\"\napp_secret = \"y\"\n");
        assert!(remove_entry(&mut d, "a"));
        let reg = parse_registry(&d.to_string());
        assert!(reg.get("a").is_none());
        assert!(reg.get("b").is_some());
    }

    #[test]
    fn name_validation() {
        assert!(valid_name("main"));
        assert!(valid_name("team-1_x"));
        assert!(!valid_name(""));
        assert!(!valid_name("has space"));
        assert!(!valid_name("dot.ted"));
    }
}
