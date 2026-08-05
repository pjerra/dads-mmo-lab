//! One-shot startup resolution of the four `DML_*` variables.
//!
//! WHY THE ENVIRONMENT. `backend::selected()` and the three path readers
//! (`games_dir_from_env`, `ConfigReader::title_dir_from_env`,
//! `find_dml_script`) read the process environment fresh on EVERY call, and
//! native children inherit it (`DmlRunner` only prepends PATH). Writing the
//! resolved values here therefore fixes ~60 native command gates and the
//! bash children at once, without threading a resolver through any of them.
//!
//! WHY ONLY-IF-UNSET. Precedence is `env → launcher.json → auto-detect`, and
//! env must stay highest: the parity, bats and CLI integration suites all
//! inject these variables as override seams.
//!
//! ORDERING. `std::env::set_var` is only sound before other threads exist, so
//! `resolve_and_export()` MUST be the first statement of `run()` — before
//! `tauri::Builder::default()`, whose `.setup()` spawns the interval-backup
//! watcher thread.

use std::path::PathBuf;

/// Whether the inherited `DML_BACKEND` PINNED a backend, captured before we
/// export our own resolved value over the top of that emptiness.
///
/// Without this the Settings dropdown is permanently read-only: we always
/// export `DML_BACKEND`, so a later `std::env::var` can never distinguish
/// "the user pinned it" from "we resolved it", and the UI would report every
/// session as env-locked.
static BACKEND_PINNED_BY_ENV: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

/// True only when the `DML_BACKEND` we inherited named a concrete backend.
/// Defaults to false if `resolve_and_export` never ran.
///
/// `auto` is NOT a pin — see [`backend_env_pins`].
pub fn backend_pinned_by_env() -> bool {
    *BACKEND_PINNED_BY_ENV.get().unwrap_or(&false)
}

/// Whether a `DML_BACKEND` value names a concrete backend, as opposed to
/// asking us to detect one.
///
/// `auto` is an INSTRUCTION, not a choice — the word `launcher.json` documents
/// and the Settings dropdown labels "Detect automatically" — so a user who
/// exports `DML_BACKEND=auto` is asking for detection, not pinning `Wsl`.
/// This is the same rule [`dml_core::backend::resolve`] applies to its own env
/// argument; stated once here so the export and the UI cannot drift from it.
pub fn backend_env_pins(env_value: Option<&str>) -> bool {
    env_value
        .map(str::trim)
        .is_some_and(|v| !v.is_empty() && !v.eq_ignore_ascii_case("auto"))
}

/// Pure: what to write for one variable, or `None` to leave it alone.
pub fn value_to_export(env_value: Option<&str>, resolved: Option<&str>) -> Option<String> {
    if env_value.map(str::trim).is_some_and(|v| !v.is_empty()) {
        return None; // the user set it; never overwrite
    }
    resolved.map(str::to_string)
}

/// The whole `DML_BACKEND` composition in one place production actually calls:
/// what [`dml_core::backend::resolve`] decided, turned into the string
/// [`dml_core::backend::selected`] will read back.
///
/// WHY THIS IS NOT JUST `value_to_export`. `value_to_export`'s rule is "an env
/// value the user set is never overwritten", which is right for the three PATH
/// variables and WRONG for the one value that can mean "work it out for me".
/// `DML_BACKEND=auto` composed the two correct halves into a broken whole:
/// `resolve` answered `Native` on a fresh Docker Desktop PC, the export
/// declined to write it because the env was non-empty, `selected()` then read
/// the surviving `auto`, and `from_override` maps every unrecognised string to
/// `Wsl` — so the launcher drove a distro that does not exist, while Settings
/// reported the dropdown locked by an env var and refused to let the user
/// repair it. Both halves had tests; the composition had none.
///
/// Pinned by `an_auto_env_value_still_means_detect_after_the_export`.
pub fn backend_value_to_export(
    env_value: Option<&str>,
    resolved: dml_core::backend::Backend,
) -> Option<String> {
    let pin = env_value.filter(|v| backend_env_pins(Some(v)));
    value_to_export(pin, Some(backend_env_value(resolved)))
}

/// The conventional native install location, used when neither the
/// environment nor `launcher.json` names one.
pub fn default_games_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(|u| PathBuf::from(u).join("dml-native"))
}

