//! Native-install preflight — the gate that REFUSES a doomed source build.
//!
//! WHY THIS EXISTS. Building AzerothCore from source takes hours. On a machine
//! that cannot finish it, an unguarded attempt costs those hours and then dies
//! — which happened for real on 2026-07-29, and is the reason ratified decision
//! 3 of `docs/superpowers/plans/2026-07-29-native-first-install.md` says the
//! preflight refuses rather than warns. The cost asymmetry is the whole
//! argument: a false refusal costs one `--allow-underspec` flag, a false pass
//! costs an evening.
//!
//! THE TRI-STATE RULE, restated because this codebase keeps paying for it.
//! `docker` failing to answer is evidence of NOTHING — not "Docker is not
//! installed", not "the machine is fine". It is only evidence that the install
//! cannot proceed right now. So the docker probe answers
//! [`dml_core::setup::Tri`] via the existing [`ProbeOutcome`] idiom, and a
//! could-not-tell never becomes a sentence that asserts a cause. This matters
//! here more than usual because of a REAL captured sample from this box with
//! the engine stopped:
//!
//! ```text
//! $ docker info --format '{{.MemTotal}}|{{.NCPU}}|{{.DockerRootDir}}|{{.ServerVersion}}'
//! exit=1
//! stdout: 0|0||
//! stderr: failed to connect to the docker API at npipe:////./pipe/dockerDesktopLinuxEngine; ...
//! ```
//!
//! A down engine PRINTS ZEROES. Anything that reads `MemTotal` without first
//! establishing that the engine answered will conclude "0 GB of RAM" and
//! refuse with a fabricated number. Hence [`classify_docker_info`] only trusts
//! a reading once it has positive evidence the engine spoke.
//!
//! REPORT THE NUMBERS, NOT A VERDICT. "4.2 GB available, needs 6.0 GB" tells a
//! user what to change; "preflight failed" does not. Every [`Finding`] carries
//! the measured value and the floor it was compared against, and the
//! `--allow-underspec` override keeps them in the copy rather than replacing
//! the message with a shrug.
//!
//! PURITY. Everything that DECIDES is pure and lives here with its unit tests;
//! the impure half (spawning `docker info`, asking the filesystem for free
//! space) only gathers [`PreflightFacts`]. That split is what lets the floors
//! be tested just-under / at / just-over instead of on whatever hardware CI
//! happens to run.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use dml_core::proc::{run_bounded_outcome, windows_no_window};
use dml_core::setup::{ProbeOutcome, Tri};
use serde::Serialize;

// ---------------------------------------------------------------------------
// Floors. Every number below is justified from evidence this project already
// paid for — see the per-constant comments. They are `pub` on purpose: the
// copy quotes them, the tests compare against them, and a silent edit here is
// exactly what the boundary tests exist to catch.
// ---------------------------------------------------------------------------

/// One gibibyte. Sizes are reported in 1024-base and labelled "GB", which is
/// what Windows' own Explorer/Task Manager and Docker Desktop's Resources
/// slider show — the numbers a user will compare ours against.
pub const GB: u64 = 1024 * 1024 * 1024;

/// Peak resident memory of ONE C++ compiler job in an AzerothCore build.
///
/// EVIDENCE: `guides/DML-Windows/Install-DML.ps1` Step 6 (lines 734-741) sizes
/// the WSL distro with `memBoundCores = floor(wslRamGB / 2)` under the comment
/// "one C++ compiler per core, ~2 GB each at peak". That figure was measured
/// against this exact build, not guessed, and it is the only per-job number
/// this project has ever established.
pub const MEM_PER_COMPILER_JOB_BYTES: u64 = 2 * GB;

/// Hard floor: below this the install REFUSES.
///
/// EVIDENCE: at ~2 GB per compiler job, 6 GB is the point where the Docker VM
/// can still hold two concurrent jobs plus the MySQL container and the linker
/// peak. Below it the build reaches the heaviest translation units and the
/// Linux OOM killer SIGKILLs a compiler.
///
/// WHY THAT IS A REFUSAL AND NOT A WARNING: an OOM kill does not present as an
/// error that names memory. `Install-DML.ps1`'s own comment records the
/// symptom — the build "dies at the same low % every retry" — so what the user
/// actually sees is a bare `Killed` / signal-9 line buried hours into a build
/// log, with no cause attached. A user cannot diagnose that, so they retry the
/// same multi-hour build. Refusing up front is the only place the truth is
/// cheap.
pub const REFUSE_MEM_BYTES: u64 = 6 * GB;

/// Comfort floor: between [`REFUSE_MEM_BYTES`] and this we WARN and proceed.
///
/// EVIDENCE: the same `floor(RAM / 2 GB)` formula caps build parallelism at 4
/// jobs (`min(4, hostCores, memBoundCores)`); 8 GB is the first VM size at
/// which that capped maximum fits without swapping. Between 6 and 8 GB the
/// build works but is memory-bound, which is a real warning and not a refusal.
pub const WARN_MEM_BYTES: u64 = 8 * GB;

/// Hard floor for free space on the drive holding Docker's data root.
///
/// EVIDENCE, all in-repo: the downloaded client-data volume is ~6 GB and the
/// built AzerothCore/MySQL images are ~3-5 GB (`crates/dml-wow-cli/src/cli.rs`
/// `GamesRemove`'s `--keep-data`/`--remove-images` help text, and the same
/// numbers in `cli/src/90-main.sh`). A SOURCE build additionally holds the full
/// azerothcore-wotlk checkout and BuildKit's build cache. The WSL installer's
/// own 15 GB check (`install-wow-wotlk.sh:137-142`) sizes the PULL path, not a
/// source build, so it is not the right floor here.
///
/// And the cost compounds: Docker Desktop's `ext4.vhdx` grows and never shrinks
/// by itself (the vhdx-shrink tooling was deliberately dropped for native mode,
/// `poc/native-docker/README.md:76-79`), so a user who retries a failing build
/// pays the cache cost again rather than reusing the space.
pub const REFUSE_FREE_BYTES: u64 = 40 * GB;

/// Comfort floor for free space: below this we WARN and proceed.
///
/// EVIDENCE: [`REFUSE_FREE_BYTES`] is barely enough for ONE clean build with no
/// room to retry. 60 GB is that plus headroom for a second build cache, which
/// is the normal outcome when a first attempt fails and is re-run.
pub const WARN_FREE_BYTES: u64 = 60 * GB;

/// Hard floor for free space on the drive holding the GAMES DIR — a different
/// drive from Docker's data root as often as not, and the reason this check
/// exists separately at all.
///
/// EVIDENCE, measured on this machine 2026-07-29 against the very checkout a
/// Route A install reproduces (`du -sh --exclude=env/dist/data
/// ~/games/wow-server-playerbots` inside `dml-arch`):
///
/// ```text
/// 2.4G   /home/dml/games/wow-server-playerbots     (whole checkout)
/// 1.3G   .../.git                                  (full history, not shallow)
/// 127M   .../modules
/// 1.2M   .../env
/// ```
///
/// The finished tree is 2.4 GB, but the PEAK is higher than the result: a
/// clone lands the ~1.3 GB pack, indexes it, and then writes the ~1.1 GB
/// working tree, so it needs roughly 3.7 GB before anything is compiled. 8 GB
/// is that peak plus the room a second attempt needs — retrying is the normal
/// response to a failed clone, and a half-finished checkout is usually still
/// sitting there when it happens.
///
/// NOT USER-RATIFIED. DECISIONS TAKEN #3 in the plan ratified the memory and
/// Docker-data-root numbers only; these two are derived from the measurement
/// above and should be confirmed at the Task 4 gate.
pub const REFUSE_GAMES_FREE_BYTES: u64 = 8 * GB;

/// Comfort floor for free space on the games-dir drive.
///
/// EVIDENCE: same measurement — 15 GB is one clone peak (~3.7 GB), the
/// finished 2.4 GB tree kept alongside it during a re-clone, and room for the
/// title's logs to grow, without the drive going critical.
pub const WARN_GAMES_FREE_BYTES: u64 = 15 * GB;

/// Machine-readable code for "git is not on this machine". NOT overridable:
/// `--allow-underspec` is about hardware floors, and no flag makes `git clone`
/// run without git.
pub const CODE_GIT_MISSING: &str = "INSTALL_GIT_MISSING";

/// What the Docker Hub probe asks for. The registry answers `401` to an
/// unauthenticated `/v2/` — an ANSWER, which is all this probe wants. Same
/// endpoint the WSL installer's `check_docker_hub` uses
/// (`guides/wow-wotlk/install-wow-wotlk.sh:304-334`).
pub const DOCKER_HUB_PROBE_URL: &str = "https://registry-1.docker.io/v2/";

/// Machine-readable code for "this machine is below a hardware floor".
/// Overridable with `--allow-underspec`.
pub const CODE_UNDERSPEC: &str = "INSTALL_UNDERSPEC";

/// Machine-readable code for "docker did not answer". NOT overridable — the
/// build literally cannot run, and no flag changes that.
pub const CODE_DOCKER_UNREACHABLE: &str = "INSTALL_DOCKER_UNREACHABLE";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// What the preflight decided. One value the caller switches on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Nothing to say — start the build.
    Proceed,
    /// Start the build, but the user has been told what will hurt.
    Warn,
    /// Do not start. `PreflightReport::code` says which kind.
    Refuse,
}

/// Severity of one individual check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Ok,
    Warn,
    Refuse,
}

/// Which question a [`Finding`] answers.
///
/// One variant per question plan Task 4 asks. They are listed there as a set
/// on purpose: a preflight that answers four of six questions and says nothing
/// about the other two reads exactly like a preflight that passed, which is
/// how a machine with no git and no network got a `Proceed`.
/// [`checks_task_4_requires`] pins the set so a dropped family is a test
/// failure rather than a silence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Check {
    Docker,
    /// `git` itself — the clone is the first thing the engine does.
    Git,
    /// Can we reach the registry the base images come from?
    DockerHub,
    Memory,
    /// Free space where DOCKER keeps its data (images, build cache, volumes).
    Disk,
    /// Free space where the CHECKOUT goes. Not the same drive as [`Check::Disk`]
    /// on any machine that moved Docker's disk image to a bigger volume — the
    /// single most common fix for a full C:.
    GamesDisk,
    Cpu,
}

/// Every question the preflight must answer before an install may start.
///
/// Source: plan Task 4 — "docker engine reachable ...; Docker VM resources ...;
/// free disk on BOTH the games-dir drive and docker info's DockerRootDir
/// drive; Docker Hub reachability (port of check_docker_hub); git present".
/// [`Check::Cpu`] is deliberately absent: it is an advisory that only exists
/// when both of its inputs are real.
pub fn checks_task_4_requires() -> [Check; 6] {
    [
        Check::Docker,
        Check::Git,
        Check::DockerHub,
        Check::Memory,
        Check::Disk,
        Check::GamesDisk,
    ]
}

/// One check's answer, with the numbers in the message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    pub check: Check,
    pub severity: Severity,
    /// Human sentence. MUST contain the measured value and, when it is a
    /// shortfall, the floor it fell short of.
    pub message: String,
    /// What to do about it. Empty for `Ok`.
    pub hint: String,
}

/// Everything the docker probe learned. `reachable` is the tri-state; the
/// readings are `None` whenever they could not be trusted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockerFacts {
    pub reachable: Tri,
    /// One line of the engine's own complaint, for the report. `None` when
    /// there was nothing to quote.
    pub detail: Option<String>,
    /// RAM available to Docker's Linux VM, in bytes.
    pub mem_bytes: Option<u64>,
    /// CPUs available to Docker's Linux VM.
    pub ncpu: Option<u32>,
    /// The engine's own `DockerRootDir` — a path INSIDE the VM on Docker
    /// Desktop, which is why it is not used blindly as a host path.
    pub docker_root_dir: Option<String>,
    pub server_version: Option<String>,
}

