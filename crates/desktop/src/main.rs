//! raiox as a desktop app: the same panel the `raiox` command serves, in a
//! window of its own, with a menu bar icon and native notifications. The
//! panel server runs inside the app (phone access keeps working through it).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod texts;
mod tray;

use std::path::PathBuf;

use serde_json::Value;
use tauri::webview::NewWindowResponse;
use tauri::window::Color;
use tauri::{AppHandle, Manager, RunEvent, Url, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_notification::NotificationExt;

use crate::texts::texts;

const MAIN: &str = "main";

/// The running panel: its address (with the session token) and where its state lives.
pub struct Panel {
    pub url: String,
    pub state_dir: PathBuf,
}

/// Brings the window back (from the menu bar, the Dock, or a second launch).
pub fn show(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// The panel and the app's own loading page stay in the window; any other page opens in the browser.
fn stays_inside(url: &Url) -> bool {
    let local = matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "tauri.localhost")) || url.scheme() == "tauri";
    if !local {
        open_outside(url);
    }
    local
}

/// Web links open in the default browser; nothing else (files, scripts) is opened.
fn open_outside(url: &Url) {
    if matches!(url.scheme(), "http" | "https") {
        let _ = open::that(url.as_str());
    }
}

/// The built interface: inside the app bundle, or the checkout's build when run with cargo.
fn web_dir(app: &AppHandle) -> PathBuf {
    let bundled = app.path().resource_dir().map(|dir| dir.join("web")).ok().filter(|dir| dir.join("index.html").exists());
    bundled.unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../apps/web/dist"))
}

/// A notification when an AI finishes, unless the person is looking at the window.
fn notify(app: &AppHandle, note: &Value) {
    let looking = app.get_webview_window(MAIN).is_some_and(|w| w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false));
    if looking {
        return;
    }
    let t = texts();
    let field = |name: &str| note.get(name).and_then(Value::as_str).unwrap_or("");
    let lead = match field("kind") {
        "proposal" => t.proposal,
        "draft" => t.draft,
        "failed" => t.broke,
        _ => "",
    };
    let body = [lead, field("body")].into_iter().filter(|part| !part.is_empty()).collect::<Vec<_>>().join(" · ");
    let _ = app.notification().builder().title(field("title")).body(body).show();
}

/// Starts the panel off the main thread (indexing the logs can take a moment), then shows it.
fn start(app: AppHandle) {
    std::thread::spawn(move || {
        // RAIOX_DB points the app at another index (tests); by default it is the one `raiox` uses.
        let db = std::env::var_os("RAIOX_DB").map(PathBuf::from);
        match raiox::start_panel(web_dir(&app), db) {
            Ok(started) => {
                let handle = app.clone();
                raiox::push::on_turn_end(move |note| notify(&handle, note));
                tray::watch(app.clone(), started.jobs.clone());
                let url = started.url.clone();
                if std::env::var_os("RAIOX_DB").is_some() {
                    println!("raiox panel: {url}");
                }
                app.manage(Panel { url: started.url, state_dir: started.state_dir });
                if let (Some(window), Ok(url)) = (app.get_webview_window(MAIN), Url::parse(&url)) {
                    let _ = window.navigate(url);
                }
            }
            Err(error) => {
                let message = texts().failed.replace("{e}", &format!("{error:#}"));
                if let Some(window) = app.get_webview_window(MAIN) {
                    let _ = window.eval(format!("document.getElementById('estado').textContent = {}", Value::String(message)));
                }
            }
        }
    });
}

fn main() {
    let app = tauri::Builder::default()
        // A second launch brings the open window forward instead of a second panel.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show(app)))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .setup(|app| {
            let window = WebviewWindowBuilder::new(app, MAIN, WebviewUrl::App("index.html".into()))
                .title("raiox")
                .inner_size(1440.0, 900.0)
                .min_inner_size(960.0, 640.0)
                .theme(Some(tauri::Theme::Dark))
                .background_color(Color(6, 6, 7, 255))
                .on_navigation(stays_inside)
                .on_new_window(|url, _| {
                    open_outside(&url);
                    NewWindowResponse::Deny
                })
                .build()?;
            // Closing the window keeps raiox running in the menu bar; Quit ends it.
            let hidden = window.clone();
            window.on_window_event(move |event| {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = hidden.hide();
                }
            });
            tray::build(app.handle())?;
            start(app.handle().clone());
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("the raiox app could not start");
    app.run(|app, event| match event {
        // The Dock icon brings the window back (macOS has a Dock; elsewhere the tray does it).
        #[cfg(target_os = "macos")]
        RunEvent::Reopen { .. } => show(app),
        // Dev servers and the tunnel the panel started stop with the app.
        RunEvent::Exit => {
            if let Some(panel) = app.try_state::<Panel>() {
                raiox::procs::cleanup(&panel.state_dir);
            }
        }
        _ => {}
    });
}
