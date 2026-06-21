//! Console-configurable notification routing, shared by the source integrations.
//!
//! An integration (github, gitlab, …) maps a *subject* — a project/repo path such as
//! `group/project` or `owner/repo` — and an *event* string to one or more Lark
//! [`Destination`]s (a group chat by `chat_id`, or a DM by user `open_id`/email). The ruleset is a single
//! JSON blob in the per-App [`StateStore`] (key [`KEY`]), edited live from the console and
//! [loaded](Config::load) fresh on every webhook, so changes apply without a restart.
//!
//! The matcher and model are source-agnostic; each app supplies its own subject and event
//! vocabulary. Card delivery goes through the Lark bot ([`deliver`]).

mod admin;
mod delivery;
mod matcher;
mod model;
mod spec;
mod store;
#[cfg(test)]
mod tests;
mod validate;

pub use admin::RoutingApi;
pub use delivery::{deliver, deliver_all};
pub use model::{Config, DestKind, Destination, KEY, Rule, UserMap};
pub use spec::{RoutingEvent, RoutingFeatures, RoutingSpec, SubjectSpec};
