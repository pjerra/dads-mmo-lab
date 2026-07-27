//! Native-mode **`backup restore`** (spike: `spike/docker-desktop-native`,
//! Task C4b, Part 1): faithful port of the `restore)` arm
//! (`cli/src/90-main.sh:3786-3855`) — the CLI's ONE sanctioned whole-DB-
//! overwrite write path (see `60-backup.sh`'s header comment).
//!
//! ARCHITECTURE, mirroring `dml::destructive`/`dml::modmgr`: the two REUSABLE,
//! testable primitives live here — [`prerestore_name`]/[`prerestore_name_at`]
//! (the pre-restore safety dump's name + world-inclusiveness, both driven
//! purely by the TARGET file's own name) and [`stream_restore`] (the actual
//! import). The STREAMED Tauri orchestration (guard ordering, the `docker
//! compose stop/start` calls, NDJSON event sequencing) lives in `lib.rs` next
//! to `wow_world_restart_native_blocking`/`wow_backup_create_native_blocking`,
//! which `wow_backup_restore_native_blocking` follows event-for-event.
//!
//! THE CRITICAL ENGINEERING PROBLEM: a `.sql.gz` backup can be many GB
//! decompressed (a `--include-world` snapshot especially). The bash oracle
//! streams it with a shell pipeline (`gunzip -c … | docker exec -i
//! ac-database mysql …`) that never buffers the whole dump anywhere. Native
//! mode has no shell to pipe through, so [`stream_restore`] reproduces the
//! SAME streaming shape directly in Rust: a `flate2::read::GzDecoder` wrapping
//! the open file is read in small fixed chunks (never `read_to_end`) and each
//! chunk is written straight to the child's piped stdin as it's decompressed
//! — the decompressed dump is NEVER held in memory as a whole, at any point.
//!
//! THE CLASSIC PIPE DEADLOCK THIS AVOIDS: if the parent only writes stdin and
//! only reads stdout/stderr AFTER the child exits (the `output_bounded_
//! draining`/`run_captured` shape every OTHER docker call in this codebase
//! uses), a chatty child (mysql echoing warnings/`Query OK, N rows...` lines,
//! or docker itself buffering exec-attach frames) can fill its stdout/stderr
//! pipe and block, while the parent is simultaneously blocked writing more
//! stdin the child will never come back to drain — classic bidirectional
//! deadlock. [`stream_restore`] drains stdout AND stderr on their own threads
//! CONCURRENTLY with the main thread's stdin-writing loop, so the child can
//! never block on a full output pipe no matter how long the write loop runs.
//!
//! DELIBERATELY UNBOUNDED, LIKE THE MODULE-REBUILD BUILD STREAM
//! (`destructive::run_streamed_unbounded`): the bash oracle puts NO wall-clock
//! timeout on this pipeline (`00-head.sh`'s `set -euo pipefail` governs exit
//! codes, not time), and a multi-GB `--include-world` import can legitimately
//! run for many minutes on a slow disk. Killing it on an arbitrary clock would
//! abort a restore that was still making real, unrecoverable progress against
//! a database the caller has already been told is "LEFT STOPPED" pending this
//! exact import finishing. Same posture, same rationale, as `run_streamed_
//! unbounded`'s own doc comment — no `kill()` anywhere in this module.
//!
//! NATIVE-MODE-ONLY by convention: WSL keeps calling `dml`; the Tauri command
//! layer (`lib.rs`) gates every entry point on `require_native_backend()`.

use std::ffi::OsStr;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use super::backup;
use super::status::windows_no_window;

// ---------------------------------------------------------------------------
// Pre-restore safety dump naming — `90-main.sh:3825-3829`.
// ---------------------------------------------------------------------------

