//! Read + replace the console `config.toml`.

use std::sync::Arc;

use axum::{
    Json,
    extract::State,
    http::{StatusCode, header},
};
use tracing::info;

use crate::HostState;

use super::{ApiError, OkResponse};

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
