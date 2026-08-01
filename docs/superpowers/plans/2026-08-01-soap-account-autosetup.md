# SOAP Account Autosetup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The launcher creates its own GM3 SOAP account, with a password nobody types, the moment it notices a server that answers and refuses it — and the install ends at `ready` with no account card.

**Architecture:** A new pure Rust module `crates/dml-wow/src/soap_autosetup.rs` holds password/name generation and a `AutoSetup` state machine driven through injected seams. One Tauri command `wow_soap_autosetup` binds those seams to the existing `account_write` + `soap_bootstrap` functions and latches the result in `AppState`. The frontend triggers it from the status poll it already runs, shows a shell-level banner, and `Library.svelte` loses its SOAP surface entirely.

**Tech Stack:** Rust (dml-wow, tauri 2), Svelte 5 runes, vitest, cargo test.

**Spec:** `docs/superpowers/specs/2026-08-01-soap-account-autosetup-design.md`

## Global Constraints

- **Password length is 16, not more.** `soap_cmds::valid_account_pass` enforces `{4,16}` and `create_gm_account` runs that validator first. A longer password is a self-inflicted `BAD_ARG` on every fresh install.
- **Password alphabet is exactly `[A-Za-z0-9_@#%+=!-]`** — 70 symbols, matching `valid_account_pass`'s charset byte for byte.
- **Account name rules:** `[A-Za-z0-9_]{3,20}` (`valid_account_user`).
- **Never overwrite an account.** `account_write::create_gm_account` refuses on collision; nothing in this plan adds an UPDATE or DELETE path to `acore_auth`.
- **Autosetup fires only on `VerifyOutcome::Rejected`.** Never on `Ok`, never on `Unreachable`.
- **One create attempt per launcher run.** Enforced by the `AppState` latch and by the `Pending` state.
- **Verify before save, always.** `~/.dml/soap.env` is written only by `soap_bootstrap::bootstrap_verify_with`, after a real round-trip. Do not add a second writer.
- **No bash mirror.** `srp6.rs` / `account_write.rs` are native-only; so is this.
- **LF line endings** on any shell file; this plan touches none.
- Rust dev loop runs from the **repo root**: `cargo test --workspace`. Frontend from `launcher/`: `npm test`, `npm run check`.
- Do not run bats and the cargo parity suites concurrently (bats rewrites `cli/dml` in place, which the parity suites spawn).

---

### Task 1: Password and name generation

**Files:**
- Create: `crates/dml-wow/src/soap_autosetup.rs`
- Modify: `crates/dml-wow/src/lib.rs` (add `pub mod soap_autosetup;`)

**Interfaces:**
- Consumes: `crate::soap_bootstrap::DEFAULT_SOAP_USER`, `crate::soap_cmds::{valid_account_user, valid_account_pass}`
- Produces:
  - `pub const PASSWORD_LEN: usize = 16`
  - `pub const PASSWORD_ALPHABET: &[u8]` (70 bytes)
  - `pub fn generate_password_from(fill: impl FnMut(&mut [u8])) -> String`
  - `pub fn generate_password() -> String`
  - `pub fn random_hex6() -> String`
  - `pub fn fallback_user(hex6: &str) -> String`

- [ ] **Step 1: Write the failing tests**

Create `crates/dml-wow/src/soap_autosetup.rs` containing ONLY the test module below plus the four `use` lines. It will not compile until Step 3 — that is the point.

```rust
//! (module doc comes in Step 3)

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
}
```

Add to `crates/dml-wow/src/lib.rs`, in the existing alphabetical `pub mod` block (it sits between `pub mod soap_bootstrap;` and `pub mod soap_cmds;`):

```rust
pub mod soap_autosetup;
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p dml-wow --lib soap_autosetup`
Expected: FAIL — compile errors, `cannot find value PASSWORD_ALPHABET in this scope` and friends.

- [ ] **Step 3: Write the implementation**

Put this ABOVE the `#[cfg(test)] mod tests` block in `crates/dml-wow/src/soap_autosetup.rs`:

```rust
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

use crate::soap_bootstrap::DEFAULT_SOAP_USER;

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
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p dml-wow --lib soap_autosetup`
Expected: PASS, 6 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/dml-wow/src/soap_autosetup.rs crates/dml-wow/src/lib.rs
git commit -m "feat(soap): a password nobody types, and no modulo bias in it

16 characters because that is AzerothCore's ceiling and the validator
create_gm_account already runs -- a longer one would BAD_ARG on every fresh
install, at the one moment the user has nothing to retype.

Rejection sampling rather than byte % 70: 256 = 3*70 + 46, so plain modulo
hands the first 46 symbols a fourth chance the other 24 never get. The bias
is invisible in any output anyone would look at, which is the reason to
handle it here instead of noticing it later.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: The `AutoSetup` state machine

**Files:**
- Modify: `crates/dml-wow/src/soap_autosetup.rs`

**Interfaces:**
- Consumes: Task 1's `generate_password`, `random_hex6`, `fallback_user`; `crate::soap_bootstrap::{DEFAULT_SOAP_USER, VerifyOutcome}`; `dml_core::error::CmdError`
- Produces:
  - `pub enum AutoSetup { Idle, Pending { user: String, pass: String, tries: u8 }, Done(Conclusion) }`
  - `pub enum Conclusion { Saved { user: String }, GaveUp { reason: String } }`
  - `pub enum AutoOutcome { NotNeeded, Created { user: String }, Pending, GaveUp { reason: String }, Latched }`
  - `pub const MAX_VERIFY_TRIES: u8 = 3`
  - `pub fn advance_with(state, status, exists, create, verify, hex6, gen_pass) -> (AutoSetup, AutoOutcome)`
  - `pub fn outcome_status(&AutoOutcome) -> &'static str`

- [ ] **Step 1: Write the failing tests**

Append to the `mod tests` block in `crates/dml-wow/src/soap_autosetup.rs`:

```rust
    use crate::soap_bootstrap::VerifyOutcome;
    use dml_core::error::CmdError;
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
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p dml-wow --lib soap_autosetup`
Expected: FAIL — `cannot find type AutoSetup in this scope`.

