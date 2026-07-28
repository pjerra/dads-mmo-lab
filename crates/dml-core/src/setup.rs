//! First-run backend probe chain — "can this machine actually run a server,
//! and if not, what is the FIRST thing missing?"
//!
//! SHIP-LIST Phase 4 exists because the launcher could not set itself up on
//! anybody else's computer: a stranger lands on Home and sees a status card
//! for a server that does not exist. This module is the seam that makes a
//! useful first-run screen possible — one ordered chain, one typed answer:
//!
//!   1. is WSL present at all?
//!   2. is the `dml-arch` distro registered?
//!   3. is the `dml` CLI installed inside it, and at what version?
//!   4. are any titles installed?
//!
//! TWO CONSUMERS, ONE ANSWER. The first-run screen switches on
//! [`BackendStatus::state`] to pick its one sentence + one button; the setup
//! command (SHIP-LIST 4.2) reads the same value to decide whether it has
//! anything to do and to re-check afterwards. Neither re-derives anything:
//! [`Probes`] is diagnostics, [`SetupState`] is the decision.
//!
//! THE TRI-STATE RULE, restated because this codebase keeps paying for it.
//! "wsl.exe did not answer" is evidence of NOTHING. It is not "WSL is
//! missing", and a first-run screen that treats it as such would offer to
//! re-provision a machine that is merely busy. Every probe therefore answers
//! [`Tri`], and every could-not-tell lands in [`SetupState::Unknown`] with
//! [`BackendStatus::blocked_at`] naming the step that went dark — never in a
//! "no" state that has an action attached to it.
//!
//! BOUNDEDNESS. Every spawn goes through [`dml_core::proc::run_bounded_outcome`]
//! (see [`SetupProbeEnv::timeout`]): a missing wsl.exe returns instantly, a
//! hung one is killed and reaped at the deadline. The chain also
//! short-circuits — probes 3 and 4 are never spawned when the distro is
//! absent, because a question about the inside of a distro that does not
//! exist has no honest answer.

use std::ffi::OsString;
use std::time::Duration;

use serde::Serialize;

use crate::envelope::{decode_wsl_output, parse_envelope};
use crate::proc::{run_bounded_outcome, windows_no_window, BoundedOutcome};

/// The `dml` CLI version this launcher's JSON contract is written against.
/// Source of truth is `cli/src/00-head.sh`'s `VERSION`; bump both together.
///
/// This is not pedantry: `guides/DML-Windows/Install-DML.ps1` still embeds
/// CLI **v2.6.0** as its bootstrap, so a freshly-created distro genuinely
/// arrives with an old `dml` that the launcher has to replace. That is the
/// [`SetupState::CliOutdated`] case, and it is the common one.
pub const EXPECTED_CLI_VERSION: &str = "3.0.0";

/// Default wall-clock bound for one probe spawn.
pub const DEFAULT_PROBE_TIMEOUT: Duration = Duration::from_secs(20);

/// A probe's answer. `Unknown` is NOT a synonym for `No` — see the module doc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Tri {
    Yes,
    No,
    Unknown,
}

/// Which link of the chain a probe belongs to. Only ever surfaced for
/// [`SetupState::Unknown`], so the UI can say *which* question went
/// unanswered instead of a bare "something went wrong".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupStep {
    Wsl,
    Distro,
    Cli,
    Titles,
}

/// The single value a consumer switches on. Ordered by the chain: the first
/// thing that is missing wins, so there is always exactly one next step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupState {
    /// No WSL on this machine. Needs the elevated substrate installer
    /// (`Install-DML.ps1`) — the launcher cannot fix this itself.
    NoWsl,
    /// WSL works, but the `dml-arch` distro is not registered. Also the
    /// elevated installer's job.
    NoDistro,
    /// The distro is there but has no `dml`. THIS the launcher fixes, from
    /// its own bundled resources.
    NoCli,
    /// A `dml` is installed but is not the contract version (the common case
    /// on a fresh substrate install, which bootstraps an old CLI). Same fix
    /// as `NoCli`.
    CliOutdated,
    /// Fully provisioned, no titles installed yet — send them to Library.
    NoTitles,
    /// Everything is in place.
    Ready,
    /// A probe could not answer. NOT a "no": offer a retry, never a repair.
    /// [`BackendStatus::blocked_at`] names the step.
    Unknown,
}

/// Raw probe results. Diagnostics for the UI to *display*, never to
/// re-derive [`SetupState`] from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Probes {
    pub wsl: Tri,
    pub distro: Tri,
    pub cli: Tri,
    /// The version string `dml version --json` reported, when it did.
    pub cli_version: Option<String>,
    /// Installed title count, or `None` for could-not-tell / not probed.
    pub titles: Option<usize>,
}

impl Probes {
    /// All-unknown: nothing has been asked yet.
    pub fn unprobed() -> Self {
        Probes { wsl: Tri::Unknown, distro: Tri::Unknown, cli: Tri::Unknown, cli_version: None, titles: None }
    }
}

