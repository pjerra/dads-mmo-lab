//! The post-install step that makes every SOAP feature work — or honestly says
//! it does not.
//!
//! ## Why this exists
//!
//! A freshly installed AzerothCore has NO accounts. Until one exists with GM
//! level 3, every SOAP-backed feature in the launcher — GM Tools, My Party, the
//! console's send box, announcements, the server-info tiles — fails, and fails
//! with a `SOAP_AUTH` that names nothing the user did wrong. A user who skips
//! this step gets an app where half the buttons quietly do not work and no
//! screen explains why. That is the single worst outcome available at the end
//! of an otherwise successful multi-hour install.
//!
//! ## Why it is not automated
//!
//! Creating the account means either driving the worldserver console or writing
//! an SRP6 row into `acore_auth`. Both were investigated on 2026-08-01 and
//! neither is available today:
//!
//! * **The console cannot be scripted.** The worldserver runs with
//!   `stdin_open: true` AND `tty: true`, and `docker attach` REFUSES piped
//!   stdin against a TTY container outright — *"cannot attach stdin to a
//!   TTY-enabled container because stdin is not a terminal"*, verified against a
//!   live Docker Desktop. Dropping the tty does not rescue it either: `attach`
//!   then accepts the pipe but never returns, so it would need a bounded spawn
//!   plus a kill, and the console's behaviour without a tty is unproven.
//! * **The SRP6 write is a doctrine change.** This project's standing rule is
//!   that MySQL is read-only except for two sanctioned writes. Adding a third —
//!   hashing a password and INSERTing an account — is the user's call to make,
//!   not a detail to slip in. It is recorded as an open question, with the
//!   evidence above attached, rather than assumed either way.
//!
//! So the step is GUIDED. The user types two lines into their own server's
//! console.
//!
//! ## What this module actually guarantees
//!
//! **That "done" is earned.** The credentials are written to `~/.dml/soap.env`
//! ONLY after a real SOAP round-trip against the running server has succeeded
//! with them. Writing them first and hoping would reproduce exactly the failure
//! this step exists to prevent: an app that believes it is configured while
//! every SOAP button is dead.

use crate::soap;
use dml_core::error::CmdError;
use std::path::{Path, PathBuf};

/// The account the launcher uses for SOAP. Named for what it is so a server
/// operator reading their account list can tell it from a player.
pub const DEFAULT_SOAP_USER: &str = "dmlsoap";

/// GM level 3 — AzerothCore's administrator tier, and the minimum the SOAP
/// endpoint accepts for the commands the launcher issues.
pub const REQUIRED_GM_LEVEL: u8 = 3;

/// `-1` is AzerothCore's "every realm" wildcard for `account set gmlevel`.
/// Without it the level applies to realm 1 only, which is right until the day
/// someone adds a second realm and the launcher's tools silently stop working
/// on it.
pub const ALL_REALMS: &str = "-1";

/// The exact lines to type into the worldserver console, in order.
///
/// Returned as data rather than baked into a UI string so the same list drives
/// the on-screen instructions, the clipboard copy and the tests — three places
/// that must not drift, because a mistyped account is indistinguishable from a
/// broken SOAP setup from the outside.
pub fn console_commands(user: &str, pass: &str) -> Vec<String> {
    vec![
        format!("account create {user} {pass}"),
        format!("account set gmlevel {user} {REQUIRED_GM_LEVEL} {ALL_REALMS}"),
    ]
}

/// How to reach the console, given the compose project the install created.
///
/// Scoped through `docker compose -p`, never a bare `docker attach
/// ac-worldserver`: the container names are global to the engine, so a bare
/// name answers for whichever stack happens to own it — the same lesson the log
/// snapshot incident already taught this project.
pub fn attach_hint(project: &str) -> String {
    format!("docker compose -p {project} attach ac-worldserver")
}

/// The one thing a user must know before attaching, and the reason it is
/// carried as its own string rather than buried in a paragraph.
///
/// Ctrl-C in an attached console does not detach — it forwards the signal and
/// STOPS THE SERVER, taking the world down mid-session with players on it. The
/// detach sequence is Ctrl-P then Ctrl-Q.
pub const DETACH_WARNING: &str =
    "To leave the console press Ctrl-P then Ctrl-Q. Do NOT press Ctrl-C — that stops the server.";

