//! Daily Standup bot for Lark/Feishu.
//!
//! Runs 4 scheduled jobs per day (UTC+8):
//! - 20:00 create next-day Docx + announce to chat
//! - 22:00 DM anyone who hasn't filled the next-day doc
//! - 09:30 same reminder for today's doc
//! - 10:00 final reminder + in-app urgent escalation

pub mod actions;
pub mod app;
pub mod commands;
pub mod config;
pub mod flow;
pub mod run;
pub mod scheduler;
pub mod templates;

pub use app::app;
pub use config::{AppConfig, LarkConfig, StandupConfig};
pub use run::{build_bot, run, run_with_bot};
