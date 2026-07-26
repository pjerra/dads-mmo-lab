//! Native-mode module **install / update / remove** (spike:
//! `spike/docker-desktop-native`, Chunk 3a). Faithful port of the module
//! registries + shared helpers in `cli/src/70-modules.sh` and the
//! `install)`/`update)`/`remove)` arms in `cli/src/90-main.sh` (install
//! 4586-4841, update 5472-5544, remove 4842-4966). Native-only; WSL keeps
//! calling `dml` (`wow_module_install`/`wow_module_update`/
//! `wow_module_remove` in `lib.rs`).
//!
//! ARCHITECTURE. This module holds every REUSABLE primitive (registries,
//! validators, rebuild-pending marker I/O, git/mysql subprocess helpers, the
//! per-family lua/sql install-time helpers, `_wow_pull_repo`) as free
//! functions, mirroring how `dml::backup`/`dml::maint` split "library" from
//! "orchestration". The three STREAMED Tauri commands' `_blocking` functions
//! (the actual arm-shaped orchestration: precondition order, ndjson event
//! sequencing, `done` shapes) live in `lib.rs` next to
//! `wow_world_restart_native_blocking`, which they follow event-for-event
//! (`section_start`/`line`/`section_end`/`done`/`error`, domain errors
//! travel IN the stream, the command itself still resolves `Ok(())`).
//!
//! SUBPROCESS DISCIPLINE. Every `git`/`docker` call here goes through
//! [`super::status::output_bounded_draining`] (never `crate::output_bounded`
//! — see that function's own doc comment for the pipe-deadlock hazard a
//! `git clone`/`docker exec … mysqldump` can trigger). SQL file application
//! (the lua/sql families) pipes the `.sql` file's bytes to `docker exec -i
//! ac-database mysql …` over real stdin — EXACTLY like the bash — rather
//! than the `mysql` crate: a module's `.sql` file can carry multiple
//! statements (and, upstream, `DELIMITER` blocks), which the crate's
//! single-statement `exec_drop` cannot run safely. The same shell-out is
//! used for the handful of FIXED canned statements this module runs
//! (`_sqlmod_run_stmt`'s tweak-multiplier UPDATE, the hearthstone-cd/
//! npc-teleporter reversal statements) — the bash's own `-e "..."` payload
//! for `npc-teleporter`'s DELETE is two `;`-joined statements in one call,
//! which the crate (no `CLIENT_MULTI_STATEMENTS` handshake) cannot run
//! either; staying on the shell keeps every SQL path uniform and byte-exact.
//!
//! TWO DIFFERENT EXECUTABLES. `git` (PATH-resolved, same convention as
//! `maint`'s `update-check`) and the resolved Docker Desktop `docker`
//! binary (`native::docker_program()`) are NEVER the same handle — every
//! function below takes exactly the one(s) it actually shells out to, named
//! `git_program`/`docker_program` (never a shared `program`) so a caller
//! can't accidentally hand a `docker` path to a git-only function or vice
//! versa. `install_cpp`/`update_module` are git-only (cpp install/update and
//! module update never touch docker — module SQL lands via the server's own
//! db-import at rebuild time); `remove_sql`/`module_backup_now` are
//! docker-only; `install_lua`/`install_sql` need BOTH (a git clone step plus
//! a docker-applied SQL/backup step).
//!
//! NATIVE-MODE-ONLY by convention: WSL keeps calling `dml`; the Tauri
//! command layer (`lib.rs`) gates every entry point on
//! `require_native_backend()`.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use serde_json::Value;

use super::status::{output_bounded_draining, windows_no_window};

// ---------------------------------------------------------------------------
// Timeouts.
// ---------------------------------------------------------------------------

/// Small local `git` reads (`rev-parse`, `status --porcelain`, `remote
/// get-url`) — mirrors `maint::PROBE_TIMEOUT`/`status::DOCKER_PROBE_TIMEOUT`.
pub const GIT_PROBE_TIMEOUT: Duration = Duration::from_secs(5);
/// `git clone`/`git pull`/`git fetch` hit the network and, for a couple of
/// registry entries (`mod-arac` ships DBC + an MPQ patch), can move a
/// sizeable payload even at `--depth 1` — generous like `backup::
/// DUMP_TIMEOUT`, bounded so a wedged remote can never hang the UI forever.
pub const GIT_NET_TIMEOUT: Duration = Duration::from_secs(1200);
/// `docker exec … mysql` applying one module's SQL file/statement — small
/// relative to a full-DB dump, but still network+docker, so it gets its own
/// generous-but-bounded budget rather than the 5s probe timeout.
pub const SQL_APPLY_TIMEOUT: Duration = Duration::from_secs(600);

// ---------------------------------------------------------------------------
// Registries — `_module_registry_cpp`/`_module_registry_lua`/
// `_module_registry_sql` (`70-modules.sh:12-65`), ported VERBATIM from the
// heredocs. NOTE: the sql registry has 10 rows (all-stackables, baby-mobs,
// buff-mobs, xbuff-mobs, hearthstone-cd, lvl1-mounts, nerf-mobs,
// npc-teleporter, portals-capitals, rare-drops) — the task brief's "9 rows"
// undercounts the live oracle by one; row COUNT tests assert the true
// (oracle) count, not the brief's summary.
// ---------------------------------------------------------------------------

/// One `_module_registry_cpp` row: key|name|url|sqldirs (sqldirs is
/// informational only — AzerothCore's db-import applies module SQL
/// automatically, never by hand; kept for parity/future catalog reuse).
pub struct CppRow {
    pub key: &'static str,
    pub name: &'static str,
    pub url: &'static str,
    pub sqldirs: &'static str,
}

pub const CPP_REGISTRY: &[CppRow] = &[
    CppRow { key: "mod-1v1-arena", name: "1v1 Arena", url: "https://github.com/azerothcore/mod-1v1-arena.git", sqldirs: "characters" },
    CppRow { key: "mod-aoe-loot", name: "AoE Loot", url: "https://github.com/azerothcore/mod-aoe-loot.git", sqldirs: "world" },
    CppRow { key: "mod-ah-bot", name: "Auction House Bot", url: "https://github.com/azerothcore/mod-ah-bot.git", sqldirs: "world" },
    CppRow { key: "mod-ah-bot-plus", name: "Auction House Bot Plus (blizzlike pricing)", url: "https://github.com/NathanHandley/mod-ah-bot-plus.git", sqldirs: "world" },
    CppRow { key: "mod-autobalance", name: "Auto Balance (dynamic difficulty)", url: "https://github.com/azerothcore/mod-autobalance.git", sqldirs: "world" },
    CppRow { key: "mod-ale", name: "AzerothCore Lua Engine (ALE)", url: "https://github.com/azerothcore/mod-ale.git", sqldirs: "" },
    CppRow { key: "mod-player-bot-level-brackets", name: "Bot Level Brackets (Playerbot distribution)", url: "https://github.com/DustinHendrickson/mod-player-bot-level-brackets.git", sqldirs: "characters" },
    CppRow { key: "mod-challenge-modes", name: "Challenge Modes (Hardcore, Iron Man, etc.)", url: "https://github.com/nl-saw/mod-challenge-modes.git", sqldirs: "world,characters" },
    CppRow { key: "mod-custom-login", name: "Custom Login (starter gear + rep on first login)", url: "https://github.com/azerothcore/mod-custom-login.git", sqldirs: "characters" },
    CppRow { key: "mod-individual-progression", name: "Individual Progression (Vanilla -> TBC -> WotLK)", url: "https://github.com/ZhengPeiRu21/mod-individual-progression.git", sqldirs: "world,characters" },
    CppRow { key: "mod-junk-to-gold", name: "Junk to Gold (auto-sell gray items)", url: "https://github.com/noisiver/mod-junk-to-gold.git", sqldirs: "world" },
    CppRow { key: "mod-learn-spells", name: "Learn Spells on Levelup", url: "https://github.com/azerothcore/mod-learn-spells.git", sqldirs: "world" },
    CppRow { key: "mod-npc-beastmaster", name: "NPC Beastmaster (pets for all classes)", url: "https://github.com/azerothcore/mod-npc-beastmaster.git", sqldirs: "world,characters" },
    CppRow { key: "mod-quest-loot-party", name: "Quest Loot Party (quest items drop for all eligible party members)", url: "https://github.com/pangolp/mod-quest-loot-party.git", sqldirs: "world" },
    CppRow { key: "mod-arac", name: "All Races All Classes (ARAC - data mod: SQL + DBC + MPQ)", url: "https://github.com/heyitsbench/mod-arac.git", sqldirs: "world" },
    CppRow { key: "mod-dungeon-master", name: "Dungeon Master (roguelike dungeon challenge system)", url: "https://github.com/InstanceForge/mod-dungeon-master.git", sqldirs: "world,characters" },
    CppRow { key: "mod-solocraft", name: "Solocraft (solo dungeon/raid scaling)", url: "https://github.com/azerothcore/mod-solocraft.git", sqldirs: "world" },
    CppRow { key: "mod-talentbutton", name: "Talent Button (dual-spec at 10 + anywhere talent reset)", url: "https://github.com/brian8544/mod-talentbutton.git", sqldirs: "" },
    CppRow { key: "mod-transmog", name: "Transmogrification", url: "https://github.com/azerothcore/mod-transmog.git", sqldirs: "world,characters" },
];

/// One `_module_registry_lua` row: key|name|url.
pub struct LuaRow {
    pub key: &'static str,
    pub name: &'static str,
    pub url: &'static str,
}

pub const LUA_REGISTRY: &[LuaRow] = &[
    LuaRow { key: "accountwide", name: "Accountwide Systems (achievements, currency, mounts, pets)", url: "https://github.com/Aldori15/azerothcore-eluna-accountwide.git" },
    LuaRow { key: "activechat", name: "Azeroth Chatter (lore-grounded ambient world RP chat)", url: "https://github.com/svey-xyz/ActiveChat.git" },
    LuaRow { key: "battlepass", name: "Battle Pass System (XP progression + rewards)", url: "https://github.com/Shonik/lua-battlepass.git" },
    LuaRow { key: "bmah", name: "Black Market Auction House (MoP-style BMAH + client addon)", url: "https://github.com/DadsMmoLab/dads-mmo-lab.git" },
    LuaRow { key: "lootpet", name: "Loot Pet (vanity pet auto-loots nearby corpses)", url: "https://github.com/Brytenwally/Lootpet.git" },
    LuaRow { key: "paragon", name: "Paragon Anniversary (endless post-80 stat progression + client files)", url: "https://github.com/Grim-Batol/Paragon-Anniversary.git" },
    LuaRow { key: "sitmeanrest", name: "Sit Means Rest (regen buff on /sit; strips on movement)", url: "https://github.com/Brytenwally/SitMeansRest.git" },
    LuaRow { key: "sod", name: "Season of Discovery Buffs (phased leveling XP rate bonus)", url: "https://github.com/DadsMmoLab/dads-mmo-lab.git" },
    LuaRow { key: "unlimitedammo", name: "Unlimited Ammo (auto-refills Hunter arrows/bullets)", url: "https://github.com/Day36512/Acore_Lua_Unlimited_Ammo.git" },
];

/// One `_module_registry_sql` row: key|name|url|install type.
pub struct SqlRow {
    pub key: &'static str,
    pub name: &'static str,
    pub url: &'static str,
    pub kind: &'static str,
}

pub const SQL_REGISTRY: &[SqlRow] = &[
    SqlRow { key: "all-stackables", name: "All Stackables to 200", url: "https://github.com/AsgavinYT/azerothcore-all-stackables-200.git", kind: "clone_sql" },
    SqlRow { key: "baby-mobs", name: "Baby Mobs (HPx0.25 / DMGx0.25 / ARMx0.25)", url: "", kind: "tweak_world" },
    SqlRow { key: "buff-mobs", name: "Buff Mobs (HPx2 / DMGx1.5 / ARMx1.5)", url: "", kind: "tweak_world" },
    SqlRow { key: "xbuff-mobs", name: "Extreme Buff Mobs (HPx4 / DMGx2 / ARMx2)", url: "", kind: "tweak_world" },
    SqlRow { key: "hearthstone-cd", name: "Hearthstone Cooldown Tweaks", url: "https://github.com/AsgavinYT/hearthstone-cooldowns.git", kind: "clone_sql_pick" },
    SqlRow { key: "lvl1-mounts", name: "Level One Mounts (ride at level 1)", url: "https://github.com/tomcoffingiii/mod-level-one-mounts.git", kind: "clone_sql" },
    SqlRow { key: "nerf-mobs", name: "Nerf Mobs (HPx0.5 / DMGx0.75 / ARMx0.75)", url: "", kind: "tweak_world" },
    SqlRow { key: "npc-teleporter", name: "NPC Teleporter (capital + starting zones)", url: "https://github.com/Zoidwaffle/sql-npc-teleporter.git", kind: "clone_dist" },
    SqlRow { key: "portals-capitals", name: "Portals in All Capitals", url: "https://github.com/azerothcore/portals-in-all-capitals.git", kind: "clone_sql" },
    SqlRow { key: "rare-drops", name: "Rare Drops (450 Classic rares loot)", url: "https://github.com/StraysFromPath/mod-rare-drops.git", kind: "clone_sql_norevert" },
];

