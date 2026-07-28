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

// ---------------------------------------------------------------------------
// Task 13: every validated argument of the SOAP/account/GM/mail/teleport/motd/
// party family gets a rejection test proving its guard fires BEFORE any side
// effect.
//
// HOW THE "no side effect" HALF IS PROVEN, without needing a server up OR
// down: [`Sealed`] points the child at a SOAP endpoint and a MySQL port that
// are both guaranteed dead (127.0.0.1:1 — nothing may bind port 1), and gives
// it a private HOME/USERPROFILE + games dir. So for every case below there are
// only three possible outcomes, and they are mutually exclusive:
//
//   * the guard fires first        -> BAD_ARG      (what each test asserts)
//   * the SOAP call was reached    -> SOAP_UNREACHABLE
//   * a DB read was reached        -> DB_UNREACHABLE, or the NOT_FOUND that
//                                     `char_is_online`/`party_online_guid`
//                                     produce when they swallow a DB failure
//
// Asserting BAD_ARG (and the exact message) therefore proves the ordering, not
// merely that an error happened. The preset arms additionally snapshot the
// private home dir to prove no file was created, moved or deleted.
// ---------------------------------------------------------------------------

/// A child-process sandbox with every outside resource sealed off: a dead SOAP
/// endpoint, a dead DB port, and a private home + games dir. Anything that
/// escapes a guard hits a wall instead of the user's real server.
struct Sealed {
    root: PathBuf,
}