- [ ] **Step 3: Write the implementation**

Append to the non-test part of `crates/dml-wow/src/soap_autosetup.rs`:

```rust
use crate::soap_bootstrap::VerifyOutcome;
use dml_core::error::CmdError;

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

/// What the caller is told. Serialized to the frontend by `outcome_status`.
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
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p dml-wow --lib soap_autosetup`
Expected: PASS, 16 tests.

Then the whole crate, to catch anything the new `pub mod` disturbed:

Run: `cargo test -p dml-wow --lib`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/dml-wow/src/soap_autosetup.rs
git commit -m "feat(soap): Pending, so a failed verify cannot write a second account

The state machine exists for one branch. Create succeeds, verify does not:
if that left the latch open the next poll would create another account, and
the one after that a third -- one row per tick into the user's auth
database, forever. Pending carries the credential forward and re-verifies.

A taken name is worked AROUND, never over. create_gm_account refuses to
overwrite a row it did not write, and dmlsoap_<hex> is how that refusal
stays survivable instead of becoming a dead end.

The negative the seams exist to express: a healthy server must not cause a
single DB read, and the test asserts create/exists were never called rather
than asserting a return value that a broken implementation could also
produce.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: The `wow_soap_autosetup` command and its latch

**Files:**
- Modify: `launcher/src-tauri/src/lib.rs` — `AppState` struct (~line 73–103), the `.manage(AppState { … })` block (~line 6776), a new command after `wow_soap_account_create` (~line 6418), and the `invoke_handler` list (~line 6888)

**Interfaces:**
- Consumes: Task 2's `dml_wow::soap_autosetup::{AutoSetup, AutoOutcome, advance_with, outcome_status, generate_password, random_hex6}`
- Produces: Tauri command `wow_soap_autosetup` returning `{ status: string, user: string|null, reason: string|null }`

- [ ] **Step 1: Add the latch to `AppState`**

In `launcher/src-tauri/src/lib.rs`, add this field to `pub struct AppState` immediately after `last_status_push`:

```rust
    /// Where automatic SOAP account setup got to THIS launcher run.
    ///
    /// One attempt per run, and that bound is the feature. The trigger is the
    /// status poll, which ticks every few seconds; without a latch a server
    /// that keeps refusing us would get one INSERT per tick. Once this reaches
    /// `Done`, `wow_soap_autosetup` returns `latched` without opening a SOAP
    /// connection or a DB connection.
    ///
    /// Known limit, deliberate: wiping the auth database mid-session needs a
    /// launcher restart to self-heal. The alternative is an unlatched loop
    /// writing rows into a database that keeps losing them.
    pub soap_autosetup: Arc<Mutex<dml_wow::soap_autosetup::AutoSetup>>,
```

And in the `.manage(AppState { … })` block, after `last_status_push: Arc::new(Mutex::new(None)),`:

```rust
            soap_autosetup: Arc::new(Mutex::new(dml_wow::soap_autosetup::AutoSetup::Idle)),
```

- [ ] **Step 2: Write the command**

Insert directly after the closing brace of `wow_soap_account_create` (before the `/// Prove the account works, and ONLY then remember it.` doc block of `wow_soap_bootstrap_verify`):

```rust
/// Set SOAP up by itself, once per launcher run.
///
/// The fully automatic replacement for the account card: the user types
/// nothing, clicks nothing, and — when this succeeds — never learns the step
/// existed. `dml_wow::soap_autosetup` holds the decision tree; this function is
/// only the wiring between its seams and the real database, SOAP client and
/// filesystem.
///
/// Three properties are load-bearing and all three live in the seams below:
///
/// * **It asks before it acts.** The status comes from `soap_status_with`, the
///   same classifier `wow_soap_status` uses, so a `Fault` is not mistaken for
///   an auth failure and a world server that is merely still booting is not
///   mistaken for a broken account. A non-`Rejected` verdict returns
///   `not_needed` having opened no DB connection at all.
/// * **It proves itself before it saves.** The verify seam is
///   `bootstrap_verify_with`, which writes `~/.dml/soap.env` only after a real
///   round-trip. A mistake in the SRP6 produces a verifier that is perfectly
///   well-formed and simply never authenticates, so "the INSERT returned Ok"
///   proves nothing.
/// * **It stops.** The `AppState` latch means a poll that ticks every few
///   seconds cannot turn into one INSERT per tick.
///
/// Never returns `Err` for an unhappy server — a refusal is an answer about the
/// machine and comes back as a verdict the banner or the fallback card renders.
#[tauri::command]
async fn wow_soap_autosetup(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    use dml_wow::soap_autosetup as auto;
    use dml_wow::soap_bootstrap as sb;

    let home = dml_core::util::home_dir().ok_or_else(|| CmdError {
        code: "NO_HOME".into(),
        message: "Could not find your user folder, so the credentials could not be saved.".into(),
        hint: String::new(),
    })?;
    let url = dml_wow::soap::SoapConfig::load().url;
    let soap_lock = state.soap_lock.clone();
    let latch = state.soap_autosetup.clone();

    let outcome = tauri::async_runtime::spawn_blocking(move || {
        // Cheap exit first: a concluded run must not even ask the server.
        {
            let g = latch.lock().unwrap();
            if matches!(*g, auto::AutoSetup::Done(_)) {
                return auto::AutoOutcome::Latched;
            }
        }

        let status = sb::soap_status_with(|cfg, cmd| {
            let _guard = soap_lock.lock();
            dml_wow::soap::exec(cfg, cmd)
        });

        let cfg = dml_wow::db::DbConfig::from_env();
        let mut g = latch.lock().unwrap();
        let state_now = g.clone();
        let (next, outcome) = auto::advance_with(
            state_now,
            &status,
            |u| dml_wow::account_write::account_exists(&cfg, u),
            |u, p| dml_wow::account_write::create_gm_account(&cfg, u, p),
            |u, p| {
                // This is the writer of ~/.dml/soap.env, and it writes only
                // after the round-trip below succeeds.
                sb::bootstrap_verify_with(&home, &url, u, p, |c, cmd| {
                    let _guard = soap_lock.lock();
                    dml_wow::soap::exec(c, cmd)
                })
                .map(|(v, _path)| v)
            },
            auto::random_hex6,
            auto::generate_password,
        );
        *g = next;
        outcome
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?;

    Ok(serde_json::json!({
        "status": auto::outcome_status(&outcome),
        "user": match &outcome {
            auto::AutoOutcome::Created { user } => serde_json::Value::String(user.clone()),
            _ => serde_json::Value::Null,
        },
        "reason": match &outcome {
            auto::AutoOutcome::GaveUp { reason } => serde_json::Value::String(reason.clone()),
            _ => serde_json::Value::Null,
        },
    }))
}
```

