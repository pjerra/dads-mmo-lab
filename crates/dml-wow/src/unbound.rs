//! Wrath Unbound add-on: the native install engine.
//!
//! A staged, RESUMABLE state machine that layers the Wrath Unbound multi-class
//! add-on onto an EXISTING native-backend AzerothCore server and rebuilds the
//! worldserver to match, streamed as NDJSON. It is a port of
//! `guides/unbound-wrath/install-wrath-unbound-addon.sh` (v1.2.2) in the same
//! sense `install_native.rs` is a port of the title installers: the bash
//! specifies the OUTCOME, not the mechanism.
//!
//! ## Why a port and not a script runner
//!
//! The native backend has no bash to run the script in — and the script's
//! dominant failure class is the silent half-success, which only a rewrite can
//! convert into refusals. The ones that mattered (each verified against the
//! source, with line numbers in the design doc
//! `docs/superpowers/specs/2026-08-01-wrath-unbound-native-port-design.md`):
//!
//! * `git apply`'s exit status was never checked, and the idempotency probe
//!   read ONE of the six patched files — a partially applied core patch
//!   reported success and the server booted missing features.
//! * The consent screen named migrations 03/05 but not 06's
//!   `DELETE FROM skillraceclassinfo_dbc WHERE ID >= 10000` or the 12/13/14
//!   catalog deletes.
//! * Stage 5 wrote 19 files, checked none of them, and printed "Module files
//!   staged" unconditionally (no `set -e`, no `set -u`).
//! * Readiness grepped the container's WHOLE log, so a marker from a previous
//!   boot read as success; and verify asked "Continue anyway?" in front of a
//!   90-minute build.
//! * Every `docker exec` used the engine-global name `ac-database`, which
//!   answers for whichever stack owns it. This engine resolves the container
//!   ID through the server's own compose project and execs THAT.
//!
//! ## The engine shape, inherited from `install_native`
//!
//! Same discipline, same reasons (see that module's doc comments for the
//! incident history): state is written ONLY after a stage really finished;
//! guards are deliberately never recorded — a guard a resume skips is not a
//! guard; every probe is time-bounded; a probe that cannot answer is evidence
//! of NOTHING; and the long stages stream with `pct` from the ninja counter.
//!
//! ## This is a sanctioned MySQL WRITE surface
//!
//! The SQL migrations go through `docker exec -i … mysql`, the same channel
//! `wow backup restore` uses. The user sanctioned the class on 2026-08-02 by
//! commissioning this port ("port it"); it is recorded beside the other
//! sanctioned writes in CLAUDE.md and `docs/cli-contract.md`. What keeps it
//! narrow: every byte that reaches mysql is an EMBEDDED, fingerprint-pinned
//! payload file ([`crate::unbound_payload`]) — nothing here splices user input
//! into SQL — and nothing runs before a verified safety backup exists.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use dml_core::conf;
use dml_core::events::{
    done_event, error_event, line_event, pct_event, section_end, section_start,
    section_start_limited,
};

use crate::install_native::{
    BuildProgress, Call, InstallIo, ProcIo, Program, RunOutcome, PROBE_TIMEOUT,
};
use crate::unbound_payload::{self, ADDON_VERSION, MANIFEST, SQL_ORDER};
use crate::{composegen, lifecycle, logsnap, maint};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// The engine's own state file, in the SERVER dir. Never `.dml-install.json` —
/// `install_native` owns that name, and the two must coexist in one directory.
pub const STATE_FILE: &str = ".dml-unbound.json";
pub const STATE_VERSION: u32 = 1;

/// Default title the add-on installs onto.
pub const DEFAULT_TITLE_ID: &str = "wow-server-playerbots";

/// The line the Mentor's Lua prints once its prerequisite map is built — the
/// proof that mod-unbound compiled in, Eluna loaded, and the script ran.
/// Everything short of this can be true on a server where the add-on silently
/// did not take.
pub const READY_MARKER: &str = "[UNBOUND] Prereq map built.";

pub const READY_TIMEOUT: Duration = Duration::from_secs(1800);
pub const READY_POLL: Duration = Duration::from_secs(10);

/// mod-ale (the Eluna engine mod-unbound's Lua needs), pinned to the commit
/// the add-on was tested against. The bash WARNED and built whatever HEAD was
/// when this checkout failed; an untested Lua engine under a 90-minute build
/// is not "may or may not match", it is a refusal.
pub const MOD_ALE_URL: &str = "https://github.com/azerothcore/mod-ale.git";
pub const MOD_ALE_COMMIT: &str = "1cb86c9600260c3731c96dc3c98d25b4fc3f2153";

/// The Playerbots evidence: the 9 synthetic trainer IDs the base install
/// seeds. `>= 9` distinct and `>= 100` rows is the bash's own gate.
pub const TRAINER_PROBE_SQL: &str = "SELECT COUNT(DISTINCT ID), COUNT(*) FROM npc_trainer WHERE ID IN (200002,200004,200006,200008,200010,200012,200014,200016,200018) AND SpellID > 0;";
/// The canary the installer's own migration 01 creates.
pub const CANARY_SQL: &str = "SELECT 1 FROM unbound_milestones LIMIT 1;";

/// The six core files the patch touches. Every hunk references
/// `UnboundClassMask` (verified against the embedded patch), which is what
/// makes a per-file symbol probe possible at all.
pub const PATCHED_FILES: [&str; 6] = [
    "src/server/game/Conditions/ConditionMgr.cpp",
    "src/server/game/Entities/Creature/Trainer.cpp",
    "src/server/game/Entities/Player/Player.cpp",
    "src/server/game/Entities/Player/Player.h",
    "src/server/game/Entities/Player/PlayerQuest.cpp",
    "src/server/game/Entities/Player/PlayerStorage.cpp",
];
pub const PATCH_SYMBOL: &str = "UnboundClassMask";

/// The conf key without which the add-on destroys its own work: at the
/// AzerothCore default of 1, every login strips cross-class spells from the
/// character — silently wiping anything bought through the Mentor.
pub const VALIDATE_KEY: &str = "ValidateSkillLearnedBySpells";

/// The container path ALE scans for Lua — a constant of the compose layout,
/// not of this machine (it is the path INSIDE ac-worldserver).
pub const ALE_SCRIPT_PATH: &str = "/azerothcore/env/dist/etc/modules/lua_scripts";

/// mod_ale.conf as the installer creates it FRESH. On an existing file only
/// the first two keys are enforced — the rest are defaults a user may have
/// deliberately changed, and repairing a conf must not undo their choices.
pub const ALE_CONF_DEFAULTS: [(&str, &str); 9] = [
    ("ALE.Enabled", "1"),
    ("ALE.ScriptPath", "\"/azerothcore/env/dist/etc/modules/lua_scripts\""),
    ("ALE.TraceBack", "false"),
    ("ALE.PlayerAnnounceReload", "false"),
    ("ALE.RequirePaths", "\"\""),
    ("ALE.RequireCPaths", "\"\""),
    ("ALE.AutoReload", "false"),
    ("ALE.AutoReloadInterval", "1"),
    ("ALE.BytecodeCache", "true"),
];
/// How many of [`ALE_CONF_DEFAULTS`] are ENFORCED on an existing file.
pub const ALE_CONF_REQUIRED: usize = 2;

pub const MIN_FREE_BYTES: u64 = 10 * 1024 * 1024 * 1024;

// Error codes — all documented in docs/cli-contract.md.
pub const CODE_NO_SERVER: &str = "UNBOUND_NO_SERVER";
pub const CODE_NOT_AZEROTHCORE: &str = "UNBOUND_NOT_AZEROTHCORE";
pub const CODE_SERVER_NOT_RUNNING: &str = "UNBOUND_SERVER_NOT_RUNNING";
pub const CODE_DB_UNREACHABLE: &str = "UNBOUND_DB_UNREACHABLE";
pub const CODE_NOT_PLAYERBOTS: &str = "UNBOUND_NOT_PLAYERBOTS";
pub const CODE_ALREADY_INSTALLED: &str = "UNBOUND_ALREADY_INSTALLED";
pub const CODE_VERSION_MISMATCH: &str = "UNBOUND_VERSION_MISMATCH";
pub const CODE_PATCH_INCOHERENT: &str = "UNBOUND_PATCH_INCOHERENT";
pub const CODE_CONSENT_REQUIRED: &str = "UNBOUND_CONSENT_REQUIRED";
pub const CODE_DISK_LOW: &str = "UNBOUND_DISK_LOW";
pub const CODE_GIT_MISSING: &str = "UNBOUND_GIT_MISSING";
pub const CODE_DOCKER_UNAVAILABLE: &str = "UNBOUND_DOCKER_UNAVAILABLE";
pub const CODE_BACKUP_FAILED: &str = "UNBOUND_BACKUP_FAILED";
pub const CODE_WRITE_FAILED: &str = "UNBOUND_WRITE_FAILED";
pub const CODE_ALE_CLONE_FAILED: &str = "UNBOUND_ALE_CLONE_FAILED";
pub const CODE_ALE_PIN_MISMATCH: &str = "UNBOUND_ALE_PIN_MISMATCH";
pub const CODE_SQL_FAILED: &str = "UNBOUND_SQL_FAILED";
pub const CODE_PATCH_CHECK_FAILED: &str = "UNBOUND_PATCH_CHECK_FAILED";
pub const CODE_CONF_MISSING: &str = "UNBOUND_CONF_MISSING";
pub const CODE_VERIFY_FAILED: &str = "UNBOUND_VERIFY_FAILED";
pub const CODE_BUILD_FAILED: &str = "UNBOUND_BUILD_FAILED";
pub const CODE_UP_FAILED: &str = "UNBOUND_UP_FAILED";
pub const CODE_READY_TIMEOUT: &str = "UNBOUND_READY_TIMEOUT";

// ---------------------------------------------------------------------------
// Stages
// ---------------------------------------------------------------------------

/// Install stages, in run order.
///
/// One deliberate departure from the design doc's diagram: SQL application is
/// ONE stage, not a `sql-world` / `sql-chars` pair. The split bought no
/// resume granularity (the whole list is minutes, and each file is recorded
/// per-line anyway) and mislabeled `npc_setup.sql`, which targets acore_world
/// but must run LAST of all — after db-characters — so a section named
/// `sql-chars` would have contained a world migration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Preflight,
    Locate,
    Guard,
    Backup,
    StageFiles,
    CloneAle,
    Sql,
    CorePatch,
    Conf,
    Verify,
    Build,
    Up,
    Ready,
}

pub const STAGE_ORDER: [Stage; 13] = [
    Stage::Preflight,
    Stage::Locate,
    Stage::Guard,
    Stage::Backup,
    Stage::StageFiles,
    Stage::CloneAle,
    Stage::Sql,
    Stage::CorePatch,
    Stage::Conf,
    Stage::Verify,
    Stage::Build,
    Stage::Up,
    Stage::Ready,
];

impl Stage {
    pub fn name(self) -> &'static str {
        match self {
            Stage::Preflight => "preflight",
            Stage::Locate => "locate",
            Stage::Guard => "guard",
            Stage::Backup => "backup",
            Stage::StageFiles => "stage-files",
            Stage::CloneAle => "clone-ale",
            Stage::Sql => "sql",
            Stage::CorePatch => "core-patch",
            Stage::Conf => "conf",
            Stage::Verify => "verify",
            Stage::Build => "build",
            Stage::Up => "up",
            Stage::Ready => "ready",
        }
    }

    /// Whether completion is recorded in the state file. The guards are
    /// deliberately NOT — a guard a resume skips is not a guard — and neither
    /// is `verify`, which must re-run on every attempt because the files it
    /// checks can change between runs for reasons this engine never sees.
    pub fn records_completion(self) -> bool {
        !matches!(self, Stage::Preflight | Stage::Locate | Stage::Guard | Stage::Verify)
    }
}

/// Uninstall stages, in run order. The database revert comes FIRST among the
/// mutations (the bash's order, kept): it needs the OLD worldserver still
/// happily running against a database whose unbound tables are going away —
/// mod-unbound tolerates missing tables far better than a rebuilt stock
/// server tolerates their presence mattering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UninstallStage {
    Preflight,
    Locate,
    Guard,
    Backup,
    SqlRevert,
    RemoveFiles,
    PatchRevert,
    ConfRevert,
    Build,
    Up,
    Ready,
}

pub const UNINSTALL_ORDER: [UninstallStage; 11] = [
    UninstallStage::Preflight,
    UninstallStage::Locate,
    UninstallStage::Guard,
    UninstallStage::Backup,
    UninstallStage::SqlRevert,
    UninstallStage::RemoveFiles,
    UninstallStage::PatchRevert,
    UninstallStage::ConfRevert,
    UninstallStage::Build,
    UninstallStage::Up,
    UninstallStage::Ready,
];

impl UninstallStage {
    pub fn name(self) -> &'static str {
        match self {
            UninstallStage::Preflight => "preflight",
            UninstallStage::Locate => "locate",
            UninstallStage::Guard => "guard",
            UninstallStage::Backup => "backup",
            UninstallStage::SqlRevert => "sql-revert",
            UninstallStage::RemoveFiles => "remove-files",
            UninstallStage::PatchRevert => "patch-revert",
            UninstallStage::ConfRevert => "conf-revert",
            UninstallStage::Build => "build",
            UninstallStage::Up => "up",
            UninstallStage::Ready => "ready",
        }
    }
    pub fn records_completion(self) -> bool {
        !matches!(
            self,
            UninstallStage::Preflight | UninstallStage::Locate | UninstallStage::Guard
        )
    }
}

/// The database revert, verbatim from the bash uninstaller (order included:
/// `creature_addon` references `creature.guid`, so it goes first).
///
/// Two of these look recklessly wide and are NOT: migration 06 opens with
/// `DELETE FROM skillraceclassinfo_dbc WHERE ID >= 10000` as its OWN
/// idempotency header, and 03 opens with the exact classmask wipe — the
/// add-on defines those ranges as add-on-owned, so they are the canonical
/// revert predicates, not an approximation of one. (This is why the design
/// doc's "narrowed predicates" idea was dropped: narrower than the add-on's
/// own re-run semantics would revert less than an upgrade already deletes.)
///
/// TASK 6 SCOPE RULING: the `acore_*` db column stays STANDARD on purpose —
/// this reverts the fingerprint-pinned Unbound payload, whose own SQL
/// hardcodes the standard names; the add-on requires them end-to-end. See
/// `unbound_payload::SqlDb::database` for the full ruling. Do not resolve.
pub const REVERT_SQL: [(&str, &str, &str); 11] = [
    (
        "acore_world",
        "DELETE FROM creature_addon WHERE guid IN (SELECT guid FROM creature WHERE id1 = 900001);",
        "Mentor spawn add-on rows removed",
    ),
    ("acore_world", "DELETE FROM creature WHERE id1 = 900001;", "Mentor spawns despawned"),
    (
        "acore_world",
        "DELETE FROM creature_template_model WHERE CreatureID = 900001;",
        "Mentor model removed",
    ),
    (
        "acore_world",
        "DELETE FROM creature_template WHERE entry = 900001;",
        "Mentor template removed",
    ),
    (
        "acore_world",
        "DELETE FROM playercreateinfo_item WHERE itemid = 900100;",
        "Mentor Stone removed from character creation",
    ),
    (
        "acore_world",
        "DELETE FROM item_template WHERE entry = 900100;",
        "Mentor Stone item template removed",
    ),
    (
        "acore_world",
        "DELETE FROM playercreateinfo_spell_custom WHERE racemask = 0 AND classmask IN (1,2,4,8,16,64,128,256,1024);",
        "creation gift spells removed",
    ),
    (
        "acore_world",
        "DELETE FROM skillraceclassinfo_dbc WHERE ID >= 10000;",
        "universal skill access rows removed",
    ),
    ("acore_world", "DROP TABLE IF EXISTS unbound_class_catalog;", "unbound_class_catalog dropped"),
    ("acore_world", "DROP TABLE IF EXISTS unbound_milestones;", "unbound_milestones dropped"),
    (
        "acore_characters",
        "DROP TABLE IF EXISTS unbound_character_unlocks;",
        "unbound_character_unlocks dropped (character class-unlock progression)",
    ),
];

/// Handed to the user as TEXT in the uninstall's done event, never executed:
/// deleting items out of character inventories is a decision about people's
/// bags, not cleanup.
pub const MENTOR_STONE_CLEANUP_SQL: &str = "DELETE ci FROM character_inventory ci JOIN item_instance ii ON ci.item=ii.guid WHERE ii.itemEntry=900100; DELETE FROM item_instance WHERE itemEntry=900100;";

pub const CODE_NOT_INSTALLED: &str = "UNBOUND_NOT_INSTALLED";
/// The server has no `build:` configuration for ac-worldserver anywhere —
/// it runs from prebuilt images and the module can never be compiled in.
pub const CODE_NO_BUILD_CONFIG: &str = "UNBOUND_NO_BUILD_CONFIG";

// ---------------------------------------------------------------------------
// Options and state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct UnboundOpts {
    /// Title dir name under `games_dir`. The launcher passes its title id;
    /// the CLI defaults to [`DEFAULT_TITLE_ID`].
    pub id: String,
    pub games_dir: PathBuf,
    /// The consent flag. The migrations DELETE third-party rows
    /// (`playercreateinfo_spell_custom` for 9 classmasks in 03/05,
    /// `skillraceclassinfo_dbc ID>=10000` in 06, catalog rows in 12/13/14) and
    /// there is no prompting in this engine — the caller asks, we verify.
    pub accept_data_changes: bool,
    /// Re-run everything over an install this engine believes is complete.
    pub repair: bool,
    /// Uninstall only: run the revert even when no evidence of an install is
    /// found - for the server whose evidence is itself broken.
    ///
    /// NOT a safe no-op, and the earlier claim that it was ("every statement
    /// is IF-EXISTS-shaped") was false: the DROPs are, but two of the eleven
    /// are unconditional wide DELETEs on tables OTHER mods share
    /// (`playercreateinfo_spell_custom` racemask=0 across 9 class masks, and
    /// `skillraceclassinfo_dbc ID >= 10000`). On a server that never had the
    /// add-on, those delete somebody else's rows.
    pub force: bool,
    pub ready_timeout: Duration,
    pub ready_poll: Duration,
}

impl UnboundOpts {
    pub fn new(games_dir: impl Into<PathBuf>) -> Self {
        UnboundOpts {
            id: DEFAULT_TITLE_ID.to_string(),
            games_dir: games_dir.into(),
            accept_data_changes: false,
            repair: false,
            force: false,
            ready_timeout: READY_TIMEOUT,
            ready_poll: READY_POLL,
        }
    }
}

