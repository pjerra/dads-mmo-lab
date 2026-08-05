# Whole-branch review — code findings F1–F8

**Branch:** `worktree-agent-a79cc4a90a9e96145`
**Base:** `33d560a docs: CLAUDE.md and the roadmap were lying about this branch's own work`
(the worktree was seeded from the wrong commit — `a624a38 Update Install-DML.ps1 …` — and was
`git reset --hard 33d560a` before any work; `crates/dml-core/src/vocab.rs` and
`launcher/src-tauri/src/wsl_keepalive.rs` both confirmed present afterwards.)

**Status: all 8 closed.** Every one has its mutation re-run after the commit and confirmed RED
for the right reason.

| SHA | Findings |
|---|---|
| `e4d0737` | F1 — keep-alive production wiring pinned |
| `df5f2a4` | F2 — Start recovers from a give-up · F5 — a give-up is announced once |
| `5721f5a` | F3 — positional-ORDER errors are no longer invisible |
| `6c11918` | F4 — `start_engine`'s bound is actually tested (+ a real bug found) |
| `75a666b` | F6 — `production_half` truncated · F7 — `soap.env` was unquoted shell source · F8 — env mutation in a parallel test binary |

**Test numbers**

| | baseline (`33d560a`) | final |
|---|---|---|
| `cargo test --workspace` | 1889 passed / 0 failed / 29 ignored | **1908 passed / 0 failed / 29 ignored** |
| `launcher --lib` | 234 | 245 |
| launcher vitest | 763 (63 files) | 763 (63 files) |
| `cargo build --workspace` | — | 0 warnings |

19 net new tests. No test was deleted or weakened.

> A note on the baseline: the very first `--workspace` run showed 2 failures in
> `dml-wow-cli --test cli_integration` (`DB_UNREACHABLE` / exit 1). Both are live-database
> tests and both passed on the immediately following run and every run since. A transient
> blip in the user's stack, not a regression, and not something I touched.

---

## F1 (Important) — the keep-alive's entire production wiring was untested

`wsl_keepalive`'s 18 unit tests each build their own `Keepalive` and drive it through a fake
spawner. They prove the decision thoroughly and say nothing whatsoever about `lib.rs`. With
`install()` no-op'd, `STATE` is never set, `state()` answers `None`, every `apply()` returns at
its first line and no holder is ever spawned — silently, which is the exact failure mode the
module exists to end.

**Fixed** — `launcher/src-tauri/src/lib.rs`, new `keepalive_wiring_tests` module, modelled on
`every_command_that_saves_soap_credentials_also_publishes_them_to_the_cli`:

- `every_wsl_keepalive_entry_point_has_a_production_call_site` — all six call sites
  (`install`, `server_should_run` ×2, `server_should_stop`, `observed_status`, `shutdown`)
  checked against the **brace-matched body** of their host function, per site rather than by a
  count. Two negatives are asserted too, so an over-broad `fn_body` cannot satisfy the loop.
- `the_keep_alive_is_armed_exactly_once`.
- `the_intent_is_declared_before_a_start_and_after_a_stop` — the documented ordering, asserted
  against the **real work calls in the real body** rather than a restated step list (the
  recorded `lifecycle_steps_for_mode` lesson). Load-bearing: a lifecycle command is itself a
  session into the distro, so the 15 s clock starts when it exits.

`production_half` became `pub(crate)` so both scans read one production text.

**Mutation → RED.** Reviewer's mutation (no-op `install`, delete the other four):

