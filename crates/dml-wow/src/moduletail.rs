//! Native-mode **module update-check / conf-activate / tracking / repair /
//! fixit / place-npc / client-patch** (Chunk 5, Part 5b). Faithful port of
//! the matching sub-arms of `cli/src/90-main.sh`'s `module)` case (conf-
//! activate 5026-5050, tracking 5052-5106, repair 5107-5180, fixit 5181-
//! 5253, place-npc 5254-5345, client-patch 5346-5432, update-check 5434-
//! 5470) plus their `cli/src/70-modules.sh`/`cli/src/47-commands.sh`
//! helpers (`_module_conf_name_var`/`_module_conf_dist`/`_module_conf_
//! state_var`, `_module_discover_sql_files`, `_valid_module_sql_filename`,
//! `_npc_coord_specs`).
//!
//! ARCHITECTURE. Pure lookups/parses/SQL-text live here (no DB/docker/fs
//! mutation), mirroring `dml::modmgr`'s split between "library" (this file's
//! shape) and the orchestration that lives in `lib.rs`. The handful of
//! filesystem READS needed to answer conf-state/discover-sql-files (both
//! read-only, `std::fs`) live here too since they're pure enough to unit
//! test against a `tempfile` tree, same convention as `dml::config`'s
//! `cfg_file_path`.

use std::path::{Path, PathBuf};

use dml_core::error::{io_internal_err, not_found_err, CmdError};
use serde_json::Value;

use super::db::{count_result, db_unreachable_err};

// ---------------------------------------------------------------------------
// conf-activate / `module conf` — `_module_conf_name_var`/`_module_conf_
// dist`/`_module_conf_state_var` (`70-modules.sh:189-244`).
// ---------------------------------------------------------------------------

/// What [`conf_activate`] did. Only `Activated` wrote anything; the other
/// three are QUIET outcomes, not errors — the auto-activation on install is
/// advisory and must never fail an install (the launcher's `conf-activate`
/// command maps them back onto its own error codes, see
/// `launcher/src-tauri/src/lib.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfActivateOutcome {
    /// The `.conf.dist` was copied to the active dir; carries the conf name.
    Activated(&'static str),
    /// An active conf already exists and `force` was false — left untouched.
    AlreadyActive,
    /// The module has a standard conf name but no `.conf.dist` on disk yet
    /// (it appears after a worldserver rebuild with the module present).
    NoDistYet,
    /// The module has no standard conf file at all.
    NoConf,
}

/// Copies a cpp module's `.conf.dist` into the server's active module-conf
/// dir (`env/dist/etc/modules/<conf_name>`), creating parents. ONE resolver
/// shared by the launcher's `module conf-activate` command and the
/// auto-activation on install (`modmgr::install_cpp`) — the launcher's
/// inline copy used to be the only copy.
///
/// `force == false` + an existing active file is [`ConfActivateOutcome::
/// AlreadyActive`], NOT an error: a user's edited conf is never overwritten
/// unless they ask. Mirrors bash `_module_conf_auto_activate`
/// (`70-modules.sh`) and the `conf-activate)` arm's copy step
/// (`90-main.sh`).
pub fn conf_activate(sdir: &Path, key: &str, force: bool) -> std::io::Result<ConfActivateOutcome> {
    let Some(conf_name) = module_conf_name(key) else {
        return Ok(ConfActivateOutcome::NoConf);
    };
    let active = sdir.join("env").join("dist").join("etc").join("modules").join(conf_name);
    if active.is_file() && !force {
        return Ok(ConfActivateOutcome::AlreadyActive);
    }
    let Some(dist) = module_conf_dist_path(sdir, key) else {
        return Ok(ConfActivateOutcome::NoDistYet);
    };
    if let Some(parent) = active.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(&dist, &active)?;
    Ok(ConfActivateOutcome::Activated(conf_name))
}

/// `_module_conf_name_var` (`70-modules.sh:189-211`): the verbatim table of
/// modules with a standard conf file. `None` for any module without one
/// (matches the bash `*) REPLY="" ;;` fallthrough).
pub fn module_conf_name(key: &str) -> Option<&'static str> {
    match key {
        "mod-1v1-arena" => Some("1v1arena.conf"),
        "mod-aoe-loot" => Some("mod_aoe_loot.conf"),
        "mod-ah-bot" => Some("mod_ahbot.conf"),
        "mod-ah-bot-plus" => Some("mod_ahbot.conf"),
        "mod-autobalance" => Some("AutoBalance.conf"),
        "mod-dungeon-master" => Some("mod_dungeon_master.conf"),
        "mod-talentbutton" => Some("mod_talentbutton.conf"),
        "mod-ale" => Some("mod_ale.conf"),
        "mod-player-bot-level-brackets" => Some("mod_player_bot_level_brackets.conf"),
        "mod-challenge-modes" => Some("challenge_modes.conf"),
        "mod-custom-login" => Some("mod_customlogin.conf"),
        "mod-individual-progression" => Some("individualProgression.conf"),
        "mod-learn-spells" => Some("mod_learnspells.conf"),
        "mod-npc-beastmaster" => Some("mod_npc_beastmaster.conf"),
        "mod-quest-loot-party" => Some("mod-quest-loot-party.conf"),
        "mod-solocraft" => Some("Solocraft.conf"),
        "mod-transmog" => Some("transmog.conf"),
        _ => None,
    }
}

/// Bounded first-match filesystem walk shared by [`module_conf_dist_path`]
/// (maxdepth 4) and [`find_module_sql_file`] (unbounded — pass `u32::MAX`).
/// `cur_depth` is the depth of `dir`'s OWN contents (1 for the starting
/// dir's direct children, matching GNU `find`'s depth accounting where the
/// starting point itself is depth 0). Subdirs are visited in sorted order
/// for determinism (the bash `find`'s own traversal order is filesystem-
/// dependent and NOT relied on for correctness there either — any single
/// match answers the "does this file exist somewhere under here" question).
fn find_first_by_name(dir: &Path, filename: &str, cur_depth: u32, max_depth: u32) -> Option<PathBuf> {
    let rd = std::fs::read_dir(dir).ok()?;
    let mut subdirs = Vec::new();
    for entry in rd.flatten() {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_file() {
            if entry.file_name().to_str() == Some(filename) {
                return Some(path);
            }
        } else if ft.is_dir() {
            subdirs.push(path);
        }
    }
    if cur_depth >= max_depth {
        return None;
    }
    subdirs.sort();
    for sd in subdirs {
        if let Some(found) = find_first_by_name(&sd, filename, cur_depth + 1, max_depth) {
            return Some(found);
        }
    }
    None
}

