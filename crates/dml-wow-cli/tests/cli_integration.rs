//! Subprocess-level integration tests for the REAL compiled `dml-wow`
//! binary's own process-boundary wiring: argv parsing, exit codes, and the
//! stdout/stderr split. Everything in `cli.rs`/`out.rs` is unit-tested
//! in-process against pure logic; these instead spawn the actual binary via
//! the `CARGO_BIN_EXE_dml-wow` env var cargo sets for every integration test
//! in this package, so a future edit to `main.rs::handle_parse_error` (or
//! the clap tree in `cli.rs`) that silently breaks the 0/1/2 exit-code
//! contract fails a test instead of shipping quietly. (Task 10 review,
//! Minor finding.)
//!
//! Deliberately NOT here: an automated broken-pipe (Finding 2) simulation.
//! All four of today's subcommands emit exactly one line, so a
//! close-the-child's-stdout-handle-immediately trick races the child's own
//! near-instant write+exit and is not reliably able to land on the
//! BrokenPipe branch — the actual OS process-startup overhead usually (but
//! not deterministically) resolves the race, and a flaky regression test is
//! worse than none. The pure `classify_write` decision has direct unit
//! coverage in `out.rs`; the REAL end-to-end broken-pipe path (piping into
//! `head -1`) was verified manually and recorded verbatim in
//! `task-10-report.md`.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_dml-wow"))
}

fn parse_envelope(stdout: &[u8]) -> serde_json::Value {
    let text = String::from_utf8_lossy(stdout);
    serde_json::from_str(text.trim())
        .unwrap_or_else(|e| panic!("stdout was not one JSON envelope ({e}): {text:?}"))
}

/// (a) An unknown subcommand is a usage error: exit 2, a BAD_ARGS envelope
/// on stdout, and clap's own message on stderr.
#[test]
fn unknown_subcommand_is_bad_args_exit_2() {
    let out = bin()
        .arg("definitely-not-a-cmd")
        .output()
        .expect("spawn dml-wow definitely-not-a-cmd");

    assert_eq!(out.status.code(), Some(2), "stderr: {}", String::from_utf8_lossy(&out.stderr));

    let envelope = parse_envelope(&out.stdout);
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["error"]["code"], "BAD_ARGS");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unrecognized subcommand"),
        "expected clap's usage message on stderr, got: {stderr:?}"
    );
}

/// (b) `--help` and `--version` are not usage errors — clap's own handling
/// exits 0 for both (not folded into the BAD_ARGS/exit-2 path).
#[test]
fn help_flag_exits_zero() {
    let out = bin().arg("--help").output().expect("spawn dml-wow --help");
    assert_eq!(out.status.code(), Some(0));
    assert!(!out.stdout.is_empty(), "expected clap's help text on stdout");
}

#[test]
fn version_flag_exits_zero() {
    let out = bin().arg("--version").output().expect("spawn dml-wow --version");
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("dml-wow"), "expected clap's own version line, got: {stdout:?}");
}

/// (c) The `version` SUBCOMMAND (distinct from the `--version` FLAG above)
/// emits exactly one ok envelope and exits 0.
#[test]
fn version_subcommand_emits_exactly_one_ok_envelope() {
    let out = bin().arg("version").output().expect("spawn dml-wow version");
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stderr.is_empty(), "expected no stderr, got: {:?}", String::from_utf8_lossy(&out.stderr));

    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(lines.len(), 1, "expected exactly one line on stdout, got: {stdout:?}");

    let envelope: serde_json::Value = serde_json::from_str(lines[0]).expect("valid JSON");
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["contract"], "dml-json-v3");
    assert_eq!(envelope["data"]["backend"], "native");
}

/// Bonus, tying Finding 1's range gate to the real process boundary too (not
/// just the in-process clap parse tests in `cli.rs`): an out-of-range
/// `--lines` is a usage error, not a value that reaches `docker logs --tail`.
#[test]
fn console_tail_lines_out_of_range_is_bad_args_exit_2() {
    let out = bin()
        .args(["console-tail", "--lines", "1001"])
        .output()
        .expect("spawn dml-wow console-tail --lines 1001");

    assert_eq!(out.status.code(), Some(2));
    let envelope = parse_envelope(&out.stdout);
    assert_eq!(envelope["error"]["code"], "BAD_ARGS");
}

