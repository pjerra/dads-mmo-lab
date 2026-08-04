//! Which orchestration backend the launcher drives.
//!
//! Three arms. [`Backend::Wsl`] — the bash `dml` CLI inside the `dml-arch`
//! distro — is STILL the default, and still where an unrecognised value lands.
//! [`Backend::Arch`] is the same distro driven by the Rust `dml-wow` binary
//! running INSIDE it against that distro's own `dockerd`; it is built, tested
//! and reachable ONLY by an explicit `DML_BACKEND=arch` or an explicit
//! `launcher.json` value. [`Backend::Native`] ([`super::native`]) drives
//! `docker compose` directly against Docker Desktop on the Windows host; it is
//! kept working as a fallback — never extended — for a user who already has a
//! server there and no distro to move it to.
//!
//! WHY ARCH IS NOT THE DEFAULT YET (user ruling, 2026-08-04). The whole Arch
//! backend landed on this branch and every piece of it passed its own review,
//! but the launcher does not yet speak `dml-wow`'s command vocabulary. Live
//! call sites in `launcher/src-tauri/src/lib.rs` send `games list`,
//! `games status <id>`, `wow server-detail` and `games <action> <id>`, and
//! [`DmlRunner::command`](crate::runner::DmlRunner) appends `--json`
//! unconditionally — `dml-wow-cli` has neither a global `--json` flag nor a
//! `games`/`wow` namespace (its subcommands are flat: `status`, `start`,
//! `stop`, `restart`, `version`, ...). Defaulting to Arch today would hand an
//! existing WSL user "unrecognized subcommand 'games'" in place of their status
//! card, and break Start/Stop/Restart, Library and the auto-shutdown watcher
//! against a server that is perfectly healthy. The default flips in the NEXT
//! plan, in the same change that teaches the launcher that vocabulary — not
//! before.
//!
//! This selector is the SINGLE wiring point for the switch. It exists and is
//! tested now so the migration is a routing change at call sites, not a
//! search-and-replace: a `games_*` command asks [`Backend::selected`] and, when
//! it is `Native`, routes to the [`NativeDocker`](super::native::NativeDocker)
//! lifecycle instead of the WSL/Arch runner.

#![allow(dead_code)] // selector wired at call sites during the port, not yet

/// Environment variable that overrides the backend. `native` (or `docker`)
/// selects Docker Desktop and `arch` selects the in-distro Rust binary;
/// anything else — unset, empty, `wsl`, or a typo — resolves to
/// [`Backend::Wsl`], the backend the launcher can actually drive today. See
/// [`from_override`].
pub const BACKEND_ENV: &str = "DML_BACKEND";

use crate::setup::Tri;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// The `dml-arch` WSL distro hosting its own `dockerd`, with the Rust
    /// `dml-wow` binary running INSIDE it. Built and tested; an EXPLICIT
    /// opt-in only (`DML_BACKEND=arch` / `launcher.json`), because the
    /// launcher's call sites still speak the bash CLI's vocabulary — see the
    /// module doc.
    Arch,
    /// THE default: the bash `dml` CLI inside the `dml-arch` distro. Every
    /// live call site in the launcher is written against its command surface
    /// and its `--json` envelope.
    Wsl,
    /// Docker Desktop on the Windows host. Kept working as a fallback, never
    /// extended.
    Native,
}

/// Pure parse of the backend choice from an override value.
///
/// `native`/`docker` select Docker Desktop, `arch` selects the in-distro Rust
/// binary, and EVERYTHING ELSE — `wsl`, an empty string, an absent value, a
/// typo — resolves to [`Backend::Wsl`]. A value we do not recognise must never
/// silently strand the user on a backend the launcher cannot yet drive, and
/// today that is both of the other two: Native points at a Docker Desktop they
/// may not have installed, and Arch answers `games list` with "unrecognized
/// subcommand" (see the module doc for why the Arch default is deferred).
pub fn from_override(value: Option<&str>) -> Backend {
    match value.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
        Some("native") | Some("docker") => Backend::Native,
        Some("arch") => Backend::Arch,
        _ => Backend::Wsl,
    }
}

/// The backend selected for this process, read from `DML_BACKEND`.
pub fn selected() -> Backend {
    from_override(std::env::var(BACKEND_ENV).ok().as_deref())
}

