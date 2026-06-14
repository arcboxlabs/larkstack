//! Per-App lifecycle supervisor.
//!
//! One task per registered App owns its full lifecycle: enable/disable via the
//! `[name].enabled` config flag (default off), build, run + concurrent action
//! dispatch, status reporting, and crash/backoff restart. An App restarts only
//! when its [`ChangeKey`] changes — its own config section plus the
//! `[lark-apps.<name>]` entry it references — so editing one App, or the shared
//! Lark credentials it binds to, never bounces an unrelated App.

use std::sync::Arc;
use std::time::Duration;

use larkstack_core::{ActionEnvelope, App, AppServices, ControlPlane, Instance};
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

/// Spawn the supervising task for one registered App.
pub(crate) fn supervise(control: ControlPlane, app: Arc<dyn App>, services: AppServices) {
    let name = app.manifest().name;
    let handle = control.handle(&name);
    let mut config_rx = control.watch_config();

    tokio::spawn(async move {
        let mut backoff = Backoff::new();
        loop {
            let config = (*config_rx.borrow_and_update()).clone();
            let key = ChangeKey::compute(&config, &name);

            if !key.enabled() {
                handle.stopped().await;
                if config_rx.changed().await.is_err() {
                    break;
                }
                continue;
            }

            handle.starting().await;
            let instance = match app.build(&config, services.clone()).await {
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
            match run_generation(instance, &name, action_rx, &mut config_rx, &key).await {
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
    key: &ChangeKey,
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
                if ChangeKey::compute(&current, name) != *key {
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

/// What a config change is diffed against for one App. Holds the App's own
/// `[name]` section plus the `[lark-apps.<ref>]` entry it binds to, so the
/// supervisor restarts the App when either changes — and only then.
#[derive(Debug, PartialEq)]
struct ChangeKey {
    section: Option<toml::Value>,
    lark_app: Option<toml::Value>,
}

impl ChangeKey {
    fn compute(full_toml: &str, name: &str) -> Self {
        let root = toml::from_str::<toml::Value>(full_toml).ok();
        let section = root
            .as_ref()
            .and_then(|v| v.as_table())
            .and_then(|t| t.get(name).cloned());
        let lark_app = section
            .as_ref()
            .and_then(|s| s.get("lark_app"))
            .and_then(|v| v.as_str())
            .and_then(|reference| {
                root.as_ref()
                    .and_then(|v| v.get("lark-apps"))
                    .and_then(|apps| apps.get(reference).cloned())
            });
        Self { section, lark_app }
    }

    fn enabled(&self) -> bool {
        self.section
            .as_ref()
            .and_then(|s| s.get("enabled"))
            .and_then(|e| e.as_bool())
            .unwrap_or(false)
    }
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

#[cfg(test)]
mod tests {
    use super::ChangeKey;

    const CFG: &str = r#"
[lark-apps.main]
app_id = "a"
app_secret = "s"

[lark-apps.other]
app_id = "b"
app_secret = "t"

[standup]
enabled = true
lark_app = "main"
"#;

    #[test]
    fn enabled_reflects_section_flag() {
        assert!(ChangeKey::compute(CFG, "standup").enabled());
        // A section that is absent (or has no `enabled`) is disabled.
        assert!(!ChangeKey::compute(CFG, "minutes").enabled());
    }

    #[test]
    fn editing_the_referenced_lark_app_flips_the_key() {
        let base = ChangeKey::compute(CFG, "standup");
        let edited = CFG.replace(r#"app_secret = "s""#, r#"app_secret = "rotated""#);
        assert_ne!(ChangeKey::compute(&edited, "standup"), base);
    }

    #[test]
    fn editing_an_unreferenced_lark_app_leaves_the_key() {
        // standup binds to `main`, not `other` — touching `other` must not
        // bounce it.
        let base = ChangeKey::compute(CFG, "standup");
        let edited = CFG.replace(r#"app_secret = "t""#, r#"app_secret = "rotated""#);
        assert_eq!(ChangeKey::compute(&edited, "standup"), base);
    }

    #[test]
    fn editing_own_section_flips_the_key() {
        let base = ChangeKey::compute(CFG, "standup");
        let edited = CFG.replace("lark_app = \"main\"", "lark_app = \"other\"");
        assert_ne!(ChangeKey::compute(&edited, "standup"), base);
    }
}