// ---------------------------------------------------------------------------
// Task 11: the `config write` allowlist, pinned at the PROCESS boundary.
//
// `dml_wow::config::raw_write` may only overwrite module confs (plus
// worldserver/authserver.conf); `.env` and `docker-compose.override.yml` are
// readable but read-only, because an editor that could rewrite either of them
// plus a `games restart` amounts to host command execution (see that
// function's SECURITY comment and `cli/src/90-main.sh:2752-2765`). The plan
// requires proof that the CLI arm actually inherits that refusal — as a
// SUBPROCESS test, so it pins the real argv -> dispatch -> library -> envelope
// -> exit-code path rather than an in-process call that could drift from what
// the shipped binary does.
//
// These run entirely inside a temp `DML_GAMES_DIR`: they never see, let alone
// write to, the real title dir.
// ---------------------------------------------------------------------------

/// A throwaway `DML_GAMES_DIR` holding one `wow-server-playerbots` title that
/// looks installed (has a compose file), with a `.env`, an override, and one
/// ordinary module conf. Returns the games dir; the title dir is one level in.
struct TempGames(PathBuf);

impl TempGames {
    fn new(tag: &str) -> TempGames {
        let dir = std::env::temp_dir().join(format!("dml-cli-it-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let title = dir.join("wow-server-playerbots");
        fs::create_dir_all(title.join("env").join("dist").join("etc").join("modules")).unwrap();
        fs::write(
            title.join("docker-compose.yml"),
            "services:\n  ac-worldserver:\n    image: dummy\n",
        )
        .unwrap();
        fs::write(title.join(".env"), "AC_SECRET=original\n").unwrap();
        fs::write(
            title.join("docker-compose.override.yml"),
            "services:\n  ac-worldserver:\n    environment:\n      AC_X: \"1\"\n",
        )
        .unwrap();
        fs::write(
            title
                .join("env")
                .join("dist")
                .join("etc")
                .join("modules")
                .join("mod_test.conf"),
            "SomeKey = old\n",
        )
        .unwrap();
        TempGames(dir)
    }

    fn title(&self) -> PathBuf {
        self.0.join("wow-server-playerbots")
    }

    /// Snapshot every file under the title dir as (relative path, bytes), so
    /// a test can prove NOTHING changed — not the target, not a `.bak`, not a
    /// stray `.tmp`.
    fn snapshot(&self) -> Vec<(PathBuf, Vec<u8>)> {
        fn walk(dir: &Path, base: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
            let Ok(rd) = fs::read_dir(dir) else { return };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, base, out);
                } else if let Ok(bytes) = fs::read(&p) {
                    out.push((p.strip_prefix(base).unwrap_or(&p).to_path_buf(), bytes));
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.title(), &self.title(), &mut out);
        out.sort();
        out
    }

    /// Run `dml-wow <args>` against this games dir, feeding `stdin_body` in.
    fn run(&self, args: &[&str], stdin_body: &str) -> (i32, serde_json::Value) {
        let mut child = bin()
            .args(args)
            .env("DML_GAMES_DIR", &self.0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|e| panic!("spawn dml-wow {args:?}: {e}"));
        child
            .stdin
            .as_mut()
            .expect("piped stdin")
            .write_all(stdin_body.as_bytes())
            .expect("write stdin");
        let out = child.wait_with_output().expect("wait for dml-wow");
        (out.status.code().unwrap_or(-1), parse_envelope(&out.stdout))
    }
}

impl Drop for TempGames {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// THE plan-required test: `dml-wow config write .env` must be refused with
/// an error envelope, and must not touch a single byte on disk.
#[test]
fn config_write_dot_env_is_refused_and_touches_no_file() {
    let g = TempGames::new("dotenv");
    let before = g.snapshot();

    let (code, envelope) = g.run(&["config", "write", ".env"], "AC_SECRET=pwned\n");

    assert_eq!(code, 1, "a refused write is an error envelope, exit 1: {envelope}");
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["error"]["code"], "BAD_ARG");
    assert_eq!(envelope["error"]["message"], "That file is read-only in the editor");

