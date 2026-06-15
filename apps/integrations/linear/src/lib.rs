//! Linear webhook → Lark notification integration.
//!
//! Layout follows the data path: [`routes`] is the inbound HTTP surface,
//! [`domain`] the normalized core, [`source`] the Linear adapter (webhook
//! payloads + GraphQL API), and [`lark`] the Lark adapter (cards + sink).
//! [`db`] holds the app's persistence (sea-orm entities + migrations).

pub mod config;
pub mod db;

mod actions;
mod app;
mod domain;
mod lark;
mod routes;
mod scheduler;
mod source;

pub use app::app;
