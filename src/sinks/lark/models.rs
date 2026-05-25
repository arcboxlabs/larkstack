//! Serializable Lark interactive card structures.

#[cfg(feature = "native")]
pub use larkoapi::card::{LarkCard, LarkHeader, LarkMessage, LarkTitle};

#[cfg(not(feature = "native"))]
mod local {
    use serde::Serialize;

    #[derive(Serialize)]
    pub struct LarkMessage {
        pub msg_type: &'static str,
        pub card: LarkCard,
    }

    #[derive(Serialize, Clone)]
    pub struct LarkCard {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub config: Option<serde_json::Value>,
        pub header: LarkHeader,
        pub elements: Vec<serde_json::Value>,
    }

    #[derive(Serialize, Clone)]
    pub struct LarkHeader {
        pub template: String,
        pub title: LarkTitle,
    }

    #[derive(Serialize, Clone)]
    pub struct LarkTitle {
        pub content: String,
        pub tag: &'static str,
    }
}

#[cfg(not(feature = "native"))]
pub use local::*;