/// What a previous attempt got through — same discipline as
/// [`crate::install_native::InstallState`], plus the add-on version (what
/// `UNBOUND_VERSION_MISMATCH` compares) and the conf value the install
/// replaced (what uninstall restores instead of guessing).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnboundState {
    pub version: u32,
    /// [`composegen::install_id`] of the SERVER dir — the binding that makes a
    /// copied state file worthless in any other directory.
    pub install_id: String,
    pub addon_version: String,
    pub completed: Vec<String>,
    /// `ValidateSkillLearnedBySpells` as found BEFORE the conf stage first
    /// touched it. `None` = never recorded (conf stage not reached yet).
    pub prior_validate_skill: Option<String>,
    /// Uninstall progress, tracked separately so an interrupted uninstall
    /// resumes instead of starting over. A COMPLETED uninstall deletes the
    /// whole state file — a file claiming install-complete on a stock server
    /// would be a lie the next install trips over.
    ///
    /// CLEARED by any successful install (review CRITICAL, 2026-08-02): left
    /// in place, a `backup` token from a long-abandoned uninstall attempt
    /// would let every FUTURE uninstall of this server skip its safety dump
    /// forever — right before the DROP of character progression.
    #[serde(default)]
    pub uninstalling: Vec<String>,
    /// The safety dump this install/uninstall run is protected by — the FILE
    /// NAME under `~/.dml/backups`. Recorded so a resume can check the dump
    /// still EXISTS before trusting the `backup` completion token: the backup
    /// pool is shared and pruned to newest-N by every other dump on the box,
    /// so "recorded done" and "still on disk" genuinely diverge.
    #[serde(default)]
    pub backup_file: Option<String>,
    pub last_error: Option<String>,
    pub updated_unix: u64,
}

impl UnboundState {
    pub fn new(install_id: &str) -> Self {
        UnboundState {
            version: STATE_VERSION,
            install_id: install_id.to_string(),
            addon_version: ADDON_VERSION.to_string(),
            completed: Vec::new(),
            prior_validate_skill: None,
            uninstalling: Vec::new(),
            backup_file: None,
            last_error: None,
            updated_unix: 0,
        }
    }
    pub fn is_done(&self, stage: Stage) -> bool {
        self.completed.iter().any(|s| s == stage.name())
    }
    pub fn mark(&mut self, stage: Stage) {
        if !self.is_done(stage) {
            self.completed.push(stage.name().to_string());
        }
    }
    pub fn is_undone(&self, stage: UninstallStage) -> bool {
        self.uninstalling.iter().any(|s| s == stage.name())
    }
    pub fn mark_undone(&mut self, stage: UninstallStage) {
        if !self.is_undone(stage) {
            self.uninstalling.push(stage.name().to_string());
        }
    }
    /// All recordable install stages claimed?
    pub fn complete(&self) -> bool {
        STAGE_ORDER.iter().all(|s| !s.records_completion() || self.is_done(*s))
    }
}

pub fn state_path(server_dir: &Path) -> PathBuf {
    server_dir.join(STATE_FILE)
}

/// Read the state file, or `None` for absent / unreadable / wrong-version /
/// WRONG-DIRECTORY — all of which mean "no trustworthy progress".
pub fn load_state(server_dir: &Path) -> Option<UnboundState> {
    let text = std::fs::read_to_string(state_path(server_dir)).ok()?;
    let state: UnboundState = serde_json::from_str(&text).ok()?;
    if state.version != STATE_VERSION {
        return None;
    }
    if state.install_id != composegen::install_id(server_dir) {
        return None;
    }
    Some(state)
}

pub fn save_state(server_dir: &Path, state: &UnboundState) -> std::io::Result<()> {
    let mut state = state.clone();
    state.updated_unix = now_unix();
    let text = serde_json::to_string_pretty(&state).unwrap_or_default();
    conf::atomic_write(&state_path(server_dir), &text)
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// IO seam
// ---------------------------------------------------------------------------

/// [`InstallIo`] plus the two impure things this engine does that the title
/// installer never needed: pipe bytes into a subprocess's stdin (the SQL
/// migrations), and take a safety backup.
pub trait UnboundIo: InstallIo {
    /// Run `call` feeding `input` on stdin. Only ever used with a bounded
    /// timeout — a migration that outlives [`crate::modmgr::SQL_APPLY_TIMEOUT`]
    /// is a wedged mysql, not a slow one.
    fn run_stdin(&self, call: &Call, input: &[u8], on_line: &mut dyn FnMut(&str)) -> RunOutcome;

    /// The pre-mutation safety dump (world INCLUDED — the migrations write
    /// acore_world), taken from `container` — the database container the
    /// GUARD resolved through this server's own compose project, never the
    /// engine-global name (a dump of whichever stack owns `ac-database` can
    /// capture a different server's data than the one about to be mutated).
    /// `Ok(file_name)` only when the dump really landed; `Err(tail)` carries
    /// the stderr tail the bash used to discard.
    fn safety_backup(
        &self,
        container: &str,
        password: &str,
        on_line: &mut dyn FnMut(String, String),
    ) -> Result<String, String>;

    /// Is this dump file still present in the backup pool? A resume consults
    /// this before trusting a recorded `backup` token: the pool is shared and
    /// pruned to newest-N by every other dump on the box.
    fn backup_exists(&self, file_name: &str) -> bool {
        crate::backup::backup_dir().is_some_and(|d| d.join(file_name).is_file())
    }
}

/// Production IO: [`ProcIo`] for run/preflight/ensure_engine, plus the real
/// stdin pipe and the real dump.
pub struct ProcUnboundIo {
    pub inner: ProcIo,
}

impl ProcUnboundIo {
    pub fn from_env() -> Self {
        ProcUnboundIo { inner: ProcIo::from_env() }
    }
}

impl InstallIo for ProcUnboundIo {
    fn preflight(&self, games_dir: &Path) -> crate::preflight::PreflightFacts {
        self.inner.preflight(games_dir)
    }
    fn run(&self, call: &Call, on_line: &mut dyn FnMut(&str)) -> RunOutcome {
        self.inner.run(call, on_line)
    }
    fn ensure_engine(&self, on_line: &mut dyn FnMut(String, String)) -> bool {
        self.inner.ensure_engine(on_line)
    }
}

impl UnboundIo for ProcUnboundIo {
    fn run_stdin(&self, call: &Call, input: &[u8], on_line: &mut dyn FnMut(&str)) -> RunOutcome {
        let program = match call.program {
            Program::Git => &self.inner.git,
            Program::Docker => &self.inner.docker,
        };
        let mut cmd = std::process::Command::new(program);
        cmd.args(&call.args);
        if let Some(dir) = call.cwd.as_deref() {
            cmd.current_dir(dir);
        }
        dml_core::proc::windows_no_window(&mut cmd);
        let limit = call.timeout.unwrap_or(crate::modmgr::SQL_APPLY_TIMEOUT);
        match crate::modmgr::run_with_stdin_bounded_draining(cmd, input.to_vec(), limit) {
            Some(out) => {
                for l in String::from_utf8_lossy(&out.stdout).lines() {
                    on_line(l);
                }
                for l in String::from_utf8_lossy(&out.stderr).lines() {
                    on_line(l);
                }
                RunOutcome::Exited(out.status.code().unwrap_or(-1))
            }
            None => RunOutcome::CouldNotTell(format!(
                "{} did not answer within {}s",
                Path::new(program).display(),
                limit.as_secs()
            )),
        }
    }

    fn safety_backup(
        &self,
        container: &str,
        password: &str,
        on_line: &mut dyn FnMut(String, String),
    ) -> Result<String, String> {
        use crate::backup;
        let bdir = backup::backup_dir().ok_or_else(|| "no home directory".to_string())?;
        std::fs::create_dir_all(&bdir).map_err(|e| e.to_string())?;
        let file_name = backup::new_backup_file_name(true);
        let out_path = bdir.join(&file_name);
        on_line(
            "info".into(),
            "backing up characters, bots, accounts and world before changing anything...".into(),
        );
        // Task 6 scope ruling: the unbound subsystem is a recorded EXCEPTION
        // to resolved-name splicing — its fingerprint-pinned SQL payload
        // hardcodes the standard `acore_*` schemas internally, so the whole
        // add-on requires standard names end-to-end (its own guard probes
        // `acore_world` and refuses a renamed server before this dump can
        // run). The safety dump therefore names the SAME standard set the
        // migrations are about to mutate — not a guess, the subsystem's
        // contract.
        let standard = crate::db::DatabaseNames {
            world: "acore_world".to_string(),
            characters: "acore_characters".to_string(),
            auth: "acore_auth".to_string(),
            playerbots: Some("acore_playerbots".to_string()),
        };
        backup::dump_to_container(&self.inner.docker, container, password, true, &out_path, &standard)?;
        for p in backup::prune(&bdir) {
            on_line("info".into(), format!("pruned old backup: {p}"));
        }
        Ok(file_name)
    }
}

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

/// What the six patched files say about the core patch, read from disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchPresence {
    /// Every file carries the symbol — the patch is applied.
    All,
    /// No file carries it — a clean tree.
    None,
    /// SOME do. This is the bash's worst bug made visible: a half-applied
    /// patch, which it reported as success because it probed one file.
    Mixed { missing: Vec<String> },
}

pub fn probe_patch_presence(server_dir: &Path) -> PatchPresence {
    let mut present = 0usize;
    let mut missing: Vec<String> = Vec::new();
    for rel in PATCHED_FILES {
        let has = std::fs::read_to_string(server_dir.join(rel))
            .map(|t| t.contains(PATCH_SYMBOL))
            .unwrap_or(false);
        if has {
            present += 1;
        } else {
            missing.push(rel.to_string());
        }
    }
    match present {
        6 => PatchPresence::All,
        0 => PatchPresence::None,
        _ => PatchPresence::Mixed { missing },
    }
}

/// `"9\t1084"` (or space-separated) → `(distinct, rows)`.
pub fn parse_trainer_counts(out: &str) -> Option<(i64, i64)> {
    let line = out.lines().map(str::trim).find(|l| !l.is_empty())?;
    let mut it = line.split_whitespace();
    let a = it.next()?.parse().ok()?;
    let b = it.next()?.parse().ok()?;
    Some((a, b))
}

/// The last few interesting lines of a failed command's output — for error
/// messages. mysql's password-on-the-command-line warning is dropped because
/// it appears on every call and would bury the real error.
pub fn stderr_tail(collected: &str) -> String {
    let lines: Vec<&str> = collected
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter(|l| !l.contains("Using a password on the command line"))
        .collect();
    lines.iter().rev().take(4).rev().cloned().collect::<Vec<_>>().join(" | ")
}

/// Build-log file name; the timestamp leads nothing here (one file per run,
/// under the server's own `logs/`), it just keeps runs distinct.
pub fn build_log_name(unix: u64) -> String {
    format!("unbound-build-{unix}.log")
}

/// The DB credentials for a server dir: env wins, then the server's `.env`,
/// then the stock defaults — the same precedence every other native reader
/// uses ([`crate::db::resolve_db_config`]).
pub fn db_password_for(server_dir: &Path) -> String {
    let dotenv = std::fs::read_to_string(server_dir.join(".env")).ok();
    crate::db::resolve_db_config(|k| std::env::var(k).ok(), dotenv.as_deref()).password
}

// ---------------------------------------------------------------------------
// The engine
// ---------------------------------------------------------------------------

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

struct Engine<'a> {
    io: &'a dyn UnboundIo,
    opts: &'a UnboundOpts,
    emit: &'a dyn Fn(Value),
    /// Filled by `locate`. Every stage after it goes through [`Self::sdir`].
    server_dir: Option<PathBuf>,
    state: Option<UnboundState>,
    resumed: bool,
    /// The ac-database container ID resolved through THIS server's compose
    /// project (guard step b). Every `docker exec` uses this, never the
    /// engine-global name.
    db_container: Option<String>,
    db_password: String,
    backup_file: Option<String>,
    /// Uninstall only: everything the run could not (or deliberately did not)
    /// revert. Reported in the done event, never silently banner-ed away.
    residue: Vec<String>,
    /// Which direction this engine is running — locate uses it to clean up
    /// the OTHER direction's leftovers in the state file.
    uninstalling_mode: bool,
    /// The `-f` arguments the build stage needs. Empty = let compose
    /// auto-load. Filled by the guard: a composegen server keeps its `build:`
    /// sections in docker-compose.build.yml, which compose NEVER auto-loads —
    /// so a bare `compose build` there "succeeds" in seconds having built
    /// nothing, and the ready stage then times out on an unmodified server.
    build_files: Vec<String>,
}

impl<'a> Engine<'a> {
    fn line(&self, level: &str, text: impl Into<String>) {
        (self.emit)(line_event(level, text));
    }

    fn sdir(&self) -> &Path {
        self.server_dir.as_deref().expect("locate ran before every later stage")
    }
    fn st(&mut self) -> &mut UnboundState {
        self.state.as_mut().expect("locate ran before every later stage")
    }

    fn run_collect(&self, call: &Call) -> (RunOutcome, String) {
        let mut buf = String::new();
        let outcome = self.io.run(call, &mut |l| {
            buf.push_str(l);
            buf.push('\n');
        });
        (outcome, buf)
    }

    fn docker_probe(&self, args: Vec<String>, cwd: Option<PathBuf>) -> Call {
        Call { program: Program::Docker, args, cwd, timeout: Some(PROBE_TIMEOUT) }
    }
    fn docker_unbounded(&self, args: Vec<String>, cwd: Option<PathBuf>) -> Call {
        Call { program: Program::Docker, args, cwd, timeout: None }
    }
    fn git_probe(&self, args: Vec<String>) -> Call {
        Call { program: Program::Git, args, cwd: None, timeout: Some(PROBE_TIMEOUT) }
    }

    /// `docker exec <cid> mysql …` for a single READ probe against a
    /// database. PROBE_TIMEOUT — its answer is the point.
    fn mysql_probe(&self, db: &str, sql: &str) -> Call {
        let mut call = self.mysql_stmt_call(db, sql);
        call.timeout = Some(PROBE_TIMEOUT);
        call
    }

    /// The same exec for a MUTATING statement — bounded by the SQL timeout,
    /// not the probe one (review finding, 2026-08-02: the uninstall's DROPs
    /// ran under the 30s probe bound; a big `unbound_character_unlocks` or a
    /// cold buffer pool can exceed that, and killing the docker-exec CLIENT
    /// does not stop the statement inside mysqld — the engine then reports a
    /// failure for a revert that in fact completed).
    fn mysql_stmt_call(&self, db: &str, sql: &str) -> Call {
        let cid = self.db_container.clone().expect("guard resolved the container");
        Call {
            program: Program::Docker,
            args: vec![
                "exec".into(),
                cid,
                "mysql".into(),
                "-uroot".into(),
                format!("-p{}", self.db_password),
                "-N".into(),
                db.into(),
                "-e".into(),
                sql.into(),
            ],
            cwd: None,
            timeout: Some(crate::modmgr::SQL_APPLY_TIMEOUT),
        }
    }

    /// Which `-f` files the BUILD needs, from disk evidence — see
    /// [`crate::buildcap::build_files`], extracted from this method.
    fn resolve_build_files(&mut self) {
        self.build_files = crate::buildcap::build_files(&self.sdir().to_path_buf());
    }

    fn persist(&mut self) {
        let sdir = self.sdir().to_path_buf();
        let state = self.state.as_ref().expect("state exists when persisting").clone();
        if let Err(e) = save_state(&sdir, &state) {
            self.line("warn", format!("could not record install progress: {e}"));
        }
    }

    fn persist_failure(&mut self, f: &Fail) {
        if self.state.is_some() && self.server_dir.is_some() {
            self.st().last_error = Some(format!("{}: {}", f.code, f.message));
            // Unlike install_native, the server dir ALWAYS pre-exists here
            // (we refuse to run without one), so recording a failure can never
            // be the thing that pollutes an empty directory.
            self.persist();
        }
    }

    // -- stages -------------------------------------------------------------

    fn do_preflight(&mut self) -> Result<(), Fail> {
        let mut facts = self.io.preflight(&self.opts.games_dir);

        // Start a stopped engine rather than refusing over it — the same
        // choice install_native made after the user hit the refusal on a
        // machine where Home's Start button would have just fixed it.
        if facts.docker.reachable != dml_core::setup::Tri::Yes {
            let mut lines: Vec<(String, String)> = Vec::new();
            let attempted = self.io.ensure_engine(&mut |level, text| lines.push((level, text)));
            for (level, text) in lines {
                self.line(&level, text);
            }
            if attempted {
                facts = self.io.preflight(&self.opts.games_dir);
            }
        }
        if facts.docker.reachable != dml_core::setup::Tri::Yes {
            return Err(Fail::new(
                CODE_DOCKER_UNAVAILABLE,
                "Docker is not answering, and the rebuild cannot run without it.",
                "Start Docker Desktop, wait for the whale icon to settle, then run this again.",
            ));
        }
        if facts.git != dml_core::setup::Tri::Yes {
            return Err(Fail::new(
                CODE_GIT_MISSING,
                "git was not found, and the core patch cannot be applied without it.",
                "Install Git for Windows (git-scm.com), then run this again.",
            ));
        }
        // Tri-state: an UNKNOWN free-space reading refuses nothing — a probe
        // that cannot answer is evidence of nothing. Only a KNOWN-low disk is.
        if let Some(free) = facts.games_disk.as_ref().and_then(|d| d.free_bytes) {
            if free < MIN_FREE_BYTES {
                return Err(Fail::new(
                    CODE_DISK_LOW,
                    format!(
                        "Only {:.1} GiB free where the server lives -- the rebuild needs room to work.",
                        free as f64 / (1024.0 * 1024.0 * 1024.0)
                    ),
                    "Free up at least 10 GiB and run this again.",
                ));
            }
        } else {
            self.line("warn", "could not read free disk space -- continuing without the check.");
        }
        Ok(())
    }

