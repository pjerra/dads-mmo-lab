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
}
