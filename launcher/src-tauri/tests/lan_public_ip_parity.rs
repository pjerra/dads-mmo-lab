//! Parity gate for the native-mode **lan public-ip** reader (task D1a,
//! spike: `spike/docker-desktop-native`).
//!
//! Both readers hit the real internet (`curl`/`reqwest` -> api.ipify.org),
//! so this is inherently less deterministic than the pure-filesystem parity
//! tests: TOOL-GATED like the others, plus it treats a `null` from either
//! side as "network unavailable, skip" rather than a hard failure — the
//! shape contract (`{"public_ip": string-or-null}`) is exercised by the
//! `dml::lanip` unit tests; this test's job is just to catch a real
//! validation-logic divergence (e.g. the native reader accepting/rejecting
//! a shape the CLI wouldn't) when both sides DO get an answer.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use dml_wow::lanip::fetch_public_ip;

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
fn native_reader_agrees_with_wow_lan_public_ip_when_both_reach_the_network() {
    let games = games_dir();
    let Some(bash) = find_bash() else {
        eprintln!("SKIP lan-public-ip parity: no bash (set DML_BASH)");
        return;
    };
    let Some(script) = find_script() else {
        eprintln!("SKIP lan-public-ip parity: no dml script (set DML_SCRIPT)");
        return;
    };
    let path = augmented_path(&bash);

    let want_env = run_dml(&bash, &script, &games, &path, &["wow", "lan", "public-ip"]);
    assert_eq!(want_env["ok"], true, "wow lan public-ip not ok: {want_env}");
    let want_ip = want_env["data"]["public_ip"].as_str().map(str::to_string);

    let got_ip = fetch_public_ip();

    match (want_ip, got_ip) {
        (Some(w), Some(g)) => assert_eq!(g, w, "public IP diverged between CLI and native reader"),
        (want, got) => eprintln!(
            "SKIP lan-public-ip parity: at least one side got no answer (cli={want:?}, native={got:?})"
        ),
    }
}
