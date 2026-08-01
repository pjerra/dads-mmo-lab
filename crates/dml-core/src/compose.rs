//! Generic `docker compose` lifecycle primitives — title/compose-dir
//! resolution, the `games status` running-count helper, the per-mode argv
//! sequence, and the port-conflict bind probe. Moved out of the launcher's
//! `dml::lifecycle` (cargo-workspace refactor, Task 6); everything WoW/
//! playerbots-specific (the flush-heal guard, the hardcoded game-port
//! registry, `check_port_conflicts`, the automatic-backup step list) stayed
//! behind in `dml::lifecycle`, which re-exports these names so every existing
//! caller keeps compiling unchanged.

use std::path::{Path, PathBuf};

/// `GAMES_DIR` base -- same env var + fallback as `ConfigReader::
/// title_dir_from_env()`, but generalized to an arbitrary title `id` rather
/// than hardcoding `wow-server-playerbots` (mirrors `_games_resolve_or_fail`'s
/// `dir="$GAMES_DIR/$gid"`, which works for any installed title).
pub fn games_dir_from_env() -> PathBuf {
    std::env::var_os("DML_GAMES_DIR")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
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
