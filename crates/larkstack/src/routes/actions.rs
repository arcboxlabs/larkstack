//! Console-dispatched App actions.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use larkstack_core::{ActionEnvelope, DispatchError};
use serde::Serialize;
use utoipa::ToSchema;

use crate::HostState;

use super::ApiError;

/// `POST /api/actions/{app}/{action}` — fire-and-forget an App action. The
/// result string from `Instance::handle_action` surfaces on the event stream,
/// not in this response.
#[utoipa::path(
    post, path = "/actions/{app}/{action}", tag = "console",
    security(("session" = [])),
    params(
        ("app" = String, Path, description = "Registered app name"),
        ("action" = String, Path, description = "Action name from the app's manifest"),
    ),
    request_body(content = Object, description = "Optional action parameters (JSON object)"),
    responses(
        (status = 202, description = "Action dispatched", body = ActionAck),
        (status = 404, description = "Unknown app", body = super::ErrorResponse),
        (status = 503, description = "App is not running", body = super::ErrorResponse),
    ),
)]
pub(crate) async fn dispatch(
    State(s): State<HostState>,
    Path((app, action)): Path<(String, String)>,
    body: Option<Json<serde_json::Value>>,
) -> Result<(StatusCode, Json<ActionAck>), ApiError> {
    let params = body.map(|j| j.0).unwrap_or(serde_json::Value::Null);
    let envelope = ActionEnvelope {
        name: action.clone(),
        params,
    };
    match s.control.dispatch(&app, envelope).await {
        Ok(()) => Ok((
            StatusCode::ACCEPTED,
            Json(ActionAck {
                ok: true,
                app,
                action,
            }),
        )),
        Err(DispatchError::UnknownSubsystem) => {
            Err(ApiError::not_found(format!("unknown app '{app}'")))
        }
        Err(DispatchError::Closed) => {
            Err(ApiError::unavailable(format!("app '{app}' is not running")))
        }
    }
}

/// Acknowledgement that an action was queued for the running App.
#[derive(Serialize, ToSchema)]
pub(crate) struct ActionAck {
    ok: bool,
    app: String,
    action: String,
}
