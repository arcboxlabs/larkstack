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
