//! The console admin API for routing config, mounted by the host under
//! `/api/apps/<app>/` (behind the session gate). Each integration exposes it from
//! `App::routes` via [`RoutingApi`], bound to its own [`StateStore`] namespace.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use larkstack_core::StateStore;

use super::Config;

/// The routing admin API for one App, bound to its [`StateStore`] namespace.
///
/// Construct with [`RoutingApi::new`] and mount [`RoutingApi::router`] from `App::routes`:
///
/// ```ignore
/// fn routes(&self, services: &AppServices) -> Option<axum::Router> {
///     Some(RoutingApi::new(services.state.clone(), "github").router())
/// }
/// ```
#[derive(Clone)]
pub struct RoutingApi {
    store: Arc<dyn StateStore>,
    namespace: &'static str,
}

impl RoutingApi {
    /// Bind the API to `store` under `namespace` (the App's name).
    pub fn new(store: Arc<dyn StateStore>, namespace: &'static str) -> Self {
        Self { store, namespace }
    }

    /// The axum router: `GET`/`PUT /routing`, stated on `self`.
    pub fn router(self) -> Router {
        Router::new()
            .route("/routing", get(Self::get).put(Self::put))
            .with_state(self)
    }

    /// The current config (defaults when unset).
    async fn current(&self) -> Config {
        Config::load(&self.store, self.namespace).await
    }

    /// Validate and persist a new config, returning it on success.
    async fn replace(&self, config: Config) -> Result<Config, RoutingError> {
        config.validate().map_err(RoutingError::Invalid)?;
        Config::save(&self.store, self.namespace, &config)
            .await
            .map_err(RoutingError::Store)?;
        Ok(config)
    }

    async fn get(State(api): State<RoutingApi>) -> Json<Config> {
        Json(api.current().await)
    }

    async fn put(
        State(api): State<RoutingApi>,
        Json(config): Json<Config>,
    ) -> Result<Json<Config>, RoutingError> {
        api.replace(config).await.map(Json)
    }
}

/// An error from the routing admin API, mapped to an HTTP response via [`IntoResponse`].
enum RoutingError {
    /// The submitted config failed validation → `400`.
    Invalid(String),
    /// Persisting the config failed → `500`.
    Store(anyhow::Error),
}

impl IntoResponse for RoutingError {
    fn into_response(self) -> Response {
        match self {
            RoutingError::Invalid(msg) => (StatusCode::BAD_REQUEST, msg).into_response(),
            RoutingError::Store(e) => {
                (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
            }
        }
    }
}
