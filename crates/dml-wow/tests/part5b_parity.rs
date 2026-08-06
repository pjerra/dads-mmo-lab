//! Live parity gate for Part 5b of the mop-up brief (spike:
//! `spike/docker-desktop-native`): `module conf`/`module tracking`.
//!
//! Same SKIP-GUARDED shape as `part5a_parity.rs`/`module_parity.rs`: needs
//! `bash` + the `dml` script + the native games dir (default
//! `C:\Users\perzi\dml-native`, override with `DML_GAMES_DIR`/`DML_BASH`/
//! `DML_SCRIPT`) to run the CLI ground truth, plus a live `ac-database` for
//! `module tracking`'s DB reads.
//!
//! SCOPE. Only the two READ-ONLY module-tail pieces are live-tested here
//! (`module conf`'s state derivation and `module tracking`'s LIKE/exact-count
//! reads) — both cheap and side-effect-free. The rest of Part 5b is
//! deliberately NOT live-exercised in this file, documented in the task
//! report:
//!   - `module repair`/`module fixit`/`module place-npc` all MUTATE real
//!     tables (`updates` tracking rows / `creature`+`creature_template`
//!     spawns) with no undo path available under this task's "no rebuild/
//!     restore/flush/remove" constraint — exercising them live on the shared
//!     disposable snapshot would leave permanent state behind with no way to
//!     clean it up here, so they stay unit-tested-only (see `dml::
//!     moduletail`'s pure-logic tests).
//!   - `module update-check` fetches every installed module's origin over
//!     the network (same cost class the mop-up brief's own pb-keys/conf-keys
//!     finding flags as "not cheap") — covered by `dml::maint::
//!     read_repo_check`'s existing tests + `part5a_parity.rs`'s
//!     `wow_update_check_read` coverage (same underlying function).
//!   - `party add`/`kick`/`dismiss-all`/`relogin`/`botcmd` all require a
//!     REAL online character in a live WoW client, which this non-
//!     interactive session cannot drive — covered by `dml::party`'s pure
//!     command-string-builder/validator/preset-file unit tests instead.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use dml_wow::db::{self, Database, DbConfig};
use dml_wow::moduletail;

fn games_dir() -> PathBuf {
    std::env::var_os("DML_GAMES_DIR")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Users\perzi\dml-native"))
}

