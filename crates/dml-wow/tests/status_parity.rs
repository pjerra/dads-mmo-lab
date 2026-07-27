//! Live parity gate for the native-mode status/console reads (Task B2, spike:
//! `spike/docker-desktop-native`).
//!
//! Compares `dml::status::{read_server_info, read_server_detail,
//! read_console_tail}`, driven directly against the live SOAP listener /
//! `docker` / MySQL on `127.0.0.1`, against the SAME live server's `dml wow
//! server-info` / `server-detail` / `console-tail --json` arms.
//!
//! SKIP-GUARDED, like `soap_parity.rs`: the server may be down. The harness
//! probes SOAP reachability first (`server info`) — `Unreachable`/`Auth` ->
//! print why and pass, no assertions run. It also needs `bash` + the `dml`
//! script (to run the CLI ground truth) and the native games dir, exactly
//! like `db_pages_parity.rs`/`config_parity.rs`.
//!
//! `server-detail` carries genuinely VOLATILE fields between the two
//! separate calls this test makes (live player count, SOAP mean/median
//! timing stats, bot online count, and — in a razor-thin window — the
//! verdict itself if the server happens to transition mid-test). Per the
//! task brief, only the STABLE fields are deep-compared: `verdict`,
//! container `state`s, `world_ready`, `bots.max`, and `ports`. `console-tail`
//! is even more volatile (a live server continuously appends log lines), so
//! only `available` is compared — content is a best-effort read on both
//! sides, never byte-identical by construction.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use dml_wow::config::ConfigReader;
use dml_wow::db::DbConfig;
use dml_wow::native::docker_program;
use dml_wow::soap::{exec, SoapConfig, SoapOutcome};
use dml_wow::status::{read_console_tail, read_server_detail, read_server_info};

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
    // Fallback probes are per-platform: Git Bash on Windows, the system
    // shell elsewhere. Probing ONLY the Windows locations made every Linux
    // run of this suite skip silently, even with bash at /usr/bin/bash.
    #[cfg(windows)]
    let candidates = [r"C:\Program Files\Git\bin\bash.exe", r"C:\Program Files\Git\usr\bin\bash.exe"];
    #[cfg(not(windows))]
    let candidates = ["/usr/bin/bash", "/bin/bash"];
    for c in candidates {
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
        if p.exists() {
            return Some(p);
        }
    }
    // The vendored tool is yq.exe on Windows, plain yq on a Linux dev box.
    #[cfg(windows)]
    let p = games.join("tools").join("yq.exe");
    #[cfg(not(windows))]
    let p = games.join("tools").join("yq");
    p.exists().then_some(p)
}

/// PATH with bash toolchain + Docker Desktop's bin prepended, so the spawned
/// `dml` finds awk/grep/sed and the `docker`/`curl` it shells for SOAP/DB.
fn augmented_path(bash: &Path) -> OsString {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(bin) = bash.parent() {
        dirs.push(bin.to_path_buf());
        if let Some(root) = bin.parent() {
            dirs.push(root.join("usr").join("bin"));
        }
    }
    let docker = PathBuf::from(r"C:\Users\perzi\AppData\Local\Programs\DockerDesktop\resources\bin");
    if docker.exists() {
        dirs.push(docker);
    }
    if let Some(cur) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&cur));
    }
    std::env::join_paths(dirs).unwrap_or_default()
}

fn run_dml(bash: &Path, script: &Path, games: &Path, yq: &Path, path: &OsString, args: &[&str]) -> serde_json::Value {
    let mut cmd = Command::new(bash);
    cmd.arg(script).args(args).arg("--json");
    cmd.env("DML_GAMES_DIR", games);
    cmd.env("DML_YQ_BIN", yq);
    cmd.env("PATH", path);
    let out = cmd.output().expect("spawn dml under bash");
    let stdout = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str(stdout.trim()).unwrap_or_else(|e| panic!("dml {args:?} output not JSON ({e}): {stdout}"))
}

struct Harness {
    bash: PathBuf,
    script: PathBuf,
    games: PathBuf,
    yq: PathBuf,
    path: OsString,
    soap_cfg: SoapConfig,
    db_cfg: DbConfig,
}