- [ ] **Step 3: Register the command**

In the `tauri::generate_handler![…]` list, add `wow_soap_autosetup,` immediately after `wow_soap_account_create,`.

- [ ] **Step 4: Verify it compiles and the workspace is still green**

Run: `cargo test --workspace`
Expected: PASS. (Run with the stack down; the live/ignored tests do not run.)

If `AutoSetup` does not `Clone`, add `Clone` to its derive in Task 2's implementation — `state_now = g.clone()` needs it. (`#[derive(Debug, Clone, PartialEq, Eq)]` as written already provides it; this note exists so a reviewer does not "tidy" it away.)

- [ ] **Step 5: Commit**

```bash
git add launcher/src-tauri/src/lib.rs
git commit -m "feat(soap): wire the autosetup seams to the real server, once per run

The latch is the feature, not an optimisation. The trigger is the status
poll, which ticks every few seconds; without it a server that keeps refusing
us gets one INSERT per tick. A concluded run returns latched before it opens
a SOAP connection, let alone a DB one.

The verify seam is bootstrap_verify_with rather than a fresh round-trip,
because that function is also what writes ~/.dml/soap.env and it writes only
after the trip succeeds. Two definitions of done is how they come to
disagree.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: The frontend contract — api wrapper and store

**Files:**
- Modify: `launcher/src/lib/api.ts` (after `wowSoapAccountCreate`, ~line 1589)
- Modify: `launcher/src/lib/soap-setup-state.svelte.ts`
- Create: `launcher/src/lib/soap-setup-state.test.ts`

**Interfaces:**
- Consumes: Task 3's `wow_soap_autosetup` JSON shape
- Produces:
  - `export interface SoapAutosetupVerdict { status: string; user: string | null; reason: string | null }`
  - `export async function wowSoapAutosetup(): Promise<SoapAutosetupVerdict>`
  - `soapSetupState` gains `autoResult: { user: string } | null` and `gaveUpReason: string | null`
  - `export function applyAutosetupOutcome(v: SoapAutosetupVerdict): boolean` — returns **settled**

- [ ] **Step 1: Write the failing tests**

Create `launcher/src/lib/soap-setup-state.test.ts`:

```ts
import { describe, it, expect, beforeEach } from "vitest";
import {
  soapSetupState,
  applyAutosetupOutcome,
  clearSoapSetup,
} from "./soap-setup-state.svelte";

beforeEach(() => {
  clearSoapSetup();
});