impl Default for DockerFacts {
    /// The honest zero value: nothing has been probed yet, so every answer is
    /// a shrug. `Tri` has no `Default` of its own precisely so that a struct
    /// like this has to choose `Unknown` deliberately rather than inherit a
    /// `No` from somewhere.
    fn default() -> Self {
        Self {
            reachable: Tri::Unknown,
            detail: None,
            mem_bytes: None,
            ncpu: None,
            docker_root_dir: None,
            server_version: None,
        }
    }
}

/// Free space on the drive Docker's data lives on. `free_bytes` is `None` when
/// the path was resolved but could not be measured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskFacts {
    pub path: String,
    pub free_bytes: Option<u64>,
}

/// Everything the impure half gathered. The pure [`decide`] reads only this.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreflightFacts {
    pub docker: DockerFacts,
    /// `None` = we could not even work out WHICH drive to measure, which is a
    /// different (and equally honest) could-not-tell from a path we failed to
    /// read.
    pub disk: Option<DiskFacts>,
    /// Free space where the checkout goes. `None` when no games dir was
    /// resolved at all.
    pub games_disk: Option<DiskFacts>,
    /// True when the games dir and Docker's data root live on the SAME volume.
    /// Then the two demands are not independent — they come out of one pool —
    /// and checking each against its own floor passes a drive that cannot hold
    /// both. See [`same_volume`].
    pub games_shares_docker_volume: bool,
    /// Is `git` on this machine? [`Tri::No`] only for a definitively absent
    /// program.
    pub git: Tri,
    /// Did the image registry answer? See [`classify_hub_probe`] for why this
    /// never becomes a [`Tri::No`].
    pub hub: Tri,
    /// The probe's own words when it could not tell, for the report.
    pub hub_detail: Option<String>,
}

impl Default for PreflightFacts {
    /// Nothing probed yet, so every tri-state is a shrug. Spelled out rather
    /// than derived because `Tri` has no `Default` — choosing `Unknown` has to
    /// be deliberate, never inherited.
    fn default() -> Self {
        Self {
            docker: DockerFacts::default(),
            disk: None,
            games_disk: None,
            games_shares_docker_volume: false,
            git: Tri::Unknown,
            hub: Tri::Unknown,
            hub_detail: None,
        }
    }
}

/// The preflight's whole answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PreflightReport {
    pub verdict: Verdict,
    /// Set only when `verdict == Refuse`.
    pub code: Option<&'static str>,
    pub findings: Vec<Finding>,
    /// True when `--allow-underspec` actually changed the outcome.
    pub overridden: bool,
}

impl PreflightReport {
    pub fn is_refusal(&self) -> bool {
        self.verdict == Verdict::Refuse
    }
    /// The finding for one check, if the report has one.
    pub fn finding(&self, check: Check) -> Option<&Finding> {
        self.findings.iter().find(|f| f.check == check)
    }
}

/// Where the measured drive came from, so the report can explain WHY it looked
/// there. Docker Desktop's data does not have to live on C:.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DataRootSource {
    /// Docker Desktop's configured "Disk image location".
    Configured,
    /// The engine's own `DockerRootDir`, when it is a real HOST path.
    DockerRootDir,
    /// Docker Desktop's default WSL data folder under `%LOCALAPPDATA%`.
    DockerDesktopDefault,
}

// ---------------------------------------------------------------------------
// Pure: the docker probe's argv + classification
// ---------------------------------------------------------------------------

/// Field separator in the `docker info` format string. A pipe, because none of
/// the four fields can contain one.
pub const INFO_SEP: char = '|';

/// The `docker info` argv the preflight uses. Pure so the format string is
/// asserted without a spawn.
pub fn docker_info_args() -> [&'static str; 3] {
    [
        "info",
        "--format",
        "{{.MemTotal}}|{{.NCPU}}|{{.DockerRootDir}}|{{.ServerVersion}}",
    ]
}

/// The four fields [`docker_info_args`] asks for, each `None` when it was not
/// a usable answer.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InfoFields {
    pub mem_bytes: Option<u64>,
    pub ncpu: Option<u32>,
    pub docker_root_dir: Option<String>,
    pub server_version: Option<String>,
}

/// Split one `docker info --format` line into [`InfoFields`].
///
/// ZERO IS NOT A READING. A stopped engine still prints `0|0||` (see the module
/// doc's captured sample), and no live engine has 0 bytes of RAM or 0 CPUs, so
/// a zero here is a broken answer rather than a very small machine. Treating it
/// as a measurement is how a preflight refuses an install with a number it
/// invented.
pub fn parse_docker_info_fields(stdout: &str) -> InfoFields {
    let line = stdout.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or_default();
    let mut it = line.split(INFO_SEP);
    let mut next = || it.next().map(str::trim).filter(|s| !s.is_empty());
    InfoFields {
        mem_bytes: next().and_then(|s| s.parse::<u64>().ok()).filter(|&b| b > 0),
        ncpu: next().and_then(|s| s.parse::<u32>().ok()).filter(|&n| n > 0),
        docker_root_dir: next().map(str::to_string),
        server_version: next().map(str::to_string),
    }
}

