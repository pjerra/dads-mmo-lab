//! Start-with-Windows via `HKCU\...\Run`, using `reg.exe`.
//!
//! No new crate: the repo already shells `reg query` (wslconfig.rs) rather
//! than depending on winreg, and this keeps the toggle on the same Rust path
//! as every other launcher.json setting. Chosen over `tauri-plugin-autostart`
//! because that adds a crate AND, if driven from the frontend, needs a
//! permission in capabilities/default.json — whose absence fails silently at
//! runtime with no compile-time signal.
//!
//! This is the repo's first registry WRITE; existing access is a read-only
//! `reg query` for WSL detection.

const KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE: &str = "DML Launcher";

fn reg(args: &[&str]) -> std::io::Result<std::process::Output> {
    let mut cmd = std::process::Command::new("reg");
    cmd.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.output()
}

/// True when a Run entry exists AND still points at a file that exists.
///
/// The existence check matters: dev and installed builds live at different
/// paths (`target\debug\launcher.exe` vs the installed location, and both
/// NSIS and MSI ship), so an entry left behind by a deleted build is a
/// realistic state. Reporting that as "enabled" would show a toggle that is
/// on while nothing actually starts.
pub fn enabled() -> bool {
    let Ok(out) = reg(&["query", KEY, "/v", VALUE]) else {
        return false;
    };
    if !out.status.success() {
        return false;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let Some(line) = text.lines().find(|l| l.contains(VALUE)) else {
        return false;
    };
    // `    DML Launcher    REG_SZ    C:\path\launcher.exe`
    let Some(path) = line.split("REG_SZ").nth(1).map(str::trim) else {
        return false;
    };
    std::path::Path::new(path.trim_matches('"')).is_file()
}

pub fn set(on: bool) -> Result<(), String> {
    if !on {
        // Deleting an absent value returns nonzero; that is success for us.
        let _ = reg(&["delete", KEY, "/v", VALUE, "/f"]);
        return Ok(());
    }
    let exe = std::env::current_exe().map_err(|e| format!("could not resolve the exe path: {e}"))?;
    let exe = exe.to_string_lossy().into_owned();
    let out = reg(&["add", KEY, "/v", VALUE, "/t", "REG_SZ", "/d", &exe, "/f"])
        .map_err(|e| format!("could not run reg.exe: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}
