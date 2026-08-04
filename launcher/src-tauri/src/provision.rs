//! Provision the `dml-arch` distro from the launcher's OWN bundled resources.
//!
//! SHIP-LIST Phase 4.2. Until now the only thing that installed the `dml` CLI,
//! the Eluna bridge scripts and the six title installers into the distro was
//! `cli/dev-install.ps1` — a dev script with a repo path hardcoded in it. This
//! module is the user's route instead: the same install steps, sourced from
//! [`crate::payload`] (the bundler's output, resolved through Tauri's resource
//! dir at runtime), with no repo and no hardcoded path anywhere.
//!
//! SCOPE, deliberately. The distro itself — WSL2 enablement plus `dml-arch` —
//! stays the elevated `Install-DML.ps1`'s job, because creating it needs
//! elevation. Everything AFTER a distro exists is this module. So the two
//! "there is nothing here to fix" states ([`SetupState::NoWsl`],
//! [`SetupState::NoDistro`]) are refusals with a real sentence, never an
//! attempt.
//!
//! WHY `wsl.exe --exec` AND NOT `--`. Verified on this machine, 2026-07-28:
//! `wsl.exe -d D -u dml -- printf '[%s]\n' 'a;whoami' 'b$HOME' '*'` runs the
//! remainder through a SHELL — the `;` split the command, `$HOME` expanded and
//! `*` globbed against the cwd. The same call with `--exec` printed all three
//! back verbatim. Our source paths come from wherever the user installed the
//! launcher (`C:\Program Files\…`, a `$`-bearing folder name, anything), so the
//! `--` form is both a correctness bug and a command-injection surface here.
//! Every spawn in this module uses `--exec` and passes real argv.
//!
//! WHY `wslpath` AND NOT STRING SURGERY. `C:\x` → `/mnt/c/x` is only true for
//! the default automount root; `/etc/wsl.conf` can move it, and a UNC path has
//! no `/mnt/<letter>` form at all. One `wslpath -a -u` call translates the
//! resource dir authoritatively, and every source path is then a pure join onto
//! that — so a launcher installed somewhere WSL genuinely cannot read fails
//! with "WSL could not read …" instead of copying from a path that does not
//! exist.
//!
//! THE PROBE IS NOT RE-IMPLEMENTED HERE. Both gates — "may I run?" before and
//! "did it take?" after — go through [`dml_core::setup::probe_with`], the same
//! chain `backend_status` reports. One question, one answer: the setup command
//! and the first-run screen can never disagree about what state the machine is
//! in, and a fake `run` in the tests below sees the probe calls happen.

use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

use dml_core::events::{done_event, error_event, line_event, section_end, section_start};
use dml_core::proc::{run_bounded_outcome, windows_no_window};
use dml_core::setup::{
    cli_version_matches, probe_with, BackendStatus, ProbeOutcome, SetupState, SetupStep,
    EXPECTED_CLI_VERSION,
};
use serde_json::Value;

use crate::payload::{self, PayloadStatus, INSTALLERS_DIR, INSTALLER_SCRIPTS};
use dml_core::setup::Tri;

/// Terminal section name. Matches the `backend-setup` feature key.
pub const SECTION: &str = "backend-setup";

/// The distro user every copy runs as. `root` owns `/usr/local`; WSL grants it
/// without a password, which is why `dev-install.ps1` has always used it too.
pub const INSTALL_USER: &str = "root";

// Install destinations inside the distro. These are the CLI's own lookup
// paths, not free choices: `/usr/local/bin` is on `PATH`, `_bridge_lua_root`
// (cli/src/50-party.sh) defaults to `/usr/local/share/dml/lua`, and
// `_installers_dir` (cli/src/80-titles.sh) to `/usr/local/share/dml/installers`.
pub const DEST_CLI: &str = "/usr/local/bin/dml";
pub const DEST_LUA_PARTY: &str = "/usr/local/share/dml/lua/party";
pub const DEST_LUA_GM: &str = "/usr/local/share/dml/lua/gm";
pub const DEST_INSTALLERS: &str = "/usr/local/share/dml/installers";
/// The Arch backend's whole runtime. `/usr/local/bin` is on the distro's
/// default PATH, which is why `DmlRunner::arch()` (crates/dml-core/src/runner.rs)
/// can invoke a bare `dml-wow` rather than an absolute path.
pub const DEST_DML_WOW: &str = "/usr/local/bin/dml-wow";

/// Executable payload (the CLI, the title installers).
pub const MODE_EXEC: &str = "0755";
/// Data payload (the Eluna bridge scripts — read by the server, never run).
pub const MODE_DATA: &str = "0644";

/// One bounded spawn's wall clock. Generous on purpose: the FIRST `wsl.exe`
/// call on a cold machine boots the WSL2 VM, which can take the better part of
/// a minute. The probe chain's own 20s default is right for a background
/// status poll and wrong here — a user who pressed a button and is watching a
/// streamed log would rather wait than have their provisioning killed halfway
/// through.
pub const DEFAULT_SETUP_TIMEOUT: Duration = Duration::from_secs(120);

/// Where one step's files land.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dest {
    /// A single file at an exact path (`install -D -m MODE SRC DEST`).
    File(&'static str),
    /// A directory the sources are copied into (`install -D -m MODE -t DIR SRC…`).
    Dir(&'static str),
}

impl Dest {
    pub fn path(&self) -> &'static str {
        match self {
            Dest::File(p) | Dest::Dir(p) => p,
        }
    }
}

/// One install step: a set of resource-relative sources → one destination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallStep {
    /// The sentence streamed before the step runs.
    pub label: &'static str,
    pub mode: &'static str,
    pub dest: Dest,
    /// Resource-relative, forward-slash source paths (`cli/dml`,
    /// `cli/lua/party/dml_login.lua`, …). Joined onto the translated WSL
    /// resource dir at spawn time — never Windows paths.
    pub sources: Vec<String>,
}

/// Why a plan could not be built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanError {
    /// A bridge-script directory exists but holds no `.lua`. Copying nothing
    /// would let `bridge-setup` "succeed" while deploying nothing, so it is an
    /// error here rather than an empty step.
    NoLuaScripts(&'static str),
}

impl PlanError {
    pub fn message(&self) -> String {
        match self {
            PlanError::NoLuaScripts(dir) => format!(
                "This build of the launcher carries no bridge scripts under {dir}."
            ),
        }
    }
}

/// Which distro/user the flow talks to.
#[derive(Debug, Clone)]
pub struct ProvisionCfg {
    pub distro: String,
    /// The unprivileged distro user the PROBES run as (`dml`). Copies always
    /// run as [`INSTALL_USER`].
    pub user: String,
}

impl ProvisionCfg {
    pub fn new(distro: &str, user: &str) -> Self {
        ProvisionCfg { distro: distro.to_string(), user: user.to_string() }
    }
}

/// Where the flow spawns to.
#[derive(Debug, Clone)]
pub struct ProvisionEnv {
    pub wsl_program: OsString,
    pub cfg: ProvisionCfg,
    pub timeout: Duration,
}

impl ProvisionEnv {
    pub fn new(distro: &str, user: &str) -> Self {
        ProvisionEnv {
            wsl_program: OsString::from("wsl.exe"),
            cfg: ProvisionCfg::new(distro, user),
            timeout: DEFAULT_SETUP_TIMEOUT,
        }
    }
}

// ---------------------------------------------------------------------------
// Pure: the step list, and the argv each step becomes
// ---------------------------------------------------------------------------

/// Sorted `*.lua` file names directly under `dir`.
fn lua_names(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|e| e.path().is_file())
        .filter(|e| {
            e.path().extension().and_then(|x| x.to_str()).is_some_and(|x| x.eq_ignore_ascii_case("lua"))
        })
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    // Deterministic order so the streamed log and the tests agree. `read_dir`
    // order is filesystem-defined, not sorted.
    names.sort();
    names
}