/// The typed answer both the first-run screen and the setup command consume.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BackendStatus {
    /// The decision. Switch on this.
    pub state: SetupState,
    /// Set only when `state` is [`SetupState::Unknown`].
    pub blocked_at: Option<SetupStep>,
    /// The distro the chain asked about, so messages can name it.
    pub distro: String,
    pub expected_cli_version: String,
    /// Diagnostics. Do not re-derive `state` from these.
    pub probes: Probes,
}

/// What actually came back from one bounded probe spawn — the only shapes a
/// caller can observe, kept apart so the classifiers below never have to
/// guess whether silence meant absence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeOutcome {
    /// The program itself is not on this machine (`ErrorKind::NotFound`).
    /// The one genuinely definitive negative.
    ProgramMissing,
    /// Spawn failed for some other reason, or the call blew its deadline.
    CouldNotTell,
    /// It ran. Exit code plus decoded output (may still be a failure).
    Ran { code: Option<i32>, stdout: String, stderr: String },
}

impl ProbeOutcome {
    /// Adapt a bounded run. `NotFound` is the only spawn error that proves
    /// absence; everything else is a shrug.
    pub fn from_bounded(outcome: BoundedOutcome) -> Self {
        match outcome {
            BoundedOutcome::SpawnFailed(e) if e.kind() == std::io::ErrorKind::NotFound => {
                ProbeOutcome::ProgramMissing
            }
            BoundedOutcome::SpawnFailed(_) | BoundedOutcome::TimedOut => ProbeOutcome::CouldNotTell,
            BoundedOutcome::Ran(out) => ProbeOutcome::Ran {
                code: out.status.code(),
                stdout: decode_wsl_output(&out.stdout),
                stderr: decode_wsl_output(&out.stderr),
            },
        }
    }
}

/// Probe 1+2's answer: one `wsl --list --quiet` call settles both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WslProbe {
    pub wsl: Tri,
    pub distro: Tri,
}

/// Probe 3's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliProbe {
    pub cli: Tri,
    pub version: Option<String>,
}

// ---------------------------------------------------------------------------
// Pure classifiers — one per probe, so every branch is unit-testable without
// a machine that happens to be in the right state.
// ---------------------------------------------------------------------------

/// One listed distro name, cleaned. `wsl.exe` writes UTF-16LE with a BOM and
/// CRLF line endings, so the first name arrives as `"\u{feff}dml-arch\r"`.
fn clean_line(line: &str) -> &str {
    line.trim().trim_start_matches('\u{feff}').trim()
}

/// Classify `wsl.exe --list --quiet`, which settles probes 1 and 2 at once.
///
/// The non-zero-exit arm is where the tri-state discipline lives: WSL has two
/// well-known failure messages that mean genuinely different things, and
/// ANYTHING ELSE is a shrug. Guessing here would put a repair button in front
/// of a user whose machine is merely unhappy.
pub fn classify_wsl_list(outcome: &ProbeOutcome, distro: &str) -> WslProbe {
    match outcome {
        // No wsl.exe on the machine at all: WSL is not installed, and there
        // is therefore no distro either.
        ProbeOutcome::ProgramMissing => WslProbe { wsl: Tri::No, distro: Tri::No },
        ProbeOutcome::CouldNotTell => WslProbe { wsl: Tri::Unknown, distro: Tri::Unknown },
        ProbeOutcome::Ran { code, stdout, stderr } => {
            let text = format!("{stdout}\n{stderr}").to_lowercase();
            // Checked BEFORE the exit code, because the exit code lies here.
            // Windows ships an inbox wsl.exe stub on every machine; with WSL
            // not installed it prints this and STILL EXITS 0. Trusting exit 0
            // first is the exact bug that shipped in Install-DML.ps1 and cost
            // a clean-VM install run tonight (f304629) -- the same mistake had
            // been made independently here.
            if text.contains("is not installed") {
                return WslProbe { wsl: Tri::No, distro: Tri::No };
            }
            if *code == Some(0) {
                let present = stdout.lines().any(|l| clean_line(l) == distro);
                return WslProbe {
                    wsl: Tri::Yes,
                    distro: if present { Tri::Yes } else { Tri::No },
                };
            }
            if text.contains("no installed distributions") {
                // WSL itself works; the machine simply has nothing installed.
                WslProbe { wsl: Tri::Yes, distro: Tri::No }
            } else if text.contains("optional component is not enabled")
                || text.contains("has not been enabled")
                || text.contains("0x8007019e")
            {
                WslProbe { wsl: Tri::No, distro: Tri::No }
            } else {
                // Unrecognised failure. Evidence of NOTHING.
                WslProbe { wsl: Tri::Unknown, distro: Tri::Unknown }
            }
        }
    }
}

