//! This app's persistence layer — sea-orm entities + migrations backing the
//! shared App database (`larkstack_core::db`). The host runs [`user_map`]'s
//! migrations at startup; the entity's table is namespaced `linear_` as the
//! framework requires. Future entities get their own submodule here.

pub mod user_map;
