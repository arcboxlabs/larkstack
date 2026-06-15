//! A live state cell for host-mounted ingress routers.
//!
//! An integration's webhook router is mounted once on the console port at
//! startup (via [`larkstack_core::App::ingress_routes`]), but its handler state —
//! the config-built `AppState` — is rebuilt by the supervisor on every config
//! reload. A [`StateSlot`] bridges the two: the App descriptor owns the slot and
//! hands clones to both the router (read side) and each running instance (write
//! side). The instance publishes its `AppState` on start and clears it on stop
//! through a [`SlotGuard`], so the once-mounted router always sees live state —
//! or `None` (→ `503`) while the app is disabled, mid-reload, or failed to build.

use std::ops::Deref;
use std::sync::Arc;

use arc_swap::ArcSwapOption;
use axum::extract::{FromRef, FromRequestParts};
use axum::http::StatusCode;
use axum::http::request::Parts;

/// A shareable, atomically-swappable cell holding the current `Arc<T>` (or none).
pub type StateSlot<T> = Arc<ArcSwapOption<T>>;

/// Creates an empty [`StateSlot`] — the initial state before any instance builds.
pub fn slot<T>() -> StateSlot<T> {
    Arc::new(ArcSwapOption::empty())
}

/// Empties a [`StateSlot`] on drop. Held inside `Instance::run` so the slot
/// clears on cooperative shutdown *or* crash — whichever ends the run — before
/// the supervisor builds the next generation.
pub struct SlotGuard<T>(StateSlot<T>);

impl<T> SlotGuard<T> {
    /// Publishes `state` into `slot` and returns a guard that clears it on drop.
    pub fn publish(slot: StateSlot<T>, state: Arc<T>) -> Self {
        slot.store(Some(state));
        Self(slot)
    }
}

impl<T> Drop for SlotGuard<T> {
    fn drop(&mut self) {
        self.0.store(None);
    }
}

/// Extractor for the live `Arc<T>` published into a [`StateSlot`] router state.
///
/// Rejects with `503 Service Unavailable` when the slot is empty (the app is
/// disabled, mid-reload, or failed to build). Destructure as `Live(state)` to
/// recover the `Arc<T>` a handler would otherwise get from `State<Arc<T>>`.
pub struct Live<T>(pub Arc<T>);

impl<T> Deref for Live<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<S, T> FromRequestParts<S> for Live<T>
where
    StateSlot<T>: FromRef<S>,
    S: Send + Sync,
    T: Send + Sync + 'static,
{
    type Rejection = StatusCode;

    async fn from_request_parts(_parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        StateSlot::<T>::from_ref(state)
            .load_full()
            .map(Live)
            .ok_or(StatusCode::SERVICE_UNAVAILABLE)
    }
}
