//! Parity gate for the native-mode **item-info** / **entity-info** readers
//! (task D1b, spike: `spike/docker-desktop-native`).
//!
//! Both backends read/write the SAME `~/.dml/wowhead-cache` tree (same
//! `$HOME`/`USERPROFILE` resolution as `cache_status_parity.rs`), so
//! whichever side runs first here warms the cache the other reads straight
//! back — this compares real wowhead-sourced output, not two independent
//! network fetches racing each other.
//!
//! Uses well-known, stable WotLK ids (item 25 "Worn Shortsword", spell 133
//! "Fireball") that should always resolve `source: "wowhead"` and never
//! need the (untested-here) local-DB fallback.
//!
//! FILE/TOOL/NETWORK-GATED, same convention as `cache_status_parity.rs`:
//! skips cleanly (prints, doesn't fail) when `bash`/the `dml` script isn't
//! found OR there's no live internet.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use dml_wow::db::DbConfig;
use dml_wow::iteminfo::{read_entity_info, read_item_info};
use dml_wow::cachestatus::cache_dir;

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

fn internet_reachable() -> bool {
    std::net::TcpStream::connect_timeout(&"1.1.1.1:443".parse().unwrap(), Duration::from_millis(500)).is_ok()
}

#[test]
fn native_reader_matches_wow_item_info_for_a_known_item() {
    if !internet_reachable() {
        eprintln!("SKIP item-info parity: no internet");
        return;
    }
    let games = games_dir();
    let Some(bash) = find_bash() else {
        eprintln!("SKIP item-info parity: no bash (set DML_BASH)");
        return;
    };
    let Some(script) = find_script() else {
        eprintln!("SKIP item-info parity: no dml script (set DML_SCRIPT)");
        return;
    };
    let path = augmented_path(&bash);

    let want_env = run_dml(&bash, &script, &games, &path, &["wow", "item-info", "--entries", "25"]);
    assert_eq!(want_env["ok"], true, "wow item-info not ok: {want_env}");
    let want = &want_env["data"]["items"][0];

    let cache_root = cache_dir().expect("resolve wowhead cache dir");
    let db_cfg = DbConfig::from_env();
    let got_env = read_item_info(&cache_root, Some(&db_cfg), &[25]);
    let got = &got_env["items"][0];

    assert_eq!(got["entry"], want["entry"]);
    assert_eq!(got["source"], want["source"], "source diverged: got {got} want {want}");
    if want["source"] == "wowhead" {
        assert_eq!(got["icon"], want["icon"]);
        assert_eq!(got["wowhead"]["name"], want["wowhead"]["name"]);
        // display_id: same cache file either side ends up reading -- must
        // agree on presence and value, not just on being "some number".
        assert_eq!(got.get("display_id"), want.get("display_id"));
    }
}

#[test]
fn native_reader_matches_wow_entity_info_for_a_known_spell() {
    if !internet_reachable() {
        eprintln!("SKIP entity-info parity: no internet");
        return;
    }
    let games = games_dir();
    let Some(bash) = find_bash() else {
        eprintln!("SKIP entity-info parity: no bash (set DML_BASH)");
        return;
    };
    let Some(script) = find_script() else {
        eprintln!("SKIP entity-info parity: no dml script (set DML_SCRIPT)");
        return;
    };
    let path = augmented_path(&bash);

    let want_env =
        run_dml(&bash, &script, &games, &path, &["wow", "entity-info", "--kind", "spell", "--ids", "133"]);
    assert_eq!(want_env["ok"], true, "wow entity-info not ok: {want_env}");
    let want = &want_env["data"]["entities"][0];

    let cache_root = cache_dir().expect("resolve wowhead cache dir");
    let got_env = read_entity_info(&cache_root, "spell", &[133]);
    let got = &got_env["entities"][0];

    assert_eq!(got["id"], want["id"]);
    assert_eq!(got["source"], want["source"], "source diverged: got {got} want {want}");
    if want["source"] == "wowhead" {
        assert_eq!(got["icon"], want["icon"]);
        assert_eq!(got["wowhead"]["name"], want["wowhead"]["name"]);
    }
}
