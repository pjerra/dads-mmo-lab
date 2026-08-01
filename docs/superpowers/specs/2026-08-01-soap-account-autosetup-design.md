# Automatic SOAP account setup

**Date:** 2026-08-01
**Status:** BUILT 2026-08-01, plus two adversarial fix waves. This document has
been rewritten to describe the code as it stands — where the build diverged from
the original design, the divergence is what is written here and the reason is
named. One USER gate remains: the live click-through (see "Testing").
**Scope:** native backend, launcher only. No bash mirror (see "Surfaces").

## The problem

A freshly installed AzerothCore has no accounts. Until one exists at GM level 3,
every SOAP-backed feature in the launcher — GM Tools, My Party, the console's
send box, announcements, the server-info tiles — fails with a bare `SOAP_AUTH`
that names nothing the user did wrong.

The manual worldserver-console step was removed earlier today:
`crates/dml-wow/src/srp6.rs` and `crates/dml-wow/src/account_write.rs` write the
account row directly, and `wow_soap_account_create` wires a one-click button into
`SoapBootstrap.svelte`. That is the third sanctioned MySQL write, user-approved
2026-08-01.

What remains is still three manual acts at the end of a multi-hour install:

1. The card renders **only inside `Library.svelte`**. `soapSetupState` is
   module-level so the *flag* survives navigation, but the *card* does not — a
   user who is on Home has no way to know the step exists.
2. The user must invent and type a password.
3. The user must click **Create the account**.

This design removes all three.

## Decisions taken

| Question | Decision |
|---|---|
| Where does the password come from? | The launcher generates a random one. |
| When does it fire? | Any time SOAP is *reachable and rejecting us*, from the existing status poll — not only at the end of an install. |
| How visible is it? | Silent, with a dismissible one-line result banner. |
| If the name is taken? | Create `dmlsoap_<random>` instead — but only if no `dmlsoap_*` exists yet; otherwise stop. Never touch an account we did not create. |

## Architecture

### New module: `crates/dml-wow/src/soap_autosetup.rs`

Pure helpers plus one orchestrator with injected seams, so every branch is
testable with no server and no database.

#### Password generation

```rust
pub const PASSWORD_LEN: usize = 16;
/// Exactly `valid_account_pass`'s charset: 26 + 26 + 10 + 8 = 70 symbols.
pub const PASSWORD_ALPHABET: &[u8] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_@#%+=!-";
pub fn generate_password_from(fill: impl FnMut(&mut [u8])) -> String;
pub fn generate_password() -> String;   // the same, fed by `getrandom`
```

The byte source is a seam because the discard rule cannot be proven against a
real RNG: "these 16 characters look fine" is true of a biased generator too.

**16 characters, not 32.** `soap_cmds::valid_account_pass` enforces `{4,16}` and
that limit is AzerothCore's, not ours — a longer password would be refused by
the very validator `create_gm_account` runs first. 16 characters over a 70-symbol
alphabet is ≈98 bits, which is far past anything that matters here.

The alphabet is exactly `valid_account_pass`'s charset
(`[A-Za-z0-9_@#%+=!-]`). **Rejection sampling**, not `byte % 70`: 256 is not a
multiple of 70 (`256 = 3×70 + 46`), so plain modulo gives the first 46 symbols a
fourth chance the rest never get. A credential is the wrong place to accept a
bias nobody will ever notice. Draw a byte, discard anything ≥ 210, then index.
Randomness comes from `getrandom`, already
a dependency for `srp6::random_salt`, for the same reason stated there — a
predictable salt or password would let one leaked verifier be attacked across
every DML install at once.

#### Name selection

```rust
pub fn fallback_user(rand_hex6: &str) -> String;  // "dmlsoap_ab12ef"
pub fn fallback_prefix() -> String;               // "dmlsoap_"
```

