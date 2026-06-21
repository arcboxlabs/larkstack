use std::sync::Arc;

use larkstack_core::StateStore;
use tracing::warn;

use super::model::{Config, KEY};

impl Config {
    /// Load the config from `store` under `namespace`, falling back to defaults when absent
    /// or unparseable (the integration degrades to "no routing" rather than failing).
    pub async fn load(store: &Arc<dyn StateStore>, namespace: &str) -> Config {
        match store.get(namespace, KEY).await {
            Ok(Some(json)) => serde_json::from_str(&json).unwrap_or_else(|e| {
                warn!("routing config parse failed for '{namespace}', using empty: {e}");
                Config::default()
            }),
            Ok(None) => Config::default(),
            Err(e) => {
                warn!("routing config load failed for '{namespace}', using empty: {e}");
                Config::default()
            }
        }
    }

    /// Persist the (already-[validated](Config::validate)) config.
    pub async fn save(
        store: &Arc<dyn StateStore>,
        namespace: &str,
        config: &Config,
    ) -> anyhow::Result<()> {
        let json = serde_json::to_string(config)?;
        store.set(namespace, KEY, &json).await
    }
}
