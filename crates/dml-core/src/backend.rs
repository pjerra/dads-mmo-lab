//! Which orchestration backend the launcher drives (spike:
//! `spike/docker-desktop-native`).
//!
//! Today the app is hard-wired to [`Wsl`](Backend::Wsl): every feature shells
//! `wsl.exe -d dml-arch -u dml -- dml … --json`. The native path
//! ([`Backend::Native`], see [`super::native`]) drives `docker compose`
//! directly on the Windows host.
//!
//! This selector is the SINGLE wiring point for the switch. It exists and is
//! tested now so the migration is a routing change at call sites, not a
//! search-and-replace: a `games_*` command asks [`Backend::selected`] and, when
//! it is `Native`, routes to the [`NativeDocker`](super::native::NativeDocker)
//! lifecycle instead of the WSL runner. Default stays `Wsl`, so nothing changes
//! until `DML_BACKEND=native` is set — the spike ships dormant.

#![allow(dead_code)] // selector wired at call sites during the port, not yet

/// Environment variable that overrides the backend. `native` selects the
/// Docker-Desktop path; anything else (or unset) stays on WSL.
pub const BACKEND_ENV: &str = "DML_BACKEND";

use crate::setup::Tri;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// Current default: the bash `dml` CLI inside the `dml-arch` WSL distro.
    Wsl,
    /// Native `docker compose` against Docker Desktop — no distro, no bash CLI.
    Native,
}

/// Pure parse of the backend choice from an override value, so the mapping is
/// testable without touching the process environment. Case-insensitive;
/// unrecognized or absent values fall back to the safe current default (WSL) —
/// a typo must never silently strand the user on an unfinished backend.
pub fn from_override(value: Option<&str>) -> Backend {
    match value.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
        Some("native") | Some("docker") => Backend::Native,
        _ => Backend::Wsl,
    }
}

/// The backend selected for this process, read from `DML_BACKEND`.
pub fn selected() -> Backend {
    from_override(std::env::var(BACKEND_ENV).ok().as_deref())
}

/// Which backend a machine looks like it wants.
///
/// `wsl_usable` answers "could the WSL backend actually work here?" — i.e. is
/// the distro registered. It is a [`Tri`] because this project's standing rule
/// is that a probe which cannot answer is evidence of NOTHING; see the
/// `Unknown` arm below.
///
/// ## The bug this signature exists to fix
///
/// Detection used to be `native_dir_exists && docker_present`, which contained
/// a trap that only sprang on the machines that mattered most: **a fresh PC
/// could never select Native.** `native_dir_exists` is true only once a native
/// server has been installed, and installing one requires being in native mode
/// — so a brand-new user with Docker Desktop and no distro at all was routed to
/// WSL, the one backend their machine could not run. The launcher then showed
/// them a "set up WSL" screen for a path we are actively retiring.
///
/// The missing question was never "has a native server been installed"; it was
/// "can WSL work here at all". Now:
///
/// | docker | native dir | WSL usable | → |
/// |---|---|---|---|
/// | no | — | — | Wsl (native cannot start anything without Docker) |
/// | yes | yes | — | **Native** (a native server is present: use it) |
/// | yes | no | `No` | **Native** (the fix: WSL provably cannot work) |
/// | yes | no | `Yes` | Wsl (they have a working distro — do not move them) |
/// | yes | no | `Unknown` | Wsl (unknown is not absence — keep the safe default) |
///
/// The `Unknown` row is the one worth defending. Treating "the probe fell over"
/// as "no distro" would flip an existing, working WSL user to Native the first
/// time `wsl.exe` was slow or busy, pointing the app at a server directory that
/// is not theirs. Staying put costs a user with no distro one extra click; the
/// other direction costs a working user their server.
pub fn detect(native_dir_exists: bool, docker_present: bool, wsl_usable: Tri) -> Backend {
    if !docker_present {
        return Backend::Wsl;
    }
    if native_dir_exists {
        return Backend::Native;
    }
    match wsl_usable {
        Tri::No => Backend::Native,
        Tri::Yes | Tri::Unknown => Backend::Wsl,
    }
}

