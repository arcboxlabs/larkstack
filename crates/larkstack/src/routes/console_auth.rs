//! Console sign-in (Lark OAuth) binding API.
//!
//! The `[console]` section binds one `[lark-apps.<name>]` entry as the console's
//! OAuth client and lists the admin allowlist. These handlers let the setup UI
//! write that binding without hand-editing TOML — it is the one config section
//! that, once set, flips the whole console from OPEN to sign-in-required. Edits
//! go through `toml_edit` (comments preserved) and broadcast like every other
//! structured edit.

use axum::Json;
use axum::extract::State;
use larkstack_core::LarkRegistry;
use serde::{Deserialize, Serialize};
use toml_edit::{Array, DocumentMut, Item, Table, value};
use tracing::info;
use utoipa::ToSchema;

use crate::HostState;

use super::{ApiError, OkResponse, oauth, parse_doc, write_doc};

#[derive(Deserialize, Default)]
struct ConsoleSection {
    #[serde(default)]
    lark_app: Option<String>,
    #[serde(default)]
    admins: Vec<String>,
    #[serde(default)]
    redirect_uri: Option<String>,
    #[serde(default)]
    scope: Option<String>,
}

#[derive(Deserialize, Default)]
struct View {
    #[serde(default)]
    console: ConsoleSection,
    #[serde(default, rename = "lark-apps")]
    lark_apps: LarkRegistry,
}

/// Read `[console]` + `[lark-apps]`; a malformed config yields an empty view so
/// the GET handler never fails on a stray manual edit.
fn parse_view(toml_str: &str) -> View {
    toml::from_str(toml_str).unwrap_or_default()
}

/// Body of `GET /api/console-auth`.
#[derive(Serialize, ToSchema)]
pub(crate) struct ConsoleAuthView {
    /// Whether sign-in is currently enforced (a Lark app with working
    /// credentials is bound). When false the console is OPEN.
    configured: bool,
    /// The bound `[lark-apps.<name>]`, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    lark_app: Option<String>,
    /// Admin email allowlist (empty = any tenant user may sign in).
    admins: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    redirect_uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scope: Option<String>,
}

/// `GET /api/console-auth` — the current console sign-in binding.
#[utoipa::path(
    get, path = "/console-auth", tag = "console",
    security(("session" = [])),
    responses((status = 200, description = "Console sign-in binding", body = ConsoleAuthView)),
)]
pub(crate) async fn get_console_auth(State(s): State<HostState>) -> Json<ConsoleAuthView> {
    let cfg = s.control.config();
    let view = parse_view(&cfg);
    Json(ConsoleAuthView {
        configured: oauth::resolve(&cfg).is_some(),
        lark_app: view.console.lark_app,
        admins: view.console.admins,
        redirect_uri: view.console.redirect_uri,
        scope: view.console.scope,
    })
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct BindReq {
    /// Name of a registered `[lark-apps.<name>]` to sign users in with.
    lark_app: String,
    /// Admin email allowlist; empty = any tenant user may sign in.
    #[serde(default)]
    admins: Vec<String>,
    /// Override the auto-derived `<host>/auth/callback` (must match what the
    /// Lark app registers).
    #[serde(default)]
    redirect_uri: Option<String>,
    /// Extra OAuth scopes, space-separated (usually none).
    #[serde(default)]
    scope: Option<String>,
}

/// `PUT /api/console-auth` — bind a Lark app as the console's OAuth client.
/// Saving this enforces sign-in on the next request, so the bound app must
/// already carry working credentials in the registry (else the console would
/// silently stay open).
#[utoipa::path(
    put, path = "/console-auth", tag = "console",
    security(("session" = [])),
    request_body = BindReq,
    responses(
        (status = 200, description = "Binding saved; sign-in now required", body = OkResponse),
        (status = 400, description = "lark_app is not a registered app with credentials", body = super::ErrorResponse),
        (status = 500, description = "Could not write the config file", body = super::ErrorResponse),
    ),
)]
pub(crate) async fn put_console_auth(
    State(s): State<HostState>,
    Json(req): Json<BindReq>,
) -> Result<Json<OkResponse>, ApiError> {
    let name = req.lark_app.trim();
    if name.is_empty() {
        return Err(ApiError::bad_request("lark_app is required"));
    }
    // Guard against locking the console behind a ghost/empty binding: the bound
    // entry must exist and carry credentials (mirrors `oauth::resolve`).
    match parse_view(&s.control.config()).lark_apps.get(name) {
        Some(app) if !app.app_id.is_empty() && !app.app_secret.is_empty() => {}
        _ => {
            return Err(ApiError::bad_request(format!(
                "lark_app '{name}' is not a registered Lark app with credentials — register it first"
            )));
        }
    }

    let mut doc = parse_doc(&s.control.config()).map_err(ApiError::internal)?;
    write_console(
        &mut doc,
        name,
        &req.admins,
        req.redirect_uri.as_deref(),
        req.scope.as_deref(),
    );
    write_doc(&s, doc).map_err(ApiError::internal)?;
    info!(lark_app = %name, "console sign-in bound — sign-in now required");
    Ok(Json(OkResponse::ok()))
}