/// Which backend a machine looks like it wants.
///
/// **NEVER returns [`Backend::Arch`].** Detection can only choose between the
/// two backends the launcher's call sites can drive today; Arch is an explicit
/// opt-in until the vocabulary wiring lands (module doc). Auto-detecting a user
/// onto a backend that answers `games list` with "unrecognized subcommand" is
/// worse than any of the rows below.
///
/// `distro_usable` answers "is `dml-arch` registered?" and is a [`Tri`]
/// because a probe that could not answer is evidence of nothing.
///
/// | docker | native dir | distro usable | → |
/// |---|---|---|---|
/// | no | — | — | Wsl (native cannot start anything without an engine) |
/// | yes | yes | — | **Native** (a native server is present: use it) |
/// | yes | no | `No` | **Native** (WSL provably cannot work here) |
/// | yes | no | `Yes` | Wsl (they have a working distro — do not move them) |
/// | yes | no | `Unknown` | Wsl (unknown is not absence — keep the default) |
///
/// Row 3 is the fresh-PC fix and the "not stranded" guarantee: `native_dir_exists`
/// only becomes true once a native server has been installed, which requires
/// being in native mode — so a brand-new PC with Docker Desktop and no distro
/// must not be routed at WSL, the one backend it cannot run.
///
/// The `Unknown` row is the other one worth defending. Reading "the probe fell
/// over" as "no distro" would flip an existing, working WSL user to Native the
/// first time `wsl.exe` was slow or busy, pointing the app at a server
/// directory that is not theirs. Staying put costs a distro-less user one
/// extra click; the other direction costs a working user their server.
pub fn detect(native_dir_exists: bool, docker_present: bool, distro_usable: Tri) -> Backend {
    if !docker_present {
        return Backend::Wsl;
    }
    if native_dir_exists {
        return Backend::Native;
    }
    match distro_usable {
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
    distro_usable: Tri,
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
    detect(native_dir_exists, docker_present, distro_usable)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WHY THE DEFAULT IS STILL `Wsl`, and why nobody should "fix" this back:
    /// the launcher's live call sites send `games list`, `games status <id>`,
    /// `wow server-detail` and `games <action> <id>`, and `DmlRunner::command`
    /// appends `--json` unconditionally. `dml-wow-cli` defines none of that —
    /// no global `--json`, no `games`/`wow` namespace. Landing an unrecognised
    /// value (or an absent one) on `Arch` therefore replaces a working user's
    /// status card with "unrecognized subcommand 'games'". Ruled by the user
    /// 2026-08-04: the default flips in the plan that teaches the launcher
    /// `dml-wow`'s vocabulary, in the same change.
    #[test]
    fn wsl_is_the_default_and_an_unrecognised_value_lands_there() {
        assert_eq!(from_override(None), Backend::Wsl);
        assert_eq!(from_override(Some("")), Backend::Wsl);
        assert_eq!(from_override(Some("  WSL ")), Backend::Wsl);
        assert_eq!(from_override(Some("natve")), Backend::Wsl);
    }

    /// The Arch backend is fully built — it is reachable, but only when
    /// someone asks for it BY NAME. (Same test as before the reversal, same
    /// direction: `arch` has always meant Arch.)
    #[test]
    fn arch_is_reachable_but_only_as_an_explicit_opt_in() {
        assert_eq!(from_override(Some("arch")), Backend::Arch);
        assert_eq!(from_override(Some("  ARCH ")), Backend::Arch);
    }

    #[test]
    fn native_still_needs_saying_so_explicitly() {
        assert_eq!(from_override(Some("native")), Backend::Native);
        assert_eq!(from_override(Some("  DOCKER ")), Backend::Native);
    }

    /// The hard rule on detection, and the one that must survive any future
    /// tidy-up of the table: auto-detect may never choose a backend the
    /// launcher cannot drive. Exhaustive over every input combination, so it
    /// fails the moment someone reintroduces an `Arch` arm here rather than in
    /// `from_override` where an explicit opt-in belongs.
    #[test]
    fn detect_never_chooses_arch_on_its_own() {
        for dir in [true, false] {
            for docker in [true, false] {
                for distro in [Tri::Yes, Tri::No, Tri::Unknown] {
                    assert_ne!(
                        detect(dir, docker, distro),
                        Backend::Arch,
                        "dir={dir} docker={docker} distro={distro:?}: Arch is an explicit opt-in \
                         until the launcher speaks dml-wow's command vocabulary"
                    );
                }
            }
        }
    }

    /// Was `a_usable_distro_is_always_arch`. Same input, opposite direction:
    /// a user with a registered distro stays on the backend that drives it
    /// today, which is the bash CLI.
    #[test]
    fn a_usable_distro_keeps_the_wsl_backend() {
        assert_eq!(detect(false, true, Tri::Yes), Backend::Wsl);
        assert_eq!(detect(false, false, Tri::Yes), Backend::Wsl);
    }

    #[test]
    fn a_working_native_user_with_no_distro_is_left_on_native() {
        // They have a server installed under Docker Desktop and no distro to
        // move it to. Moving them would point the app at a directory that does
        // not exist yet.
        assert_eq!(detect(true, true, Tri::No), Backend::Native);
        assert_eq!(detect(true, true, Tri::Unknown), Backend::Native);
        assert_eq!(detect(true, true, Tri::Yes), Backend::Native);
    }

    /// THE FRESH-MACHINE ROW, preserved through the reversal (it predates this
    /// branch — commit 22b7df1). `native_dir_exists` is only true once a native
    /// server has been installed, which requires being in native mode, so a
    /// brand-new PC with Docker Desktop and no distro must NOT be sent to WSL.
    #[test]
    fn a_machine_with_docker_and_no_distro_is_not_stranded() {
        assert_eq!(detect(false, true, Tri::No), Backend::Native);
    }

    /// Was `a_fresh_machine_gets_arch_because_arch_is_the_one_we_can_provision`.
    /// Without an engine on the host there is nothing for Native to drive, so
    /// no other signal may override this.
    #[test]
    fn without_docker_there_is_nothing_native_can_drive() {
        for dir in [true, false] {
            for distro in [Tri::Yes, Tri::No, Tri::Unknown] {
                assert_eq!(detect(dir, false, distro), Backend::Wsl, "dir={dir} distro={distro:?}");
            }
        }
    }

    /// Tri-state discipline: a probe that could not answer is not a probe that
    /// answered "no". Reading Unknown as absence would flip a working WSL user
    /// to Native the first time `wsl.exe` was slow.
    #[test]
    fn a_probe_that_could_not_answer_is_not_evidence_of_no_distro() {
        assert_eq!(detect(false, true, Tri::Unknown), Backend::Wsl);
    }

    #[test]
    fn resolve_env_still_outranks_everything() {
        // Load-bearing: the parity, bats and CLI-integration suites all inject
        // these vars as override seams.
        assert_eq!(resolve(Some("native"), Some("arch"), false, false, Tri::Yes), Backend::Native);
        assert_eq!(resolve(Some("arch"), Some("native"), true, true, Tri::No), Backend::Arch);
        assert_eq!(resolve(Some("wsl"), Some("native"), true, true, Tri::Yes), Backend::Wsl);
    }

    /// `arch` in `launcher.json` is the OTHER explicit opt-in, and it must keep
    /// working while the default points elsewhere — it is how the next plan's
    /// wiring gets exercised on a real machine before it ships.
    #[test]
    fn arch_can_be_opted_into_from_the_config_file_too() {
        assert_eq!(resolve(None, Some("arch"), true, true, Tri::Yes), Backend::Arch);
    }

    #[test]
    fn resolve_ignores_empty_env_and_falls_through_to_file() {
        assert_eq!(resolve(Some(""), Some("native"), false, false, Tri::Yes), Backend::Native);
        assert_eq!(resolve(None, Some("native"), false, false, Tri::Yes), Backend::Native);
    }

    #[test]
    fn resolve_auto_means_detect_in_both_places() {
        assert_eq!(resolve(Some("auto"), None, true, true, Tri::No), Backend::Native);
        assert_eq!(resolve(None, Some("  AUTO "), false, false, Tri::Yes), Backend::Wsl);
    }

    /// A typo in the file is `Wsl`, not "fall through to detect": the same
    /// doctrine as `from_override`, restated at the resolver because that is
    /// where a persisted value enters.
    #[test]
    fn resolve_typo_in_file_is_the_default_not_detect() {
        assert_eq!(resolve(None, Some("natve"), true, true, Tri::No), Backend::Wsl);
    }
}