/// Classify `dml version --json` run inside the distro.
///
/// `Yes` is returned ONLY alongside a version parsed out of a real `ok`
/// envelope — [`derive`] leans on that (a `Yes` with no version is treated as
/// a lie, not as Ready).
pub fn classify_cli_version(outcome: &ProbeOutcome) -> CliProbe {
    let (code, stdout, stderr) = match outcome {
        // wsl.exe vanished between probes. Says nothing about `dml`.
        ProbeOutcome::ProgramMissing | ProbeOutcome::CouldNotTell => {
            return CliProbe { cli: Tri::Unknown, version: None }
        }
        ProbeOutcome::Ran { code, stdout, stderr } => (code, stdout, stderr),
    };
    if let Ok(env) = parse_envelope(stdout) {
        if env.ok {
            if let Some(v) = env.data.get("version").and_then(|v| v.as_str()) {
                let v = v.trim();
                if !v.is_empty() {
                    return CliProbe { cli: Tri::Yes, version: Some(v.to_string()) };
                }
            }
        }
    }
    let text = format!("{stdout}\n{stderr}").to_lowercase();
    // The shell's own verdict that the program does not exist — the only way
    // to say "not installed" without guessing.
    if *code == Some(127)
        || text.contains("command not found")
        || text.contains("dml: not found")
        || text.contains("no such file or directory")
    {
        return CliProbe { cli: Tri::No, version: None };
    }
    // A `dml` that answers in PLAIN TEXT is an OLD one, not an unknown one.
    //
    // This is the single most important case in the whole chain and it was
    // missed: the bootstrap CLI that guides/DML-Windows/Install-DML.ps1
    // base64-installs is v2.6.0, which has no `--json` flag whatsoever — its
    // `version` arm is `echo "dml v$VERSION"`. So the ONE machine state this
    // feature exists for (a stranger who just ran the elevated installer)
    // parsed as Unknown, which renders the dead-end "couldn't check this PC"
    // screen with no Set up backend button, and backend_setup then refused to
    // run. The upgrade path was invisible on exactly the machine that needed
    // it. Found by review before it ever reached a user, 2026-07-28.
    if let Some(v) = parse_plain_text_version(stdout) {
        return CliProbe { cli: Tri::Yes, version: Some(v) };
    }
    CliProbe { cli: Tri::Unknown, version: None }
}

/// Pull a version out of a pre-JSON `dml version` line, e.g. `dml v2.6.0`.
///
/// Deliberately strict: it must look like a dml version banner, so unrelated
/// chatter on stdout cannot be mistaken for a working CLI. Returns the bare
/// version so the caller compares it exactly as it compares a JSON one.
fn parse_plain_text_version(stdout: &str) -> Option<String> {
    for line in stdout.lines() {
        let line = line.trim();
        // `dml v2.6.0` / `dml 2.6.0` / `dml version 2.6.0`
        // NB `continue`, never `?`: the banner is rarely the first line (a
        // blank line or a warning can precede it), and bailing out of the
        // whole scan on the first non-matching line would reintroduce the
        // Unknown verdict this function exists to prevent.
        let Some(rest) = line.strip_prefix("dml ") else { continue };
        let rest = rest.trim_start();
        let rest = rest.strip_prefix("version ").unwrap_or(rest);
        let ver = rest.strip_prefix('v').unwrap_or(rest).trim();
        let head = ver.split_whitespace().next().unwrap_or("");
        if !head.is_empty()
            && head.starts_with(|c: char| c.is_ascii_digit())
            && head.chars().all(|c| c.is_ascii_digit() || c == '.')
        {
            return Some(head.to_string());
        }
    }
    None
}

/// Classify `dml games list --json`. `None` = could not tell (which must NOT
/// become "zero titles" — that has a button attached to it).
pub fn classify_titles(outcome: &ProbeOutcome) -> Option<usize> {
    let stdout = match outcome {
        ProbeOutcome::Ran { stdout, .. } => stdout,
        _ => return None,
    };
    let env = parse_envelope(stdout).ok()?;
    if !env.ok {
        return None;
    }
    env.data.get("games").and_then(|g| g.as_array()).map(Vec::len)
}

/// Whether a reported CLI version satisfies the contract. Exact match on
/// [`EXPECTED_CLI_VERSION`], forgiving only a `v` prefix and surrounding
/// whitespace — a launcher built against 3.0.0's JSON has no basis for
/// assuming 3.1.0 or 2.6.0 speaks it.
pub fn cli_version_matches(found: &str) -> bool {
    found.trim().trim_start_matches('v').trim() == EXPECTED_CLI_VERSION
}

/// Derive the one state from the probe results.
///
/// Order IS the contract: the first unanswered or missing link wins, so the
/// consumer always has exactly one next step. Because each `Unknown`/`No`
/// returns before the next link is read, probes that were never run (left
/// `Unknown`/`None` by [`probe_with`]) can never leak into the answer.
pub fn derive(distro: &str, probes: Probes) -> BackendStatus {
    let unknown_at = |step: SetupStep, probes: Probes| BackendStatus {
        state: SetupState::Unknown,
        blocked_at: Some(step),
        distro: distro.to_string(),
        expected_cli_version: EXPECTED_CLI_VERSION.to_string(),
        probes,
    };
    let settled = |state: SetupState, probes: Probes| BackendStatus {
        state,
        blocked_at: None,
        distro: distro.to_string(),
        expected_cli_version: EXPECTED_CLI_VERSION.to_string(),
        probes,
    };

    match probes.wsl {
        Tri::Unknown => return unknown_at(SetupStep::Wsl, probes),
        Tri::No => return settled(SetupState::NoWsl, probes),
        Tri::Yes => {}
    }
    match probes.distro {
        Tri::Unknown => return unknown_at(SetupStep::Distro, probes),
        Tri::No => return settled(SetupState::NoDistro, probes),
        Tri::Yes => {}
    }
    match probes.cli {
        Tri::Unknown => return unknown_at(SetupStep::Cli, probes),
        Tri::No => return settled(SetupState::NoCli, probes),
        Tri::Yes => {}
    }
    match probes.cli_version.as_deref() {
        // A `Yes` with no version means a classifier broke its own contract.
        // Refuse to call that Ready.
        None => return unknown_at(SetupStep::Cli, probes),
        Some(v) if !cli_version_matches(v) => return settled(SetupState::CliOutdated, probes),
        Some(_) => {}
    }
    match probes.titles {
        None => unknown_at(SetupStep::Titles, probes),
        Some(0) => settled(SetupState::NoTitles, probes),
        Some(_) => settled(SetupState::Ready, probes),
    }
}

