//! Parity gate for the native-mode COMPLEX DB readers (Task 4, spike:
//! `spike/docker-desktop-native`): `dml::stats::read_stats` and
//! `dml::paperdoll::read_paperdoll`.
//!
//! Each reader, over the LIVE MySQL the native stack publishes on
//! `127.0.0.1:<port>`, must assemble JSON that DEEP-EQUALS a real `dml wow stats`
//! / `dml wow paperdoll --char <name>` run against the SAME database.
//!
//! DB-GATED like `db_pages_parity.rs`: the test TCP-probes the resolved MySQL
//! port and skips (printing why, still passing) when nothing answers, so the
//! suite stays green on a box with no server — the pure-fn unit tests in
//! `dml::stats`/`dml::paperdoll` remain the always-run coverage. Also gates on
//! the native files + `bash` + `yq` to run the CLI ground truth.
//!
//! VOLATILITY / "MOTION-RECONCILED" COMPARE. Statistics reads 19 aggregates off
//! a live server whose ambient bots move, save and log in/out constantly, and
//! the CLI ground truth itself takes ~20s (19 `docker exec` queries), so SOME
//! field almost always changes during a run — a naive deep-equal would flake,
//! yet that drift is genuine DB motion, not a code defect. Rather than hardcode
//! which fields are "time-sensitive", the test brackets the CLI read with two
//! native reads (`before`, `after`) and reconciles: every field the two native
//! snapshots AGREE on (stable across the window) MUST equal the CLI exactly; only
//! fields the two snapshots DISAGREE on (proven in motion) may differ from the
//! CLI. A derivation bug shows up as a stable field that mismatches — caught with
//! a precise JSON path. Oscillating counts can occasionally read equal at both
//! ends yet differ mid-window (a false positive), so a run with residual
//! mismatches is retried; a real bug reproduces every attempt, oscillation
//! does not. This validates the WHOLE envelope structurally on every run and the
//! stable majority of it exactly.

use std::ffi::OsString;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use launcher_lib::dml::db::{self, Database, DbConfig};
use launcher_lib::dml::paperdoll::read_paperdoll;
use launcher_lib::dml::stats::read_stats;

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

struct Harness {
    bash: PathBuf,
    script: PathBuf,
    games: PathBuf,
    yq: PathBuf,
    path: OsString,
    cfg: DbConfig,
}

