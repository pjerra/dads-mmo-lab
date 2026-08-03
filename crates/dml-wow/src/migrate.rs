//! Import a WSL export into a native Docker Desktop stack (Task 10).
//!
//! # What this replaces
//!
//! `poc/native-docker/migrate/import-to-desktop.sh` proved the path and is a
//! fine POC, but it asks the user to hand-author two compose files before it
//! will run — and then spends 200 lines validating what they wrote, because
//! every way of getting them wrong produces a server that *starts, looks
//! healthy, and is not theirs*. That validation is real knowledge and it is in
//! the wrong place: a check that can only say "you typed it wrong" is worse
//! than a generator that cannot type it wrong.
//!
//! So the compose trio is GENERATED here ([`composegen`]), with the exported
//! override's environment merged in. The recorded "biggest lesson" of the whole
//! migration effort is that dropping that environment boots a 500-bot / 1x-rate
//! / SOAP-off server that passes every check anyone thought to write, so the
//! merge is not a nicety — it is the reason this engine reads the export's
//! `conf/docker-compose.override.yml.orig` at all.
//!
//! # Shape
//!
//! Staged and resumable, the same machine as [`crate::install_native`] and
//! [`crate::unbound`]: state recorded ONLY after a stage really finished, bound
//! to its directory by [`composegen::install_id`] so a copied state file is
//! refused, and guards deliberately never recorded — a guard a resume skips is
//! not a guard.
//!
//! # The one MySQL write, and what makes it safe
//!
//! Restoring the dump is a write to `acore_*`, which puts this on the short
//! list of sanctioned character-data writes (CLAUDE.md, "THE MySQL WRITE
//! POLICY"). Its safety is NOT a consent prompt. Consent cannot tell you
//! whether the database you are about to write into has someone's characters in
//! it; this engine asks that question directly and REFUSES a non-empty target.
//! There is no `--replace` in v1, deliberately — "overwrite the server that is
//! already here" is [`crate::restore`]'s job, which has a safety dump, and
//! duplicating those semantics badly is worse than not offering them.
//!
//! The emptiness question is asked in three states, never two. A live server
//! with rows is a refusal; a database with no `acore_characters` schema at all
//! is genuinely fresh and proceeds; and a database that FAILED TO ANSWER is a
//! refusal too. That last one is the whole tri-state discipline in one place: a
//! probe that could not answer is evidence of nothing, and reading it as
//! "empty" would turn a wedged database into a licence to overwrite it.
//!
//! # Native-only, no bash mirror
//!
//! Same rationale as `install-native` and `unbound`: bash's own
//! `_installers_supported` refuses on Windows, and this engine exists precisely
//! for the machine that has no distro. The EXPORT half stays a Linux script run
//! inside the distro — it reads a server that only exists there.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use dml_core::error::CmdError;
use dml_core::events::{line_event, section_end, section_start, section_start_limited, pct_event};
use serde_json::{json, Value};

use crate::composegen;
use crate::install_native::{
    canon_path, conflicting_owner, games_dir_for_install, parse_stack_owners, stack_owner_argv,
    Call, InstallIo, InstallState, Program, RunOutcome, PROBE_TIMEOUT,
};

// ---------------------------------------------------------------------------
// Contract constants
// ---------------------------------------------------------------------------

/// Default title id — and it is not cosmetic. The FOLDER NAME is the id the
/// launcher and CLI look a server up by, and `export-from-wsl.sh` defaults to
/// this same name for exactly that reason (found live 2026-07-24: a differently
/// named folder makes `games list` and every `wow` feature miss the server).
pub const DEFAULT_TITLE_ID: &str = "wow-server-playerbots";

pub const STATE_FILE: &str = ".dml-migrate.json";
pub const STATE_VERSION: u32 = 1;

/// The four images `export-from-wsl.sh` saves, in its own order.
pub const IMAGE_SERVICES: [&str; 4] = ["worldserver", "authserver", "db-import", "client-data"];

/// The tag the tarballs LOAD as. Upstream's own, because that is the tag the
/// WSL server ran under and `docker save` records what it was given.
pub const LOADED_IMAGE_TAG: &str = "master";

/// The tag the imported stack RUNS as, under [`composegen::DEFAULT_IMAGE_PREFIX`].
///
/// The retag is the fix for a live incident (2026-08-02, the author's own
/// server). Loaded images carry `acore/ac-wotlk-<svc>:master`, so a
/// CUSTOM-BUILT server — playerbots compiled into the binary — sits on a MOVING
/// upstream tag. Weeks later an ordinary `docker compose up` pulled a fresher
/// `:master` and replaced all four with stock AzerothCore: the bots were gone
/// (the code was no longer in the binary), the worldserver demanded newer client
/// data than the volume held, and the client-data init container then failed
/// outright. Three symptoms, nothing in the output naming the cause.
///
/// `dml.local/` is served by no registry, so the failure mode inverts: a
/// missing image is a loud `pull access denied` instead of a silent
/// substitution.
pub const RUNNING_IMAGE_TAG: &str = "migrated";

/// Files `export-from-wsl.sh` always produces. Absent = incomplete export, and
/// saying so up front beats failing four stages in with half a stack built.
pub const REQUIRED_FILES: [&str; 2] = ["db-dump.sql.gz", "client-data.tar"];

/// Directories the export always produces.
pub const REQUIRED_DIRS: [&str; 1] = ["etc"];

/// The export's copy of the source server's real runtime settings. Its absence
/// is a REFUSAL rather than a warning — see [`CODE_NO_OVERRIDE`].
pub const EXPORTED_OVERRIDE: &str = "conf/docker-compose.override.yml.orig";

/// Optional, carried across when present.
pub const EXPORTED_SOAP_ENV: &str = "conf/soap.env";

