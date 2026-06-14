//! Linear webhook → Lark notification integration.
//!
//! Layout follows the data path: [`routes`] is the inbound HTTP surface,
//! [`domain`] the normalized core, [`source`] the Linear adapter (webhook
//! payloads + GraphQL API), and [`lark`] the Lark adapter (cards + sink).

pub mod config;
pub mod user_map;

mod actions;
mod app;
mod domain;
mod lark;
mod routes;
mod source;

pub use app::app;
pub use routes::run;