`dmlsoap` first. If taken, one retry as `dmlsoap_<6 lowercase hex>` — 14
characters, inside `valid_account_user`'s 20-character limit, charset
`[A-Za-z0-9_]`. If *that* is taken too, give up; the odds say something is wrong
that another random name will not fix.

**Before minting, ask whether the FAMILY is taken** — `account_family_exists`,
a `LIKE 'DMLSOAP\_%' ESCAPE '\'` against the upper-cased name AzerothCore
stores. This is the only thing that bounds the account count, and the original
design did not have it (fix wave 2). Checking the minted name alone bounds
nothing: `dmlsoap_<fresh random hex>` is free *by construction*, so that lookup
can only ever answer "free". The unbounded case is ordinary rather than exotic —
`SoapConfig::load` gives `DML_SOAP_USER`/`DML_SOAP_PASS` precedence over the
`~/.dml/soap.env` this feature writes, so with those exported the status stays
`Rejected` no matter how many accounts exist and **every launcher start inserted
one more GM-level-3 account, forever**. The prefix deliberately excludes
`dmlsoap` itself: that name being taken is the whole reason a fallback is being
considered, so a pattern matching it too would refuse every install that already
has one.

The existing `create_gm_account` refuses on collision rather than overwriting,
and that refusal stays exactly as it is. This module handles the collision by
choosing a different name, never by resetting someone's password.

#### The state machine

```
Idle ──create + verify──▶ Done{Saved}
  │                          ▲
  └── verify failed ──▶ Pending{user, pass, tries} ──┘
                             │ tries == 3
                             ▼
                        Done{GaveUp} ──▶ the existing manual card
```

```rust
pub enum AutoSetup {
    Idle,
    Pending { user: String, pass: String, tries: u8 },
    Done(Conclusion),
}
pub enum Conclusion { Saved { user: String }, GaveUp { reason: String } }

pub const MAX_VERIFY_TRIES: u8 = 3;
```

**`Pending` is the whole reason this is a state machine and not a function.**
Without it, a create that succeeded followed by a verify that failed would leave
the latch open, and the next poll would create a *second* account — one row per
poll, forever. `Pending` says: the account exists and we know its password, so
re-verify it; never create again.

Verify can fail after a successful create for exactly one interesting reason
(the SRP6 produced a well-formed verifier the server rejects) and one boring one
(the world server went away between the two calls). Three tries distinguishes
them without spinning.

#### The orchestrator

```rust
pub fn advance_with(
    state: AutoSetup,
    status: &soap_bootstrap::VerifyOutcome,
    exists: impl Fn(&str) -> Result<bool, CmdError>,
    family_taken: impl Fn(&str) -> Result<bool, CmdError>,
    create: impl Fn(&str, &str) -> Result<i64, CmdError>,
    verify: impl Fn(&str, &str) -> Result<VerifyOutcome, CmdError>,
    hex6: impl Fn() -> String,
    gen_pass: impl Fn() -> String,
) -> (AutoSetup, AutoOutcome);
```

Pure transition given its seams. `AutoOutcome` is what the UI is told:
`NotNeeded | Created{user} | Pending | GaveUp{reason}`.

`family_taken` is the seam added by fix wave 2, and it is the only bound on how
many accounts this feature can create — see "Name selection": the old code
checked the freshly minted `dmlsoap_<hex>`, which is free by construction and is
therefore not a guard at all, so with `DML_SOAP_USER`/`DML_SOAP_PASS` exported
every launcher start minted another GM3 account, unbounded. `gen_pass` is a seam
for the same reason the others are: a test that asserts the generated password
reaches *both* the create and the verify call unchanged needs to know what that
password was, and a verify run against a different string passes offline and
fails on a real server.

Rule, asserted by a test whose `create` seam panics if called: a status that is
not `Rejected` performs **zero** writes and zero DB reads. Each of the three
lookups can also FAIL rather than answer — a stopped MySQL container is the
ordinary case while a stack is still starting — and all three end the run with
the database's own words, writing nothing. Guessing "free" would send an INSERT
down a dead connection; guessing "taken" would abandon setup on a healthy
server.

