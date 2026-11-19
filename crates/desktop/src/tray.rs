//! The menu bar icon: how many AIs are working, and the app's few actions.

use std::time::Duration;

use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager};
use tauri_plugin_autostart::ManagerExt;

use crate::texts::{texts, working};

const TRAY: &str = "raiox";
/// How often the menu bar looks at the AIs at work.
const WATCH_EVERY: Duration = Duration::from_secs(2);

/// The status line of the menu, kept to update it.
pub struct Status(pub MenuItem<tauri::Wry>);

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let t = texts();
    let status = MenuItem::with_id(app, "status", t.starting, false, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", t.open, true, None::<&str>)?;
    let browser = MenuItem::with_id(app, "browser", t.open_browser, true, None::<&str>)?;
    let at_login = CheckMenuItem::with_id(app, "login", t.start_at_login, true, app.autolaunch().is_enabled().unwrap_or(false), None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", t.quit, true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[&status, &PredefinedMenuItem::separator(app)?, &open, &browser, &PredefinedMenuItem::separator(app)?, &at_login, &PredefinedMenuItem::separator(app)?, &quit],
    )?;
    TrayIconBuilder::with_id(TRAY)
        .icon(Image::from_bytes(include_bytes!("../icons/tray.png"))?)
        .icon_as_template(true)
        .tooltip("raiox")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "open" => crate::show(app),
            "browser" => {
                if let Some(panel) = app.try_state::<crate::Panel>() {
                    let _ = open::that(&panel.url);
                }
            }
            "login" => {
                let launcher = app.autolaunch();
                let on = launcher.is_enabled().unwrap_or(false);
                let _ = if on { launcher.disable() } else { launcher.enable() };
                let _ = at_login.set_checked(launcher.is_enabled().unwrap_or(!on));
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    app.manage(Status(status));
    Ok(())
}

/// Keeps the menu bar's count of working AIs current (a number beside the icon while any work).
pub fn watch(app: AppHandle, jobs: raiox::bots::Jobs) {
    std::thread::spawn(move || {
        let mut shown = None;
        loop {
            let busy = jobs.busy_count();
            if shown != Some(busy) {
                if let Some(status) = app.try_state::<Status>() {
                    let _ = status.0.set_text(working(busy));
                }
                if let Some(tray) = app.tray_by_id(TRAY) {
                    let _ = tray.set_title((busy > 0).then(|| busy.to_string()));
                }
                shown = Some(busy);
            }
            std::thread::sleep(WATCH_EVERY);
        }
    });
}
