//! Generic `docker compose` lifecycle primitives — title/compose-dir
//! resolution, the `games status` running-count helper, the per-mode argv
//! sequence, and the port-conflict bind probe. Moved out of the launcher's
//! `dml::lifecycle` (cargo-workspace refactor, Task 6); everything WoW/
//! playerbots-specific (the flush-heal guard, the hardcoded game-port
//! registry, `check_port_conflicts`, the automatic-backup step list) stayed
//! behind in `dml::lifecycle`, which re-exports these names so every existing
//! caller keeps compiling unchanged.

use std::path::{Path, PathBuf};

/// The pure decision behind [`games_dir_from_env`].
///
/// Split out so both branches are testable on both platforms without
/// `std::env::set_var`, which mutates process-global state every other test in
/// the binary shares — and cargo runs them in parallel.
///
/// Empty is NOT a value: an empty `DML_GAMES_DIR` falls through to the home
/// default rather than resolving to nothing. Treating empty as set is the
/// `${VAR:-default}` trap this repo hit on 2026-07-29, where a test that set a
/// stub's value empty to mean "printed nothing" silently got the default back
/// and proved nothing.
pub fn games_dir_from(env_value: Option<std::ffi::OsString>, home: Option<std::ffi::OsString>) -> PathBuf {
    if let Some(dir) = env_value.filter(|s| !s.is_empty()) {
        return PathBuf::from(dir);
    }
    // Off Windows nothing exports DML_GAMES_DIR for a bare CLI invocation, so
    // this fallback IS the answer, and a cwd-relative "." would resolve the
    // server to wherever the process happened to start. bash's own resolution
    // is `${DML_GAMES_DIR:-$HOME/games}` (`cli/src/00-head.sh`), so `$HOME/
    // games` is the one answer the two CLIs can share.
    if let Some(home) = home.filter(|s| !s.is_empty()) {
        return PathBuf::from(home).join("games");
    }
    PathBuf::from(".")
}

/// Whether the `HOME` fallback applies on this platform.
///
/// Off Windows, `HOME` is the user's own variable and is exactly what we
/// want. ON Windows it must be ignored: **Git for Windows sets `HOME`**, and
/// this repo's own documented workflow runs Git Bash (it drives `wslpath`
/// translation and the bats suite) — so honouring it would silently resolve
/// the games directory to `C:\Users\<name>\games` for any invocation that
/// does not go through the launcher (before `resolve_and_export()` has run,
/// from a shell, or from a future entry point). That is a THIRD location,
/// distinct from both the old `.` fallback and the native default
/// `%USERPROFILE%\dml-native`. On Windows the launcher exports
/// `DML_GAMES_DIR` before anything else runs, so there is no gap the
/// fallback needs to cover there — the unset answer stays byte-identical to
/// what it always was (`.`). `is_windows` is a plain argument (rather
/// than an inline `cfg!(windows)`) so both branches are testable on either
/// build platform.
pub fn home_fallback(is_windows: bool, home: Option<std::ffi::OsString>) -> Option<std::ffi::OsString> {
    if is_windows {
        None
    } else {
        home
    }
}

/// THE ONE READ of `DML_GAMES_DIR` from the process environment.
///
/// `Some` only when the variable is set AND non-empty (the `${VAR-default}` vs
/// `${VAR:-default}` distinction this repo was bitten by on 2026-07-29); `None`
/// means "the user pinned nothing", which each caller answers in its own way:
///
///   * [`games_dir_from_env`] falls back (home, then `.`) — reads may miss;
///   * `dml_wow::install_native::games_dir_for_install` REFUSES — a command
///     that clones gigabytes must never guess;
///   * the launcher's `startup::resolve_and_export` moves on to
///     `~/.dml/launcher.json` and then to auto-detection, and exports the
///     answer so every child process sees a pinned value.
///
/// It exists as a named function, rather than three `var_os` calls that happen
/// to agree, because they DIDN'T agree: `ConfigReader::title_dir_from_env`
/// carried a second copy of this resolution whose fallback was the CURRENT
/// WORKING DIRECTORY, so a bare CLI invocation — nothing exports
/// `DML_GAMES_DIR` for those, and a Windows-side value does not cross
/// `wsl.exe` — had every file-backed read answer `ok:true` off a title dir
/// that does not exist. The Config page showed 1x rates on a server running
/// 3x, with no error to notice (live differential smoke, 2026-08-04, on the
/// arch-backend sibling branch). A test pins the count of production readers;
/// see `startup.rs`'s `games_dir_reader_scan_tests`.
pub fn games_dir_override() -> Option<std::ffi::OsString> {
    std::env::var_os("DML_GAMES_DIR").filter(|s| !s.is_empty())
}