pub fn cpp_row(key: &str) -> Option<&'static CppRow> {
    CPP_REGISTRY.iter().find(|r| r.key == key)
}
pub fn lua_row(key: &str) -> Option<&'static LuaRow> {
    LUA_REGISTRY.iter().find(|r| r.key == key)
}
pub fn sql_row(key: &str) -> Option<&'static SqlRow> {
    SQL_REGISTRY.iter().find(|r| r.key == key)
}

// ---------------------------------------------------------------------------
// Validators — `_valid_module_key`/`_valid_cpp_key`/`_valid_module_url`/
// `_module_key_from_url` (`70-modules.sh:123-135`). `_valid_cpp_key` itself
// is NOT duplicated here — `dml::modules::valid_cpp_key` already ports it
// (Task 2), reused verbatim.
// ---------------------------------------------------------------------------

pub use super::modules::valid_cpp_key;

/// `_valid_module_key`: `^[a-z0-9-]{1,64}$`.
pub fn valid_module_key(s: &str) -> bool {
    let n = s.chars().count();
    (1..=64).contains(&n) && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// `_valid_module_url`: `^https://[A-Za-z0-9._~/-]+(\.git)?$`.
pub fn valid_module_url(s: &str) -> bool {
    let Some(rest) = s.strip_prefix("https://") else { return false };
    let rest = rest.strip_suffix(".git").unwrap_or(rest);
    !rest.is_empty()
        && rest.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '~' | '/' | '-'))
}

