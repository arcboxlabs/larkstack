use std::sync::Arc;

use github::config::AppState;
use larkstack_core::ControlPlane;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let state = Arc::new(AppState::from_env());
    let plane = ControlPlane::new();
    let handle = plane.handle("github");
    github::run(state, handle).await
}
