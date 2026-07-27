//! Parity gate for the native-mode **commands** reader (task D1a, spike:
//! `spike/docker-desktop-native`).
//!
//! Asserts that `dml::commands::assemble_commands`, fed the real `dml wow
//! module catalog --json` `.data` and reading the real on-disk native
//! modules dir (+ `dml::modules::ModuleReader` for install-state), assembles
//! JSON that DEEP-EQUALS a real `dml wow commands --json` run against the
//! same files.
//!
//! FILE/TOOL-GATED, same convention as `module_parity.rs`: runs only when
//! the native title dir + `bash` are present (this dev box has them at
//! `C:/Users/perzi/dml-native`); elsewhere it prints why it skipped and
//! passes, leaving the pure-fn unit tests in `dml::commands` as always-run
//! coverage. No yq/docker needed (pure file + the cached catalog fetch).

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use dml_wow::commands::assemble_commands;
use dml_wow::modules::ModuleReader;

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
fn native_reader_deep_equals_wow_commands() {
    let games = games_dir();
    let title_dir = games.join("wow-server-playerbots");
    let override_file = title_dir.join("docker-compose.override.yml");
    if !override_file.is_file() {
        eprintln!("SKIP commands parity: no {}", override_file.display());
        return;
    }
    let Some(bash) = find_bash() else {
        eprintln!("SKIP commands parity: no bash (set DML_BASH)");
        return;
    };
    let Some(script) = find_script() else {
        eprintln!("SKIP commands parity: no dml script (set DML_SCRIPT)");
        return;
    };
    let path = augmented_path(&bash);

    let want_env = run_dml(&bash, &script, &games, &path, &["wow", "commands"]);
    assert_eq!(want_env["ok"], true, "wow commands not ok: {want_env}");
    let want = &want_env["data"];

    let catalog_env = run_dml(&bash, &script, &games, &path, &["wow", "module", "catalog"]);
    assert_eq!(catalog_env["ok"], true, "module catalog not ok: {catalog_env}");
    let catalog = &catalog_env["data"];

    let reader = ModuleReader::for_title(&title_dir);
    let modules_dir = title_dir.join("modules");
    let got = assemble_commands(catalog, &reader, &modules_dir);

    assert_eq!(got, *want, "commands reader diverged from `wow commands`");
}
