//! GitLab webhook source — receives merge-request, issue, pipeline, note, and
//! push events and posts Lark cards.

mod handler;
pub mod payload;
mod verify;

pub use handler::webhook_handler;