impl Sealed {
    fn new(tag: &str) -> Sealed {
        let root = std::env::temp_dir().join(format!("dml-cli-t13-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("home")).unwrap();
        fs::create_dir_all(root.join("games")).unwrap();
        Sealed { root }
    }

    fn home(&self) -> PathBuf {
        self.root.join("home")
    }

    /// `~/.dml/party-presets`, the dir `party::preset_dir()` resolves to
    /// inside this sandbox.
    fn presets(&self) -> PathBuf {
        self.home().join(".dml").join("party-presets")
    }

    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = bin();
        c.args(args)
            // Dead SOAP endpoint: port 1 can never be bound, so ANY SOAP call
            // that gets past a guard reports SOAP_UNREACHABLE.
            .env("DML_SOAP_URL", "http://127.0.0.1:1/")
            .env("DML_SOAP_USER", "sealed")
            .env("DML_SOAP_PASS", "sealed")
            // Dead MySQL port, same idea (see Task 12's
            // `db_unreachable_reports_the_launchers_exact_hint`).
            .env("DOCKER_DB_EXTERNAL_PORT", "1")
            // Private state dir: `~/.dml/party-presets` and `~/.dml/soap.env`
            // both resolve in here, never in the developer's real home.
            .env("USERPROFILE", self.home())
            .env("HOME", self.home())
            .env("DML_GAMES_DIR", self.root.join("games"));
        c
    }

    /// Run `dml-wow <args>` sealed; returns (exit code, stdout envelope).
    fn run(&self, args: &[&str]) -> (i32, serde_json::Value) {
        let out = self
            .cmd(args)
            .output()
            .unwrap_or_else(|e| panic!("spawn dml-wow {args:?}: {e}"));
        (out.status.code().unwrap_or(-1), parse_envelope(&out.stdout))
    }

    /// Every file under the private home, as (relative path, bytes).
    fn home_snapshot(&self) -> Vec<(PathBuf, Vec<u8>)> {
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
        walk(&self.home(), &self.home(), &mut out);
        out.sort();
        out
    }
}

impl Drop for Sealed {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Assert one sealed invocation is a BAD_ARG error envelope with `message`,
/// exit 1 — i.e. the guard beat the SOAP/DB call to it.
fn assert_bad_arg(s: &Sealed, args: &[&str], message: &str) {
    let (code, envelope) = s.run(args);
    assert_eq!(code, 1, "{args:?} should be an error envelope, got: {envelope}");
    assert_eq!(envelope["ok"], false, "{args:?}: {envelope}");
    assert_eq!(
        envelope["error"]["code"], "BAD_ARG",
        "{args:?} must be refused by its guard, not by the sealed SOAP/DB endpoint: {envelope}"
    );
    assert_eq!(envelope["error"]["message"], message, "{args:?}: {envelope}");
}

/// `console` rejects a WHITESPACE-only command (not merely an empty one)
/// before dialling SOAP.
#[test]
fn console_whitespace_only_command_is_refused_before_any_soap_call() {
    let s = Sealed::new("console");
    assert_bad_arg(&s, &["console", "   "], "console-send requires a non-empty --command");
    // Sanity check on the seal itself: a WELL-FORMED command DOES reach SOAP
    // and reports the dead endpoint. Without this, the assertion above could
    // pass for the wrong reason (e.g. if the arm were broken and always said
    // BAD_ARG).
    let (code, envelope) = s.run(&["console", "server", "info"]);
    assert_eq!(code, 1);
    assert_eq!(envelope["error"]["code"], "SOAP_UNREACHABLE", "{envelope}");
}

#[test]
fn account_arms_reject_bad_credentials_before_any_soap_call() {
    let s = Sealed::new("account");
    assert_bad_arg(&s, &["account", "create", "ab", "pw12"], "Invalid username (3-20 letters/digits/_)");
    assert_bad_arg(
        &s,
        &["account", "create", "bob", "x"],
        "Invalid password (4-16 chars, letters/digits/_@#%+=!-)",
    );
    assert_bad_arg(&s, &["account", "set-password", "bob", "x"], "Invalid password (4-16 chars, letters/digits/_@#%+=!-)");
    // `set-password`'s USERNAME validation (review Fix 2) -- `validate_account_user`
    // runs before `validate_account_pass`, so a bad username must be caught even
    // with an otherwise-valid password.
    assert_bad_arg(
        &s,
        &["account", "set-password", "bad name", "pw12"],
        "Invalid username (3-20 letters/digits/_)",
    );
    assert_bad_arg(&s, &["account", "set-gm", "bob", "4"], "--level must be 0-3");
    // An out-of-u8 level is still a DOMAIN rejection (exit 1), not a clap
    // integer-parse usage error -- that is why `set-gm`'s level is a String.
    assert_bad_arg(&s, &["account", "set-gm", "bob", "9999"], "--level must be 0-3");
    // `set-gm`'s USERNAME validation (review Fix 2) -- the same guard, checked
    // before the 0-3 level match.
    assert_bad_arg(
        &s,
        &["account", "set-gm", "bad name", "3"],
        "Invalid username (3-20 letters/digits/_)",
    );
    // The admin-account refusal is a guard too: it must never reach SOAP.
    assert_bad_arg(&s, &["account", "delete", "ADMIN"], "Refusing to delete the admin account");
    // `delete`'s GENERIC username guard (review Fix 2) -- a syntactically
    // invalid username must be caught by `validate_account_user`, distinctly
    // from the admin-account special case above.
    assert_bad_arg(&s, &["account", "delete", "bad name"], "Invalid username (3-20 letters/digits/_)");
}

#[test]
fn gm_arms_reject_bad_names_levels_and_entries_before_any_soap_or_db_call() {
    let s = Sealed::new("gm");
    // The brief's own example.
    assert_bad_arg(&s, &["gm", "level", "bad name", "10"], "Invalid player name: bad name");
    assert_bad_arg(&s, &["gm", "level", "Testen", "256"], "Invalid level: 256");
    assert_bad_arg(&s, &["gm", "level", "Testen", "0"], "Invalid level: 0");
    assert_bad_arg(&s, &["gm", "level", "Testen", "-5"], "Invalid level: -5");
    assert_bad_arg(&s, &["gm", "at-login", "Testen", "resurrect"], "Invalid flag: resurrect");
    // `at-login`'s PLAYER guard (review Fix 2) -- `gm_at_login_cmd` validates
    // the name before the four-flag allowlist, same order as `gm level`.
    assert_bad_arg(&s, &["gm", "at-login", "bad name", "rename"], "Invalid player name: bad name");
    assert_bad_arg(&s, &["gm", "gold", "Testen", "214749"], "Invalid gold amount: 214749");
    assert_bad_arg(&s, &["gm", "summon", "Testen", "0"], "Invalid creature entry: 0");
    assert_bad_arg(&s, &["gm", "summon", "Testen", "1000000"], "Invalid creature entry: 1000000");
    // `summon`'s PLAYER guard (review Fix 2) -- `gm_summon_cmd` validates the
    // name before the 1..=999999 entry range, so this must be BAD_ARG rather
    // than reaching the World-DB creature lookup `gm_summon` does next.
    assert_bad_arg(&s, &["gm", "summon", "bad name", "190"], "Invalid player name: bad name");
    // The three bridge ops validate the NAME before the online DB check --
    // otherwise this would come back NOT_FOUND (`char_is_online` swallows the
    // sealed DB's failure and reads as "not online"), not BAD_ARG.
    assert_bad_arg(&s, &["gm", "heal", "bad name"], "Invalid player name: bad name");
    assert_bad_arg(&s, &["gm", "revive", "bad name"], "Invalid player name: bad name");
    assert_bad_arg(&s, &["gm", "gold", "bad name", "5"], "Invalid player name: bad name");
}

/// The ordering that the sealed DB makes visible: for a VALID name, `gm heal`
/// gets past the guard and is stopped by the online check -- proving the
/// BAD_ARG cases above really were the guard and not a blanket refusal.
#[test]
fn gm_heal_valid_name_reaches_the_online_check_and_stops_there() {
    let s = Sealed::new("gmorder");
    let (code, envelope) = s.run(&["gm", "heal", "Testen"]);
    assert_eq!(code, 1);
    assert_eq!(envelope["error"]["code"], "NOT_FOUND", "{envelope}");
    assert_eq!(envelope["error"]["message"], "Character not online: Testen");
    assert_eq!(
        envelope["error"]["hint"],
        "This action needs the character logged in. (Set level works offline.)"
    );
}

#[test]
fn mail_item_rejects_bad_recipient_specs_and_counts_before_any_soap_call() {
    let s = Sealed::new("mail");
    assert_bad_arg(&s, &["mail-item", "bad name", "6948:1"], "Invalid character name: bad name");
    assert_bad_arg(&s, &["mail-item", "Testen", "nope"], "Malformed item spec: nope");
    assert_bad_arg(&s, &["mail-item", "Testen", "6948"], "Malformed item spec: 6948");
    // An empty items token splits to ZERO fields (bash's `IFS=','` semantics),
    // so it lands on the COUNT error, not the spec one.
    assert_bad_arg(&s, &["mail-item", "Testen", ""], "Provide 1-12 items as id:count[,id:count…]");
    // 13 specs, given as one comma list -- the 1-12 cap.
    let thirteen = vec!["1:1"; 13].join(",");
    assert_bad_arg(
        &s,
        &["mail-item", "Testen", &thirteen],
        "Provide 1-12 items as id:count[,id:count…]",
    );
}

#[test]
fn teleport_rejects_bad_character_and_location_before_any_soap_call() {
    let s = Sealed::new("tele");
    assert_bad_arg(&s, &["teleport", "bad name", "Orgrimmar"], "Invalid character name: bad name");
    // A multi-word destination is refused rather than quote-wrapped: AC's
    // console parser keeps quotes LITERAL, so quoting would not have helped.
    assert_bad_arg(&s, &["teleport", "Testen", "bad name"], "Invalid location name: bad name");
    assert_bad_arg(&s, &["teleport", "Testen", ""], "Missing --to <location>");
}

/// `motd` goes through `config_set`'s `server.motd` special case, so its first
/// guard is that special case's own installed-server check -- reached before
/// any SOAP call. (The sanitization that strips quotes/CR-LF runs immediately
/// after, inside the same function; it has its own library coverage.)
#[test]
fn motd_is_refused_when_no_server_is_installed_before_any_soap_call() {
    let s = Sealed::new("motd");
    let (code, envelope) = s.run(&["motd", "Welcome \"home\"\nfriends"]);
    assert_eq!(code, 1);
    assert_eq!(envelope["error"]["code"], "NOT_FOUND", "{envelope}");
    assert_eq!(envelope["error"]["message"], "WoW Playerbots server not installed");
}

#[test]
fn party_add_rejects_bad_player_class_gender_and_spec_before_any_soap_or_db_call() {
    let s = Sealed::new("partyadd");
    assert_bad_arg(&s, &["party", "add", "bad name", "warrior"], "Invalid player name: bad name");
    assert_bad_arg(&s, &["party", "add", "Testen", "deathknight"], "Invalid class: deathknight");
    assert_bad_arg(
        &s,
        &["party", "add", "Testen", "warrior", "--gender", "other"],
        "Invalid gender: other",
    );
    // No playerbots.conf is deployed in the sealed games dir, so the spec
    // check falls back to the static premade-name mirror -- and rejects.
    assert_bad_arg(
        &s,
        &["party", "add", "Testen", "warrior", "--spec", "bear pvp"],
        "Unknown spec: bear pvp",
    );
}

impl Sealed {
    /// Deploy a `playerbots.conf` under this sandbox's title dir carrying
    /// `names` as premade spec names, VERBATIM (one
    /// `AiPlayerbot.PremadeSpecName.8.<n>` row each) -- the same seeding
    /// `cli/tests/wow-party-specs.bats`'s "the wider charset is still
    /// injection-safe for the whisper tail" does.
    ///
    /// WHY IT MATTERS: without a conf the games dir is empty,
    /// `party::live_spec_names` returns `None`, and `valid_bot_spec` falls
    /// back to the static `FALLBACK_SPEC_NAMES` mirror -- where every hostile
    /// name is already a non-member, so the membership lookup alone produces
    /// the exact rejection envelope and the shape guard could be deleted with
    /// no test noticing (round-2 review finding). Seeding the hostile names
    /// makes them MEMBERS, which leaves `valid_bot_spec_shape` as the only
    /// thing that can still reject them.
    fn seed_playerbots_conf(&self, names: &[&str]) {
        let title = self.root.join("games").join("wow-server-playerbots");
        let mods = title.join("env").join("dist").join("etc").join("modules");
        fs::create_dir_all(&mods).unwrap();
        // `party_specs::find_conf` resolves through `maint::resolve_server_dir`,
        // which only accepts a dir that HAS a compose file -- without this the
        // conf is invisible and the seeding would silently do nothing.
        fs::write(
            title.join("docker-compose.yml"),
            "services:\n  ac-worldserver:\n    image: dummy\n",
        )
        .unwrap();
        let mut conf = String::from("# seeded by cli_integration.rs\n");
        for (i, name) in names.iter().enumerate() {
            conf.push_str(&format!("AiPlayerbot.PremadeSpecName.8.{i} = {name}\n"));
        }
        fs::write(mods.join("playerbots.conf"), conf).unwrap();
    }
}

/// Shape-INVALID spec names, all seeded into the live conf by the test below.
/// SQL/shell-metacharacter, quote and backslash shapes that would be dangerous
/// concatenated into the `dml_whisper <p> <b> talents spec <name>` tail, plus
/// `.frost` for the "must START alphanumeric" branch (`.` is legal in the TAIL,
/// so nothing but the first-character rule can refuse it). NB a hyphen/dot/
/// underscore/uppercase INSIDE a name is deliberately absent: the charset was
/// widened to cover hand-written conf names (a picker offering a spec the
/// validator refused was the drift being fixed), so `Frost-PvE 2.0` is
/// shape-VALID and only membership decides it. A leading SPACE is likewise not
/// here: `parse_spec_rows` trims the conf value, so ` frost` can never be a
/// member and would be a vacuous case again.
const SHAPE_INVALID_SPECS: &[&str] = &[
    "frost'; DROP TABLE bots; --",
    "frost\"pve",
    "frost\\pve",
    "frost&pve",
    "frost<pve>",
    "frost|pve",
    "$(id)",
    "frost`id`",
    ".frost",
];

/// The other first-character case, kept apart because clap swallows a bare
/// `--spec -frost` as an unknown FLAG (BAD_ARGS/exit 2) long before the guard
/// runs, so it can only be delivered in the `--spec=<value>` form.
const LEADING_HYPHEN_SPEC: &str = "-frost";

/// Shape-VALID, and absent from `FALLBACK_SPEC_NAMES` on purpose: it can only
/// pass validation when the seeded conf was really read. The test's
/// non-vacuity control.
const SEEDED_CONTROL_SPEC: &str = "custom test pve";

/// `valid_bot_spec_shape`'s `^[A-Za-z0-9][A-Za-z0-9 ._-]*$` injection guard
/// (review Fix 2): `bear pvp` above is shape-VALID and fails only on
/// membership, so it proves nothing about whether the shape guard itself
/// still runs -- deleting `valid_bot_spec_shape` entirely would not change
/// that assertion's outcome.
///
/// The same trap applies to shape-INVALID inputs run against an EMPTY games
/// dir, which is how this test used to work: with no conf deployed the
/// membership lookup falls back to the static mirror, which rejects every
/// hostile name too, so `valid_bot_spec_shape` could be replaced by `true` and
/// this test stayed green (round-2 review finding). So every hostile name is
/// now SEEDED into a live `playerbots.conf` verbatim -- making it a MEMBER --
/// which leaves the shape guard as the only thing that can still refuse it, on
/// BOTH `party add --spec` and `party botcmd ... spec --spec`.
///
/// The message asserted is the SAME shape for every shape-invalid input
/// ("Unknown spec: <value>"): `valid_bot_spec` collapses shape and membership
/// failures into one BAD_ARG, so the value round-tripping into the message
/// unmangled is itself the proof the guard rejected the raw string rather
/// than something derived from it.
#[test]
fn party_add_and_botcmd_spec_reject_shape_invalid_values_before_any_soap_or_db_call() {
    let s = Sealed::new("specshape");
    let mut seeded: Vec<&str> = SHAPE_INVALID_SPECS.to_vec();
    seeded.push(LEADING_HYPHEN_SPEC);
    seeded.push(SEEDED_CONTROL_SPEC);
    s.seed_playerbots_conf(&seeded);

    let add_hint =
        "A premade spec name like 'frost pve' -- see the launcher's role picker for the full list.";
    let botcmd_hint = "A premade spec name like 'frost pve'.";

    // NON-VACUITY CONTROL, first: the seeded conf must actually be found,
    // parsed and consulted, or the loop below is back to proving nothing.
    // `custom test pve` is shape-valid and NOT in the static mirror, so it can
    // only get past the spec guard via the live conf -- and then dies at the
    // next guard, the sealed DB's online lookup (NOT_FOUND). A BAD_ARG here
    // means the seeding broke.
    for args in [
        vec!["party", "add", "Testen", "warrior", "--spec", SEEDED_CONTROL_SPEC],
        vec!["party", "botcmd", "Testen", "Botty", "spec", "--spec", SEEDED_CONTROL_SPEC],
    ] {
        let (code, envelope) = s.run(&args);
        assert_eq!(code, 1, "{args:?}: {envelope}");
        assert_eq!(
            envelope["error"]["code"], "NOT_FOUND",
            "{args:?}: the seeded playerbots.conf must be the live spec source \
             (BAD_ARG here = the conf was not read, so the shape assertions below \
             would be vacuous): {envelope}"
        );
        assert_eq!(
            envelope["error"]["message"], "Character not online: Testen",
            "{args:?}: {envelope}"
        );
    }

    // One shape rejection, asserted whole. Factored out so the `--spec=<value>`
    // form below can reuse it verbatim.
    let assert_unknown_spec = |args: &[&str], bad_spec: &str, hint: &str| {
        let (code, envelope) = s.run(args);
        assert_eq!(code, 1, "{args:?}: {envelope}");
        assert_eq!(
            envelope["error"]["code"], "BAD_ARG",
            "{args:?}: this name IS in the seeded conf, so only the shape guard \
             can refuse it: {envelope}"
        );
        assert_eq!(
            envelope["error"]["message"],
            format!("Unknown spec: {bad_spec}"),
            "{args:?}: {envelope}"
        );
        assert_eq!(envelope["error"]["hint"], hint, "{args:?}: {envelope}");
    };

    for &bad_spec in SHAPE_INVALID_SPECS {
        assert_unknown_spec(
            &["party", "add", "Testen", "warrior", "--spec", bad_spec],
            bad_spec,
            add_hint,
        );
        assert_unknown_spec(
            &["party", "botcmd", "Testen", "Botty", "spec", "--spec", bad_spec],
            bad_spec,
            botcmd_hint,
        );
    }

    // The leading-hyphen case, in the only form that reaches the guard (see
    // `LEADING_HYPHEN_SPEC`) -- and the form a front end that builds argv
    // itself would naturally produce.
    let add_eq = format!("--spec={LEADING_HYPHEN_SPEC}");
    assert_unknown_spec(
        &["party", "add", "Testen", "warrior", &add_eq],
        LEADING_HYPHEN_SPEC,
        add_hint,
    );
    assert_unknown_spec(
        &["party", "botcmd", "Testen", "Botty", "spec", &add_eq],
        LEADING_HYPHEN_SPEC,
        botcmd_hint,
    );
}

#[test]
fn party_kick_relogin_and_botcmd_reject_bad_input_before_any_soap_or_db_call() {
    let s = Sealed::new("partycmds");
    // `kick` validates the MASTER itself (its own hint) -- the uninvite
    // builder only ever sees the bot.
    let (code, envelope) = s.run(&["party", "kick", "bad name", "Botty"]);
    assert_eq!(code, 1);
    assert_eq!(envelope["error"]["code"], "BAD_ARG", "{envelope}");
    assert_eq!(envelope["error"]["message"], "Invalid player name: bad name");
    assert_eq!(
        envelope["error"]["hint"],
        "Kick needs --player (the bot's master) so the bot can also be dismissed."
    );

    assert_bad_arg(&s, &["party", "kick", "Testen", "bad bot"], "Invalid bot name: bad bot");
    assert_bad_arg(&s, &["party", "relogin", "bad name", "Botty"], "Invalid player name: bad name");
    assert_bad_arg(&s, &["party", "relogin", "Testen", "bad bot"], "Invalid bot name: bad bot");

    assert_bad_arg(&s, &["party", "botcmd", "bad name", "Botty", "gear"], "Invalid player name: bad name");
    assert_bad_arg(&s, &["party", "botcmd", "Testen", "bad bot", "gear"], "Invalid bot name: bad bot");
    assert_bad_arg(&s, &["party", "botcmd", "Testen", "Botty", "dance"], "Invalid action: dance");
    assert_bad_arg(
        &s,
        &["party", "botcmd", "Testen", "Botty", "spec"],
        "Action spec requires --spec <name>",
    );
    assert_bad_arg(
        &s,
        &["party", "botcmd", "Testen", "Botty", "spec", "--spec", "nonsense"],
        "Unknown spec: nonsense",
    );
}

/// The preset arms' name guard is a PATH-TRAVERSAL guard: it must fire before
/// `~/.dml/party-presets` is ever joined with the argument. Proven by
/// snapshotting the whole private home before and after.
#[test]
fn preset_arms_reject_traversal_names_and_touch_no_file() {
    let s = Sealed::new("presetguard");
    fs::create_dir_all(s.presets()).unwrap();
    fs::write(s.presets().join("keepme"), "warrior\n").unwrap();
    let before = s.home_snapshot();

    assert_bad_arg(&s, &["party", "preset-save", "Testen", "../evil"], "Invalid preset name: ../evil");
    assert_bad_arg(&s, &["party", "preset-delete", "../evil"], "Invalid preset name: ../evil");
    assert_bad_arg(&s, &["party", "preset-load", "Testen", "../evil"], "Invalid preset name: ../evil");
    assert_bad_arg(&s, &["party", "preset-save", "bad name", "raiders"], "Invalid player name: bad name");
    assert_bad_arg(&s, &["party", "preset-load", "bad name", "raiders"], "Invalid player name: bad name");

    assert_eq!(s.home_snapshot(), before, "a refused preset command must not change ANY file");
    assert!(!s.root.parent().unwrap().join("evil").exists(), "nothing may escape the preset dir");
}

/// `preset-load` is the CLI's first STREAMING subcommand, so its guards have
/// an extra obligation: a rejection is ONE ordinary error envelope, never a
/// half-emitted NDJSON stream. (A stream that had started would have printed a
/// `section_start` line first.)
#[test]
fn preset_load_guard_emits_exactly_one_envelope_not_a_stream() {
    let s = Sealed::new("presetload");
    let out = s
        .cmd(&["party", "preset-load", "bad name", "raiders"])
        .output()
        .expect("spawn dml-wow party preset-load");
    assert_eq!(out.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(lines.len(), 1, "expected one envelope, got a stream: {stdout:?}");
    let envelope: serde_json::Value = serde_json::from_str(lines[0]).expect("valid JSON");
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["error"]["code"], "BAD_ARG");
    assert!(envelope["event"].is_null(), "an envelope, not a stream event: {envelope}");
}

/// A well-formed `preset-load` for a preset that does not exist stops at the
/// NOT_FOUND check -- and does so as a STREAM (section_start -> section_end ->
/// error), exiting 1 off the terminal `error` event. This is the wiring later
/// streaming tasks copy, so it is pinned here.
#[test]
fn preset_load_missing_preset_streams_an_error_event_and_exits_1() {
    let s = Sealed::new("presetmissing");
    let out = s
        .cmd(&["party", "preset-load", "Testen", "nosuchpreset"])
        .output()
        .expect("spawn dml-wow party preset-load");
    assert_eq!(out.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let events: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("not JSON ({e}): {l:?}")))
        .collect();
    assert_eq!(events.first().map(|e| e["event"].clone()), Some(serde_json::json!("section_start")));
    let last = events.last().expect("at least one event");
    assert_eq!(last["event"], "error");
    assert_eq!(last["error"]["code"], "NOT_FOUND");
    assert_eq!(last["error"]["message"], "No preset named nosuchpreset");
}

/// The preset read/write family, end to end inside the private home: list an
/// empty dir, fail to delete something absent, then delete something present.
/// No server of any kind is involved.
#[test]
fn preset_list_and_delete_round_trip_in_a_private_home() {
    let s = Sealed::new("presetrt");

    // (1) No preset dir at all -> an empty list, not an error.
    let (code, envelope) = s.run(&["party", "preset-list"]);
    assert_eq!(code, 0, "{envelope}");
    assert_eq!(envelope["data"]["presets"].as_array().expect("presets[]").len(), 0);

    // (2) Two presets, one with an invalid on-disk name that must be skipped.
    fs::create_dir_all(s.presets()).unwrap();
    fs::write(s.presets().join("raiders"), "warrior\npriest\nmage\n").unwrap();
    fs::write(s.presets().join("duo"), "warrior\npriest\n").unwrap();
    fs::write(s.presets().join("bad name"), "warrior\n").unwrap();
    let (code, envelope) = s.run(&["party", "preset-list"]);
    assert_eq!(code, 0, "{envelope}");
    assert_eq!(
        envelope["data"]["presets"],
        serde_json::json!([{"name": "duo", "bots": 2}, {"name": "raiders", "bots": 3}])
    );

    // (3) Deleting a preset that isn't there is NOT_FOUND, and removes nothing.
    let before = s.home_snapshot();
    let (code, envelope) = s.run(&["party", "preset-delete", "nosuch"]);
    assert_eq!(code, 1);
    assert_eq!(envelope["error"]["code"], "NOT_FOUND");
    assert_eq!(envelope["error"]["message"], "No preset named nosuch");
    assert_eq!(s.home_snapshot(), before);

    // (4) Deleting a real one works and removes exactly that file.
    let (code, envelope) = s.run(&["party", "preset-delete", "duo"]);
    assert_eq!(code, 0, "{envelope}");
    assert_eq!(envelope["data"]["deleted"], true);
    assert_eq!(envelope["data"]["name"], "duo");
    assert!(!s.presets().join("duo").exists());
    assert!(s.presets().join("raiders").exists());
}

// ---------------------------------------------------------------------------
// Task 14: lifecycle, module mutations, backup/restore and the four DESTRUCTIVE
// subcommands.
//
// SAFETY RULE FOR THIS WHOLE SECTION, stated precisely (an absolute that is
// untrue is worse than a narrow one that holds — Task 14 review, Fix 3):
// `backup restore`, `docker-clean` and `games-remove` are NEVER invoked with
// `--yes` anywhere in this file. `bots-flush --yes` IS spawned three times, in
// `bots_flush_without_yes_or_ack_is_refused_and_arms_nothing`, and only ever
// with a MISSING or WRONG `--ack` — `lifecycle::bots_flush_confirmed(yes, ack)`
// refuses every one of them before `bots_flush_stream` is reached, which is
// precisely what those three cases exist to prove. So the invariant that
// actually holds, and the one to preserve when adding tests here, is: NO
// destructive subcommand is ever invoked with `--yes` AND a valid `--ack`.
// Everything below exercises the REFUSAL side of those four, plus the
// non-destructive arms. What the confirmed path actually does when it is
// allowed to run is owed to the user's supervised smoke session, and is
// recorded as such in the task report.
//
// Everything reuses [`Sealed`] from the Task 13 block above: a dead SOAP
// endpoint, a dead MySQL port, and a private HOME/USERPROFILE + games dir. A
// guard that fires produces its own envelope; a guard that DIDN'T fire would
// instead reach the sealed wall (SOAP_UNREACHABLE / DB_UNREACHABLE) or, for
// the streaming arms, an NDJSON stream whose `section_start` line is visible
// in stdout. Asserting the envelope AND the line count therefore pins the
// ordering, not merely that an error happened.
// ---------------------------------------------------------------------------

impl Sealed {
    /// `~/.dml/backups`, the dir `backup::backup_dir()` resolves to inside
    /// this sandbox.
    fn backups(&self) -> PathBuf {
        self.home().join(".dml").join("backups")
    }