/// The four install steps, in the order they run, for a payload laid out under
/// `resource_dir`.
///
/// The bridge scripts are ENUMERATED from disk rather than listed as constants:
/// `payload.rs` bundles `cli/lua/party` and `cli/lua/gm` as directories exactly
/// so a new bridge script is picked up without a code change, and a hardcoded
/// list here would quietly undo that.
pub fn plan(resource_dir: &Path) -> Result<Vec<InstallStep>, PlanError> {
    let p = payload::paths(resource_dir);

    let party = lua_names(&p.lua_party_dir);
    if party.is_empty() {
        return Err(PlanError::NoLuaScripts(payload::LUA_PARTY_DIR));
    }
    let gm = lua_names(&p.lua_gm_dir);
    if gm.is_empty() {
        return Err(PlanError::NoLuaScripts(payload::LUA_GM_DIR));
    }

    Ok(vec![
        InstallStep {
            label: "Installing the dml CLI",
            mode: MODE_EXEC,
            dest: Dest::File(DEST_CLI),
            sources: vec![payload::CLI_SCRIPT.to_string()],
        },
        InstallStep {
            label: "Installing the party bridge scripts",
            mode: MODE_DATA,
            dest: Dest::Dir(DEST_LUA_PARTY),
            sources: party.iter().map(|n| format!("{}/{n}", payload::LUA_PARTY_DIR)).collect(),
        },
        InstallStep {
            label: "Installing the GM bridge scripts",
            mode: MODE_DATA,
            dest: Dest::Dir(DEST_LUA_GM),
            sources: gm.iter().map(|n| format!("{}/{n}", payload::LUA_GM_DIR)).collect(),
        },
        InstallStep {
            label: "Installing the title installers",
            mode: MODE_EXEC,
            dest: Dest::Dir(DEST_INSTALLERS),
            sources: INSTALLER_SCRIPTS.iter().map(|s| format!("{INSTALLERS_DIR}/{s}")).collect(),
        },
        InstallStep {
            label: "Installing the Arch backend",
            // A 0644 binary is not executable, and the failure -- `Permission
            // denied` from `--exec` -- reads like a missing binary rather than
            // a wrong mode, so the mode is part of the diagnosis, not just the
            // function.
            mode: MODE_EXEC,
            dest: Dest::File(DEST_DML_WOW),
            sources: vec![payload::DML_WOW_BIN.to_string()],
        },
    ])
}

/// Join a resource-relative path onto the translated WSL resource dir.
pub fn join_wsl(wsl_dir: &str, rel: &str) -> String {
    format!("{}/{}", wsl_dir.trim_end_matches('/'), rel)
}

/// `wsl.exe` argv that translates `windows_dir` into a distro-visible path.
pub fn wslpath_argv(distro: &str, windows_dir: &str) -> Vec<String> {
    vec![
        "-d".into(),
        distro.into(),
        "-u".into(),
        INSTALL_USER.into(),
        "--exec".into(),
        "wslpath".into(),
        "-a".into(),
        "-u".into(),
        windows_dir.into(),
    ]
}

/// `wsl.exe` argv for one step.
///
/// `install -D` creates every leading directory itself (and, with `-t`, every
/// component of the target directory), so there is no separate `mkdir -p`
/// spawn to get out of sync with the destination list. Verified against the
/// distro's GNU coreutils 9.11.
pub fn install_argv(distro: &str, step: &InstallStep, wsl_resource_dir: &str) -> Vec<String> {
    let mut argv: Vec<String> = vec![
        "-d".into(),
        distro.into(),
        "-u".into(),
        INSTALL_USER.into(),
        // NOT `--`: see the module doc. `--` runs a shell.
        "--exec".into(),
        "install".into(),
        "-D".into(),
        "-m".into(),
        step.mode.into(),
    ];
    match &step.dest {
        Dest::File(dest) => {
            for s in &step.sources {
                argv.push(join_wsl(wsl_resource_dir, s));
            }
            argv.push((*dest).to_string());
        }
        Dest::Dir(dir) => {
            argv.push("-t".into());
            argv.push((*dir).to_string());
            for s in &step.sources {
                argv.push(join_wsl(wsl_resource_dir, s));
            }
        }
    }
    argv
}

// ---------------------------------------------------------------------------
// Messages. Every failure mode gets a sentence that names the thing and the
// next move -- "it didn't work" is not acceptable copy on the one screen a
// stranger cannot get past.
// ---------------------------------------------------------------------------

const HINT_ELEVATED: &str =
    "The distro itself is created by the one-time elevated installer: right-click Install-DML.ps1 and run it as Administrator, then come back here.";

fn step_name(step: SetupStep) -> &'static str {
    match step {
        SetupStep::Wsl => "WSL",
        SetupStep::Distro => "the dml-arch distro",
        SetupStep::Cli => "the dml CLI",
        SetupStep::Titles => "the installed titles",
        // Native-chain steps. This module only ever provisions the WSL distro,
        // so these are unreachable here -- named anyway rather than swept into
        // a `_`, so the next step added still forces a decision.
        SetupStep::Docker => "Docker Desktop",
        SetupStep::Engine => "the Docker engine",
    }
}

/// The refusal for a machine that is not ready for this command, or `None`
/// when the flow may proceed. `(code, message, hint)`.
pub fn refusal(status: &BackendStatus) -> Option<(&'static str, String, String)> {
    match status.state {
        SetupState::NoWsl => Some((
            "NO_WSL",
            "WSL is not available on this PC, so there is no distro to set up.".to_string(),
            HINT_ELEVATED.to_string(),
        )),
        SetupState::NoDistro => Some((
            "NO_DISTRO",
            format!(
                "WSL works, but the '{}' distro is not installed on this PC.",
                status.distro
            ),
            HINT_ELEVATED.to_string(),
        )),
        SetupState::Unknown => {
            let at = status.blocked_at.map(step_name).unwrap_or("this PC");
            Some((
                "PROBE_FAILED",
                format!("Could not check {at}, so setup did not start. Nothing was changed."),
                "WSL may still be starting or may be wedged. Wait a moment and try again; if it keeps failing, run 'wsl --shutdown' from PowerShell and retry."
                    .to_string(),
            ))
        }
        // NoCli and CliOutdated are exactly what this command fixes. NoTitles
        // and Ready mean it is already provisioned -- re-running is safe and
        // is the honest way to repair a half-broken install, so it is allowed.
        SetupState::NoCli | SetupState::CliOutdated | SetupState::NoTitles | SetupState::Ready => None,
        // Native-chain states. This command provisions the WSL distro, so
        // reaching it in native mode means the caller routed wrongly -- refuse
        // and say which backend it belongs to rather than half-running.
        SetupState::NoDocker | SetupState::DockerStopped => Some((
            "WRONG_BACKEND",
            "This sets up the WSL distro, and this PC is running in native Docker mode.".to_string(),
            "Nothing was changed. Native mode needs no distro -- install a server from the Library instead."
                .to_string(),
        )),
    }
}

/// The refusal for a payload that did not ship with this build, or `None`.
///
/// The `dml-wow` entry gets its own hint. Every other entry missing means the
/// same thing (a user's install is incomplete or was moved — "reinstall the
/// launcher" is the right advice), but `dml-wow` missing/a placeholder is a
/// RELEASE-PROCESS gap: the maintainer building the installer forgot to stage
/// the CI-built Linux binary before running `npm run tauri build`. Telling a
/// user to "reinstall" fixes nothing there — every copy of that build has the
/// same placeholder, so the actionable line has to name what actually broke.
pub fn payload_refusal(payload: &PayloadStatus) -> Option<(&'static str, String, String)> {
    match payload.present {
        Tri::Yes => None,
        Tri::No => {
            let dml_wow_missing = payload.missing.iter().any(|m| m == payload::DML_WOW_BIN);
            Some((
                "PAYLOAD_MISSING",
                format!(
                    "This copy of the launcher is missing {} of the backend files it installs: {}.",
                    payload.missing.len(),
                    payload.missing.join(", ")
                ),
                if dml_wow_missing {
                    "The Linux backend binary was not bundled into this build. This is a release \
                     mistake, not a broken install: whoever built this installer needs to stage the \
                     real dml-wow binary (CI's dml-wow-linux-x86_64 artifact) at \
                     launcher/src-tauri/backend/dml-wow before running `npm run tauri build`, then \
                     rebuild and re-ship it."
                        .to_string()
                } else {
                    "The install is incomplete or was moved. Reinstall the launcher and try again."
                        .to_string()
                },
            ))
        }
        Tri::Unknown => Some((
            "PAYLOAD_UNKNOWN",
            "Could not locate the launcher's own program folder, so the backend files it installs could not be found.".to_string(),
            "Reinstall the launcher and try again.".to_string(),
        )),
    }
}

/// What the pre-probe found, as one sentence — the honesty requirement for a
/// second run.
pub fn found_line(status: &BackendStatus) -> String {
    match status.state {
        SetupState::NoCli => format!("No dml CLI in {} yet - installing it.", status.distro),
        SetupState::CliOutdated => format!(
            "Found dml {} in {} - replacing it with {}.",
            status.probes.cli_version.as_deref().unwrap_or("(unknown)"),
            status.distro,
            EXPECTED_CLI_VERSION
        ),
        // Ready / NoTitles: already provisioned. Say so rather than pretending
        // this is a first install.
        _ => format!(
            "dml {EXPECTED_CLI_VERSION} is already installed in {} - reinstalling it from this app's copy.",
            status.distro
        ),
    }
}