/// The `DML_BACKEND` string that reads back as this backend.
///
/// THIS IS THE WHOLE ARCH OPT-IN. Whatever `resolve_and_export` writes here is
/// what `backend::selected()` parses back when `AppState` builds its runner, so
/// the arms must round-trip through [`from_override`](dml_core::backend::from_override)
/// — collapsing `Arch` onto `"wsl"` would mean a user who opted in via
/// `launcher.json` silently got the bash runner, with no error and nothing to
/// see. That is not hypothetical: it is the exact bug FIX 1 removed, and the
/// final gate then proved it could be reintroduced invisibly (both Rust suites
/// stayed fully green with this arm collapsed). Hence
/// `every_backend_round_trips_through_the_value_we_export`.
///
/// NOT to be conflated with `lib.rs`'s `backend_mode()`, which deliberately
/// answers `"wsl"` for BOTH `Arch` and `Wsl` — that one feeds a frontend union
/// with only two members and is a different question. See the comment there.
///
/// A `match`, so adding a `Backend` variant is a compile error here rather than
/// a variant that quietly exports nothing.
fn backend_env_value(b: dml_core::backend::Backend) -> &'static str {
    match b {
        dml_core::backend::Backend::Native => "native",
        dml_core::backend::Backend::Arch => "arch",
        dml_core::backend::Backend::Wsl => "wsl",
    }
}

/// Whether `backend::detect`'s answer for this machine can actually change
/// depending on what the distro probe says.
///
/// Given `detect`'s table, the probe's answer only matters when Docker is
/// present AND no native server directory exists yet: that is the one
/// combination where `Tri::No` ("this PC provably has no distro") picks Native
/// while `Yes`/`Unknown` keep the WSL default. Without Docker the answer is
/// always `Wsl`, and with a native server already installed it is always
/// `Native` — so skipping the probe in those rows costs nothing and saves a
/// `wsl.exe` spawn before the window is shown.
///
/// Extracted as a pure function so this claim is directly testable without
/// spawning `wsl.exe` or reaching into `resolve_and_export`, and cross-checked
/// against `detect` itself by
/// `distro_probe_matters_agrees_with_detect_itself` — which is what caught the
/// inversion when the default flipped, and caught it again when it flipped
/// back (2026-08-04).
fn distro_probe_matters(native_dir_exists: bool, docker_present: bool) -> bool {
    docker_present && !native_dir_exists
}