    /// Run `dml-wow <args>` sealed and return (exit code, every non-blank
    /// stdout line parsed as JSON). For a single-envelope command that is one
    /// element; for a streaming one it is the whole NDJSON event list.
    fn run_lines(&self, args: &[&str]) -> (i32, Vec<serde_json::Value>) {
        let out = self
            .cmd(args)
            .output()
            .unwrap_or_else(|e| panic!("spawn dml-wow {args:?}: {e}"));
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        let lines = stdout
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| {
                serde_json::from_str(l).unwrap_or_else(|e| panic!("not JSON ({e}): {l:?}"))
            })
            .collect();
        (out.status.code().unwrap_or(-1), lines)
    }
}

/// Assert one sealed invocation is refused by the `--yes` gate: exit 1,
/// EXACTLY ONE line on stdout (an envelope, never a half-opened NDJSON
/// stream), and the brief's CONFIRM_REQUIRED shape.
fn assert_confirm_required(s: &Sealed, args: &[&str], message: &str) {
    let (code, lines) = s.run_lines(args);
    assert_eq!(code, 1, "{args:?} must exit 1, got lines: {lines:?}");
    assert_eq!(
        lines.len(),
        1,
        "{args:?} must print ONE envelope and open no stream, got: {lines:?}"
    );
    let envelope = &lines[0];
    assert!(envelope["event"].is_null(), "{args:?}: an envelope, not a stream event: {envelope}");
    assert_eq!(envelope["ok"], false, "{args:?}: {envelope}");
    assert_eq!(envelope["error"]["code"], "CONFIRM_REQUIRED", "{args:?}: {envelope}");
    assert_eq!(envelope["error"]["message"], message, "{args:?}: {envelope}");
    assert_eq!(envelope["error"]["hint"], "", "{args:?}: {envelope}");
}

