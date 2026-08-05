//! The SOAP credentials the launcher proved must reach the CLI that uses them.
//!
//! # The split this closes (R6)
//!
//! `dml_wow::soap_autosetup` runs IN THE LAUNCHER, on Windows, and writes the
//! Windows `~/.dml/soap.env` (`%USERPROFILE%\.dml\soap.env`). On
//! [`Backend::Arch`] and [`Backend::Wsl`] the CLI that answers every SOAP-backed
//! verb — GM Tools, My Party, console send, announcements, motd — runs INSIDE
//! the `dml-arch` distro and reads `/home/dml/.dml/soap.env`. Those are two
//! different files on two different filesystems, and nothing has ever copied one
//! to the other.
//!
//! The result is the worst shape available: the launcher proves a real SOAP
//! round-trip, reports success, shows the account in its credentials panel — and
//! every CLI-routed SOAP verb comes back `SOAP_AUTH`. Measured on the author's
//! own machine on 2026-08-05: both copies named the account `dmlsoap`, and their
//! passwords hashed differently.
//!
//! # Which side owns the truth
//!
//! **The launcher owns it, and the distro copy is a replica it publishes** — but
//! it publishes ONLY over a copy the distro's own CLI has just been refused
//! with. The launcher is the only side that generates the password, proves it
//! with a live round-trip and can persist it on all three backends; the distro
//! copy is the only thing 74 translated verbs can read. So the launcher writes,
//! and the tie-break when the two disagree is decided by the server rather than
//! by either file: **a credential the CLI route still authenticates with is
//! never overwritten** ([`DistroSoap::Working`] → [`SyncOutcome::CliAlreadyWorks`]),
//! and a probe that could not answer is not a probe that answered "broken"
//! ([`DistroSoap::Unknown`] → nothing is written).
//!
//! That ordering is what makes this safe on [`Backend::Wsl`], where a real
//! user's real server lives today: a Wsl install whose distro credentials work
//! sees this code make one read-only probe and stop.
//!
//! # Secrets never go in argv
//!
//! A password on a command line is visible in `ps` inside the distro and in the
//! Windows process listing of `wsl.exe` itself. So the file is delivered on
//! **stdin**: [`WRITE_SCRIPT`] is a fixed literal that interpolates nothing, and
//! the only thing that crosses the boundary carrying the secret is the child's
//! standard input, which no process listing shows.
//!
//! Rejected alternatives, for the record:
//!
//! * **`WSLENV` forwarding of `DML_SOAP_USER`/`DML_SOAP_PASS`.** Those two
//!   variables OUTRANK `~/.dml/soap.env` in `SoapConfig::load`, and this repo
//!   has a recorded incident where exactly that precedence produced one GM3
//!   account per launcher start, forever: the launcher kept reading `Rejected`
//!   because the env pair shadowed the file it had just written. Setting them in
//!   the launcher's own process reintroduces that shape verbatim. Setting them
//!   per-spawn avoids the loop but spreads the secret into the environment of
//!   every unrelated child, inverts precedence inside the distro (its own
//!   `soap.env` would stop mattering), reaches only spawns that go through
//!   [`DmlRunner`], and helps nobody running `dml` in the distro by hand.
//! * **The distro owning it and the launcher reading through.** `SoapConfig::load`
//!   is called on every status poll, is synchronous, and exists on
//!   [`Backend::Native`] where there is no distro at all. Putting a `wsl.exe`
//!   spawn inside it would put one on the hot path of a backend that must not
//!   change.
//!
//! # `Backend::Native` is untouched, structurally
//!
//! [`cli_home_is_the_distro`] is the only gate, it is consulted before anything
//! else, and it answers `false` for [`Backend::Native`] — whose CLI is Git Bash
//! on Windows reading the very file the launcher just wrote. No probe, no spawn,
//! no write. `native_never_touches_a_distro` pins it.

use crate::backend::Backend;
use crate::envelope::Envelope;
use crate::runner::{DmlRunner, DISTRO, USER};

