//! Per-App lifecycle supervisor.
//!
//! One task per registered App owns its full lifecycle: enable/disable via the
//! `[name].enabled` config flag (default off), build, run + concurrent action
//! dispatch, status reporting, and crash/backoff restart. Only a change to the
//! App's own config section restarts it — editing one app never bounces another.

use std::sync::Arc;
use std::time::Duration;

use larkstack_core::{ActionEnvelope, App, ControlPlane, Instance};
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

/// Spawn the supervising task for one registered App.
pub(crate) fn supervise(control: ControlPlane, app: Arc<dyn App>) {
    let name = app.manifest().name;
    let handle = control.handle(&name);
    let mut config_rx = control.watch_config();

    tokio::spawn(async move {
        let mut backoff = Backoff::new();
        loop {
            let config = (*config_rx.borrow_and_update()).clone();
            let section = section_value(&config, &name);

            if !is_enabled(&section) {
                handle.stopped().await;
                if config_rx.changed().await.is_err() {
                    break;
                }
                continue;
            }

            handle.starting().await;
            let instance = match app.build(&config).await {
                Ok(inst) => inst,
                Err(e) => {
                    warn!(app = %name, "build failed: {e:#}");
                    handle.errored(format!("build: {e:#}")).await;
                    if wait_or_shutdown(&mut config_rx, backoff.next()).await {
                        break;
                    }
                    continue;
                }
            };
            backoff.reset();
            handle.running().await;

            let action_rx = control.register_actions(&name).await;
            match run_generation(instance, &name, action_rx, &mut config_rx, &section).await {
                Outcome::Shutdown => break,
                Outcome::Restart => info!(app = %name, "config changed; restarting"),
                Outcome::Crashed(msg) => {
                    warn!(app = %name, "{msg}");
                    handle.errored(msg).await;
                    if wait_or_shutdown(&mut config_rx, backoff.next()).await {
                        break;
                    }
                }
            }
        }
    });
}

enum Outcome {
    /// The config channel closed — the host is shutting down.
    Shutdown,
    /// This app's config section changed — rebuild.
    Restart,
    /// `Instance::run` returned or panicked while still enabled.
    Crashed(String),
}

/// Drive `Instance::run` and the action loop concurrently until the run loop
/// ends or this app's config section changes. On a clean stop, cooperatively
/// cancel and await the run loop's graceful shutdown.
async fn run_generation(
    instance: Arc<dyn Instance>,
    name: &str,
    action_rx: mpsc::UnboundedReceiver<ActionEnvelope>,
    config_rx: &mut watch::Receiver<Arc<String>>,
    section: &Option<toml::Value>,
) -> Outcome {
    let cancel = CancellationToken::new();
    let action_task = tokio::spawn(action_loop(instance.clone(), action_rx, name.to_string()));
    let mut run_task = tokio::spawn({
        let instance = instance.clone();
        let cancel = cancel.clone();
        async move { instance.run(cancel).await }
    });

    let outcome = loop {
        tokio::select! {
            res = &mut run_task => {
                break match res {
                    Ok(Ok(())) => Outcome::Crashed("run loop exited unexpectedly".into()),
                    Ok(Err(e)) => Outcome::Crashed(format!("{e:#}")),
                    Err(e) => Outcome::Crashed(format!("panicked: {e}")),
                };
            }
            changed = config_rx.changed() => {
                if changed.is_err() {
                    break Outcome::Shutdown;
                }
                let current = (*config_rx.borrow()).clone();
                if &section_value(&current, name) != section {
                    break Outcome::Restart;
                }
            }
        }
    };

    action_task.abort();
    if !matches!(outcome, Outcome::Crashed(_)) {
        cancel.cancel();
        let _ = run_task.await;
    }
    outcome
}

/// Drain dispatched actions, calling the instance and surfacing each result to
/// the event stream (attributed to the app via the `app` field).
async fn action_loop(
    instance: Arc<dyn Instance>,
    mut rx: mpsc::UnboundedReceiver<ActionEnvelope>,
    name: String,
) {
    while let Some(env) = rx.recv().await {
        match instance.handle_action(&env.name, env.params).await {
            Ok(msg) => info!(app = %name, action = %env.name, "{msg}"),
            Err(e) => warn!(app = %name, action = %env.name, "{e:#}"),
        }
    }
}

/// Extract an app's top-level config section as a value for the enabled check
/// and change detection.
fn section_value(full_toml: &str, name: &str) -> Option<toml::Value> {
    toml::from_str::<toml::Value>(full_toml)
        .ok()
        .and_then(|v| v.as_table().and_then(|t| t.get(name).cloned()))
}

fn is_enabled(section: &Option<toml::Value>) -> bool {
    section
        .as_ref()
        .and_then(|s| s.get("enabled"))
        .and_then(|e| e.as_bool())
        .unwrap_or(false)
}

/// Sleep for `delay`, waking early on a config change. Returns `true` if the
/// config channel closed (host shutting down).
async fn wait_or_shutdown(config_rx: &mut watch::Receiver<Arc<String>>, delay: Duration) -> bool {
    tokio::select! {
        _ = tokio::time::sleep(delay) => false,
        changed = config_rx.changed() => changed.is_err(),
    }
}

/// Exponential backoff for crash/build-error restarts, capped at 60s.
struct Backoff {
    current: Duration,
}

impl Backoff {
    fn new() -> Self {
        Self {
            current: Duration::from_secs(1),
        }
    }

    fn reset(&mut self) {
        self.current = Duration::from_secs(1);
    }

    fn next(&mut self) -> Duration {
        let delay = self.current;
        self.current = (self.current * 2).min(Duration::from_secs(60));
        delay
    }
}
