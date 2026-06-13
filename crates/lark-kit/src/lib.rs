//! Shared toolkit for larkstack Lark integration apps.
//!
//! An integration App (linear, github, x, …) owns its source and its cards;
//! everything Lark-facing and source-agnostic lives here: the card builders,
//! the group-webhook sender and DM bot, the inbound HTTP server, the per-app
//! [`LarkConfig`]/[`ServerConfig`], the event-callback scaffold ([`event`]),
//! and shared crypto/text [`utils`].

pub mod card;
pub mod config;
pub mod event;
pub mod server;
pub mod utils;

mod bot;
mod webhook;

pub use bot::LarkBotClient;
pub use config::{LarkConfig, ServerConfig, TomlLark};
pub use utils::{truncate, verify_hmac_sha256};
pub use webhook::send_lark_card;
