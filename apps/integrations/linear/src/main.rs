use std::path::PathBuf;
use std::sync::Arc;

use larkstack_core::{ControlPlane, db};
use linear::config::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    // Standalone runs without the host, so it opens the shared App database and
    // applies its own migrations itself (the host does this for the console).
    let data_dir =
        PathBuf::from(std::env::var("CONSOLE_DATA_DIR").unwrap_or_else(|_| "data".to_string()));
    std::fs::create_dir_all(&data_dir)?;
    let db = db::open(data_dir.join("apps.db")).await?;
    db::run_migrations(&db, "linear", linear::user_map::migrations()).await?;

    let state = Arc::new(AppState::from_env(db));
    let plane = ControlPlane::new();
    let handle = plane.handle("linear");
    linear::run(state, handle).await
}
