//! Lark-app onboarding API.
//!
//! Lark credentials are registered in the console config under
//! `[lark-apps.<name>]` and referenced by Apps via `lark_app = "<name>"`. These
//! handlers let the console manage that registry: list it (secrets redacted),
//! upsert an entry (gated on a **live** credential test against Lark), and
//! delete one. Edits go through `toml_edit` so the rest of the config — comments
//! and all — survives untouched, then broadcast via the control plane so every
//! App referencing the changed entry restarts.

use axum::Json;
use axum::extract::{Path as AxPath, State};
use larkstack_core::{LarkRegistry, default_base_url};
use serde::{Deserialize, Serialize};
use serde_json::json;
use toml_edit::{DocumentMut, Item, Table, value};
use tracing::info;
use utoipa::ToSchema;

use crate::HostState;

use super::{ApiError, OkResponse, parse_doc, write_doc};

/// `GET /api/lark-apps` — the registry, `app_secret` redacted to a boolean.
#[utoipa::path(
    get, path = "/lark-apps", tag = "console",
    security(("session" = [])),
    responses((status = 200, description = "Registered Lark apps (secrets redacted)", body = LarkAppsResponse)),
)]
pub(crate) async fn list(State(s): State<HostState>) -> Json<LarkAppsResponse> {
    let registry = parse_registry(&s.control.config());
    let mut lark_apps: Vec<LarkAppView> = registry
        .iter()
        .map(|(name, app)| LarkAppView {
            name: name.to_string(),
            app_id: app.app_id.clone(),
            base_url: app.base_url.clone(),
            has_secret: !app.app_secret.is_empty(),
        })
        .collect();
    lark_apps.sort_by(|a, b| a.name.cmp(&b.name));
    Json(LarkAppsResponse { lark_apps })
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct UpsertReq {
    name: String,
    app_id: String,
    app_secret: String,
    base_url: Option<String>,
}

/// `POST /api/lark-apps` — live-test the credentials, then write the entry.
/// Nothing is persisted unless the test passes.
#[utoipa::path(
    post, path = "/lark-apps", tag = "console",
    security(("session" = [])),
    request_body = UpsertReq,
    responses(
        (status = 200, description = "Credentials valid and saved", body = UpsertAck),
        (status = 400, description = "Invalid input or failed credential test", body = super::ErrorResponse),
        (status = 500, description = "Could not write the config file", body = super::ErrorResponse),
    ),
)]
pub(crate) async fn upsert(
    State(s): State<HostState>,
    Json(req): Json<UpsertReq>,
) -> Result<Json<UpsertAck>, ApiError> {
    if !valid_name(&req.name) {
        return Err(ApiError::bad_request(
            "name must be non-empty and use only [A-Za-z0-9_-]",
        ));
    }
    if req.app_id.is_empty() || req.app_secret.is_empty() {
        return Err(ApiError::bad_request("app_id and app_secret are required"));
    }
    let base_url = normalize_base_url(req.base_url);

    if let Err(e) = verify(&s.http, &req.app_id, &req.app_secret, &base_url).await {
        return Err(ApiError::bad_request(format!(
            "credential test failed: {e}"
        )));
    }

    let mut doc = parse_doc(&s.control.config()).map_err(ApiError::internal)?;
    upsert_entry(&mut doc, &req.name, &req.app_id, &req.app_secret, &base_url);
    write_doc(&s, doc).map_err(ApiError::internal)?;
    info!(lark_app = %req.name, "lark-app credentials saved");
    Ok(Json(UpsertAck {
        ok: true,
        name: req.name,
    }))
}

/// `DELETE /api/lark-apps/{name}` — drop an entry. Apps still referencing it
/// will fail their next build (surfaced as Errored in the console).
#[utoipa::path(
    delete, path = "/lark-apps/{name}", tag = "console",
    security(("session" = [])),
    params(("name" = String, Path, description = "Registry entry name")),
    responses(
        (status = 200, description = "Entry removed", body = OkResponse),
        (status = 404, description = "No such entry", body = super::ErrorResponse),
        (status = 500, description = "Could not write the config file", body = super::ErrorResponse),
    ),
)]
pub(crate) async fn delete(
    State(s): State<HostState>,
    AxPath(name): AxPath<String>,
) -> Result<Json<OkResponse>, ApiError> {
    let mut doc = parse_doc(&s.control.config()).map_err(ApiError::internal)?;
    if !remove_entry(&mut doc, &name) {
        return Err(ApiError::not_found(format!("no lark-app '{name}'")));
    }
    write_doc(&s, doc).map_err(ApiError::internal)?;
    info!(lark_app = %name, "lark-app deleted");
    Ok(Json(OkResponse::ok()))
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct TestReq {
    app_id: String,
    app_secret: String,
    base_url: Option<String>,
}

/// `POST /api/lark-apps/test` — dry-run a credential check without saving.
/// Always `200`; the body's `ok` flag carries the verdict.
#[utoipa::path(
    post, path = "/lark-apps/test", tag = "console",
    security(("session" = [])),
    request_body = TestReq,
    responses((status = 200, description = "Credential-test verdict", body = TestResult)),
)]
pub(crate) async fn test(State(s): State<HostState>, Json(req): Json<TestReq>) -> Json<TestResult> {
    if req.app_id.is_empty() || req.app_secret.is_empty() {
        return Json(TestResult::err("app_id and app_secret are required"));
    }
    let base_url = normalize_base_url(req.base_url);
    match verify(&s.http, &req.app_id, &req.app_secret, &base_url).await {
        Ok(expire) => Json(TestResult::ok(expire)),
        Err(e) => Json(TestResult::err(e)),
    }
}

// ---- response bodies -------------------------------------------------------

/// Body of `GET /api/lark-apps`.
#[derive(Serialize, ToSchema)]
pub(crate) struct LarkAppsResponse {
    lark_apps: Vec<LarkAppView>,
}

/// One registry entry, with the secret redacted to a flag.
#[derive(Serialize, ToSchema)]
pub(crate) struct LarkAppView {
    name: String,
    app_id: String,
    base_url: String,
    /// Whether an `app_secret` is stored (the secret itself is never returned).
    has_secret: bool,
}

/// Body of a successful `POST /api/lark-apps`.
#[derive(Serialize, ToSchema)]
pub(crate) struct UpsertAck {
    ok: bool,
    name: String,
}

/// Body of `POST /api/lark-apps/test`: `ok` plus either the token lifetime or
/// the failure reason.
#[derive(Serialize, ToSchema)]
pub(crate) struct TestResult {
    ok: bool,
    /// Token lifetime in seconds, on success.
    #[serde(skip_serializing_if = "Option::is_none")]
    expire: Option<u64>,
    /// Failure reason, on error.
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl TestResult {
    fn ok(expire: u64) -> Self {
        Self {
            ok: true,
            expire: Some(expire),
            error: None,
        }
    }
    fn err(error: impl Into<String>) -> Self {
        Self {
            ok: false,
            expire: None,
            error: Some(error.into()),
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(s: &str) -> DocumentMut {
        s.parse().expect("valid toml")
    }

    #[test]
    fn upsert_writes_nested_header_and_keeps_comments() {
        let mut d = doc("# keep me\n[standup]\nenabled = true\n");
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