/// Resolve the backend and the three paths, then export whatever the user has
/// not already set. Call FIRST in `run()`.
pub fn resolve_and_export() {
    let home = match dml_core::util::dml_home_dir() {
        Some(h) => h,
        None => return, // no USERPROFILE/HOME: nothing to read, nothing to write
    };
    let cfg = dml_core::launcher_config::load(&home);

    // Capture pinned-ness BEFORE any export, or it is unrecoverable.
    //
    // `auto` deliberately does NOT count (see `backend_env_pins`). It used to,
    // and the two halves disagreed about one word: the FILE arm below already
    // reads `auto` as "detect" (`launcher_config_read`'s `source`), while the
    // ENV arm read it as a lock — so `DML_BACKEND=auto` greyed the dropdown out
    // and told the user it was locked by an environment variable, on the exact
    // path where they had asked us to choose for them.
    let env_backend_raw = std::env::var("DML_BACKEND").ok();
    let _ = BACKEND_PINNED_BY_ENV.set(backend_env_pins(env_backend_raw.as_deref()));

    // --- games dir -------------------------------------------------------
    // Env FIRST. It is not merely an override to pass through: the probe
    // below uses this path, so ignoring a user's DML_GAMES_DIR would detect
    // against the wrong directory and could land them on the very "offline
    // while the server runs" bug this module exists to fix.
    //
    // Read ONCE and reused by the export below. `resolve_and_export` is the
    // one production site outside `dml_core::compose::games_dir_override` that
    // may read this variable, because it is the DECIDER: it must see the raw
    // env before anything is exported in order to honour "env outranks
    // launcher.json outranks auto-detect", and it is what makes the variable
    // set for every reader downstream. `games_dir_reader_scan_tests` (below)
    // pins that there are exactly these two, and that this one reads once.
    //
    // The `trim` here is deliberately stricter than the core override's plain
    // emptiness test: a whitespace-only value is a user typo in a settings
    // field, and the question being answered here is "did they pin one?".
    let env_games_dir: Option<String> =
        std::env::var("DML_GAMES_DIR").ok().filter(|s| !s.trim().is_empty());
    let games_dir: Option<PathBuf> = env_games_dir
        .clone()
        .map(PathBuf::from)
        .or_else(|| {
            cfg.games_dir
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .map(PathBuf::from)
        })
        .or_else(default_games_dir);

    // --- probes for auto-detection ---------------------------------------
    // A COMPOSE FILE, not a bare directory. Same weak-evidence class as the
    // catalog's `[[ -d ... ]]` test: the install engine creates the title dir at
    // stage 3 of 8, so bare existence would call a folder holding nothing but a
    // half-done clone a native install. Requiring the generated compose file at
    // least means something was configured. (It does not close the class --
    // generate-compose is stage 5, so a failed BUILD still looks
    // native-installed. That is the right answer here anyway: it IS a native
    // title dir, and `native_title_count` is what decides whether it is
    // playable.)
    let native_dir_exists = games_dir
        .as_ref()
        .map(|g| {
            g.join("wow-server-playerbots").join(dml_wow::composegen::BASE_FILE).is_file()
        })
        .unwrap_or(false);
    // `docker_desktop_program` has NO bare-name fallback, so `Some` means a
    // real Docker Desktop executable was found on disk.
    let docker_present = dml_core::engine::docker_desktop_program().is_some();
    // "Is `dml-arch` registered?" -- the third input to `backend::detect`.
    //
    // Only asked when it can change `detect`'s answer (see
    // `distro_probe_matters`): in every OTHER combination of
    // native_dir_exists/docker_present, `detect` settles on one backend
    // whether the probe says Yes, No or Unknown, so spawning `wsl.exe` for it
    // would just be startup latency for a value nothing reads. This runs
    // before the window is shown, so that matters.
    let distro_usable = if distro_probe_matters(native_dir_exists, docker_present) {
        dml_core::setup::distro_registered(&dml_core::setup::SetupProbeEnv::new(
            dml_core::runner::DISTRO,
            dml_core::runner::USER,
        ))
    } else {
        dml_core::setup::Tri::Unknown
    };

    let backend = dml_core::backend::resolve(
        env_backend_raw.as_deref(),
        cfg.backend.as_deref(),
        native_dir_exists,
        docker_present,
        distro_usable,
    );
    // Round-trips through `from_override` — see `backend_env_value`. (`detect`
    // never returns Arch, so that arm is only ever reached from an explicit env
    // or file value.) The `auto` case is why this goes through
    // `backend_value_to_export` rather than `value_to_export` directly.
    let backend_export = backend_value_to_export(env_backend_raw.as_deref(), backend);

    // --- yq: default to the path the one-click installer downloads to -----
    let yq: Option<String> = cfg
        .yq_bin
        .clone()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            games_dir
                .as_ref()
                .map(|g| g.join("tools").join("yq.exe").to_string_lossy().into_owned())
        });

    // --- script: NO invented default, but a SHIPPED one is not invented ---
    //
    // Precedence stays env > launcher.json, then falls back to the copy of
    // `cli/dml` this build bundles next to the exe (SHIP-LIST 4.1). Without
    // that last step the whole point of bundling the payload was lost for
    // native mode: `find_dml_script` fell back to a bare `dml`, so a packaged
    // launcher still needed a repo checkout to run natively -- exactly the
    // "clone the repo, be named perzi" problem Phase 4 exists to end.
    //
    // Only used when the file is actually THERE, so a dev build without a
    // bundled payload still behaves as before. Consumed by DmlRunner::native()
    // alone; WSL mode never reads DML_SCRIPT, so this cannot disturb it.
    let script: Option<String> = cfg
        .dml_script
        .clone()
        .filter(|s| !s.trim().is_empty())
        .or_else(bundled_cli_script);

    let exports: Vec<(&str, Option<String>)> = vec![
        ("DML_BACKEND", backend_export),
        (
            "DML_GAMES_DIR",
            value_to_export(
                env_games_dir.as_deref(),
                games_dir.as_ref().map(|g| g.to_string_lossy().into_owned()).as_deref(),
            ),
        ),
        ("DML_SCRIPT", value_to_export(std::env::var("DML_SCRIPT").ok().as_deref(), script.as_deref())),
        ("DML_YQ_BIN", value_to_export(std::env::var("DML_YQ_BIN").ok().as_deref(), yq.as_deref())),
    ];

    for (name, value) in exports {
        if let Some(v) = value {
            std::env::set_var(name, v);
        }
    }
}

