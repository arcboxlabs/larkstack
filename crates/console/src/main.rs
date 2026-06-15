//! larkstack-console: registers the bundled apps and runs the framework host.

use larkstack::Larkstack;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    Larkstack::new()
        .register(linear::app())
        .register(github::app())
        .register(gitlab::app())
        .register(x::app())
        .register(minutes::app())
        .register(standup::app())
        .run()
        .await
}
