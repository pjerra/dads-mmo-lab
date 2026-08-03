//! Task 10 — the migration import engine.
//!
//! Four things are proven here, and each one maps to a way the bash POC or an
//! earlier round of this project got it wrong on a real machine:
//!
//! 1. **Call order.** The whole import against a fake docker, asserting the
//!    sequence the stages actually made — not a restatement of a list.
//! 2. **The emptiness guard.** The restore must not be reachable without
//!    evidence the target is empty, and the fake REFUSES to answer the restore
//!    call unless that evidence was asked for first. This is the shape the
//!    Backups round's shipped Critical needed and did not have: a permissive
//!    stub hid credentials that were never passed, so the feature was dead on a
//!    real box and green in CI.
//! 3. **The CR strip** on `soap.env`, byte for byte.
//! 4. **The env merge**, plus the absence of any `build:` key in the generated
//!    output — loaded images must not be replaceable by a source build.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use dml_wow::composegen;
use dml_wow::install_native::{Call, InstallIo, Program, RunOutcome};
use dml_wow::migrate::MigrateIo;
use dml_wow::migrate::{self, Emptiness, MigrateOpts, Stage};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("dml-migrate-tests").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A COMPLETE export payload, the shape `export-from-wsl.sh` produces.
fn write_export(title_dir: &Path) {
    std::fs::create_dir_all(title_dir.join("etc")).unwrap();
    std::fs::write(title_dir.join("etc").join("worldserver.conf"), "LogLevel = 1\n").unwrap();
    std::fs::create_dir_all(title_dir.join("conf")).unwrap();
    std::fs::write(title_dir.join("db-dump.sql.gz"), b"not really gzip").unwrap();
    std::fs::write(title_dir.join("client-data.tar"), b"not really a tar").unwrap();
    for svc in migrate::IMAGE_SERVICES {
        std::fs::write(title_dir.join(format!("img-{svc}.tar.gz")), b"x").unwrap();
    }
    std::fs::write(title_dir.join(migrate::EXPORTED_OVERRIDE), EXPORTED_OVERRIDE_YAML).unwrap();
}

/// The real shape of the distro's override: the settings whose loss produces a
/// server that boots and is not the user's.
const EXPORTED_OVERRIDE_YAML: &str = r#"
services:
  ac-worldserver:
    environment:
      AC_SOAP_ENABLED: "1"
      AC_RATE_XP_KILL: "5"
      AC_PLAYERBOTS_RANDOM_BOT_MIN: "40"
      AC_PLAYERBOTS_RANDOM_BOT_MAX: "80"
      AC_LOGIN_DATABASE_INFO: "172.17.0.1;3306;root;password;acore_auth"
"#;