/// THE plan-required test #1: `backup restore` without `--yes` refuses, and
/// the backups directory is byte-identical afterwards — no pre-restore safety
/// dump was taken, nothing was renamed, and (the point) no container was ever
/// asked to stop, because the gate runs before the stream opens.
#[test]
fn backup_restore_without_yes_is_refused_and_touches_no_file() {
    let s = Sealed::new("restoreguard");
    fs::create_dir_all(s.backups()).unwrap();
    // A well-formed, PRESENT backup name: so the refusal cannot be mistaken for
    // the library's own `Invalid backup name`/`No backup named` checks, which
    // would both have been reached only by opening the stream.
    let file = "wow-20260101-000000.sql.gz";
    fs::write(s.backups().join(file), b"pretend gzip").unwrap();
    let before = s.home_snapshot();

    assert_confirm_required(
        &s,
        &["backup", "restore", file],
        "backup restore is destructive; re-run with --yes",
    );

    assert_eq!(s.home_snapshot(), before, "a refused restore must not change ANY file");
    assert!(s.backups().join(file).exists(), "the backup itself must still be there");
    // A restore that had started would have written this alongside it.
    let (prerestore, _) = ("wow-20260101-000000-prerestore.sql.gz", ());
    assert!(!s.backups().join(prerestore).exists(), "no pre-restore dump may be taken");
}

/// THE plan-required test #2: `bots-flush` needs BOTH halves, and neither the
/// arm marker nor the conf flag may be written on the way to saying so. (An
/// interrupted flush is exactly what `lifecycle::flush_heal_flag` exists to
/// clean up — so a refusal that armed the flag first would be a bot-wipe
/// hazard, not merely an untidy error path.)
#[test]
fn bots_flush_without_yes_or_ack_is_refused_and_arms_nothing() {
    let s = Sealed::new("flushguard");
    let before = s.home_snapshot();

    assert_confirm_required(
        &s,
        &["bots-flush"],
        "bots-flush is destructive; re-run with --yes",
    );
    assert_confirm_required(
        &s,
        &["bots-flush", "--ack", "flush"],
        "bots-flush is destructive; re-run with --yes",
    );
    // `--yes` alone is NOT enough, and the message says which half is missing.
    assert_confirm_required(
        &s,
        &["bots-flush", "--yes"],
        "bots-flush is destructive; re-run with --yes --ack flush",
    );
    assert_confirm_required(
        &s,
        &["bots-flush", "--yes", "--ack", "FLUSH"],
        "bots-flush is destructive; re-run with --yes --ack flush",
    );
    assert_confirm_required(
        &s,
        &["bots-flush", "--yes", "--ack", "yes"],
        "bots-flush is destructive; re-run with --yes --ack flush",
    );

    assert_eq!(s.home_snapshot(), before, "a refused flush must not change ANY file");
    // The arm marker lives next to the compose file, in the games dir.
    let marker = s.root.join("games").join("wow-server-playerbots").join(".dml-bot-flush-armed");
    assert!(!marker.exists(), "the delete-flag marker must never be written by a refusal");
}

#[test]
fn docker_clean_and_games_remove_without_yes_are_refused_and_change_nothing() {
    let s = Sealed::new("destructiveguards");
    // A title dir that LOOKS installed, so `games-remove`'s refusal cannot be
    // the library's own `NOT_FOUND` (which would mean the stream had opened).
    let title = s.root.join("games").join("wow-tbc-server");
    fs::create_dir_all(&title).unwrap();
    fs::write(title.join("docker-compose.yml"), "services: {}\n").unwrap();

    assert_confirm_required(
        &s,
        &["docker-clean", "--level", "1"],
        "docker-clean is destructive; re-run with --yes",
    );
    // The `--yes` gate precedes even the library's own `--level` 1..=3 check:
    // an out-of-range level with no `--yes` is still CONFIRM_REQUIRED, not
    // BAD_ARG. (This is the ONLY place an out-of-range level is exercised at
    // all — `docker-clean --yes` is never run in this suite.)
    assert_confirm_required(
        &s,
        &["docker-clean", "--level", "9"],
        "docker-clean is destructive; re-run with --yes",
    );
    assert_confirm_required(
        &s,
        &["games-remove", "wow-tbc-server"],
        "games-remove is destructive; re-run with --yes",
    );
    assert_confirm_required(
        &s,
        &["games-remove", "wow-tbc-server", "--keep-data", "--remove-images"],
        "games-remove is destructive; re-run with --yes",
    );
    // Even a title that isn't in the registry at all stops at the CONFIRM gate
    // rather than at `title_row`'s "Unknown title" — nothing is probed on the
    // way to asking for confirmation.
    assert_confirm_required(
        &s,
        &["games-remove", "definitely-not-a-title"],
        "games-remove is destructive; re-run with --yes",
    );

    assert!(title.join("docker-compose.yml").is_file(), "the title dir must be untouched");
}

