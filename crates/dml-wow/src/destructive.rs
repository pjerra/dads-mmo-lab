//! Native-mode **docker/fs-destructive** primitives (spike:
//! `spike/docker-desktop-native`, Task C4a): `docker-clean`, `module
//! rebuild`, `games remove`. Faithful ports of `cli/src/90-main.sh`'s
//! `docker-clean)` arm (1519-1591), the `module)`'s `rebuild)` case
//! (4967-5008), and the `remove)` arm (1184-1309), plus the shared helpers
//! `_resolve_compose_dir`/`_compose_server_images` (90-main.sh:61-103) and
//! the title catalog `_title_registry`/`_title_row`/`_title_installed`
//! (`cli/src/80-titles.sh:12-40`).
//!
//! ARCHITECTURE, mirroring `dml::modmgr`/`dml::lifecycle`: every REUSABLE,
//! pure-or-nearly-pure primitive (the title registry, the two DIFFERENT
//! project-name sanitizers, the build-volume regex, the client-data-volume
//! detector, the server-image list parser, the games-dir FS-removal
//! decision) lives here so the guards are independently unit-tested without
//! a live docker engine. The actual STREAMED Tauri orchestration
//! (`section_start`/`line`/`section_end`/`done`/`error` sequencing, the real
//! bounded/unbounded subprocess spawns) lives in `lib.rs` next to
//! `wow_world_restart_native_blocking`, which each `_native_blocking`
//! function here follows event-for-event.
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
}

/// `_title_registry` (`80-titles.sh:12-21`), verbatim — six rows, in order.
pub const TITLE_REGISTRY: &[TitleRow] = &[
    TitleRow {
        id: "wow-server-playerbots",
        name: "WoW WotLK (Playerbots)",
        installer: "install-wow-wotlk.sh",
        kind: "games",
        launcher: "wow-playerbots-launcher.sh",
    },
    TitleRow {
        id: "wow-vanilla-server",
        name: "WoW Vanilla",
        installer: "install-wow-vanilla.sh",
        kind: "home",
        launcher: "wow-vanilla-launcher.sh",
    },
    TitleRow {
        id: "wow-tbc-server",
        name: "WoW TBC",
        installer: "install-wow-tbc.sh",
        kind: "home",
        launcher: "wow-tbc-launcher.sh",
    },
    TitleRow {
        id: "maplestory-server",
        name: "MapleStory v83",
        installer: "install-maplestory.sh",
        kind: "home",
        launcher: "maplestory-launcher.sh",
    },
    TitleRow {
        id: "runescape-server",
        name: "RuneScape",
        installer: "install-runescape.sh",
        kind: "home",
        launcher: "runescape-launcher.sh",
    },
    TitleRow {
        id: "muonline-server",
        name: "MU Online",
        installer: "install-muonline.sh",
        kind: "home",
        launcher: "muonline-launcher.sh",
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
        assert_eq!(basename(Path::new(r"C:\games\wow-server-playerbots")), "wow-server-playerbots");
        assert_eq!(basename(Path::new("")), "");
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