/// YAML with `#` comments removed.
///
/// The repo rule, and this suite proved it the hard way: the base template's
/// own header lists the three generated files, one line of which reads
/// "docker-compose.build.yml  the build:/target: keys". A raw scan for `build:`
/// therefore failed on completely correct output -- a red test on working code,
/// which is how a scan like this gets deleted instead of fixed.
///
/// Not a full YAML parse on purpose: `#` inside a quoted scalar would be
/// mangled, and there is none in this output. If that ever changes, the shape
/// assertion below is what fails, loudly.
fn uncommented(yaml: &str) -> String {
    yaml.lines()
        .map(|l| match l.find('#') {
            Some(i) => &l[..i],
            None => l,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn opts(games: &Path) -> MigrateOpts {
    MigrateOpts {
        id: migrate::DEFAULT_TITLE_ID.to_string(),
        games_dir: games.to_path_buf(),
        db_password: "password".to_string(),
        db_timeout: Duration::from_millis(200),
        db_poll: Duration::from_millis(10),
        ready_timeout: Duration::from_millis(200),
        ready_poll: Duration::from_millis(10),
        dml_home: Some(games.join("dml-home")),
    }
}

// ---------------------------------------------------------------------------
// The fake docker
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Reply {
    /// All of these substrings must appear in the joined argv.
    needles: Vec<String>,
    code: i32,
    out: String,
}

struct FakeIo {
    calls: RefCell<Vec<Vec<String>>>,
    replies: RefCell<Vec<Reply>>,
    /// Set once the engine asks the emptiness question. `restore_dump`
    /// consults it — see the [`MigrateIo`] impl.
    emptiness_asked: RefCell<bool>,
    /// Make the restore itself fail.
    restore_error: RefCell<Option<String>>,
}

impl FakeIo {
    fn new() -> Self {
        FakeIo {
            calls: RefCell::new(Vec::new()),
            replies: RefCell::new(Vec::new()),
            emptiness_asked: RefCell::new(false),
            restore_error: RefCell::new(None),
        }
    }

    /// Register a reply, REPLACING any entry with the same needles in place.
    ///
    /// Not a `push`. [`FakeIo::run`] resolves with a first-match scan, so
    /// appending would make every override a test registers after the shared
    /// builder unreachable — the exact bug `install_native`'s harness had, where
    /// seven tests silently asserted against the builder's answers and reported
    /// a working engine as broken.
    fn set(&self, needles: &[&str], code: i32, out: &str) {
        let r = Reply {
            needles: needles.iter().map(|s| s.to_string()).collect(),
            code,
            out: out.to_string(),
        };
        let mut replies = self.replies.borrow_mut();
        match replies.iter().position(|e| e.needles == r.needles) {
            Some(i) => replies[i] = r,
            None => replies.push(r),
        }
    }

    fn argvs(&self) -> Vec<String> {
        self.calls.borrow().iter().map(|a| a.join(" ")).collect()
    }

    /// Index of the first call containing every needle, or `None`.
    fn find(&self, needles: &[&str]) -> Option<usize> {
        self.argvs().iter().position(|a| needles.iter().all(|n| a.contains(n)))
    }

    fn made(&self, needles: &[&str]) -> bool {
        self.find(needles).is_some()
    }
}

/// The happy path: every call the engine makes answers 0, the database is
/// healthy, the world reports ready, and the target database is EMPTY.
fn happy() -> FakeIo {
    let io = FakeIo::new();
    io.set(&["ps", "-a", "--format"], 0, "");
    io.set(&["inspect", "Health"], 0, "healthy\n");
    io.set(&["compose", "ps", "-a", "-q", "ac-database"], 0, "db-container-id\n");
    io.set(&["compose", "ps", "-a", "-q", "ac-worldserver"], 0, "world-container-id\n");
    io.set(&["logs"], 0, "World initialised in 12 seconds.\nWorld Initialized In 12s\n");
    // Fresh target: information_schema reports no characters table.
    io.set(&[migrate::SQL_TABLE_PRESENT], 0, "0\n");
    io.set(&[migrate::SQL_CHARACTER_COUNT], 0, "2505\n");
    io.set(&[migrate::SQL_ACCOUNT_COUNT], 0, "255\n");
    io
}

impl InstallIo for FakeIo {
    fn preflight(&self, _games_dir: &Path) -> dml_wow::preflight::PreflightFacts {
        // Unused by this engine — it does its own payload preflight.
        dml_wow::preflight::PreflightFacts::default()
    }

    fn run(&self, call: &Call, on_line: &mut dyn FnMut(&str)) -> RunOutcome {
        assert_eq!(call.program, Program::Docker, "this engine must only ever run docker");
        let joined = call.args.join(" ");
        self.calls.borrow_mut().push(call.args.clone());

        if joined.contains(migrate::SQL_TABLE_PRESENT) {
            *self.emptiness_asked.borrow_mut() = true;
        }

        for r in self.replies.borrow().iter() {
            if r.needles.iter().all(|n| joined.contains(n.as_str())) {
                for l in r.out.lines() {
                    on_line(l);
                }
                return RunOutcome::Exited(r.code);
            }
        }
        RunOutcome::Exited(0)
    }
}

impl MigrateIo for FakeIo {
    /// THE GUARD, enforced by the double rather than asserted after the fact.
    ///
    /// A restore reached without the emptiness question having been asked is
    /// not "a test that fails later" -- it is a call this fake refuses to
    /// serve, so deleting the engine's guard cannot produce a green run by any
    /// route. That property is what the Backups round's shipped Critical
    /// lacked: a permissive stub accepted a restore carrying no credentials at
    /// all, so the feature was dead on a real box and green in CI.
    ///
    /// The other assertions are the same idea applied to the arguments. Before
    /// this trait existed the engine called `restore::stream_restore_into`
    /// directly and no double could see any of it.
    fn restore_dump(&self, container: &str, password: &str, gz: &Path) -> Result<(), String> {
        assert!(
            *self.emptiness_asked.borrow(),
            "the engine tried to restore into the database WITHOUT first asking whether it was empty"
        );
        assert!(!container.is_empty(), "the restore must name a resolved container");
        assert_ne!(
            container, "ac-database",
            "the restore must not fall back to the engine-global bare name"
        );
        assert!(!password.is_empty(), "the restore must carry credentials");
        assert!(gz.exists(), "the dump must really be there: {}", gz.display());
        self.calls.borrow_mut().push(vec!["RESTORE".into(), container.to_string()]);
        match self.restore_error.borrow().clone() {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}

/// Run an import, collecting the emitted events.
fn run_import(io: &FakeIo, o: &MigrateOpts) -> (i32, Vec<Value>) {
    let events = RefCell::new(Vec::new());
    let rc = migrate::migrate_import_stream_with(io, o, &|v| events.borrow_mut().push(v));
    (rc, events.into_inner())
}

fn terminal(events: &[Value]) -> &Value {
    events
        .iter()
        .rev()
        .find(|e| matches!(e.get("event").and_then(|v| v.as_str()), Some("done") | Some("error")))
        .expect("every run must end with a terminal event")
}

fn err_code(events: &[Value]) -> String {
    terminal(events).get("code").and_then(|v| v.as_str()).unwrap_or_default().to_string()
}

/// Stage names in the order their `section_end` reported ok.
fn completed_sections(events: &[Value]) -> Vec<String> {
    events
        .iter()
        .filter(|e| e.get("event").and_then(|v| v.as_str()) == Some("section_end"))
        .filter(|e| e.get("status").and_then(|v| v.as_str()) == Some("ok"))
        .filter_map(|e| e.get("name").and_then(|v| v.as_str()).map(str::to_string))
        .collect()
}

// ---------------------------------------------------------------------------
// 1. Call order
// ---------------------------------------------------------------------------

#[test]
fn a_clean_import_runs_every_stage_in_order() {
    let games = fixture("happy");
    let title = games.join(migrate::DEFAULT_TITLE_ID);
    std::fs::create_dir_all(&title).unwrap();
    write_export(&title);

    let io = happy();
    let (rc, events) = run_import(&io, &opts(&games));
    assert_eq!(rc, 0, "{:#?}", terminal(&events));

    assert_eq!(
        completed_sections(&events),
        migrate::STAGE_ORDER.iter().map(|s| s.name().to_string()).collect::<Vec<_>>(),
        "every stage must run, in STAGE_ORDER"
    );
}

#[test]
fn the_docker_calls_happen_in_the_order_the_stack_needs() {
    // Read off the calls the run ACTUALLY made, not off a list. Each pair here
    // is a real dependency: images before the stack that references them, the
    // volume before the database that lives in it, the database up before
    // anything asks it a question, and the full `up` only after the restore.
    let games = fixture("order");
    let title = games.join(migrate::DEFAULT_TITLE_ID);
    std::fs::create_dir_all(&title).unwrap();
    write_export(&title);

    let io = happy();
    let (rc, events) = run_import(&io, &opts(&games));
    assert_eq!(rc, 0, "{:#?}", terminal(&events));

    let load = io.find(&["load", "img-worldserver"]).expect("images are loaded");
    let tag = io.find(&["tag", "acore/ac-wotlk-worldserver"]).expect("images are retagged");
    let shell = io.find(&["compose", "up", "--no-start"]).expect("the shell is created");
    let volume = io.find(&["run", "--rm", "client-data.tar"]).expect("the volume is restored");
    let db_up = io.find(&["compose", "up", "-d", "ac-database"]).expect("the database starts");
    let ask = io.find(&[migrate::SQL_TABLE_PRESENT]).expect("emptiness is asked");
    // NB the joined argv is `compose -p <project> up -d`, so this cannot be a
    // substring match on "compose up -d" -- it reads the argv vector instead.
    let full_up = io
        .calls
        .borrow()
        .iter()
        .position(|a| {
            a.first().map(String::as_str) == Some("compose")
                && a.contains(&"up".to_string())
                && a.contains(&"-d".to_string())
                && !a.iter().any(|x| x == "ac-database")
        })
        .expect("the stack starts");

    assert!(load < tag, "retag must follow the load that produces the image");
    assert!(tag < shell, "the compose shell references the retagged images");
    assert!(shell < volume, "the volume only exists after `up --no-start`");
    assert!(volume < db_up, "client data is restored before the stack runs");
    assert!(db_up < ask, "nothing can be asked of a database that is not up");
    assert!(ask < full_up, "the world must not start before the database is restored");
}

#[test]
fn it_addresses_the_database_through_its_own_compose_project() {
    // Never a bare `ac-database`. A bare name answers for whichever project
    // owns it, and this engine's own guard exists because a second AzerothCore
    // stack may be on the machine. Restoring a dump into the wrong database is
    // not a recoverable mistake. (Same lesson as the worldserver log snapshot.)
    let games = fixture("resolve");
    let title = games.join(migrate::DEFAULT_TITLE_ID);
    std::fs::create_dir_all(&title).unwrap();
    write_export(&title);

    let io = happy();
    let (rc, _) = run_import(&io, &opts(&games));
    assert_eq!(rc, 0);

    assert!(
        io.made(&["compose", "ps", "-a", "-q", "ac-database"]),
        "the container must be resolved through the project"
    );
    for argv in io.argvs() {
        if argv.contains("mysql") {
            assert!(
                argv.contains("db-container-id"),
                "a mysql call addressed something other than the resolved container: {argv}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 2. The emptiness guard
// ---------------------------------------------------------------------------

/// A database it cannot resolve is a REFUSAL, never a fall back to the bare
/// name.
///
/// This test exists because a mutation proved it was missing: replacing the
/// error arm with `Ok("ac-database")` left the whole suite green, since the
/// happy fake always resolves successfully and nothing exercised the failure
/// path. The bare name answers for whichever compose project owns it, and
/// restoring a dump into the wrong database is not recoverable.
#[test]
fn an_unresolvable_database_is_refused_rather_than_guessed() {
    let games = fixture("unresolvable");
    let title = games.join(migrate::DEFAULT_TITLE_ID);
    std::fs::create_dir_all(&title).unwrap();
    write_export(&title);

    let io = happy();
    io.set(&["compose", "ps", "-a", "-q", "ac-database"], 0, "");

    let (rc, events) = run_import(&io, &opts(&games));
    assert_eq!(rc, 1);
    assert_eq!(err_code(&events), migrate::CODE_DB_UNHEALTHY);
    assert!(
        !io.made(&["RESTORE"]),
        "nothing may be restored into a database we could not identify"
    );
}

/// A restore that fails must not be recorded as done.
///
/// Otherwise the next run skips `db-restore` — the one stage whose skip is
/// driven by the state file rather than the disk — and boots a world on an
/// empty database, which looks like a brand-new server rather than a failure.
#[test]
fn a_failed_restore_is_not_recorded_as_finished() {
    let games = fixture("restore-fails");
    let title = games.join(migrate::DEFAULT_TITLE_ID);
    std::fs::create_dir_all(&title).unwrap();
    write_export(&title);

    let io = happy();
    *io.restore_error.borrow_mut() = Some("gzip stream ended early".into());

    let (rc, events) = run_import(&io, &opts(&games));
    assert_eq!(rc, 1);
    assert_eq!(err_code(&events), migrate::CODE_RESTORE_FAILED);

    let state = migrate::load_state(&title).expect("progress is still recorded");
    assert!(
        !state.is_done_named(Stage::DbRestore.name()),
        "a failed restore must not be recorded, or the resume skips it forever"
    );
    assert!(state.last_error.is_some(), "the state should carry why it stopped");
}

#[test]
fn it_refuses_a_target_that_already_holds_characters() {
    let games = fixture("occupied");
    let title = games.join(migrate::DEFAULT_TITLE_ID);
    std::fs::create_dir_all(&title).unwrap();
    write_export(&title);

    let io = happy();
    io.set(&[migrate::SQL_TABLE_PRESENT], 0, "1\n"); // the table is there...
    io.set(&[migrate::SQL_CHARACTER_COUNT], 0, "2505\n"); // ...with somebody's server in it

    let (rc, events) = run_import(&io, &opts(&games));
    assert_eq!(rc, 1);
    assert_eq!(err_code(&events), migrate::CODE_TARGET_NOT_EMPTY);
    assert!(
        !io.made(&["compose", "up", "-d"]) || !io.made(&["logs"]),
        "a refused import must not go on to boot the world"
    );
}

#[test]
fn a_database_that_cannot_answer_is_a_refusal_too() {
    // The tri-state rule, at the one place in this crate where getting it wrong
    // overwrites character data. "Could not tell" is NOT "empty".
    let games = fixture("unknown");
    let title = games.join(migrate::DEFAULT_TITLE_ID);
    std::fs::create_dir_all(&title).unwrap();
    write_export(&title);

    let io = happy();
    io.set(&[migrate::SQL_TABLE_PRESENT], 1, "ERROR 2002 (HY000): Can't connect\n");

    let (rc, events) = run_import(&io, &opts(&games));
    assert_eq!(rc, 1);
    assert_eq!(err_code(&events), migrate::CODE_TARGET_UNKNOWN);
}

#[test]
fn an_existing_but_empty_characters_table_is_still_importable() {
    // A stack that was started once has the schema and no rows. Refusing there
    // would block the ordinary case and teach the user to delete things.
    let games = fixture("schema-only");
    let title = games.join(migrate::DEFAULT_TITLE_ID);
    std::fs::create_dir_all(&title).unwrap();
    write_export(&title);

    let io = happy();
    io.set(&[migrate::SQL_TABLE_PRESENT], 0, "1\n");
    io.set(&[migrate::SQL_CHARACTER_COUNT], 0, "0\n");

    let (rc, events) = run_import(&io, &opts(&games));
    assert_eq!(rc, 0, "{:#?}", terminal(&events));
}

#[test]
fn the_emptiness_decision_reads_three_states_not_two() {
    // The pure core, so the ranking is pinned without a database.
    assert_eq!(migrate::classify_emptiness(Some("0"), || None), Emptiness::Empty);
    assert_eq!(
        migrate::classify_emptiness(Some("1"), || Some("2505".into())),
        Emptiness::Occupied(2505)
    );
    assert_eq!(migrate::classify_emptiness(Some("1"), || Some("0".into())), Emptiness::Empty);
    // Both unanswerable shapes, and a garbage answer, land in Unknown.
    assert!(matches!(migrate::classify_emptiness(None, || None), Emptiness::Unknown(_)));
    assert!(matches!(
        migrate::classify_emptiness(Some("1"), || None),
        Emptiness::Unknown(_)
    ));
    assert!(matches!(
        migrate::classify_emptiness(Some("nonsense"), || None),
        Emptiness::Unknown(_)
    ));
}

#[test]
fn a_resume_does_not_re_ask_a_question_it_already_answered() {
    // The one stage whose evidence is NOT the disk: a successful restore makes
    // the target look exactly like the thing the guard refuses. The recorded
    // state is the only thing that can tell "we put those rows there" from
    // "somebody else's server is here" — so a resume past db-restore must skip
    // it rather than refuse the import it completed a minute ago.
    let games = fixture("resume-restore");
    let title = games.join(migrate::DEFAULT_TITLE_ID);
    std::fs::create_dir_all(&title).unwrap();
    write_export(&title);

    // First run gets all the way through.
    let (rc, _) = run_import(&happy(), &opts(&games));
    assert_eq!(rc, 0);

    // Now the database is FULL — exactly what a re-run sees.
    let io = happy();
    io.set(&[migrate::SQL_TABLE_PRESENT], 0, "1\n");
    io.set(&[migrate::SQL_CHARACTER_COUNT], 0, "2505\n");

    let (rc, events) = run_import(&io, &opts(&games));
    assert_eq!(rc, 0, "a resume must not refuse the data it restored: {:#?}", terminal(&events));
    assert!(
        !io.made(&[migrate::SQL_TABLE_PRESENT]),
        "the recorded stage should make the question unnecessary, not merely survivable"
    );
}

#[test]
fn a_state_file_from_another_directory_is_not_trusted() {
    // The identity binding. Without it, copying a completed .dml-migrate.json
    // into a folder holding somebody's live server would skip the restore
    // guard entirely.
    let games = fixture("foreign-state");
    let title = games.join(migrate::DEFAULT_TITLE_ID);
    std::fs::create_dir_all(&title).unwrap();
    write_export(&title);

    let mut state = dml_wow::install_native::InstallState::new(
        migrate::DEFAULT_TITLE_ID,
        &composegen::install_id(Path::new("/somewhere/else/entirely")),
    );
    state.version = migrate::STATE_VERSION;
    state.mark_named(Stage::DbRestore.name());
    std::fs::write(
        migrate::state_path(&title),
        serde_json::to_string_pretty(&state).unwrap(),
    )
    .unwrap();

    assert!(migrate::load_state(&title).is_none(), "a foreign state file must be refused");

    let io = happy();
    io.set(&[migrate::SQL_TABLE_PRESENT], 0, "1\n");
    io.set(&[migrate::SQL_CHARACTER_COUNT], 0, "2505\n");
    let (rc, events) = run_import(&io, &opts(&games));
    assert_eq!(rc, 1, "the guard must still run");
    assert_eq!(err_code(&events), migrate::CODE_TARGET_NOT_EMPTY);
}

// ---------------------------------------------------------------------------
// 3. The CR strip
// ---------------------------------------------------------------------------

#[test]
fn soap_credentials_lose_their_carriage_returns() {
    // wsl.exe translates line endings on the way out of the distro, and native
    // `dml` sources this file from the WINDOWS home. A surviving \r rides
    // INSIDE the value: DML_SOAP_PASS=hunter2\r authenticates as hunter2\r and
    // every SOAP call fails with a bare SOAP_AUTH, which reads as a wrong
    // password rather than a line-ending bug.
    assert_eq!(migrate::strip_cr(b"A=1\r\nB=2\r\n"), b"A=1\nB=2\n".to_vec());
    assert_eq!(migrate::strip_cr(b"A=1\n"), b"A=1\n".to_vec());

    let games = fixture("soap");
    let title = games.join(migrate::DEFAULT_TITLE_ID);
    std::fs::create_dir_all(&title).unwrap();
    write_export(&title);
    std::fs::write(
        title.join(migrate::EXPORTED_SOAP_ENV),
        b"DML_SOAP_USER=dmlsoap\r\nDML_SOAP_PASS=hunter2\r\n",
    )
    .unwrap();

    let o = opts(&games);
    let (rc, _) = run_import(&happy(), &o);
    assert_eq!(rc, 0);

    let written = std::fs::read(o.dml_home.unwrap().join("soap.env")).unwrap();
    assert!(!written.contains(&b'\r'), "a CR survived into ~/.dml/soap.env");
    assert_eq!(written, b"DML_SOAP_USER=dmlsoap\nDML_SOAP_PASS=hunter2\n".to_vec());
}

#[test]
fn a_missing_soap_env_is_a_note_and_not_a_failure() {
    // The launcher creates its own GM account on the first status poll, so an
    // export without credentials is an inconvenience, never a dead import.
    let games = fixture("no-soap");
    let title = games.join(migrate::DEFAULT_TITLE_ID);
    std::fs::create_dir_all(&title).unwrap();
    write_export(&title);

    let (rc, events) = run_import(&happy(), &opts(&games));
    assert_eq!(rc, 0, "{:#?}", terminal(&events));
    assert!(
        events.iter().any(|e| e.get("text").and_then(|v| v.as_str()).is_some_and(|t| t.contains("soap.env"))),
        "the run should SAY the credentials were not carried across"
    );
}

// ---------------------------------------------------------------------------
// 4. The env merge, and no build overlay
// ---------------------------------------------------------------------------

#[test]
fn the_exported_settings_land_in_the_generated_override() {
    // The migration's recorded biggest lesson: dropping these boots a
    // 500-bot / 1x-rate / SOAP-off server that passes every check and is not
    // the server the user migrated (found live 2026-07-24).
    let games = fixture("merge");
    let title = games.join(migrate::DEFAULT_TITLE_ID);
    std::fs::create_dir_all(&title).unwrap();
    write_export(&title);

    let (rc, events) = run_import(&happy(), &opts(&games));
    assert_eq!(rc, 0, "{:#?}", terminal(&events));

    let text = std::fs::read_to_string(title.join(composegen::OVERRIDE_FILE)).unwrap();
    for key in ["AC_SOAP_ENABLED", "AC_RATE_XP_KILL", "AC_PLAYERBOTS_RANDOM_BOT_MIN", "AC_PLAYERBOTS_RANDOM_BOT_MAX"] {
        assert!(text.contains(key), "the exported {key} did not reach the override:\n{text}");
    }
    assert!(text.contains("\"80\"") || text.contains("80"), "the VALUE must travel too, not just the key");

    // ...but the source server's DB wiring must NOT: it points at the old
    // host's addresses, and the generated base file owns those keys.
    assert!(
        !text.contains("172.17.0.1"),
        "stale database wiring from the source host leaked into the override:\n{text}"
    );
}

#[test]
fn no_build_overlay_is_written_anywhere() {
    // The images were LOADED. A build overlay in this directory would let an
    // ordinary `docker compose build` or `up --build` replace the user's own
    // playerbots worldserver with a fresh source build — the same silent
    // substitution the dml.local retag exists to prevent, from the other side.
    let games = fixture("no-build");
    let title = games.join(migrate::DEFAULT_TITLE_ID);
    std::fs::create_dir_all(&title).unwrap();
    write_export(&title);

    let (rc, _) = run_import(&happy(), &opts(&games));
    assert_eq!(rc, 0);

    assert!(
        !title.join(composegen::BUILD_FILE).exists(),
        "a build overlay was written for a stack whose images cannot be rebuilt"
    );
    for f in [composegen::BASE_FILE, composegen::OVERRIDE_FILE] {
        let raw = std::fs::read_to_string(title.join(f)).unwrap();
        let text = uncommented(&raw);
        assert!(text.contains("services:"), "the comment stripper ate the document: {f}");
        for key in ["build:", "dockerfile:", "context:"] {
            assert!(!text.contains(key), "{f} carries a {key} key:\n{text}");
        }
    }
}

#[test]
fn the_generated_stack_runs_the_retagged_images() {
    // Both halves of the 2026-08-02 incident fix in one assertion: the compose
    // must name `dml.local/...:migrated` (a namespace no registry serves, so a
    // missing image is a loud refusal instead of a silent upstream
    // substitution), and the retag must actually produce that reference.
    let games = fixture("retag");
    let title = games.join(migrate::DEFAULT_TITLE_ID);
    std::fs::create_dir_all(&title).unwrap();
    write_export(&title);

    let io = happy();
    let (rc, _) = run_import(&io, &opts(&games));
    assert_eq!(rc, 0);

    let base = std::fs::read_to_string(title.join(composegen::BASE_FILE)).unwrap();
    assert!(base.contains(composegen::DEFAULT_IMAGE_PREFIX), "compose must use the local namespace:\n{base}");
    assert!(
        !base.contains(composegen::UPSTREAM_IMAGE_PREFIX),
        "compose still names upstream's namespace, which an ordinary pull can replace:\n{base}"
    );
    for svc in migrate::IMAGE_SERVICES {
        let want = migrate::image_ref(composegen::DEFAULT_IMAGE_PREFIX, svc, migrate::RUNNING_IMAGE_TAG);
        assert!(io.made(&["tag", &want]), "{svc} was never retagged to {want}");
    }
}

// ---------------------------------------------------------------------------
// Payload refusals
// ---------------------------------------------------------------------------

#[test]
fn an_incomplete_export_is_refused_before_anything_is_loaded() {
    // Discovering at the database stage that there was never a dump means
    // having already loaded four gigabytes of images.
    let games = fixture("incomplete");
    let title = games.join(migrate::DEFAULT_TITLE_ID);
    std::fs::create_dir_all(&title).unwrap();
    write_export(&title);
    std::fs::remove_file(title.join("db-dump.sql.gz")).unwrap();

    let io = happy();
    let (rc, events) = run_import(&io, &opts(&games));
    assert_eq!(rc, 1);
    assert_eq!(err_code(&events), migrate::CODE_INCOMPLETE_EXPORT);
    assert!(
        terminal(&events)["message"].as_str().unwrap().contains("db-dump.sql.gz"),
        "the refusal must NAME what is missing"
    );
    assert!(!io.made(&["load"]), "nothing should have been loaded");
}

#[test]
fn an_export_without_the_source_settings_is_refused_outright() {
    // Not a warning. This is the file whose absence produces a healthy-looking
    // server running module defaults.
    let games = fixture("no-override");
    let title = games.join(migrate::DEFAULT_TITLE_ID);
    std::fs::create_dir_all(&title).unwrap();
    write_export(&title);
    std::fs::remove_file(title.join(migrate::EXPORTED_OVERRIDE)).unwrap();

    let io = happy();
    let (rc, events) = run_import(&io, &opts(&games));
    assert_eq!(rc, 1);
    assert_eq!(err_code(&events), migrate::CODE_NO_OVERRIDE);
    assert!(!io.made(&["load"]));
}

#[test]
fn it_will_not_generate_over_a_compose_file_it_did_not_write() {
    // The folder the user points at may be a WORKING server. Same refusal, same
    // reason as install-native's.
    let games = fixture("existing-compose");
    let title = games.join(migrate::DEFAULT_TITLE_ID);
    std::fs::create_dir_all(&title).unwrap();
    write_export(&title);
    std::fs::write(title.join(composegen::BASE_FILE), "services: {}\n").unwrap();

    let io = happy();
    let (rc, events) = run_import(&io, &opts(&games));
    assert_eq!(rc, 1);
    assert_eq!(err_code(&events), migrate::CODE_COMPOSE_EXISTS);
    assert!(!io.made(&["load"]));
}

#[test]
fn a_foreign_stack_on_the_ac_names_is_refused() {
    let games = fixture("conflict");
    let title = games.join(migrate::DEFAULT_TITLE_ID);
    std::fs::create_dir_all(&title).unwrap();
    write_export(&title);

    let io = happy();
    io.set(
        &["ps", "-a", "--format"],
        0,
        "ac-worldserver\tsomeone-elses-project\tC:\\elsewhere\n",
    );

    let (rc, events) = run_import(&io, &opts(&games));
    assert_eq!(rc, 1);
    assert_eq!(err_code(&events), migrate::CODE_STACK_CONFLICT);
    assert!(!io.made(&["load"]), "the guard must precede the expensive work");
}

// ---------------------------------------------------------------------------
// Status + payload helpers
// ---------------------------------------------------------------------------

#[test]
fn status_names_what_is_missing_without_running_anything() {
    // The launcher checks a folder BEFORE offering to import it — an Import
    // button that only discovers the folder is wrong after it started is the
    // thing this avoids.
    let games = fixture("status");
    let title = games.join(migrate::DEFAULT_TITLE_ID);
    std::fs::create_dir_all(&title).unwrap();

    let st = migrate::status(&opts(&games));
    assert!(!st.export_present);
    assert!(st.missing.contains(&"db-dump.sql.gz".to_string()));
    assert!(st.missing.contains(&"etc/".to_string()));
    assert!(st.missing.iter().any(|m| m.contains("img-worldserver")));

    write_export(&title);
    let st = migrate::status(&opts(&games));
    assert!(st.export_present, "a complete export should be recognised: {:?}", st.missing);
    assert!(st.missing.is_empty());
}

#[test]
fn the_done_event_says_a_migration_is_a_snapshot() {
    // Snapshot semantics, said out loud: the source server keeps running and
    // keeps diverging, and nothing syncs back. A user who does not know that
    // plays on the old one for a week.
    let games = fixture("snapshot-note");
    let title = games.join(migrate::DEFAULT_TITLE_ID);
    std::fs::create_dir_all(&title).unwrap();
    write_export(&title);

    let (rc, events) = run_import(&happy(), &opts(&games));
    assert_eq!(rc, 0);
    let note = terminal(&events).get("note").and_then(|v| v.as_str()).unwrap_or_default();
    assert!(note.to_lowercase().contains("snapshot"), "the done event must state snapshot semantics");
}

#[test]
fn guards_are_never_recorded() {
    // A guard a resume skips is not a guard.
    for s in migrate::STAGE_ORDER {
        let recordable = s.records_completion();
        match s {
            Stage::Preflight | Stage::Guard => assert!(!recordable, "{} must not be recorded", s.name()),
            _ => assert!(recordable, "{} should be recorded", s.name()),
        }
    }
}

#[test]
fn stage_names_are_stable_tokens() {
    // They are what the state file stores, so renaming one silently discards a
    // real migration's recorded progress.
    let names: Vec<&str> = migrate::STAGE_ORDER.iter().map(|s| s.name()).collect();
    assert_eq!(
        names,
        vec![
            "preflight",
            "guard",
            "load-images",
            "generate-compose",
            "client-data",
            "db-restore",
            "settings",
            "up",
            "ready"
        ]
    );
    let mut sorted = names.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), names.len(), "stage names must be unique");
}

#[test]
fn compose_options_carry_the_exported_env_and_the_migrated_tag() {
    let mut exported = BTreeMap::new();
    exported.insert("AC_SOAP_ENABLED".to_string(), "1".to_string());
    exported.insert("AC_LOGIN_DATABASE_INFO".to_string(), "old-host;3306;root;pw;acore_auth".to_string());

    let o = MigrateOpts { db_password: "s3cret".into(), ..Default::default() };
    let c = migrate::compose_opts(&o, &exported).unwrap();

    assert_eq!(c.image_tag.as_deref(), Some(migrate::RUNNING_IMAGE_TAG));
    assert_eq!(c.image_prefix, composegen::DEFAULT_IMAGE_PREFIX);
    assert_eq!(c.db_password, "s3cret");
    assert!(c.extra_env.iter().any(|(k, _)| k == "AC_SOAP_ENABLED"));
    assert!(
        !c.extra_env.iter().any(|(k, _)| k == "AC_LOGIN_DATABASE_INFO"),
        "the base file owns the DB wiring; carrying the source host's copy would point the stack at a machine that is not here"
    );
}
