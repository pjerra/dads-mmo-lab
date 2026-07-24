//! Parity gate for the native-mode module-**tuning** reader (Task 2, spike:
//! `spike/docker-desktop-native`).
//!
//! Asserts that `dml::tuning::TuningReader`, fed the real
//! `dml wow config tuning-registry --json` rows and reading the real on-disk
//! native files, assembles JSON that DEEP-EQUALS a real
//! `dml wow config tuning-list --json` run against those same files. Every
//! field here is file-derived (conf/.dist/lua + existence checks), so NO field
//! divergence is tolerated — an exact deep-equal is required.
//!
//! FILE/TOOL-GATED like `config_parity.rs`: runs only when the native files +
//! `bash` + `yq` are present (this dev box has them at `C:/Users/perzi/
//! dml-native`); elsewhere it prints why it skipped and passes, leaving the
//! pure-fn unit tests in `dml::tuning` as always-run coverage.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use launcher_lib::dml::tuning::TuningReader;

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
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("cli").join("dml");
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

fn augmented_path(bash: &Path) -> OsString {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(bin) = bash.parent() {
        dirs.push(bin.to_path_buf());
        if let Some(root) = bin.parent() {
            dirs.push(root.join("usr").join("bin"));
        }
    }
    let docker =
        PathBuf::from(r"C:\Users\perzi\AppData\Local\Programs\DockerDesktop\resources\bin");
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
fn native_reader_deep_equals_tuning_list() {
    let games = games_dir();
    let title_dir = games.join("wow-server-playerbots");
    let override_file = title_dir.join("docker-compose.override.yml");

    if !override_file.is_file() {
        eprintln!("SKIP tuning parity: no {}", override_file.display());
        return;
    }
    let Some(bash) = find_bash() else {
        eprintln!("SKIP tuning parity: no bash (set DML_BASH)");
        return;
    };
    let bash = PathBuf::from(bash);
    let Some(script) = find_script() else {
        eprintln!("SKIP tuning parity: no dml script (set DML_SCRIPT)");
        return;
    };
    let Some(yq) = yq_path(&games) else {
        eprintln!("SKIP tuning parity: no yq (set DML_YQ_BIN)");
        return;
    };
    let path = augmented_path(&bash);

    // Ground truth: the real `config tuning-list`.
    let list = run_dml(&bash, &script, &games, &yq, &path, &["wow", "config", "tuning-list"]);
    assert_eq!(list["ok"], true, "tuning-list not ok: {list}");
    let want = &list["data"];
    let want_rows = want["settings"].as_array().expect("tuning-list settings[]");
    assert_eq!(want_rows.len(), 13, "expected 13 tuning rows");

    // Registry rows the Rust command would cache.
    let registry =
        run_dml(&bash, &script, &games, &yq, &path, &["wow", "config", "tuning-registry"]);
    assert_eq!(registry["ok"], true, "tuning-registry not ok: {registry}");
    let rows = registry["data"]["settings"].as_array().cloned().expect("registry settings[]");

    // The reader under test — must deep-equal exactly (no tolerated divergence).
    let mut reader = TuningReader::for_title(&title_dir);
    let got = reader.assemble(&rows);
    assert_eq!(
        got["settings"], want["settings"],
        "tuning reader diverged from `config tuning-list`"
    );
}