/// The verification verdict after the copies, or `None` when it took.
pub fn verify_failure(after: &BackendStatus) -> Option<(&'static str, String, String)> {
    match after.state {
        SetupState::NoCli => Some((
            "VERIFY_NO_CLI",
            format!(
                "The files were copied, but dml still does not run inside {}.",
                after.distro
            ),
            "Open a shell in the distro (wsl -d dml-arch) and run 'dml version' to see what it says."
                .to_string(),
        )),
        SetupState::CliOutdated => Some((
            "VERIFY_VERSION",
            format!(
                "Installed, but dml reports version {} - this launcher speaks {}.",
                after.probes.cli_version.as_deref().unwrap_or("(unknown)"),
                after.expected_cli_version
            ),
            "Something else on PATH is shadowing /usr/local/bin/dml, or the launcher and the CLI are from different builds. Reinstall the launcher and try again."
                .to_string(),
        )),
        SetupState::Unknown => {
            let at = after.blocked_at.map(step_name).unwrap_or("the distro");
            Some((
                "VERIFY_UNKNOWN",
                format!("The files were copied, but the check afterwards could not reach {at}."),
                "The install may well be fine. Try the check again in a moment.".to_string(),
            ))
        }
        // NoWsl/NoDistro here would mean the distro vanished mid-run; treat it
        // the same as any other failed verification rather than as success.
        // A native-chain answer here would mean the backend flipped mid-run.
        // Grouped with the vanished-distro case for the same reason: the copies
        // happened, the verification did not, and calling that success is the
        // one thing this function must never do.
        SetupState::NoWsl
        | SetupState::NoDistro
        | SetupState::NoDocker
        | SetupState::DockerStopped => Some((
            "VERIFY_UNKNOWN",
            format!("The files were copied, but {} could no longer be reached.", after.distro),
            "Try again in a moment.".to_string(),
        )),
        SetupState::NoTitles | SetupState::Ready => None,
    }
}

/// One line of a failed spawn's output, for the error message. The picking and
/// bounding now live in [`dml_core::setup::first_diagnostic_line`] — the probe
/// chain needs the identical thing for the could-not-tell screen's `detail`
/// (SHIP-LIST Phase 4 review, P9), and two copies of "quote one line of what
/// wsl.exe said" is exactly the drift this codebase keeps paying for. The only
/// difference kept here is the fallback: an error SENTENCE has to say
/// something, a diagnostics line may simply be absent.
fn first_diagnostic_line(stdout: &str, stderr: &str) -> String {
    dml_core::setup::first_diagnostic_line(stdout, stderr)
        .unwrap_or_else(|| "no output".to_string())
}

// ---------------------------------------------------------------------------
// The flow
// ---------------------------------------------------------------------------

fn fail(emit: &impl Fn(Value), code: &str, message: impl Into<String>, hint: &str) {
    emit(section_end(SECTION, "error"));
    emit(error_event(code, message, hint));
}

/// Provision the distro, streaming progress. Never returns an error: an
/// unhappy outcome is an `error` event on the stream, matching every other
/// streamed command's contract (the UI derives its verdict from the
/// `done`/`error` events, not from the promise).
///
/// `run` receives `wsl.exe` argv and answers with what happened — the single
/// injection seam, shared with the probe chain so the tests see BOTH.
///
/// The chain's per-probe [`dml_core::setup::ProbeBudget`] is deliberately
/// IGNORED here: this flow already gives every spawn [`DEFAULT_SETUP_TIMEOUT`]
/// (120s), which is the cold-start budget anyway, and a user who pressed a
/// button and is watching a streamed log would rather wait than have their
/// provisioning killed halfway through.
pub fn provision_with(
    cfg: &ProvisionCfg,
    resource_dir: Option<&Path>,
    mut run: impl FnMut(&[&str]) -> ProbeOutcome,
    emit: impl Fn(Value),
) {
    emit(section_start(SECTION));

    // 1. May we run? The same chain `backend_status` reports -- not a second
    //    opinion about the same machine.
    emit(line_event("info", "Checking this PC..."));
    let before = probe_with(&cfg.distro, &cfg.user, |a, _budget| run(a));
    if let Some((code, message, hint)) = refusal(&before) {
        fail(&emit, code, message, &hint);
        return;
    }

    // 2. Did the payload we would install actually ship inside this exe?
    let payload_status = payload::resolve_opt(resource_dir);
    if let Some((code, message, hint)) = payload_refusal(&payload_status) {
        fail(&emit, code, message, &hint);
        return;
    }
    // `resolve_opt` only returns `Yes` for a `Some(dir)`, so this cannot fail.
    let Some(dir) = resource_dir else {
        fail(
            &emit,
            "PAYLOAD_UNKNOWN",
            "Could not locate the launcher's own program folder.",
            "Reinstall the launcher and try again.",
        );
        return;
    };
    let steps = match plan(dir) {
        Ok(s) => s,
        Err(e) => {
            fail(&emit, "PAYLOAD_MISSING", e.message(), "Reinstall the launcher and try again.");
            return;
        }
    };

    emit(line_event("info", found_line(&before)));

    // 3. Translate the resource dir ONCE; every source path is a join onto it.
    let win_dir = dir.to_string_lossy().into_owned();
    let wsl_dir = {
        let argv = wslpath_argv(&cfg.distro, &win_dir);
        let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
        match run(&refs) {
            ProbeOutcome::Ran { code: Some(0), stdout, .. } if !stdout.trim().is_empty() => {
                stdout.trim().to_string()
            }
            ProbeOutcome::Ran { stdout, stderr, .. } => {
                fail(
                    &emit,
                    "PATH_UNREACHABLE",
                    format!(
                        "WSL could not read the launcher's program folder ({win_dir}): {}",
                        first_diagnostic_line(&stdout, &stderr)
                    ),
                    "Install the launcher on a local drive (C:) rather than a network share or a removable disk, then try again.",
                );
                return;
            }
            _ => {
                fail(
                    &emit,
                    "PATH_UNREACHABLE",
                    format!("WSL did not answer when asked to read {win_dir}."),
                    "Wait a moment and try again; if it keeps failing, run 'wsl --shutdown' from PowerShell and retry.",
                );
                return;
            }
        }
    };

    // 4. Copy. `install` overwrites, so a second run is a clean replace -- the
    //    whole flow is idempotent by construction.
    let mut files = 0usize;
    for step in &steps {
        // TOCTOU defence for the Arch backend binary specifically. Step 2's
        // `payload::resolve` proved the bundle was a real ELF at an EARLIER
        // moment; between then and this exact spawn the file on disk could
        // have changed underneath us (AV quarantine-and-replace, a
        // half-finished re-stage, ...), and the very next thing that happens
        // to it is `install -m 0755` -- chmod executable, ready for the
        // distro's PATH to find and run. Re-read the magic bytes immediately
        // before that spawn and refuse rather than deploy-and-hope; this is
        // NOT redundant with the earlier check, it closes the gap between it
        // and the actual copy.
        if step.dest.path() == DEST_DML_WOW {
            let bin_path = payload::paths(dir).dml_wow_bin;
            if !payload::is_elf(&bin_path) {
                fail(
                    &emit,
                    "PAYLOAD_MISSING",
                    format!(
                        "The Arch backend binary at {} is no longer a valid Linux executable.",
                        payload::DML_WOW_BIN
                    ),
                    "This copy of the launcher's files changed on disk during setup. Reinstall the launcher and try again.",
                );
                return;
            }
        }
        emit(line_event("info", format!("{}...", step.label)));
        let argv = install_argv(&cfg.distro, step, &wsl_dir);
        let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
        match run(&refs) {
            ProbeOutcome::Ran { code: Some(0), .. } => files += step.sources.len(),
            ProbeOutcome::Ran { stdout, stderr, .. } => {
                fail(
                    &emit,
                    "COPY_FAILED",
                    format!(
                        "Could not install {}: {}",
                        step.dest.path(),
                        first_diagnostic_line(&stdout, &stderr)
                    ),
                    // NOT "nothing was changed": earlier steps really did land.
                    // Say what is true, and say that a retry is safe -- every
                    // step overwrites, so re-running repairs a half-finished run.
                    "Setup stopped here; the steps before this one are still in place, and running setup again is safe. If the distro is out of disk space or read-only, fix that first.",
                );
                return;
            }
            _ => {
                fail(
                    &emit,
                    "COPY_FAILED",
                    format!("WSL did not answer while installing {}.", step.dest.path()),
                    "Wait a moment and try again; if it keeps failing, run 'wsl --shutdown' from PowerShell and retry.",
                );
                return;
            }
        }
    }

    // 5. Verify -- same chain again, so "installed" means the same thing the
    //    first-run screen will independently see.
    emit(line_event("info", "Checking the installed CLI..."));
    let after = probe_with(&cfg.distro, &cfg.user, |a, _budget| run(a));
    if let Some((code, message, hint)) = verify_failure(&after) {
        fail(&emit, code, message, &hint);
        return;
    }

    let reinstalled = before
        .probes
        .cli_version
        .as_deref()
        .is_some_and(cli_version_matches);
    emit(line_event(
        "info",
        if reinstalled {
            format!("Backend reinstalled - dml {EXPECTED_CLI_VERSION} is in place.")
        } else {
            format!("Backend installed - dml {EXPECTED_CLI_VERSION} is ready.")
        },
    ));
    emit(section_end(SECTION, "ok"));
    emit(done_event(serde_json::json!({
        "files": files,
        "reinstalled": reinstalled,
        "previous_cli_version": before.probes.cli_version,
        // The post-probe, so a caller can update its first-run screen without
        // a second round trip (and can never disagree with what we just did).
        "backend": after,
    })));
}