    fn do_locate(&mut self) -> Result<(), Fail> {
        let title_dir = self.opts.games_dir.join(&self.opts.id);
        let Some(sdir) = maint::resolve_server_dir(&title_dir) else {
            return Err(Fail::new(
                CODE_NO_SERVER,
                format!("No server was found under {}.", title_dir.display()),
                "Wrath Unbound layers onto an EXISTING WotLK Playerbots server. Install one first, then add this.",
            ));
        };
        self.line("info", format!("server found: {}", sdir.display()));
        self.db_password = db_password_for(&sdir);

        let existing = load_state(&sdir);
        self.resumed = existing.is_some();
        let mut state =
            existing.unwrap_or_else(|| UnboundState::new(&composegen::install_id(&sdir)));
        if self.resumed {
            if let Some(err) = state.last_error.as_deref() {
                self.line("info", format!("the last attempt stopped with: {err}"));
            }
        }
        // An INSTALL over an interrupted UNINSTALL starts FRESH: the disk is
        // half-reverted in ways `completed` no longer describes, so every
        // stage must re-run (and the backup must be retaken -- clearing
        // `completed` is what makes that happen). `prior_validate_skill` is
        // deliberately KEPT: it is still the best record of the pre-Unbound
        // value.
        if !self.uninstalling_mode && !state.uninstalling.is_empty() {
            self.line(
                "info",
                "an interrupted uninstall was found -- running every install stage fresh over it.",
            );
            state.completed.clear();
            state.uninstalling.clear();
            state.backup_file = None;
        }
        self.state = Some(state);
        self.server_dir = Some(sdir);
        Ok(())
    }

    fn do_guard(&mut self) -> Result<(), Fail> {
        let sdir = self.sdir().to_path_buf();

        // (a) An AzerothCore SOURCE checkout — the thing the core patch needs.
        // Deliberately NOT a remote-URL comparison: a migrated or forked server
        // is still patchable, and `git apply --check` is the real gate. What
        // cannot work is a tree without the files the patch rewrites.
        if !sdir.join(".git").exists()
            || !sdir.join("src/server/game/Entities/Player/Player.h").is_file()
        {
            return Err(Fail::new(
                CODE_NOT_AZEROTHCORE,
                format!(
                    "{} does not look like an AzerothCore source checkout.",
                    sdir.display()
                ),
                "Wrath Unbound patches the core engine, so the server dir must hold the AzerothCore sources it was built from.",
            ));
        }

        // (b) The stack must be RUNNING (the migrations go into its database),
        // and the container is resolved through THIS server's compose project.
        let (outcome, out) = self.run_collect(&self.docker_probe(
            vec!["compose".into(), "ps".into(), "-q".into(), "ac-database".into()],
            Some(sdir.clone()),
        ));
        let cid = if outcome.is_ok() { logsnap::parse_container_id(out.as_bytes()) } else { None };
        let Some(cid) = cid else {
            return Err(Fail::new(
                CODE_SERVER_NOT_RUNNING,
                "The server's database container is not running.",
                "Start the server first (Home page, or `dml-wow start`), then install the add-on. The migrations need a live database.",
            ));
        };
        self.db_container = Some(cid);

        // (c) Playerbots evidence: the 9 synthetic trainer IDs. This is the
        // bash's own gate, kept because the add-on's SQL assumes those rows.
        let (outcome, out) = self.run_collect(&self.mysql_probe("acore_world", TRAINER_PROBE_SQL));
        if !outcome.is_ok() {
            return Err(Fail::new(
                CODE_DB_UNREACHABLE,
                format!("The database did not answer the compatibility probe ({}).", outcome.detail()),
                "The container is up but mysql is not answering -- it may still be starting. Wait a minute and run this again.",
            ));
        }
        let (distinct, rows) = parse_trainer_counts(&out).unwrap_or((0, 0));
        if distinct < 9 || rows < 100 {
            return Err(Fail::new(
                CODE_NOT_PLAYERBOTS,
                format!(
                    "This server does not look like a DML WotLK Playerbots install (trainer probe: {distinct} of 9 IDs, {rows} rows)."
                ),
                "Wrath Unbound is built against the Playerbots trainer data. Install it onto the WotLK Playerbots title.",
            ));
        }

        // (d) Patch coherence across ALL SIX files. A mixed answer is a
        // half-applied patch and no automatic action is safe on top of it.
        if let PatchPresence::Mixed { missing } = probe_patch_presence(&sdir) {
            return Err(Fail::new(
                CODE_PATCH_INCOHERENT,
                format!(
                    "The core patch is HALF-applied: {} of 6 files are missing it ({}).",
                    missing.len(),
                    missing.join(", ")
                ),
                "Restore the six patched files to a clean state (git checkout -- <file>) and run this again.",
            ));
        }

        // (e) Already installed? The state file is the strong evidence; a
        // bash-scripted install left no state, so fall back to the canary
        // table + the module tree + the applied patch, ALL of which must
        // agree before this refuses. Partial evidence proceeds -- every
        // mutating stage is individually idempotent, and refusing on half an
        // install would strand exactly the users a resume exists for.
        let state_complete = self.state.as_ref().is_some_and(|s| s.complete());
        let state_version = self.state.as_ref().map(|s| s.addon_version.clone());
        if let Some(v) = state_version.as_deref() {
            if v != ADDON_VERSION {
                return Err(Fail::new(
                    CODE_VERSION_MISMATCH,
                    format!("Wrath Unbound {v} is on this server, and this build installs {ADDON_VERSION}."),
                    "Uninstall the old version first -- there is no in-place upgrade yet.",
                ));
            }
        }
        let installed = if state_complete {
            true
        } else if self.state.as_ref().is_some_and(|s| !s.completed.is_empty()) {
            false // a half-finished RESUME, never "already installed"
        } else {
            let canary =
                self.run_collect(&self.mysql_probe("acore_world", CANARY_SQL)).0.is_ok();
            let module = sdir.join(unbound_payload::MODULE_REL).join("src").is_dir();
            let patched = probe_patch_presence(&sdir) == PatchPresence::All;
            canary && module && patched
        };
        if installed {
            if self.opts.repair {
                self.line("info", "repair requested -- re-running every stage over the existing install.");
                let st = self.st();
                st.completed.clear();
                st.uninstalling.clear();
                st.backup_file = None;
                st.addon_version = ADDON_VERSION.to_string();
            } else {
                return Err(Fail::new(
                    CODE_ALREADY_INSTALLED,
                    "Wrath Unbound is already on this server.",
                    "Run with --repair to re-apply everything, or uninstall first.",
                ));
            }
        }

        // (g) The rebuild must be able to BUILD. On a composegen server the
        // build: sections live in docker-compose.build.yml, which compose
        // never auto-loads -- and `compose build <svc>` for a service with no
        // build section exits 0 with only a warning, so without this check
        // the "rebuild" finishes in seconds having compiled nothing and the
        // ready stage times out on an unmodified server (review CRITICAL,
        // 2026-08-02). A server running from imported images with no build
        // config at all cannot compile the module, and refusing HERE --
        // before the backup and the migrations -- is what keeps that from
        // being discovered after the database was already changed.
        self.resolve_build_files();
        let mut cfg_args: Vec<String> = vec!["compose".into()];
        cfg_args.extend(self.build_files.iter().cloned());
        cfg_args.extend(["config".into(), "--format".into(), "json".into()]);
        let (outcome, out) = self.run_collect(&self.docker_probe(cfg_args, Some(sdir.clone())));
        if outcome.is_ok() {
            match crate::buildcap::worldserver_has_build(&out) {
                Some(has_build) => {
                    if !has_build {
                        return Err(Fail::new(
                            CODE_NO_BUILD_CONFIG,
                            "This server has no build configuration for ac-worldserver -- it runs from prebuilt images, so mod-unbound cannot be compiled into it.",
                            "Wrath Unbound needs a server whose worldserver is built from source (a native install, or a WSL install with its compose build sections). A migrated image-only server cannot take the add-on.",
                        ));
                    }
                }
                None => self.line(
                    "warn",
                    "could not parse the compose configuration -- proceeding without the build-config check.",
                ),
            }
        } else {
            // Tri-state: a compose that cannot answer is evidence of nothing.
            self.line(
                "warn",
                "could not read the compose configuration -- proceeding without the build-config check.",
            );
        }

        // (f) Consent — the LAST guard, so everything above can refuse for a
        // better reason first. The list names what the bash's consent text
        // omitted; that omission is why this engine cannot inherit a "the
        // user already agreed once" shortcut.
        if !self.opts.accept_data_changes {
            return Err(Fail::new(
                CODE_CONSENT_REQUIRED,
                "The migrations change data other mods may also touch: they DELETE and re-seed \
                 playercreateinfo_spell_custom rows for 9 class masks (migrations 03/05), delete \
                 skillraceclassinfo_dbc rows with ID >= 10000 (06), and replace catalog rows \
                 (12/13/14).",
                "Re-run with --accept-data-changes after taking that in. A full safety backup is made first either way.",
            ));
        }
        Ok(())
    }

    /// Can this run skip its backup stage? Only when the state RECORDS one
    /// AND the recorded file is still on disk — the pool is shared and
    /// pruned to newest-N by every other dump on the box, so a completion
    /// token from two weeks ago routinely outlives its file (review finding,
    /// 2026-08-02). A recorded-done with a missing file re-runs the dump.
    fn backup_skippable(&self, recorded: bool) -> bool {
        if !recorded {
            return false;
        }
        match self.state.as_ref().and_then(|s| s.backup_file.as_deref()) {
            Some(f) => self.io.backup_exists(f),
            // Recorded done but no file name (a pre-fix state file): the
            // claim cannot be verified, so it is not trusted.
            None => false,
        }
    }

    fn take_backup(&mut self) -> Result<(), Fail> {
        let password = self.db_password.clone();
        let container = self.db_container.clone().expect("guard resolved the container");
        let mut lines: Vec<(String, String)> = Vec::new();
        let result =
            self.io.safety_backup(&container, &password, &mut |lvl, txt| lines.push((lvl, txt)));
        for (lvl, txt) in lines {
            self.line(&lvl, txt);
        }
        match result {
            Ok(file) => {
                self.line("info", format!("backup created: {file}"));
                self.backup_file = Some(file.clone());
                self.st().backup_file = Some(file);
                Ok(())
            }
            Err(tail) => Err(Fail::new(
                CODE_BACKUP_FAILED,
                if tail.is_empty() {
                    "The safety backup failed, so nothing was changed.".to_string()
                } else {
                    format!("The safety backup failed, so nothing was changed: {tail}")
                },
                "Nothing on the server was touched. Fix the backup problem (disk space is the usual one) and run this again.",
            )),
        }
    }

    fn do_backup(&mut self) -> Result<(), Fail> {
        let recorded = self.state.as_ref().is_some_and(|s| s.is_done(Stage::Backup));
        if self.backup_skippable(recorded) {
            self.backup_file = self.state.as_ref().and_then(|s| s.backup_file.clone());
            self.line("info", "this install's safety backup is still on disk -- not taking another.");
            return Ok(());
        }
        if recorded {
            self.line("warn", "the recorded safety backup is gone (the backup pool prunes to the newest few) -- taking a fresh one.");
        }
        self.take_backup()
    }