/// Does this backend's CLI read a DIFFERENT `~/.dml/soap.env` from the one the
/// launcher writes?
///
/// `Arch` and `Wsl` both run their CLI inside `dml-arch`, where `$HOME` is
/// `/home/dml` — so both are split from the Windows file, and both are repaired
/// by the same replica. `Native` runs the `dml` script under Git Bash on the
/// Windows host, with the same `USERPROFILE`, so its CLI reads the launcher's
/// own file and there is nothing to copy.
pub fn cli_home_is_the_distro(b: Backend) -> bool {
    match b {
        Backend::Arch | Backend::Wsl => true,
        Backend::Native => false,
    }
}

/// The read-only verb used to ask the CLI route whether it authenticates.
///
/// `wow server-info` is chosen for three reasons and all three are load-bearing:
/// it is READ-ONLY (it runs against a server the user cares about), both CLIs
/// implement it with the same rc dispatch (`dml-wow`'s `Cmd::ServerInfo` and
/// bash's `server-info)` arm both map SOAP rc 3 to `SOAP_AUTH` and everything
/// else to `ok` with `online:false`), and it is already in
/// [`crate::vocab::TABLE`], so the Arch runner translates it to `dml-wow
/// server-info` and the Wsl runner sends `dml wow server-info --json`. The
/// probe therefore travels the EXACT route the 74 translated verbs travel — a
/// probe that took a different road would answer a different question.
pub const PROBE_VERB: [&str; 2] = ["wow", "server-info"];

/// What the CLI route's own SOAP credentials just did.
///
/// Three states, not two, for the reason this repo keeps relearning: a world
/// server that is not answering tells us NOTHING about the credentials, and
/// reading that silence as "broken" would overwrite a perfectly good file every
/// time the server was merely starting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DistroSoap {
    /// The CLI authenticated and the world answered. Its credentials are good.
    Working,
    /// The server answered and REFUSED the CLI's credentials (`SOAP_AUTH`).
    Refused,
    /// No verdict: the server is down, the probe failed, or the answer was a
    /// shape we do not recognise.
    Unknown(String),
}

/// Classify the envelope `PROBE_VERB` returns.
///
/// Pure, so every arm is provable without a distro. The mapping mirrors both
/// CLIs' `server-info` contract exactly:
///
/// * `ok:false` + `SOAP_AUTH` → [`DistroSoap::Refused`] — the ONLY thing that
///   licenses a write.
/// * `ok:true` + `online:true` → [`DistroSoap::Working`].
/// * `ok:true` + `online:false` → [`DistroSoap::Unknown`]. Down and "SOAP
///   faulted" both fold into `online:false` by design, and neither is a
///   statement about the password.
/// * any other error code → [`DistroSoap::Unknown`]. A `NOT_FOUND` for a title
///   that is not installed must not be read as a credentials verdict.
pub fn classify_probe(env: &Envelope) -> DistroSoap {
    if !env.ok {
        return match env.error.as_ref() {
            Some(e) if e.code == "SOAP_AUTH" => DistroSoap::Refused,
            Some(e) => DistroSoap::Unknown(format!("{}: {}", e.code, e.message)),
            None => DistroSoap::Unknown("the CLI returned an error with no code".to_string()),
        };
    }
    match env.data["online"].as_bool() {
        Some(true) => DistroSoap::Working,
        Some(false) => DistroSoap::Unknown(
            "the world server is not answering SOAP, so its credentials cannot be judged"
                .to_string(),
        ),
        None => DistroSoap::Unknown(format!(
            "server-info answered without an `online` field: {}",
            env.data
        )),
    }
}

/// Ask the CLI route, through the real runner.
pub fn probe_cli_soap(runner: &DmlRunner) -> DistroSoap {
    let argv: Vec<&str> = PROBE_VERB.to_vec();
    match runner.run_json(&argv) {
        Ok(env) => classify_probe(&env),
        Err(e) => DistroSoap::Unknown(e.to_string()),
    }
}

/// How many failed publish attempts one launcher run gets.
///
/// A bound rather than an unlatched retry, for the same reason
/// `soap_autosetup::MAX_VERIFY_TRIES` exists: this runs off a poll that ticks
/// every few seconds, and an unbounded retry against a distro that will never
/// answer is a `wsl.exe` spawn storm for the life of the process.
pub const MAX_PUBLISH_FAILURES: u8 = 3;