/// `_module_key_from_url` (`70-modules.sh:129-135`): basename, `.git`
/// stripped, lowercased, must then pass [`valid_cpp_key`]. `None` when the
/// derived key isn't `mod-*` shaped (the bash prints nothing in that case).
pub fn module_key_from_url(url: &str) -> Option<String> {
    let base = url.rsplit('/').next().unwrap_or(url);
    let base = base.strip_suffix(".git").unwrap_or(base);
    let base = base.to_lowercase();
    if valid_cpp_key(&base) {
        Some(base)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// State — `_cpp_installed`/`_lua_cloned`/`_lua_deployed`/`_sql_installed`
// (`70-modules.sh:138,248-264,288`), reimplemented locally (each `dml::`
// module stays self-contained rather than reaching into `dml::modules`'
// private helpers).
// ---------------------------------------------------------------------------

pub fn cpp_installed(sdir: &Path, key: &str) -> bool {
    sdir.join("modules").join(key).join(".git").is_dir()
}

pub fn lua_cloned(sdir: &Path, key: &str) -> bool {
    sdir.join("ale_scripts").join(key).join(".git").is_dir()
}

/// `_lua_deployed` (`70-modules.sh:248-262`).
pub fn lua_deployed(sdir: &Path, key: &str) -> bool {
    let lua = sdir.join("env").join("dist").join("etc").join("modules").join("lua_scripts");
    match key {
        "accountwide" => lua.join("accountwide").is_dir(),
        "activechat" => lua.join("AzerothChatter").is_dir(),
        "battlepass" => lua.join("battlepass").is_dir(),
        "bmah" => lua.join("BMAH.lua").is_file(),
        "lootpet" => lua.join("LootPet.lua").is_file(),
        "paragon" => lua.join("paragon").is_dir(),
        "sitmeanrest" => lua.join("SitMeansRest.lua").is_file(),
        "sod" => lua.join("SOD.lua").is_file(),
        "unlimitedammo" => lua.join("UnlimitedAmmo.lua").is_file(),
        _ => false,
    }
}

/// `_lua_has_sql` (`70-modules.sh:379`).
pub fn lua_has_sql(key: &str) -> bool {
    matches!(key, "accountwide" | "battlepass" | "bmah" | "paragon")
}

fn lua_scripts_dir(sdir: &Path) -> PathBuf {
    sdir.join("env").join("dist").join("etc").join("modules").join("lua_scripts")
}

/// `_sqlmod_marker` (`70-modules.sh:709`).
pub fn sqlmod_marker_path(sdir: &Path, key: &str) -> PathBuf {
    sdir.join("sql_scripts").join("installed").join(format!("{key}.installed"))
}

pub fn sql_installed(sdir: &Path, key: &str) -> bool {
    sqlmod_marker_path(sdir, key).is_file()
}

/// `_sqlmod_dirs` (`70-modules.sh:708`).
pub fn sqlmod_dirs(sdir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(sdir.join("sql_scripts").join("installed"))?;
    std::fs::create_dir_all(sdir.join("sql_scripts").join("clones"))?;
    Ok(())
}

fn sqlmod_clone_dir(sdir: &Path, key: &str) -> PathBuf {
    sdir.join("sql_scripts").join("clones").join(key)
}

// ---------------------------------------------------------------------------
// Rebuild-pending marker — `_rebuild_pending_add` (`70-modules.sh:142-146`):
// `<sdir>/.dml-rebuild-pending`, plain LF lines, append+dedup (grep -qxF
// against the WHOLE file before appending). Byte-compatible with the
// already-native `dml::modules::ModuleReader`'s reader (`load_pending`) and
// bash `module rebuild`'s consumer: no CRLF, no sort/rewrite, ever.
// ---------------------------------------------------------------------------

pub fn rebuild_pending_add(sdir: &Path, key: &str) -> std::io::Result<()> {
    let f = sdir.join(".dml-rebuild-pending");
    let existing = std::fs::read_to_string(&f).unwrap_or_default();
    if existing.split('\n').any(|line| line == key) {
        return Ok(());
    }
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&f)?;
    write!(file, "{key}\n")
}

/// `_rebuild_pending_clear` (`70-modules.sh:148`): `rm -f
/// <sdir>/.dml-rebuild-pending` — the WHOLE marker file, not a per-key
/// removal (a full `module rebuild` recompiles everything pending at once).
/// Best-effort: a missing/unremovable file is silently ignored, matching
/// `rm -f`.
pub fn rebuild_pending_clear(sdir: &Path) {
    let _ = std::fs::remove_file(sdir.join(".dml-rebuild-pending"));
}

// ---------------------------------------------------------------------------
// Shared "line" ndjson event — identical shape regardless of which section
// (module-install/-update/-remove) is emitting it (`{"event":"line",
// "level":…,"text":…}`, `10-json.sh`'s `ndjson_line`), so every helper below
// that streams progress reuses this ONE constructor instead of each owning a
// near-duplicate. `section_start`/`section_end`/`done`/`error` DO carry a
// section name, so those stay local to their `lib.rs` orchestration
// function, matching `wow_world_restart_native_blocking`'s convention.
// ---------------------------------------------------------------------------

pub fn line_event(level: &str, text: impl Into<String>) -> Value {
    serde_json::json!({"event": "line", "level": level, "text": text.into()})
}

/// Section names — `ndjson_section_start`'s `$1` in each arm
/// (`90-main.sh:4587,4843,5481`).
pub const SECTION_INSTALL: &str = "module-install";
pub const SECTION_REMOVE: &str = "module-remove";
pub const SECTION_UPDATE: &str = "module-update";

pub fn section_start(name: &str) -> Value {
    serde_json::json!({"event": "section_start", "name": name})
}
pub fn section_end(name: &str, status: &str) -> Value {
    serde_json::json!({"event": "section_end", "name": name, "status": status})
}
pub fn done_event(data: Value) -> Value {
    serde_json::json!({"event": "done", "data": data})
}
pub fn error_event(code: &str, message: impl Into<String>, hint: &str) -> Value {
    serde_json::json!({"event": "error", "error": {"code": code, "message": message.into(), "hint": hint}})
}

/// `_client_path` (`70-modules.sh:317-325`): the saved WoW client folder,
/// but ONLY while it still exists as a directory on disk — deliberately NOT
/// `clientpath::client_path_value`'s stricter `valid_client_dir` check (that
/// one also requires a `Wow.exe`/`Interface` shape); the bash helper this
/// ports tests existence only (`[[ -d "$p" ]]`).
pub fn effective_client_path() -> Option<PathBuf> {
    let f = super::clientpath::client_path_file()?;
    let raw = std::fs::read_to_string(f).ok()?;
    let trimmed = raw.trim_end_matches(['\n', '\r']);
    if trimmed.is_empty() {
        return None;
    }
    let p = PathBuf::from(trimmed);
    if p.is_dir() {
        Some(p)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Subprocess primitives — bounded + DRAINED (status.rs style). `git_field`
// mirrors `maint::git_field` (private there) but stays local per the
// self-contained-module convention every other `dml::` reader already
// follows.
// ---------------------------------------------------------------------------

/// Trimmed stdout of a bounded LOCAL `git -C <dir> <args…>` call, or `""` on
/// any spawn failure/nonzero exit/timeout — the `_wow_git_*` guarded-
/// assignment contract (`70-modules.sh:842-845`): always "succeeds" from the
/// caller's point of view.
fn git_field(program: &OsStr, dir: &Path, args: &[&str]) -> String {
    let mut cmd = Command::new(program);
    cmd.arg("-C").arg(dir).args(args);
    windows_no_window(&mut cmd);
    match output_bounded_draining(cmd, GIT_PROBE_TIMEOUT) {
        Some(out) if out.status.success() => {
            String::from_utf8_lossy(&out.stdout).trim_end_matches(['\n', '\r']).to_string()
        }
        _ => String::new(),
    }
}

/// `true` on a successful (exit 0) bounded LOCAL `git -C <dir> <args…>`.
fn git_ok(program: &OsStr, dir: &Path, args: &[&str], timeout: Duration) -> bool {
    let mut cmd = Command::new(program);
    cmd.arg("-C").arg(dir).args(args);
    windows_no_window(&mut cmd);
    matches!(output_bounded_draining(cmd, timeout), Some(out) if out.status.success())
}

/// `git -C <dir> diff`'s raw stdout bytes on success, `None` on failure —
/// used for the local-edits patch backup in [`wow_pull_repo`].
fn git_diff_capture(program: &OsStr, dir: &Path) -> Option<Vec<u8>> {
    let mut cmd = Command::new(program);
    cmd.arg("-C").arg(dir).arg("diff");
    windows_no_window(&mut cmd);
    match output_bounded_draining(cmd, GIT_PROBE_TIMEOUT) {
        Some(out) if out.status.success() => Some(out.stdout),
        _ => None,
    }
}

/// `true` when `dir/.git` exists.
pub fn is_git_checkout(dir: &Path) -> bool {
    dir.join(".git").is_dir()
}

/// `_wow_git_url` (`70-modules.sh:842`). `pub`: reused by `wow_update_native`
/// (Chunk 3b, `lib.rs`) for the AzerothCore/mod-playerbots remote gates.
pub fn git_remote_url(program: &OsStr, dir: &Path) -> String {
    git_field(program, dir, &["remote", "get-url", "origin"])
}

/// `_wow_git_head` (`70-modules.sh:844`). `pub`: reused by `wow_update_native`
/// for the before/after summary (same pattern [`update_module`] uses below).
pub fn git_short_head(program: &OsStr, dir: &Path) -> String {
    git_field(program, dir, &["rev-parse", "--short", "HEAD"])
}

/// `_wow_git_branch` (`70-modules.sh:843`). `pub`: reused by
/// `wow_update_native`'s `BRANCH_MISMATCH` gate (`90-main.sh:5688-5693`).
pub fn git_branch(program: &OsStr, dir: &Path) -> String {
    git_field(program, dir, &["rev-parse", "--abbrev-ref", "HEAD"])
}

/// `_wow_remote_ok` (`70-modules.sh:850`): does a remote URL point at the
/// expected fork? Old installs may still point at the pre-rename
/// `liyunfan1223` org -- GitHub redirects those, so they're accepted too
/// (ported verbatim, including that fallback).
pub fn wow_remote_ok(url: &str, expected_substr: &str) -> bool {
    url.contains(expected_substr) || url.contains("liyunfan1223")
}

/// `_WOW_PULL_SUMMARY`'s assembly (`70-modules.sh:927-931`): `"up to date"`
/// when the short HEAD didn't move, else `"<before> -> <after>"`.
pub fn pull_summary(before: &str, after: &str) -> String {
    if before == after {
        "up to date".to_string()
    } else {
        format!("{before} -> {after}")
    }
}

/// Which gate rejects first, given already-computed synthetic state -- a
/// pure, unit-testable mirror of the `wow update` arm's fail-closed gate
/// ORDER (`90-main.sh:5645-5732`: server dir -> git checkout -> AzerothCore
/// remote -> mod-playerbots remote (only if the module is present) -> branch
/// -> explicit `--backup`/`--no-backup`). `None` means every gate passed (the
/// real function proceeds to the backup + pull steps). The real
/// `wow_update_native_blocking` (`lib.rs`) evaluates the SAME conditions in
/// the SAME order directly (each with its own message/hint) rather than
/// calling this helper -- it exists so that order is independently asserted
/// against synthetic states without a live git checkout.
pub fn update_gate_order(
    server_dir_exists: bool,
    is_git_checkout: bool,
    ac_remote_ok: bool,
    pb_present: bool,
    pb_remote_ok: bool,
    branch_is_playerbot: bool,
    backup_choice: Option<bool>,
) -> Option<&'static str> {
    if !server_dir_exists {
        return Some("NOT_FOUND");
    }
    if !is_git_checkout {
        return Some("GIT_MISSING");
    }
    if !ac_remote_ok {
        return Some("REMOTE_MISMATCH");
    }
    if pb_present && !pb_remote_ok {
        return Some("REMOTE_MISMATCH");
    }
    if !branch_is_playerbot {
        return Some("BRANCH_MISMATCH");
    }
    if backup_choice.is_none() {
        return Some("BAD_ARG");
    }
    None
}

fn git_clone_depth1(program: &OsStr, url: &str, dest: &Path) -> bool {
    let mut cmd = Command::new(program);
    cmd.args(["clone", "--depth", "1", url]).arg(dest);
    windows_no_window(&mut cmd);
    matches!(output_bounded_draining(cmd, GIT_NET_TIMEOUT), Some(out) if out.status.success())
}

// ---------------------------------------------------------------------------
// `docker exec … mysql` — statement (`-e`) and file-via-stdin (`-i` + real
// stdin) variants. See the module doc comment for why BOTH stay on the
// shell rather than the `mysql` crate.
// ---------------------------------------------------------------------------

/// `_sqlmod_run_stmt`/`_tweak_apply` (`70-modules.sh:711,748-750`): `docker
/// exec ac-database mysql -uroot -p<pw> <db> -e "<stmt>"`, bounded+drained.
fn mysql_run_stmt(program: &OsStr, password: &str, db: &str, stmt: &str) -> bool {
    let mut cmd = Command::new(program);
    cmd.args(["exec", "ac-database", "mysql", "-uroot", &format!("-p{password}"), db, "-e", stmt]);
    windows_no_window(&mut cmd);
    matches!(output_bounded_draining(cmd, SQL_APPLY_TIMEOUT), Some(out) if out.status.success())
}

/// Bounded, pipe-DRAINED runner that also feeds `input` on the child's
/// stdin — `output_bounded_draining`'s sibling for the one shape it doesn't
/// cover (piped-in SQL files). A dedicated writer thread streams `input`
/// then drops the handle (closing stdin / EOF-ing the child) WHILE the
/// stdout/stderr drain threads and the `try_wait` poll loop run
/// concurrently, so neither a large SQL file nor a chatty `mysql` response
/// can deadlock the pipe — same reasoning as `status::
/// output_bounded_draining`'s own doc comment.
fn run_with_stdin_bounded_draining(mut cmd: Command, input: Vec<u8>, timeout: Duration) -> Option<std::process::Output> {
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().ok()?;
    let mut stdin = child.stdin.take().expect("stdin was piped");
    let mut stdout = child.stdout.take().expect("stdout was piped");
    let mut stderr = child.stderr.take().expect("stderr was piped");

    let stdin_handle = std::thread::spawn(move || {
        use std::io::Write;
        let _ = stdin.write_all(&input);
        // `stdin` drops here -> pipe closed -> EOF for the child.
    });
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

    let deadline = std::time::Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let _ = stdin_handle.join();
    let stdout_buf = stdout_handle.join().unwrap_or_default();
    let stderr_buf = stderr_handle.join().unwrap_or_default();
    let status = status?;
    Some(std::process::Output { status, stdout: stdout_buf, stderr: stderr_buf })
}

/// `_ale_sql_file`/`_sqlmod_run_file` (`70-modules.sh:382,712`): `docker exec
/// -i ac-database mysql -uroot -p<pw> <db> < <file>`.
fn mysql_run_file(program: &OsStr, password: &str, db: &str, file: &Path) -> bool {
    let Ok(bytes) = std::fs::read(file) else { return false };
    let mut cmd = Command::new(program);
    cmd.args(["exec", "-i", "ac-database", "mysql", "-uroot", &format!("-p{password}"), db]);
    windows_no_window(&mut cmd);
    matches!(run_with_stdin_bounded_draining(cmd, bytes, SQL_APPLY_TIMEOUT), Some(out) if out.status.success())
}

// ---------------------------------------------------------------------------
// Directory helpers.
// ---------------------------------------------------------------------------

fn rm_rf(path: &Path) {
    if path.is_dir() {
        let _ = std::fs::remove_dir_all(path);
    } else if path.exists() {
        let _ = std::fs::remove_file(path);
    }
}

/// `cp -r <src>/. <dst>/` — recursively copies every entry (files and
/// subdirs) FROM `src` INTO `dst`, creating `dst` if needed and MERGING with
/// any existing content there (an existing file at the same relative path is
/// overwritten). Used for the lua family's directory-payload deploys
/// (activechat/battlepass/paragon) and the best-effort client-file copies.
fn copy_dir_contents(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    let rd = std::fs::read_dir(src)?;
    for entry in rd {
        let entry = entry?;
        let ft = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if ft.is_dir() {
            copy_dir_contents(&from, &to)?;
        } else if ft.is_file() {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// Recursively collect every FILE under `dir` (like `find <dir>`, no
/// maxdepth — walks `.git` too, matching the bash's own unfiltered `find`).
fn walk_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for entry in rd.flatten() {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            walk_files(&path, out);
        } else if ft.is_file() {
            out.push(path);
        }
    }
}

fn sort_by_path_string(paths: &mut [PathBuf]) {
    paths.sort_by(|a, b| a.to_string_lossy().cmp(&b.to_string_lossy()));
}

// ---------------------------------------------------------------------------
// sql-mod family — `_sqlmod_up_files`/`_sqlmod_down_files` (`70-modules.sh:
// 714-735`): every `*.sql` under a clone (recursive), classified by
// BASENAME (not full path), sorted for determinism.
// ---------------------------------------------------------------------------

fn contains_ci(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

/// Basename passes the "up" filter: does NOT look like a reversal/example
/// file (`grep -qviE 'down|example'`, i.e. kept when NEITHER substring is
/// present, case-insensitive).
pub fn is_up_file(basename: &str) -> bool {
    !contains_ci(basename, "down") && !contains_ci(basename, "example")
}

/// Basename looks like a reversal file (`grep -qiE 'down'`, case-insensitive
/// substring).
pub fn is_down_file(basename: &str) -> bool {
    contains_ci(basename, "down")
}

fn basename_str(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
}

/// `find <dir> -name '*.sql'` — case-SENSITIVE (bash `-name`, not `-iname`);
/// only [`hearthstone_pick`]'s `-iname` glob is genuinely case-insensitive.
fn has_sql_extension(p: &Path) -> bool {
    p.extension().map(|e| e == "sql").unwrap_or(false)
}

pub fn sqlmod_up_files(dir: &Path) -> Vec<PathBuf> {
    let mut all = Vec::new();
    walk_files(dir, &mut all);
    let mut kept: Vec<PathBuf> =
        all.into_iter().filter(|p| has_sql_extension(p)).filter(|p| is_up_file(&basename_str(p))).collect();
    sort_by_path_string(&mut kept);
    kept
}

pub fn sqlmod_down_files(dir: &Path) -> Vec<PathBuf> {
    let mut all = Vec::new();
    walk_files(dir, &mut all);
    let mut kept: Vec<PathBuf> =
        all.into_iter().filter(|p| has_sql_extension(p)).filter(|p| is_down_file(&basename_str(p))).collect();
    sort_by_path_string(&mut kept);
    kept
}

// ---------------------------------------------------------------------------
// `clone_sql_pick` (hearthstone-cd) variant picker —
// `find "$scdir" -iname "*${mvariant}*.sql" | grep -viE "[0-9]${mvariant}" |
// sort | head -n1` (`90-main.sh:4791`). Split into a pure core (unit-
// testable on a synthetic path list) + a thin disk-walking wrapper.
// ---------------------------------------------------------------------------

/// `true` when `haystack_lc` contains `needle_lc` at some position
/// immediately preceded by an ASCII digit — the `grep -viE "[0-9]<variant>"`
/// exclusion (disambiguates e.g. "5min" from "15min"). Both inputs must
/// already be lowercased by the caller.
fn digit_then_needle(haystack_lc: &str, needle_lc: &str) -> bool {
    if needle_lc.is_empty() {
        return false;
    }
    let bytes = haystack_lc.as_bytes();
    let mut start = 0usize;
    while let Some(rel) = haystack_lc.get(start..).and_then(|s| s.find(needle_lc)) {
        let idx = start + rel;
        if idx > 0 && bytes[idx - 1].is_ascii_digit() {
            return true;
        }
        start = idx + 1;
        if start >= haystack_lc.len() {
            break;
        }
    }
    false
}

/// Pure core of the hearthstone-cd variant picker: `paths` are candidate
/// `.sql` path strings (any case); returns the first (ascending-sorted)
/// match whose basename contains `variant` case-insensitively AND is not
/// digit-prefixed (the `[0-9]<variant>` exclusion).
pub fn hearthstone_pick(paths: &[String], variant: &str) -> Option<String> {
    let variant_lc = variant.to_lowercase();
    let mut matched: Vec<&String> = paths
        .iter()
        .filter(|p| {
            let lc = p.to_lowercase();
            lc.ends_with(".sql") && lc.contains(&variant_lc) && !digit_then_needle(&lc, &variant_lc)
        })
        .collect();
    matched.sort();
    matched.first().map(|s| s.to_string())
}

fn hearthstone_pick_on_disk(scdir: &Path, variant: &str) -> Option<PathBuf> {
    let mut all = Vec::new();
    walk_files(scdir, &mut all);
    let strings: Vec<String> = all.iter().map(|p| p.to_string_lossy().to_string()).collect();
    let picked = hearthstone_pick(&strings, variant)?;
    Some(PathBuf::from(picked))
}

/// `1sec|1min|5min|15min|30min` — the `clone_sql_pick` `--variant` allowlist
/// (`90-main.sh:4747-4751`).
pub fn valid_hearthstone_variant(v: &str) -> bool {
    matches!(v, "1sec" | "1min" | "5min" | "15min" | "30min")
}

// ---------------------------------------------------------------------------
// `clone_dist` (npc-teleporter) variant normalization — `90-main.sh:4753-
// 4760`: default 80, digits-only, base-10 forced (`10#$var`, avoids bash's
// octal misparse of a leading-zero string), range 1..=80.
// ---------------------------------------------------------------------------

pub fn normalize_teleporter_variant(v: Option<&str>) -> Result<u32, ()> {
    let s = match v {
        Some(s) if !s.is_empty() => s,
        _ => "80",
    };
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err(());
    }
    let n: u32 = s.parse().map_err(|_| ())?;
    if (1..=80).contains(&n) {
        Ok(n)
    } else {
        Err(())
    }
}

/// `sed "s/@ONY_LEVEL := [0-9]*/@ONY_LEVEL := $mvariant/"` (`90-main.sh:
/// 4803`): FIRST occurrence per line only (no `/g`), matching the exact
/// prefix then zero-or-more digits.
fn replace_ony_level_line(line: &str, variant: u32) -> String {
    const PAT: &str = "@ONY_LEVEL := ";
    if let Some(idx) = line.find(PAT) {
        let after = idx + PAT.len();
        let rest = &line[after..];
        let digit_len = rest.bytes().take_while(|b| b.is_ascii_digit()).count();
        format!("{}{}{}{}", &line[..idx], PAT, variant, &rest[digit_len..])
    } else {
        line.to_string()
    }
}

pub fn replace_ony_level(input: &str, variant: u32) -> String {
    input.split('\n').map(|l| replace_ony_level_line(l, variant)).collect::<Vec<_>>().join("\n")
}

fn list_dist_files(dir: &Path) -> Vec<PathBuf> {
    let mut all = Vec::new();
    walk_files(dir, &mut all);
    let mut kept: Vec<PathBuf> =
        all.into_iter().filter(|p| p.extension().map(|e| e == "dist").unwrap_or(false)).collect();
    sort_by_path_string(&mut kept);
    kept
}

// ---------------------------------------------------------------------------
// `tweak_world` — `_tweak_mults`/`_tweak_apply`/`_tweak_installed_sibling`/
// `_tweak_reverse` (`70-modules.sh:737-781`). Multipliers stay as the
// registry's own decimal STRINGS (never round-tripped through `f64`) so the
// forward UPDATE is byte-identical to the bash; only the reverse computes a
// float (matching the bash's own `awk` inverse).
// ---------------------------------------------------------------------------

pub const TWEAK_KEYS: &[&str] = &["baby-mobs", "buff-mobs", "xbuff-mobs", "nerf-mobs"];

pub fn tweak_mults(key: &str) -> Option<(&'static str, &'static str, &'static str)> {
    match key {
        "baby-mobs" => Some(("0.25", "0.25", "0.25")),
        "buff-mobs" => Some(("2", "1.5", "1.5")),
        "xbuff-mobs" => Some(("4", "2", "2")),
        "nerf-mobs" => Some(("0.5", "0.75", "0.75")),
        _ => None,
    }
}

fn tweak_apply(program: &OsStr, password: &str, h: &str, d: &str, a: &str) -> bool {
    let stmt = format!(
        "UPDATE creature_template SET HealthModifier = HealthModifier * {h}, DamageModifier = DamageModifier * {d}, ArmorModifier = ArmorModifier * {a};"
    );
    mysql_run_stmt(program, password, "acore_world", &stmt)
}

pub fn tweak_installed_sibling(sdir: &Path, me: &str) -> Option<String> {
    TWEAK_KEYS.iter().find(|k| **k != me && sqlmod_marker_path(sdir, k).is_file()).map(|k| k.to_string())
}

fn tweak_reverse(program: &OsStr, password: &str, sdir: &Path, key: &str) -> bool {
    let marker = sqlmod_marker_path(sdir, key);
    let (mut h, mut d, mut a) = (String::new(), String::new(), String::new());
    if let Ok(content) = std::fs::read_to_string(&marker) {
        for line in content.lines() {
            if h.is_empty() {
                if let Some(v) = line.strip_prefix("APPLIED_HP_MULT=") {
                    h = v.to_string();
                }
            }
            if d.is_empty() {
                if let Some(v) = line.strip_prefix("APPLIED_DMG_MULT=") {
                    d = v.to_string();
                }
            }
            if a.is_empty() {
                if let Some(v) = line.strip_prefix("APPLIED_ARM_MULT=") {
                    a = v.to_string();
                }
            }
        }
    }
    if h.is_empty() || d.is_empty() || a.is_empty() {
        match tweak_mults(key) {
            Some((dh, dd, da)) => {
                h = dh.to_string();
                d = dd.to_string();
                a = da.to_string();
            }
            None => return false,
        }
    }
    let (Ok(hf), Ok(df), Ok(af)) = (h.parse::<f64>(), d.parse::<f64>(), a.parse::<f64>()) else { return false };
    if hf == 0.0 || df == 0.0 || af == 0.0 {
        return false;
    }
    let ih = format!("{:.6}", 1.0 / hf);
    let id = format!("{:.6}", 1.0 / df);
    let ia = format!("{:.6}", 1.0 / af);
    tweak_apply(program, password, &ih, &id, &ia)
}

// ---------------------------------------------------------------------------
// Safety backup — `_module_backup_now` (`70-modules.sh:294-312`): the SAME
// `_backup_dump_to … 1` the CLI's own `backup create --include-world` uses
// (world INCLUDED — module installs/removals mutate `acore_world`), reusing
// the native `dml::backup` create path (`new_backup_file_name`/`dump_to`/
// `prune`). No `.meta` summary sidecar (the bash helper never writes one
// either — only `backup create`'s own arm does).
// ---------------------------------------------------------------------------

pub fn module_backup_now(program: &OsStr, password: &str, emit: &dyn Fn(Value)) -> bool {
    use super::backup;
    let Some(bdir) = backup::backup_dir() else { return false };
    if std::fs::create_dir_all(&bdir).is_err() {
        return false;
    }
    let file_name = backup::new_backup_file_name(true);
    let out_path = bdir.join(&file_name);
    emit(line_event("info", "backing up characters, bots, accounts and world..."));
    if let Err(errtail) = backup::dump_to(program, password, true, &out_path) {
        if !errtail.is_empty() {
            emit(line_event("warn", format!("backup failed: {errtail}")));
        }
        return false;
    }
    emit(line_event("info", format!("backup created: {file_name}")));
    for p in backup::prune(&bdir) {
        emit(line_event("info", format!("pruned old backup: {p}")));
    }
    true
}

// ---------------------------------------------------------------------------
// lua (ALE) family — `_lua_clone`/`_lua_deploy`/`_lua_apply_sql`/
// `_lua_client_copy`/`_lua_remove_deployed` (`70-modules.sh:387-584`).
// ---------------------------------------------------------------------------

/// `_lua_clone` (`70-modules.sh:387-418`): clone or update an ALE script.
/// `sod`/`bmah` live as subfolders of the DadsMmoLab monorepo, so they use a
/// sparse checkout instead of a full clone.
pub fn lua_clone(program: &OsStr, sdir: &Path, key: &str, url: &str, emit: &dyn Fn(Value)) -> bool {
    let cdir = sdir.join("ale_scripts").join(key);
    let sparse: Option<&str> = match key {
        "sod" => Some("guides/wow-wotlk/ALE-Kegs/SeasonOfDiscovery"),
        "bmah" => Some("guides/wow-wotlk/ALE-Kegs/BlackMarketAuctionHouse"),
        _ => None,
    };
    let _ = std::fs::create_dir_all(sdir.join("ale_scripts"));
    if cdir.join(".git").is_dir() {
        emit(line_event("info", format!("updating {key}...")));
        return if let Some(sp) = sparse {
            let _ = sp;
            git_ok(program, &cdir, &["pull", "--depth=1", "origin", "HEAD"], GIT_NET_TIMEOUT)
        } else {
            git_ok(program, &cdir, &["pull", "--depth", "1"], GIT_NET_TIMEOUT)
        };
    }
    rm_rf(&cdir);
    emit(line_event("info", format!("cloning {key}...")));
    if let Some(sp) = sparse {
        if std::fs::create_dir_all(&cdir).is_err() {
            return false;
        }
        if !git_ok(program, &cdir, &["init", "-q"], GIT_PROBE_TIMEOUT) {
            return false;
        }
        if !git_ok(program, &cdir, &["remote", "add", "origin", url], GIT_PROBE_TIMEOUT) {
            return false;
        }
        if !git_ok(program, &cdir, &["config", "core.sparseCheckout", "true"], GIT_PROBE_TIMEOUT) {
            return false;
        }
        let sparse_file = cdir.join(".git").join("info").join("sparse-checkout");
        if std::fs::write(&sparse_file, format!("{sp}/\n")).is_err() {
            return false;
        }
        git_ok(program, &cdir, &["pull", "--depth=1", "origin", "HEAD"], GIT_NET_TIMEOUT)
    } else {
        git_clone_depth1(program, url, &cdir)
    }
}

/// `_lua_deploy` (`70-modules.sh:423-483`): copy the script's `.lua` payload
/// from the clone into `lua_scripts/`.
pub fn lua_deploy(sdir: &Path, key: &str, emit: &dyn Fn(Value)) -> bool {
    let cdir = sdir.join("ale_scripts").join(key);
    let lua = lua_scripts_dir(sdir);
    if std::fs::create_dir_all(&lua).is_err() {
        return false;
    }
    emit(line_event("info", format!("deploying {key} lua files...")));
    match key {
        "accountwide" => {
            let src = cdir.join("lua_scripts").join("AccountWide");
            if !src.is_dir() {
                return false;
            }
            let dst = lua.join("accountwide");
            if std::fs::create_dir_all(&dst).is_err() {
                return false;
            }
            let Ok(rd) = std::fs::read_dir(&src) else { return false };
            for entry in rd.flatten() {
                let p = entry.path();
                if p.extension().map(|e| e == "lua").unwrap_or(false) {
                    if std::fs::copy(&p, dst.join(entry.file_name())).is_err() {
                        return false;
                    }
                }
            }
            true
        }
        "activechat" => {
            let src = cdir.join("AzerothChatter");
            if !src.is_dir() {
                return false;
            }
            let dst = lua.join("AzerothChatter");
            if std::fs::create_dir_all(&dst).is_err() || copy_dir_contents(&src, &dst).is_err() {
                return false;
            }
            // ALE's loader dedupes by basename across subdirs -- rename the
            // colliding data-pair and patch every require() call site.
            let data = dst.join("data");
            let chatter = data.join("chatter.lua");
            if chatter.is_file() {
                let _ = std::fs::rename(&chatter, data.join("chatter_data.lua"));
            }
            let context = data.join("context.lua");
            if context.is_file() {
                let _ = std::fs::rename(&context, data.join("context_data.lua"));
            }
            let mut files = Vec::new();
            walk_files(&dst, &mut files);
            for f in files {
                if f.extension().map(|e| e == "lua").unwrap_or(false) {
                    if let Ok(content) = std::fs::read_to_string(&f) {
                        let patched = content
                            .replace("require(\"data.chatter\")", "require(\"data.chatter_data\")")
                            .replace("require(\"data.context\")", "require(\"data.context_data\")");
                        if patched != content {
                            let _ = std::fs::write(&f, patched);
                        }
                    }
                }
            }
            true
        }
        "battlepass" => {
            let src = cdir.join("lua_scripts");
            if !src.is_dir() {
                return false;
            }
            if copy_dir_contents(&src, &lua).is_err() {
                return false;
            }
            // ALE auto-loads .ext files before .lua; the explicit require
            // re-executes CSMH and corrupts its state -- strip the line.
            let comm = lua.join("05_BP_Communication.lua");
            if comm.is_file() {
                if let Ok(content) = std::fs::read_to_string(&comm) {
                    let filtered: String = content
                        .lines()
                        .filter(|l| !l.contains("require(\"lib.CSMH.CSMH_SMH\")"))
                        .map(|l| format!("{l}\n"))
                        .collect();
                    let _ = std::fs::write(&comm, filtered);
                }
            }
            true
        }
        "bmah" => {
            let src = cdir
                .join("guides")
                .join("wow-wotlk")
                .join("ALE-Kegs")
                .join("BlackMarketAuctionHouse")
                .join("BMAH.lua");
            if !src.is_file() {
                return false;
            }
            std::fs::copy(&src, lua.join("BMAH.lua")).is_ok()
        }
        "lootpet" => {
            let src = cdir.join("LootPet.lua");
            if !src.is_file() {
                return false;
            }
            std::fs::copy(&src, lua.join("LootPet.lua")).is_ok()
        }
        "paragon" => {
            let src = cdir.join("serverside").join("paragon");
            if !src.is_dir() {
                return false;
            }
            let dst = lua.join("paragon");
            std::fs::create_dir_all(&dst).is_ok() && copy_dir_contents(&src, &dst).is_ok()
        }
        "sitmeanrest" => {
            let src = cdir.join("SitMeansRest.lua");
            if !src.is_file() {
                return false;
            }
            std::fs::copy(&src, lua.join("SitMeansRest.lua")).is_ok()
        }
        "sod" => {
            let src = cdir
                .join("guides")
                .join("wow-wotlk")
                .join("ALE-Kegs")
                .join("SeasonOfDiscovery")
                .join("SOD.lua");
            if !src.is_file() {
                return false;
            }
            let ok = std::fs::copy(&src, lua.join("SOD.lua")).is_ok();
            rm_rf(&lua.join("sod")); // stale duplicate-load guard (manager parity)
            ok
        }
        "unlimitedammo" => {
            let src = cdir.join("UnlimitedAmmo.lua");
            if !src.is_file() {
                return false;
            }
            std::fs::copy(&src, lua.join("UnlimitedAmmo.lua")).is_ok()
        }
        _ => false,
    }
}

/// `_lua_apply_sql` (`70-modules.sh:486-519`): per-key SQL application (this
/// family's SQL IS hand-applied by design, unlike the cpp family).
pub fn lua_apply_sql(program: &OsStr, password: &str, sdir: &Path, key: &str, emit: &dyn Fn(Value)) -> bool {
    let cdir = sdir.join("ale_scripts").join(key);
    match key {
        "accountwide" => {
            emit(line_event("info", "applying accountwide SQL (acore_characters)..."));
            mysql_run_file(program, password, "acore_characters", &cdir.join("sql").join("create_accountwide_tables.sql"))
        }
        "battlepass" => {
            emit(line_event("info", "applying battlepass SQL (acore_world + acore_characters)..."));
            mysql_run_file(program, password, "acore_world", &cdir.join("sql").join("battlepass_world.sql"))
                && mysql_run_file(program, password, "acore_characters", &cdir.join("sql").join("battlepass_characters.sql"))
        }
        "bmah" => {
            emit(line_event("info", "applying BMAH SQL (acore_world)..."));
            mysql_run_file(
                program,
                password,
                "acore_world",
                &cdir.join("guides").join("wow-wotlk").join("ALE-Kegs").join("BlackMarketAuctionHouse").join("sql").join("BMAH_Up.sql"),
            )
        }
        "paragon" => {
            emit(line_event("info", "applying paragon migrations..."));
            let create = cdir.join("sql").join("01_create_database.sql");
            if create.is_file() && !mysql_run_file(program, password, "acore_world", &create) {
                return false;
            }
            let mut files = Vec::new();
            walk_files(&cdir.join("sql"), &mut files);
            let mut sql_files: Vec<PathBuf> = files.into_iter().filter(|p| has_sql_extension(p)).collect();
            sort_by_path_string(&mut sql_files);
            for f in sql_files {
                let bn = basename_str(&f);
                if bn == "01_create_database.sql" {
                    continue;
                }
                if paragon_migration_skip(&bn) {
                    continue;
                }
                emit(line_event("info", format!("  {bn}")));
                if !mysql_run_file(program, password, "acore_ale", &f) {
                    return false;
                }
            }
            true
        }
        _ => true,
    }
}

/// `[[ "$bn" =~ ^[0-9]{2}-[0-9]{2} || "$bn" =~ [Ee]xample ]]` (`70-modules.sh:512`).
fn paragon_migration_skip(bn: &str) -> bool {
    let starts_date_prefixed = {
        let b = bn.as_bytes();
        b.len() >= 5
            && b[0].is_ascii_digit()
            && b[1].is_ascii_digit()
            && b[2] == b'-'
            && b[3].is_ascii_digit()
            && b[4].is_ascii_digit()
    };
    starts_date_prefixed || bn.contains("Example") || bn.contains("example")
}

/// `_lua_client_copy` (`70-modules.sh:524-566`): best-effort client-side
/// file copies; never fails the caller (matches the bash's own `||
/// ndjson_line warn ...` swallow-everything contract).
pub fn lua_client_copy(sdir: &Path, key: &str, client_path: Option<&Path>, emit: &dyn Fn(Value)) {
    if !matches!(key, "bmah" | "paragon" | "sod") {
        return;
    }
    let cdir = sdir.join("ale_scripts").join(key);
    let Some(cpath) = client_path else {
        emit(line_event("warn", "no client folder set — skipped client files (set it on the Modules page)"));
        if key == "sod" {
            emit(line_event(
                "warn",
                "SOD server DBC files must be copied manually — see guides/wow-wotlk/wow-manage.sh copy_server_dbc",
            ));
        }
        return;
    };
    match key {
        "bmah" => {
            let src = cdir
                .join("guides")
                .join("wow-wotlk")
                .join("ALE-Kegs")
                .join("BlackMarketAuctionHouse")
                .join("Client Files")
                .join("AddOns")
                .join("BlackMarketUI");
            if src.is_dir() {
                emit(line_event("info", "installing BlackMarketUI client addon..."));
                let dst = cpath.join("Interface").join("AddOns").join("BlackMarketUI");
                if std::fs::create_dir_all(&dst).is_err() {
                    emit(line_event("warn", "client copy failed — copy it manually"));
                    return;
                }
                if copy_dir_contents(&src, &dst).is_err() {
                    emit(line_event("warn", "client addon copy failed — copy it manually"));
                }
            }
        }
        "paragon" => {
            let src = cdir.join("clientside").join("Interface");
            if src.is_dir() {
                emit(line_event("info", "installing paragon client Interface files..."));
                let dst = cpath.join("Interface");
                if std::fs::create_dir_all(&dst).is_err() {
                    emit(line_event("warn", "client copy failed — copy it manually"));
                    return;
                }
                if copy_dir_contents(&src, &dst).is_err() {
                    emit(line_event("warn", "client Interface copy failed — copy it manually"));
                }
            }
        }
        "sod" => {
            let data_src = cdir.join("guides").join("wow-wotlk").join("ALE-Kegs").join("SeasonOfDiscovery").join("Data");
            if data_src.is_dir() {
                emit(line_event("info", "installing SOD client Data patches..."));
                let dst = cpath.join("Data");
                if std::fs::create_dir_all(&dst).is_err() {
                    emit(line_event("warn", "client copy failed — copy it manually"));
                    return;
                }
                if copy_dir_contents(&data_src, &dst).is_err() {
                    emit(line_event("warn", "client Data copy failed — copy it manually"));
                }
            }
            let iface_src = cdir.join("guides").join("wow-wotlk").join("ALE-Kegs").join("SeasonOfDiscovery").join("Interface");
            if iface_src.is_dir() {
                let dst = cpath.join("Interface");
                if std::fs::create_dir_all(&dst).is_err() {
                    emit(line_event("warn", "client copy failed — copy it manually"));
                } else if copy_dir_contents(&iface_src, &dst).is_err() {
                    emit(line_event("warn", "client Interface copy failed — copy it manually"));
                }
            }
            emit(line_event(
                "warn",
                "SOD server DBC files must be copied manually — see guides/wow-wotlk/wow-manage.sh copy_server_dbc",
            ));
        }
        _ => {}
    }
}

/// `_lua_remove_deployed` (`70-modules.sh:569-584`): delete the deployed
/// payload (DB tables kept).
pub fn lua_remove_deployed(sdir: &Path, key: &str) {
    let lua = lua_scripts_dir(sdir);
    match key {
        "accountwide" => rm_rf(&lua.join("accountwide")),
        "activechat" => rm_rf(&lua.join("AzerothChatter")),
        "battlepass" => {
            rm_rf(&lua.join("battlepass"));
            rm_rf(&lua.join("lib").join("CSMH"));
            if let Ok(rd) = std::fs::read_dir(&lua) {
                for e in rd.flatten() {
                    let name = e.file_name().to_string_lossy().to_string();
                    if name.starts_with("05_BP_") && name.ends_with(".lua") {
                        let _ = std::fs::remove_file(e.path());
                    }
                }
            }
        }
        "bmah" => {
            let _ = std::fs::remove_file(lua.join("BMAH.lua"));
        }
        "lootpet" => {
            let _ = std::fs::remove_file(lua.join("LootPet.lua"));
        }
        "paragon" => rm_rf(&lua.join("paragon")),
        "sitmeanrest" => {
            let _ = std::fs::remove_file(lua.join("SitMeansRest.lua"));
        }
        "sod" => {
            let _ = std::fs::remove_file(lua.join("SOD.lua"));
        }
        "unlimitedammo" => {
            let _ = std::fs::remove_file(lua.join("UnlimitedAmmo.lua"));
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// sql-mod family install/remove flows -- the per-type SQL apply used by both
// `install`'s and `remove`'s `sql)` arms.
// ---------------------------------------------------------------------------

/// Result of applying a `sql` family install/remove SQL step: `Ok(())` or
/// the sibling-tweak note (only meaningful for `tweak_world` install).
pub struct SqlApplyOutcome {
    pub ok: bool,
    pub sibling_note: String,
}

/// `clone_sql`/`clone_sql_norevert` install step (`90-main.sh:4780-4789`):
/// apply every up-file, fail if none were found.
pub fn sql_install_clone_sql(program: &OsStr, password: &str, scdir: &Path, emit: &dyn Fn(Value)) -> bool {
    let files = sqlmod_up_files(scdir);
    if files.is_empty() {
        return false;
    }
    for f in &files {
        emit(line_event("info", format!("applying {}...", basename_str(f))));
        if !mysql_run_file(program, password, "acore_world", f) {
            return false;
        }
    }
    true
}

/// `clone_sql_pick` install step (`90-main.sh:4790-4796`).
pub fn sql_install_clone_sql_pick(program: &OsStr, password: &str, scdir: &Path, variant: &str, emit: &dyn Fn(Value)) -> bool {
    let Some(pf) = hearthstone_pick_on_disk(scdir, variant) else { return false };
    emit(line_event("info", format!("applying {}...", basename_str(&pf))));
    mysql_run_file(program, password, "acore_world", &pf)
}

/// `clone_dist` install step (`90-main.sh:4797-4808`): sed-generate a
/// per-variant copy of every `.dist` under `db-world`, apply each.
pub fn sql_install_clone_dist(program: &OsStr, password: &str, sdir: &Path, scdir: &Path, key: &str, variant: u32, emit: &dyn Fn(Value)) -> bool {
    let dist_root = scdir.join("data").join("sql").join("db-world");
    let files = list_dist_files(&dist_root);
    if files.is_empty() {
        return false;
    }
    for (i, df) in files.iter().enumerate() {
        let gn = i + 1;
        let gf = sdir.join("sql_scripts").join("clones").join(format!("{key}_gen_{gn}.sql"));
        let Ok(content) = std::fs::read_to_string(df) else { return false };
        let patched = replace_ony_level(&content, variant);
        if std::fs::write(&gf, patched).is_err() {
            return false;
        }
        emit(line_event("info", format!("applying {} (level {variant})...", basename_str(df))));
        if !mysql_run_file(program, password, "acore_world", &gf) {
            return false;
        }
    }
    true
}

/// `tweak_world` install step (`90-main.sh:4809-4820`): reverse a
/// mutually-exclusive sibling tweak first (tweaks don't stack), then apply.
pub fn sql_install_tweak_world(program: &OsStr, password: &str, sdir: &Path, key: &str, emit: &dyn Fn(Value)) -> SqlApplyOutcome {
    let mut sibling_note = String::new();
    if let Some(sib) = tweak_installed_sibling(sdir, key) {
        emit(line_event("info", format!("removing active tweak {sib} first (tweaks don't stack)...")));
        if !tweak_reverse(program, password, sdir, &sib) {
            return SqlApplyOutcome { ok: false, sibling_note };
        }
        rm_rf(&sqlmod_marker_path(sdir, &sib));
        sibling_note = format!(" (note: the previous tweak {sib} was already removed)");
    }
    let Some((th, td, ta)) = tweak_mults(key) else { return SqlApplyOutcome { ok: false, sibling_note } };
    emit(line_event("info", format!("applying {key} (HP x{th} / DMG x{td} / ARM x{ta})...")));
    SqlApplyOutcome { ok: tweak_apply(program, password, th, td, ta), sibling_note }
}

/// `clone_sql` removal step (`90-main.sh:4925-4937`): every down-file, fail
/// with `NO_REVERT` semantics (signalled by the caller checking `dcount==0`
/// via the returned `Option`) if none exist.
pub fn sql_remove_clone_sql(program: &OsStr, password: &str, scdir: &Path, emit: &dyn Fn(Value)) -> Option<bool> {
    let files = sqlmod_down_files(scdir);
    if files.is_empty() {
        return None; // caller reports NO_REVERT
    }
    for f in &files {
        emit(line_event("info", format!("applying {}...", basename_str(f))));
        if !mysql_run_file(program, password, "acore_world", f) {
            return Some(false);
        }
    }
    Some(true)
}

/// `clone_sql_pick` removal step (`90-main.sh:4939-4942`): reset hearthstone
/// cooldown to the 30-minute default.
pub fn sql_remove_clone_sql_pick(program: &OsStr, password: &str) -> bool {
    mysql_run_stmt(
        program,
        password,
        "acore_world",
        "UPDATE spell_dbc SET RecoveryTime = 1800000, CategoryRecoveryTime = 1800000 WHERE Id = 8690;",
    )
}

/// `clone_dist` removal step (`90-main.sh:4943-4946`): delete the
/// teleporter NPCs.
pub fn sql_remove_clone_dist(program: &OsStr, password: &str) -> bool {
    mysql_run_stmt(
        program,
        password,
        "acore_world",
        "DELETE FROM creature WHERE id1 IN (190000,190001); DELETE FROM creature_template WHERE entry IN (190000,190001);",
    )
}

pub fn sql_remove_tweak_world(program: &OsStr, password: &str, sdir: &Path, key: &str) -> bool {
    tweak_reverse(program, password, sdir, key)
}

// ---------------------------------------------------------------------------
// `_wow_pull_repo` (`70-modules.sh:872-933`) — `module update`'s pull core:
// dirty check -> patch backup + stash -> `git pull --ff-only` -> stash pop
// (or conflict recovery). On failure this function has ALREADY emitted its
// own `error` event (matching the bash's `ndjson_error` + `return 1`
// contract) — the caller only needs to close its section and stop.
// ---------------------------------------------------------------------------

/// `[[ -n "$dirty" ]]` (`70-modules.sh:886`) — pure.
pub fn has_dirty_changes(raw: &str) -> bool {
    !raw.is_empty()
}

/// `[[ "$before" != "$after" ]]` (`70-modules.sh:911`) — pure.
pub fn pull_changed(before: &str, after: &str) -> bool {
    before != after
}

pub fn wow_pull_repo(program: &OsStr, dir: &Path, label: &str, emit: &dyn Fn(Value)) -> Result<bool, ()> {
    let before = git_field(program, dir, &["rev-parse", "--short", "HEAD"]);
    let dirty_raw = git_field(program, dir, &["status", "--porcelain", "--untracked-files=no"]);
    let dirty = has_dirty_changes(&dirty_raw);
    let mut patch_path: Option<PathBuf> = None;

    if dirty {
        let stamp = super::backup::format_utc_compact(
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
        );
        let patch = dir.join(format!("local-changes-{stamp}.patch"));
        match git_diff_capture(program, dir) {
            Some(bytes) if std::fs::write(&patch, &bytes).is_ok() => {}
            _ => {
                rm_rf(&patch);
                emit(error_event("EDIT_BACKUP_FAILED", format!("{label}: could not back up local changes -- aborting"), "Nothing was pulled."));
                return Err(());
            }
        }
        emit(line_event("info", format!("{label}: local changes backed up to {}", patch.display())));
        if !git_ok(program, dir, &["stash", "push", "-m", &format!("dml update auto-stash {stamp}")], GIT_PROBE_TIMEOUT) {
            emit(error_event("EDIT_BACKUP_FAILED", format!("{label}: could not stash local changes -- aborting"), "Nothing was pulled."));
            return Err(());
        }
        patch_path = Some(patch);
    }

    emit(line_event("info", format!("updating {label}...")));
    if !git_ok(program, dir, &["pull", "--ff-only"], GIT_NET_TIMEOUT) {
        if dirty {
            let _ = git_ok(program, dir, &["stash", "pop"], GIT_PROBE_TIMEOUT);
            emit(line_event("info", format!("{label}: your local changes were restored unchanged.")));
        }
        emit(error_event(
            "PULL_FAILED",
            format!("git pull failed for {label} (diverged branch?)"),
            &format!("Inspect manually: cd {} && git status", dir.display()),
        ));
        return Err(());
    }

    let after = git_field(program, dir, &["rev-parse", "--short", "HEAD"]);
    let changed = pull_changed(&before, &after);

    if dirty {
        if git_ok(program, dir, &["stash", "pop"], GIT_PROBE_TIMEOUT) {
            emit(line_event("info", format!("{label}: your local edits were re-applied on top of the update.")));
        } else {
            let _ = git_ok(program, dir, &["checkout", "-f", "--", "."], GIT_PROBE_TIMEOUT);
            let _ = git_ok(program, dir, &["reset", "--hard", "HEAD"], GIT_PROBE_TIMEOUT);
            emit(line_event(
                "warn",
                format!("{label}: some of your edits CONFLICT with this update. The updated files were kept so the server builds cleanly."),
            ));
            if let Some(p) = &patch_path {
                emit(line_event("warn", format!("Your edits are safe in two places -- patch file: {}", p.display())));
            }
            emit(line_event("warn", format!("...and git stash: cd {} && git stash pop   (resolve conflicts manually)", dir.display())));
        }
    }

    Ok(changed)
}

// ---------------------------------------------------------------------------
// Top-level per-family/-verb orchestration -- faithful ports of the
// `install)`/`remove)` arms' `case "$family" in …` bodies and the whole
// `update)` arm (`90-main.sh:4586-4841` install, 4842-4966 remove, 5472-5544
// update). Each function returns `Ok(<done-data>)` on success WITHOUT
// emitting `section_end`/`done` itself (the caller in `lib.rs` does that,
// once, after dispatch — it's identical regardless of family); on any
// domain failure the function emits `section_end(name,"error")` +
// `error_event(...)` ITSELF (matching each arm's own `ndjson_section_end …
// error; ndjson_error …` pair) and returns `Err(())`. The ONE exception is
// [`wow_pull_repo`] above, whose failures emit ONLY the error (no
// section_end) — its caller, [`update_module`], supplies the section_end
// right after, matching the update arm's own (reversed-order) `if !
// _wow_pull_repo …; then ndjson_section_end module-update error; fi`.
// ---------------------------------------------------------------------------

/// `install)`'s `cpp)` case (`90-main.sh:4606-4671`). `git_program` only —
/// cpp install/update never touches docker (module SQL lands via the
/// server's own db-import at rebuild time, never applied by hand here).
pub fn install_cpp(
    git_program: &OsStr,
    sdir: &Path,
    key: Option<&str>,
    url: Option<&str>,
    backup: Option<bool>,
    emit: &dyn Fn(Value),
) -> Result<Value, ()> {
    if backup.is_some() {
        emit(section_end(SECTION_INSTALL, "error"));
        emit(error_event(
            "BAD_ARG",
            "cpp installs don't take backup flags",
            "Module SQL lands at rebuild time -- the backup choice belongs to: dml wow module rebuild",
        ));
        return Err(());
    }

    let (mkey, murl): (String, String) = if let Some(u) = url {
        if key.is_some() {
            emit(section_end(SECTION_INSTALL, "error"));
            emit(error_event("BAD_ARG", "--url and --key are mutually exclusive", "Custom modules derive their key from the URL."));
            return Err(());
        }
        if !valid_module_url(u) {
            emit(section_end(SECTION_INSTALL, "error"));
            emit(error_event("BAD_ARG", "Invalid module URL", "https://... git URLs only"));
            return Err(());
        }
        match module_key_from_url(u) {
            Some(k) => (k, u.to_string()),
            None => {
                emit(section_end(SECTION_INSTALL, "error"));
                emit(error_event("BAD_ARG", "Custom module repos must be named mod-*", "e.g. https://github.com/you/mod-my-thing.git"));
                return Err(());
            }
        }
    } else {
        let k = key.unwrap_or("");
        if !valid_cpp_key(k) {
            emit(section_end(SECTION_INSTALL, "error"));
            emit(error_event("BAD_ARG", format!("Invalid module key: {k}"), ""));
            return Err(());
        }
        match cpp_row(k) {
            Some(row) => (k.to_string(), row.url.to_string()),
            None => {
                emit(section_end(SECTION_INSTALL, "error"));
                emit(error_event("BAD_ARG", format!("Unknown module: {k}"), "Pass --url for a custom module."));
                return Err(());
            }
        }
    };

    let mdir = sdir.join("modules").join(&mkey);
    let action = if cpp_installed(sdir, &mkey) {
        emit(line_event("info", format!("updating {mkey}...")));
        if !git_ok(git_program, &mdir, &["pull", "--depth", "1"], GIT_NET_TIMEOUT) {
            emit(section_end(SECTION_INSTALL, "error"));
            emit(error_event("GIT_FAILED", format!("git pull failed for {mkey}"), ""));
            return Err(());
        }
        "updated"
    } else {
        let _ = std::fs::create_dir_all(sdir.join("modules"));
        if mdir.is_dir() && !mdir.join(".git").is_dir() {
            rm_rf(&mdir);
        }
        emit(line_event("info", format!("cloning {mkey}...")));
        if !git_clone_depth1(git_program, &murl, &mdir) {
            emit(section_end(SECTION_INSTALL, "error"));
            emit(error_event("GIT_FAILED", format!("git clone failed for {mkey}"), ""));
            return Err(());
        }
        "installed"
    };

    // mod-arac ships NO C++ (data-only: SQL + DBC + MPQ) -- a rebuild would
    // be a 30-90 minute no-op, so it never joins the rebuild-pending list.
    let rebuild_required = if mkey == "mod-arac" {
        emit(line_event(
            "info",
            "mod-arac is data-only: no rebuild needed. Next: Apply client patch (Modules page), then restart.",
        ));
        false
    } else {
        let _ = rebuild_pending_add(sdir, &mkey);
        true
    };
    emit(line_event("info", "module SQL (if any) is applied automatically by the server's db-import on next start -- never by hand"));
    Ok(serde_json::json!({"key": mkey, "action": action, "rebuild_required": rebuild_required}))
}

/// `install)`'s `lua)` case (`90-main.sh:4672-4724`).
pub fn install_lua(
    git_program: &OsStr,
    docker_program: &OsStr,
    sdir: &Path,
    key: &str,
    url: Option<&str>,
    backup: Option<bool>,
    password: &str,
    client_path: Option<&Path>,
    emit: &dyn Fn(Value),
) -> Result<Value, ()> {
    let Some(lrow) = lua_row(key) else {
        emit(section_end(SECTION_INSTALL, "error"));
        emit(error_event("BAD_ARG", format!("Unknown lua script: {key}"), "Lua scripts come from the registry only."));
        return Err(());
    };
    if url.is_some() {
        emit(section_end(SECTION_INSTALL, "error"));
        emit(error_event("BAD_ARG", "--url is not supported for lua scripts", ""));
        return Err(());
    }
    if !cpp_installed(sdir, "mod-ale") {
        emit(section_end(SECTION_INSTALL, "error"));
        emit(error_event("NOT_READY", "Install the ALE module (mod-ale) first", "It's in the C++ modules list."));
        return Err(());
    }
    let has_sql = lua_has_sql(key);
    if has_sql {
        match backup {
            None => {
                emit(section_end(SECTION_INSTALL, "error"));
                emit(error_event("BAD_ARG", "Pick --backup or --no-backup", "This script applies SQL to the database."));
                return Err(());
            }
            Some(true) => {
                if !module_backup_now(docker_program, password, emit) {
                    emit(section_end(SECTION_INSTALL, "error"));
                    emit(error_event("BACKUP_FAILED", "Safety backup failed — nothing was installed", ""));
                    return Err(());
                }
            }
            Some(false) => {}
        }
    } else if backup.is_some() {
        emit(section_end(SECTION_INSTALL, "error"));
        emit(error_event("BAD_ARG", format!("{key} applies no SQL — backup flags don't apply"), ""));
        return Err(());
    }

    if !lua_clone(git_program, sdir, key, lrow.url, emit) {
        emit(section_end(SECTION_INSTALL, "error"));
        emit(error_event("GIT_FAILED", format!("git failed for {key}"), ""));
        return Err(());
    }
    if !lua_deploy(sdir, key, emit) {
        emit(section_end(SECTION_INSTALL, "error"));
        emit(error_event("DEPLOY_FAILED", format!("Could not deploy {key} lua files"), "The clone's layout was not what the installer expects."));
        return Err(());
    }
    if has_sql && !lua_apply_sql(docker_program, password, sdir, key, emit) {
        emit(section_end(SECTION_INSTALL, "error"));
        emit(error_event(
            "SQL_FAILED",
            format!("SQL for {key} failed"),
            "Is ac-database running? The lua files are deployed; re-run install once the DB is up.",
        ));
        return Err(());
    }
    lua_client_copy(sdir, key, client_path, emit);
    let relmsg = if key == "bmah" {
        "restart the server (new creature_template rows don't hot-load)"
    } else {
        ".reload ale (in-game or Console page)"
    };
    emit(line_event("info", format!("done — {relmsg}")));
    Ok(serde_json::json!({"key": key, "action": "installed", "reload": relmsg}))
}

/// `install)`'s `sql)` case (`90-main.sh:4725-4835`).
pub fn install_sql(
    git_program: &OsStr,
    docker_program: &OsStr,
    sdir: &Path,
    key: &str,
    url: Option<&str>,
    backup: Option<bool>,
    variant: Option<&str>,
    password: &str,
    emit: &dyn Fn(Value),
) -> Result<Value, ()> {
    let Some(srow) = sql_row(key) else {
        emit(section_end(SECTION_INSTALL, "error"));
        emit(error_event("BAD_ARG", format!("Unknown SQL mod: {key}"), ""));
        return Err(());
    };
    if url.is_some() {
        emit(section_end(SECTION_INSTALL, "error"));
        emit(error_event("BAD_ARG", "--url is not supported for SQL mods", ""));
        return Err(());
    }
    let stype = srow.kind;
    let surl = srow.url;
    let _ = sqlmod_dirs(sdir);
    let marker = sqlmod_marker_path(sdir, key);
    if marker.is_file() {
        emit(section_end(SECTION_INSTALL, "error"));
        emit(error_event("EXISTS", format!("{key} is already installed"), "Remove it first to re-apply."));
        return Err(());
    }
    if backup.is_none() {
        emit(section_end(SECTION_INSTALL, "error"));
        emit(error_event("BAD_ARG", "Pick --backup or --no-backup", "SQL mods change the world database."));
        return Err(());
    }
    if stype == "clone_sql_pick" && !valid_hearthstone_variant(variant.unwrap_or("")) {
        emit(section_end(SECTION_INSTALL, "error"));
        emit(error_event("BAD_ARG", "hearthstone-cd needs --variant 1sec|1min|5min|15min|30min", ""));
        return Err(());
    }
    let mut teleporter_variant: u32 = 0;
    if stype == "clone_dist" {
        match normalize_teleporter_variant(variant) {
            Ok(n) => teleporter_variant = n,
            Err(()) => {
                emit(section_end(SECTION_INSTALL, "error"));
                emit(error_event("BAD_ARG", "npc-teleporter --variant is a level 1-80", ""));
                return Err(());
            }
        }
    }
    if backup == Some(true) && !module_backup_now(docker_program, password, emit) {
        emit(section_end(SECTION_INSTALL, "error"));
        emit(error_event("BACKUP_FAILED", "Safety backup failed — nothing was installed", ""));
        return Err(());
    }

    let scdir = sqlmod_clone_dir(sdir, key);
    if !surl.is_empty() && !scdir.join(".git").is_dir() {
        rm_rf(&scdir);
        emit(line_event("info", format!("cloning {key}...")));
        if !git_clone_depth1(git_program, surl, &scdir) {
            emit(section_end(SECTION_INSTALL, "error"));
            emit(error_event("GIT_FAILED", format!("git clone failed for {key}"), ""));
            return Err(());
        }
    }

    let sqlfail;
    let mut sibnote = String::new();
    match stype {
        "clone_sql" | "clone_sql_norevert" => {
            sqlfail = !sql_install_clone_sql(docker_program, password, &scdir, emit);
        }
        "clone_sql_pick" => {
            sqlfail = !sql_install_clone_sql_pick(docker_program, password, &scdir, variant.unwrap_or(""), emit);
        }
        "clone_dist" => {
            sqlfail = !sql_install_clone_dist(docker_program, password, sdir, &scdir, key, teleporter_variant, emit);
        }
        "tweak_world" => {
            let outcome = sql_install_tweak_world(docker_program, password, sdir, key, emit);
            sqlfail = !outcome.ok;
            sibnote = outcome.sibling_note;
        }
        _ => {
            sqlfail = false;
        }
    }
    if sqlfail {
        emit(section_end(SECTION_INSTALL, "error"));
        emit(error_event("SQL_FAILED", format!("SQL for {key} failed"), &format!("Is ac-database running? Nothing was marked installed.{sibnote}")));
        return Err(());
    }

    let marker_content = match stype {
        "clone_sql_pick" => format!("HEARTHSTONE_COOLDOWN={}\n", variant.unwrap_or("")),
        "tweak_world" => {
            let (th, td, ta) = tweak_mults(key).unwrap_or(("", "", ""));
            format!("APPLIED_HP_MULT={th}\nAPPLIED_DMG_MULT={td}\nAPPLIED_ARM_MULT={ta}\n")
        }
        _ => String::new(),
    };
    let _ = std::fs::write(&marker, marker_content);
    Ok(serde_json::json!({"key": key, "action": "installed", "type": stype}))
}

/// `remove)`'s `cpp)` case (`90-main.sh:4860-4880`).
pub fn remove_cpp(sdir: &Path, key: &str, backup: Option<bool>, emit: &dyn Fn(Value)) -> Result<Value, ()> {
    if backup.is_some() {
        emit(section_end(SECTION_REMOVE, "error"));
        emit(error_event("BAD_ARG", "cpp removals don't take backup flags", "The compiled code leaves at rebuild time."));
        return Err(());
    }
    if !valid_cpp_key(key) || !cpp_installed(sdir, key) {
        emit(section_end(SECTION_REMOVE, "error"));
        emit(error_event("NOT_FOUND", format!("Module not installed: {key}"), ""));
        return Err(());
    }
    if key == "mod-arac" {
        emit(line_event("warn", "mod-arac is data-only. Removing the clone does NOT revert:"));
        emit(line_event("warn", "  - arac.sql data already imported into acore_world"));
        emit(line_event("warn", "  - DBC files already copied to the server data volume"));
        emit(line_event("warn", "  - Patch-A.MPQ already installed in your WoW client Data/"));
    }
    rm_rf(&sdir.join("modules").join(key));
    let _ = rebuild_pending_add(sdir, key);
    emit(line_event("info", "database rows from this module are kept -- removing them risks data loss"));
    Ok(serde_json::json!({"key": key, "removed": true, "rebuild_required": true}))
}

/// `remove)`'s `lua)` case (`90-main.sh:4881-4896`).
pub fn remove_lua(sdir: &Path, key: &str, backup: Option<bool>, emit: &dyn Fn(Value)) -> Result<Value, ()> {
    if backup.is_some() {
        emit(section_end(SECTION_REMOVE, "error"));
        emit(error_event(
            "BAD_ARG",
            "lua removal never touches the database — backup flags don't apply",
            "Tables the script created are kept.",
        ));
        return Err(());
    }
    if lua_row(key).is_none() {
        emit(section_end(SECTION_REMOVE, "error"));
        emit(error_event("BAD_ARG", format!("Unknown lua script: {key}"), ""));
        return Err(());
    }
    if !lua_cloned(sdir, key) && !lua_deployed(sdir, key) {
        emit(section_end(SECTION_REMOVE, "error"));
        emit(error_event("NOT_FOUND", format!("Lua script not installed: {key}"), ""));
        return Err(());
    }
    rm_rf(&sdir.join("ale_scripts").join(key));
    lua_remove_deployed(sdir, key);
    emit(line_event("info", "database tables created by this script are kept — removing them risks data loss"));
    emit(line_event("info", "client-side files (if any) are kept"));
    Ok(serde_json::json!({"key": key, "removed": true}))
}

/// `remove)`'s `sql)` case (`90-main.sh:4897-4960`). `docker_program` only —
/// removal never touches git.
pub fn remove_sql(docker_program: &OsStr, sdir: &Path, key: &str, backup: Option<bool>, password: &str, emit: &dyn Fn(Value)) -> Result<Value, ()> {
    let Some(srow) = sql_row(key) else {
        emit(section_end(SECTION_REMOVE, "error"));
        emit(error_event("BAD_ARG", format!("Unknown SQL mod: {key}"), ""));
        return Err(());
    };
    let stype = srow.kind;
    let _ = sqlmod_dirs(sdir);
    let marker = sqlmod_marker_path(sdir, key);
    if !marker.is_file() {
        emit(section_end(SECTION_REMOVE, "error"));
        emit(error_event("NOT_FOUND", format!("SQL mod not installed: {key}"), ""));
        return Err(());
    }
    if stype == "clone_sql_norevert" {
        emit(section_end(SECTION_REMOVE, "error"));
        emit(error_event("NO_REVERT", format!("{key} has no automated reversal SQL"), "Restore a backup from the Backups page instead."));
        return Err(());
    }
    if backup.is_none() {
        emit(section_end(SECTION_REMOVE, "error"));
        emit(error_event("BAD_ARG", "Pick --backup or --no-backup", "Removal changes the world database."));
        return Err(());
    }
    if backup == Some(true) && !module_backup_now(docker_program, password, emit) {
        emit(section_end(SECTION_REMOVE, "error"));
        emit(error_event("BACKUP_FAILED", "Safety backup failed — nothing was removed", ""));
        return Err(());
    }

    let scdir = sqlmod_clone_dir(sdir, key);
    let ok = match stype {
        "clone_sql" => match sql_remove_clone_sql(docker_program, password, &scdir, emit) {
            None => {
                emit(section_end(SECTION_REMOVE, "error"));
                emit(error_event("NO_REVERT", format!("{key}'s clone has no down.sql"), "Restore a backup instead."));
                return Err(());
            }
            Some(ok) => ok,
        },
        "clone_sql_pick" => {
            emit(line_event("info", "resetting hearthstone cooldown to the 30-minute default..."));
            sql_remove_clone_sql_pick(docker_program, password)
        }
        "clone_dist" => {
            emit(line_event("info", "deleting teleporter NPCs..."));
            sql_remove_clone_dist(docker_program, password)
        }
        "tweak_world" => {
            emit(line_event("info", format!("reversing {key} multipliers...")));
            sql_remove_tweak_world(docker_program, password, sdir, key)
        }
        _ => true,
    };
    if !ok {
        emit(section_end(SECTION_REMOVE, "error"));
        emit(error_event("SQL_FAILED", format!("Reversal SQL for {key} failed"), "Is ac-database running? The installed marker was kept."));
        return Err(());
    }
    let _ = std::fs::remove_file(&marker);
    rm_rf(&scdir);
    Ok(serde_json::json!({"key": key, "removed": true, "type": stype}))
}

/// `update)` (`90-main.sh:5472-5544`). `git_program` only — module update
/// never touches docker (no SQL is applied here; that's the server's own
/// db-import at rebuild time).
pub fn update_module(git_program: &OsStr, sdir: &Path, key: &str, emit: &dyn Fn(Value)) -> Result<Value, ()> {
    if !valid_cpp_key(key) {
        emit(section_end(SECTION_UPDATE, "error"));
        emit(error_event("BAD_ARG", format!("Invalid module key: {key}"), "Usage: dml wow module update --key mod-<name> --json"));
        return Err(());
    }
    let umdir = sdir.join("modules").join(key);
    if !umdir.is_dir() {
        emit(section_end(SECTION_UPDATE, "error"));
        emit(error_event("NOT_FOUND", format!("{key} is not installed"), "Install it on the Modules page first."));
        return Err(());
    }
    if !is_git_checkout(&umdir) {
        emit(section_end(SECTION_UPDATE, "error"));
        emit(error_event("GIT_MISSING", format!("{} is not a git checkout", umdir.display()), "Can't update this module from source."));
        return Err(());
    }
    // mod-playerbots tracks the custom AzerothCore fork -- pulling it alone
    // would desync the pair (`wow update` pulls both together).
    if key == "mod-playerbots" {
        emit(section_end(SECTION_UPDATE, "error"));
        emit(error_event(
            "BAD_ARG",
            "mod-playerbots cannot be updated on its own",
            "mod-playerbots updates together with the server core - use Server update on the Modules page",
        ));
        return Err(());
    }
    let uurl = git_remote_url(git_program, &umdir);
    if uurl.is_empty() {
        emit(section_end(SECTION_UPDATE, "error"));
        emit(error_event(
            "REMOTE_MISSING",
            format!("{key} has no origin remote -- nothing to pull from"),
            &format!("Fix it manually: cd {} && git remote add origin <url>", umdir.display()),
        ));
        return Err(());
    }

    let ubefore = git_short_head(git_program, &umdir);
    // `_wow_pull_repo` emits its OWN error event on failure but no
    // section_end -- the arm's own (reversed-order) contract -- so this is
    // the one call site where the caller supplies section_end AFTER the
    // error has already gone out.
    let changed = match wow_pull_repo(git_program, &umdir, key, emit) {
        Ok(c) => c,
        Err(()) => {
            emit(section_end(SECTION_UPDATE, "error"));
            return Err(());
        }
    };
    let uafter = git_short_head(git_program, &umdir);

    let mut pending_rebuild = false;
    if changed {
        if key == "mod-arac" {
            emit(line_event(
                "info",
                "mod-arac is data-only: no rebuild needed. Next: Apply client patch (Modules page), then restart.",
            ));
        } else {
            let _ = rebuild_pending_add(sdir, key);
            pending_rebuild = true;
            emit(line_event("info", "Rebuild required to compile the update -- use the rebuild banner on the Modules page."));
        }
        emit(line_event("info", "module SQL (if any) is applied automatically by the server's db-import on next start -- never by hand"));
    }

    Ok(serde_json::json!({"key": key, "changed": changed, "before": ubefore, "after": uafter, "pending_rebuild": pending_rebuild}))
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- registries ------------------------------------------------------------

    #[test]
    fn registry_row_counts() {
        assert_eq!(CPP_REGISTRY.len(), 19);
        assert_eq!(LUA_REGISTRY.len(), 9);
        assert_eq!(SQL_REGISTRY.len(), 10);
    }

    #[test]
    fn registry_spot_rows() {
        let ale = cpp_row("mod-ale").expect("mod-ale");
        assert_eq!(ale.url, "https://github.com/azerothcore/mod-ale.git");
        assert_eq!(ale.sqldirs, "");
        let arac = cpp_row("mod-arac").expect("mod-arac");
        assert!(arac.name.contains("ARAC"));
        assert!(cpp_row("mod-does-not-exist").is_none());

        let bmah = lua_row("bmah").expect("bmah");
        assert_eq!(bmah.url, "https://github.com/DadsMmoLab/dads-mmo-lab.git");
        assert!(lua_row("nope").is_none());

        let hs = sql_row("hearthstone-cd").expect("hearthstone-cd");
        assert_eq!(hs.kind, "clone_sql_pick");
        let baby = sql_row("baby-mobs").expect("baby-mobs");
        assert_eq!(baby.url, "");
        assert_eq!(baby.kind, "tweak_world");
        assert!(sql_row("nope").is_none());
    }

    // -- validators --------------------------------------------------------------

    #[test]
    fn valid_module_key_shape() {
        assert!(valid_module_key("solocraft"));
        assert!(valid_module_key("a"));
        assert!(!valid_module_key(""));
        assert!(!valid_module_key("Solocraft"));
        assert!(!valid_module_key("solo_craft"));
        assert!(valid_module_key(&"a".repeat(64)));
        assert!(!valid_module_key(&"a".repeat(65)));
    }

    #[test]
    fn valid_module_url_shape() {
        assert!(valid_module_url("https://github.com/azerothcore/mod-solocraft.git"));
        assert!(valid_module_url("https://github.com/azerothcore/mod-solocraft"));
        assert!(!valid_module_url("http://github.com/x/y.git")); // http, not https
        assert!(!valid_module_url("https://"));
        assert!(!valid_module_url("ftp://x/y.git"));
        assert!(!valid_module_url("https://x/y with space.git"));
    }

    #[test]
    fn module_key_from_url_derives_and_rejects() {
        assert_eq!(module_key_from_url("https://github.com/you/mod-my-thing.git").as_deref(), Some("mod-my-thing"));
        assert_eq!(module_key_from_url("https://github.com/you/Mod-My-Thing.git").as_deref(), Some("mod-my-thing"));
        assert_eq!(module_key_from_url("https://github.com/you/mod-my-thing").as_deref(), Some("mod-my-thing"));
        assert_eq!(module_key_from_url("https://github.com/you/not-a-mod-repo"), None);
        assert_eq!(module_key_from_url("https://github.com/you/mod-"), None); // needs >=1 char after prefix
    }

    // -- rebuild-pending append/dedup --------------------------------------------

    fn tmp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("dml-modmgr-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn rebuild_pending_add_appends_and_dedups_byte_exact() {
        let d = tmp_dir("pending");
        rebuild_pending_add(&d, "mod-a").unwrap();
        rebuild_pending_add(&d, "mod-b").unwrap();
        rebuild_pending_add(&d, "mod-a").unwrap(); // dup, must not append again
        let content = std::fs::read_to_string(d.join(".dml-rebuild-pending")).unwrap();
        assert_eq!(content, "mod-a\nmod-b\n");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn rebuild_pending_add_creates_file_when_absent() {
        let d = tmp_dir("pending-new");
        assert!(!d.join(".dml-rebuild-pending").exists());
        rebuild_pending_add(&d, "core-update").unwrap();
        assert_eq!(std::fs::read_to_string(d.join(".dml-rebuild-pending")).unwrap(), "core-update\n");
        let _ = std::fs::remove_dir_all(&d);
    }

    // -- lua_has_sql / cpp_installed / sql_installed / lua_deployed --------------

    #[test]
    fn lua_has_sql_gate() {
        assert!(lua_has_sql("accountwide"));
        assert!(lua_has_sql("battlepass"));
        assert!(lua_has_sql("bmah"));
        assert!(lua_has_sql("paragon"));
        assert!(!lua_has_sql("lootpet"));
        assert!(!lua_has_sql("sod"));
    }

    #[test]
    fn cpp_installed_checks_git_dir() {
        let d = tmp_dir("cpp-installed");
        assert!(!cpp_installed(&d, "mod-solocraft"));
        std::fs::create_dir_all(d.join("modules").join("mod-solocraft").join(".git")).unwrap();
        assert!(cpp_installed(&d, "mod-solocraft"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn sql_installed_checks_marker_file() {
        let d = tmp_dir("sql-installed");
        assert!(!sql_installed(&d, "rare-drops"));
        sqlmod_dirs(&d).unwrap();
        std::fs::write(sqlmod_marker_path(&d, "rare-drops"), "").unwrap();
        assert!(sql_installed(&d, "rare-drops"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn lua_deployed_per_key_presence() {
        let d = tmp_dir("lua-deployed");
        let lua = lua_scripts_dir(&d);
        std::fs::create_dir_all(&lua).unwrap();
        assert!(!lua_deployed(&d, "bmah"));
        std::fs::write(lua.join("BMAH.lua"), "").unwrap();
        assert!(lua_deployed(&d, "bmah"));
        assert!(!lua_deployed(&d, "unknown-key"));
        let _ = std::fs::remove_dir_all(&d);
    }

    // -- remove_lua key gate (path-traversal regression) --------------------------

    #[test]
    fn remove_lua_rejects_unregistered_key_before_touching_any_path() {
        let d = tmp_dir("remove-lua-guard");
        // Simulate the server dir being a git checkout, like the real WoW
        // server dir always is.
        std::fs::create_dir_all(d.join(".git")).unwrap();
        std::fs::write(d.join("sentinel"), "still here").unwrap();

        let noop = |_ev: Value| {};
        let result = remove_lua(&d, "..", None, &noop);
        assert!(result.is_err());
        // The sdir and its .git must survive untouched -- this key must never
        // reach `rm_rf`.
        assert!(d.join(".git").is_dir());
        assert!(d.join("sentinel").is_file());

        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn remove_lua_rejects_key_not_in_registry_even_if_dirs_happen_to_exist() {
        let d = tmp_dir("remove-lua-guard-unknown");
        std::fs::create_dir_all(d.join("ale_scripts").join("not-a-real-key").join(".git")).unwrap();

        let noop = |_ev: Value| {};
        let result = remove_lua(&d, "not-a-real-key", None, &noop);
        assert!(result.is_err());
        assert!(d.join("ale_scripts").join("not-a-real-key").is_dir());

        let _ = std::fs::remove_dir_all(&d);
    }

    // -- sqlmod up/down file classification ---------------------------------------

    #[test]
    fn is_up_file_excludes_down_and_example_case_insensitive() {
        assert!(is_up_file("001_up.sql"));
        assert!(!is_up_file("001_down.sql"));
        assert!(!is_up_file("001_DOWN.sql"));
        assert!(!is_up_file("example_seed.sql"));
        assert!(!is_up_file("Example.sql"));
    }

    #[test]
    fn is_down_file_matches_down_case_insensitive_only() {
        assert!(is_down_file("001_down.sql"));
        assert!(is_down_file("001_DOWN.sql"));
        assert!(!is_down_file("001_up.sql"));
        assert!(!is_down_file("example.sql"));
    }

    #[test]
    fn sqlmod_up_and_down_files_walk_recursively_and_sort() {
        let d = tmp_dir("sqlmod-files");
        std::fs::create_dir_all(d.join("sub")).unwrap();
        std::fs::write(d.join("b_up.sql"), "").unwrap();
        std::fs::write(d.join("a_up.sql"), "").unwrap();
        std::fs::write(d.join("sub").join("c_up.sql"), "").unwrap();
        std::fs::write(d.join("z_down.sql"), "").unwrap();
        std::fs::write(d.join("example.sql"), "").unwrap();
        std::fs::write(d.join("notsql.txt"), "").unwrap();
        let up = sqlmod_up_files(&d);
        assert_eq!(up.len(), 3);
        assert!(up.iter().all(|p| basename_str(p) != "z_down.sql" && basename_str(p) != "example.sql"));
        let down = sqlmod_down_files(&d);
        assert_eq!(down.len(), 1);
        assert_eq!(basename_str(&down[0]), "z_down.sql");
        let _ = std::fs::remove_dir_all(&d);
    }

    // -- hearthstone-cd picker -----------------------------------------------------

    #[test]
    fn hearthstone_pick_disambiguates_5min_from_15min() {
        let paths = vec![
            "/scdir/hearthstone_15min.sql".to_string(),
            "/scdir/hearthstone_5min.sql".to_string(),
        ];
        assert_eq!(hearthstone_pick(&paths, "5min").as_deref(), Some("/scdir/hearthstone_5min.sql"));
        assert_eq!(hearthstone_pick(&paths, "15min").as_deref(), Some("/scdir/hearthstone_15min.sql"));
    }

    #[test]
    fn hearthstone_pick_case_insensitive_and_sorted() {
        let paths = vec!["/scdir/B_1sec.SQL".to_string(), "/scdir/A_1sec.sql".to_string()];
        assert_eq!(hearthstone_pick(&paths, "1sec").as_deref(), Some("/scdir/A_1sec.sql"));
    }

    #[test]
    fn hearthstone_pick_none_when_no_match() {
        let paths = vec!["/scdir/foo.sql".to_string()];
        assert_eq!(hearthstone_pick(&paths, "30min"), None);
    }

    #[test]
    fn valid_hearthstone_variant_allowlist() {
        for v in ["1sec", "1min", "5min", "15min", "30min"] {
            assert!(valid_hearthstone_variant(v));
        }
        for v in ["", "2min", "1SEC", "60min"] {
            assert!(!valid_hearthstone_variant(v));
        }
    }

    // -- npc-teleporter variant normalization + sed port --------------------------

    #[test]
    fn normalize_teleporter_variant_defaults_and_bounds() {
        assert_eq!(normalize_teleporter_variant(None), Ok(80));
        assert_eq!(normalize_teleporter_variant(Some("")), Ok(80));
        assert_eq!(normalize_teleporter_variant(Some("1")), Ok(1));
        assert_eq!(normalize_teleporter_variant(Some("80")), Ok(80));
        assert_eq!(normalize_teleporter_variant(Some("007")), Ok(7)); // base-10 forced, no octal
        assert_eq!(normalize_teleporter_variant(Some("0")), Err(()));
        assert_eq!(normalize_teleporter_variant(Some("81")), Err(()));
        assert_eq!(normalize_teleporter_variant(Some("abc")), Err(()));
        assert_eq!(normalize_teleporter_variant(Some("-5")), Err(()));
    }

    #[test]
    fn replace_ony_level_first_occurrence_per_line_only() {
        let input = "SET @ONY_LEVEL := 63;\nno match here\n@ONY_LEVEL := 1 and @ONY_LEVEL := 2\n";
        let out = replace_ony_level(input, 40);
        assert_eq!(out, "SET @ONY_LEVEL := 40;\nno match here\n@ONY_LEVEL := 40 and @ONY_LEVEL := 2\n");
    }

    #[test]
    fn replace_ony_level_handles_zero_digits() {
        assert_eq!(replace_ony_level_line("@ONY_LEVEL := ;", 55), "@ONY_LEVEL := 55;");
    }

    // -- tweak_world ----------------------------------------------------------------

    #[test]
    fn tweak_mults_known_and_unknown_keys() {
        assert_eq!(tweak_mults("baby-mobs"), Some(("0.25", "0.25", "0.25")));
        assert_eq!(tweak_mults("buff-mobs"), Some(("2", "1.5", "1.5")));
        assert_eq!(tweak_mults("xbuff-mobs"), Some(("4", "2", "2")));
        assert_eq!(tweak_mults("nerf-mobs"), Some(("0.5", "0.75", "0.75")));
        assert_eq!(tweak_mults("rare-drops"), None);
    }

    #[test]
    fn tweak_installed_sibling_finds_the_other_marker_excludes_self() {
        let d = tmp_dir("tweak-sibling");
        sqlmod_dirs(&d).unwrap();
        assert_eq!(tweak_installed_sibling(&d, "buff-mobs"), None);
        std::fs::write(sqlmod_marker_path(&d, "nerf-mobs"), "").unwrap();
        assert_eq!(tweak_installed_sibling(&d, "buff-mobs").as_deref(), Some("nerf-mobs"));
        assert_eq!(tweak_installed_sibling(&d, "nerf-mobs"), None); // excludes self
        let _ = std::fs::remove_dir_all(&d);
    }

    // -- pull_repo pure pieces --------------------------------------------------------

    #[test]
    fn has_dirty_changes_empty_vs_nonempty() {
        assert!(!has_dirty_changes(""));
        assert!(has_dirty_changes(" M some/file.lua"));
    }

    #[test]
    fn pull_changed_before_after() {
        assert!(!pull_changed("abc1234", "abc1234"));
        assert!(pull_changed("abc1234", "def5678"));
    }

    #[test]
    fn stamp_format_matches_date_dash_u() {
        // `_wow_pull_repo`'s stamp reuses `backup::format_utc_compact`
        // (`date -u +%Y%m%d-%H%M%S`) -- already covered by that module's own
        // tests; this just asserts the shape modmgr depends on.
        let s = super::super::backup::format_utc_compact(1_700_000_000);
        assert_eq!(s, "20231114-221320");
        assert_eq!(s.len(), 15);
        assert_eq!(s.as_bytes()[8], b'-');
    }

    // -- copy_dir_contents ------------------------------------------------------------

    #[test]
    fn copy_dir_contents_recursively_merges() {
        let d = tmp_dir("copy-dir");
        let src = d.join("src");
        let dst = d.join("dst");
        std::fs::create_dir_all(src.join("nested")).unwrap();
        std::fs::write(src.join("a.txt"), "A").unwrap();
        std::fs::write(src.join("nested").join("b.txt"), "B").unwrap();
        std::fs::create_dir_all(&dst).unwrap();
        std::fs::write(dst.join("existing.txt"), "keep me").unwrap();
        copy_dir_contents(&src, &dst).unwrap();
        assert_eq!(std::fs::read_to_string(dst.join("a.txt")).unwrap(), "A");
        assert_eq!(std::fs::read_to_string(dst.join("nested").join("b.txt")).unwrap(), "B");
        assert_eq!(std::fs::read_to_string(dst.join("existing.txt")).unwrap(), "keep me");
        let _ = std::fs::remove_dir_all(&d);
    }

    // -- paragon migration skip ---------------------------------------------------

    #[test]
    fn paragon_migration_skip_dates_and_examples() {
        assert!(paragon_migration_skip("26-07-19_desc.sql"));
        assert!(paragon_migration_skip("Example_data.sql"));
        assert!(paragon_migration_skip("some_example.sql"));
        assert!(!paragon_migration_skip("02_add_column.sql"));
        assert!(!paragon_migration_skip("01_create_database.sql")); // caller special-cases this one separately, but the predicate itself doesn't skip it
    }

    // -- wow_remote_ok / pull_summary (Chunk 3b self-update primitives) ---------

    #[test]
    fn wow_remote_ok_matches_expected_substring() {
        assert!(wow_remote_ok("https://github.com/mod-playerbots/azerothcore-wotlk.git", "mod-playerbots/azerothcore-wotlk"));
        assert!(wow_remote_ok("git@github.com:mod-playerbots/mod-playerbots.git", "mod-playerbots/mod-playerbots"));
        assert!(!wow_remote_ok("https://github.com/azerothcore/azerothcore-wotlk.git", "mod-playerbots/azerothcore-wotlk"));
    }

    #[test]
    fn wow_remote_ok_accepts_legacy_liyunfan1223_org() {
        // Old installs may still point at the pre-rename org; GitHub redirects
        // those, so they're accepted even though the substring doesn't match.
        assert!(wow_remote_ok("https://github.com/liyunfan1223/mod-playerbots.git", "mod-playerbots/mod-playerbots"));
    }

    #[test]
    fn wow_remote_ok_empty_url_rejected() {
        assert!(!wow_remote_ok("", "mod-playerbots/azerothcore-wotlk"));
    }

    #[test]
    fn pull_summary_up_to_date_when_unchanged() {
        assert_eq!(pull_summary("abc1234", "abc1234"), "up to date");
    }

    #[test]
    fn pull_summary_before_arrow_after_when_changed() {
        assert_eq!(pull_summary("abc1234", "def5678"), "abc1234 -> def5678");
    }

    // -- update_gate_order (Chunk 3b self-update gate ORDER) --------------------

    #[test]
    fn update_gate_order_all_pass_is_none() {
        assert_eq!(update_gate_order(true, true, true, true, true, true, Some(true)), None);
        assert_eq!(update_gate_order(true, true, true, false, true, true, Some(false)), None);
    }

    #[test]
    fn update_gate_order_not_found_first() {
        // Every other input is deliberately also-failing, to prove NOT_FOUND
        // wins regardless -- it is checked first.
        assert_eq!(update_gate_order(false, false, false, true, false, false, None), Some("NOT_FOUND"));
    }

    #[test]
    fn update_gate_order_git_missing_before_remote_checks() {
        assert_eq!(update_gate_order(true, false, false, true, false, false, None), Some("GIT_MISSING"));
    }

    #[test]
    fn update_gate_order_ac_remote_mismatch_before_branch_and_backup() {
        assert_eq!(update_gate_order(true, true, false, false, true, false, None), Some("REMOTE_MISMATCH"));
    }

    #[test]
    fn update_gate_order_pb_remote_mismatch_only_when_pb_present() {
        // pb absent -> its own remote check is skipped entirely, so a "bad"
        // pb_remote_ok=false doesn't matter.
        assert_eq!(update_gate_order(true, true, true, false, false, true, Some(true)), None);
        // pb present + bad remote -> rejected.
        assert_eq!(update_gate_order(true, true, true, true, false, true, Some(true)), Some("REMOTE_MISMATCH"));
    }

    #[test]
    fn update_gate_order_branch_mismatch_before_backup_choice() {
        assert_eq!(update_gate_order(true, true, true, false, true, false, None), Some("BRANCH_MISMATCH"));
    }

    #[test]
    fn update_gate_order_bad_arg_when_backup_choice_missing() {
        assert_eq!(update_gate_order(true, true, true, false, true, true, None), Some("BAD_ARG"));
    }
}