// ---------------------------------------------------------------------------
// The chain
// ---------------------------------------------------------------------------

/// Run the whole chain against an injected runner. `run` receives the
/// arguments for `wsl.exe` and returns what happened.
///
/// SHORT-CIRCUITS. Each link is only asked when the one before it said `Yes`.
/// That is not just an optimisation: "is `dml` installed inside `dml-arch`"
/// has no honest answer when there is no `dml-arch`, and every skipped spawn
/// is one fewer [`SetupProbeEnv::timeout`] a user with a sick machine waits
/// through before the screen tells them anything.
pub fn probe_with(
    distro: &str,
    user: &str,
    mut run: impl FnMut(&[&str]) -> ProbeOutcome,
) -> BackendStatus {
    let wsl = classify_wsl_list(&run(&["--list", "--quiet"]), distro);
    let mut probes = Probes {
        wsl: wsl.wsl,
        distro: wsl.distro,
        cli: Tri::Unknown,
        cli_version: None,
        titles: None,
    };

    if probes.distro == Tri::Yes {
        let cli = classify_cli_version(&run(&[
            "-d", distro, "-u", user, "--", "dml", "version", "--json",
        ]));
        probes.cli = cli.cli;
        probes.cli_version = cli.version;

        // Only ask a CLI we can actually speak to. An outdated `dml` is
        // already a settled state, and its `games list` shape is not
        // guaranteed to be the one this launcher parses.
        let usable = probes.cli == Tri::Yes
            && probes.cli_version.as_deref().is_some_and(cli_version_matches);
        if usable {
            probes.titles = classify_titles(&run(&[
                "-d", distro, "-u", user, "--", "dml", "games", "list", "--json",
            ]));
        }
    }

    derive(distro, probes)
}

/// Where the probe chain spawns to.
#[derive(Debug, Clone)]
pub struct SetupProbeEnv {
    pub wsl_program: OsString,
    pub distro: String,
    pub user: String,
    /// Wall-clock bound per spawn.
    pub timeout: Duration,
}

impl SetupProbeEnv {
    pub fn new(distro: &str, user: &str) -> Self {
        SetupProbeEnv {
            wsl_program: OsString::from("wsl.exe"),
            distro: distro.to_string(),
            user: user.to_string(),
            timeout: DEFAULT_PROBE_TIMEOUT,
        }
    }
}

