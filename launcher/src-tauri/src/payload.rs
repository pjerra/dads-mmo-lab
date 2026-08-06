//! The backend payload the installer carries, and where it lands at runtime.
//!
//! SHIP-LIST Phase 4.1. Until now the only thing that provisioned the
//! `dml-arch` distro was `cli/dev-install.ps1`, which hardcodes
//! `/mnt/c/Users/perzi/dads-mmo-lab` — so the install path was "clone the
//! repo, be called perzi". The MSI/NSIS shipped the GUI and nothing else.
//!
//! `bundle.resources` in `tauri.conf.json` now carries the exact set
//! `dev-install.ps1` installs, and this module is the ONE place that knows
//! the layout it lands in. The setup command (4.2) asks here for its source
//! paths; the first-run screen (4.4) asks here whether the payload arrived at
//! all. Nothing else may re-derive these strings — `tauri_conf_bundles_every_payload_target`
//! below fails the build's test run if the manifest and this list drift.
//!
//! WHY THE MAP FORM, AND WHY DIRECTORIES FOR THE LUA. Tauri's resource
//! resolution (`tauri_utils::resources`) treats the three config shapes
//! differently, and only one of them does what we need:
//!   * a LIST entry keeps the source's own path, rewriting `..` to `_up_` —
//!     `../../cli/dml` would land at `_up_/_up_/cli/dml`. Unusable.
//!   * a MAP entry whose source is a GLOB flattens: every match lands
//!     directly under the target dir by file name. `cli/lua/**` would
//!     collapse `party/` and `gm/` into one directory and silently merge two
//!     namespaces of bridge scripts.
//!   * a MAP entry whose source is a DIRECTORY walks it and PRESERVES the
//!     structure under the target. That is what `cli/lua/party` and
//!     `cli/lua/gm` use, so new bridge scripts are picked up automatically
//!     without the two sets ever mixing.
//! THE CATCH, and why `build.rs` has a `WATCH_DIRS` list: the walk happens
//! only when the build script RUNS, and `tauri_build::build()` asks cargo to
//! watch each file it walked, never the directory. A script that does not exist
//! yet is on no such list, so adding one used to leave cargo thinking the build
//! script was fresh — build.rs never re-ran and the new file never shipped.
//! `build.rs` names the two directories on their own `rerun-if-changed` lines
//! (cargo rescans a directory), and
//! `the_build_script_makes_cargo_watch_every_directory_resource` below fails if
//! a directory-form resource is ever added here without being added there.
//! The six installers are listed one by one rather than globbed: `guides/`
//! also holds `install-wow-wotlk-fedora.sh`, `-ubuntu.sh` and the unbound
//! addon script, none of which the title registry can run. Enumerating them
//! also makes a renamed file a hard bundler error instead of a silent
//! omission a stranger discovers halfway through an install.

use std::path::{Path, PathBuf};

use dml_core::setup::Tri;
use serde::Serialize;

/// The `dml` CLI script → `/usr/local/bin/dml` in the distro.
pub const CLI_SCRIPT: &str = "cli/dml";
/// Eluna party bridge scripts → `/usr/local/share/dml/lua/party/`.
pub const LUA_PARTY_DIR: &str = "cli/lua/party";
/// Eluna GM bridge scripts → `/usr/local/share/dml/lua/gm/`.
pub const LUA_GM_DIR: &str = "cli/lua/gm";
/// Title installers → `/usr/local/share/dml/installers/` (`_installers_dir`
/// in `cli/src/80-titles.sh`).
pub const INSTALLERS_DIR: &str = "installers";

/// Exactly the six scripts `cli/src/80-titles.sh`'s `_title_registry` names,
/// which is exactly the six `cli/dev-install.ps1` installs.
pub const INSTALLER_SCRIPTS: [&str; 6] = [
    "install-wow-wotlk.sh",
    "install-wow-vanilla.sh",
    "install-wow-tbc.sh",
    "install-maplestory.sh",
    "install-runescape.sh",
    "install-muonline.sh",
];

/// Where each piece of the payload lives for a given resource dir. The setup
/// command's source list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayloadPaths {
    pub cli_script: PathBuf,
    pub lua_party_dir: PathBuf,
    pub lua_gm_dir: PathBuf,
    /// The six installers, in registry order.
    pub installers: Vec<PathBuf>,
}