/// `DELETE /api/console-auth` — unbind, reopening the console. The recovery path
/// when an operator is about to lock themselves out (admins list omits them).
#[utoipa::path(
    delete, path = "/console-auth", tag = "console",
    security(("session" = [])),
    responses(
        (status = 200, description = "Unbound; the console is open again", body = OkResponse),
        (status = 500, description = "Could not write the config file", body = super::ErrorResponse),
    ),
)]
pub(crate) async fn delete_console_auth(
    State(s): State<HostState>,
) -> Result<Json<OkResponse>, ApiError> {
    let mut doc = parse_doc(&s.control.config()).map_err(ApiError::internal)?;
    if let Some(console) = doc.get_mut("console").and_then(Item::as_table_mut) {
        console.remove("lark_app");
    }
    write_doc(&s, doc).map_err(ApiError::internal)?;
    info!("console sign-in unbound — console is open");
    Ok(Json(OkResponse::ok()))
}

/// Write the `[console]` binding keys, creating the table if absent. `lark_app`
/// and `admins` are always set; `redirect_uri`/`scope` are written only when
/// non-empty, and removed otherwise so a cleared field doesn't linger.
fn write_console(
    doc: &mut DocumentMut,
    lark_app: &str,
    admins: &[String],
    redirect_uri: Option<&str>,
    scope: Option<&str>,
) {
    let console = doc
        .entry("console")
        .or_insert(Item::Table(Table::new()))
        .as_table_mut()
        .expect("console is a table");
    console["lark_app"] = value(lark_app);
    let mut arr = Array::new();
    for a in admins.iter().map(|a| a.trim()).filter(|a| !a.is_empty()) {
        arr.push(a);
    }
    console["admins"] = value(arr);
    set_or_remove(console, "redirect_uri", redirect_uri);
    set_or_remove(console, "scope", scope);
}

fn set_or_remove(table: &mut Table, key: &str, v: Option<&str>) {
    match v.map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => table[key] = value(s),
        None => {
            table.remove(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(s: &str) -> DocumentMut {
        s.parse().expect("valid toml")
    }

    const REGISTRY: &str = "[lark-apps.main]\napp_id = \"cli_x\"\napp_secret = \"s\"\n";

    #[test]
    fn bind_writes_console_and_resolve_picks_it_up() {
        let mut d = doc(&format!("# keep me\n{REGISTRY}"));
        write_console(
            &mut d,
            "main",
            &["You@Example.com".into(), "  ".into()],
            None,
            None,
        );
        let out = d.to_string();
        assert!(out.contains("# keep me"), "comment preserved: {out}");
        assert!(out.contains("[console]"), "console table written: {out}");
        // The gate's own reader now sees a configured (sign-in-required) console.
        assert!(
            oauth::resolve(&out).is_some(),
            "configured after bind: {out}"
        );
        // Blank admin entries are dropped on write.
        assert_eq!(parse_view(&out).console.admins, vec!["You@Example.com"]);
    }

    #[test]
    fn redirect_and_scope_set_when_present_removed_when_blank() {
        let mut d = doc(REGISTRY);
        write_console(
            &mut d,
            "main",
            &[],
            Some("https://c.example/auth/callback"),
            Some("a b"),
        );
        let out = d.to_string();
        assert!(out.contains("redirect_uri = \"https://c.example/auth/callback\""));
        assert!(out.contains("scope = \"a b\""));

        // Re-binding with blanks clears them.
        let mut d2 = doc(&out);
        write_console(&mut d2, "main", &[], Some("  "), None);
        let out2 = d2.to_string();
        assert!(!out2.contains("redirect_uri"), "cleared: {out2}");
        assert!(!out2.contains("scope"), "cleared: {out2}");
    }

    #[test]
    fn parse_view_reads_back_binding() {
        let mut d = doc(REGISTRY);
        write_console(&mut d, "main", &["a@x.io".into()], None, None);
        let v = parse_view(&d.to_string());
        assert_eq!(v.console.lark_app.as_deref(), Some("main"));
        assert_eq!(v.console.admins, vec!["a@x.io"]);
    }
}