pub const CODE_NO_EXPORT: &str = "MIGRATE_NO_EXPORT";
pub const CODE_INCOMPLETE_EXPORT: &str = "MIGRATE_INCOMPLETE_EXPORT";
pub const CODE_NO_OVERRIDE: &str = "MIGRATE_NO_OVERRIDE";
pub const CODE_COMPOSE_EXISTS: &str = "MIGRATE_COMPOSE_EXISTS";
pub const CODE_STACK_CONFLICT: &str = "MIGRATE_STACK_CONFLICT";
pub const CODE_ENGINE_DOWN: &str = "MIGRATE_ENGINE_DOWN";
pub const CODE_LOAD_FAILED: &str = "MIGRATE_LOAD_FAILED";
pub const CODE_RETAG_FAILED: &str = "MIGRATE_RETAG_FAILED";
pub const CODE_GENERATE_FAILED: &str = "MIGRATE_GENERATE_FAILED";
pub const CODE_VOLUME_FAILED: &str = "MIGRATE_VOLUME_FAILED";
pub const CODE_DB_UNHEALTHY: &str = "MIGRATE_DB_UNHEALTHY";
/// The refusal this engine exists to be able to make.
pub const CODE_TARGET_NOT_EMPTY: &str = "MIGRATE_TARGET_NOT_EMPTY";
/// Its tri-state sibling: we could not find out, so we do not write.
pub const CODE_TARGET_UNKNOWN: &str = "MIGRATE_TARGET_UNKNOWN";
pub const CODE_RESTORE_FAILED: &str = "MIGRATE_RESTORE_FAILED";
pub const CODE_UP_FAILED: &str = "MIGRATE_UP_FAILED";
pub const CODE_READY_TIMEOUT: &str = "MIGRATE_READY_TIMEOUT";

// ---------------------------------------------------------------------------
// Stages
// ---------------------------------------------------------------------------

/// Read by the driver loop, so pinning it in a test is not the
/// "ordering invariant on a list nobody reads" trap.
pub const STAGE_ORDER: [Stage; 9] = [
    Stage::Preflight,
    Stage::Guard,
    Stage::LoadImages,
    Stage::GenerateCompose,
    Stage::ClientData,
    Stage::DbRestore,
    Stage::Settings,
    Stage::Up,
    Stage::Ready,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Preflight,
    Guard,
    LoadImages,
    GenerateCompose,
    ClientData,
    DbRestore,
    Settings,
    Up,
    Ready,
}

impl Stage {
    /// The `section_start`/`section_end` name AND the token in the state file.
    /// A string, not an ordinal, so reordering the enum cannot re-interpret an
    /// existing migration's recorded progress.
    pub fn name(self) -> &'static str {
        match self {
            Stage::Preflight => "preflight",
            Stage::Guard => "guard",
            Stage::LoadImages => "load-images",
            Stage::GenerateCompose => "generate-compose",
            Stage::ClientData => "client-data",
            Stage::DbRestore => "db-restore",
            Stage::Settings => "settings",
            Stage::Up => "up",
            Stage::Ready => "ready",
        }
    }

    /// `preflight` and `guard` are GUARDS and are never recorded.
    pub fn records_completion(self) -> bool {
        !matches!(self, Stage::Preflight | Stage::Guard)
    }
}

/// Human-facing one-liner per stage, for a UI that wants more than the token.
pub fn stage_title(stage: Stage) -> &'static str {
    match stage {
        Stage::Preflight => "Checking the export",
        Stage::Guard => "Checking nothing else owns this stack",
        Stage::LoadImages => "Loading the server images",
        Stage::GenerateCompose => "Writing the compose files",
        Stage::ClientData => "Restoring the game data volume",
        Stage::DbRestore => "Restoring the databases",
        Stage::Settings => "Carrying settings across",
        Stage::Up => "Starting the stack",
        Stage::Ready => "Waiting for the world",
    }
}

// ---------------------------------------------------------------------------
// Options
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct MigrateOpts {
    /// Title id — also the folder name under the games dir.
    pub id: String,
    /// Where the games live. Resolved by the caller so the launcher and the CLI
    /// share one answer.
    pub games_dir: PathBuf,
    /// `DB_ROOT_PASSWORD` the dump was taken with and the stack will run with.
    pub db_password: String,
    /// How long to wait for `ac-database` to report healthy.
    pub db_timeout: Duration,
    pub db_poll: Duration,
    /// How long to wait for the world server to say it is up.
    pub ready_timeout: Duration,
    pub ready_poll: Duration,
    /// Where `~/.dml` is. Injected so tests never write to the real home.
    pub dml_home: Option<PathBuf>,
}

impl Default for MigrateOpts {
    fn default() -> Self {
        MigrateOpts {
            id: DEFAULT_TITLE_ID.to_string(),
            games_dir: PathBuf::new(),
            db_password: "password".to_string(),
            db_timeout: Duration::from_secs(300),
            db_poll: Duration::from_secs(5),
            // A migrated world boots from a restored database rather than an
            // import, so it is faster than a first native boot — but it still
            // loads the whole world, and a bounded wait that is too short
            // reports a failure about a server that was merely busy.
            ready_timeout: Duration::from_secs(1800),
            ready_poll: Duration::from_secs(5),
            dml_home: None,
        }
    }
}

impl MigrateOpts {
    pub fn title_dir(&self) -> PathBuf {
        self.games_dir.join(&self.id)
    }
}

/// `~/.dml`, or the injected override.
fn dml_home(opts: &MigrateOpts) -> Option<PathBuf> {
    opts.dml_home.clone().or_else(dml_core::util::dml_home_dir)
}

// ---------------------------------------------------------------------------
// IO seam
// ---------------------------------------------------------------------------

/// [`InstallIo`] plus the one operation this engine has that an install does
/// not: streaming a gzipped dump into the target database.
///
/// A SEPARATE, REQUIRED method rather than another `Call`, because the restore
/// is not a fire-and-collect subprocess -- it decompresses incrementally into a
/// child's stdin while draining that child's output on other threads, which is
/// what keeps a multi-gigabyte dump from deadlocking the pipe
/// ([`crate::restore`] documents the failure it avoids).
///
/// And required rather than defaulted ON PURPOSE. A default implementation that
/// performed the real restore would let a test double forget to override it and
/// then quietly write to whatever database the machine has running, while
/// reporting a clean run -- which is precisely what happened before this trait
/// existed: the first version of this engine called `stream_restore_into`
/// directly, the fake could not see it, and the suite went red against the
/// developer's live Docker. Every side effect in this engine goes through a
/// seam; this is the one that matters most.
pub trait MigrateIo: InstallIo {
    /// Restore `gz_path` into `container`. `Err` carries a human-readable
    /// reason; the engine turns it into [`CODE_RESTORE_FAILED`].
    fn restore_dump(&self, container: &str, password: &str, gz_path: &Path) -> Result<(), String>;
}