/// `_module_conf_dist` (`70-modules.sh:235-244`): the expected fixed
/// location first (`modules/<key>/conf/<name>.dist`), else a bounded
/// (maxdepth 4) find under `modules/<key>`. `None` when the module has no
/// standard conf name, or neither location has the `.dist`.
pub fn module_conf_dist_path(sdir: &Path, key: &str) -> Option<PathBuf> {
    let name = module_conf_name(key)?;
    let dist_name = format!("{name}.dist");
    let expected = sdir.join("modules").join(key).join("conf").join(&dist_name);
    if expected.is_file() {
        return Some(expected);
    }
    find_first_by_name(&sdir.join("modules").join(key), &dist_name, 1, 4)
}

/// `_module_conf_state_var` (`70-modules.sh:221-230`): none | needs-rebuild |
/// ready | active.
pub fn module_conf_state(sdir: &Path, key: &str) -> &'static str {
    let Some(name) = module_conf_name(key) else { return "none" };
    let active = sdir.join("env").join("dist").join("etc").join("modules").join(name);
    if active.is_file() {
        return "active";
    }
    if module_conf_dist_path(sdir, key).is_some() {
        "ready"
    } else {
        "needs-rebuild"
    }
}

// ---------------------------------------------------------------------------
// tracking / repair — `_module_discover_sql_files`/`_valid_module_sql_
// filename`/`_module_db_read` (`70-modules.sh:799-822`).
// ---------------------------------------------------------------------------

/// `_valid_module_sql_filename`: `^[A-Za-z0-9._-]+\.sql$` — no slashes, so
/// `../evil.sql` or `x.sql;DROP` can never pass.
pub fn valid_module_sql_filename(s: &str) -> bool {
    s.len() > 4
        && s.ends_with(".sql")
        && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// `_module_discover_sql_files` (`70-modules.sh:799-806`): the `*.sql`
/// basenames directly under `data/sql/db-<db_short>/` (checked first) or
/// `sql/<db_short>/` (fallback) — NOT recursive (subdirs there are usually
/// versioned variants, per the bash comment this ports). Sorted for
/// deterministic output (the bash's `ls` also sorts by name by default).
pub fn module_discover_sql_files(sdir: &Path, key: &str, db_short: &str) -> Vec<String> {
    let modules = sdir.join("modules").join(key);
    let mut dir = modules.join("data").join("sql").join(format!("db-{db_short}"));
    if !dir.is_dir() {
        dir = modules.join("sql").join(db_short);
    }
    let Ok(rd) = std::fs::read_dir(&dir) else { return Vec::new() };
    let mut names: Vec<String> = rd
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            if p.is_file() && p.extension().map(|x| x == "sql").unwrap_or(false) {
                p.file_name().and_then(|n| n.to_str()).map(str::to_string)
            } else {
                None
            }
        })
        .collect();
    names.sort();
    names
}

/// `find "$sdir/modules/$mkey" -name "$f" | head -1` (`90-main.sh:5154`,
/// `repair`'s mark-mode file lookup) — UNBOUNDED depth, first match.
pub fn find_module_sql_file(sdir: &Path, key: &str, filename: &str) -> Option<PathBuf> {
    find_first_by_name(&sdir.join("modules").join(key), filename, 1, u32::MAX)
}

/// `stripped`/`term1` (`90-main.sh:5069-5070`): the module key minus its
/// `mod-` prefix, plus a dash-to-underscore variant — the two substrings
/// `module tracking` LIKE-matches against `updates.name`.
pub fn tracking_like_terms(key: &str) -> (String, String) {
    let stripped = key.strip_prefix("mod-").unwrap_or(key).to_string();
    let term1 = stripped.replace('-', "_");
    (stripped, term1)
}

/// Map a `--db` short name (`world`/`characters`/`auth`) to the [`super::
/// db::Database`] enum, matching `_module_db_read`'s dispatch
/// (`70-modules.sh:811-817`) — `repair`'s own `--db` allowlist (`case "$rdb"
/// in world|characters|auth)`) is re-validated at the call site in `lib.rs`
/// before this is ever consulted, so `None` here is unreachable in practice,
/// not a distinct error path.
pub fn database_for_short(short: &str) -> Option<super::db::Database> {
    match short {
        "world" => Some(super::db::Database::World),
        "characters" => Some(super::db::Database::Characters),
        "auth" => Some(super::db::Database::Auth),
        _ => None,
    }
}

// SQL text — bound-param builders, connection-free (see module doc comment).
pub const TRACKING_LIKE_SQL: &str = "SELECT name FROM updates WHERE name LIKE ? OR name LIKE ?";
pub const TRACKING_EXACT_COUNT_SQL: &str = "SELECT COUNT(*) FROM updates WHERE name = ?";
pub const REPAIR_MARK_SQL: &str =
    "INSERT INTO updates (name, hash, state, timestamp, speed) VALUES (?, ?, 'RELEASED', NOW(), 0) \
     ON DUPLICATE KEY UPDATE hash=?, state='RELEASED'";
pub const REPAIR_CLEAR_COUNT_SQL: &str = "SELECT COUNT(*) FROM updates WHERE name=?";
pub const REPAIR_CLEAR_DELETE_SQL: &str = "DELETE FROM updates WHERE name=?";

// ---------------------------------------------------------------------------
// update-check — reuses `maint::read_repo_check` (already the server-level
// `wow update-check` oracle port) per installed cpp registry module, plus a
// dedup'd scan of any custom (non-registry) git clone under `modules/`
// (`90-main.sh:5434-5470`).
// ---------------------------------------------------------------------------

