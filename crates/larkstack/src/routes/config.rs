//! Read + replace the console `config.toml`.

use std::sync::Arc;

use axum::{
    Json,
    extract::{Path as AxPath, State},
    http::{StatusCode, header},
};
use serde::{Deserialize, Serialize};
use toml_edit::{Item, Table, value};
use tracing::info;
use utoipa::ToSchema;

use crate::HostState;

use super::{ApiError, OkResponse, parse_doc, write_doc};

/// `GET /api/config` — the current `config.toml`, verbatim.
#[utoipa::path(
    get, path = "/config", tag = "console",
    security(("session" = [])),
    responses((status = 200, description = "Current config.toml", content_type = "application/toml", body = String)),
)]
pub(crate) async fn get_config(
    State(s): State<HostState>,
) -> (StatusCode, [(header::HeaderName, &'static str); 1], String) {
    let toml = (*s.control.config()).clone();
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/toml; charset=utf-8")],
        toml,
    )
}

/// `PUT /api/config` — validate, persist, and broadcast a replacement config.
/// Only the apps whose change key moved are restarted.
#[utoipa::path(
    put, path = "/config", tag = "console",
    security(("session" = [])),
    request_body(content = String, content_type = "application/toml", description = "Full replacement config.toml"),
    responses(
        (status = 200, description = "Saved; affected apps restart", body = OkResponse),
        (status = 400, description = "Body is not valid TOML", body = super::ErrorResponse),
        (status = 500, description = "Could not write the config file", body = super::ErrorResponse),
    ),
)]
pub(crate) async fn put_config(
    State(s): State<HostState>,
    body: String,
) -> Result<Json<OkResponse>, ApiError> {
    if let Err(e) = toml::from_str::<toml::Value>(&body) {
        return Err(ApiError::bad_request(format!("invalid TOML: {e}")));
    }
    if let Err(e) = std::fs::write(s.config_path.as_path(), &body) {
        return Err(ApiError::internal(format!(
            "write {}: {e}",
            s.config_path.display()
        )));
    }
    s.control.set_config(Arc::new(body));
    info!("config updated; affected apps will restart");
    Ok(Json(OkResponse::ok()))
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct EnabledReq {
    enabled: bool,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct EnabledAck {
    ok: bool,
    name: String,
    enabled: bool,
}

/// `PUT /api/config/{app}/enabled` — flip whether the supervisor runs an app,
/// writing just `[<app>].enabled` and leaving the rest of the config (comments
/// and all) untouched. A one-field affordance over the raw TOML editor for the
/// most common config edit; the affected app (re)starts or stops on broadcast.
#[utoipa::path(
    put, path = "/config/{app}/enabled", tag = "console",
    security(("session" = [])),
    params(("app" = String, Path, description = "Registered app name")),
    request_body = EnabledReq,
    responses(
        (status = 200, description = "Flag set; the app starts or stops", body = EnabledAck),
        (status = 404, description = "No such registered app", body = super::ErrorResponse),
        (status = 400, description = "That config section is not a table", body = super::ErrorResponse),
        (status = 500, description = "Could not write the config file", body = super::ErrorResponse),
    ),
)]
pub(crate) async fn set_app_enabled(
    State(s): State<HostState>,
    AxPath(app): AxPath<String>,
    Json(req): Json<EnabledReq>,
) -> Result<Json<EnabledAck>, ApiError> {
    if !s.manifests.iter().any(|m| m.name == app) {
        return Err(ApiError::not_found(format!("no registered app '{app}'")));
    }
    let mut doc = parse_doc(&s.control.config()).map_err(ApiError::internal)?;
    let table = doc
        .entry(app.as_str())
        .or_insert(Item::Table(Table::new()))
        .as_table_mut()
        .ok_or_else(|| ApiError::bad_request(format!("[{app}] is not a table")))?;
    table["enabled"] = value(req.enabled);
    write_doc(&s, doc).map_err(ApiError::internal)?;
    info!(app = %app, enabled = req.enabled, "app enabled flag set");
    Ok(Json(EnabledAck {
        ok: true,
        name: app,
        enabled: req.enabled,
    }))
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct LarkAppReq {
    /// The `[lark-apps.<name>]` to bind for this app's Lark credentials, or
    /// `null`/empty to unbind (falling back to env / inline `[<app>.lark]`).
    #[serde(default)]
    lark_app: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct LarkAppAck {
    ok: bool,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    lark_app: Option<String>,
}

/// `PUT /api/config/{app}/lark-app` — bind (or clear) the `[lark-apps.<name>]`
/// an app uses for Lark credentials, writing just `[<app>].lark_app` and leaving
/// the rest of the config untouched. A one-field affordance over the raw TOML
/// editor; the affected app restarts on broadcast and resolves the new binding.
/// A non-empty value must name a registered Lark app, else the app would fail
/// its next build — so we reject it up front.
#[utoipa::path(
    put, path = "/config/{app}/lark-app", tag = "console",
    security(("session" = [])),
    params(("app" = String, Path, description = "Registered app name")),
    request_body = LarkAppReq,
    responses(
        (status = 200, description = "Binding set (or cleared); the app restarts", body = LarkAppAck),
        (status = 404, description = "No such registered app", body = super::ErrorResponse),
        (status = 400, description = "lark_app is not a registered Lark app, or the section is not a table", body = super::ErrorResponse),
        (status = 500, description = "Could not write the config file", body = super::ErrorResponse),
    ),
)]
pub(crate) async fn set_app_lark_app(
    State(s): State<HostState>,
    AxPath(app): AxPath<String>,
    Json(req): Json<LarkAppReq>,
) -> Result<Json<LarkAppAck>, ApiError> {
    if !s.manifests.iter().any(|m| m.name == app) {
        return Err(ApiError::not_found(format!("no registered app '{app}'")));
    }
    let binding = req
        .lark_app
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let cfg = s.control.config();
    if let Some(name) = binding
        && !registry_has(&cfg, name)
    {
        return Err(ApiError::bad_request(format!(
            "lark_app '{name}' is not a registered Lark app — register it in the Lark Apps tab first"
        )));
    }

    let mut doc = parse_doc(&cfg).map_err(ApiError::internal)?;
    let table = doc
        .entry(app.as_str())
        .or_insert(Item::Table(Table::new()))
        .as_table_mut()
        .ok_or_else(|| ApiError::bad_request(format!("[{app}] is not a table")))?;
    match binding {
        Some(name) => table["lark_app"] = value(name),
        None => {
            table.remove("lark_app");
        }
    }
    write_doc(&s, doc).map_err(ApiError::internal)?;
    info!(app = %app, lark_app = ?binding, "app lark_app binding set");
    Ok(Json(LarkAppAck {
        ok: true,
        name: app,
        lark_app: binding.map(str::to_string),
    }))
}