/// Validate the pair before anything is typed, shown or written.
///
/// The rules are the CLI's, deliberately: an account this refuses is one
/// `dml wow account create` would refuse too, and letting the user create one
/// here that the rest of the app cannot address would be a trap with a
/// multi-hour install behind it.
pub fn validate(user: &str, pass: &str) -> Result<(), CmdError> {
    if !crate::soap_cmds::valid_account_user(user) {
        return Err(CmdError {
            code: "BAD_ARG".to_string(),
            message: format!("{user:?} is not a usable account name."),
            hint: "3-20 characters, letters, digits and underscore only.".to_string(),
        });
    }
    if !crate::soap_cmds::valid_account_pass(pass) {
        return Err(CmdError {
            code: "BAD_ARG".to_string(),
            message: "That password cannot be used for a WoW account.".to_string(),
            hint: "4-16 characters. Letters, digits and _ @ # % + = ! - only.".to_string(),
        });
    }
    Ok(())
}

/// `~/.dml/soap.env`'s contents for these credentials.
///
/// Every value is SINGLE-QUOTED, because this file is `.`-sourced by bash.
///
/// `_soap_load_env` in `cli/src/20-soap.sh` runs `. "$f"`, so an unquoted value
/// is shell SOURCE: `$(…)`, backticks, `$VAR` and globs all expand. Inside
/// single quotes bash expands nothing at all.
///
/// It was safe only by luck rather than by design. `user` and `pass` go through
/// [`validate`], which admits no shell metacharacter — but `url` has NO
/// validator, and this file travels INTO `dml-arch`, where `dml` holds NOPASSWD
/// sudo. Quoting costs two characters per line and does not depend on anyone
/// remembering which of the three fields is checked, or on `url` staying
/// non-user-supplied forever.
///
/// LF endings on purpose. The file is sourced by the bash CLI inside WSL, and
/// a CRLF written by a Windows-side helper leaves a trailing `\r` on the
/// password — an authentication failure whose only symptom is `SOAP_AUTH`, with
/// nothing on screen suggesting the file is the problem. The Rust reader strips
/// `\r` defensively for the same reason; this end simply never writes one.
pub fn soap_env_contents(url: &str, user: &str, pass: &str) -> String {
    format!(
        "DML_SOAP_URL={}\nDML_SOAP_USER={}\nDML_SOAP_PASS={}\n",
        sh_single_quote(url),
        sh_single_quote(user),
        sh_single_quote(pass)
    )
}

/// Wrap one value in single quotes for a file bash sources.
///
/// A literal `'` cannot appear inside single quotes, so the standard form
/// closes, escapes and reopens: `'\''`. [`crate::soap::parse_soap_env`] undoes
/// exactly this, so both ends agree on what a quoted value means — a writer
/// that escaped and a reader that did not would hand the world a password with
/// four stray characters in it and call it authentication failure.
fn sh_single_quote(v: &str) -> String {
    format!("'{}'", v.replace('\'', r"'\''"))
}

/// Where the credentials live, given a home directory.
pub fn soap_env_path_in(home: &Path) -> PathBuf {
    home.join(".dml").join("soap.env")
}

/// Write the credentials. Callers must only reach this AFTER a successful
/// round-trip — see [`bootstrap_verify_with`].
pub fn write_soap_env(home: &Path, url: &str, user: &str, pass: &str) -> Result<PathBuf, CmdError> {
    let path = soap_env_path_in(home);
    let dir = path.parent().unwrap_or(home);
    std::fs::create_dir_all(dir).map_err(|e| CmdError {
        code: "WRITE_FAILED".to_string(),
        message: format!("Could not create {}: {e}", dir.display()),
        hint: String::new(),
    })?;
    std::fs::write(&path, soap_env_contents(url, user, pass)).map_err(|e| CmdError {
        code: "WRITE_FAILED".to_string(),
        message: format!("Could not write {}: {e}", path.display()),
        hint: String::new(),
    })?;
    // Best-effort 0600. It carries a GM3 password, so on any platform that has
    // a mode this is worth doing; on Windows the ACL is inherited from the
    // user's profile directory and there is no equivalent to set.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(path)
}

/// What a verification attempt concluded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyOutcome {
    /// A real round-trip succeeded with these credentials.
    Ok,
    /// The server answered and REJECTED them. The account is missing, the
    /// password is wrong, or its gmlevel is below 3.
    Rejected(String),
    /// The server could not be reached at all. NOT a credentials verdict, and
    /// must never be reported as one — the usual cause is a world server that
    /// has not finished booting.
    Unreachable(String),
}

impl VerifyOutcome {
    pub fn is_ok(&self) -> bool {
        matches!(self, VerifyOutcome::Ok)
    }
}

