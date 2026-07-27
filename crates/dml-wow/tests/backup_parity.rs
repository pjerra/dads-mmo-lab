//! Live parity/smoke gate for the native-mode **backup** family (Chunk 2,
//! task C2a): `dml::backup::{dump_to, list_backups, validate_backup,
//! delete_backup, write_meta}` against the REAL `ac-database` container the
//! bash `dml wow backup ...` CLI also talks to. FILE/DB-GATED, same
//! convention as `db_pages_parity.rs`: skips (prints why, passes) when
//! bash/the `dml` script aren't found or the DB port isn't reachable — the
//! pure-fn unit tests in `dml::backup` remain the always-run coverage.
//!
//! Exercises BOTH directions of the on-disk-format-is-backend-agnostic claim
//! `dml::backup`'s module doc comment makes:
//!   1. bash `dml wow backup create` writes a REAL backup -> the native
//!      reader (`list_backups`/`validate_backup`) must see it correctly,
//!      then native `delete_backup` cleans it up.
//!   2. native `dump_to` writes a REAL backup -> bash `dml wow backup
//!      validate`/`list`/`delete` must see and remove it correctly.
//! Neither test leaves a stray file behind in `~/.dml/backups` on a passing
//! run (each cleans up its own artifact, including on early failure via a
//! best-effort `Drop` guard).

use std::ffi::OsString;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use dml_wow::backup;
use dml_wow::db::DbConfig;
use dml_wow::native::docker_program;

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

/// PATH with bash toolchain (bin + usr/bin) and Docker Desktop's bin
/// prepended, so the spawned `dml` finds awk/grep/sed and the `docker` it
/// shells for `mysqldump`.
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

/// `backup create` is a STREAMED (ndjson) command, not a single `{"ok":...}`
/// envelope: one JSON object per line (`section_start`/`line`/`section_end`/
/// `done`|`error`). Runs it and returns the `done` event's `data` object, or
/// panics with the `error` event's payload if the run failed.
fn run_dml_stream_done(bash: &Path, script: &Path, games: &Path, yq: &Path, path: &OsString, args: &[&str]) -> serde_json::Value {
    let mut cmd = Command::new(bash);
    cmd.arg(script).args(args).arg("--json");
    cmd.env("DML_GAMES_DIR", games);
    cmd.env("DML_YQ_BIN", yq);
    cmd.env("PATH", path);
    let out = cmd.output().expect("spawn dml under bash");
    let stdout = String::from_utf8_lossy(&out.stdout);
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let ev: serde_json::Value =
            serde_json::from_str(line).unwrap_or_else(|e| panic!("dml {args:?} ndjson line not JSON ({e}): {line}"));
        match ev["event"].as_str() {
            Some("done") => return ev["data"].clone(),
            Some("error") => panic!("dml {args:?} reported an error event: {ev}"),
            _ => {}
        }
    }
    panic!("dml {args:?} ndjson stream had no done/error event: {stdout}");
}

struct Harness {
    bash: PathBuf,
    script: PathBuf,
    games: PathBuf,
    yq: PathBuf,
    path: OsString,
}

impl Harness {
    fn dml(&self, args: &[&str]) -> serde_json::Value {
        run_dml(&self.bash, &self.script, &self.games, &self.yq, &self.path, args)
    }

    /// For STREAMED commands (`backup create`) — see [`run_dml_stream_done`].
    fn dml_stream_done(&self, args: &[&str]) -> serde_json::Value {
        run_dml_stream_done(&self.bash, &self.script, &self.games, &self.yq, &self.path, args)
    }
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
    let Some(script) = find_script() else {
        eprintln!("SKIP {label}: no dml script (set DML_SCRIPT)");
        return None;
    };
    let Some(yq) = yq_path(&games) else {
        eprintln!("SKIP {label}: no yq (set DML_YQ_BIN)");
        return None;
    };
    let path = augmented_path(&bash);

    // DB-reachability probe: mysqldump needs the engine (and ac-database) UP.
    let cfg = DbConfig::from_env();
    let addr = format!("{}:{}", cfg.host, cfg.port);
    let reachable = addr.parse().ok().and_then(|a| TcpStream::connect_timeout(&a, Duration::from_millis(600)).ok()).is_some();
    if !reachable {
        eprintln!("SKIP {label}: DB not reachable at {addr}");
        return None;
    }

    Some(Harness { bash, script, games, yq, path })
}