/// The three lifecycle arms reproduce the launcher's `validate_game_id` guard,
/// and it fires before ANY docker call — including, for `start`, the Docker
/// Desktop engine-ensure step that would otherwise be the very first thing to
/// run. One envelope, no stream, exit 1.
#[test]
fn lifecycle_arms_reject_a_bad_id_before_touching_docker() {
    let s = Sealed::new("badid");
    for verb in ["start", "stop", "restart"] {
        let (code, lines) = s.run_lines(&[verb, "--id", "wow/../etc"]);
        assert_eq!(code, 1, "{verb}: {lines:?}");
        assert_eq!(lines.len(), 1, "{verb} must open no stream, got: {lines:?}");
        assert_eq!(lines[0]["error"]["code"], "BAD_ID", "{verb}: {}", lines[0]);
        assert_eq!(lines[0]["error"]["message"], "invalid game id: \"wow/../etc\"", "{verb}");
        assert_eq!(lines[0]["error"]["hint"], "Game ids come from games_list", "{verb}");
    }
    // An empty id is refused the same way (and never becomes a bare join).
    let (code, lines) = s.run_lines(&["restart", "--id", ""]);
    assert_eq!(code, 1);
    assert_eq!(lines[0]["error"]["code"], "BAD_ID", "{}", lines[0]);
}

/// Task 14 review, Fix 1 — THE REGRESSION PIN for a failing `stop` that must
/// not exit 0.
///
/// `stop` is the only arm that runs a SECOND stream (the Docker Desktop
/// engine-stop) after its orchestration finishes. If anything is written after
/// the terminal `done`/`error` event, a consumer that follows this CLI's own
/// documented contract (`out.rs`: a stream ends at its terminal event) closes
/// the pipe there — and the next write hits BrokenPipe, which
/// `print_stdout_line_or_exit` correctly answers with `exit(0)`. A FAILED stop
/// would then report success.
///
/// The pin is STRUCTURAL rather than a live pipe-close race (which the header
/// of this file explains is not deterministic): assert that the engine-stop
/// line is present (so the second stream really ran), that it comes BEFORE the
/// terminal event, and that the terminal event is the LAST line on stdout.
/// With that invariant there is no write left to break the pipe on, so the
/// exit code cannot be corrupted.
///
/// NO DOCKER IS INVOLVED. `DML_DOCKER` is an unconditional override
/// (`engine::resolve_docker_program`), so pointing it at a path that does not
/// exist means every docker spawn fails instantly at `Command::new` — the
/// engine-stop degrades to its `warn` line without a real `docker` process
/// ever being created, and the lifecycle stream never gets past its
/// title-not-found gate.
#[test]
fn a_failing_stop_ends_at_its_terminal_event_so_a_truncating_consumer_cannot_see_exit_0() {
    let s = Sealed::new("stopterminal");
    let no_docker = s.root.join("no-such-docker-binary.exe");
    let out = s
        .cmd(&["stop", "--id", "wow-server-playerbots"])
        .env("DML_DOCKER", &no_docker)
        .output()
        .expect("spawn dml-wow stop");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let events: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("not JSON ({e}): {l:?}")))
        .collect();

    // The stop itself FAILED (no such title in the sealed games dir) -- which
    // is the whole point: this is the case a truncating consumer must not see
    // as success.
    assert_eq!(out.status.code(), Some(1), "a failing stop must exit 1: {events:?}");
    let last = events.last().expect("at least one event");
    assert_eq!(last["event"], "error", "the LAST line must be the terminal event: {events:?}");
    assert_eq!(last["error"]["code"], "NOT_FOUND", "{events:?}");

    // The engine-stop stream really did run...
    let engine_idx = events
        .iter()
        .position(|e| {
            e["event"] == "line"
                && e["text"].as_str().is_some_and(|t| t.contains("Docker Desktop"))
        })
        .unwrap_or_else(|| panic!("the engine-stop stream did not run: {events:?}"));
    // ...and every one of its lines precedes the terminal event.
    assert!(
        engine_idx < events.len() - 1,
        "engine-stop lines must come BEFORE the terminal event, not after it: {events:?}"
    );
    assert!(
        events[engine_idx + 1..events.len() - 1]
            .iter()
            .all(|e| e["event"] == "line"),
        "nothing but engine `line` events may sit between the section end and the terminal event: {events:?}"
    );
}

/// The same arm with `--no-stop-engine`: no second stream at all, so the
/// terminal event is trivially last. Proves the flag is wired and that the
/// assertion above was observing a real engine-stop, not an artefact.
#[test]
fn stop_with_no_stop_engine_runs_no_second_stream() {
    let s = Sealed::new("stopnoengine");
    let no_docker = s.root.join("no-such-docker-binary.exe");
    let out = s
        .cmd(&["stop", "--id", "wow-server-playerbots", "--no-stop-engine"])
        .env("DML_DOCKER", &no_docker)
        .output()
        .expect("spawn dml-wow stop --no-stop-engine");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    assert!(
        !stdout.contains("Docker Desktop"),
        "--no-stop-engine must skip the engine stream entirely: {stdout}"
    );
    let last: serde_json::Value =
        serde_json::from_str(stdout.lines().rfind(|l| !l.trim().is_empty()).expect("a line"))
            .expect("valid JSON");
    assert_eq!(last["event"], "error");
    assert_eq!(last["error"]["code"], "NOT_FOUND");
}

/// `module repair`'s three closed-allowlist guards (controller ruling D1). The
/// `--db` one is the load-bearing case: `moduletail::module_repair` does
/// `database_for_short(&db).expect("validated above")`, so WITHOUT this guard
/// a bad `--db` is a PANIC (exit 101, no envelope at all) rather than a
/// BAD_ARG. Exit 1 + a well-formed envelope is what proves the guard is there.
#[test]
fn module_repair_rejects_bad_key_db_and_mode_before_any_sql() {
    let s = Sealed::new("repairguard");

    let (code, lines) = s.run_lines(&["module", "repair", "--key", "transmog", "--db", "world", "--mode", "mark"]);
    assert_eq!(code, 1, "{lines:?}");
    assert_eq!(lines[0]["error"]["code"], "BAD_ARG", "{}", lines[0]);
    assert_eq!(lines[0]["error"]["message"], "Invalid module key: transmog");

    for bad_db in ["acore_world", "world; DROP DATABASE acore_world", "World", ""] {
        let (code, lines) =
            s.run_lines(&["module", "repair", "--key", "mod-transmog", "--db", bad_db, "--mode", "mark"]);
        assert_eq!(code, 1, "--db {bad_db:?} must be a clean BAD_ARG, not a panic: {lines:?}");
        assert_eq!(lines.len(), 1, "--db {bad_db:?}: {lines:?}");
        assert_eq!(lines[0]["error"]["code"], "BAD_ARG", "--db {bad_db:?}: {}", lines[0]);
        assert_eq!(lines[0]["error"]["message"], format!("Invalid --db: {bad_db}"));
        assert_eq!(lines[0]["error"]["hint"], "Use world, characters, or auth.");
    }

    for bad_mode in ["delete", "Mark", ""] {
        let (code, lines) =
            s.run_lines(&["module", "repair", "--key", "mod-transmog", "--db", "world", "--mode", bad_mode]);
        assert_eq!(code, 1, "--mode {bad_mode:?}: {lines:?}");
        assert_eq!(lines[0]["error"]["code"], "BAD_ARG", "--mode {bad_mode:?}: {}", lines[0]);
        assert_eq!(lines[0]["error"]["message"], format!("Invalid --mode: {bad_mode}"));
        assert_eq!(lines[0]["error"]["hint"], "Use mark or clear.");
    }

    // ORDERING PROOF: with all three arguments valid, the guards pass and the
    // library is reached — where the sealed (empty) games dir stops it at
    // "server not installed", before any DB round trip.
    let (code, lines) =
        s.run_lines(&["module", "repair", "--key", "mod-transmog", "--db", "world", "--mode", "mark"]);
    assert_eq!(code, 1, "{lines:?}");
    assert_eq!(lines[0]["error"]["code"], "NOT_FOUND", "{}", lines[0]);
    assert_eq!(lines[0]["error"]["message"], "WoW Playerbots server not installed");
}