/// Classify a SOAP result into the three outcomes above.
///
/// Pure, so every branch is testable without a server. The distinction between
/// `Rejected` and `Unreachable` is the whole point: telling a user their
/// password is wrong when the world server is merely still booting sends them
/// to delete and recreate a perfectly good account.
pub fn classify(outcome: &soap::SoapOutcome) -> VerifyOutcome {
    match outcome {
        soap::SoapOutcome::Ok(_) => VerifyOutcome::Ok,
        soap::SoapOutcome::Auth => VerifyOutcome::Rejected(
            "The server rejected those details. Check the account name and password, and that you ran the second command (gmlevel 3) as well as the first.".to_string(),
        ),
        // A fault means the server ANSWERED and authenticated us -- it just did
        // not like the command. For a bare `server info` that is bizarre, but
        // it is emphatically not an auth failure, and calling it one would send
        // the user to fix credentials that already work.
        soap::SoapOutcome::Fault(f) => VerifyOutcome::Rejected(format!(
            "The server answered but refused the check: {f}"
        )),
        soap::SoapOutcome::Unreachable(e) => VerifyOutcome::Unreachable(format!(
            "Could not reach the server's SOAP port ({e}). If the install has just finished, the world server may still be starting -- give it a minute and try again."
        )),
    }
}

/// The command used to prove the credentials work.
///
/// `server info` is chosen because it is READ-ONLY. A verification step that
/// mutated anything would be a poor trade: it runs against a server the user
/// may already care about, and a failed check must leave no trace.
pub const VERIFY_COMMAND: &str = "server info";

/// Verify, then persist — in that order, and only in that order.
///
/// `exec` is injected so the ordering rule is testable without a live server:
/// the tests assert that a REJECTED verification leaves no file behind, which
/// is the property that actually protects the user. Writing first and verifying
/// afterwards would leave a plausible-looking `soap.env` full of credentials
/// that do not work, which is precisely the "configured but every button is
/// dead" state this step exists to eliminate.
pub fn bootstrap_verify_with(
    home: &Path,
    url: &str,
    user: &str,
    pass: &str,
    exec: impl Fn(&soap::SoapConfig, &str) -> soap::SoapOutcome,
) -> Result<(VerifyOutcome, Option<PathBuf>), CmdError> {
    validate(user, pass)?;
    let cfg = soap::SoapConfig {
        url: url.to_string(),
        user: user.to_string(),
        pass: pass.to_string(),
    };
    let outcome = classify(&exec(&cfg, VERIFY_COMMAND));
    if !outcome.is_ok() {
        return Ok((outcome, None));
    }
    let path = write_soap_env(home, url, user, pass)?;
    Ok((outcome, Some(path)))
}

/// Is the launcher's SOAP access actually working RIGHT NOW?
///
/// Asked with the credentials the rest of the app would use
/// ([`soap::SoapConfig::load`]), so a "yes" here means every SOAP feature will
/// work — not that a file exists somewhere.
///
/// THE FILE'S EXISTENCE IS NOT THE QUESTION, and assuming it was would have
/// shipped a bug that is already sitting on the author's own machine: a
/// `~/.dml/soap.env` left over from a DIFFERENT server carries a real account
/// name and a real password for a realm that no longer exists. Every SOAP
/// feature fails, and a presence check reports everything configured.
///
/// The three outcomes carry their usual meanings, and the caller must respect
/// the difference: only [`VerifyOutcome::Rejected`] means "this needs setting
/// up". `Unreachable` means the server is not answering — which is also the
/// state in which the setup step is IMPOSSIBLE, because creating the account
/// needs a running worldserver console.
pub fn soap_status_with(
    exec: impl Fn(&soap::SoapConfig, &str) -> soap::SoapOutcome,
) -> VerifyOutcome {
    let cfg = soap::SoapConfig::load();
    classify(&exec(&cfg, VERIFY_COMMAND))
}