/// Turn one bounded probe outcome into [`DockerFacts`].
///
/// THE TRI-STATE, spelled out because collapsing it is the mistake this
/// codebase keeps re-buying:
/// * `ProgramMissing` — the ONE definitive negative. There is no `docker` on
///   this machine, so [`Tri::No`].
/// * `CouldNotTell` — spawn failure or deadline. Evidence of NOTHING;
///   [`Tri::Unknown`].
/// * `Ran` — positive evidence outranks the exit code: a non-empty
///   `ServerVersion` means the engine spoke, whatever it exited with. Without
///   one, we did not hear from an engine and it is a shrug, NOT a "no engine".
///
/// The readings are only carried out of the `Yes` branch, so a dead engine's
/// zeroes can never reach a floor comparison.
pub fn classify_docker_info(outcome: &ProbeOutcome) -> DockerFacts {
    match outcome {
        ProbeOutcome::ProgramMissing => DockerFacts { reachable: Tri::No, ..Default::default() },
        ProbeOutcome::CouldNotTell => DockerFacts { reachable: Tri::Unknown, ..Default::default() },
        ProbeOutcome::Ran { stdout, .. } => {
            let f = parse_docker_info_fields(stdout);
            match f.server_version {
                Some(version) => DockerFacts {
                    reachable: Tri::Yes,
                    detail: None,
                    mem_bytes: f.mem_bytes,
                    ncpu: f.ncpu,
                    docker_root_dir: f.docker_root_dir,
                    server_version: Some(version),
                },
                // It ran and said nothing an engine would say. Keep its own
                // complaint (`detail`) so the report can quote it instead of
                // inventing a cause.
                None => DockerFacts {
                    reachable: Tri::Unknown,
                    detail: outcome.detail(),
                    ..Default::default()
                },
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Pure: formatting
// ---------------------------------------------------------------------------

/// Format bytes as a 1024-base "GB" string with one decimal. See [`GB`] for
/// why the base is 1024 while the label says GB.
pub fn format_gb(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / GB as f64)
}

/// How many ~2 GB compiler jobs this much RAM can hold, floored at 1.
///
/// This is `Install-DML.ps1`'s own `memBoundCores = floor(wslRamGB / 2)` moved
/// to the native side: same figure, same purpose (bound parallelism by MEMORY,
/// not by core count). Floored at 1 because callers divide by it and because
/// "zero jobs" is not advice anyone can act on.
pub fn mem_bound_jobs(mem_bytes: u64) -> u32 {
    ((mem_bytes / MEM_PER_COMPILER_JOB_BYTES) as u32).max(1)
}

// ---------------------------------------------------------------------------
// Pure: which drive holds Docker's data
// ---------------------------------------------------------------------------

/// Whether `p` can be a path on THIS host's filesystem.
///
/// This exists for one specific trap: on Docker Desktop the engine reports
/// `DockerRootDir=/var/lib/docker`, which is a path inside its Linux VM, NOT on
/// the user's disk. Handing that to a Windows free-space call answers for
/// whatever drive the process happens to be on — a plausible number about the
/// wrong volume, which is worse than no number at all.
pub fn looks_like_host_path(on_windows: bool, p: &str) -> bool {
    let p = p.trim();
    if p.is_empty() {
        return false;
    }
    if on_windows {
        // `X:\...` or a UNC `\\server\share`.
        let b = p.as_bytes();
        p.starts_with("\\\\") || (b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':')
    } else {
        p.starts_with('/')
    }
}

/// Resolve which host directory to measure free space on — "the Docker
/// data-root drive, NOT necessarily C:".
///
/// Order, most-authoritative first:
/// 1. Docker Desktop's configured "Disk image location" (Settings > Resources >
///    Advanced). This is the whole reason the answer is not always C: — moving
///    the disk image to a bigger drive is the standard fix for a full C:.
/// 2. The engine's own `DockerRootDir`, but ONLY when it is a real host path
///    for this platform (true on a Linux host, false for Docker Desktop's
///    WSL2 backend — see [`looks_like_host_path`]).
/// 3. Docker Desktop's default WSL data folder, `%LOCALAPPDATA%\Docker\wsl`
///    (verified on this box: `...\Docker\wsl\disk\docker_data.vhdx`).
///
/// `None` when there is nothing to go on. A guessed drive would produce a
/// confident number about the wrong volume.
pub fn resolve_data_root(
    on_windows: bool,
    docker_root_dir: Option<&str>,
    configured: Option<&str>,
    localappdata: Option<&str>,
) -> Option<(String, DataRootSource)> {
    fn nonempty(s: Option<&str>) -> Option<&str> {
        s.map(str::trim).filter(|v| !v.is_empty())
    }

    if let Some(c) = nonempty(configured) {
        return Some((c.to_string(), DataRootSource::Configured));
    }
    if let Some(r) = nonempty(docker_root_dir) {
        if looks_like_host_path(on_windows, r) {
            return Some((r.to_string(), DataRootSource::DockerRootDir));
        }
    }
    if on_windows {
        if let Some(la) = nonempty(localappdata) {
            let base = la.trim_end_matches(['\\', '/']);
            return Some((format!(r"{base}\Docker\wsl"), DataRootSource::DockerDesktopDefault));
        }
    }
    None
}

/// Pull Docker Desktop's configured disk-image location out of its settings
/// JSON. Key name has moved across versions (`settings.json`'s `dataFolder`,
/// `settings-store.json`'s `DataFolder`, and `diskPath` in between), and the
/// file only records NON-DEFAULT settings — verified on this box, where the
/// whole store is 320 bytes and has no such key. So a `None` here means
/// "default location", not "no Docker".
pub fn parse_docker_settings_data_folder(json: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    for key in ["DataFolder", "dataFolder", "diskPath", "DiskPath"] {
        if let Some(s) = v.get(key).and_then(|x| x.as_str()) {
            let s = s.trim();
            if !s.is_empty() {
                return Some(s.to_string());
            }
        }
    }
    None
}

/// Walk up from `path` to the first ancestor `exists` accepts.
///
/// The preflight runs BEFORE anything is cloned, so the directory it is asked
/// about routinely does not exist yet. The drive does, and the drive is what
/// the question is actually about.
pub fn nearest_existing_ancestor(path: &Path, exists: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    let mut cur = Some(path);
    while let Some(p) = cur {
        if exists(p) {
            return Some(p.to_path_buf());
        }
        cur = p.parent();
    }
    None
}

/// Do two host paths live on the same volume?
///
/// Only ever used to make the disk check STRICTER (one pool, one combined
/// floor), so a false "no" degrades to the two independent checks and a false
/// "yes" is the shape that would over-refuse. That asymmetry is why this is
/// deliberately conservative: on Windows a volume is identified by its drive
/// letter or its UNC share, both readable straight off the path; anywhere else
/// a path alone cannot tell you (two directories can be different mounts), so
/// only literally identical roots count.
pub fn same_volume(on_windows: bool, a: &str, b: &str) -> bool {
    fn win_volume(p: &str) -> Option<String> {
        let p = p.trim();
        if let Some(rest) = p.strip_prefix("\\\\").or_else(|| p.strip_prefix("//")) {
            // UNC: the volume is \\server\share, so two components deep.
            let mut it = rest.split(['\\', '/']).filter(|s| !s.is_empty());
            let server = it.next()?;
            let share = it.next()?;
            return Some(format!(r"\\{}\{}", server.to_ascii_lowercase(), share.to_ascii_lowercase()));
        }
        let bytes = p.as_bytes();
        if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
            return Some(p[..1].to_ascii_lowercase());
        }
        None
    }
    if !on_windows {
        return false;
    }
    match (win_volume(a), win_volume(b)) {
        (Some(x), Some(y)) => x == y,
        _ => false,
    }
}

/// Is `git` on this machine? Tri-state, same discipline as the docker probe:
/// only a `NotFound` spawn proves absence, and only output that looks like
/// git's proves presence.
pub fn classify_git_probe(outcome: &ProbeOutcome) -> Tri {
    match outcome {
        ProbeOutcome::ProgramMissing => Tri::No,
        ProbeOutcome::CouldNotTell => Tri::Unknown,
        ProbeOutcome::Ran { stdout, stderr, .. } => {
            // Positive evidence, not the exit code: what proves git is here is
            // git's own banner. A shell that answered "'git' is not
            // recognized" is NOT this process finding out that git is absent —
            // it is a program we do not know answering, so a shrug.
            let said_git = |s: &str| s.to_ascii_lowercase().contains("git version");
            if said_git(stdout) || said_git(stderr) {
                Tri::Yes
            } else {
                Tri::Unknown
            }
        }
    }
}

/// Did the image registry answer?
///
/// ANY HTTP status is a `Yes` — the question is "did something answer", not
/// "did it like us"; an unauthenticated `/v2/` legitimately answers `401`.
///
/// A transport error is [`Tri::Unknown`], NEVER a `No`. A client-side probe
/// cannot prove there is no route: a proxy the docker daemon is configured for,
/// a registry mirror, or a firewall that blocks this process and not the engine
/// all look identical from here. That is why this check can only ever warn.
pub fn classify_hub_probe(result: Result<u16, String>) -> (Tri, Option<String>) {
    match result {
        Ok(_status) => (Tri::Yes, None),
        Err(e) => {
            let line = e.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or_default();
            let line = if line.chars().count() > 200 {
                format!("{}...", line.chars().take(200).collect::<String>())
            } else {
                line.to_string()
            };
            (Tri::Unknown, (!line.is_empty()).then_some(line))
        }
    }
}

/// Parse the "Available" column (in 1024-byte blocks) out of `df -Pk` output.
/// `-P` pins the POSIX column layout, so field 3 (0-based) is Available.
/// Anything unparseable is `None` — a wrong number here would be spent as if
/// it were a measurement.
pub fn parse_df_available_kb(text: &str) -> Option<u64> {
    let line = text.lines().skip(1).map(str::trim).find(|l| !l.is_empty())?;
    let fields: Vec<&str> = line.split_whitespace().collect();
    if fields.len() < 4 {
        return None;
    }
    fields[3].parse::<u64>().ok()
}

// ---------------------------------------------------------------------------
// Pure: the decision
// ---------------------------------------------------------------------------

/// Is the engine reachable? A refusal here is NOT overridable: no flag makes a
/// build run without a docker engine.
///
/// COPY DISCIPLINE. The [`Tri::Unknown`] arm must never say "Docker is not
/// installed" — we did not observe that, and telling a user with a perfectly
/// good install to reinstall Docker is the tri-state mistake in its most
/// expensive form. It says what WAS observed (nothing answered) and what to do
/// (start/restart Docker Desktop). Only [`Tri::No`] — `docker` itself absent
/// from the machine — is allowed to talk about absence, and even then it speaks
/// about the command it looked for.
fn docker_finding(d: &DockerFacts) -> Finding {
    match d.reachable {
        Tri::Yes => Finding {
            check: Check::Docker,
            severity: Severity::Ok,
            message: match &d.server_version {
                Some(v) => format!("Docker engine reachable (server version {v})."),
                None => "Docker engine reachable.".to_string(),
            },
            hint: String::new(),
        },
        Tri::No => Finding {
            check: Check::Docker,
            severity: Severity::Refuse,
            message: "The docker command could not be found on this PC, so there is nothing to build with.".to_string(),
            hint: "Install Docker Desktop (or set DML_DOCKER to an existing docker.exe), then run this again.".to_string(),
        },
        Tri::Unknown => Finding {
            check: Check::Docker,
            severity: Severity::Refuse,
            message: format!(
                "Docker did not answer, so the install cannot start.{} This says nothing about whether Docker is set up correctly on this PC — only that nothing is answering right now.",
                match &d.detail {
                    Some(t) => format!(" It reported: {t}"),
                    None => String::new(),
                }
            ),
            hint: "Start (or restart) Docker Desktop, wait until it reports Running, then run this again.".to_string(),
        },
    }
}

/// Memory available to Docker's Linux VM, against [`REFUSE_MEM_BYTES`] /
/// [`WARN_MEM_BYTES`]. A could-not-tell warns: it is not a refusal (evidence of
/// nothing) and it is not a pass either.
fn memory_finding(mem_bytes: Option<u64>) -> Finding {
    let Some(mem) = mem_bytes else {
        return Finding {
            check: Check::Memory,
            severity: Severity::Warn,
            message: format!(
                "Could not read how much memory Docker's Linux VM has, so the {} a source build needs is unchecked — that is not a pass.",
                format_gb(REFUSE_MEM_BYTES)
            ),
            hint: "Open Docker Desktop > Settings > Resources and check the Memory slider before starting a build.".to_string(),
        };
    };
    let raise_it = "Raise Docker Desktop's memory limit (Settings > Resources > Memory), or set memory= in %USERPROFILE%\\.wslconfig and run 'wsl --shutdown'. Then run this again.";
    if mem < REFUSE_MEM_BYTES {
        Finding {
            check: Check::Memory,
            severity: Severity::Refuse,
            message: format!(
                "Docker's Linux VM has {} of memory; a source build needs at least {}. At roughly {} per compiler job there is not enough for even the smallest safe build, so the compiler gets killed by the out-of-memory killer partway through — which looks like the build dying at the same percentage on every retry rather than like an error.",
                format_gb(mem),
                format_gb(REFUSE_MEM_BYTES),
                format_gb(MEM_PER_COMPILER_JOB_BYTES)
            ),
            hint: raise_it.to_string(),
        }
    } else if mem < WARN_MEM_BYTES {
        Finding {
            check: Check::Memory,
            severity: Severity::Warn,
            message: format!(
                "Docker's Linux VM has {} of memory. That clears the {} floor but is under the {} this build wants, so it will be memory-bound and slow.",
                format_gb(mem),
                format_gb(REFUSE_MEM_BYTES),
                format_gb(WARN_MEM_BYTES)
            ),
            hint: raise_it.to_string(),
        }
    } else {
        Finding {
            check: Check::Memory,
            severity: Severity::Ok,
            message: format!("Docker's Linux VM has {} of memory.", format_gb(mem)),
            hint: String::new(),
        }
    }
}

/// Free space where Docker keeps its data, against [`REFUSE_FREE_BYTES`] /
/// [`WARN_FREE_BYTES`]. Every message names the PATH, because the whole point
/// of the data-root resolution is that this drive is not necessarily C:.
fn disk_finding(disk: Option<&DiskFacts>) -> Finding {
    let unchecked_hint = format!(
        "Check Docker Desktop > Settings > Resources > Advanced > Disk image location and make sure that drive has at least {} free.",
        format_gb(REFUSE_FREE_BYTES)
    );
    let Some(d) = disk else {
        return Finding {
            check: Check::Disk,
            severity: Severity::Warn,
            message: format!(
                "Could not work out which drive Docker keeps its data on, so the {} a source build needs there is unchecked — that is not a pass.",
                format_gb(REFUSE_FREE_BYTES)
            ),
            hint: unchecked_hint,
        };
    };
    let Some(free) = d.free_bytes else {
        return Finding {
            check: Check::Disk,
            severity: Severity::Warn,
            message: format!(
                "Could not read free space on {} (where Docker keeps its data), so the {} a source build needs is unchecked — that is not a pass.",
                d.path,
                format_gb(REFUSE_FREE_BYTES)
            ),
            hint: unchecked_hint,
        };
    };
    let free_up = "Free up space on that drive, or move Docker's disk image to a bigger one (Docker Desktop > Settings > Resources > Advanced > Disk image location), then run this again.".to_string();
    if free < REFUSE_FREE_BYTES {
        Finding {
            check: Check::Disk,
            severity: Severity::Refuse,
            message: format!(
                "{} free on {}, where Docker keeps its data; a source build needs at least {}. The checkout, the build cache, the ~6 GB client-data volume and the built images all land there, and Docker Desktop's disk image never shrinks by itself — so a failed attempt costs that space again on the retry.",
                format_gb(free),
                d.path,
                format_gb(REFUSE_FREE_BYTES)
            ),
            hint: free_up,
        }
    } else if free < WARN_FREE_BYTES {
        Finding {
            check: Check::Disk,
            severity: Severity::Warn,
            message: format!(
                "{} free on {}, where Docker keeps its data. That clears the {} floor but is under the {} that leaves room to retry a failed build without clearing the cache first.",
                format_gb(free),
                d.path,
                format_gb(REFUSE_FREE_BYTES),
                format_gb(WARN_FREE_BYTES)
            ),
            hint: free_up,
        }
    } else {
        Finding {
            check: Check::Disk,
            severity: Severity::Ok,
            message: format!("{} free on {}, where Docker keeps its data.", format_gb(free), d.path),
            hint: String::new(),
        }
    }
}

/// Free space where the CHECKOUT lands.
///
/// WHY THIS IS NOT THE SAME QUESTION AS [`disk_finding`]. Docker Desktop's disk
/// image can be moved to another drive — it is the standard fix for a full C:,
/// and [`DataRootSource::Configured`] exists precisely to follow it. Once it
/// has been moved, "free space" is two numbers about two volumes, and checking
/// only Docker's passes a machine whose C: cannot hold the 2.4 GB checkout.
///
/// AND WHEN THEY ARE ONE VOLUME, THE FLOORS ADD. Both demands then come out of
/// one pool, so 42 GB free clears 40 and clears 5 and still cannot hold both.
/// The message says so, because a floor a user cannot see is advice they
/// cannot follow.
fn games_disk_finding(games: Option<&DiskFacts>, shares_with_docker: bool) -> Finding {
    let (refuse_floor, warn_floor) = if shares_with_docker {
        (REFUSE_FREE_BYTES + REFUSE_GAMES_FREE_BYTES, WARN_FREE_BYTES + WARN_GAMES_FREE_BYTES)
    } else {
        (REFUSE_GAMES_FREE_BYTES, WARN_GAMES_FREE_BYTES)
    };
    let shared_note = if shares_with_docker {
        format!(
            " That is the same drive Docker keeps its data on, so it has to hold both: {} for Docker plus {} for the checkout.",
            format_gb(REFUSE_FREE_BYTES),
            format_gb(REFUSE_GAMES_FREE_BYTES)
        )
    } else {
        String::new()
    };
    let free_up = "Free up space on that drive, or point the games folder at a bigger one, then run this again.".to_string();

    let Some(d) = games else {
        return Finding {
            check: Check::GamesDisk,
            severity: Severity::Warn,
            message: format!(
                "No games folder was resolved, so the {} the server checkout needs is unchecked — that is not a pass.",
                format_gb(REFUSE_GAMES_FREE_BYTES)
            ),
            hint: "Set DML_GAMES_DIR (or the games folder in the launcher's settings) and run this again.".to_string(),
        };
    };
    let Some(free) = d.free_bytes else {
        return Finding {
            check: Check::GamesDisk,
            severity: Severity::Warn,
            message: format!(
                "Could not read free space on {} (where the server files go), so the {} the checkout needs is unchecked — that is not a pass.",
                d.path,
                format_gb(refuse_floor)
            ),
            hint: free_up,
        };
    };
    if free < refuse_floor {
        Finding {
            check: Check::GamesDisk,
            severity: Severity::Refuse,
            message: format!(
                "{} free on {}, where the server files go; this needs at least {}. The AzerothCore checkout alone is about 2.4 GB (1.3 GB of it git history), and it is cloned before anything is compiled.{}",
                format_gb(free),
                d.path,
                format_gb(refuse_floor),
                shared_note
            ),
            hint: free_up,
        }
    } else if free < warn_floor {
        Finding {
            check: Check::GamesDisk,
            severity: Severity::Warn,
            message: format!(
                "{} free on {}, where the server files go. That clears the {} floor but is under the {} that leaves room to re-clone after a failed attempt.{}",
                format_gb(free),
                d.path,
                format_gb(refuse_floor),
                format_gb(warn_floor),
                shared_note
            ),
            hint: free_up,
        }
    } else {
        Finding {
            check: Check::GamesDisk,
            severity: Severity::Ok,
            message: format!("{} free on {}, where the server files go.", format_gb(free), d.path),
            hint: String::new(),
        }
    }
}

/// Is `git` here? The engine's very first stage is `git clone`, so this is a
/// refusal — but its own code, because `--allow-underspec` is about hardware
/// and cannot conjure a git.
///
/// COPY DISCIPLINE, same as the docker probe: only [`Tri::No`] — an
/// `ErrorKind::NotFound` on every candidate — may speak about absence. A
/// could-not-tell says what was observed and lets the install proceed, because
/// a clone that then fails costs seconds, not an evening.
fn git_finding(git: Tri) -> Finding {
    match git {
        Tri::Yes => Finding {
            check: Check::Git,
            severity: Severity::Ok,
            message: "Git is installed.".to_string(),
            hint: String::new(),
        },
        Tri::No => Finding {
            check: Check::Git,
            severity: Severity::Refuse,
            message: "Git could not be found on this PC, and the install starts by cloning the AzerothCore sources with it.".to_string(),
            hint: "Install Git for Windows (https://git-scm.com/download/win), then run this again. If it is installed somewhere unusual, set DML_GIT to its git.exe.".to_string(),
        },
        Tri::Unknown => Finding {
            check: Check::Git,
            severity: Severity::Warn,
            message: "Git did not answer when asked for its version, so whether it is usable here is unchecked — that is not a pass. The install clones the AzerothCore sources with it.".to_string(),
            hint: "Run 'git --version' in a terminal; if that fails, install Git for Windows or set DML_GIT to its git.exe.".to_string(),
        },
    }
}

/// Can the image registry be reached? Advisory ONLY.
///
/// The WSL installer exits 1 here (`check_docker_hub`,
/// `guides/wow-wotlk/install-wow-wotlk.sh:315-331`); this one warns, and the
/// difference is deliberate. A client-side HTTPS probe cannot distinguish "no
/// route to the registry" from "a proxy/mirror the docker daemon knows about
/// and this process does not", so a refusal here would be a fabricated cause —
/// the exact tri-state mistake the rest of this module is built to avoid. The
/// cost of being wrong is also asymmetric in the other direction: a pull that
/// genuinely cannot reach Hub fails within minutes, while the hardware floors
/// exist to prevent failures that cost hours.
fn hub_finding(hub: Tri, detail: Option<&str>) -> Finding {
    match hub {
        Tri::Yes => Finding {
            check: Check::DockerHub,
            severity: Severity::Ok,
            message: format!("Docker Hub answered at {DOCKER_HUB_PROBE_URL}."),
            hint: String::new(),
        },
        // No definitive negative exists for this probe (see
        // `classify_hub_probe`), so both non-Yes answers are the same shrug.
        Tri::No | Tri::Unknown => Finding {
            check: Check::DockerHub,
            severity: Severity::Warn,
            message: format!(
                "Could not reach {DOCKER_HUB_PROBE_URL} from here, so whether the base images can be pulled is unchecked — that is not a pass, and it is not proof the registry is unreachable either: a proxy or mirror Docker itself uses would not show up in this check.{}",
                match detail {
                    Some(t) if !t.is_empty() => format!(" The probe reported: {t}"),
                    _ => String::new(),
                }
            ),
            hint: "If the build later fails to pull an image, check your connection and DNS, and if you are behind a firewall or a rate limit, configure a registry mirror in Docker Desktop > Settings > Docker Engine.".to_string(),
        },
    }
}

/// Advisory only: more CPUs than the VM's RAM can feed at ~2 GB per compiler
/// job. Never a refusal — the build still completes, it just needs the CPU cap
/// that `Install-DML.ps1` applies on the WSL side and that nothing applies here.
/// Emitted only when BOTH numbers are real; guessing one would produce advice
/// about a machine we did not measure.
fn cpu_finding(ncpu: Option<u32>, mem_bytes: Option<u64>) -> Option<Finding> {
    let (ncpu, mem) = (ncpu?, mem_bytes?);
    let jobs = mem_bound_jobs(mem);
    if ncpu <= jobs {
        return None;
    }
    Some(Finding {
        check: Check::Cpu,
        severity: Severity::Warn,
        message: format!(
            "Docker's Linux VM has {ncpu} CPUs but only {} of memory — at roughly {} per compiler job that is room for {jobs} jobs, not {ncpu}. Nothing caps build parallelism for you here, so the build can out-run its own memory.",
            format_gb(mem),
            format_gb(MEM_PER_COMPILER_JOB_BYTES)
        ),
        hint: format!(
            "Set Docker Desktop > Settings > Resources > CPUs to {jobs} (or raise the memory limit) before starting the build."
        ),
    })
}

/// The whole decision. Pure: same facts in, same report out.
///
/// `allow_underspec` is the explicit "I insist" override. It downgrades
/// HARDWARE refusals to warnings and records itself in
/// [`PreflightReport::overridden`] — it never touches the docker refusal, and
/// it never strips the numbers out of a message, because a user who forced past
/// a floor is exactly the user who will need them when the build dies.
pub fn decide(facts: &PreflightFacts, allow_underspec: bool) -> PreflightReport {
    // Prerequisites first: things no amount of hardware makes up for.
    let docker = docker_finding(&facts.docker);
    let git = git_finding(facts.git);
    let hub = hub_finding(facts.hub, facts.hub_detail.as_deref());
    // ...then the floors, which ARE what `--allow-underspec` is about.
    let mut hardware = vec![
        memory_finding(facts.docker.mem_bytes),
        disk_finding(facts.disk.as_ref()),
        games_disk_finding(facts.games_disk.as_ref(), facts.games_shares_docker_volume),
    ];
    if let Some(cpu) = cpu_finding(facts.docker.ncpu, facts.docker.mem_bytes) {
        hardware.push(cpu);
    }

    let docker_blocks = docker.severity == Severity::Refuse;
    let git_blocks = git.severity == Severity::Refuse;
    let hardware_short = hardware.iter().any(|f| f.severity == Severity::Refuse);
    let overridden = allow_underspec && hardware_short;
    if overridden {
        for f in hardware.iter_mut().filter(|f| f.severity == Severity::Refuse) {
            f.severity = Severity::Warn;
            f.message.push_str(" Continuing anyway because --allow-underspec was passed.");
        }
    }

    let mut findings = Vec::with_capacity(3 + hardware.len());
    findings.push(docker);
    findings.push(git);
    findings.push(hub);
    findings.extend(hardware);

    // Docker outranks everything: it is the first refusal no flag can clear,
    // so it must be the code the caller sees. Git is the second — also
    // un-overridable, and also nothing to do with the hardware floors.
    let (verdict, code) = if docker_blocks {
        (Verdict::Refuse, Some(CODE_DOCKER_UNREACHABLE))
    } else if git_blocks {
        (Verdict::Refuse, Some(CODE_GIT_MISSING))
    } else if findings.iter().any(|f| f.severity == Severity::Refuse) {
        (Verdict::Refuse, Some(CODE_UNDERSPEC))
    } else if findings.iter().any(|f| f.severity == Severity::Warn) {
        (Verdict::Warn, None)
    } else {
        (Verdict::Proceed, None)
    };

    PreflightReport { verdict, code, findings, overridden }
}

// ---------------------------------------------------------------------------
// Impure: gathering the facts. Nothing below DECIDES anything — every one of
// these functions answers exactly one question and hands the answer to
// [`decide`], which is where the tested logic lives.
// ---------------------------------------------------------------------------

/// Wall-clock bound on the `docker info` probe. Generous because a Docker
/// Desktop that is mid-start genuinely takes a while to answer, and stingy
/// enough that a wedged engine does not hold the install screen forever. Either
/// way the outcome is the same tri-state shrug, never a fabricated reading.
pub const DOCKER_INFO_TIMEOUT: Duration = Duration::from_secs(30);

/// Ask the engine for its resources. Bounded; a miss is a tri-state shrug.
pub fn probe_docker(program: &OsStr, timeout: Duration) -> DockerFacts {
    let mut cmd = Command::new(program);
    cmd.args(docker_info_args());
    windows_no_window(&mut cmd);
    classify_docker_info(&ProbeOutcome::from_bounded(run_bounded_outcome(cmd, timeout)))
}

/// Docker Desktop's configured disk-image location, if it has one. Reads the
/// current settings store first, then the legacy file. `None` = default
/// location (the file only records non-defaults), not "no Docker".
pub fn docker_settings_data_folder() -> Option<String> {
    let appdata = std::env::var_os("APPDATA")?;
    for name in ["settings-store.json", "settings.json"] {
        let p = PathBuf::from(&appdata).join("Docker").join(name);
        if let Ok(text) = std::fs::read_to_string(&p) {
            if let Some(folder) = parse_docker_settings_data_folder(&text) {
                return Some(folder);
            }
        }
    }
    None
}

/// Free bytes on the volume holding `path`, measured against the nearest
/// ancestor that exists (the preflight runs before the directory does).
/// `None` is a could-not-tell, never a zero.
pub fn free_space_bytes(path: &Path) -> Option<u64> {
    let target = nearest_existing_ancestor(path, |p| p.exists())?;
    free_space_of_existing(&target)
}

#[cfg(windows)]
fn free_space_of_existing(dir: &Path) -> Option<u64> {
    // Declared inline rather than pulling in `windows-sys`: this is the only
    // Win32 call in the whole crate, and the plan's dependency budget for the
    // native work is one new external crate (clap), already spent.
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn GetDiskFreeSpaceExW(
            lp_directory_name: *const u16,
            lp_free_bytes_available_to_caller: *mut u64,
            lp_total_number_of_bytes: *mut u64,
            lp_total_number_of_free_bytes: *mut u64,
        ) -> i32;
    }
    let wide: Vec<u16> = dir.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
    // SAFETY: `wide` is a NUL-terminated UTF-16 buffer that outlives the call,
    // and the three out-params are live stack slots.
    let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut avail, &mut total, &mut free) };
    // "Available to the caller" is the honest number: it respects any disk
    // quota, which is what the build will actually be allowed to use.
    if ok == 0 {
        None
    } else {
        Some(avail)
    }
}