impl MigrateIo for crate::install_native::ProcIo {
    fn restore_dump(&self, container: &str, password: &str, gz_path: &Path) -> Result<(), String> {
        let program: OsString = dml_core::engine::docker_program();
        match crate::restore::stream_restore_into(&program, container, password, gz_path) {
            Err(e) => Err(e),
            Ok(r) if !r.success() => {
                let tail = String::from_utf8_lossy(&r.stderr).trim().to_string();
                Err(if tail.is_empty() { "the import exited nonzero".to_string() } else { tail })
            }
            Ok(_) => Ok(()),
        }
    }
}

// ---------------------------------------------------------------------------
// Failure
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
struct Fail {
    code: &'static str,
    message: String,
    hint: String,
}

impl Fail {
    fn new(code: &'static str, message: impl Into<String>, hint: impl Into<String>) -> Self {
        Fail { code, message: message.into(), hint: hint.into() }
    }
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

pub fn state_path(title_dir: &Path) -> PathBuf {
    title_dir.join(STATE_FILE)
}

/// Read the state file, or `None` for absent / unreadable / wrong-version /
/// WRONG-DIRECTORY. Each of those means "no trustworthy progress".
pub fn load_state(title_dir: &Path) -> Option<InstallState> {
    let text = std::fs::read_to_string(state_path(title_dir)).ok()?;
    let state: InstallState = serde_json::from_str(&text).ok()?;
    if state.version != STATE_VERSION {
        return None;
    }
    if state.install_id != composegen::install_id(title_dir) {
        return None;
    }
    Some(state)
}

pub fn save_state(title_dir: &Path, state: &InstallState) -> Result<(), CmdError> {
    std::fs::create_dir_all(title_dir).map_err(|e| CmdError {
        code: "WRITE_FAILED".to_string(),
        message: format!("Could not create {}: {e}", title_dir.display()),
        hint: String::new(),
    })?;
    let mut state = state.clone();
    state.updated_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let text = serde_json::to_string_pretty(&state).unwrap_or_default();
    std::fs::write(state_path(title_dir), text).map_err(|e| CmdError {
        code: "WRITE_FAILED".to_string(),
        message: format!("Could not write {}: {e}", state_path(title_dir).display()),
        hint: String::new(),
    })
}

/// The first recordable stage a resume must do again, or `None` when the state
/// claims all of them.
pub fn next_stage(state: &InstallState) -> Option<Stage> {
    STAGE_ORDER.into_iter().find(|s| s.records_completion() && !state.is_done_named(s.name()))
}

// ---------------------------------------------------------------------------
// Export payload
// ---------------------------------------------------------------------------

/// What is missing from an export directory, in the order a human would fix it.
///
/// Pure so the launcher can check a directory the user picked BEFORE offering
/// to import it — an "Import" button that only discovers the folder is wrong
/// after it has started is the thing this avoids.
pub fn missing_payload(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for d in REQUIRED_DIRS {
        if !dir.join(d).is_dir() {
            out.push(format!("{d}/"));
        }
    }
    for f in REQUIRED_FILES {
        if !dir.join(f).is_file() {
            out.push(f.to_string());
        }
    }
    for svc in IMAGE_SERVICES {
        let f = format!("img-{svc}.tar.gz");
        if !dir.join(&f).is_file() {
            out.push(f);
        }
    }
    out
}

/// Is this directory a complete export?
pub fn is_export_dir(dir: &Path) -> bool {
    dir.is_dir() && missing_payload(dir).is_empty()
}

/// `image_ref(prefix, svc, tag)` — one place so the load, the retag and the
/// generated compose cannot drift apart.
pub fn image_ref(prefix: &str, svc: &str, tag: &str) -> String {
    format!("{prefix}{svc}:{tag}")
}

/// The compose options for an imported stack.
///
/// Two deliberate differences from a fresh install, and both are consequences
/// of the images being LOADED rather than built:
///
/// * `image_tag` is [`RUNNING_IMAGE_TAG`] under the normal `dml.local/` prefix,
///   matching what the retag step produces.
/// * no build overlay is written at all (see [`Engine::do_generate`]).
pub fn compose_opts(opts: &MigrateOpts, exported_env: &BTreeMap<String, String>) -> Result<composegen::ComposeOpts, CmdError> {
    let base = composegen::ComposeOpts {
        image_tag: Some(RUNNING_IMAGE_TAG.to_string()),
        db_password: opts.db_password.clone(),
        ..Default::default()
    };
    composegen::merge_exported_override(&base, exported_env)
}

/// Parse the exported override's `ac-worldserver` environment block.
///
/// Missing file is `None` and the caller refuses — see [`CODE_NO_OVERRIDE`].
/// An UNPARSEABLE file is an empty map rather than an error, which
/// [`crate::config::parse_override_env`] decides and this function inherits;
/// the caller treats an empty map as a refusal too, so the distinction never
/// reaches a user as a silently-defaulted server.
pub fn read_exported_env(dir: &Path) -> Option<BTreeMap<String, String>> {
    let text = std::fs::read_to_string(dir.join(EXPORTED_OVERRIDE)).ok()?;
    Some(crate::config::parse_override_env(&text).into_iter().collect())
}

/// Strip CRs from `soap.env` bytes.
///
/// `export-from-wsl.sh` runs inside the distro and its output reaches Windows
/// through `wsl.exe`, which translates line endings. Native `dml` sources this
/// file from the WINDOWS home, so a surviving `\r` rides along INSIDE the value
/// — `DML_SOAP_PASS=hunter2\r` authenticates as `hunter2\r` and every SOAP call
/// fails with a bare `SOAP_AUTH`, which reads as a wrong password.
pub fn strip_cr(bytes: &[u8]) -> Vec<u8> {
    bytes.iter().copied().filter(|b| *b != b'\r').collect()
}

// ---------------------------------------------------------------------------
// The emptiness question
// ---------------------------------------------------------------------------

/// `docker exec <container> mysql …` argv for a single scalar question.
pub fn mysql_scalar_argv(container: &str, password: &str, sql: &str) -> Vec<String> {
    vec![
        "exec".into(),
        container.into(),
        "mysql".into(),
        "-uroot".into(),
        format!("-p{password}"),
        "-N".into(),
        "-B".into(),
        "-e".into(),
        sql.into(),
    ]
}

/// Does the target hold a `acore_characters.characters` TABLE at all?
///
/// Asked through `information_schema` on purpose. The obvious
/// `SELECT COUNT(*) FROM acore_characters.characters` ERRORS on a genuinely
/// fresh database — which is the case we most need to say "yes, proceed" to —
/// and that error is indistinguishable from "the server did not answer". A
/// question that returns 0 instead of failing is the only one whose failure
/// means what a failure should mean.
pub const SQL_TABLE_PRESENT: &str = "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema='acore_characters' AND table_name='characters'";

/// How many characters are on the target. Only asked when the table exists.
pub const SQL_CHARACTER_COUNT: &str = "SELECT COUNT(*) FROM acore_characters.characters";

pub const SQL_ACCOUNT_COUNT: &str = "SELECT COUNT(*) FROM acore_auth.account";

/// Three answers, never two.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Emptiness {
    /// No `acore_characters.characters` table: a fresh database. Safe to write.
    Empty,
    /// The table exists and holds this many rows. Refuse.
    Occupied(u64),
    /// The database did not answer. Refuse — a probe that could not answer is
    /// evidence of NOTHING, and reading it as "empty" turns a wedged database
    /// into a licence to overwrite it.
    Unknown(String),
}

