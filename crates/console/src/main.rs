//! larkstack-console: registers the bundled apps and runs the framework host.

use larkstack::Larkstack;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    Larkstack::new()
        .register(linear_bridge::app())
        .register(meeting_digest::app())
        .register(standup_bot::app())
        .run()
        .await
}
