//! Webhook receivers that normalize platform payloads into [`Event`](crate::event::Event)s.
//!
//! `x` is the exception: it produces link-preview cards on demand (via the Lark
//! event callback), not [`Event`](crate::event::Event)s.

pub mod github;
pub mod linear;
pub mod x;
