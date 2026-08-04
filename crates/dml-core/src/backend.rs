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
    /// THE supported backend: the `dml-arch` WSL distro hosting its own
    /// `dockerd`, with the Rust `dml-wow` binary running INSIDE it.
    Arch,
    /// Retired as a runtime path. Kept so an existing `launcher.json` still
    /// parses; `from_override` maps it to [`Backend::Arch`], which names the
    /// same distro and the same daemon.
    Wsl,
    /// Docker Desktop on the Windows host. Kept working as a fallback, never
    /// extended.
    Native,
}

/// Pure parse of the backend choice from an override value.
///
/// Everything that is not explicitly Docker Desktop resolves to
/// [`Backend::Arch`] — including `wsl`, an empty string, and a typo. That is a
/// reversal of the old rule (unknown → Wsl) and it is deliberate: Arch is now
/// the backend the launcher can provision from nothing, so it is the safe
/// place for an unrecognised value to land. Sending a typo to Native would
/// point the user at a Docker Desktop they may not have installed.
pub fn from_override(value: Option<&str>) -> Backend {
    match value.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
        Some("native") | Some("docker") => Backend::Native,
        _ => Backend::Arch,
    }
}

/// The backend selected for this process, read from `DML_BACKEND`.
pub fn selected() -> Backend {
    from_override(std::env::var(BACKEND_ENV).ok().as_deref())
}

/// Which backend a machine looks like it wants.
///
/// `distro_usable` answers "is `dml-arch` registered?" and is a [`Tri`]
/// because a probe that could not answer is evidence of nothing.
///
/// | distro usable | native dir | docker | → |
/// |---|---|---|---|
/// | `Yes` | — | — | **Arch** (a distro we can talk to IS the supported backend) |
/// | `No`/`Unknown` | yes | yes | Native (they have a working server; do not move them) |
/// | `No`/`Unknown` | otherwise | | **Arch** (the one backend we can build from nothing) |
///
/// The middle row is the one worth defending. A user with a server already
/// installed under Docker Desktop and no distro to move it to must not be
/// routed at a directory that does not exist. Everyone else — including the
/// fresh machine with neither — gets Arch, because Arch is provisionable and
/// Docker Desktop is a separate download with its own licence terms.
///
/// A user with BOTH a usable distro and a native server gets Arch, and can say
/// `DML_BACKEND=native` to say otherwise. That is the cost of having a default
/// at all, and it is one setting rather than a lost server.
pub fn detect(native_dir_exists: bool, docker_present: bool, distro_usable: Tri) -> Backend {
    if distro_usable == Tri::Yes {
        return Backend::Arch;
    }
    if native_dir_exists && docker_present {
        return Backend::Native;
    }
    Backend::Arch
}

/// Full precedence: `DML_BACKEND` env → `launcher.json` → auto-detect.
///
/// `file_value` is the persisted setting; `"auto"` (its default) means "fall
/// through to detection", which is why it cannot simply be handed to
/// [`from_override`] — that maps every unrecognized string to `Arch`.
pub fn resolve(
    env_value: Option<&str>,
    file_value: Option<&str>,
    native_dir_exists: bool,
    docker_present: bool,
    distro_usable: Tri,
) -> Backend {
    // `"auto"` means detection wherever it appears. Honouring it only in the
    // file would silently pin a user who hand-writes `DML_BACKEND=auto` to
    // Arch (from_override maps unrecognised strings there) — and `auto` is
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
    detect(native_dir_exists, docker_present, distro_usable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arch_is_the_default_and_wsl_resolves_to_it() {
        // Decision 2 of the spec: the bash CLI is retired as a runtime path,
        // but `wsl` names the same distro and the same daemon, so an existing
        // launcher.json or a hand-written DML_BACKEND=wsl must land on Arch.
        // Refusing would strand every current user; mapping it to Native would
        // point them at a server directory that is not theirs.
        assert_eq!(from_override(None), Backend::Arch);
        assert_eq!(from_override(Some("")), Backend::Arch);
        assert_eq!(from_override(Some("arch")), Backend::Arch);
        assert_eq!(from_override(Some("  WSL ")), Backend::Arch);
        assert_eq!(from_override(Some("natve")), Backend::Arch);
    }

    #[test]
    fn native_still_needs_saying_so_explicitly() {
        assert_eq!(from_override(Some("native")), Backend::Native);
        assert_eq!(from_override(Some("  DOCKER ")), Backend::Native);
    }

    #[test]
    fn a_usable_distro_is_always_arch() {
        for dir in [true, false] {
            for docker in [true, false] {
                assert_eq!(detect(dir, docker, Tri::Yes), Backend::Arch, "dir={dir} docker={docker}");
            }
        }
    }

    #[test]
    fn a_working_native_user_with_no_distro_is_left_on_native() {
        // They have a server installed under Docker Desktop and no distro to
        // move it to. Moving them would point the app at a directory that does
        // not exist yet.
        assert_eq!(detect(true, true, Tri::No), Backend::Native);
        assert_eq!(detect(true, true, Tri::Unknown), Backend::Native);
    }

    #[test]
    fn a_fresh_machine_gets_arch_because_arch_is_the_one_we_can_provision() {
        assert_eq!(detect(false, false, Tri::No), Backend::Arch);
        assert_eq!(detect(false, true, Tri::No), Backend::Arch);
        assert_eq!(detect(false, false, Tri::Unknown), Backend::Arch);
    }

    #[test]
    fn resolve_env_still_outranks_everything() {
        // Load-bearing: the parity, bats and CLI-integration suites all inject
        // these vars as override seams.
        assert_eq!(resolve(Some("native"), Some("arch"), false, false, Tri::Yes), Backend::Native);
        assert_eq!(resolve(Some("arch"), Some("native"), true, true, Tri::No), Backend::Arch);
    }

    #[test]
    fn resolve_ignores_empty_env_and_falls_through_to_file() {
        assert_eq!(resolve(Some(""), Some("native"), false, false, Tri::Yes), Backend::Native);
        assert_eq!(resolve(None, Some("native"), false, false, Tri::Yes), Backend::Native);
    }

    #[test]
    fn resolve_auto_means_detect_in_both_places() {
        assert_eq!(resolve(Some("auto"), None, true, true, Tri::No), Backend::Native);
        assert_eq!(resolve(None, Some("  AUTO "), false, false, Tri::Yes), Backend::Arch);
    }
}