describe("applyAutosetupOutcome", () => {
  it("a created account announces itself and never raises the manual card", () => {
    const settled = applyAutosetupOutcome({ status: "created", user: "dmlsoap", reason: null });
    expect(settled).toBe(true);
    expect(soapSetupState.autoResult).toEqual({ user: "dmlsoap" });
    // THE point of the whole change: success must not also show the card the
    // user was never meant to see.
    expect(soapSetupState.needed).toBe(false);
  });

  it("only a give-up raises the manual card", () => {
    const settled = applyAutosetupOutcome({
      status: "gave_up",
      user: null,
      reason: "Both names exist.",
    });
    expect(settled).toBe(true);
    expect(soapSetupState.needed).toBe(true);
    expect(soapSetupState.gaveUpReason).toBe("Both names exist.");
    expect(soapSetupState.autoResult).toBeNull();
  });

  it("pending settles nothing, so the next poll tries again", () => {
    expect(applyAutosetupOutcome({ status: "pending", user: null, reason: null })).toBe(false);
    expect(soapSetupState.needed).toBe(false);
    expect(soapSetupState.autoResult).toBeNull();
  });

  it("not_needed is silent and shows nothing at all", () => {
    expect(applyAutosetupOutcome({ status: "not_needed", user: null, reason: null })).toBe(false);
    expect(soapSetupState.needed).toBe(false);
    expect(soapSetupState.autoResult).toBeNull();
  });

  it("latched settles without changing what is on screen", () => {
    expect(applyAutosetupOutcome({ status: "latched", user: null, reason: null })).toBe(true);
    expect(soapSetupState.needed).toBe(false);
  });

  it("an unknown status is ignored rather than crashing", () => {
    // Same rule the TermEvent union follows: an older/newer backend must not
    // take the UI down.
    expect(applyAutosetupOutcome({ status: "who-knows", user: null, reason: null })).toBe(false);
    expect(soapSetupState.needed).toBe(false);
  });

  it("clearSoapSetup wipes the banner as well as the card", () => {
    applyAutosetupOutcome({ status: "created", user: "dmlsoap", reason: null });
    clearSoapSetup();
    expect(soapSetupState.autoResult).toBeNull();
    expect(soapSetupState.needed).toBe(false);
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run (from `launcher/`): `npm test -- soap-setup-state`
Expected: FAIL — `applyAutosetupOutcome is not exported`.

- [ ] **Step 3: Write the implementation**

Add to `launcher/src/lib/api.ts` after `wowSoapAccountCreate`:

```ts
/** What automatic SOAP setup concluded this launcher run. */
export interface SoapAutosetupVerdict {
  /** "not_needed" | "created" | "pending" | "gave_up" | "latched" */
  status: string;
  /** The account that now exists. Non-null ONLY on "created". */
  user: string | null;
  /** Why this run stopped trying. Non-null ONLY on "gave_up". */
  reason: string | null;
}
// One call per launcher run does real work; every later call returns "latched"
// without opening a SOAP or DB connection. Safe to invoke from the poll loop.
export async function wowSoapAutosetup(): Promise<SoapAutosetupVerdict> {
  return await invoke("wow_soap_autosetup");
}
```

Replace the body of `launcher/src/lib/soap-setup-state.svelte.ts` below its existing header comment with:

```ts
import type { SoapAutosetupVerdict } from "./api";

export const soapSetupState = $state({
  /**
   * Show the MANUAL account card.
   *
   * Since autosetup landed this is the FALLBACK, not the default: it is true
   * only when the launcher tried to create the account itself and could not.
   */
  needed: false,
  /** Set when the launcher created the account on its own. Drives the banner. */
  autoResult: null as { user: string } | null,
  /** Why autosetup stopped trying. Shown above the manual card. */
  gaveUpReason: null as string | null,
});

/**
 * Fold one autosetup verdict into the UI state. Returns whether this run is
 * SETTLED — i.e. whether the poll should stop asking.
 *
 * An unknown status settles nothing and shows nothing. Same rule the TermEvent
 * union follows: a backend from a different build must not take the UI down.
 */
export function applyAutosetupOutcome(v: SoapAutosetupVerdict): boolean {
  switch (v.status) {
    case "created":
      soapSetupState.autoResult = { user: v.user ?? "" };
      soapSetupState.needed = false;
      soapSetupState.gaveUpReason = null;
      return true;
    case "gave_up":
      // The ONLY path that raises the manual card.
      soapSetupState.needed = true;
      soapSetupState.autoResult = null;
      soapSetupState.gaveUpReason = v.reason;
      return true;
    case "latched":
      return true;
    default:
      return false;
  }
}

/** The credentials were verified and saved, or the user dismissed the notice. */
export function clearSoapSetup(): void {
  soapSetupState.needed = false;
  soapSetupState.autoResult = null;
  soapSetupState.gaveUpReason = null;
}
```

Delete `noteNativeInstallFinished` — its only caller goes away in Task 6, and leaving an exported "an install finished" flag setter beside a store that no longer works that way is an invitation to re-wire the bug it documents.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `npm test -- soap-setup-state`
Expected: PASS, 7 tests.

- [ ] **Step 5: Commit**

```bash
git add launcher/src/lib/api.ts launcher/src/lib/soap-setup-state.svelte.ts launcher/src/lib/soap-setup-state.test.ts
git commit -m "feat(soap): the manual card becomes the fallback, not the default

soapSetupState.needed is now raised by exactly one status, gave_up. A
created account sets autoResult and leaves needed false -- success must not
also show the screen the user was never meant to see.

An unknown status settles nothing and renders nothing, the same rule the
TermEvent union follows: a backend from a different build must not take the
UI down.

noteNativeInstallFinished is deleted rather than left unused. Its own
comment records the bug it was written for, and an exported 'an install
finished' setter sitting beside a store that no longer works that way is an
invitation to re-wire it.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 5: Trigger it from the status poll

**Files:**
- Modify: `launcher/src/lib/server-status.svelte.ts` (imports; new pure decision + impure caller; one call inside `refreshServerStatus`'s try block)
- Create: `launcher/src/lib/soap-autosetup-trigger.test.ts`

**Interfaces:**
- Consumes: Task 4's `wowSoapAutosetup`, `applyAutosetupOutcome`; existing `ServerDetail` (`soap.reachable: boolean`, `soap.auth_ok: boolean | null`)
- Produces: `export function shouldTryAutosetup(detail: ServerDetail | null, settled: boolean, inFlight: boolean): boolean`

- [ ] **Step 1: Write the failing tests**

Create `launcher/src/lib/soap-autosetup-trigger.test.ts`:

```ts
import { describe, it, expect } from "vitest";
import { shouldTryAutosetup } from "./server-status.svelte";
import type { ServerDetail } from "./api";

function detail(soap: Partial<ServerDetail["soap"]>): ServerDetail {
  return {
    verdict: "online",
    exit_code: null,
    containers: [],
    world_ready: true,
    soap: {
      reachable: true,
      auth_ok: false,
      version: null,
      players: null,
      uptime: null,
      mean_ms: null,
      median_ms: null,
      ...soap,
    },
    ports: { world: null, auth: null, soap: null, db: null },
    bots: { online: null, max: null },
  };
}

describe("shouldTryAutosetup", () => {
  it("fires for a server that answers and refuses us", () => {
    expect(shouldTryAutosetup(detail({}), false, false)).toBe(true);
  });

  it("never fires when SOAP already works", () => {
    expect(shouldTryAutosetup(detail({ auth_ok: true }), false, false)).toBe(false);
  });

  it("never fires for an unreachable SOAP", () => {
    // A world server still booting is not a broken account. Rust would answer
    // not_needed anyway; not asking saves a pointless round-trip on every tick
    // of a stopped server.
    expect(shouldTryAutosetup(detail({ reachable: false }), false, false)).toBe(false);
  });

  it("never fires on an unknown auth state", () => {
    // auth_ok is `boolean | null` and null means "not determined". Treating it
    // as false would create an account on evidence we do not have.
    expect(shouldTryAutosetup(detail({ auth_ok: null }), false, false)).toBe(false);
  });

  it("never fires without a detail at all", () => {
    expect(shouldTryAutosetup(null, false, false)).toBe(false);
  });

  it("stops once the run is settled", () => {
    // Rust latches too, but this stops a pointless IPC call every poll tick
    // for the whole life of the app.
    expect(shouldTryAutosetup(detail({}), true, false)).toBe(false);
  });

  it("does not stack up while one call is in flight", () => {
    expect(shouldTryAutosetup(detail({}), false, true)).toBe(false);
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `npm test -- soap-autosetup-trigger`
Expected: FAIL — `shouldTryAutosetup is not exported`.

- [ ] **Step 3: Write the implementation**

In `launcher/src/lib/server-status.svelte.ts`, extend the existing `./api` import with `wowSoapAutosetup` and add a new import:

```ts
import { applyAutosetupOutcome } from "./soap-setup-state.svelte";
```

Add below `shouldReleaseKeepAwakeOnFailure`:

```ts
// Automatic SOAP account setup rides the poll that already runs, rather than
// adding a second one.
//
// `soap.reachable && soap.auth_ok === false` is exactly "the server answered
// and refused us" -- the one state in which an account needs creating. Rust
// re-derives that verdict authoritatively with `soap_bootstrap::classify`
// before it writes anything, so this is a cheap trigger and NOT the decision:
// a false positive costs one `server info` call and nothing else.
//
// `auth_ok === null` means the poll did not determine it. Treating null as
// false would create a GM3 account on evidence we do not have.
export function shouldTryAutosetup(
  detail: ServerDetail | null,
  settled: boolean,
  inFlight: boolean,
): boolean {
  if (settled || inFlight || !detail) return false;
  return detail.soap.reachable === true && detail.soap.auth_ok === false;
}

let autosetupSettled = false;
let autosetupInFlight = false;

// Best-effort by design: a failed autosetup call must never break the status
// poll it rides on. The manual card is still reachable either way.
async function maybeAutosetup(): Promise<void> {
  if (!shouldTryAutosetup(serverStatus.detail, autosetupSettled, autosetupInFlight)) return;
  autosetupInFlight = true;
  try {
    autosetupSettled = applyAutosetupOutcome(await wowSoapAutosetup());
  } catch {
    /* leave it unsettled; the next poll tries again */
  } finally {
    autosetupInFlight = false;
  }
}
```

Inside `refreshServerStatus`, add one line at the end of the `try` block, immediately after `runTransitionActions(prev, serverStatus.detail.verdict);`:

```ts
    void maybeAutosetup();
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `npm test -- soap-autosetup-trigger`
Expected: PASS, 7 tests.

Run: `npm run check`
Expected: 0 errors, 0 warnings.

- [ ] **Step 5: Commit**

```bash
git add launcher/src/lib/server-status.svelte.ts launcher/src/lib/soap-autosetup-trigger.test.ts
git commit -m "feat(soap): ride the poll that already runs

soap.reachable && auth_ok === false is exactly 'answered and refused us',
which the shell poll already reports -- no second poll, no new timer. The
trigger is deliberately NOT the decision: Rust re-derives the verdict with
the tested classifier before writing anything, so a false positive costs one
server info call.

auth_ok is boolean | null and null means undetermined. Treating it as false
would create a GM3 account on evidence we do not have, so the check is
=== false rather than a falsy test.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 6: The banner moves to the shell, and Library loses the step

**Files:**
- Modify: `launcher/src/routes/+page.svelte` (imports; markup before the `readyToast` block ~line 328; styles)
- Modify: `launcher/src/lib/pages/Library.svelte` (delete `refreshSoapNeed` + both call sites, the `SoapBootstrap` mount, and three imports)
- Modify: `launcher/src/lib/SoapBootstrap.svelte` (show `gaveUpReason` when present)
- Create: `launcher/src/lib/soap-surface.test.ts`

**Interfaces:**
- Consumes: Task 4's `soapSetupState`, `clearSoapSetup`
- Produces: nothing new; this is placement.

- [ ] **Step 1: Write the failing test**

Create `launcher/src/lib/soap-surface.test.ts`:

```ts
import { describe, it, expect } from "vitest";

// Sources come in via import.meta.glob(?raw), the same technique
// feature-keys.test.ts uses (the app has no @types/node).
const SOURCES = import.meta.glob(
  ["./pages/Library.svelte", "../routes/+page.svelte"],
  { query: "?raw", import: "default", eager: true },
) as Record<string, string>;

function find(suffix: string): string {
  const hit = Object.entries(SOURCES).find(([f]) => f.endsWith(suffix));
  if (!hit) throw new Error(`no source for ${suffix} — the glob is wrong`);
  return hit[1];
}

/**
 * Strip comments before matching.
 *
 * This repo was bitten TWICE on 2026-08-01 by source scans that read an
 * explanation of a thing as the thing itself. Library.svelte is dense with
 * `// … soap …` prose about why the step worked the way it did, and a raw grep
 * would report the surface as still present after it was removed — a red test
 * on correct code, which is how a scan like this gets deleted.
 */
function code(src: string): string {
  return src
    .replace(/<!--[\s\S]*?-->/g, "")
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/(^|[^:])\/\/.*$/gm, "$1");
}

describe("the SOAP account step is a shell surface, not a Library one", () => {
  it("strips comments rather than grepping raw source", () => {
    // Non-vacuity for the stripper itself.
    expect(code("// SoapBootstrap\nconst a = 1;")).not.toContain("SoapBootstrap");
    expect(code("<!-- soapSetupState -->\n<div/>")).not.toContain("soapSetupState");
    expect(code("import X from './SoapBootstrap.svelte';")).toContain("SoapBootstrap");
    // A protocol-relative URL must survive the line-comment rule.
    expect(code("const u = 'https://x/y';")).toContain("https://x/y");
  });

  it("Library.svelte has no SOAP surface left", () => {
    const src = code(find("pages/Library.svelte"));
    for (const token of [
      "SoapBootstrap",
      "soapSetupState",
      "wowSoapStatus",
      "refreshSoapNeed",
      "clearSoapSetup",
    ]) {
      expect(src, `Library still references ${token}`).not.toContain(token);
    }
  });

  it("the shell carries both the banner and the fallback card", () => {
    // A fallback reachable from one page only is the same bug this change
    // removes; it must not survive in the failure path.
    const src = code(find("routes/+page.svelte"));
    expect(src).toContain("SoapBootstrap");
    expect(src).toContain("soapSetupState");
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `npm test -- soap-surface`
Expected: FAIL — "Library still references SoapBootstrap", and the shell assertions fail too.

- [ ] **Step 3: Strip Library.svelte**

Delete all of the following from `launcher/src/lib/pages/Library.svelte`:

1. `import SoapBootstrap from "$lib/SoapBootstrap.svelte";` (~line 16)
2. The `soapSetupState`, `noteNativeInstallFinished`, `clearSoapSetup` names from the `$lib/soap-setup-state.svelte` import (~lines 17–21) — remove the whole import statement if nothing else is taken from it.
3. `wowSoapStatus` from the `$lib/api` import (~line 22), keeping `gamesInstallNativeState`.
4. The whole `refreshSoapNeed` function including its comment block (~lines 159–176).
5. `if (backendMode === "native") await refreshSoapNeed();` from `onMount` (~line 156).
6. In `onInstallExit`, the comment block about the account step and the line `if (code === 0 && backendMode === "native") void refreshSoapNeed();` (~lines 357–366). Leave the rest of the function.
7. The mount block (~lines 623–625):

```svelte
  {#if soapSetupState.needed}
    <SoapBootstrap onverified={clearSoapSetup} ondismiss={clearSoapSetup} />
  {/if}
```

If `onInstallExit` ends up with an empty body, keep it and its existing catalog-refresh comment — `InstallTerminal` requires the prop.

- [ ] **Step 4: Add the banner and the card to the shell**

In `launcher/src/routes/+page.svelte`, add to the script block:

```ts
  import SoapBootstrap from "$lib/SoapBootstrap.svelte";
  import { soapSetupState, clearSoapSetup } from "$lib/soap-setup-state.svelte";
```

Add this markup immediately BEFORE the `{#if serverStatus.readyToast}` block:

```svelte
  {#if soapSetupState.autoResult}
    <!-- Silent by default, but not invisible: a GM3 account now exists on the
         user's server because the launcher put it there, and that is theirs to
         know. Shell-level because a native install runs for HOURS and the user
         will be on some other page when it lands -- the old card rendered only
         inside Library, which is the bug this replaces. -->
    <button
      class="soap-banner"
      onclick={clearSoapSetup}
      title="Dismiss"
    >
      Server access set up automatically as <strong>{soapSetupState.autoResult.user}</strong>.
      <span class="soap-sub">GM Tools, My Party and the console are live.</span>
    </button>
  {/if}

  {#if soapSetupState.needed}
    <!-- The fallback, and it lives here for the same reason the banner does. A
         fallback reachable from one page only is the failure path inheriting
         the bug the success path just lost. -->
    {#if soapSetupState.gaveUpReason}
      <p class="soap-gaveup">{soapSetupState.gaveUpReason}</p>
    {/if}
    <SoapBootstrap onverified={clearSoapSetup} ondismiss={clearSoapSetup} />
  {/if}
```

Add to the `<style>` block:

```css
  .soap-banner {
    display: block;
    width: 100%;
    text-align: left;
    margin: 0.6rem 0 0;
    padding: 0.55rem 0.8rem;
    border: 1px solid var(--ok-fg, #8ec07c);
    border-radius: 0.4rem;
    background: rgba(142, 192, 124, 0.08);
    color: inherit;
    font: inherit;
    font-size: 0.88rem;
    cursor: pointer;
  }
  .soap-sub { display: block; opacity: 0.75; font-size: 0.8rem; }
  .soap-gaveup {
    margin: 0.6rem 0 0;
    font-size: 0.88rem;
    color: var(--warn-fg, #f0c674);
  }
```

- [ ] **Step 5: Show the give-up reason inside the card too**

In `launcher/src/lib/SoapBootstrap.svelte`, change the opening paragraph so a user who reaches the card knows why. Replace the `<p class="why">…</p>` element with:

```svelte
  <p class="why">
    Your server has no account the launcher can use. Until one exists, GM Tools, My Party,
    the console's command box and announcements can't work — they'll fail with an
    authentication error that doesn't explain itself. The launcher tried to set this up on
    its own and couldn't, so it needs you for a minute.
  </p>
```

- [ ] **Step 6: Run the tests**

Run: `npm test -- soap-surface`
Expected: PASS, 3 tests.

Run: `npm test`
Expected: PASS (baseline was 603; expect ~620 with Tasks 4–6).

Run: `npm run check`
Expected: 0 errors, 0 warnings, 306 files.

- [ ] **Step 7: Commit**

```bash
git add launcher/src/routes/+page.svelte launcher/src/lib/pages/Library.svelte launcher/src/lib/SoapBootstrap.svelte launcher/src/lib/soap-surface.test.ts
git commit -m "feat(soap): the install ends at ready, and the notice follows the user

Library loses refreshSoapNeed, both its call sites and the SoapBootstrap
mount. The reasoning in those comments is kept, not dropped: the step still
ASKS whether SOAP works rather than remembering an install finished, and an
unreachable server is still not a broken account -- both now live in
soap_autosetup, where they are not tied to one page being mounted.

The fallback card moves to the shell alongside the banner. Leaving it in
Library would hand the failure path exactly the bug the success path just
lost: a native install runs for hours, one sidebar click destroys Library,
and the screen that finishes the server becomes unreachable.

The source scan strips comments before matching. Library is dense with
prose about the old step, and a raw grep would fail on correct code -- the
same trap that bit feature-keys.test.ts and Test-InstallerNative.ps1 today.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 7: Reveal the password on Home

**Files:**
- Modify: `launcher/src-tauri/src/lib.rs` (new command after `wow_soap_autosetup`; register it)
- Modify: `launcher/src/lib/api.ts`
- Modify: `launcher/src/lib/pages/Home.svelte` (the health panel's SOAP row, ~lines 487–491)

**Interfaces:**
- Consumes: `dml_wow::soap::SoapConfig::load()`
- Produces:
  - Tauri command `wow_soap_credentials` → `{ user: string, url: string }` plus `pass` **only when asked**
  - `export interface SoapCredentials { user: string; url: string; pass: string | null }`
  - `export async function wowSoapCredentials(reveal: boolean): Promise<SoapCredentials>`

- [ ] **Step 1: Write the failing Rust test**

Add to the `#[cfg(test)] mod tests` at the bottom of `crates/dml-wow/src/soap_autosetup.rs`:

```rust
    /// The reveal must be OPT-IN at the boundary, not filtered in the UI.
    /// A password that always crosses the IPC boundary is a password in every
    /// devtools trace of every poll, whether or not a control shows it.
    #[test]
    fn credentials_hide_the_password_unless_it_is_asked_for() {
        let shown = credentials_payload("dmlsoap", "hunter2", "http://x/", true);
        assert_eq!(shown.2, Some("hunter2".to_string()));
        let hidden = credentials_payload("dmlsoap", "hunter2", "http://x/", false);
        assert_eq!(hidden.2, None);
        assert_eq!(hidden.0, "dmlsoap", "the name is never secret");
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p dml-wow --lib soap_autosetup`
Expected: FAIL — `cannot find function credentials_payload`.

- [ ] **Step 3: Implement the helper and the command**

Append to the non-test part of `crates/dml-wow/src/soap_autosetup.rs`:

```rust
/// `(user, url, pass)` for the credentials panel.
///
/// The password is opt-in at this boundary rather than filtered in the UI. A
/// secret that always crosses the IPC boundary is a secret in every devtools
/// trace, whether or not a control is showing it.
pub fn credentials_payload(
    user: &str,
    pass: &str,
    url: &str,
    reveal: bool,
) -> (String, String, Option<String>) {
    (
        user.to_string(),
        url.to_string(),
        if reveal { Some(pass.to_string()) } else { None },
    )
}
```

Add to `launcher/src-tauri/src/lib.rs` after `wow_soap_autosetup`:

```rust
/// Which account the launcher uses, and — only when asked — its password.
///
/// The launcher generates that password, so it is the one credential the app
/// knows and the user does not. This is where they can read it back: a
/// generated secret with no way to see it is a secret the user cannot use when
/// they need it (a second tool, a support question, a manual SOAP call).
///
/// Read-only. There is no write path here.
#[tauri::command]
async fn wow_soap_credentials(reveal: bool) -> Result<serde_json::Value, CmdError> {
    let cfg = dml_wow::soap::SoapConfig::load();
    let (user, url, pass) =
        dml_wow::soap_autosetup::credentials_payload(&cfg.user, &cfg.pass, &cfg.url, reveal);
    Ok(serde_json::json!({ "user": user, "url": url, "pass": pass }))
}
```

Register `wow_soap_credentials,` in the `generate_handler!` list next to `wow_soap_autosetup,`.

Add to `launcher/src/lib/api.ts`:

```ts
/** The SOAP account the launcher uses. `pass` is non-null only when revealed. */
export interface SoapCredentials {
  user: string;
  url: string;
  pass: string | null;
}
// The password crosses the IPC boundary ONLY when reveal is true — it is a
// generated secret, and a secret that rides along on every status render is a
// secret in every devtools trace.
export async function wowSoapCredentials(reveal: boolean): Promise<SoapCredentials> {
  return await invoke("wow_soap_credentials", { reveal });
}
```

- [ ] **Step 4: Wire the Home health panel**

In `launcher/src/lib/pages/Home.svelte`, add to the script block:

```ts
  import { wowSoapCredentials, type SoapCredentials } from "$lib/api";

  let soapCreds = $state<SoapCredentials | null>(null);
  let soapPassShown = $state(false);

  // The launcher generates this password, so this is the only place a user can
  // read it back. Fetched on demand, never on render.
  async function toggleSoapPass() {
    if (soapPassShown) {
      soapCreds = null;
      soapPassShown = false;
      return;
    }
    try {
      soapCreds = await wowSoapCredentials(true);
      soapPassShown = true;
    } catch {
      soapCreds = null;
    }
  }
```

Replace the existing SOAP health row (the `<span class="hname">SOAP</span>` block, ~lines 487–491) with:

```svelte
              <span class="hname">SOAP</span>
              <span class="hval">
                {d.soap.reachable ? "reachable" : "unreachable"}{d.soap.auth_ok === false
                  ? " — authentication failing, check ~/.dml/soap.env"
                  : ""}
                <button class="linky" onclick={toggleSoapPass}>
                  {soapPassShown ? "Hide account" : "Show account"}
                </button>
                {#if soapPassShown && soapCreds}
                  <span class="creds">
                    {soapCreds.user} / <code>{soapCreds.pass ?? "?"}</code>
                  </span>
                {/if}
              </span>
```

Add to Home's `<style>`:

```css
  .linky {
    background: none;
    border: none;
    padding: 0;
    margin-left: 0.4rem;
    color: var(--accent, #4a90d9);
    font: inherit;
    font-size: 0.8rem;
    text-decoration: underline;
    cursor: pointer;
  }
  .creds { display: block; font-size: 0.8rem; opacity: 0.85; }
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p dml-wow --lib soap_autosetup`
Expected: PASS, 17 tests.

Run (from `launcher/`): `npm run check`
Expected: 0 errors, 0 warnings.

- [ ] **Step 6: Commit**

```bash
git add crates/dml-wow/src/soap_autosetup.rs launcher/src-tauri/src/lib.rs launcher/src/lib/api.ts launcher/src/lib/pages/Home.svelte
git commit -m "feat(soap): let the user read back the password the app invented

Nobody types this password any more, which makes it the one credential the
app knows and the user does not. A generated secret with no way to see it is
a secret they cannot use when they need it -- a second tool, a support
question, a manual SOAP call.

Opt-in at the IPC boundary rather than filtered in the UI: a password that
rides along on every status render is a password in every devtools trace,
whether or not a control is showing it.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 8: Full sweep and the live gate

**Files:**
- Modify: `crates/dml-wow/src/soap_autosetup.rs` (one `#[ignore]` live test)
- Modify: `CLAUDE.md` (crates section)
- Modify: `docs/superpowers/plans/2026-07-20-post-smoke-roadmap.md` (mark Round 5.6 built)

- [ ] **Step 1: Write the live test**

Append to the `mod tests` block in `crates/dml-wow/src/soap_autosetup.rs`:

```rust
    /// LIVE. The only test that can prove the whole chain.
    ///
    /// Every offline test above passes just as happily on an SRP6 verifier the
    /// server will reject: wrong endianness, a missed uppercase or an unpadded
    /// value all produce 32 self-consistent bytes and a well-formed row. The
    /// only oracle is a real login.
    ///
    /// ```text
    /// cargo test -p dml-wow --lib soap_autosetup::tests::live_ -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "writes to a live acore_auth and needs the worldserver up"]
    fn live_autosetup_creates_an_account_that_really_authenticates() {
        use crate::db::{self, Database, DbConfig};

        let cfg = DbConfig::from_env();
        let pass = generate_password();
        let user = format!("dmlauto{}", &random_hex6()[..4]);
        eprintln!("creating {user} / {pass}");

        crate::account_write::create_gm_account(&cfg, &user, &pass).expect("create");

        let soap = crate::soap::SoapConfig {
            url: crate::soap::SoapConfig::load().url,
            user: user.clone(),
            pass: pass.clone(),
        };
        let outcome = crate::soap::exec(&soap, "server info");
        eprintln!("soap said: {outcome:?}");

        // Tidy up BEFORE asserting, so a failure leaves nothing on the server.
        let _ = db::execute(
            &cfg,
            Database::Auth,
            "DELETE FROM account_access WHERE id IN (SELECT id FROM account WHERE username = ?)",
            vec![mysql::Value::from(user.to_ascii_uppercase())],
        );
        let _ = db::execute(
            &cfg,
            Database::Auth,
            "DELETE FROM account WHERE username = ?",
            vec![mysql::Value::from(user.to_ascii_uppercase())],
        );

        assert!(
            matches!(outcome, crate::soap::SoapOutcome::Ok(_)),
            "a GENERATED password must authenticate: {outcome:?}"
        );
    }
```

- [ ] **Step 2: Run the full offline sweep**

Run these one at a time — **never bats and the cargo parity suites concurrently** (bats rewrites `cli/dml` in place while the parity suites spawn it as their oracle):

```bash
cargo test --workspace
```
Expected: PASS. Baseline was 1427 passed / 0 failed; expect ~1444.

```bash
cd launcher && npm test && npm run check
```
Expected: tests PASS (~623), check 0/0.

Judge bats by exit code, never by a piped tail — `bats tests/ | tail -40` reports `tail`'s status and has already cost this repo a false all-green:

```bash
wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bats tests/ > /tmp/bats.out 2>&1; echo EXIT=\$?; grep -c '^not ok' /tmp/bats.out"
```
Expected: `EXIT=0`, `0` failures, 813 tests. (This plan touches no bash; a change here means something unrelated broke.)

- [ ] **Step 3: Run the live gate**

Requires the snapshot server up. From the repo root:

```powershell
$env:DML_GAMES_DIR="C:\Users\perzi\dml-native"
$env:DML_YQ_BIN="C:\Users\perzi\dml-native\tools\yq.exe"
target\debug\dml-wow.exe start
cargo test -p dml-wow --lib soap_autosetup::tests::live_ -- --ignored --nocapture
```
Expected: PASS, and the printed `soap said:` line is `Ok(...)`.

If it fails with `SoapOutcome::Auth`, the SRP6 is wrong — that is a `srp6.rs` bug, not an autosetup one; run `srp6::tests::live_our_verifier_matches_the_one_azerothcore_wrote` to localise it.

- [ ] **Step 4: Update CLAUDE.md**

In the `crates/` section, immediately after the `soap_bootstrap.rs` bullet, add:

```markdown
- **`soap_autosetup.rs` — the account creates itself (2026-08-01).** The guided
  step is gone: the launcher generates the password, writes the account and
  proves it, and the user never learns the step existed unless it fails. Fires
  off the status poll the shell already runs, on `soap.reachable && auth_ok ===
  false` only — so the migrated `dml-native` server self-heals too, not just
  fresh installs. Three rules are load-bearing. **The password is 16 characters**
  because `valid_account_pass` caps there and `create_gm_account` runs it first;
  a "stronger" 32 would `BAD_ARG` on every fresh install. **Rejection sampling,
  not `byte % 70`** — `256 = 3×70 + 46`, so plain modulo hands the first 46
  symbols a fourth chance the other 24 never get. And **`Pending` is why it is a
  state machine**: a create that succeeds followed by a verify that fails must
  not leave the latch open, or the poll writes one account per tick forever;
  `Pending` carries the credential forward and re-verifies, never re-creates. A
  taken name becomes `dmlsoap_<hex>` — never an overwrite, because
  `account_write` refuses to touch a row it did not write. Latched to ONE attempt
  per launcher run (`AppState.soap_autosetup`), so a mid-session auth wipe needs
  a relaunch to self-heal — the deliberate alternative to a loop. `Library.svelte`
  no longer carries any SOAP surface (pinned by `soap-surface.test.ts`, which
  strips comments before matching); the banner and the fallback card both live in
  the shell. No bash mirror, like `srp6`/`account_write`.
```

- [ ] **Step 5: Mark the roadmap entry built**

In `docs/superpowers/plans/2026-07-20-post-smoke-roadmap.md`, change the Round 5.6 heading line to:

```markdown
## Round 5.6 — Fully automatic SOAP account setup (user request, 2026-08-01) — BUILT 2026-08-01
```

and change `Design approved by the user the same day it was asked for. Not yet implemented.` to:

```markdown
Design approved and built the same day it was asked for. Plan:
`docs/superpowers/plans/2026-08-01-soap-account-autosetup.md`. Remaining gate:
the user's own click-through — install or start a server whose SOAP is refusing,
and confirm the banner appears and GM Tools works without typing anything.
```

- [ ] **Step 6: Commit**

```bash
git add crates/dml-wow/src/soap_autosetup.rs CLAUDE.md docs/superpowers/plans/2026-07-20-post-smoke-roadmap.md
git commit -m "test(soap): prove a GENERATED password can actually log in

Every offline test passes just as happily on a verifier the server will
reject -- wrong endianness, a missed uppercase, an unpadded value all
produce 32 self-consistent bytes. A real login is the only oracle, and the
generated password is a new input to a path that was only ever proven with a
hand-typed one.

Cleanup runs before the assertion so a failure leaves nothing behind on the
user's server.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

## Remaining user gate

Not automatable, and it is the only thing between this and done:

1. Start a server whose SOAP is refusing (a fresh `install-native`, or delete `~/.dml/soap.env` and restart the launcher against `dml-native`).
2. Wait for the world to come up.
3. Confirm the green banner appears **on whatever page you happen to be on**, naming `dmlsoap`.
4. Open GM Tools and run something — revive or heal on an online character.
5. Confirm Home → health panel → **Show account** prints the account and a 16-character password.

Nothing was typed at any point. That is the pass condition.
