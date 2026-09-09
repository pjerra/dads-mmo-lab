# T7 — The direct-SQL guard reaches the app's own buttons, and "Stop, then install" can actually be done

**Status:** MERGED, code half (`9097264e`, suite green behind it, 2026-09-09 12:21 CEST; Codex pass owed on the range); live half queued for the box, hand kept
**Filed:** 2026-09-09 11:50 by the lead (Fable), from T2's live press
**Hand:** Opus, worktree branched from `yulon-phase8b` (ff-merge `origin/yulon-phase8b` first; report the sha)
**File set (yours alone):** `pylauncher/yulon/apply.py`; the four factory sites T2's reviewer located — `pylauncher/yulon/controller_wow_wotlk/modules.py` (`applier()`, near line 205), `pylauncher/yulon/controller_wow_tbc/modules.py`, `pylauncher/yulon/controller_wow_vanilla/modules.py`, and `pylauncher/yulon/controller_wow_tortoise/autoupdate.py` (`GuardedApplier` sets the *public* `world_running` near line 461, a different attribute from the private one the guard reads); `pylauncher/yulon/ui/controller_view.py` **only** at the module-Applier construction near lines 919-924 (T5 owns `_build_my_party_group` and the `MyPartySeam` Protocol — do not touch those, and start only after the lead says T5's code half is merged); `pylauncher/yulon/docker.py` only if `start_database()`'s signature must change; their test files (name them); `pyplan/write-ledger.md` if a write site moves; and a NEW `pyplan/gates/8.7a-guard-wired-yulon-ubuntu2-2026-09-09/` for the live half. **Not** `install_wiring.py` — T2's reviewer read it: it holds no `Applier` and wires no seam of this kind; the one existing `world_running` wiring is My Party's at `controller_view.py:948`, fail-closed on a blank inspect, which is the shape to copy. Not `pyplan/checklist.md`.
**Box:** `yulon-ubuntu2` for the live half — **only when the lead says so** (T3 holds it now, T5's live half is also queued). Do the code half first and report DONE for it.

## Why

T2 pressed the guard (`apply.py::_refuse_direct_sql_into_a_running_world`, `f8cd55df`) against a running world and it held: refused, the step named, sixteen readings identical. Then two things it found (`pyplan/gates/8.7a-direct-sql-yulon-ubuntu2-2026-09-09/README.md`):

1. **The seam is unwired in the shipped app.** `Applier` takes `world_running` as a seam; the press wired it by hand. The app's own construction (`install_wiring.py`, and wherever the Modules tab builds its `Applier`) passes nothing, so `_world_running` is `None` and the guard never fires — *today's Modules tab would have written 7219 rows into a live world without a word*. Verify the exact site (`apply.py` around line 1920 per the hand; grep for `Applier(`).
2. **The refusal's own instruction cannot be followed.** It ends *"Press Stop, then install again."* Stop through the app brings the database down with the world, and a direct SQL step then fails with `container … is not running`. `docker.start_database()` exists and put the database back alone in 6.6 s. The route lacks a caller, not a primitive.

Exit clause 3 forbids a capability reachable only from a script; 8.7a's second clause is met by the engine and by no button until both of these are fixed.

## What to build

- **Wire the seam** where the app builds every `Applier`, reading the install's actual world state the way `install_wiring` already knows it (the ready/status seams the Server tab uses — reuse, do not re-derive). Every construction site, not one: enumerate them with a grep and say the count.
- **Make the stopped-world path work**: when a direct SQL step runs with the world down and the database down, start the database alone (`docker.start_database()`), run the step, and say so in the report text; do not start the world. If the world is *up*, refuse as now. If the database cannot be started, refuse with the daemon's sentence.
- **The refusal sentence** stays true: after this change "Press Stop, then install again" is a thing a user can do and have succeed.
- Tests, TDD, each naming the mutation it catches: an `Applier` built through the app's wiring refuses with the world up (the seam arrives, not a `None`); the stopped path starts the database and applies; the database-cannot-start path refuses; no construction site is left with `world_running=None` (a test that enumerates them, so the next site added is caught).
- `applied_by` defaults to `direct` — T2 counted 43 steps across 18 manifests on this path; your enumeration test should say the number it found.

## Live half (later, on the lead's word)

Repeat T2's press through the app's own surface: the refusal with the world up, then Stop, then install — and it succeeds this time with the database started alone — then remove and restore the six tables from a fresh dump as T2 did. Same rules as T2 for the box, the owner's things, and the realm row. README leads with whether 8.7a's second clause is now met **by a button**.

## Definition of done

- Code half: `--checks` ALL GREEN from your worktree's `pylauncher/` (announce on `yulon-fedora` first); the enumeration test green with its count stated.
- Live half: the three captures above, counts before and after, `test_no_secrets_in_evidence.py` green locally before any log is committed.
- One commit per half, `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>` only; no push; the ticket file is not yours.

## Report format (final message)

`## Report (code half)` — sha, gate last line, diff stat, the construction sites (file:line, count), the tests and their mutations, deviations, status DONE (code half).

## Report, code half (hand, 2026-09-09 16:48)

- sha `ef0cee36` on `worktree-agent-ae3ae26e084b062b7` (base `583a61ce`); gate ALL GREEN 3787; diff 15 files +991/-36.
- **13 construction sites wired**, found by an AST audit that asserts the count and what each site binds: the `Applier(...)` / `apply_module()` pairs in the TBC, Vanilla and WotLK factories, `GuardedApplier(...)` and the Tortoise pair, and **the four shipped Modules-tab appliers in `controller_view.py`** (`:937`, `:1224`, `:1382`, `:1503` -- four, not the one the ticket named). `world_running` is a required keyword-only parameter on all four factories; the audit also pins that each site binds `docker.world_running` (a site rewired to `.settled` is caught).
- The seam: new `docker.world_running(container, *, wsl_distro)`: blank status -> `None` (fails closed), `running`/`restarting` -> `True`, else `False`; deliberately not `.settled`. My Party's wiring untouched.
- Tortoise fixed at `autoupdate.py:458`: `GuardedApplier.__init__` forwards `world_running=` to `super().__init__`; `_guard()` narrows with `is True`.
- The stopped-world path: `Applier._start_the_database_for_direct_sql()` runs from `_sql()` after the refusal, only with a `start_database` seam, a runner and a direct step in this action; the view binds `docker.start_database(...)`, which starts the database alone; `start_database()` returns `bool` and the report says `started the database alone; the world server was left stopped` only when it did; a failure raises `ApplyError` with Docker's sentence before anything runs.
- Tests: 7 behavioural + 3 audit/census in `test_apply.py`, 2 in `test_docker.py`, 2 each in the TBC/Vanilla/Tortoise module tests, one call site in `test_catalog_view.py`; ten mutations caught (listed with their tests); one not caught and not real (`is True` vs `bool()` on `bool | None`). Census: 43 steps / 18 manifests / 4 games (the 44th direct step is `paragon.json`'s into `ale`).
- Deviations: four `controller_view.py` sites edited, not one; `docker.py` gained a function; one annotation widened in `native.py:1378`; one call site in `test_catalog_view.py`; the test through `ControllerServices.for_entry()` belongs in `test_controller_view.py` (T5's, now merged) -- proposed, not written; write-ledger untouched; six laptop-only pre-existing failures.

## Review 1, code half (Codex adversarial, 2026-09-09 17:10) -- REWORK, one

- [high] `apply.py:1880-1887`: `_sql()` reads `world_running` once, then `start_database()` may wait up to 120 s for health, then the SQL runs -- a Server-tab Start or an external `compose up` in that interval puts a live world behind the databases the SQL writes, and the report would still say the world was left stopped. Must-fix: re-read the three-valued state after `start_database()` returns and refuse before the first statement unless it is explicitly `False`; a test where the seam answers `False` before the start and `True` (and `None`) after. Also named: serialising Start and module actions with a shared lifecycle lock.

## Review 2, code half (cold Fable reviewer, 2026-09-09 17:18) -- REWORK, two

Merge-tree against `yulon-phase8b` clean; T5's one `controller_view.py` hunk (the Protocol) does not touch T7's four. The thirteen sites counted by hand from the diff and all bind both seams; the Tortoise `super().__init__` bug confirmed real at the parent; the stopped path traced to `compose up -d --no-deps <db>` + `wait_db_healthy` with no world start anywhere; every named mutation traced. Must-fix:

1. **WotLK -- the game T2 pressed -- has no seam-arrival test at any level, and nothing builds through `ControllerServices.for_entry()` and refuses.** TBC/Vanilla/Tortoise prove arrival through their factories; `test_catalog_view.py:1609` passes `lambda: False` and asserts nothing about the guard; the AST audit's `_seam_source` (`test_apply.py:3201`) accepts *any* `ast.Name` as passthrough. The hand's reason (T5 owns `test_controller_view.py`) expired at `e593d3d0`; `test_controller_view.py:1380-1425` already has the `for_entry(WOTLK, scratch_dir)` + `.db_password` pattern. Add (a) a WotLK factory test mirroring TBC's (`controller_wow_wotlk/modules.py:187`, `sql=_RecordingSql()` since that factory defaults to a real `DockerSql`); (b) a test through `for_entry(WOTLK, ...)` with `docker.world_running` monkeypatched to `True` (and once to `None`), pressing `services.applier.install()` on a manifest with a direct `world` step, asserting the refusal and no runner calls. That is the test that proves the *button*.
2. **`test_apply.py:3222`: the audit's docstring claims what it cannot do** ("a site added anywhere in the package fails this test on the number even if its spelling hides the seam" -- an alias, `functools.partial(Applier, ...)`, `cls(...)` or a variable is not matched by `name.endswith(("Applier","applier"))` and is invisible; the real backstop is that all four factories require the keyword). Say what it enumerates and what it cannot see; tighten `_seam_source` to `value.id == keyword.arg`.

Notes for the live half: show the exact sequence Stop (`compose stop`, container kept) -> `world_running` False -> `start_database` -> SQL -> `done` line, with the `started the database alone...` sentence in the real report and the world container still `exited` at the capture; record that `container_state()` reads a missing container and an unreachable daemon both as `None`, where the "Stop the server, then install again" sentence is not followable either (not T7's bug); `start_database` starts `compose_services()[0]`, live-gated only on AzerothCore -- a per-spec fact for the CMaNGOS trees; Tortoise's `_guard()` runs before the 8.7a check, so an armed updater shows `AutoUpdateRefused` first (one README line if Tortoise is ever pressed).

## Rejection (lead, round 1) -- both reviews

1. **Close the check/start/SQL race** (Codex): after `_start_the_database_for_direct_sql()` returns, re-read the seam and refuse before the first statement unless it is explicitly `False`; the refusal names the steps as the first one does. Tests: the seam answers `False` before the start and `True` after -> refused, runner never called, database-start recorded; the same with `None` after. If you also want the lifecycle lock Codex named, describe it for the lead rather than build it -- it crosses into the Server tab.
2. **WotLK arrival tests** (Fable 1): the factory test at `controller_wow_wotlk/modules.py:187` with `_RecordingSql()`, and the `for_entry(WOTLK, ...)` test in `test_controller_view.py` (now in your file set for that test only) with `docker.world_running` patched to `True` and to `None`, `services.applier.install()` on a manifest with a direct `world` step, the refusal asserted and no runner call. Name each test's mutation.
3. **The audit says what it sees** (Fable 2): rewrite the `test_apply.py:3222` docstring; `_seam_source` requires `value.id == keyword.arg`.
Add a commit (do not amend); gate; report in the same format, with the new sha.

## Report, code half round 2 (hand, 2026-09-09 18:05)

- Second commit `a6e2aff6` (round 1 `ef0cee36` untouched) after merging `origin/yulon-phase8b` at `342ea1e7` (merge `b953086c`, clean); gate ALL GREEN (yulon-fedora, 3866 passed); this commit 3 files +374/-16 (`test_apply.py`, `test_controller_view.py`, `apply.py`); whole ticket vs `yulon-phase8b` 16 files +1350/-37.
- **Race closed**: `_start_the_database_for_direct_sql()` returns whether the start seam was *consulted*; on a true answer `_sql()` calls `_refuse_direct_sql_into_a_running_world` again -- the same function, so the second refusal names the steps in the first one's words and refuses on anything but an explicit `False`. A press with no start seam reads the world once (no time passed). The comment records the trade: `install()` returns no report when it refuses, so a database this run started is left up and unmentioned. **Lifecycle lock described, not built**: a per-install mutex from first reading to last statement would let an install block a Start for a two-minute health wait and would not cover a `compose up` typed elsewhere, which the re-read does; recommendation leave it.
- **WotLK arrival**: two factory tests in `test_apply.py` (`world_running` dropped; `start_database` dropped) with `sql=_FakeSql()`; two button tests in `test_controller_view.py` through `ControllerServices.for_entry(WOTLK, server_dir)` with the `.db_password` file, `docker.world_running` patched to `True` and to `None`, `services.applier.install()` on a source-less manifest with one inline direct `world` statement; nothing attached to the applier, no private field touched; "no runner calls" asserted by recording `DockerSql.run_statement`/`run_file` at class level; the first also asserts the container asked about is `WOTLK.container_spec().world`. The `None` test catches a site rewired to `.settled` or `status == "running"`.
- **Audit**: the docstring has three labelled parts (what it enumerates; what it cannot see -- alias, `partial`, `cls(...)`, a factory in a variable or dict, round 1's count claim withdrawn; the backstop -- the required keyword on all four factories). `_seam_source(value, name)` requires `value.id == name`; a direct helper test (`...reads_a_differently_named_pass_through_as_a_stranger`) because no live site exercises the tightening.
- Round-2 mutations: the second refusal disabled (three race tests); the verdict softened to `is not True` (one race test + two seam tests); WotLK factory drops both seams (two factory tests + audit); WotLK view site -> `.settled` (two button tests + binding audit); `_seam_source` loosened (the helper test). Still equivalent and uncaught: `is True` vs `bool()` in Tortoise's `_guard()`.
- Deviations: `test_controller_view.py` edited (two tests + one import, as the rejection allowed); round-1 deviations stand; write-ledger needs nothing. The reviewer's live-half notes carried into the live half.

## Review, code half round 2 (Codex adversarial, 2026-09-09 18:10) -- NOT RUN

Codex refused the run: "You've hit your usage limit ... try again at 3:04 PM." Owed; to be re-run on this commit range when the limit lifts. The Fable verdict decides the merge in the meantime; a later Codex finding becomes a follow-up ticket.

## Review, code half round 2 (cold Fable reviewer, 2026-09-09 12:18 CEST) -- ACCEPT

All three `_sql()` callers (`:1229/:1259/:1277`) covered; the second reading runs before the `for step` loop and refuses on `None` (`:1996`, `:2003-2015`); the helper's False return happens only when nothing was asked of Docker (`docker.start_database()` calls `status()` before returning False for a db already up, and the helper still returns True there); merge `b953086c` added nothing beyond origin + round 1; the button tests resolve `docker.world_running` through the lambda at `controller_view.py:960-967` and `for_entry` through `_FACTORIES`; the runner is reached only at `apply.py:2104/2113` (`:1426` is `query`, prompts only), so the class-level patch is sufficient; the conftest guard (`conftest.py:784-850`) fails any test whose argv reaches the docker CLI; the audit docstring true and the four-in-view count exact.
Notes: (1) the comments say 120 s, `docker.py:38` says `_DB_HEALTHY_TIMEOUT_SECONDS = 180.0` -- `apply.py:1886`, `:2051`, `test_apply.py:3396`; fix in the live-half commit. (2) The recorded trade is smaller than the comment claims: only the `None` branch can leave the db up alone; no wording change needed; the live README states the container states at the refusal. (3) Lifecycle lock: leave it; the live README names the residual window (one `docker inspect` ~0.3 s plus statement latency) and does not claim zero. (4) `test_without_a_start_seam_the_world_is_read_exactly_once` passes at the parent -- an over-fix guard, honestly labelled. (6) The live half must show Stop -> `world_running` False -> `start_database` -> SQL -> `done` with the "started the database alone" sentence, the world container `exited` at capture, and if feasible the second reading firing.

## Merge, code half (lead, 2026-09-09 12:21 CEST)

`git merge --no-ff a6e2aff6` -> `9097264e`; `--checks` on `yulon-phase8b` behind it ALL GREEN (yulon-fedora); pushed. The hand and its worktree stay for the live half; the 120 s / 180 s comment correction (Fable note 1) goes into the live-half commit.
