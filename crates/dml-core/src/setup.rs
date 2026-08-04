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
//! BOUNDEDNESS, AND WHY IT IS TWO NUMBERS. Every spawn goes through
//! [`dml_core::proc::run_bounded_outcome`]: a missing wsl.exe returns
//! instantly, a hung one is killed and reaped at the deadline. But the probes
//! do not cost the same. The host-side distro list boots nothing and gets
//! [`DEFAULT_PROBE_TIMEOUT`]; the first call INTO the distro boots the WSL2 VM
//! and gets [`DEFAULT_COLD_START_TIMEOUT`] (see [`ProbeBudget`]). Spending the
//! short budget on the cold call is how a perfectly good machine gets reported
//! as broken; spending the long one on every call is how a broken machine
//! takes six minutes to say so.
//!
//! The chain also short-circuits — probes 3 and 4 are never spawned when the
//! distro is absent, because a question about the inside of a distro that does
//! not exist has no honest answer.

use std::ffi::OsString;
use std::time::Duration;

use serde::Serialize;

use crate::compose::COMPOSE_FILE_CANDIDATES;
use crate::engine::{systemd_is_active_argv, SYSTEMCTL_PROGRAM};
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

/// Default wall-clock bound for a probe spawn that boots NOTHING —
/// `wsl --list --quiet` is a host-side registry read, and `dml games list`
/// only runs after the distro has already answered once. If either of those
/// takes 20 seconds, something really is wrong.
pub const DEFAULT_PROBE_TIMEOUT: Duration = Duration::from_secs(20);

/// Default wall-clock bound for the ONE spawn that may pay for a cold start.
///
/// THE PROBES ARE NOT ALL EQUAL. `wsl -d dml-arch -u dml -- dml version --json`
/// is the first call that enters the distro, so it is the call that boots the
/// WSL2 utility VM — and `Install-DML.ps1` writes `/etc/wsl.conf` with
/// `[boot] systemd=true`, so that boot includes bringing systemd (and dockerd)
/// up inside it. After a Windows reboot that routinely runs past 20 seconds.
///
/// The number is not a guess, it is this project's own measured budget for the
/// identical spawn: `provision.rs`'s `DEFAULT_SETUP_TIMEOUT` is 120s ("the
/// FIRST `wsl.exe` call on a cold machine boots the WSL2 VM, which can take
/// the better part of a minute") and `Install-DML.ps1` allows systemd a
/// further `timeout 60` to reach running/degraded. A probe that GATES Home
/// cannot be stingier than the command it gates: the cost of overrunning is
/// not a slow screen, it is an established user's working Home being replaced
/// by "Couldn't check this PC's setup", with a Check again button that costs
/// another full budget each press.
///
/// This is deliberately NOT applied to every spawn. A genuinely absent
/// `wsl.exe` still answers instantly — that is a `NotFound` spawn error, not a
/// timeout, so no budget is spent at all — and a wedged host-side `wsl.exe`
/// still fails at [`DEFAULT_PROBE_TIMEOUT`] rather than making a broken
/// machine wait six minutes to be told so.
pub const DEFAULT_COLD_START_TIMEOUT: Duration = Duration::from_secs(120);

/// Which budget a given probe spawn is entitled to.
///
/// The chain hands one of these to its runner per call, so "how long may this
/// wait" is a property of WHICH QUESTION is being asked, not of the runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeBudget {
    /// This spawn may have to cold-start the WSL2 VM (and systemd inside the
    /// distro). Exactly one probe in the chain is in this position: the first
    /// one that runs a command INSIDE the distro.
    ColdStart,
    /// Everything else — the host-side distro list, and any call made after
    /// the distro has already answered. A slow answer here is a real fault.
    Warm,
}

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
    /// Native chain: is Docker Desktop installed on this PC?
    Docker,
    /// Native chain: is its engine actually up?
    Engine,
    /// ARCH chain: is the registered distro actually PREPARED — packages
    /// installed, the `dml` user created, sudoers written? Sits between
    /// `Distro` and `Engine`; see [`SetupState::DistroUnprepared`].
    Prepared,
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
    /// ARCH chain only: the distro is REGISTERED but was never finished —
    /// no docker, and (usually) no `dml` user either.
    ///
    /// This state exists because provisioning is eight steps and can die in
    /// the middle of them: a dead mirror on `pacman -Syu`, a reboot, a closed
    /// lid. The distro is created at step 2, so what is left afterwards is a
    /// registered name and nothing else. Without this state the chain went
    /// straight from `Distro` to the dockerd probe, which ran
    /// `systemctl is-active` as a user that does not exist yet, got a non-zero
    /// exit for a reason that had nothing to do with docker, and offered
    /// "start the container engine" on a distro that HAS no container engine.
    /// `NoDistro` could never be reached to say otherwise — the distro really
    /// is registered. The repair is to run provisioning again.
    DistroUnprepared,
    /// The distro is there but has no `dml`. THIS the launcher fixes, from
    /// its own bundled resources.
    NoCli,
    /// A `dml` is installed but is not the contract version (the common case
    /// on a fresh substrate install, which bootstraps an old CLI). Same fix
    /// as `NoCli`.
    CliOutdated,
    /// NATIVE chain only: Docker Desktop is not on this PC. The launcher does
    /// not install it — it is a separate download with its own licence terms.
    NoDocker,
    /// NATIVE chain only: Docker Desktop is installed but its engine is not
    /// running. THIS the launcher fixes, by starting the engine.
    DockerStopped,
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
    /// Verbatim words of the probe that went dark, if any — the CARRIER only.
    /// Deliberately not on the wire: [`derive`] copies it to
    /// [`BackendStatus::detail`], which is the one the screen reads. Two
    /// copies of the same string in one payload is how a UI ends up reading
    /// the wrong one.
    #[serde(skip)]
    pub detail: Option<String>,
}

impl Probes {
    /// All-unknown: nothing has been asked yet.
    pub fn unprobed() -> Self {
        Probes {
            wsl: Tri::Unknown,
            distro: Tri::Unknown,
            cli: Tri::Unknown,
            cli_version: None,
            titles: None,
            detail: None,
        }
    }
}

/// The typed answer both the first-run screen and the setup command consume.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BackendStatus {
    /// The decision. Switch on this.
    pub state: SetupState,
    /// Set only when `state` is [`SetupState::Unknown`].
    pub blocked_at: Option<SetupStep>,
    /// What the probe that went dark actually SAID, verbatim and bounded to one
    /// line. Set only alongside `blocked_at`, for the same reason: a settled
    /// state already has a written sentence, and a raw `wsl.exe` line under it
    /// would only make a working screen look broken.
    ///
    /// This exists because a could-not-tell is the ONE screen with no repair on
    /// it. Without the message, a non-English Windows — or any wording
    /// Microsoft changes next month — becomes a permanent "Check again" loop
    /// with nothing for the user OR a bug report to go on.
    pub detail: Option<String>,
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
            // DECODED HERE, ONCE. wsl.exe's own messages are UTF-16LE on
            // Windows; `decode_wsl_output` sniffs that and is lossy on both
            // paths, so a half-read pipe or a stray byte can never panic a
            // diagnostic. Everything downstream — matching, and the `detail`
            // the screen shows — therefore works on real text.
            BoundedOutcome::Ran(out) => ProbeOutcome::Ran {
                code: out.status.code(),
                stdout: decode_wsl_output(&out.stdout),
                stderr: decode_wsl_output(&out.stderr),
            },
        }
    }

    /// This outcome's own words, for the could-not-tell screen. `None` when
    /// there is nothing to quote: a program that is not installed said nothing,
    /// and neither did one that was killed at its deadline. Inventing a
    /// sentence for those would be the same guessing this module refuses to do
    /// everywhere else.
    pub fn detail(&self) -> Option<String> {
        match self {
            ProbeOutcome::ProgramMissing | ProbeOutcome::CouldNotTell => None,
            ProbeOutcome::Ran { stdout, stderr, .. } => first_diagnostic_line(stdout, stderr),
        }
    }
}

/// One line of a probe's output for the screen: prefers stderr (where wsl.exe
/// and the shell put their complaints), falls back to stdout, and bounds it —
/// a multi-KB dump under a button helps nobody, and the screen has one line.
pub fn first_diagnostic_line(stdout: &str, stderr: &str) -> Option<String> {
    let pick = |s: &str| {
        s.lines().map(str::trim).find(|l| !l.is_empty()).map(|l| {
            if l.chars().count() > 200 {
                format!("{}...", l.chars().take(200).collect::<String>())
            } else {
                l.to_string()
            }
        })
    };
    pick(stderr).or_else(|| pick(stdout))
}

/// Probe 1+2's answer: one `wsl --list --quiet` call settles both.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WslProbe {
    pub wsl: Tri,
    pub distro: Tri,
    /// Set only alongside a `Tri::Unknown`: what wsl.exe actually said.
    pub detail: Option<String>,
}

/// Probe 3's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliProbe {
    pub cli: Tri,
    pub version: Option<String>,
    /// Set only alongside `Tri::Unknown`: what the distro actually said.
    pub detail: Option<String>,
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

/// What a `wsl.exe` message we RECOGNISE means, as `(wsl, distro)`. `None` is
/// "nothing here we can honestly claim to understand".
///
/// Deliberately few, and every one of them a sentence Microsoft actually
/// prints. A phrase we cannot cite is a guess, and a guess here puts a repair
/// button in front of a machine that is merely unhappy.
fn recognised_message(text: &str) -> Option<(Tri, Tri)> {
    let text = text.to_lowercase();
    if text.contains("is not installed")
        || text.contains("optional component is not enabled")
        || text.contains("has not been enabled")
        || text.contains("0x8007019e")
    {
        // No WSL — so no distro either.
        return Some((Tri::No, Tri::No));
    }
    if text.contains("no installed distributions") {
        // WSL itself works; the machine simply has nothing registered.
        return Some((Tri::Yes, Tri::No));
    }
    None
}

/// Punctuation a registered distro name legitimately carries
/// (`Ubuntu-22.04`, `openSUSE-Leap-15.5`, `SUSE_Linux(preview)`). Everything
/// outside this set, **in any script**, is a sentence's furniture: `。`, `、`,
/// `！`, `«`, `"`, `,`, `;`.
const NAME_PUNCT: &[char] = &['-', '_', '.', '+', '(', ')'];

/// Does this cleaned stdout line look like a registered distro NAME, as
/// opposed to a sentence `wsl.exe` printed at us?
///
/// WHITESPACE CANNOT BE THE TEST. `wsl --import "Ubuntu 22.04" ...` is legal,
/// the Store registers "Ubuntu 20.04"/"Ubuntu 22.04" verbatim, and "Arch
/// Linux" is an ordinary name in this project's own audience — so "every line
/// is a single token" declares a perfectly readable list unreadable, which is
/// one of the two regressions this function has already had.
///
/// AND ASCII SHAPE CANNOT BE THE TEST EITHER — the other one. "short, few
/// SPACE-separated words, does not end in `.` or `:`" describes an English
/// label, and a Japanese or Chinese wsl.exe satisfies all three by accident:
/// its full stop is `。`, and it spends no spaces at all, so a whole sentence
/// counts as ONE word. That read a localized "WSL is not installed" as a
/// one-entry distro list and answered `wsl=Yes, distro=No` — a confident
/// "just create the distro" to a machine with no WSL on it.
///
/// So the shape test is asked in a script-agnostic way:
///
/// * **No punctuation a label would not carry** (see [`NAME_PUNCT`]). This is
///   what catches `。`/`！`/`、` without keeping a list of the world's full
///   stops, and it keeps the ASCII terminator rule as the special case it
///   always was (`.` is legal *inside* `Ubuntu 22.04`, never at the end).
/// * **No caseless letters.** The word budget below counts spaces, and only
///   scripts that spend spaces on word boundaries can be counted that way.
///   "Has this letter no upper- and no lower-case form" is std's cheapest
///   proxy for "a script we cannot count words in": it catches CJK, kana,
///   Hangul and Thai, and over-catches Arabic/Hebrew/Devanagari, which do use
///   spaces. That over-catch costs a name in one of those scripts a
///   [`Tri::Unknown`] instead of a `NoDistro` — the tri-state's safe side, and
///   the honest answer for a line whose word count we are guessing at. Cased
///   scripts — Latin, Greek, Cyrillic — are untouched, so "Min Grønne Distro"
///   is still read as the name it is.
///
/// This is a heuristic and it is only ever allowed to decide between "the
/// distro is absent" and "we could not tell" — never to contradict the name
/// being listed (checked first, below). Every `false` it returns lands on
/// [`SetupState::Unknown`], which is why it may be strict and may not be
/// generous.
fn looks_like_a_distro_name(name: &str) -> bool {
    if name.is_empty() || name.chars().count() > 64 {
        return false;
    }
    if name.ends_with('.') || name.ends_with(':') {
        return false;
    }
    for c in name.chars() {
        if c.is_whitespace() || c.is_numeric() {
            continue;
        }
        if c.is_alphabetic() {
            if !c.is_lowercase() && !c.is_uppercase() {
                return false; // caseless script: no word boundaries to count
            }
            continue;
        }
        if !NAME_PUNCT.contains(&c) {
            return false;
        }
    }
    name.split_whitespace().count() <= 5
}

