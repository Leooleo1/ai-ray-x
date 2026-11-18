//! Local panel server. Listens on 127.0.0.1 only, on a random port, and the
//! API answers only with the session token carried in the URL fragment
//! (fragments never reach server logs or the Referer header).

use std::collections::hash_map::RandomState;
use std::fs;
use std::hash::{BuildHasher, Hasher};
use std::io::Cursor;
use std::path::{Component, Path, PathBuf};

use anyhow::{anyhow, Result};
use raiox_core::{Roots, Store};
use tiny_http::{Header, Method, Request, Response, Server};

use crate::{actions, data};
use raiox_core::apply;
use serde_json::{json, Value};
use std::io::Read;

const MAX_BODY: u64 = 4096;

type Reply = Response<Cursor<Vec<u8>>>;

const SECURITY_HEADERS: [(&str, &str); 5] = [
    (
        "Content-Security-Policy",
        "default-src 'self'; img-src 'self' data: blob:; style-src 'self' 'unsafe-inline'; font-src 'self'; \
         connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'",
    ),
    ("X-Content-Type-Options", "nosniff"),
    ("Referrer-Policy", "no-referrer"),
    ("Cross-Origin-Resource-Policy", "same-origin"),
    ("Cache-Control", "no-store"),
];

const MISSING_BUILD: &str = "<!doctype html><meta charset=utf-8><title>raiox</title>\
<body style=\"background:#060607;color:#9a9aa6;font:14px ui-monospace,monospace;padding:40px\">\
The panel is not built yet. Run <code>pnpm --dir apps/web build</code>, then reload.";

pub struct Panel {
    pub store: Store,
    pub roots: Roots,
    pub web_dir: PathBuf,
    /// Where the change ledger and backups live (next to the index).
    pub state_dir: PathBuf,
    /// The project changes apply to: the folder `raiox open` started in.
    pub project_dir: PathBuf,
}

pub fn run(mut panel: Panel, port: u16, open_browser: bool) -> Result<()> {
    let server = Server::http(("127.0.0.1", port)).map_err(|e| anyhow!("cannot listen on 127.0.0.1:{port}: {e}"))?;
    let port = server.server_addr().to_ip().map(|addr| addr.port()).ok_or_else(|| anyhow!("no TCP address"))?;
    let token = session_token();
    let url = format!("http://127.0.0.1:{port}/#t={token}");
    println!("raiox panel: {url}");
    println!("Only this machine can open it. Ctrl+C stops it.");
    if open_browser && let Err(error) = open::that(&url) {
        eprintln!("could not open the browser ({error}); open the link above");
    }
    let hosts = [format!("127.0.0.1:{port}"), format!("localhost:{port}")];
    for mut request in server.incoming_requests() {
        let reply = route(&mut panel, &mut request, &token, &hosts);
        // The browser may have gone away; nothing to do about it.
        let _ = request.respond(reply);
    }
    Ok(())
}

fn route(panel: &mut Panel, request: &mut Request, token: &str, hosts: &[String]) -> Reply {
    if !header(request, "Host").is_some_and(|host| hosts.iter().any(|h| h == host)) {
        return text(403, "forbidden host");
    }
    let path = request.url().split(['?', '#']).next().unwrap_or("/");
    match (request.method(), path) {
        (_, p) if p.starts_with("/api/") && !authorized(request, token) => text(401, "missing or wrong session token"),
        (Method::Get, "/api/report") => respond(report(panel)),
        (Method::Post, "/api/apply") => match body(request) {
            Ok(input) => respond(actions::apply_suggestion(
                &panel.store,
                &panel.state_dir,
                &panel.project_dir,
                input.get("id").and_then(Value::as_str).unwrap_or(""),
                input.get("source").and_then(Value::as_str).unwrap_or("claude"),
            )),
            Err(reply) => reply,
        },
        (Method::Post, "/api/ask") => match body(request) {
            Ok(input) => respond(
                crate::room::ask(
                    &panel.store,
                    &panel.roots,
                    &panel.state_dir,
                    &panel.project_dir,
                    input.get("agent").and_then(Value::as_str).unwrap_or(""),
                    input.get("text").and_then(Value::as_str),
                )
                .map(|messages| json!(messages)),
            ),
            Err(reply) => reply,
        },
        (Method::Post, "/api/undo") => match body(request) {
            Ok(input) => respond(actions::undo_change(
                &panel.store,
                &panel.state_dir,
                &panel.project_dir,
                input.get("change").and_then(Value::as_str).unwrap_or(""),
            )),
            Err(reply) => reply,
        },
        (Method::Get, _) => static_file(&panel.web_dir, path),
        _ => text(405, "method not allowed"),
    }
}