#[cfg(not(windows))]
fn free_space_of_existing(dir: &Path) -> Option<u64> {
    // POSIX `-P` pins the column layout so `parse_df_available_kb` can trust
    // field 3. Bounded like every other probe.
    let mut cmd = Command::new("df");
    cmd.arg("-Pk").arg(dir);
    match run_bounded_outcome(cmd, Duration::from_secs(10)) {
        dml_core::proc::BoundedOutcome::Ran(out) if out.status.success() => {
            parse_df_available_kb(&String::from_utf8_lossy(&out.stdout))
                .map(|kb| kb.saturating_mul(1024))
        }
        _ => None,
    }
}

/// Wall-clock bound on the `git --version` probe. Tiny: git either answers
/// immediately or is not there.
pub const GIT_PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// Wall-clock bound on the Docker Hub probe. Same 10s the WSL installer's
/// `curl --max-time 10` uses.
pub const HUB_PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// Where to look for `git`. `DML_GIT` wins; otherwise a bare `git` on PATH,
/// then the two standard Git-for-Windows locations.
///
/// The fallbacks matter: Git for Windows can be installed with "do not modify
/// PATH", and refusing an install because a perfectly present git is not on
/// PATH would be a fabricated refusal. Pure so the candidate list is testable
/// without a filesystem.
pub fn git_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(p) = std::env::var_os("DML_GIT").filter(|v| !v.is_empty()) {
        out.push(PathBuf::from(p));
    }
    out.push(PathBuf::from("git"));
    if cfg!(windows) {
        out.push(PathBuf::from(r"C:\Program Files\Git\cmd\git.exe"));
        out.push(PathBuf::from(r"C:\Program Files\Git\bin\git.exe"));
    }
    out
}