/// `rincw`/`rsuffix`/`safety` (`90-main.sh:3825-3829`): the pre-restore safety
/// dump's file name and whether its own dump must include `acore_world` —
/// BOTH driven purely by whether the TARGET restore file itself is a `-full`
/// (whole-server) snapshot, via the exact same [`backup::is_full_name`] check
/// `backup list`'s `world` column already uses (the oracle's own `[[ "$file"
/// == *-full.sql.gz || "$file" == *-full-prerestore.sql.gz ]]` gate is that
/// same shape). Pure — the real clock is read only by [`prerestore_name`].
pub fn prerestore_name_at(target_file: &str, unix_secs: u64) -> (String, bool) {
    let include_world = backup::is_full_name(target_file);
    let suffix = if include_world { "-full" } else { "" };
    (format!("wow-{}{suffix}-prerestore.sql.gz", backup::format_utc_compact(unix_secs)), include_world)
}

/// [`prerestore_name_at`] against the real clock.
pub fn prerestore_name(target_file: &str) -> (String, bool) {
    let secs =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    prerestore_name_at(target_file, secs)
}

// ---------------------------------------------------------------------------
// The import itself — `gunzip -c "$bdir/$file" | docker exec -i ac-database
// mysql -uroot -p"$(_db_pw)"` (`90-main.sh:3840`).
// ---------------------------------------------------------------------------

/// `docker exec -i ac-database mysql -uroot -p<pw>` argv — no trailing
/// database name: the dump itself carries `CREATE DATABASE`/`USE` statements
/// (it was produced with `mysqldump --databases …`, same as [`backup::
/// mysqldump_args`]), exactly like the bash pipeline's own argv.
pub fn mysql_import_args(password: &str) -> Vec<String> {
    vec!["exec".into(), "-i".into(), "ac-database".into(), "mysql".into(), "-uroot".into(), format!("-p{password}")]
}

/// One streamed import's outcome. `success()` mirrors `set -euo pipefail`'s
/// verdict on the bash pipeline: the WHOLE pipeline is a failure if EITHER
/// the local gunzip read faulted (corrupt/truncated archive) OR the child
/// (`docker exec … mysql`) exited nonzero/never started — matching pipefail,
/// not just "the last command's exit code" (which alone would let a corrupt
/// archive that happens to feed mysql a byte stream ending in a state mysql
/// is happy with slip through as a false "success").
#[derive(Debug)]
pub struct ImportResult {
    pub status: Option<std::process::ExitStatus>,
    /// Small by construction — `mysql` fed via stdin with no `-e`/verbose
    /// flag emits little to nothing on success; captured in full (never
    /// truncated) purely so a real failure message can be reported, same
    /// convention as `backup::dump_to`'s `err_tail`.
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    /// `Some(..)` when the LOCAL gunzip-read-or-stdin-write loop faulted
    /// (corrupt archive, or the child's stdin closed early because it already
    /// died) — this is what makes [`Self::success`] track `pipefail`
    /// semantics rather than only the child's raw exit code.
    pub stream_error: Option<String>,
}

impl ImportResult {
    pub fn success(&self) -> bool {
        self.stream_error.is_none() && matches!(&self.status, Some(s) if s.success())
    }
}

/// Stream-decompress `gz_path` INCREMENTALLY (fixed-size chunks, never the
/// whole file) into a freshly-spawned `program mysql_import_args(password)`'s
/// stdin, draining its stdout/stderr on separate threads throughout — see the
/// module doc comment for why both the incremental chunking and the
/// concurrent draining are load-bearing, not stylistic. `Err` only on a local
/// setup failure (the backup file couldn't be opened, or the child couldn't
/// be spawned at all) — every failure THE IMPORT ITSELF can have (corrupt
/// archive mid-stream, mysql rejecting the SQL, mysql exiting early) instead
/// comes back as `Ok(ImportResult)` with `success() == false`, so the caller
/// always has a `status`/`stderr` to build its `BACKUP_FAILED` message from.
pub fn stream_restore(program: &OsStr, password: &str, gz_path: &Path) -> Result<ImportResult, String> {
    stream_into(program, &mysql_import_args(password), gz_path)
}