    assert_eq!(g.snapshot(), before, "a refused write must not change ANY file");
    assert_eq!(
        fs::read_to_string(g.title().join(".env")).unwrap(),
        "AC_SECRET=original\n",
        ".env content must be untouched"
    );
    assert!(!g.title().join(".env.bak").exists(), "no .bak may be taken for a refused write");
}

/// The compose override is refused the same way when the submitted body is
/// valid YAML...
#[test]
fn config_write_override_is_refused_and_touches_no_file() {
    let g = TempGames::new("override");
    let before = g.snapshot();

    let (code, envelope) = g.run(
        &["config", "write", "docker-compose.override.yml"],
        "services:\n  ac-worldserver:\n    entrypoint: [\"/bin/sh\", \"-c\", \"id\"]\n",
    );

    assert_eq!(code, 1);
    assert_eq!(envelope["error"]["code"], "BAD_ARG");
    assert_eq!(envelope["error"]["message"], "That file is read-only in the editor");
    assert_eq!(g.snapshot(), before, "a refused write must not change ANY file");
}

/// ...and the LOAD-BEARING ORDER survives the trip through the CLI: broken
/// YAML for the override reports "not valid YAML", NOT "read-only", because
/// `raw_write` validates the syntax BEFORE it rejects the file. If a future
/// edit swapped those two guards, this is the test that notices.
#[test]
fn config_write_override_yaml_check_runs_before_the_readonly_reject() {
    let g = TempGames::new("yamlorder");
    let before = g.snapshot();

    // An unterminated flow sequence — a hard parse error, not merely an
    // unexpected shape (a "wrong-looking" but well-formed document would sail
    // past the YAML gate and prove nothing about the ordering).
    let (code, envelope) = g.run(
        &["config", "write", "docker-compose.override.yml"],
        "services:\n  ac-worldserver: [unclosed\n",
    );

    assert_eq!(code, 1);
    assert_eq!(envelope["error"]["code"], "BAD_ARG");
    assert_eq!(
        envelope["error"]["message"], "That is not valid YAML - not saved",
        "the YAML gate must fire before the read-only rejection"
    );
    assert_eq!(g.snapshot(), before, "a refused write must not change ANY file");
}

/// The refusals above are meaningful only if the arm can write at all: an
/// allowlisted module conf goes through, takes its automatic `.bak`, and
/// reports `written`. (Also the one place the stdin body plumbing is proven
/// end to end.)
#[test]
fn config_write_module_conf_is_allowed_and_backs_up() {
    let g = TempGames::new("modconf");
    let conf = g
        .title()
        .join("env")
        .join("dist")
        .join("etc")
        .join("modules")
        .join("mod_test.conf");

    let (code, envelope) = g.run(&["config", "write", "mod_test.conf"], "SomeKey = new\n");

    assert_eq!(code, 0, "expected an ok envelope: {envelope}");
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["written"], true);
    assert_eq!(envelope["data"]["backup"], "mod_test.conf.bak");
    assert_eq!(fs::read_to_string(&conf).unwrap(), "SomeKey = new\n");
    assert_eq!(
        fs::read_to_string(conf.with_file_name("mod_test.conf.bak")).unwrap(),
        "SomeKey = old\n"
    );
}

/// A name outside the allowlist entirely (traversal attempt) is NOT_FOUND,
/// and nothing is created anywhere.
#[test]
fn config_write_rejects_a_traversal_name() {
    let g = TempGames::new("traversal");
    let before = g.snapshot();

    let (code, envelope) = g.run(&["config", "write", "../../evil.conf"], "x\n");

    assert_eq!(code, 1);
    assert_eq!(envelope["error"]["code"], "NOT_FOUND");
    assert_eq!(envelope["error"]["message"], "Not an editable file: ../../evil.conf");
    assert_eq!(g.snapshot(), before);
    assert!(!g.0.parent().unwrap().join("evil.conf").exists());
}