```
test result: FAILED. 234 passed; 3 failed; 2 ignored
  keepalive_wiring_tests::every_wsl_keepalive_entry_point_has_a_production_call_site
  keepalive_wiring_tests::the_intent_is_declared_before_a_start_and_after_a_stop
  keepalive_wiring_tests::the_keep_alive_is_armed_exactly_once

`run` does not call `wsl_keepalive::install(`. On Backend::Arch that is not a missing
nicety: WSL powers the distro off ~15s after the last session into it exits …
```

234 passed is exactly the number the reviewer measured, now with three guards on top.

**Second mutation → RED** (ordering only — move `server_should_run()` after the work in
`games_start`, leaving the call present):

```
games_start declares the keep-alive intent AFTER it starts working. The hold must exist
before this command's own wsl.exe session ends, or the server it just started dies ~15s later.
```

---

## F2 (Important) — after a give-up, pressing Start again could not recover

`want_running` cleared `gave_up` only when the intent CHANGED, and `games_start`/`games_restart`
call `server_should_run()` unconditionally — so by the time a user presses Start after a
failure the intent is already `Run` and the clear never fires. Five transient `wsl.exe`
failures latch it; `reconcile` then returns `Step::GaveUp` without going near the spawner and
Start silently does nothing for the rest of the session. The doc comment described the intended
behaviour correctly; the code disagreed with it.

**Fixed** — `wsl_keepalive.rs`. A fresh **ask** reopens the budget even when the intent is
already `Run`. The infinite-retry hazard the old gating was meant to prevent is still closed,
and closed where it belongs: adoption now goes through `assert_run(false)`, so the 7-second
`tray_set_status` poll can never reopen a give-up. Pinned in both directions —
`pressing_start_again_after_a_give_up_reaches_the_spawner` (asserts the **spawner was really
reached** and the holder is alive, not just that a flag flipped) and
`a_status_poll_never_reopens_a_give_up`.

Frontend: the banner now names the way out (`Press Start (or Restart) on the Home page to try
again.`), pinned by a vitest case. Reporting a dead keep-alive and its consequence but not the
one action that fixes it is how the broken recovery path went unnoticed.

**Mutation → RED** (restore the intent-CHANGE-only gate):

```
pressing_start_again_after_a_give_up_reaches_the_spawner
  a fresh ask must reopen the budget even though the intent was already Run
a_give_up_after_a_fresh_ask_is_announced_again
  left: 1  right: 2
```

**Mirror mutation → RED** (let a poll count as a fresh ask — the retry-loop hazard):

```
a_status_poll_never_reopens_a_give_up
  left: Failed("nope")  right: GaveUp
```

---

## F3 (Important) — a positional-ORDER error in `vocab.rs` was invisible

T1 used clap **parse success** as its oracle, and two same-typed positionals parse in either
order. Swapping `wow party kick`'s `pos` turns `--player Bob --bot Zug` into
`party kick Zug Bob` — kicking a bot named "Bob" from a player named "Zug" — with the whole
workspace green. ~19 rows were exposed, several of them mutating GM verbs.

**Fixed** — `crates/dml-wow-cli/tests/vocab_surface.rs`. The oracle stays the **clap derive**
rather than a hand-written argv table (hand-writing expected output is the failure mode this
file was built to avoid): clap knows every subcommand's positionals *and their index order*, so
each row's promotion order must equal the leaf subcommand's positional order, name for name.
Two tests — the table-wide guard (collecting every mismatch in one run) and
`party_kick_sends_the_player_first_and_the_bot_second`, which spells out the reviewer's case end
to end through a real parse.

Two things worth carrying forward:

- **`Command::build()` is mandatory**, and the code says so. Clap assigns positional indices
  during the build; on an unbuilt tree every `get_index()` is `None`, every list is empty and
  every prefix comparison passes at length zero — a vacuous pass that looks exactly like a
  clean one. Found while writing the test. The length floors are the second guard.
- **Seven genuine spelling divergences** surfaced, every one of them single-positional and
  therefore order-safe, which is precisely why nobody had noticed the vocabularies had drifted:
  `--char` cannot be `char` (Rust keyword), and `--entries`/`--file`/`--path` are bash's words.
  Each is named in `ALIASES` with its reason, keyed by `(verb, flag)` so an alias cannot leak
  into another row. **No row was in the wrong order today** — the bug was the absence of
  protection, not an existing swap.

**Mutation → RED** (the reviewer's exact one):

```
party_kick_sends_the_player_first_and_the_bot_second
  left: ["party", "kick", "Zug", "Bob"]  right: ["party", "kick", "Bob", "Zug"]

every_promoted_flag_lands_on_the_positional_that_means_the_same_thing
  ["wow","party","kick"] promotes ["--bot","--player"] -> ["bot","player"],
  but `dml-wow kick` fills ["player","bot"] IN THAT ORDER
```

**Second mutation → RED** (a mutating GM verb, to show the guard is broad):

```
["wow","gm","gold"] promotes ["--gold","--player"] -> ["gold","player"],
but `dml-wow gold` fills ["player","gold"] IN THAT ORDER
```

---

## F4 (Important) — `start_engine`'s bound was unprotected, and a second bug behind it

The test named after `start_engine` built its own `Command` and called
`proc::output_bounded_draining` directly, so it exercised the helper and never touched the
function. A test named after something it does not call is not a weak test, it is a decoration.

**Fixed** — `crates/dml-core/src/engine.rs`. The body moves to
`start_engine_with(program, args, timeout)`; `start_engine` becomes the one-line delegation and
the test calls the real code. **The args had to be part of the seam, not just the timeout**: the
production argv is `desktop start -d`, and every never-returning program available to a test
needs *its own* arguments to block (`ping -n 600 …`), so a test that could only choose the
program would be handed a child that exits immediately — a bounded-wait test satisfied by a
process that never waited. The delegation itself is pinned by a 4-line source scan, because the
one property it carries (the 60 s constant) cannot be shown by a fast test.

**A real bug fell out of the sibling.** `a_spawn_failure_is_never_mistaken_for_a_started_engine`
was a bare `is_err()`; asserting the error **kind** turned it red —
`output_bounded_draining` collapses a spawn failure and a deadline into the same `None`, so a
machine with **no docker CLI at all** was told *"docker desktop start did not answer"*, naming
the wrong thing and pointing at the wrong repair. It now goes through `run_bounded_outcome` and
reports `NotFound` as `NotFound`. The outcome was always correct (`start_engine_succeeded` is
false either way and the caller falls back to the exe), which is exactly why it could stay wrong
for as long as it liked — a probe whose failure mode is indistinguishable from its *other*
failure mode is still not a probe.

**Mutation → RED** (revert the body to a bare `cmd.output()`):

```
start_engine_is_bounded_rather_than_blocking_forever has been running for over 60 seconds
start_engine_is_bounded_rather_than_blocking_forever ... FAILED
  a process that never exits must time out, not return output: Output { status: ExitStatus(0),
  stdout: "\r\nPinging 127.0.0.1 with 32 bytes of data:\r\nReply from 127.0.0.1 …
```

It took ~10 minutes and returned the full `ping -n 600` transcript — which *is* the proof: the
unbounded call waited out the entire child. (Before the fix the same mutation was invisible.)

**Second mutation → RED** (break the delegation only):

```
start_engine's body does not name "docker_desktop_start_args()", so the bounded seam below
it is not what production reaches:
{
    start_engine_with(program, &["desktop", "start"], std::time::Duration::from_secs(3600))
```

---

## F5 (Minor) — `announce()` emitted a `wsl-keepalive` event every 5 s forever

The guard was `gave_up && attempts >= MAX_ATTEMPTS` read off the report, and its comment claimed
it *"fires exactly once per give-up"*. It did not: the latched arm never touches `attempts`, so
both halves stay true on every subsequent tick. Bounded only by a frontend string-equality check
in another file, in another language, which nothing pinned.

**Fixed** — the decision moves into `Keepalive::announcement`, a `&mut self` method latched next
to `gave_up` and cleared with the rest of the budget, so a give-up **after a fresh ask is news
again** (a latch that never reopens is the contentless "already told you" this repo has been
burned by before). `announce` is now a pure emitter, and the announcement is computed inside the
lock because computing it mutates the latch. `Step::Failed` is deliberately not latched — each
failed attempt is a distinct event and there are at most `MAX_ATTEMPTS` of them.

**Mutation → RED** (restore the old guard):

```
a_latched_give_up_is_announced_once_and_then_goes_quiet
  20 latched ticks produced 20 give-up events; the storm is the bug
  left: 20  right: 1
```

Exactly the "20 of 20 latched ticks would emit" the reviewer predicted.

---

## F6 (Minor) — `production_half` cut at the FIRST `#[cfg(test)]`

Nothing below line 7765 of `lib.rs` was ever scanned. Harmless the day it was found, and latent
in the worst direction: a real call site **appended to the end of the file** sails through
`every_launcher_call_site_is_classified`, while the identical function moved 2000 lines up fails
it loudly. A guard whose coverage depends on where in the file you type is not a guard.

**Fixed** — `strip_cfg_test` (already correct, 700 lines away in `startup.rs`) moves next to
`strip_comments` so there is **one** answer to "what is the production half"; `startup.rs`'s
five call sites point at it. Two tests: a production call **between** two test modules must
survive, and the `#[cfg(test)] use …;` form must be cut at its semicolon rather than at a brace
it does not have.

**Mutation → RED** (restore the truncating version):

```
production_half_removes_test_modules_rather_than_truncating_at_the_first
  left: ["wow before"]  right: ["wow before", "wow after"]
  `wow after` missing means the scan still truncates
a_cfg_test_use_statement_is_cut_at_its_semicolon
  left: []  right: ["wow survivor"]
```

---

## F7 (Minor) — `soap.env` values were written unquoted into a file bash `.`-sources

`_soap_load_env` (`cli/src/20-soap.sh:23`) runs `. "$f"`, so an unquoted value is shell
**source**: `$(…)`, backticks, `$VAR`, globs. Not exploitable today — `user`/`pass` go through
`validate()` and `url` has never been user-supplied — but `url` has **no** validator and this
file travels into `dml-arch`, where `dml` holds NOPASSWD sudo. "Safe because of what someone
else checks" is not a property of the writer.

**Fixed on both ends.** All three values are single-quoted, with a literal `'` written in the
standard `'\''` form — **and `soap::parse_soap_env` undoes that escape**, because a writer that
escaped against a reader that did not would deliver a password four characters too long and
report it as an authentication failure with nothing pointing at the file.

Verified empirically against real bash (no distro, no server contact):

```
URL   =[http://h/$(echo PWNED)`echo PWNED`]   <- data, not executed
USER  =[dmlsoap]
PASS  =[hunter2]
ESCAPE=[http://h/it's]                        <- the '\'' form round-trips
```

**Mutation → RED** (drop the quoting):

```
test result: FAILED. 11 passed; 4 failed
  every_value_is_single_quoted_because_bash_sources_this_file
    unquoted value in a file bash sources: "DML_SOAP_URL=http://127.0.0.1:7878/"
  a_metacharacter_in_the_url_is_data_rather_than_a_command
  a_quote_in_a_value_survives_the_round_trip_to_the_reader
  a_successful_round_trip_persists_the_credentials
```

---

## F8 (Minor) — a process-global `DML_GAMES_DIR` mutation inside a parallel test binary

`an_absent_games_dir_holds_zero_titles_not_an_unknown_number` set the variable process-globally
while `native_title_count`/`native_facts` read it in the same binary — the flake generator an
earlier task removed from `games_dir_from`, reintroduced one function along.

**Fixed** — the read is now `native_title_count_in(dir)`, pure with respect to the environment,
restructured exactly as the earlier task did. Its sibling case (an **unreadable** dir is `None`,
never zero) is asserted through the same seam so the two answers are *shown* to differ rather
than assumed to. No `set_var`/`remove_var` remains anywhere in the launcher's test code.

**Mutation → RED** (put the env read back inside the tested path):

```
an_absent_games_dir_holds_zero_titles_not_an_unknown_number
  left: Some(2)  right: Some(0)
a_games_dir_that_is_a_file_is_unknown_not_zero
  left: Some(2)  right: None
```

`Some(2)` is the real machine's games directory leaking into the assertion — the flake, caught.

---

## Method notes

- Every finding: test written or fixed first → change → **commit** → re-run the reviewer's
  mutation → confirm red for the right reason → `git checkout -- <file>` → `git status --short`
  clean. Nothing was mutated before it was committed.
- The full-branch diff carries no line-ending rewrites (`git diff 33d560a --stat`: 9 files,
  +1134 / −156).
- `launcher/node_modules` was temporarily junctioned from the main checkout to run vitest (the
  worktree has none). Removed afterwards with `cmd /c rmdir`, which never follows into the
  target; the parent's `node_modules` was verified intact.
- Nothing touched the `dml-arch` distro, the server, or the database. `docs/backend-comparison-*`
  were not touched.