fn header<'r>(request: &'r Request, name: &'static str) -> Option<&'r str> {
    request.headers().iter().find(|h| h.field.equiv(name)).map(|h| h.value.as_str())
}

fn authorized(request: &Request, token: &str) -> bool {
    header(request, "Authorization")
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|given| constant_time_eq(given.as_bytes(), token.as_bytes()))
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// 128 bits from the OS-seeded hasher keys; no extra dependency needed.
fn session_token() -> String {
    (0..2u64)
        .map(|i| {
            let mut hasher = RandomState::new().build_hasher();
            hasher.write_u64(i);
            format!("{:016x}", hasher.finish())
        })
        .collect()
}

fn static_file(root: &Path, url_path: &str) -> Reply {
    let relative = match url_path.trim_start_matches('/') {
        "" => "index.html",
        other => other,
    };
    let candidate = Path::new(relative);
    if !candidate.components().all(|c| matches!(c, Component::Normal(_))) {
        return text(400, "bad path");
    }
    let file = root.join(candidate);
    let is_page = relative == "index.html" || !relative.contains('.');
    match fs::read(&file) {
        Ok(bytes) => with_type(Response::from_data(bytes), content_type(&file)),
        // Client-side routes fall back to the app shell.
        Err(_) if is_page => {
            let shell = fs::read(root.join("index.html")).unwrap_or_else(|_| MISSING_BUILD.as_bytes().to_vec());
            with_type(Response::from_data(shell), "text/html; charset=utf-8")
        }
        Err(_) => text(404, "not found"),
    }
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "png" => "image/png",
        "woff2" => "font/woff2",
        "ico" => "image/x-icon",
        _ => "application/octet-stream",
    }
}

fn text(status: u16, message: &str) -> Reply {
    with_type(Response::from_data(message.as_bytes().to_vec()), "text/plain; charset=utf-8").with_status_code(status)
}

fn with_type(response: Reply, content_type: &str) -> Reply {
    SECURITY_HEADERS
        .iter()
        .chain([("Content-Type", content_type)].iter())
        .filter_map(|(name, value)| Header::from_bytes(name.as_bytes(), value.as_bytes()).ok())
        .fold(response, |reply, header| reply.with_header(header))
}

fn report(panel: &mut Panel) -> anyhow::Result<Value> {
    let mut value = data::refresh_and_collect(&mut panel.store, &panel.roots)?;
    if let Some(object) = value.as_object_mut() {
        object.insert("changes".into(), json!(apply::ledger(&panel.state_dir.join("changes.json"))?));
        let measured = crate::room::outcomes(&panel.store, &panel.roots, &panel.state_dir, &panel.project_dir)?;
        object.insert("outcomes".into(), json!(measured.iter().map(|(_, o)| o).collect::<Vec<_>>()));
        object.insert("applicable".into(), json!({ "claude": apply::applicable("claude"), "codex": apply::applicable("codex") }));
        object.insert("project_dir".into(), json!(panel.project_dir.to_string_lossy()));
        object.insert(
            "room".into(),
            json!({
                "opted_in": crate::room::opted_in(&panel.project_dir),
                "messages": crate::room::messages(&panel.state_dir, &panel.project_dir),
            }),
        );
    }
    Ok(value)
}

fn respond(result: anyhow::Result<Value>) -> Reply {
    match result {
        Ok(value) => with_type(Response::from_data(value.to_string().into_bytes()), "application/json"),
        Err(error) => {
            let message = json!({ "error": format!("{error:#}") }).to_string();
            with_type(Response::from_data(message.into_bytes()), "application/json").with_status_code(422)
        }
    }
}

/// Small JSON bodies only; anything else is refused.
fn body(request: &mut Request) -> Result<Value, Reply> {
    let mut raw = String::new();
    request.as_reader().take(MAX_BODY).read_to_string(&mut raw).map_err(|_| text(400, "unreadable body"))?;
    serde_json::from_str(&raw).map_err(|_| text(400, "body must be JSON"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_long_and_differ() {
        let (a, b) = (session_token(), session_token());
        assert_eq!(a.len(), 32);
        assert_ne!(a, b);
    }

    #[test]
    fn comparison_needs_the_exact_token() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
    }
}
