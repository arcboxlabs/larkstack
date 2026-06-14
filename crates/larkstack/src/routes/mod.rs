//! The console HTTP surface: the gated admin API, the OAuth endpoints, the
//! generated OpenAPI spec + Scalar docs, and the SPA fallback.
//!
//! [`build`] wires every route into one [`axum::Router`]. Route registration
//! and OpenAPI collection are unified through [`utoipa_axum::router::OpenApiRouter`],
//! so the spec can never drift from the routes that actually exist.

use axum::{
    Json, Router,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Serialize;
use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityScheme};
use utoipa::{Modify, OpenApi, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};
use utoipa_scalar::{Scalar, Servable};

use crate::HostState;

pub(crate) mod actions;
pub(crate) mod config;
pub(crate) mod events;
pub(crate) mod lark_apps;
pub(crate) mod oauth;
pub(crate) mod status;

/// Generic `{ "ok": true }` success body.
#[derive(Serialize, ToSchema)]
pub(crate) struct OkResponse {
    ok: bool,
}

impl OkResponse {
    pub(crate) fn ok() -> Self {
        Self { ok: true }
    }
}

/// Error body shared by every non-2xx JSON response.
#[derive(Serialize, ToSchema)]
pub(crate) struct ErrorResponse {
    error: String,
}

/// A typed API error: a status plus a message, rendered as [`ErrorResponse`].
pub(crate) struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }
    pub(crate) fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }
    pub(crate) fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, message)
    }
    pub(crate) fn unavailable(message: impl Into<String>) -> Self {
        Self::new(StatusCode::SERVICE_UNAVAILABLE, message)
    }
    pub(crate) fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorResponse {
                error: self.message,
            }),
        )
            .into_response()
    }
}

/// `GET /api/health` — ungated liveness probe.
#[utoipa::path(
    get, path = "/health", tag = "console",
    responses((status = 200, description = "Service is up", body = String)),
)]
pub(crate) async fn health() -> &'static str {
    "ok"
}

/// OpenAPI document root. Paths + component schemas are collected from the
/// [`OpenApiRouter`] in [`build`]; this only carries the metadata and the
/// session security scheme.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "larkstack console API",
        description = "Admin API for the larkstack host: subsystem status, live config, \
                       an event stream (SSE), the Lark-app registry, action dispatch, and \
                       Lark OAuth login.",
        version = env!("CARGO_PKG_VERSION"),
    ),
    modifiers(&SessionScheme),
    tags(
        (name = "console", description = "Admin API — session-gated once OAuth is configured"),
        (name = "auth", description = "Lark OAuth login flow"),
    ),
)]
struct ApiDoc;

/// Declares the signed `lk_session` cookie as the `session` security scheme.
struct SessionScheme;

impl Modify for SessionScheme {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "session",
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::new("lk_session"))),
        );
    }
}

/// Assemble the full console router: gated `/api/*`, ungated `/auth/*`, the
/// OpenAPI spec + Scalar docs, and the embedded SPA fallback.
///
/// `app_routers` are per-App admin routers (each self-stated); they are mounted
/// at `/api/apps/<name>/` and inherit the same session gate. App routes are not
/// part of the OpenAPI spec.
pub(crate) fn build(state: HostState, app_routers: Vec<(String, Router)>) -> Router {
    let gate = axum::middleware::from_fn_with_state(state.clone(), oauth::require_session);

    // `/api/*` — session-gated, except `/api/health` (added after `route_layer`).
    let mut api = OpenApiRouter::new()
        .routes(routes!(status::status))
        .routes(routes!(status::apps))
        .routes(routes!(config::get_config, config::put_config))
        .routes(routes!(events::events))
        .routes(routes!(lark_apps::list, lark_apps::upsert))
        .routes(routes!(lark_apps::test))
        .routes(routes!(lark_apps::delete))
        .routes(routes!(actions::dispatch));
    for (name, router) in app_routers {
        api = api.nest_service(&format!("/apps/{name}"), router);
    }
    let api = api.route_layer(gate).routes(routes!(health));

    // `/auth/*` — ungated.
    let auth = OpenApiRouter::new()
        .routes(routes!(oauth::login))
        .routes(routes!(oauth::callback))
        .routes(routes!(oauth::logout))
        .routes(routes!(oauth::me));

    let (router, spec) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .nest("/api", api)
        .nest("/auth", auth)
        .split_for_parts();

    let spec_json = spec.to_pretty_json().expect("OpenAPI spec serializes");
    let docs: Router<HostState> = Scalar::with_url("/api/docs", spec).into();

    router
        .route(
            "/api/openapi.json",
            get(move || {
                let spec = spec_json.clone();
                async move { ([(header::CONTENT_TYPE, "application/json")], spec) }
            }),
        )
        .merge(docs)
        .fallback(crate::assets::serve)
        .with_state(state)
}
