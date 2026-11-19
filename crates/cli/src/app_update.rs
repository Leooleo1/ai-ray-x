//! A new version of the desktop app, as the panel sees it. The app checks for
//! one (Tauri's updater, against the latest GitHub release) and says so here;
//! the panel shows it, and its Update button asks the app to install it and
//! restart. Run as `raiox open`, there is no app: nothing is shown.

use std::sync::{Mutex, OnceLock};

use anyhow::{bail, Result};
use serde_json::{json, Value};

type Installer = Box<dyn Fn() + Send + Sync>;

static AVAILABLE: Mutex<Option<String>> = Mutex::new(None);
static INSTALLING: Mutex<bool> = Mutex::new(false);
static INSTALLER: OnceLock<Installer> = OnceLock::new();

/// The app found a newer version.
pub fn set_available(version: &str) {
    *AVAILABLE.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(version.to_string());
}

/// The app's way to download, install and restart (it does not return when it works).
pub fn on_install(install: impl Fn() + Send + Sync + 'static) {
    let _ = INSTALLER.set(Box::new(install));
}

/// The person pressed Update.
pub fn install() -> Result<Value> {
    let Some(run) = INSTALLER.get() else { bail!("only the raiox app updates itself") };
    if AVAILABLE.lock().unwrap_or_else(std::sync::PoisonError::into_inner).is_none() {
        bail!("there is no new version");
    }
    *INSTALLING.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = true;
    run();
    Ok(state())
}

pub fn state() -> Value {
    json!({
        "version": env!("CARGO_PKG_VERSION"),
        "available": *AVAILABLE.lock().unwrap_or_else(std::sync::PoisonError::into_inner),
        "installing": *INSTALLING.lock().unwrap_or_else(std::sync::PoisonError::into_inner),
        "can_update": INSTALLER.get().is_some(),
    })
}
