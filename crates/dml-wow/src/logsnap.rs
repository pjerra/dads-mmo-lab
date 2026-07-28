//! Pre-stop/-restart **worldserver log snapshots** (incident follow-up 3).
//!
//! WHY THIS EXISTS. `docker compose down` + `up -d` RECREATES the containers,
//! and a recreated container starts with an EMPTY log — everything the old
//! worldserver printed is gone the moment the restart runs. During the
//! 2026-07-21 freeze incident that destroyed the evidence TWICE: the restart
//! the operator reached for to fix the freeze was also the thing that erased
//! the reason for it. So every stop/restart now leaves a bounded tail of the
//! worldserver log on disk under `~/.dml/logs` first.
//!
//! FOUR RULES, all of them load-bearing:
//!   0. SCOPED TO THE TITLE BEING STOPPED. The container is resolved through
//!      that title's OWN compose project ([`resolve_world_container`]), never
//!      by the global name `ac-worldserver` — that name answers for whichever
//!      title happens to own it, so stopping a non-WoW title while the WoW
//!      stack is up saved the WoW log under the wrong name and evicted the
//!      real evidence from the shared newest-N window.
//!   1. BOUNDED. [`SNAPSHOT_TAIL_LINES`] lines, then a hard
//!      [`SNAPSHOT_MAX_BYTES`] byte cap — see each constant for the sizing
//!      argument. A whole `docker logs` dump of a long-lived world is
//!      unbounded output, which is also the pipe-buffer deadlock hazard
//!      `dml_core::proc::output_bounded_draining` exists for.
//!   2. NON-FATAL. Nothing here can fail a stop or a restart. A failure is one
//!      `warn` line and the lifecycle continues — same doctrine as
//!      `lifecycle::auto_backup_before_stop` ("never aborts the stop/restart").
//!   3. RETAINED WITH A CAP. Newest-N retention modelled on `backup::prune`
//!      (`DML_LOG_SNAPSHOT_KEEP`, default 10), so `~/.dml/logs` cannot grow
//!      without bound on a box that restarts many times a day.
//!
//! SPLIT, like every other module here: [`save_snapshot`] is the whole
//! decision + write + prune, pure enough to unit-test against real files with
//! no docker at all; [`snapshot_world_log_into`] is the thin live wrapper that
//! only adds the resolution + the bounded `docker logs` read (its directory and
//! retention come from the caller's `lifecycle::LifecycleEnv`, never from a
//! lookup of its own). The bash twin lives in
//! `cli/src/90-main.sh` (`_snapshot_world_log`) and must stay behaviourally
//! identical — WSL mode runs bash, native mode runs this, and a snapshot that
//! only exists on one of them half-ships the feature.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use dml_core::proc::{output_bounded_draining, windows_no_window};

/// `docker logs --tail N`. WHY 2000: the console page already reads 1000 lines
/// for display (`status::read_console_tail`), and a boot loop's evidence is one
/// boot banner plus a few hundred lines of failure PER CYCLE — 2000 keeps
/// several cycles, so the snapshot shows the pattern and not just the last
/// death. At AzerothCore's line lengths that is a few hundred KB, small enough
/// to write during a stop the user is waiting on.
pub const SNAPSHOT_TAIL_LINES: u32 = 2000;

/// Hard byte cap on what lands on disk, applied AFTER the line cap. The line
/// cap alone is not a size bound: one pathological line (a stack dump, a
/// binary splat down stderr) can be arbitrarily long. 2 MiB is ~10x the
/// expected size of [`SNAPSHOT_TAIL_LINES`] real log lines, so it never
/// truncates a normal snapshot and still bounds the worst case.
pub const SNAPSHOT_MAX_BYTES: usize = 2 * 1024 * 1024;

/// Bounded budget for the `docker logs` read itself — the same budget
/// `status::read_console_tail` gets for its comparable `--tail 1000` read, and
/// for the same reason: this is the one call here that can move real bytes.
/// `DML_LOG_SNAPSHOT_TIMEOUT` overrides it (seconds) on BOTH surfaces; it
/// exists because a wedged dockerd accepts the socket-activated connection and
/// then never answers, and a stop the user asked for must not wait on that.
pub const SNAPSHOT_TIMEOUT_SECS: u64 = 20;

/// Bounded budget for the container-resolution call. Its own, shorter budget:
/// `compose ps -q` moves one line and is answered from compose's metadata, so
/// anything slower than this is the daemon being unreachable, not a big read.
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(10);

/// The compose SERVICE this module snapshots. Only the WORLD server is
/// captured: it is the one that carries the boot/crash evidence.
///
/// WHY A SERVICE AND NOT A CONTAINER NAME. `docker logs ac-worldserver`
/// answers for whichever title happens to own that container, so stopping a
/// non-WoW title while the WoW stack is up used to save the WoW world's log
/// under the other title's name — and, because retention is one shared
/// newest-N pool, evict the genuine WoW evidence this module exists to keep.
/// The container is therefore resolved through the stopping title's OWN
/// compose project ([`resolve_world_container`]); a project that owns no such
/// container takes no snapshot at all.
pub const WORLD_SERVICE: &str = "ac-worldserver";