/// Live `module update-check` read: one `repos[]` entry per installed cpp
/// module (registry first, in registry order, then any custom clone not
/// already in the registry — sorted by key for determinism). NEVER mutates
/// a worktree (delegates entirely to [`super::maint::read_repo_check`],
/// which is itself fetch-only).
pub fn module_update_check(program: &std::ffi::OsStr, sdir: &Path) -> Value {
    use super::{maint, modmgr};
    let mut repos = Vec::new();
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for row in modmgr::CPP_REGISTRY {
        seen.insert(row.key);
        if modmgr::cpp_installed(sdir, row.key) {
            repos.push(maint::read_repo_check(program, &sdir.join("modules").join(row.key), row.key));
        }
    }
    let modules_dir = sdir.join("modules");
    if let Ok(rd) = std::fs::read_dir(&modules_dir) {
        let mut extra: Vec<(String, PathBuf)> = rd
            .flatten()
            .filter_map(|e| {
                let path = e.path();
                if !path.is_dir() || !path.join(".git").is_dir() {
                    return None;
                }
                let key = path.file_name()?.to_str()?.to_string();
                if seen.contains(key.as_str()) || !modmgr::valid_cpp_key(&key) {
                    return None;
                }
                Some((key, path))
            })
            .collect();
        extra.sort_by(|a, b| a.0.cmp(&b.0));
        for (key, path) in extra {
            repos.push(maint::read_repo_check(program, &path, &key));
        }
    }
    serde_json::json!({ "repos": repos })
}

// ---------------------------------------------------------------------------
// fixit (battlepass-npc) / place-npc — `90-main.sh:5181-5345`. Every SQL
// string below is a FIXED literal (zero user input reaches a statement) —
// ported verbatim from the oracle. The multi-statement ones (SET/PREPARE/
// EXECUTE blocks) MUST run via `modmgr::mysql_run_stmt` (docker exec -e —
// see that module's doc comment on why the `mysql` crate can't run these);
// the single-statement COUNT reads / spawn INSERT are plain bound-param
// `db::query_with_params`/`db::execute` calls, shared by fixit AND
// place-npc (identical shape, different entry/coords).
//
// TASK 6 SCOPE RULING: the `acore_world` literals inside the BATTLEPASS_*
// consts (both the `TABLE_SCHEMA='acore_world'` probes and the dotted names
// inside the single-quoted dynamic-SQL strings) are a recorded EXCEPTION to
// resolved-name splicing — these statements repair the Battle Pass MODULE,
// whose own shipped SQL hardcodes the standard schema names internally, so
// the module subsystem requires standard names end-to-end (see
// `modmgr::mysql_run_stmt`'s ruling). Do not resolve them.
// ---------------------------------------------------------------------------

pub const CREATURE_TEMPLATE_COUNT_SQL: &str = "SELECT COUNT(*) FROM creature_template WHERE entry=?";
pub const CREATURE_SPAWN_COUNT_SQL: &str = "SELECT COUNT(*) FROM creature WHERE id=? AND map=?";
pub const CREATURE_SPAWN_INSERT_SQL: &str =
    "INSERT INTO creature (id, map, position_x, position_y, position_z, orientation, spawntimesecs) \
     VALUES (?, ?, ?, ?, ?, ?, 300)";

/// The Battle Pass vendor's fixed creature entry (`90100`, `90-main.sh:5206`
/// onward).
pub const BATTLEPASS_NPC_ENTRY: u32 = 90100;

/// `90-main.sh:5225` — template INSERT wrapped in a disable-FK-checks /
/// permissive-sql_mode pair (multi-statement; shell out via `mysql_run_stmt`).
pub const BATTLEPASS_TEMPLATE_INSERT_SQL: &str =
    "SET foreign_key_checks=0; SET sql_mode=''; INSERT INTO creature_template (`entry`,`name`,`subname`,\
     `gossip_menu_id`,`minlevel`,`maxlevel`,`exp`,`faction`,`npcflag`,`speed_walk`,`speed_run`,`rank`,\
     `dmgschool`,`DamageModifier`,`BaseAttackTime`,`RangeAttackTime`,`BaseVariance`,`RangeVariance`,\
     `unit_class`,`unit_flags`,`unit_flags2`,`dynamicflags`,`type`,`AIName`,`MovementType`,`HoverHeight`,\
     `HealthModifier`,`ManaModifier`,`ArmorModifier`,`RegenHealth`,`flags_extra`,`VerifiedBuild`) VALUES \
     (90100,'Battle Pass Vendor','Seasonal Rewards',0,80,80,0,35,1,1.0,1.14286,0,0,1.0,2000,2000,1.0,1.0,1,\
     33536,2048,0,7,'',0,1.0,1.0,1.0,1.0,1,2,0); SET foreign_key_checks=1;";

/// `90-main.sh:5229` — schema-adaptive `scale` UPDATE, best-effort.
pub const BATTLEPASS_SCALE_UPDATE_SQL: &str =
    "SET @h=(SELECT COUNT(*) FROM information_schema.COLUMNS WHERE TABLE_SCHEMA='acore_world' AND \
     TABLE_NAME='creature_template' AND COLUMN_NAME='scale'); SET @s=IF(@h>0,'UPDATE acore_world.\
     creature_template SET scale=1.0 WHERE entry=90100','SELECT 1'); PREPARE _p FROM @s; EXECUTE _p; \
     DEALLOCATE PREPARE _p;";

/// `90-main.sh:5230` — schema-adaptive model-row DELETE, best-effort.
pub const BATTLEPASS_MODEL_DELETE_SQL: &str =
    "SET @h=(SELECT COUNT(*) FROM information_schema.TABLES WHERE TABLE_SCHEMA='acore_world' AND \
     TABLE_NAME='creature_template_model'); SET @s=IF(@h>0,'DELETE FROM acore_world.creature_template_model \
     WHERE CreatureID=90100','SELECT 1'); PREPARE _p FROM @s; EXECUTE _p; DEALLOCATE PREPARE _p;";

