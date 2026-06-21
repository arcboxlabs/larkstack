//! The public inbound router (webhook + Lark preview callback), mounted by the
//! host at `/webhooks/linear/`.

use axum::{Router, routing::post};
use lark_kit::StateSlot;

use crate::config::AppState;

mod preview;
mod webhook;

/// Build the Linear ingress router. It reads its live [`AppState`] from `slot`
/// per request via the [`lark_kit::Live`] extractor (`503` while the app is
/// stopped), so the once-mounted router tracks config reloads.
pub fn ingress_router(slot: StateSlot<AppState>) -> Router {
    Router::new()
        .route("/webhook", post(webhook::webhook_handler))
        .route("/lark/event", post(preview::lark_event_handler))
        .with_state(slot)
}

#[cfg(test)]
mod test_support {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use hmac::{Hmac, Mac};
    use lark_kit::Live;
    use larkstack_core::StateStore;
    use sea_orm::{Database, DatabaseConnection};
    use sha2::Sha256;

    use crate::config::AppState;

    const WEBHOOK_SECRET: &str = "test-linear-secret";

    #[derive(Default)]
    struct MemoryStore {
        values: Mutex<HashMap<(String, String), String>>,
    }

    #[async_trait]
    impl StateStore for MemoryStore {
        async fn get(&self, namespace: &str, key: &str) -> anyhow::Result<Option<String>> {
            Ok(self
                .values
                .lock()
                .expect("memory store mutex")
                .get(&(namespace.to_string(), key.to_string()))
                .cloned())
        }

        async fn set(&self, namespace: &str, key: &str, value: &str) -> anyhow::Result<()> {
            self.values
                .lock()
                .expect("memory store mutex")
                .insert((namespace.to_string(), key.to_string()), value.to_string());
            Ok(())
        }

        async fn delete(&self, namespace: &str, key: &str) -> anyhow::Result<()> {
            self.values
                .lock()
                .expect("memory store mutex")
                .remove(&(namespace.to_string(), key.to_string()));
            Ok(())
        }
    }

    pub(super) async fn live_state() -> Live<AppState> {
        let db = in_memory_db().await;
        let store: Arc<dyn StateStore> = Arc::new(MemoryStore::default());
        let state = AppState::from_toml(
            r#"
            [linear]
            webhook_secret = "test-linear-secret"
            debounce_delay_ms = 60000
            "#,
            store,
            db,
        )
        .expect("test config builds");
        Live(Arc::new(state))
    }

    async fn in_memory_db() -> DatabaseConnection {
        Database::connect("sqlite::memory:")
            .await
            .expect("in-memory db opens")
    }

    pub(super) fn linear_signature(payload: &str) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(WEBHOOK_SECRET.as_bytes())
            .expect("HMAC accepts any key length");
        mac.update(payload.as_bytes());
        hex::encode(mac.finalize().into_bytes())
    }
}