/// Should the guided account step be offered?
///
/// `true` ONLY for a server that is answering and refusing us. Deliberately not
/// for an unreachable one: the setup asks the user to type into the
/// worldserver's console, which does not exist while the server is down, so
/// raising it then would present a task that cannot be performed and would fire
/// on every machine with a stopped server.
pub fn needs_bootstrap(outcome: &VerifyOutcome) -> bool {
    matches!(outcome, VerifyOutcome::Rejected(_))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("dml-soapboot-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// The card must appear for a server that is UP and refusing us -- and for
    /// nothing else.
    #[test]
    fn the_step_is_offered_only_when_the_server_answers_and_refuses() {
        assert!(needs_bootstrap(&VerifyOutcome::Rejected("nope".into())));
        assert!(!needs_bootstrap(&VerifyOutcome::Ok));
        // NOT for an unreachable server. Creating the account means typing into
        // the worldserver console, which does not exist while it is down, so
        // offering it then presents a task that cannot be performed -- and
        // would fire on every machine with a stopped server.
        assert!(!needs_bootstrap(&VerifyOutcome::Unreachable("down".into())));
    }

    /// A leftover `soap.env` from a DIFFERENT server is the case a
    /// file-existence check gets wrong, and it is not hypothetical: exactly
    /// that file, carrying a real account for a realm that no longer exists,
    /// was sitting on the author's machine while every SOAP feature failed.
    #[test]
    fn stale_credentials_read_as_needing_setup_not_as_configured() {
        let got = soap_status_with(|_, _| soap::SoapOutcome::Auth);
        assert!(needs_bootstrap(&got), "{got:?}");
    }

    #[test]
    fn the_console_commands_are_both_there_and_in_order() {
        let c = console_commands("dmlsoap", "hunter2");
        assert_eq!(c.len(), 2, "creating the account without setting gmlevel leaves SOAP dead");
        assert!(c[0].starts_with("account create "), "{c:?}");
        assert!(c[1].contains("gmlevel"), "{c:?}");
        assert!(c[1].contains(" 3 "), "GM level 3 is what SOAP requires: {c:?}");
        assert!(c[1].ends_with("-1"), "the all-realms wildcard must be there: {c:?}");
    }

    #[test]
    fn the_attach_hint_is_scoped_to_the_compose_project() {
        // A bare `docker attach ac-worldserver` answers for whichever stack owns
        // that name -- the logsnap incident's lesson, in a command we hand
        // the user to paste.
        let h = attach_hint("dml-native-test-0205b295");
        assert!(h.contains("-p dml-native-test-0205b295"), "{h}");
    }

    #[test]
    fn the_detach_warning_names_the_keys_and_the_danger() {
        // Ctrl-C in an attached console STOPS THE SERVER. A user who learns
        // that by doing it has taken their world down with players on it.
        assert!(DETACH_WARNING.contains("Ctrl-P"));
        assert!(DETACH_WARNING.contains("Ctrl-C"));
    }

    #[test]
    fn bad_credentials_are_refused_before_anything_is_typed() {
        assert!(validate("ab", "hunter2").is_err(), "too short a name");
        assert!(validate("dmlsoap", "abc").is_err(), "too short a password");
        assert!(validate("dml soap", "hunter2").is_err(), "a space is not allowed");
        assert!(validate("dmlsoap", "hunter2").is_ok());
    }

    #[test]
    fn the_env_file_never_carries_a_carriage_return() {
        // A trailing \r on the password authenticates as a different string,
        // and the only symptom is SOAP_AUTH with nothing pointing at the file.
        let s = soap_env_contents("http://127.0.0.1:7878/", "dmlsoap", "hunter2");
        assert!(!s.contains('\r'), "{s:?}");
        assert!(s.ends_with('\n'));
    }

    /// THE ORDERING RULE, and the reason this module exists rather than a
    /// two-line command. A failed check must leave NOTHING behind: a plausible
    /// `soap.env` full of credentials that do not work is the exact
    /// "configured, but every SOAP button is dead" state the step exists to
    /// prevent, and it is harder to diagnose than having no file at all.
    #[test]
    fn a_rejected_verification_writes_no_credentials() {
        let home = tmp("rejected");
        let (outcome, path) = bootstrap_verify_with(
            &home,
            "http://127.0.0.1:7878/",
            "dmlsoap",
            "hunter2",
            |_, _| soap::SoapOutcome::Auth,
        )
        .unwrap();
        assert!(matches!(outcome, VerifyOutcome::Rejected(_)));
        assert!(path.is_none());
        assert!(!soap_env_path_in(&home).exists(), "no file may be left behind");
    }

    #[test]
    fn an_unreachable_server_writes_no_credentials_either() {
        let home = tmp("unreachable");
        let (outcome, path) = bootstrap_verify_with(
            &home,
            "http://127.0.0.1:7878/",
            "dmlsoap",
            "hunter2",
            |_, _| soap::SoapOutcome::Unreachable("connection refused".to_string()),
        )
        .unwrap();
        assert!(matches!(outcome, VerifyOutcome::Unreachable(_)));
        assert!(path.is_none());
        assert!(!soap_env_path_in(&home).exists());
    }

    #[test]
    fn a_successful_round_trip_persists_the_credentials() {
        let home = tmp("ok");
        let (outcome, path) = bootstrap_verify_with(
            &home,
            "http://127.0.0.1:7878/",
            "dmlsoap",
            "hunter2",
            |cfg, cmd| {
                // The injected exec sees the REAL credentials and the real
                // command -- a stub that ignored them would let a regression
                // that verifies with the wrong account pass unnoticed.
                assert_eq!(cfg.user, "dmlsoap");
                assert_eq!(cfg.pass, "hunter2");
                assert_eq!(cmd, VERIFY_COMMAND);
                soap::SoapOutcome::Ok("AzerothCore rev. abc".to_string())
            },
        )
        .unwrap();
        assert!(outcome.is_ok());
        let p = path.expect("a verified setup must persist");
        let body = std::fs::read_to_string(&p).unwrap();
        assert!(body.contains("DML_SOAP_USER='dmlsoap'"), "{body}");
        assert!(body.contains("DML_SOAP_PASS='hunter2'"), "{body}");
    }

    /// THE FILE IS SHELL SOURCE, so every value is single-quoted.
    ///
    /// `_soap_load_env` runs `. "$HOME/.dml/soap.env"`. Unquoted, a value is
    /// code: `$(…)`, backticks, `$VAR` and globs all expand — inside a distro
    /// where `dml` holds NOPASSWD sudo. Not reachable today (`user`/`pass` go
    /// through `validate`, and `url` is ours), but `url` has NO validator, and
    /// "safe because of what someone else checks" is not a property of this
    /// function.
    #[test]
    fn every_value_is_single_quoted_because_bash_sources_this_file() {
        let s = soap_env_contents("http://127.0.0.1:7878/", "dmlsoap", "hunter2");
        for line in s.lines() {
            let (_, v) = line.split_once('=').unwrap_or_else(|| panic!("not KEY=VALUE: {line:?}"));
            assert!(
                v.starts_with('\'') && v.ends_with('\''),
                "unquoted value in a file bash sources: {line:?}"
            );
        }
    }

    /// The values that WOULD have been code, neutralised — asserted as the
    /// exact text, because that is the whole of the fix.
    #[test]
    fn a_metacharacter_in_the_url_is_data_rather_than_a_command() {
        let s = soap_env_contents("http://h/$(id>/tmp/pwned)`whoami`", "dmlsoap", "hunter2");
        assert!(
            s.contains("DML_SOAP_URL='http://h/$(id>/tmp/pwned)`whoami`'"),
            "the expansion must sit inside single quotes verbatim: {s}"
        );
    }

    /// A literal `'` closes the quoting, so it is escaped — and the READER
    /// undoes exactly this escape. A writer that escaped against a reader that
    /// did not would deliver a password with four extra characters in it and
    /// call the result an authentication failure.
    #[test]
    fn a_quote_in_a_value_survives_the_round_trip_to_the_reader() {
        let url = "http://h/it's";
        let s = soap_env_contents(url, "dmlsoap", "hunter2");
        assert!(s.contains(r"DML_SOAP_URL='http://h/it'\''s'"), "{s}");
        let (got, user, pass) = crate::soap::parse_soap_env(&s);
        assert_eq!(got.as_deref(), Some(url), "the reader must undo the escape");
        assert_eq!(user.as_deref(), Some("dmlsoap"));
        assert_eq!(pass.as_deref(), Some("hunter2"));
    }

    /// A server that answers and refuses the COMMAND has still authenticated
    /// us. Reporting that as bad credentials sends the user to delete and
    /// recreate an account that already works.
    #[test]
    fn a_fault_is_not_an_auth_failure_and_neither_is_unreachable() {
        let fault = classify(&soap::SoapOutcome::Fault("no such command".to_string()));
        match fault {
            VerifyOutcome::Rejected(m) => assert!(m.contains("refused the check"), "{m}"),
            other => panic!("{other:?}"),
        }
        let down = classify(&soap::SoapOutcome::Unreachable("refused".to_string()));
        match down {
            // The world server still booting is the common case, and the copy
            // must say so rather than blaming the password.
            VerifyOutcome::Unreachable(m) => {
                assert!(m.contains("starting"), "must name the likely cause: {m}");
                assert!(
                    !m.to_lowercase().contains("password"),
                    "an unreachable server says nothing about the password: {m}"
                );
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_verification_command_is_read_only() {
        // It runs against a server the user may already care about, and a
        // failed check must leave no trace.
        assert_eq!(VERIFY_COMMAND, "server info");
    }
}