fn harness(label: &str) -> Option<Harness> {
    let soap_cfg = SoapConfig::load();
    match exec(&soap_cfg, "server info") {
        SoapOutcome::Unreachable(_) => {
            eprintln!("SKIP {label}: server not reachable");
            return None;
        }
        SoapOutcome::Auth => {
            eprintln!("SKIP {label}: SOAP auth failed (check ~/.dml/soap.env)");
            return None;
        }
        _ => {}
    }
    let games = games_dir();
    let Some(bash) = find_bash().map(PathBuf::from) else {
        eprintln!("SKIP {label}: no bash (set DML_BASH)");
        return None;
    };
    let Some(script) = find_script() else {
        eprintln!("SKIP {label}: no dml script (set DML_SCRIPT)");
        return None;
    };
    let Some(yq) = yq_path(&games) else {
        eprintln!("SKIP {label}: no yq (set DML_YQ_BIN)");
        return None;
    };
    let path = augmented_path(&bash);
    Some(Harness { bash, script, games, yq, path, soap_cfg, db_cfg: DbConfig::from_env() })
}

#[test]
fn server_info_parity_when_reachable() {
    let Some(h) = harness("server-info parity") else { return };

    let want = run_dml(&h.bash, &h.script, &h.games, &h.yq, &h.path, &["wow", "server-info"]);
    assert_eq!(want["ok"], true, "server-info not ok: {want}");
    let got = read_server_info(&h.soap_cfg).expect("native server-info read (SOAP auth already probed reachable)");

    // Stable fields only: `online` and `version` don't change between two
    // back-to-back calls on a healthy server. players/uptime/mean_ms/
    // median_ms are live stats — genuinely volatile, not compared.
    assert_eq!(got["online"], want["data"]["online"], "server-info online diverged: got={got} want={want}");
    assert_eq!(got["version"], want["data"]["version"], "server-info version diverged: got={got} want={want}");
}

#[test]
fn server_detail_parity_stable_fields_when_reachable() {
    let Some(h) = harness("server-detail parity") else { return };

    let want = run_dml(&h.bash, &h.script, &h.games, &h.yq, &h.path, &["wow", "server-detail"]);
    assert_eq!(want["ok"], true, "server-detail not ok: {want}");

    let program = docker_program();
    // `ConfigReader::from_env()` resolves the title dir off THIS process's
    // own `DML_GAMES_DIR` env var, which the harness never sets (only the
    // spawned `dml` child gets it, via `cmd.env(...)` in `run_dml`) — build
    // the reader directly off the resolved `games` dir instead, exactly
    // like `config_parity.rs` does.
    let mut reader = ConfigReader::for_title(h.games.join("wow-server-playerbots"));
    let got = read_server_detail(&program, &h.soap_cfg, &h.db_cfg, &mut reader);

    assert_eq!(got["verdict"], want["data"]["verdict"], "verdict diverged: got={got} want={want}");
    assert_eq!(got["world_ready"], want["data"]["world_ready"], "world_ready diverged: got={got} want={want}");
    assert_eq!(got["bots"]["max"], want["data"]["bots"]["max"], "bots.max diverged: got={got} want={want}");
    assert_eq!(got["ports"], want["data"]["ports"], "ports diverged: got={got} want={want}");

    let got_containers = got["containers"].as_array().expect("containers array");
    let want_containers = want["data"]["containers"].as_array().expect("containers array");
    assert_eq!(got_containers.len(), want_containers.len(), "container count diverged");
    for (g, w) in got_containers.iter().zip(want_containers.iter()) {
        assert_eq!(g["name"], w["name"], "container name diverged");
        assert_eq!(g["role"], w["role"], "container role diverged");
        assert_eq!(g["state"], w["state"], "container state diverged for {}", g["name"]);
    }

    // exit_code is stable (it's a static value once the container has
    // stopped/crashed — only volatile mid-transition, which the SOAP-reachable
    // gate above already filters past the coldest-boot window).
    assert_eq!(got["exit_code"], want["data"]["exit_code"], "exit_code diverged: got={got} want={want}");
}

#[test]
fn console_tail_parity_availability_when_reachable() {
    let Some(h) = harness("console-tail parity") else { return };

    let want = run_dml(&h.bash, &h.script, &h.games, &h.yq, &h.path, &["wow", "console-tail", "--lines", "50"]);
    assert_eq!(want["ok"], true, "console-tail not ok: {want}");

    let program = docker_program();
    let got = read_console_tail(&program, 50);

    // `available` is the only non-volatile field — live log content differs
    // by construction between two separate calls to a running server.
    assert_eq!(got["available"], want["data"]["available"], "console-tail availability diverged: got={got} want={want}");
    if got["available"] == true {
        let got_lines = got["lines"].as_array().expect("lines array");
        // Sanity: sanitization actually ran (no raw ANSI escape / CR survives).
        for line in got_lines {
            let s = line.as_str().expect("line is a string");
            assert!(!s.contains('\u{1b}'), "unsanitized ANSI escape in console line: {s:?}");
            assert!(!s.contains('\r'), "unsanitized CR in console line: {s:?}");
        }
    }
}