/// `90-main.sh:5231` — schema-adaptive model-row INSERT, best-effort.
pub const BATTLEPASS_MODEL_INSERT_SQL: &str =
    "SET @h=(SELECT COUNT(*) FROM information_schema.TABLES WHERE TABLE_SCHEMA='acore_world' AND \
     TABLE_NAME='creature_template_model'); SET @s=IF(@h>0,'INSERT INTO acore_world.creature_template_model \
     (CreatureID,Idx,CreatureDisplayID,DisplayScale,Probability,VerifiedBuild) VALUES (90100,0,25478,1.0,1.0,0)\
     ','SELECT 1'); PREPARE _p FROM @s; EXECUTE _p; DEALLOCATE PREPARE _p;";

/// `90-main.sh:5232` — schema-adaptive `modelid1` UPDATE, best-effort.
pub const BATTLEPASS_MODELID1_UPDATE_SQL: &str =
    "SET @h=(SELECT COUNT(*) FROM information_schema.COLUMNS WHERE TABLE_SCHEMA='acore_world' AND \
     TABLE_NAME='creature_template' AND COLUMN_NAME='modelid1'); SET @s=IF(@h>0,'UPDATE acore_world.\
     creature_template SET modelid1=25478 WHERE entry=90100','SELECT 1'); PREPARE _p FROM @s; EXECUTE _p; \
     DEALLOCATE PREPARE _p;";

/// Stormwind (map 0) / Orgrimmar (map 1) spawn coords for the Battle Pass
/// NPC (`90-main.sh:5236-5238`). `(map, x, y, z, o)`.
pub const BATTLEPASS_CAPITALS: [(u32, f64, f64, f64, f64); 2] =
    [(0, -8819.3, 636.2, 94.1, 3.7), (1, 1609.2, -4407.7, 17.5, 4.5)];

/// One capital-coordinate spawn spec (map, entry, x, y, z, orientation).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NpcSpec {
    pub map: u32,
    pub entry: u32,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub o: f64,
}

/// `place-npc`'s closed key allowlist (`90-main.sh:5274-5279`) — battlepass
/// is deliberately excluded (it goes through `fixit`, which also creates the
/// creature_template these mods' own SQL already ships).
pub const PLACE_NPC_KEYS: &[&str] = &["mod-1v1-arena", "mod-transmog", "mod-npc-beastmaster", "bmah"];

pub fn valid_place_npc_key(key: &str) -> bool {
    PLACE_NPC_KEYS.contains(&key)
}