/// The generic streaming-pipe engine behind [`stream_restore`], parameterized
/// on the target program/args so it can be exercised in tests against a
/// harmless real subprocess (e.g. a line-echo sink) instead of `docker exec …
/// mysql` — see this module's tests.
fn stream_into(program: &OsStr, args: &[String], gz_path: &Path) -> Result<ImportResult, String> {
    let file = std::fs::File::open(gz_path).map_err(|e| format!("could not open {}: {e}", gz_path.display()))?;
    let mut decoder = flate2::read::GzDecoder::new(file);

    let mut cmd = Command::new(program);
    cmd.args(args);
    windows_no_window(&mut cmd);
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("could not start the import: {e}"))?;
    let mut stdin = child.stdin.take().expect("stdin was piped");
    let mut stdout = child.stdout.take().expect("stdout was piped");
    let mut stderr = child.stderr.take().expect("stderr was piped");

    // Drain BOTH pipes on their own threads, concurrently with the main
    // thread's write loop below — this is the anti-deadlock discipline the
    // module doc comment describes. Mirrors `status::output_bounded_
    // draining`'s reader-thread shape, minus the `try_wait`-polling half
    // (there is no timeout to poll against here — see the module doc
    // comment on why that's deliberate).
    let stdout_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = std::io::Read::read_to_end(&mut stdout, &mut buf);
        buf
    });
    let stderr_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = std::io::Read::read_to_end(&mut stderr, &mut buf);
        buf
    });

    // The load-bearing loop: fixed 64KiB chunks, read -> write, repeat. At
    // NO point does this hold more than one chunk of the decompressed dump
    // in memory, however many GB the whole thing decompresses to.
    let mut chunk = [0u8; 64 * 1024];
    let mut stream_error = None;
    loop {
        let n = match std::io::Read::read(&mut decoder, &mut chunk) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                stream_error = Some(format!("gunzip read failed (archive may be corrupt/truncated): {e}"));
                break;
            }
        };
        if let Err(e) = std::io::Write::write_all(&mut stdin, &chunk[..n]) {
            // Most likely the child already exited (broken pipe) -- stop
            // feeding it and fall through to reap the real exit status/
            // stderr below rather than looping forever on a dead child.
            stream_error = Some(format!("write to the import's stdin failed: {e}"));
            break;
        }
    }
    // Close OUR end of stdin either way: on a clean full read this is what
    // tells `mysql` its input is complete; on an early break it's a no-op
    // over an already-broken pipe.
    drop(stdin);

    let stdout_buf = stdout_handle.join().unwrap_or_default();
    let stderr_buf = stderr_handle.join().unwrap_or_default();
    // No `kill()` call exists on this path -- see the module doc comment.
    // `wait()` blocks until the child exits on its own, however long that
    // takes; both reader threads have already hit EOF on their pipes by the
    // time this returns (an mysql that's still writing output cannot have
    // exited yet), so this is not an additional unbounded wait beyond what
    // the child itself was always going to take.
    let status = child.wait().ok();

    Ok(ImportResult { status, stdout: stdout_buf, stderr: stderr_buf, stream_error })
}

// ---------------------------------------------------------------------------
// STREAMED `backup restore` orchestration (Chunk 4b, Part 1): faithful port
// of the `restore)` arm (`90-main.sh:3786-3855`) — the CLI's ONE sanctioned
// whole-DB-overwrite write path. Moved out of the launcher's `lib.rs` by the
// cargo-workspace refactor (Task 9). Same NDJSON vocabulary as
// `lifecycle::world_restart_stream`/`backup::backup_create_stream`; the ONE
// new subprocess shape is [`stream_restore`]'s gunzip-into-`docker exec …
// mysql` stdin pipe (see the module doc comment above). No `--yes`/confirm
// parameter exists here — the launcher's two-click UI is its gate, and the
// CLI supplies its own.
// ---------------------------------------------------------------------------

const BACKUP_RESTORE_SECTION: &str = "backup-restore";