// ---------------------------------------------------------------------------
// Task 11: the read arms are wired and emit exactly one ok envelope. These two
// read NO runtime files at all (pure embedded registries), so they are stable
// on any machine, with or without a games dir, engine up or down.
// ---------------------------------------------------------------------------

#[test]
fn config_registry_emits_the_embedded_settings() {
    let out = bin().args(["config", "registry"]).output().expect("spawn dml-wow config registry");
    assert_eq!(out.status.code(), Some(0));
    let envelope = parse_envelope(&out.stdout);
    assert_eq!(envelope["ok"], true);
    let settings = envelope["data"]["settings"].as_array().expect("settings[]");
    assert_eq!(settings.len(), 66);
}

#[test]
fn module_catalog_emits_the_embedded_families() {
    let out = bin().args(["module", "catalog"]).output().expect("spawn dml-wow module catalog");
    assert_eq!(out.status.code(), Some(0));
    let envelope = parse_envelope(&out.stdout);
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["families"]["cpp"].as_array().expect("cpp[]").len(), 19);
}

/// `config get` on a key that isn't in the registry is a NOT_FOUND envelope
/// (exit 1), not a crash and not an empty ok.
#[test]
fn config_get_unknown_key_is_not_found() {
    let g = TempGames::new("getunknown");
    let (code, envelope) = g.run(&["config", "get", "no.such.setting"], "");
    assert_eq!(code, 1);
    assert_eq!(envelope["error"]["code"], "NOT_FOUND");
    assert_eq!(envelope["error"]["message"], "Unknown setting: no.such.setting");
}

// ---------------------------------------------------------------------------
// Task 12 review, finding 3: subprocess-level coverage for the database page
// reads (argv parsing, validator branches, error mapping, exit codes).
// Nothing above this point exercises those subcommands at all -- the
// DB-gated parity suites in `crates/dml-wow/tests` (`db_pages_parity.rs`,
// `stats_parity.rs`, `iteminfo_parity.rs`) call the reader FUNCTIONS
// directly and never spawn the real `dml-wow` binary, so a bug in this
// crate's own argv parsing, validation, or error-code mapping (exactly
// finding 1's hand-copied `DB_UNREACHABLE` hint) was invisible to every test
// that ran. These five close that gap.
// ---------------------------------------------------------------------------

/// `true` when the real DB `DbConfig::from_env()` resolves is reachable
/// right now -- the same lightweight TCP probe `db_pages_parity.rs` uses to
/// skip its DB-gated tests gracefully rather than failing the whole suite
/// when only the DB tier (or nothing) is running.
fn db_reachable() -> bool {
    let cfg = dml_wow::db::DbConfig::from_env();
    let addr = format!("{}:{}", cfg.host, cfg.port);
    addr.parse()
        .ok()
        .and_then(|a: std::net::SocketAddr| {
            std::net::TcpStream::connect_timeout(&a, std::time::Duration::from_millis(600)).ok()
        })
        .is_some()
}

