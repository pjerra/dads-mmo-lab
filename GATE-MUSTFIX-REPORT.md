# Gate MUST-FIX wave — V1–V5

**Branch:** `worktree-agent-add03411480688cf0`
**Base:** `feat/arch-wsl-backend` @ `76dc264`
**Status:** all five closed; every recorded mutation re-run and now RED.

## Worktree correction, before anything else

The worktree was seeded from the **wrong base**. `git branch --show-current` gave
`worktree-agent-add03411480688cf0`, but HEAD was `a624a38` — byte-identical to
`origin/main`, a docs/installer commit line with no `crates/`, no `launcher/`
and no `Cargo.toml`. `git merge-base HEAD feat/arch-wsl-backend` was `a624a38`
itself and `git log feat/arch-wsl-backend..HEAD` was empty, i.e. the branch had
**zero unique commits** and the move to `76dc264` was a pure fast-forward with
nothing to lose. Corrected with `git reset --hard 76dc264`.

The sibling worktree `agent-acffab9c8b6e84b39` was listed at the same wrong
commit. **The controller should check it before trusting its output.**

Baselines then reproduced the gate log exactly: dml-core 285, launcher 202,
vitest 752/62 files, svelte-check 331 files / 0 errors.

## Final numbers

| Suite | Before | After |
|---|---|---|
| `cargo test -p dml-core` | 285 passed / 0 failed / 1 ignored | **289 passed / 0 failed / 1 ignored** |
| `cargo test -p launcher` | 202 passed / 0 failed / 1 ignored | **206 passed / 0 failed / 1 ignored** |
| `cargo build --workspace` | OK | **OK** |
| `npm test` | 752 passed / 62 files | **757 passed / 62 files** |
| `npm run check` | 331 files, 0 errors | **331 files, 0 errors, 0 warnings** |

`cargo test --workspace` deliberately not run (wedging pre-existing test owned
by a sibling agent).

---

## V1 — `backend_str` had no test at all

**Commit `d22e9e3`.** Extracted `backend_env_value(Backend) -> &'static str` and
added `every_backend_round_trips_through_the_value_we_export`, asserting
`from_override(Some(backend_env_value(b))) == b` for every variant plus pairwise
distinctness of the three strings. The round trip alone is satisfiable by a
collapse on *both* sides at once; distinctness is not.

Rewrote the inviting comment at `lib.rs` `backend_mode()`. "Arch and Wsl name
the same distro and the same daemon" is true *of that function* (its value feeds
a two-member frontend union) and was read as a licence to dedup the two sites.
It now says the collapse is local and names `startup.rs::backend_env_value` as
the site it must not be copied to.

- Log's GREEN: `Backend::Arch => "wsl"` → launcher **202 passed; 0 failed**, dml-core **285 passed; 0 failed**.
- My RED: `test result: FAILED. 202 passed; 1 failed` —
  `startup.rs:281 assertion left == right failed: we export Arch as "wsl", but backend::selected() reads that back as Wsl — a user who asked for Arch would silently get the other backend`.

## V2 — the bounded drain's WIRING was unpinned

**Commits `efec48a`, `29fdb4f`.** The bound is a property of the wiring between
the poll loop and `collect_by`, each of which is correct in isolation and
already tested. Extracted `bounded_outcome_after_spawn`, generic over a new
`Pollable` trait — the same role `Abandonable` already plays in this file: a
test seam, not an abstraction. A fake child that exits at once plus two
`NeverEnds` readers reproduces "child gone, grandchild still holding the pipes"
deterministically.

The pin brackets elapsed time from **both** sides: `>= timeout` (a shortened or
already-expired deadline returns early) and `< patience` (a fabricated,
re-derived or absent one returns late). Only the deadline the bound was computed
from satisfies both.

Runs off-thread behind a `recv_timeout`, deliberately: every mutation this
catches turns the drain into a long block, and an inline test would **hang**
rather than fail — the exact signature that let FIX 4's production half go
missing without a red, because `cargo test` never completes and nobody sees a
result at all.

- Log's GREEN: fabricated `Instant::now()+3600s` on both success-path drains → `proc:: setup:: engine::` **154 passed; 0 failed**.
- My RED: **`test result: FAILED. 155 passed; 2 failed ... finished in 10.02s`** — `the_drain_is_bounded_by_the_same_clock_the_poll_loop_used` ("did not return within 10s for a 300ms bound") and `a_single_unfinished_pipe_still_ends_at_the_deadline`. Fails, does not hang.

**A decorative claim of my own, found and closed.** The first version's doc
asserted it caught the natural typo `collect_by(Instant::now() + timeout)`. It
did not — against a child that exits at once the poll loop spends nothing, so
the two spellings are indistinguishable. Added `ExitsAfter` (a child living for
90% of the bound) and `the_drain_does_not_re_derive_the_clock_the_poll_loop_already_spent`.
Measured rather than predicted: honest **1.00s**, typo **2.93s** — worse than
the "doubling" the comment guessed, because the two pipes drain in *sequence*
and each buys a fresh full timeout. Comment corrected to the measured number.
Mutation RED: `took 2.9199922s for a 1s bound after the poll loop had already spent 900ms of it`.

Also covers one slow pipe in **each** position: the drain collects stdout then
stderr sequentially, so a bound applied to only the first leaves the second
unbounded.

## V3 — `distro_unprepared` had zero coverage

**Commit `ccd7bbf`.** Added the state — but the root cause is the defect class,
not the missing row. `cases` was a hand-written array parallel to the
`SetupState` union and had drifted **three** states behind it
(`distro_unprepared`, `no_docker`, `docker_stopped`). Replaced with
`Record<SetupState, BackendStatusReport>`, plus a count assertion as the belt
for vitest (which transpiles without typechecking). `NO_SCREEN` names the two
states that deliberately render nothing and is **not** an escape hatch: each is
asserted to really render null, so moving a state there to dodge the screen
rules fails instead of passing.

