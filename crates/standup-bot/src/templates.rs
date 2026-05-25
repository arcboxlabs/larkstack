//! Askama text templates for bot replies and CLI output.
//!
//! Card JSON stays in `flow.rs` behind `serde_json::json!()` — templates here
//! are for plain-text messages where iteration or multi-line layout pays off.

use askama::Template;

#[derive(Template)]
#[template(path = "help.txt")]
pub struct HelpTemplate;

#[derive(Template)]
#[template(path = "check.txt")]
pub struct CheckTemplate<'a> {
    pub date: &'a str,
    pub url: &'a str,
    pub missing: Vec<String>,
}
