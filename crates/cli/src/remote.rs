//! Requests through the tunnel, from outside this machine: only paired
//! devices get in (with the person's own hostname, only after a Cloudflare
//! Access login too), and some things stay on this machine. The panel's own
//! API for the phone settings (tunnel, pairing, devices) lives here as well.

use std::path::Path;

use base64::Engine;
use qrcode::render::svg;
use qrcode::QrCode;
use serde_json::{json, Value};
use tiny_http::{Method, Request};

use crate::serve::{body, header, respond, text, Reply};
use crate::{access, devices, tunnel};

/// What a device may not do: manage the phone access, change the setup or the CLIs, start programs.
fn desk_only(method: &Method, path: &str) -> bool {
    path.starts_with("/api/remote")
        || path.starts_with("/api/telegram")
        || path == "/api/app/update"
        || path == "/api/app/check"
        || (*method != Method::Get && (path.starts_with("/api/setup") || path == "/api/settings"))
        || ["/api/bot/preview_start", "/api/bot/allow", "/api/bot/grants", "/api/bot/auditor", "/api/bot/request_approve", "/api/bot/request_discard", "/api/bot/rsi_create", "/api/bot/create_project"].contains(&path)
}

fn bearer(request: &Request) -> Option<&str> {
    header(request, "Authorization").and_then(|value| value.strip_prefix("Bearer "))
}

/// The checks on a request from outside. None lets it through to the panel's routes.
pub fn gate(state_dir: &Path, request: &mut Request, path: &str) -> Option<Reply> {
    if let Some((team, aud)) = tunnel::access_rule(state_dir)
        && let Err(error) = access::check(&team, &aud, header(request, access::HEADER))
    {
        return Some(text(403, &format!("{error:#}")));
    }
    // The app itself (page, scripts, styles) holds nothing private.
    if !path.starts_with("/api/") {
        return None;
    }
    if path == "/api/pair" {
        return Some(pair(state_dir, request));
    }
    let Some(device) = bearer(request).and_then(|token| devices::check(state_dir, token)) else {
        return Some(text(401, "this device is not paired"));
    };
    if path.starts_with("/api/push") {
        return Some(push_route(state_dir, request, path, &device));
    }
    if desk_only(request.method(), path) {
        return Some(text(403, "only on the computer"));
    }
    None
}

fn pair(state_dir: &Path, request: &mut Request) -> Reply {
    if *request.method() != Method::Post {
        return text(405, "method not allowed");
    }
    let name = devices::name_from(header(request, "User-Agent").unwrap_or(""));
    match body(request) {
        Ok(input) => respond(devices::pair(state_dir, input.get("code").and_then(Value::as_str).unwrap_or(""), &name).map(|token| json!({ "token": token, "name": name }))),
        Err(reply) => reply,
    }
}

/// Notifications for the device asking: its state, turning them on and off, a test note.
fn push_route(state_dir: &Path, request: &mut Request, path: &str, device: &str) -> Reply {
    let state = || -> anyhow::Result<Value> { Ok(json!({ "public_key": crate::push::public_key(state_dir)?, "on": devices::push_of(state_dir, device).is_some() })) };
    match (request.method().clone(), path) {
        (Method::Get, "/api/push") => respond(state()),
        (Method::Post, "/api/push/subscribe") => match body(request) {
            Ok(input) => respond(crate::push::subscription(&input).and_then(|push| devices::set_push(state_dir, device, Some(push))).and_then(|()| state())),
            Err(reply) => reply,
        },
        (Method::Post, "/api/push/unsubscribe") => respond(devices::set_push(state_dir, device, None).and_then(|()| state())),
        (Method::Post, "/api/push/test") => respond(crate::push::test(state_dir, device).and_then(|()| state())),
        _ => text(404, "not found"),
    }
}

fn state(state_dir: &Path) -> anyhow::Result<Value> {
    Ok(json!({ "tunnel": tunnel::status(state_dir), "devices": devices::list(state_dir) }))
}

/// A pairing link for the running tunnel, with its QR code as an image.
fn new_pairing(state_dir: &Path) -> anyhow::Result<Value> {
    let host = tunnel::public_host(state_dir).ok_or_else(|| anyhow::anyhow!("turn the phone access on first"))?;
    let (code, expires_ms) = devices::new_code()?;
    let url = format!("https://{host}/#pair={code}");
    let typed = format!("{}-{}", &code[..5], &code[5..]);
    let image = QrCode::new(url.as_bytes()).map_err(|error| anyhow::anyhow!("could not draw the QR code: {error:?}"))?.render::<svg::Color>().min_dimensions(232, 232).dark_color(svg::Color("#0b0b0e")).light_color(svg::Color("#ffffff")).build();
    Ok(json!({
        "url": url,
        "code": typed,
        "qr": format!("data:image/svg+xml;base64,{}", base64::engine::general_purpose::STANDARD.encode(image)),
        "expires_ms": expires_ms,
    }))
}

/// The phone settings, on this machine's panel only.
pub fn desk(state_dir: &Path, request: &mut Request, path: &str, port: u16) -> Reply {
    match (request.method().clone(), path) {
        (Method::Get, "/api/remote") => respond(state(state_dir)),
        (Method::Post, "/api/remote/pair") => respond(new_pairing(state_dir)),
        (Method::Post, "/api/remote/tunnel") => match body(request) {
            Ok(input) => respond(serde_json::from_value::<tunnel::Config>(input).map_err(anyhow::Error::from).and_then(|config| tunnel::set(state_dir, port, config)).and_then(|()| state(state_dir))),
            Err(reply) => reply,
        },
        (Method::Post, "/api/remote/forget") => match body(request) {
            Ok(input) => respond(devices::remove(state_dir, input.get("id").and_then(Value::as_str).unwrap_or("")).and_then(|()| state(state_dir))),
            Err(reply) => reply,
        },
        _ => text(404, "not found"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn devices_cannot_manage_access_or_start_programs() {
        assert!(desk_only(&Method::Post, "/api/remote/pair") && desk_only(&Method::Get, "/api/remote"));
        assert!(desk_only(&Method::Post, "/api/settings") && desk_only(&Method::Post, "/api/setup/clone") && desk_only(&Method::Post, "/api/setup/install") && desk_only(&Method::Post, "/api/setup/create_project"));
        assert!(desk_only(&Method::Post, "/api/bot/preview_start") && desk_only(&Method::Post, "/api/bot/allow") && desk_only(&Method::Post, "/api/bot/auditor") && desk_only(&Method::Post, "/api/bot/rsi_create") && desk_only(&Method::Post, "/api/bot/create_project"));
        assert!(!desk_only(&Method::Get, "/api/settings") && !desk_only(&Method::Post, "/api/bot/approve") && !desk_only(&Method::Get, "/api/bots"));
    }
}