/// Full precedence: `DML_BACKEND` env → `launcher.json` → auto-detect.
///
/// `file_value` is the persisted setting; `"auto"` (its default) means "fall
/// through to detection", which is why it cannot simply be handed to
/// [`from_override`] — that maps every unrecognized string to `Wsl`.
pub fn resolve(
    env_value: Option<&str>,
    file_value: Option<&str>,
    native_dir_exists: bool,
    docker_present: bool,
    wsl_usable: Tri,
) -> Backend {
    // `"auto"` means detection wherever it appears. Honouring it only in the
    // file would silently pin a user who hand-writes `DML_BACKEND=auto` to
    // Wsl (from_override maps unrecognised strings there) — and `auto` is
    // exactly what the file documents and the UI labels "Detect
    // automatically", so writing it into the env is entirely plausible.
    let env_value = env_value.map(str::trim).filter(|v| !v.is_empty());
    if let Some(v) = env_value.filter(|v| !v.eq_ignore_ascii_case("auto")) {
        return from_override(Some(v));
    }
    if env_value.is_none() {
        if let Some(v) = file_value
            .map(str::trim)
            .filter(|v| !v.is_empty() && !v.eq_ignore_ascii_case("auto"))
        {
            return from_override(Some(v));
        }
    }
    detect(native_dir_exists, docker_present, wsl_usable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_wsl() {
        assert_eq!(from_override(None), Backend::Wsl);
        assert_eq!(from_override(Some("")), Backend::Wsl);
    }

    #[test]
    fn native_aliases() {
        assert_eq!(from_override(Some("native")), Backend::Native);
        assert_eq!(from_override(Some("Native")), Backend::Native);
        assert_eq!(from_override(Some("  DOCKER ")), Backend::Native);
    }

    #[test]
    fn unknown_falls_back_to_wsl() {
        // A typo must not strand the user on a half-built backend.
        assert_eq!(from_override(Some("natve")), Backend::Wsl);
        assert_eq!(from_override(Some("wsl")), Backend::Wsl);
    }

    #[test]
    fn detect_never_picks_native_without_docker() {
        // Native cannot start anything without an engine, so no other signal
        // may override this.
        for dir in [true, false] {
            for wsl in [Tri::Yes, Tri::No, Tri::Unknown] {
                assert_eq!(detect(dir, false, wsl), Backend::Wsl, "dir={dir} wsl={wsl:?}");
            }
        }
    }

    #[test]
    fn detect_picks_native_when_a_native_server_is_already_there() {
        for wsl in [Tri::Yes, Tri::No, Tri::Unknown] {
            assert_eq!(detect(true, true, wsl), Backend::Native, "wsl={wsl:?}");
        }
    }

    /// THE FRESH-MACHINE BUG. Detection used to be
    /// `native_dir_exists && docker_present`, and `native_dir_exists` only
    /// becomes true once a native server has been installed — which requires
    /// being in native mode. So a new PC with Docker Desktop and no distro was
    /// routed to WSL, the one backend it could not run, and shown a "set up
    /// WSL" screen for the path being retired.
    #[test]
    fn a_machine_with_docker_and_no_distro_gets_native() {
        assert_eq!(detect(false, true, Tri::No), Backend::Native);
    }

    #[test]
    fn a_working_wsl_user_is_left_where_they_are() {
        // They have a distro and no native server: moving them would point the
        // app at a server directory that is not theirs.
        assert_eq!(detect(false, true, Tri::Yes), Backend::Wsl);
    }

    /// Tri-state discipline, and the row most likely to be "simplified" away:
    /// a probe that could not answer is NOT a probe that answered "no". Reading
    /// Unknown as absence would flip a working WSL user to Native the first
    /// time `wsl.exe` was slow or busy. Staying put costs a distro-less user one
    /// extra click; the other direction costs a working user their server.
    #[test]
    fn a_probe_that_could_not_answer_is_not_evidence_of_no_distro() {
        assert_eq!(detect(false, true, Tri::Unknown), Backend::Wsl);
    }

    #[test]
    fn resolve_env_outranks_everything() {
        // Load-bearing: the 18 parity suites, the bats suite and the CLI
        // integration tests all inject these vars as override seams. If the
        // file outranked env, those tests would start reading a developer's
        // persisted launcher.json.
        assert_eq!(resolve(Some("wsl"), Some("native"), true, true, Tri::Yes), Backend::Wsl);
        assert_eq!(resolve(Some("native"), Some("wsl"), false, false, Tri::Yes), Backend::Native);
    }

    #[test]
    fn resolve_ignores_empty_env_and_falls_through_to_file() {
        assert_eq!(resolve(Some(""), Some("native"), false, false, Tri::Yes), Backend::Native);
        assert_eq!(resolve(None, Some("native"), false, false, Tri::Yes), Backend::Native);
    }

    #[test]
    fn resolve_auto_in_file_means_detect_not_wsl() {
        // "auto" is NOT a value from_override understands -- passing it
        // straight through would silently mean Wsl and defeat detection.
        assert_eq!(resolve(None, Some("auto"), true, true, Tri::Yes), Backend::Native);
        assert_eq!(resolve(None, Some("  AUTO "), true, true, Tri::Yes), Backend::Native);
        assert_eq!(resolve(None, Some("auto"), false, false, Tri::Yes), Backend::Wsl);
    }

    #[test]
    fn resolve_absent_file_value_means_detect() {
        assert_eq!(resolve(None, None, true, true, Tri::Yes), Backend::Native);
        assert_eq!(resolve(None, Some(""), true, true, Tri::Yes), Backend::Native);
    }

    #[test]
    fn resolve_auto_in_the_ENV_also_means_detect() {
        // Final-review finding: honouring "auto" only in the file silently
        // pinned a user who hand-wrote DML_BACKEND=auto to Wsl, because
        // from_override maps unrecognised strings there. "auto" is exactly
        // what the file documents and the UI labels "Detect automatically",
        // so writing it into the env is entirely plausible.
        assert_eq!(resolve(Some("auto"), None, true, true, Tri::Yes), Backend::Native);
        assert_eq!(resolve(Some("  AUTO "), Some("wsl"), true, true, Tri::Yes), Backend::Native);
        assert_eq!(resolve(Some("auto"), None, false, false, Tri::Yes), Backend::Wsl);
    }

    #[test]
    fn resolve_typo_in_file_is_wsl_not_detect() {
        // Same doctrine as from_override: a typo must never silently strand
        // the user on an unfinished backend, so it resolves to Wsl rather
        // than being treated as "auto".
        assert_eq!(resolve(None, Some("natve"), true, true, Tri::Yes), Backend::Wsl);
    }
}