/// Decide emptiness from the two raw query answers.
///
/// Pure, so the decision can be tested without a database. `present` is the
/// output of [`SQL_TABLE_PRESENT`]; `count` is [`SQL_CHARACTER_COUNT`]'s, and
/// is only consulted when `present` parsed to a non-zero number.
pub fn classify_emptiness(present: Option<&str>, count: impl FnOnce() -> Option<String>) -> Emptiness {
    let Some(present) = present else {
        return Emptiness::Unknown("the database did not answer the schema question".into());
    };
    let Ok(n) = present.trim().parse::<u64>() else {
        return Emptiness::Unknown(format!("unexpected answer to the schema question: {present:?}"));
    };
    if n == 0 {
        return Emptiness::Empty;
    }
    match count() {
        None => Emptiness::Unknown("the database did not answer the character count".into()),
        Some(raw) => match raw.trim().parse::<u64>() {
            Err(_) => Emptiness::Unknown(format!("unexpected answer to the character count: {raw:?}")),
            // A table that exists but is EMPTY is still a fresh-enough target:
            // a `db-import` run creates the schema before anyone has played.
            // Refusing there would block the ordinary case of importing into a
            // stack that had been started once.
            Ok(0) => Emptiness::Empty,
            Ok(rows) => Emptiness::Occupied(rows),
        },
    }
}

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------

struct Engine<'a> {
    io: &'a dyn MigrateIo,
    opts: &'a MigrateOpts,
    emit: &'a dyn Fn(Value),
    title_dir: PathBuf,
    project: String,
    state: InstallState,
    resumed: bool,
    /// Resolved once in `guard` and reused: the worldserver/database container
    /// ids from THIS stack's compose project. Empty until `up` has run.
    db_container: Option<String>,
}

