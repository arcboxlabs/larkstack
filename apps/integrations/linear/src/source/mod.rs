//! Linear adapter — the external system this app bridges from.
//!
//! [`payload`] deserializes inbound webhook events, [`changes`] diffs an
//! updated issue against its previous state, and [`api`] queries Linear's
//! GraphQL API for link previews.

pub mod api;
pub mod changes;
pub mod payload;