fn is_uint_token(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// `^-?[0-9]+(\.[0-9]+)?$` — the coordinate-token shape `_npc_coord_specs`'s
/// awk regex checks (`47-commands.sh:427-428`).
fn is_signed_decimal_token(s: &str) -> bool {
    let rest = s.strip_prefix('-').unwrap_or(s);
    match rest.split_once('.') {
        Some((int_part, frac_part)) => {
            is_uint_token(int_part) && !frac_part.is_empty() && frac_part.bytes().all(|b| b.is_ascii_digit())
        }
        None => is_uint_token(rest),
    }
}

/// `_npc_coord_specs` (`47-commands.sh:421-433`): parses every fully
/// specified `npc add <entry> <map> <x> <y> <z> <o>` line out of the key's
/// `_cmd_block_for` text (reused verbatim via [`super::commands::
/// cmd_block_for`] — single source of truth, no second copy of the
/// coordinates). Lines with fewer than 8 whitespace-separated tokens, a
/// non-"npc add" head, or any non-numeric-shaped field are skipped. Empty
/// result means "this mod ships no ready-made spawn block" (e.g.
/// mod-dungeon-master's bare `npc add 500000`).
pub fn npc_coord_specs(key: &str) -> Vec<NpcSpec> {
    let Some(block) = super::commands::cmd_block_for(key) else { return Vec::new() };
    let mut out = Vec::new();
    for line in block.split('\n') {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.len() < 8 || tokens[0] != "npc" || tokens[1] != "add" {
            continue;
        }
        let (entry_s, map_s, x_s, y_s, z_s, o_s) = (tokens[2], tokens[3], tokens[4], tokens[5], tokens[6], tokens[7]);
        if !is_uint_token(entry_s)
            || !is_uint_token(map_s)
            || !is_signed_decimal_token(x_s)
            || !is_signed_decimal_token(y_s)
            || !is_signed_decimal_token(z_s)
            || !is_signed_decimal_token(o_s)
        {
            continue;
        }
        let (Ok(entry), Ok(map), Ok(x), Ok(y), Ok(z), Ok(o)) =
            (entry_s.parse(), map_s.parse(), x_s.parse(), y_s.parse(), z_s.parse(), o_s.parse())
        else {
            continue; // shape guards above make this unreachable; defensive skip, not a panic.
        };
        out.push(NpcSpec { map, entry, x, y, z, o });
    }
    out
}

// ---------------------------------------------------------------------------
// client-patch (mod-arac) — `90-main.sh:5346-5432`.
// ---------------------------------------------------------------------------

/// The documented fallback client-data volume name when `docker inspect`
/// can't resolve one (`90-main.sh:5390`).
pub const DEFAULT_CLIENT_DATA_VOLUME: &str = "wow-server-playerbots_ac-client-data";

/// Resolve the worldserver's client-data VOLUME NAME via `docker inspect`
/// (`90-main.sh:5388-5392`): the mount whose Destination is
/// `/azerothcore/env/dist/data`. Returns `(volume, used_fallback)` so the
/// caller can emit the same "using the default name" warning line the
/// oracle does when it falls back.
pub fn resolve_client_data_volume(program: &std::ffi::OsStr) -> (String, bool) {
    let mut cmd = std::process::Command::new(program);
    cmd.args([
        "inspect",
        "ac-worldserver",
        "--format",
        "{{range .Mounts}}{{if eq .Destination \"/azerothcore/env/dist/data\"}}{{.Name}}{{end}}{{end}}",
    ]);
    super::status::windows_no_window(&mut cmd);
    let vol = match super::status::output_bounded_draining(cmd, std::time::Duration::from_secs(10)) {
        Some(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        _ => String::new(),
    };
    if vol.is_empty() {
        (DEFAULT_CLIENT_DATA_VOLUME.to_string(), true)
    } else {
        (vol, false)
    }
}

/// One `docker run --rm -v <vol>:/data -v <file>:/src/<bn>:ro alpine cp
/// /src/<bn> /data/dbc/<bn>` DBC copy (`90-main.sh:5398`).
pub fn docker_copy_dbc_into_volume(program: &std::ffi::OsStr, vol: &str, src_file: &Path, bn: &str) -> bool {
    let src_mount = format!("{}:/src/{bn}:ro", src_file.display());
    let vol_mount = format!("{vol}:/data");
    let dest = format!("/data/dbc/{bn}");
    let mut cmd = std::process::Command::new(program);
    cmd.args(["run", "--rm", "-v", &vol_mount, "-v", &src_mount, "alpine", "cp", &format!("/src/{bn}"), &dest]);
    super::status::windows_no_window(&mut cmd);
    matches!(
        super::status::output_bounded_draining(cmd, std::time::Duration::from_secs(60)),
        Some(out) if out.status.success()
    )
}

/// Sorted `.dbc` basenames directly under `mdir/patch-contents/DBFilesContent`
/// (`90-main.sh:5377,5394`) — the client-patch DBC source dir.
pub fn dbc_source_files(mdir: &Path) -> Vec<PathBuf> {
    let dbcsrc = mdir.join("patch-contents").join("DBFilesContent");
    let Ok(rd) = std::fs::read_dir(&dbcsrc) else { return Vec::new() };
    let mut files: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().map(|x| x == "dbc").unwrap_or(false))
        .collect();
    files.sort();
    files
}

/// `module client-patch`'s section name (`90-main.sh:5347`).
const CLIENT_PATCH_SECTION: &str = "client-patch";

/// `module client-patch`'s full orchestration (`90-main.sh:5346-5432`, `--key
/// mod-arac` only): copy every server DBC into the running worldserver's
/// data volume via a throwaway `docker run` container, then best-effort
/// copy `Patch-A.MPQ` into the saved client folder's `Data/`. Streamed
/// NDJSON — same vocabulary as [`super::modmgr::module_install_stream`].
/// Moved out of the launcher's `lib.rs` by the cargo-workspace refactor
/// (Task 9); the Tauri command is now a thin `spawn_blocking` adapter.
pub fn module_client_patch_stream(key: String, emit: impl Fn(serde_json::Value)) {
    use crate::modmgr::{done_event, error_event, line_event, section_end, section_start};
    use crate::moduletail as mt;

    emit(section_start(CLIENT_PATCH_SECTION));

    if key != "mod-arac" {
        emit(section_end(CLIENT_PATCH_SECTION, "error"));
        emit(error_event(
            "BAD_ARG",
            "client-patch supports only --key mod-arac",
            "Other modules ship no client patch step.",
        ));
        return;
    }

    let sdir = match super::maint::require_server_dir("Install it first.") {
        Ok(d) => d,
        Err(e) => {
            emit(section_end(CLIENT_PATCH_SECTION, "error"));
            emit(error_event(&e.code, e.message, &e.hint));
            return;
        }
    };
    if !crate::modmgr::cpp_installed(&sdir, "mod-arac") {
        emit(section_end(CLIENT_PATCH_SECTION, "error"));
        emit(error_event("NOT_INSTALLED", "mod-arac is not installed", "Install it on the Modules page first."));
        return;
    }
    let mdir = sdir.join("modules").join("mod-arac");
    let dbcsrc = mdir.join("patch-contents").join("DBFilesContent");
    if !dbcsrc.is_dir() {
        emit(section_end(CLIENT_PATCH_SECTION, "error"));
        let hint = format!("Expected {} -- try Update on the Modules page to refresh the clone.", dbcsrc.display());
        emit(error_event("NOT_FOUND", "DBC files not found in the mod-arac clone", &hint));
        return;
    }

    let docker_program = crate::native::docker_program();
    let (vol, used_fallback) = mt::resolve_client_data_volume(&docker_program);
    if used_fallback {
        emit(line_event(
            "warn",
            format!("could not resolve the data volume from the worldserver container -- using the default name {vol}"),
        ));
    }

    let dbc_files = mt::dbc_source_files(&mdir);
    let mut dbc_n = 0u32;
    for f in &dbc_files {
        let bn = f.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
        emit(line_event("info", format!("copying {bn} into the server data volume...")));
        if !mt::docker_copy_dbc_into_volume(&docker_program, &vol, f, &bn) {
            emit(section_end(CLIENT_PATCH_SECTION, "error"));
            emit(error_event("COPY_FAILED", format!("Could not copy {bn} into the data volume"), "Is Docker running?"));
            return;
        }
        dbc_n += 1;
    }
    if dbc_n == 0 {
        emit(section_end(CLIENT_PATCH_SECTION, "error"));
        emit(error_event(
            "NOT_FOUND",
            format!("No .dbc files found in {}", dbcsrc.display()),
            "Try Update on the Modules page to refresh the clone.",
        ));
        return;
    }
    emit(line_event("info", format!("{dbc_n} server DBC files installed")));

    let client_path = crate::modmgr::effective_client_path();
    let mut client_done = false;
    match client_path {
        None => emit(line_event(
            "warn",
            "no client folder set — skipped Patch-A.MPQ (set it on the Modules page, then re-run this)",
        )),
        Some(cpath) => {
            let mpq = mdir.join("Patch-A.MPQ");
            if !mpq.is_file() {
                emit(line_event(
                    "warn",
                    format!("Patch-A.MPQ not found in the mod-arac clone — try Update on the Modules page, or copy it manually into {}/Data/", mdir.display()),
                ));
            } else {
                emit(line_event("info", "installing Patch-A.MPQ into the client Data folder..."));
                let dest = cpath.join("Data").join("Patch-A.MPQ");
                if std::fs::copy(&mpq, &dest).is_ok() {
                    client_done = true;
                    emit(line_event("info", "Patch-A.MPQ installed"));
                } else {
                    emit(line_event(
                        "warn",
                        format!("could not copy Patch-A.MPQ — copy {} into <client>/Data/ manually", mpq.display()),
                    ));
                }
            }
        }
    }

    emit(line_event(
        "info",
        "restart the server (Home) to load the new race/class combinations — no rebuild needed",
    ));
    emit(section_end(CLIENT_PATCH_SECTION, "ok"));
    emit(done_event(serde_json::json!({
        "key": "mod-arac", "dbc_files": dbc_n, "client_patched": client_done, "restart_required": true,
    })));
}

// ---------------------------------------------------------------------------
// NATIVE-MODE `module repair` (`90-main.sh:5107-5180`) — moved out of the
// launcher's `lib.rs` by the cargo-workspace refactor (Task 9b). The FOURTH
// sanctioned direct MySQL write (see `db`/`backup`'s headers): INSERT/DELETE
// on the `updates` tracking tables ONLY, via bound-param `db::execute`.
// Every filename is validated BEFORE any SQL runs, matching the oracle's
// abort-before-mutation contract. The `mark` mode hashes each `.sql` file
// the SAME way the bash oracle's `sha1sum` does, so the tracking row's hash
// matches what AzerothCore's own db-import would have written.
//
// `key`/`db`/`mode` arrive PRE-VALIDATED by the caller (module-key shape,
// the three database names, mark|clear) -- the same split every other
// hoisted body uses.
// ---------------------------------------------------------------------------

pub fn module_repair(
    key: String,
    db: String,
    mode: String,
    files: Option<String>,
) -> Result<serde_json::Value, CmdError> {
    let sdir = crate::maint::require_server_dir("")?;
    if !crate::modmgr::cpp_installed(&sdir, &key) {
        return Err(not_found_err(format!("Module not installed: {key}"), "Install it first."));
    }
    let file_list: Vec<String> = match &files {
        Some(f) => f.split_whitespace().map(str::to_string).collect(),
        None => crate::moduletail::module_discover_sql_files(&sdir, &key, &db),
    };
    for f in &file_list {
        if !crate::moduletail::valid_module_sql_filename(f) {
            return Err(CmdError {
                code: "BAD_ARG".into(),
                message: format!("Invalid filename: {f}"),
                hint: "Filenames must match ^[A-Za-z0-9._-]+\\.sql$ (no slashes).".into(),
            });
        }
    }
    let database = crate::moduletail::database_for_short(&db).expect("validated above");
    let cfg = crate::db::DbConfig::from_env();
    let mut results = Vec::new();
    for f in file_list {
        let res = if mode == "mark" {
            match crate::moduletail::find_module_sql_file(&sdir, &key, &f) {
                None => "file_missing",
                Some(path) => {
                    let bytes = std::fs::read(&path).map_err(io_internal_err)?;
                    let hash = {
                        use sha1::Digest;
                        let mut hasher = sha1::Sha1::new();
                        hasher.update(&bytes);
                        let digest = hasher.finalize();
                        digest.iter().map(|b| format!("{b:02X}")).collect::<String>()
                    };
                    let params: Vec<mysql::Value> =
                        vec![mysql::Value::from(&f), mysql::Value::from(&hash), mysql::Value::from(&hash)];
                    crate::db::execute(&cfg, database, crate::moduletail::REPAIR_MARK_SQL, params)
                        .map_err(|e| db_unreachable_err(format!("Could not write to acore_{db}.updates: {e}")))?;
                    "marked"
                }
            }
        } else {
            let cnt_params: Vec<mysql::Value> = vec![mysql::Value::from(&f)];
            let cnt = crate::db::query_with_params(&cfg, database, crate::moduletail::REPAIR_CLEAR_COUNT_SQL, cnt_params)
                .map_err(|e| db_unreachable_err(format!("Could not reach the {db} database: {e}")))
                .map(count_result)?;
            if cnt == 0 {
                "not_tracked"
            } else {
                let del_params: Vec<mysql::Value> = vec![mysql::Value::from(&f)];
                crate::db::execute(&cfg, database, crate::moduletail::REPAIR_CLEAR_DELETE_SQL, del_params)
                    .map_err(|e| db_unreachable_err(format!("Could not write to acore_{db}.updates: {e}")))?;
                "cleared"
            }
        };
        results.push(serde_json::json!({"file": f, "result": res}));
    }
    Ok(serde_json::json!({"key": key, "db": db, "mode": mode, "results": results}))
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- module_conf_name -------------------------------------------------

    #[test]
    fn module_conf_name_known_and_unknown() {
        assert_eq!(module_conf_name("mod-ah-bot"), Some("mod_ahbot.conf"));
        assert_eq!(module_conf_name("mod-ah-bot-plus"), Some("mod_ahbot.conf"));
        assert_eq!(module_conf_name("mod-transmog"), Some("transmog.conf"));
        assert_eq!(module_conf_name("mod-junk-to-gold"), None);
        assert_eq!(module_conf_name("nonexistent"), None);
    }

    // -- module_conf_dist_path / module_conf_state ---------------------------

    #[test]
    fn module_conf_state_none_when_no_conf_name() {
        let dir = std::env::temp_dir().join(format!("dml-mtail-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        assert_eq!(module_conf_state(&dir, "mod-junk-to-gold"), "none");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn module_conf_state_active_when_deployed() {
        let dir = std::env::temp_dir().join(format!("dml-mtail-active-{}", std::process::id()));
        let etc = dir.join("env").join("dist").join("etc").join("modules");
        std::fs::create_dir_all(&etc).unwrap();
        std::fs::write(etc.join("transmog.conf"), "x").unwrap();
        assert_eq!(module_conf_state(&dir, "mod-transmog"), "active");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn module_conf_state_ready_when_dist_present_but_not_active() {
        let dir = std::env::temp_dir().join(format!("dml-mtail-ready-{}", std::process::id()));
        let conf_dir = dir.join("modules").join("mod-transmog").join("conf");
        std::fs::create_dir_all(&conf_dir).unwrap();
        std::fs::write(conf_dir.join("transmog.conf.dist"), "x").unwrap();
        assert_eq!(module_conf_state(&dir, "mod-transmog"), "ready");
        assert!(module_conf_dist_path(&dir, "mod-transmog").is_some());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn module_conf_state_needs_rebuild_when_neither_present() {
        let dir = std::env::temp_dir().join(format!("dml-mtail-needsrebuild-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("modules").join("mod-transmog")).unwrap();
        assert_eq!(module_conf_state(&dir, "mod-transmog"), "needs-rebuild");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn module_conf_dist_path_falls_back_to_bounded_find() {
        let dir = std::env::temp_dir().join(format!("dml-mtail-find-{}", std::process::id()));
        let nested = dir.join("modules").join("mod-transmog").join("a").join("b");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("transmog.conf.dist"), "x").unwrap();
        let found = module_conf_dist_path(&dir, "mod-transmog");
        assert_eq!(found, Some(nested.join("transmog.conf.dist")));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // -- conf_activate -------------------------------------------------------
    //
    // Fixture shape copied from `module_conf_state_ready_when_dist_present_
    // but_not_active` above (`std::env::temp_dir()` + a per-test unique name
    // -- `tempfile` is NOT a dependency of this crate). Key is `mod-ah-bot`,
    // the real registry key whose conf is `mod_ahbot.conf`.

    #[test]
    fn a_conf_activate_copies_dist_when_ready() {
        let sdir = std::env::temp_dir().join(format!("dml-mtail-act-copy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&sdir);
        let dist = sdir.join("modules").join("mod-ah-bot").join("conf");
        std::fs::create_dir_all(&dist).unwrap();
        std::fs::write(dist.join("mod_ahbot.conf.dist"), "Key = 1\n").unwrap();
        let out = conf_activate(&sdir, "mod-ah-bot", false).unwrap();
        assert!(matches!(out, ConfActivateOutcome::Activated("mod_ahbot.conf")), "{out:?}");
        let active = sdir.join("env").join("dist").join("etc").join("modules").join("mod_ahbot.conf");
        assert_eq!(std::fs::read_to_string(active).unwrap(), "Key = 1\n");
        let _ = std::fs::remove_dir_all(&sdir);
    }

    #[test]
    fn a_conf_activate_never_overwrites_an_existing_conf() {
        let sdir = std::env::temp_dir().join(format!("dml-mtail-act-keep-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&sdir);
        std::fs::create_dir_all(sdir.join("modules").join("mod-ah-bot").join("conf")).unwrap();
        std::fs::write(sdir.join("modules").join("mod-ah-bot").join("conf").join("mod_ahbot.conf.dist"), "DEFAULT\n").unwrap();
        std::fs::create_dir_all(sdir.join("env").join("dist").join("etc").join("modules")).unwrap();
        std::fs::write(sdir.join("env").join("dist").join("etc").join("modules").join("mod_ahbot.conf"), "USER EDIT\n").unwrap();
        let out = conf_activate(&sdir, "mod-ah-bot", false).unwrap();
        assert!(matches!(out, ConfActivateOutcome::AlreadyActive), "{out:?}");
        assert_eq!(
            std::fs::read_to_string(sdir.join("env").join("dist").join("etc").join("modules").join("mod_ahbot.conf")).unwrap(),
            "USER EDIT\n"
        );
        let _ = std::fs::remove_dir_all(&sdir);
    }

    #[test]
    fn a_conf_activate_force_overwrites_an_existing_conf() {
        // The launcher arm's `--force` contract: the ONLY way the active
        // file is replaced. Isolates the `force` half of the guard from the
        // `active.is_file()` half (a single sequence test would let either
        // half satisfy it -- the recorded two-halves lesson).
        let sdir = std::env::temp_dir().join(format!("dml-mtail-act-force-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&sdir);
        std::fs::create_dir_all(sdir.join("modules").join("mod-ah-bot").join("conf")).unwrap();
        std::fs::write(sdir.join("modules").join("mod-ah-bot").join("conf").join("mod_ahbot.conf.dist"), "DEFAULT\n").unwrap();
        std::fs::create_dir_all(sdir.join("env").join("dist").join("etc").join("modules")).unwrap();
        std::fs::write(sdir.join("env").join("dist").join("etc").join("modules").join("mod_ahbot.conf"), "USER EDIT\n").unwrap();
        let out = conf_activate(&sdir, "mod-ah-bot", true).unwrap();
        assert!(matches!(out, ConfActivateOutcome::Activated("mod_ahbot.conf")), "{out:?}");
        assert_eq!(
            std::fs::read_to_string(sdir.join("env").join("dist").join("etc").join("modules").join("mod_ahbot.conf")).unwrap(),
            "DEFAULT\n"
        );
        let _ = std::fs::remove_dir_all(&sdir);
    }

    #[test]
    fn a_conf_activate_no_dist_is_a_quiet_outcome_not_an_error() {
        let sdir = std::env::temp_dir().join(format!("dml-mtail-act-nodist-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&sdir);
        std::fs::create_dir_all(sdir.join("modules").join("mod-ah-bot")).unwrap();
        let out = conf_activate(&sdir, "mod-ah-bot", false).unwrap();
        assert!(matches!(out, ConfActivateOutcome::NoDistYet), "{out:?}");
        let _ = std::fs::remove_dir_all(&sdir);
    }

    #[test]
    fn a_conf_activate_no_conf_name_is_no_conf() {
        let sdir = std::env::temp_dir().join(format!("dml-mtail-act-noconf-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&sdir);
        std::fs::create_dir_all(sdir.join("modules").join("mod-junk-to-gold")).unwrap();
        let out = conf_activate(&sdir, "mod-junk-to-gold", false).unwrap();
        assert!(matches!(out, ConfActivateOutcome::NoConf), "{out:?}");
        let _ = std::fs::remove_dir_all(&sdir);
    }

    // -- valid_module_sql_filename -------------------------------------------------

    #[test]
    fn valid_module_sql_filename_shape() {
        assert!(valid_module_sql_filename("mod_auctionhousebot.sql"));
        assert!(valid_module_sql_filename("v2.1-fix.sql"));
        assert!(!valid_module_sql_filename("../evil.sql"));
        assert!(!valid_module_sql_filename("x.sql;DROP"));
        assert!(!valid_module_sql_filename(".sql"));
        assert!(!valid_module_sql_filename("noext"));
    }

    // -- module_discover_sql_files -------------------------------------------------

    #[test]
    fn module_discover_sql_files_prefers_data_sql_db_dir() {
        let dir = std::env::temp_dir().join(format!("dml-mtail-discover-{}", std::process::id()));
        let sqldir = dir.join("modules").join("mod-x").join("data").join("sql").join("db-world");
        std::fs::create_dir_all(&sqldir).unwrap();
        std::fs::write(sqldir.join("b.sql"), "").unwrap();
        std::fs::write(sqldir.join("a.sql"), "").unwrap();
        std::fs::write(sqldir.join("readme.txt"), "").unwrap();
        let got = module_discover_sql_files(&dir, "mod-x", "world");
        assert_eq!(got, vec!["a.sql".to_string(), "b.sql".to_string()]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn module_discover_sql_files_falls_back_to_sql_dir() {
        let dir = std::env::temp_dir().join(format!("dml-mtail-discoverfb-{}", std::process::id()));
        let sqldir = dir.join("modules").join("mod-x").join("sql").join("characters");
        std::fs::create_dir_all(&sqldir).unwrap();
        std::fs::write(sqldir.join("c.sql"), "").unwrap();
        let got = module_discover_sql_files(&dir, "mod-x", "characters");
        assert_eq!(got, vec!["c.sql".to_string()]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn module_discover_sql_files_missing_dir_is_empty() {
        let dir = std::env::temp_dir().join(format!("dml-mtail-discovermissing-{}", std::process::id()));
        assert_eq!(module_discover_sql_files(&dir, "mod-x", "auth"), Vec::<String>::new());
    }

    // -- find_module_sql_file -------------------------------------------------

    #[test]
    fn find_module_sql_file_unbounded_depth() {
        let dir = std::env::temp_dir().join(format!("dml-mtail-findsql-{}", std::process::id()));
        let nested = dir.join("modules").join("mod-x").join("a").join("b").join("c").join("d").join("e");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("target.sql"), "").unwrap();
        assert_eq!(find_module_sql_file(&dir, "mod-x", "target.sql"), Some(nested.join("target.sql")));
        assert_eq!(find_module_sql_file(&dir, "mod-x", "missing.sql"), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // -- tracking_like_terms -------------------------------------------------

    #[test]
    fn tracking_like_terms_strips_prefix_and_underscores_dashes() {
        assert_eq!(tracking_like_terms("mod-ah-bot"), ("ah-bot".to_string(), "ah_bot".to_string()));
        assert_eq!(tracking_like_terms("noprefix"), ("noprefix".to_string(), "noprefix".to_string()));
    }

    // -- database_for_short -------------------------------------------------

    #[test]
    fn database_for_short_maps_three_names() {
        assert_eq!(database_for_short("world"), Some(super::super::db::Database::World));
        assert_eq!(database_for_short("characters"), Some(super::super::db::Database::Characters));
        assert_eq!(database_for_short("auth"), Some(super::super::db::Database::Auth));
        assert_eq!(database_for_short("bogus"), None);
    }

    // -- valid_place_npc_key -------------------------------------------------

    #[test]
    fn valid_place_npc_key_closed_allowlist() {
        for k in PLACE_NPC_KEYS {
            assert!(valid_place_npc_key(k));
        }
        assert!(!valid_place_npc_key("battlepass"));
        assert!(!valid_place_npc_key("mod-solocraft"));
    }

    // -- npc_coord_specs -------------------------------------------------

    #[test]
    fn npc_coord_specs_parses_both_capitals_for_transmog() {
        let specs = npc_coord_specs("mod-transmog");
        assert_eq!(specs.len(), 2);
        assert_eq!(specs[0], NpcSpec { map: 0, entry: 190010, x: -8831.3, y: 628.2, z: 94.1, o: 3.7 });
        assert_eq!(specs[1], NpcSpec { map: 1, entry: 190010, x: 1597.2, y: -4415.7, z: 17.5, o: 4.5 });
    }

    #[test]
    fn npc_coord_specs_empty_for_dungeon_master_bare_spawn() {
        // mod-dungeon-master's cheat sheet ships `npc add 500000` with no
        // coords -- fewer than 8 fields, so it must yield zero specs.
        assert_eq!(npc_coord_specs("mod-dungeon-master"), Vec::new());
    }

    #[test]
    fn npc_coord_specs_empty_for_unknown_key() {
        assert_eq!(npc_coord_specs("mod-does-not-exist"), Vec::new());
    }

    // -- resolve_client_data_volume -------------------------------------------------

    #[test]
    fn resolve_client_data_volume_falls_back_when_docker_missing() {
        // A program name that can't possibly exist -> spawn fails -> fallback.
        let (vol, used_fallback) = resolve_client_data_volume(std::ffi::OsStr::new("dml-definitely-not-a-real-binary-xyz"));
        assert_eq!(vol, DEFAULT_CLIENT_DATA_VOLUME);
        assert!(used_fallback);
    }

    // -- dbc_source_files -------------------------------------------------

    #[test]
    fn dbc_source_files_sorted_dbc_only() {
        let dir = std::env::temp_dir().join(format!("dml-mtail-dbc-{}", std::process::id()));
        let dbcsrc = dir.join("patch-contents").join("DBFilesContent");
        std::fs::create_dir_all(&dbcsrc).unwrap();
        std::fs::write(dbcsrc.join("z.dbc"), "").unwrap();
        std::fs::write(dbcsrc.join("a.dbc"), "").unwrap();
        std::fs::write(dbcsrc.join("readme.txt"), "").unwrap();
        let got = dbc_source_files(&dir);
        assert_eq!(got, vec![dbcsrc.join("a.dbc"), dbcsrc.join("z.dbc")]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn dbc_source_files_missing_dir_is_empty() {
        let dir = std::env::temp_dir().join(format!("dml-mtail-dbcmissing-{}", std::process::id()));
        assert_eq!(dbc_source_files(&dir), Vec::<PathBuf>::new());
    }
}