/// File-name prefix + extension. The retention prune is a plain descending
/// NAME sort (see [`snapshot_names_desc`]), so the pool it sorts MUST be
/// prefix-scoped — a foreign `.log` file would neither sort chronologically
/// alongside these nor deserve a keep slot.
pub const SNAPSHOT_PREFIX: &str = "world-";
pub const SNAPSHOT_EXT: &str = ".log";

/// What [`save_snapshot`] did. `Skipped` is NOT a failure: it means there was
/// nothing to preserve (this title's compose project owns no world container,
/// docker down, empty log, retention turned off), which is the normal case for
/// every non-WoW title and must stay silent.
#[derive(Debug, PartialEq, Eq)]
pub enum SnapshotOutcome {
    Saved { file: String, pruned: Vec<String> },
    Skipped,
    Failed,
}

/// `~/.dml/logs` — a new sibling of `~/.dml/backups`, created on demand by the
/// first snapshot (no existing install has it).
pub fn logs_dir() -> Option<PathBuf> {
    super::dml_home_dir().map(|h| h.join("logs"))
}

/// Keep the newest N snapshots — `DML_LOG_SNAPSHOT_KEEP`, default 10, the same
/// retention shape and default as `backup::backup_keep_from_env`. Any
/// unset/unparseable value falls back to the default (bash parameter-expansion
/// semantics: the override is never validated there either).
pub fn snapshot_keep_from_env() -> usize {
    std::env::var("DML_LOG_SNAPSHOT_KEEP").ok().and_then(|v| v.trim().parse().ok()).unwrap_or(10)
}

/// Bound on the `docker logs` read — `DML_LOG_SNAPSHOT_TIMEOUT` seconds,
/// default [`SNAPSHOT_TIMEOUT_SECS`]. Unset/unparseable falls back to the
/// default (same lenient shape as `snapshot_keep_from_env`), and so does `0`:
/// `timeout 0` means NO limit to the bash twin's coreutils `timeout(1)` while
/// it would mean "no time at all" here, and one value with two opposite
/// meanings across the surfaces is worse than not accepting it.
pub fn snapshot_timeout_from_env() -> Duration {
    let secs = std::env::var("DML_LOG_SNAPSHOT_TIMEOUT")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|s| *s > 0)
        .unwrap_or(SNAPSHOT_TIMEOUT_SECS);
    Duration::from_secs(secs)
}

/// Title ids reach this from the caller's argv. Anything outside
/// `[A-Za-z0-9._-]` becomes `_` so the id can only ever contribute a file NAME
/// — never a path traversal, never a separator — matching the bash twin's
/// `${title//[^A-Za-z0-9._-]/_}`.
pub fn sanitize_title(title: &str) -> String {
    title.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') { c } else { '_' }).collect()
}

/// `world-<YYYYMMDD-HHMMSS>-<title>-<mode>.log`.
///
/// TIMESTAMP BEFORE TITLE, deliberately: retention sorts these names as plain
/// strings (no `stat` calls, so bash and Rust agree byte for byte), and a
/// name that led with the title would sort by title first — two titles'
/// snapshots would interleave and the "newest N" prune would delete the wrong
/// files. Same trick as `backup::new_backup_file_name`'s fixed `wow-<ts>`
/// prefix.
pub fn snapshot_file_name_at(unix_secs: u64, title: &str, mode: &str) -> String {
    let ts = super::backup::format_utc_compact(unix_secs);
    format!("{SNAPSHOT_PREFIX}{ts}-{}-{}{SNAPSHOT_EXT}", sanitize_title(title), sanitize_title(mode))
}

/// Every snapshot directly under `dir`, newest-name-first. Missing/unreadable
/// `dir` degrades to empty (matches the bash's `ls … 2>/dev/null`).
pub fn snapshot_names_desc(dir: &Path) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut names: Vec<String> = rd
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.starts_with(SNAPSHOT_PREFIX) && n.ends_with(SNAPSHOT_EXT))
        .collect();
    names.sort_by(|a, b| b.cmp(a));
    names
}

/// Names beyond the newest `keep` in an already-sorted-descending list — the
/// pure core of the prune, same shape as `backup::prune_names`.
pub fn prune_names(sorted_desc: &[String], keep: usize) -> &[String] {
    if sorted_desc.len() <= keep {
        &[]
    } else {
        &sorted_desc[keep..]
    }
}

