//! Native-mode **docker/fs-destructive** primitives (spike:
//! `spike/docker-desktop-native`, Task C4a): `docker-clean`, `module
//! rebuild`, `games remove`. Faithful ports of `cli/src/90-main.sh`'s
//! `docker-clean)` arm (1519-1591), the `module)`'s `rebuild)` case
//! (4967-5008), and the `remove)` arm (1184-1309), plus the shared helpers
//! `_resolve_compose_dir`/`_compose_server_images` (90-main.sh:61-103) and
//! the title catalog `_title_registry`/`_title_row`/`_title_installed`
//! (`cli/src/80-titles.sh:12-40`).
//!
//! ARCHITECTURE, mirroring `modmgr`/`lifecycle`: every REUSABLE,
//! pure-or-nearly-pure primitive (the title registry, the two DIFFERENT
//! project-name sanitizers, the build-volume regex, the client-data-volume
//! detector, the server-image list parser, the games-dir FS-removal
//! decision) lives here so the guards are independently unit-tested without
//! a live docker engine. The STREAMED orchestration itself
//! ([`docker_clean_stream`], [`bots_flush_stream`], [`games_remove_stream`] —
//! `section_start`/`line`/`section_end`/`done`/`error` sequencing plus the
//! real bounded/unbounded subprocess spawns) lives at the BOTTOM of this
//! file, moved out of the launcher's `lib.rs` by the cargo-workspace
//! refactor (Task 9) so the standalone CLI can drive it too. `module
//! rebuild`'s own stream lives in [`super::modmgr::module_rebuild_stream`],
//! next to the rest of the module verbs.
//!
//! THE ONE NEW SUBPROCESS SHAPE: [`run_streamed_unbounded`]. Every other
//! docker/git call in this codebase goes through `status::
//! output_bounded_draining` (bounded, and only readable AFTER the child
//! exits). `module rebuild`'s `docker compose up -d --build` is different on
//! purpose: it can legitimately run 30-90 minutes (a first build compiles
//! the whole AzerothCore core), so a wall-clock kill would abort real
//! progress, and the UI wants to see build output LIVE, not just at the end.
//! [`run_streamed_unbounded`] drains stdout/stderr on background threads
//! (same anti-deadlock discipline as `output_bounded_draining` — see that
//! function's doc comment for the live repro this avoids), forwards each
//! line to the caller as it arrives, tees it to a log file, and — critically
//! — never kills the child on any clock or on the caller's callback failing;
//! it only ever calls `child.wait()` once both pipes hit EOF.
//!
//! NATIVE-MODE-ONLY by convention: WSL keeps calling `dml`; the Tauri
//! command layer (`lib.rs`) gates every entry point on
//! `require_native_backend()`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Bounded/streamed generic subprocess helpers moved into `dml-core` (Task 4,
/// cargo-workspace refactor) — everything DOMAIN-specific (titles, volumes,
/// compose, removal targets) stays in this module; re-exported here so every
/// existing caller in this crate keeps compiling unchanged.
pub use dml_core::proc::{CapturedRun, run_captured, drain_lines, run_streamed_unbounded, combined_nonempty_lines};

/// Bounded timeout for the small local docker ops here (`volume ls`, `volume
/// rm`, `image rm`, `compose down` with no explicit `-t`) — mirrors
/// `maint::PROBE_TIMEOUT`'s "quick local read" budget, just a little more
/// generous since these are mutations, not pure reads.
pub const QUICK_OP_TIMEOUT: Duration = Duration::from_secs(30);
/// `docker builder prune -af` / `docker image prune -af` walk and delete
/// potentially many cache layers/images — more budget than
/// [`QUICK_OP_TIMEOUT`], but still bounded (best-effort: a timeout here
/// degrades to a warn line, never aborts the rest of `docker-clean`).
pub const PRUNE_TIMEOUT: Duration = Duration::from_secs(180);

// ---------------------------------------------------------------------------
// Title catalog — `_title_registry`/`_title_row`/`_title_installed`
// (`cli/src/80-titles.sh:12-40`), ported VERBATIM. This is the traversal
// guard for `games remove`: `id` must EXACT-match one of these six rows
// before any path (`games_dir.join(id)`, `home.join(id)`, ...) is ever
// built, closing off `../../etc/passwd`-style ids the same way the bash's
// `grep -m1 -F "$1|"` + prefix re-check does.
// ---------------------------------------------------------------------------

/// One `_title_registry` row: id, display name, installer script, kind
/// (`"games"` = the installer manages `~/games` itself, `"home"` = legacy
/// `$HOME/<id>` layout needing the post-install symlink), launcher file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TitleRow {
    pub id: &'static str,
    pub name: &'static str,
    pub installer: &'static str,
    pub kind: &'static str,
    pub launcher: &'static str,
    /// Which emulator family this title's installer BUILDS — a catalog default
    /// for a title that may not be installed yet, not a claim about any
    /// installed server. The operating answer is resolved from the server
    /// itself (`family::family_from_container_names`); this is what the Library
    /// filters on and what a fresh install records.
    ///
    /// `"azerothcore" | "cmangos" | "other"`. A string, not `CoreFamily`,
    /// because bash carries the identical value in `_title_registry`'s 6th
    /// field and the two surfaces must be byte-comparable.
    pub family: &'static str,
}

/// `_title_registry` (`80-titles.sh:12-21`), verbatim — six rows, in order.
pub const TITLE_REGISTRY: &[TitleRow] = &[
    TitleRow {
        id: "wow-server-playerbots",
        name: "WoW WotLK (Playerbots)",
        installer: "install-wow-wotlk.sh",
        kind: "games",
        launcher: "wow-playerbots-launcher.sh",
        family: "azerothcore",
    },
    TitleRow {
        id: "wow-vanilla-server",
        name: "WoW Vanilla",
        installer: "install-wow-vanilla.sh",
        kind: "home",
        launcher: "wow-vanilla-launcher.sh",
        family: "cmangos",
    },
    TitleRow {
        id: "wow-tbc-server",
        name: "WoW TBC",
        installer: "install-wow-tbc.sh",
        kind: "home",
        launcher: "wow-tbc-launcher.sh",
        family: "cmangos",
    },
    TitleRow {
        id: "maplestory-server",
        name: "MapleStory v83",
        installer: "install-maplestory.sh",
        kind: "home",
        launcher: "maplestory-launcher.sh",
        family: "other",
    },
    TitleRow {
        id: "runescape-server",
        name: "RuneScape",
        installer: "install-runescape.sh",
        kind: "home",
        launcher: "runescape-launcher.sh",
        family: "other",
    },
    TitleRow {
        id: "muonline-server",
        name: "MU Online",
        installer: "install-muonline.sh",
        kind: "home",
        launcher: "muonline-launcher.sh",
        family: "other",
    },
];

/// `_title_row` (`80-titles.sh:24-29`): the registry row for `id`, or `None`
/// on ANY non-exact match — this is the whole traversal guard, so it is
/// deliberately a plain `==` (no prefix/contains matching of any kind).
pub fn title_row(id: &str) -> Option<&'static TitleRow> {
    TITLE_REGISTRY.iter().find(|r| r.id == id)
}

/// `_title_installed` (`80-titles.sh:38-40`): present at either the
/// `games_dir` or legacy `home` location. `Path::is_dir()` follows symlinks
/// (matches bash `-d`, which is also true through a symlink to a directory,
/// false for a dangling one) — same as the oracle.
pub fn title_installed(games_dir: &Path, home: Option<&Path>, id: &str) -> bool {
    games_dir.join(id).is_dir() || home.map(|h| h.join(id).is_dir()).unwrap_or(false)
}

