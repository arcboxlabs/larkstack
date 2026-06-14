//! Coalesces rapid-fire updates on the same issue into a single notification.

use std::collections::HashMap;

use tokio::sync::{Mutex, oneshot};

use super::IssueNotification;

/// A pending notification waiting for the debounce window to expire.
pub struct PendingUpdate {
    /// The latest issue state (merged on every new update).
    pub notif: IssueNotification,
    /// Email to DM if any update in the window changed the assignee.
    pub dm_email: Option<String>,
    /// The most recent triggering user's id (to exclude from subscriber fan-out).
    pub actor_id: Option<String>,
    /// Send on this to cancel the currently-scheduled timer task.
    cancel_tx: oneshot::Sender<()>,
}

/// Thread-safe map of issue ids to their pending debounced updates.
pub struct DebounceMap(Mutex<HashMap<String, PendingUpdate>>);

impl Default for DebounceMap {
    fn default() -> Self {
        Self::new()
    }
}

impl DebounceMap {
    pub fn new() -> Self {
        Self(Mutex::new(HashMap::new()))
    }

    /// Inserts or merges an update for `key`. When an entry already exists the
    /// old timer is cancelled, change descriptions are merged (deduplicating
    /// exact matches), and a create-then-update stays a create.
    ///
    /// Returns a [`oneshot::Receiver`] the caller should `select!` against a
    /// sleep — if it fires, a newer update has taken over.
    pub async fn upsert(
        &self,
        key: String,
        mut notif: IssueNotification,
        dm_email: Option<String>,
        actor_id: Option<String>,
    ) -> oneshot::Receiver<()> {
        let mut map = self.0.lock().await;

        let (dm_email, actor_id) = if let Some(existing) = map.remove(&key) {
            let _ = existing.cancel_tx.send(());

            // Accumulate change descriptions; skip exact duplicates.
            let mut changes = existing.notif.changes;
            for c in notif.changes {
                if !changes.contains(&c) {
                    changes.push(c);
                }
            }
            notif.changes = changes;
            // A create followed by updates is still a "create"; a status change
            // anywhere in the window counts for subscriber fan-out.
            notif.is_create = existing.notif.is_create || notif.is_create;
            notif.status_changed = existing.notif.status_changed || notif.status_changed;

            (
                dm_email.or(existing.dm_email),
                actor_id.or(existing.actor_id),
            )
        } else {
            (dm_email, actor_id)
        };

        let (cancel_tx, cancel_rx) = oneshot::channel();
        map.insert(
            key,
            PendingUpdate {
                notif,
                dm_email,
                actor_id,
                cancel_tx,
            },
        );
        cancel_rx
    }

    /// Removes and returns the pending update for `key`, if any.
    pub async fn take(&self, key: &str) -> Option<PendingUpdate> {
        self.0.lock().await.remove(key)
    }
}