impl Engine<'_> {
    fn line(&self, level: &str, text: impl Into<String>) {
        (self.emit)(line_event(level, text));
    }

    fn persist(&self) {
        // Best-effort: a state file we could not write costs a resume its
        // shortcut, and is never a reason to fail an import that is working.
        if let Err(e) = save_state(&self.title_dir, &self.state) {
            self.line("warn", format!("could not record progress: {}", e.message));
        }
    }

    fn docker(&self, args: Vec<String>, timeout: Option<Duration>) -> Call {
        Call { program: Program::Docker, args, cwd: Some(self.title_dir.clone()), timeout }
    }

    /// A docker PROBE: bounded, cwd-free, output collected.
    fn probe(&self, args: Vec<String>) -> Call {
        Call { program: Program::Docker, args, cwd: None, timeout: Some(PROBE_TIMEOUT) }
    }

    fn run_collect(&self, call: &Call) -> (RunOutcome, String) {
        let mut buf = String::new();
        let outcome = self.io.run(call, &mut |l| {
            buf.push_str(l);
            buf.push('\n');
        });
        (outcome, buf)
    }

    /// Run and echo every line into the terminal stream.
    fn run_echo(&self, call: &Call) -> RunOutcome {
        self.io.run(call, &mut |l| (self.emit)(line_event("info", l)))
    }

    fn compose(&self, mut args: Vec<String>, timeout: Option<Duration>) -> Call {
        let mut v = vec!["compose".to_string(), "-p".to_string(), self.project.clone()];
        v.append(&mut args);
        self.docker(v, timeout)
    }

    // -- stages ------------------------------------------------------------

    /// Is the export here, and is it complete?
    ///
    /// Deliberately NOT recorded, and deliberately first: every later stage
    /// reads these files, and discovering at the database stage that there was
    /// never a dump means having already loaded 4 GB of images.
    fn do_preflight(&mut self) -> Result<(), Fail> {
        if !self.title_dir.is_dir() {
            return Err(Fail::new(
                CODE_NO_EXPORT,
                format!("No export found at {}.", self.title_dir.display()),
                "Run poc/native-docker/migrate/export-from-wsl.sh inside the distro first — it writes the export into this folder.",
            ));
        }
        let missing = missing_payload(&self.title_dir);
        if !missing.is_empty() {
            return Err(Fail::new(
                CODE_INCOMPLETE_EXPORT,
                format!(
                    "The export at {} is missing: {}.",
                    self.title_dir.display(),
                    missing.join(", ")
                ),
                "Re-run the export inside the distro. A partial export produces a server that boots and is not yours.",
            ));
        }

        // The override is a REFUSAL, not a warning, and this is the migration's
        // recorded biggest lesson: without the source server's real environment
        // the generated stack runs module DEFAULTS — 500 bots, 1x rates, SOAP
        // off — and looks completely healthy while being somebody else's
        // server. Found live 2026-07-24.
        let env = read_exported_env(&self.title_dir).filter(|m| !m.is_empty());
        let Some(env) = env else {
            return Err(Fail::new(
                CODE_NO_OVERRIDE,
                format!(
                    "The export carries no usable {EXPORTED_OVERRIDE} — that file IS the source server's real settings (SOAP on, its bot counts, its rates)."
                ),
                "Re-run the export. Importing without it would build a defaults server that looks fine and is not the one you migrated.",
            ));
        };
        self.line(
            "info",
            format!("export looks complete; carrying {} settings across", env.len()),
        );

        // Never generate over a compose file DML did not write. Same refusal as
        // install-native, and for the same reason: the folder the user points
        // at may be a WORKING server.
        let base = self.title_dir.join(composegen::BASE_FILE);
        if base.is_file() && !self.resumed {
            return Err(Fail::new(
                CODE_COMPOSE_EXISTS,
                format!("{} already has a {} that this import did not write.", self.title_dir.display(), composegen::BASE_FILE),
                "If this folder is already a working server, import somewhere else. If it is a half-finished import, keep its .dml-migrate.json so the resume can recognise it.",
            ));
        }
        Ok(())
    }

    /// Does anything else own the `ac-*` container names?
    ///
    /// The names are global to the docker ENGINE, so one AzerothCore stack per
    /// PC. "Ours" holds on EITHER signal — the derived project name matches, or
    /// the compose working-dir label names the directory we are about to
    /// compose from — because a migrated server can legitimately run under a
    /// project name this generator would never produce (that exact case refused
    /// a user's own server on 2026-08-02).
    fn do_guard(&mut self) -> Result<(), Fail> {
        let (outcome, out) = self.run_collect(&self.probe(stack_owner_argv()));
        match outcome {
            RunOutcome::CouldNotTell(why) => {
                return Err(Fail::new(
                    CODE_ENGINE_DOWN,
                    format!("Could not ask Docker what is running: {why}"),
                    "Start Docker Desktop and try again.",
                ));
            }
            RunOutcome::Exited(code) if code != 0 => {
                return Err(Fail::new(
                    CODE_ENGINE_DOWN,
                    format!("Docker refused to list containers (exit {code})."),
                    "Start Docker Desktop and try again.",
                ));
            }
            RunOutcome::Exited(_) => {}
        }
        let owners = parse_stack_owners(&out);
        if let Some((name, project)) = conflicting_owner(&owners, &self.project, &self.title_dir) {
            return Err(Fail::new(
                CODE_STACK_CONFLICT,
                format!("The container {name} already belongs to the compose project {project}."),
                "Those ac-* names are global to the Docker engine, so only one AzerothCore stack can exist at a time. Stop that stack first.",
            ));
        }
        Ok(())
    }

    /// `docker load` the four tarballs, then retag them out of upstream's
    /// namespace.
    fn do_load_images(&mut self) -> Result<(), Fail> {
        let total = IMAGE_SERVICES.len() as u32;
        for (i, svc) in IMAGE_SERVICES.into_iter().enumerate() {
            let tar = self.title_dir.join(format!("img-{svc}.tar.gz"));
            self.line("info", format!("loading {}", tar.display()));
            // `docker load -i` reads the gzip itself, so nothing has to shuttle
            // gigabytes through this process.
            let call = self.docker(
                vec!["load".into(), "-i".into(), tar.display().to_string()],
                None,
            );
            match self.run_echo(&call) {
                RunOutcome::Exited(0) => {}
                RunOutcome::Exited(code) => {
                    return Err(Fail::new(
                        CODE_LOAD_FAILED,
                        format!("Loading img-{svc}.tar.gz failed (exit {code})."),
                        "A truncated tarball is the usual cause — re-run the export.",
                    ))
                }
                RunOutcome::CouldNotTell(why) => {
                    return Err(Fail::new(CODE_LOAD_FAILED, format!("Could not run docker load: {why}"), ""))
                }
            }
            (self.emit)(pct_event((((i as u32 + 1) * 100) / total).min(100) as u8));
        }

        self.line(
            "info",
            "pinning the loaded images so no upstream pull can replace them",
        );
        for svc in IMAGE_SERVICES {
            let from = image_ref(composegen::UPSTREAM_IMAGE_PREFIX, svc, LOADED_IMAGE_TAG);
            let to = image_ref(composegen::DEFAULT_IMAGE_PREFIX, svc, RUNNING_IMAGE_TAG);
            let call = self.probe(vec!["tag".into(), from.clone(), to.clone()]);
            match self.run_echo(&call) {
                RunOutcome::Exited(0) => self.line("info", format!("{from} -> {to}")),
                RunOutcome::Exited(code) => {
                    return Err(Fail::new(
                        CODE_RETAG_FAILED,
                        format!("Could not retag {from} (exit {code})."),
                        "The load step should have produced that image — re-run the import.",
                    ))
                }
                RunOutcome::CouldNotTell(why) => {
                    return Err(Fail::new(CODE_RETAG_FAILED, format!("Could not run docker tag: {why}"), ""))
                }
            }
        }
        Ok(())
    }

    /// Stage the exported config tree and write the compose files.
    ///
    /// NO BUILD OVERLAY. The imported images were loaded, never built, and a
    /// `docker-compose.build.yml` in this directory would let an ordinary
    /// `docker compose build` (or an `up --build`) overwrite the user's own
    /// worldserver with a fresh source build — the same class of silent
    /// substitution the retag exists to prevent, from the other direction.
    fn do_generate(&mut self) -> Result<(), Fail> {
        // The compose binds ./env/dist/etc; without this the worldserver reads
        // the IMAGE's built-in .conf.dist defaults, the exported config is
        // never used, and every setting the launcher saves appears to save and
        // does nothing.
        let etc_dst = self.title_dir.join("env").join("dist").join("etc");
        let logs_dst = self.title_dir.join("env").join("dist").join("logs");
        for d in [&etc_dst, &logs_dst] {
            std::fs::create_dir_all(d).map_err(|e| {
                Fail::new(CODE_GENERATE_FAILED, format!("Could not create {}: {e}", d.display()), "")
            })?;
        }
        let copied = copy_tree(&self.title_dir.join("etc"), &etc_dst).map_err(|e| {
            Fail::new(CODE_GENERATE_FAILED, format!("Could not stage the exported config tree: {e}"), "")
        })?;
        self.line("info", format!("staged {copied} config files into env/dist/etc"));

        let exported = read_exported_env(&self.title_dir).unwrap_or_default();
        let opts = compose_opts(self.opts, &exported).map_err(|e| Fail {
            code: Box::leak(e.code.into_boxed_str()),
            message: e.message,
            hint: e.hint,
        })?;

        let base_text = composegen::render_base(&self.title_dir, &opts)
            .map_err(|e| Fail { code: Box::leak(e.code.into_boxed_str()), message: e.message, hint: e.hint })?;
        let override_text = composegen::render_override(&opts)
            .map_err(|e| Fail { code: Box::leak(e.code.into_boxed_str()), message: e.message, hint: e.hint })?;

        write_text(&self.title_dir.join(composegen::BASE_FILE), &base_text)?;
        write_text(&self.title_dir.join(composegen::OVERRIDE_FILE), &override_text)?;
        self.line("info", format!("wrote {}", composegen::BASE_FILE));
        self.line("info", format!("wrote {} with {} carried settings", composegen::OVERRIDE_FILE, exported.len()));

        let lines = composegen::dotenv_lines(&opts);
        if !lines.is_empty() {
            let p = self.title_dir.join(composegen::DOTENV_FILE);
            let existing = std::fs::read_to_string(&p).unwrap_or_default();
            write_text(&p, &composegen::merge_dotenv(&existing, &lines))?;
            self.line("info", format!("wrote {}", composegen::DOTENV_FILE));
        }
        Ok(())
    }

    /// Create the stack shell, then unpack `client-data.tar` into its volume.
    fn do_client_data(&mut self) -> Result<(), Fail> {
        self.line("info", "creating the stack shell (volumes and network only)");
        match self.run_echo(&self.compose(vec!["up".into(), "--no-start".into()], Some(Duration::from_secs(300)))) {
            RunOutcome::Exited(0) => {}
            RunOutcome::Exited(code) => {
                return Err(Fail::new(CODE_VOLUME_FAILED, format!("compose up --no-start failed (exit {code})."), ""))
            }
            RunOutcome::CouldNotTell(why) => {
                return Err(Fail::new(CODE_VOLUME_FAILED, format!("Could not run docker compose: {why}"), ""))
            }
        }

        let volume = format!("{}_client-data", self.project);
        self.line("info", format!("restoring the game data volume {volume}"));
        // A throwaway container is the only way to write into a named volume.
        // The source directory is bind-mounted read-only and the tar is
        // extracted container-side, so no multi-GB stream crosses this process.
        let call = self.docker(
            vec![
                "run".into(),
                "--rm".into(),
                "-v".into(),
                format!("{volume}:/to"),
                "-v".into(),
                format!("{}:/src:ro", self.title_dir.display()),
                "--entrypoint".into(),
                "tar".into(),
                "nginx:alpine".into(),
                "-C".into(),
                "/to".into(),
                "-xf".into(),
                "/src/client-data.tar".into(),
            ],
            Some(Duration::from_secs(3600)),
        );
        match self.run_echo(&call) {
            RunOutcome::Exited(0) => Ok(()),
            RunOutcome::Exited(code) => Err(Fail::new(
                CODE_VOLUME_FAILED,
                format!("Restoring the client-data volume failed (exit {code})."),
                "Without the maps/vmaps/mmaps the world server starts and no zone is walkable.",
            )),
            RunOutcome::CouldNotTell(why) => {
                Err(Fail::new(CODE_VOLUME_FAILED, format!("Could not run the extract container: {why}"), ""))
            }
        }
    }

    /// Start the database, prove the target is empty, restore the dump.
    ///
    /// This is the sanctioned MySQL write. See the module docs for why the
    /// safety is a refusal rather than a consent prompt.
    fn do_db_restore(&mut self) -> Result<(), Fail> {
        // The recorded state is the ONLY thing that can distinguish "these rows
        // are the ones we just restored" from "someone else's server is here".
        // Every other stage answers that question from the disk; this one
        // cannot, because a successful restore makes the target look exactly
        // like the thing the guard refuses. Bound to this directory by
        // install_id, so a state file copied in from elsewhere is not trusted.
        if self.state.is_done_named(Stage::DbRestore.name()) {
            self.line("info", "databases were already restored by an earlier run — leaving them alone");
            return Ok(());
        }

        self.line("info", "starting the database");
        match self.run_echo(&self.compose(vec!["up".into(), "-d".into(), "ac-database".into()], Some(Duration::from_secs(300)))) {
            RunOutcome::Exited(0) => {}
            RunOutcome::Exited(code) => {
                return Err(Fail::new(CODE_DB_UNHEALTHY, format!("Could not start ac-database (exit {code})."), ""))
            }
            RunOutcome::CouldNotTell(why) => {
                return Err(Fail::new(CODE_DB_UNHEALTHY, format!("Could not run docker compose: {why}"), ""))
            }
        }

        let container = self.resolve_db_container()?;
        self.wait_db_healthy(&container)?;

        match self.check_emptiness(&container) {
            Emptiness::Empty => {}
            Emptiness::Occupied(rows) => {
                return Err(Fail::new(
                    CODE_TARGET_NOT_EMPTY,
                    format!("The database already holds {rows} characters, so this import will not write to it."),
                    "Importing would overwrite somebody's server. Use an empty games folder, or restore a backup with `wow backup restore` if replacing this server is what you meant.",
                ))
            }
            Emptiness::Unknown(why) => {
                return Err(Fail::new(
                    CODE_TARGET_UNKNOWN,
                    format!("Could not confirm the target database is empty: {why}."),
                    "The import writes character data, so it will not proceed on a maybe. Check `docker logs ac-database` and try again.",
                ))
            }
        }
        self.line("info", "target database is empty — restoring");

        let dump = self.title_dir.join("db-dump.sql.gz");
        match self.io.restore_dump(&container, &self.opts.db_password, &dump) {
            Err(e) => Err(Fail::new(
                CODE_RESTORE_FAILED,
                format!("The database restore failed. {e}").trim().to_string(),
                "A truncated db-dump.sql.gz is the usual cause — re-run the export.",
            )),
            Ok(()) => {
                self.report_counts(&container);
                Ok(())
            }
        }
    }

    /// Resolve `ac-database` through THIS stack's own compose project.
    ///
    /// Never a bare container name: a bare name answers for whichever project
    /// happens to own it, and the whole point of the guard above is that a
    /// second AzerothCore stack may exist. Same lesson the worldserver log
    /// snapshot had to learn.
    fn resolve_db_container(&mut self) -> Result<String, Fail> {
        if let Some(c) = &self.db_container {
            return Ok(c.clone());
        }
        let call = self.compose(vec!["ps".into(), "-a".into(), "-q".into(), "ac-database".into()], Some(PROBE_TIMEOUT));
        let (outcome, out) = self.run_collect(&call);
        let id = out.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("").to_string();
        match outcome {
            RunOutcome::Exited(0) if !id.is_empty() => {
                self.db_container = Some(id.clone());
                Ok(id)
            }
            _ => Err(Fail::new(
                CODE_DB_UNHEALTHY,
                "Could not resolve this stack's database container.".to_string(),
                "Refusing to fall back to the bare name `ac-database`: on a machine running a second AzerothCore stack that would address the wrong database.",
            )),
        }
    }

    fn wait_db_healthy(&self, container: &str) -> Result<(), Fail> {
        let deadline = Instant::now() + self.opts.db_timeout;
        loop {
            let call = self.probe(vec![
                "inspect".into(),
                "--format".into(),
                "{{.State.Health.Status}}".into(),
                container.to_string(),
            ]);
            let (outcome, out) = self.run_collect(&call);
            if matches!(outcome, RunOutcome::Exited(0)) && out.trim() == "healthy" {
                self.line("info", "database is healthy");
                return Ok(());
            }
            let now = Instant::now();
            if now >= deadline {
                return Err(Fail::new(
                    CODE_DB_UNHEALTHY,
                    format!(
                        "The database did not report healthy within {} minutes.",
                        self.opts.db_timeout.as_secs() / 60
                    ),
                    "Open the Console and read `docker logs ac-database` — a port already in use is the usual cause.",
                ));
            }
            std::thread::sleep(self.opts.db_poll.min(deadline.saturating_duration_since(now)));
        }
    }

    fn ask_scalar(&self, container: &str, sql: &str) -> Option<String> {
        let call = self.probe(mysql_scalar_argv(container, &self.opts.db_password, sql));
        let (outcome, out) = self.run_collect(&call);
        match outcome {
            RunOutcome::Exited(0) => Some(out.trim().to_string()),
            _ => None,
        }
    }

    fn check_emptiness(&self, container: &str) -> Emptiness {
        let present = self.ask_scalar(container, SQL_TABLE_PRESENT);
        classify_emptiness(present.as_deref(), || self.ask_scalar(container, SQL_CHARACTER_COUNT))
    }

    /// Echo what actually landed. The migration's own verification step: a
    /// number the user can compare against the server they left behind.
    fn report_counts(&self, container: &str) {
        let chars = self.ask_scalar(container, SQL_CHARACTER_COUNT);
        let accounts = self.ask_scalar(container, SQL_ACCOUNT_COUNT);
        match (chars, accounts) {
            (Some(c), Some(a)) => {
                self.line("info", format!("restored: {c} characters, {a} accounts"));
                (self.emit)(json!({"event": "counts", "characters": c, "accounts": a}));
            }
            _ => self.line(
                "warn",
                "restore finished but the verification counts could not be read — check them in game",
            ),
        }
    }

    /// Carry the launcher-side settings across.
    ///
    /// Every one of these is best-effort: they improve the imported server and
    /// none of them is a reason to fail an import whose database is already in.
    fn do_settings(&mut self) -> Result<(), Fail> {
        let src = self.title_dir.join(EXPORTED_SOAP_ENV);
        match std::fs::read(&src) {
            Err(_) => self.line(
                "warn",
                "the export carries no conf/soap.env — the launcher will set up its own SOAP account on the first status poll",
            ),
            Ok(bytes) => match dml_home(self.opts) {
                None => self.line("warn", "could not locate ~/.dml, so SOAP credentials were not installed"),
                Some(home) => {
                    let dst = home.join("soap.env");
                    let r = std::fs::create_dir_all(&home)
                        .and_then(|()| std::fs::write(&dst, strip_cr(&bytes)));
                    match r {
                        Ok(()) => self.line("info", format!("installed SOAP credentials -> {}", dst.display())),
                        Err(e) => self.line("warn", format!("could not write {}: {e}", dst.display())),
                    }
                }
            },
        }
        Ok(())
    }

    fn do_up(&mut self) -> Result<(), Fail> {
        self.line("info", "starting the stack");
        match self.run_echo(&self.compose(vec!["up".into(), "-d".into()], Some(Duration::from_secs(900)))) {
            RunOutcome::Exited(0) => Ok(()),
            RunOutcome::Exited(code) => {
                Err(Fail::new(CODE_UP_FAILED, format!("compose up failed (exit {code})."), ""))
            }
            RunOutcome::CouldNotTell(why) => {
                Err(Fail::new(CODE_UP_FAILED, format!("Could not run docker compose: {why}"), ""))
            }
        }
    }

    /// Wait for the world server's own readiness marker.
    ///
    /// Reports no percentage on purpose: this is a bounded WAIT, so
    /// `section_start` carries `limit_secs` and the consumer renders elapsed.
    /// A percentage here would measure the clock, not the work.
    fn do_ready(&mut self) -> Result<(), Fail> {
        let deadline = Instant::now() + self.opts.ready_timeout;
        loop {
            let call = self.compose(vec!["ps".into(), "-a".into(), "-q".into(), "ac-worldserver".into()], Some(PROBE_TIMEOUT));
            let (_, out) = self.run_collect(&call);
            if let Some(cid) = out.lines().map(str::trim).find(|l| !l.is_empty()) {
                let logs = self.probe(vec![
                    "logs".into(),
                    "--tail".into(),
                    crate::lifecycle::BOOT_LOOP_CAUSE_TAIL_LINES.to_string(),
                    cid.to_string(),
                ]);
                let (outcome, text) = self.run_collect(&logs);
                if outcome == RunOutcome::Exited(0) && crate::status::world_ready_from_logs(&text) {
                    self.line("info", "the world server is ready.");
                    return Ok(());
                }
            }
            let now = Instant::now();
            if now >= deadline {
                return Err(Fail::new(
                    CODE_READY_TIMEOUT,
                    format!(
                        "The world server did not report ready within {} minutes.",
                        self.opts.ready_timeout.as_secs() / 60
                    ),
                    "The containers are still running — open the Console to see where it stopped.",
                ));
            }
            std::thread::sleep(self.opts.ready_poll.min(deadline.saturating_duration_since(now)));
        }
    }

    fn run_stage(&mut self, stage: Stage) -> Result<(), Fail> {
        (self.emit)(match stage {
            Stage::Ready => section_start_limited(stage.name(), self.opts.ready_timeout.as_secs()),
            _ => section_start(stage.name()),
        });
        let result = match stage {
            Stage::Preflight => self.do_preflight(),
            Stage::Guard => self.do_guard(),
            Stage::LoadImages => self.do_load_images(),
            Stage::GenerateCompose => self.do_generate(),
            Stage::ClientData => self.do_client_data(),
            Stage::DbRestore => self.do_db_restore(),
            Stage::Settings => self.do_settings(),
            Stage::Up => self.do_up(),
            Stage::Ready => self.do_ready(),
        };
        match &result {
            Ok(()) => {
                (self.emit)(section_end(stage.name(), "ok"));
                // ONLY here, and only after the stage really finished.
                if stage.records_completion() {
                    self.state.mark_named(stage.name());
                    self.state.last_error = None;
                    self.persist();
                }
            }
            Err(f) => {
                (self.emit)(section_end(stage.name(), "error"));
                self.state.last_error = Some(format!("{}: {}", f.code, f.message));
                self.persist();
            }
        }
        result
    }

    fn go(&mut self) -> Result<(), Fail> {
        for stage in STAGE_ORDER {
            self.run_stage(stage)?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn write_text(path: &Path, text: &str) -> Result<(), Fail> {
    std::fs::write(path, text)
        .map_err(|e| Fail::new(CODE_GENERATE_FAILED, format!("Could not write {}: {e}", path.display()), ""))
}

/// Recursive copy, returning how many FILES were written.
fn copy_tree(from: &Path, to: &Path) -> std::io::Result<usize> {
    let mut n = 0;
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let src = entry.path();
        let dst = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            n += copy_tree(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst)?;
            n += 1;
        }
    }
    Ok(n)
}

