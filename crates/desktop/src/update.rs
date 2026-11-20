//! New versions of the app: Tauri's updater checks the latest GitHub release a
//! little after the app opens and then every few hours. When there is one, the
//! panel shows it (with Update) and a notification says so once. Update
//! downloads it, checks its signature against the key built into the app,
//! installs it and restarts. The check sends nothing about the person.

use std::sync::Mutex;
use std::time::Duration;

use tauri::{AppHandle, Manager, Url};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::texts::texts;

const FIRST_CHECK: Duration = Duration::from_secs(20);
const EVERY: Duration = Duration::from_secs(6 * 60 * 60);

/// The update found, kept for when the person presses Update.
struct Pending(Mutex<Option<Update>>);

/// For tests only: a local manifest instead of the GitHub release.
fn local_manifest() -> Option<Url> {
    std::env::var("RAIOX_UPDATE_URL").ok().filter(|url| url.starts_with("http://127.0.0.1:")).and_then(|url| Url::parse(&url).ok())
}

async fn check(app: &AppHandle) -> Option<Update> {
    let builder = app.updater_builder();
    let builder = match local_manifest() {
        // Plain http works only in a test build whose config allows it; release builds stay on https.
        Some(url) => builder.endpoints(vec![url]).ok()?,
        None => builder,
    };
    builder.build().ok()?.check().await.ok().flatten()
}

async fn install(app: AppHandle) {
    let pending = app.state::<Pending>().0.lock().ok().and_then(|mut update| update.take());
    let Some(update) = pending else { return };
    match update.download_and_install(|_, _| {}, || {}).await {
        Ok(()) => app.restart(),
        Err(error) => {
            let message = texts().update_failed.replace("{e}", &error.to_string());
            let _ = app.notification().builder().title("raiox").body(message).show();
        }
    }
}

/// Starts checking; the panel's Update button installs what was found.
pub fn watch(app: AppHandle) {
    app.manage(Pending(Mutex::new(None)));
    let handle = app.clone();
    raiox::app_update::on_install(move || {
        tauri::async_runtime::spawn(install(handle.clone()));
    });
    // "Check for updates" in the panel: ask now, and keep what was found for Update.
    let asking = app.clone();
    raiox::app_update::on_check(move || {
        let found = tauri::async_runtime::block_on(check(&asking));
        Ok(found.map(|update| {
            let version = update.version.clone();
            if let Ok(mut pending) = asking.state::<Pending>().0.lock() {
                *pending = Some(update);
            }
            version
        }))
    });
    tauri::async_runtime::spawn(async move {
        tokio_sleep(FIRST_CHECK).await;
        let mut told: Option<String> = None;
        loop {
            if let Some(update) = check(&app).await {
                let version = update.version.clone();
                raiox::app_update::set_available(&version);
                if let Ok(mut pending) = app.state::<Pending>().0.lock() {
                    *pending = Some(update);
                }
                if told.as_deref() != Some(version.as_str()) {
                    let t = texts();
                    let _ = app.notification().builder().title(t.update_ready.replace("{v}", &version)).body(t.update_hint).show();
                    told = Some(version);
                }
            }
            tokio_sleep(EVERY).await;
        }
    });
}

/// Waits without holding a runtime thread.
async fn tokio_sleep(duration: Duration) {
    let _ = tauri::async_runtime::spawn_blocking(move || std::thread::sleep(duration)).await;
}