/// Run the chain for real.
pub fn probe(env: &SetupProbeEnv) -> BackendStatus {
    probe_with(&env.distro, &env.user, |args| {
        let mut cmd = std::process::Command::new(&env.wsl_program);
        cmd.args(args);
        windows_no_window(&mut cmd);
        ProbeOutcome::from_bounded(run_bounded_outcome(cmd, env.timeout))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DISTRO: &str = "dml-arch";
    const USER: &str = "dml";

    fn ran(code: i32, stdout: &str) -> ProbeOutcome {
        ProbeOutcome::Ran { code: Some(code), stdout: stdout.to_string(), stderr: String::new() }
    }

    fn ran_err(code: i32, stderr: &str) -> ProbeOutcome {
        ProbeOutcome::Ran { code: Some(code), stdout: String::new(), stderr: stderr.to_string() }
    }

    // -- classify_wsl_list ---------------------------------------------------

    #[test]
    fn wsl_list_missing_program_means_no_wsl_and_no_distro() {
        // wsl.exe genuinely absent is the ONE definitive negative.
        let got = classify_wsl_list(&ProbeOutcome::ProgramMissing, DISTRO);
        assert_eq!(got, WslProbe { wsl: Tri::No, distro: Tri::No });
    }

    #[test]
    fn wsl_list_could_not_tell_is_unknown_on_both_never_no() {
        // The tri-state rule: a hung/blocked wsl.exe proves nothing.
        let got = classify_wsl_list(&ProbeOutcome::CouldNotTell, DISTRO);
        assert_eq!(got, WslProbe { wsl: Tri::Unknown, distro: Tri::Unknown });
    }

    #[test]
    fn wsl_list_finds_the_distro_among_others() {
        let got = classify_wsl_list(&ran(0, "Ubuntu\r\ndml-arch\r\ndocker-desktop\r\n"), DISTRO);
        assert_eq!(got, WslProbe { wsl: Tri::Yes, distro: Tri::Yes });
    }

    #[test]
    fn wsl_list_tolerates_the_utf16_bom_wsl_emits() {
        // wsl.exe writes UTF-16LE; decode leaves a BOM on the first line, and
        // a naive `line == distro` would miss a distro listed first.
        let got = classify_wsl_list(&ran(0, "\u{feff}dml-arch\r\nUbuntu\r\n"), DISTRO);
        assert_eq!(got.distro, Tri::Yes);
    }

    #[test]
    fn wsl_list_present_but_distro_absent() {
        let got = classify_wsl_list(&ran(0, "Ubuntu\r\ndocker-desktop\r\n"), DISTRO);
        assert_eq!(got, WslProbe { wsl: Tri::Yes, distro: Tri::No });
    }

    #[test]
    fn wsl_list_does_not_match_a_distro_by_prefix() {
        // `dml-arch-old` is a different distro; matching it would send the
        // CLI probe into the wrong machine.
        let got = classify_wsl_list(&ran(0, "dml-arch-old\r\n"), DISTRO);
        assert_eq!(got.distro, Tri::No);
    }

    #[test]
    fn wsl_list_no_installed_distributions_is_wsl_yes_distro_no() {
        let got = classify_wsl_list(
            &ran_err(1, "Windows Subsystem for Linux has no installed distributions."),
            DISTRO,
        );
        assert_eq!(got, WslProbe { wsl: Tri::Yes, distro: Tri::No });
    }

    #[test]
    fn wsl_list_optional_component_not_enabled_is_no_wsl() {
        let got = classify_wsl_list(
            &ran_err(1, "The Windows Subsystem for Linux optional component is not enabled."),
            DISTRO,
        );
        assert_eq!(got, WslProbe { wsl: Tri::No, distro: Tri::No });
    }

    #[test]
    fn wsl_list_unrecognized_failure_is_unknown_not_no() {
        // The default must be a shrug. Anything else invents a diagnosis.
        let got = classify_wsl_list(&ran_err(1, "Error code: Wsl/Service/0x8007273f"), DISTRO);
        assert_eq!(got, WslProbe { wsl: Tri::Unknown, distro: Tri::Unknown });
    }

    // -- classify_cli_version ------------------------------------------------

    #[test]
    fn cli_version_read_from_the_ok_envelope() {
        let got = classify_cli_version(&ran(0, r#"{"ok":true,"data":{"version":"3.0.0"}}"#));
        assert_eq!(got, CliProbe { cli: Tri::Yes, version: Some("3.0.0".into()) });
    }

    #[test]
    fn cli_version_reports_an_old_cli_verbatim() {
        // The bootstrap CLI the elevated installer still embeds.
        let got = classify_cli_version(&ran(0, r#"{"ok":true,"data":{"version":"2.6.0"}}"#));
        assert_eq!(got, CliProbe { cli: Tri::Yes, version: Some("2.6.0".into()) });
    }

    #[test]
    fn cli_missing_inside_the_distro_is_a_definitive_no() {
        let got = classify_cli_version(&ran_err(127, "bash: dml: command not found"));
        assert_eq!(got, CliProbe { cli: Tri::No, version: None });
    }

    #[test]
    fn cli_could_not_tell_is_unknown() {
        let got = classify_cli_version(&ProbeOutcome::CouldNotTell);
        assert_eq!(got, CliProbe { cli: Tri::Unknown, version: None });
    }

    #[test]
    fn cli_garbage_output_is_unknown_not_no() {
        // Something answered but not in our contract. Absence is not proven.
        //
        // This used to assert on "dml v3.0.0" as its garbage sample. That was
        // wrong once we learned the bootstrap CLI (v2.6.0, no --json at all)
        // answers in exactly that shape: a `dml vX.Y.Z` banner is now PROOF a
        // dml is installed, and treating it as unreadable is what dead-ended
        // the first-run screen. The Unknown contract still holds for output
        // that is genuinely not a dml banner, which is what it now uses.
        let got = classify_cli_version(&ran(0, "some other program v3.0.0"));
        assert_eq!(got, CliProbe { cli: Tri::Unknown, version: None });
    }

    #[test]
    fn the_inbox_wsl_stub_saying_not_installed_beats_its_own_exit_zero() {
        // Windows' inbox wsl.exe exits 0 while printing that WSL is missing.
        // Believing the exit code here would report a WORKING WSL with no
        // distro, and the first-run screen would send the user to create a
        // distro on a machine that cannot host one. Same lie, same evening,
        // as the Install-DML.ps1 bug (f304629).
        let got = classify_wsl_list(
            &ran(0, "The Windows Subsystem for Linux is not installed. You can install by running 'wsl.exe --install'."),
            "dml-arch",
        );
        assert_eq!(got, WslProbe { wsl: Tri::No, distro: Tri::No });
    }

    #[test]
    fn a_plain_text_banner_is_not_garbage() {
        // The counterpart to the test above, stated explicitly so the two
        // cannot drift back together.
        assert_eq!(
            classify_cli_version(&ran(0, "dml v3.0.0")),
            CliProbe { cli: Tri::Yes, version: Some("3.0.0".to_string()) }
        );
    }

    #[test]
    fn cli_error_envelope_is_unknown_not_no() {
        let got = classify_cli_version(&ran(1, r#"{"ok":false,"error":{"code":"X","message":"y"}}"#));
        assert_eq!(got.cli, Tri::Unknown);
    }

    // -- cli_version_matches -------------------------------------------------

    #[test]
    fn version_match_is_exact_but_forgives_v_prefix_and_whitespace() {
        assert!(cli_version_matches("3.0.0"));
        assert!(cli_version_matches("v3.0.0"));
        assert!(cli_version_matches("  3.0.0\n"));
        assert!(!cli_version_matches("2.6.0"));
        assert!(!cli_version_matches("3.0.1"));
        assert!(!cli_version_matches(""));
    }

    // -- classify_titles -----------------------------------------------------

    #[test]
    fn titles_counted_from_the_games_array() {
        let got = classify_titles(&ran(
            0,
            r#"{"ok":true,"data":{"games":[{"id":"a"},{"id":"b"}]}}"#,
        ));
        assert_eq!(got, Some(2));
    }

    #[test]
    fn titles_empty_array_is_zero_not_could_not_tell() {
        // Zero is a real answer with a real next step (install a title).
        assert_eq!(classify_titles(&ran(0, r#"{"ok":true,"data":{"games":[]}}"#)), Some(0));
    }

    #[test]
    fn titles_could_not_tell_stays_none() {
        assert_eq!(classify_titles(&ProbeOutcome::CouldNotTell), None);
        assert_eq!(classify_titles(&ran(0, "not json")), None);
        assert_eq!(
            classify_titles(&ran(1, r#"{"ok":false,"error":{"code":"X","message":"y"}}"#)),
            None
        );
    }

    // -- derive: every combination, in chain order ---------------------------

    fn p(wsl: Tri, distro: Tri, cli: Tri, version: Option<&str>, titles: Option<usize>) -> Probes {
        Probes { wsl, distro, cli, cli_version: version.map(str::to_string), titles }
    }

    #[test]
    fn derive_no_wsl_wins_first() {
        let got = derive(DISTRO, p(Tri::No, Tri::No, Tri::Unknown, None, None));
        assert_eq!(got.state, SetupState::NoWsl);
        assert_eq!(got.blocked_at, None);
    }

    #[test]
    fn derive_unknown_wsl_is_unknown_blocked_at_wsl_not_no_wsl() {
        // The whole point: a screen that says "WSL is missing, run the
        // installer" because wsl.exe was busy is a lie with a button on it.
        let got = derive(DISTRO, p(Tri::Unknown, Tri::Unknown, Tri::Unknown, None, None));
        assert_eq!(got.state, SetupState::Unknown);
        assert_eq!(got.blocked_at, Some(SetupStep::Wsl));
    }

    #[test]
    fn derive_no_distro_when_wsl_is_there() {
        let got = derive(DISTRO, p(Tri::Yes, Tri::No, Tri::Unknown, None, None));
        assert_eq!(got.state, SetupState::NoDistro);
    }

    #[test]
    fn derive_unknown_distro_is_unknown_blocked_at_distro() {
        let got = derive(DISTRO, p(Tri::Yes, Tri::Unknown, Tri::Unknown, None, None));
        assert_eq!(got.state, SetupState::Unknown);
        assert_eq!(got.blocked_at, Some(SetupStep::Distro));
    }

    #[test]
    fn derive_no_cli_when_the_distro_is_there() {
        let got = derive(DISTRO, p(Tri::Yes, Tri::Yes, Tri::No, None, None));
        assert_eq!(got.state, SetupState::NoCli);
    }

    #[test]
    fn derive_unknown_cli_is_unknown_blocked_at_cli() {
        let got = derive(DISTRO, p(Tri::Yes, Tri::Yes, Tri::Unknown, None, None));
        assert_eq!(got.state, SetupState::Unknown);
        assert_eq!(got.blocked_at, Some(SetupStep::Cli));
    }

    #[test]
    fn derive_outdated_cli_is_its_own_state() {
        let got = derive(DISTRO, p(Tri::Yes, Tri::Yes, Tri::Yes, Some("2.6.0"), Some(1)));
        assert_eq!(got.state, SetupState::CliOutdated);
        assert_eq!(got.probes.cli_version.as_deref(), Some("2.6.0"));
    }

    #[test]
    fn derive_cli_yes_without_a_version_is_unknown_not_ready() {
        // Defensive: `Yes` is only ever set alongside a parsed version, so a
        // versionless Yes means something upstream lied. Never call it Ready.
        let got = derive(DISTRO, p(Tri::Yes, Tri::Yes, Tri::Yes, None, Some(1)));
        assert_eq!(got.state, SetupState::Unknown);
        assert_eq!(got.blocked_at, Some(SetupStep::Cli));
    }

    #[test]
    fn derive_unknown_titles_is_unknown_not_no_titles() {
        // Offering "install your first title" to someone who already has a
        // server is the same class of lie as the WSL one above.
        let got = derive(DISTRO, p(Tri::Yes, Tri::Yes, Tri::Yes, Some("3.0.0"), None));
        assert_eq!(got.state, SetupState::Unknown);
        assert_eq!(got.blocked_at, Some(SetupStep::Titles));
    }

    #[test]
    fn derive_no_titles_when_everything_else_is_provisioned() {
        let got = derive(DISTRO, p(Tri::Yes, Tri::Yes, Tri::Yes, Some("3.0.0"), Some(0)));
        assert_eq!(got.state, SetupState::NoTitles);
    }

    #[test]
    fn derive_ready_is_the_only_all_green_answer() {
        let got = derive(DISTRO, p(Tri::Yes, Tri::Yes, Tri::Yes, Some("3.0.0"), Some(2)));
        assert_eq!(got.state, SetupState::Ready);
        assert_eq!(got.blocked_at, None);
        assert_eq!(got.distro, DISTRO);
        assert_eq!(got.expected_cli_version, EXPECTED_CLI_VERSION);
        assert_eq!(got.probes.titles, Some(2));
    }

    #[test]
    fn derive_blocked_at_is_only_ever_set_for_unknown() {
        for probes in [
            p(Tri::No, Tri::No, Tri::Unknown, None, None),
            p(Tri::Yes, Tri::No, Tri::Unknown, None, None),
            p(Tri::Yes, Tri::Yes, Tri::No, None, None),
            p(Tri::Yes, Tri::Yes, Tri::Yes, Some("2.6.0"), Some(1)),
            p(Tri::Yes, Tri::Yes, Tri::Yes, Some("3.0.0"), Some(0)),
            p(Tri::Yes, Tri::Yes, Tri::Yes, Some("3.0.0"), Some(3)),
        ] {
            let got = derive(DISTRO, probes.clone());
            assert_ne!(got.state, SetupState::Unknown, "{probes:?}");
            assert_eq!(got.blocked_at, None, "{probes:?}");
        }
    }

    // -- probe_with: the chain, with a recording fake ------------------------

    struct Fake {
        calls: std::cell::RefCell<Vec<Vec<String>>>,
        replies: std::cell::RefCell<Vec<ProbeOutcome>>,
    }

    impl Fake {
        fn new(replies: Vec<ProbeOutcome>) -> Self {
            Fake {
                calls: std::cell::RefCell::new(Vec::new()),
                replies: std::cell::RefCell::new(replies),
            }
        }
        fn run(&self, args: &[&str]) -> ProbeOutcome {
            self.calls.borrow_mut().push(args.iter().map(|s| s.to_string()).collect());
            let mut r = self.replies.borrow_mut();
            if r.is_empty() {
                ProbeOutcome::CouldNotTell
            } else {
                r.remove(0)
            }
        }
        fn calls(&self) -> Vec<Vec<String>> {
            self.calls.borrow().clone()
        }
    }

    #[test]
    fn chain_asks_wsl_list_first() {
        let fake = Fake::new(vec![ProbeOutcome::ProgramMissing]);
        let _ = probe_with(DISTRO, USER, |a| fake.run(a));
        let calls = fake.calls();
        assert_eq!(calls.first().map(Vec::as_slice), Some(&["--list".to_string(), "--quiet".to_string()][..]));
    }

    #[test]
    fn chain_stops_at_the_first_missing_link_and_spawns_nothing_further() {
        // Asking what is inside a distro that does not exist has no honest
        // answer, and each extra spawn is another `timeout` the user waits.
        let fake = Fake::new(vec![ProbeOutcome::ProgramMissing]);
        let got = probe_with(DISTRO, USER, |a| fake.run(a));
        assert_eq!(got.state, SetupState::NoWsl);
        assert_eq!(fake.calls().len(), 1, "no probe may run past a missing link");
    }

    #[test]
    fn chain_does_not_probe_the_cli_when_the_distro_is_absent() {
        let fake = Fake::new(vec![ran(0, "Ubuntu\r\n")]);
        let got = probe_with(DISTRO, USER, |a| fake.run(a));
        assert_eq!(got.state, SetupState::NoDistro);
        assert_eq!(fake.calls().len(), 1);
    }

    #[test]
    fn chain_runs_the_cli_probe_inside_the_distro_as_the_dml_user() {
        let fake = Fake::new(vec![
            ran(0, "dml-arch\r\n"),
            ran_err(127, "bash: dml: command not found"),
        ]);
        let got = probe_with(DISTRO, USER, |a| fake.run(a));
        assert_eq!(got.state, SetupState::NoCli);
        let calls = fake.calls();
        assert_eq!(calls.len(), 2, "the titles probe must not run without a CLI");
        assert_eq!(
            calls[1],
            vec!["-d", DISTRO, "-u", USER, "--", "dml", "version", "--json"]
        );
    }

    #[test]
    fn chain_skips_the_titles_probe_when_the_cli_is_outdated() {
        let fake = Fake::new(vec![
            ran(0, "dml-arch\r\n"),
            ran(0, r#"{"ok":true,"data":{"version":"2.6.0"}}"#),
        ]);
        let got = probe_with(DISTRO, USER, |a| fake.run(a));
        assert_eq!(got.state, SetupState::CliOutdated);
        assert_eq!(fake.calls().len(), 2);
    }

    #[test]
    fn chain_runs_all_four_and_reports_ready() {
        let fake = Fake::new(vec![
            ran(0, "dml-arch\r\n"),
            ran(0, r#"{"ok":true,"data":{"version":"3.0.0"}}"#),
            ran(0, r#"{"ok":true,"data":{"games":[{"id":"wow-server-playerbots"}]}}"#),
        ]);
        let got = probe_with(DISTRO, USER, |a| fake.run(a));
        assert_eq!(got.state, SetupState::Ready);
        let calls = fake.calls();
        assert_eq!(calls.len(), 3);
        assert_eq!(
            calls[2],
            vec!["-d", DISTRO, "-u", USER, "--", "dml", "games", "list", "--json"]
        );
    }

    #[test]
    fn chain_reports_no_titles_on_a_provisioned_but_empty_distro() {
        let fake = Fake::new(vec![
            ran(0, "dml-arch\r\n"),
            ran(0, r#"{"ok":true,"data":{"version":"3.0.0"}}"#),
            ran(0, r#"{"ok":true,"data":{"games":[]}}"#),
        ]);
        let got = probe_with(DISTRO, USER, |a| fake.run(a));
        assert_eq!(got.state, SetupState::NoTitles);
    }

    // -- probe: the real spawn, bounded --------------------------------------

    #[test]
    fn probe_against_a_missing_wsl_exe_says_no_wsl_and_returns_at_once() {
        // Boundedness half 1: a program that is not there must not cost a
        // timeout, and must land on NoWsl (definitive), never Unknown.
        let mut env = SetupProbeEnv::new(DISTRO, USER);
        env.wsl_program = OsString::from("definitely-not-a-real-wsl-9f2.exe");
        env.timeout = Duration::from_secs(30);
        let start = std::time::Instant::now();
        let got = probe(&env);
        let elapsed = start.elapsed();
        assert_eq!(got.state, SetupState::NoWsl);
        assert!(
            elapsed < Duration::from_secs(5),
            "a missing program must fail fast, took {elapsed:?}"
        );
    }

    #[test]
    fn probe_outcome_maps_a_timeout_to_could_not_tell_not_missing() {
        assert_eq!(
            ProbeOutcome::from_bounded(BoundedOutcome::TimedOut),
            ProbeOutcome::CouldNotTell
        );
        assert_eq!(
            ProbeOutcome::from_bounded(BoundedOutcome::SpawnFailed(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "nope"
            ))),
            ProbeOutcome::CouldNotTell
        );
        assert_eq!(
            ProbeOutcome::from_bounded(BoundedOutcome::SpawnFailed(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "nope"
            ))),
            ProbeOutcome::ProgramMissing
        );
    }

    // --- the bootstrap CLI the elevated installer actually ships -----------

    #[test]
    fn plain_text_version_is_an_old_cli_not_an_unknown_one() {
        // VERBATIM output of the v2.6.0 CLI that Install-DML.ps1 base64-installs:
        // its `version)` arm is `echo "dml v$VERSION"` and it has no --json flag.
        // Classifying this as Unknown sent a stranger to the dead-end screen.
        let out = ProbeOutcome::Ran {
            code: Some(0),
            stdout: "dml v2.6.0
".to_string(),
            stderr: String::new(),
        };
        let probe = classify_cli_version(&out);
        assert_eq!(probe.cli, Tri::Yes, "an answering dml is present, just old");
        assert_eq!(probe.version.as_deref(), Some("2.6.0"));
    }

    #[test]
    fn plain_text_version_survives_noise_before_the_banner() {
        // A `?` on strip_prefix would abandon the scan on line 1 and report
        // Unknown -- the exact bug this test exists to keep out.
        let out = ProbeOutcome::Ran {
            code: Some(0),
            stdout: "
warning: something chatty
dml v2.6.0
".to_string(),
            stderr: String::new(),
        };
        assert_eq!(classify_cli_version(&out).version.as_deref(), Some("2.6.0"));
    }

    #[test]
    fn plain_text_scan_does_not_invent_a_version_from_unrelated_output() {
        for noise in ["dml: some error
", "hello world
", "dml 
", "dmlv2
"] {
            let out = ProbeOutcome::Ran {
                code: Some(0),
                stdout: noise.to_string(),
                stderr: String::new(),
            };
            assert_eq!(
                classify_cli_version(&out).cli,
                Tri::Unknown,
                "must not read a version out of {noise:?}"
            );
        }
    }

    #[test]
    fn an_old_cli_derives_the_state_that_offers_the_upgrade_button() {
        // End to end: v2.6.0 present -> CliOutdated, which is the state whose
        // screen carries "Set up backend". Not Unknown, which carries nothing.
        let st = derive(
            "dml-arch",
            Probes {
                wsl: Tri::Yes,
                distro: Tri::Yes,
                cli: Tri::Yes,
                cli_version: Some("2.6.0".to_string()),
                titles: Some(1),
            },
        );
        assert_eq!(st.state, SetupState::CliOutdated, "an old CLI must offer the upgrade, not a shrug");
        assert_eq!(st.blocked_at, None, "a known-old CLI is not a could-not-tell");
    }

}
