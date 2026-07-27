//! Dependency-free single-instance guard.
//!
//! Once the app survives window close (close-to-tray), launching the exe
//! again — from the taskbar, the Start menu, or autostart — would start a
//! SECOND app fighting over the same server. `tauri-plugin-single-instance`
//! would solve it but is a new crate, and this plan adds none.
//!
//! Binding a fixed loopback port is atomic: exactly one process can hold it.
//! A second launch fails to bind, connects instead (which wakes the primary
//! into surfacing its window), and exits. The socket doubles as the "focus
//! the existing window" channel, and unlike a lock file there is no stale
//! state to clean up — the OS releases the port when the process dies.

use std::net::{TcpListener, TcpStream};

/// Arbitrary high port, loopback only. Changing it strands running instances
/// (an old instance holds the old port, a new one binds the new port, and
/// both run) — so treat it as a constant, not a setting.
const PORT: u16 = 51789;

/// `Some(listener)` if we are the first instance; `None` if another is live
/// (after poking it so it surfaces its window).
pub fn acquire() -> Option<TcpListener> {
    match TcpListener::bind(("127.0.0.1", PORT)) {
        Ok(l) => Some(l),
        Err(_) => {
            // Best-effort: if the poke fails the other instance is wedged or
            // dying, and exiting anyway is still better than running two.
            let _ = TcpStream::connect(("127.0.0.1", PORT));
            None
        }
    }
}

/// Focus the window whenever another launch pokes us.
pub fn serve(listener: TcpListener, app: tauri::AppHandle) {
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            drop(stream);
            crate::tray::show_main_window(&app);
        }
    });
}
