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
use serde::Serialize;

use super::{Config, RoutingSpec};
use crate::{Live, StateSlot, bot::LarkBotClient};

/// The routing admin API for one App, bound to its [`StateStore`] namespace.
///
/// Construct with [`RoutingApi::new`] and mount [`RoutingApi::router`] from `App::routes`:
///
/// ```ignore
/// fn routes(&self, services: &AppServices) -> Option<axum::Router> {
///     Some(RoutingApi::new(services.state.clone(), ROUTING_SPEC).router(self.bot_slot.clone()))
/// }
/// ```
#[derive(Clone)]
pub struct RoutingApi {
    store: Arc<dyn StateStore>,
    spec: RoutingSpec,
}

impl RoutingApi {
    /// Bind the API to `store` using the App's routing spec.
    pub fn new(store: Arc<dyn StateStore>, spec: RoutingSpec) -> Self {
        Self { store, spec }
    }

    /// The axum router: `GET /routing/spec`, `GET`/`PUT /routing` (config) +
    /// `GET /chats` (the bot's group chats, for the console chat-picker) +
    /// `GET /users` (the users the bot can DM, for the user-picker). `bots` is
    /// the App's live-bot slot — `/chats` and `/users` read it via [`Live`] and
    /// return `503` while the App is stopped or has no bot.
    pub fn router(self, bots: StateSlot<LarkBotClient>) -> Router {
        let config = Router::new()
            .route("/routing/spec", get(Self::spec))
            .route("/routing", get(Self::get).put(Self::put))
            .with_state(self);
        let live = Router::new()
            .route("/chats", get(list_chats))
            .route("/users", get(list_users))
            .with_state(bots);
        config.merge(live)
    }

    /// The current config (defaults when unset).
    async fn current(&self) -> Config {
        Config::load(&self.store, self.spec.namespace).await
    }

    /// Validate and persist a new config, returning it on success.
    async fn replace(&self, config: Config) -> Result<Config, RoutingError> {
        config
            .validate_for(&self.spec)
            .map_err(RoutingError::Invalid)?;
        Config::save(&self.store, self.spec.namespace, &config)
            .await
            .map_err(RoutingError::Store)?;
        Ok(config)
    }

    async fn spec(State(api): State<RoutingApi>) -> Json<RoutingSpec> {
        Json(api.spec)
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

/// A chat the bot belongs to, as offered to the console picker.
#[derive(Serialize)]
struct ChatInfo {
    chat_id: String,
    name: String,
}

async fn list_chats(Live(bot): Live<LarkBotClient>) -> Result<Json<Vec<ChatInfo>>, RoutingError> {
    let chats = bot.list_chats().await.map_err(RoutingError::Bot)?;
    let out = chats
        .into_iter()
        .map(|c| ChatInfo {
            chat_id: c.chat_id,
            name: c.name,
        })
        .collect();
    Ok(Json(out))
}

/// A user the bot can DM, as offered to the console picker.
#[derive(Serialize)]
struct UserInfo {
    open_id: String,
    name: String,
}

async fn list_users(Live(bot): Live<LarkBotClient>) -> Result<Json<Vec<UserInfo>>, RoutingError> {
    let users = bot.list_users().await.map_err(RoutingError::Bot)?;
    let out = users
        .into_iter()
        .map(|u| UserInfo {
            open_id: u.open_id,
            name: u.name,
        })
        .collect();
    Ok(Json(out))
}

/// An error from the routing admin API, mapped to an HTTP response via [`IntoResponse`].
enum RoutingError {
    /// The submitted config failed validation → `400`.
    Invalid(String),
    /// Persisting the config failed → `500`.
    Store(anyhow::Error),
    /// A Lark API call (e.g. listing chats) failed → `502`.
    Bot(String),
}

impl IntoResponse for RoutingError {
    fn into_response(self) -> Response {
        match self {
            RoutingError::Invalid(msg) => (StatusCode::BAD_REQUEST, msg).into_response(),
            RoutingError::Store(e) => {
                (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
            }
            RoutingError::Bot(msg) => (StatusCode::BAD_GATEWAY, msg).into_response(),
        }
    }
}