/// Is `git` on this machine? Tries each candidate and keeps the best answer —
/// one `Yes` anywhere beats a `No` from a name that simply is not on PATH.
pub fn probe_git(timeout: Duration) -> Tri {
    let mut best = Tri::Unknown;
    for cand in git_candidates() {
        let mut cmd = Command::new(&cand);
        cmd.arg("--version");
        windows_no_window(&mut cmd);
        match classify_git_probe(&ProbeOutcome::from_bounded(run_bounded_outcome(cmd, timeout))) {
            Tri::Yes => return Tri::Yes,
            // A definitive "this candidate is not here" is only the whole
            // answer if no other candidate does better.
            Tri::No if best == Tri::Unknown => best = Tri::No,
            _ => {}
        }
    }
    best
}

/// Ask the image registry whether it is there. Bounded; a miss is a shrug.
pub fn probe_docker_hub(timeout: Duration) -> (Tri, Option<String>) {
    let client = match reqwest::blocking::Client::builder().timeout(timeout).build() {
        Ok(c) => c,
        Err(e) => return classify_hub_probe(Err(e.to_string())),
    };
    match client.head(DOCKER_HUB_PROBE_URL).send() {
        Ok(r) => classify_hub_probe(Ok(r.status().as_u16())),
        Err(e) => classify_hub_probe(Err(e.to_string())),
    }
}

/// Gather every fact the preflight needs. `program` is the resolved docker
/// executable (`dml_core::engine::docker_program()` at the call site);
/// `games_dir` is where the title (and therefore the CHECKOUT) will land.
///
/// The games dir is a PARAMETER, not something re-derived here: the caller
/// already resolved it (`DML_GAMES_DIR` -> `~/.dml/launcher.json` -> default),
/// and a preflight that resolved its own would happily measure a different
/// drive from the one the install then writes to.
pub fn gather(program: &OsStr, games_dir: &Path) -> PreflightFacts {
    let docker = probe_docker(program, DOCKER_INFO_TIMEOUT);
    let disk = resolve_data_root(
        cfg!(windows),
        docker.docker_root_dir.as_deref(),
        docker_settings_data_folder().as_deref(),
        std::env::var("LOCALAPPDATA").ok().as_deref(),
    )
    .map(|(path, _source)| DiskFacts {
        free_bytes: free_space_bytes(Path::new(&path)),
        path,
    });
    let games_path = games_dir.display().to_string();
    let games_disk = Some(DiskFacts {
        free_bytes: free_space_bytes(games_dir),
        path: games_path.clone(),
    });
    let games_shares_docker_volume = disk
        .as_ref()
        .is_some_and(|d| same_volume(cfg!(windows), &d.path, &games_path));
    let (hub, hub_detail) = probe_docker_hub(HUB_PROBE_TIMEOUT);
    PreflightFacts {
        docker,
        disk,
        games_disk,
        games_shares_docker_volume,
        git: probe_git(GIT_PROBE_TIMEOUT),
        hub,
        hub_detail,
    }
}