/// Delete every snapshot past the retention window **except `fresh`** (the one
/// just written), returning the pruned names (newest-of-the-pruned first).
/// Best-effort per file (`rm -f` semantics).
///
/// WHY `fresh` IS EXCLUDED FROM THE POOL. Retention is a plain descending NAME
/// sort, so a backwards clock jump (WSL2's clock lagging the host after a long
/// sleep/resume) leaves newer-NAMED snapshots on disk and sorts the file just
/// written to the BOTTOM — pruning it on the spot while the caller reports
/// "snapshot saved: <name>" for a file that is already gone. Excluding it
/// makes the report true by construction instead of by luck, and it keeps the
/// freshest evidence, which is the one the operator came for. The window is
/// still honoured exactly: the others are trimmed to `keep - 1`, so the
/// directory holds at most `keep` snapshots afterwards. `keep == 0` never
/// reaches here — [`save_snapshot`] treats it as "snapshots are off".
pub fn prune_excluding(dir: &Path, keep: usize, fresh: &str) -> Vec<String> {
    let names: Vec<String> = snapshot_names_desc(dir).into_iter().filter(|n| n != fresh).collect();
    let pruned = prune_names(&names, keep.saturating_sub(1));
    for f in pruned {
        let _ = std::fs::remove_file(dir.join(f));
    }
    pruned.to_vec()
}

/// The LAST `max` bytes of `body`. Keeping the tail (not the head) is the
/// whole point: a crash is at the END of a log, so a head-truncating cap would
/// throw away exactly the evidence this module exists to preserve. The cut is
/// a raw byte offset, so a truncated snapshot can begin mid-line — acceptable
/// for a forensic tail, and it keeps the bash twin (`tail -c`) byte-identical.
pub fn tail_bytes(body: &[u8], max: usize) -> &[u8] {
    if body.len() <= max {
        body
    } else {
        &body[body.len() - max..]
    }
}

/// Write one snapshot into `dir` and prune. `logs` is `None` when docker could
/// not answer at all (engine down, the read hit its deadline) and
/// `Some(bytes)` otherwise; `None`, an empty body and `keep == 0` are all
/// [`SnapshotOutcome::Skipped`], which the caller reports as NOTHING (no line,
/// no warn, and — asserted by test — not even a stray empty file or a created
/// directory).
pub fn save_snapshot(
    dir: &Path,
    unix_secs: u64,
    title: &str,
    mode: &str,
    keep: usize,
    logs: Option<&[u8]>,
) -> SnapshotOutcome {
    // `keep == 0` = retention for zero files = the feature is OFF. Writing a
    // snapshot and deleting it again on the way out (the old behaviour) still
    // reported "snapshot saved: <name>" for a file that no longer existed.
    if keep == 0 {
        return SnapshotOutcome::Skipped;
    }
    let Some(body) = logs else { return SnapshotOutcome::Skipped };
    if body.is_empty() {
        return SnapshotOutcome::Skipped;
    }
    if std::fs::create_dir_all(dir).is_err() {
        return SnapshotOutcome::Failed;
    }
    let file = snapshot_file_name_at(unix_secs, title, mode);
    if std::fs::write(dir.join(&file), tail_bytes(body, SNAPSHOT_MAX_BYTES)).is_err() {
        return SnapshotOutcome::Failed;
    }
    let pruned = prune_excluding(dir, keep, &file);
    SnapshotOutcome::Saved { file, pruned }
}

/// The exact NDJSON lines a caller emits for an outcome, as
/// `(level, text)` pairs — the single place the stream vocabulary of this
/// feature is decided, so the bash twin (`_snapshot_world_log_report`) has one
/// thing to match.
///
/// PRUNES ARE SILENT, on both surfaces. The native path used to emit one
/// `pruned old log snapshot: <name>` info line per deleted file, which bash
/// never did — the same stop narrated differently depending on the backend,
/// against this round's own rule that a new NDJSON line lands on BOTH surfaces
/// or the two diverge. Silence is also the better of the two behaviours: the
/// prune is retention bookkeeping the user did not ask for and cannot act on,
/// and in the pathological case (a file both saved and immediately pruned) the
/// pair of lines contradicted each other.
pub fn snapshot_lines(outcome: &SnapshotOutcome) -> Vec<(&'static str, String)> {
    match outcome {
        SnapshotOutcome::Saved { file, .. } => {
            vec![("info", format!("worldserver log snapshot saved: {file}"))]
        }
        SnapshotOutcome::Failed => {
            vec![("warn", "could not snapshot the worldserver log -- continuing".to_string())]
        }
        SnapshotOutcome::Skipped => Vec::new(),
    }
}

/// `docker compose ps -a -q ac-worldserver`, run in the stopping title's own
/// compose dir. `-a` because a container that has already EXITED still holds
/// the log this module exists to preserve.
pub fn world_container_argv() -> Vec<&'static str> {
    vec!["compose", "ps", "-a", "-q", WORLD_SERVICE]
}

