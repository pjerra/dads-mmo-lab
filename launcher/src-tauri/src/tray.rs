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

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager};

/// The window label is `"main"` in tauri.conf.json (implicit default) and in
/// capabilities/default.json. Reuse it rather than guessing.
pub const MAIN_WINDOW: &str = "main";

/// One row of the tray's server list, pushed from the frontend (which already
/// has the list, the display names and the running flags) — the same doctrine
/// as `tray_set_status`: Rust owns no poller of its own.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct TrayServer {
    pub id: String,
    pub display_name: String,
    pub running: bool,
}

/// What a clicked menu item is asking for.
#[derive(Debug, PartialEq)]
pub enum TrayRequest {
    Open,
    Quit,
    /// Handed to the frontend, which runs the SAME act() its own buttons call.
    /// `id` is `None` for server-independent actions (doctor).
    Action { action: String, id: Option<String> },
}

/// Verbs a server submenu may ask for. CLOSED on purpose: the id string is the
/// only channel between a click and the frontend, so a new verb must be added
/// here deliberately rather than by whatever a menu item happens to be called.
const SERVER_ACTIONS: [&str; 3] = ["start", "stop", "restart"];

/// The menu-item id for a per-server action. Paired with `parse_menu_id` —
/// they are one encoding, so they live next to each other.
pub fn server_menu_id(action: &str, id: &str) -> String {
    format!("srv:{action}:{id}")
}

/// Same character class as `crate::validate_game_id`, duplicated here rather
/// than imported so this module stays free of the command layer. An id read
/// back off a menu goes on to become a spawn argument, so it is validated at
/// the point it re-enters the program, not only where it left.
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && !id.contains("..")
        && id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// Decode a menu-item id. `None` = ignore the click (unknown item, unknown
/// verb, or an id that failed validation) — never a panic, never a guess.
pub fn parse_menu_id(raw: &str) -> Option<TrayRequest> {
    match raw {
        "tray_open" => return Some(TrayRequest::Open),
        "tray_quit" => return Some(TrayRequest::Quit),
        "tray_doctor" => {
            return Some(TrayRequest::Action { action: "doctor".into(), id: None })
        }
        _ => {}
    }
    let rest = raw.strip_prefix("srv:")?;
    let (action, id) = rest.split_once(':')?;
    if !SERVER_ACTIONS.contains(&action) || !valid_id(id) {
        return None;
    }
    Some(TrayRequest::Action { action: action.to_string(), id: Some(id.to_string()) })
}

/// The submenu label for a server. Falls back to the id when the display name
/// is blank: a nameless tray entry is effectively unclickable.
pub fn server_label(s: &TrayServer) -> String {
    let name = s.display_name.trim();
    let name = if name.is_empty() { s.id.as_str() } else { name };
    let state = if s.running { "running" } else { "stopped" };
    format!("{name} ({state})")
}

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

/// Build the whole menu for a server list. One function for the initial
/// (empty) menu and every later push, so the two can never drift apart.
///
/// Enablement mirrors reality: Start only when stopped, Stop/Restart only when
/// running. An empty list gets a DISABLED placeholder rather than nothing —
/// a tray whose right-click shows only "Exit" reads as a broken app.
fn build_menu(app: &tauri::AppHandle, servers: &[TrayServer]) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::new(app)?;
    menu.append(&MenuItem::with_id(app, "tray_open", "Open DML Launcher", true, None::<&str>)?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    if servers.is_empty() {
        menu.append(&MenuItem::with_id(app, "tray_none", "No servers installed", false, None::<&str>)?)?;
    } else {
        for s in servers {
            let sub = Submenu::with_id(app, format!("srv_menu:{}", s.id), server_label(s), true)?;
            sub.append(&MenuItem::with_id(
                app,
                server_menu_id("start", &s.id),
                "Start",
                !s.running,
                None::<&str>,
            )?)?;
            sub.append(&MenuItem::with_id(
                app,
                server_menu_id("stop", &s.id),
                "Stop",
                s.running,
                None::<&str>,
            )?)?;
            sub.append(&MenuItem::with_id(
                app,
                server_menu_id("restart", &s.id),
                "Restart",
                s.running,
                None::<&str>,
            )?)?;
            menu.append(&sub)?;
        }
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "tray_doctor", "Run doctor", true, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "tray_quit", "Exit", true, None::<&str>)?)?;
    Ok(menu)
}

