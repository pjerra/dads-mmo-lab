//! The launcher sets up its own SOAP access, without asking anyone to invent a
//! password.
//!
//! `soap_bootstrap` removed the worldserver console; `account_write` and `srp6`
//! made the account row writable directly. What was left was still three manual
//! acts at the end of a multi-hour install: find the card (it lived on ONE
//! page), invent a password, click a button. This module removes all three.
//!
//! Everything here is pure or seam-injected, so the whole decision tree is
//! testable with no server and no database. The impure bindings live in the
//! launcher's `wow_soap_autosetup` command.

use crate::soap_bootstrap::{VerifyOutcome, DEFAULT_SOAP_USER};
use dml_core::error::CmdError;

/// AzerothCore's ceiling, not a preference. `soap_cmds::valid_account_pass`
/// enforces `{4,16}` and `account_write::create_gm_account` runs it before it
/// writes anything, so a "stronger" 32-character password would be refused on
/// every fresh install -- at the one moment the user has nothing to retype.
pub const PASSWORD_LEN: usize = 16;

/// Exactly `valid_account_pass`'s charset: 26 + 26 + 10 + 8 = 70 symbols.
/// ~98 bits over 16 characters, which is far past anything that matters here.
pub const PASSWORD_ALPHABET: &[u8] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_@#%+=!-";

/// Generate a password from a caller-supplied byte source.
///
/// **Rejection sampling, not `byte % 70`.** 256 is not a multiple of 70
/// (`256 = 3x70 + 46`), so plain modulo hands the first 46 symbols a fourth
/// chance the remaining 24 never get. The bias is invisible in any output a
/// human would look at, which is exactly why it has to be handled here rather
/// than noticed later.
///
/// `fill` is a seam so the discard rule can be proven with a scripted byte
/// stream; production passes `getrandom`.
pub fn generate_password_from(mut fill: impl FnMut(&mut [u8])) -> String {
    let n = PASSWORD_ALPHABET.len();
    // The largest multiple of n that fits in a byte: 3 * 70 = 210. Anything at
    // or above it is thrown away rather than folded.
    let limit = (256 / n) * n;
    let mut out = String::with_capacity(PASSWORD_LEN);
    let mut buf = [0u8; 64];
    let mut i = buf.len();
    while out.len() < PASSWORD_LEN {
        if i == buf.len() {
            fill(&mut buf);
            i = 0;
        }
        let b = buf[i] as usize;
        i += 1;
        if b < limit {
            out.push(PASSWORD_ALPHABET[b % n] as char);
        }
    }
    out
}

/// A fresh random password.
///
/// `getrandom` for the same reason `srp6::random_salt` uses it: this is a
/// credential for a GM-level-3 account on a server whose auth port is
/// published, and a predictable one would be predictable on every DML install
/// at once.
pub fn generate_password() -> String {
    generate_password_from(|buf| {
        getrandom::fill(buf).expect("the OS random source is unavailable")
    })
}

/// Six lowercase hex digits, for the collision-fallback account name.
pub fn random_hex6() -> String {
    let mut b = [0u8; 3];
    getrandom::fill(&mut b).expect("the OS random source is unavailable");
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// The name to try when `dmlsoap` is taken: `dmlsoap_<6 hex>`.
///
/// 14 characters, inside `valid_account_user`'s 20. Chosen over resetting the
/// existing account's password because this code has no business touching a row
/// it did not write -- `create_gm_account` refuses to, and this is how that
/// refusal stays survivable instead of becoming a dead end.
pub fn fallback_user(hex6: &str) -> String {
    format!("{DEFAULT_SOAP_USER}_{hex6}")
}

/// How many verification attempts a created-but-unverified account gets before
/// the manual card takes over.
///
/// Verify can fail after a successful create for one interesting reason (the
/// SRP6 produced a well-formed verifier the server rejects) and one boring one
/// (the world server went away between the two calls). Three tries tells them
/// apart without spinning.
pub const MAX_VERIFY_TRIES: u8 = 3;

/// How this launcher run ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Conclusion {
    Saved { user: String },
    GaveUp { reason: String },
}

/// Where this launcher run is.
///
/// **`Pending` is why this is a state machine and not a function.** A create
/// that succeeded followed by a verify that failed must not leave the latch
/// open: the next poll would create a SECOND account, and the one after that a
/// third -- one row per tick into the user's auth database, forever. `Pending`
/// carries the credential forward and says re-verify, never re-create.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoSetup {
    Idle,
    Pending { user: String, pass: String, tries: u8 },
    Done(Conclusion),
}

