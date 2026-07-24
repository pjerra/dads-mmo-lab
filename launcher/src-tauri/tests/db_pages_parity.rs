//! Parity gate for the native-mode direct-MySQL page readers (Task 3, spike:
//! `spike/docker-desktop-native`).
//!
//! Asserts that `dml::pages::{read_teleport_list, read_bots, read_accounts}`,
//! reading the LIVE MySQL the native stack publishes on `127.0.0.1:<port>`,
//! assemble JSON that DEEP-EQUALS a real `dml wow teleport-list` / `dml wow bots
//! list` / `dml wow accounts --json` run against the SAME database. This is the
//! load-bearing guarantee for Task 3: the Teleport/Bot-Browser/Accounts pages get
//! byte-for-byte identical data with zero `docker exec` and zero forks.
//!
//! DB-GATED (not just file-gated): unlike the config/module/tuning parity tests
//! (which read files with the engine closed), these readers need the DB UP. The
//! test first TCP-probes the resolved MySQL port; if nothing answers it prints why
//! it skipped and passes, so the suite stays green on a box with no server
//! running — the pure-fn unit tests in `dml::pages` remain the always-run
//! coverage. It also gates on the native files + `bash` + `yq` (to run the CLI
//! ground truth), exactly like `config_parity.rs`. Overridable via `DML_GAMES_DIR`
//! / `DML_BASH` / `DML_SCRIPT` / `DML_YQ_BIN`.
//!
//! NO divergence is tolerated: every field is DB-derived and both sides read the
//! same rows, so an exact deep-equal is required for each of the three surfaces
//! (including the CAST-rendered float coordinates — see `dml::pages` header).

use std::ffi::OsString;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use launcher_lib::dml::db::DbConfig;
use launcher_lib::dml::pages::{
    clamp_limit, read_accounts, read_bots, read_teleport_list, BotFilters,
};

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
        if p.exists() {
            return Some(p);
        }
    }
    let p = games.join("tools").join("yq.exe");
    p.exists().then_some(p)
}

/// PATH with bash toolchain (bin + usr/bin) and Docker Desktop's bin prepended,
/// so the spawned `dml` finds awk/grep/sed and the `docker` it shells for the DB.
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

/// Shared gate: resolve every prerequisite or return `None` (printing why). When
/// `Some`, the DB is confirmed reachable and the CLI is runnable.
struct Harness {
    bash: PathBuf,
    script: PathBuf,
    games: PathBuf,
    yq: PathBuf,
    path: OsString,
    cfg: DbConfig,
}

fn harness(label: &str) -> Option<Harness> {
    let games = games_dir();
    let title_dir = games.join("wow-server-playerbots");
    let override_file = title_dir.join("docker-compose.override.yml");
    if !override_file.is_file() {
        eprintln!("SKIP {label}: no {}", override_file.display());
        return None;
    }
    let Some(bash) = find_bash() else {
        eprintln!("SKIP {label}: no bash (set DML_BASH)");
        return None;
    };
    let bash = PathBuf::from(bash);
    let Some(script) = find_script() else {
        eprintln!("SKIP {label}: no dml script (set DML_SCRIPT)");
        return None;
    };
    let Some(yq) = yq_path(&games) else {
        eprintln!("SKIP {label}: no yq (set DML_YQ_BIN)");
        return None;
    };
    let path = augmented_path(&bash);

    // DB-reachability probe: these readers need the engine UP.
    let cfg = DbConfig::from_env();
    let addr = format!("{}:{}", cfg.host, cfg.port);
    let reachable = addr
        .parse()
        .ok()
        .and_then(|a| TcpStream::connect_timeout(&a, Duration::from_millis(600)).ok())
        .is_some();
    if !reachable {
        eprintln!("SKIP {label}: no DB reachable on {addr} (start the native server)");
        return None;
    }
    Some(Harness { bash, script, games, yq, path, cfg })
}

#[test]
fn teleport_reader_deep_equals_cli() {
    let Some(h) = harness("teleport parity") else { return };

    // No-search: up to 500 rows, ordered by name — the broad float-parity sweep.
    let want = run_dml(&h.bash, &h.script, &h.games, &h.yq, &h.path, &["wow", "teleport-list"]);
    assert_eq!(want["ok"], true, "teleport-list not ok: {want}");
    let got = read_teleport_list(&h.cfg, None).expect("native teleport read");
    assert_eq!(got, want["data"], "teleport reader diverged from `teleport-list` (no search)");

    // Searched: exercises the LIKE WHERE path.
    let want = run_dml(
        &h.bash,
        &h.script,
        &h.games,
        &h.yq,
        &h.path,
        &["wow", "teleport-list", "--search", "Orgri"],
    );
    let got = read_teleport_list(&h.cfg, Some("Orgri")).expect("native teleport read (search)");
    assert_eq!(got, want["data"], "teleport reader diverged from `teleport-list --search`");
}

#[test]
fn bots_reader_deep_equals_cli() {
    let Some(h) = harness("bots parity") else { return };

    // Case A: a plain page (default-ish limit, offset).
    let want =
        run_dml(&h.bash, &h.script, &h.games, &h.yq, &h.path, &["wow", "bots", "list", "--limit", "25", "--offset", "10"]);
    assert_eq!(want["ok"], true, "bots list not ok: {want}");
    let f = BotFilters {
        name: None,
        class: None,
        min_level: None,
        max_level: None,
        online: false,
        limit: clamp_limit(Some(25)),
        offset: 10,
    };
    let got = read_bots(&h.cfg, &f).expect("native bots read");
    assert_eq!(got, want["data"], "bots reader diverged (limit/offset page)");

    // Case B: filters — class + level band.
    let want = run_dml(
        &h.bash,
        &h.script,
        &h.games,
        &h.yq,
        &h.path,
        &["wow", "bots", "list", "--class", "5", "--min-level", "10", "--max-level", "40", "--limit", "30"],
    );
    let f = BotFilters {
        name: None,
        class: Some(5),
        min_level: Some(10),
        max_level: Some(40),
        online: false,
        limit: clamp_limit(Some(30)),
        offset: 0,
    };
    let got = read_bots(&h.cfg, &f).expect("native bots read (filtered)");
    assert_eq!(got, want["data"], "bots reader diverged (class + level band)");
}

#[test]
fn accounts_reader_deep_equals_cli() {
    let Some(h) = harness("accounts parity") else { return };

    let want = run_dml(&h.bash, &h.script, &h.games, &h.yq, &h.path, &["wow", "accounts"]);
    assert_eq!(want["ok"], true, "accounts not ok: {want}");
    let got = read_accounts(&h.cfg).expect("native accounts read");
    assert_eq!(got, want["data"], "accounts reader diverged from `wow accounts`");
}
