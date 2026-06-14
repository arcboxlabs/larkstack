//! Admin CRUD for the user-map table. Mounted by the host at
//! `/api/apps/linear/` behind the console session gate, so these endpoints are
//! `GET/POST /api/apps/linear/user-map` and
//! `DELETE /api/apps/linear/user-map/{linear_email}`.

use std::fmt::Display;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::get,
};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection, EntityTrait};
use serde::{Deserialize, Serialize};

use super::entity::{self, Entity};

/// One mapping row as returned to the admin UI.
#[derive(Serialize)]
struct Mapping {
    linear_email: String,
    lark_email: String,
    lark_open_id: Option<String>,
    note: Option<String>,
    updated_by: Option<String>,
    updated_at: i64,
}

impl From<entity::Model> for Mapping {
    fn from(m: entity::Model) -> Self {
        Self {
            linear_email: m.linear_email,
            lark_email: m.lark_email,
            lark_open_id: m.lark_open_id,
            note: m.note,
            updated_by: m.updated_by,
            updated_at: m.updated_at,
        }
    }
}

/// Create/replace payload. `linear_email` is the key.
#[derive(Deserialize)]
struct Upsert {
    linear_email: String,
    lark_email: String,
    #[serde(default)]
    lark_open_id: Option<String>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    updated_by: Option<String>,
}

/// The admin router, stated on the shared App database.
pub fn router(db: DatabaseConnection) -> Router {
    Router::new()
        .route("/user-map", get(list).post(upsert))
        .route("/user-map/{linear_email}", axum::routing::delete(remove))
        .with_state(db)
}

async fn list(
    State(db): State<DatabaseConnection>,
) -> Result<Json<Vec<Mapping>>, (StatusCode, String)> {
    let rows = Entity::find().all(&db).await.map_err(internal)?;
    Ok(Json(rows.into_iter().map(Mapping::from).collect()))
}

async fn upsert(
    State(db): State<DatabaseConnection>,
    Json(body): Json<Upsert>,
) -> Result<Json<Mapping>, (StatusCode, String)> {
    if body.linear_email.trim().is_empty() || body.lark_email.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "linear_email and lark_email are required".into(),
        ));
    }

    let am = entity::ActiveModel {
        linear_email: Set(body.linear_email.clone()),
        lark_email: Set(body.lark_email),
        lark_open_id: Set(body.lark_open_id),
        note: Set(body.note),
        updated_by: Set(body.updated_by),
        updated_at: Set(now_ms()),
    };

    let exists = Entity::find_by_id(&body.linear_email)
        .one(&db)
        .await
        .map_err(internal)?
        .is_some();
    let model = if exists {
        am.update(&db).await.map_err(internal)?
    } else {
        am.insert(&db).await.map_err(internal)?
    };
    Ok(Json(Mapping::from(model)))
}

async fn remove(
    State(db): State<DatabaseConnection>,
    Path(linear_email): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let res = Entity::delete_by_id(linear_email)
        .exec(&db)
        .await
        .map_err(internal)?;
    if res.rows_affected == 0 {
        Err((StatusCode::NOT_FOUND, "no such mapping".into()))
    } else {
        Ok(StatusCode::NO_CONTENT)
    }
}

fn internal(e: impl Display) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