impl Harness {
    fn dml(&self, args: &[&str]) -> serde_json::Value {
        run_dml(&self.bash, &self.script, &self.games, &self.yq, &self.path, args)
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

/// Walk `cli`, `before`, `after` together and collect the JSON paths where the
/// two native snapshots AGREE (`before == after`, a stable field) yet the CLI
/// disagrees — a genuine parity divergence. Fields where the natives disagree
/// were in motion during the CLI run and are tolerated. Recurses through objects
/// (matching keys) and equal-length arrays to pin the exact leaf; a
/// stable-but-mismatching leaf or a stable shape change is reported.
fn reconcile(
    cli: &serde_json::Value,
    before: &serde_json::Value,
    after: &serde_json::Value,
    path: &str,
    out: &mut Vec<String>,
) {
    use serde_json::Value;
    if cli == before {
        return; // matches the pre-CLI snapshot exactly
    }
    // Try to recurse for a precise path.
    if let (Value::Object(c), Value::Object(b), Value::Object(a)) = (cli, before, after) {
        let same_keys = c.len() == b.len()
            && c.len() == a.len()
            && c.keys().all(|k| b.contains_key(k) && a.contains_key(k));
        if same_keys {
            for (k, cv) in c {
                reconcile(cv, &b[k], &a[k], &format!("{path}.{k}"), out);
            }
            return;
        }
    }
    if let (Value::Array(c), Value::Array(b), Value::Array(a)) = (cli, before, after) {
        if c.len() == b.len() && b.len() == a.len() {
            for (i, cv) in c.iter().enumerate() {
                reconcile(cv, &b[i], &a[i], &format!("{path}[{i}]"), out);
            }
            return;
        }
    }
    // Leaf, or a shape that changed. Stable (before==after) but cli differs -> a
    // real divergence; otherwise the field was moving and is tolerated.
    if before == after {
        out.push(format!("{path}: stable native={before} but cli={cli}"));
    }
}

#[test]
fn stats_reader_deep_equals_cli() {
    let Some(h) = harness("stats parity") else { return };

    // Motion-reconciled compare (see header): bracket the CLI with two native
    // reads; stable fields must match the CLI exactly, moving fields are
    // tolerated. Retry to shake off the rare oscillation false positive — a real
    // derivation bug reports the same stable-field path every attempt.
    let mut last: Vec<String> = Vec::new();
    for attempt in 1..=6 {
        let before = read_stats(&h.cfg).expect("native stats read (before)");
        let cli = h.dml(&["wow", "stats"]);
        assert_eq!(cli["ok"], true, "wow stats not ok: {cli}");
        let after = read_stats(&h.cfg).expect("native stats read (after)");

        let mut errs = Vec::new();
        reconcile(&cli["data"], &before, &after, "stats", &mut errs);
        if errs.is_empty() {
            // Every stable field matched the CLI; moving fields were consistent
            // with the observed motion. Parity holds.
            return;
        }
        eprintln!("stats parity attempt {attempt}: {} residual mismatch(es)", errs.len());
        last = errs;
    }
    panic!("stats reader diverged from `wow stats` on stable fields:\n{}", last.join("\n"));
}

#[test]
fn paperdoll_reader_deep_equals_cli() {
    let Some(h) = harness("paperdoll parity") else { return };

    // Find an OFFLINE character that has equipped items (offline so the CLI's
    // online-only SOAP saveall is skipped on both sides — see the reader header).
    let pick = db::query(
        &h.cfg,
        Database::Characters,
        "SELECT c.name FROM characters c \
         JOIN character_inventory ci ON ci.guid=c.guid AND ci.bag=0 AND ci.slot BETWEEN 0 AND 18 \
         WHERE c.online=0 AND c.name REGEXP '^[A-Za-z0-9_]{1,12}$' \
         GROUP BY c.name ORDER BY c.name LIMIT 1;",
    )
    .expect("discovery query");
    let name = match pick.rows.first().and_then(|r| r.first()) {
        Some(db::SqlValue::Text(s)) => s.clone(),
        _ => {
            eprintln!("SKIP paperdoll parity: no offline geared character found");
            return;
        }
    };

    // Stability sandwich (offline chars are near-stable, but a login could still
    // race a save): assert only across a quiescent window.
    for attempt in 1..=6 {
        let before = read_paperdoll(&h.cfg, &name)
            .expect("native paperdoll read (before)")
            .expect("character has gear");
        let cli = h.dml(&["wow", "paperdoll", "--char", &name]);
        assert_eq!(cli["ok"], true, "paperdoll not ok for {name}: {cli}");
        let after = read_paperdoll(&h.cfg, &name)
            .expect("native paperdoll read (after)")
            .expect("character still has gear");

        if before != after {
            eprintln!("paperdoll parity: {name} moved during attempt {attempt}, retrying");
            continue;
        }
        assert_eq!(before, cli["data"], "paperdoll reader diverged for {name}");

        // NOT_FOUND parity: a well-formed but nonexistent name errors NOT_FOUND
        // on the CLI and reads as Ok(None) natively.
        let bogus = "Zzqxnonexist";
        let cli_nf = h.dml(&["wow", "paperdoll", "--char", bogus]);
        assert_eq!(cli_nf["ok"], false, "expected NOT_FOUND for {bogus}: {cli_nf}");
        assert_eq!(cli_nf["error"]["code"], "NOT_FOUND");
        assert!(
            read_paperdoll(&h.cfg, bogus).expect("native bogus read").is_none(),
            "native paperdoll should be None for a nonexistent character"
        );
        return;
    }
    eprintln!("SKIP paperdoll parity: {name} too busy to catch a quiescent window");
}