/// `GAMES_DIR` base -- the resolution `dml_wow::config::ConfigReader::
/// title_dir_from_env()` now defers to, generalized to an arbitrary title `id`
/// rather than hardcoding `wow-server-playerbots` (mirrors
/// `_games_resolve_or_fail`'s `dir="$GAMES_DIR/$gid"`, which works for any
/// installed title).
pub fn games_dir_from_env() -> PathBuf {
    games_dir_from(games_dir_override(), home_fallback(cfg!(windows), std::env::var_os("HOME")))
}

/// `$GAMES_DIR/$gid` (`90-main.sh:174`).
pub fn title_dir_for_id(id: &str) -> PathBuf {
    games_dir_from_env().join(id)
}

/// The four canonical compose filenames, in the fixed scan order both
/// `_has_compose` (`90-main.sh:9-15`) and `_compose_running`
/// (`90-main.sh:17-24`) use.
const COMPOSE_FILE_CANDIDATES: [&str; 4] =
    ["docker-compose.yml", "docker-compose.yaml", "compose.yml", "compose.yaml"];

fn has_compose(dir: &Path) -> bool {
    COMPOSE_FILE_CANDIDATES.iter().any(|name| dir.join(name).is_file())
}

/// `_resolve_compose_dir` (`90-main.sh:61-71`): the title dir itself if it
/// carries a compose file, else its first subdir that does. `None` when
/// neither exists (title installed via `install.sh` only, or truly missing
/// its compose file) -- the caller's `NO_COMPOSE` gate.
pub fn resolve_compose_dir(title_dir: &Path) -> Option<PathBuf> {
    if has_compose(title_dir) {
        return Some(title_dir.to_path_buf());
    }
    let entries = std::fs::read_dir(title_dir).ok()?;
    entries.flatten().map(|e| e.path()).find(|p| p.is_dir() && has_compose(p))
}

// ---------------------------------------------------------------------------
// `games status` (Part 5a) -- `_compose_running` (`90-main.sh:17-24`), the
// docker-ps-derived running/stopped classification the `games status` arm
// feeds into its `state` field (`90-main.sh:1074-1091`).
// ---------------------------------------------------------------------------

/// The first compose filename present in `dir`, in `_compose_running`'s
/// exact scan order (SAME order `has_compose` already used to establish
/// `dir` carries ONE of these four). `None` only if `dir` somehow lost its
/// compose file between `resolve_compose_dir` and this call (a live
/// TOCTOU race, not a real code path) -- the caller then reports 0 running
/// containers without ever invoking docker, matching `_compose_running`'s
/// own `[[ -z "$compose_file" ]] && echo 0` short-circuit.
pub fn compose_file_name(dir: &Path) -> Option<&'static str> {
    COMPOSE_FILE_CANDIDATES.into_iter().find(|name| dir.join(name).is_file())
}

/// Parse `docker compose -f <file> ps --status running -q` stdout into a
/// running-container count — a port of `_compose_running`'s tail pipeline
/// (`| wc -l`, `90-main.sh:23`): the `-q` flag prints one container ID per
/// running container, one per line, so every non-empty (non-whitespace-only)
/// line counts as one.
pub fn count_running_ids(ps_out: &str) -> usize {
    ps_out.lines().filter(|l| !l.trim().is_empty()).count()
}

/// Best-effort "is something already listening on this port" probe: attempt
/// to bind the wildcard address, exactly the address `docker-proxy` would
/// need for a published container port. A bind failure (any reason --
/// already bound, permission, whatever) reads as "in use"; this is
/// deliberately conservative (warn-only caller, never a hard gate).
pub fn port_listening(port: u16) -> bool {
    std::net::TcpListener::bind(("0.0.0.0", port)).is_err()
}

/// The same question, answered honestly enough to REFUSE on.
///
/// [`port_listening`] cannot be used for that and says so in its own doc: it
/// reads ANY bind failure as "in use", which is the right conservative bias for
/// a warning and completely wrong for a gate. A `PermissionDenied`, or a port
/// inside one of the ranges Hyper-V/WSL reserves on Windows, would become a hard
/// refusal to start a server that would in fact have started.
///
/// So only `AddrInUse` is a `Yes`. A clean bind is a `No`. Everything else is
/// `Unknown`, and callers must treat that as evidence of NOTHING — the standing
/// tri-state rule, and the one that keeps this from blocking a start it should
/// not.
pub fn port_probe(port: u16) -> crate::setup::Tri {
    use crate::setup::Tri;
    match std::net::TcpListener::bind(("0.0.0.0", port)) {
        Ok(_) => Tri::No,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => Tri::Yes,
        Err(_) => Tri::Unknown,
    }
}

