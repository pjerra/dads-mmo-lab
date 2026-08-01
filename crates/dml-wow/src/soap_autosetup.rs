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

/// The prefix every fallback name shares: `dmlsoap_`.
///
/// The unit the `family_taken` seam asks about. It deliberately does NOT cover
/// `dmlsoap` itself: `dmlsoap` being taken is the whole reason a fallback is
/// being considered, so a pattern that also matched it would refuse every
/// install that already has one.
pub fn fallback_prefix() -> String {
    format!("{DEFAULT_SOAP_USER}_")
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
}

/// The wire word for an outcome. The frontend switches on these literals.
pub fn outcome_status(o: &AutoOutcome) -> &'static str {
    match o {
        AutoOutcome::NotNeeded => "not_needed",
        AutoOutcome::Created { .. } => "created",
        AutoOutcome::Pending => "pending",
        AutoOutcome::GaveUp { .. } => "gave_up",
    }
}

/// What a run that has already concluded reports — re-derived from what it
/// concluded, every time it is asked.
///
/// **There is no contentless "already done" outcome, and that is deliberate.**
/// One used to exist. It cost the manual fallback card: the frontend's record
/// of how setup went lives in a module-level store, a webview reload wipes it,
/// and a reloaded UI that asks again and hears only "already concluded" learns
/// nothing — so the card never rendered again for the rest of the process, on
/// the exact path where the launcher had FAILED to make the account and the
/// card was the only thing left that worked.
///
/// Both "already concluded" guards — the launcher command's cheap exit and
/// [`advance_with`]'s own — answer through here, so they cannot drift into
/// telling the frontend two different things about the same run.
pub fn concluded_outcome(c: &Conclusion) -> AutoOutcome {
    match c {
        Conclusion::Saved { user } => AutoOutcome::Created { user: user.clone() },
        Conclusion::GaveUp { reason } => AutoOutcome::GaveUp { reason: reason.clone() },
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
/// * `family_taken` — `account_write::account_family_exists`
/// * `create` — `account_write::create_gm_account`
/// * `verify` — `soap_bootstrap::bootstrap_verify_with`, which is also what
///   WRITES `~/.dml/soap.env`, and only after a real round-trip succeeds. That
///   ordering is not restated here; reusing it is what keeps one definition of
///   "done" instead of two that can disagree.
pub fn advance_with(
    state: AutoSetup,
    status: &VerifyOutcome,
    exists: impl Fn(&str) -> Result<bool, CmdError>,
    family_taken: impl Fn(&str) -> Result<bool, CmdError>,
    create: impl Fn(&str, &str) -> Result<i64, CmdError>,
    verify: impl Fn(&str, &str) -> Result<VerifyOutcome, CmdError>,
    hex6: impl Fn() -> String,
    gen_pass: impl Fn() -> String,
) -> (AutoSetup, AutoOutcome) {
    if let AutoSetup::Done(c) = &state {
        let out = concluded_outcome(c);
        return (state, out);
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
                    //
                    // Ask about the FAMILY before minting, because asking about
                    // the minted name bounds nothing: `dmlsoap_<random hex>` is
                    // free BY CONSTRUCTION, so that check can only ever answer
                    // "free". Something has to bound it, because "created"
                    // does not always stick -- `SoapConfig::load` gives
                    // `DML_SOAP_USER`/`DML_SOAP_PASS` precedence over the
                    // `~/.dml/soap.env` this run writes, so with those exported
                    // the status stays `Rejected` and EVERY launcher start
                    // would insert one more GM-level-3 account, forever.
                    match family_taken(&fallback_prefix()) {
                        Ok(true) => {
                            return gave_up(format!(
                                "{DEFAULT_SOAP_USER:?} is taken and a {}* account already \
                                 exists, so no further account was created. Use that \
                                 account's password, or make a GM account by hand.",
                                fallback_prefix()
                            ))
                        }
                        Ok(false) => {}
                        Err(e) => return gave_up(e.message),
                    }
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

/// `(user, url, pass, configured)` for the credentials panel.
///
/// The password is opt-in at this boundary rather than filtered in the UI. A
/// secret that always crosses the IPC boundary is a secret in every devtools
/// trace, whether or not a control is showing it.
///
/// `configured` is carried through UNCHANGED from
/// [`crate::soap::SoapConfig::load_with_provenance`], because that resolver is
/// the only thing that knows whether these values came from `DML_SOAP_*` /
/// `~/.dml/soap.env` or from the compiled-in `admin`/`admin`. The panel used to
/// decide by comparing the strings against `"admin"`, which reads a real account
/// named `admin` as "nothing set up" and a fresh install as an account that
/// exists. Deriving it again here would just rebuild that same guess one layer
/// closer to the UI.
pub fn credentials_payload(
    user: &str,
    pass: &str,
    url: &str,
    configured: bool,
    reveal: bool,
) -> (String, String, Option<String>, bool) {
    (
        user.to_string(),
        url.to_string(),
        if reveal { Some(pass.to_string()) } else { None },
        configured,
    )
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
        // Feed rejects followed by 0..16 and assert NONE of the rejects reached
        // the output: the password must be the first 16 symbols of the
        // alphabet, in order.
        //
        // THE FEED IS NOT ARBITRARY, do not "simplify" it back to `210..=255`.
        // That range ALIASES onto the answer: 210 == 3*70, so 210..=225 under
        // plain modulo map to indices 0..=15 -- character for character the
        // prefix this test expects. With that feed the assertion passes whether
        // or not the discard rule exists, and the one test whose stated purpose
        // is proving rejection sampling cannot tell it from `byte % 70`
        // (measured: deleting the `if b < limit` guard left it green).
        // 255 % 70 == 45 instead, so an unguarded run starts with 46 copies of
        // PASSWORD_ALPHABET[45] and this test fails loudly.
        let feed: Vec<u8> = std::iter::repeat(255u8).take(46).chain(0..16).collect();
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

    /// What a lookup against a database that is simply not there comes back as.
    /// Wrong `DML_DB_*` credentials or a stopped MySQL container both land
    /// here, and the launcher polls this feature WHILE the stack is coming up,
    /// so it is the ordinary case rather than an exotic one.
    const DB_DOWN: &str = "Could not reach the database: Connection refused (os error 111).";

    fn db_down() -> CmdError {
        CmdError { code: "DB_ERROR".into(), message: DB_DOWN.into(), hint: String::new() }
    }

    /// A scripted set of seams. Counters are what the assertions read: the
    /// property that matters most is a NEGATIVE one (create was not called),
    /// and a stub that only returned values could not express it.
    ///
    /// Every lookup can also FAIL rather than answer. A seam that can only
    /// succeed leaves the "the DB did not answer" arms with zero coverage,
    /// which is how they stayed unreachable-in-tests while being the arms a
    /// real user hits first on a server that is still starting.
    #[derive(Default)]
    struct Seams {
        existing: Vec<String>,
        creates: RefCell<Vec<(String, String)>>,
        verifies: RefCell<Vec<(String, String)>>,
        verify_results: RefCell<Vec<VerifyOutcome>>,
        /// `(message, hint)` of a create that fails. BOTH halves, because the
        /// hint is the actionable one and dropping it is invisible in a
        /// `contains` assertion.
        create_error: Option<(String, String)>,
        /// Names whose existence lookup errors instead of answering.
        exists_fails_for: Vec<String>,
        /// Whether the family lookup errors instead of answering.
        family_fails: bool,
    }

    impl Seams {
        fn run(&self, state: AutoSetup, status: &VerifyOutcome) -> (AutoSetup, AutoOutcome) {
            advance_with(
                state,
                status,
                |u| {
                    if self.exists_fails_for.iter().any(|e| e.eq_ignore_ascii_case(u)) {
                        return Err(db_down());
                    }
                    Ok(self.existing.iter().any(|e| e.eq_ignore_ascii_case(u)))
                },
                |p| {
                    if self.family_fails {
                        return Err(db_down());
                    }
                    // The real seam is a `LIKE 'PREFIX%'`; upper-cased here for
                    // the same reason it is there -- AzerothCore stores names
                    // upper-cased, so a case-sensitive scan would report a
                    // family free that is not.
                    let p = p.to_ascii_uppercase();
                    Ok(self.existing.iter().any(|e| e.to_ascii_uppercase().starts_with(&p)))
                },
                |u, p| {
                    self.creates.borrow_mut().push((u.to_string(), p.to_string()));
                    match &self.create_error {
                        Some((message, hint)) => Err(CmdError {
                            code: "ACCOUNT_WRITE_FAILED".into(),
                            message: message.clone(),
                            hint: hint.clone(),
                        }),
                        None => Ok(1),
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

    /// ONE GM-LEVEL-3 ACCOUNT PER LAUNCHER START, and this is what stops it.
    ///
    /// The fallback name carries fresh random hex, so "is `dmlsoap_ab12ef`
    /// free?" is free by construction and guards nothing. Here an EARLIER
    /// fallback (`dmlsoap_99beef`) already exists and the name about to be
    /// minted does not: without the family question this run would happily
    /// insert a second GM3 account, and the next start a third — the latch is
    /// per-process, and the trigger is ordinary (`DML_SOAP_USER`/`PASS`
    /// exported shadow the credentials we save, so SOAP keeps reading
    /// `Rejected` no matter how many accounts get made).
    #[test]
    fn an_existing_fallback_account_stops_a_second_one_from_being_minted() {
        let s = Seams {
            existing: vec!["DMLSOAP".into(), "DMLSOAP_99BEEF".into()],
            ..Default::default()
        };
        let (state, out) = s.run(AutoSetup::Idle, &rejected());
        match out {
            AutoOutcome::GaveUp { reason } => {
                assert!(reason.contains("dmlsoap_"), "the reason must name the family: {reason}")
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(state, AutoSetup::Done(Conclusion::GaveUp { .. })));
        assert!(s.creates.borrow().is_empty(), "minted another GM3 account: {:?}", s.creates);
    }

    #[test]
    fn a_failed_insert_gives_up_carrying_both_the_message_and_the_hint() {
        // Both halves verbatim from `account_write`: the hint is the ACTIONABLE
        // one, and a `contains("schema")` assertion cannot tell a reason that
        // carries it from one that silently dropped it.
        let message = "The account was created but could not be given GM access: \
                       Unknown table 'account_access'."
            .to_string();
        let hint = "Run `account set gmlevel <name> 3 -1` in the worldserver console to finish it."
            .to_string();
        let s = Seams {
            create_error: Some((message.clone(), hint.clone())),
            ..Default::default()
        };
        let (state, out) = s.run(AutoSetup::Idle, &rejected());
        match out {
            AutoOutcome::GaveUp { reason } => assert_eq!(reason, format!("{message} {hint}")),
            other => panic!("{other:?}"),
        }
        assert!(matches!(state, AutoSetup::Done(Conclusion::GaveUp { .. })));
    }

    #[test]
    fn a_failure_with_no_hint_does_not_grow_a_trailing_space() {
        // The other arm of the same branch. Not cosmetic: this string is
        // rendered straight into the fallback card, and a reason that ends in
        // whitespace reads as a sentence that got truncated.
        let message = "Could not create the account: the connection was lost.".to_string();
        let s = Seams {
            create_error: Some((message.clone(), String::new())),
            ..Default::default()
        };
        let (_, out) = s.run(AutoSetup::Idle, &rejected());
        match out {
            AutoOutcome::GaveUp { reason } => {
                assert_eq!(reason, message);
                assert!(!reason.ends_with(' '), "trailing space: {reason:?}");
            }
            other => panic!("{other:?}"),
        }
    }

    /// The lookups can FAIL rather than answer — a stopped MySQL container or
    /// wrong `DML_DB_*` credentials, which is the state a launcher polling a
    /// stack that is still starting sees constantly. All three call sites must
    /// end the run with the database's OWN words and write nothing: guessing
    /// "the name is free" would send an INSERT down the same dead connection,
    /// and guessing "taken" would abandon setup on a healthy server.
    #[test]
    fn a_lookup_that_cannot_answer_gives_up_with_the_databases_own_words() {
        // 1. The first question of all.
        let s = Seams { exists_fails_for: vec!["dmlsoap".into()], ..Default::default() };
        let (state, out) = s.run(AutoSetup::Idle, &rejected());
        match out {
            AutoOutcome::GaveUp { reason } => assert_eq!(reason, DB_DOWN),
            other => panic!("{other:?}"),
        }
        assert!(matches!(state, AutoSetup::Done(Conclusion::GaveUp { .. })));
        assert!(s.creates.borrow().is_empty());

        // 2. The family question, reached only once `dmlsoap` is taken.
        let s = Seams {
            existing: vec!["DMLSOAP".into()],
            family_fails: true,
            ..Default::default()
        };
        let (_, out) = s.run(AutoSetup::Idle, &rejected());
        match out {
            AutoOutcome::GaveUp { reason } => assert_eq!(reason, DB_DOWN),
            other => panic!("{other:?}"),
        }
        assert!(s.creates.borrow().is_empty());

        // 3. The minted name's own check, the last one before a write.
        let s = Seams {
            existing: vec!["DMLSOAP".into()],
            exists_fails_for: vec!["dmlsoap_ab12ef".into()],
            ..Default::default()
        };
        let (_, out) = s.run(AutoSetup::Idle, &rejected());
        match out {
            AutoOutcome::GaveUp { reason } => assert_eq!(reason, DB_DOWN),
            other => panic!("{other:?}"),
        }
        assert!(s.creates.borrow().is_empty());
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
        let reason = match out {
            AutoOutcome::GaveUp { reason } => reason,
            other => panic!("{other:?}"),
        };
        assert_eq!(s.verifies.borrow().len(), MAX_VERIFY_TRIES as usize);

        // And it STOPS -- while still ANSWERING. A give-up that kept being
        // retried is the loop this whole design refuses to ship; a give-up that
        // stopped SAYING WHY is how the manual fallback card disappeared after
        // a webview reload, on the one path where it is all the user has left.
        let (_, out) = s.run(state, &rejected());
        match out {
            AutoOutcome::GaveUp { reason: again } => assert_eq!(again, reason),
            other => panic!("{other:?}"),
        }
        assert_eq!(s.verifies.borrow().len(), MAX_VERIFY_TRIES as usize);
    }

    #[test]
    fn a_finished_run_touches_no_seam_ever_again_and_still_reports_its_verdict() {
        let s = Seams::default();
        let state = AutoSetup::Done(Conclusion::Saved { user: "dmlsoap".into() });
        let (state, out) = s.run(state, &rejected());
        // Re-derived from the stored conclusion rather than a contentless
        // "already done": the frontend's memory of this run is module-level
        // state that a reload wipes, so every answer has to stand alone.
        match out {
            AutoOutcome::Created { user } => assert_eq!(user, "dmlsoap"),
            other => panic!("{other:?}"),
        }
        assert!(matches!(state, AutoSetup::Done(_)));
        assert!(s.creates.borrow().is_empty());
        assert!(s.verifies.borrow().is_empty());
    }

    #[test]
    fn a_concluded_run_is_reported_as_what_it_concluded() {
        // The shared derivation both "already concluded" guards use -- the
        // launcher command's cheap exit and `advance_with`'s own. They must not
        // be able to answer differently about the same run.
        assert_eq!(
            concluded_outcome(&Conclusion::Saved { user: "dmlsoap_ab12ef".into() }),
            AutoOutcome::Created { user: "dmlsoap_ab12ef".into() }
        );
        assert_eq!(
            concluded_outcome(&Conclusion::GaveUp { reason: "no schema".into() }),
            AutoOutcome::GaveUp { reason: "no schema".into() }
        );
    }

    #[test]
    fn the_status_strings_are_the_ones_the_frontend_switches_on() {
        // The TypeScript switch matches these literals. A rename on one side
        // only would silently fall through to "do nothing". There is no
        // "latched" here any more, and there must not be one again: the
        // frontend has no case for it, by agreement.
        assert_eq!(outcome_status(&AutoOutcome::NotNeeded), "not_needed");
        assert_eq!(outcome_status(&AutoOutcome::Created { user: "x".into() }), "created");
        assert_eq!(outcome_status(&AutoOutcome::Pending), "pending");
        assert_eq!(outcome_status(&AutoOutcome::GaveUp { reason: "x".into() }), "gave_up");
    }

    /// The reveal must be OPT-IN at the boundary, not filtered in the UI.
    /// A password that always crosses the IPC boundary is a password in every
    /// devtools trace of every poll, whether or not a control shows it.
    #[test]
    fn credentials_hide_the_password_unless_it_is_asked_for() {
        let shown = credentials_payload("dmlsoap", "hunter2", "http://x/", true, true);
        assert_eq!(shown.2, Some("hunter2".to_string()));
        let hidden = credentials_payload("dmlsoap", "hunter2", "http://x/", true, false);
        assert_eq!(hidden.2, None);
        assert_eq!(hidden.0, "dmlsoap", "the name is never secret");
    }

    /// The two booleans sit next to each other in the signature, so a swapped
    /// call site is a live hazard -- and a swap is not cosmetic: it would leak
    /// the password on every unconfigured poll while telling the UI there is no
    /// account to show. Each case here pins a DIFFERENT pair, so the two cannot
    /// be exchanged without one of them failing.
    #[test]
    fn configured_is_reported_independently_of_the_reveal() {
        let (_, _, pass, configured) =
            credentials_payload("admin", "admin", "http://x/", false, false);
        assert_eq!(pass, None);
        assert!(!configured, "the built-in default is not a configured account");

        let (_, _, pass, configured) =
            credentials_payload("dmlsoap", "hunter2", "http://x/", true, false);
        assert_eq!(pass, None, "a configured account still hides its password");
        assert!(configured);

        let (_, _, pass, configured) =
            credentials_payload("admin", "admin", "http://x/", false, true);
        assert_eq!(pass, Some("admin".to_string()), "revealing works when unconfigured too");
        assert!(!configured);
    }
}