/// Where this launcher run got to. One instance, held by the launcher's
/// `AppState`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SoapEnvSync {
    /// Fingerprint of the file contents last successfully published, if any.
    ///
    /// A fingerprint rather than a bare `bool` because the credential can CHANGE
    /// mid-run: automatic setup creates an account and rewrites the launcher's
    /// file, and a `bool` latch set by an earlier publish would leave the distro
    /// holding the previous one. A hash rather than the contents so the struct
    /// this state lives in never carries a second copy of the password.
    pub published: Option<u64>,
    pub failures: u8,
}

/// FNV-1a 64, written out for the same reason `composegen::fnv1a64` is: a value
/// compared across runs must not depend on a hasher Rust is free to change.
///
/// Used ONLY to notice that the credential changed. It is never logged, never
/// serialized and never leaves the process.
pub fn fingerprint(contents: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in contents.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// What one sync attempt concluded. Reported, never thrown: none of these is a
/// failure of the command that triggered it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncOutcome {
    /// This backend's CLI reads the launcher's own file. Nothing was spawned.
    NotApplicable,
    /// Nobody has supplied credentials, so there is nothing to copy. Publishing
    /// the compiled-in `admin`/`admin` would replace a real file with a guess.
    NoCredentials,
    /// This exact credential was already published in this run.
    AlreadyPublished,
    /// Too many failures this run; the manual route (`native_soap_copy`, or
    /// editing the file in the distro) is what is left.
    Exhausted,
    /// The CLI route authenticates with what it already has. NOTHING WAS
    /// WRITTEN — this is the arm that makes the fix safe on `Backend::Wsl`.
    CliAlreadyWorks,
    /// The CLI route could not be asked. Nothing was written; a later poll asks
    /// again.
    Unknown(String),
    /// Written, and the CLI route now authenticates.
    Published,
    /// Written, but the CLI route still does not authenticate. Counted as a
    /// failure: whatever is wrong, this replica did not fix it.
    Unproven(String),
    /// The write itself failed.
    Failed(String),
}

/// The wire word for an outcome. Non-secret by construction — no arm here can
/// carry a password, because none of the payloads above is one.
pub fn outcome_status(o: &SyncOutcome) -> &'static str {
    match o {
        SyncOutcome::NotApplicable => "not_applicable",
        SyncOutcome::NoCredentials => "no_credentials",
        SyncOutcome::AlreadyPublished => "already_published",
        SyncOutcome::Exhausted => "exhausted",
        SyncOutcome::CliAlreadyWorks => "cli_already_works",
        SyncOutcome::Unknown(_) => "unknown",
        SyncOutcome::Published => "published",
        SyncOutcome::Unproven(_) => "unproven",
        SyncOutcome::Failed(_) => "failed",
    }
}

/// One sync attempt, with every effect injected.
///
/// `contents` is the file the launcher wrote on its own side — produced by
/// `dml_wow::soap_bootstrap::soap_env_contents`, which is where the LF-only rule
/// lives (a CRLF leaves a trailing `\r` on the password, and the only symptom is
/// `SOAP_AUTH`). This crate takes it as bytes rather than reproducing the format,
/// so there is one definition of what a `soap.env` looks like.
///
/// `configured` comes from `SoapConfig::load_with_provenance` and is passed
/// through rather than re-derived: it is the only thing that can tell a real
/// account named `admin` from a fresh install that has none, and guessing it
/// here would rebuild that mistake one layer down.
///
/// ORDER IS THE SAFETY PROPERTY. The probe runs BEFORE the write, so a working
/// distro credential is never overwritten; the probe runs AGAIN after the write,
/// so "published" means the CLI route really authenticates now rather than "the
/// bytes were delivered". The second probe is what turns this from a hopeful
/// copy into a proof, and it is the same proof `soap_bootstrap` insists on
/// before the launcher's own file is written.
pub fn sync_with(
    backend: Backend,
    configured: bool,
    contents: &str,
    state: &mut SoapEnvSync,
    probe: impl Fn() -> DistroSoap,
    write: impl Fn(&str) -> Result<(), String>,
) -> SyncOutcome {
    if !cli_home_is_the_distro(backend) {
        return SyncOutcome::NotApplicable;
    }
    if !configured {
        return SyncOutcome::NoCredentials;
    }
    let fp = fingerprint(contents);
    if state.published == Some(fp) {
        return SyncOutcome::AlreadyPublished;
    }
    if state.failures >= MAX_PUBLISH_FAILURES {
        return SyncOutcome::Exhausted;
    }

    match probe() {
        // The tie-break, and the reason this is safe to turn on for the backend
        // a real server runs on today.
        DistroSoap::Working => SyncOutcome::CliAlreadyWorks,
        // Not a verdict about credentials. Not counted as a failure either: a
        // world server that has not finished booting must not burn the budget.
        DistroSoap::Unknown(why) => SyncOutcome::Unknown(why),
        DistroSoap::Refused => {
            if let Err(e) = write(contents) {
                state.failures += 1;
                return SyncOutcome::Failed(e);
            }
            match probe() {
                DistroSoap::Working => {
                    state.published = Some(fp);
                    SyncOutcome::Published
                }
                DistroSoap::Refused => {
                    state.failures += 1;
                    SyncOutcome::Unproven(
                        "the credentials were copied into the distro but its CLI still could \
                         not authenticate with them"
                            .to_string(),
                    )
                }
                DistroSoap::Unknown(why) => {
                    state.failures += 1;
                    SyncOutcome::Unproven(why)
                }
            }
        }
    }
}

