//! GitHub webhook source — receives PR, issue, CI, and security-alert events
//! and posts Lark cards.

mod handler;
mod utils;

pub use handler::webhook_handler;
