//! Daily Standup bot for Lark/Feishu.
//!
//! Runs 4 scheduled jobs per day (UTC+8):
//! - 20:00 create next-day Docx + announce to chat
//! - 22:00 DM anyone who hasn't filled the next-day doc
//! - 09:30 same reminder for today's doc
//! - 10:00 final reminder + in-app urgent escalation

pub mod config;
pub mod date;
pub mod flow;
pub mod lark;
pub mod runtime;
pub mod settings;
pub mod template;
pub mod trigger;

pub use config::{AppConfig, LarkConfig, StandupConfig};
pub use runtime::app::app;
pub use runtime::run::{build_bot, run, run_with_bot};