/// Replace the tray menu with one row per server. Best-effort and infallible:
/// the tray is a convenience surface, so a failed rebuild leaves the previous
/// menu standing rather than disturbing whatever the user is doing.
pub fn set_servers(app: &tauri::AppHandle, servers: &[TrayServer]) {
    if let Some(tray) = app.tray_by_id("dml-tray") {
        if let Ok(menu) = build_menu(app, servers) {
            let _ = tray.set_menu(Some(menu));
        }
    }
}

pub fn build(app: &tauri::AppHandle) -> tauri::Result<()> {
    // Starts empty; the frontend pushes the real list via `tray_set_servers`
    // as soon as it has one (it owns the poll — see tray_set_status).
    let menu = build_menu(app, &[])?;

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
        .on_menu_event(|app, event| match parse_menu_id(event.id.as_ref()) {
            Some(TrayRequest::Open) => show_main_window(app),
            // Surface the window and hand the request to the frontend, which
            // runs the SAME act() its Home buttons call. The tray does not
            // drive the lifecycle API itself — one implementation, one place
            // to change, and the streamed output stays visible in the
            // terminal the user is now looking at. The payload carries the
            // server id because the tray can now name several.
            Some(TrayRequest::Action { action, id }) => {
                show_main_window(app);
                let _ = app.emit("tray-action", serde_json::json!({ "action": action, "id": id }));
            }
            // MUST go through app.exit() so the existing RunEvent::Exit arm
            // still fires and clears the keep-awake execution state. A window
            // destroy would bypass it and leave the PC pinned awake.
            Some(TrayRequest::Quit) => app.exit(0),
            None => {}
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

    // --- menu id round-trip (server rename / multi-server tray) -------------
    //
    // The menu id is the ONLY channel between a clicked tray item and the
    // frontend handler, and the id it carries goes on to become a spawn
    // argument. So it is parsed by a closed allowlist, not by string
    // matching at the click site.

    fn srv(id: &str, running: bool) -> TrayServer {
        TrayServer { id: id.into(), display_name: "Dad's Server".into(), running }
    }

    #[test]
    fn menu_ids_round_trip_through_the_parser() {
        for action in ["start", "stop", "restart"] {
            let raw = server_menu_id(action, "wow-server-playerbots");
            match parse_menu_id(&raw) {
                Some(TrayRequest::Action { action: a, id }) => {
                    assert_eq!(a, action);
                    assert_eq!(id.as_deref(), Some("wow-server-playerbots"));
                }
                other => panic!("{raw} parsed as {other:?}"),
            }
        }
    }

    #[test]
    fn open_quit_and_doctor_parse_to_their_own_requests() {
        assert!(matches!(parse_menu_id("tray_open"), Some(TrayRequest::Open)));
        assert!(matches!(parse_menu_id("tray_quit"), Some(TrayRequest::Quit)));
        // doctor is server-independent -- its id is null by contract.
        match parse_menu_id("tray_doctor") {
            Some(TrayRequest::Action { action, id }) => {
                assert_eq!(action, "doctor");
                assert_eq!(id, None);
            }
            other => panic!("doctor parsed as {other:?}"),
        }
    }

    #[test]
    fn unknown_actions_and_unsafe_ids_parse_to_nothing() {
        // Closed allowlist: a new verb has to be added deliberately.
        assert!(parse_menu_id("srv:remove:wow-server-playerbots").is_none());
        assert!(parse_menu_id("srv:start:").is_none());
        assert!(parse_menu_id("srv:start:bad id").is_none());
        assert!(parse_menu_id("srv:start:../escape").is_none());
        assert!(parse_menu_id("srv:start:wow; rm -rf /").is_none());
        assert!(parse_menu_id("srv:start:a:b").is_none());
        assert!(parse_menu_id("").is_none());
        assert!(parse_menu_id("tray_none").is_none());
        assert!(parse_menu_id("nonsense").is_none());
    }

    #[test]
    fn server_labels_name_the_server_and_its_state() {
        assert_eq!(server_label(&srv("wow-server-playerbots", true)), "Dad's Server (running)");
        assert_eq!(server_label(&srv("wow-server-playerbots", false)), "Dad's Server (stopped)");
    }

    #[test]
    fn a_server_with_no_display_name_falls_back_to_its_id() {
        // A blank tray entry would be unclickable in practice -- the label
        // chain must never bottom out at "".
        let mut s = srv("maplestory-server", false);
        s.display_name = "   ".into();
        assert_eq!(server_label(&s), "maplestory-server (stopped)");
    }
}