- Log's GREEN: gut the screen to `title:"Setup failed"`, `body:""` → **62 passed (62)** / **752 passed (752)**; `distro_unprepared` absent from the file entirely.
- My RED: `Tests 2 failed | 755 passed (757)` — `expected '' to be truthy` and `expected 'Setup failed ' not to match /\b(error|failed|failure|fatal)\b/i`. Both invariants fire.
- **Durability proven, not asserted.** Adding `| "future_state_nobody_added_to_the_fixture"` to `SetupState` → `npm run check` errors *in the test file*: `ERROR "src\lib\first-run.test.ts" 582:9 "Property 'future_state_nobody_added_to_the_fixture' is missing ... but required in type 'Record<SetupState, BackendStatusReport>'."` A future state can no longer slip past this fixture.

## V4 — `DML_BACKEND=auto` was nullified by the export

**Commit `115c513`.** Composed behaviour, traced end to end: `resolve` answers
`Native` on a fresh Docker Desktop PC; `value_to_export` declines to write it
because the env is non-empty; `selected()` reads the surviving `auto`;
`from_override`'s catch-all yields **`Wsl`** — a distro that does not exist. And
`backend_was_user_set()` was true, so Settings called the dropdown env-locked
and the user could not repair it in the UI.

`backend_env_pins()` states the rule once (`auto` is an instruction, not a
choice). `backend_value_to_export()` composes resolve → export → selected in one
place **production calls**, so the test is not a parallel model of it.

`BACKEND_WAS_USER_SET` → `BACKEND_PINNED_BY_ENV` was required for coherence, not
tidiness: once the export writes a concrete value over `auto`, a still-true "was
set" would make `launcher_config_read` read our own resolved `"native"` back out
and attribute it to the user — trading one false statement for another. The FILE
arm already read `auto` as "detect"; only the ENV arm read the same word as a
lock.

- Log's GREEN: delete `resolve`'s env-`auto` case → exactly 1 dml-core red and **ZERO launcher reds**.
- My RED (same mutation): launcher `FAILED. 205 passed; 1 failed` —
  `startup.rs:400 DML_BACKEND=auto on a Docker-Desktop PC with no distro must DETECT Native, not fall through from_override's catch-all to Wsl`. dml-core still reddens `resolve_auto_means_detect_in_both_places` (284 passed; 1 failed). The composition is now covered on both sides.
- **Second mutation, reverting my own export half:** `FAILED. 203 passed; 3 failed`, including `env=Some("auto") file=None dir=true docker=true distro=Yes: resolve chose Native but the exported value reads back as something else`. Both halves of the composition are pinned independently.

## V5 — two untrue statements in `CLAUDE.md:62-63`

**Commit below.** Both verified against source before editing, not taken on trust.

1. **"Nothing in `launcher/` calls any of it"** — false. `provision.rs:243`
   (`sources: vec![payload::DML_WOW_BIN.to_string()]`, label "Installing the
   Arch backend", `MODE_EXEC`, `Dest::File(DEST_DML_WOW)`) is a live production
   install step. Narrowed to name `probe_arch_with`/`derive_arch`/`dml_core::distro`
   and to state the exception exactly: the ELF is **deployed** at 0755 and then
   **never invoked**.
2. **"(user ruling, 2026-08-04)"** — false, and the most costly kind. The
   user's own approved spec says the opposite twice: decision 2 retires the bash
   CLI as a runtime path, and the architecture section has `DML_BACKEND=wsl`
   **resolve to `Arch`** with `Backend::Arch` "becomes what detection picks"
   (`docs/superpowers/specs/2026-08-04-arch-wsl-backend-design.md:60,87-91`).
   Replaced with the real provenance (deferred by the whole-branch review), the
   note that it **owes the user a re-ratification rather than being a ruling to
   cite back at him**, and the scoped flip location
   (`docs/superpowers/plans/2026-08-04-arch-wsl-backend.md:1988`).

Also corrected one adjacent sentence that **my own V4 change** made untrue:
"exports only what is UNSET" now carries the `DML_BACKEND=auto` exception.

### Same false claims still live OUTSIDE my scope — for the controller to route

I did not edit these; they are other agents' files or other items on the ranked
list, and silently editing them risks conflicting with a sibling.

- `crates/dml-core/src/backend.rs:13` — module doc: "WHY ARCH IS NOT THE DEFAULT YET **(user ruling, 2026-08-04)**".
- `crates/dml-core/src/backend.rs:169` — test doc: "**Ruled by the user 2026-08-04**".
- `docs/ROADMAP-TO-BETA.md:423` — "**THIS RETIRES `dml-arch`**" still asserted unqualified, ~350 lines below the amendment that reverses it (verdict item #12).

The roadmap's amendment (lines 72–96) is already accurate and was the model I
followed for the dormancy wording.

---

## Process note — I hit the exact trap the brief warned about

While vacuity-checking V2 I ran `git checkout -- crates/dml-core/src/proc.rs` to
revert a mutation while that file **also** held an uncommitted new test
(`ExitsAfter` + the re-derivation test). Both were destroyed. Confirmed with
`grep -c` returning 0, then re-authored and committed *before* re-running the
mutation.

The rule as written ("commit, then mutate, then checkout") is correct; what
caught me is that it also applies to work added *after* the commit that closed
the item — a second edit inside an already-committed item silently re-enters the
danger state. Worth recording in the ledger alongside the wave's own
`git status --short` lesson: they are the same class.