/// Classify `wsl.exe --list --quiet`, which settles probes 1 and 2 at once.
///
/// READ WHAT IT SAID, NOT WHAT IT RETURNED. Windows ships an inbox `wsl.exe`
/// stub on every machine; with WSL not installed it prints "The Windows
/// Subsystem for Linux is not installed" and STILL EXITS 0. That is the exact
/// lie that broke `Install-DML.ps1` on a clean Windows 11 VM (f304629), and it
/// generalises: the exit code is not evidence for ANY of these messages, so
/// every message we claim to recognise is matched BEFORE the exit code is
/// consulted at all. The exit code's only remaining job is to corroborate
/// "and this stdout is a distro list".
///
/// WHY THE STREAMS ARE READ SEPARATELY, AND IN THIS ORDER. Merging them into
/// one blob throws away the only structure this output has:
///
///   * **stdout is the ANSWER SLOT.** `--list --quiet` writes the registered
///     names there — and the inbox stub writes its "not installed" banner
///     there too. So whatever occupies stdout is wsl.exe's answer to the
///     question we asked, and it is read first.
///   * **stderr is COMMENTARY.** A healthy WSL emits advisory lines there
///     routinely (localhost-proxy notices, console warnings), and "is not
///     installed" is a fragment about *something* — not always about WSL
///     ("wsl: Docker Desktop is not installed."). Commentary may settle the
///     question only when the answer slot did not.
///
/// Hence: (1) the name we asked about, literally listed, beats everything —
/// no sentence on either stream can make a distro that wsl.exe just enumerated
/// stop existing, and no exit code can either; (2) a recognised message *in
/// the answer slot* beats reading that slot as a list, because a message is
/// not a list and splitting an unrecognised complaint into lines is how a
/// machine with no WSL at all got told to go create a distro; (3) a readable
/// list beats a recognised message on stderr, because the list is proof and
/// the commentary is about something else; (4) only then does stderr get to
/// settle it.
///
/// The fall-through is where the tri-state discipline lives: ANYTHING ELSE is
/// a shrug that carries its own text out (`detail`) so the screen has
/// something to show. Guessing here would put a repair button in front of a
/// user whose machine is merely unhappy — or, worse, offer to create a distro
/// on a machine that cannot host one.
pub fn classify_wsl_list(outcome: &ProbeOutcome, distro: &str) -> WslProbe {
    let settled = |wsl, d| WslProbe { wsl, distro: d, detail: None };
    let shrug =
        || WslProbe { wsl: Tri::Unknown, distro: Tri::Unknown, detail: outcome.detail() };
    match outcome {
        // No wsl.exe on the machine at all: WSL is not installed, and there
        // is therefore no distro either.
        ProbeOutcome::ProgramMissing => settled(Tri::No, Tri::No),
        ProbeOutcome::CouldNotTell => shrug(),
        ProbeOutcome::Ran { code, stdout, stderr } => {
            let lines: Vec<&str> =
                stdout.lines().map(clean_line).filter(|l| !l.is_empty()).collect();

            // 1. POSITIVE EVIDENCE, and it outranks everything — including the
            //    exit code, which is not evidence, and including any message
            //    on either stream. The name we asked about is on a line of its
            //    own in the answer slot; nothing wsl.exe prints does that by
            //    accident. Compared against EVERY line (not just the
            //    name-shaped ones) so that a multi-word distro we were asked
            //    about is matched rather than filtered away.
            if lines.contains(&distro) {
                return settled(Tri::Yes, Tri::Yes);
            }
            // 2. A message we recognise, IN THE ANSWER SLOT. This is the inbox
            //    stub (exit 0 + "is not installed" on stdout) and it must beat
            //    step 3: a message is not a list.
            if let Some((wsl, d)) = recognised_message(stdout) {
                return settled(wsl, d);
            }
            // 3. stdout really is a list of names. Beats step 4 because the
            //    list is proof that WSL works, while stderr is commentary that
            //    may well be about something else entirely. Every non-empty
            //    line must be name-shaped: a stdout that mixes names with
            //    prose is one we cannot safely split, so it is a shrug.
            if *code == Some(0)
                && !lines.is_empty()
                && lines.iter().all(|l| looks_like_a_distro_name(l))
            {
                return settled(Tri::Yes, Tri::No);
            }
            // 4. Nothing in the answer slot settled it — now stderr may.
            if let Some((wsl, d)) = recognised_message(stderr) {
                return settled(wsl, d);
            }
            // 5. Silent success: a working wsl.exe with nothing registered.
            //    Requires real silence on BOTH streams; unexplained stderr
            //    next to an empty answer is not an empty list.
            if *code == Some(0) && lines.is_empty() && stderr.trim().is_empty() {
                return settled(Tri::Yes, Tri::No);
            }
            // 6. Unrecognised. Evidence of NOTHING — but say what it said.
            shrug()
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
            return CliProbe { cli: Tri::Unknown, version: None, detail: outcome.detail() }
        }
        ProbeOutcome::Ran { code, stdout, stderr } => (code, stdout, stderr),
    };
    if let Ok(env) = parse_envelope(stdout) {
        if env.ok {
            if let Some(v) = env.data.get("version").and_then(|v| v.as_str()) {
                let v = v.trim();
                if !v.is_empty() {
                    return CliProbe { cli: Tri::Yes, version: Some(v.to_string()), detail: None };
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
        return CliProbe { cli: Tri::No, version: None, detail: None };
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
        return CliProbe { cli: Tri::Yes, version: Some(v), detail: None };
    }
    // Unrecognised. A corrupt distro ("The distribution failed to start"), a
    // localized wsl.exe error, a `dml` that crashed — none of them are proof
    // of absence, and all of them are things the user can only act on if they
    // can SEE them.
    CliProbe { cli: Tri::Unknown, version: None, detail: outcome.detail() }
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
        // The could-not-tell screen is the one with no repair on it, so this
        // is the only place the raw text earns its space.
        detail: probes.detail.clone(),
        distro: distro.to_string(),
        expected_cli_version: EXPECTED_CLI_VERSION.to_string(),
        probes,
    };
    let settled = |state: SetupState, probes: Probes| BackendStatus {
        state,
        blocked_at: None,
        detail: None,
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
    mut run: impl FnMut(&[&str], ProbeBudget) -> ProbeOutcome,
) -> BackendStatus {
    // Host-side registry read: it does not start the VM, so it gets the short
    // budget and a dead WSL is reported fast.
    let wsl = classify_wsl_list(&run(&["--list", "--quiet"], ProbeBudget::Warm), distro);
    let mut probes = Probes {
        wsl: wsl.wsl,
        distro: wsl.distro,
        cli: Tri::Unknown,
        cli_version: None,
        titles: None,
        // The diagnostic always belongs to the link that went dark, and the
        // chain stops at the first one — so each step that runs OVERWRITES it,
        // and by construction only a step whose predecessor said `Yes` can.
        detail: wsl.detail,
    };

    if probes.distro == Tri::Yes {
        // THE cold-start call: the first thing that runs inside the distro, so
        // it is the one that pays for booting the WSL2 VM + systemd.
        let cli = classify_cli_version(&run(
            &["-d", distro, "-u", user, "--", "dml", "version", "--json"],
            ProbeBudget::ColdStart,
        ));
        probes.cli = cli.cli;
        probes.cli_version = cli.version;
        probes.detail = cli.detail;

        // Only ask a CLI we can actually speak to. An outdated `dml` is
        // already a settled state, and its `games list` shape is not
        // guaranteed to be the one this launcher parses.
        let usable = probes.cli == Tri::Yes
            && probes.cli_version.as_deref().is_some_and(cli_version_matches);
        if usable {
            // The distro just answered, so it is warm by construction.
            let out = run(
                &["-d", distro, "-u", user, "--", "dml", "games", "list", "--json"],
                ProbeBudget::Warm,
            );
            probes.titles = classify_titles(&out);
            if probes.titles.is_none() {
                probes.detail = out.detail();
            }
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
    /// Wall-clock bound for a spawn that boots nothing.
    pub timeout: Duration,
    /// Wall-clock bound for the one spawn that may cold-start the WSL2 VM.
    pub cold_timeout: Duration,
}

impl SetupProbeEnv {
    pub fn new(distro: &str, user: &str) -> Self {
        SetupProbeEnv {
            wsl_program: OsString::from("wsl.exe"),
            distro: distro.to_string(),
            user: user.to_string(),
            timeout: DEFAULT_PROBE_TIMEOUT,
            cold_timeout: DEFAULT_COLD_START_TIMEOUT,
        }
    }

    /// The wall clock a given probe is entitled to.
    pub fn budget(&self, budget: ProbeBudget) -> Duration {
        match budget {
            ProbeBudget::ColdStart => self.cold_timeout,
            ProbeBudget::Warm => self.timeout,
        }
    }
}

/// ONE bounded probe spawn, with the budget already resolved to a wall clock.
///
/// The only place in the chain that touches the OS, so it is also the seam a
/// caller substitutes to test its own wiring — see the launcher's
/// `backend_status_with`, which needs to prove that the budget it hands this
/// function is the cold-start one.
pub fn spawn_probe(program: &std::ffi::OsStr, args: &[&str], budget: Duration) -> ProbeOutcome {
    let mut cmd = std::process::Command::new(program);
    cmd.args(args);
    windows_no_window(&mut cmd);
    ProbeOutcome::from_bounded(run_bounded_outcome(cmd, budget))
}

/// Run the chain for real.
pub fn probe(env: &SetupProbeEnv) -> BackendStatus {
    probe_with(&env.distro, &env.user, |args, budget| {
        spawn_probe(&env.wsl_program, args, env.budget(budget))
    })
}

/// What the NATIVE chain needs to know. Facts in, decision out — same shape as
/// [`derive`], and pure for the same reason: the states a user can land in are
/// worth testing without owning a machine that happens to be in each of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeFacts {
    /// Is Docker Desktop on this PC? `Unknown` when the look-up itself failed.
    pub docker_installed: Tri,
    /// Is its engine answering? Only meaningful when `docker_installed` is
    /// `Yes`; the chain never reads it otherwise.
    pub engine_running: Tri,
    /// Installed titles. `None` is a could-not-tell, NOT zero — the distinction
    /// is the difference between "install your server" and "we could not look".
    pub titles: Option<usize>,
}

/// The native chain: Docker Desktop installed → engine up → a title installed.
///
/// The WSL chain asks about WSL, a distro and a `dml` binary, none of which
/// exist in native mode — which is why running it there produced an answer
/// (`no_wsl`) that would have told a user with a perfectly good Docker setup to
/// go install WSL. The frontend's defence was to show a native user NOTHING at
/// all, so a native machine with no server got a status card for a server that
/// does not exist: exactly the first-run problem this whole phase exists to
/// remove, still present for the backend the project is moving to.
///
/// Same discipline as [`derive`]: first missing link wins, so there is always
/// exactly one next step, and `Unknown` is never read as absence — a Docker
/// look-up that fell over offers a retry, never an "install Docker" button
/// aimed at someone who already has it.
pub fn derive_native(f: NativeFacts) -> BackendStatus {
    let probes = Probes {
        // The native chain answers none of these; leaving them Unknown is the
        // honest encoding. They are diagnostics only, and `derive_native` never
        // reads them back.
        wsl: Tri::Unknown,
        distro: Tri::Unknown,
        cli: Tri::Unknown,
        cli_version: None,
        titles: f.titles,
        detail: None,
    };
    let at = |state: SetupState, blocked_at: Option<SetupStep>| BackendStatus {
        state,
        blocked_at,
        detail: None,
        // Native mode has no distro. Naming one here would put "dml-arch" in
        // front of a user who has deliberately never had it.
        distro: String::new(),
        expected_cli_version: EXPECTED_CLI_VERSION.to_string(),
        probes: probes.clone(),
    };

    match f.docker_installed {
        Tri::Unknown => return at(SetupState::Unknown, Some(SetupStep::Docker)),
        Tri::No => return at(SetupState::NoDocker, None),
        Tri::Yes => {}
    }
    match f.engine_running {
        Tri::Unknown => return at(SetupState::Unknown, Some(SetupStep::Engine)),
        Tri::No => return at(SetupState::DockerStopped, None),
        Tri::Yes => {}
    }
    match f.titles {
        None => at(SetupState::Unknown, Some(SetupStep::Titles)),
        Some(0) => at(SetupState::NoTitles, None),
        Some(_) => at(SetupState::Ready, None),
    }
}

/// The JSON contract marker `dml-wow-cli`'s envelope reports
/// (`{"ok":true,"data":{"contract":"dml-json-v3",...}}`).
///
/// NOT the same axis as [`EXPECTED_CLI_VERSION`]. That constant is the BASH
/// CLI's own version string (`cli/src/00-head.sh`'s `VERSION`) and has no
/// relationship whatsoever to `dml-wow-cli`'s crate version (`Cargo.toml`
/// currently says `0.1.0`) — comparing the Arch binary against
/// `EXPECTED_CLI_VERSION` would either reject a perfectly good binary forever
/// or require hand-syncing two numbers that answer different questions,
/// which is a permanent trap. The binary carries its own wire-contract marker
/// instead, so THAT is what gets compared.
pub const EXPECTED_ARCH_CLI_CONTRACT: &str = "dml-json-v3";

/// Probe 3's answer on the ARCH chain. Distinct from [`CliProbe`] /
/// [`classify_cli_version`], which belong to the bash chain and read a
/// `version` field under a DIFFERENT env var contract; conflating the two
/// would corrupt whichever chain isn't being tested at the moment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchCliProbe {
    pub cli: Tri,
    /// The contract string the binary reported, whatever it was — including a
    /// MISMATCHED one, so [`derive_arch`] can settle on `CliOutdated` (an
    /// honest, actionable state) instead of a bare could-not-tell.
    pub contract: Option<String>,
    /// Set only alongside `Tri::Unknown`: what the distro actually said.
    pub detail: Option<String>,
}

/// Classify `dml-wow version` (no `--json` — the binary's clap parser rejects
/// an argument it does not define, and its envelope is unconditional either
/// way; verified live 2026-08-04).
///
/// `ProgramMissing` is the one definitive negative: the binary is not on this
/// machine, and "install `dml-wow`" is a real repair. Everything else that
/// could not be read (a spawn failure, unparseable stdout, `ok:false`) is a
/// shrug, same tri-state discipline as [`classify_cli_version`] — an
/// `ok:true` envelope with NO `contract` field is included in that shrug,
/// because a `dml-wow` old enough to omit the field entirely is a machine
/// state this classifier cannot honestly describe as "outdated" (it does not
/// know what contract, if any, that binary speaks) or "fine".
pub fn classify_arch_cli(outcome: &ProbeOutcome) -> ArchCliProbe {
    let (stdout, stderr) = match outcome {
        ProbeOutcome::ProgramMissing => {
            return ArchCliProbe { cli: Tri::No, contract: None, detail: None }
        }
        ProbeOutcome::CouldNotTell => {
            return ArchCliProbe { cli: Tri::Unknown, contract: None, detail: None }
        }
        ProbeOutcome::Ran { stdout, stderr, .. } => (stdout, stderr),
    };
    if let Ok(env) = parse_envelope(stdout) {
        if env.ok {
            if let Some(c) = env.data.get("contract").and_then(|v| v.as_str()) {
                let c = c.trim();
                if !c.is_empty() {
                    return ArchCliProbe { cli: Tri::Yes, contract: Some(c.to_string()), detail: None };
                }
            }
        }
    }
    // Unparseable, `ok:false`, or an `ok:true` envelope with no readable
    // `contract` — none of these prove the binary is ABSENT.
    ArchCliProbe { cli: Tri::Unknown, contract: None, detail: first_diagnostic_line(stdout, stderr) }
}

/// The in-distro question that separates "registered" from "prepared".
///
/// `command -v docker` is chosen over `test -f /etc/sudoers.d/99-dml` because
/// it is the thing the NEXT link actually needs: a distro whose sudoers
/// drop-in landed but whose `pacman -Syu` did not has no dockerd to ask about,
/// and `docker-group`/`docker-enable` are the last two first-boot steps, so
/// docker's presence is also the closest cheap proxy for "the sequence ran to
/// the end". It is one `sh -c` and no package database read.
pub const PREPARED_PROBE_SCRIPT: &str = "command -v docker";

/// Classify [`PREPARED_PROBE_SCRIPT`]'s answer.
///
/// Follows [`classify_wsl_list`]'s doctrine: read what the output SAID before
/// consulting the exit code. `command -v` prints the resolved path, so a
/// non-empty stdout at exit 0 is positive evidence; an exit 0 with nothing on
/// stdout is a shell that did not do what we asked, which is a shrug and not a
/// yes.
///
/// A NON-ZERO exit here is a definitive `No`, and that is deliberate even
/// though it also covers `wsl.exe` refusing to enter the distro as user `dml`
/// ("the user name or password is incorrect"). That is not a misreading — a
/// distro we cannot enter as the user provisioning creates IS unprepared, and
/// it is the exact half-built state this link exists to name. The
/// alternative, `Unknown`, offers a retry forever on a machine whose real
/// answer is "run setup again".
///
/// `ProgramMissing` stays `Unknown`: `wsl.exe` disappearing between two calls
/// says nothing about the distro.
pub fn classify_arch_prepared(outcome: &ProbeOutcome) -> Tri {
    match outcome {
        ProbeOutcome::ProgramMissing | ProbeOutcome::CouldNotTell => Tri::Unknown,
        ProbeOutcome::Ran { code, stdout, .. } => match code {
            Some(0) if !stdout.trim().is_empty() => Tri::Yes,
            Some(0) => Tri::Unknown,
            _ => Tri::No,
        },
    }
}

/// Classify `systemctl is-active --quiet docker`.
///
/// EXIT CODE 3 IS THE ONLY DEFINITIVE NO. It is systemd's (and LSB's)
/// documented "program is not running", and it is the answer a healthy distro
/// with a stopped unit gives. Every other non-zero code crosses a different
/// failure domain — `wsl.exe` not reaching the distro, the user not existing,
/// systemd not being PID 1, `systemctl` itself absent — and none of those are
/// repaired by "start the container engine". Reading them as `No` is what put
/// that button in front of a distro that has no docker at all; they are
/// `Unknown`, which offers a retry and carries the machine's own words.
pub fn classify_arch_dockerd(outcome: &ProbeOutcome) -> Tri {
    match outcome {
        // wsl.exe itself failing to spawn a SECOND time (Windows Update
        // replacing it mid-session, AV interference, anything that made the
        // very first `--list --quiet` call answer but this one not) says
        // nothing about dockerd.
        ProbeOutcome::ProgramMissing | ProbeOutcome::CouldNotTell => Tri::Unknown,
        ProbeOutcome::Ran { code, .. } => match code {
            Some(0) => Tri::Yes,
            Some(3) => Tri::No,
            _ => Tri::Unknown,
        },
    }
}

/// Classify the titles-count probe: a bare non-negative integer on stdout at
/// exit 0, nothing else. `None` (could-not-tell) covers a killed/failed spawn
/// AND stdout that is not a clean number — [`derive_arch`] turns that into
/// `Unknown` at [`SetupStep::Titles`], never into "zero titles".
pub fn classify_arch_titles(outcome: &ProbeOutcome) -> Option<usize> {
    match outcome {
        ProbeOutcome::Ran { code, stdout, .. } if *code == Some(0) => stdout.trim().parse().ok(),
        _ => None,
    }
}

/// What the ARCH chain needs to know, in chain order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchFacts {
    pub wsl: Tri,
    pub distro: Tri,
    /// Has the registered distro actually been PREPARED (packages, user,
    /// sudoers)? See [`SetupState::DistroUnprepared`] — this link exists
    /// because a provisioning run that dies at step 2 of 8 leaves a distro
    /// that is registered and nothing else.
    pub prepared: Tri,
    /// Is the in-distro `dockerd` unit active? `Unknown` when the question
    /// itself went unanswered.
    pub dockerd: Tri,
    /// Is the Rust `dml-wow` binary present and runnable inside the distro?
    pub cli: Tri,
    /// The wire contract the binary reported — see [`EXPECTED_ARCH_CLI_CONTRACT`].
    /// Despite the name overlap with the bash chain's `cli_version`, this is
    /// never a semver string.
    pub cli_contract: Option<String>,
    pub titles: Option<usize>,
    /// The verbatim words of whichever probe went dark.
    pub detail: Option<String>,
}

/// The Arch chain: WSL → distro → PREPARED → dockerd → `dml-wow` → a title.
///
/// Same discipline as [`derive`] and [`derive_native`]: the first unanswered or
/// missing link wins, so the consumer always has exactly one next step, and
/// `Unknown` is never read as absence. A dockerd that is merely stopped has a
/// repair (start the unit); a dockerd that did not answer has only a retry, and
/// putting the repair button in front of the second case is the exact mistake
/// the tri-state exists to prevent.
///
/// Reuses [`SetupState::DockerStopped`] and [`SetupStep::Engine`] rather than
/// minting Arch-specific twins: the user-facing sentence ("the container engine
/// is not running") and the repair ("start it") are the same on both backends,
/// and a second pair of states would mean every consumer grows a branch that
/// says the same thing twice.
pub fn derive_arch(distro: &str, f: ArchFacts) -> BackendStatus {
    let probes = Probes {
        wsl: f.wsl,
        distro: f.distro,
        cli: f.cli,
        // `Probes` is the one shared diagnostics shape across all three
        // chains; on THIS chain the string it carries is a wire-contract
        // marker, never a semver.
        cli_version: f.cli_contract.clone(),
        titles: f.titles,
        detail: f.detail.clone(),
    };
    // THE CONTRACT THIS CHAIN COMPARES, not the bash CLI's version string.
    // `derive_arch` settles `CliOutdated` by comparing against
    // EXPECTED_ARCH_CLI_CONTRACT ("dml-json-v3"), and this field is rendered
    // verbatim by the UI -- stamping EXPECTED_CLI_VERSION ("3.0.0", the BASH
    // CLI's own version, an entirely different axis; see that constant's doc)
    // produced one sentence built out of two: "dml-arch has DML backend
    // dml-json-v2 in it and this launcher speaks 3.0.0". Whatever a screen
    // says the launcher expects must be the thing the code actually checked.
    let unknown_at = |step: SetupStep| BackendStatus {
        state: SetupState::Unknown,
        blocked_at: Some(step),
        detail: f.detail.clone(),
        distro: distro.to_string(),
        expected_cli_version: EXPECTED_ARCH_CLI_CONTRACT.to_string(),
        probes: probes.clone(),
    };
    let settled = |state: SetupState| BackendStatus {
        state,
        blocked_at: None,
        detail: None,
        distro: distro.to_string(),
        expected_cli_version: EXPECTED_ARCH_CLI_CONTRACT.to_string(),
        probes: probes.clone(),
    };

    match f.wsl {
        Tri::Unknown => return unknown_at(SetupStep::Wsl),
        Tri::No => return settled(SetupState::NoWsl),
        Tri::Yes => {}
    }
    match f.distro {
        Tri::Unknown => return unknown_at(SetupStep::Distro),
        Tri::No => return settled(SetupState::NoDistro),
        Tri::Yes => {}
    }
    // BETWEEN `Distro` AND `Engine`, and that position is the whole point:
    // asking "is dockerd up" first crosses three failure domains at once
    // (wsl.exe reaching the distro, the `dml` user existing, systemd/the
    // unit), and collapsed all of them into "the engine is stopped".
    match f.prepared {
        Tri::Unknown => return unknown_at(SetupStep::Prepared),
        Tri::No => return settled(SetupState::DistroUnprepared),
        Tri::Yes => {}
    }
    match f.dockerd {
        Tri::Unknown => return unknown_at(SetupStep::Engine),
        Tri::No => return settled(SetupState::DockerStopped),
        Tri::Yes => {}
    }
    match f.cli {
        Tri::Unknown => return unknown_at(SetupStep::Cli),
        Tri::No => return settled(SetupState::NoCli),
        Tri::Yes => {}
    }
    match f.cli_contract.as_deref() {
        // A `Yes` with no contract means a classifier broke its own contract.
        None => return unknown_at(SetupStep::Cli),
        Some(c) if c != EXPECTED_ARCH_CLI_CONTRACT => return settled(SetupState::CliOutdated),
        Some(_) => {}
    }
    match f.titles {
        None => unknown_at(SetupStep::Titles),
        Some(0) => settled(SetupState::NoTitles),
        Some(_) => settled(SetupState::Ready),
    }
}

/// Run the Arch chain against an injected runner. SHORT-CIRCUITS: each link is
/// asked only when the one before it said `Yes`. "Is `dml-wow` installed inside
/// `dml-arch`" has no honest answer when there is no `dml-arch`, and every
/// skipped spawn is one fewer timeout a user with a sick machine sits through.
///
/// Every in-distro call uses `--exec`. The bare `--` form runs a shell inside
/// the distro, which splits on `;`, expands `$HOME` and globs (verified
/// 2026-07-28).
/// Shell one-liner for the titles probe: count subdirectories of `$HOME/games`
/// that carry one of the four canonical compose filenames — the same test
/// [`crate::compose`]'s own title-resolution uses, via [`COMPOSE_FILE_CANDIDATES`]
/// (never retyped here).
///
/// `dml-wow` is a per-title CLI fixed to one already-installed title BY
/// DESIGN — there is no `games list` subcommand and there will not be one in
/// this shape (ruled 2026-08-04) — so "how many titles are installed" has no
/// CLI question to ask at all. The games directory itself is the only source
/// of truth left, and this reads it the same way a human would: is there a
/// compose file in there.
///
/// Uses `$HOME`, NOT `~`. This script is handed to `sh -c` explicitly as the
/// invoked PROGRAM (see [`crate::distro`]'s first-boot steps for the same
/// pattern) — `--exec` itself does no shell parsing, so a bare `~` in an argv
/// element that never reaches a shell would not expand at all. Once `sh -c`
/// is the thing being invoked, tilde expansion becomes a real shell feature
/// again, but `$HOME` is used anyway: it is unambiguous in every quoting
/// context this string passes through, whereas `~` only expands unquoted and
/// only at the start of a word, which is one extra way to get this wrong for
/// free.
///
/// A missing/empty `~/games` is not an error case that needs handling: the
/// glob simply matches nothing (POSIX `sh` leaves an unmatched glob as a
/// literal string, which then fails every `-f` test), so the loop body never
/// runs and the count is honestly `0` — no titles installed, not a probe
/// failure.
fn titles_count_script() -> String {
    let checks = COMPOSE_FILE_CANDIDATES
        .iter()
        .map(|name| format!(r#"[ -f "${{d}}{name}" ]"#))
        .collect::<Vec<_>>()
        .join(" || ");
    format!(
        "n=0; for d in \"$HOME\"/games/*/; do if {checks}; then n=$((n+1)); fi; done; echo \"$n\""
    )
}

pub fn probe_arch_with(
    distro: &str,
    user: &str,
    mut run: impl FnMut(&[&str], ProbeBudget) -> ProbeOutcome,
) -> BackendStatus {
    let wsl = classify_wsl_list(&run(&["--list", "--quiet"], ProbeBudget::Warm), distro);
    let mut facts = ArchFacts {
        wsl: wsl.wsl,
        distro: wsl.distro,
        prepared: Tri::Unknown,
        dockerd: Tri::Unknown,
        cli: Tri::Unknown,
        cli_contract: None,
        titles: None,
        detail: wsl.detail,
    };

    if facts.distro == Tri::Yes {
        // THE cold-start call is now THIS one — the first thing to run inside
        // the distro, so it is the one that pays for booting the WSL2 VM and
        // systemd.
        let out = run(
            &["-d", distro, "-u", user, "--exec", "sh", "-c", PREPARED_PROBE_SCRIPT],
            ProbeBudget::ColdStart,
        );
        facts.prepared = classify_arch_prepared(&out);
        if facts.prepared != Tri::Yes {
            // A `No` carries words too here: "the user name or password is
            // incorrect" is the single most useful line on this screen, and
            // the state it settles on is one the user can act on.
            facts.detail = out.detail();
        }
    }

    if facts.prepared == Tri::Yes {
        // Warm by construction: the distro has just answered.
        let mut argv: Vec<&str> = vec!["-d", distro, "-u", user, "--exec", SYSTEMCTL_PROGRAM];
        argv.extend(systemd_is_active_argv());
        let out = run(&argv, ProbeBudget::Warm);
        facts.dockerd = classify_arch_dockerd(&out);
        if facts.dockerd == Tri::Unknown {
            facts.detail = out.detail();
        }
    }

    if facts.dockerd == Tri::Yes {
        // No `--json`: `dml-wow version` takes no such flag (clap rejects an
        // argument it does not define) and emits its envelope unconditionally
        // either way — verified live against the real binary, 2026-08-04.
        let cli = classify_arch_cli(&run(
            &["-d", distro, "-u", user, "--exec", "dml-wow", "version"],
            ProbeBudget::Warm,
        ));
        facts.cli = cli.cli;
        facts.cli_contract = cli.contract;
        if cli.detail.is_some() {
            facts.detail = cli.detail;
        }

        // Only ask a binary we can actually speak to. A binary reporting a
        // DIFFERENT contract is already a settled CliOutdated — asking it for
        // titles risks parsing output in a shape this launcher does not know.
        let usable = facts.cli == Tri::Yes
            && facts.cli_contract.as_deref() == Some(EXPECTED_ARCH_CLI_CONTRACT);
        if usable {
            let script = titles_count_script();
            let out = run(
                &["-d", distro, "-u", user, "--exec", "sh", "-c", &script],
                ProbeBudget::Warm,
            );
            facts.titles = classify_arch_titles(&out);
            if facts.titles.is_none() {
                facts.detail = out.detail();
            }
        }
    }

    derive_arch(distro, facts)
}

/// JUST the first link: is the distro registered?
///
/// [`backend::detect`](crate::backend::detect) needs this at startup to avoid
/// routing a machine with no distro to the WSL backend, and it must not pay for
/// the rest of the chain to learn it. This is affordable precisely because
/// `wsl --list --quiet` is a host-side registry read on the WARM budget — it
/// does not cold-start the WSL2 VM, so it cannot add VM boot time to launching
/// the app. Running the full chain here would, and startup happens before the
/// window is shown.
///
/// Returns a [`Tri`], and callers must respect it: `Unknown` means the question
/// went unanswered, never "no distro".
pub fn distro_registered_with(
    distro: &str,
    mut run: impl FnMut(&[&str], ProbeBudget) -> ProbeOutcome,
) -> Tri {
    classify_wsl_list(&run(&["--list", "--quiet"], ProbeBudget::Warm), distro).distro
}

/// [`distro_registered_with`] against the real `wsl.exe`.
pub fn distro_registered(env: &SetupProbeEnv) -> Tri {
    distro_registered_with(&env.distro, |args, budget| {
        spawn_probe(&env.wsl_program, args, env.budget(budget))
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

    // -- the NATIVE chain ----------------------------------------------------

    fn native_ok() -> NativeFacts {
        NativeFacts { docker_installed: Tri::Yes, engine_running: Tri::Yes, titles: Some(1) }
    }

    #[test]
    fn native_ready_needs_docker_engine_and_a_title() {
        assert_eq!(derive_native(native_ok()).state, SetupState::Ready);
    }

    #[test]
    fn native_no_docker_is_its_own_state_not_no_wsl() {
        // Running the WSL chain in native mode answered `no_wsl` here, which
        // would tell a user with a perfectly good Docker setup to install WSL.
        let f = NativeFacts { docker_installed: Tri::No, ..native_ok() };
        assert_eq!(derive_native(f).state, SetupState::NoDocker);
    }

    #[test]
    fn native_engine_down_is_distinguishable_from_docker_absent() {
        // These need different buttons: one starts the engine, the other sends
        // the user to a download page. Collapsing them is how a user with
        // Docker installed gets told to install Docker.
        let f = NativeFacts { engine_running: Tri::No, ..native_ok() };
        assert_eq!(derive_native(f).state, SetupState::DockerStopped);
    }

    /// THE ARM TASK 6 EXISTS FOR: a native machine that is fully able to run a
    /// server and simply has not installed one. Before this chain existed the
    /// frontend showed such a user nothing at all, so they landed on Home
    /// looking at a status card for a server that does not exist.
    #[test]
    fn native_with_no_titles_sends_the_user_to_install_one() {
        let f = NativeFacts { titles: Some(0), ..native_ok() };
        assert_eq!(derive_native(f).state, SetupState::NoTitles);
    }

    #[test]
    fn native_unknowns_are_never_read_as_absence() {
        // Each shrug names the step that went dark and stays Unknown -- an
        // Unknown docker probe must NOT become "install Docker", which is a
        // repair aimed at someone who may already have it.
        let f = NativeFacts { docker_installed: Tri::Unknown, ..native_ok() };
        let got = derive_native(f);
        assert_eq!(got.state, SetupState::Unknown);
        assert_eq!(got.blocked_at, Some(SetupStep::Docker));

        let f = NativeFacts { engine_running: Tri::Unknown, ..native_ok() };
        let got = derive_native(f);
        assert_eq!(got.state, SetupState::Unknown);
        assert_eq!(got.blocked_at, Some(SetupStep::Engine));

        // None titles is a could-not-tell, NOT zero.
        let f = NativeFacts { titles: None, ..native_ok() };
        let got = derive_native(f);
        assert_eq!(got.state, SetupState::Unknown);
        assert_eq!(got.blocked_at, Some(SetupStep::Titles));
    }

    #[test]
    fn native_never_names_a_distro() {
        // "dml-arch" in front of a user who has deliberately never had one.
        for f in [
            native_ok(),
            NativeFacts { docker_installed: Tri::No, ..native_ok() },
            NativeFacts { titles: Some(0), ..native_ok() },
        ] {
            assert_eq!(derive_native(f).distro, "", "native mode has no distro to name");
        }
    }

    #[test]
    fn native_stops_at_the_first_missing_link() {
        // No docker AND no titles: the answer is the FIRST one, so there is
        // always exactly one next step rather than a list of complaints.
        let f = NativeFacts { docker_installed: Tri::No, engine_running: Tri::No, titles: Some(0) };
        assert_eq!(derive_native(f).state, SetupState::NoDocker);
    }

    // -- classify_wsl_list ---------------------------------------------------

    #[test]
    fn wsl_list_missing_program_means_no_wsl_and_no_distro() {
        // wsl.exe genuinely absent is the ONE definitive negative.
        let got = classify_wsl_list(&ProbeOutcome::ProgramMissing, DISTRO);
        assert_eq!(got, WslProbe { wsl: Tri::No, distro: Tri::No, detail: None });
    }

    #[test]
    fn wsl_list_could_not_tell_is_unknown_on_both_never_no() {
        // The tri-state rule: a hung/blocked wsl.exe proves nothing.
        let got = classify_wsl_list(&ProbeOutcome::CouldNotTell, DISTRO);
        assert_eq!(got, WslProbe { wsl: Tri::Unknown, distro: Tri::Unknown, detail: None });
    }

    #[test]
    fn wsl_list_finds_the_distro_among_others() {
        let got = classify_wsl_list(&ran(0, "Ubuntu\r\ndml-arch\r\ndocker-desktop\r\n"), DISTRO);
        assert_eq!(got, WslProbe { wsl: Tri::Yes, distro: Tri::Yes, detail: None });
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
        assert_eq!(got, WslProbe { wsl: Tri::Yes, distro: Tri::No, detail: None });
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
        assert_eq!(got, WslProbe { wsl: Tri::Yes, distro: Tri::No, detail: None });
    }

    #[test]
    fn wsl_list_optional_component_not_enabled_is_no_wsl() {
        let got = classify_wsl_list(
            &ran_err(1, "The Windows Subsystem for Linux optional component is not enabled."),
            DISTRO,
        );
        assert_eq!(got, WslProbe { wsl: Tri::No, distro: Tri::No, detail: None });
    }

    #[test]
    fn wsl_list_unrecognized_failure_is_unknown_not_no() {
        // The default must be a shrug. Anything else invents a diagnosis.
        let got = classify_wsl_list(&ran_err(1, "Error code: Wsl/Service/0x8007273f"), DISTRO);
        assert_eq!(
            got,
            WslProbe {
                wsl: Tri::Unknown,
                distro: Tri::Unknown,
                // ...and the shrug carries wsl.exe's own words out with it,
                // because this screen has no repair on it (P9).
                detail: Some("Error code: Wsl/Service/0x8007273f".to_string()),
            }
        );
    }

    // -- classify_cli_version ------------------------------------------------

    #[test]
    fn cli_version_read_from_the_ok_envelope() {
        let got = classify_cli_version(&ran(0, r#"{"ok":true,"data":{"version":"3.0.0"}}"#));
        assert_eq!(got, CliProbe { cli: Tri::Yes, version: Some("3.0.0".into()), detail: None });
    }

    #[test]
    fn cli_version_reports_an_old_cli_verbatim() {
        // The bootstrap CLI the elevated installer still embeds.
        let got = classify_cli_version(&ran(0, r#"{"ok":true,"data":{"version":"2.6.0"}}"#));
        assert_eq!(got, CliProbe { cli: Tri::Yes, version: Some("2.6.0".into()), detail: None });
    }

    #[test]
    fn cli_missing_inside_the_distro_is_a_definitive_no() {
        let got = classify_cli_version(&ran_err(127, "bash: dml: command not found"));
        assert_eq!(got, CliProbe { cli: Tri::No, version: None, detail: None });
    }

    #[test]
    fn cli_could_not_tell_is_unknown() {
        let got = classify_cli_version(&ProbeOutcome::CouldNotTell);
        assert_eq!(got, CliProbe { cli: Tri::Unknown, version: None, detail: None });
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
        assert_eq!(
            got,
            CliProbe {
                cli: Tri::Unknown,
                version: None,
                // The unreadable answer is quoted back, not swallowed (P9).
                detail: Some("some other program v3.0.0".to_string()),
            }
        );
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
        assert_eq!(got, WslProbe { wsl: Tri::No, distro: Tri::No, detail: None });
    }

    #[test]
    fn every_known_wsl_message_beats_the_exit_code_it_arrives_with() {
        // P3. The lesson of f304629 is not "add one more string": it is that
        // wsl.exe's EXIT CODE is not evidence. The inbox stub exits 0 while
        // saying WSL is missing -- and there is nothing special about that one
        // sentence, so EVERY message we claim to recognise has to be read
        // before the exit code, not only on the non-zero branch.
        //
        // Each row is (message, expected) and is asserted at BOTH exit 0 and
        // exit 1, because either is possible and neither is the evidence.
        let cases: &[(&str, Tri, Tri)] = &[
            (
                "The Windows Subsystem for Linux is not installed. You can install by running 'wsl.exe --install'.",
                Tri::No,
                Tri::No,
            ),
            (
                "The Windows Subsystem for Linux optional component is not enabled. Please enable it and try again.",
                Tri::No,
                Tri::No,
            ),
            (
                "WslRegisterDistribution failed: the optional component has not been enabled.",
                Tri::No,
                Tri::No,
            ),
            (
                "Error: 0x8007019e The Windows Subsystem for Linux has not been enabled.",
                Tri::No,
                Tri::No,
            ),
            ("Windows Subsystem for Linux has no installed distributions.", Tri::Yes, Tri::No),
        ];
        for (message, wsl, distro) in cases {
            for code in [0, 1] {
                let on_stdout = classify_wsl_list(&ran(code, message), DISTRO);
                assert_eq!(
                    (on_stdout.wsl, on_stdout.distro),
                    (*wsl, *distro),
                    "exit {code} must not change the reading of {message:?}"
                );
                let on_stderr = classify_wsl_list(&ran_err(code, message), DISTRO);
                assert_eq!(
                    (on_stderr.wsl, on_stderr.distro),
                    (*wsl, *distro),
                    "exit {code} on stderr must not change the reading of {message:?}"
                );
            }
        }
    }

    #[test]
    fn an_unrecognised_message_at_exit_zero_is_not_an_empty_distro_list() {
        // P3 + P9, the compounding pair. A non-English Windows with WSL absent
        // prints a message we do not recognise and (inbox stub) exits 0. The
        // old code read "exit 0" as "wsl works" and the ABSENCE of a dml-arch
        // line as "the distro is missing", producing NoDistro: a user with no
        // WSL at all told to go create a distro. A message is not a list.
        let got = classify_wsl_list(
            &ProbeOutcome::Ran {
                code: Some(0),
                stdout: String::new(),
                stderr: "Das Windows-Subsystem für Linux wurde nicht installiert.".to_string(),
            },
            DISTRO,
        );
        assert_eq!(got.wsl, Tri::Unknown, "an unreadable answer is evidence of nothing");
        assert_eq!(got.distro, Tri::Unknown);
    }

    #[test]
    fn a_message_on_stdout_at_exit_zero_is_not_a_one_entry_distro_list() {
        // The other half of the same hole. wsl.exe writes its own complaints to
        // stdout as readily as to stderr -- the inbox stub's "is not installed"
        // banner arrives there, which is why the regression test above models
        // it that way. So a sentence we do NOT recognise arrives there too, and
        // splitting it into lines produced a "distro list" of one entry that
        // simply was not `dml-arch`: a machine with no WSL, told to go create a
        // distro. A distro name is one token; a sentence is not.
        let got = classify_wsl_list(
            &ran(0, "Das Windows-Subsystem für Linux wurde nicht installiert.\r\n"),
            DISTRO,
        );
        assert_eq!(got.wsl, Tri::Unknown, "prose is not a distro list");
        assert_eq!(got.distro, Tri::Unknown);
        assert!(
            got.detail.as_deref().unwrap_or("").contains("nicht installiert"),
            "and the shrug must quote it: {:?}",
            got.detail
        );
    }

    #[test]
    fn a_listed_distro_is_believed_even_next_to_output_we_cannot_parse() {
        // The guard above must never cost a machine that is demonstrably fine:
        // the name we asked about being present is positive evidence, and it
        // outranks anything else on the stream.
        let got = classify_wsl_list(&ran(0, "Some chatty banner line\r\ndml-arch\r\n"), DISTRO);
        assert_eq!((got.wsl, got.distro), (Tri::Yes, Tri::Yes));
        // ...and ordinary stderr chatter (WSL emits proxy/localhost warnings
        // routinely) must not turn a real list into a shrug either.
        let noisy = ProbeOutcome::Ran {
            code: Some(0),
            stdout: "Ubuntu\r\ndocker-desktop\r\n".to_string(),
            stderr: "wsl: A localhost proxy configuration was detected...".to_string(),
        };
        assert_eq!(
            (classify_wsl_list(&noisy, DISTRO).wsl, classify_wsl_list(&noisy, DISTRO).distro),
            (Tri::Yes, Tri::No)
        );
    }

    #[test]
    fn a_real_distro_list_at_exit_zero_is_still_read_as_a_list() {
        // The guard above must not cost the happy path: a list is a list.
        assert_eq!(
            classify_wsl_list(&ran(0, "Ubuntu\r\ndml-arch\r\n"), DISTRO).distro,
            Tri::Yes
        );
        // A silent exit 0 (no stdout, no stderr) is a working wsl.exe with
        // nothing registered -- a real answer, and the one NoDistro exists for.
        let silent = classify_wsl_list(&ran(0, ""), DISTRO);
        assert_eq!((silent.wsl, silent.distro), (Tri::Yes, Tri::No));
    }

    /// Build a `Ran` with BOTH streams populated -- the shape the two
    /// regressions below live in, and one `ran`/`ran_err` cannot express.
    fn ran_both(code: i32, stdout: &str, stderr: &str) -> ProbeOutcome {
        ProbeOutcome::Ran {
            code: Some(code),
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
        }
    }

    #[test]
    fn a_legal_distro_name_with_a_space_does_not_poison_the_whole_list() {
        // REGRESSION. The "is this really a list of names" guard was written
        // as "EVERY line is a single token", which is not true of distro
        // names: `wsl --import "Ubuntu 22.04" ...` is legal, the Store's own
        // packages register as "Ubuntu 20.04"/"Ubuntu 22.04", and "Arch Linux"
        // is ordinary in exactly this project's audience. One such neighbour
        // downgraded a perfectly readable list to a shrug -- i.e. a machine
        // whose honest answer is "dml-arch isn't installed yet, here is the
        // installer" got the one screen with no repair on it instead, with a
        // diagnostics line quoting a DIFFERENT distro's name at them.
        let got = classify_wsl_list(&ran(0, "Ubuntu 22.04\r\ndocker-desktop\r\n"), DISTRO);
        assert_eq!(
            (got.wsl, got.distro),
            (Tri::Yes, Tri::No),
            "a multi-word neighbour must not make a readable list unreadable"
        );
        assert_eq!(got.detail, None, "a settled answer carries no diagnostic");

        // The same list WITH dml-arch in it is still positive evidence...
        let got = classify_wsl_list(&ran(0, "Ubuntu 22.04\r\ndml-arch\r\n"), DISTRO);
        assert_eq!((got.wsl, got.distro), (Tri::Yes, Tri::Yes));

        // ...and a multi-word name we were ASKED about is matched, not
        // filtered away as unparseable.
        let got = classify_wsl_list(&ran(0, "Ubuntu\r\nMy Distro\r\n"), "My Distro");
        assert_eq!((got.wsl, got.distro), (Tri::Yes, Tri::Yes));
    }

    #[test]
    fn a_localized_message_is_not_a_distro_list_in_any_script() {
        // REGRESSION, and the other half of the trade the test above made. The
        // "is this a list of names" guard was rewritten around ASCII shape: an
        // ASCII '.'/':' terminator plus a count of SPACE-separated words. A
        // Japanese or Chinese wsl.exe has neither -- its full stop is U+3002
        // and it spends no spaces at all, so a whole sentence counts as ONE
        // word and reads as a one-entry distro list. The verdict on a machine
        // with no WSL was then a confident wsl=Yes / distro=No: "WSL works,
        // dml-arch just isn't registered", with a Create-the-distro path in
        // front of someone who cannot host one.
        //
        // Real localized shapes -- Japanese and Chinese, each both with the
        // Latin product names Windows keeps inline and without them, and one
        // with NO terminator at all so the fix cannot be "learn one more
        // full stop character".
        for stdout in [
            "Linux 用 Windows サブシステムがインストールされていません。\r\n",
            "指定された名前のディストリビューションが見つかりませんでした。\r\n",
            "ディストリビューションが見つかりません\r\n",
            "适用于 Linux 的 Windows 子系统未安装。\r\n",
            "系统找不到指定的文件。\r\n",
            // ...and a CASED script, which the caseless-letter half of the rule
            // cannot see: Russian is spaced and counts as 4 words, so only the
            // punctuation half rejects it. `wsl: ` is wsl.exe's own prefix (see
            // the "wsl: Docker Desktop is not installed." samples below), and a
            // short localized complaint under it clears a 5-word budget easily.
            "wsl: Не удалось запустить\r\n",
        ] {
            let got = classify_wsl_list(&ran(0, stdout), DISTRO);
            assert_eq!(
                (got.wsl, got.distro),
                (Tri::Unknown, Tri::Unknown),
                "prose is not a distro list, whatever script it is in: {stdout:?}"
            );
            assert!(
                !got.detail.as_deref().unwrap_or("").is_empty(),
                "and the shrug must quote it: {:?}",
                got.detail
            );
        }
    }

    #[test]
    fn a_non_ascii_multi_word_name_is_still_a_list() {
        // The counterweight, so the guard above cannot be paid for by
        // declaring every non-ASCII line unreadable: `wsl --import` takes the
        // name it is given, and a Danish user's "Min Grønne Distro" next to a
        // Store "Ubuntu 22.04" is an ordinary, readable list. NoDistro has a
        // real sentence and a route to Install-DML.ps1 on it; Unknown has a
        // Check-again button that can never succeed on this machine.
        let got = classify_wsl_list(
            &ran(0, "Ubuntu 22.04\r\nMin Grønne Distro\r\nArch Linux\r\n"),
            DISTRO,
        );
        assert_eq!(
            (got.wsl, got.distro),
            (Tri::Yes, Tri::No),
            "a legal multi-word name in a Latin script is still a list"
        );
        assert_eq!(got.detail, None, "a settled answer carries no diagnostic");
    }

    #[test]
    fn a_space_in_a_neighbours_name_still_reaches_the_installer_screen() {
        // The same regression stated as the user-visible consequence, because
        // that is what the finding is about: NoDistro has a real sentence and
        // a way to reach Install-DML.ps1; Unknown has a Check-again button
        // that can never succeed on this machine.
        let st = probe_with(DISTRO, USER, |_, _| ran(0, "Ubuntu 22.04\r\n"));
        assert_eq!(st.state, SetupState::NoDistro);
        assert_eq!(st.blocked_at, None);
    }

    #[test]
    fn a_listed_distro_outranks_a_recognised_message_on_the_other_stream() {
        // REGRESSION. The message scan ran over `stdout + stderr` MERGED and
        // ahead of the positive evidence, so any recognised phrase anywhere on
        // either stream unregistered a distro that wsl.exe had just listed by
        // name. "wsl: Docker Desktop is not installed." is not hypothetical --
        // "is not installed" is a sentence fragment about SOMETHING, and the
        // subject is not always WSL.
        for message in [
            "wsl: Docker Desktop is not installed.",
            "The Windows Subsystem for Linux optional component is not enabled.",
            "WslRegisterDistribution failed: the optional component has not been enabled.",
            "Error: 0x8007019e",
            "Windows Subsystem for Linux has no installed distributions.",
        ] {
            let got = classify_wsl_list(&ran_both(0, "Ubuntu\r\ndml-arch\r\n", message), DISTRO);
            assert_eq!(
                (got.wsl, got.distro),
                (Tri::Yes, Tri::Yes),
                "stderr {message:?} must not unregister a distro that is literally listed"
            );
        }
    }

    #[test]
    fn a_readable_list_on_stdout_outranks_a_recognised_message_on_stderr() {
        // The same ordering bug one step down the chain: dml-arch is absent,
        // but the list PROVES WSL works, and stderr is where wsl.exe puts
        // commentary. Answering "WSL is not installed" to a machine that just
        // enumerated its distros is a confident No with a repair button on it.
        let got = classify_wsl_list(
            &ran_both(0, "Ubuntu\r\ndocker-desktop\r\n", "wsl: Docker Desktop is not installed."),
            DISTRO,
        );
        assert_eq!((got.wsl, got.distro), (Tri::Yes, Tri::No));
        assert_eq!(got.detail, None);
    }

    #[test]
    fn a_message_in_the_answer_slot_still_beats_a_list_shaped_reading() {
        // The gain the previous wave made, pinned so this reordering cannot
        // give it back: stdout is where the ANSWER goes, so a message we
        // recognise ARRIVING THERE is the answer -- ahead of any attempt to
        // read stdout as a list, and regardless of the exit code (the inbox
        // stub prints exactly this and exits 0).
        //
        // The second sample is the sharp one, and the reason this test is not
        // redundant with the inbox-stub test above: every line of it IS
        // name-shaped, so dropping the answer-slot message check would let the
        // list reader answer "WSL works, dml-arch just isn't registered" to a
        // machine that has no WSL at all. (Mutation-checked: deleting that
        // check reds exactly this assertion.)
        for stdout in [
            "The Windows Subsystem for Linux is not installed.\r\nrun wsl --install\r\n",
            "WSL is not installed\r\n",
        ] {
            for code in [0, 1] {
                let got = classify_wsl_list(&ran_both(code, stdout, ""), DISTRO);
                assert_eq!(
                    (got.wsl, got.distro),
                    (Tri::No, Tri::No),
                    "exit {code}, stdout {stdout:?}"
                );
            }
        }
    }

    #[test]
    fn a_plain_text_banner_is_not_garbage() {
        // The counterpart to the test above, stated explicitly so the two
        // cannot drift back together.
        assert_eq!(
            classify_cli_version(&ran(0, "dml v3.0.0")),
            CliProbe { cli: Tri::Yes, version: Some("3.0.0".to_string()), detail: None }
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
        Probes { wsl, distro, cli, cli_version: version.map(str::to_string), titles, detail: None }
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
        let _ = probe_with(DISTRO, USER, |a, _| fake.run(a));
        let calls = fake.calls();
        assert_eq!(calls.first().map(Vec::as_slice), Some(&["--list".to_string(), "--quiet".to_string()][..]));
    }

    #[test]
    fn chain_stops_at_the_first_missing_link_and_spawns_nothing_further() {
        // Asking what is inside a distro that does not exist has no honest
        // answer, and each extra spawn is another `timeout` the user waits.
        let fake = Fake::new(vec![ProbeOutcome::ProgramMissing]);
        let got = probe_with(DISTRO, USER, |a, _| fake.run(a));
        assert_eq!(got.state, SetupState::NoWsl);
        assert_eq!(fake.calls().len(), 1, "no probe may run past a missing link");
    }

    #[test]
    fn chain_does_not_probe_the_cli_when_the_distro_is_absent() {
        let fake = Fake::new(vec![ran(0, "Ubuntu\r\n")]);
        let got = probe_with(DISTRO, USER, |a, _| fake.run(a));
        assert_eq!(got.state, SetupState::NoDistro);
        assert_eq!(fake.calls().len(), 1);
    }

    #[test]
    fn chain_runs_the_cli_probe_inside_the_distro_as_the_dml_user() {
        let fake = Fake::new(vec![
            ran(0, "dml-arch\r\n"),
            ran_err(127, "bash: dml: command not found"),
        ]);
        let got = probe_with(DISTRO, USER, |a, _| fake.run(a));
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
        let got = probe_with(DISTRO, USER, |a, _| fake.run(a));
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
        let got = probe_with(DISTRO, USER, |a, _| fake.run(a));
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
        let got = probe_with(DISTRO, USER, |a, _| fake.run(a));
        assert_eq!(got.state, SetupState::NoTitles);
    }

    // -- P9: the unrecognised path must carry wsl.exe's own words -------------
    // These assert on the SERIALIZED report, not on an internal field: the
    // finding is that the message never reaches the screen, and the screen
    // reads this JSON. A field that exists but is not serialized would fix
    // nothing.

    fn detail_of(st: &BackendStatus) -> String {
        serde_json::to_value(st).unwrap()["detail"].as_str().unwrap_or("").to_string()
    }

    #[test]
    fn an_unrecognised_wsl_failure_reaches_the_screen_with_its_own_message() {
        // Without this the user gets "Couldn't check this PC's setup" and a
        // Check-again button that will never succeed, and neither they nor a
        // bug report has one word to go on.
        let st = probe_with(DISTRO, USER, |_, _| {
            ran_err(1, "Wsl/Service/CreateInstance/CreateVm/HCS/0x80370102")
        });
        assert_eq!(st.state, SetupState::Unknown);
        assert_eq!(st.blocked_at, Some(SetupStep::Wsl));
        assert!(
            detail_of(&st).contains("0x80370102"),
            "the screen has nothing to show: {:?}",
            serde_json::to_value(&st).unwrap()
        );
    }

    #[test]
    fn a_corrupt_distro_names_itself_instead_of_looping_the_user_forever() {
        // Concrete case 2 from the finding: the distro IS registered, but
        // starting it fails. That is neither exit 127 nor any matched string,
        // so it is a could-not-tell at the CLI step -- the one screen with no
        // repair on it. It must at least say why.
        let st = probe_with(DISTRO, USER, |args, _| {
            if args.first().copied() == Some("--list") {
                ran(0, "dml-arch\r\n")
            } else {
                ran_err(1, "The distribution failed to start: Wsl/Service/0x8007019e_NOPE")
            }
        });
        assert_eq!(st.state, SetupState::Unknown);
        assert_eq!(st.blocked_at, Some(SetupStep::Cli));
        assert!(
            detail_of(&st).contains("The distribution failed to start"),
            "got {:?}",
            detail_of(&st)
        );
    }

    /// `wsl.exe`'s OWN messages are UTF-16LE (see `envelope::decode_wsl_output`).
    fn utf16le(s: &str) -> Vec<u8> {
        s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect()
    }

    #[test]
    fn a_localized_utf16_message_arrives_readable_and_a_truncated_one_cannot_panic() {
        // The population this finding is written for: a non-English Windows
        // whose message matches none of our substrings. It must survive the
        // decode intact, and a HALF-decoded buffer (odd length, a truncated
        // pipe read) must degrade to mojibake -- never to a panic, which in a
        // diagnostic path would take the whole first-run screen down.
        let german = "Das Windows-Subsystem für Linux wurde nicht installiert.";
        let bytes = utf16le(german);
        let decoded = decode_wsl_output(&bytes);
        assert!(decoded.contains("nicht installiert"), "decode lost the message: {decoded:?}");

        let st = probe_with(DISTRO, USER, |_, _| ProbeOutcome::Ran {
            code: Some(0),
            stdout: String::new(),
            stderr: decoded.clone(),
        });
        assert_eq!(st.state, SetupState::Unknown, "an unreadable answer is not a missing distro");
        assert!(detail_of(&st).contains("nicht installiert"), "got {:?}", detail_of(&st));

        // Truncated mid-code-unit: lossy, not fatal.
        let truncated = decode_wsl_output(&bytes[..bytes.len() - 1]);
        let st2 = probe_with(DISTRO, USER, |_, _| ProbeOutcome::Ran {
            code: Some(0),
            stdout: String::new(),
            stderr: truncated.clone(),
        });
        assert_eq!(st2.state, SetupState::Unknown);
        assert!(!detail_of(&st2).is_empty(), "even garbled bytes are better than silence");
    }

    #[test]
    fn a_recognised_answer_carries_no_detail_at_all() {
        // `detail` is the could-not-tell's evidence, not a debug dump: every
        // settled state already has a real sentence written for it, and a raw
        // wsl.exe line under it would only make that screen look broken.
        //
        // The probes here deliberately arrive CARRYING a diagnostic. Feeding
        // in `detail: None` would let `settled` copy the carrier through and
        // this test would still pass -- that mutant survived the first draft.
        let carried = "Wsl/Service/0x8007273f (left over from an earlier link)";
        for probes in [
            p(Tri::No, Tri::No, Tri::Unknown, None, None),
            p(Tri::Yes, Tri::No, Tri::Unknown, None, None),
            p(Tri::Yes, Tri::Yes, Tri::No, None, None),
            p(Tri::Yes, Tri::Yes, Tri::Yes, Some("2.6.0"), Some(1)),
            p(Tri::Yes, Tri::Yes, Tri::Yes, Some("3.0.0"), Some(0)),
            p(Tri::Yes, Tri::Yes, Tri::Yes, Some("3.0.0"), Some(3)),
        ] {
            let st = derive(DISTRO, Probes { detail: Some(carried.to_string()), ..probes });
            assert_ne!(st.state, SetupState::Unknown, "{st:?}");
            assert_eq!(detail_of(&st), "", "settled state leaked a diagnostic: {st:?}");
        }

        // And the chain's own settled answers really do arrive detail-free.
        for st in [
            probe_with(DISTRO, USER, |_, _| ProbeOutcome::ProgramMissing),
            probe_with(DISTRO, USER, |_, _| {
                ran_err(1, "Windows Subsystem for Linux has no installed distributions.")
            }),
            probe_with(DISTRO, USER, |args, _| {
                if args.first().copied() == Some("--list") {
                    ran(0, "dml-arch\r\n")
                } else {
                    ran_err(127, "bash: dml: command not found")
                }
            }),
        ] {
            assert_ne!(st.state, SetupState::Unknown, "{st:?}");
            assert_eq!(detail_of(&st), "", "settled state leaked a diagnostic: {st:?}");
        }
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

    // -- P4/P8/P12: the cold-boot budget --------------------------------------
    // These assert ELAPSED WALL CLOCK, not "an error came back": an
    // implementation that hands every spawn the same short budget produces the
    // identical `Unknown` verdict, so only the clock can tell the two apart.

    /// A child that outlives any budget these tests set, spawned DIRECTLY (one
    /// process, so the kill closes the pipes the draining threads read — the
    /// grandchild trap `proc.rs` and `dml_wow::backup` both document).
    #[cfg(windows)]
    fn hang_command() -> std::process::Command {
        let mut cmd = std::process::Command::new("ping");
        cmd.args(["-n", "30", "127.0.0.1"]);
        cmd
    }
    #[cfg(not(windows))]
    fn hang_command() -> std::process::Command {
        let mut cmd = std::process::Command::new("sleep");
        cmd.arg("30");
        cmd
    }

    /// Scaled-down stand-ins for the two real budgets, so the suite stays fast
    /// while the RATIO under test (cold >> poll) is the real one.
    const TEST_POLL: Duration = Duration::from_millis(300);
    const TEST_COLD: Duration = Duration::from_millis(2000);

    #[test]
    fn the_first_call_into_the_distro_may_outlast_the_status_poll_budget() {
        // THE REGRESSION: `wsl -d dml-arch -u dml -- dml version --json` is the
        // call that boots the WSL2 utility VM (plus systemd inside it). After a
        // Windows reboot that routinely runs past a status-poll budget, and the
        // consequence is not a slow Home — it is an established user's WORKING
        // Home being replaced by "Couldn't check this PC's setup".
        let mut env = SetupProbeEnv::new(DISTRO, USER);
        env.timeout = TEST_POLL;
        env.cold_timeout = TEST_COLD;
        let start = std::time::Instant::now();
        let got = probe_with(DISTRO, USER, |args, budget| {
            if args.first().copied() == Some("--list") {
                return ran(0, "dml-arch\r\n");
            }
            let mut cmd = hang_command();
            windows_no_window(&mut cmd);
            ProbeOutcome::from_bounded(run_bounded_outcome(cmd, env.budget(budget)))
        });
        let elapsed = start.elapsed();
        assert_eq!(got.state, SetupState::Unknown, "the hung probe must still be a shrug");
        assert_eq!(got.blocked_at, Some(SetupStep::Cli));
        assert!(
            elapsed >= TEST_COLD,
            "the cold call was cut off at the poll budget after only {elapsed:?} - \
             a slow-but-fine machine gets reported as broken"
        );
        assert!(elapsed < TEST_COLD * 4, "must still return promptly after it, took {elapsed:?}");
    }

    #[test]
    fn a_host_side_wsl_call_does_not_inherit_the_cold_budget() {
        // The other half, and the reason this is two budgets rather than one
        // bigger number: `wsl --list --quiet` is a registry read that never
        // starts a VM, so a wedged or absent WSL must still answer fast. Make
        // every probe wait two minutes and a broken machine takes six.
        let mut env = SetupProbeEnv::new(DISTRO, USER);
        env.timeout = TEST_POLL;
        env.cold_timeout = TEST_COLD;
        let start = std::time::Instant::now();
        let got = probe_with(DISTRO, USER, |_args, budget| {
            let mut cmd = hang_command();
            windows_no_window(&mut cmd);
            ProbeOutcome::from_bounded(run_bounded_outcome(cmd, env.budget(budget)))
        });
        let elapsed = start.elapsed();
        assert_eq!(got.state, SetupState::Unknown);
        assert_eq!(got.blocked_at, Some(SetupStep::Wsl));
        assert!(elapsed >= TEST_POLL, "must have actually waited, {elapsed:?}");
        assert!(
            elapsed < TEST_COLD,
            "probe 1 boots nothing and must not be given the cold budget, waited {elapsed:?}"
        );
    }

    #[test]
    fn the_shipped_budgets_cover_a_real_cold_wsl2_boot() {
        // The numbers themselves, pinned. `provision.rs` already budgets 120s
        // for the identical first spawn ("the FIRST wsl.exe call on a cold
        // machine boots the WSL2 VM, which can take the better part of a
        // minute") and Install-DML.ps1 allows systemd 60s on top; a probe that
        // gates Home cannot be stingier than the command it gates.
        let env = SetupProbeEnv::new(DISTRO, USER);
        assert_eq!(env.budget(ProbeBudget::ColdStart), DEFAULT_COLD_START_TIMEOUT);
        assert_eq!(env.budget(ProbeBudget::Warm), DEFAULT_PROBE_TIMEOUT);
        assert!(
            DEFAULT_COLD_START_TIMEOUT >= Duration::from_secs(90),
            "a cold WSL2 + systemd boot routinely passes a minute"
        );
        assert!(
            DEFAULT_PROBE_TIMEOUT <= Duration::from_secs(30),
            "the host-side call must still fail fast"
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
                detail: None,
            },
        );
        assert_eq!(st.state, SetupState::CliOutdated, "an old CLI must offer the upgrade, not a shrug");
        assert_eq!(st.blocked_at, None, "a known-old CLI is not a could-not-tell");
    }

    // -- the ARCH chain -------------------------------------------------------

    fn arch_ok() -> ArchFacts {
        ArchFacts {
            wsl: Tri::Yes,
            distro: Tri::Yes,
            prepared: Tri::Yes,
            dockerd: Tri::Yes,
            cli: Tri::Yes,
            cli_contract: Some(EXPECTED_ARCH_CLI_CONTRACT.to_string()),
            titles: Some(1),
            detail: None,
        }
    }

    /// A fake reply for `probe_arch_with` that answers every real probe of
    /// the chain correctly, keyed on distinguishing argv elements. `titles`
    /// controls the titles-probe (`sh -c ...`) reply.
    fn arch_happy_run(titles: usize) -> impl FnMut(&[&str], ProbeBudget) -> ProbeOutcome {
        move |args, _| {
            if args.first().copied() == Some("--list") {
                ran(0, "dml-arch\n")
            } else if args.iter().any(|a| *a == PREPARED_PROBE_SCRIPT) {
                // `command -v docker` prints the resolved path.
                ran(0, "/usr/bin/docker")
            } else if args.iter().any(|a| *a == "is-active") {
                ran(0, "")
            } else if args.iter().any(|a| *a == "version") {
                ran(0, &format!(r#"{{"ok":true,"data":{{"contract":"{EXPECTED_ARCH_CLI_CONTRACT}"}}}}"#))
            } else {
                // the titles probe: `sh -c '<script>'`
                ran(0, &format!("{titles}\n"))
            }
        }
    }

    #[test]
    fn arch_all_green_is_ready() {
        assert_eq!(derive_arch(DISTRO, arch_ok()).state, SetupState::Ready);
    }

    #[test]
    fn arch_chain_stops_at_the_first_missing_link() {
        let no_wsl = ArchFacts { wsl: Tri::No, ..arch_ok() };
        assert_eq!(derive_arch(DISTRO, no_wsl).state, SetupState::NoWsl);

        let no_distro = ArchFacts { distro: Tri::No, ..arch_ok() };
        assert_eq!(derive_arch(DISTRO, no_distro).state, SetupState::NoDistro);

        let unprepared = ArchFacts { prepared: Tri::No, ..arch_ok() };
        assert_eq!(derive_arch(DISTRO, unprepared).state, SetupState::DistroUnprepared);

        let no_dockerd = ArchFacts { dockerd: Tri::No, ..arch_ok() };
        assert_eq!(derive_arch(DISTRO, no_dockerd).state, SetupState::DockerStopped);

        let no_cli = ArchFacts { cli: Tri::No, ..arch_ok() };
        assert_eq!(derive_arch(DISTRO, no_cli).state, SetupState::NoCli);

        let old_cli = ArchFacts { cli_contract: Some("dml-json-v2".to_string()), ..arch_ok() };
        assert_eq!(derive_arch(DISTRO, old_cli).state, SetupState::CliOutdated);

        let no_titles = ArchFacts { titles: Some(0), ..arch_ok() };
        assert_eq!(derive_arch(DISTRO, no_titles).state, SetupState::NoTitles);
    }

    /// Tri-state discipline: each unanswered link must name ITSELF, so the
    /// screen can say which question went dark instead of "something failed".
    #[test]
    fn an_unanswered_link_blocks_at_that_link() {
        for (facts, step) in [
            (ArchFacts { wsl: Tri::Unknown, ..arch_ok() }, SetupStep::Wsl),
            (ArchFacts { distro: Tri::Unknown, ..arch_ok() }, SetupStep::Distro),
            (ArchFacts { prepared: Tri::Unknown, ..arch_ok() }, SetupStep::Prepared),
            (ArchFacts { dockerd: Tri::Unknown, ..arch_ok() }, SetupStep::Engine),
            (ArchFacts { cli: Tri::Unknown, ..arch_ok() }, SetupStep::Cli),
            (ArchFacts { titles: None, ..arch_ok() }, SetupStep::Titles),
        ] {
            let got = derive_arch(DISTRO, facts);
            assert_eq!(got.state, SetupState::Unknown, "{step:?}");
            assert_eq!(got.blocked_at, Some(step));
        }
    }

    /// A dockerd that is merely stopped is REPAIRABLE (start the unit); a
    /// question that went unanswered is not. Collapsing them would put a
    /// "start docker" button in front of a machine that never answered.
    #[test]
    fn a_dockerd_that_did_not_answer_is_not_a_stopped_dockerd() {
        let quiet = ArchFacts { dockerd: Tri::Unknown, ..arch_ok() };
        assert_ne!(derive_arch(DISTRO, quiet).state, SetupState::DockerStopped);
    }

    #[test]
    fn arch_probe_short_circuits_and_never_asks_a_question_it_cannot_answer() {
        // No distro: nothing may be asked INSIDE it.
        let mut asked: Vec<String> = Vec::new();
        let got = probe_arch_with(DISTRO, USER, |args, _| {
            asked.push(args.join(" "));
            ran(0, "Ubuntu\n")
        });
        assert_eq!(got.state, SetupState::NoDistro);
        assert_eq!(asked.len(), 1, "asked {asked:?}");
    }

    /// The in-distro probes must cross the boundary with --exec. The bare --
    /// form runs a shell there (verified 2026-07-28).
    #[test]
    fn every_in_distro_probe_uses_exec() {
        let mut asked: Vec<Vec<String>> = Vec::new();
        let _ = probe_arch_with(DISTRO, USER, |args, _| {
            asked.push(args.iter().map(|s| s.to_string()).collect());
            arch_happy_run(0)(args, ProbeBudget::Warm)
        });
        for argv in asked.iter().filter(|a| a.contains(&"-d".to_string())) {
            assert!(argv.iter().any(|a| a == "--exec"), "shell form in {argv:?}");
            assert!(!argv.iter().any(|a| a == "--"), "shell form in {argv:?}");
        }
        // The titles probe specifically: `sh -c` invoked EXPLICITLY as the
        // program (distro.rs's first-boot-step pattern), not a bare script
        // handed to --exec (which would try to exec a multi-word string as a
        // single binary name and fail).
        let titles_call = asked
            .iter()
            .find(|a| a.iter().any(|e| e == "sh") && !a.iter().any(|e| e == PREPARED_PROBE_SCRIPT))
            .expect("titles probe never ran");
        assert_eq!(titles_call[4], "--exec", "{titles_call:?}");
        assert_eq!(titles_call[5], "sh", "{titles_call:?}");
        assert_eq!(titles_call[6], "-c", "{titles_call:?}");
    }

    /// CRITICAL 1 (fix round 1). `dml-wow version` takes no `--json` — clap
    /// rejects an argument it does not define, and the binary emits its
    /// envelope unconditionally either way (verified live 2026-08-04:
    /// `dml-wow.exe version --json` -> `error: unexpected argument '--json'
    /// found`). Passing it would make the version probe read that clap error
    /// as CouldNotTell forever, on every machine, however healthy.
    #[test]
    fn the_version_probe_passes_no_json_flag_the_binary_rejects() {
        let mut asked: Vec<Vec<String>> = Vec::new();
        let _ = probe_arch_with(DISTRO, USER, |args, budget| {
            asked.push(args.iter().map(|s| s.to_string()).collect());
            arch_happy_run(1)(args, budget)
        });
        let version_call = asked
            .iter()
            .find(|a| a.iter().any(|e| e == "version"))
            .expect("version probe never ran");
        assert_eq!(
            version_call,
            &vec!["-d", DISTRO, "-u", USER, "--exec", "dml-wow", "version"]
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>(),
            "must not carry --json: {version_call:?}"
        );
    }

    // -- classify_arch_cli ----------------------------------------------------

    #[test]
    fn arch_cli_matching_contract_is_yes() {
        let got = classify_arch_cli(&ran(
            0,
            &format!(r#"{{"ok":true,"data":{{"contract":"{EXPECTED_ARCH_CLI_CONTRACT}","version":"0.1.0"}}}}"#),
        ));
        assert_eq!(
            got,
            ArchCliProbe {
                cli: Tri::Yes,
                contract: Some(EXPECTED_ARCH_CLI_CONTRACT.to_string()),
                detail: None,
            }
        );
    }

    /// CRITICAL 2 (fix round 1). A binary speaking a DIFFERENT contract is a
    /// real, actionable answer -- `Yes` with that contract, so `derive_arch`
    /// settles on `CliOutdated` rather than a bare could-not-tell. This is
    /// deliberately its OWN classifier: `classify_cli_version` belongs to the
    /// bash chain's `version`-field contract and must keep working for it.
    #[test]
    fn arch_cli_mismatched_contract_is_yes_with_that_contract() {
        let got = classify_arch_cli(&ran(0, r#"{"ok":true,"data":{"contract":"dml-json-v2"}}"#));
        assert_eq!(
            got,
            ArchCliProbe { cli: Tri::Yes, contract: Some("dml-json-v2".to_string()), detail: None }
        );
    }

    #[test]
    fn arch_cli_program_missing_is_a_definitive_no() {
        // The one genuine negative: "install dml-wow" is a real repair here.
        assert_eq!(
            classify_arch_cli(&ProbeOutcome::ProgramMissing),
            ArchCliProbe { cli: Tri::No, contract: None, detail: None }
        );
    }

    #[test]
    fn arch_cli_unreadable_answers_are_unknown_not_no() {
        for outcome in [
            ProbeOutcome::CouldNotTell,
            ran(1, r#"{"ok":false,"error":{"code":"X","message":"y"}}"#),
            ran(0, "not json at all"),
            // ok:true but no contract field -- a shape this classifier does
            // not recognise, not proof of absence.
            ran(0, r#"{"ok":true,"data":{"version":"0.1.0"}}"#),
        ] {
            let got = classify_arch_cli(&outcome);
            assert_eq!(got.cli, Tri::Unknown, "{outcome:?}");
            assert_eq!(got.contract, None, "{outcome:?}");
        }
    }

    // -- classify_arch_titles --------------------------------------------------

    #[test]
    fn arch_titles_counted_from_a_bare_stdout_number() {
        assert_eq!(classify_arch_titles(&ran(0, "3\n")), Some(3));
        assert_eq!(classify_arch_titles(&ran(0, "0\n")), Some(0));
        assert_eq!(classify_arch_titles(&ran(0, "0")), Some(0));
    }

    #[test]
    fn arch_titles_could_not_tell_stays_none() {
        assert_eq!(classify_arch_titles(&ProbeOutcome::CouldNotTell), None);
        assert_eq!(classify_arch_titles(&ProbeOutcome::ProgramMissing), None);
        assert_eq!(classify_arch_titles(&ran(0, "not a number")), None);
        assert_eq!(classify_arch_titles(&ran(1, "0")), None);
    }

    /// CRITICAL 3 (fix round 1). There is no `games list` on `dml-wow` -- it
    /// is a per-title CLI by design. Titles are counted by a bounded shell
    /// probe reading `$HOME/games` inside the distro, reusing
    /// `COMPOSE_FILE_CANDIDATES` rather than retyping the four names.
    #[test]
    fn chain_counts_titles_via_a_bounded_shell_probe_reusing_the_compose_candidate_list() {
        let mut titles_script: Option<String> = None;
        let st = probe_arch_with(DISTRO, USER, |args, budget| {
            if args.first().copied() == Some("--list") {
                return ran(0, "dml-arch\n");
            }
            if args.iter().any(|a| *a == PREPARED_PROBE_SCRIPT) {
                return ran(0, "/usr/bin/docker");
            }
            if args.iter().any(|a| *a == "is-active") {
                return ran(0, "");
            }
            if args.iter().any(|a| *a == "version") {
                return ran(0, &format!(r#"{{"ok":true,"data":{{"contract":"{EXPECTED_ARCH_CLI_CONTRACT}"}}}}"#));
            }
            // The titles probe.
            titles_script = Some(args[7].to_string());
            arch_happy_run(2)(args, budget)
        });
        assert_eq!(st.state, SetupState::Ready);
        assert_eq!(st.probes.titles, Some(2));

        let script = titles_script.expect("titles probe never ran");
        for name in COMPOSE_FILE_CANDIDATES {
            assert!(script.contains(name), "script missing {name}: {script}");
        }
        // `$HOME`, never a bare `~`: `sh -c` is invoked explicitly as the
        // program (a real shell), but `~` only expands unquoted at the start
        // of a word, which is one more way to get this wrong for free.
        assert!(script.contains("$HOME"), "script must resolve the games dir via $HOME: {script}");
        assert!(!script.contains('~'), "script must not rely on tilde expansion: {script}");
    }

    #[test]
    fn a_binary_speaking_a_different_contract_settles_as_outdated_not_unknown() {
        let st = probe_arch_with(DISTRO, USER, |args, _| {
            if args.first().copied() == Some("--list") {
                ran(0, "dml-arch\n")
            } else if args.iter().any(|a| *a == PREPARED_PROBE_SCRIPT) {
                ran(0, "/usr/bin/docker")
            } else if args.iter().any(|a| *a == "is-active") {
                ran(0, "")
            } else if args.iter().any(|a| *a == "version") {
                ran(0, r#"{"ok":true,"data":{"contract":"dml-json-v2"}}"#)
            } else {
                ran(0, "0\n")
            }
        });
        assert_eq!(st.state, SetupState::CliOutdated);
        assert_eq!(st.probes.cli_version.as_deref(), Some("dml-json-v2"));
    }

    /// IMPORTANT 4 (fix round 1). `ProbeOutcome::ProgramMissing` on the
    /// dockerd call means `wsl.exe` itself failed to spawn a second time
    /// (Windows Update replacing it mid-session, AV interference) -- NOT that
    /// dockerd is stopped. Reading it as `DockerStopped` offers a "start the
    /// container engine" repair that predictably does nothing on a machine
    /// where the real problem is wsl.exe.
    #[test]
    fn wsl_exe_vanishing_between_calls_is_unknown_not_a_stopped_dockerd() {
        let st = probe_arch_with(DISTRO, USER, |args, _| {
            if args.first().copied() == Some("--list") {
                ran(0, "dml-arch\n")
            } else if args.iter().any(|a| *a == PREPARED_PROBE_SCRIPT) {
                // The distro IS prepared, so the only thing left for a
                // vanishing wsl.exe to be evidence about is dockerd.
                ran(0, "/usr/bin/docker")
            } else {
                ProbeOutcome::ProgramMissing
            }
        });
        assert_eq!(st.state, SetupState::Unknown);
        assert_eq!(st.blocked_at, Some(SetupStep::Engine));
    }

    // -- the "registered but unprepared" link ---------------------------------

    /// THE FAILURE SCENARIO this state exists for, end to end.
    ///
    /// Provisioning is eight steps and creates the distro at step 2. If it then
    /// dies — dead mirror, reboot, closed lid — the machine is left with a
    /// REGISTERED `dml-arch` and nothing in it: no packages, no `dml` user.
    /// The next start used to go straight from `Distro` to the dockerd probe,
    /// which runs `systemctl is-active` as a user that does not exist, took the
    /// non-zero exit as "the engine is stopped", and offered "start the
    /// container engine" on a distro that has no container engine. `NoDistro`
    /// could never be reached to say otherwise — the distro really IS
    /// registered — so no state in the enum sent the user back to provisioning.
    #[test]
    fn a_registered_but_unprovisioned_distro_is_not_a_stopped_engine() {
        let st = probe_arch_with(DISTRO, USER, |args, _| {
            if args.first().copied() == Some("--list") {
                return ran(0, "dml-arch");
            }
            // What wsl.exe really says when `-u dml` names nobody.
            ran_err(1, "wsl: The user name or password is incorrect.")
        });
        assert_eq!(st.state, SetupState::DistroUnprepared);
        assert_ne!(st.state, SetupState::DockerStopped);
        assert_eq!(st.blocked_at, None, "this is a settled state with a repair, not a shrug");
    }

    /// ...and it stops there: a distro we cannot even enter must not be asked
    /// three more questions it has no way to answer.
    #[test]
    fn an_unprepared_distro_is_asked_nothing_further() {
        let mut asked: Vec<String> = Vec::new();
        let st = probe_arch_with(DISTRO, USER, |args, _| {
            asked.push(args.join(" "));
            if args.first().copied() == Some("--list") {
                return ran(0, "dml-arch");
            }
            ran_err(1, "wsl: The user name or password is incorrect.")
        });
        assert_eq!(st.state, SetupState::DistroUnprepared);
        assert_eq!(asked.len(), 2, "asked {asked:?}");
    }

    /// The prepared probe is the one that pays for the cold start now — it is
    /// the FIRST call into the distro, so it is the call that boots the WSL2 VM
    /// and systemd. Giving it the warm budget would time it out on exactly the
    /// machine it is meant to describe (a PC that has just booted).
    #[test]
    fn the_prepared_probe_gets_the_cold_start_budget_and_the_rest_do_not() {
        let mut budgets: Vec<(String, ProbeBudget)> = Vec::new();
        let _ = probe_arch_with(DISTRO, USER, |args, budget| {
            budgets.push((args.join(" "), budget));
            arch_happy_run(1)(args, budget)
        });
        let cold: Vec<&String> = budgets
            .iter()
            .filter(|(_, b)| *b == ProbeBudget::ColdStart)
            .map(|(a, _)| a)
            .collect();
        assert_eq!(cold.len(), 1, "exactly one call may pay for a cold start: {cold:?}");
        assert!(
            cold[0].contains(PREPARED_PROBE_SCRIPT),
            "the cold budget must belong to the FIRST in-distro call: {}",
            cold[0]
        );
    }

    #[test]
    fn the_prepared_probe_crosses_the_boundary_as_an_explicit_shell_program() {
        let mut asked: Vec<Vec<String>> = Vec::new();
        let _ = probe_arch_with(DISTRO, USER, |args, budget| {
            asked.push(args.iter().map(|s| s.to_string()).collect());
            arch_happy_run(1)(args, budget)
        });
        let call = asked
            .iter()
            .find(|a| a.iter().any(|e| e == PREPARED_PROBE_SCRIPT))
            .expect("prepared probe never ran");
        assert_eq!(
            call,
            &vec!["-d", DISTRO, "-u", USER, "--exec", "sh", "-c", PREPARED_PROBE_SCRIPT]
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>()
        );
    }

    // -- classify_arch_prepared ------------------------------------------------

    #[test]
    fn prepared_reads_the_resolved_path_before_the_exit_code() {
        assert_eq!(classify_arch_prepared(&ran(0, "/usr/bin/docker")), Tri::Yes);
        // Exit 0 with nothing said is a shell that did not do what we asked —
        // a shrug, not a yes.
        assert_eq!(classify_arch_prepared(&ran(0, "   ")), Tri::Unknown);
    }

    #[test]
    fn prepared_treats_a_distro_it_cannot_enter_as_unprepared_not_unknown() {
        // `command -v` exits 1 when the command is not there, and wsl.exe
        // exits non-zero when `-u dml` names nobody. BOTH are honestly "this
        // distro was never finished", and both have the same repair. Calling
        // them Unknown would offer a retry forever on a machine whose real
        // answer is "run setup again".
        assert_eq!(classify_arch_prepared(&ran(1, "")), Tri::No);
        assert_eq!(
            classify_arch_prepared(&ran_err(1, "wsl: The user name or password is incorrect.")),
            Tri::No
        );
    }

    #[test]
    fn prepared_says_nothing_when_wsl_exe_itself_could_not_answer() {
        assert_eq!(classify_arch_prepared(&ProbeOutcome::ProgramMissing), Tri::Unknown);
        assert_eq!(classify_arch_prepared(&ProbeOutcome::CouldNotTell), Tri::Unknown);
    }

    // -- classify_arch_dockerd -------------------------------------------------

    /// The old classifier read EVERY non-zero exit as "the engine is stopped",
    /// which crossed three failure domains in one call. Exit 3 is systemd's
    /// (and LSB's) documented "program is not running" and is the only one that
    /// means what the repair button says.
    #[test]
    fn only_systemds_documented_not_running_code_is_a_stopped_engine() {
        assert_eq!(classify_arch_dockerd(&ran(0, "")), Tri::Yes);
        assert_eq!(classify_arch_dockerd(&ran(3, "")), Tri::No);
        for code in [1, 4, 5, 127, 255] {
            assert_eq!(
                classify_arch_dockerd(&ran(code, "")),
                Tri::Unknown,
                "exit {code} is not systemd saying the unit is stopped"
            );
        }
        assert_eq!(classify_arch_dockerd(&ProbeOutcome::ProgramMissing), Tri::Unknown);
        assert_eq!(classify_arch_dockerd(&ProbeOutcome::CouldNotTell), Tri::Unknown);
    }

    // -- the stamped contract (whole-branch review, Important 6) --------------

    /// `derive_arch` compares against [`EXPECTED_ARCH_CLI_CONTRACT`] but used to
    /// stamp [`EXPECTED_CLI_VERSION`] — the BASH CLI's own version string, an
    /// entirely different axis — into the struct the UI renders verbatim. On
    /// screen that reads as one sentence built out of two: "dml-arch has DML
    /// backend dml-json-v2 in it and this launcher speaks 3.0.0". Whatever a
    /// screen says the launcher expects must be the thing the code checked.
    #[test]
    fn the_arch_chain_reports_the_contract_it_actually_compares() {
        let old_cli = ArchFacts { cli_contract: Some("dml-json-v2".to_string()), ..arch_ok() };
        let got = derive_arch(DISTRO, old_cli);
        assert_eq!(got.state, SetupState::CliOutdated);
        assert_eq!(got.expected_cli_version, EXPECTED_ARCH_CLI_CONTRACT);
        assert_ne!(
            got.expected_cli_version, EXPECTED_CLI_VERSION,
            "the bash CLI's version has no meaning on this chain"
        );
        // Every arm, not just the interesting one: `unknown_at` stamps it too.
        assert_eq!(
            derive_arch(DISTRO, arch_ok()).expected_cli_version,
            EXPECTED_ARCH_CLI_CONTRACT
        );
        assert_eq!(
            derive_arch(DISTRO, ArchFacts { wsl: Tri::Unknown, ..arch_ok() }).expected_cli_version,
            EXPECTED_ARCH_CLI_CONTRACT
        );
    }

    /// The bash chain is untouched by that fix: it really does compare a
    /// semver, and its screen must keep saying so.
    #[test]
    fn the_bash_chain_still_reports_the_bash_cli_version() {
        let bash = derive(
            DISTRO,
            Probes {
                wsl: Tri::Yes,
                distro: Tri::Yes,
                cli: Tri::No,
                cli_version: None,
                titles: None,
                detail: None,
            },
        );
        assert_eq!(bash.expected_cli_version, EXPECTED_CLI_VERSION);
    }
}
