//! System tray. Built in `.setup()`; owns show/hide of the main window.
//!
//! The icon needs no extra Cargo feature: `icons/icon.ico` is embedded at
//! COMPILE time by tauri-codegen (the first `bundle.icon` entry ending in
//! `.ico` on Windows), so `default_window_icon()` hands us a decoded image.
//! Loading one at runtime instead would need the non-default `image-ico`
//! feature.
//!
//! Tray permissions need no capabilities change either: `core:default`
//! already includes `core:tray:default` and `core:menu:default`, and a tray
//! built in Rust is not subject to the capability system at all. The ONLY
//! build change this needs is the `tray-icon` Cargo feature on `tauri` --
//! note that `tray-icon` already appears in `Cargo.lock` as an OPTIONAL
//! dependency, which is NOT evidence the feature is enabled.

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager};

/// The window label is `"main"` in tauri.conf.json (implicit default) and in
/// capabilities/default.json. Reuse it rather than guessing.
pub const MAIN_WINDOW: &str = "main";

/// Show, unminimize and focus the main window.
pub fn show_main_window(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window(MAIN_WINDOW) {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Pure: tray tooltip for a `ServerDetail.verdict`.
pub fn tooltip_for(verdict: &str) -> String {
    let tail = match verdict {
        "online" => "server online",
        "stopped" => "server stopped",
        "starting" => "server starting",
        "crashed" => "server crashed",
        "soap_unreachable" => "server unreachable",
        // The verdict union may grow; an unknown value must not panic or
        // produce an empty tooltip.
        _ => return "DML Launcher".to_string(),
    };
    format!("DML Launcher — {tail}")
}

/// Apply a pushed verdict to the tray icon.
pub fn apply_status(app: &tauri::AppHandle, verdict: &str) {
    if let Some(tray) = app.tray_by_id("dml-tray") {
        let _ = tray.set_tooltip(Some(tooltip_for(verdict)));
    }
}

pub fn build(app: &tauri::AppHandle) -> tauri::Result<()> {
    let open_i = MenuItem::with_id(app, "tray_open", "Open DML Launcher", true, None::<&str>)?;
    let start_i = MenuItem::with_id(app, "tray_start", "Start server", true, None::<&str>)?;
    let stop_i = MenuItem::with_id(app, "tray_stop", "Stop server", true, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, "tray_quit", "Exit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open_i, &start_i, &stop_i, &quit_i])?;

    TrayIconBuilder::with_id("dml-tray")
        // Returned as an error rather than .expect()ed: this runs inside
        // .setup(), so a bundle.icon change should abort cleanly instead of
        // panicking the app.
        .icon(
            app.default_window_icon()
                .ok_or_else(|| tauri::Error::UnknownPath)?
                .clone(),
        )
        .tooltip("DML Launcher")
        .menu(&menu)
        // Left click opens the window; the menu is the right-click surface.
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "tray_open" => show_main_window(app),
            // Surface the window and hand the request to the frontend, which
            // runs the SAME act() its Home buttons call. The tray does not
            // drive the lifecycle API itself — one implementation, one place
            // to change, and the streamed output stays visible in the
            // terminal the user is now looking at.
            "tray_start" | "tray_stop" => {
                show_main_window(app);
                let action = if event.id.as_ref() == "tray_start" { "start" } else { "stop" };
                let _ = app.emit("tray-action", action);
            }
            // MUST go through app.exit() so the existing RunEvent::Exit arm
            // still fires and clears the keep-awake execution state. A window
            // destroy would bypass it and leave the PC pinned awake.
            "tray_quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tooltip_names_the_app_and_the_state() {
        assert_eq!(tooltip_for("online"), "DML Launcher — server online");
        assert_eq!(tooltip_for("stopped"), "DML Launcher — server stopped");
        assert_eq!(tooltip_for("starting"), "DML Launcher — server starting");
        assert_eq!(tooltip_for("crashed"), "DML Launcher — server crashed");
        assert_eq!(tooltip_for("soap_unreachable"), "DML Launcher — server unreachable");
    }

    #[test]
    fn tooltip_falls_back_for_an_unknown_verdict() {
        assert_eq!(tooltip_for("nonsense"), "DML Launcher");
    }
}