/// The script the distro runs. It receives the file on STDIN and interpolates
/// NOTHING — this is a fixed literal, and it is the reason no password ever
/// appears in an argv.
///
/// `umask 077` before the create, so the temp file is 0600 from birth rather
/// than 0644 for a moment. The write goes to `.new` and is then `mv`'d over the
/// target, so a CLI reading the file mid-publish can never see half of it —
/// a half-written password authenticates as a different string, and the only
/// symptom would be a `SOAP_AUTH` nobody could explain.
pub const WRITE_SCRIPT: &str = "set -e; umask 077; mkdir -p \"$HOME/.dml\"; \
cat > \"$HOME/.dml/soap.env.new\"; chmod 600 \"$HOME/.dml/soap.env.new\"; \
mv -f \"$HOME/.dml/soap.env.new\" \"$HOME/.dml/soap.env\"";

/// Deliver `contents` to `$HOME/.dml/soap.env` inside the distro.
///
/// `--exec` rather than the bare `--` form, as everywhere else that crosses this
/// boundary: `--` runs a shell inside the distro, which would split and expand
/// the argv. Here the argv is our own literal either way, but the rule is
/// structural rather than case-by-case.
///
/// The secret travels on the child's stdin. `sh` is spawned with `-c
/// WRITE_SCRIPT`, so the process listing on both sides of the boundary shows the
/// script and nothing else.
pub fn publish_to_distro(contents: &str) -> Result<(), String> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut cmd = Command::new("wsl.exe");
    cmd.args(["-d", DISTRO, "-u", USER, "--exec", "sh", "-c", WRITE_SCRIPT]);
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("could not run wsl.exe to reach the {DISTRO} distro: {e}"))?;
    {
        let mut stdin = child.stdin.take().expect("stdin piped above");
        stdin
            .write_all(contents.as_bytes())
            .map_err(|e| format!("could not send the credentials to the distro: {e}"))?;
    } // dropped, so the child sees EOF and `cat` finishes
    let out = child
        .wait_with_output()
        .map_err(|e| format!("the distro write did not finish: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    // The child's stderr, never its stdin. `WRITE_SCRIPT` echoes nothing, so
    // there is no path by which the password reaches this string.
    let err = crate::envelope::decode_wsl_output(&out.stderr);
    let err = err.trim();
    Err(if err.is_empty() {
        format!(
            "writing soap.env inside {DISTRO} exited with code {}",
            out.status.code().unwrap_or(-1)
        )
    } else {
        err.to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn env_ok(data: serde_json::Value) -> Envelope {
        crate::envelope::parse_envelope(
            &serde_json::json!({ "ok": true, "data": data }).to_string(),
        )
        .unwrap()
    }

    fn env_err(code: &str) -> Envelope {
        crate::envelope::parse_envelope(
            &serde_json::json!({
                "ok": false,
                "error": { "code": code, "message": "m", "hint": "" }
            })
            .to_string(),
        )
        .unwrap()
    }

    /// A scripted set of seams. The counters are what most assertions read: the
    /// properties that matter most here are NEGATIVE ones (nothing was written,
    /// nothing was even probed), and a stub that only returned values could not
    /// express them.
    #[derive(Default)]
    struct Seams {
        probes: RefCell<Vec<DistroSoap>>,
        probe_calls: RefCell<usize>,
        writes: RefCell<Vec<String>>,
        write_error: Option<String>,
    }

    impl Seams {
        /// `answers` are consumed in order; the last one repeats, so a test that
        /// wants "refused, then working" says exactly that.
        fn new(answers: Vec<DistroSoap>) -> Self {
            Seams { probes: RefCell::new(answers), ..Default::default() }
        }

        fn run(
            &self,
            backend: Backend,
            configured: bool,
            contents: &str,
            state: &mut SoapEnvSync,
        ) -> SyncOutcome {
            sync_with(
                backend,
                configured,
                contents,
                state,
                || {
                    *self.probe_calls.borrow_mut() += 1;
                    let mut q = self.probes.borrow_mut();
                    if q.len() > 1 {
                        q.remove(0)
                    } else {
                        q.first().cloned().unwrap_or(DistroSoap::Working)
                    }
                },
                |c| {
                    self.writes.borrow_mut().push(c.to_string());
                    match &self.write_error {
                        Some(e) => Err(e.clone()),
                        None => Ok(()),
                    }
                },
            )
        }
    }

    const CREDS: &str = "DML_SOAP_URL=http://127.0.0.1:7878/\nDML_SOAP_USER=dmlsoap\nDML_SOAP_PASS=hunter2hunter2AB\n";

    // -- the gate -----------------------------------------------------------

    /// CONSTRAINT 1, pinned structurally. `Backend::Native` has no distro; its
    /// CLI is Git Bash on Windows reading the very file the launcher wrote. Not
    /// one probe, not one spawn, not one byte.
    #[test]
    fn native_never_touches_a_distro() {
        let s = Seams::new(vec![DistroSoap::Refused]);
        let mut st = SoapEnvSync::default();
        assert_eq!(
            s.run(Backend::Native, true, CREDS, &mut st),
            SyncOutcome::NotApplicable
        );
        assert_eq!(*s.probe_calls.borrow(), 0, "native must not even ASK");
        assert!(s.writes.borrow().is_empty());
        assert_eq!(st, SoapEnvSync::default(), "and it must not move the latch");
    }

    #[test]
    fn both_in_distro_backends_are_in_scope() {
        assert!(cli_home_is_the_distro(Backend::Arch));
        assert!(cli_home_is_the_distro(Backend::Wsl));
        assert!(!cli_home_is_the_distro(Backend::Native));
    }

    /// `admin`/`admin` is a value the resolver INVENTED. Copying it into the
    /// distro would replace a real file with a guess, and would report the guess
    /// as configured on the far side.
    #[test]
    fn an_unconfigured_launcher_has_nothing_to_publish() {
        let s = Seams::new(vec![DistroSoap::Refused]);
        let mut st = SoapEnvSync::default();
        assert_eq!(
            s.run(Backend::Arch, false, CREDS, &mut st),
            SyncOutcome::NoCredentials
        );
        assert_eq!(*s.probe_calls.borrow(), 0);
        assert!(s.writes.borrow().is_empty());
    }

    // -- the tie-break ------------------------------------------------------

    /// THE PROPERTY THAT MAKES THIS SAFE ON `Backend::Wsl`.
    ///
    /// A user whose distro credentials work is a user whose server works. The
    /// launcher's copy may be newer, may be the one its own panel displays, and
    /// may even be for a different server entirely (measured: on 2026-08-05 the
    /// two copies on the author's machine named the same account with different
    /// passwords). None of that licenses replacing a credential the server is
    /// still accepting.
    #[test]
    fn a_working_cli_credential_is_never_overwritten() {
        let s = Seams::new(vec![DistroSoap::Working]);
        let mut st = SoapEnvSync::default();
        assert_eq!(
            s.run(Backend::Wsl, true, CREDS, &mut st),
            SyncOutcome::CliAlreadyWorks
        );
        assert!(s.writes.borrow().is_empty(), "clobbered a working credential");
        assert_eq!(*s.probe_calls.borrow(), 1, "one read-only probe, and then stop");
        // Nothing was published, so a later poll (after, say, the account is
        // changed) is still free to act.
        assert_eq!(st.published, None);
        assert_eq!(st.failures, 0);
    }

    /// Tri-state discipline. A world server that has not finished booting says
    /// NOTHING about the credentials, and reading its silence as "broken" would
    /// overwrite a good file on every restart.
    #[test]
    fn a_probe_that_could_not_answer_writes_nothing_and_costs_no_budget() {
        let s = Seams::new(vec![DistroSoap::Unknown("world is down".into())]);
        let mut st = SoapEnvSync::default();
        match s.run(Backend::Arch, true, CREDS, &mut st) {
            SyncOutcome::Unknown(why) => assert!(why.contains("down"), "{why}"),
            other => panic!("{other:?}"),
        }
        assert!(s.writes.borrow().is_empty());
        assert_eq!(
            st.failures, 0,
            "a server that is merely starting must not burn the retry budget"
        );
    }

    // -- the fix -------------------------------------------------------------

    /// R6, closed: refused → write → proven.
    #[test]
    fn a_refused_cli_credential_is_replaced_and_then_reproven() {
        let s = Seams::new(vec![DistroSoap::Refused, DistroSoap::Working]);
        let mut st = SoapEnvSync::default();
        assert_eq!(s.run(Backend::Arch, true, CREDS, &mut st), SyncOutcome::Published);
        assert_eq!(s.writes.borrow().len(), 1);
        assert_eq!(
            s.writes.borrow()[0],
            CREDS,
            "the bytes must reach the distro unchanged -- a mangled password is a SOAP_AUTH \
             with nothing pointing at the file"
        );
        assert_eq!(
            *s.probe_calls.borrow(),
            2,
            "the SECOND probe is what makes 'published' a proof rather than a hope"
        );
        assert_eq!(st.published, Some(fingerprint(CREDS)));
        assert_eq!(st.failures, 0);
    }

    /// A write that lands and still does not authenticate is NOT a success.
    /// Reporting it as one would recreate the exact lie this whole area exists
    /// to remove: the app saying SOAP is configured while every button is dead.
    #[test]
    fn a_write_that_does_not_authenticate_is_not_published() {
        let s = Seams::new(vec![DistroSoap::Refused, DistroSoap::Refused]);
        let mut st = SoapEnvSync::default();
        match s.run(Backend::Arch, true, CREDS, &mut st) {
            SyncOutcome::Unproven(why) => assert!(why.contains("still could not"), "{why}"),
            other => panic!("{other:?}"),
        }
        assert_eq!(st.published, None, "an unproven copy must not latch");
        assert_eq!(st.failures, 1);
    }

    #[test]
    fn a_failed_write_is_reported_with_the_distros_own_words() {
        let s = Seams {
            probes: RefCell::new(vec![DistroSoap::Refused]),
            write_error: Some("mv: cannot move: Read-only file system".into()),
            ..Default::default()
        };
        let mut st = SoapEnvSync::default();
        match s.run(Backend::Arch, true, CREDS, &mut st) {
            SyncOutcome::Failed(e) => assert!(e.contains("Read-only file system"), "{e}"),
            other => panic!("{other:?}"),
        }
        assert_eq!(st.failures, 1);
        assert_eq!(*s.probe_calls.borrow(), 1, "a failed write is not re-probed");
    }

    // -- the bounds ----------------------------------------------------------

    /// ONE PUBLISH PER CREDENTIAL PER RUN. This rides a poll that ticks every
    /// few seconds; without the latch every tick would be two `wsl.exe` spawns
    /// and a rewrite of a file the CLI may be reading.
    #[test]
    fn the_same_credential_is_published_once_per_run() {
        let s = Seams::new(vec![DistroSoap::Refused, DistroSoap::Working]);
        let mut st = SoapEnvSync::default();
        assert_eq!(s.run(Backend::Arch, true, CREDS, &mut st), SyncOutcome::Published);
        for _ in 0..5 {
            assert_eq!(
                s.run(Backend::Arch, true, CREDS, &mut st),
                SyncOutcome::AlreadyPublished
            );
        }
        assert_eq!(s.writes.borrow().len(), 1);
        assert_eq!(*s.probe_calls.borrow(), 2, "a latched run must not keep probing");
    }

    /// ...but the credential can CHANGE mid-run, and that is not exotic: it is
    /// what happens the moment automatic setup creates an account and rewrites
    /// the launcher's own file. A `bool` latch would leave the distro holding
    /// the previous password forever, which is the bug this module exists to
    /// close, one layer in.
    #[test]
    fn a_new_credential_is_published_even_after_an_earlier_one_was() {
        let s = Seams::new(vec![DistroSoap::Refused, DistroSoap::Working]);
        let mut st = SoapEnvSync::default();
        assert_eq!(s.run(Backend::Arch, true, CREDS, &mut st), SyncOutcome::Published);

        let rotated = CREDS.replace("hunter2hunter2AB", "ZZZZZZZZZZZZZZZZ");
        assert_ne!(rotated, CREDS);
        *s.probes.borrow_mut() = vec![DistroSoap::Refused, DistroSoap::Working];
        assert_eq!(s.run(Backend::Arch, true, &rotated, &mut st), SyncOutcome::Published);

        assert_eq!(s.writes.borrow().len(), 2);
        assert_eq!(s.writes.borrow()[1], rotated);
        assert_eq!(st.published, Some(fingerprint(&rotated)));
    }

    #[test]
    fn failures_are_bounded_so_a_broken_distro_is_not_a_spawn_storm() {
        let s = Seams {
            probes: RefCell::new(vec![DistroSoap::Refused]),
            write_error: Some("wsl.exe: no such distribution".into()),
            ..Default::default()
        };
        let mut st = SoapEnvSync::default();
        for _ in 0..MAX_PUBLISH_FAILURES {
            assert!(matches!(
                s.run(Backend::Arch, true, CREDS, &mut st),
                SyncOutcome::Failed(_)
            ));
        }
        assert_eq!(s.run(Backend::Arch, true, CREDS, &mut st), SyncOutcome::Exhausted);
        assert_eq!(s.writes.borrow().len(), MAX_PUBLISH_FAILURES as usize);
        assert_eq!(
            *s.probe_calls.borrow(),
            MAX_PUBLISH_FAILURES as usize,
            "exhausted means it stops asking too"
        );
    }

    #[test]
    fn the_fingerprint_separates_credentials_and_is_stable() {
        assert_eq!(fingerprint(CREDS), fingerprint(CREDS));
        assert_ne!(fingerprint(CREDS), fingerprint(&CREDS.replace("dmlsoap", "dmlsoap_ab12ef")));
        // A one-character password change must move it -- that is the whole job.
        assert_ne!(
            fingerprint(CREDS),
            fingerprint(&CREDS.replace("hunter2hunter2AB", "hunter2hunter2Ab"))
        );
    }

    // -- the probe classifier -------------------------------------------------

    #[test]
    fn only_soap_auth_licenses_a_write() {
        assert_eq!(classify_probe(&env_err("SOAP_AUTH")), DistroSoap::Refused);
        // Every other refusal is a statement about something else. A NOT_FOUND
        // for an uninstalled title read as "bad credentials" would overwrite the
        // distro's file on a machine with no server at all.
        for code in ["NOT_FOUND", "BAD_ARGS", "DB_UNREACHABLE", "CLI_CRASH"] {
            assert!(
                matches!(classify_probe(&env_err(code)), DistroSoap::Unknown(_)),
                "{code} must not be read as a credentials verdict"
            );
        }
    }

    #[test]
    fn an_online_world_means_the_cli_authenticated() {
        assert_eq!(
            classify_probe(&env_ok(serde_json::json!({ "online": true, "players": 3 }))),
            DistroSoap::Working
        );
    }

    /// `online:false` is what BOTH CLIs emit for an unreachable server AND for a
    /// SOAP fault. Neither says anything about the password, so neither may
    /// license a write.
    #[test]
    fn an_offline_world_is_unknown_not_refused() {
        assert!(matches!(
            classify_probe(&env_ok(serde_json::json!({ "online": false, "version": null }))),
            DistroSoap::Unknown(_)
        ));
        // ...and a shape we do not recognise is unknown too, rather than being
        // read as either verdict.
        assert!(matches!(
            classify_probe(&env_ok(serde_json::json!({ "something": "else" }))),
            DistroSoap::Unknown(_)
        ));
    }

    // -- the write script -----------------------------------------------------

    /// SECRETS NEVER GO IN ARGV. The script is a fixed literal with no format
    /// placeholder and no credential-shaped token, so there is no version of it
    /// that could carry a password into a process listing.
    #[test]
    fn the_write_script_interpolates_nothing_and_reads_the_secret_from_stdin() {
        assert!(!WRITE_SCRIPT.contains("{}"), "no interpolation site: {WRITE_SCRIPT}");
        assert!(
            !WRITE_SCRIPT.contains("DML_SOAP_PASS"),
            "the script must never name the secret: {WRITE_SCRIPT}"
        );
        // `cat >` with no source operand IS the stdin read. An `echo`/`printf`
        // rewrite would put the contents on the command line, which is the one
        // thing this whole delivery mechanism exists to avoid.
        assert!(WRITE_SCRIPT.contains("cat > "), "{WRITE_SCRIPT}");
        assert!(!WRITE_SCRIPT.contains("echo "), "{WRITE_SCRIPT}");
        assert!(!WRITE_SCRIPT.contains("printf "), "{WRITE_SCRIPT}");
    }

    /// A CLI reading the file while it is being replaced must never see half of
    /// it: half a password authenticates as a different string, and the only
    /// symptom is a `SOAP_AUTH` with nothing pointing at the cause.
    #[test]
    fn the_write_is_atomic_and_private_from_birth() {
        assert!(WRITE_SCRIPT.contains("umask 077"), "{WRITE_SCRIPT}");
        assert!(WRITE_SCRIPT.contains("chmod 600"), "{WRITE_SCRIPT}");
        let tmp = WRITE_SCRIPT.find("soap.env.new").expect("writes to a temp name first");
        let mv = WRITE_SCRIPT.find("mv -f").expect("and then moves it into place");
        assert!(tmp < mv, "the temp file must be written before the move: {WRITE_SCRIPT}");
        // The invariant that actually holds: nothing writes the LIVE name
        // directly. Anchored on the refusal rather than on "x appears before y".
        assert!(
            !WRITE_SCRIPT.contains("cat > \"$HOME/.dml/soap.env\""),
            "the live file must never be the direct target of the write: {WRITE_SCRIPT}"
        );
    }

    #[test]
    fn the_probe_verb_is_read_only_and_is_a_classified_verb() {
        assert_eq!(PROBE_VERB, ["wow", "server-info"]);
        // If it stopped being classified, the Arch runner would fall back to the
        // bash CLI for it -- which still answers, but the probe would then be
        // measuring a route the translated verbs do not take.
        assert!(
            crate::vocab::is_classified(&PROBE_VERB),
            "the probe must travel the same road the translated verbs travel"
        );
        let t = crate::vocab::translate_for_arch(&PROBE_VERB);
        assert_eq!(t.target, crate::vocab::Target::DmlWow);
        assert_eq!(t.argv, vec!["server-info"]);
    }

    #[test]
    fn the_wire_words_are_distinct() {
        let all = [
            outcome_status(&SyncOutcome::NotApplicable),
            outcome_status(&SyncOutcome::NoCredentials),
            outcome_status(&SyncOutcome::AlreadyPublished),
            outcome_status(&SyncOutcome::Exhausted),
            outcome_status(&SyncOutcome::CliAlreadyWorks),
            outcome_status(&SyncOutcome::Unknown(String::new())),
            outcome_status(&SyncOutcome::Published),
            outcome_status(&SyncOutcome::Unproven(String::new())),
            outcome_status(&SyncOutcome::Failed(String::new())),
        ];
        let mut seen: Vec<&str> = Vec::new();
        for w in all {
            assert!(!seen.contains(&w), "duplicate wire word {w}");
            seen.push(w);
        }
    }
}