/// Run the flow for real.
pub fn provision(env: &ProvisionEnv, resource_dir: Option<&Path>, emit: impl Fn(Value)) {
    provision_with(
        &env.cfg,
        resource_dir,
        |args| {
            let mut cmd = std::process::Command::new(&env.wsl_program);
            cmd.args(args);
            windows_no_window(&mut cmd);
            ProbeOutcome::from_bounded(run_bounded_outcome(cmd, env.timeout))
        },
        emit,
    )
}

/// Every destination the plan writes to, in order. Used by the drift guard
/// against `cli/dev-install.ps1`.
pub fn destinations() -> [(&'static str, &'static str); 5] {
    [
        (DEST_CLI, MODE_EXEC),
        (DEST_LUA_PARTY, MODE_DATA),
        (DEST_LUA_GM, MODE_DATA),
        (DEST_INSTALLERS, MODE_EXEC),
        (DEST_DML_WOW, MODE_EXEC),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    // Only the tests build paths; production here only ever borrows them, so
    // PathBuf lives with its user rather than as an import the release build
    // warns about.
    use std::path::PathBuf;

    const DISTRO: &str = "dml-arch";
    const USER: &str = "dml";

    // -- fixtures ------------------------------------------------------------

    fn tmp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir()
            .join(format!("dml-provision-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A minimal, valid ELF header prefix -- just enough for `payload::is_elf`
    /// to read it as a real binary. Mirrors `payload::tests::FAKE_ELF`.
    const FAKE_ELF: &[u8] = b"\x7fELF\x02\x01\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00";

    /// A complete fake resource dir, the shape `bundle.resources` produces.
    fn complete_payload(root: &Path) {
        std::fs::create_dir_all(root.join("cli")).unwrap();
        std::fs::write(root.join(payload::CLI_SCRIPT), "#!/usr/bin/env bash\n").unwrap();
        std::fs::create_dir_all(root.join(payload::LUA_PARTY_DIR)).unwrap();
        for n in ["dml_addclass.lua", "dml_login.lua"] {
            std::fs::write(root.join(payload::LUA_PARTY_DIR).join(n), "-- x\n").unwrap();
        }
        std::fs::create_dir_all(root.join(payload::LUA_GM_DIR)).unwrap();
        std::fs::write(root.join(payload::LUA_GM_DIR).join("dml_gm.lua"), "-- x\n").unwrap();
        std::fs::create_dir_all(root.join(INSTALLERS_DIR)).unwrap();
        for s in INSTALLER_SCRIPTS {
            std::fs::write(root.join(INSTALLERS_DIR).join(s), "#!/usr/bin/env bash\n").unwrap();
        }
        std::fs::create_dir_all(root.join("backend")).unwrap();
        std::fs::write(root.join(payload::DML_WOW_BIN), FAKE_ELF).unwrap();
    }

    fn ran(code: i32, stdout: &str) -> ProbeOutcome {
        ProbeOutcome::Ran { code: Some(code), stdout: stdout.to_string(), stderr: String::new() }
    }

    fn ran_err(code: i32, stderr: &str) -> ProbeOutcome {
        ProbeOutcome::Ran { code: Some(code), stdout: String::new(), stderr: stderr.to_string() }
    }

    /// The three probe replies for a distro with no `dml` in it.
    fn probe_no_cli() -> Vec<ProbeOutcome> {
        vec![ran(0, "dml-arch\r\n"), ran_err(127, "bash: dml: command not found")]
    }

    /// The three probe replies for a fully provisioned distro.
    fn probe_ready() -> Vec<ProbeOutcome> {
        vec![
            ran(0, "dml-arch\r\n"),
            ran(0, r#"{"ok":true,"data":{"version":"3.0.0"}}"#),
            ran(0, r#"{"ok":true,"data":{"games":[{"id":"wow-server-playerbots"}]}}"#),
        ]
    }

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
        fn install_calls(&self) -> Vec<Vec<String>> {
            self.calls().into_iter().filter(|c| c.iter().any(|a| a == "install")).collect()
        }
    }

    /// Collector for the emitted stream.
    #[derive(Default)]
    struct Events(std::cell::RefCell<Vec<Value>>);

    impl Events {
        fn push(&self, v: Value) {
            self.0.borrow_mut().push(v);
        }
        fn all(&self) -> Vec<Value> {
            self.0.borrow().clone()
        }
        fn kinds(&self) -> Vec<String> {
            self.all().iter().map(|v| v["event"].as_str().unwrap_or("?").to_string()).collect()
        }
        fn error(&self) -> Option<Value> {
            self.all().into_iter().find(|v| v["event"] == "error")
        }
        fn done(&self) -> Option<Value> {
            self.all().into_iter().find(|v| v["event"] == "done")
        }
        fn lines(&self) -> String {
            self.all()
                .iter()
                .filter(|v| v["event"] == "line")
                .filter_map(|v| v["text"].as_str().map(str::to_string))
                .collect::<Vec<_>>()
                .join("\n")
        }
    }

    // -- plan ----------------------------------------------------------------

    #[test]
    fn plan_covers_every_piece_of_the_payload_with_the_right_mode() {
        // Breaks if a step is dropped, a destination is retargeted, or a mode
        // is loosened -- e.g. shipping the CLI 0644 (unrunnable) or the bridge
        // scripts 0755.
        let d = tmp_dir("plan-all");
        complete_payload(&d);
        let steps = plan(&d).expect("complete payload plans");
        let got: Vec<(&str, &str)> =
            steps.iter().map(|s| (s.dest.path(), s.mode)).collect();
        assert_eq!(
            got,
            vec![
                (DEST_CLI, MODE_EXEC),
                (DEST_LUA_PARTY, MODE_DATA),
                (DEST_LUA_GM, MODE_DATA),
                (DEST_INSTALLERS, MODE_EXEC),
                (DEST_DML_WOW, MODE_EXEC),
            ]
        );
        assert_eq!(steps[0].dest, Dest::File(DEST_CLI), "the CLI is one exact file, not a dir drop");
        assert_eq!(steps[0].sources, vec![payload::CLI_SCRIPT.to_string()]);
        assert_eq!(steps[3].sources.len(), 6, "all six title installers");
        assert_eq!(steps[4].dest, Dest::File(DEST_DML_WOW), "the Arch binary is one exact file, not a dir drop");
        assert_eq!(steps[4].sources, vec![payload::DML_WOW_BIN.to_string()]);
        assert_eq!(steps[4].mode, MODE_EXEC, "a 0644 binary cannot run");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn the_binary_is_deployed_executable_to_a_path_on_the_distro_path() {
        // /usr/local/bin is on the distro's default PATH, which is why
        // `DmlRunner::arch()` can invoke a bare `dml-wow` rather than an
        // absolute path.
        let found = destinations()
            .into_iter()
            .find(|(dest, _)| *dest == DEST_DML_WOW)
            .expect("the Linux binary must be a provisioning destination");
        assert_eq!(found.0, "/usr/local/bin/dml-wow");
        assert_eq!(found.1, MODE_EXEC);
    }

    #[test]
    fn the_binary_step_is_mode_0755_because_a_0644_binary_cannot_run() {
        let d = tmp_dir("provision-plan-bin");
        complete_payload(&d);
        let steps = plan(&d).expect("plan");
        let step = steps
            .iter()
            .find(|s| s.dest.path() == DEST_DML_WOW)
            .expect("binary step");
        assert_eq!(step.mode, "0755");
        assert_eq!(step.sources, vec![payload::DML_WOW_BIN.to_string()]);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn plan_enumerates_the_bridge_scripts_it_finds_rather_than_a_fixed_list() {
        // payload.rs bundles the lua as DIRECTORIES precisely so a new bridge
        // script needs no code change. A hardcoded name list here would undo
        // that and silently stop shipping the new script.
        let d = tmp_dir("plan-lua");
        complete_payload(&d);
        std::fs::write(d.join(payload::LUA_PARTY_DIR).join("dml_brand_new.lua"), "-- x\n").unwrap();
        let steps = plan(&d).unwrap();
        assert_eq!(
            steps[1].sources,
            vec![
                format!("{}/dml_addclass.lua", payload::LUA_PARTY_DIR),
                format!("{}/dml_brand_new.lua", payload::LUA_PARTY_DIR),
                format!("{}/dml_login.lua", payload::LUA_PARTY_DIR),
            ],
            "sorted, and every .lua present"
        );
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn plan_refuses_an_empty_bridge_directory() {
        // Copying nothing would let bridge-setup "succeed" while deploying
        // nothing -- the exact silent-no-op class dml_wow::bridge already had
        // to fix once.
        let d = tmp_dir("plan-empty-lua");
        complete_payload(&d);
        std::fs::remove_file(d.join(payload::LUA_GM_DIR).join("dml_gm.lua")).unwrap();
        assert_eq!(plan(&d), Err(PlanError::NoLuaScripts(payload::LUA_GM_DIR)));
        std::fs::remove_dir_all(&d).unwrap();
    }

    // -- argv ----------------------------------------------------------------

    #[test]
    fn install_argv_uses_exec_so_no_shell_can_touch_the_paths() {
        // THE security-relevant assertion. Verified on this machine: the `--`
        // form runs a shell (`;` splits, `$HOME` expands, `*` globs), and our
        // source paths come from wherever the user installed the launcher.
        let d = tmp_dir("argv-exec");
        complete_payload(&d);
        let steps = plan(&d).unwrap();
        // Non-vacuity: an empty plan or an empty argv would satisfy every
        // "must not contain" assertion below without proving anything.
        assert_eq!(steps.len(), 5, "the plan must have steps to check");
        for step in &steps {
            let argv = install_argv(DISTRO, step, "/mnt/c/Program Files/DML Launcher");
            assert!(argv.len() >= 6, "argv must be a real command: {argv:?}");
            assert!(argv.contains(&"--exec".to_string()), "{argv:?}");
            assert!(
                !argv.iter().any(|a| a == "--"),
                "the bare `--` form runs a shell; never use it here: {argv:?}"
            );
            assert!(
                !argv.iter().any(|a| a == "bash" || a == "sh" || a == "-lc" || a == "-c"),
                "no shell may be interposed: {argv:?}"
            );
        }
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn install_argv_for_a_file_destination() {
        let step = InstallStep {
            label: "x",
            mode: MODE_EXEC,
            dest: Dest::File(DEST_CLI),
            sources: vec![payload::CLI_SCRIPT.to_string()],
        };
        let argv = install_argv(DISTRO, &step, "/mnt/c/Program Files/DML Launcher");
        assert_eq!(
            argv,
            vec![
                "-d",
                DISTRO,
                "-u",
                "root",
                "--exec",
                "install",
                "-D",
                "-m",
                MODE_EXEC,
                "/mnt/c/Program Files/DML Launcher/cli/dml",
                DEST_CLI,
            ]
        );
    }

    #[test]
    fn install_argv_for_a_directory_destination_uses_target_directory() {
        // Without `-t` the LAST source would be treated as the destination and
        // the second-to-last file would be copied over it.
        let step = InstallStep {
            label: "x",
            mode: MODE_DATA,
            dest: Dest::Dir(DEST_LUA_GM),
            sources: vec!["cli/lua/gm/a.lua".into(), "cli/lua/gm/b.lua".into()],
        };
        let argv = install_argv(DISTRO, &step, "/mnt/c/app");
        assert_eq!(
            argv,
            vec![
                "-d",
                DISTRO,
                "-u",
                "root",
                "--exec",
                "install",
                "-D",
                "-m",
                MODE_DATA,
                "-t",
                DEST_LUA_GM,
                "/mnt/c/app/cli/lua/gm/a.lua",
                "/mnt/c/app/cli/lua/gm/b.lua",
            ]
        );
    }

    #[test]
    fn install_argv_runs_as_root_and_probes_do_not() {
        // /usr/local is root-owned; the probes must NOT run privileged.
        let step = InstallStep {
            label: "x",
            mode: MODE_EXEC,
            dest: Dest::File(DEST_CLI),
            sources: vec![payload::CLI_SCRIPT.to_string()],
        };
        let argv = install_argv(DISTRO, &step, "/mnt/c/app");
        let u = argv.iter().position(|a| a == "-u").expect("-u present");
        assert_eq!(argv[u + 1], "root");
    }

    #[test]
    fn wslpath_argv_asks_the_distro_to_translate_the_windows_dir() {
        assert_eq!(
            wslpath_argv(DISTRO, r"C:\Program Files\DML Launcher"),
            vec![
                "-d",
                DISTRO,
                "-u",
                "root",
                "--exec",
                "wslpath",
                "-a",
                "-u",
                r"C:\Program Files\DML Launcher",
            ]
        );
    }

    #[test]
    fn join_wsl_never_doubles_a_separator() {
        assert_eq!(join_wsl("/mnt/c/app", "cli/dml"), "/mnt/c/app/cli/dml");
        assert_eq!(join_wsl("/mnt/c/app/", "cli/dml"), "/mnt/c/app/cli/dml");
    }

    // -- the flow: refusals --------------------------------------------------

    #[test]
    fn refuses_when_the_distro_is_absent_and_installs_nothing() {
        let d = tmp_dir("flow-no-distro");
        complete_payload(&d);
        let fake = Fake::new(vec![ran(0, "Ubuntu\r\n")]);
        let ev = Events::default();
        provision_with(&ProvisionCfg::new(DISTRO, USER), Some(&d), |a| fake.run(a), |v| ev.push(v));

        let err = ev.error().expect("an error event");
        assert_eq!(err["error"]["code"], "NO_DISTRO");
        assert!(
            err["error"]["message"].as_str().unwrap().contains(DISTRO),
            "the message must name the distro: {err}"
        );
        assert!(
            err["error"]["hint"].as_str().unwrap().contains("Install-DML.ps1"),
            "the hint must name the elevated installer: {err}"
        );
        assert!(fake.install_calls().is_empty(), "nothing may be copied into a distro that is not there");
        assert!(ev.done().is_none(), "a refusal is never a done");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn refuses_when_wsl_itself_is_missing() {
        let d = tmp_dir("flow-no-wsl");
        complete_payload(&d);
        let fake = Fake::new(vec![ProbeOutcome::ProgramMissing]);
        let ev = Events::default();
        provision_with(&ProvisionCfg::new(DISTRO, USER), Some(&d), |a| fake.run(a), |v| ev.push(v));
        assert_eq!(ev.error().unwrap()["error"]["code"], "NO_WSL");
        assert!(fake.install_calls().is_empty());
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_probe_that_could_not_tell_is_a_retry_not_a_repair() {
        // The tri-state rule with a button on it: a wedged wsl.exe must not be
        // reported as "your distro is missing", and must not start copying.
        let d = tmp_dir("flow-unknown");
        complete_payload(&d);
        let fake = Fake::new(vec![ProbeOutcome::CouldNotTell]);
        let ev = Events::default();
        provision_with(&ProvisionCfg::new(DISTRO, USER), Some(&d), |a| fake.run(a), |v| ev.push(v));
        let err = ev.error().unwrap();
        assert_eq!(err["error"]["code"], "PROBE_FAILED");
        assert!(
            err["error"]["message"].as_str().unwrap().contains("Nothing was changed"),
            "{err}"
        );
        assert!(fake.install_calls().is_empty());
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn refuses_when_the_bundled_payload_did_not_ship() {
        // "The MSI shipped the GUI and nothing else" -- the exact state
        // SHIP-LIST 4.1 fixed. If it ever regresses, the setup command must
        // say so instead of running four copies of nothing.
        let d = tmp_dir("flow-no-payload");
        let fake = Fake::new(probe_no_cli());
        let ev = Events::default();
        provision_with(&ProvisionCfg::new(DISTRO, USER), Some(&d), |a| fake.run(a), |v| ev.push(v));
        let err = ev.error().unwrap();
        assert_eq!(err["error"]["code"], "PAYLOAD_MISSING");
        assert!(
            err["error"]["message"].as_str().unwrap().contains(payload::CLI_SCRIPT),
            "the message must name what is missing: {err}"
        );
        assert!(fake.install_calls().is_empty());
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn refuses_when_the_resource_dir_could_not_be_resolved_at_all() {
        let fake = Fake::new(probe_no_cli());
        let ev = Events::default();
        provision_with(&ProvisionCfg::new(DISTRO, USER), None, |a| fake.run(a), |v| ev.push(v));
        assert_eq!(ev.error().unwrap()["error"]["code"], "PAYLOAD_UNKNOWN");
        assert!(fake.install_calls().is_empty());
    }

    // -- the flow: failures during the work ----------------------------------

    #[test]
    fn reports_a_resource_dir_wsl_cannot_read() {
        let d = tmp_dir("flow-wslpath");
        complete_payload(&d);
        let mut replies = probe_no_cli();
        replies.push(ran_err(1, "wslpath: Z:\\app: Invalid argument"));
        let fake = Fake::new(replies);
        let ev = Events::default();
        provision_with(&ProvisionCfg::new(DISTRO, USER), Some(&d), |a| fake.run(a), |v| ev.push(v));
        let err = ev.error().unwrap();
        assert_eq!(err["error"]["code"], "PATH_UNREACHABLE");
        assert!(
            err["error"]["message"].as_str().unwrap().contains("Invalid argument"),
            "the real wslpath output must reach the user: {err}"
        );
        assert!(fake.install_calls().is_empty());
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_failed_copy_names_the_destination_and_the_reason_and_stops() {
        let d = tmp_dir("flow-copy-failed");
        complete_payload(&d);
        let mut replies = probe_no_cli();
        replies.push(ran(0, "/mnt/c/app\n")); // wslpath
        replies.push(ran(0, "")); // cli install OK
        replies.push(ran_err(1, "install: cannot create regular file '/usr/local/share/dml/lua/party/x.lua': No space left on device"));
        let fake = Fake::new(replies);
        let ev = Events::default();
        provision_with(&ProvisionCfg::new(DISTRO, USER), Some(&d), |a| fake.run(a), |v| ev.push(v));
        let err = ev.error().unwrap();
        assert_eq!(err["error"]["code"], "COPY_FAILED");
        let msg = err["error"]["message"].as_str().unwrap();
        assert!(msg.contains(DEST_LUA_PARTY), "must name the destination: {msg}");
        assert!(msg.contains("No space left on device"), "must carry the real reason: {msg}");
        assert_eq!(
            fake.install_calls().len(),
            2,
            "a failed step must stop the run, not press on to the next one"
        );
        // A half-finished run is the state most likely to make someone give up
        // or start deleting things. The hint must say that the steps that DID
        // land are still good and that pressing setup again is safe -- claiming
        // "nothing was changed" here would be a lie, since the CLI already
        // installed.
        let hint = err["error"]["hint"].as_str().unwrap();
        assert!(hint.contains("again is safe"), "hint must license a retry: {hint}");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_version_mismatch_after_installing_is_reported_with_both_versions() {
        let d = tmp_dir("flow-version");
        complete_payload(&d);
        let mut replies = probe_no_cli();
        replies.push(ran(0, "/mnt/c/app\n"));
        replies.extend((0..5).map(|_| ran(0, "")));
        // Verify probe: a `dml` that is there but is the wrong build.
        replies.push(ran(0, "dml-arch\r\n"));
        replies.push(ran(0, r#"{"ok":true,"data":{"version":"2.6.0"}}"#));
        let fake = Fake::new(replies);
        let ev = Events::default();
        provision_with(&ProvisionCfg::new(DISTRO, USER), Some(&d), |a| fake.run(a), |v| ev.push(v));
        let err = ev.error().unwrap();
        assert_eq!(err["error"]["code"], "VERIFY_VERSION");
        let msg = err["error"]["message"].as_str().unwrap();
        assert!(msg.contains("2.6.0"), "must name what was found: {msg}");
        assert!(msg.contains(EXPECTED_CLI_VERSION), "must name what was expected: {msg}");
        assert!(ev.done().is_none(), "a failed verification is not a success");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_cli_that_still_does_not_run_after_the_copies_is_reported() {
        let d = tmp_dir("flow-verify-nocli");
        complete_payload(&d);
        let mut replies = probe_no_cli();
        replies.push(ran(0, "/mnt/c/app\n"));
        replies.extend((0..5).map(|_| ran(0, "")));
        replies.push(ran(0, "dml-arch\r\n"));
        replies.push(ran_err(127, "bash: dml: command not found"));
        let fake = Fake::new(replies);
        let ev = Events::default();
        provision_with(&ProvisionCfg::new(DISTRO, USER), Some(&d), |a| fake.run(a), |v| ev.push(v));
        assert_eq!(ev.error().unwrap()["error"]["code"], "VERIFY_NO_CLI");
        std::fs::remove_dir_all(&d).unwrap();
    }

    /// TOCTOU defence, not redundancy with `payload::resolve`'s earlier check.
    /// That check (flow step 2) proves the bundle is a real ELF at an EARLIER
    /// moment; this proves the file is re-verified immediately before the
    /// actual `install -m 0755` spawn -- the point at which the next thing
    /// that happens to it is a chmod and, moments later, a real exec inside
    /// the distro.
    ///
    /// Fix round 1 (vacuous-pass trap caught by mutation review): corrupting
    /// the file BEFORE the run starts makes the EARLIER gate (step 2) refuse
    /// for the identical reason and the same visible outcome
    /// (`PAYLOAD_MISSING`, nothing installed to `/usr/local/bin/dml-wow`) --
    /// so the pre-exec recheck this test exists to prove never gets a turn.
    /// Disabling the recheck entirely left the naive version of this test
    /// green. To actually exercise it, the file must still be a GOOD ELF when
    /// the earlier gate reads it, and go bad only afterwards: this `run`
    /// wrapper corrupts the binary right after the FOURTH copy spawn (CLI,
    /// party-lua, gm-lua, installers -- identified by the installers step's
    /// own destination arg, the last thing installed before dml-wow), so the
    /// recheck is the ONLY thing standing between the now-corrupted file and
    /// the fifth spawn (`install -m 0755 ... /usr/local/bin/dml-wow`).
    #[test]
    fn the_binary_is_re_checked_for_elf_magic_immediately_before_the_chmod() {
        let d = tmp_dir("flow-toctou");
        complete_payload(&d);
        let bin_path = d.join(payload::DML_WOW_BIN);

        let fake = Fake::new(happy_replies());
        let ev = Events::default();
        provision_with(
            &ProvisionCfg::new(DISTRO, USER),
            Some(&d),
            |a| {
                let out = fake.run(a);
                if a.iter().any(|arg| *arg == DEST_INSTALLERS) {
                    std::fs::write(&bin_path, b"not an elf any more").unwrap();
                }
                out
            },
            |v| ev.push(v),
        );

        let err = ev.error().expect("an error event");
        assert_eq!(err["error"]["code"], "PAYLOAD_MISSING");
        assert!(
            err["error"]["message"].as_str().unwrap().contains(payload::DML_WOW_BIN),
            "{err}"
        );
        // The half that makes this about the recheck rather than about error
        // plumbing: the corrupted file must never reach the copy/chmod spawn.
        assert!(
            !fake.install_calls().iter().any(|c| c.iter().any(|a| a == DEST_DML_WOW)),
            "a corrupted binary must never reach an install/chmod spawn: {:?}",
            fake.install_calls()
        );
        assert!(ev.done().is_none(), "a refused deploy is never a success");
        std::fs::remove_dir_all(&d).unwrap();
    }

    // -- the flow: success ---------------------------------------------------

    /// Replies for a full, clean provisioning run from a distro with no CLI.
    fn happy_replies() -> Vec<ProbeOutcome> {
        let mut replies = probe_no_cli();
        replies.push(ran(0, "/mnt/c/Program Files/DML Launcher\n"));
        replies.extend((0..5).map(|_| ran(0, "")));
        replies.extend(probe_ready());
        replies
    }

    #[test]
    fn a_clean_run_copies_every_step_and_finishes_with_done() {
        let d = tmp_dir("flow-happy");
        complete_payload(&d);
        let fake = Fake::new(happy_replies());
        let ev = Events::default();
        provision_with(&ProvisionCfg::new(DISTRO, USER), Some(&d), |a| fake.run(a), |v| ev.push(v));

        assert!(ev.error().is_none(), "unexpected error: {:?}", ev.error());
        let done = ev.done().expect("a done event");
        // 1 CLI + 2 party lua + 1 gm lua + 6 installers + 1 dml-wow binary.
        assert_eq!(done["data"]["files"], 11);
        assert_eq!(done["data"]["reinstalled"], false);
        assert!(done["data"]["previous_cli_version"].is_null());
        // The post-probe rides along so a first-run screen needs no second
        // round trip and cannot disagree with what just happened.
        assert_eq!(done["data"]["backend"]["state"], "ready");

        let kinds = ev.kinds();
        assert_eq!(kinds.first().map(String::as_str), Some("section_start"));
        assert_eq!(kinds.iter().filter(|k| *k == "section_end").count(), 1);
        assert_eq!(kinds.last().map(String::as_str), Some("done"));
        assert_eq!(fake.install_calls().len(), 5);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn the_source_paths_are_the_translated_wsl_dir_never_windows_paths() {
        // The whole point of 4.2: no repo path, no drive letter, no user name
        // baked in. A regression to `C:\...` sources would make `install`
        // fail inside the distro with a confusing "No such file".
        let d = tmp_dir("flow-sources");
        complete_payload(&d);
        let fake = Fake::new(happy_replies());
        let ev = Events::default();
        provision_with(&ProvisionCfg::new(DISTRO, USER), Some(&d), |a| fake.run(a), |v| ev.push(v));

        let cli_call = &fake.install_calls()[0];
        assert!(
            cli_call.contains(&"/mnt/c/Program Files/DML Launcher/cli/dml".to_string()),
            "sources must be joined onto wslpath's answer: {cli_call:?}"
        );
        for arg in fake.calls().iter().flatten() {
            assert!(
                !arg.contains('\\') || arg.contains("wslpath") || arg == &d.to_string_lossy(),
                "a Windows path leaked into a distro-side argument: {arg}"
            );
        }
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn the_flow_asks_the_probe_chain_before_and_after_rather_than_its_own_questions() {
        // "Consume backend_status, do not re-probe." Both gates must be the
        // same `wsl --list --quiet` -> `dml version --json` chain the status
        // command reports, so the two can never disagree.
        let d = tmp_dir("flow-probes");
        complete_payload(&d);
        let fake = Fake::new(happy_replies());
        let ev = Events::default();
        provision_with(&ProvisionCfg::new(DISTRO, USER), Some(&d), |a| fake.run(a), |v| ev.push(v));

        let calls = fake.calls();
        assert_eq!(calls[0], vec!["--list", "--quiet"], "the chain leads");
        assert_eq!(calls[1], vec!["-d", DISTRO, "-u", USER, "--", "dml", "version", "--json"]);
        let list_calls = calls.iter().filter(|c| c.first().map(String::as_str) == Some("--list")).count();
        assert_eq!(list_calls, 2, "the same chain runs again to verify");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_second_run_says_it_reinstalled_rather_than_claiming_a_fresh_install() {
        // Idempotency, reported honestly: running setup twice is safe, and the
        // second run must not pretend it installed something that was already
        // there.
        let d = tmp_dir("flow-second-run");
        complete_payload(&d);
        let mut replies = probe_ready();
        replies.push(ran(0, "/mnt/c/app\n"));
        replies.extend((0..5).map(|_| ran(0, "")));
        replies.extend(probe_ready());
        let fake = Fake::new(replies);
        let ev = Events::default();
        provision_with(&ProvisionCfg::new(DISTRO, USER), Some(&d), |a| fake.run(a), |v| ev.push(v));

        assert!(ev.error().is_none(), "a second run is not a failure: {:?}", ev.error());
        let done = ev.done().unwrap();
        assert_eq!(done["data"]["reinstalled"], true);
        assert_eq!(done["data"]["previous_cli_version"], EXPECTED_CLI_VERSION);
        assert!(
            ev.lines().contains("already installed"),
            "the log must say what it found: {}",
            ev.lines()
        );
        assert_eq!(fake.install_calls().len(), 5, "a repair run still replaces every file");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn an_outdated_cli_is_reported_as_a_replacement_by_version() {
        let d = tmp_dir("flow-outdated");
        complete_payload(&d);
        let mut replies =
            vec![ran(0, "dml-arch\r\n"), ran(0, r#"{"ok":true,"data":{"version":"2.6.0"}}"#)];
        replies.push(ran(0, "/mnt/c/app\n"));
        replies.extend((0..5).map(|_| ran(0, "")));
        replies.extend(probe_ready());
        let fake = Fake::new(replies);
        let ev = Events::default();
        provision_with(&ProvisionCfg::new(DISTRO, USER), Some(&d), |a| fake.run(a), |v| ev.push(v));

        assert!(ev.error().is_none());
        let lines = ev.lines();
        assert!(lines.contains("2.6.0"), "must name the old version it is replacing: {lines}");
        assert!(lines.contains(EXPECTED_CLI_VERSION), "and the new one: {lines}");
        assert_eq!(ev.done().unwrap()["data"]["reinstalled"], false);
        std::fs::remove_dir_all(&d).unwrap();
    }

    // -- messages ------------------------------------------------------------

    fn status_in(state: SetupState) -> BackendStatus {
        BackendStatus {
            state,
            blocked_at: Some(SetupStep::Distro),
            detail: None,
            distro: DISTRO.to_string(),
            expected_cli_version: EXPECTED_CLI_VERSION.to_string(),
            probes: dml_core::setup::Probes::unprobed(),
        }
    }

    #[test]
    fn refusal_covers_exactly_the_three_states_this_command_cannot_fix() {
        // BOTH directions in one test on purpose: a `refusal` that always
        // answered `None` would satisfy a standalone "these states are allowed"
        // test while silently letting the command try to provision a PC with no
        // WSL on it.
        for (state, code) in [
            (SetupState::NoWsl, "NO_WSL"),
            (SetupState::NoDistro, "NO_DISTRO"),
            (SetupState::Unknown, "PROBE_FAILED"),
        ] {
            let (got_code, msg, hint) = refusal(&status_in(state)).expect("a refusal");
            assert_eq!(got_code, code);
            // "It didn't work" is not acceptable copy on the one screen a
            // stranger cannot get past: every refusal names a thing and a move.
            assert!(msg.len() > 30, "message too thin for {code}: {msg}");
            assert!(hint.len() > 30, "hint too thin for {code}: {hint}");
            assert!(msg.ends_with('.'), "message must be a sentence for {code}: {msg}");
        }
        for state in [
            SetupState::NoCli,
            SetupState::CliOutdated,
            SetupState::NoTitles,
            SetupState::Ready,
        ] {
            assert!(refusal(&status_in(state)).is_none(), "{state:?} must be allowed to run");
        }
    }

    #[test]
    fn verify_failure_covers_every_state_that_is_not_a_working_install() {
        // Same both-directions shape: only a distro that now answers with the
        // contract version counts as done.
        for (state, code) in [
            (SetupState::NoCli, "VERIFY_NO_CLI"),
            (SetupState::CliOutdated, "VERIFY_VERSION"),
            (SetupState::Unknown, "VERIFY_UNKNOWN"),
            (SetupState::NoWsl, "VERIFY_UNKNOWN"),
            (SetupState::NoDistro, "VERIFY_UNKNOWN"),
        ] {
            let (got_code, msg, hint) = verify_failure(&status_in(state)).expect("a verdict");
            assert_eq!(got_code, code, "{state:?}");
            assert!(msg.len() > 30, "message too thin for {state:?}: {msg}");
            assert!(hint.len() > 20, "hint too thin for {state:?}: {hint}");
        }
        for state in [SetupState::NoTitles, SetupState::Ready] {
            assert!(verify_failure(&status_in(state)).is_none(), "{state:?} is a success");
        }
    }

    #[test]
    fn payload_refusal_only_lets_a_complete_payload_through() {
        assert!(payload_refusal(&PayloadStatus {
            present: Tri::Yes,
            dir: Some("C:/x".into()),
            missing: Vec::new(),
            dml_wow_bin_present: false,
        })
        .is_none());
        let (code, msg, _) = payload_refusal(&PayloadStatus {
            present: Tri::No,
            dir: Some("C:/x".into()),
            missing: vec![payload::CLI_SCRIPT.to_string()],
            dml_wow_bin_present: false,
        })
        .expect("a refusal");
        assert_eq!(code, "PAYLOAD_MISSING");
        assert!(msg.contains(payload::CLI_SCRIPT), "{msg}");
        assert_eq!(
            payload_refusal(&payload::resolve_opt(None)).map(|r| r.0),
            Some("PAYLOAD_UNKNOWN")
        );
    }

    /// A missing/placeholder `dml-wow` is a maintainer's release mistake, not
    /// a user's broken install -- "reinstall the launcher" fixes nothing
    /// there, because every copy of that build carries the same placeholder.
    /// The hint must say so distinctly from the generic missing-file case.
    #[test]
    fn payload_refusal_names_a_missing_dml_wow_as_a_release_mistake_not_a_broken_install() {
        let (code, msg, hint) = payload_refusal(&PayloadStatus {
            present: Tri::No,
            dir: Some("C:/x".into()),
            missing: vec![payload::DML_WOW_BIN.to_string()],
            dml_wow_bin_present: false,
        })
        .expect("a refusal");
        assert_eq!(code, "PAYLOAD_MISSING");
        assert!(msg.contains(payload::DML_WOW_BIN), "{msg}");
        assert!(
            hint.to_lowercase().contains("linux") || hint.to_lowercase().contains("dml-wow"),
            "hint does not name the actual cause: {hint}"
        );
        assert!(
            !hint.to_lowercase().contains("reinstall"),
            "the generic 'reinstall the launcher' advice fixes nothing here -- every copy of this \
             build has the same placeholder: {hint}"
        );

        // The generic case must be untouched: a missing file that is NOT
        // dml-wow keeps the original "reinstall" hint.
        let (_, _, other_hint) = payload_refusal(&PayloadStatus {
            present: Tri::No,
            dir: Some("C:/x".into()),
            missing: vec![payload::CLI_SCRIPT.to_string()],
            dml_wow_bin_present: true,
        })
        .expect("a refusal");
        assert!(other_hint.to_lowercase().contains("reinstall"), "{other_hint}");
    }

    #[test]
    fn first_diagnostic_line_prefers_stderr_and_stays_bounded() {
        assert_eq!(first_diagnostic_line("out", "the real error"), "the real error");
        assert_eq!(first_diagnostic_line("out", "   \n\nthe real error\nmore"), "the real error");
        assert_eq!(first_diagnostic_line("only stdout", ""), "only stdout");
        assert_eq!(first_diagnostic_line("", ""), "no output");
        let long = "x".repeat(5000);
        let got = first_diagnostic_line("", &long);
        assert!(got.chars().count() <= 203, "unbounded error text: {} chars", got.chars().count());
    }

    // -- the drift guard against the dev script ------------------------------

    #[test]
    fn dev_install_ps1_installs_the_same_destinations_at_the_same_modes() {
        // `cli/dev-install.ps1` stays the dev loop's route (SHIP-LIST 4.2 says
        // keep it), which means there are now TWO provisioners. They cannot be
        // literally one list -- one is Rust, one is PowerShell -- so this is
        // the seam that fails when they drift: a fifth payload piece added
        // here and forgotten there would ship to users and never reach the dev
        // machine (or vice versa), and the difference would only show up as a
        // "works on my box" bug.
        let script = include_str!("../../../cli/dev-install.ps1");
        assert_eq!(destinations().len(), 5, "the destination list must be the real one");
        for (dest, mode) in destinations() {
            // dev-install.ps1 is a Windows PowerShell dev script; it can never
            // build or cross-compile the Linux dml-wow ELF (no such toolchain
            // is assumed on a dev machine). That binary comes from CI and is
            // staged into the installer by the release process (payload.rs /
            // `npm run tauri build`), never by this script -- so it is exempt
            // from the drift check that applies to everything the script DOES
            // own.
            if dest == DEST_DML_WOW {
                continue;
            }
            assert!(
                script.contains(dest),
                "cli/dev-install.ps1 does not install to {dest}"
            );
            assert!(
                script.contains(&format!("-m {mode}")),
                "cli/dev-install.ps1 never uses mode {mode} (needed for {dest})"
            );
        }
        for s in INSTALLER_SCRIPTS {
            assert!(script.contains(s), "cli/dev-install.ps1 does not install {s}");
        }
    }

    #[test]
    fn dev_install_ps1_has_no_hardcoded_user_or_drive_path() {
        // It was `/mnt/c/Users/perzi/dads-mmo-lab` on line 4, which is why it
        // could not run on anybody else's machine. It must resolve its own
        // location at runtime.
        let script = include_str!("../../../cli/dev-install.ps1");
        assert!(
            !script.contains("/mnt/c/Users/"),
            "a hardcoded user path is back in cli/dev-install.ps1"
        );
        assert!(
            script.contains("PSScriptRoot"),
            "cli/dev-install.ps1 must resolve the repo from its own location"
        );
    }

    // -- the cross-lane command contract -------------------------------------

    #[test]
    fn the_setup_command_exists_is_registered_and_is_named_the_same_on_both_sides() {
        // Three separate ways this ships broken, all silent until a click:
        // a command that is defined but never listed in `generate_handler!`
        // answers "command not found"; a rename on either side of the IPC does
        // the same. The frontend half (api.ts) is another lane's file, which is
        // exactly why the name is pinned here rather than agreed in prose.
        let rust = include_str!("lib.rs");
        assert!(
            rust.contains("async fn backend_setup("),
            "no backend_setup command in lib.rs"
        );
        assert!(
            rust.contains("backend_setup,"),
            "backend_setup is defined but missing from the invoke_handler list"
        );
        let ts = include_str!("../../src/lib/api.ts");
        assert!(
            ts.contains("invoke(\"backend_setup\""),
            "api.ts does not invoke backend_setup"
        );
        // It must be the STREAMING shape: a one-shot command would leave the
        // user staring at a frozen button for the length of a WSL cold boot.
        assert!(
            ts.contains("invoke(\"backend_setup\", { onEvent: ch })"),
            "backend_setup must be invoked with a Channel, not as a one-shot call"
        );
    }

    // -- the real spawn, bounded ---------------------------------------------

    /// LIVE, opt-in. Everything above proves the state machine against a fake
    /// `run`; NONE of it proves that the argv we build is a command the distro
    /// will actually accept. That is a real gap — `install -D -t`, `wslpath -a
    /// -u` and `--exec`'s verbatim argv are all assumptions about a specific
    /// GNU coreutils and a specific wsl.exe.
    ///
    /// `#[ignore]` rather than a runtime skip, deliberately: a self-skipping
    /// test that quietly passes on a machine with no WSL is the vacuous-pass
    /// trap (CLAUDE.md), and this one also WRITES to the distro. Run it by
    /// hand, on a machine that has the distro:
    ///
    ///   cargo test -p launcher --lib provision::tests::live_ -- --ignored --nocapture
    ///
    /// It installs the same files `cli/dev-install.ps1` installs, from
    /// `target/debug` (which `tauri-build` populates from `bundle.resources`
    /// on every build), so running it is equivalent to a dev-install.
    ///
    /// NEW precondition since Task 9: `target/debug/backend/dml-wow` must be a
    /// REAL Linux ELF, not `build.rs`'s inert placeholder stub -- an ordinary
    /// dev build never has one (see `payload::tests::the_real_build_lays_the_
    /// payload_out_where_paths_looks_for_it`), and `provision_with` now
    /// refuses at the payload-resolve gate (`PAYLOAD_MISSING`) before this
    /// test's copy phase ever runs without it. Stage a real
    /// `dml-wow-linux-x86_64` build at that path by hand before running this.
    #[test]
    #[ignore = "writes to the real dml-arch distro; run by hand with --ignored"]
    fn live_provisioning_a_real_distro_installs_and_verifies() {
        // The dev-run resource dir: `resource_dir()` is the exe's own directory
        // on Windows, and this test binary sits in `target/debug/deps`.
        let exe = std::env::current_exe().expect("test exe path");
        let resource_dir = exe.parent().and_then(Path::parent).expect("target/debug");

        let ev = Events::default();
        provision(&ProvisionEnv::new(DISTRO, USER), Some(resource_dir), |v| {
            eprintln!("{v}");
            ev.push(v);
        });

        assert!(ev.error().is_none(), "live provisioning failed: {:?}", ev.error());
        let done = ev.done().expect("a done event");
        // 1 CLI + 4 party lua + 2 gm lua + 6 installers + 1 dml-wow binary.
        assert_eq!(done["data"]["files"], 14);
        // The verification probe is the proof that matters: the CLI we just
        // copied really runs inside the distro and really answers with the
        // contract version.
        assert_eq!(done["data"]["backend"]["probes"]["cli"], "yes");
        assert_eq!(
            done["data"]["backend"]["probes"]["cli_version"],
            EXPECTED_CLI_VERSION
        );
    }

    #[test]
    fn provision_against_a_missing_wsl_exe_refuses_at_once() {
        // Boundedness: a missing program must cost nothing and must land on a
        // definitive refusal, not a timeout.
        let d = tmp_dir("real-no-wsl");
        complete_payload(&d);
        let mut env = ProvisionEnv::new(DISTRO, USER);
        env.wsl_program = OsString::from("definitely-not-a-real-wsl-9f2.exe");
        let ev = Events::default();
        let start = std::time::Instant::now();
        provision(&env, Some(&d), |v| ev.push(v));
        let elapsed = start.elapsed();
        assert_eq!(ev.error().unwrap()["error"]["code"], "NO_WSL");
        assert!(elapsed < Duration::from_secs(5), "must fail fast, took {elapsed:?}");
        std::fs::remove_dir_all(&d).unwrap();
    }
}
