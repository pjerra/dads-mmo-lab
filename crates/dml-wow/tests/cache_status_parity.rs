//! Parity gate for the native-mode **cache-status** reader (task D1a, spike:
//! `spike/docker-desktop-native`).
//!
//! `cache-status` reports the size of `~/.dml/wowhead-cache`. In NATIVE mode
//! `dml` runs under Git Bash directly on Windows (no WSL — see `dml::
//! runner::DmlRunner::native`), whose `$HOME` resolves to the same Windows
//! profile dir `USERPROFILE` does, so both readers walk the literal SAME
//! directory here. The bash arm sums bytes via `du -sb` / counts files via
//! `find … | wc -l`; the Rust reader does an equivalent `std::fs` walk —
//! `present`/`files`/`bytes` must match exactly (verified live: both are
//! apparent-size sums over the same regular files).
//!
//! ONE documented divergence: `path`. Git Bash reports its own MSYS view of
//! the directory (`/c/Users/...`), while the native Rust reader reports the
//! real Windows path (`C:\Users\...`) -- the correct, more useful form for a
//! native Windows app (e.g. to hand to Explorer). Same rationale as
//! `config_parity.rs`'s documented `motd` divergence: this ONE field is
//! excluded from the deep-equal and checked separately for "same directory,
//! different OS notation" instead.
//!
//! FILE/TOOL-GATED, same convention as `module_parity.rs`.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use dml_wow::cachestatus::read_cache_status;

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
    for c in [r"C:\Program Files\Git\bin\bash.exe", r"C:\Program Files\Git\usr\bin\bash.exe"] {
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
fn native_reader_deep_equals_wow_cache_status() {
    let games = games_dir();
    let Some(bash) = find_bash() else {
        eprintln!("SKIP cache-status parity: no bash (set DML_BASH)");
        return;
    };
    let Some(script) = find_script() else {
        eprintln!("SKIP cache-status parity: no dml script (set DML_SCRIPT)");
        return;
    };
    let path = augmented_path(&bash);

    let want_env = run_dml(&bash, &script, &games, &path, &["wow", "cache-status"]);
    assert_eq!(want_env["ok"], true, "wow cache-status not ok: {want_env}");
    let want = &want_env["data"]["caches"][0];

    let got = read_cache_status();
    let got = &got["caches"][0];

    assert_eq!(got["key"], want["key"]);
    assert_eq!(got["label"], want["label"]);
    assert_eq!(got["present"], want["present"], "present diverged");
    assert_eq!(got["bytes"], want["bytes"], "bytes diverged");
    assert_eq!(got["files"], want["files"], "files diverged");

    // `path`: same directory, different OS notation -- see module doc comment.
    let want_path = want["path"].as_str().unwrap();
    let got_path = got["path"].as_str().unwrap();
    assert!(
        want_path.ends_with(".dml/wowhead-cache") || want_path.ends_with(".dml\\wowhead-cache"),
        "unexpected CLI path shape: {want_path}"
    );
    assert!(
        got_path.ends_with(".dml\\wowhead-cache"),
        "unexpected native path shape: {got_path}"
    );
}