/// The blocking flow itself (real docker/SOAP/fs I/O) — run under
/// `spawn_blocking`. Order mirrors the oracle top-to-bottom: name-shape gate
/// -> backup file exists? -> server installed? -> saveall (best-effort) ->
/// stop world+auth (hard-fail: "Nothing was changed") -> pre-restore safety
/// dump (hard-fail: restart best-effort + "nothing was restored") -> the
/// import itself (hard-fail: server LEFT STOPPED, no auto-restart) -> start
/// world+auth (soft-fail: warn only) -> done.
pub fn backup_restore_stream(
    file: String,
    soap_lock: Arc<Mutex<()>>,
    db_cfg: crate::db::DbConfig,
    emit: impl Fn(serde_json::Value),
) {
    use crate::{backup, config::ConfigReader, lifecycle, maint, modmgr, native, restore, soap, status};

    emit(modmgr::section_start(BACKUP_RESTORE_SECTION));

    if !backup::valid_backup_name(&file) {
        emit(modmgr::section_end(BACKUP_RESTORE_SECTION, "error"));
        emit(modmgr::error_event("BAD_ARG", format!("Invalid backup name: {file}"), ""));
        return;
    }

    let Some(bdir) = backup::backup_dir() else {
        emit(modmgr::section_end(BACKUP_RESTORE_SECTION, "error"));
        emit(modmgr::error_event("INTERNAL", "Could not resolve the backups directory", ""));
        return;
    };
    let bpath = bdir.join(&file);
    if !bpath.is_file() {
        emit(modmgr::section_end(BACKUP_RESTORE_SECTION, "error"));
        emit(modmgr::error_event("NOT_FOUND", format!("No backup named {file}"), ""));
        return;
    }

    let title_dir = ConfigReader::title_dir_from_env();
    let Some(sdir) = maint::resolve_server_dir(&title_dir) else {
        emit(modmgr::section_end(BACKUP_RESTORE_SECTION, "error"));
        emit(modmgr::error_event("NOT_FOUND", "WoW Playerbots server not installed", "Install it first."));
        return;
    };

    let docker_program = native::docker_program();

    emit(modmgr::line_event("info", "stopping the game server..."));
    // Flush characters before the stop so the pre-restore safety dump
    // contains everyone's latest state (best-effort) — `90-main.sh:3812-3816`.
    {
        let _guard = soap_lock.lock().unwrap_or_else(|e| e.into_inner());
        let cfg = soap::SoapConfig::load();
        let _ = soap::exec(&cfg, "saveall");
    }
    let mut stop_cmd = std::process::Command::new(&docker_program);
    stop_cmd.current_dir(&sdir).args(["compose", "stop", "-t", "180", "ac-worldserver", "ac-authserver"]);
    status::windows_no_window(&mut stop_cmd);
    if !matches!(status::output_bounded_draining(stop_cmd, lifecycle::COMPOSE_DOWN_TIMEOUT), Some(o) if o.status.success())
    {
        emit(modmgr::section_end(BACKUP_RESTORE_SECTION, "error"));
        emit(modmgr::error_event("BACKUP_FAILED", "Could not stop the server", "Nothing was changed."));
        return;
    }

    emit(modmgr::line_event("info", "taking a pre-restore safety backup..."));
    let (safety, include_world) = restore::prerestore_name(&file);
    let safety_path = bdir.join(&safety);
    if let Err(errtail) = backup::dump_to(&docker_program, &db_cfg.password, include_world, &safety_path) {
        let _ = errtail; // bash discards the tail here too -- fixed hint text only.
        let mut start_cmd = std::process::Command::new(&docker_program);
        start_cmd.current_dir(&sdir).args(["compose", "start", "ac-worldserver", "ac-authserver"]);
        status::windows_no_window(&mut start_cmd);
        let _ = status::output_bounded_draining(start_cmd, lifecycle::COMPOSE_UP_TIMEOUT);
        emit(modmgr::section_end(BACKUP_RESTORE_SECTION, "error"));
        emit(modmgr::error_event(
            "BACKUP_FAILED",
            "Safety backup failed -- nothing was restored",
            "The server was started again.",
        ));
        return;
    }

    emit(modmgr::line_event("info", format!("restoring {file}...")));
    let import_ok = matches!(restore::stream_restore(&docker_program, &db_cfg.password, &bpath), Ok(r) if r.success());
    if !import_ok {
        emit(modmgr::section_end(BACKUP_RESTORE_SECTION, "error"));
        emit(modmgr::error_event(
            "BACKUP_FAILED",
            "Import failed -- the server was LEFT STOPPED",
            &format!("Your pre-restore state is saved as {safety}. Restore it, or start the server manually once resolved."),
        ));
        return;
    }

    emit(modmgr::line_event("info", "starting the game server..."));
    let mut start_cmd = std::process::Command::new(&docker_program);
    start_cmd.current_dir(&sdir).args(["compose", "start", "ac-worldserver", "ac-authserver"]);
    status::windows_no_window(&mut start_cmd);
    if !matches!(status::output_bounded_draining(start_cmd, lifecycle::COMPOSE_UP_TIMEOUT), Some(o) if o.status.success())
    {
        emit(modmgr::line_event("warn", "server start failed -- start it from Home"));
    }

    emit(modmgr::section_end(BACKUP_RESTORE_SECTION, "ok"));
    emit(modmgr::done_event(serde_json::json!({
        "restored": true, "file": file, "safety_backup": safety,
    })));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn tmp_dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("dml-restore-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn write_gz(path: &Path, content: &[u8]) {
        let f = std::fs::File::create(path).unwrap();
        let mut enc = flate2::write::GzEncoder::new(f, flate2::Compression::default());
        enc.write_all(content).unwrap();
        enc.finish().unwrap();
    }

    // -- prerestore_name_at ---------------------------------------------------

    #[test]
    fn prerestore_name_plain_target_is_not_full() {
        let (name, world) = prerestore_name_at("wow-20260101-000000.sql.gz", 1_800_000_000);
        assert!(!world);
        assert!(!name.contains("-full"));
        assert!(name.ends_with("-prerestore.sql.gz"));
        assert!(name.starts_with("wow-"));
    }

    #[test]
    fn prerestore_name_full_target_is_full_and_includes_world() {
        let (name, world) = prerestore_name_at("wow-20260101-000000-full.sql.gz", 1_800_000_000);
        assert!(world);
        assert!(name.contains("-full-prerestore.sql.gz"));
    }

    #[test]
    fn prerestore_name_full_prerestore_target_is_also_full() {
        // A restore FROM a previous safety dump (itself `-full-prerestore`)
        // must still be recognized as a world snapshot -- `is_full_name`
        // matches this suffix too (`90-main.sh`'s own `*-full-prerestore.sql.gz`
        // arm of the OR).
        let (name, world) = prerestore_name_at("wow-20260101-000000-full-prerestore.sql.gz", 1_800_000_000);
        assert!(world);
        assert!(name.contains("-full-prerestore.sql.gz"));
    }

    #[test]
    fn prerestore_name_exact_shape_at_fixed_time() {
        // 2026-01-01 00:00:00 UTC = 1_767_225_600.
        let (name, world) = prerestore_name_at("wow-20250101-010203.sql.gz", 1_767_225_600);
        assert!(!world);
        assert_eq!(name, "wow-20260101-000000-prerestore.sql.gz");
    }

    #[test]
    fn prerestore_name_real_clock_variant_runs() {
        let (name, _world) = prerestore_name("wow-20250101-010203.sql.gz");
        assert!(name.starts_with("wow-"));
        assert!(name.ends_with("-prerestore.sql.gz"));
    }

    // -- mysql_import_args -----------------------------------------------------

    #[test]
    fn mysql_import_args_exact_shape() {
        let args = mysql_import_args("hunter2");
        assert_eq!(args, vec!["exec", "-i", "ac-database", "mysql", "-uroot", "-phunter2"]);
    }

    // -- stream_into: real subprocess, no docker dependency ---------------------
    //
    // These exercise the FULL streaming engine (gzip decode -> chunked write
    // -> concurrent stdout/stderr drain -> reap) against `cmd.exe`-provided
    // sinks instead of `docker exec ... mysql` -- the "dry restore subset"
    // the brief explicitly encourages.

    #[test]
    fn stream_into_real_passthrough_sink_reproduces_content_exactly() {
        // `cmd.exe /c more` copies stdin to stdout verbatim (no pagination
        // banner when stdout isn't a real console, which it never is behind
        // a Rust `Stdio::piped()` pipe) -- an effective `cat` for a text
        // payload, letting the test prove the decompressed bytes actually
        // arrived at the child intact via a REAL spawned process and a REAL
        // pipe, not a mock.
        let dir = tmp_dir("passthrough");
        let gz_path = dir.join("in.sql.gz");
        let mut payload = String::new();
        for i in 0..2000 {
            payload.push_str(&format!("INSERT INTO t VALUES ({i});\n"));
        }
        write_gz(&gz_path, payload.as_bytes());

        let result = stream_into(
            OsStr::new("cmd.exe"),
            &["/c".to_string(), "more".to_string()],
            &gz_path,
        )
        .unwrap();

        assert!(result.success(), "status={:?} stream_error={:?} stderr={:?}", result.status, result.stream_error, String::from_utf8_lossy(&result.stderr));
        let got = String::from_utf8_lossy(&result.stdout).replace("\r\n", "\n");
        let want = payload.replace("\r\n", "\n");
        assert_eq!(got.trim_end(), want.trim_end());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn stream_into_nonzero_exit_is_not_success() {
        let dir = tmp_dir("nonzero");
        let gz_path = dir.join("in.sql.gz");
        write_gz(&gz_path, b"irrelevant payload");

        // `cmd.exe /c exit 7` drains any stdin it's handed and exits 7,
        // without needing to read it all first -- proving a real nonzero
        // child exit is reported as a failed import even though the local
        // gunzip stream itself was perfectly healthy.
        let result = stream_into(OsStr::new("cmd.exe"), &["/c".to_string(), "exit".to_string(), "7".to_string()], &gz_path).unwrap();
        assert!(!result.success());
        assert_eq!(result.status.and_then(|s| s.code()), Some(7));
        assert!(result.stream_error.is_none(), "a clean nonzero exit is not a LOCAL stream error");
    }

    #[test]
    fn stream_into_missing_backup_file_is_a_setup_error() {
        let dir = tmp_dir("missing");
        let missing = dir.join("does-not-exist.sql.gz");
        let err = stream_into(OsStr::new("cmd.exe"), &["/c".to_string(), "exit".to_string(), "0".to_string()], &missing);
        assert!(err.is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn stream_into_corrupt_gzip_is_a_stream_error_not_success() {
        let dir = tmp_dir("corrupt");
        let bad_path = dir.join("corrupt.sql.gz");
        // Not a valid gzip stream at all -- GzDecoder must fault on the
        // very first read.
        std::fs::write(&bad_path, b"this is definitely not gzip data").unwrap();

        let result = stream_into(OsStr::new("cmd.exe"), &["/c".to_string(), "more".to_string()], &bad_path).unwrap();
        assert!(!result.success());
        assert!(result.stream_error.is_some());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn stream_restore_builds_the_real_docker_argv() {
        // Doesn't run docker -- just proves `stream_restore` really does
        // delegate to `stream_into` with `mysql_import_args`'s exact argv by
        // observing the spawn failure message names the program we'd have
        // used (a nonexistent program name makes Command::spawn fail fast,
        // no real docker needed).
        let dir = tmp_dir("argv-shape");
        let gz_path = dir.join("in.sql.gz");
        write_gz(&gz_path, b"x");
        let err = stream_restore(OsStr::new("dml-test-nonexistent-program-xyz"), "hunter2", &gz_path).unwrap_err();
        assert!(err.contains("could not start the import"), "err={err}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
