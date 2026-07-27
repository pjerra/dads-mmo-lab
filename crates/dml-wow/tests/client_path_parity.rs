//! Parity gate for the native-mode **client-path get** reader (task D1a,
//! spike: `spike/docker-desktop-native`).
//!
//! `client-path get` is a pure `~/.dml/client-path` file read. In NATIVE
//! mode `dml` itself runs under Git Bash directly on Windows (`dml::
//! runner::DmlRunner::native`, no WSL) — Git Bash's `$HOME` resolves to the
//! same Windows profile dir `USERPROFILE` does, so both readers hit the
//! literal SAME file on disk here, and this asserts `dml::clientpath::
//! read_client_path()` deep-equals a real `dml wow client-path get --json`
//! run. `detect` is NOT compared here: its candidate roots are necessarily
//! Windows-native (no WSL `/mnt/*`, and the real WSL backend's roots don't
//! apply to native mode either), so shape parity (not path-list equality)
//! is the gate for that one — covered by unit tests in `dml::clientpath`
//! instead.
//!
//! FILE/TOOL-GATED, same convention as `module_parity.rs`.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use dml_wow::clientpath::read_client_path;

fn games_dir() -> PathBuf {
    std::env::var_os("DML_GAMES_DIR")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Users\perzi\dml-native"))
}

fn find_bash() -> Option<PathBuf> {
    if let Some(b) = std::env::var_os("DML_BASH").filter(|s| !s.is_empty()) {
        return Some(PathBuf::from(b));
    }
    // Fallback probes are per-platform: Git Bash on Windows, the system
    // shell elsewhere. Probing ONLY the Windows locations made every Linux
    // run of this suite skip silently, even with bash at /usr/bin/bash.
    #[cfg(windows)]
    let candidates = [r"C:\Program Files\Git\bin\bash.exe", r"C:\Program Files\Git\usr\bin\bash.exe"];
    #[cfg(not(windows))]
    let candidates = ["/usr/bin/bash", "/bin/bash"];
    for c in candidates {
        if Path::new(c).exists() {
            return Some(PathBuf::from(c));
        }
    }
    None
}

fn find_script() -> Option<PathBuf> {
    if let Some(s) = std::env::var_os("DML_SCRIPT").filter(|s| !s.is_empty()) {
        return Some(PathBuf::from(s));
    }
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("cli").join("dml");
    p.exists().then_some(p)
}

fn augmented_path(bash: &Path) -> OsString {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(bin) = bash.parent() {
        dirs.push(bin.to_path_buf());
        if let Some(root) = bin.parent() {
            dirs.push(root.join("usr").join("bin"));
        }
    }
    if let Some(cur) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&cur));
    }
    std::env::join_paths(dirs).unwrap_or_default()
}

fn run_dml(bash: &Path, script: &Path, games: &Path, path: &OsString, args: &[&str]) -> serde_json::Value {
    let mut cmd = Command::new(bash);
    cmd.arg(script).args(args).arg("--json");
    cmd.env("DML_GAMES_DIR", games);
    cmd.env("PATH", path);
    let out = cmd.output().expect("spawn dml under bash");
    let stdout = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("dml {args:?} output not JSON ({e}): {stdout}"))
}

#[test]
fn native_reader_deep_equals_wow_client_path_get() {
    let games = games_dir();
    let Some(bash) = find_bash() else {
        eprintln!("SKIP client-path parity: no bash (set DML_BASH)");
        return;
    };
    let Some(script) = find_script() else {
        eprintln!("SKIP client-path parity: no dml script (set DML_SCRIPT)");
        return;
    };
    let path = augmented_path(&bash);

    let want_env = run_dml(&bash, &script, &games, &path, &["wow", "client-path", "get"]);
    assert_eq!(want_env["ok"], true, "wow client-path get not ok: {want_env}");
    let want = &want_env["data"];

    let got = read_client_path();
    assert_eq!(got, *want, "client-path reader diverged from `wow client-path get`");
}