/// Whether `[lark-apps.<name>]` exists in the config (the bind target must be
/// registered before an app can resolve it).
fn registry_has(config: &str, name: &str) -> bool {
    toml::from_str::<toml::Value>(config)
        .ok()
        .and_then(|v| Some(v.get("lark-apps")?.get(name).is_some()))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    const REGISTRY: &str = "[lark-apps.main]\napp_id = \"cli_x\"\napp_secret = \"s\"\n";

    #[test]
    fn registry_has_finds_registered_entry() {
        assert!(registry_has(REGISTRY, "main"));
        assert!(!registry_has(REGISTRY, "other"));
        assert!(!registry_has("[github]\nenabled = true\n", "main"));
        // A malformed config never panics and reports "absent".
        assert!(!registry_has("not = valid = toml", "main"));
    }

    #[test]
    fn binding_write_sets_then_clears_lark_app() {
        let mut doc = parse_doc(REGISTRY).unwrap();
        let table = doc
            .entry("github")
            .or_insert(Item::Table(Table::new()))
            .as_table_mut()
            .unwrap();
        table["lark_app"] = value("main");
        let out = doc.to_string();
        assert!(out.contains("[github]"), "section written: {out}");
        assert!(
            out.contains("lark_app = \"main\""),
            "binding written: {out}"
        );
        assert!(
            out.contains("[lark-apps.main]"),
            "registry preserved: {out}"
        );

        // Clearing removes just the key, leaving the section (and registry) intact.
        let mut doc2 = parse_doc(&out).unwrap();
        doc2.get_mut("github")
            .and_then(Item::as_table_mut)
            .unwrap()
            .remove("lark_app");
        let out2 = doc2.to_string();
        assert!(!out2.contains("lark_app"), "binding cleared: {out2}");
        assert!(out2.contains("[lark-apps.main]"), "registry intact: {out2}");
    }
}