    fn do_stage_files(&mut self) -> Result<(), Fail> {
        let sdir = self.sdir().to_path_buf();
        for f in MANIFEST {
            let dest = sdir.join(f.dest);
            if let Some(parent) = dest.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    return Err(stage_write_fail(parent, &e));
                }
            }
            if let Err(e) = conf::atomic_write(&dest, f.body) {
                return Err(stage_write_fail(&dest, &e));
            }
        }
        // The pre-1.2.2 location. ALE scans one directory, but an upgrade
        // that leaves this copy behind can have BOTH registered when an old
        // override still points ALE at lua_scripts/ - two gossip handlers on
        // creature 900001. The bash installer had the same gap and only its
        // uninstaller cleaned it; removing it here is cheap and one-way.
        let legacy = sdir.join("lua_scripts/unbound_mentor.lua");
        if legacy.is_file() {
            match std::fs::remove_file(&legacy) {
                Ok(()) => self.line("info", "removed the pre-1.2.2 copy at lua_scripts/unbound_mentor.lua."),
                Err(e) => self.line("warn", format!("could not remove the legacy lua_scripts/unbound_mentor.lua: {e}")),
            }
        }
        self.line("info", format!("staged {} module files.", MANIFEST.len()));
        Ok(())
    }

    /// The mod-ale pin checkout -- shared by the first attempt and the
    /// fetch-rescue retry in the mismatch branch below.
    fn checkout_ale_pin(&self, ale: &Path) -> bool {
        let (co, _) = self.run_collect(&self.git_probe(vec![
            "-C".into(),
            ale.display().to_string(),
            "checkout".into(),
            "--quiet".into(),
            MOD_ALE_COMMIT.into(),
        ]));
        co.is_ok()
    }

    fn do_clone_ale(&mut self) -> Result<(), Fail> {
        let sdir = self.sdir().to_path_buf();
        let ale = sdir.join("modules/mod-ale");

        // Disk evidence decides, not the state file: CMakeLists.txt at the
        // module root is what makes this a REAL checkout. git populates .git
        // before the working tree, so an interrupted clone leaves a non-empty
        // dir that a bare existence check would call "done" forever.
        if ale.join("CMakeLists.txt").is_file() {
            // ...but a complete-looking checkout can still be the WRONG
            // commit: a previous run that cloned fine and then FAILED the pin
            // checkout left exactly this state, and skipping here would build
            // untested mod-ale HEAD under a 90-minute rebuild -- the bash
            // behaviour whose refusal was the point (review finding,
            // 2026-08-02). Verify the pin; re-pin if the answer is wrong; a
            // probe that cannot answer refuses nothing (tri-state).
            let (outcome, out) = self.run_collect(&self.git_probe(vec![
                "-C".into(),
                ale.display().to_string(),
                "rev-parse".into(),
                "HEAD".into(),
            ]));
            if outcome.is_ok() {
                let head = out.trim();
                if !head.starts_with(MOD_ALE_COMMIT) {
                    self.line("info", "mod-ale is present but not at the pinned commit -- re-pinning it.");
                    let mut co = self.checkout_ale_pin(&ale);
                    if !co {
                        // A Modules-page install is a --depth 1 clone: the
                        // pin can simply be absent locally (live 2026-08-09).
                        // Fetch exactly that commit, then retry, before
                        // concluding anything.
                        self.line(
                            "info",
                            "the pinned commit is not in the local clone (shallow module-page clone) -- fetching it...",
                        );
                        let fetch = Call {
                            program: Program::Git,
                            args: vec![
                                "-C".into(),
                                ale.display().to_string(),
                                "fetch".into(),
                                "origin".into(),
                                MOD_ALE_COMMIT.into(),
                            ],
                            cwd: None,
                            // git_probe's PROBE_TIMEOUT is for reads that
                            // already have their answer locally; a fetch
                            // reaches the network for one commit, so it gets
                            // the crate's git network bound instead (the
                            // GIT_NET_TIMEOUT convention in modmgr.rs).
                            timeout: Some(crate::modmgr::GIT_NET_TIMEOUT),
                        };
                        let (f, _) = self.run_collect(&fetch);
                        if f.is_ok() {
                            co = self.checkout_ale_pin(&ale);
                        }
                    }
                    if !co {
                        return Err(Fail::new(
                            CODE_ALE_PIN_MISMATCH,
                            format!("mod-ale is checked out at {head}, not the pinned commit, and re-pinning failed."),
                            "The add-on is tested against exactly that commit. A Modules-page install clones it shallowly, so the pin can simply be missing locally -- the automatic fetch of that commit just failed too. Check the network, or remove modules/mod-ale and run this again.",
                        ));
                    }
                }
            } else {
                self.line("warn", "could not verify mod-ale's pinned commit -- continuing with what is there.");
            }
            self.line("info", "mod-ale (the Lua engine) is already present -- skipping the clone.");
            return Ok(());
        }
        if ale.exists() {
            self.line("warn", "found an incomplete modules/mod-ale (no CMakeLists.txt) -- removing it and cloning fresh.");
            let _ = std::fs::remove_dir_all(&ale);
        }
        self.line("info", "cloning mod-ale (the Lua engine mod-unbound depends on)...");
        let clone = Call {
            program: Program::Git,
            args: vec!["clone".into(), MOD_ALE_URL.into(), ale.display().to_string()],
            cwd: None,
            timeout: None, // a clone is not a probe
        };
        let mut sink = |_l: &str| {};
        if !self.io.run(&clone, &mut sink).is_ok() {
            return Err(Fail::new(
                CODE_ALE_CLONE_FAILED,
                "Cloning mod-ale failed.",
                "Check the network and that modules/ is writable, then run this again.",
            ));
        }
        // The PIN is a refusal, not the bash's warn-and-build-HEAD: an
        // untested Lua engine under a 90-minute rebuild is not a maybe.
        let (outcome, _) = self.run_collect(&self.git_probe(vec![
            "-C".into(),
            ale.display().to_string(),
            "checkout".into(),
            "--quiet".into(),
            MOD_ALE_COMMIT.into(),
        ]));
        if !outcome.is_ok() {
            return Err(Fail::new(
                CODE_ALE_PIN_MISMATCH,
                format!("mod-ale cloned, but the pinned commit ({MOD_ALE_COMMIT}) could not be checked out."),
                "The add-on is tested against exactly that commit. Check the clone (git -C modules/mod-ale log) and run this again.",
            ));
        }
        if !ale.join("CMakeLists.txt").is_file() {
            return Err(Fail::new(
                CODE_ALE_CLONE_FAILED,
                "mod-ale was cloned but its working tree looks incomplete (no CMakeLists.txt).",
                "Usually disk space. Remove modules/mod-ale and run this again.",
            ));
        }
        self.line("info", "mod-ale staged at the pinned commit.");
        Ok(())
    }

    fn do_sql(&mut self) -> Result<(), Fail> {
        // The container id was resolved by the guard, but Backup and CloneAle
        // between there and here can take many minutes -- long enough for a
        // restart to have recreated ac-database under a new id. Re-resolving
        // is one bounded compose call; a stale id would make every exec fail
        // (or worse, hit a same-named container from another stack).
        self.resolve_db_container()?;
        if self.state.as_ref().is_some_and(|s| s.is_done(Stage::Sql)) {
            // Recorded means every file REALLY applied once -- but the
            // DATABASE outranks the bookkeeping: a restore-from-backup
            // between attempts rolls the migrations back while the state
            // still claims them, and resuming into a 90-minute rebuild whose
            // server then cannot come up helps nobody. The canary table
            // (migration 01's own artifact) is the cheap tiebreaker.
            let canary_ok =
                self.run_collect(&self.mysql_probe("acore_world", CANARY_SQL)).0.is_ok();
            if canary_ok {
                self.line("info", "migrations already applied by this install -- not re-running them.");
                return Ok(());
            }
            self.line("warn", "the state file claims the migrations ran, but the database disagrees (restored from a backup?) -- re-applying them.");
        }
        let backup_hint = self.backup_hint();
        for (dest, db) in SQL_ORDER {
            let file = unbound_payload::file(dest)
                .expect("SQL_ORDER entries are pinned to MANIFEST by test");
            let short = dest.rsplit('/').next().unwrap_or(dest);
            self.line("info", format!("applying {short} to {}...", db.database()));
            // The EMBEDDED bytes, not the staged file: same content (the
            // stage-files write is atomic and pinned), one less thing that can
            // have been edited in between.
            let call = self.mysql_apply_call(db.database());
            let mut out = String::new();
            let outcome = self.io.run_stdin(&call, file.body.as_bytes(), &mut |l| {
                out.push_str(l);
                out.push('\n');
            });
            if !outcome.is_ok() {
                return Err(Fail::new(
                    CODE_SQL_FAILED,
                    format!("{short} failed against {} ({}): {}", db.database(), outcome.detail(), stderr_tail(&out)),
                    backup_hint.clone(),
                ));
            }
        }
        self.line("info", format!("all {} migrations applied.", SQL_ORDER.len()));

        // THE SUMMONS MODULE'S SCRIPT ROWS, best-effort (v1.4.0).
        //
        // Not in SQL_ORDER because a failure here must not abort the install:
        // it registers spell_script_names for mod-multiclass-summons, and the
        // module's own file lives at data/sql/db-world/base/, the one module
        // path AzerothCore's DBUpdater applies itself. The bash pre-applies it
        // "so the rebuilt worldserver registers them on its next startup" and
        // treats a failure as a warning for that reason.
        //
        // It is NOT redundant, though, and the reason is worth stating: the
        // rebuild ends in `up -d --force-recreate ac-worldserver`, which
        // recreates ONLY the worldserver -- `ac-db-import` does not re-run. So
        // on the path this engine actually takes, the pre-apply may be the only
        // thing that puts these rows in before the world caches the table, and
        // the auto-apply is the backstop rather than the mechanism. Both, then:
        // try it, and say plainly when it did not take.
        let (dest, db) = (unbound_payload::SUMMONS_SQL_DEST, unbound_payload::SqlDb::World);
        if let Some(f) = unbound_payload::file(dest) {
            let call = self.mysql_apply_call(db.database());
            let mut out = String::new();
            let outcome = self.io.run_stdin(&call, f.body.as_bytes(), &mut |l| {
                out.push_str(l);
                out.push('\n');
            });
            if outcome.is_ok() {
                self.line("info", "multi-class summons spell scripts registered.");
            } else {
                self.line(
                    "warn",
                    format!(
                        "could not pre-register the multi-class summons spell scripts ({}): {} -- AzerothCore should apply them itself on the next full stack start.",
                        outcome.detail(),
                        stderr_tail(&out)
                    ),
                );
            }
        }
        Ok(())
    }

    fn mysql_apply_call(&self, db: &str) -> Call {
        let cid = self.db_container.clone().expect("guard resolved the container");
        Call {
            program: Program::Docker,
            args: vec![
                "exec".into(),
                "-i".into(),
                cid,
                "mysql".into(),
                "-uroot".into(),
                format!("-p{}", self.db_password),
                db.into(),
            ],
            cwd: None,
            timeout: Some(crate::modmgr::SQL_APPLY_TIMEOUT),
        }
    }

    fn backup_hint(&self) -> String {
        match &self.backup_file {
            Some(f) => format!(
                "Your databases were backed up first ({f} under ~/.dml/backups) -- restore from there if you need to roll back."
            ),
            None => "Your databases were backed up before this run's first change (see ~/.dml/backups).".to_string(),
        }
    }

    fn do_core_patch(&mut self) -> Result<(), Fail> {
        let sdir = self.sdir().to_path_buf();

        // Disk evidence first — the state file may be stale, and a tree that
        // already carries all six symbols must not be patched twice.
        match probe_patch_presence(&sdir) {
            PatchPresence::All => {
                self.line("info", "the core patch is already applied -- skipping.");
                return Ok(());
            }
            PatchPresence::Mixed { missing } => {
                // The guard checked this, but stages between them wrote files;
                // paranoia here is one cheap read of six files.
                return Err(Fail::new(
                    CODE_PATCH_INCOHERENT,
                    format!("The core patch is HALF-applied ({} files missing it).", missing.len()),
                    "Restore the six patched files to a clean state (git checkout -- <file>) and run this again.",
                ));
            }
            PatchPresence::None => {}
        }

        // `git -C <dir>` makes relative args resolve inside <dir>, so the
        // patch path is the staged payload file, relative to the server dir.
        let check = self.git_probe(vec![
            "-C".into(),
            sdir.display().to_string(),
            "apply".into(),
            "--check".into(),
            unbound_payload::PATCH_DEST.into(),
        ]);
        let (outcome, out) = self.run_collect(&check);
        if !outcome.is_ok() {
            return Err(Fail::new(
                CODE_PATCH_CHECK_FAILED,
                format!("The core patch does not apply cleanly to this tree: {}", stderr_tail(&out)),
                format!(
                    "The six files it changes ({}) have local edits or come from a different AzerothCore revision. Restore them (git checkout -- <file>) and run this again.",
                    PATCHED_FILES.join(", ")
                ),
            ));
        }
        let apply = self.git_probe(vec![
            "-C".into(),
            sdir.display().to_string(),
            "apply".into(),
            unbound_payload::PATCH_DEST.into(),
        ]);
        let (outcome, out) = self.run_collect(&apply);
        if !outcome.is_ok() {
            // --check just passed, so this is rare — but the bash NEVER
            // checked this exit status at all, and that is the bug class this
            // port exists to close.
            return Err(Fail::new(
                CODE_PATCH_CHECK_FAILED,
                format!("git apply failed after a clean --check: {}", stderr_tail(&out)),
                "The tree changed between the check and the apply. Run this again.",
            ));
        }
        if probe_patch_presence(&sdir) != PatchPresence::All {
            return Err(Fail::new(
                CODE_PATCH_CHECK_FAILED,
                "git apply reported success but the six files do not all carry the change.",
                "This should not happen -- inspect the six patched files by hand.",
            ));
        }
        self.line("info", "core patch applied to all six files.");
        Ok(())
    }

    fn do_conf(&mut self) -> Result<(), Fail> {
        let sdir = self.sdir().to_path_buf();

        // worldserver.conf — the value without which the server WIPES the
        // add-on's purchases on every login. A missing conf is a hard stop
        // here, not the bash's warn-then-banner: continuing would build a
        // server that destroys player progress silently.
        let ws = sdir.join("env/dist/etc/worldserver.conf");
        match conf::conf_ensure(&ws) {
            Ok(true) => {}
            Ok(false) => {
                return Err(Fail::new(
                    CODE_CONF_MISSING,
                    format!("worldserver.conf was not found at {} (nor its .dist to seed from).", ws.display()),
                    "Without ValidateSkillLearnedBySpells = 0 the server strips cross-class spells on every login. Locate your worldserver.conf and run this again.",
                ));
            }
            Err(e) => return Err(stage_write_fail(&ws, &e)),
        }
        // Record what we are about to replace, ONCE — uninstall restores this
        // instead of guessing the AzerothCore default was in force.
        //
        // NEVER record "0" as the prior (review finding, 2026-08-02): a "0"
        // found here is far more likely the ADD-ON's own earlier write (a
        // bash-scripted install being taken over, a state file invalidated by
        // a dir move) than a user's deliberate pre-Unbound choice — and
        // recording it makes uninstall "restore" the add-on's value while
        // claiming it restored the user's. Left unrecorded, uninstall
        // restores the stock default and DECLARES the guess in residue.
        if self.st().prior_validate_skill.is_none() {
            let current = std::fs::read_to_string(&ws)
                .map(|t| conf::parse_conf(&t))
                .ok()
                .and_then(|m| m.get(VALIDATE_KEY).map(|v| conf::strip_conf_quotes(v)));
            match current.as_deref() {
                Some("0") => self.line(
                    "info",
                    format!("{VALIDATE_KEY} is already 0 (likely from an earlier Unbound install) -- an uninstall will restore the stock default."),
                ),
                Some(v) => {
                    self.st().prior_validate_skill = Some(v.to_string());
                    self.persist();
                }
                None => {
                    self.st().prior_validate_skill = Some("1".to_string());
                    self.persist();
                }
            }
        }
        backup_sibling_once(&ws);
        match conf::conf_write(&ws, VALIDATE_KEY, "0") {
            Ok(true) => self.line("info", format!("{VALIDATE_KEY} = 0 set in worldserver.conf.")),
            Ok(false) => self.line("info", format!("{VALIDATE_KEY} is already 0.")),
            Err(e) => return Err(stage_write_fail(&ws, &e)),
        }

        // mod_ale.conf — fresh file gets the full default set; an existing one
        // gets ONLY the two keys the add-on requires, because the other seven
        // are user-tunable and repairing a conf must not undo their choices.
        let mdir = sdir.join("env/dist/etc/modules");
        if let Err(e) = std::fs::create_dir_all(&mdir) {
            return Err(stage_write_fail(&mdir, &e));
        }
        let ale = mdir.join("mod_ale.conf");
        let fresh = !ale.exists();
        if !fresh {
            backup_sibling_once(&ale);
        }
        let keys: &[(&str, &str)] =
            if fresh { &ALE_CONF_DEFAULTS } else { &ALE_CONF_DEFAULTS[..ALE_CONF_REQUIRED] };
        for (key, value) in keys {
            if let Err(e) = conf::conf_write(&ale, key, value) {
                return Err(stage_write_fail(&ale, &e));
            }
        }
        self.line(
            "info",
            if fresh {
                "mod_ale.conf created (ALE enabled, script path set)."
            } else {
                "mod_ale.conf checked (ALE enabled, script path set; your other settings kept)."
            },
        );
        Ok(())
    }

    fn do_verify(&mut self) -> Result<(), Fail> {
        let sdir = self.sdir().to_path_buf();
        let mut wrong: Vec<String> = Vec::new();

        let lua = sdir.join("env/dist/etc/modules/lua_scripts/unbound_mentor.lua");
        if !lua.is_file() {
            wrong.push(format!("unbound_mentor.lua is missing at {}", lua.display()));
        }
        // Read-back goes through the same unquoting the conf engine's writer
        // uses (`strip_conf_quotes`), so the check and the write cannot
        // disagree about a value that differs only in quoting.
        let read_key = |path: &Path, key: &str| -> Option<String> {
            std::fs::read_to_string(path)
                .ok()
                .and_then(|t| conf::parse_conf(&t).get(key).cloned())
                .map(|v| conf::strip_conf_quotes(&v))
        };
        let ale = sdir.join("env/dist/etc/modules/mod_ale.conf");
        if read_key(&ale, "ALE.Enabled").as_deref() != Some("1") {
            wrong.push("mod_ale.conf: ALE.Enabled is not 1".to_string());
        }
        if read_key(&ale, "ALE.ScriptPath").as_deref() != Some(ALE_SCRIPT_PATH) {
            wrong.push(format!("mod_ale.conf: ALE.ScriptPath is not \"{ALE_SCRIPT_PATH}\""));
        }
        let ws = sdir.join("env/dist/etc/worldserver.conf");
        if read_key(&ws, VALIDATE_KEY).as_deref() != Some("0") {
            wrong.push(format!("worldserver.conf: {VALIDATE_KEY} is not 0"));
        }

        if !wrong.is_empty() {
            // The bash asked "Continue anyway?" here, in front of a 90-minute
            // build whose product would not work. This refuses instead.
            return Err(Fail::new(
                CODE_VERIFY_FAILED,
                format!("The staged configuration does not check out: {}", wrong.join("; ")),
                "Fix the listed items (or re-run, which re-stages them) before the rebuild.",
            ));
        }
        self.line("info", "configuration verified -- the Mentor's Lua will load on the rebuilt server.");
        Ok(())
    }

    fn do_build(&mut self) -> Result<(), Fail> {
        let sdir = self.sdir().to_path_buf();
        let log_dir = sdir.join("logs");
        let log_path = log_dir.join(build_log_name(now_unix()));
        // Neutral wording on purpose: this stage serves BOTH directions -- the
        // install compiles mod-unbound in, the uninstall compiles it back out.
        self.line("info", "rebuilding the worldserver from the current module set (30-90 minutes)...");
        self.line("info", format!("build log: {}", log_path.display()));

        // A fresh BuildProgress per attempt — a resumed build's bar starts
        // over BY CONSTRUCTION rather than continuing a dead run's number.
        let mut progress = BuildProgress::default();
        // The -f set the guard resolved: a composegen server keeps build:
        // in docker-compose.build.yml, which compose never auto-loads — a
        // bare `compose build` there builds NOTHING and exits 0.
        self.resolve_build_files();
        let mut args: Vec<String> = vec!["compose".into()];
        args.extend(self.build_files.iter().cloned());
        args.extend(["build".into(), "ac-worldserver".into()]);
        let call = self.docker_unbounded(args, Some(sdir.clone()));

        use std::io::Write;
        let _ = std::fs::create_dir_all(&log_dir);
        let mut log = std::fs::File::create(&log_path).ok();
        let log_landed = log.is_some();
        let emit = self.emit;
        let outcome = self.io.run(&call, &mut |l| {
            if let Some(f) = log.as_mut() {
                let _ = writeln!(f, "{l}");
            }
            if let Some(pct) = progress.observe(l) {
                emit(pct_event(pct));
            }
            emit(line_event("info", l));
        });
        if !outcome.is_ok() {
            // Only name the log file when it actually exists — a hint
            // pointing at a file that could not be created sends the user
            // hunting for evidence that is not there.
            let where_ = if log_landed {
                format!("The full build log is at {}.", log_path.display())
            } else {
                "The full build output is in the terminal above (the log file could not be created).".to_string()
            };
            return Err(Fail::new(
                CODE_BUILD_FAILED,
                format!("The worldserver rebuild failed ({}).", outcome.detail()),
                format!("{where_} {}", self.backup_hint()),
            ));
        }
        Ok(())
    }

    fn do_up(&mut self) -> Result<(), Fail> {
        let sdir = self.sdir().to_path_buf();
        self.line("info", "recreating the worldserver container with the new binary...");
        let call = self.docker_unbounded(
            vec![
                "compose".into(),
                "up".into(),
                "-d".into(),
                "--force-recreate".into(),
                "ac-worldserver".into(),
            ],
            Some(sdir.clone()),
        );
        let mut out = String::new();
        let outcome = self.io.run(&call, &mut |l| {
            out.push_str(l);
            out.push('\n');
        });
        if !outcome.is_ok() {
            return Err(Fail::new(
                CODE_UP_FAILED,
                format!("Recreating ac-worldserver failed ({}): {}", outcome.detail(), stderr_tail(&out)),
                "The old container may still be present. Check `docker compose ps` in the server dir.",
            ));
        }
        Ok(())
    }

    fn do_ready(&mut self) -> Result<(), Fail> {
        let sdir = self.sdir().to_path_buf();
        self.line("info", "waiting for the rebuilt world server (and the Mentor's Lua) to come up...");
        let started = Instant::now();
        let deadline = started + self.opts.ready_timeout;
        let mut watch = lifecycle::BootLoopWatch::new();
        let mut inspect_warned = false;
        let mut polls: u64 = 0;

        loop {
            polls += 1;
            let (ps_outcome, ps_out) = self.run_collect(&self.docker_probe(
                logsnap::world_container_argv().into_iter().map(String::from).collect(),
                Some(sdir.clone()),
            ));
            let container =
                if ps_outcome.is_ok() { logsnap::parse_container_id(ps_out.as_bytes()) } else { None };

            if let Some(cid) = container {
                let (rc_outcome, rc_out) = self.run_collect(&self.docker_probe(
                    vec![
                        "inspect".into(),
                        "-f".into(),
                        crate::install_native::READY_INSPECT_FORMAT.into(),
                        cid.clone(),
                    ],
                    None,
                ));
                let (started_at, reading) = if rc_outcome.is_ok() {
                    crate::install_native::parse_started_and_restarts(&rc_out)
                } else {
                    // Say it once. A failed inspect sends the log read to a
                    // fixed tail, and this engine's marker
                    // (`[UNBOUND] Prereq map built.`) prints ONCE, early --
                    // 500 bots logging in push it out of the window in
                    // seconds. Silence here cost a full readiness timeout on a
                    // server that was already up (2026-08-03).
                    if !inspect_warned {
                        inspect_warned = true;
                        self.line(
                            "warn",
                            format!(
                                "could not read the container's start time ({}) -- falling back to a log tail, which can miss the ready marker on a busy boot.",
                                rc_outcome.detail()
                            ),
                        );
                    }
                    (None, None)
                };

                // `--since <StartedAt>` — never the whole log. The marker from
                // a PREVIOUS boot (this server may have run Unbound before a
                // --repair) must not count as this boot's success; that stale
                // read was one of the bash's recorded false positives.
                let log_args = match started_at.as_deref() {
                    Some(s) => vec!["logs".to_string(), "--since".into(), s.into(), cid.clone()],
                    None => vec![
                        "logs".to_string(),
                        "--tail".into(),
                        lifecycle::BOOT_LOOP_CAUSE_TAIL_LINES.to_string(),
                        cid.clone(),
                    ],
                };
                let (log_outcome, logs) = self.run_collect(&self.docker_probe(log_args, None));
                if log_outcome.is_ok() && logs.contains(READY_MARKER) {
                    self.line("info", "the Mentor is up -- Wrath Unbound is live.");
                    return Ok(());
                }
                if let Some(new_restarts) = watch.observe(reading) {
                    let mysql = crate::status::mysql_connect_failures(&logs)
                        >= lifecycle::BOOT_LOOP_MYSQL_HITS_MIN;
                    self.line("warn", lifecycle::boot_loop_note(new_restarts, mysql));
                }
            }

            let now = Instant::now();
            if now >= deadline {
                break;
            }
            std::thread::sleep(self.opts.ready_poll.min(deadline.saturating_duration_since(now)));
        }

        let _ = polls; // observed by tests through the recorded calls
        Err(Fail::new(
            CODE_READY_TIMEOUT,
            format!(
                "The rebuilt server did not print \"{READY_MARKER}\" within {} minutes.",
                self.opts.ready_timeout.as_secs() / 60
            ),
            "The containers are still running -- check the worldserver log. If the marker never appears, the Mentor's Lua did not load; the usual cause is ALE not compiled in (was mod-ale staged?).",
        ))
    }

    /// Put the CLIENT addons in place, if we know where the client is.
    ///
    /// Runs AFTER `ready`, and NEVER fails the install: a 30–90 minute rebuild
    /// that worked must not be reported as failed because a file copy into a
    /// different application's folder did not. What it returns is recorded in
    /// the done event so "installed" is not ambiguous about which half
    /// happened.
    ///
    /// The talent system is why this is automatic at all: the server-side
    /// bridge only validates picks the client addon SENDS, so a server with the
    /// bridge and no addon has a feature the player cannot see, with nothing
    /// in-game explaining the absence.
    fn install_client_addons(&mut self) -> Value {
        use crate::unbound_addons;
        let cp = crate::clientpath::read_client_path();
        let path = cp.get("path").and_then(|v| v.as_str()).unwrap_or_default();
        let valid = cp.get("valid").and_then(|v| v.as_bool()).unwrap_or(false);
        if path.is_empty() {
            self.line("warn", "no WoW client folder is configured, so the Wrath Unbound addons were NOT installed -- set it on the Settings page, then use 'Install addons' on the Tools card.");
            return serde_json::json!({"installed": false, "reason": "no_client_path"});
        }
        if !valid {
            self.line("warn", format!("the configured WoW client folder ({path}) does not look like a client, so the addons were NOT installed."));
            return serde_json::json!({"installed": false, "reason": "client_path_invalid", "path": path});
        }
        match unbound_addons::install_addons(std::path::Path::new(path)) {
            Ok(done) => {
                self.line(
                    "info",
                    format!("installed {} client addon files into {}", done.files, done.addons_dir),
                );
                serde_json::json!({
                    "installed": true,
                    "files": done.files,
                    "addons_dir": done.addons_dir,
                    "addons": done.addons,
                })
            }
            Err(e) => {
                self.line("warn", format!("could not install the client addons: {e}"));
                serde_json::json!({"installed": false, "reason": "write_failed", "detail": e})
            }
        }
    }

    fn run_stage(&mut self, stage: Stage) -> Result<(), Fail> {
        (self.emit)(match stage {
            Stage::Ready => section_start_limited(stage.name(), self.opts.ready_timeout.as_secs()),
            _ => section_start(stage.name()),
        });
        let result = match stage {
            Stage::Preflight => self.do_preflight(),
            Stage::Locate => self.do_locate(),
            Stage::Guard => self.do_guard(),
            Stage::Backup => self.do_backup(),
            Stage::StageFiles => self.do_stage_files(),
            Stage::CloneAle => self.do_clone_ale(),
            Stage::Sql => self.do_sql(),
            Stage::CorePatch => self.do_core_patch(),
            Stage::Conf => self.do_conf(),
            Stage::Verify => self.do_verify(),
            Stage::Build => self.do_build(),
            Stage::Up => self.do_up(),
            Stage::Ready => self.do_ready(),
        };
        match &result {
            Ok(()) => {
                (self.emit)(section_end(stage.name(), "ok"));
                // ONLY here, and only after the stage really finished.
                if stage.records_completion() {
                    self.st().mark(stage);
                    self.persist();
                }
            }
            Err(_) => (self.emit)(section_end(stage.name(), "error")),
        }
        result
    }

    fn go(&mut self) -> Result<(), Fail> {
        for stage in STAGE_ORDER {
            self.run_stage(stage)?;
        }
        Ok(())
    }

    // -- uninstall stages ----------------------------------------------------

    /// Resolve the running ac-database container through THIS server's
    /// compose project — shared by both guards.
    fn resolve_db_container(&mut self) -> Result<(), Fail> {
        let sdir = self.sdir().to_path_buf();
        let (outcome, out) = self.run_collect(&self.docker_probe(
            vec!["compose".into(), "ps".into(), "-q".into(), "ac-database".into()],
            Some(sdir),
        ));
        let cid = if outcome.is_ok() { logsnap::parse_container_id(out.as_bytes()) } else { None };
        match cid {
            Some(cid) => {
                self.db_container = Some(cid);
                Ok(())
            }
            None => Err(Fail::new(
                CODE_SERVER_NOT_RUNNING,
                "The server's database container is not running.",
                "Start the server first (Home page, or `dml-wow start`) -- the database revert needs a live database.",
            )),
        }
    }

    fn do_un_guard(&mut self) -> Result<(), Fail> {
        let sdir = self.sdir().to_path_buf();
        self.resolve_db_container()?;

        // Evidence of an install, from FOUR independent signals. Any one is
        // enough to proceed (a half-install still wants cleaning up); NONE
        // means there is nothing here to remove, and running the revert
        // anyway is a --force decision, not a default.
        // An interrupted uninstall is itself evidence: its own progress list
        // says the add-on WAS here, even after remove-files deleted the tree
        // and sql-revert dropped the canary.
        let state_evidence = self.resumed
            || self.state.as_ref().is_some_and(|s| !s.uninstalling.is_empty());
        let canary = self.run_collect(&self.mysql_probe("acore_world", CANARY_SQL)).0.is_ok();
        let module = sdir.join(MODULE_REL).exists();
        let patched = probe_patch_presence(&sdir) != PatchPresence::None;
        if !(state_evidence || canary || module || patched) && !self.opts.force {
            return Err(Fail::new(
                CODE_NOT_INSTALLED,
                "No trace of Wrath Unbound was found on this server (no state file, no unbound tables, no module tree, no core patch).",
                "Nothing to uninstall. --force runs the revert anyway, but it is NOT a no-op: two of its statements delete rows in tables other mods share (playercreateinfo_spell_custom, and skillraceclassinfo_dbc ID >= 10000). Only use it if you know the add-on was here.",
            ));
        }

        if !self.opts.accept_data_changes {
            return Err(Fail::new(
                CODE_CONSENT_REQUIRED,
                "Uninstalling DROPS unbound_character_unlocks -- every character's class-unlock \
                 progression -- and runs the add-on's own wide revert deletes \
                 (playercreateinfo_spell_custom for 9 class masks, skillraceclassinfo_dbc ID >= 10000).",
                "Re-run with --accept-data-changes after taking that in. A full safety backup is made first either way.",
            ));
        }
        Ok(())
    }

    fn do_un_backup(&mut self) -> Result<(), Fail> {
        let recorded =
            self.state.as_ref().is_some_and(|s| s.is_undone(UninstallStage::Backup));
        if self.backup_skippable(recorded) {
            self.backup_file = self.state.as_ref().and_then(|s| s.backup_file.clone());
            self.line("info", "this uninstall's safety backup is still on disk -- not taking another.");
            return Ok(());
        }
        if recorded {
            self.line("warn", "the recorded safety backup is gone (the backup pool prunes to the newest few) -- taking a fresh one.");
        }
        self.take_backup()
    }

    fn do_sql_revert(&mut self) -> Result<(), Fail> {
        // Same stale-id hazard as do_sql: the multi-minute backup ran since
        // the guard resolved the container.
        self.resolve_db_container()?;
        // Per-statement, warn-and-continue — the bash's deliberate choice,
        // kept: half a cleanup beats stranding the user mid-revert. What the
        // bash then FORGOT is the other half of the deal: every failure here
        // lands in `residue`, so the done event still tells the whole truth.
        //
        // ONE ordering exception: statement 0 deletes creature_addon rows
        // through a subquery anchored on the creature rows statement 1 then
        // deletes. If 0 FAILS and 1 succeeds, the addon rows are orphaned
        // with their anchor gone — unremovable by any re-run, including
        // --force. So a failure of 0 SKIPS 1, keeping the anchor alive for
        // the retry, and both land in residue.
        let mut skip_next = false;
        for (i, (db, sql, label)) in REVERT_SQL.iter().enumerate() {
            if skip_next {
                skip_next = false;
                self.line("warn", format!("{label} -- SKIPPED (its companion delete failed; re-run uninstall to retry both)"));
                self.residue.push(format!("database revert skipped: {label} (companion failed)"));
                continue;
            }
            let call = self.mysql_stmt_call(db, sql);
            let (outcome, out) = self.run_collect(&call);
            if outcome.is_ok() {
                self.line("info", label.to_string());
            } else {
                let detail = stderr_tail(&out);
                self.line("warn", format!("{label} -- FAILED: {detail}"));
                self.residue.push(format!("database revert failed: {label} ({detail})"));
                if i == 0 {
                    skip_next = true;
                }
            }
        }
        Ok(())
    }

    fn do_remove_files(&mut self) -> Result<(), Fail> {
        let sdir = self.sdir().to_path_buf();
        let module = sdir.join(MODULE_REL);
        if module.exists() {
            if let Err(e) = std::fs::remove_dir_all(&module) {
                // A locked file (an editor holding UnboundSystem.cpp open) is
                // recoverable by hand; the rest of the uninstall is not made
                // better by aborting over it.
                self.line("warn", format!("could not remove {}: {e}", module.display()));
                self.residue.push(format!("modules/mod-unbound left on disk ({e})"));
            } else {
                self.line("info", "modules/mod-unbound removed.");
            }
        }
        for rel in
            ["env/dist/etc/modules/lua_scripts/unbound_mentor.lua", "lua_scripts/unbound_mentor.lua"]
        {
            let p = sdir.join(rel);
            if p.is_file() {
                match std::fs::remove_file(&p) {
                    Ok(()) => self.line("info", format!("{rel} removed.")),
                    Err(e) => {
                        self.line("warn", format!("could not remove {rel}: {e}"));
                        self.residue.push(format!("{rel} left on disk ({e})"));
                    }
                }
            }
        }
        // Deliberate residue, named every time: the Lua ENGINE stays. Other
        // ALE scripts may use it, and an unused engine is harmless; removing
        // a shared component during an uninstall of one consumer is how other
        // people's mods break.
        if sdir.join("modules/mod-ale").exists() {
            self.residue.push(
                "modules/mod-ale left in place (shared Lua engine, harmless without the Mentor script)"
                    .to_string(),
            );
        }
        if sdir.join("env/dist/etc/modules/mod_ale.conf").exists() {
            self.residue
                .push("mod_ale.conf left in place (shared with any other ALE scripts)".to_string());
        }
        Ok(())
    }

    fn do_patch_revert(&mut self) -> Result<(), Fail> {
        let sdir = self.sdir().to_path_buf();
        match probe_patch_presence(&sdir) {
            PatchPresence::None => {
                self.line("info", "the core patch is not applied -- nothing to revert.");
                return Ok(());
            }
            PatchPresence::Mixed { missing } => {
                self.line(
                    "warn",
                    format!(
                        "the core patch is HALF-applied ({} of 6 files) -- leaving it; a reverse-apply cannot succeed on a mixed tree.",
                        6 - missing.len()
                    ),
                );
                self.residue.push(
                    "core patch left half-applied (restore the six files with git checkout -- <file>)"
                        .to_string(),
                );
                return Ok(());
            }
            PatchPresence::All => {}
        }

        // remove-files already deleted the staged patch, so the reverse apply
        // uses a temp copy of the EMBEDDED bytes — the uninstall must never
        // depend on a file it just removed (the bash uninstaller carries its
        // own copy for exactly this reason).
        let tmp = sdir.join(".dml-unbound-revert.patch");
        let body = unbound_payload::file(unbound_payload::PATCH_DEST)
            .expect("patch is pinned into MANIFEST")
            .body;
        if let Err(e) = conf::atomic_write(&tmp, body) {
            self.residue.push(format!("core patch left applied (could not stage revert file: {e})"));
            return Ok(());
        }
        let check = self.git_probe(vec![
            "-C".into(),
            sdir.display().to_string(),
            "apply".into(),
            "-R".into(),
            "--check".into(),
            ".dml-unbound-revert.patch".into(),
        ]);
        let (outcome, _) = self.run_collect(&check);
        if !outcome.is_ok() {
            // The bash got this one RIGHT and it is kept verbatim: a tree
            // that has diverged cannot be force-reverted, and the patch is
            // inert without mod-unbound (m_unboundClassMask just stays 0).
            let _ = std::fs::remove_file(&tmp);
            self.line("warn", "the core patch cannot be cleanly reverted (the source tree has diverged) -- leaving it; it is inert without mod-unbound.");
            self.residue.push("core patch left applied (inert without mod-unbound)".to_string());
            return Ok(());
        }
        let apply = self.git_probe(vec![
            "-C".into(),
            sdir.display().to_string(),
            "apply".into(),
            "-R".into(),
            ".dml-unbound-revert.patch".into(),
        ]);
        let (outcome, out) = self.run_collect(&apply);
        let _ = std::fs::remove_file(&tmp);
        if !outcome.is_ok() {
            self.line("warn", format!("reverse-apply failed after a clean check: {}", stderr_tail(&out)));
            self.residue.push("core patch left applied (reverse-apply failed)".to_string());
            return Ok(());
        }
        if probe_patch_presence(&sdir) != PatchPresence::None {
            self.residue
                .push("core patch symbols still present after the reverse-apply".to_string());
        } else {
            self.line("info", "core patch reverted across all six files.");
        }
        Ok(())
    }

    fn do_conf_revert(&mut self) -> Result<(), Fail> {
        let sdir = self.sdir().to_path_buf();
        let ws = sdir.join("env/dist/etc/worldserver.conf");
        // Restore what the INSTALL recorded, not a hardcoded 1: a user who ran
        // with 0 before Unbound existed gets their 0 back. The bash restored a
        // literal 1 no matter what — residue in its purest form.
        let prior = self
            .state
            .as_ref()
            .and_then(|s| s.prior_validate_skill.clone())
            .unwrap_or_else(|| "1".to_string());
        if ws.is_file() {
            match conf::conf_write(&ws, VALIDATE_KEY, &prior) {
                Ok(true) => self.line("info", format!("{VALIDATE_KEY} restored to {prior}.")),
                Ok(false) => self.line("info", format!("{VALIDATE_KEY} already {prior}.")),
                Err(e) => {
                    self.line("warn", format!("could not restore {VALIDATE_KEY}: {e}"));
                    self.residue.push(format!("{VALIDATE_KEY} left at 0 ({e})"));
                }
            }
        } else {
            self.residue.push(format!("{VALIDATE_KEY} not restored (worldserver.conf missing)"));
        }
        if self.state.as_ref().and_then(|s| s.prior_validate_skill.as_deref()).is_none() {
            self.residue.push(format!(
                "{VALIDATE_KEY} restored to the stock default ({prior}) -- the install that set it left no record of the prior value"
            ));
        }
        Ok(())
    }

    fn do_un_ready(&mut self) -> Result<(), Fail> {
        let sdir = self.sdir().to_path_buf();
        self.line("info", "waiting for the stock world server to come back up...");
        let started = Instant::now();
        let deadline = started + self.opts.ready_timeout;
        let mut watch = lifecycle::BootLoopWatch::new();
        let mut inspect_warned = false;

        loop {
            let (ps_outcome, ps_out) = self.run_collect(&self.docker_probe(
                logsnap::world_container_argv().into_iter().map(String::from).collect(),
                Some(sdir.clone()),
            ));
            let container =
                if ps_outcome.is_ok() { logsnap::parse_container_id(ps_out.as_bytes()) } else { None };

            if let Some(cid) = container {
                let (rc_outcome, rc_out) = self.run_collect(&self.docker_probe(
                    vec![
                        "inspect".into(),
                        "-f".into(),
                        crate::install_native::READY_INSPECT_FORMAT.into(),
                        cid.clone(),
                    ],
                    None,
                ));
                let (started_at, reading) = if rc_outcome.is_ok() {
                    crate::install_native::parse_started_and_restarts(&rc_out)
                } else {
                    // Say it once. A failed inspect sends the log read to a
                    // fixed tail, and this engine's marker
                    // (`[UNBOUND] Prereq map built.`) prints ONCE, early --
                    // 500 bots logging in push it out of the window in
                    // seconds. Silence here cost a full readiness timeout on a
                    // server that was already up (2026-08-03).
                    if !inspect_warned {
                        inspect_warned = true;
                        self.line(
                            "warn",
                            format!(
                                "could not read the container's start time ({}) -- falling back to a log tail, which can miss the ready marker on a busy boot.",
                                rc_outcome.detail()
                            ),
                        );
                    }
                    (None, None)
                };
                let log_args = match started_at.as_deref() {
                    Some(s) => vec!["logs".to_string(), "--since".into(), s.into(), cid.clone()],
                    None => vec![
                        "logs".to_string(),
                        "--tail".into(),
                        lifecycle::BOOT_LOOP_CAUSE_TAIL_LINES.to_string(),
                        cid.clone(),
                    ],
                };
                let (log_outcome, logs) = self.run_collect(&self.docker_probe(log_args, None));
                // INVERTED AND DOUBLED: the world must be ready AND the
                // Unbound marker must be absent from THIS boot. A crash-looping
                // server fails the first; a build that quietly kept the module
                // fails the second. Either alone can look like a clean
                // uninstall while being nothing of the kind.
                if log_outcome.is_ok() && crate::status::world_ready_from_logs(&logs) {
                    if logs.contains(READY_MARKER) {
                        self.line("warn", "the rebuilt server STILL prints the Unbound marker -- the module is still compiled in.");
                        self.residue.push(
                            "the rebuilt worldserver still loads Unbound (marker present after rebuild)"
                                .to_string(),
                        );
                    } else {
                        self.line("info", "the stock world server is up, without the add-on.");
                    }
                    return Ok(());
                }
                if let Some(new_restarts) = watch.observe(reading) {
                    let mysql = crate::status::mysql_connect_failures(&logs)
                        >= lifecycle::BOOT_LOOP_MYSQL_HITS_MIN;
                    self.line("warn", lifecycle::boot_loop_note(new_restarts, mysql));
                }
            }

            let now = Instant::now();
            if now >= deadline {
                break;
            }
            std::thread::sleep(self.opts.ready_poll.min(deadline.saturating_duration_since(now)));
        }
        Err(Fail::new(
            CODE_READY_TIMEOUT,
            format!(
                "The rebuilt server did not report ready within {} minutes.",
                self.opts.ready_timeout.as_secs() / 60
            ),
            "The containers are still running -- check the worldserver log to see where it stopped.",
        ))
    }

    fn run_un_stage(&mut self, stage: UninstallStage) -> Result<(), Fail> {
        (self.emit)(match stage {
            UninstallStage::Ready => {
                section_start_limited(stage.name(), self.opts.ready_timeout.as_secs())
            }
            _ => section_start(stage.name()),
        });
        let result = match stage {
            UninstallStage::Preflight => self.do_preflight(),
            UninstallStage::Locate => self.do_locate(),
            UninstallStage::Guard => self.do_un_guard(),
            UninstallStage::Backup => self.do_un_backup(),
            UninstallStage::SqlRevert => self.do_sql_revert(),
            UninstallStage::RemoveFiles => self.do_remove_files(),
            UninstallStage::PatchRevert => self.do_patch_revert(),
            UninstallStage::ConfRevert => self.do_conf_revert(),
            UninstallStage::Build => self.do_build(),
            UninstallStage::Up => self.do_up(),
            UninstallStage::Ready => self.do_un_ready(),
        };
        match &result {
            Ok(()) => {
                (self.emit)(section_end(stage.name(), "ok"));
                if stage.records_completion() {
                    self.st().mark_undone(stage);
                    self.persist();
                }
            }
            Err(_) => (self.emit)(section_end(stage.name(), "error")),
        }
        result
    }

    fn go_uninstall(&mut self) -> Result<(), Fail> {
        for stage in UNINSTALL_ORDER {
            self.run_un_stage(stage)?;
        }
        Ok(())
    }
}