/// `module rebuild` with NEITHER `--backup` nor `--no-backup` is the
/// three-state flag pair's whole reason for existing: the library refuses it
/// ("Module SQL lands during the rebuild -- decide explicitly"), and it does so
/// as its FIRST statement, before the server dir is even resolved. Also the
/// smallest end-to-end proof that a module arm's streaming wiring is correct:
/// section_start -> section_end(error) -> error, exit 1.
#[test]
fn module_rebuild_without_a_backup_choice_streams_bad_arg_and_exits_1() {
    let s = Sealed::new("rebuildchoice");
    let (code, lines) = s.run_lines(&["module", "rebuild"]);
    assert_eq!(code, 1, "{lines:?}");
    assert_eq!(lines.first().map(|e| e["event"].clone()), Some(serde_json::json!("section_start")));
    assert_eq!(lines[0]["name"], "module-rebuild");
    let last = lines.last().expect("a terminal event");
    assert_eq!(last["event"], "error", "{lines:?}");
    assert_eq!(last["error"]["code"], "BAD_ARG");
    assert_eq!(last["error"]["message"], "Pick --backup or --no-backup");

    // ...and WITH an explicit choice the gate passes, so the next gate (server
    // installed?) is what answers. Same stream shape, different code — which is
    // what proves the assertion above was the backup gate and not a blanket
    // refusal of `module rebuild`.
    let (code, lines) = s.run_lines(&["module", "rebuild", "--no-backup"]);
    assert_eq!(code, 1, "{lines:?}");
    let last = lines.last().expect("a terminal event");
    assert_eq!(last["error"]["code"], "NOT_FOUND", "{lines:?}");
    assert_eq!(last["error"]["message"], "WoW Playerbots server not installed");
}

/// The other module mutators and `self-update` are wired and stream: with no
/// server in the sealed games dir every one of them stops at its first gate,
/// having run no git, no docker and no SQL.
#[test]
fn module_mutators_and_self_update_stream_their_first_gate() {
    let s = Sealed::new("modstreams");
    for args in [
        vec!["module", "install", "--family", "cpp", "--key", "mod-transmog"],
        vec!["module", "install", "--family", "perl"],
        vec!["module", "remove", "--family", "cpp", "--key", "mod-transmog"],
        vec!["module", "update", "--key", "mod-transmog"],
        vec!["self-update", "--no-backup"],
        vec!["self-update"],
    ] {
        let (code, lines) = s.run_lines(&args);
        assert_eq!(code, 1, "{args:?}: {lines:?}");
        assert_eq!(
            lines.first().map(|e| e["event"].clone()),
            Some(serde_json::json!("section_start")),
            "{args:?} must STREAM: {lines:?}"
        );
        let last = lines.last().expect("a terminal event");
        assert_eq!(last["event"], "error", "{args:?}: {lines:?}");
        assert_eq!(
            last["error"]["code"], "NOT_FOUND",
            "{args:?} must stop at the server-dir gate: {lines:?}"
        );
        assert_eq!(last["error"]["message"], "WoW Playerbots server not installed", "{args:?}");
    }
}

/// The read/write backup family, end to end inside the private home — the
/// non-destructive half of `backup`, and the proof that `require_backup_file`'s
/// two-step gate (name shape, then existence) is reproduced faithfully.
#[test]
fn backup_list_validate_and_delete_round_trip_in_a_private_home() {
    let s = Sealed::new("backuprt");

    // (1) No backups dir at all -> an empty list, not an error, and nothing
    // created just by asking.
    let (code, envelope) = s.run(&["backup", "list"]);
    assert_eq!(code, 0, "{envelope}");
    assert_eq!(envelope["data"]["backups"].as_array().expect("backups[]").len(), 0);
    assert!(!s.backups().exists(), "listing must not create the directory");

    // (2) A traversal name is BAD_ARG on BOTH gated arms, and the join never
    // happens — proven by the dir still not existing and nothing appearing
    // outside it.
    for verb in ["validate", "delete"] {
        for bad in ["../../evil.sql.gz", "wow-x.sql.gz", "not a backup"] {
            let (code, envelope) = s.run(&["backup", verb, bad]);
            assert_eq!(code, 1, "backup {verb} {bad:?}: {envelope}");
            assert_eq!(envelope["error"]["code"], "BAD_ARG", "backup {verb} {bad:?}: {envelope}");
            assert_eq!(envelope["error"]["message"], format!("Invalid backup name: {bad}"));
        }
    }
    assert!(!s.home().join("evil.sql.gz").exists());

    // (3) A well-formed but absent name is NOT_FOUND (the second gate).
    fs::create_dir_all(s.backups()).unwrap();
    let absent = "wow-20260101-000000.sql.gz";
    let (code, envelope) = s.run(&["backup", "validate", absent]);
    assert_eq!(code, 1, "{envelope}");
    assert_eq!(envelope["error"]["code"], "NOT_FOUND");
    assert_eq!(envelope["error"]["message"], format!("No backup named {absent}"));

    // (4) A real (if bogus) file: it lists, it validates (as INVALID — it is
    // not gzip), and deleting it removes exactly that file.
    let file = "wow-20260102-030405.sql.gz";
    fs::write(s.backups().join(file), b"not gzip at all").unwrap();
    let keep = "wow-20260103-030405-full.sql.gz";
    fs::write(s.backups().join(keep), b"also not gzip").unwrap();

    let (code, envelope) = s.run(&["backup", "list"]);
    assert_eq!(code, 0, "{envelope}");
    let listed: Vec<&str> = envelope["data"]["backups"]
        .as_array()
        .expect("backups[]")
        .iter()
        .map(|b| b["file"].as_str().expect("file"))
        .collect();
    assert_eq!(listed, vec![keep, file], "newest first");
    assert_eq!(envelope["data"]["backups"][0]["world"], true, "the -full name means world-inclusive");

    let (code, envelope) = s.run(&["backup", "validate", file]);
    assert_eq!(code, 0, "a validate RESULT is an ok envelope even when invalid: {envelope}");
    assert_eq!(envelope["data"]["valid"], false);
    assert_eq!(envelope["data"]["gzip_ok"], false);
    assert_eq!(envelope["data"]["file"], file);

    let (code, envelope) = s.run(&["backup", "delete", file]);
    assert_eq!(code, 0, "{envelope}");
    assert_eq!(envelope["data"]["deleted"], true);
    assert_eq!(envelope["data"]["file"], file);
    assert!(!s.backups().join(file).exists());
    assert!(s.backups().join(keep).exists(), "only the named file may go");
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

// ---------------------------------------------------------------------------
// Task 15: `lan` (Controller ruling D1), the remaining offline reads
// (`cache`, `client-path`, `accountwide`, `commands`), and the `install`
// preflight. Everything below reuses [`Sealed`] from the Task 13/14 blocks
// above -- a dead SOAP endpoint, a dead MySQL port, and a private
// HOME/USERPROFILE + games dir.
// ---------------------------------------------------------------------------

/// D1's own adversarial transcript: `dml_wow::lan::lan_action` carries an
/// `.expect("validated: ip required for on/refresh")` and an
/// `unreachable!()` that a caller can trigger directly if it skips
/// `validate_lan_request` -- so every one of these has to land as a clean
/// BAD_ARG envelope (exit 1), never a Rust panic (exit 101). None of these
/// reach a docker/DB probe: `validate_lan_request` runs entirely before
/// `lan_action` is ever called (see `run.rs::dispatch`'s `Lan` arm).
#[test]
fn lan_rejects_bad_input_before_any_docker_or_db_probe() {
    let s = Sealed::new("lan");

    // Unknown action.
    assert_bad_arg(&s, &["lan", "reset"], "invalid lan action: \"reset\"");
    // Missing IP on the two actions that require one.
    assert_bad_arg(&s, &["lan", "on"], "ip is required for the on/refresh actions");
    assert_bad_arg(&s, &["lan", "refresh"], "ip is required for the on/refresh actions");
    // Malformed IP (non-internet -- strict IPv4 shape).
    assert_bad_arg(&s, &["lan", "on", "not-an-ip"], "invalid IPv4 address: \"not-an-ip\"");
    assert_bad_arg(&s, &["lan", "refresh", "not-an-ip"], "invalid IPv4 address: \"not-an-ip\"");
    // Malformed address under --internet (hostname-shape check instead).
    assert_bad_arg(
        &s,
        &["lan", "on", "bad host;", "--internet"],
        "invalid address or hostname: \"bad host;\"",
    );

    // Every one of the above is BAD_ARG with the library's own hint, not a
    // hand-copied one.
    let (_, envelope) = s.run(&["lan", "reset"]);
    assert_eq!(envelope["error"]["hint"], "Check the value and try again.");

    // Sanity check on the seal itself (same doctrine as `console`'s own
    // companion check above): a WELL-FORMED call passes `validate_lan_request`
    // (no BAD_ARG) and reaches `lan_action`'s own first gate (server not
    // installed, since the sealed DML_GAMES_DIR is empty) -- reported as a
    // DIFFERENT error code (`LAN_ERROR`, review Fix 1 below) than the BAD_ARG
    // guard-rejections above. Without this, those BAD_ARG assertions could be
    // passing for the wrong reason (e.g. if the arm always answered BAD_ARG
    // regardless of input).
    let (code, envelope) = s.run(&["lan", "status"]);
    assert_eq!(code, 1, "{envelope}");
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["error"]["code"], "LAN_ERROR", "{envelope}");
    let message = envelope["error"]["message"].as_str().unwrap_or_default();
    assert!(message.contains("not installed"), "{envelope}");
}

