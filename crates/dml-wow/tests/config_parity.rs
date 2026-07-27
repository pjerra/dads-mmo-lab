//! Parity gate for the native-mode config VALUE reader (Task 2, spike:
//! `spike/docker-desktop-native`).
//!
//! Asserts that `dml::config::ConfigReader`, fed the real
//! `dml wow config registry --json` rows and reading the real on-disk native
//! files, assembles JSON that DEEP-EQUALS a real `dml wow config list --json`
//! run against those same files. This is the load-bearing guarantee: the
//! Settings/Tuning pages get byte-for-byte the same data with zero forks.
//!
//! FILE/TOOL-GATED. It runs only when the native files + `bash` + `yq` are all
//! present (this dev box has them at `C:/Users/perzi/dml-native`); anywhere
//! else it prints why it skipped and passes, so the pure-function unit tests in
//! `dml::config` remain the always-run coverage. Override the location with
//! `DML_GAMES_DIR`, the bash with `DML_BASH`, the CLI with `DML_SCRIPT`, and yq
//! with `DML_YQ_BIN`.
//!
//! server.motd note: bash `list` sources motd from `acore_auth.motd` (a DB
//! read); the file-only reader can't, so it emits the registry default — which
//! is exactly what `list` emits when the DB is unreachable (Docker Desktop
//! closed, the case this native path targets). If the DB happens to be serving
//! a CUSTOM motd during the run, the test tolerates that single documented
//! field divergence and still requires every other row to match.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use dml_wow::config::ConfigReader;

fn games_dir() -> PathBuf {
    std::env::var_os("DML_GAMES_DIR")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Users\perzi\dml-native"))
}

fn find_bash() -> Option<OsString> {
    if let Some(b) = std::env::var_os("DML_BASH").filter(|s| !s.is_empty()) {
        return Some(b);
    }
    for c in [
        r"C:\Program Files\Git\bin\bash.exe",
        r"C:\Program Files\Git\usr\bin\bash.exe",
    ] {
        if Path::new(c).exists() {
            return Some(OsString::from(c));
        }
    }
    None
}

fn find_script() -> Option<PathBuf> {
    if let Some(s) = std::env::var_os("DML_SCRIPT").filter(|s| !s.is_empty()) {
        return Some(PathBuf::from(s));
    }
    // <repo>/cli/dml, relative to crates/dml-wow.
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("cli")
        .join("dml");
    p.exists().then_some(p)
}

fn yq_path(games: &Path) -> Option<PathBuf> {
    if let Some(y) = std::env::var_os("DML_YQ_BIN").filter(|s| !s.is_empty()) {
        let p = PathBuf::from(y);
        return p.exists().then_some(p);
    }
    let p = games.join("tools").join("yq.exe");
    p.exists().then_some(p)
}

/// PATH with the bash toolchain dirs (bin + usr/bin) and Docker Desktop's bin
/// prepended, so the spawned `dml` finds awk/grep/sed and a `docker` that fails
/// GRACEFULLY (motd DB read) rather than being absent in a surprising way.
fn augmented_path(bash: &Path) -> OsString {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(bin) = bash.parent() {
        dirs.push(bin.to_path_buf());
        if let Some(root) = bin.parent() {
            dirs.push(root.join("usr").join("bin"));
        }
    }
    let docker = PathBuf::from(
        r"C:\Users\perzi\AppData\Local\Programs\DockerDesktop\resources\bin",
    );
    if docker.exists() {
        dirs.push(docker);
    }
    if let Some(cur) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&cur));
    }
    std::env::join_paths(dirs).unwrap_or_default()
}

fn run_dml(
    bash: &Path,
    script: &Path,
    games: &Path,
    yq: &Path,
    path: &OsString,
    args: &[&str],
) -> serde_json::Value {
    let mut cmd = Command::new(bash);
    cmd.arg(script).args(args).arg("--json");
    cmd.env("DML_GAMES_DIR", games);
    cmd.env("DML_YQ_BIN", yq);
    cmd.env("PATH", path);
    let out = cmd.output().expect("spawn dml under bash");
    let stdout = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("dml {args:?} output not JSON ({e}): {stdout}"))
}

#[test]
fn native_reader_deep_equals_config_list() {
    let games = games_dir();
    let title_dir = games.join("wow-server-playerbots");
    let override_file = title_dir.join("docker-compose.override.yml");

    // --- gate: everything must be present, else skip (and pass) ---
    if !override_file.is_file() {
        eprintln!("SKIP config parity: no {}", override_file.display());
        return;
    }
    let Some(bash) = find_bash() else {
        eprintln!("SKIP config parity: no bash (set DML_BASH)");
        return;
    };
    let bash = PathBuf::from(bash);
    let Some(script) = find_script() else {
        eprintln!("SKIP config parity: no dml script (set DML_SCRIPT)");
        return;
    };
    let Some(yq) = yq_path(&games) else {
        eprintln!("SKIP config parity: no yq (set DML_YQ_BIN)");
        return;
    };
    let path = augmented_path(&bash);

    // --- ground truth: the real `config list` ---
    let list = run_dml(&bash, &script, &games, &yq, &path, &["wow", "config", "list"]);
    assert_eq!(list["ok"], true, "config list not ok: {list}");
    let want = &list["data"];
    let want_rows = want["settings"].as_array().expect("list settings[]");
    assert_eq!(want_rows.len(), 66, "expected 66 rows from config list");

    // --- registry rows the Rust command would cache ---
    let registry = run_dml(&bash, &script, &games, &yq, &path, &["wow", "config", "registry"]);
    assert_eq!(registry["ok"], true, "config registry not ok: {registry}");
    let rows = registry["data"]["settings"]
        .as_array()
        .cloned()
        .expect("registry settings[]");

    // --- the reader under test ---
    let mut reader = ConfigReader::for_title(&title_dir);
    let got = reader.assemble(&rows);

    if &got["settings"] == &want["settings"] {
        return; // exact deep-equal
    }

    // Tolerate ONLY the documented server.motd DB divergence: every OTHER row
    // must match exactly, and the motd row may differ solely because the live
    // DB served a custom value the file-only reader cannot see.
    let got_rows = got["settings"].as_array().expect("assembled settings[]");
    assert_eq!(got_rows.len(), want_rows.len());
    for (g, w) in got_rows.iter().zip(want_rows.iter()) {
        if g == w {
            continue;
        }
        let key = w["key"].as_str().unwrap_or("");
        assert_eq!(
            key, "server.motd",
            "unexpected non-motd row divergence:\n got: {g}\nwant: {w}"
        );
        // Off-motd fields must still match; only `value` may differ, and only
        // because the DB served something other than the registry default.
        let mut g2 = g.clone();
        let mut w2 = w.clone();
        g2.as_object_mut().unwrap().remove("value");
        let w_val = w2.as_object_mut().unwrap().remove("value");
        assert_eq!(g2, w2, "server.motd differs in a non-value field");
        let default = w["default"].as_str().unwrap_or("");
        let w_val = w_val.as_ref().and_then(|v| v.as_str()).unwrap_or("");
        assert_ne!(
            w_val, default,
            "server.motd value differs but equals its default — that is a real bug, not the DB divergence"
        );
        eprintln!(
            "NOTE config parity: server.motd differs only because the live DB serves {w_val:?}; file-only reader emits the default. Tolerated."
        );
    }
}
