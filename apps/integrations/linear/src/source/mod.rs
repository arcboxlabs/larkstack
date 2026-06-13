//! Linear webhook source — receives issue/comment events and normalizes them.

pub mod client;
mod handler;
pub mod models;
mod utils;

pub use handler::webhook_handler;
