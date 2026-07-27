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

/// Handshake so we only ever defer to ANOTHER DML LAUNCHER.
///
/// The port sits in Windows' dynamic range (49152-65535), so an unrelated
/// process can hold it. Without this check that stranger would make the
/// launcher exit silently on every double-click — indistinguishable from a
/// broken install, and permanent until reboot. It also stops any local
/// process from popping our window by connecting.
const HELLO: &[u8] = b"dml-launcher-1\n";

/// The three genuinely different outcomes. Collapsing the last two into
/// "None" would make the app exit silently when an unrelated process happens
/// to hold the port.
pub enum Instance {
    /// We are the only instance. Hold this listener for the app's lifetime.
    First(TcpListener),
    /// A real sibling answered and has been asked to surface. Exit quietly.
    AlreadyRunning,
    /// The port belongs to something that is not us. Start anyway, unguarded.
    PortUnavailable,
}

pub fn acquire() -> Instance {
    match TcpListener::bind(("127.0.0.1", PORT)) {
        Ok(l) => Instance::First(l),
        Err(_) if poke_existing_instance() => Instance::AlreadyRunning,
        // Someone else owns the port. Carry on WITHOUT the guard rather than
        // refusing to start: a second window is a far smaller failure than an
        // app that will not launch at all.
        Err(_) => Instance::PortUnavailable,
    }
}

/// Connect and exchange the handshake, RETRYING for a few seconds.
///
/// The retry is load-bearing, not politeness. The listener only starts
/// accepting inside Tauri's `.setup()`, hundreds of milliseconds after the
/// first instance binds the port. A second launch during that window
/// completes its TCP connect against the kernel backlog, gets no reply
/// because nobody is accepting yet, and would conclude "stranger on the
/// port" — starting a second full app, which is exactly what this guard
/// exists to prevent. Retrying spans the gap; a genuine stranger just costs
/// a few seconds before we start unguarded.
fn poke_existing_instance() -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(4);
    while std::time::Instant::now() < deadline {
        if handshake_once() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    false
}

fn handshake_once() -> bool {
    use std::io::{Read, Write};
    let Ok(mut s) = TcpStream::connect(("127.0.0.1", PORT)) else {
        return false;
    };
    let _ = s.set_read_timeout(Some(std::time::Duration::from_millis(400)));
    if s.write_all(HELLO).is_err() {
        return false;
    }
    let mut buf = [0u8; HELLO.len()];
    matches!(s.read_exact(&mut buf), Ok(())) && buf == HELLO
}

/// Focus the window whenever another launcher pokes us.
pub fn serve(listener: TcpListener, app: tauri::AppHandle) {
    use std::io::{Read, Write};
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            // An accept error is not a poke; treat only real connections as
            // one, or a transient error would raise the window unbidden.
            let Ok(mut s) = stream else { continue };
            let app = app.clone();
            // One short-lived thread per connection: a stranger that connects
            // and then goes silent must not hold the accept loop for its read
            // timeout and delay a real sibling's window-surfacing.
            std::thread::spawn(move || {
                let _ = s.set_read_timeout(Some(std::time::Duration::from_millis(500)));
                let mut buf = [0u8; HELLO.len()];
                if s.read_exact(&mut buf).is_ok() && buf == HELLO {
                    let _ = s.write_all(HELLO);
                    crate::tray::show_main_window(&app);
                }
            });
        }
    });
}
