//! Lark adapter: the Docx standup-table mechanics ([`doc`]) and the announce/
//! reminder card builders ([`card`]). The [`crate::flow`] operations compose
//! these; nothing else reaches into the Lark surface directly.

pub mod card;
pub mod doc;