### Latch: `AppState`

`Mutex<AutoSetup>`, one per launcher run. After `Done`, every later call skips
the work — no SOAP call, no DB connection — but it still **answers**, by
re-deriving the verdict from the stored `Conclusion` through
`concluded_outcome()`: `Saved{user}` → `Created{user}`, `GaveUp{reason}` →
`GaveUp{reason}`.

**There is no contentless "already done" outcome, and that is fix wave 2.** One
existed (`AutoOutcome::Latched`, wire literal `"latched"`) and it cost the manual
fallback card. The frontend's record of how the run went lives in a module-level
store; a webview reload wipes it; a reloaded UI that asks again and hears only
"already concluded" learns nothing — so the card never rendered again for the
rest of the process, **on the exact path where the launcher had failed to create
the account and the card was the only thing left that worked**. Both
"already concluded" guards — the Tauri command's cheap exit before it touches
the server, and `advance_with`'s own — answer through `concluded_outcome`, so
they cannot drift into telling the frontend two different things about one run.

**Known limit, deliberate:** wiping the auth database mid-session needs a
launcher restart to self-heal. The alternative is an unlatched loop that writes
rows into a database that keeps losing them, and this project already has one
recorded incident of an automatic path that looked healthy while doing the wrong
thing.

### New command: `wow_soap_autosetup`

`launcher/src-tauri/src/lib.rs`, alongside the existing `wow_soap_*` commands.

1. Take the latch. `Done` → return `concluded_outcome(c)` without asking the
   server anything.
2. `soap_bootstrap::soap_status_with(...)` under `state.soap_lock`, exactly as
   `wow_soap_status` does. Not `Rejected` → return `NotNeeded`, latch untouched.
3. Drive `advance_with`, whose seams are:
   - `exists` → `account_write::account_exists`
   - `family_taken` → `account_write::account_family_exists`
   - `create` → `account_write::create_gm_account`
   - `verify` → `soap_bootstrap::bootstrap_verify_with`, which is what actually
     writes `~/.dml/soap.env` — **and only after a real round-trip succeeds**.
     That ordering is not re-implemented here; reusing it means there is one
     definition of "done" rather than two that can disagree.
4. Store the new state, return the outcome.

All blocking work goes through `spawn_blocking` and takes `state.soap_lock` for
SOAP, matching every other native SOAP call — the worldserver's SOAP listener
runs on the single world thread.

Like `wow_soap_account_create`, it returns `Ok` with a verdict for an unhappy
server. `Err` is reserved for a malformed request or a disk that would not take
the file.

### New command: `wow_soap_credentials`

Read-only, no write path. Takes `reveal: bool` and returns
`{ user, url, pass, configured }`.

It reports `SoapConfig::load()` — the same resolver every other SOAP call uses
(env → `~/.dml/soap.env` → compiled-in default), **not** a parse of `soap.env`
alone. That matters because the resolver never returns nulls: with nothing
configured it hands back `admin`/`admin`, and a panel that printed those as
"your credentials" would invent a pair that has never worked on a fresh install.

`configured` is the flag that tells the two apart, and only the resolver can
answer it: true when the **user or the password** was supplied by `DML_SOAP_USER`
/`DML_SOAP_PASS` or by `soap.env`, false when both fell through to the
compiled-in default (`SoapConfig::load_with_provenance`). A `DML_SOAP_URL` on its
own does **not** count — pointing the launcher at a different host says nothing
about whether an account exists there, and reporting "configured" while the
credentials are still `admin`/`admin` shows exactly the invented pair this
boolean exists to suppress. An empty value is not a supplied one, for the same
reason it is not a winning one.