/// Best-effort cleanup: removes a native-side backup + its `.meta` sidecar
/// on drop, so an assertion failure partway through a test doesn't leave a
/// stray file in the REAL `~/.dml/backups`.
struct CleanupGuard(PathBuf);
impl Drop for CleanupGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let _ = std::fs::remove_file(backup::meta_path_for(&self.0));
    }
}

#[test]
fn bash_created_backup_is_readable_and_deletable_natively() {
    let Some(h) = harness("bash-create / native-read+delete") else { return };

    let bdir = backup::backup_dir().expect("resolve backups dir");

    let data = h.dml_stream_done(&["wow", "backup", "create"]);
    let file = data["file"].as_str().expect("file field").to_string();
    // Registered immediately once the file name is known, so a panic in any
    // assertion below still cleans up the real file `backup create` wrote.
    let _guard = CleanupGuard(bdir.join(&file));
    let bash_size = data["size"].as_u64().expect("size field");
    let bash_world = data["world"].as_bool().expect("world field");
    assert!(!bash_world, "plain create must not be a --include-world snapshot");

    // Native list must see the bash-created file with a matching size/shape.
    let entries = backup::list_backups(&bdir);
    let entry = entries.iter().find(|e| e.file == file).unwrap_or_else(|| {
        panic!("native list_backups did not see bash-created backup {file}; saw: {entries:?}")
    });
    assert_eq!(entry.size, bash_size);
    assert!(!entry.world);
    assert!(!entry.created.is_empty());

    // Native validate must accept it as a real, intact character dump.
    let path = bdir.join(&file);
    let v = backup::validate_backup(&path);
    assert!(v.valid, "native validate rejected a real bash-created backup: {v:?}");
    assert!(v.gzip_ok && v.sql_ok);
    assert!(v.markers.contains(&"characters"));
    assert!(v.markers.contains(&"account"));

    // Native delete must remove both the file and its (bash-written) .meta.
    backup::delete_backup(&bdir, &file);
    assert!(!path.is_file(), "native delete left the backup file behind");
    assert!(!backup::meta_path_for(&path).is_file(), "native delete left the .meta sidecar behind");
}

#[test]
fn native_created_backup_is_readable_and_deletable_by_bash() {
    let Some(h) = harness("native-create / bash-read+delete") else { return };

    let bdir = backup::backup_dir().expect("resolve backups dir");
    std::fs::create_dir_all(&bdir).expect("mkdir backups dir");
    let cfg = DbConfig::from_env();
    let program = docker_program();
    let file = backup::new_backup_file_name(false);
    let out_path = bdir.join(&file);
    let _guard = CleanupGuard(out_path.clone());

    backup::dump_to(&program, &cfg.password, false, &out_path)
        .unwrap_or_else(|e| panic!("native dump_to failed against the live DB: {e}"));
    backup::write_meta(&cfg, &out_path, None);
    let expected_size = std::fs::metadata(&out_path).expect("stat native backup").len();
    assert!(expected_size > 0);

    // bash validate must accept the native-created archive.
    let validated = h.dml(&["wow", "backup", "validate", "--file", &file]);
    assert_eq!(validated["ok"], true, "wow backup validate failed: {validated}");
    assert_eq!(validated["data"]["valid"], true, "bash rejected a native-created backup: {validated}");
    assert_eq!(validated["data"]["gzip_ok"], true);
    assert_eq!(validated["data"]["sql_ok"], true);

    // bash list must show it with a matching size and a non-null summary
    // (write_meta above wrote the sidecar bash's own reader must parse).
    let listed = h.dml(&["wow", "backup", "list"]);
    assert_eq!(listed["ok"], true, "wow backup list failed: {listed}");
    let rows = listed["data"]["backups"].as_array().expect("backups array");
    let row = rows.iter().find(|r| r["file"] == file).unwrap_or_else(|| {
        panic!("bash backup list did not see native-created backup {file}; saw: {rows:?}")
    });
    assert_eq!(row["size"].as_u64().unwrap(), expected_size);
    assert_eq!(row["world"], false);
    assert!(!row["summary"].is_null(), "bash did not parse the native-written .meta sidecar: {row}");

    // bash delete cleans it up.
    let deleted = h.dml(&["wow", "backup", "delete", "--file", &file]);
    assert_eq!(deleted["ok"], true, "wow backup delete failed: {deleted}");
    assert!(!out_path.is_file(), "bash delete left the backup file behind");
}