fn stage_write_fail(path: &Path, e: &std::io::Error) -> Fail {
    Fail::new(
        CODE_WRITE_FAILED,
        format!("Could not write {}: {e}", path.display()),
        "If this is a permission error, the directory may be root-owned from an earlier container run.",
    )
}

/// Copy `<file>` to `<file>.bak` once — the first run's original is the one
/// worth keeping; later runs must not overwrite it with our own edits.
fn backup_sibling_once(path: &Path) {
    let bak = conf::bak_sibling(path);
    if !bak.exists() && path.exists() {
        let _ = std::fs::copy(path, &bak);
    }
}

// ---------------------------------------------------------------------------
// Entry points
// ---------------------------------------------------------------------------

/// Install (or resume installing) the Wrath Unbound add-on, streaming NDJSON.
/// Returns the process exit code: 0 installed, 1 refused or failed.
pub fn unbound_install_stream(opts: &UnboundOpts, emit: impl Fn(Value)) -> i32 {
    unbound_install_stream_with(&ProcUnboundIo::from_env(), opts, &emit)
}

/// [`unbound_install_stream`] with its IO supplied — the seam the tests drive.
/// Production reaches this through the wrapper above, so this IS the real
/// orchestration and not a test-only restatement of it.
pub fn unbound_install_stream_with(
    io: &dyn UnboundIo,
    opts: &UnboundOpts,
    emit: &dyn Fn(Value),
) -> i32 {
    if !crate::install_native::valid_title_id(&opts.id) {
        emit(error_event(
            "BAD_ARG",
            format!("{:?} is not a valid title name.", opts.id),
            "Use letters, digits, '.', '_' and '-' only.",
        ));
        return 1;
    }
    let mut engine = Engine {
        io,
        opts,
        emit,
        server_dir: None,
        state: None,
        resumed: false,
        db_container: None,
        db_password: String::new(),
        backup_file: None,
        residue: Vec::new(),
        uninstalling_mode: false,
        build_files: Vec::new(),
    };
    match engine.go() {
        Ok(()) => {
            engine.st().last_error = None;
            engine.persist();
            // LAST, and outside the stage machine on purpose: this writes into
            // the user's GAME folder, not the server's, and must never be able
            // to fail a rebuild that succeeded.
            let addons = engine.install_client_addons();
            (engine.emit)(done_event(serde_json::json!({
                "addon_version": ADDON_VERSION,
                "server_dir": engine.sdir().display().to_string(),
                "backup": engine.backup_file,
                "migrations_applied": SQL_ORDER.len(),
                "resumed": engine.resumed,
                // The one step this engine does NOT do: spawning the Mentor
                // NPC needs a GM in-game (or SOAP, which a fresh server may
                // not have). `done` says so instead of claiming it happened —
                // the bash banner's lie, not repeated.
                "manual_step": if engine.opts.repair {
                    // On a REPAIR the previous install's Mentor is still
                    // standing - nothing here deletes creature spawns - so
                    // telling the user to add another leaves two.
                    "If your Mentor from the previous install is still standing, there is nothing to do. Otherwise, in game as a GM: .npc add 900001"
                } else {
                    "In game as a GM, stand where the Mentor should appear and run: .npc add 900001"
                },
                // Which half of "installed" actually happened for the client.
                // The cross-class talent UI lives here, so a silent skip would
                // leave a feature the player can neither see nor ask about.
                "client_addons": addons,
            })));
            0
        }
        Err(f) => {
            engine.persist_failure(&f);
            (engine.emit)(error_event(f.code, f.message, &f.hint));
            1
        }
    }
}