/// (1) A DB-gated success path, end to end: argv -> real DB read -> ok
/// envelope -> exit 0. `--search Orgrimmar` is stable ground truth (a
/// `game_tele` row every AzerothCore DB ships with).
#[test]
fn teleport_list_search_succeeds_end_to_end_against_the_real_db() {
    if !db_reachable() {
        eprintln!("SKIP teleport_list_search_succeeds_end_to_end_against_the_real_db: no DB reachable");
        return;
    }
    let out = bin()
        .args(["teleport-list", "--search", "Orgrimmar"])
        .output()
        .expect("spawn dml-wow teleport-list --search Orgrimmar");
    assert_eq!(out.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let envelope = parse_envelope(&out.stdout);
    assert_eq!(envelope["ok"], true);
    let locations = envelope["data"]["locations"].as_array().expect("locations[]");
    assert!(
        locations.iter().any(|l| l["name"].as_str() == Some("Orgrimmar")),
        "expected an Orgrimmar row, got: {locations:?}"
    );
}

/// (2) A DB-gated domain BAD_ARG rejection: an invalid `--class` is refused
/// BEFORE any SQL runs (exit 1) -- distinct from a clap usage error (exit 2)
/// and from a DB round trip.
#[test]
fn bots_invalid_class_is_bad_arg_exit_1() {
    if !db_reachable() {
        eprintln!("SKIP bots_invalid_class_is_bad_arg_exit_1: no DB reachable");
        return;
    }
    let out = bin().args(["bots", "--class", "999"]).output().expect("spawn dml-wow bots --class 999");
    assert_eq!(out.status.code(), Some(1));
    let envelope = parse_envelope(&out.stdout);
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["error"]["code"], "BAD_ARG");
    assert_eq!(envelope["error"]["message"], "Invalid class id: 999");
}

/// (3) A DB-gated NOT_FOUND path: a syntactically valid but nonexistent
/// character name.
#[test]
fn paperdoll_unknown_character_is_not_found_exit_1() {
    if !db_reachable() {
        eprintln!("SKIP paperdoll_unknown_character_is_not_found_exit_1: no DB reachable");
        return;
    }
    let out = bin().args(["paperdoll", "NoSuchChar"]).output().expect("spawn dml-wow paperdoll NoSuchChar");
    assert_eq!(out.status.code(), Some(1));
    let envelope = parse_envelope(&out.stdout);
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["error"]["code"], "NOT_FOUND");
    assert_eq!(envelope["error"]["message"], "No such character or no equipped items: NoSuchChar");
}

/// (4) THE test that would have caught review finding 1: an unreachable DB
/// (via an env override that needs no real server, up or down) must report
/// the EXACT hint `dml_wow::db::db_err_to_cmd` uses, not a hand-copied
/// string that merely looked plausible. NOT DB-gated -- the whole point is
/// that this works regardless of whether `ac-database` is up.
#[test]
fn db_unreachable_reports_the_launchers_exact_hint() {
    let out = bin()
        .arg("players-online")
        .env("DOCKER_DB_EXTERNAL_PORT", "1")
        .output()
        .expect("spawn dml-wow players-online with an unreachable DB port");
    assert_eq!(out.status.code(), Some(1));
    let envelope = parse_envelope(&out.stdout);
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["error"]["code"], "DB_UNREACHABLE");
    assert_eq!(
        envelope["error"]["hint"],
        "Is ac-database running? (native mode reads MySQL directly on 127.0.0.1)",
        "must be db_err_to_cmd's own hint, not a second hand-copied string"
    );
}

/// (5a) item-info's FORMAT check is a clap usage error (exit 2) -- fails at
/// parse time, no DB touched.
#[test]
fn item_info_malformed_ids_is_usage_error_exit_2() {
    let out = bin().args(["item-info", "abc"]).output().expect("spawn dml-wow item-info abc");
    assert_eq!(out.status.code(), Some(2));
    let envelope = parse_envelope(&out.stdout);
    assert_eq!(envelope["error"]["code"], "BAD_ARGS");
}

/// (5b) item-info's 25-id CAP (review finding 2) is a `run.rs` domain
/// rejection, exit 1 -- distinct from the exit-2 format check above, and
/// matching the bash/launcher siblings' exact wording ("--entries max 25 ids
/// per call", even though this CLI's own flag isn't named `--entries`).
#[test]
fn item_info_over_25_ids_is_bad_arg_exit_1() {
    let too_many = (1..=26).map(|n| n.to_string()).collect::<Vec<_>>().join(",");
    let out = bin().args(["item-info", &too_many]).output().expect("spawn dml-wow item-info <26 ids>");
    assert_eq!(out.status.code(), Some(1));
    let envelope = parse_envelope(&out.stdout);
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["error"]["code"], "BAD_ARG");
    assert_eq!(envelope["error"]["message"], "--entries max 25 ids per call");
}