/// This suite's own [`DbConfig`], resolved against [`games_dir`] instead of
/// the ambient `DML_GAMES_DIR`. Schema names come from the server's own
/// config now, so the in-process half has to be told WHICH server — the same
/// one `games_dir()` hands the bash oracle. Without this an unset
/// `DML_GAMES_DIR` leaves `db_names` `None` and every native read refuses
/// with `DB_NAMES_UNRESOLVED` while bash answers normally, which reads
/// exactly like a parity regression and is not one.
fn live_db_config() -> DbConfig {
    DbConfig::for_title(&games_dir().join(dml_wow::config::TITLE))
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

struct Harness {
    bash: PathBuf,
    script: PathBuf,
    games: PathBuf,
    path: OsString,
}

fn harness(label: &str) -> Option<Harness> {
    let games = games_dir();
    let Some(bash) = find_bash().map(PathBuf::from) else {
        eprintln!("SKIP {label}: no bash (set DML_BASH)");
        return None;
    };
    let Some(script) = find_script() else {
        eprintln!("SKIP {label}: no dml script (set DML_SCRIPT)");
        return None;
    };
    if !games.join("wow-server-playerbots").is_dir() {
        eprintln!("SKIP {label}: no native title dir at {}", games.display());
        return None;
    }
    let path = augmented_path(&bash);
    Some(Harness { bash, script, games, path })
}

fn run_dml(h: &Harness, args: &[&str]) -> serde_json::Value {
    let mut cmd = Command::new(&h.bash);
    cmd.arg(&h.script).args(args).arg("--json");
    cmd.env("DML_GAMES_DIR", &h.games);
    cmd.env("PATH", &h.path);
    let out = cmd.output().expect("spawn dml under bash");
    let stdout = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str(stdout.trim()).unwrap_or_else(|e| panic!("dml {args:?} output not JSON ({e}): {stdout}"))
}

/// `mod-transmog` is installed on this dev box's native title dir (verified
/// via `Get-ChildItem …/modules` while writing this task) with an ACTIVE
/// conf (`transmog.conf` under `env/dist/etc/modules/`), so it exercises the
/// "active" branch of `module_conf_state` end-to-end against the real
/// filesystem the CLI itself reads.
const INSTALLED_KEY: &str = "mod-transmog";

// ---------------------------------------------------------------------------
// module conf (state derivation)
// ---------------------------------------------------------------------------

#[test]
fn module_conf_state_native_matches_cli_for_installed_module() {
    let Some(h) = harness("module conf parity") else { return };
    let want = run_dml(&h, &["wow", "module", "conf", "--key", INSTALLED_KEY]);
    assert_eq!(want["ok"], true, "module conf not ok: {want}");

    let sdir = h.games.join("wow-server-playerbots");
    let got_conf_name = moduletail::module_conf_name(INSTALLED_KEY);
    let got_state = moduletail::module_conf_state(&sdir, INSTALLED_KEY);

    assert_eq!(want["data"]["key"].as_str().unwrap(), INSTALLED_KEY);
    assert_eq!(got_conf_name, want["data"]["conf_name"].as_str(), "conf_name diverged");
    assert_eq!(got_state, want["data"]["state"].as_str().unwrap(), "state diverged");
}

// ---------------------------------------------------------------------------
// module tracking (read-only LIKE/exact-count reads)
// ---------------------------------------------------------------------------

/// Reproduces `wow_module_tracking_native`'s exact query sequence (see
/// `lib.rs`) directly against the live DB, so this test doesn't need a Tauri
/// `AppState` to exercise the same bound-param SQL the real command runs.
fn native_tracking(cfg: &DbConfig, sdir: &Path, key: &str) -> serde_json::Value {
    let (stripped, term1) = moduletail::tracking_like_terms(key);
    let mut dbs = serde_json::Map::new();
    for db_short in ["world", "characters", "auth"] {
        let database = moduletail::database_for_short(db_short).unwrap();
        let params: Vec<mysql::Value> =
            vec![mysql::Value::from(format!("%{stripped}%")), mysql::Value::from(format!("%{term1}%"))];
        let res = db::query_with_params(cfg, database, moduletail::TRACKING_LIKE_SQL, params)
            .unwrap_or_else(|e| panic!("tracking LIKE query failed for {db_short}: {e}"));
        let tracked_rows: Vec<String> = res
            .rows
            .iter()
            .filter_map(|r| r.first())
            .filter_map(|v| match v {
                db::SqlValue::Text(s) => Some(s.clone()),
                db::SqlValue::Int(i) => Some(i.to_string()),
                db::SqlValue::Null => None,
            })
            .collect();

        let mut files = Vec::new();
        for f in moduletail::module_discover_sql_files(sdir, key, db_short) {
            if !moduletail::valid_module_sql_filename(&f) {
                continue;
            }
            let params: Vec<mysql::Value> = vec![mysql::Value::from(&f)];
            let cnt_res = db::query_with_params(cfg, database, moduletail::TRACKING_EXACT_COUNT_SQL, params)
                .unwrap_or_else(|e| panic!("tracking exact-count query failed for {f}: {e}"));
            let cnt: i64 = cnt_res
                .rows
                .first()
                .and_then(|r| r.first())
                .and_then(|v| match v {
                    db::SqlValue::Int(i) => Some(*i),
                    db::SqlValue::Text(s) => s.parse().ok(),
                    db::SqlValue::Null => None,
                })
                .unwrap_or(0);
            files.push(serde_json::json!({"name": f, "tracked": cnt > 0}));
        }
        dbs.insert(db_short.to_string(), serde_json::json!({"tracked_rows": tracked_rows, "files": files}));
    }
    serde_json::json!({"key": key, "dbs": serde_json::Value::Object(dbs)})
}

/// Both sides' file lists, normalised to byte order by name.
///
/// The bash oracle sorts with `sort(1)`, whose collation follows the CALLER'S
/// LOCALE: under en_US.UTF-8 it is case-insensitive
/// (`..._NPC.sql`, `..._texts.sql`, `..._VendorItems.sql`) while under
/// LC_ALL=C it is byte order (`..._NPC.sql`, `..._VendorItems.sql`,
/// `..._texts.sql`) — the latter being what the native reader's `Vec::sort`
/// always produces. Comparing the arrays positionally therefore made this test
/// pass or fail on the DEVELOPER'S LOCALE rather than on the code: it passed
/// for years only because no installed module shipped SQL files whose
/// case-insensitive and byte orders disagreed, and mod-transmog's
/// `trasm_world_VendorItems.sql` broke that tie.
///
/// The file SET and each file's `tracked` flag are the contract; the order is
/// an artifact of the oracle's environment. Returns `None` for a non-array so
/// a present-vs-missing divergence still fails (None != Some).
fn files_by_name(v: &serde_json::Value) -> Option<Vec<serde_json::Value>> {
    let mut rows = v.as_array()?.clone();
    rows.sort_by(|a, b| a["name"].as_str().unwrap_or("").cmp(b["name"].as_str().unwrap_or("")));
    Some(rows)
}

#[test]
fn module_tracking_native_matches_cli_for_installed_module() {
    let Some(h) = harness("module tracking parity") else { return };
    if db::connect(&live_db_config(), Database::World).is_err() {
        eprintln!("SKIP module tracking parity: ac-database not reachable");
        return;
    }
    let want = run_dml(&h, &["wow", "module", "tracking", "--key", INSTALLED_KEY]);
    assert_eq!(want["ok"], true, "module tracking not ok: {want}");

    let sdir = h.games.join("wow-server-playerbots");
    let cfg = live_db_config();
    let got = native_tracking(&cfg, &sdir, INSTALLED_KEY);

    assert_eq!(got["key"], want["data"]["key"], "key diverged");
    for db_short in ["world", "characters", "auth"] {
        assert_eq!(
            got["dbs"][db_short]["tracked_rows"], want["data"]["dbs"][db_short]["tracked_rows"],
            "{db_short} tracked_rows diverged"
        );
        assert_eq!(
            files_by_name(&got["dbs"][db_short]["files"]),
            files_by_name(&want["data"]["dbs"][db_short]["files"]),
            "{db_short} files diverged"
        );
    }
}