// ---------------------------------------------------------------------------
// `games remove`'s targets message — `90-main.sh:1212-1219`, the
// CONFIRM_REQUIRED error text. Native mode's Tauri command hardcodes
// confirm=true (the typed-id UI IS the user gate, matching the WSL sibling's
// hardcoded `--yes`) so this path is never actually reached in the shipped
// UI — ported anyway for parity/testability, same posture as `bots flush`'s
// CONFIRM logic per the task brief.
// ---------------------------------------------------------------------------

/// The `$targets` string `remove)` builds before the `CONFIRM_REQUIRED` gate.
/// `Path::is_symlink()` (stable, no `symlink_metadata` juggling needed) plus
/// `canonicalize()` for the bash's `readlink -f` (fully resolves the link;
/// `None` on any I/O error, matching `readlink -f ... || true`).
pub fn removal_targets(games_dir: &Path, home: Option<&Path>, id: &str, launcher_file: &str) -> String {
    let mut out = String::new();
    let glink = games_dir.join(id);
    let glink_is_symlink = glink.is_symlink();
    // `-e || -L`: any dirent at all (including a dangling symlink).
    if glink.exists() || glink_is_symlink {
        out.push_str(&glink.display().to_string());
        out.push(' ');
    }
    if glink_is_symlink {
        if let Ok(target) = std::fs::canonicalize(&glink) {
            out.push_str("-> ");
            out.push_str(&target.display().to_string());
            out.push(' ');
        }
    }
    if let Some(h) = home {
        let hdir = h.join(id);
        if hdir.is_dir() && !hdir.is_symlink() {
            out.push_str(&hdir.display().to_string());
            out.push(' ');
        }
        if !launcher_file.is_empty() {
            let lf = h.join(launcher_file);
            if lf.exists() {
                out.push_str(&lf.display().to_string());
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// `games remove`'s FS deletion — `90-main.sh:1297-1305`. [`games_dir_action`]
// is the PURE three-way decision (symlink / plain dir / neither), so the
// branch logic is unit-tested with synthetic facts; [`remove_title_fs`] is
// the thin live-I/O wrapper around it (not further unit-tested beyond the
// non-symlink branches below — creating a real symlink in a portable
// `cargo test` on Windows needs Developer Mode or an elevated process,
// which this sandbox cannot guarantee; the symlink removal path is covered
// by [`games_dir_action`]'s table plus the user-smoke gate).
// ---------------------------------------------------------------------------

/// The pure decision `90-main.sh:1297-1305` makes for the `$GAMES_DIR/$gid`
/// entry: a symlink removes itself PLUS (only if the resolved target is
/// itself a directory — the bash's `[[ -n "$ttarget" && -d "$ttarget" ]]`
/// gate) the resolved target; a plain directory just removes itself;
/// anything else (missing, a file, ...) does nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GamesDirAction {
    RemoveSymlinkAndTarget(Option<PathBuf>),
    RemoveDir,
    Nothing,
}

/// Decide [`GamesDirAction`] from already-known facts — pure, so every
/// branch is testable without touching a real symlink.
pub fn games_dir_action(is_symlink: bool, is_dir: bool, resolved_target: Option<PathBuf>) -> GamesDirAction {
    if is_symlink {
        GamesDirAction::RemoveSymlinkAndTarget(resolved_target)
    } else if is_dir {
        GamesDirAction::RemoveDir
    } else {
        GamesDirAction::Nothing
    }
}

/// Live wrapper: resolve the real facts for `games_dir.join(id)`, run
/// [`games_dir_action`], execute it, then the `home)` half (`[[ -d
/// "$HOME/$gid" ]] && rm -rf`, then the launcher-shortcut file). Every step
/// is best-effort (`rm -f`/`rm -rf` semantics — a missing/unremovable entry
/// is silently skipped, matching the bash's unchecked removes).
pub fn remove_title_fs(games_dir: &Path, home: Option<&Path>, id: &str, launcher_file: &str) {
    let glink = games_dir.join(id);
    let is_symlink = glink.is_symlink();
    let is_dir = glink.is_dir();
    let resolved_target = if is_symlink {
        std::fs::canonicalize(&glink).ok().filter(|t| t.is_dir())
    } else {
        None
    };
    match games_dir_action(is_symlink, is_dir, resolved_target) {
        GamesDirAction::RemoveSymlinkAndTarget(target) => {
            let _ = std::fs::remove_file(&glink);
            if let Some(t) = target {
                let _ = std::fs::remove_dir_all(&t);
            }
        }
        GamesDirAction::RemoveDir => {
            let _ = std::fs::remove_dir_all(&glink);
        }
        GamesDirAction::Nothing => {}
    }

    if let Some(h) = home {
        let hdir = h.join(id);
        if hdir.is_dir() {
            let _ = std::fs::remove_dir_all(&hdir);
        }
        if !launcher_file.is_empty() {
            let _ = std::fs::remove_file(h.join(launcher_file));
        }
    }
}

// ---------------------------------------------------------------------------
// Project-name sanitizers. `docker-clean`'s build-volume search and `games
// remove`'s client-data-volume name BOTH lowercase a directory basename then
// strip everything outside an allowlist — but the allowlist DIFFERS by one
// character (`_`), so these are deliberately TWO functions, not one shared
// helper with a flag: conflating them is exactly the kind of one-character
// slip that would resurrect the leaked-6GB-volume / wrong-volume-deleted bug
// `_compose_server_images`'s own doc comment (90-main.sh:1241-1251) warns
// about.
// ---------------------------------------------------------------------------

/// `tr '[:upper:]' '[:lower:]' | tr -cd 'a-z0-9-'` — docker-clean's build
/// volume project derivation (`90-main.sh:1563`). NO underscore.
pub fn sanitize_project_no_underscore(raw: &str) -> String {
    raw.to_ascii_lowercase().chars().filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-').collect()
}

/// `tr -cd 'a-z0-9_-'` — games-remove's client-data-volume project
/// derivation (`90-main.sh:1262`). Underscore IS kept.
pub fn sanitize_project_with_underscore(raw: &str) -> String {
    raw.to_ascii_lowercase()
        .chars()
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-' || *c == '_')
        .collect()
}

/// `basename "$dir"` — the final path component, or `""` if `dir` has none
/// (root/empty path; never happens for a real title dir, but a total
/// function beats a panic).
pub fn basename(dir: &Path) -> String {
    dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Build-volume regex — `grep -E "^${dcproject}.*(ac.build|build)"`
// (`90-main.sh:1565`), hand-rolled (no `regex` crate in this workspace).
// THIS REGEX IS THE GUARD keeping `ac-database`/`ac-client-data` safe from
// `docker-clean --level 2`'s volume removal — porting it exactly (not an
// approximation) is load-bearing.
// ---------------------------------------------------------------------------

/// `(ac.build|build)` found ANYWHERE in `tail` — `.` matches any single
/// `char` (not just literal `.`), so `"ac_build"`/`"ac-build"`/`"acXbuild"`
/// all match `ac.build`, same as POSIX ERE. Character-based (not byte-based)
/// so a multi-byte char occupying the wildcard slot can never panic on a
/// misaligned boundary.
fn contains_ac_anychar_build(tail: &str) -> bool {
    let chars: Vec<char> = tail.chars().collect();
    let n = chars.len();
    for i in 0..n {
        // Need chars[i..i+2] == "ac", chars[i+2] as the wildcard (must
        // exist), then chars[i+3..i+8] == "build".
        if i + 8 > n {
            break;
        }
        if chars[i] == 'a' && chars[i + 1] == 'c' && chars[i + 3..i + 8].iter().collect::<String>() == "build" {
            return true;
        }
    }
    false
}

/// `^${project}.*(ac.build|build)`: `project` must match at the very start
/// of `volname` (a literal prefix — `project` itself carries no wildcards),
/// then `(ac.build|build)` must appear somewhere from that point on
/// (`.*` before the group is unbounded, so the match can start anywhere at
/// or after `project`'s end).
pub fn matches_build_volume(volname: &str, project: &str) -> bool {
    let Some(tail) = volname.strip_prefix(project) else { return false };
    tail.contains("build") || contains_ac_anychar_build(tail)
}

/// `docker volume ls --format '{{.Name}}' | grep -E ... | head -1` — pure
/// half: first name in `names` (in the order docker returned them, matching
/// `head -1`) that satisfies [`matches_build_volume`], or `None`.
pub fn find_build_volume<'a>(names: &'a [String], project: &str) -> Option<&'a str> {
    names.iter().map(String::as_str).find(|n| matches_build_volume(n, project))
}

// ---------------------------------------------------------------------------
// `games remove`'s client-data volume — `90-main.sh:1231-1273`.
// ---------------------------------------------------------------------------

/// The four canonical compose filenames, checked in this order everywhere
/// they're scanned in this module (matches the bash's own `for _c in ...`
/// loops).
pub const COMPOSE_FILE_NAMES: &[&str] = &["docker-compose.yml", "docker-compose.yaml", "compose.yml", "compose.yaml"];

/// `grep -qE '^[[:space:]]*ac-client-data:'` applied to one file's text —
/// line-anchored (leading whitespace only, then the literal key), NOT a
/// bare substring search anywhere in the line.
fn has_client_data_key_line(text: &str) -> bool {
    text.lines().any(|l| l.trim_start().starts_with("ac-client-data:"))
}

/// `90-main.sh:1253-1260`: does ANY of the title's compose files declare the
/// top-level `ac-client-data:` volume? First match wins (matches the bash's
/// `break` on the first hit).
pub fn compose_declares_client_data(compose_dir: &Path) -> bool {
    COMPOSE_FILE_NAMES.iter().any(|name| {
        std::fs::read_to_string(compose_dir.join(name)).map(|t| has_client_data_key_line(&t)).unwrap_or(false)
    })
}

/// `"${vproj}_ac-client-data"` (`90-main.sh:1261-1263`). DELIBERATELY takes
/// no env override of any kind — see the module doc comment / the bash's own
/// extensive comment (`90-main.sh:1241-1251`) for why honoring
/// `DOCKER_VOL_DATA` here would either build a volume name that cannot exist
/// (leaking the real ~6 GB volume) or, worse, resolve onto `ac-database` and
/// delete the wrong data entirely.
pub fn client_data_volume_name(compose_dir: &Path) -> String {
    format!("{}_ac-client-data", sanitize_project_with_underscore(&basename(compose_dir)))
}

// ---------------------------------------------------------------------------
// `_compose_server_images` (`90-main.sh:80-103`) — the image list `--remove-
// images` deletes.
// ---------------------------------------------------------------------------

/// `_c`'s scan order for image extraction — the four canonical names PLUS
/// the override file (matches `90-main.sh:86`).
pub const COMPOSE_FILE_NAMES_WITH_OVERRIDE: &[&str] =
    &["docker-compose.yml", "docker-compose.yaml", "compose.yml", "compose.yaml", "docker-compose.override.yml"];

/// `grep -m1 '^DOCKER_IMAGE_TAG=' .env | cut -d= -f2-` (`90-main.sh:83-84`):
/// first matching line, everything after the FIRST `=` (so a value that
/// itself contains `=` is kept whole). A trailing `\r` is stripped — this
/// native path runs directly on Windows, where a `.env` can carry CRLF line
/// endings the bash-on-WSL oracle never has to account for (same rationale
/// as `maint::parse_docker_usage_lines`'s own `\r` strip).
fn parse_docker_image_tag(env_text: &str) -> Option<String> {
    for line in env_text.lines() {
        if let Some(v) = line.strip_prefix("DOCKER_IMAGE_TAG=") {
            let v = v.trim_end_matches('\r');
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

/// One `image:` line's extracted value, BEFORE tag substitution — a port of
/// the bash's parameter-expansion chain (`90-main.sh:89-92`): line must
/// start with `image:` after only leading whitespace, followed (after
/// further whitespace) by a non-space token; `None` for any line that
/// doesn't shape up (mirrors the bash's `grep -E` pre-filter — a
/// non-matching line contributes nothing).
fn parse_image_line(line: &str) -> Option<String> {
    let after = line.trim_start().strip_prefix("image:")?;
    after.split_whitespace().next().map(str::to_string)
}

/// `_compose_server_images` (`90-main.sh:80-103`), pure half: given the
/// already-read text of each compose file (in [`COMPOSE_FILE_NAMES_WITH_OVERRIDE`]
/// order) plus the resolved image tag, extract every `image:` value,
/// substitute `${DOCKER_IMAGE_TAG:-master}`/`${DOCKER_IMAGE_TAG}`, skip any
/// that still carries an unresolved `${...}`, and dedupe preserving first-
/// seen order.
pub fn extract_server_images(compose_texts: &[Option<String>], image_tag: &str) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for text in compose_texts.iter().flatten() {
        for line in text.lines() {
            let Some(raw) = parse_image_line(line) else { continue };
            let img = raw
                .replace("${DOCKER_IMAGE_TAG:-master}", image_tag)
                .replace("${DOCKER_IMAGE_TAG}", image_tag);
            if img.contains("${") {
                continue;
            }
            if !seen.iter().any(|s| s == &img) {
                seen.push(img);
            }
        }
    }
    seen
}

/// Live `_compose_server_images`: reads `.env` for the tag override, then
/// every compose file in [`COMPOSE_FILE_NAMES_WITH_OVERRIDE`] order, and
/// assembles the image list via [`extract_server_images`].
pub fn compose_server_images(compose_dir: &Path) -> Vec<String> {
    let image_tag = std::fs::read_to_string(compose_dir.join(".env"))
        .ok()
        .and_then(|t| parse_docker_image_tag(&t))
        .unwrap_or_else(|| "master".to_string());
    let texts: Vec<Option<String>> = COMPOSE_FILE_NAMES_WITH_OVERRIDE
        .iter()
        .map(|name| std::fs::read_to_string(compose_dir.join(name)).ok())
        .collect();
    extract_server_images(&texts, &image_tag)
}

// ---------------------------------------------------------------------------
// NATIVE-MODE `games remove` (Chunk 4a): faithful port of the `remove)` arm
// (`90-main.sh:1184-1309`) + its shared helpers `_title_row`/
// `_title_installed`/`_resolve_compose_dir`/`_compose_server_images`. Same
// NDJSON vocabulary as `lifecycle::world_restart_stream`. Traversal guard
// FIRST: `id` must EXACT-match the static title registry above before any
// path is built from it. Moved out of the launcher's `lib.rs` by the
// cargo-workspace refactor (Task 9) — the Tauri command hardcodes
// confirm=true (the typed-id UI IS the user gate, matching the WSL sibling's
// hardcoded `--yes`), but the CONFIRM_REQUIRED gate stays ported here for
// parity/testability and for the CLI's own `--yes` guard.
// ---------------------------------------------------------------------------

const GAMES_REMOVE_SECTION: &str = "games-remove";

pub fn games_remove_stream(
    id: String,
    keep_data: bool,
    remove_images: bool,
    confirm: bool,
    emit: impl Fn(serde_json::Value),
) {
    use crate::{destructive, lifecycle, native, status};

    emit(crate::modmgr::section_start(GAMES_REMOVE_SECTION));

    let Some(row) = destructive::title_row(&id) else {
        emit(crate::modmgr::section_end(GAMES_REMOVE_SECTION, "error"));
        emit(crate::modmgr::error_event("BAD_ARG", format!("Unknown title: {id}"), ""));
        return;
    };

    let games_dir = lifecycle::games_dir_from_env();
    let home = crate::home_dir();
    if !destructive::title_installed(&games_dir, home.as_deref(), &id) {
        emit(crate::modmgr::section_end(GAMES_REMOVE_SECTION, "error"));
        emit(crate::modmgr::error_event("NOT_FOUND", format!("{id} is not installed"), ""));
        return;
    }

    if !confirm {
        let targets = destructive::removal_targets(&games_dir, home.as_deref(), &id, row.launcher);
        emit(crate::modmgr::section_end(GAMES_REMOVE_SECTION, "error"));
        emit(crate::modmgr::error_event(
            "CONFIRM_REQUIRED",
            format!("Removing {id} deletes: {targets}"),
            "Re-run with --yes (add --remove-images to also delete the server docker images). Backups under ~/.dml are kept.",
        ));
        return;
    }

    let tdir = if games_dir.join(&id).is_dir() {
        games_dir.join(&id)
    } else {
        home.as_deref().map(|h| h.join(&id)).unwrap_or_else(|| games_dir.join(&id))
    };
    let tcompose = lifecycle::resolve_compose_dir(&tdir);

    let docker_program = native::docker_program();

    if let Some(compose_dir) = &tcompose {
        emit(crate::modmgr::line_event("info", format!("stopping {id}...")));
        let mut down_cmd = std::process::Command::new(&docker_program);
        down_cmd.current_dir(compose_dir).args(["compose", "down"]);
        status::windows_no_window(&mut down_cmd);
        let _ = status::output_bounded_draining(down_cmd, destructive::QUICK_OP_TIMEOUT);
    }

    // --- client-data volume (Batch 3 F13c parity) ---------------------------
    if let Some(compose_dir) = &tcompose {
        if destructive::compose_declares_client_data(compose_dir) {
            let tvol = destructive::client_data_volume_name(compose_dir);
            if keep_data {
                emit(crate::modmgr::line_event(
                    "info",
                    format!("keeping the downloaded game data volume ({tvol}, ~6 GB) for a faster reinstall"),
                ));
            } else {
                let mut rm_cmd = std::process::Command::new(&docker_program);
                rm_cmd.args(["volume", "rm", &tvol]);
                status::windows_no_window(&mut rm_cmd);
                if matches!(status::output_bounded_draining(rm_cmd, destructive::QUICK_OP_TIMEOUT), Some(o) if o.status.success())
                {
                    emit(crate::modmgr::line_event("info", format!("removed game data volume {tvol}")));
                } else {
                    emit(crate::modmgr::line_event(
                        "warn",
                        format!("could not remove game data volume {tvol} (may not exist or still in use)"),
                    ));
                }
            }
        }
    }

    // --- server docker images (Batch 6 B parity) ----------------------------
    if remove_images {
        if let Some(compose_dir) = &tcompose {
            let images = destructive::compose_server_images(compose_dir);
            let mut removed_count = 0;
            for img in &images {
                let mut rm_cmd = std::process::Command::new(&docker_program);
                rm_cmd.args(["image", "rm", img]);
                status::windows_no_window(&mut rm_cmd);
                if matches!(status::output_bounded_draining(rm_cmd, destructive::QUICK_OP_TIMEOUT), Some(o) if o.status.success())
                {
                    emit(crate::modmgr::line_event("info", format!("removed server image {img}")));
                    removed_count += 1;
                } else {
                    emit(crate::modmgr::line_event(
                        "warn",
                        format!("could not remove image {img} (in use by another title, or already gone)"),
                    ));
                }
            }
            if removed_count == 0 {
                emit(crate::modmgr::line_event("info", "no server images to remove"));
            }
        }
    } else if tcompose.is_some() {
        emit(crate::modmgr::line_event(
            "info",
            "kept the downloaded server images for a faster reinstall (use --remove-images to delete them)",
        ));
    }

    destructive::remove_title_fs(&games_dir, home.as_deref(), &id, row.launcher);

    emit(crate::modmgr::line_event("info", "removed (backups under ~/.dml are kept)"));
    emit(crate::modmgr::section_end(GAMES_REMOVE_SECTION, "ok"));
    emit(crate::modmgr::done_event(serde_json::json!({"id": id, "removed": true})));
}

// ---------------------------------------------------------------------------
// NATIVE-MODE `docker-clean` (Chunk 4a): faithful port of the `docker-clean)`
// arm (`90-main.sh:1519-1591`). Every docker call here is best-effort past
// the initial NOT_FOUND/DOCKER_DOWN gates (a partial clean is still useful,
// matching the bash's own doctrine) — `docker builder prune -af`/`docker
// image prune -af` are captured-then-split (NOT the live streaming
// [`run_streamed_unbounded`] `module rebuild` uses; the bash itself captures
// the WHOLE output into a variable first, then loops it, so this is the
// byte-faithful shape).
// ---------------------------------------------------------------------------

const DOCKER_CLEAN_SECTION: &str = "docker-clean";

pub fn docker_clean_stream(level: u8, emit: impl Fn(serde_json::Value)) {
    use crate::{config::ConfigReader, destructive, lifecycle, maint, modmgr, native, status};

    emit(modmgr::section_start(DOCKER_CLEAN_SECTION));

    if !(1..=3).contains(&level) {
        emit(modmgr::section_end(DOCKER_CLEAN_SECTION, "error"));
        emit(modmgr::error_event(
            "BAD_ARG",
            "Level must be 1, 2, or 3",
            "Usage: dml wow docker-clean --level 1|2|3 --json",
        ));
        return;
    }

    let title_dir = ConfigReader::title_dir_from_env();
    let Some(sdir) = maint::resolve_server_dir(&title_dir) else {
        emit(modmgr::section_end(DOCKER_CLEAN_SECTION, "error"));
        emit(modmgr::error_event("NOT_FOUND", "WoW Playerbots server not installed", "Install it first."));
        return;
    };

    let docker_program = native::docker_program();
    if !maint::docker_engine_up(&docker_program, maint::PROBE_TIMEOUT) {
        emit(modmgr::section_end(DOCKER_CLEAN_SECTION, "error"));
        emit(modmgr::error_event("DOCKER_DOWN", "Docker is not running", "Start Docker in the distro first."));
        return;
    }

    emit(modmgr::line_event("info", "protecting the database volume..."));
    let mut up_cmd = std::process::Command::new(&docker_program);
    up_cmd.current_dir(&sdir).args(["compose", "up", "-d", "ac-database"]);
    status::windows_no_window(&mut up_cmd);
    if !matches!(status::output_bounded_draining(up_cmd, lifecycle::COMPOSE_UP_TIMEOUT), Some(o) if o.status.success()) {
        emit(modmgr::line_event("warn", "could not start ac-database -- continuing"));
    }

    emit(modmgr::line_event("info", "stopping worldserver..."));
    let mut stop_cmd = std::process::Command::new(&docker_program);
    stop_cmd.current_dir(&sdir).args(["compose", "stop", "-t", "180", "ac-worldserver"]);
    status::windows_no_window(&mut stop_cmd);
    if !matches!(status::output_bounded_draining(stop_cmd, lifecycle::COMPOSE_DOWN_TIMEOUT), Some(o) if o.status.success())
    {
        emit(modmgr::line_event("warn", "could not stop worldserver -- continuing"));
    }

    emit(modmgr::line_event("info", "pruning build cache..."));
    let builder_run = destructive::run_captured(&docker_program, &["builder", "prune", "-af"], destructive::PRUNE_TIMEOUT);
    for line in &builder_run.lines {
        emit(modmgr::line_event("info", line.clone()));
    }
    if !builder_run.success {
        let code = builder_run.code.map(|c| c.to_string()).unwrap_or_else(|| "?".to_string());
        emit(modmgr::line_event("warn", format!("build cache prune exited {code} -- may already be empty")));
    }

    if level >= 2 {
        emit(modmgr::line_event("info", "identifying build volume..."));
        let project = destructive::sanitize_project_no_underscore(&destructive::basename(&sdir));
        let mut ls_cmd = std::process::Command::new(&docker_program);
        ls_cmd.args(["volume", "ls", "--format", "{{.Name}}"]);
        status::windows_no_window(&mut ls_cmd);
        let names: Vec<String> = match status::output_bounded_draining(ls_cmd, destructive::QUICK_OP_TIMEOUT) {
            Some(out) if out.status.success() => {
                String::from_utf8_lossy(&out.stdout).lines().map(str::to_string).collect()
            }
            _ => Vec::new(),
        };
        match destructive::find_build_volume(&names, &project).map(str::to_string) {
            Some(vol) => {
                emit(modmgr::line_event("info", format!("removing build volume: {vol}")));
                let mut rm_cmd = std::process::Command::new(&docker_program);
                rm_cmd.args(["volume", "rm", &vol]);
                status::windows_no_window(&mut rm_cmd);
                if matches!(status::output_bounded_draining(rm_cmd, destructive::QUICK_OP_TIMEOUT), Some(o) if o.status.success())
                {
                    emit(modmgr::line_event("info", "build volume removed -- CMake cache cleared."));
                } else {
                    emit(modmgr::line_event("warn", format!("could not remove {vol} (may still be in use)")));
                }
            }
            None => {
                emit(modmgr::line_event(
                    "info",
                    format!("no build volume found matching '{project}*build' -- nothing to remove"),
                ));
            }
        }
    }

    if level >= 3 {
        emit(modmgr::line_event("info", "pruning unused images..."));
        let image_run = destructive::run_captured(&docker_program, &["image", "prune", "-af"], destructive::PRUNE_TIMEOUT);
        for line in &image_run.lines {
            emit(modmgr::line_event("info", line.clone()));
        }
        if !image_run.success {
            let code = image_run.code.map(|c| c.to_string()).unwrap_or_else(|| "?".to_string());
            emit(modmgr::line_event("warn", format!("image prune exited {code}")));
        }
    }

    emit(modmgr::line_event("info", "Next rebuild will be a full recompile (30-90 min)."));
    emit(modmgr::section_end(DOCKER_CLEAN_SECTION, "ok"));
    emit(modmgr::done_event(serde_json::json!({"level": level, "cleaned": true})));
}

// ---------------------------------------------------------------------------
// NATIVE-MODE `wow bots flush` (Chunk 4b, Part 2): faithful port of the
// `flush)` arm (`90-main.sh:3945-4093`) + `_flush_restart_authworld`
// (`40-config.sh:698-726`) — the second of the two scariest commands in the
// product. THE GUARD: [`super::lifecycle::FlushGuard`] is the Rust analogue
// of the bash arm's EXIT + signal traps (see that struct's doc comment for
// the full trap-mapping rationale) — construct it AFTER the marker/flag-arm
// decision is reached, and call `.disarm()` only once the flag is ALREADY
// back at 0 on disk, right before the rebuild restart. No `--yes`/`--ack`
// parameter exists here: the launcher's typed-"flush" UI is its gate, and
// the CLI supplies its own via `lifecycle::bots_flush_confirmed`.
// ---------------------------------------------------------------------------

const BOTS_FLUSH_SECTION: &str = "bots-flush";

/// One `_flush_restart_authworld` outcome (`40-config.sh:698-726`): `Ready`
/// (rc 0), `ComposeFailed` (rc 1 — a `compose stop`/`compose up` call
/// itself failed), or `Timeout` (rc 2 — the world never came back inside
/// `DML_READY_TIMEOUT_SECS`).
enum FlushRestartOutcome {
    Ready,
    ComposeFailed,
    Timeout,
}

/// `_flush_restart_authworld` (`40-config.sh:698-726`): one staged auth+world
/// restart, reused for BOTH the bot-deletion boot and the rebuild boot (only
/// `label` differs, feeding the "still waiting" progress line). Reuses the
/// exact same timeout/heartbeat pure helpers `wow_world_restart_native_
/// blocking` already established (`wr_ready_timeout_secs`/`wr_timeout_
/// exceeded`/`wr_should_note_wait`) and `status::world_ready` for the
/// readiness poll — no duplicate polling logic.
fn flush_restart_authworld(
    program: &std::ffi::OsStr,
    sdir: &std::path::Path,
    soap_lock: &Arc<Mutex<()>>,
    label: &str,
    emit: &impl Fn(serde_json::Value),
) -> FlushRestartOutcome {
    use crate::{lifecycle, maint, modmgr, soap, status};

    emit(modmgr::line_event("info", "saving all characters (best effort)..."));
    {
        let _guard = soap_lock.lock().unwrap_or_else(|e| e.into_inner());
        let cfg = soap::SoapConfig::load();
        let _ = soap::exec(&cfg, "saveall");
    }

    emit(modmgr::line_event("info", format!("stopping auth + world ({label})...")));
    let mut stop_cmd = std::process::Command::new(program);
    stop_cmd.current_dir(sdir).args(["compose", "stop", "-t", "180", "ac-worldserver", "ac-authserver"]);
    status::windows_no_window(&mut stop_cmd);
    if !matches!(status::output_bounded_draining(stop_cmd, lifecycle::COMPOSE_DOWN_TIMEOUT), Some(o) if o.status.success())
    {
        return FlushRestartOutcome::ComposeFailed;
    }

    emit(modmgr::line_event("info", "starting auth + world (compose, no deps)..."));
    let mut up_cmd = std::process::Command::new(program);
    // `--no-deps` is deliberate (matches the oracle exactly): skips the
    // db-import/client-data one-shot init containers, which only need to run
    // once, ever.
    up_cmd.current_dir(sdir).args(["compose", "up", "-d", "--no-deps", "ac-authserver", "ac-worldserver"]);
    status::windows_no_window(&mut up_cmd);
    if !matches!(status::output_bounded_draining(up_cmd, lifecycle::COMPOSE_UP_TIMEOUT), Some(o) if o.status.success())
    {
        return FlushRestartOutcome::ComposeFailed;
    }

    emit(modmgr::line_event("info", format!("waiting for the world ({label})...")));
    let timeout_secs = crate::lifecycle::wr_ready_timeout_secs();
    let t0 = std::time::Instant::now();
    let mut last_note: u64 = 0;
    loop {
        if status::world_ready(program, maint::PROBE_TIMEOUT) {
            return FlushRestartOutcome::Ready;
        }
        let elapsed = t0.elapsed().as_secs();
        if crate::lifecycle::wr_timeout_exceeded(elapsed, timeout_secs) {
            return FlushRestartOutcome::Timeout;
        }
        if crate::lifecycle::wr_should_note_wait(elapsed, last_note) {
            last_note = elapsed;
            emit(modmgr::line_event(
                "info",
                format!("still waiting (~{}m) - deleting/creating thousands of bots takes a while...", elapsed / 60),
            ));
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
}

/// The blocking flow itself (real docker/SOAP/fs I/O) — run under
/// `spawn_blocking`. Order mirrors the oracle top-to-bottom: docker up? ->
/// server installed? -> playerbots.conf ensured? -> (1) chars-only safety
/// backup (hard-fail, nothing changed yet) -> (2) arm (marker then flag) ->
/// (3) restart #1 (the wipe happens during this boot) -> (4)+(5) disarm
/// (flag back to 0, remove marker) BEFORE the rebuild restart -> (6) restart
/// #2 (rebuild) -> (7) done.
pub fn bots_flush_stream(soap_lock: Arc<Mutex<()>>, db_cfg: crate::db::DbConfig, emit: impl Fn(serde_json::Value)) {
    use crate::{backup, config::ConfigReader, lifecycle, maint, modmgr, native};

    emit(modmgr::section_start(BOTS_FLUSH_SECTION));

    let docker_program = native::docker_program();
    if !maint::docker_engine_up(&docker_program, maint::PROBE_TIMEOUT) {
        emit(modmgr::section_end(BOTS_FLUSH_SECTION, "error"));
        emit(modmgr::error_event("DOCKER_DOWN", "Docker is not running", "Start Docker in the distro first."));
        return;
    }

    let title_dir = ConfigReader::title_dir_from_env();
    let Some(sdir) = maint::resolve_server_dir(&title_dir) else {
        emit(modmgr::section_end(BOTS_FLUSH_SECTION, "error"));
        emit(modmgr::error_event("NOT_FOUND", "WoW Playerbots server not installed", "Install it first."));
        return;
    };

    let conf_path = lifecycle::flush_conf_path(&sdir);
    match crate::config::conf_ensure(&conf_path) {
        Ok(true) => {}
        Ok(false) => {
            emit(modmgr::section_end(BOTS_FLUSH_SECTION, "error"));
            emit(modmgr::error_event("NOT_FOUND", "playerbots.conf not found (nor its .dist)", "Is the WoW server fully installed?"));
            return;
        }
        Err(_) => {
            emit(modmgr::section_end(BOTS_FLUSH_SECTION, "error"));
            emit(modmgr::error_event("NOT_FOUND", "playerbots.conf not found (nor its .dist)", "Is the WoW server fully installed?"));
            return;
        }
    }

    let t0 = std::time::Instant::now();

    // (1) safety backup FIRST -- a failed dump aborts before any destructive
    // step, nothing has changed yet. Deliberately CHARS-ONLY (no
    // --include-world): matches `_backup_dump_to ... 0` exactly, NOT the
    // world-inclusive `modmgr::module_backup_now` module install/update/
    // rebuild use -- a bot flush never touches `acore_world`.
    emit(modmgr::line_event("info", "backing up characters, bots and accounts first..."));
    let Some(bdir) = backup::backup_dir() else {
        emit(modmgr::section_end(BOTS_FLUSH_SECTION, "error"));
        emit(modmgr::error_event("BACKUP_FAILED", "The safety backup failed - nothing was changed", ""));
        return;
    };
    if std::fs::create_dir_all(&bdir).is_err() {
        emit(modmgr::section_end(BOTS_FLUSH_SECTION, "error"));
        emit(modmgr::error_event("BACKUP_FAILED", "The safety backup failed - nothing was changed", ""));
        return;
    }
    let bfile = backup::new_backup_file_name(false);
    let bpath = bdir.join(&bfile);
    if let Err(errtail) = backup::dump_to(&docker_program, &db_cfg.password, false, &bpath) {
        emit(modmgr::section_end(BOTS_FLUSH_SECTION, "error"));
        emit(modmgr::error_event("BACKUP_FAILED", "The safety backup failed - nothing was changed", &errtail));
        return;
    }
    emit(modmgr::line_event("info", format!("backup created: {bfile}")));
    for p in backup::prune(&bdir) {
        emit(modmgr::line_event("info", format!("pruned old backup: {p}")));
    }

    // (2) arm: marker FIRST (best-effort, inside FlushGuard::arm), then the
    // conf flag itself -- see `FlushGuard`'s doc comment for the full
    // trap-mapping rationale. From this point on, ANY early return (or a
    // panic) restores the flag + removes the marker via `Drop`, unless
    // `guard.disarm()` has already run.
    let marker_path = lifecycle::flush_marker_path(&sdir);
    let guard = lifecycle::FlushGuard::arm(conf_path.clone(), marker_path);
    if crate::config::conf_write(&conf_path, "AiPlayerbot.DeleteRandomBotAccounts", "1").is_err() {
        emit(modmgr::section_end(BOTS_FLUSH_SECTION, "error"));
        emit(modmgr::error_event("WRITE_FAILED", "Could not write playerbots.conf", ""));
        return; // guard still armed -> Drop restores 0 + removes the marker.
    }
    emit(modmgr::line_event("info", "delete flag armed - restarting so the server wipes the random bots..."));

    // (3) restart #1: the wipe happens during this boot.
    match flush_restart_authworld(&docker_program, &sdir, &soap_lock, "bot deletion", &emit) {
        FlushRestartOutcome::Ready => {}
        FlushRestartOutcome::ComposeFailed => {
            emit(modmgr::section_end(BOTS_FLUSH_SECTION, "error"));
            emit(modmgr::error_event(
                "RESTART_FAILED",
                "Could not restart the server for bot deletion",
                "The delete flag was restored to 0. Check the server from Home.",
            ));
            return; // guard still armed -> Drop restores 0 + removes the marker.
        }
        FlushRestartOutcome::Timeout => {
            emit(modmgr::section_end(BOTS_FLUSH_SECTION, "error"));
            emit(modmgr::error_event(
                "TIMEOUT",
                "Timed out waiting for the world during bot deletion",
                "The delete flag was restored to 0. Check the server from Home, then try again.",
            ));
            return; // guard still armed -> Drop restores 0 + removes the marker.
        }
    }

    // (4)+(5): bots are gone -- put the flag back BEFORE the rebuild
    // restart, or the next boot would wipe them again. Disarm the guard only
    // once this write has actually succeeded.
    emit(modmgr::line_event("info", "bots deleted - restoring the setting..."));
    if crate::config::conf_write(&conf_path, "AiPlayerbot.DeleteRandomBotAccounts", "0").is_err() {
        emit(modmgr::section_end(BOTS_FLUSH_SECTION, "error"));
        emit(modmgr::error_event(
            "WRITE_FAILED",
            "Could not restore playerbots.conf - fix AiPlayerbot.DeleteRandomBotAccounts back to 0 by hand before the next restart",
            "",
        ));
        return; // guard still armed -> Drop retries the write + removes the marker anyway.
    }
    guard.disarm();

    // (6) restart #2: the server recreates the population from the current
    // Bot World settings during this boot. The guard is already disarmed --
    // nothing left here can wipe the flag back on.
    emit(modmgr::line_event("info", "restarting again to rebuild the bot population (this is the long part)..."));
    match flush_restart_authworld(&docker_program, &sdir, &soap_lock, "bot rebuild", &emit) {
        FlushRestartOutcome::Ready => {}
        FlushRestartOutcome::ComposeFailed => {
            emit(modmgr::section_end(BOTS_FLUSH_SECTION, "error"));
            emit(modmgr::error_event(
                "RESTART_FAILED",
                "Could not restart the server for the rebuild",
                "Start it from Home - the delete flag is already back at 0.",
            ));
            return;
        }
        FlushRestartOutcome::Timeout => {
            emit(modmgr::section_end(BOTS_FLUSH_SECTION, "error"));
            emit(modmgr::error_event(
                "TIMEOUT",
                "Timed out waiting for the world during the rebuild",
                "The bots may still be logging in - check Home before retrying.",
            ));
            return;
        }
    }

    // (7) done.
    let elapsed_secs = t0.elapsed().as_secs();
    emit(modmgr::section_end(BOTS_FLUSH_SECTION, "ok"));
    emit(modmgr::done_event(serde_json::json!({
        "flushed": true, "backup": bfile, "elapsed_secs": elapsed_secs,
    })));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("dml-destructive-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    // -- title registry --------------------------------------------------

    #[test]
    fn title_registry_has_exactly_six_rows() {
        assert_eq!(TITLE_REGISTRY.len(), 6);
    }

    #[test]
    fn title_row_exact_match_only() {
        let row = title_row("wow-server-playerbots").unwrap();
        assert_eq!(row.kind, "games");
        assert_eq!(row.launcher, "wow-playerbots-launcher.sh");

        let row2 = title_row("muonline-server").unwrap();
        assert_eq!(row2.name, "MU Online");
    }

    #[test]
    fn title_row_rejects_prefix_suffix_and_traversal_ids() {
        assert!(title_row("wow-server-playerbots-extra").is_none());
        assert!(title_row("wow-server-playerbot").is_none());
        assert!(title_row("../../etc/passwd").is_none());
        assert!(title_row("").is_none());
        assert!(title_row("WOW-SERVER-PLAYERBOTS").is_none()); // case-sensitive
    }

    #[test]
    fn every_title_declares_a_family() {
        for row in TITLE_REGISTRY {
            assert!(
                matches!(row.family, "azerothcore" | "cmangos" | "other"),
                "{}: family {:?} is not one of the three known values",
                row.id,
                row.family
            );
        }
    }

    #[test]
    fn the_wow_titles_declare_the_family_their_installer_builds() {
        for (id, family) in [
            ("wow-server-playerbots", "azerothcore"),
            ("wow-vanilla-server", "cmangos"),
            ("wow-tbc-server", "cmangos"),
            ("maplestory-server", "other"),
            ("runescape-server", "other"),
            ("muonline-server", "other"),
        ] {
            let row = title_row(id).unwrap_or_else(|| panic!("{id} missing from the registry"));
            assert_eq!(row.family, family, "{id}");
        }
    }

    /// THE MIRROR. `_title_registry` and `TITLE_REGISTRY` are the same table on
    /// two surfaces; a family added to one and not the other is a Library that
    /// shows different titles depending on which binary answered.
    #[test]
    fn the_bash_registry_carries_the_same_families() {
        let sh = include_str!("../../../cli/src/80-titles.sh").replace("\r\n", "\n");
        let body = sh
            .split_once("_title_registry() {\ncat <<'EOF'\n")
            .expect("the _title_registry heredoc was renamed or reshaped")
            .1
            .split_once("\nEOF\n")
            .expect("unterminated _title_registry heredoc")
            .0;
        let rows: Vec<&str> = body.lines().filter(|l| !l.trim().is_empty()).collect();
        assert_eq!(
            rows.len(),
            TITLE_REGISTRY.len(),
            "bash has {} rows, Rust has {}",
            rows.len(),
            TITLE_REGISTRY.len()
        );
        for (line, row) in rows.iter().zip(TITLE_REGISTRY) {
            let f: Vec<&str> = line.split('|').collect();
            assert_eq!(f.len(), 6, "row {line:?} does not have 6 fields");
            assert_eq!(f[0], row.id, "id mismatch in {line:?}");
            assert_eq!(f[5], row.family, "family mismatch for {}", row.id);
        }
    }

    // -- title_installed ---------------------------------------------------

    #[test]
    fn title_installed_checks_both_locations() {
        let games = tmp_dir("installed-games");
        let home = tmp_dir("installed-home");
        assert!(!title_installed(&games, Some(&home), "foo"));

        std::fs::create_dir_all(games.join("foo")).unwrap();
        assert!(title_installed(&games, Some(&home), "foo"));

        assert!(!title_installed(&games, Some(&home), "bar"));
        std::fs::create_dir_all(home.join("bar")).unwrap();
        assert!(title_installed(&games, Some(&home), "bar"));

        assert!(!title_installed(&games, None, "bar"));

        std::fs::remove_dir_all(&games).unwrap();
        std::fs::remove_dir_all(&home).unwrap();
    }

    // -- removal_targets (non-symlink branches) -----------------------------

    #[test]
    fn removal_targets_empty_when_nothing_present() {
        let games = tmp_dir("targets-empty-games");
        let home = tmp_dir("targets-empty-home");
        assert_eq!(removal_targets(&games, Some(&home), "gone", "launcher.sh"), "");
        std::fs::remove_dir_all(&games).unwrap();
        std::fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn removal_targets_lists_games_dir_and_home_dir_and_launcher() {
        let games = tmp_dir("targets-full-games");
        let home = tmp_dir("targets-full-home");
        std::fs::create_dir_all(games.join("t")).unwrap();
        std::fs::create_dir_all(home.join("t")).unwrap();
        std::fs::write(home.join("t-launcher.sh"), b"x").unwrap();

        let got = removal_targets(&games, Some(&home), "t", "t-launcher.sh");
        assert!(got.contains(&games.join("t").display().to_string()));
        assert!(got.contains(&home.join("t").display().to_string()));
        assert!(got.contains("t-launcher.sh"));

        std::fs::remove_dir_all(&games).unwrap();
        std::fs::remove_dir_all(&home).unwrap();
    }

    // -- games_dir_action (pure decision) ------------------------------------

    #[test]
    fn games_dir_action_symlink_with_dir_target_removes_both() {
        let target = PathBuf::from(r"C:\some\real\dir");
        assert_eq!(
            games_dir_action(true, false, Some(target.clone())),
            GamesDirAction::RemoveSymlinkAndTarget(Some(target))
        );
    }

    #[test]
    fn games_dir_action_symlink_with_no_dir_target_still_removes_the_link() {
        // e.g. a dangling symlink, or one whose target isn't a directory --
        // the bash's `-d "$ttarget"` gate failed, so `target` is None here.
        assert_eq!(games_dir_action(true, false, None), GamesDirAction::RemoveSymlinkAndTarget(None));
    }

    #[test]
    fn games_dir_action_plain_dir_removes_dir() {
        assert_eq!(games_dir_action(false, true, None), GamesDirAction::RemoveDir);
    }

    #[test]
    fn games_dir_action_neither_does_nothing() {
        assert_eq!(games_dir_action(false, false, None), GamesDirAction::Nothing);
    }

    // -- remove_title_fs (plain-dir + home + launcher; no real symlinks) -----

    #[test]
    fn remove_title_fs_removes_plain_games_dir_home_dir_and_launcher() {
        let games = tmp_dir("remove-games");
        let home = tmp_dir("remove-home");
        std::fs::create_dir_all(games.join("t").join("inner")).unwrap();
        std::fs::create_dir_all(home.join("t")).unwrap();
        std::fs::write(home.join("t-launcher.sh"), b"x").unwrap();

        remove_title_fs(&games, Some(&home), "t", "t-launcher.sh");

        assert!(!games.join("t").exists());
        assert!(!home.join("t").exists());
        assert!(!home.join("t-launcher.sh").exists());

        std::fs::remove_dir_all(&games).unwrap();
        std::fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn remove_title_fs_missing_entries_is_a_silent_noop() {
        let games = tmp_dir("remove-missing-games");
        let home = tmp_dir("remove-missing-home");
        // Nothing exists at all -- must not panic (rm -f/-rf semantics).
        remove_title_fs(&games, Some(&home), "nope", "nope-launcher.sh");
        remove_title_fs(&games, None, "nope", "");
        std::fs::remove_dir_all(&games).unwrap();
        std::fs::remove_dir_all(&home).unwrap();
    }

    // -- sanitizers -----------------------------------------------------------

    #[test]
    fn sanitize_project_no_underscore_drops_underscores_and_uppercase() {
        assert_eq!(sanitize_project_no_underscore("Wow_Server-Playerbots"), "wowserver-playerbots");
        assert_eq!(sanitize_project_no_underscore("MU Online v2!"), "muonlinev2");
    }

    #[test]
    fn sanitize_project_with_underscore_keeps_underscores() {
        assert_eq!(sanitize_project_with_underscore("Wow_Server-Playerbots"), "wow_server-playerbots");
        assert_eq!(sanitize_project_with_underscore("MU Online v2!"), "muonlinev2");
    }

    #[test]
    fn basename_extracts_final_component() {
        // Forward slashes on purpose: `std::path` accepts `/` as a separator
        // on BOTH flavours, so this case is real coverage on Windows and on
        // Linux CI alike. The backslash form is Windows-only (below).
        assert_eq!(basename(Path::new("/games/wow-server-playerbots")), "wow-server-playerbots");
        assert_eq!(basename(Path::new("")), "");
    }

    /// WINDOWS-ONLY: on Linux `C:\games\…` is a single component, so
    /// `file_name()` returns the whole string — correct behaviour there, just
    /// not this assertion.
    #[cfg(windows)]
    #[test]
    fn basename_extracts_final_component_from_a_backslash_path() {
        assert_eq!(basename(Path::new(r"C:\games\wow-server-playerbots")), "wow-server-playerbots");
    }

    // -- build-volume regex ---------------------------------------------------

    #[test]
    fn matches_build_volume_literal_build_substring() {
        assert!(matches_build_volume("wow-server-playerbots_build-cache", "wow-server-playerbots"));
        assert!(matches_build_volume("wow-server-playerbots-ac-build-vol", "wow-server-playerbots"));
    }

    #[test]
    fn matches_build_volume_ac_anychar_build_wildcard() {
        assert!(matches_build_volume("wowserverplaybots_ac.build", "wowserverplaybots"));
        assert!(matches_build_volume("wowserverplaybots_acXbuild", "wowserverplaybots"));
        assert!(matches_build_volume("wowserverplaybots_ac_build", "wowserverplaybots"));
    }

    #[test]
    fn matches_build_volume_requires_project_prefix() {
        assert!(!matches_build_volume("other-project-build-cache", "wow-server-playerbots"));
        assert!(!matches_build_volume("build-cache", "wow-server-playerbots"));
    }

    #[test]
    fn matches_build_volume_no_match_without_build_or_ac_pattern() {
        assert!(!matches_build_volume("wow-server-playerbots_data", "wow-server-playerbots"));
        assert!(!matches_build_volume("wow-server-playerbots_ac", "wow-server-playerbots"));
    }

    #[test]
    fn matches_build_volume_never_panics_on_multibyte_wildcard_slot() {
        // The "any one char" slot in `ac.build` landing on a multi-byte char
        // must never panic -- exercised via char-based (not byte-based)
        // indexing in `contains_ac_anychar_build`.
        assert!(matches_build_volume("proj_ac\u{e9}build", "proj"));
        assert!(!matches_build_volume("proj_a", "proj"));
    }

    #[test]
    fn find_build_volume_returns_first_match() {
        let names = vec!["ac-database".to_string(), "wow-server-playerbots_ac-build".to_string(), "wow-server-playerbots_build2".to_string()];
        assert_eq!(find_build_volume(&names, "wow-server-playerbots"), Some("wow-server-playerbots_ac-build"));
        assert_eq!(find_build_volume(&names, "no-such-project"), None);
    }

    // -- client-data volume ---------------------------------------------------

    #[test]
    fn has_client_data_key_line_matches_leading_whitespace_only() {
        assert!(has_client_data_key_line("volumes:\n  ac-client-data:\n"));
        assert!(has_client_data_key_line("ac-client-data:\n"));
        assert!(!has_client_data_key_line("  # ac-client-data:\n"));
        assert!(!has_client_data_key_line("not-ac-client-data:\n"));
        assert!(!has_client_data_key_line(""));
    }

    #[test]
    fn compose_declares_client_data_scans_all_four_filenames() {
        let dir = tmp_dir("client-data-scan");
        std::fs::write(dir.join("compose.yaml"), "volumes:\n  ac-client-data:\n").unwrap();
        assert!(compose_declares_client_data(&dir));
        std::fs::remove_dir_all(&dir).unwrap();

        let dir2 = tmp_dir("client-data-none");
        std::fs::write(dir2.join("docker-compose.yml"), "volumes:\n  ac-database:\n").unwrap();
        assert!(!compose_declares_client_data(&dir2));
        std::fs::remove_dir_all(&dir2).unwrap();
    }

    #[test]
    fn client_data_volume_name_shape() {
        // `/` separators — real coverage on both path flavours (see
        // `basename_extracts_final_component`).
        let dir = Path::new("/games/Wow_Server-Playerbots");
        assert_eq!(client_data_volume_name(dir), "wow_server-playerbots_ac-client-data");
    }

    #[cfg(windows)]
    #[test]
    fn client_data_volume_name_shape_from_a_backslash_path() {
        let dir = Path::new(r"C:\games\Wow_Server-Playerbots");
        assert_eq!(client_data_volume_name(dir), "wow_server-playerbots_ac-client-data");
    }

    // -- compose_server_images ------------------------------------------------

    #[test]
    fn parse_image_line_extracts_first_token() {
        assert_eq!(parse_image_line("    image: acore/ac-wotlk-worldserver:master"), Some("acore/ac-wotlk-worldserver:master".to_string()));
        assert_eq!(parse_image_line("image:mysql:8.4"), Some("mysql:8.4".to_string()));
        assert_eq!(parse_image_line("image:"), None);
        assert_eq!(parse_image_line("image:   "), None);
        assert_eq!(parse_image_line("not an image line"), None);
    }

    #[test]
    fn extract_server_images_substitutes_tag_and_dedupes() {
        let texts = vec![
            Some("services:\n  world:\n    image: acore/worldserver:${DOCKER_IMAGE_TAG:-master}\n".to_string()),
            Some("services:\n  db:\n    image: mysql:8.4\n".to_string()),
            Some("services:\n  world2:\n    image: acore/worldserver:${DOCKER_IMAGE_TAG:-master}\n".to_string()),
            None,
        ];
        let got = extract_server_images(&texts, "v1.2.3");
        assert_eq!(got, vec!["acore/worldserver:v1.2.3".to_string(), "mysql:8.4".to_string()]);
    }

    #[test]
    fn extract_server_images_skips_unresolved_vars() {
        let texts = vec![Some("image: ${SOME_OTHER_VAR}\n".to_string())];
        assert!(extract_server_images(&texts, "master").is_empty());
    }

    #[test]
    fn extract_server_images_plain_dollar_form_also_substitutes() {
        let texts = vec![Some("image: acore/authserver:${DOCKER_IMAGE_TAG}\n".to_string())];
        assert_eq!(extract_server_images(&texts, "master"), vec!["acore/authserver:master".to_string()]);
    }

    #[test]
    fn parse_docker_image_tag_first_match_and_strips_cr() {
        assert_eq!(parse_docker_image_tag("DOCKER_IMAGE_TAG=v1.2.3\r\n"), Some("v1.2.3".to_string()));
        assert_eq!(parse_docker_image_tag("DOCKER_IMAGE_TAG=v1\nDOCKER_IMAGE_TAG=v2\n"), Some("v1".to_string()));
        assert_eq!(parse_docker_image_tag("SOME_OTHER=1\n"), None);
        assert_eq!(parse_docker_image_tag("DOCKER_IMAGE_TAG=\n"), None);
    }

    #[test]
    fn compose_server_images_end_to_end_with_env_tag() {
        let dir = tmp_dir("images-e2e");
        std::fs::write(dir.join(".env"), "DOCKER_IMAGE_TAG=v9\n").unwrap();
        std::fs::write(
            dir.join("docker-compose.yml"),
            "services:\n  world:\n    image: acore/worldserver:${DOCKER_IMAGE_TAG:-master}\n  db:\n    image: mysql:8.4\n",
        )
        .unwrap();
        let got = compose_server_images(&dir);
        assert_eq!(got, vec!["acore/worldserver:v9".to_string(), "mysql:8.4".to_string()]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn compose_server_images_defaults_tag_to_master_without_env() {
        let dir = tmp_dir("images-no-env");
        std::fs::write(dir.join("compose.yaml"), "image: acore/worldserver:${DOCKER_IMAGE_TAG:-master}\n").unwrap();
        let got = compose_server_images(&dir);
        assert_eq!(got, vec!["acore/worldserver:master".to_string()]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

}