/// First non-empty line of `compose ps -q` output, trimmed — the container id.
/// Anything outside `[A-Za-z0-9_.-]` rejects the whole reading rather than
/// being passed on to `docker logs`: this value comes back from a subprocess
/// and goes straight into the next one's argv.
pub fn parse_container_id(stdout: &[u8]) -> Option<String> {
    String::from_utf8_lossy(stdout)
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .filter(|l| l.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')))
        .map(|l| l.to_string())
}

/// Which container (if any) THIS title's compose project owns for the world
/// service. `None` = nothing to snapshot: no such service (compose exits
/// non-zero), no container for it, or docker could not answer — all of which
/// are silent skips, never warnings.
pub fn resolve_world_container(program: &OsStr, compose_dir: &Path) -> Option<String> {
    let mut cmd = std::process::Command::new(program);
    cmd.current_dir(compose_dir).args(world_container_argv());
    windows_no_window(&mut cmd);
    match output_bounded_draining(cmd, RESOLVE_TIMEOUT) {
        Some(out) if out.status.success() => parse_container_id(&out.stdout),
        _ => None,
    }
}

/// One bounded `docker logs --tail N <container>` read (both streams combined
/// — AzerothCore logs plenty to stderr, exactly why `status::world_ready`
/// merges them too). `None` on any non-success exit or on the deadline. Uses
/// `output_bounded_draining` (never the non-draining helper): `docker logs`
/// output is precisely the case that deadlocks it.
/// Why a log read produced no body — the two cases need OPPOSITE reporting,
/// so they must not collapse into one `None` (round-2 re-review residual).
enum LogRead {
    Body(Vec<u8>),
    /// The daemon never answered (deadline hit, or the spawn itself failed).
    /// The container exists, so evidence WAS lost: warn.
    Unanswered,
    /// `docker logs` answered non-zero — the container vanished between the
    /// resolve and this read. Nothing to preserve: stay silent.
    Gone,
}

fn read_world_log(program: &OsStr, container: &str) -> LogRead {
    let mut cmd = std::process::Command::new(program);
    cmd.args(["logs", "--tail", &SNAPSHOT_TAIL_LINES.to_string(), container]);
    windows_no_window(&mut cmd);
    match output_bounded_draining(cmd, snapshot_timeout_from_env()) {
        Some(out) if out.status.success() => {
            let mut combined = out.stdout;
            combined.extend_from_slice(&out.stderr);
            LogRead::Body(combined)
        }
        Some(_) => LogRead::Gone,
        None => LogRead::Unanswered,
    }
}

/// Live wrapper into an explicit directory (the seam the tests drive, so they
/// never touch the real `~/.dml/logs`): resolve THIS title's world container,
/// read its log, save.
///
/// The resolution comes first and can end the whole thing: a project that owns
/// no world container has nothing to preserve, so nothing is read and nothing
/// is reported — completely silent, not a warning, on every non-WoW title in
/// the library.
pub fn snapshot_world_log_into(
    dir: &Path,
    program: &OsStr,
    compose_dir: &Path,
    title: &str,
    mode: &str,
    keep: usize,
    unix_secs: u64,
) -> SnapshotOutcome {
    if keep == 0 {
        return SnapshotOutcome::Skipped;
    }
    let Some(container) = resolve_world_container(program, compose_dir) else {
        return SnapshotOutcome::Skipped;
    };
    // A daemon that never answers is the wedged-dockerd incident this feature
    // exists for: the container is there, so evidence WAS lost and the
    // operator must be told rather than left believing a snapshot was taken.
    // A container that merely vanished in the meantime lost nothing, and
    // stays silent like every other "nothing to preserve" case.
    match read_world_log(program, &container) {
        LogRead::Body(body) => save_snapshot(dir, unix_secs, title, mode, keep, Some(&body)),
        LogRead::Unanswered => SnapshotOutcome::Failed,
        LogRead::Gone => SnapshotOutcome::Skipped,
    }
}

// There is deliberately NO `snapshot_world_log(program, compose_dir, …)`
// wrapper that resolves `~/.dml/logs` + `DML_LOG_SNAPSHOT_KEEP` internally:
// the live caller (`lifecycle::games_lifecycle_stream_with`) resolves both into
// its `LifecycleEnv` and passes them here, which is what lets the pre-stop
// ORDER be asserted against the real call site with temp directories instead of
// the operator's actual snapshot pool (round-2 finding G17).

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dml-logsnap-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn snapshot_file_name_carries_prefix_utc_timestamp_title_and_mode() {
        // 1700000000 == 2023-11-14T22:13:20Z, the same reference value
        // `backup::format_utc_compact` is pinned against.
        assert_eq!(
            snapshot_file_name_at(1_700_000_000, "wow-server-playerbots", "stop"),
            "world-20231114-221320-wow-server-playerbots-stop.log"
        );
        assert_eq!(snapshot_file_name_at(0, "wow", "restart"), "world-19700101-000000-wow-restart.log");
    }

    #[test]
    fn snapshot_file_name_sorts_chronologically_as_a_plain_string() {
        // The retention prune is a plain descending NAME sort (no stat calls,
        // byte-identical in bash and Rust), so the timestamp MUST outrank the
        // title in the name -- two different titles a second apart have to
        // still sort by time.
        let older = snapshot_file_name_at(1_700_000_000, "zzz-title", "stop");
        let newer = snapshot_file_name_at(1_700_000_001, "aaa-title", "stop");
        assert!(newer > older, "{newer} must sort above {older}");
    }

    #[test]
    fn sanitize_title_strips_path_separators_and_spaces() {
        assert_eq!(sanitize_title("wow-server-playerbots"), "wow-server-playerbots");
        assert_eq!(sanitize_title("../../etc/passwd"), ".._.._etc_passwd");
        assert_eq!(sanitize_title("my game\\x"), "my_game_x");
        for bad in ["/", "\\", " ", ":", "*"] {
            assert!(!sanitize_title("a/b\\c d:e*f").contains(bad), "{bad} survived sanitization");
        }
    }

    #[test]
    fn snapshot_names_desc_is_newest_first_and_prefix_scoped() {
        let dir = fixture("names-desc");
        std::fs::create_dir_all(&dir).unwrap();
        for n in ["world-20260101-000000-wow-stop.log", "world-20260103-000000-wow-stop.log", "world-20260102-000000-wow-restart.log"] {
            std::fs::write(dir.join(n), b"x").unwrap();
        }
        // Foreign entries must NOT enter the retention pool: they would eat a
        // keep slot (and never sort chronologically alongside the snapshots).
        std::fs::write(dir.join("zzz-not-a-snapshot.log"), b"x").unwrap();
        std::fs::write(dir.join("world-20260104-000000-wow-stop.txt"), b"x").unwrap();

        let names = snapshot_names_desc(&dir);
        assert_eq!(
            names,
            vec![
                "world-20260103-000000-wow-stop.log".to_string(),
                "world-20260102-000000-wow-restart.log".to_string(),
                "world-20260101-000000-wow-stop.log".to_string(),
            ]
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn snapshot_names_desc_degrades_to_empty_on_a_missing_dir() {
        assert!(snapshot_names_desc(&fixture("missing-dir")).is_empty());
    }

    #[test]
    fn prune_names_keeps_the_newest_keep_entries() {
        let all: Vec<String> = ["d", "c", "b", "a"].iter().map(|s| s.to_string()).collect();
        assert_eq!(prune_names(&all, 2), &all[2..]);
        assert!(prune_names(&all, 4).is_empty());
        assert!(prune_names(&all, 99).is_empty());
        assert_eq!(prune_names(&all, 0).len(), 4);
    }

    #[test]
    fn save_snapshot_writes_the_real_log_body() {
        let dir = fixture("saves-body");
        let body = b"[MySQL] Can't connect to MySQL server on 'ac-database' (110)\nworld: bye\n";
        let out = save_snapshot(&dir, 1_700_000_000, "wow-server-playerbots", "stop", 10, Some(body));

        let SnapshotOutcome::Saved { file, pruned } = out else { panic!("expected Saved, got {out:?}") };
        assert_eq!(file, "world-20231114-221320-wow-server-playerbots-stop.log");
        assert!(pruned.is_empty());
        // The load-bearing assertion: the file holds the ACTUAL log bytes. A
        // stub that only touches an empty file (or names a file it never
        // writes) cannot satisfy this.
        let on_disk = std::fs::read(dir.join(&file)).expect("snapshot file must exist");
        assert_eq!(on_disk, body, "the snapshot must contain the log verbatim");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn save_snapshot_creates_the_logs_dir_when_absent() {
        // ~/.dml/logs does not exist on any existing install -- the very first
        // stop has to create it.
        let dir = fixture("creates-dir").join("nested").join("logs");
        assert!(!dir.exists());
        let out = save_snapshot(&dir, 1_700_000_000, "wow", "stop", 10, Some(b"boot log\n"));
        assert!(matches!(out, SnapshotOutcome::Saved { .. }), "got {out:?}");
        assert!(dir.is_dir(), "the logs dir must be created on demand");
        let _ = std::fs::remove_dir_all(fixture("creates-dir"));
    }

    #[test]
    fn save_snapshot_skips_when_docker_had_nothing_to_give() {
        // `None` = docker could not answer / there is no ac-worldserver at all
        // (a non-WoW title being stopped). Not a failure: nothing to preserve,
        // so no file, no warn, no empty-file litter in ~/.dml/logs.
        let dir = fixture("skips-none");
        assert_eq!(save_snapshot(&dir, 1_700_000_000, "wow", "stop", 10, None), SnapshotOutcome::Skipped);
        assert!(!dir.exists(), "a skip must not even create the logs dir");
        assert_eq!(save_snapshot(&dir, 1_700_000_000, "wow", "stop", 10, Some(b"")), SnapshotOutcome::Skipped);
        assert!(!dir.exists());
    }

    #[test]
    fn save_snapshot_reports_failed_when_the_dir_cannot_be_created() {
        // A FILE where the logs dir should be: create_dir_all fails, and the
        // caller must warn rather than pretend it saved something.
        let base = fixture("failed-dir");
        std::fs::create_dir_all(&base).unwrap();
        let dir = base.join("logs");
        std::fs::write(&dir, b"i am a file, not a directory").unwrap();
        assert_eq!(save_snapshot(&dir, 1_700_000_000, "wow", "stop", 10, Some(b"x\n")), SnapshotOutcome::Failed);
        std::fs::remove_dir_all(&base).unwrap();
    }

    /// A stand-in `docker` that logs `<cwd> <argv>` for every call, answers
    /// `compose ps -a -q` with `ps_id` (or nothing) and `logs` with `body`.
    /// Per-platform, per CLAUDE.md's test-portability rule: a `.cmd` on
    /// Windows, a `sh` script elsewhere — never a hardcoded interpreter.
    #[cfg(windows)]
    fn write_fake_docker(dir: &Path, log: &Path, ps_id: Option<&str>, ps_rc: i32, body: &str) -> PathBuf {
        let p = dir.join("fake-docker.cmd");
        let id_line = match ps_id {
            Some(id) => format!("echo {id}"),
            None => "rem no container in this project".to_string(),
        };
        let script = format!(
            "@echo off\r\n>>\"{log}\" echo %CD% %*\r\nif \"%~1\"==\"compose\" (\r\n{id_line}\r\nexit /b {ps_rc}\r\n)\r\nif \"%~1\"==\"logs\" (\r\necho {body}\r\nexit /b 0\r\n)\r\nexit /b 0\r\n",
            log = log.display()
        );
        std::fs::write(&p, script).unwrap();
        p
    }
    #[cfg(not(windows))]
    fn write_fake_docker(dir: &Path, log: &Path, ps_id: Option<&str>, ps_rc: i32, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let p = dir.join("fake-docker.sh");
        let id_line = match ps_id {
            Some(id) => format!("echo {id}"),
            None => ":".to_string(),
        };
        let script = format!(
            "#!/bin/sh\necho \"$(pwd) $*\" >> '{log}'\ncase \"$1\" in\n  compose) {id_line}; exit {ps_rc};;\n  logs) echo '{body}'; exit 0;;\nesac\nexit 0\n",
            log = log.display()
        );
        std::fs::write(&p, script).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    #[test]
    fn snapshot_reads_the_container_the_titles_compose_project_resolves() {
        // The whole point of the resolution: the id `docker logs` is handed
        // comes from THIS title's compose project, not from a global container
        // name that answers for whichever title happens to own it.
        let base = fixture("scoped-read");
        let compose_dir = base.join("wow-server-playerbots");
        let logs = base.join("logs");
        std::fs::create_dir_all(&compose_dir).unwrap();
        let calls = base.join("calls.log");
        let docker = write_fake_docker(&base, &calls, Some("c0ffee1234ab"), 0, "boot log line");

        let out = snapshot_world_log_into(
            &logs,
            docker.as_os_str(),
            &compose_dir,
            "wow-server-playerbots",
            "stop",
            10,
            1_700_000_000,
        );
        let SnapshotOutcome::Saved { file, .. } = out else { panic!("expected Saved, got {out:?}") };
        assert!(logs.join(&file).exists());

        let log = std::fs::read_to_string(&calls).unwrap();
        // Resolution ran, in the title's own compose dir (that is what scopes
        // it to the project) ...
        assert!(log.contains("compose ps -a -q ac-worldserver"), "calls: {log}");
        assert!(log.contains("wow-server-playerbots"), "resolution must run in the compose dir: {log}");
        // ... and the READ used the resolved id. A hardcoded container name
        // cannot satisfy this.
        assert!(log.contains("logs --tail 2000 c0ffee1234ab"), "calls: {log}");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A `docker` whose `compose ps` resolves an id but whose `logs` then
    /// fails — the container vanished between the two calls.
    #[cfg(windows)]
    fn write_vanishing_docker(dir: &Path) -> PathBuf {
        let p = dir.join("vanishing-docker.cmd");
        let script = "@echo off\r\nif \"%~1\"==\"compose\" (\r\necho c0ffee1234ab\r\nexit /b 0\r\n)\r\nif \"%~1\"==\"logs\" (\r\necho Error: No such container>&2\r\nexit /b 1\r\n)\r\nexit /b 0\r\n";
        std::fs::write(&p, script).unwrap();
        p
    }
    #[cfg(not(windows))]
    fn write_vanishing_docker(dir: &Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let p = dir.join("vanishing-docker.sh");
        let script = "#!/bin/sh\ncase \"$1\" in\n  compose) echo c0ffee1234ab; exit 0;;\n  logs) echo 'Error: No such container' >&2; exit 1;;\nesac\nexit 0\n";
        std::fs::write(&p, script).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    #[test]
    fn a_container_that_vanished_before_the_read_is_silent_not_a_warning() {
        // The re-review residual made a deadline WARN, because a daemon that
        // will not answer means evidence was lost. This is the neighbouring
        // case that must NOT warn: `docker logs` answered, non-zero, because
        // the container disappeared between the resolve and the read. Nothing
        // was lost, so a warning would be noise. (The first cut of that fix
        // collapsed both into a warn and broke the silent-skip contract.)
        let base = fixture("vanished-container");
        let compose_dir = base.join("wow-server-playerbots");
        let logs = base.join("logs");
        std::fs::create_dir_all(&compose_dir).unwrap();
        let docker = write_vanishing_docker(&base);

        let out = snapshot_world_log_into(
            &logs,
            docker.as_os_str(),
            &compose_dir,
            "wow-server-playerbots",
            "stop",
            10,
            1_700_000_000,
        );
        assert_eq!(out, SnapshotOutcome::Skipped, "a vanished container lost nothing, so it must stay silent");
        assert!(snapshot_lines(&out).is_empty(), "Skipped must emit no line at all");
        assert!(!logs.exists(), "a skip must not even create the snapshot directory");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn snapshot_skips_a_title_whose_project_owns_no_world_container() {
        // Stopping a non-WoW title: compose resolves no container. Nothing may
        // be read (the WoW stack may well be up and would answer), nothing
        // written, nothing said.
        let base = fixture("scoped-skip");
        let compose_dir = base.join("maplestory-server");
        let logs = base.join("logs");
        std::fs::create_dir_all(&compose_dir).unwrap();
        let calls = base.join("calls.log");
        let docker = write_fake_docker(&base, &calls, None, 0, "the WoW world log");

        let out = snapshot_world_log_into(
            &logs,
            docker.as_os_str(),
            &compose_dir,
            "maplestory-server",
            "stop",
            10,
            1_700_000_000,
        );
        assert_eq!(out, SnapshotOutcome::Skipped);
        assert!(!logs.exists(), "a skip must not even create the logs dir");
        let log = std::fs::read_to_string(&calls).unwrap();
        assert!(!log.contains("logs --tail"), "no log may be read at all: {log}");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn snapshot_skips_when_compose_cannot_resolve_the_service() {
        // `no such service: ac-worldserver` -- compose exits non-zero. Same
        // silent skip, and still no read.
        let base = fixture("scoped-nosvc");
        let compose_dir = base.join("runescape-server");
        let logs = base.join("logs");
        std::fs::create_dir_all(&compose_dir).unwrap();
        let calls = base.join("calls.log");
        let docker = write_fake_docker(&base, &calls, Some("ignored"), 1, "the WoW world log");

        let out = snapshot_world_log_into(
            &logs,
            docker.as_os_str(),
            &compose_dir,
            "runescape-server",
            "stop",
            10,
            1_700_000_000,
        );
        assert_eq!(out, SnapshotOutcome::Skipped);
        let log = std::fs::read_to_string(&calls).unwrap();
        assert!(!log.contains("logs --tail"), "no log may be read at all: {log}");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn parse_container_id_takes_the_first_line_and_rejects_junk() {
        assert_eq!(parse_container_id(b"c0ffee1234ab\n"), Some("c0ffee1234ab".to_string()));
        assert_eq!(parse_container_id(b"\n\n  abc123  \nsecond\n"), Some("abc123".to_string()));
        assert_eq!(parse_container_id(b""), None);
        assert_eq!(parse_container_id(b"   \n"), None);
        // The value goes straight into the next process's argv, so anything
        // that is not an id is no id at all.
        assert_eq!(parse_container_id(b"; rm -rf /\n"), None);
        assert_eq!(parse_container_id(b"no such service: ac-worldserver\n"), None);
    }

    #[test]
    fn snapshot_lines_report_the_saved_file_and_stay_silent_about_prunes() {
        // The bash twin prunes silently; a `pruned old log snapshot: X` line
        // on the native side only would narrate the same stop differently per
        // backend -- and in the pathological case contradict the "saved" line
        // above it.
        let saved = SnapshotOutcome::Saved {
            file: "world-20231114-221320-wow-stop.log".to_string(),
            pruned: vec!["world-20200101-000000-wow-stop.log".to_string(), "world-20200102-000000-wow-stop.log".to_string()],
        };
        assert_eq!(
            snapshot_lines(&saved),
            vec![("info", "worldserver log snapshot saved: world-20231114-221320-wow-stop.log".to_string())]
        );
        assert_eq!(
            snapshot_lines(&SnapshotOutcome::Failed),
            vec![("warn", "could not snapshot the worldserver log -- continuing".to_string())]
        );
        assert!(snapshot_lines(&SnapshotOutcome::Skipped).is_empty());
    }

    #[test]
    fn save_snapshot_never_reports_a_file_its_own_prune_deleted() {
        // A backwards clock jump leaves NEWER-named snapshots on disk, so the
        // fresh file sorts last: a prune over the whole pool deletes exactly
        // the file the caller is about to report as saved.
        let dir = fixture("prune-eats-fresh");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("world-29991231-235959-wow-stop.log"), b"from the future").unwrap();

        let out = save_snapshot(&dir, 1_700_000_000, "wow", "stop", 1, Some(b"the real log\n"));
        let SnapshotOutcome::Saved { file, pruned } = out else { panic!("expected Saved, got {out:?}") };
        assert!(dir.join(&file).exists(), "the reported snapshot must be ON DISK after the prune");
        assert_eq!(std::fs::read(dir.join(&file)).unwrap(), b"the real log\n");
        assert_eq!(pruned, vec!["world-29991231-235959-wow-stop.log".to_string()]);
        assert_eq!(snapshot_names_desc(&dir).len(), 1, "the window still holds at keep");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn save_snapshot_with_keep_zero_is_off_rather_than_write_then_delete() {
        let dir = fixture("keep-zero");
        assert_eq!(save_snapshot(&dir, 1_700_000_000, "wow", "stop", 0, Some(b"x\n")), SnapshotOutcome::Skipped);
        assert!(!dir.exists(), "retention 0 must not even create the logs dir");
    }

    #[test]
    fn snapshot_timeout_from_env_defaults_and_reads_the_override() {
        std::env::remove_var("DML_LOG_SNAPSHOT_TIMEOUT");
        assert_eq!(snapshot_timeout_from_env(), Duration::from_secs(SNAPSHOT_TIMEOUT_SECS));
        std::env::set_var("DML_LOG_SNAPSHOT_TIMEOUT", "3");
        assert_eq!(snapshot_timeout_from_env(), Duration::from_secs(3));
        std::env::set_var("DML_LOG_SNAPSHOT_TIMEOUT", "not-a-number");
        assert_eq!(snapshot_timeout_from_env(), Duration::from_secs(SNAPSHOT_TIMEOUT_SECS));
        // 0 = the default, not a zero deadline: coreutils' `timeout 0` (the
        // bash twin) means NO limit, so the two surfaces must not read the
        // same value in opposite directions.
        std::env::set_var("DML_LOG_SNAPSHOT_TIMEOUT", "0");
        assert_eq!(snapshot_timeout_from_env(), Duration::from_secs(SNAPSHOT_TIMEOUT_SECS));
        std::env::remove_var("DML_LOG_SNAPSHOT_TIMEOUT");
    }

    #[test]
    fn save_snapshot_prunes_past_the_retention_window() {
        let dir = fixture("prunes");
        std::fs::create_dir_all(&dir).unwrap();
        for n in ["world-20200101-000000-wow-stop.log", "world-20200102-000000-wow-stop.log"] {
            std::fs::write(dir.join(n), b"old").unwrap();
        }
        let out = save_snapshot(&dir, 1_700_000_000, "wow", "stop", 2, Some(b"new\n"));
        let SnapshotOutcome::Saved { file, pruned } = out else { panic!("expected Saved, got {out:?}") };
        assert_eq!(pruned, vec!["world-20200101-000000-wow-stop.log".to_string()]);
        assert!(!dir.join("world-20200101-000000-wow-stop.log").exists(), "the oldest must be gone from DISK");
        assert!(dir.join("world-20200102-000000-wow-stop.log").exists());
        assert!(dir.join(&file).exists());
        assert_eq!(snapshot_names_desc(&dir).len(), 2, "retention window must hold at exactly keep");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn tail_bytes_keeps_the_end_not_the_start() {
        // The crash is at the END of a log. A head-truncating cap would throw
        // away exactly the evidence this feature exists to preserve.
        assert_eq!(tail_bytes(b"0123456789", 4), b"6789");
        assert_eq!(tail_bytes(b"0123456789", 10), b"0123456789");
        assert_eq!(tail_bytes(b"0123456789", 99), b"0123456789");
        assert_eq!(tail_bytes(b"", 4), b"");
    }

    #[test]
    fn save_snapshot_enforces_the_byte_cap() {
        let dir = fixture("byte-cap");
        let body = vec![b'x'; SNAPSHOT_MAX_BYTES + 4096];
        let out = save_snapshot(&dir, 1_700_000_000, "wow", "stop", 10, Some(&body));
        let SnapshotOutcome::Saved { file, .. } = out else { panic!("expected Saved, got {out:?}") };
        let written = std::fs::metadata(dir.join(&file)).unwrap().len() as usize;
        assert_eq!(written, SNAPSHOT_MAX_BYTES, "a pathological log must not land unbounded on disk");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn snapshot_keep_from_env_defaults_to_ten_and_reads_the_override() {
        std::env::remove_var("DML_LOG_SNAPSHOT_KEEP");
        assert_eq!(snapshot_keep_from_env(), 10);
        std::env::set_var("DML_LOG_SNAPSHOT_KEEP", "3");
        assert_eq!(snapshot_keep_from_env(), 3);
        std::env::set_var("DML_LOG_SNAPSHOT_KEEP", "not-a-number");
        assert_eq!(snapshot_keep_from_env(), 10);
        std::env::remove_var("DML_LOG_SNAPSHOT_KEEP");
    }

    #[test]
    fn logs_dir_is_the_dml_home_logs_dir() {
        let home = dml_core::util::dml_home_dir().expect("test env always has USERPROFILE/HOME");
        assert_eq!(logs_dir(), Some(home.join("logs")));
    }
}