/// Whether the bundled payload actually arrived.
///
/// Tri-state for the same reason the probe chain is: `resource_dir()` failing
/// is not "the payload is missing", it is "we do not know where to look", and
/// a setup screen must not offer to reinstall over that.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PayloadStatus {
    /// `Yes` = complete. `No` = at least one entry missing. `Unknown` = the
    /// resource dir could not be resolved at all.
    pub present: Tri,
    /// The resolved resource dir, for the diagnostics line.
    pub dir: Option<String>,
    /// Resource-relative paths that are missing. Empty unless `present` is `No`.
    pub missing: Vec<String>,
}

/// Join `rel` (always written with forward slashes, which behave on both
/// flavours) onto `root` as real path components.
fn under(root: &Path, rel: &str) -> PathBuf {
    let mut p = root.to_path_buf();
    for seg in rel.split('/') {
        p.push(seg);
    }
    p
}

/// Join the payload layout onto a resource dir. Pure.
pub fn paths(resource_dir: &Path) -> PayloadPaths {
    PayloadPaths {
        cli_script: under(resource_dir, CLI_SCRIPT),
        lua_party_dir: under(resource_dir, LUA_PARTY_DIR),
        lua_gm_dir: under(resource_dir, LUA_GM_DIR),
        installers: INSTALLER_SCRIPTS
            .iter()
            .map(|s| under(resource_dir, &format!("{INSTALLERS_DIR}/{s}")))
            .collect(),
    }
}

/// Does `dir` exist AND hold at least one `.lua`?
///
/// `is_dir()` alone is not enough: an empty bridge directory is exactly what a
/// flattening glob or a half-finished copy leaves behind, and it would let
/// `bridge-setup` "succeed" while deploying nothing.
fn lua_dir_populated(dir: &Path) -> bool {
    match std::fs::read_dir(dir) {
        Ok(entries) => entries.flatten().any(|e| {
            e.path().extension().and_then(|x| x.to_str()).is_some_and(|x| x.eq_ignore_ascii_case("lua"))
        }),
        Err(_) => false,
    }
}

/// Check the payload on disk under `resource_dir`.
pub fn resolve(resource_dir: &Path) -> PayloadStatus {
    let p = paths(resource_dir);
    let mut missing: Vec<String> = Vec::new();

    if !p.cli_script.is_file() {
        missing.push(CLI_SCRIPT.to_string());
    }
    if !lua_dir_populated(&p.lua_party_dir) {
        missing.push(LUA_PARTY_DIR.to_string());
    }
    if !lua_dir_populated(&p.lua_gm_dir) {
        missing.push(LUA_GM_DIR.to_string());
    }
    for (script, path) in INSTALLER_SCRIPTS.iter().zip(p.installers.iter()) {
        if !path.is_file() {
            missing.push(format!("{INSTALLERS_DIR}/{script}"));
        }
    }

    PayloadStatus {
        present: if missing.is_empty() { Tri::Yes } else { Tri::No },
        dir: Some(resource_dir.to_string_lossy().into_owned()),
        missing,
    }
}

