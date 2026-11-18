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

use crate::data;

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
    for request in server.incoming_requests() {
        let reply = route(&mut panel, &request, &token, &hosts);
        // The browser may have gone away; nothing to do about it.
        let _ = request.respond(reply);
    }
    Ok(())
}

fn route(panel: &mut Panel, request: &Request, token: &str, hosts: &[String]) -> Reply {
    if !header(request, "Host").is_some_and(|host| hosts.iter().any(|h| h == host)) {
        return text(403, "forbidden host");
    }
    let path = request.url().split(['?', '#']).next().unwrap_or("/");
    match (request.method(), path) {
        (Method::Get, "/api/report") if !authorized(request, token) => text(401, "missing or wrong session token"),
        (Method::Get, "/api/report") => match data::refresh_and_collect(&mut panel.store, &panel.roots) {
            Ok(value) => with_type(Response::from_data(value.to_string().into_bytes()), "application/json"),
            Err(error) => {
                eprintln!("report failed: {error:#}");
                text(500, "report failed; see the terminal")
            }
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