/// The `cli/dml` this build bundles beside the exe, when it is really there.
///
/// Resolved from the executable's own directory rather than tauri's resource
/// API because this runs BEFORE the app is built (see `resolve_and_export`'s
/// placement) — and on Windows tauri's `resource_dir()` returns exactly that
/// directory anyway, so the two agree.
fn bundled_cli_script() -> Option<String> {
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let script = crate::payload::paths(&exe_dir).cli_script;
    script.is_file().then(|| script.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_only_when_env_is_absent_or_empty() {
        // Env wins: a set value is never overwritten.
        assert_eq!(value_to_export(Some("C:/set-by-user"), Some("C:/resolved")), None);
        // Unset or empty: the resolved value fills in.
        assert_eq!(value_to_export(None, Some("C:/resolved")), Some("C:/resolved".to_string()));
        assert_eq!(value_to_export(Some(""), Some("C:/resolved")), Some("C:/resolved".to_string()));
        assert_eq!(value_to_export(Some("   "), Some("C:/resolved")), Some("C:/resolved".to_string()));
    }

    #[test]
    fn export_nothing_when_there_is_nothing_to_resolve() {
        // No env AND no resolved value: leave it unset so downstream failures
        // stay honest ("not found") instead of pointing at an invented path.
        assert_eq!(value_to_export(None, None), None);
        assert_eq!(value_to_export(Some(""), None), None);
    }

    /// THE assertion that makes the Arch opt-in reachable, and the one the
    /// final gate proved was missing: with `Backend::Arch => "arch"` collapsed
    /// to `"wsl"`, all 202 launcher tests and all 285 dml-core tests stayed
    /// green (mutation log, FIX 1 §1.4). A `launcher.json` saying
    /// `{"backend":"arch"}` would then silently drive the bash runner.
    ///
    /// A ROUND TRIP rather than three literal comparisons: the property that
    /// actually matters is that what we WRITE to `DML_BACKEND` is what
    /// `backend::selected()` READS BACK, and restating the match arms as
    /// literals would pass just as happily if both sides were collapsed
    /// together.
    #[test]
    fn every_backend_round_trips_through_the_value_we_export() {
        use dml_core::backend::{from_override, Backend};

        // `backend_env_value` is a `match`, so a new `Backend` variant is a
        // compile error there — it cannot reach the export unnamed. This list
        // is what makes it reach the ASSERTION; keep them in step.
        let all = [Backend::Native, Backend::Arch, Backend::Wsl];

        for b in all {
            assert_eq!(
                from_override(Some(backend_env_value(b))),
                b,
                "we export {:?} as {:?}, but `backend::selected()` reads that back as {:?} — \
                 a user who asked for {:?} would silently get the other backend",
                b,
                backend_env_value(b),
                from_override(Some(backend_env_value(b))),
                b,
            );
        }

        // The round trip alone is satisfiable by a collapse on BOTH sides at
        // once; distinctness is not. Two backends sharing one env string means
        // one of them is unreachable by name.
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert_ne!(
                    backend_env_value(*a),
                    backend_env_value(*b),
                    "{a:?} and {b:?} export the same DML_BACKEND value, so one of them \
                     can never be selected"
                );
            }
        }
    }

    /// What `backend::selected()` answers AFTER `resolve_and_export` has run
    /// with this environment. The composition of both sites, with the process
    /// environment modelled rather than mutated — `set_var` in a test would
    /// race every other test in this binary.
    ///
    /// `exported.or(env_value)` is exactly what the real process holds
    /// afterwards: our export when we made one, otherwise whatever the user
    /// already had.
    fn selected_after_export(
        env_value: Option<&str>,
        file_value: Option<&str>,
        native_dir_exists: bool,
        docker_present: bool,
        distro_usable: dml_core::setup::Tri,
    ) -> dml_core::backend::Backend {
        let resolved = dml_core::backend::resolve(
            env_value,
            file_value,
            native_dir_exists,
            docker_present,
            distro_usable,
        );
        let exported = backend_value_to_export(env_value, resolved);
        dml_core::backend::from_override(exported.as_deref().or(env_value))
    }

    /// THE COMPOSITION TEST. `backend::resolve` honouring `auto` and the
    /// export refusing to overwrite a non-empty env var are each correct and
    /// each tested; together they discarded the answer. The gate proved it:
    /// deleting `resolve`'s env-`auto` arm reddened exactly ONE dml-core test
    /// and ZERO launcher tests — the signature of an invariant pinned on a
    /// pure function production never composes.
    ///
    /// The concrete user: a fresh PC with Docker Desktop, no distro, and
    /// `DML_BACKEND=auto` exported by hand (the word `launcher.json`
    /// documents and the dropdown labels "Detect automatically"). `resolve`
    /// answered `Native`; `selected()` read the surviving `auto` and, via
    /// `from_override`'s catch-all, drove `Wsl` against a distro that does not
    /// exist — with Settings reporting the dropdown env-locked so it could not
    /// be repaired in the UI.
    #[test]
    fn an_auto_env_value_still_means_detect_after_the_export() {
        use dml_core::backend::Backend;
        use dml_core::setup::Tri;

        // Docker present, no native dir, distro provably absent -> Native.
        assert_eq!(
            selected_after_export(Some("auto"), None, false, true, Tri::No),
            Backend::Native,
            "DML_BACKEND=auto on a Docker-Desktop PC with no distro must DETECT Native, \
             not fall through from_override's catch-all to Wsl"
        );
        // A native server already installed -> Native, same reasoning.
        assert_eq!(
            selected_after_export(Some("auto"), None, true, true, Tri::Yes),
            Backend::Native,
        );
        // And auto must still be able to detect Wsl — this is detection, not a
        // second way of spelling "native".
        assert_eq!(
            selected_after_export(Some("auto"), None, false, false, Tri::Yes),
            Backend::Wsl,
        );
        // Spelling does not matter, exactly as `resolve` treats it.
        assert_eq!(
            selected_after_export(Some("  AUTO "), None, false, true, Tri::No),
            Backend::Native,
        );
        // `auto` in the env must not shadow an explicit FILE choice's
        // detection either: file `auto` + env `auto` is still detection.
        assert_eq!(
            selected_after_export(Some("auto"), Some("auto"), false, true, Tri::No),
            Backend::Native,
        );
    }

    /// The general invariant the case above is one instance of: whatever
    /// `resolve` decided is what `selected()` reads back. Anything else means
    /// the launcher drives a backend nobody chose.
    #[test]
    fn the_backend_resolve_picked_is_the_backend_selected_reads_back() {
        use dml_core::setup::Tri;

        let envs = [None, Some(""), Some("   "), Some("auto"), Some("AUTO"), Some("native"),
                    Some("docker"), Some("wsl"), Some("arch"), Some("natve")];
        let files = [None, Some("auto"), Some("native"), Some("wsl"), Some("arch"), Some("natve")];

        for env_value in envs {
            for file_value in files {
                for native_dir in [true, false] {
                    for docker in [true, false] {
                        for distro in [Tri::Yes, Tri::No, Tri::Unknown] {
                            let resolved = dml_core::backend::resolve(
                                env_value, file_value, native_dir, docker, distro,
                            );
                            assert_eq!(
                                selected_after_export(
                                    env_value, file_value, native_dir, docker, distro,
                                ),
                                resolved,
                                "env={env_value:?} file={file_value:?} dir={native_dir} \
                                 docker={docker} distro={distro:?}: resolve chose {resolved:?} \
                                 but the exported value reads back as something else"
                            );
                        }
                    }
                }
            }
        }
    }

    /// A pinned env value is still never overwritten — the precedence the
    /// parity, bats and CLI-integration suites all depend on as an override
    /// seam. Narrowing `auto` out of "pinned" must not widen into these.
    #[test]
    fn a_pinned_env_backend_is_never_overwritten() {
        use dml_core::backend::Backend;

        assert!(backend_env_pins(Some("wsl")));
        assert!(backend_env_pins(Some("native")));
        assert!(backend_env_pins(Some("arch")));
        assert!(backend_env_pins(Some("natve")), "a typo still PINS — it just resolves to Wsl");

        assert!(!backend_env_pins(None));
        assert!(!backend_env_pins(Some("")));
        assert!(!backend_env_pins(Some("   ")));
        assert!(!backend_env_pins(Some("auto")));
        assert!(!backend_env_pins(Some("  AuTo  ")));

        // The export leaves a pin alone, whatever we resolved.
        assert_eq!(backend_value_to_export(Some("wsl"), Backend::Native), None);
        assert_eq!(backend_value_to_export(Some("native"), Backend::Wsl), None);
        // ...and fills in for every non-pin.
        assert_eq!(
            backend_value_to_export(Some("auto"), Backend::Native),
            Some("native".to_string())
        );
        assert_eq!(backend_value_to_export(None, Backend::Arch), Some("arch".to_string()));
        assert_eq!(backend_value_to_export(Some(""), Backend::Wsl), Some("wsl".to_string()));
    }

    #[test]
    fn distro_probe_matters_only_when_docker_is_here_and_no_native_server_is() {
        // The one row where the probe changes `backend::detect`'s answer:
        // `Tri::No` picks Native (the fresh-PC fix), `Yes`/`Unknown` keep the
        // WSL default. Direction reversed 2026-08-04 together with `detect`
        // itself — see `distro_probe_matters_agrees_with_detect_itself`, which
        // is the assertion that actually holds this honest.
        assert!(distro_probe_matters(false, true));
        assert!(!distro_probe_matters(true, true));
        assert!(!distro_probe_matters(true, false));
        assert!(!distro_probe_matters(false, false));
    }

    #[test]
    fn distro_probe_matters_agrees_with_detect_itself() {
        // Cross-checks the predicate against `backend::detect` directly, so
        // this test is not just re-stating the predicate's own formula: for
        // every combination of (native_dir_exists, docker_present), the probe
        // is worth asking IFF `detect` actually gives a different answer for
        // `Tri::Yes` than for `Tri::No`/`Tri::Unknown`. This is the exact
        // check that would have caught Finding 1 (the skip condition was
        // inverted relative to the new three-row `detect`).
        for native_dir_exists in [true, false] {
            for docker_present in [true, false] {
                let with_yes =
                    dml_core::backend::detect(native_dir_exists, docker_present, dml_core::setup::Tri::Yes);
                let with_no =
                    dml_core::backend::detect(native_dir_exists, docker_present, dml_core::setup::Tri::No);
                let with_unknown = dml_core::backend::detect(
                    native_dir_exists,
                    docker_present,
                    dml_core::setup::Tri::Unknown,
                );
                let probe_actually_changes_the_answer = with_yes != with_no || with_yes != with_unknown;
                assert_eq!(
                    distro_probe_matters(native_dir_exists, docker_present),
                    probe_actually_changes_the_answer,
                    "native_dir_exists={native_dir_exists} docker_present={docker_present}"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// THE RECURRENCE GUARD: one games-dir resolver, not two
// ---------------------------------------------------------------------------

/// Pins the invariant that `DML_GAMES_DIR` is read from the process
/// environment in exactly TWO production places, by scanning the workspace's
/// own Rust source.
///
/// # Why a source scan, and why here
///
/// The bug this guards against is not a wrong value — it is a SECOND resolver
/// that disagrees with the first. `dml_wow::config::ConfigReader::
/// title_dir_from_env` carried its own copy of the lookup whose fallback was
/// the current working directory, and because a missing title dir makes every
/// file-backed read fall through to registry DEFAULTS, the whole Config page
/// answered `ok:true` with numbers that were not the server's (1x XP on a 3x
/// server, 500 bots on a 2000-bot server). No behavioural test catches that
/// class, because with `DML_GAMES_DIR` SET — which is what every parity, bats
/// and CLI-integration suite does, since they use it as their override seam —
/// the two resolvers agree perfectly. The divergence lives entirely in the
/// FALLBACK, on a machine no test harness configures. What IS checkable is the
/// structural fact: how many places read the variable at all.
///
/// It lives in the launcher because the launcher is the only crate that may
/// look at all the others. `dml-core` is the game-agnostic bottom layer and
/// must not know that `dml-wow` or `launcher/src` exist (the settled ruling
/// recorded on `vocab_coverage_tests`), and a path walk reaching upward would
/// red `cargo test -p dml-core` on ubuntu CI. CONSEQUENCE, stated rather than
/// discovered: like `vocab_coverage_tests`, this runs on the WINDOWS CI job
/// only, because the ubuntu job builds the three crates and not the launcher.
/// Do not read a green ubuntu run as coverage for it.
#[cfg(test)]
mod games_dir_reader_scan_tests {
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    /// The variable's name, as data rather than as text in this file.
    const VAR: &str = "DML_GAMES_DIR";

    /// Every production site allowed to read `DML_GAMES_DIR` from the process
    /// environment, with the number of reads each may perform.
    ///
    /// Both entries are load-bearing and neither is a rubber stamp:
    ///
    ///  * `compose.rs` — `games_dir_override()`, THE resolver. Everything that
    ///    wants to know where games live goes through it, directly or through
    ///    `games_dir_from_env` / `title_dir_for_id`.
    ///  * `startup.rs` — `resolve_and_export()`, the DECIDER. It must see the
    ///    raw env before anything is exported in order to honour "env outranks
    ///    `~/.dml/launcher.json` outranks auto-detect", and it is what makes
    ///    the variable set for every reader downstream. It reads ONCE and
    ///    reuses the value for the export decision.
    ///
    /// Asserted as an EXACT map, so this fails on an addition (a third reader)
    /// AND on a removal (an entry that has quietly gone stale). Adding a row
    /// here is a deliberate act: say why, in the same breath.
    const ALLOWED: &[(&str, usize)] = &[
        ("crates/dml-core/src/compose.rs", 1),
        ("launcher/src-tauri/src/startup.rs", 1),
    ];

    /// `<repo>` — `CARGO_MANIFEST_DIR` is `<repo>/launcher/src-tauri`.
    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("launcher/src-tauri always has two ancestors")
            .to_path_buf()
    }

    /// Every `.rs` file under a `src/` tree in the workspace, walked at RUNTIME
    /// rather than listed with `include_str!`. A hardcoded file list is the
    /// obvious way to write this and the wrong one: a second resolver would
    /// most likely arrive in a NEW file, which a fixed list cannot see, and the
    /// test would stay green through exactly the change it exists to catch.
    ///
    /// `src/` only: `crates/*/tests/` sets the variable as a deliberate
    /// override seam, which is the supported use, not a resolver.
    fn production_sources(root: &Path) -> Vec<PathBuf> {
        let mut roots = vec![root.join("launcher").join("src-tauri").join("src")];
        if let Ok(entries) = std::fs::read_dir(root.join("crates")) {
            let mut crate_dirs: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
            crate_dirs.sort();
            for c in crate_dirs {
                let src = c.join("src");
                if src.is_dir() {
                    roots.push(src);
                }
            }
        }
        let mut out = Vec::new();
        for r in &roots {
            walk(r, &mut out);
        }
        out.sort();
        out
    }

    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }

    /// Repo-relative, forward-slashed, so `ALLOWED` reads the same on Windows
    /// and Linux.
    fn rel(root: &Path, p: &Path) -> String {
        p.strip_prefix(root).unwrap_or(p).to_string_lossy().replace('\\', "/")
    }

    /// Reads of `DML_GAMES_DIR` from the process environment in `code`.
    ///
    /// A REAL CALL SHAPE is required — `var("…")` or `var_os("…")` — never a
    /// bare mention of the name. This repo has been bitten TWICE by scans that
    /// read prose as a call site (`feature-keys.test.ts`,
    /// `Test-InstallerNative.ps1`), and the workspace is full of doc comments
    /// and hint strings naming this variable. Comments are stripped by the
    /// caller; requiring the call shape is the second, independent guard.
    fn env_read_count(code: &str) -> usize {
        [format!("var(\"{VAR}\")"), format!("var_os(\"{VAR}\")")]
            .iter()
            .map(|shape| code.matches(shape.as_str()).count())
            .sum()
    }

    /// Bare mentions of the name, call shape or not — the signal that the
    /// read/strip pipeline is still producing text at all.
    fn mention_count(code: &str) -> usize {
        code.matches(VAR).count()
    }

    struct Scan {
        files: usize,
        mentions: usize,
        reads: BTreeMap<String, usize>,
    }

    fn scan() -> Scan {
        let root = repo_root();
        let mut s = Scan { files: 0, mentions: 0, reads: BTreeMap::new() };
        for path in production_sources(&root) {
            let Ok(src) = std::fs::read_to_string(&path) else { continue };
            let code = crate::vocab_coverage_tests::strip_cfg_test(&crate::vocab_coverage_tests::strip_comments(&src));
            s.files += 1;
            s.mentions += mention_count(&code);
            let n = env_read_count(&code);
            if n > 0 {
                s.reads.insert(rel(&root, &path), n);
            }
        }
        s
    }

    // -- the extractor's own tests (known positives and known negatives) -----
    //
    // An extractor that finds nothing passes THE TEST below trivially, so the
    // floors there are not enough on their own.

    #[test]
    fn the_extractor_finds_a_real_call_shape() {
        let src = format!("fn f() {{ std::env::var_os(\"{VAR}\").unwrap(); }}");
        let code = crate::vocab_coverage_tests::strip_cfg_test(&crate::vocab_coverage_tests::strip_comments(&src));
        assert_eq!(env_read_count(&code), 1, "a real call site must be counted: {code}");
    }

    #[test]
    fn the_extractor_ignores_the_name_in_prose_and_in_comments() {
        let src = format!(
            "/// Resolves from {VAR} and falls back.\n\
             // let x = std::env::var(\"{VAR}\");\n\
             /* std::env::var_os(\"{VAR}\") */\n\
             fn f() {{ let hint = \"Set {VAR} to the folder your servers live in\"; }}"
        );
        let code = crate::vocab_coverage_tests::strip_cfg_test(&crate::vocab_coverage_tests::strip_comments(&src));
        assert_eq!(
            env_read_count(&code),
            0,
            "a commented-out call and a hint string are not call sites: {code}"
        );
        // ... but the pipeline still SAW text: the surviving hint string keeps
        // the name, so a stripper that deleted everything cannot hide here.
        assert!(mention_count(&code) >= 1, "the stripper ate the whole file: {code}");
    }

    #[test]
    fn the_extractor_ignores_call_sites_inside_a_test_module() {
        let src = format!(
            "fn prod() {{ std::env::var_os(\"{VAR}\"); }}\n\
             #[cfg(test)]\n\
             mod tests {{\n\
                 fn t() {{ std::env::var(\"{VAR}\"); let s = \"}}\"; }}\n\
             }}\n\
             fn after() {{ std::env::var(\"{VAR}\"); }}"
        );
        let code = crate::vocab_coverage_tests::strip_cfg_test(&crate::vocab_coverage_tests::strip_comments(&src));
        assert_eq!(
            env_read_count(&code),
            2,
            "the test module's read must be dropped and BOTH production reads kept \
             (a truncating stripper would lose `after`): {code}"
        );
    }

    #[test]
    fn the_extractor_cuts_a_cfg_test_use_at_its_semicolon() {
        let src =
            format!("#[cfg(test)]\nuse std::env::var;\nfn prod() {{ std::env::var(\"{VAR}\"); }}");
        let code = crate::vocab_coverage_tests::strip_cfg_test(&crate::vocab_coverage_tests::strip_comments(&src));
        assert_eq!(env_read_count(&code), 1, "an attributed `use` must not eat the file: {code}");
    }

    // -- THE TEST ------------------------------------------------------------

    /// Exactly two production readers of `DML_GAMES_DIR`, in exactly the two
    /// places that are allowed one.
    ///
    /// If this is red because you added a resolver: don't add one. Call
    /// `dml_core::compose::games_dir_override()` (raw override, `None` when
    /// unpinned), `games_dir_from_env()` (override + fallback), or
    /// `title_dir_for_id(id)` (`games_dir_from_env().join(id)`). If you truly
    /// need a third reader, add it to `ALLOWED` with the reason written out —
    /// the point is that it becomes a decision someone made, not a duplication
    /// nobody noticed for a month.
    #[test]
    fn only_one_place_resolves_the_games_dir() {
        let s = scan();

        // NON-VACUITY (a): the walk really found the workspace.
        assert!(
            s.files >= 40,
            "only {} source file(s) scanned — the walk is broken, not the workspace",
            s.files
        );
        // NON-VACUITY (b): the read+strip pipeline really produced text that
        // still mentions the variable. An extractor returning empty strings
        // would otherwise satisfy the equality below by finding nothing.
        assert!(
            s.mentions >= 5,
            "only {} mention(s) of the variable across {} files — the stripper ate the source",
            s.mentions,
            s.files
        );

        let expected: BTreeMap<String, usize> =
            ALLOWED.iter().map(|(f, n)| ((*f).to_string(), *n)).collect();
        assert_eq!(
            s.reads, expected,
            "\nthe set of production sites reading {VAR} changed.\n\
             found:    {:?}\n\
             expected: {:?}\n\
             A second games-dir resolver is how the Config page came to report 1x rates on a \
             3x server (2026-08-04): with the variable SET the two agree, so only the FALLBACK \
             diverges, and no behavioural suite configures that machine.",
            s.reads, expected
        );
    }
}