/// [`resolve`], but for a resource dir Tauri may have failed to give us.
pub fn resolve_opt(resource_dir: Option<&Path>) -> PayloadStatus {
    match resource_dir {
        Some(d) => resolve(d),
        // Could not tell — never "missing".
        None => PayloadStatus { present: Tri::Unknown, dir: None, missing: Vec::new() },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir()
            .join(format!("dml-payload-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Build a complete fake resource dir — the shape `bundle.resources` is
    /// declared to produce.
    fn complete_payload(root: &Path) {
        std::fs::create_dir_all(root.join("cli")).unwrap();
        std::fs::write(root.join(CLI_SCRIPT), "#!/usr/bin/env bash\n").unwrap();
        std::fs::create_dir_all(root.join(LUA_PARTY_DIR)).unwrap();
        std::fs::write(root.join(LUA_PARTY_DIR).join("dml_addclass.lua"), "-- x\n").unwrap();
        std::fs::create_dir_all(root.join(LUA_GM_DIR)).unwrap();
        std::fs::write(root.join(LUA_GM_DIR).join("dml_gm.lua"), "-- x\n").unwrap();
        std::fs::create_dir_all(root.join(INSTALLERS_DIR)).unwrap();
        for s in INSTALLER_SCRIPTS {
            std::fs::write(root.join(INSTALLERS_DIR).join(s), "#!/usr/bin/env bash\n").unwrap();
        }
    }

    // -- paths ---------------------------------------------------------------

    #[test]
    fn paths_join_the_layout_onto_the_resource_dir() {
        // Forward slashes work on both flavours (CLAUDE.md portability rule).
        let root = Path::new("/some/resource/dir");
        let got = paths(root);
        assert_eq!(got.cli_script, root.join("cli").join("dml"));
        assert_eq!(got.lua_party_dir, root.join("cli").join("lua").join("party"));
        assert_eq!(got.lua_gm_dir, root.join("cli").join("lua").join("gm"));
        assert_eq!(got.installers.len(), 6);
        assert_eq!(
            got.installers[0],
            root.join("installers").join("install-wow-wotlk.sh")
        );
    }

    // -- resolve -------------------------------------------------------------

    #[test]
    fn resolve_reports_a_complete_payload() {
        let d = tmp_dir("complete");
        complete_payload(&d);
        let got = resolve(&d);
        assert_eq!(got.present, Tri::Yes, "missing: {:?}", got.missing);
        assert!(got.missing.is_empty());
        assert_eq!(got.dir.as_deref(), Some(d.to_string_lossy().as_ref()));
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn resolve_on_an_empty_dir_lists_every_missing_entry() {
        // The exact "the installer shipped the GUI and nothing else" case.
        let d = tmp_dir("empty");
        let got = resolve(&d);
        assert_eq!(got.present, Tri::No);
        assert!(got.missing.contains(&CLI_SCRIPT.to_string()), "{:?}", got.missing);
        assert!(got.missing.contains(&LUA_PARTY_DIR.to_string()), "{:?}", got.missing);
        assert!(got.missing.contains(&LUA_GM_DIR.to_string()), "{:?}", got.missing);
        assert!(
            got.missing.contains(&format!("{INSTALLERS_DIR}/install-wow-wotlk.sh")),
            "{:?}",
            got.missing
        );
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn resolve_names_the_one_missing_installer() {
        let d = tmp_dir("one-missing");
        complete_payload(&d);
        std::fs::remove_file(d.join(INSTALLERS_DIR).join("install-runescape.sh")).unwrap();
        let got = resolve(&d);
        assert_eq!(got.present, Tri::No);
        assert_eq!(got.missing, vec![format!("{INSTALLERS_DIR}/install-runescape.sh")]);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn resolve_treats_an_empty_lua_dir_as_missing() {
        // A directory that exists but holds no .lua is exactly what a
        // flattening glob or a botched copy leaves behind. `is_dir()` alone
        // would call that fine and the bridges would silently never deploy.
        let d = tmp_dir("empty-lua");
        complete_payload(&d);
        std::fs::remove_file(d.join(LUA_GM_DIR).join("dml_gm.lua")).unwrap();
        let got = resolve(&d);
        assert_eq!(got.present, Tri::No);
        assert_eq!(got.missing, vec![LUA_GM_DIR.to_string()]);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn resolve_opt_none_is_unknown_never_missing() {
        let got = resolve_opt(None);
        assert_eq!(got.present, Tri::Unknown);
        assert!(got.missing.is_empty(), "an unresolved dir must not accuse the payload");
    }

    // -- the manifest itself -------------------------------------------------

    #[test]
    fn tauri_conf_bundles_every_payload_target() {
        // The A-lane guard: this is what fails when `bundle.resources` is
        // absent, incomplete, or written in the LIST form (which would rewrite
        // `..` to `_up_` and never land where `paths()` looks).
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("tauri.conf.json parses");
        let res = conf["bundle"]["resources"]
            .as_object()
            .expect("bundle.resources must be a source -> target MAP, not a list");
        let targets: Vec<String> = res
            .values()
            .map(|v| v.as_str().expect("every target is a string").to_string())
            .collect();

        for want in [CLI_SCRIPT, LUA_PARTY_DIR, LUA_GM_DIR] {
            assert!(targets.contains(&want.to_string()), "missing target {want}: {targets:?}");
        }
        for s in INSTALLER_SCRIPTS {
            let want = format!("{INSTALLERS_DIR}/{s}");
            assert!(targets.contains(&want), "missing target {want}: {targets:?}");
        }
    }

    #[test]
    fn tauri_conf_resource_sources_all_exist_in_the_repo() {
        // A typo'd source is a bundler error at `tauri build` time -- late,
        // and only on the release machine. Catch it here instead. Sources are
        // relative to the tauri.conf.json directory, i.e. CARGO_MANIFEST_DIR.
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let res = conf["bundle"]["resources"].as_object().expect("bundle.resources map");
        for src in res.keys() {
            let p = manifest.join(src);
            assert!(p.exists(), "bundle.resources source does not exist: {src} ({})", p.display());
        }
    }

    #[test]
    fn the_real_build_lays_the_payload_out_where_paths_looks_for_it() {
        // End-to-end for lane A, with no GUI and no bundler run.
        //
        // `tauri-build` copies `bundle.resources` into the cargo target dir on
        // every build, and on Windows `resource_dir()` is ALWAYS the exe's own
        // directory -- in `tauri dev` that is `target/debug`, in an installed
        // build it is the install dir. So the dev-run resource dir is exactly
        // `target/debug`, and this test binary lives one level below it in
        // `target/debug/deps/`. Asserting `resolve()` is happy there proves the
        // manifest really produced the layout, not just that it parses.
        //
        // Not vacuous: `cargo test` re-runs build.rs before this binary exists,
        // so the directory is always freshly populated -- and before
        // `bundle.resources` was added, `target/debug` had no `cli/` at all.
        let exe = std::env::current_exe().expect("test exe path");
        let target_dir = exe
            .parent()
            .and_then(Path::parent)
            .expect("target/debug/deps/<exe> has two parents");
        let got = resolve(target_dir);
        assert_eq!(
            got.present,
            Tri::Yes,
            "bundle.resources did not produce the expected layout in {}: missing {:?}",
            target_dir.display(),
            got.missing
        );

        // And the copy must be byte-faithful: bash inside WSL chokes on CRLF,
        // so a copy step that "helpfully" translated line endings would ship a
        // CLI that cannot run at all.
        let cli = std::fs::read(paths(target_dir).cli_script).expect("bundled cli/dml readable");
        assert!(
            !cli.windows(2).any(|w| w == b"\r\n"),
            "bundled cli/dml has CRLF line endings; bash in the distro will reject it"
        );
    }

    /// Compare paths the way both platforms write them: the build script's
    /// directives come back with the platform separator (`..\..\cli\lua\gm` on
    /// Windows), the manifest is written with forward slashes.
    fn norm(p: &str) -> String {
        p.replace('\\', "/").trim_end_matches('/').to_string()
    }

    /// Every `cargo:rerun-if-changed=` path the build script really printed on
    /// the build that produced THIS test binary. `OUT_DIR` is
    /// `target/<profile>/build/<pkg>-<hash>/out`, and cargo captures the build
    /// script's stdout as `output` next to it — so this reads the actual
    /// directives cargo was given, not a re-implementation of them.
    fn build_script_rerun_paths() -> Vec<String> {
        let out_dir = Path::new(env!("OUT_DIR"));
        let output = out_dir.parent().expect("OUT_DIR has a parent").join("output");
        let text = std::fs::read_to_string(&output)
            .unwrap_or_else(|e| panic!("cannot read the build script output at {}: {e}", output.display()));
        let paths: Vec<String> = text
            .lines()
            .filter_map(|l| {
                l.strip_prefix("cargo:rerun-if-changed=")
                    .or_else(|| l.strip_prefix("cargo::rerun-if-changed="))
            })
            .map(norm)
            .collect();
        // Non-vacuity: an empty or unparsed file would satisfy every "must
        // contain" loop below by never running it.
        assert!(
            !paths.is_empty(),
            "no rerun-if-changed directives parsed out of {}",
            output.display()
        );
        paths
    }

    #[test]
    fn the_build_script_makes_cargo_watch_every_directory_resource() {
        // The bug this pins (verified against the real `output` file before the
        // fix): `tauri_build::build()` prints `cargo:rerun-if-changed` for each
        // resource it WALKED — `..\..\cli\lua\party\dml_login.lua` and friends —
        // and never for the directory, because tauri-utils' resource iterator
        // skips directories. A bridge script that does not exist yet is on no
        // such list, so adding one leaves cargo believing the build script is
        // fresh: build.rs does not re-run, `target/<profile>/cli/lua/party`
        // keeps the old set, and `provision::plan` — which enumerates the
        // scripts from disk at runtime precisely so new ones need no code
        // change — provisions the stale set with no error anywhere.
        //
        // Cargo DOES rescan a directory named on a rerun-if-changed line, so
        // the directories themselves must be on it.
        let watched = build_script_rerun_paths();

        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let res = conf["bundle"]["resources"].as_object().expect("bundle.resources map");
        // A source tauri walks (rather than copying as one file) is exactly one
        // that `is_dir()` — the same test tauri-utils applies.
        let dirs: Vec<String> =
            res.keys().filter(|src| manifest.join(src).is_dir()).map(|s| norm(s)).collect();
        assert!(
            dirs.len() >= 2,
            "expected the two bridge-script directories among the resource sources: {dirs:?}"
        );

        for d in dirs {
            assert!(
                watched.iter().any(|w| *w == d),
                "the build script never told cargo to watch the directory {d}, so a NEW file in it \
                 will not trigger a rebuild and will not be bundled. Watched: {watched:?}"
            );
        }
    }

    #[test]
    fn the_bundled_resource_dirs_match_the_repo() {
        // P14's OTHER half, and the reason build.rs's rerun-if-changed lines are
        // not the whole fix. Cargo re-running the build script is not the same
        // as the copy being right: `tauri_build::build()` only ever COPIES, so a
        // bridge script deleted from `cli/lua/party` stays behind in
        // `target/<profile>/cli/lua/party` forever. `provision::plan` enumerates
        // that directory at RUNTIME (precisely so a NEW script needs no code
        // change), which means a dev build would keep installing a script that
        // no longer exists in the repo, with no error anywhere.
        //
        // Both directions are asserted: an extra file is the deletion half, a
        // missing one is the addition half failing on the REAL build output
        // rather than only in the directives build.rs printed.
        let exe = std::env::current_exe().expect("test exe path");
        let target_dir = exe
            .parent()
            .and_then(Path::parent)
            .expect("target/<profile>/deps/<exe> has two parents");
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let res = conf["bundle"]["resources"].as_object().expect("bundle.resources map");

        let names = |dir: &Path| -> std::collections::BTreeSet<String> {
            std::fs::read_dir(dir)
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
                .filter_map(Result::ok)
                .filter(|e| e.path().is_file())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect()
        };

        let mut checked = 0usize;
        for (src, dest) in res {
            let src_dir = manifest.join(src);
            if !src_dir.is_dir() {
                continue; // single-file resources are not walked
            }
            let dest_dir = target_dir.join(dest.as_str().expect("every target is a string"));
            assert!(
                dest_dir.is_dir(),
                "the build did not lay {src} down at {}",
                dest_dir.display()
            );
            let want = names(&src_dir);
            let got = names(&dest_dir);
            let stale: Vec<&String> = got.difference(&want).collect();
            assert!(
                stale.is_empty(),
                "{} holds {stale:?}, which {src} no longer contains: tauri-build copies resources \
                 and never prunes them, and provision::plan enumerates that directory at runtime, \
                 so a dev build would install a script deleted from the repo. Remedy: delete {} \
                 (or run `cargo clean -p launcher`).",
                dest_dir.display(),
                target_dir.join("cli").display()
            );
            let absent: Vec<&String> = want.difference(&got).collect();
            assert!(
                absent.is_empty(),
                "{} is missing {absent:?} from {src}: the build script did not re-run for a NEW \
                 file, which is the bug build.rs's rerun-if-changed lines exist to prevent.",
                dest_dir.display()
            );
            checked += 1;
        }
        // Non-vacuity: a manifest whose directory resources stopped being
        // directories would skip every assertion above.
        assert!(checked >= 2, "expected the two bridge-script directories, checked {checked}");
    }

    #[test]
    fn installer_list_matches_the_cli_title_registry() {
        // Drift guard: `_title_registry` in cli/src/80-titles.sh is the list
        // `games install` actually looks up by name. Shipping a different six
        // means a title whose installer is simply not on the machine.
        // Match on the ROW SHAPE, not on what the row happens to end with: the
        // registry grew a sixth `family` column on 2026-08-06 and an
        // `ends_with("-launcher.sh")` filter silently parsed zero rows, which
        // reads as "the bundle drifted" rather than "the guard stopped looking".
        let registry = include_str!("../../../cli/src/80-titles.sh");
        let mut from_registry: Vec<&str> = registry
            .lines()
            .map(|l| l.trim_end_matches('\r'))
            .filter(|l| !l.trim_start().starts_with('#'))
            .map(|l| l.split('|').collect::<Vec<_>>())
            .filter(|f| f.len() >= 5)
            .filter_map(|f| f.get(2).copied())
            .filter(|s| s.starts_with("install-") && s.ends_with(".sh"))
            .collect();
        from_registry.sort_unstable();
        assert_eq!(from_registry.len(), 6, "parsed registry rows: {from_registry:?}");

        let mut bundled: Vec<&str> = INSTALLER_SCRIPTS.to_vec();
        bundled.sort_unstable();
        assert_eq!(bundled, from_registry);
    }
}