The UI gates its "nothing saved yet" message on that field **alone**. It was a
string comparison against `admin`/`admin` until fix wave 2, which is a different
question that agrees most of the time and then fails at both ends: it cannot
tell an install with no account at all from one whose SOAP account is genuinely
named `admin`, so it either invented an account for a fresh install or denied a
real one.

`pass` is `null` unless `reveal` is true. Opt-in at the boundary rather than
filtered in the UI: a secret that always crosses the IPC boundary is a secret in
every devtools trace, whether or not a control is showing it.

### Trigger: the existing poll, no new one

`launcher/src/lib/server-status.svelte.ts`'s `refreshServerStatus()` already
fetches `ServerDetail`, which already carries `soap.reachable` and
`soap.auth_ok` — Home renders both today ("authentication failing, check
~/.dml/soap.env").

When `detail.soap.reachable && detail.soap.auth_ok === false`, call
`wowSoapAutosetup()` behind a module-level single-flight guard (the same shape
`refreshServerStatus` already uses for itself).

**The poll is a cheap trigger, not the verdict.** Rust re-derives the status
with `soap_bootstrap::classify`, the classifier that is already tested and
already knows that `Fault` is not an auth failure and that `Unreachable` says
nothing about the password. A false trigger therefore costs one `server info`
and nothing else — and the authoritative decision cannot drift from the UI's
rendering of it.

Never fires on an unreachable server. A world server that has not finished
booting is not a broken account, and the existing `needs_bootstrap` already
encodes that.

### Frontend

**`launcher/src/lib/soap-setup-state.svelte.ts`** gains:

```ts
export const soapSetupState = $state({
  needed: false,                      // now set ONLY on gave_up
  autoResult: null as { user: string } | null,
  gaveUpReason: null as string | null,
});
export function applyAutosetupOutcome(v: SoapAutosetupVerdict): boolean;
```

`needed` flipping only on `GaveUp` is the behavioural change: the manual card
stops being the default outcome and becomes the fallback. `gaveUpReason` renders
above it, because "the launcher could not do this for you" without the server's
own words is a card the user cannot act on.

`applyAutosetupOutcome` returns whether the run is SETTLED — that boolean is
what stops the poll asking. It has arms for `created` and `gave_up` only;
anything else, including a status from a build that is not this one, settles
nothing and shows nothing (the same rule the `TermEvent` union follows). There
is deliberately no "already ran" arm to add: the backend re-derives the real
verdict instead, so a reloaded webview rebuilds its UI from the second answer
exactly as it would from the first.

**`launcher/src/routes/+page.svelte`** renders a slim dismissible banner beside
the pending-restart one:

> Server access set up automatically as **dmlsoap**. ✕

Shell-level, so it is visible on whatever page the user happens to be on when a
multi-hour install finishes. This is the part that fixes problem (1) — the
current card is unreachable from anywhere but Library.

**`SoapBootstrap.svelte`**'s internals are unchanged, but it **moves out of
`Library.svelte` and into the shell**, next to the banner. It renders when
`needed` is true, which now means autosetup gave up, and its manual console
instructions remain the honest fallback for a schema this build does not
understand. A fallback that is only reachable from one page is the same bug as
the one this design exists to remove — it must not survive in the failure path.

**`Library.svelte` loses the SOAP step entirely** (user instruction,
2026-08-01). Three deletions:

- `refreshSoapNeed()` and both its call sites — the `onMount` probe and the
  `onInstallExit(code === 0 && backendMode === "native")` hook.
- The `{#if soapSetupState.needed}<SoapBootstrap …>` mount and its import.
- `wowSoapStatus` from the page's imports.

The reasoning those call sites carry is not being discarded, it is being
satisfied better. `refreshSoapNeed`'s comment argues that the step must ASK
whether SOAP works rather than remember that an install finished, because the
event-driven flag dies when the launcher restarts and a leftover `soap.env` from
a different server makes a file check lie. Autosetup keeps that rule exactly —
it asks `soap_status_with` every time — and drops the part that made it a
Library-page concern. `onInstallExit`'s worry about a world server whose SOAP
port has not opened yet is likewise preserved: `Unreachable` is `NotNeeded`, and
the poll simply tries again on the next tick instead of needing a remount to
re-probe.

Net effect on the install flow: an install that ends at `ready` now ends. There
is no post-install account card, because by the time the user looks the account
exists.

**`Home.svelte`** health panel: next to the existing SOAP row, show the account
name and a **Show account** toggle backed by `wow_soap_credentials`. Collapsed
by default, and the backend is called only from the click handler — a credential
fetched on every poll is a credential in every trace of every poll. This is the
answer to "the app knows a credential I do not" — it knows it, and it will tell
you where to look. When `configured` is false it says so ("no credentials saved
yet — the launcher sets this up automatically") rather than printing the
resolver's `admin`/`admin` fallback as if it were an account.

### Surfaces

**No bash mirror.** `srp6.rs` and `account_write.rs` have none — they are native
only, like `install_native.rs`, because bash's `_installers_supported` refuses on
Windows and the direct-write route was built for the native backend. Autosetup is
launcher behaviour on top of them. Recorded here explicitly so a later audit
reads this as a decision rather than as bash↔Rust drift.

## Error handling

| Situation | Behaviour |
|---|---|
| SOAP `Ok` | `NotNeeded`. No DB connection is opened. |
| SOAP `Unreachable` | `NotNeeded`. Never mistaken for bad credentials. |
| SOAP `Fault` | Treated as `Rejected` by the existing `classify`, which already explains that the server answered but refused the check. |
| `dmlsoap` taken, no `dmlsoap_*` yet | One retry as `dmlsoap_<hex>`. |
| `dmlsoap` taken and a `dmlsoap_*` already exists | `GaveUp` naming the family → manual card. This is the bound on the account count; without it, one GM3 account per launcher start. |
| Both `dmlsoap` and the minted name taken | `GaveUp` → manual card. |
| A lookup cannot answer (MySQL down, wrong `DML_DB_*`) | `GaveUp` carrying the database's own words. Nothing is written. |
| Insert fails (unknown schema) | `GaveUp`, carrying `create_gm_account`'s own hint about creating it by hand. |
| Insert ok, GM grant fails | `create_gm_account` already errors with the console command that finishes it. `GaveUp`. |
| Create ok, verify fails | `Pending`. Re-verify next poll, up to 3, then `GaveUp`. **Never a second account.** |
| No `$HOME` | `Err` from the command, as today. |

## Testing

**Rust unit (no server):**

- Every generated password satisfies `valid_account_pass` — 1000 samples.
- No generated password contains a character outside the alphabet (a `$` would
  be refused by the validator the caller runs first, i.e. a self-inflicted
  `BAD_ARG` on a fresh install).
- 1000 generated passwords are distinct.
- Rejection sampling, driven through `generate_password_from` with a **scripted
  byte feed**, asserting that no rejected byte reached the output. **The first
  version of this test was vacuous** and the codebase carries the warning in
  place: feeding the natural-looking `210..=255` proves nothing, because
  `210 == 3×70`, so under plain `byte % 70` those bytes map to indices `0..=15`
  — character for character the prefix the test expects. Deleting the discard
  guard left it green. The feed is 46 copies of `255` (`255 % 70 == 45`), which
  an unguarded run turns into 46 copies of one symbol and the test fails loudly.
- `fallback_user` output satisfies `valid_account_user`.
- A non-`Rejected` status performs zero writes — `create` seam panics if called.
- `Idle` + create-ok + verify-fail → `Pending`, and a second `advance_with` from
  `Pending` calls `verify` and **never** `create`. This is the test that would
  have caught one-account-per-poll.
- Three failed verifies → `GaveUp`, and a fourth call creates nothing while
  still repeating the same reason.
- A concluded run touches no seam and **still reports its verdict** — `Saved`
  comes back as `created`, `GaveUp` as the same `gave_up` reason. The old
  "`Done` → `Latched` without touching any seam" assertion is gone with the
  variant; it was satisfied by exactly the answer that broke the fallback card.
- Collision path: `exists("dmlsoap") == true` → `create` is called with a
  `dmlsoap_` name, never with `dmlsoap`.
- An existing `dmlsoap_*` account stops a second one being minted: `create` is
  never called, and the reason names the family.
- Each of the three lookups failing in turn → `GaveUp` with the DB's message and
  no write.

**Live proof.** Every offline check above passes just as happily on a verifier
the server will reject, so none of them is an oracle for "the account actually
works". Two things cover that, and only one of them is automated:

- `account_write::tests::live_a_written_account_can_actually_authenticate`
  (`#[ignore]`, run with `-- --ignored`) writes a real account against the live
  snapshot server, authenticates over real SOAP and deletes it again. That is
  the SRP6 half — the part a unit test cannot reach.
- **USER GATE, outstanding:** the end-to-end click-through. Point the launcher
  at a server whose SOAP is rejecting it, confirm the banner appears naming the
  account, that GM Tools/My Party start working without further input, and that
  Home's health panel reveals the password. No autosetup-level `#[ignore]` test
  was written for this: it would have to drive the Tauri command and its latch,
  and a passing version of it would still not prove the banner rendered.

**vitest:**

- The poll trigger fires once across several polls, not once per poll.
- `needed` is set only for `gave_up`; `created` sets `autoResult` and leaves
  `needed` false.
- The banner renders from the shell, so it is present on a non-Library page.
- **`Library.svelte` contains no SOAP surface at all** — `soap-surface.test.ts`
  scans it for `SoapBootstrap`, `soapSetupState`, `wowSoapStatus`,
  `refreshSoapNeed` and `clearSoapSetup`, and separately asserts the shell
  carries both the banner and the fallback card. Same shape as
  `feature-keys.test.ts`, including its lesson: it `code()`-strips comments
  before matching, because that scan shape bit this repo twice on 2026-08-01 by
  reading an explanation as the thing it explains, and `Library.svelte` is dense
  with `// … soap …` prose a raw grep would trip over. The stripper has its own
  non-vacuity test, and the shell's markers are MARKUP (`<SoapBootstrap`,
  `soapSetupState.autoResult`, `soapSetupState.needed`) rather than bare
  identifiers: bare ones are satisfied by the shell's two import lines alone, so
  the first version of the test stayed green with the entire banner block
  deleted — and `npm run check` would not have caught the orphaned imports
  either, since `tsconfig` sets no `noUnusedLocals`.

## Risks

1. **A silent GM3 INSERT on a server the user did not build.** Pointing the
   launcher at any AzerothCore whose SOAP rejects it now creates an admin
   account. Accepted, with guards: only on `Rejected`, never an overwrite, one
   attempt per launcher run, a refusal once any `dmlsoap_*` account exists, and
   the banner names the account out loud rather than doing it behind the user's
   back. Note which guard does which job — the latch bounds a single run, the
   family check is the only thing that bounds the total, and shipping without it
   meant one more GM3 account per launcher start.
2. **The password ceiling is 16 characters**, imposed by AzerothCore and by the
   existing validator. Stated here so a future reader does not "fix" it upward
   and produce a `BAD_ARG` on every fresh install.
3. **The latch needs a restart to re-arm.** Deliberate; see above.

## Out of scope

- Changing, resetting or deleting an existing account. `account_write` refuses
  to, and this does not add a route around it.
- A bash mirror.
- The WSL install route, whose installers walk the user through their own
  account step — `noteNativeInstallFinished`'s doc comment already records why
  raising this there would ask them to redo it.