/// The whole preflight against the real machine.
pub fn run(program: &OsStr, games_dir: &Path, allow_underspec: bool) -> PreflightReport {
    decide(&gather(program, games_dir), allow_underspec)
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- helpers -------------------------------------------------------------

    fn ran(code: i32, stdout: &str, stderr: &str) -> ProbeOutcome {
        ProbeOutcome::Ran {
            code: Some(code),
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
        }
    }

    /// A machine that passes everything, so a single field can be varied.
    fn healthy() -> PreflightFacts {
        PreflightFacts {
            docker: DockerFacts {
                reachable: Tri::Yes,
                detail: None,
                mem_bytes: Some(16 * GB),
                ncpu: Some(4),
                docker_root_dir: Some("/var/lib/docker".into()),
                server_version: Some("28.0.1".into()),
            },
            disk: Some(DiskFacts {
                path: r"D:\Docker".into(),
                free_bytes: Some(200 * GB),
            }),
            games_disk: Some(DiskFacts {
                path: r"C:\Users\x\dml-native".into(),
                free_bytes: Some(100 * GB),
            }),
            games_shares_docker_volume: false,
            git: Tri::Yes,
            hub: Tri::Yes,
            hub_detail: None,
        }
    }

    fn with_games_free(free: Option<u64>) -> PreflightFacts {
        let mut f = healthy();
        f.games_disk = Some(DiskFacts {
            path: r"C:\Users\x\dml-native".into(),
            free_bytes: free,
        });
        f
    }

    fn with_mem(mem: Option<u64>) -> PreflightFacts {
        let mut f = healthy();
        f.docker.mem_bytes = mem;
        f
    }

    fn with_free(free: Option<u64>) -> PreflightFacts {
        let mut f = healthy();
        f.disk = Some(DiskFacts {
            path: r"D:\Docker".into(),
            free_bytes: free,
        });
        f
    }

    fn sev(rep: &PreflightReport, check: Check) -> Severity {
        rep.finding(check)
            .unwrap_or_else(|| panic!("no finding for {check:?} in {rep:#?}"))
            .severity
    }

    fn msg(rep: &PreflightReport, check: Check) -> String {
        rep.finding(check)
            .unwrap_or_else(|| panic!("no finding for {check:?} in {rep:#?}"))
            .message
            .clone()
    }

    // -- the ratified floors themselves --------------------------------------

    /// The boundary tests below are written against the CONSTANTS
    /// (`REFUSE_MEM_BYTES - 1`, and so on), which pins the RELATIONSHIP but
    /// moves with the value — edit a floor and they all stay green. These two
    /// tests pin the VALUES, so changing a floor is a deliberate act that has
    /// to come with a decision, not a one-character edit nobody notices.
    ///
    /// Source of the numbers: DECISIONS TAKEN #3 in
    /// `docs/superpowers/plans/2026-07-29-native-first-install.md` — "Below
    /// ~6 GB Docker VM RAM or ~40 GB free on the Docker data-root drive it
    /// refuses; between there and 8 GB / 60 GB it warns."
    #[test]
    fn the_memory_floors_are_the_ratified_six_and_eight_gb() {
        assert_eq!(REFUSE_MEM_BYTES, 6 * GB, "the refuse floor is user-ratified at 6 GB");
        assert_eq!(WARN_MEM_BYTES, 8 * GB, "the warn floor is user-ratified at 8 GB");
        assert!(REFUSE_MEM_BYTES < WARN_MEM_BYTES);
    }

    #[test]
    fn the_disk_floors_are_the_ratified_forty_and_sixty_gb() {
        assert_eq!(REFUSE_FREE_BYTES, 40 * GB, "the refuse floor is user-ratified at 40 GB");
        assert_eq!(WARN_FREE_BYTES, 60 * GB, "the warn floor is user-ratified at 60 GB");
        assert!(REFUSE_FREE_BYTES < WARN_FREE_BYTES);
    }

    #[test]
    fn the_per_job_memory_figure_is_the_projects_own_measured_two_gb() {
        // `Install-DML.ps1` Step 6: "one C++ compiler per core, ~2 GB each at
        // peak". Every piece of advice this module gives is derived from it.
        assert_eq!(MEM_PER_COMPILER_JOB_BYTES, 2 * GB);
    }

    // -- the docker probe: tri-state -----------------------------------------

    #[test]
    fn docker_info_argv_asks_for_all_four_fields() {
        let args = docker_info_args();
        assert_eq!(args[0], "info");
        assert_eq!(args[1], "--format");
        for field in ["{{.MemTotal}}", "{{.NCPU}}", "{{.DockerRootDir}}", "{{.ServerVersion}}"] {
            assert!(args[2].contains(field), "format string lost {field}: {}", args[2]);
        }
        assert_eq!(args[2].matches(INFO_SEP).count(), 3, "four fields need three separators");
    }

    #[test]
    fn a_missing_docker_command_is_the_one_definitive_no() {
        let got = classify_docker_info(&ProbeOutcome::ProgramMissing);
        assert_eq!(got.reachable, Tri::No);
        assert_eq!(got.mem_bytes, None);
        assert_eq!(got.ncpu, None);
    }

    #[test]
    fn a_docker_that_did_not_answer_is_unknown_never_no() {
        // The tri-state rule: a hung/blocked docker.exe proves NOTHING.
        let got = classify_docker_info(&ProbeOutcome::CouldNotTell);
        assert_eq!(got.reachable, Tri::Unknown);
        assert_eq!(got.mem_bytes, None);
        assert_eq!(got.ncpu, None);
    }

    #[test]
    fn the_real_captured_engine_down_output_is_unknown_and_its_zeros_are_not_readings() {
        // Captured verbatim on this box, 2026-07-29, engine stopped:
        //   exit=1, stdout="0|0||", stderr="failed to connect to the docker API
        //   at npipe:////./pipe/dockerDesktopLinuxEngine; ..."
        // A down engine PRINTS ZEROES. Reading MemTotal here would refuse the
        // install with a fabricated "0.0 GB of RAM".
        let out = ran(
            1,
            "0|0||\n",
            "failed to connect to the docker API at npipe:////./pipe/dockerDesktopLinuxEngine; check if the path is correct and if the daemon is running: open //./pipe/dockerDesktopLinuxEngine: The system cannot find the file specified.\n",
        );
        let got = classify_docker_info(&out);
        assert_eq!(got.reachable, Tri::Unknown, "a down engine is a shrug, not a No");
        assert_eq!(got.mem_bytes, None, "0 from a dead engine is not a reading");
        assert_eq!(got.ncpu, None, "0 from a dead engine is not a reading");
        assert!(
            got.detail.as_deref().unwrap_or_default().contains("npipe"),
            "the engine's own complaint must survive into the report: {:?}",
            got.detail
        );
    }

    #[test]
    fn a_live_engine_answer_is_yes_with_its_numbers() {
        let got = classify_docker_info(&ran(0, "16768393216|8|/var/lib/docker|28.0.1\n", ""));
        assert_eq!(got.reachable, Tri::Yes);
        assert_eq!(got.mem_bytes, Some(16_768_393_216));
        assert_eq!(got.ncpu, Some(8));
        assert_eq!(got.docker_root_dir.as_deref(), Some("/var/lib/docker"));
        assert_eq!(got.server_version.as_deref(), Some("28.0.1"));
    }

    #[test]
    fn a_zero_reading_from_a_live_engine_is_still_not_a_reading() {
        // Engine answered (ServerVersion present) but reported 0 RAM. No live
        // engine has 0 bytes; that is a broken reading, not a tiny machine.
        let got = classify_docker_info(&ran(0, "0|0|/var/lib/docker|28.0.1\n", ""));
        assert_eq!(got.reachable, Tri::Yes);
        assert_eq!(got.mem_bytes, None);
        assert_eq!(got.ncpu, None);
    }

    // -- docker's effect on the verdict --------------------------------------

    #[test]
    fn an_unreachable_engine_refuses_and_the_copy_never_claims_docker_is_not_installed() {
        let mut facts = healthy();
        facts.docker = DockerFacts {
            reachable: Tri::Unknown,
            detail: Some("failed to connect to the docker API at npipe:...".into()),
            ..Default::default()
        };
        let rep = decide(&facts, false);
        assert_eq!(rep.verdict, Verdict::Refuse);
        assert_eq!(rep.code, Some(CODE_DOCKER_UNREACHABLE));
        let m = msg(&rep, Check::Docker).to_lowercase();
        assert!(
            !m.contains("not installed") && !m.contains("is not present") && !m.contains("no docker"),
            "a could-not-tell must not assert absence: {m}"
        );
        assert!(
            m.contains("did not answer") || m.contains("could not"),
            "the copy must say what was actually observed: {m}"
        );
    }

    #[test]
    fn allow_underspec_does_not_override_an_unreachable_engine() {
        // The override is about HARDWARE. No flag makes a build run without a
        // docker engine.
        let mut facts = healthy();
        facts.docker.reachable = Tri::Unknown;
        facts.docker.mem_bytes = None;
        let rep = decide(&facts, true);
        assert_eq!(rep.verdict, Verdict::Refuse);
        assert_eq!(rep.code, Some(CODE_DOCKER_UNREACHABLE));
    }

    #[test]
    fn a_missing_docker_command_refuses_too_but_may_name_what_was_observed() {
        let mut facts = healthy();
        facts.docker = DockerFacts { reachable: Tri::No, ..Default::default() };
        let rep = decide(&facts, false);
        assert_eq!(rep.verdict, Verdict::Refuse);
        assert_eq!(rep.code, Some(CODE_DOCKER_UNREACHABLE));
        assert_eq!(sev(&rep, Check::Docker), Severity::Refuse);
    }

    #[test]
    fn a_reachable_engine_on_good_hardware_proceeds_clean() {
        let rep = decide(&healthy(), false);
        assert_eq!(rep.verdict, Verdict::Proceed);
        assert_eq!(rep.code, None);
        assert!(!rep.overridden);
        assert_eq!(sev(&rep, Check::Docker), Severity::Ok);
        assert_eq!(sev(&rep, Check::Memory), Severity::Ok);
        assert_eq!(sev(&rep, Check::Disk), Severity::Ok);
    }

    // -- memory floor: just under / exactly at / just over -------------------

    #[test]
    fn memory_just_under_the_refuse_floor_refuses() {
        let rep = decide(&with_mem(Some(REFUSE_MEM_BYTES - 1)), false);
        assert_eq!(rep.verdict, Verdict::Refuse);
        assert_eq!(rep.code, Some(CODE_UNDERSPEC));
        assert_eq!(sev(&rep, Check::Memory), Severity::Refuse);
    }

    #[test]
    fn memory_exactly_at_the_refuse_floor_does_not_refuse() {
        // "REFUSE below 6 GB" — 6 GB itself is the warn band, not a refusal.
        let rep = decide(&with_mem(Some(REFUSE_MEM_BYTES)), false);
        assert_eq!(rep.verdict, Verdict::Warn);
        assert_eq!(sev(&rep, Check::Memory), Severity::Warn);
    }

    #[test]
    fn memory_just_under_the_warn_floor_warns() {
        let rep = decide(&with_mem(Some(WARN_MEM_BYTES - 1)), false);
        assert_eq!(rep.verdict, Verdict::Warn);
        assert_eq!(sev(&rep, Check::Memory), Severity::Warn);
        assert_eq!(rep.code, None, "a warning is not a refusal and carries no refusal code");
    }

    #[test]
    fn memory_exactly_at_the_warn_floor_is_ok() {
        let rep = decide(&with_mem(Some(WARN_MEM_BYTES)), false);
        assert_eq!(sev(&rep, Check::Memory), Severity::Ok);
    }

    #[test]
    fn memory_just_over_the_warn_floor_is_ok() {
        let rep = decide(&with_mem(Some(WARN_MEM_BYTES + 1)), false);
        assert_eq!(sev(&rep, Check::Memory), Severity::Ok);
    }

    #[test]
    fn unreadable_memory_warns_and_is_never_a_silent_pass() {
        // Could-not-tell is evidence of nothing: it must not refuse, and it
        // must not be reported as a pass either.
        let mut facts = with_mem(None);
        facts.docker.ncpu = None;
        let rep = decide(&facts, false);
        assert_eq!(sev(&rep, Check::Memory), Severity::Warn);
        assert_eq!(rep.verdict, Verdict::Warn);
        assert_ne!(rep.code, Some(CODE_UNDERSPEC));
    }

    #[test]
    fn the_memory_refusal_names_both_the_measured_value_and_the_floor() {
        // "4.2 GB available, needs 6 GB" is actionable; "preflight failed" is not.
        let rep = decide(&with_mem(Some(4_509_715_660)), false);
        let m = msg(&rep, Check::Memory);
        assert!(m.contains("4.2 GB"), "measured value missing from the copy: {m}");
        assert!(m.contains("6.0 GB"), "the floor is missing from the copy: {m}");
    }

    #[test]
    fn the_memory_refusal_explains_the_oom_symptom() {
        // The whole reason this is a refusal: an OOM kill is unrecognisable in
        // the log, so the user must be told before they spend the hours.
        let rep = decide(&with_mem(Some(4 * GB)), false);
        let m = msg(&rep, Check::Memory).to_lowercase();
        assert!(m.contains("same percentage") || m.contains("same %"), "{m}");
    }

    // -- disk floor: just under / exactly at / just over ---------------------

    #[test]
    fn free_space_just_under_the_refuse_floor_refuses() {
        let rep = decide(&with_free(Some(REFUSE_FREE_BYTES - 1)), false);
        assert_eq!(rep.verdict, Verdict::Refuse);
        assert_eq!(rep.code, Some(CODE_UNDERSPEC));
        assert_eq!(sev(&rep, Check::Disk), Severity::Refuse);
    }

    #[test]
    fn free_space_exactly_at_the_refuse_floor_does_not_refuse() {
        let rep = decide(&with_free(Some(REFUSE_FREE_BYTES)), false);
        assert_eq!(rep.verdict, Verdict::Warn);
        assert_eq!(sev(&rep, Check::Disk), Severity::Warn);
    }

    #[test]
    fn free_space_just_under_the_warn_floor_warns() {
        let rep = decide(&with_free(Some(WARN_FREE_BYTES - 1)), false);
        assert_eq!(sev(&rep, Check::Disk), Severity::Warn);
        assert_eq!(rep.verdict, Verdict::Warn);
    }

    #[test]
    fn free_space_exactly_at_the_warn_floor_is_ok() {
        let rep = decide(&with_free(Some(WARN_FREE_BYTES)), false);
        assert_eq!(sev(&rep, Check::Disk), Severity::Ok);
    }

    #[test]
    fn free_space_just_over_the_warn_floor_is_ok() {
        let rep = decide(&with_free(Some(WARN_FREE_BYTES + 1)), false);
        assert_eq!(sev(&rep, Check::Disk), Severity::Ok);
    }

    #[test]
    fn an_unreadable_drive_warns_and_names_the_path_it_tried() {
        let rep = decide(&with_free(None), false);
        assert_eq!(sev(&rep, Check::Disk), Severity::Warn);
        assert!(msg(&rep, Check::Disk).contains(r"D:\Docker"), "{}", msg(&rep, Check::Disk));
    }

    #[test]
    fn an_unresolvable_data_root_warns_rather_than_passing_silently() {
        let mut facts = healthy();
        facts.disk = None;
        let rep = decide(&facts, false);
        assert_eq!(sev(&rep, Check::Disk), Severity::Warn);
        assert_eq!(rep.verdict, Verdict::Warn);
    }

    #[test]
    fn the_disk_refusal_names_the_drive_the_value_and_the_floor() {
        // "not necessarily C:" is the point — the message has to say WHERE.
        let rep = decide(&with_free(Some(21_474_836_480)), false); // 20 GB
        let m = msg(&rep, Check::Disk);
        assert!(m.contains("20.0 GB"), "measured value missing: {m}");
        assert!(m.contains("40.0 GB"), "floor missing: {m}");
        assert!(m.contains(r"D:\Docker"), "the drive is missing: {m}");
    }

    // -- the override --------------------------------------------------------

    #[test]
    fn allow_underspec_turns_a_hardware_refusal_into_a_warning_that_keeps_the_numbers() {
        let facts = with_mem(Some(4_509_715_660));
        let strict = decide(&facts, false);
        assert_eq!(strict.verdict, Verdict::Refuse);

        let forced = decide(&facts, true);
        assert_eq!(forced.verdict, Verdict::Warn);
        assert_eq!(forced.code, None);
        assert!(forced.overridden, "the report must record that a floor was overridden");
        assert_eq!(sev(&forced, Check::Memory), Severity::Warn);
        let m = msg(&forced, Check::Memory);
        assert!(m.contains("4.2 GB") && m.contains("6.0 GB"), "the numbers must survive the override: {m}");
    }

    #[test]
    fn allow_underspec_on_a_healthy_machine_changes_nothing_and_is_not_marked_overridden() {
        let rep = decide(&healthy(), true);
        assert_eq!(rep.verdict, Verdict::Proceed);
        assert!(!rep.overridden, "nothing was overridden, so nothing should be claimed");
    }

    #[test]
    fn both_floors_can_fail_at_once_and_both_findings_survive() {
        let mut facts = healthy();
        facts.docker.mem_bytes = Some(4 * GB);
        facts.disk = Some(DiskFacts { path: r"E:\dd".into(), free_bytes: Some(10 * GB) });
        let rep = decide(&facts, false);
        assert_eq!(rep.verdict, Verdict::Refuse);
        assert_eq!(sev(&rep, Check::Memory), Severity::Refuse);
        assert_eq!(sev(&rep, Check::Disk), Severity::Refuse);
    }

    // -- the CPU-vs-RAM advisory ---------------------------------------------

    #[test]
    fn more_cpus_than_the_ram_can_feed_advises_capping_them() {
        // 8 GB / 2 GB per job = 4 jobs, but Docker will happily run 16.
        let mut facts = healthy();
        facts.docker.mem_bytes = Some(8 * GB);
        facts.docker.ncpu = Some(16);
        let rep = decide(&facts, false);
        assert_eq!(sev(&rep, Check::Cpu), Severity::Warn);
        let m = msg(&rep, Check::Cpu);
        assert!(m.contains("16"), "the measured CPU count is missing: {m}");
        assert!(m.contains('4'), "the safe job count is missing: {m}");
    }

    #[test]
    fn cpus_the_ram_can_feed_produce_no_advisory() {
        let mut facts = healthy();
        facts.docker.mem_bytes = Some(16 * GB); // 8 jobs
        facts.docker.ncpu = Some(8);
        let rep = decide(&facts, false);
        assert!(rep.finding(Check::Cpu).is_none(), "{:#?}", rep.findings);
    }

    #[test]
    fn an_unknown_cpu_count_produces_no_advisory_rather_than_a_guess() {
        let mut facts = healthy();
        facts.docker.ncpu = None;
        assert!(decide(&facts, false).finding(Check::Cpu).is_none());

        let mut facts = healthy();
        facts.docker.mem_bytes = None;
        assert!(decide(&facts, false).finding(Check::Cpu).is_none());
    }

    #[test]
    fn the_cpu_advisory_never_escalates_to_a_refusal() {
        let mut facts = healthy();
        facts.docker.mem_bytes = Some(32 * GB);
        facts.docker.ncpu = Some(64);
        let rep = decide(&facts, false);
        assert_eq!(rep.verdict, Verdict::Warn);
        assert_eq!(rep.code, None);
    }

    // -- number formatting ---------------------------------------------------

    #[test]
    fn gb_formatting_is_1024_base_with_one_decimal() {
        assert_eq!(format_gb(6 * GB), "6.0 GB");
        assert_eq!(format_gb(4_509_715_660), "4.2 GB");
        assert_eq!(format_gb(0), "0.0 GB");
    }

    #[test]
    fn mem_bound_jobs_uses_the_two_gb_per_job_figure_and_never_returns_zero() {
        assert_eq!(mem_bound_jobs(8 * GB), 4);
        assert_eq!(mem_bound_jobs(9 * GB), 4); // floored, not rounded
        assert_eq!(mem_bound_jobs(2 * GB), 1);
        assert_eq!(mem_bound_jobs(0), 1, "a job count of zero would be a divide-by-zero for callers");
    }

    // -- which drive holds Docker's data -------------------------------------

    #[test]
    fn a_configured_disk_image_location_wins() {
        let got = resolve_data_root(true, Some("/var/lib/docker"), Some(r"E:\DockerData"), Some(r"C:\Users\x\AppData\Local"));
        assert_eq!(got, Some((r"E:\DockerData".to_string(), DataRootSource::Configured)));
    }

    #[test]
    fn on_windows_the_engines_vm_internal_root_is_not_mistaken_for_a_host_path() {
        // Docker Desktop reports DockerRootDir=/var/lib/docker — a path INSIDE
        // its Linux VM. Measuring free space on "C:/var/lib/docker" would
        // answer for the wrong drive entirely.
        let got = resolve_data_root(true, Some("/var/lib/docker"), None, Some(r"C:\Users\x\AppData\Local"));
        assert_eq!(
            got,
            Some((
                r"C:\Users\x\AppData\Local\Docker\wsl".to_string(),
                DataRootSource::DockerDesktopDefault
            ))
        );
    }

    #[test]
    fn on_windows_a_real_host_root_dir_is_used_as_given() {
        // Windows-containers mode reports a genuine host path.
        let got = resolve_data_root(true, Some(r"D:\ProgramData\Docker"), None, Some(r"C:\Users\x\AppData\Local"));
        assert_eq!(got, Some((r"D:\ProgramData\Docker".to_string(), DataRootSource::DockerRootDir)));
    }

    #[test]
    fn on_linux_the_root_dir_is_a_real_host_path() {
        let got = resolve_data_root(false, Some("/var/lib/docker"), None, None);
        assert_eq!(got, Some(("/var/lib/docker".to_string(), DataRootSource::DockerRootDir)));
    }

    #[test]
    fn nothing_to_go_on_is_none_rather_than_a_guessed_drive() {
        assert_eq!(resolve_data_root(true, Some("/var/lib/docker"), None, None), None);
        assert_eq!(resolve_data_root(true, None, None, None), None);
        assert_eq!(resolve_data_root(false, None, None, None), None);
    }

    #[test]
    fn empty_strings_are_treated_as_absent_not_as_the_root_directory() {
        // `docker info` prints an EMPTY DockerRootDir when the engine is down —
        // treating "" as a path would measure the process's current drive.
        assert_eq!(resolve_data_root(true, Some(""), Some(""), None), None);
    }

    #[test]
    fn host_path_shapes_are_platform_specific() {
        assert!(looks_like_host_path(true, r"C:\Docker"));
        assert!(looks_like_host_path(true, r"\\server\share\docker"));
        assert!(!looks_like_host_path(true, "/var/lib/docker"));
        assert!(looks_like_host_path(false, "/var/lib/docker"));
        assert!(!looks_like_host_path(false, r"C:\Docker"));
        assert!(!looks_like_host_path(true, ""));
        assert!(!looks_like_host_path(false, ""));
    }

    // -- measuring the drive -------------------------------------------------

    #[test]
    fn free_space_is_measured_against_the_nearest_existing_ancestor() {
        // The games dir may not exist yet when the preflight runs; the DRIVE
        // still does, and that is what we are actually asking about.
        let got = nearest_existing_ancestor(Path::new("/a/b/c/d"), |p| p == Path::new("/a/b"));
        assert_eq!(got, Some(PathBuf::from("/a/b")));
    }

    #[test]
    fn nothing_existing_anywhere_up_the_tree_is_none() {
        assert_eq!(nearest_existing_ancestor(Path::new("/a/b/c"), |_| false), None);
    }

    #[test]
    fn a_default_docker_desktop_settings_store_has_no_data_folder_and_that_is_not_an_error() {
        // Captured verbatim from this box (the whole file is 320 bytes):
        // Docker Desktop only records NON-DEFAULT settings, so the absence of
        // a data-folder key means "default location", not "no Docker".
        let real = r#"{
  "AutoStart": false,
  "DesktopTerminalEnabled": true,
  "DisplayedOnboarding": true,
  "EnableDockerAI": true,
  "ExtensionsEnabled": true,
  "InferenceCanUseGPUVariant": true,
  "LastContainerdSnapshotterEnable": 1784843094,
  "LicenseTermsVersion": 2,
  "SettingsVersion": 45,
  "UseContainerdSnapshotter": true
}"#;
        assert_eq!(parse_docker_settings_data_folder(real), None);
    }

    #[test]
    fn a_moved_disk_image_location_is_read_under_any_of_its_key_spellings() {
        // The key has moved across Docker Desktop versions; missing the one
        // this user's version writes means measuring the wrong drive, which is
        // the exact failure the whole data-root resolution exists to avoid.
        for key in ["DataFolder", "dataFolder", "diskPath", "DiskPath"] {
            let json = format!(r#"{{"{key}":"E:\\DockerData","SettingsVersion":45}}"#);
            assert_eq!(
                parse_docker_settings_data_folder(&json).as_deref(),
                Some(r"E:\DockerData"),
                "key {key} was not read"
            );
        }
    }

    #[test]
    fn unreadable_or_empty_docker_settings_are_none_not_an_empty_path() {
        assert_eq!(parse_docker_settings_data_folder("not json at all"), None);
        assert_eq!(parse_docker_settings_data_folder(r#"{"DataFolder":""}"#), None);
        assert_eq!(parse_docker_settings_data_folder(r#"{"DataFolder":"   "}"#), None);
        assert_eq!(parse_docker_settings_data_folder(r#"{"DataFolder":42}"#), None);
    }

    #[test]
    fn free_space_of_a_drive_that_exists_is_a_real_positive_number() {
        // Anti-vacuous: this asserts something a failed measurement cannot
        // satisfy. The temp dir's volume certainly exists and certainly has
        // some free space, so `None` or 0 here means the platform call is
        // broken — which would otherwise show up only as a permanent
        // "could not read free space" warning on every real install.
        let got = free_space_bytes(&std::env::temp_dir());
        assert!(matches!(got, Some(n) if n > 0), "measured {got:?} for the temp volume");
    }

    #[test]
    fn free_space_of_a_path_that_does_not_exist_yet_still_measures_its_drive() {
        // The preflight runs BEFORE the title dir is created.
        let mut p = std::env::temp_dir();
        p.push("dml-preflight-does-not-exist-1a2b3c");
        p.push("nor-does-this");
        assert!(!p.exists());
        assert!(matches!(free_space_bytes(&p), Some(n) if n > 0));
    }

    #[test]
    fn df_output_yields_the_available_column_not_the_used_one() {
        let text = "Filesystem     1024-blocks      Used Available Capacity Mounted on\n\
                    /dev/sda1        102384636  20000000  77000000      21% /\n";
        assert_eq!(parse_df_available_kb(text), Some(77_000_000));
    }

    #[test]
    fn df_garbage_is_none_rather_than_a_wrong_number() {
        assert_eq!(parse_df_available_kb(""), None);
        assert_eq!(parse_df_available_kb("Filesystem 1024-blocks Used Available\n"), None);
        assert_eq!(parse_df_available_kb("a b c d e\nnot numbers here at all\n"), None);
    }

    // -- the games-dir drive (the drive the CHECKOUT lands on) ----------------

    #[test]
    fn the_games_dir_floors_come_from_the_measured_checkout_not_a_guess() {
        // Measured 2026-07-29 in dml-arch against the checkout a Route A
        // install reproduces: 2.4 GB total, 1.3 GB of it .git. See the
        // constants' doc comments. Pinned as VALUES so moving them is a
        // decision, not a one-character edit.
        assert_eq!(REFUSE_GAMES_FREE_BYTES, 8 * GB);
        assert_eq!(WARN_GAMES_FREE_BYTES, 15 * GB);
        assert!(REFUSE_GAMES_FREE_BYTES < WARN_GAMES_FREE_BYTES);
        // ...and the refuse floor must clear the CLONE PEAK, not just the
        // finished tree: the pack lands before the working tree is written.
        let measured_checkout = 2_576_980_378u64; // 2.4 GB, measured
        let clone_peak = measured_checkout + 1_395_864_371; // + the 1.3 GB pack
        assert!(
            REFUSE_GAMES_FREE_BYTES > clone_peak,
            "the floor must survive the clone it is meant to protect"
        );
    }

    #[test]
    fn a_full_games_drive_refuses_even_when_dockers_own_drive_is_roomy() {
        // THE case this check exists for: Docker Desktop's disk image moved to
        // a big E:, games dir left on a nearly-full C:. Measuring only Docker's
        // drive returns Proceed, and the install then dies mid-clone.
        let mut facts = healthy();
        facts.disk = Some(DiskFacts { path: r"E:\DockerData".into(), free_bytes: Some(500 * GB) });
        facts.games_disk = Some(DiskFacts { path: r"C:\Users\x\dml-native".into(), free_bytes: Some(6 * GB) });
        let rep = decide(&facts, false);
        assert_eq!(rep.verdict, Verdict::Refuse, "{:#?}", rep.findings);
        assert_eq!(rep.code, Some(CODE_UNDERSPEC));
        assert_eq!(sev(&rep, Check::Disk), Severity::Ok, "Docker's own drive is fine and must be reported so");
        assert_eq!(sev(&rep, Check::GamesDisk), Severity::Refuse);
        let m = msg(&rep, Check::GamesDisk);
        assert!(m.contains(r"C:\Users\x\dml-native"), "the drive is missing: {m}");
        assert!(m.contains("6.0 GB"), "measured value missing: {m}");
        assert!(m.contains("8.0 GB"), "floor missing: {m}");
    }

    #[test]
    fn games_free_space_just_under_the_refuse_floor_refuses() {
        let rep = decide(&with_games_free(Some(REFUSE_GAMES_FREE_BYTES - 1)), false);
        assert_eq!(rep.verdict, Verdict::Refuse);
        assert_eq!(rep.code, Some(CODE_UNDERSPEC));
        assert_eq!(sev(&rep, Check::GamesDisk), Severity::Refuse);
    }

    #[test]
    fn games_free_space_exactly_at_the_refuse_floor_does_not_refuse() {
        let rep = decide(&with_games_free(Some(REFUSE_GAMES_FREE_BYTES)), false);
        assert_eq!(sev(&rep, Check::GamesDisk), Severity::Warn);
        assert_eq!(rep.verdict, Verdict::Warn);
    }

    #[test]
    fn games_free_space_just_under_the_warn_floor_warns() {
        let rep = decide(&with_games_free(Some(WARN_GAMES_FREE_BYTES - 1)), false);
        assert_eq!(sev(&rep, Check::GamesDisk), Severity::Warn);
        assert_eq!(rep.code, None);
    }

    #[test]
    fn games_free_space_at_and_over_the_warn_floor_is_ok() {
        assert_eq!(sev(&decide(&with_games_free(Some(WARN_GAMES_FREE_BYTES)), false), Check::GamesDisk), Severity::Ok);
        assert_eq!(sev(&decide(&with_games_free(Some(WARN_GAMES_FREE_BYTES + 1)), false), Check::GamesDisk), Severity::Ok);
    }

    #[test]
    fn an_unmeasurable_games_drive_warns_and_names_the_path_it_tried() {
        let rep = decide(&with_games_free(None), false);
        assert_eq!(sev(&rep, Check::GamesDisk), Severity::Warn);
        assert!(msg(&rep, Check::GamesDisk).contains(r"C:\Users\x\dml-native"));
    }

    #[test]
    fn no_games_dir_at_all_warns_rather_than_passing_silently() {
        let mut facts = healthy();
        facts.games_disk = None;
        let rep = decide(&facts, false);
        assert_eq!(sev(&rep, Check::GamesDisk), Severity::Warn);
        assert_eq!(rep.verdict, Verdict::Warn);
    }

    #[test]
    fn one_drive_holding_both_must_clear_both_floors_together() {
        // 42 GB clears the 40 GB Docker floor and the 5 GB checkout floor
        // SEPARATELY, and cannot hold both: they come out of one pool.
        let mut facts = healthy();
        facts.disk = Some(DiskFacts { path: r"C:\ProgramData\Docker".into(), free_bytes: Some(42 * GB) });
        facts.games_disk = Some(DiskFacts { path: r"C:\Users\x\dml-native".into(), free_bytes: Some(42 * GB) });
        facts.games_shares_docker_volume = true;
        let rep = decide(&facts, false);
        assert_eq!(rep.verdict, Verdict::Refuse, "{:#?}", rep.findings);
        assert_eq!(sev(&rep, Check::GamesDisk), Severity::Refuse);
        let m = msg(&rep, Check::GamesDisk);
        assert!(
            m.contains(&format_gb(REFUSE_FREE_BYTES + REFUSE_GAMES_FREE_BYTES)),
            "the COMBINED floor must be in the copy, or the advice is unfollowable: {m}"
        );
        assert!(m.contains("same drive") || m.contains("same volume"), "the copy must say WHY the floor moved: {m}");
    }

    #[test]
    fn two_drives_are_judged_independently() {
        // The mirror of the test above: not sharing a volume must NOT inherit
        // the combined floor, or every normal machine gets a bogus refusal.
        let mut facts = healthy();
        facts.disk = Some(DiskFacts { path: r"E:\DockerData".into(), free_bytes: Some(42 * GB) });
        facts.games_disk = Some(DiskFacts { path: r"C:\Users\x\dml-native".into(), free_bytes: Some(42 * GB) });
        facts.games_shares_docker_volume = false;
        let rep = decide(&facts, false);
        assert_eq!(sev(&rep, Check::GamesDisk), Severity::Ok);
        assert_eq!(sev(&rep, Check::Disk), Severity::Warn); // 42 GB is in the 40-60 warn band
        assert_eq!(rep.verdict, Verdict::Warn);
    }

    #[test]
    fn allow_underspec_downgrades_the_games_drive_refusal_and_keeps_the_numbers() {
        let facts = with_games_free(Some(2 * GB));
        assert_eq!(decide(&facts, false).verdict, Verdict::Refuse);
        let forced = decide(&facts, true);
        assert_eq!(forced.verdict, Verdict::Warn);
        assert!(forced.overridden);
        let m = msg(&forced, Check::GamesDisk);
        assert!(m.contains("2.0 GB") && m.contains("8.0 GB"), "the numbers must survive the override: {m}");
    }

    #[test]
    fn same_volume_is_read_off_the_drive_letter_on_windows() {
        assert!(same_volume(true, r"C:\ProgramData\Docker", r"C:\Users\x\dml-native"));
        assert!(same_volume(true, r"c:\programdata\docker", r"C:\Users\x\dml-native"), "drive letters are case-insensitive");
        assert!(!same_volume(true, r"E:\DockerData", r"C:\Users\x\dml-native"));
        assert!(same_volume(true, r"\\nas\share\docker", r"\\NAS\share\games"), "a UNC share is one volume");
        assert!(!same_volume(true, r"\\nas\share\docker", r"\\nas\other\games"));
        assert!(!same_volume(true, "", r"C:\x"));
    }

    #[test]
    fn off_windows_a_path_alone_never_claims_two_dirs_share_a_volume() {
        // Two POSIX directories under / can be different mounts; guessing
        // "same volume" there would invent a refusal.
        assert!(!same_volume(false, "/var/lib/docker", "/home/dml/games"));
        assert!(!same_volume(false, "/var/lib/docker", "/var/lib/other"));
    }

    // -- git ------------------------------------------------------------------

    #[test]
    fn a_missing_git_refuses_with_its_own_code() {
        let mut facts = healthy();
        facts.git = Tri::No;
        let rep = decide(&facts, false);
        assert_eq!(rep.verdict, Verdict::Refuse);
        assert_eq!(rep.code, Some(CODE_GIT_MISSING));
        assert_eq!(sev(&rep, Check::Git), Severity::Refuse);
        assert!(msg(&rep, Check::Git).to_lowercase().contains("git"));
    }

    #[test]
    fn allow_underspec_does_not_override_a_missing_git() {
        // The override is about hardware floors. No flag makes `git clone` run
        // without git.
        let mut facts = healthy();
        facts.git = Tri::No;
        let rep = decide(&facts, true);
        assert_eq!(rep.verdict, Verdict::Refuse);
        assert_eq!(rep.code, Some(CODE_GIT_MISSING));
    }

    #[test]
    fn a_git_that_did_not_answer_warns_and_never_claims_it_is_missing() {
        let mut facts = healthy();
        facts.git = Tri::Unknown;
        let rep = decide(&facts, false);
        assert_eq!(sev(&rep, Check::Git), Severity::Warn);
        assert_eq!(rep.verdict, Verdict::Warn);
        let m = msg(&rep, Check::Git).to_lowercase();
        assert!(!m.contains("not installed") && !m.contains("could not be found"), "a shrug must not assert absence: {m}");
    }

    #[test]
    fn an_unreachable_docker_outranks_a_missing_git_in_the_code() {
        // Both refuse; the caller switches on ONE code, and docker is the
        // first thing to fix.
        let mut facts = healthy();
        facts.docker.reachable = Tri::Unknown;
        facts.git = Tri::No;
        assert_eq!(decide(&facts, false).code, Some(CODE_DOCKER_UNREACHABLE));
    }

    #[test]
    fn the_git_probe_only_calls_absence_on_a_missing_program() {
        assert_eq!(classify_git_probe(&ProbeOutcome::ProgramMissing), Tri::No);
        assert_eq!(classify_git_probe(&ProbeOutcome::CouldNotTell), Tri::Unknown);
        assert_eq!(classify_git_probe(&ran(0, "git version 2.47.1.windows.1\n", "")), Tri::Yes);
        // Something answered, but nothing that looks like git: a shrug, not a
        // claim either way.
        assert_eq!(classify_git_probe(&ran(0, "\n", "")), Tri::Unknown);
        assert_eq!(classify_git_probe(&ran(1, "", "'git' is not recognized\n")), Tri::Unknown);
    }

    // -- Docker Hub -----------------------------------------------------------

    #[test]
    fn a_registry_that_answered_is_ok_whatever_it_answered() {
        // 401 is the registry's normal answer to an unauthenticated /v2/.
        for status in [200u16, 401, 403, 500] {
            let (tri, _) = classify_hub_probe(Ok(status));
            assert_eq!(tri, Tri::Yes, "status {status} is an ANSWER");
        }
    }

    #[test]
    fn a_failed_hub_probe_is_a_shrug_never_a_claim_that_hub_is_down() {
        let (tri, detail) = classify_hub_probe(Err("dns error: failed to lookup address".into()));
        assert_eq!(tri, Tri::Unknown, "a client-side probe cannot prove there is no route");
        assert!(detail.unwrap_or_default().contains("dns error"), "the probe's own words must survive");
    }

    #[test]
    fn an_unreachable_hub_warns_and_never_refuses() {
        // Deliberately WEAKER than the WSL installer, which exits 1 here
        // (install-wow-wotlk.sh:315-331): a proxy the daemon knows about, a
        // registry mirror, or a firewall that blocks this process and not the
        // engine all look identical from here — and a wrong pull fails in
        // minutes, not in the hours the hardware floors are protecting.
        let mut facts = healthy();
        facts.hub = Tri::Unknown;
        facts.hub_detail = Some("dns error: failed to lookup address".into());
        let rep = decide(&facts, false);
        assert_eq!(rep.verdict, Verdict::Warn);
        assert_eq!(rep.code, None);
        assert_eq!(sev(&rep, Check::DockerHub), Severity::Warn);
        let m = msg(&rep, Check::DockerHub);
        assert!(m.contains("registry-1.docker.io"), "the copy must name what was probed: {m}");
        assert!(m.contains("dns error"), "the probe's own words must reach the user: {m}");
        assert!(
            !m.to_lowercase().contains("no internet") && !m.to_lowercase().contains("is down"),
            "a could-not-tell must not assert a cause: {m}"
        );
    }

    #[test]
    fn a_reachable_hub_is_ok_and_silent() {
        let rep = decide(&healthy(), false);
        assert_eq!(sev(&rep, Check::DockerHub), Severity::Ok);
        assert_eq!(rep.verdict, Verdict::Proceed);
    }

    // -- report-wide invariant ------------------------------------------------

    #[test]
    fn the_report_answers_every_question_task_4_asks() {
        // The gap this closes: `decide` used to return Proceed on a machine
        // with no git and no route to Docker Hub, because neither question was
        // ever asked. An absent check family is invisible to a report that
        // only iterates the families it happens to contain.
        let rep = decide(&healthy(), false);
        for check in checks_task_4_requires() {
            assert!(
                rep.finding(check).is_some(),
                "no finding for {check:?} — a question Task 4 requires went unasked: {:#?}",
                rep.findings
            );
        }
    }

    #[test]
    fn every_shortfall_finding_carries_its_floor_in_the_copy() {
        // The blanket version of the "report the numbers" rule: whatever the
        // combination, a non-Ok hardware finding must quote a GB figure —
        // INCLUDING the could-not-measure branches, which still know the floor
        // they could not check against. (The `|| contains("Could not")` escape
        // hatch this assertion used to carry made it satisfiable by any
        // could-not-tell message, floor or no floor.)
        for mem in [Some(4 * GB), Some(7 * GB), None, Some(32 * GB)] {
            for free in [Some(10 * GB), Some(50 * GB), None, Some(500 * GB)] {
                for games in [Some(1 * GB), Some(7 * GB), None, Some(500 * GB)] {
                    for (disk_known, games_known) in [(true, true), (false, true), (true, false)] {
                        let mut facts = healthy();
                        facts.docker.mem_bytes = mem;
                        facts.disk = disk_known
                            .then(|| DiskFacts { path: r"D:\Docker".into(), free_bytes: free });
                        facts.games_disk = games_known
                            .then(|| DiskFacts { path: r"C:\games".into(), free_bytes: games });
                        let rep = decide(&facts, false);
                        for f in rep.findings.iter().filter(|f| {
                            matches!(f.check, Check::Memory | Check::Disk | Check::GamesDisk)
                        }) {
                            if f.severity != Severity::Ok {
                                assert!(
                                    f.message.contains(" GB"),
                                    "a non-Ok hardware finding with no numbers in it: {f:?}"
                                );
                            }
                            assert!(!f.message.is_empty());
                            assert!(
                                f.severity == Severity::Ok || !f.hint.is_empty(),
                                "a non-Ok finding with nothing to do about it: {f:?}"
                            );
                        }
                    }
                }
            }
        }
    }
}
