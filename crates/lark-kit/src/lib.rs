//! Shared toolkit for larkstack Lark integration apps.
//!
//! An integration App (linear, github, x, …) owns its source and its cards;
//! everything Lark-facing and source-agnostic lives here: the card builders,
//! the group-webhook sender and DM bot, the per-app [`LarkConfig`], the
//! event-callback scaffold ([`event`]), the [`slot`] state cell that backs
//! host-mounted ingress routers, and shared crypto/text [`utils`].

pub mod card;
pub mod config;
pub mod event;
pub mod routing;
pub mod slot;
pub mod utils;

mod bot;
mod webhook;

pub use bot::LarkBotClient;
pub use config::{LarkConfig, TomlLark};
pub use slot::{Live, SlotGuard, StateSlot, slot};
pub use utils::{truncate, verify_hmac_sha256, verify_standard_webhook};
pub use webhook::send_lark_card;