/// Review Fix 1 (Important, contract violation): before this fix, EVERY
/// domain failure `lan_action` returns exited 0 with `ok:true` -- the exact
/// repro the reviewer supplied: a WELL-FORMED `lan on` with a public
/// (non-private) IP and no `--internet` is a DOMAIN failure INSIDE
/// `lan_action` itself ("not a private LAN address" -- this check runs
/// BEFORE `resolve_server_dir`, so it fires even against a fully
/// uninstalled/sealed environment, no live server required). It must now
/// exit 1 with an error envelope, matching the bash oracle's own
/// `echo "[dml] ERROR: ..."; exit 1` for the identical case
/// (`90-main.sh:901-905`) -- not the pre-fix `exit 0, ok:true`.
#[test]
fn lan_on_a_public_ip_is_a_domain_error_not_an_ok_envelope() {
    let s = Sealed::new("lan-public-ip");
    let (code, envelope) = s.run(&["lan", "on", "8.8.8.8"]);
    assert_eq!(code, 1, "{envelope}");
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["error"]["code"], "LAN_ERROR", "{envelope}");
    let message = envelope["error"]["message"].as_str().unwrap_or_default();
    assert!(message.starts_with("[dml] ERROR:"), "{envelope}");
    assert!(message.contains("is not a private LAN address"), "{envelope}");
    assert_eq!(envelope["error"]["hint"], "", "{envelope}");
}

/// Offline smoke: `cache status`/`cache clean` never touch docker/SOAP/DB,
/// so this exercises them end to end inside the private sealed home.
#[test]
fn cache_status_and_clean_round_trip_in_a_private_home() {
    let s = Sealed::new("cache");

    let (code, envelope) = s.run(&["cache", "status"]);
    assert_eq!(code, 0, "{envelope}");
    assert_eq!(envelope["data"]["caches"][0]["key"], "wowhead");
    assert_eq!(envelope["data"]["caches"][0]["present"], false);
    assert_eq!(envelope["data"]["caches"][0]["files"], 0);

    let cache_dir = s.home().join(".dml").join("wowhead-cache");
    fs::create_dir_all(&cache_dir).unwrap();
    fs::write(cache_dir.join("25.json"), b"{}").unwrap();

    let (code, envelope) = s.run(&["cache", "status"]);
    assert_eq!(code, 0, "{envelope}");
    assert_eq!(envelope["data"]["caches"][0]["present"], true);
    assert_eq!(envelope["data"]["caches"][0]["files"], 1);

    let (code, envelope) = s.run(&["cache", "clean"]);
    assert_eq!(code, 0, "{envelope}");
    assert_eq!(envelope["data"]["wiped"], true);
    assert!(!cache_dir.exists(), "cache clean must remove the directory");
}

/// Offline smoke: `client-path get/set/detect`, none of which touch
/// docker/SOAP/DB -- the guard set here is entirely `set_client_path`'s own
/// ("let the builder validate", same doctrine as Task 13's account
/// commands): a missing folder is BAD_PATH, a real-but-not-WoW folder is
/// NOT_CLIENT, before any write.
#[test]
fn client_path_get_set_and_detect_round_trip_in_a_private_home() {
    let s = Sealed::new("clientpath");

    let (code, envelope) = s.run(&["client-path", "get"]);
    assert_eq!(code, 0, "{envelope}");
    assert_eq!(envelope["data"]["path"], serde_json::Value::Null);
    assert_eq!(envelope["data"]["valid"], false);

    let missing = s.root.join("NoSuchFolder");
    let (code, envelope) = s.run(&["client-path", "set", &missing.display().to_string()]);
    assert_eq!(code, 1, "{envelope}");
    assert_eq!(envelope["error"]["code"], "BAD_PATH");

    let plain = s.root.join("PlainFolder");
    fs::create_dir_all(&plain).unwrap();
    let (code, envelope) = s.run(&["client-path", "set", &plain.display().to_string()]);
    assert_eq!(code, 1, "{envelope}");
    assert_eq!(envelope["error"]["code"], "NOT_CLIENT");

    let client = s.root.join("MyClient");
    fs::create_dir_all(client.join("Interface")).unwrap();
    let (code, envelope) = s.run(&["client-path", "set", &client.display().to_string()]);
    assert_eq!(code, 0, "{envelope}");
    assert_eq!(envelope["data"]["valid"], true);

    let (code, envelope) = s.run(&["client-path", "get"]);
    assert_eq!(code, 0, "{envelope}");
    assert_eq!(envelope["data"]["path"], client.display().to_string());
    assert_eq!(envelope["data"]["valid"], true);

    let scan_root = s.root.join("scan-root");
    fs::create_dir_all(scan_root.join("GoodClient").join("Interface")).unwrap();
    let out = s
        .cmd(&["client-path", "detect"])
        .env("DML_CLIENT_SCAN_ROOTS", &scan_root)
        .output()
        .expect("spawn dml-wow client-path detect");
    assert_eq!(out.status.code(), Some(0));
    let envelope = parse_envelope(&out.stdout);
    let candidates: Vec<&str> =
        envelope["data"]["candidates"].as_array().unwrap().iter().map(|c| c.as_str().unwrap()).collect();
    assert!(
        candidates.iter().any(|c| c.ends_with("GoodClient")),
        "expected GoodClient among {candidates:?}"
    );
}

/// The `accountwide set` guard set, in the launcher's own order
/// (`wow_accountwide_set_native`, lib.rs:2423-2457): `--value` shape, then
/// flag-name shape, BOTH before the server-dir resolve -- proven here by a
/// bad value/key reporting BAD_ARG even though NO server is installed at all
/// (if the resolve ran first, both would instead report NOT_FOUND).
#[test]
fn accountwide_set_guard_order_survives_an_uninstalled_server() {
    let s = Sealed::new("accountwide-guard");

    let (code, envelope) = s.run(&["accountwide", "get"]);
    assert_eq!(code, 1, "{envelope}");
    assert_eq!(envelope["error"]["code"], "NOT_FOUND");
    assert_eq!(envelope["error"]["message"], "WoW Playerbots server not installed");

    let (code, envelope) = s.run(&["accountwide", "set", "ENABLE_ACCOUNTWIDE_MOUNTS", "sideways"]);
    assert_eq!(code, 1, "{envelope}");
    assert_eq!(envelope["error"]["code"], "BAD_ARG", "{envelope}");
    assert_eq!(envelope["error"]["message"], "--value must be on or off");

    let (code, envelope) = s.run(&["accountwide", "set", "NOT_A_FLAG", "on"]);
    assert_eq!(code, 1, "{envelope}");
    assert_eq!(envelope["error"]["code"], "BAD_ARG", "{envelope}");
    assert_eq!(envelope["error"]["message"], "Invalid flag name: NOT_A_FLAG");

    // A well-formed value+key against the SAME uninstalled server now falls
    // through to NOT_FOUND -- proving the two BAD_ARG results above were
    // really the pre-checks firing, not a coincidence of a broken arm that
    // always says BAD_ARG.
    let (code, envelope) = s.run(&["accountwide", "set", "ENABLE_ACCOUNTWIDE_MOUNTS", "on"]);
    assert_eq!(code, 1, "{envelope}");
    assert_eq!(envelope["error"]["code"], "NOT_FOUND", "{envelope}");
}

/// Full offline round trip once a server dir (but not yet the accountwide
/// lua payload) exists: `get` reports `installed:false` (a DIFFERENT
/// question from "is the server installed" -- see `build_get`'s doc
/// comment), `set` reports the dedicated `NOT_INSTALLED`, and once the
/// payload is deployed, `set` actually flips the flag on disk.
#[test]
fn accountwide_full_round_trip_once_the_lua_payload_is_deployed() {
    let s = Sealed::new("accountwide-rt");
    let title = s.root.join("games").join("wow-server-playerbots");
    fs::create_dir_all(&title).unwrap();
    fs::write(title.join("docker-compose.yml"), "services: {}\n").unwrap();

    let (code, envelope) = s.run(&["accountwide", "get"]);
    assert_eq!(code, 0, "{envelope}");
    assert_eq!(envelope["data"]["installed"], false);

    let (code, envelope) = s.run(&["accountwide", "set", "ENABLE_ACCOUNTWIDE_MOUNTS", "on"]);
    assert_eq!(code, 1, "{envelope}");
    assert_eq!(envelope["error"]["code"], "NOT_INSTALLED");

    let aw_dir = title
        .join("env")
        .join("dist")
        .join("etc")
        .join("modules")
        .join("lua_scripts")
        .join("accountwide");
    fs::create_dir_all(&aw_dir).unwrap();
    fs::write(aw_dir.join("AccountMounts.lua"), "local ENABLE_ACCOUNTWIDE_MOUNTS = false\n").unwrap();

    let (code, envelope) = s.run(&["accountwide", "get"]);
    assert_eq!(code, 0, "{envelope}");
    assert_eq!(envelope["data"]["installed"], true);
    let mounts = envelope["data"]["subsystems"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["key"] == "ENABLE_ACCOUNTWIDE_MOUNTS")
        .expect("ENABLE_ACCOUNTWIDE_MOUNTS row");
    assert_eq!(mounts["value"], "off");

    let (code, envelope) = s.run(&["accountwide", "set", "ENABLE_ACCOUNTWIDE_MOUNTS", "on"]);
    assert_eq!(code, 0, "{envelope}");
    assert_eq!(envelope["data"]["changed"], true);
    assert_eq!(envelope["data"]["value"], "on");

    let content = fs::read_to_string(aw_dir.join("AccountMounts.lua")).unwrap();
    assert!(content.contains("local ENABLE_ACCOUNTWIDE_MOUNTS = true"), "{content:?}");
}