// ---------------------------------------------------------------------------
// Compose command sequencing -- `_games_start_impl`'s mode branch
// (`90-main.sh:225-236`, the bash arm's ELSE/no-`dml-start.sh` path) + the
// `stop)` arm's single `down` (`90-main.sh:1116`). Pure argv builders so the
// exact sequence per mode is independently unit-testable.
// ---------------------------------------------------------------------------

pub fn compose_up_argv() -> Vec<&'static str> {
    vec!["compose", "up", "-d"]
}

pub fn compose_down_argv() -> Vec<&'static str> {
    vec!["compose", "down", "-t", "180"]
}

/// `true` when `argv` is a `down` invocation, `false` for `up` -- argv[0] is
/// always `"compose"` (both builders above start with it), so the token that
/// actually distinguishes them is argv[1]. Used to pick the right progress
/// line + timeout per step of the mode sequence.
pub fn is_compose_down(argv: &[&str]) -> bool {
    argv.get(1) == Some(&"down")
}

/// The ordered sequence of `docker compose` argv's a `games` mode runs --
/// `start` = `[up]`; `restart` = `[down, up]` (mirrors the bash's own
/// `mode=="restart" && down; up` sequencing, stopping at the first
/// failure); `stop` = `[down]`. An unrecognized mode yields an empty
/// sequence (the caller never invokes this with anything else).
pub fn compose_sequence_for_mode(mode: &str) -> Vec<Vec<&'static str>> {
    match mode {
        "start" => vec![compose_up_argv()],
        "restart" => vec![compose_down_argv(), compose_up_argv()],
        "stop" => vec![compose_down_argv()],
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    fn os(s: &str) -> Option<OsString> {
        Some(OsString::from(s))
    }

    /// The override seam every parity/bats/integration suite injects.
    #[test]
    fn the_env_var_wins_over_everything() {
        // Forward slashes work on BOTH platforms; a backslash literal would be
        // Windows-only (test-portability rule).
        assert_eq!(
            games_dir_from(os("/tmp/dml-games-test"), os("/home/dml")),
            PathBuf::from("/tmp/dml-games-test")
        );
    }

    /// Empty is not a value. `:-`-style "empty means unset" has bitten this
    /// repo before (the tailscale stub, 2026-07-29), so pin the direction.
    #[test]
    fn an_empty_env_var_falls_through_rather_than_resolving_to_nothing() {
        assert_eq!(games_dir_from(os(""), os("/home/dml")), PathBuf::from("/home/dml/games"));
    }

    /// For a bare CLI invocation nothing exports DML_GAMES_DIR, so the
    /// fallback IS the answer. `.` would put a server wherever the process
    /// happened to start — and disagree with bash's `${DML_GAMES_DIR:-$HOME/
    /// games}` (`cli/src/00-head.sh:9`).
    #[test]
    fn the_fallback_is_the_home_games_dir_not_the_cwd() {
        let got = games_dir_from(None, os("/home/dml"));
        assert_eq!(got, PathBuf::from("/home/dml/games"));
        assert_ne!(got, PathBuf::from("."), "a cwd-relative default is the bug this fixes");
    }

    /// No env var and no home is the one case with nothing to go on. `.` is
    /// the honest answer there — inventing a path would be worse.
    #[test]
    fn no_home_and_no_override_is_still_the_cwd() {
        assert_eq!(games_dir_from(None, None), PathBuf::from("."));
        assert_eq!(games_dir_from(None, os("")), PathBuf::from("."));
    }

    // -- the platform gate on HOME -------------------------------------------
    //
    // Git for Windows sets HOME, so a Windows build must never honour it as
    // the games-dir fallback -- it would silently resolve to
    // `C:\Users\<name>\games`, a third location distinct from both the old
    // `.` fallback and the native default `%USERPROFILE%\dml-native`.

    /// On Windows, a set HOME must be ignored -- Git for Windows sets it, and
    /// honouring it is the exact bug this gate exists to prevent. This is also
    /// what keeps the Windows unset answer byte-identical to what it was
    /// before the home fallback existed.
    #[test]
    fn windows_ignores_a_set_home() {
        assert_eq!(home_fallback(true, os("/home/dml")), None);
    }

    /// Off Windows, HOME is the user's own variable and is exactly what we
    /// want -- honour it unchanged.
    #[test]
    fn non_windows_honours_home() {
        assert_eq!(home_fallback(false, os("/home/dml")), os("/home/dml"));
    }

    /// DML_GAMES_DIR still wins over the home fallback on both platforms --
    /// this exercises the gate feeding into the full decision, not just the
    /// gate in isolation.
    #[test]
    fn env_var_wins_regardless_of_platform() {
        assert_eq!(
            games_dir_from(os("/tmp/dml-games-test"), home_fallback(true, os("C:/Users/dml"))),
            PathBuf::from("/tmp/dml-games-test")
        );
        assert_eq!(
            games_dir_from(os("/tmp/dml-games-test"), home_fallback(false, os("/home/dml"))),
            PathBuf::from("/tmp/dml-games-test")
        );
    }

    // -- title/compose-dir resolution --------------------------------------
    //
    // NOTE: `games_dir_from_env`/`title_dir_for_id` read the process-global
    // `DML_GAMES_DIR` env var directly -- deliberately NOT exercised here via
    // `std::env::set_var` (this crate's `cargo test` runs multi-threaded in
    // one process; mutating a process-global var would race any other test
    // reading it concurrently). The join logic itself is a one-line
    // `base.join(id)`, covered indirectly by every `resolve_compose_dir` test
    // below via a directly-constructed title dir.

    #[test]
    fn resolve_compose_dir_at_root() {
        let dir = std::env::temp_dir().join(format!("dml-lifecycle-test-root-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("docker-compose.yml"), "").unwrap();
        assert_eq!(resolve_compose_dir(&dir), Some(dir.clone()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn resolve_compose_dir_in_subdir() {
        let dir = std::env::temp_dir().join(format!("dml-lifecycle-test-sub-{}", std::process::id()));
        let sub = dir.join("wow-server-playerbots");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("compose.yaml"), "").unwrap();
        assert_eq!(resolve_compose_dir(&dir), Some(sub.clone()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn resolve_compose_dir_none_when_no_compose_anywhere() {
        let dir = std::env::temp_dir().join(format!("dml-lifecycle-test-none-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("subdir")).unwrap();
        assert_eq!(resolve_compose_dir(&dir), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // -- Part 5a: compose_file_name / count_running_ids (`games status`) --

    #[test]
    fn compose_file_name_scan_order_and_absence() {
        let dir = std::env::temp_dir().join(format!("dml-lifecycle-test-cfn-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(compose_file_name(&dir), None);

        // Lower-priority name only.
        std::fs::write(dir.join("compose.yaml"), "").unwrap();
        assert_eq!(compose_file_name(&dir), Some("compose.yaml"));

        // Higher-priority name added -> scan order picks it first.
        std::fs::write(dir.join("docker-compose.yml"), "").unwrap();
        assert_eq!(compose_file_name(&dir), Some("docker-compose.yml"));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn count_running_ids_counts_nonblank_lines() {
        assert_eq!(count_running_ids(""), 0);
        assert_eq!(count_running_ids("\n\n"), 0);
        assert_eq!(count_running_ids("abc123\n"), 1);
        assert_eq!(count_running_ids("abc123\ndef456\n"), 2);
        // A trailing/whitespace-only line never counts as a container.
        assert_eq!(count_running_ids("abc123\n   \n"), 1);
    }

    // -- compose sequencing ---------------------------------------------------

    #[test]
    fn compose_sequence_start_is_up_only() {
        assert_eq!(compose_sequence_for_mode("start"), vec![compose_up_argv()]);
    }

    #[test]
    fn compose_sequence_restart_is_down_then_up() {
        assert_eq!(compose_sequence_for_mode("restart"), vec![compose_down_argv(), compose_up_argv()]);
    }

    #[test]
    fn compose_sequence_stop_is_down_only() {
        assert_eq!(compose_sequence_for_mode("stop"), vec![compose_down_argv()]);
    }

    #[test]
    fn compose_sequence_unknown_mode_is_empty() {
        assert!(compose_sequence_for_mode("bogus").is_empty());
    }

    #[test]
    fn compose_argv_exact_tokens() {
        assert_eq!(compose_up_argv(), vec!["compose", "up", "-d"]);
        assert_eq!(compose_down_argv(), vec!["compose", "down", "-t", "180"]);
    }

    #[test]
    fn is_compose_down_distinguishes_up_from_down() {
        assert!(is_compose_down(&compose_down_argv()));
        assert!(!is_compose_down(&compose_up_argv()));
    }

    #[test]
    fn is_compose_down_every_step_of_every_mode_sequence() {
        // Regression: argv[0] is "compose" for BOTH builders, so a naive
        // `argv.first() == Some(&"down")` check is always false -- this
        // exercises the real per-mode sequence end-to-end, the shape a live
        // round-trip caught the bug in.
        for (mode, expect_down) in [("start", vec![false]), ("restart", vec![true, false]), ("stop", vec![true])] {
            let got: Vec<bool> = compose_sequence_for_mode(mode).iter().map(|argv| is_compose_down(argv)).collect();
            assert_eq!(got, expect_down, "mode={mode}");
        }
    }
}