/// Uninstall (or resume uninstalling) the add-on, streaming NDJSON.
/// Returns the process exit code: 0 uninstalled, 1 refused or failed.
///
/// Honest-inverse policy: what cannot be reverted is NAMED in the done
/// event's `residue` array, never silently banner-ed. The permanent classes
/// (mod-ale kept, cross-class spells in `character_spell` until each
/// character's next login, Mentor Stones in inventories) are always there;
/// per-run failures join them.
pub fn unbound_uninstall_stream(opts: &UnboundOpts, emit: impl Fn(Value)) -> i32 {
    unbound_uninstall_stream_with(&ProcUnboundIo::from_env(), opts, &emit)
}

pub fn unbound_uninstall_stream_with(
    io: &dyn UnboundIo,
    opts: &UnboundOpts,
    emit: &dyn Fn(Value),
) -> i32 {
    if !crate::install_native::valid_title_id(&opts.id) {
        emit(error_event(
            "BAD_ARG",
            format!("{:?} is not a valid title name.", opts.id),
            "Use letters, digits, '.', '_' and '-' only.",
        ));
        return 1;
    }
    let mut engine = Engine {
        io,
        opts,
        emit,
        server_dir: None,
        state: None,
        resumed: false,
        db_container: None,
        db_password: String::new(),
        backup_file: None,
        residue: Vec::new(),
        uninstalling_mode: true,
        build_files: Vec::new(),
    };
    match engine.go_uninstall() {
        Ok(()) => {
            // The permanent residue classes, stated every time -- these are
            // true of EVERY uninstall, not only ones that hit a snag.
            engine.residue.push(
                "characters keep already-learned cross-class spells until their next login"
                    .to_string(),
            );
            engine.residue.push(
                "Mentor Stones already in character inventories were not touched (see mentor_stone_cleanup_sql)"
                    .to_string(),
            );
            // A COMPLETED uninstall removes the state file: leaving one that
            // claims install-complete on a now-stock server would misdirect
            // both `unbound status` and the next install's guard.
            let sp = state_path(engine.sdir());
            if sp.exists() {
                if let Err(e) = std::fs::remove_file(&sp) {
                    engine.line("warn", format!("could not remove {}: {e}", sp.display()));
                    engine.residue.push(format!(
                        "{} is still on disk and claims the add-on is installed - delete it by hand, or `unbound status` and the next install will disagree with reality",
                        sp.display()
                    ));
                }
            }
            (engine.emit)(done_event(serde_json::json!({
                "addon_version": ADDON_VERSION,
                "server_dir": engine.sdir().display().to_string(),
                "backup": engine.backup_file,
                "residue": engine.residue,
                "mentor_stone_cleanup_sql": MENTOR_STONE_CLEANUP_SQL,
            })));
            0
        }
        Err(f) => {
            engine.persist_failure(&f);
            (engine.emit)(error_event(f.code, f.message, &f.hint));
            1
        }
    }
}

/// What `unbound status` reports: installed / partial / absent, from disk and
/// state-file evidence only (no docker, no DB — callable while stopped).
#[derive(Debug, Clone, Serialize)]
pub struct UnboundStatus {
    pub server_dir: Option<String>,
    pub state_present: bool,
    pub addon_version: Option<String>,
    pub completed: Vec<String>,
    pub next_stage: Option<String>,
    /// Uninstall stages already done. NON-EMPTY means an uninstall was
    /// interrupted - without this field the same state reads as a COMPLETED
    /// INSTALL (every install stage recorded), which is the opposite of the
    /// truth about a server whose module tree is already gone.
    pub uninstalling: Vec<String>,
    pub next_uninstall_stage: Option<String>,
    /// `installed` | `installing` | `uninstalling` | `absent`, derived from
    /// the state file and the disk together.
    pub phase: String,
    pub patch: String,
    pub module_staged: bool,
    pub last_error: Option<String>,
}

pub fn unbound_status(games_dir: &Path, id: &str) -> UnboundStatus {
    let Some(sdir) = maint::resolve_server_dir(&games_dir.join(id)) else {
        return UnboundStatus {
            server_dir: None,
            state_present: false,
            addon_version: None,
            completed: Vec::new(),
            next_stage: None,
            uninstalling: Vec::new(),
            next_uninstall_stage: None,
            phase: "absent".into(),
            patch: "unknown".into(),
            module_staged: false,
            last_error: None,
        };
    };
    let state = load_state(&sdir);
    let patch = match probe_patch_presence(&sdir) {
        PatchPresence::All => "applied",
        PatchPresence::None => "absent",
        PatchPresence::Mixed { .. } => "MIXED",
    };
    let next = state.as_ref().and_then(|s| {
        STAGE_ORDER.iter().find(|st| st.records_completion() && !s.is_done(**st)).map(|st| st.name().to_string())
    });
    let next_un = state.as_ref().and_then(|s| {
        UNINSTALL_ORDER
            .iter()
            .find(|st| st.records_completion() && !s.is_undone(**st))
            .map(|st| st.name().to_string())
    });
    let module_staged = sdir.join(unbound_payload::MODULE_REL).join("src").is_dir();
    let uninstalling = state.as_ref().map(|s| s.uninstalling.clone()).unwrap_or_default();
    // An in-flight uninstall OUTRANKS every install claim in the same file:
    // its stages ran later and have already undone some of them.
    let phase = if !uninstalling.is_empty() {
        "uninstalling"
    } else if state.as_ref().is_some_and(|s| s.complete()) {
        "installed"
    } else if state.is_some() {
        "installing"
    } else if module_staged || patch == "applied" {
        "installed"
    } else {
        "absent"
    };
    UnboundStatus {
        server_dir: Some(sdir.display().to_string()),
        state_present: state.is_some(),
        addon_version: state.as_ref().map(|s| s.addon_version.clone()),
        completed: state.as_ref().map(|s| s.completed.clone()).unwrap_or_default(),
        next_stage: next,
        uninstalling,
        next_uninstall_stage: next_un,
        phase: phase.into(),
        patch: patch.into(),
        module_staged,
        last_error: state.and_then(|s| s.last_error),
    }
}