// ---------------------------------------------------------------------------
// Entry points
// ---------------------------------------------------------------------------

/// Read-only status of a migration in a title dir — what the launcher polls.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct MigrateStatus {
    pub title_dir: String,
    pub export_present: bool,
    pub missing: Vec<String>,
    pub state_present: bool,
    pub completed: Vec<String>,
    pub next_stage: Option<String>,
    pub last_error: Option<String>,
}

pub fn status(opts: &MigrateOpts) -> MigrateStatus {
    let dir = opts.title_dir();
    let state = load_state(&dir);
    MigrateStatus {
        title_dir: dir.display().to_string(),
        export_present: is_export_dir(&dir),
        missing: if dir.is_dir() { missing_payload(&dir) } else { Vec::new() },
        state_present: state.is_some(),
        completed: state.as_ref().map(|s| s.completed.clone()).unwrap_or_default(),
        next_stage: state.as_ref().and_then(next_stage).map(|s| s.name().to_string()),
        last_error: state.and_then(|s| s.last_error),
    }
}

/// Import, streaming NDJSON. Returns the process exit code: `0` imported,
/// `1` refused or failed.
pub fn migrate_import_stream(opts: &MigrateOpts, emit: impl Fn(Value)) -> i32 {
    migrate_import_stream_with(&crate::install_native::ProcIo::from_env(), opts, &emit)
}

