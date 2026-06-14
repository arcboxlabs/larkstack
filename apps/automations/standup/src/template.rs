//! Runtime template rendering. Templates are admin-editable (stored in the
//! [`settings`](crate::db::settings) row), so they're evaluated at runtime with
//! minijinja rather than compiled in — `{{ var }}` / `{% for %}` work as in
//! Jinja2. A render failure (bad template or missing var) degrades to an inline
//! error string instead of dropping the message.

use minijinja::Environment;
use serde::Serialize;

/// Render `tpl` with `ctx`, returning the error text on failure (so the caller
/// can surface it in the card/reply rather than silently sending nothing).
pub fn render<S: Serialize>(tpl: &str, ctx: S) -> String {
    match Environment::new().render_str(tpl, ctx) {
        Ok(s) => s,
        Err(e) => format!("[template error: {e}]"),
    }
}