// Re-exported for the uninstall engine (same file family, next task).
pub use unbound_payload::MODULE_REL;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preflight;
    use dml_core::setup::Tri;
    use std::cell::{Cell, RefCell};

    // -- fixture: a fake server dir -----------------------------------------

    fn fixture(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("dml-unbound-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A games dir holding one plausible AzerothCore server at
    /// `<games>/wow-server-playerbots`. Returns (games_dir, server_dir).
    fn fake_server(name: &str) -> (PathBuf, PathBuf) {
        let games = fixture(name);
        let sdir = games.join(DEFAULT_TITLE_ID);
        std::fs::create_dir_all(sdir.join(".git")).unwrap();
        std::fs::write(sdir.join("docker-compose.yml"), "services: {}\n").unwrap();
        std::fs::write(sdir.join(".env"), "DB_ROOT_PASSWORD=testpw\n").unwrap();
        for rel in PATCHED_FILES {
            let p = sdir.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, "// clean azerothcore source\n").unwrap();
        }
        let conf_dir = sdir.join("env/dist/etc");
        std::fs::create_dir_all(&conf_dir).unwrap();
        std::fs::write(
            conf_dir.join("worldserver.conf"),
            "SomeKey = 1\nValidateSkillLearnedBySpells = 1\n",
        )
        .unwrap();
        (games, sdir)
    }

    fn healthy_facts() -> preflight::PreflightFacts {
        preflight::PreflightFacts {
            docker: preflight::DockerFacts {
                reachable: Tri::Yes,
                detail: None,
                mem_bytes: Some(16 * preflight::GB),
                ncpu: Some(4),
                docker_root_dir: Some("/var/lib/docker".to_string()),
                server_version: Some("27.0.0".to_string()),
            },
            disk: Some(preflight::DiskFacts {
                path: "/docker-data".to_string(),
                free_bytes: Some(200 * preflight::GB),
            }),
            games_disk: Some(preflight::DiskFacts {
                path: "/games".to_string(),
                free_bytes: Some(200 * preflight::GB),
            }),
            games_shares_docker_volume: false,
            git: Tri::Yes,
            hub: Tri::Yes,
            hub_detail: None,
        }
    }

    // -- the fake IO ---------------------------------------------------------

    struct Reply {
        key: String,
        code: i32,
        out: Vec<String>,
        unanswerable: bool,
        /// Consumed on its first matching call, then gets out of the way so
        /// scanning falls through to the next reply sharing this key (see
        /// `reply_once`).
        once: bool,
        used: Cell<bool>,
    }

    /// Records every call (run and run_stdin) and answers from a scripted
    /// table keyed on a substring of the joined argv.
    struct FakeIo {
        facts: preflight::PreflightFacts,
        calls: RefCell<Vec<(String, Call)>>,
        stdin_bytes: RefCell<Vec<(String, usize)>>,
        replies: RefCell<Vec<Reply>>,
        backup: Result<String, String>,
        backup_calls: Cell<usize>,
        /// What `backup_exists` answers -- the fake's model of the shared,
        /// pruned ~/.dml/backups pool.
        existing_backups: RefCell<Vec<String>>,
        /// When Some, git clone/apply MUTATE the fixture like the real tools
        /// would (clone creates CMakeLists.txt; apply writes the symbol).
        world: Option<PathBuf>,
    }

    impl FakeIo {
        fn happy(world: &Path) -> Self {
            let io = FakeIo {
                facts: healthy_facts(),
                calls: RefCell::new(Vec::new()),
                stdin_bytes: RefCell::new(Vec::new()),
                replies: RefCell::new(Vec::new()),
                backup: Ok("backup-test.sql.gz".to_string()),
                backup_calls: Cell::new(0),
                existing_backups: RefCell::new(vec!["backup-test.sql.gz".to_string()]),
                world: Some(world.to_path_buf()),
            };
            io.reply("compose ps -q ac-database", 0, &["cid-db-1234"])
                .reply(TRAINER_PROBE_SQL, 0, &["9\t1084"])
                .reply(CANARY_SQL, 1, &["ERROR 1146 (42S02): Table 'acore_world.unbound_milestones' doesn't exist"])
                .reply("clone", 0, &[])
                .reply("checkout --quiet", 0, &[])
                .reply("apply --check", 0, &[])
                .reply("apply modules/mod-unbound", 0, &[])
                .reply("compose build ac-worldserver", 0, &["#26 10.0 [900/1808] Building CXX object", "#26 20.0 [1808/1808] Linking"])
                .reply("compose up -d --force-recreate ac-worldserver", 0, &[])
                .reply("compose ps -a -q ac-worldserver", 0, &["cid-world-9999"])
                .reply("inspect -f", 0, &["2026-08-02T10:00:00Z|0"])
                .reply("logs --since", 0, &["World initialized in 33 sec", READY_MARKER])
        }

        /// Register or REPLACE-IN-PLACE — the FakeIo lesson from
        /// install_native (2026-07-29): a first-match scan over appended
        /// entries left every later override unreachable, and 7 tests
        /// asserted against the shared builder's answers instead of their own.
        fn set(self, reply: Reply) -> Self {
            {
                let mut replies = self.replies.borrow_mut();
                match replies.iter().position(|r| r.key == reply.key) {
                    Some(i) => replies[i] = reply,
                    None => replies.push(reply),
                }
            }
            self
        }
        fn reply(self, key: &str, code: i32, out: &[&str]) -> Self {
            self.set(Reply {
                key: key.to_string(),
                code,
                out: out.iter().map(|s| s.to_string()).collect(),
                unanswerable: false,
                once: false,
                used: Cell::new(false),
            })
        }
        fn unanswerable(self, key: &str) -> Self {
            self.set(Reply {
                key: key.to_string(),
                code: -1,
                out: Vec::new(),
                unanswerable: true,
                once: false,
                used: Cell::new(false),
            })
        }
        /// Answers a matching call ONCE, then gets out of the way so the
        /// NEXT reply sharing this key (e.g. `happy()`'s permanent success
        /// entry) answers every later call. Deliberately NOT built on `set`'s
        /// replace-in-place semantics: a once-reply must coexist with the
        /// permanent entry for the same key, not overwrite it, so it is
        /// inserted at the FRONT of the scan order instead (`answer` always
        /// finds an unconsumed once-reply before anything registered via
        /// `reply`/`unanswerable`, and skips it once `used`).
        fn reply_once(self, key: &str, code: i32, out: &[&str]) -> Self {
            self.replies.borrow_mut().insert(
                0,
                Reply {
                    key: key.to_string(),
                    code,
                    out: out.iter().map(|s| s.to_string()).collect(),
                    unanswerable: false,
                    once: true,
                    used: Cell::new(false),
                },
            );
            self
        }
        fn backup(mut self, b: Result<&str, &str>) -> Self {
            self.backup = b.map(str::to_string).map_err(str::to_string);
            self
        }

        fn log(&self) -> Vec<String> {
            self.calls
                .borrow()
                .iter()
                .map(|(kind, c)| format!("{kind} {} {}", c.program.label(), c.args.join(" ")))
                .collect()
        }
        fn answer(&self, call: &Call, on_line: &mut dyn FnMut(&str)) -> RunOutcome {
            let joined = call.args.join(" ");
            // Side effects first, so a scripted success leaves the disk the
            // way the real tool would -- otherwise every disk-evidence check
            // downstream fails for a fake reason.
            if let Some(world) = &self.world {
                if call.program == Program::Git && call.args.iter().any(|a| a == "clone") {
                    let dest = call.args.last().unwrap();
                    let _ = std::fs::create_dir_all(dest);
                    let _ = std::fs::write(Path::new(dest).join("CMakeLists.txt"), "ale\n");
                }
                if call.program == Program::Git
                    && call.args.iter().any(|a| a == "apply")
                    && !call.args.iter().any(|a| a == "--check")
                {
                    // Forward apply writes the symbol into all six files;
                    // reverse (-R) strips it -- mirroring what git would do.
                    let reverse = call.args.iter().any(|a| a == "-R");
                    for rel in PATCHED_FILES {
                        let p = world.join(rel);
                        let _ = if reverse {
                            std::fs::write(&p, "// clean azerothcore source\n")
                        } else {
                            std::fs::write(&p, format!("// patched {PATCH_SYMBOL}\n"))
                        };
                    }
                }
            }
            let replies = self.replies.borrow();
            match replies.iter().find(|r| joined.contains(&r.key) && !(r.once && r.used.get())) {
                Some(r) if r.unanswerable => {
                    r.used.set(true);
                    RunOutcome::CouldNotTell("stub could not answer".into())
                }
                Some(r) => {
                    r.used.set(true);
                    for l in &r.out {
                        on_line(l);
                    }
                    RunOutcome::Exited(r.code)
                }
                None => RunOutcome::Exited(0),
            }
        }
    }

    impl InstallIo for FakeIo {
        fn preflight(&self, _games_dir: &Path) -> preflight::PreflightFacts {
            self.facts.clone()
        }
        fn run(&self, call: &Call, on_line: &mut dyn FnMut(&str)) -> RunOutcome {
            self.calls.borrow_mut().push(("run".into(), call.clone()));
            self.answer(call, on_line)
        }
    }

    impl UnboundIo for FakeIo {
        fn run_stdin(
            &self,
            call: &Call,
            input: &[u8],
            on_line: &mut dyn FnMut(&str),
        ) -> RunOutcome {
            self.calls.borrow_mut().push(("stdin".into(), call.clone()));
            self.stdin_bytes.borrow_mut().push((call.args.join(" "), input.len()));
            self.answer(call, on_line)
        }
        fn safety_backup(
            &self,
            container: &str,
            _password: &str,
            _on_line: &mut dyn FnMut(String, String),
        ) -> Result<String, String> {
            // The dump must target the container the GUARD resolved -- the
            // engine-global name was the review finding.
            assert_eq!(container, "cid-db-1234", "backup dumped the wrong container");
            self.backup_calls.set(self.backup_calls.get() + 1);
            self.backup.clone()
        }
        fn backup_exists(&self, file_name: &str) -> bool {
            self.existing_backups.borrow().iter().any(|f| f == file_name)
        }
    }

    // -- helpers -------------------------------------------------------------

    fn opts_for(games: &Path) -> UnboundOpts {
        let mut o = UnboundOpts::new(games);
        o.accept_data_changes = true;
        o.ready_timeout = Duration::from_millis(0); // one poll, no sleeping
        o.ready_poll = Duration::from_millis(0);
        o
    }

    fn run(io: &FakeIo, opts: &UnboundOpts) -> (i32, Vec<Value>) {
        let events = RefCell::new(Vec::new());
        let code = unbound_install_stream_with(io, opts, &|v| events.borrow_mut().push(v));
        (code, events.into_inner())
    }

    fn terminal(events: &[Value]) -> &Value {
        events.last().expect("at least one event")
    }
    fn error_code(events: &[Value]) -> String {
        let t = terminal(events);
        assert_eq!(t["event"], "error", "expected an error terminal: {t}");
        t["error"]["code"].as_str().unwrap_or_default().to_string()
    }
    fn error_message(events: &[Value]) -> String {
        terminal(events)["error"]["message"].as_str().unwrap_or_default().to_string()
    }
    fn error_hint(events: &[Value]) -> String {
        terminal(events)["error"]["hint"].as_str().unwrap_or_default().to_string()
    }
    fn mutating_calls(io: &FakeIo) -> Vec<String> {
        io.log()
            .into_iter()
            .filter(|l| {
                l.contains("exec -i") // SQL pipes
                    || l.contains(" clone ")
                    || (l.contains(" apply ") && !l.contains("--check"))
                    || l.contains("compose build")
                    || l.contains("compose up")
            })
            .collect()
    }

    // -- the tests -----------------------------------------------------------

    #[test]
    fn refuses_without_consent_and_mutates_nothing() {
        let (games, sdir) = fake_server("consent");
        let io = FakeIo::happy(&sdir);
        let mut opts = opts_for(&games);
        opts.accept_data_changes = false;

        let (code, events) = run(&io, &opts);
        assert_eq!(code, 1);
        assert_eq!(error_code(&events), CODE_CONSENT_REQUIRED);
        // The consent message must name the delete the bash's consent OMITTED.
        assert!(error_message(&events).contains("skillraceclassinfo_dbc"));

        // The refusal anchor: NOTHING mutating ran and no backup was taken.
        assert_eq!(mutating_calls(&io), Vec::<String>::new());
        assert_eq!(io.backup_calls.get(), 0);
        assert!(!sdir.join(MODULE_REL).exists(), "module tree must not be staged");
    }

    #[test]
    fn a_failed_backup_stops_everything_before_any_mutation() {
        let (games, sdir) = fake_server("bkfail");
        let io = FakeIo::happy(&sdir).backup(Err("mysqldump: disk full"));
        let (code, events) = run(&io, &opts_for(&games));
        assert_eq!(code, 1);
        assert_eq!(error_code(&events), CODE_BACKUP_FAILED);
        assert!(error_message(&events).contains("disk full"));
        // Ordering anchored on the REFUSAL, not on "backup call precedes sql
        // call" -- the recorded lesson about ordering tests.
        assert_eq!(mutating_calls(&io), Vec::<String>::new());
        assert!(!sdir.join(MODULE_REL).exists());
    }

    #[test]
    fn happy_path_installs_stages_in_order_and_records_exactly_the_recordable() {
        let (games, sdir) = fake_server("happy");
        let io = FakeIo::happy(&sdir);
        let (code, events) = run(&io, &opts_for(&games));

        let t = terminal(&events);
        assert_eq!(code, 0, "terminal: {t}");
        assert_eq!(t["event"], "done");
        assert_eq!(t["data"]["addon_version"], ADDON_VERSION);
        assert_eq!(t["data"]["backup"], "backup-test.sql.gz");
        // `done` NAMES the manual step instead of claiming it happened.
        assert!(t["data"]["manual_step"].as_str().unwrap().contains(".npc add 900001"));

        // Sections appear in stage order.
        let sections: Vec<String> = events
            .iter()
            .filter(|e| e["event"] == "section_start")
            .map(|e| e["name"].as_str().unwrap().to_string())
            .collect();
        let expected: Vec<String> = STAGE_ORDER.iter().map(|s| s.name().to_string()).collect();
        assert_eq!(sections, expected);

        // State discipline: exactly the recordable stages, no guard tokens.
        let state = load_state(&sdir).expect("state written");
        let mut want: Vec<String> = STAGE_ORDER
            .iter()
            .filter(|s| s.records_completion())
            .map(|s| s.name().to_string())
            .collect();
        let mut got = state.completed.clone();
        want.sort();
        got.sort();
        assert_eq!(got, want);
        assert_eq!(state.last_error, None);
        assert_eq!(state.prior_validate_skill.as_deref(), Some("1"));

        // Every staged file is on disk, byte-equal to the embedded payload.
        for f in MANIFEST {
            let disk = std::fs::read_to_string(sdir.join(f.dest)).expect(f.dest);
            assert_eq!(disk, f.body, "{} drifted from the embedded payload", f.dest);
        }

        // The migrations went through stdin in EXACTLY payload order, against
        // the container ID the guard resolved -- never the global name.
        let stdin = io.stdin_bytes.borrow();
        assert_eq!(
            stdin.len(),
            SQL_ORDER.len() + 1,
            "expected the {} migrations plus the summons registration",
            SQL_ORDER.len()
        );
        for ((argv, nbytes), (dest, db)) in stdin.iter().zip(SQL_ORDER) {
            assert!(argv.contains("cid-db-1234"), "SQL went to the wrong container: {argv}");
            assert!(!argv.contains("ac-database"), "SQL used the global name: {argv}");
            assert!(argv.contains(db.database()), "{argv} vs {dest}");
            assert_eq!(*nbytes, unbound_payload::file(dest).unwrap().body.len());
        }
        // ...and the v1.4.0 summons registration goes LAST, through the same
        // resolved container. It is best-effort, but "best-effort" must still
        // mean it was attempted -- silently skipping it would leave
        // spell_script_names unregistered on the exact path this engine takes
        // (`up --force-recreate ac-worldserver` never re-runs ac-db-import).
        let (argv, nbytes) = stdin.last().expect("a last stdin call");
        assert!(argv.contains("cid-db-1234"), "summons SQL went to the wrong container: {argv}");
        assert_eq!(
            *nbytes,
            unbound_payload::file(unbound_payload::SUMMONS_SQL_DEST).unwrap().body.len()
        );

        // conf: the required key landed; the prior value was recorded first.
        let ws = std::fs::read_to_string(sdir.join("env/dist/etc/worldserver.conf")).unwrap();
        assert!(ws.contains("ValidateSkillLearnedBySpells = 0"));
        let ale =
            std::fs::read_to_string(sdir.join("env/dist/etc/modules/mod_ale.conf")).unwrap();
        assert!(ale.contains("ALE.Enabled = 1"));
        assert!(ale.contains(ALE_SCRIPT_PATH));

        // pct flowed from the ninja counter during build.
        assert!(
            events.iter().any(|e| e["event"] == "pct"),
            "no pct events -- the 90-minute stage would be a blank wall"
        );

        // --check ran BEFORE apply (read from the recorded calls, not from a
        // constant restating the code).
        let log = io.log();
        let check_idx = log.iter().position(|l| l.contains("apply --check")).expect("check ran");
        let apply_idx = log
            .iter()
            .position(|l| l.contains("apply modules/mod-unbound") && !l.contains("--check"))
            .expect("apply ran");
        assert!(check_idx < apply_idx);

        // Probe discipline: compose/inspect/logs probes bounded, build not.
        for (kind, call) in io.calls.borrow().iter() {
            let joined = call.args.join(" ");
            if joined.contains("compose build") {
                assert_eq!(call.timeout, None, "the build must never be killed on a timer");
            } else if kind == "stdin" {
                assert_eq!(call.timeout, Some(crate::modmgr::SQL_APPLY_TIMEOUT));
            } else if joined.starts_with("compose ps") || joined.starts_with("inspect") {
                assert_eq!(call.timeout, Some(PROBE_TIMEOUT), "unbounded probe: {joined}");
            }
        }
    }

    #[test]
    fn resume_skips_backup_and_sql_but_reruns_the_guards() {
        let (games, sdir) = fake_server("resume");
        // A previous attempt got through sql, then died.
        let mut st = UnboundState::new(&composegen::install_id(&sdir));
        for s in [Stage::Backup, Stage::StageFiles, Stage::CloneAle, Stage::Sql] {
            st.mark(s);
        }
        st.backup_file = Some("backup-test.sql.gz".into());
        st.last_error = Some("UNBOUND_BUILD_FAILED: boom".into());
        save_state(&sdir, &st).unwrap();

        // The DB agrees with the state (the canary table exists) -- do_sql's
        // restored-from-backup tiebreaker must find them consistent, or the
        // skip under test would be overridden for the right reason.
        let io = FakeIo::happy(&sdir).reply(CANARY_SQL, 0, &["1"]);
        let (code, events) = run(&io, &opts_for(&games));
        assert_eq!(code, 0, "terminal: {}", terminal(&events));

        // Skipped: no second backup, no SQL pipes at all.
        assert_eq!(io.backup_calls.get(), 0, "backup must not re-run on resume");
        assert_eq!(io.stdin_bytes.borrow().len(), 0, "sql must not re-fire on resume");

        // NOT skipped: the guards re-ran -- the trainer probe is the evidence.
        assert!(
            io.log().iter().any(|l| l.contains("npc_trainer")),
            "guard probes must run on every attempt"
        );
    }

    #[test]
    fn a_half_applied_patch_is_named_file_by_file() {
        let (games, sdir) = fake_server("mixed");
        // Three of six already carry the symbol.
        for rel in &PATCHED_FILES[..3] {
            std::fs::write(sdir.join(rel), format!("// {PATCH_SYMBOL}\n")).unwrap();
        }
        let io = FakeIo::happy(&sdir);
        let (code, events) = run(&io, &opts_for(&games));
        assert_eq!(code, 1);
        assert_eq!(error_code(&events), CODE_PATCH_INCOHERENT);
        let msg = error_message(&events);
        // The MISSING files are named -- the actionable half.
        for rel in &PATCHED_FILES[3..] {
            assert!(msg.contains(*rel), "{rel} not named in: {msg}");
        }
        assert_eq!(mutating_calls(&io), Vec::<String>::new());
    }

    #[test]
    fn an_already_applied_patch_is_skipped_not_reapplied() {
        let (games, sdir) = fake_server("patched");
        for rel in PATCHED_FILES {
            std::fs::write(sdir.join(rel), format!("// {PATCH_SYMBOL}\n")).unwrap();
        }
        let io = FakeIo::happy(&sdir);
        let (code, events) = run(&io, &opts_for(&games));
        assert_eq!(code, 0, "terminal: {}", terminal(&events));
        assert!(
            !io.log().iter().any(|l| l.contains(" apply ")),
            "git apply must not run on an already-patched tree"
        );
    }

    #[test]
    fn already_installed_refuses_and_repair_overrides() {
        let (games, sdir) = fake_server("installed");
        let mut st = UnboundState::new(&composegen::install_id(&sdir));
        for s in STAGE_ORDER {
            if s.records_completion() {
                st.mark(s);
            }
        }
        save_state(&sdir, &st).unwrap();

        let io = FakeIo::happy(&sdir);
        let (code, events) = run(&io, &opts_for(&games));
        assert_eq!(code, 1);
        assert_eq!(error_code(&events), CODE_ALREADY_INSTALLED);

        // --repair: the same server installs again, INCLUDING a fresh backup.
        let io = FakeIo::happy(&sdir);
        let mut opts = opts_for(&games);
        opts.repair = true;
        let (code, events) = run(&io, &opts);
        assert_eq!(code, 0, "terminal: {}", terminal(&events));
        assert_eq!(io.backup_calls.get(), 1, "repair must take its own backup");
    }

    #[test]
    fn a_bash_installed_server_is_recognised_without_a_state_file() {
        let (games, sdir) = fake_server("bashinstalled");
        // No state file -- but the canary answers, the module tree exists and
        // the patch is applied: the three independent signals must ALL agree.
        for rel in PATCHED_FILES {
            std::fs::write(sdir.join(rel), format!("// {PATCH_SYMBOL}\n")).unwrap();
        }
        std::fs::create_dir_all(sdir.join(MODULE_REL).join("src")).unwrap();
        let io = FakeIo::happy(&sdir).reply(CANARY_SQL, 0, &["1"]);
        let (code, events) = run(&io, &opts_for(&games));
        assert_eq!(code, 1);
        assert_eq!(error_code(&events), CODE_ALREADY_INSTALLED);
    }

    #[test]
    fn version_mismatch_refuses_before_touching_anything() {
        let (games, sdir) = fake_server("version");
        let mut st = UnboundState::new(&composegen::install_id(&sdir));
        st.addon_version = "1.1.0".into();
        st.mark(Stage::Backup);
        save_state(&sdir, &st).unwrap();
        let io = FakeIo::happy(&sdir);
        let (code, events) = run(&io, &opts_for(&games));
        assert_eq!(code, 1);
        assert_eq!(error_code(&events), CODE_VERSION_MISMATCH);
        assert_eq!(mutating_calls(&io), Vec::<String>::new());
    }

    #[test]
    fn the_ale_pin_failing_is_a_refusal_not_a_warning() {
        let (games, sdir) = fake_server("alepin");
        let io = FakeIo::happy(&sdir).reply("checkout --quiet", 1, &["error: pathspec"]);
        let (code, events) = run(&io, &opts_for(&games));
        assert_eq!(code, 1);
        assert_eq!(error_code(&events), CODE_ALE_PIN_MISMATCH);
        // The bash built HEAD here. Nothing may build.
        assert!(!io.log().iter().any(|l| l.contains("compose build")));
    }

    /// mod-ale already on disk from a PRIOR shallow (`--depth 1`) clone --
    /// the Modules-page install scenario (live 2026-08-09). This routes
    /// `do_clone_ale` into its "already present" branch instead of the
    /// fresh-clone branch above. `rev-parse HEAD` has no scripted reply, so
    /// FakeIo's default (exit 0, empty output) trims to "", which never
    /// starts_with `MOD_ALE_COMMIT` -- read as a pin mismatch, same as a
    /// real shallow clone whose HEAD is not the pinned commit.
    fn ale_shallow_clone(sdir: &Path) {
        let ale = sdir.join("modules/mod-ale");
        std::fs::create_dir_all(&ale).unwrap();
        std::fs::write(ale.join("CMakeLists.txt"), "ale\n").unwrap();
    }

    #[test]
    fn a_shallow_ale_clone_is_rescued_by_fetching_the_pin() {
        let (games, sdir) = fake_server("aleshallow");
        ale_shallow_clone(&sdir);
        // First checkout fails (shallow clone lacks the commit): FakeIo::set
        // replaces same-key entries in place, so a plain `.reply()` here
        // would fail every checkout forever, including the retry. Script
        // the failure to fire ONCE; the fetch answers via happy()'s default
        // (no reply registered -> Exited(0)); the retry then falls through
        // to happy()'s own permanent "checkout --quiet" success entry.
        let io = FakeIo::happy(&sdir).reply_once("checkout --quiet", 1, &["error: pathspec"]);
        let (code, events) = run(&io, &opts_for(&games));
        let _ = code;
        // The install proceeds past clone-ale: it may complete ("done") or
        // stop at some LATER stage in this bare fixture, but never on the
        // pin mismatch the fetch was supposed to rescue.
        let t = terminal(&events);
        if t["event"] == "error" {
            assert_ne!(t["error"]["code"].as_str(), Some(CODE_ALE_PIN_MISMATCH), "{events:?}");
        }
        assert!(io.log().iter().any(|l| l.contains("fetch origin")), "{:#?}", io.log());
        // The fetch reaches the network for one commit -- it must NOT share
        // git_probe's 20s local-read bound (PROBE_TIMEOUT, under the 30s
        // line the brief drew); it gets the crate's git network timeout.
        let fetch_call = io
            .calls
            .borrow()
            .iter()
            .map(|(_, c)| c.clone())
            .find(|c| c.args.join(" ").contains("fetch origin"))
            .expect("fetch call recorded");
        assert_eq!(
            fetch_call.timeout,
            Some(crate::modmgr::GIT_NET_TIMEOUT),
            "the pin fetch must not be bounded by the local-read probe timeout"
        );
    }

    #[test]
    fn a_failing_fetch_still_refuses_with_the_pin_mismatch() {
        let (games, sdir) = fake_server("alefetchfail");
        ale_shallow_clone(&sdir);
        let io = FakeIo::happy(&sdir)
            .reply("checkout --quiet", 1, &["error: pathspec"])
            .reply("fetch origin", 1, &["fatal: could not read from remote"]);
        let (code, events) = run(&io, &opts_for(&games));
        assert_eq!(code, 1);
        assert_eq!(error_code(&events), CODE_ALE_PIN_MISMATCH);
        assert!(!io.log().iter().any(|l| l.contains("compose build")));
    }

    #[test]
    fn a_sql_failure_names_the_file_and_the_backup() {
        let (games, sdir) = fake_server("sqlfail");
        let io = FakeIo::happy(&sdir)
            .reply("acore_characters", 1, &["ERROR 1064 (42000) at line 3: syntax"]);
        let (code, events) = run(&io, &opts_for(&games));
        assert_eq!(code, 1);
        assert_eq!(error_code(&events), CODE_SQL_FAILED);
        let t = terminal(&events);
        let _ = t;
        assert!(error_message(&events).contains("01_unbound_characters.sql"));
        assert!(error_hint(&events).contains("backup-test.sql.gz"));
    }

    #[test]
    fn ready_times_out_when_the_marker_never_prints_and_says_why() {
        let (games, sdir) = fake_server("readyto");
        let io =
            FakeIo::happy(&sdir).reply("logs --since", 0, &["World initialized (no marker)"]);
        let (code, events) = run(&io, &opts_for(&games));
        assert_eq!(code, 1);
        assert_eq!(error_code(&events), CODE_READY_TIMEOUT);
        assert!(error_hint(&events).contains("Lua"));
        // The wait actually polled -- an assertion a failed spawn cannot
        // satisfy (the recorded lesson about early-error-return passes).
        assert!(io.log().iter().any(|l| l.contains("compose ps -a -q ac-worldserver")));
        assert!(io.log().iter().any(|l| l.contains("logs --since")));
    }

    #[test]
    fn the_ready_read_is_scoped_to_this_boot_never_the_whole_log() {
        let (games, sdir) = fake_server("readysince");
        let io = FakeIo::happy(&sdir);
        let (code, _) = run(&io, &opts_for(&games));
        assert_eq!(code, 0);
        for l in io.log() {
            if l.contains(" logs ") {
                assert!(
                    l.contains("--since") || l.contains("--tail"),
                    "an unscoped log read revives the bash's stale-marker bug: {l}"
                );
            }
        }
    }

    #[test]
    fn a_state_file_copied_into_another_dir_is_refused() {
        let (_games_a, sdir_a) = fake_server("stateog");
        let (_games_b, sdir_b) = fake_server("statecopy");
        let mut st = UnboundState::new(&composegen::install_id(&sdir_a));
        st.mark(Stage::Backup);
        save_state(&sdir_a, &st).unwrap();
        std::fs::copy(state_path(&sdir_a), state_path(&sdir_b)).unwrap();
        assert!(load_state(&sdir_a).is_some());
        assert!(load_state(&sdir_b).is_none(), "a copied state file must not survive the move");
    }

    #[test]
    fn missing_worldserver_conf_is_a_hard_stop_before_the_build() {
        let (games, sdir) = fake_server("noconf");
        std::fs::remove_file(sdir.join("env/dist/etc/worldserver.conf")).unwrap();
        let io = FakeIo::happy(&sdir);
        let (code, events) = run(&io, &opts_for(&games));
        assert_eq!(code, 1);
        assert_eq!(error_code(&events), CODE_CONF_MISSING);
        // The bash warned and BUILT here. The build must never start.
        assert!(!io.log().iter().any(|l| l.contains("compose build")));
    }

    #[test]
    fn an_existing_ale_conf_keeps_the_users_other_settings() {
        let (games, sdir) = fake_server("aleconf");
        let mdir = sdir.join("env/dist/etc/modules");
        std::fs::create_dir_all(&mdir).unwrap();
        std::fs::write(
            mdir.join("mod_ale.conf"),
            "ALE.Enabled = 0\nALE.TraceBack = true\nALE.ScriptPath = \"lua_scripts\"\n",
        )
        .unwrap();
        let io = FakeIo::happy(&sdir);
        let (code, _) = run(&io, &opts_for(&games));
        assert_eq!(code, 0);
        let text = std::fs::read_to_string(mdir.join("mod_ale.conf")).unwrap();
        assert!(text.contains("ALE.Enabled = 1"), "required key enforced");
        assert!(text.contains(ALE_SCRIPT_PATH), "required path enforced");
        // The user's debugging choice SURVIVES -- a repair is not a reset.
        assert!(text.contains("ALE.TraceBack = true"), "user setting clobbered: {text}");
        // And the original was kept.
        assert!(mdir.join("mod_ale.conf.bak").exists());
    }

    #[test]
    fn no_server_refuses_with_the_right_code() {
        let games = fixture("noserver");
        let io = FakeIo {
            facts: healthy_facts(),
            calls: RefCell::new(Vec::new()),
            stdin_bytes: RefCell::new(Vec::new()),
            replies: RefCell::new(Vec::new()),
            backup: Ok("x".into()),
            backup_calls: Cell::new(0),
            existing_backups: RefCell::new(Vec::new()),
            world: None,
        };
        let (code, events) = run(&io, &opts_for(&games));
        assert_eq!(code, 1);
        assert_eq!(error_code(&events), CODE_NO_SERVER);
    }

    #[test]
    fn a_stopped_stack_refuses_rather_than_execs_into_nothing() {
        let (games, sdir) = fake_server("stopped");
        let io = FakeIo::happy(&sdir).reply("compose ps -q ac-database", 0, &[]);
        let (code, events) = run(&io, &opts_for(&games));
        assert_eq!(code, 1);
        assert_eq!(error_code(&events), CODE_SERVER_NOT_RUNNING);
    }

    #[test]
    fn a_db_that_cannot_answer_is_not_called_not_playerbots() {
        // Tri-state: the container is up but mysql cannot answer. That is
        // evidence of NOTHING about whether this is a Playerbots server, and
        // the code must say "unreachable", not "incompatible".
        let (games, sdir) = fake_server("dbdown");
        let io = FakeIo::happy(&sdir).unanswerable(TRAINER_PROBE_SQL);
        let (code, events) = run(&io, &opts_for(&games));
        assert_eq!(code, 1);
        assert_eq!(error_code(&events), CODE_DB_UNREACHABLE);
    }

    #[test]
    fn fakeio_overrides_actually_take_effect() {
        // The canary for the recorded test-double trap: register a reply,
        // override it with a DIFFERENT answer for the same key, and prove the
        // override is what answers. If set() ever regresses to append-only,
        // this fails rather than every other test passing for the wrong
        // reason.
        let (_games, sdir) = fake_server("fakecanary");
        let io = FakeIo::happy(&sdir)
            .reply("compose ps -q ac-database", 0, &["FIRST"])
            .reply("compose ps -q ac-database", 0, &["SECOND"]);
        let call = Call {
            program: Program::Docker,
            args: vec!["compose".into(), "ps".into(), "-q".into(), "ac-database".into()],
            cwd: None,
            timeout: Some(PROBE_TIMEOUT),
        };
        let mut out = String::new();
        io.run(&call, &mut |l| out.push_str(l));
        assert_eq!(out, "SECOND");
    }

    // -- uninstall -----------------------------------------------------------

    /// A server with the add-on fully on it: patched files, module tree,
    /// state file claiming a complete install, canary answering.
    fn installed_server(name: &str) -> (PathBuf, PathBuf) {
        let (games, sdir) = fake_server(name);
        for rel in PATCHED_FILES {
            std::fs::write(sdir.join(rel), format!("// patched {PATCH_SYMBOL}\n")).unwrap();
        }
        std::fs::create_dir_all(sdir.join(MODULE_REL).join("src")).unwrap();
        std::fs::write(
            sdir.join("env/dist/etc/worldserver.conf"),
            "SomeKey = 1\nValidateSkillLearnedBySpells = 0\n",
        )
        .unwrap();
        let lua = sdir.join("env/dist/etc/modules/lua_scripts");
        std::fs::create_dir_all(&lua).unwrap();
        std::fs::write(lua.join("unbound_mentor.lua"), "-- mentor\n").unwrap();
        // The install staged the shared Lua engine too -- the uninstall must
        // LEAVE both of these and say so.
        std::fs::create_dir_all(sdir.join("modules/mod-ale")).unwrap();
        std::fs::write(sdir.join("modules/mod-ale/CMakeLists.txt"), "ale\n").unwrap();
        std::fs::write(
            sdir.join("env/dist/etc/modules/mod_ale.conf"),
            "ALE.Enabled = 1\nALE.ScriptPath = \"/azerothcore/env/dist/etc/modules/lua_scripts\"\n",
        )
        .unwrap();
        let mut st = UnboundState::new(&composegen::install_id(&sdir));
        for s in STAGE_ORDER {
            if s.records_completion() {
                st.mark(s);
            }
        }
        st.prior_validate_skill = Some("1".to_string());
        save_state(&sdir, &st).unwrap();
        (games, sdir)
    }

    fn happy_uninstall_io(sdir: &Path) -> FakeIo {
        FakeIo::happy(sdir)
            .reply(CANARY_SQL, 0, &["1"])
            // The stock server after the rebuild: ready, NO marker.
            .reply("logs --since", 0, &["World initialized in 31 sec"])
    }

    fn run_uninstall(io: &FakeIo, opts: &UnboundOpts) -> (i32, Vec<Value>) {
        let events = RefCell::new(Vec::new());
        let code = unbound_uninstall_stream_with(io, opts, &|v| events.borrow_mut().push(v));
        (code, events.into_inner())
    }

    #[test]
    fn uninstall_happy_path_reverts_and_names_its_residue() {
        let (games, sdir) = installed_server("unhappy");
        let io = happy_uninstall_io(&sdir);
        let (code, events) = run_uninstall(&io, &opts_for(&games));
        let t = terminal(&events);
        assert_eq!(code, 0, "terminal: {t}");
        assert_eq!(t["event"], "done");

        // Every revert statement fired, in order, against the resolved
        // container id -- never the global name.
        let sql_calls: Vec<String> = io
            .log()
            .into_iter()
            .filter(|l| l.contains("cid-db-1234") && (l.contains("DELETE") || l.contains("DROP")))
            .collect();
        assert_eq!(sql_calls.len(), REVERT_SQL.len());
        for (call, (_, sql, _)) in sql_calls.iter().zip(REVERT_SQL) {
            assert!(call.contains(sql), "revert order drifted: expected {sql} in {call}");
            assert!(!call.contains(" ac-database "), "global name used: {call}");
        }

        // Files gone; shared engine kept.
        assert!(!sdir.join(MODULE_REL).exists());
        assert!(!sdir.join("env/dist/etc/modules/lua_scripts/unbound_mentor.lua").exists());

        // Patch reverted through -R with a --check first, from the EMBEDDED
        // bytes (the staged copy was already deleted by remove-files).
        let log = io.log();
        let check = log.iter().position(|l| l.contains("-R --check")).expect("check ran");
        let apply = log
            .iter()
            .position(|l| l.contains("apply -R .dml-unbound-revert.patch"))
            .expect("reverse apply ran");
        assert!(check < apply);
        assert!(!sdir.join(".dml-unbound-revert.patch").exists(), "temp patch cleaned up");
        assert_eq!(probe_patch_presence(&sdir), PatchPresence::None);

        // Conf restored to the RECORDED prior value, not a hardcoded 1.
        let ws = std::fs::read_to_string(sdir.join("env/dist/etc/worldserver.conf")).unwrap();
        assert!(ws.contains("ValidateSkillLearnedBySpells = 1"), "{ws}");

        // The state file is GONE -- a stock server carries no claim.
        assert!(load_state(&sdir).is_none());

        // Residue names the permanent classes and the cleanup SQL is TEXT.
        let residue = t["data"]["residue"].as_array().unwrap();
        let joined = residue.iter().map(|v| v.as_str().unwrap()).collect::<Vec<_>>().join(" | ");
        assert!(joined.contains("mod-ale left in place"), "{joined}");
        assert!(joined.contains("next login"), "{joined}");
        assert!(joined.contains("Mentor Stones"), "{joined}");
        assert!(t["data"]["mentor_stone_cleanup_sql"].as_str().unwrap().contains("item_instance"));
    }

    #[test]
    fn uninstall_refuses_without_consent_and_mutates_nothing() {
        let (games, sdir) = installed_server("unconsent");
        let io = happy_uninstall_io(&sdir);
        let mut opts = opts_for(&games);
        opts.accept_data_changes = false;
        let (code, events) = run_uninstall(&io, &opts);
        assert_eq!(code, 1);
        assert_eq!(error_code(&events), CODE_CONSENT_REQUIRED);
        // The message names the DROP that destroys progression.
        assert!(error_message(&events).contains("unbound_character_unlocks"));
        assert_eq!(io.backup_calls.get(), 0);
        assert!(sdir.join(MODULE_REL).exists(), "nothing may be deleted on a refusal");
        assert!(
            !io.log().iter().any(|l| l.contains("DELETE") || l.contains("DROP")),
            "no revert SQL may run on a refusal"
        );
    }

    #[test]
    fn uninstall_on_a_virgin_server_refuses_unless_forced() {
        let (games, sdir) = fake_server("unvirgin");
        let io = FakeIo::happy(&sdir); // canary errors -> no evidence anywhere
        let (code, events) = run_uninstall(&io, &opts_for(&games));
        assert_eq!(code, 1);
        assert_eq!(error_code(&events), CODE_NOT_INSTALLED);

        let io = FakeIo::happy(&sdir).reply("logs --since", 0, &["World initialized in 3 sec"]);
        let mut opts = opts_for(&games);
        opts.force = true;
        let (code, events) = run_uninstall(&io, &opts);
        assert_eq!(code, 0, "terminal: {}", terminal(&events));
    }

    #[test]
    fn a_failed_revert_statement_continues_and_lands_in_residue() {
        let (games, sdir) = installed_server("unsqlfail");
        let io = happy_uninstall_io(&sdir)
            .reply("skillraceclassinfo_dbc", 1, &["ERROR 1142 (42000): DELETE command denied"]);
        let (code, events) = run_uninstall(&io, &opts_for(&games));
        // The run COMPLETES -- half a cleanup beats stranding -- but the done
        // event carries the failure, which is the half the bash forgot.
        assert_eq!(code, 0, "terminal: {}", terminal(&events));
        let residue = terminal(&events)["data"]["residue"].as_array().unwrap();
        let joined = residue.iter().map(|v| v.as_str().unwrap()).collect::<Vec<_>>().join(" | ");
        assert!(joined.contains("universal skill access"), "{joined}");
        assert!(joined.contains("1142"), "the mysql error travels: {joined}");
        // The statements AFTER the failed one still ran.
        assert!(io.log().iter().any(|l| l.contains("unbound_character_unlocks")));
    }

    #[test]
    fn a_diverged_tree_leaves_the_patch_and_says_so() {
        let (games, sdir) = installed_server("undiverged");
        let io = happy_uninstall_io(&sdir).reply("-R --check", 1, &["error: patch does not apply"]);
        let (code, events) = run_uninstall(&io, &opts_for(&games));
        assert_eq!(code, 0, "terminal: {}", terminal(&events));
        // No reverse apply after a failed check.
        assert!(
            !io.log().iter().any(|l| l.contains("apply -R .dml-unbound-revert.patch")),
            "reverse apply must not run after a failed check"
        );
        let residue = terminal(&events)["data"]["residue"].as_array().unwrap();
        let joined = residue.iter().map(|v| v.as_str().unwrap()).collect::<Vec<_>>().join(" | ");
        assert!(joined.contains("core patch left applied"), "{joined}");
        assert!(joined.contains("inert"), "the user is told it is harmless: {joined}");
    }

    #[test]
    fn a_rebuild_that_still_prints_the_marker_is_named_in_residue() {
        // The build "succeeded" but the module is somehow still compiled in
        // (BuildKit cache pathology, a second module copy). world-ready alone
        // would call this a clean uninstall; the inverted marker check says
        // otherwise.
        let (games, sdir) = installed_server("unmarker");
        let io = happy_uninstall_io(&sdir)
            .reply("logs --since", 0, &["World initialized in 31 sec", READY_MARKER]);
        let (code, events) = run_uninstall(&io, &opts_for(&games));
        assert_eq!(code, 0);
        let residue = terminal(&events)["data"]["residue"].as_array().unwrap();
        let joined = residue.iter().map(|v| v.as_str().unwrap()).collect::<Vec<_>>().join(" | ");
        assert!(joined.contains("still loads Unbound"), "{joined}");
    }

    #[test]
    fn uninstall_restores_the_stock_default_and_flags_it_when_no_prior_was_recorded() {
        let (games, sdir) = installed_server("unnoprior");
        // A bash-scripted install left no state file at all.
        std::fs::remove_file(state_path(&sdir)).unwrap();
        let io = happy_uninstall_io(&sdir);
        let (code, events) = run_uninstall(&io, &opts_for(&games));
        assert_eq!(code, 0, "terminal: {}", terminal(&events));
        let ws = std::fs::read_to_string(sdir.join("env/dist/etc/worldserver.conf")).unwrap();
        assert!(ws.contains("ValidateSkillLearnedBySpells = 1"));
        // ...and the guess is DECLARED, not passed off as a restoration.
        let residue = terminal(&events)["data"]["residue"].as_array().unwrap();
        let joined = residue.iter().map(|v| v.as_str().unwrap()).collect::<Vec<_>>().join(" | ");
        assert!(joined.contains("no record of the prior value"), "{joined}");
    }

    #[test]
    fn status_reports_partial_progress_honestly() {
        let (games, sdir) = fake_server("statuspartial");
        let mut st = UnboundState::new(&composegen::install_id(&sdir));
        st.mark(Stage::Backup);
        st.mark(Stage::StageFiles);
        st.last_error = Some("UNBOUND_SQL_FAILED: x".into());
        save_state(&sdir, &st).unwrap();
        let s = unbound_status(&games, DEFAULT_TITLE_ID);
        assert!(s.state_present);
        assert_eq!(s.next_stage.as_deref(), Some("clone-ale"));
        assert_eq!(s.patch, "absent");
        assert!(s.last_error.unwrap().contains("UNBOUND_SQL_FAILED"));
    }
}