/// What the caller is told. Serialized to the frontend by [`outcome_status`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoOutcome {
    /// SOAP works, or the server is not answering. Nothing was read or written.
    NotNeeded,
    /// The account exists and has been proven over real SOAP.
    Created { user: String },
    /// The account exists; the proof has not landed yet. Will retry.
    Pending,
    /// This run will not try again. The manual card takes over.
    GaveUp { reason: String },
    /// Already concluded this run. No seam was touched.
    Latched,
}

/// The wire word for an outcome. The frontend switches on these literals.
pub fn outcome_status(o: &AutoOutcome) -> &'static str {
    match o {
        AutoOutcome::NotNeeded => "not_needed",
        AutoOutcome::Created { .. } => "created",
        AutoOutcome::Pending => "pending",
        AutoOutcome::GaveUp { .. } => "gave_up",
        AutoOutcome::Latched => "latched",
    }
}

fn gave_up(reason: String) -> (AutoSetup, AutoOutcome) {
    (
        AutoSetup::Done(Conclusion::GaveUp { reason: reason.clone() }),
        AutoOutcome::GaveUp { reason },
    )
}

/// One tick of the machine.
///
/// Every effect is a seam, so the whole tree is exercised with no server and no
/// database -- including the negative that matters most: a healthy server must
/// not cause a single DB read.
///
/// * `exists` — `account_write::account_exists`
/// * `create` — `account_write::create_gm_account`
/// * `verify` — `soap_bootstrap::bootstrap_verify_with`, which is also what
///   WRITES `~/.dml/soap.env`, and only after a real round-trip succeeds. That
///   ordering is not restated here; reusing it is what keeps one definition of
///   "done" instead of two that can disagree.
pub fn advance_with(
    state: AutoSetup,
    status: &VerifyOutcome,
    exists: impl Fn(&str) -> Result<bool, CmdError>,
    create: impl Fn(&str, &str) -> Result<i64, CmdError>,
    verify: impl Fn(&str, &str) -> Result<VerifyOutcome, CmdError>,
    hex6: impl Fn() -> String,
    gen_pass: impl Fn() -> String,
) -> (AutoSetup, AutoOutcome) {
    if matches!(state, AutoSetup::Done(_)) {
        return (state, AutoOutcome::Latched);
    }
    // ONLY for a server that answers and refuses us. `Ok` means there is
    // nothing to do; `Unreachable` means we know nothing at all, and a booting
    // world server must never be read as a broken account.
    if !matches!(status, VerifyOutcome::Rejected(_)) {
        return (state, AutoOutcome::NotNeeded);
    }

    match state {
        AutoSetup::Done(_) => unreachable!("handled above"),

        AutoSetup::Pending { user, pass, tries } => match verify(&user, &pass) {
            Ok(VerifyOutcome::Ok) => (
                AutoSetup::Done(Conclusion::Saved { user: user.clone() }),
                AutoOutcome::Created { user },
            ),
            other => {
                let tries = tries + 1;
                if tries >= MAX_VERIFY_TRIES {
                    let why = match other {
                        Ok(VerifyOutcome::Rejected(m)) | Ok(VerifyOutcome::Unreachable(m)) => m,
                        Err(e) => e.message,
                        Ok(VerifyOutcome::Ok) => unreachable!("handled above"),
                    };
                    gave_up(format!(
                        "Created the account {user:?} but could not prove it works: {why}"
                    ))
                } else {
                    (AutoSetup::Pending { user, pass, tries }, AutoOutcome::Pending)
                }
            }
        },

        AutoSetup::Idle => {
            let user = match exists(DEFAULT_SOAP_USER) {
                Ok(false) => DEFAULT_SOAP_USER.to_string(),
                Ok(true) => {
                    // Taken. Work around it -- never over it. Resetting an
                    // account this code did not create is a different and more
                    // dangerous operation, and `create_gm_account` refuses to.
                    let alt = fallback_user(&hex6());
                    match exists(&alt) {
                        Ok(false) => alt,
                        Ok(true) => {
                            return gave_up(format!(
                                "Both {DEFAULT_SOAP_USER:?} and {alt:?} already exist."
                            ))
                        }
                        Err(e) => return gave_up(e.message),
                    }
                }
                Err(e) => return gave_up(e.message),
            };

            let pass = gen_pass();
            if let Err(e) = create(&user, &pass) {
                // The server's own words. "Setup failed" is not actionable;
                // "your auth database may use a schema this build does not
                // know" is.
                let reason = if e.hint.is_empty() {
                    e.message
                } else {
                    format!("{} {}", e.message, e.hint)
                };
                return gave_up(reason);
            }

            match verify(&user, &pass) {
                Ok(VerifyOutcome::Ok) => (
                    AutoSetup::Done(Conclusion::Saved { user: user.clone() }),
                    AutoOutcome::Created { user },
                ),
                // Created but unproven. Carry the credential forward; the next
                // tick re-verifies it and does NOT create anything.
                _ => (AutoSetup::Pending { user, pass, tries: 1 }, AutoOutcome::Pending),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soap_cmds::{valid_account_pass, valid_account_user};

    #[test]
    fn the_alphabet_is_exactly_what_the_validator_accepts() {
        // Byte-for-byte, because a symbol the validator refuses would produce a
        // BAD_ARG on a fresh install -- the one place a user cannot retype it.
        assert_eq!(PASSWORD_ALPHABET.len(), 70, "26 + 26 + 10 + 8");
        for b in PASSWORD_ALPHABET {
            let s = (*b as char).to_string().repeat(4);
            assert!(valid_account_pass(&s), "alphabet leaks {:?}", *b as char);
        }
        // Non-vacuity: a character the validator rejects must actually fail the
        // check above, or this test proves nothing.
        assert!(!valid_account_pass("$$$$"));
    }

    #[test]
    fn every_generated_password_is_one_the_server_will_take() {
        for _ in 0..1000 {
            let p = generate_password();
            assert_eq!(p.len(), PASSWORD_LEN);
            assert!(valid_account_pass(&p), "generated an unusable password: {p:?}");
        }
    }

    #[test]
    fn generated_passwords_are_not_repeated() {
        // A constant password across installs is the failure mode we rejected
        // when we chose not to ship a fixed default.
        let mut seen = std::collections::HashSet::new();
        for _ in 0..1000 {
            assert!(seen.insert(generate_password()), "a password repeated in 1000 draws");
        }
    }

    #[test]
    fn bytes_at_or_above_the_rejection_limit_are_discarded() {
        // 256 = 3*70 + 46. Plain `byte % 70` would fold 210..=255 back onto the
        // first 46 symbols and give them a fourth chance the other 24 never get.
        // Feed exactly those rejects followed by 0..16 and assert NONE of the
        // rejects reached the output: the password must be the first 16 symbols
        // of the alphabet, in order.
        let feed: Vec<u8> = (210u8..=255).chain(0..16).collect();
        let mut k = 0usize;
        let pw = generate_password_from(|buf| {
            for slot in buf.iter_mut() {
                *slot = feed[k % feed.len()];
                k += 1;
            }
        });
        let want: String = PASSWORD_ALPHABET[..PASSWORD_LEN].iter().map(|b| *b as char).collect();
        assert_eq!(pw, want, "a rejected byte reached the password");
    }

    #[test]
    fn the_fallback_name_is_one_the_server_will_take() {
        let u = fallback_user("ab12ef");
        assert_eq!(u, "dmlsoap_ab12ef");
        assert!(u.len() <= 20, "valid_account_user caps at 20: {u}");
        assert!(valid_account_user(&u), "{u}");
    }

    #[test]
    fn the_hex_is_six_lowercase_hex_digits_and_varies() {
        let a = random_hex6();
        assert_eq!(a.len(), 6);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()), "{a}");
        // 24 bits: a repeat in 50 draws would mean the RNG is not running.
        let mut seen = std::collections::HashSet::new();
        for _ in 0..50 {
            seen.insert(random_hex6());
        }
        assert!(seen.len() > 40, "hex barely varies: {} distinct in 50", seen.len());
    }

    // -- the state machine --------------------------------------------------

    use std::cell::RefCell;

    /// A scripted set of seams. Counters are what the assertions read: the
    /// property that matters most is a NEGATIVE one (create was not called),
    /// and a stub that only returned values could not express it.
    #[derive(Default)]
    struct Seams {
        existing: Vec<String>,
        creates: RefCell<Vec<(String, String)>>,
        verifies: RefCell<Vec<(String, String)>>,
        verify_results: RefCell<Vec<VerifyOutcome>>,
        create_fails: bool,
    }

    impl Seams {
        fn run(&self, state: AutoSetup, status: &VerifyOutcome) -> (AutoSetup, AutoOutcome) {
            advance_with(
                state,
                status,
                |u| Ok(self.existing.iter().any(|e| e.eq_ignore_ascii_case(u))),
                |u, p| {
                    self.creates.borrow_mut().push((u.to_string(), p.to_string()));
                    if self.create_fails {
                        Err(CmdError {
                            code: "ACCOUNT_WRITE_FAILED".into(),
                            message: "schema".into(),
                            hint: "by hand".into(),
                        })
                    } else {
                        Ok(1)
                    }
                },
                |u, p| {
                    self.verifies.borrow_mut().push((u.to_string(), p.to_string()));
                    let mut q = self.verify_results.borrow_mut();
                    Ok(if q.is_empty() { VerifyOutcome::Ok } else { q.remove(0) })
                },
                || "ab12ef".to_string(),
                || "Generated_1234".to_string(),
            )
        }
    }

    fn rejected() -> VerifyOutcome {
        VerifyOutcome::Rejected("no".into())
    }

    #[test]
    fn a_working_soap_writes_nothing_at_all() {
        // The single most important negative: an app pointed at a healthy
        // server must not open a DB connection, let alone insert into it.
        let s = Seams::default();
        let (state, out) = s.run(AutoSetup::Idle, &VerifyOutcome::Ok);
        assert!(matches!(out, AutoOutcome::NotNeeded));
        assert!(matches!(state, AutoSetup::Idle), "the latch must stay open");
        assert!(s.creates.borrow().is_empty(), "created an account on a healthy server");
        assert!(s.verifies.borrow().is_empty());
    }

    #[test]
    fn an_unreachable_server_writes_nothing_either() {
        // A world server that has not finished booting is not a broken account,
        // and treating it as one is how a user gets sent to fix credentials
        // that already work.
        let s = Seams::default();
        let (_, out) = s.run(AutoSetup::Idle, &VerifyOutcome::Unreachable("refused".into()));
        assert!(matches!(out, AutoOutcome::NotNeeded));
        assert!(s.creates.borrow().is_empty());
    }

    #[test]
    fn the_happy_path_creates_dmlsoap_and_latches() {
        let s = Seams::default();
        let (state, out) = s.run(AutoSetup::Idle, &rejected());
        match out {
            AutoOutcome::Created { user } => assert_eq!(user, "dmlsoap"),
            other => panic!("{other:?}"),
        }
        assert!(matches!(state, AutoSetup::Done(Conclusion::Saved { .. })));
        assert_eq!(s.creates.borrow().len(), 1);
        assert_eq!(s.creates.borrow()[0], ("dmlsoap".into(), "Generated_1234".into()));
        // The generated password must reach BOTH calls unchanged. A verify run
        // against a different string would pass here and fail on a real server.
        assert_eq!(s.verifies.borrow()[0], ("dmlsoap".into(), "Generated_1234".into()));
    }

    #[test]
    fn a_taken_name_is_worked_around_never_overwritten() {
        let s = Seams { existing: vec!["DMLSOAP".into()], ..Default::default() };
        let (_, out) = s.run(AutoSetup::Idle, &rejected());
        match out {
            AutoOutcome::Created { user } => assert_eq!(user, "dmlsoap_ab12ef"),
            other => panic!("{other:?}"),
        }
        // The existing account is never a create target -- that is the whole
        // guarantee. (Comparison is case-insensitive because AzerothCore stores
        // names uppercased.)
        assert!(
            !s.creates.borrow().iter().any(|(u, _)| u.eq_ignore_ascii_case("dmlsoap")),
            "tried to create over an existing account"
        );
    }

    #[test]
    fn both_names_taken_gives_up_rather_than_guessing_forever() {
        let s = Seams {
            existing: vec!["DMLSOAP".into(), "DMLSOAP_AB12EF".into()],
            ..Default::default()
        };
        let (state, out) = s.run(AutoSetup::Idle, &rejected());
        assert!(matches!(out, AutoOutcome::GaveUp { .. }));
        assert!(matches!(state, AutoSetup::Done(Conclusion::GaveUp { .. })));
        assert!(s.creates.borrow().is_empty());
    }

    #[test]
    fn a_failed_insert_gives_up_and_carries_the_reason() {
        let s = Seams { create_fails: true, ..Default::default() };
        let (state, out) = s.run(AutoSetup::Idle, &rejected());
        match out {
            // The user needs the server's own words here: "schema this build
            // does not know" is actionable, "setup failed" is not.
            AutoOutcome::GaveUp { reason } => assert!(reason.contains("schema"), "{reason}"),
            other => panic!("{other:?}"),
        }
        assert!(matches!(state, AutoSetup::Done(Conclusion::GaveUp { .. })));
    }

    /// THE TEST THIS STATE MACHINE EXISTS FOR.
    ///
    /// Create succeeded, verify did not. If that left the latch open, the next
    /// poll would create a SECOND account -- one row per poll tick, forever,
    /// into the user's auth database.
    #[test]
    fn a_created_account_whose_verify_failed_is_re_verified_never_re_created() {
        let s = Seams {
            verify_results: RefCell::new(vec![
                VerifyOutcome::Unreachable("gone".into()),
                VerifyOutcome::Ok,
            ]),
            ..Default::default()
        };
        let (state, out) = s.run(AutoSetup::Idle, &rejected());
        assert!(matches!(out, AutoOutcome::Pending));
        match &state {
            AutoSetup::Pending { user, pass, tries } => {
                assert_eq!(user, "dmlsoap");
                assert_eq!(pass, "Generated_1234");
                assert_eq!(*tries, 1, "the failure that just happened counts");
            }
            other => panic!("{other:?}"),
        }

        let (state, out) = s.run(state, &rejected());
        match out {
            AutoOutcome::Created { user } => assert_eq!(user, "dmlsoap"),
            other => panic!("{other:?}"),
        }
        assert!(matches!(state, AutoSetup::Done(Conclusion::Saved { .. })));
        assert_eq!(s.creates.borrow().len(), 1, "a second account was created");
        assert_eq!(s.verifies.borrow().len(), 2);
    }

    #[test]
    fn three_failed_verifies_give_up_and_stop() {
        let s = Seams {
            verify_results: RefCell::new(vec![
                VerifyOutcome::Unreachable("1".into()),
                VerifyOutcome::Unreachable("2".into()),
                VerifyOutcome::Unreachable("3".into()),
            ]),
            ..Default::default()
        };
        let (state, _) = s.run(AutoSetup::Idle, &rejected());
        let (state, out) = s.run(state, &rejected());
        assert!(matches!(out, AutoOutcome::Pending), "{out:?}");
        let (state, out) = s.run(state, &rejected());
        assert!(matches!(out, AutoOutcome::GaveUp { .. }), "{out:?}");
        assert_eq!(s.verifies.borrow().len(), MAX_VERIFY_TRIES as usize);

        // And it STOPS. A give-up that kept being retried is the loop this
        // whole design refuses to ship.
        let (_, out) = s.run(state, &rejected());
        assert!(matches!(out, AutoOutcome::Latched));
        assert_eq!(s.verifies.borrow().len(), MAX_VERIFY_TRIES as usize);
    }

    #[test]
    fn a_finished_run_touches_no_seam_ever_again() {
        let s = Seams::default();
        let state = AutoSetup::Done(Conclusion::Saved { user: "dmlsoap".into() });
        let (state, out) = s.run(state, &rejected());
        assert!(matches!(out, AutoOutcome::Latched));
        assert!(matches!(state, AutoSetup::Done(_)));
        assert!(s.creates.borrow().is_empty());
        assert!(s.verifies.borrow().is_empty());
    }

    #[test]
    fn the_status_strings_are_the_ones_the_frontend_switches_on() {
        // The TypeScript switch matches these literals. A rename on one side
        // only would silently fall through to "do nothing".
        assert_eq!(outcome_status(&AutoOutcome::NotNeeded), "not_needed");
        assert_eq!(outcome_status(&AutoOutcome::Created { user: "x".into() }), "created");
        assert_eq!(outcome_status(&AutoOutcome::Pending), "pending");
        assert_eq!(outcome_status(&AutoOutcome::GaveUp { reason: "x".into() }), "gave_up");
        assert_eq!(outcome_status(&AutoOutcome::Latched), "latched");
    }
}
