//! The inbound webhook server, shared by every integration's `run::serve`.

use anyhow::Context;
use axum::Router;
use tokio_util::sync::CancellationToken;
use tracing::info;

/// Binds `0.0.0.0:{port}` and serves `router` until `cancel` fires
/// (cooperative shutdown). `name` is used only for the startup log line.
pub async fn serve(
    name: &str,
    router: Router,
    port: u16,
    cancel: CancellationToken,
) -> anyhow::Result<()> {
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("bind {addr}"))?;
    info!("{name} listening on {addr}");
    axum::serve(listener, router)
        .with_graceful_shutdown(async move { cancel.cancelled().await })
        .await?;
    Ok(())
}
