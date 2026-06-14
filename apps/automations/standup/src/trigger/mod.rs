//! Inbound surfaces that drive the [`crate::flow`] operations. Each module
//! translates one kind of external stimulus into flow calls:
//!
//! - [`scheduler`] — the autonomous Asia/Shanghai timer (announce/remind/urgent)
//! - [`commands`] — the WebSocket chat-command bot (`@bot <cmd>`)
//! - [`actions`] — console action dispatch (`handle_action`)
//!
//! The standalone CLI (`main.rs`) is the fourth surface; it lives at the crate
//! root because it owns the binary entry point.

pub mod actions;
pub mod commands;
pub mod scheduler;