/// [`migrate_import_stream`] with its IO supplied — the seam the tests drive.
/// Production reaches this through the wrapper above, so this IS the real
/// orchestration rather than a test-only restatement of it.
pub fn migrate_import_stream_with(
    io: &dyn MigrateIo,
    opts: &MigrateOpts,
    emit: &dyn Fn(Value),
) -> i32 {
    let title_dir = opts.title_dir();
    let project = composegen::project_name_for(&title_dir);
    let install_id = composegen::install_id(&title_dir);
    let loaded = load_state(&title_dir);
    let resumed = loaded.is_some();
    let state = loaded.unwrap_or_else(|| {
        let mut s = InstallState::new(&opts.id, &install_id);
        s.version = STATE_VERSION;
        s
    });

    if resumed {
        match next_stage(&state) {
            Some(s) => emit(line_event("info", format!("resuming from {}", s.name()))),
            None => emit(line_event("info", "every stage is already recorded — re-checking each one")),
        }
    }

    let mut engine = Engine {
        io,
        opts,
        emit,
        title_dir,
        project,
        state,
        resumed,
        db_container: None,
    };

    match engine.go() {
        Ok(()) => {
            emit(json!({
                "event": "done",
                "ok": true,
                "id": opts.id,
                "dir": engine.title_dir.display().to_string(),
                "project": engine.project,
                // Snapshot semantics, said out loud. The source server keeps
                // running and keeps diverging; nothing syncs back.
                "note": "This is a snapshot. Progress made on the old server after the export does not come across.",
            }));
            0
        }
        Err(f) => {
            emit(json!({
                "event": "error",
                "code": f.code,
                "message": f.message,
                "hint": f.hint,
            }));
            1
        }
    }
}

/// Resolve the games dir the same way `install-native` does, so the two
/// commands can never disagree about where servers live.
pub fn games_dir() -> Result<PathBuf, CmdError> {
    games_dir_for_install()
}

/// Canonical form of a path, re-exported so callers comparing directories use
/// the same folding rules the guard does.
pub fn canon(p: &str) -> String {
    canon_path(p)
}