/// `commands`: NOT_FOUND before an uninstalled server's catalog/reader are
/// ever touched, then a clean empty list once a server dir exists but no
/// modules are deployed.
#[test]
fn commands_reports_not_found_uninstalled_then_an_empty_list_once_a_server_dir_exists() {
    let s = Sealed::new("commands");

    let (code, envelope) = s.run(&["commands"]);
    assert_eq!(code, 1, "{envelope}");
    assert_eq!(envelope["error"]["code"], "NOT_FOUND");
    assert_eq!(envelope["error"]["message"], "WoW Playerbots server not installed");

    let title = s.root.join("games").join("wow-server-playerbots");
    fs::create_dir_all(&title).unwrap();
    fs::write(title.join("docker-compose.yml"), "services: {}\n").unwrap();

    let (code, envelope) = s.run(&["commands"]);
    assert_eq!(code, 0, "{envelope}");
    assert_eq!(envelope["data"]["mods"].as_array().unwrap().len(), 0);
}

// ---------------------------------------------------------------------------
// Task 15: `install`'s preflight. Do NOT actually run an installer here --
// every case below is engineered so BOTH (or one) of bash/the dml script
// fail to resolve, which the code checks BEFORE any `Command::spawn` at
// all. A real installer run is left to the user's supervised smoke session.
// ---------------------------------------------------------------------------

/// The id guard (`run.rs::valid_game_id`, a CLI-specific addition -- see the
/// task report) runs BEFORE the prereqs check: a malformed id is BAD_ID even
/// when bash/the script are ALSO both unresolvable, proving the ordering
/// (if the prereqs check ran first, this would instead report
/// INSTALL_PREREQS).
#[test]
fn install_rejects_a_bad_id_before_checking_prereqs() {
    let out = bin()
        .args(["install", "wow server"]) // a space is invalid
        .env("DML_BASH", r"C:\definitely\does\not\exist\bash.exe")
        // An explicit NONEXISTENT ABSOLUTE path, not `.env_remove` (Task 15
        // review Minor 7): removing the var lets `find_dml_script` fall back
        // to the bare name "dml", and the assertion then rests on THAT not
        // existing relative to the test binary's cwd -- true under `cargo
        // test` today, but if this binary ever ran from `cli/` (where a file
        // literally named `dml` exists) this would flip from "prereqs
        // missing" to "script found" and the case below could spawn a REAL
        // installer with inherited stdio. An explicit bogus path can never
        // resolve regardless of cwd.
        .env("DML_SCRIPT", r"C:\definitely\does\not\exist\dml-script-that-is-not-real")
        .output()
        .expect("spawn dml-wow install");
    assert_eq!(out.status.code(), Some(1), "stdout: {}", String::from_utf8_lossy(&out.stdout));
    let envelope = parse_envelope(&out.stdout);
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["error"]["code"], "BAD_ID");
}

/// THE plan-required preflight test: neither bash nor the dml script resolve
/// -> a single INSTALL_PREREQS envelope, exit 1, and (since `DML_BASH` here
/// is a nonexistent path) nothing could possibly have been spawned.
#[test]
fn install_preflight_missing_bash_and_script_reports_install_prereqs_and_spawns_nothing() {
    let out = bin()
        .args(["install", "wow-server-playerbots"])
        .env("DML_BASH", r"C:\definitely\does\not\exist\bash.exe")
        // An explicit NONEXISTENT ABSOLUTE path, not `.env_remove` (Task 15
        // review Minor 7): removing the var lets `find_dml_script` fall back
        // to the bare name "dml", and the assertion then rests on THAT not
        // existing relative to the test binary's cwd -- true under `cargo
        // test` today, but if this binary ever ran from `cli/` (where a file
        // literally named `dml` exists) this would flip from "prereqs
        // missing" to "script found" and the case below could spawn a REAL
        // installer with inherited stdio. An explicit bogus path can never
        // resolve regardless of cwd.
        .env("DML_SCRIPT", r"C:\definitely\does\not\exist\dml-script-that-is-not-real")
        .output()
        .expect("spawn dml-wow install");
    assert_eq!(out.status.code(), Some(1), "stdout: {}", String::from_utf8_lossy(&out.stdout));
    let envelope = parse_envelope(&out.stdout);
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["error"]["code"], "INSTALL_PREREQS");
    assert_eq!(
        envelope["error"]["message"],
        "Git Bash (or bash on PATH) and the dml script (DML_SCRIPT) are required for install"
    );
    assert_eq!(envelope["error"]["hint"], "");
}

/// BOTH halves are required, not just one: a REAL, resolvable executable for
/// `DML_BASH` (a harmless stand-in -- never actually invoked here, just
/// `Path::is_file()`-checked by the preflight) with the script STILL missing
/// must still refuse — proving the check isn't a short-circuiting OR that
/// would let a present bash alone through.
///
/// The stand-in is picked per-platform (`cmd.exe` on Windows, `/bin/sh`
/// elsewhere) so the "bash resolves" half of the claim is actually true on
/// Linux CI too (Task 16) — a hardcoded Windows path would silently fail to
/// resolve on Linux as well, and the test would still pass, but vacuously:
/// it would be exercising "neither half resolves" instead of the "script
/// missing, bash present" case the name and doc comment promise.
#[cfg(windows)]
const RESOLVABLE_BASH_STANDIN: &str = r"C:\Windows\System32\cmd.exe";
#[cfg(not(windows))]
const RESOLVABLE_BASH_STANDIN: &str = "/bin/sh";

#[test]
fn install_preflight_bash_resolves_but_missing_script_still_refuses() {
    let out = bin()
        .args(["install", "wow-server-playerbots"])
        .env("DML_BASH", RESOLVABLE_BASH_STANDIN)
        // An explicit NONEXISTENT ABSOLUTE path, not `.env_remove` (Task 15
        // review Minor 7): removing the var lets `find_dml_script` fall back
        // to the bare name "dml", and the assertion then rests on THAT not
        // existing relative to the test binary's cwd -- true under `cargo
        // test` today, but if this binary ever ran from `cli/` (where a file
        // literally named `dml` exists) this would flip from "prereqs
        // missing" to "script found" and the case below could spawn a REAL
        // installer with inherited stdio. An explicit bogus path can never
        // resolve regardless of cwd.
        .env("DML_SCRIPT", r"C:\definitely\does\not\exist\dml-script-that-is-not-real")
        .output()
        .expect("spawn dml-wow install");
    assert_eq!(out.status.code(), Some(1), "stdout: {}", String::from_utf8_lossy(&out.stdout));
    let envelope = parse_envelope(&out.stdout);
    assert_eq!(envelope["error"]["code"], "INSTALL_PREREQS");
}

/// Review Minor 6: `INSTALL_SPAWN_FAILED` is deterministically reachable, no
/// race required — point `DML_BASH` at a REAL file that is NOT a valid
/// executable. It passes the preflight's only check (`Path::is_file()`), so
/// the code proceeds to `Command::spawn`, which `CreateProcess` itself then
/// refuses (Windows os error 193, "%1 is not a valid Win32 application").
/// WINDOWS-ONLY by construction (relies on `CreateProcess`'s own rejection of
/// a non-PE file) — see the task report for the cross-platform-coverage note.
#[test]
fn install_reports_install_spawn_failed_for_a_non_executable_bash() {
    let dir = std::env::temp_dir().join(format!("dml-cli-t15-nonexec-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let fake_bash = dir.join("not-a-real-exe.exe");
    fs::write(&fake_bash, b"not a PE file, just plain text").unwrap();
    // The script itself doesn't need to do anything real -- `CreateProcess`
    // rejects `fake_bash` before the script path is ever read.
    let script = dir.join("dml-script-need-not-exist-for-this-test.sh");
    fs::write(&script, b"#!/bin/sh\necho hi\n").unwrap();

    let out = bin()
        .args(["install", "wow-server-playerbots"])
        .env("DML_BASH", &fake_bash)
        .env("DML_SCRIPT", &script)
        .output()
        .expect("spawn dml-wow install");

    assert_eq!(out.status.code(), Some(1), "stdout: {}", String::from_utf8_lossy(&out.stdout));
    let envelope = parse_envelope(&out.stdout);
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["error"]["code"], "INSTALL_SPAWN_FAILED", "{envelope}");

    let _ = fs::remove_dir_all(&dir);
}
