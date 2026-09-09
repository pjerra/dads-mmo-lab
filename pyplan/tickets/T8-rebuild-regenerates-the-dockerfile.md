# T8 — A Rebuild that recompiles the Dockerfile on disk can never carry a template fix to an existing install

**Status:** ACCEPT (Fable 2026-09-09 12:42 CEST; Codex accepted round 2, its round-3 pass owed) -- merging after the T11 gate
**Filed:** 2026-09-09 12:40 by the lead (Fable), from T4's live upgrade
**Hand:** Opus, worktree branched from `yulon-phase8b` (ff-merge `origin/yulon-phase8b` first; report the sha)
**File set (yours alone):** `pylauncher/yulon/catalog/native.py` (`rebuild_stages()` and what it feeds), `pylauncher/tests/test_rebuild.py`, `pylauncher/yulon/catalog/installer.py` only for `rebuild_confirmation`'s sentence, and `pyplan/write-ledger.md` if a write site moves. Not `install_wiring.py` (T7 holds it), not the UI, not `pyplan/checklist.md`.
**Box:** none for the code half. A live press is T4's folder's business if the lead asks for it later.

## The fact (T4, verified by its cold reviewer on the tree)

`native.py:1763` `rebuild_stages()` returns `(build, recreate, ready)`. `write-dockerfile` is not in it. So the Rebuild button compiles whatever `Dockerfile` is on disk in the install — rendered when the install was made — and a fix shipped in the app's template (`pylauncher/catalog/installers/<game>/native/Dockerfile.tmpl`) never reaches an existing install through the button. The Tortoise upgrade on 2026-09-09 needed the family's `write-dockerfile` stage run by hand first: the on-disk file was rendered 2026-09-07 (`FROM ubuntu:22.04`, zero `INSERT IGNORE`), the template had been fixed 2026-09-08 (`3a1ed6ee`), and a bare Rebuild would have produced the old image again. The docstring at `native.py:1766-1773` explains the exclusion in terms of `generate-compose` rewriting files a running server uses — the Dockerfile is not such a file, and the docstring does not name it.

## What to decide, then build

Two honest shapes; pick one and say why, or do the first and name the second:
1. **Regenerate the Dockerfile as the first rebuild stage**, from the current template, exactly as the install did — same renderer, same markers — and only then build. Idempotent when nothing changed. Say in `rebuild_confirmation` that the build recipe is refreshed from the app's current template.
2. **Detect drift and refuse** with the diff and the sentence that says what a rebuild would compile.

Either way: TDD, the failing test first (an install whose on-disk Dockerfile differs from the template; today's `rebuild()` builds the stale one), the mutation each test catches, the rollback contract unchanged (`_keep_rollback`/`_restore_rollback`/`_let_go` and their twelve tests still green), and the `ready` marker rules T2/T3 fixed (`REALM_ADDRESS_PATTERN`) untouched.

## Definition of done

`--checks` ALL GREEN from your worktree's `pylauncher/` (announce on `yulon-fedora` first); the test that fails today passes; the confirmation sentence tells the user what changed. One commit, `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>` only; no push; the ticket file is not yours.

## Report format (final message)

`## Report` — sha, gate last line, diff stat, which shape and why, the tests and their mutations, deviations, status DONE.

## Report (hand, 2026-09-09 15:35)

- sha `aa481117` on `worktree-agent-a718b80bb84529648` (base `583a61ce`); gate ALL GREEN 3774; diff `test_rebuild.py +224/-2`, `installer.py +31`, `native.py +153/-30`; no new write site (the re-render goes through the family's existing `write-dockerfile` stage).
- Shape 1: `rebuild_stages()` runs the family's `write-dockerfile` stage first, same renderer, same tokens, same markers as the install; written only if the bytes differ (mtime and layer cache survive). Shape 2 rejected because it never delivers the fix -- every existing install of a game would be refused after any template change with the only remedy outside the app; and shape 1 inherits its refusal for free (`dockerfile.write()` refuses an unmarked, user-edited file). Selected by presence: `dockerfile_dir` is None for AzerothCore (its checkout ships its own Dockerfile), so an unconditional prepend would have refused every WotLK rebuild.
- The confirmation gains one paragraph for entries with a `dockerfile_dir`; `REBUILD_OPENING_NOTE` became `rebuild_opening_note(renders_dockerfile=…)` because "three things and nothing else" went false; `MODULE_SURFACE_AFTER_7_2` intact.
- Five tests RED first (the headline message recorded, with the 22.04/20.04 diff); mutations: the old tuple; the re-render after `build`; the positional wrapper; an unconditional prepend; an unconditional confirmation clause (only one test); a tolerant try/except around the stage (only one test). The twelve rollback tests untouched and green; `_keep_rollback`/`_restore_rollback`/`_let_go` and `REALM_ADDRESS_PATTERN` unchanged.
- **Finding, fixed:** `rebuild()` wrapped the first two stages positionally (`first, second, *rest`) and `replace(first, run=build)` replaces the stage body -- prepending a stage would have run `stage_build` under the name `write-dockerfile` and `stage_recreate` under `build`. The wrappers now bind by name, with a refusal before `_keep_rollback` tags anything if either name is absent.
- Deviations: the cited evidence folder was read out of T4's commit (now merged); five laptop-only pre-existing failures; black reformatted one ternary; file set respected.

## Review 1 (Codex adversarial, 2026-09-09 16:12) -- REWORK, one

- [medium] `native.py:250-252`: the opening note promises that stopping any time before the containers are replaced leaves the server "exactly as it is"; the new first stage rewrites a stale Dockerfile and records the stage in the install-state file before the compile, and on a cancel or a failed build the code only drops the rollback tags -- the previous Dockerfile is not put back, so the recipe the next build uses has changed. Must-fix: either keep and restore the previous generated Dockerfile (and `.dockerignore`, and the state) on every pre-recreate cancel or failure, or narrow the sentence to "the running containers stay as they are; the build recipe may already have been refreshed"; a test that cancels or fails right after `write-dockerfile` and asserts the promised disk state.
Fable verdict pending; the rejection body carries both.

## Review 2 (cold Fable reviewer, 2026-09-09 16:45) -- REWORK, one clause

The mechanism verified in full (the family's own stage first, only with `dockerfile_dir`, only on difference; AzerothCore's three stages untouched; the positional hazard real, traced at the parent; the rollback functions untouched; every one of the five tests RED at the parent and the mutations mapped). Must-fix: `installer.py:395`'s new clause says "if you have edited **that file** yourself the rebuild stops" and "Nothing else in the folder is rewritten" -- but `_write_dockerfile` renders and writes **two** files, `Dockerfile` and `.dockerignore`, through one `dockerfile.write()`, which rewrites whichever differs and refuses either that carries no marker; a `.dockerignore` behind its template is rewritten while the dialog promised nothing else would be. Say both files, drop or rescope "nothing else"; `native.py:244`'s opening note the same. Notes: the refusal sentence for an unmarked file is install-shaped ("Point the install at an empty folder"), wrong advice under a rebuild, outside this file set; the `if wrappers: raise` branch has no test; `rebuild_stages()` now runs before `_keep_rollback`, an unclaimed improvement; a live-press list.

## Rejection (lead, round 1) -- both reviews

1. **Keep the cancel promise true** (Codex): the opening note says stopping before the containers are replaced leaves the server "exactly as it is", and the first stage now rewrites the recipe before the compile. Keep the previous generated `Dockerfile` and `.dockerignore` (the bytes as found) and put them back on every cancel or failure before `recreate`, alongside `_let_go`, with the state file's stage record. A test that cancels or fails right after `write-dockerfile` and asserts the disk state is the ground's. (Narrowing the sentence is the fallback; say why if you take it.)
2. **The clause names what the stage writes** (Fable): both files, as "the build recipe"; drop "nothing else in the folder"; `native.py:244` the same.
3. A test for the `if wrappers: raise` branch.
4. Optional: the install-shaped refusal sentence under a rebuild is outside your set; describe the one-line change for the lead.
Amend or add a commit (say which); gate; report.

## Report, round 2 (hand, 2026-09-09 17:48)

- The single commit amended: `1e15d652` on base `583a61ce`, not pushed; gate ALL GREEN (yulon-fedora, 3778 passed); diff vs base: `test_rebuild.py` +404/-2, `native.py` +266/-31, `installer.py` +39, `write-ledger.md` +2.
- **Must-fix 1, restoring not narrowing**: `rebuild()` reads both recipe files byte-for-byte before the first stage (`_recipe_ground()`, only for families that render one) and `_put_recipe_back()` puts them back on every failure and cancel where `not touched` -- first in the `except`, outside the three rollback branches. Bytes, never `read_text`/`write_text` (a text round-trip would fabricate a CRLF ground on Windows). The state record goes back with the files (`completed` reset, `last_error` kept). A restore that moved nothing says nothing. Narrowing rejected in the docstring in one line.
- **Must-fix 2**: `DOCKERFILE_STAGE`'s docstring leads with the two files; both sentences name the pair as "the build recipe"; "Nothing else in the folder is rewritten" is gone; the confirmation paragraph quoted in the report ends "... if you stop the rebuild or it fails before your server is replaced, both are put back as they were."
- **Must-fix 3**: a test overrides `rebuild_stages()` to drop `recreate` and asserts the sentence, no `tag:` call, nothing compiled.
- Tests and their mutations: fails-puts-both-back (restore call deleted; files restored but not the record -- only this one; only `DOCKERFILE` as ground), stop-between-compile-and-recreate (the `_restore_rollback` arm), restore-stays-quiet, tuple-missing-the-stage (the `if wrappers:` block deleted -- only this one), confirmation extended (`.dockerignore` named, "Nothing else" absent, "put back" present). `_stale()` stales both files. The twelve rollback tests untouched and green; 45 tests in `test_rebuild.py` (was 36).
- For the lead (item 4, not made): `families/dockerfile.py:617-621` the `_Verdict.THEIRS` refusal ends "Point the install at an empty folder, or move that file aside." -- wrong under a rebuild; suggested "Move that file aside and press again, or point a new install at an empty folder."
- Deviations: `pyplan/write-ledger.md` gained two rows (`_put_recipe_back::write_bytes`, `::unlink`; the ledger test went red on the restore); `native.py` imports `families.dockerfile` inside `_recipe_ground()` to break the same cycle `installer.py:599,847` breaks the same way; black reformatted `test_rebuild.py` once.

## Review, round 2 (Codex adversarial, 2026-09-09 17:58) -- ACCEPT

Traced the amended four-file diff read-only, with attention to exception ordering, byte and state restoration, and whether the tests discriminate the regressions: "No material findings."

## Review, round 2 (cold Fable reviewer, 2026-09-09 12:22 CEST) -- REWORK, two

The mechanism holds: the ground is read at `native.py:1938` before the opening note, the cancel check, the wrapper refusal and `_keep_rollback`; every pre-`recreate` exit reaches `_put_recipe_back` before any rollback branch; bytes end to end; the state record truthful; the cancel test really enters the `_restore_rollback` arm; ledger rows right; the lazy import the same shape as `installer.py:607,855`. Must-fix:

1. **`native.py:2240-2243` + `2277-2281`: an unreadable ground file is deleted and reported as "put back exactly as it was".** `_recipe_ground()` maps every `OSError` to `None` ("there was no file"), `PermissionError` included. An unreadable `Dockerfile` -> ground `None` -> `write-dockerfile` refuses `UNREADABLE` ("Nothing was touched") -> `rebuild()` except, `touched` False -> `_put_recipe_back()`: `was is None`, path exists -> `unlink()` -> "put back exactly as it was". The user's file is gone and the sentence lies; on Windows the `unlink` raises `PermissionError`, not `InstallerError`, so `_let_go` is skipped and the rollback tags stay. Fix: `FileNotFoundError` -> absent (unlink on restore); any other `OSError` -> a third value, never unlinked, never written; a test with an unreadable ground asserting the file survives.
2. **`installer.py:400-401`**: "If you have edited either of those two files yourself the rebuild stops and says so instead of replacing it" is false for any edit that keeps the first-line marker -- `_look()` (`dockerfile.py:564`) answers `OURS` on the marker alone and `write()` overwrites the rest. Say what is true: a file that no longer starts with the line Yu'lon put at the top stops the rebuild; one that still carries it is replaced.

Notes: an `OSError` from `write_bytes` inside the restore propagates past `_let_go`/`_restore_rollback` (tags left) -- a `try`/`except OSError` yielding a sentence would be cheap; `test_a_dockerfile_the_user_owns...:276`'s `"put back" not in str(raised.value)` can never fail (the sentence is yielded, not raised); the tests that pass at the parent are guards against round 1, the right target; for the lead, `dockerfile.py:631-634` and `:636-640` are the refusals reachable from Rebuild with install-shaped advice ("Point the install at an empty folder"; "nothing was installed"), `:617-621` is the rendered-text check and unreachable from disk.

## Rejection (lead, round 2) -- Fable (Codex accepted)

1. **Three-valued ground** (Fable 1): `FileNotFoundError` -> absent; any other `OSError` -> leave-alone (no unlink, no write, a sentence naming the file if the restore reaches it); a test with an unreadable ground file asserting it survives and the sentence does not claim "exactly as it was".
2. **The clause says what the marker check does** (Fable 2): `installer.py:400-401` reworded to the first-line marker; keep "both are put back as they were".
3. Cheap: wrap the restore's `write_bytes`/`unlink` in `try`/`except OSError` that yields a sentence and falls through to the tag handling; make the `:276` assertion look at the yielded lines or drop it.
4. For the lead: the two refusal sentences at `dockerfile.py:631-640`, not `:617-621` -- keep your suggested replacement in mind, the lead files it.
Amend or add (say which); gate; report.

## Report, round 3 (hand, 2026-09-09 12:38 CEST)

- The single commit amended again: `b6178611` on base `583a61ce`; gate ALL GREEN (yulon-fedora, 3780 passed; waited for another hand's gate rather than switch boxes); diff vs base 4 files +871/-33; `test_rebuild.py` 47 tests (36 at the parent).
- **Three-valued ground**: `bytes`, `None` (`FileNotFoundError` only -> the re-render created it, the restore unlinks it), `UNREADABLE_RECIPE` (any other `OSError` -> never unlinked, never written, logged when read); a module singleton whose docstring cites `_keep_rollback`'s own fail-closed rule; tested with `isinstance` (mypy's `bytes | _UnreadableRecipe` on `write_bytes` proves the branch load-bearing). The restore then yields a sentence naming the path ("Part of the build recipe was left as it is now: `<path>` could not be read when this rebuild started ...") and never says "put back exactly as it was". Test uses a directory in place of the file (no skip, both platforms; `_look()` sees `UNREADABLE`); asserts `_recipe_ground()`'s answer directly and end to end (directory survives, no false line, the path named, `rmi:` calls happened). Mutation: the two `except` arms collapsed -> only this test red; honest limit stated (the deletion itself needs a regular unreadable file, which Windows cannot create).
- **The clause says what the marker check does**: "...Yu'lon knows those two files by the line it writes at the top of each: while that line is there the file is replaced, including anything you changed underneath it, and a file that no longer starts with it stops the rebuild instead of being overwritten. If you stop the rebuild or it fails before your server is replaced, both are put back as they were." Confirmation test pins the marker wording in and the false clause out; the headline test edits both files below the marker and requires them replaced; the user-owns test plants a file with the first line removed. Mutation: the old sentence restored -> only the confirmation test red.
- **The restore cannot take the rollback with it**: every filesystem call in `_put_recipe_back` inside `try`/`except OSError` appending a sentence; the state write wrapped and logged; the `:276` assertion replaced by one on the yielded lines. Test drives `_put_recipe_back()` directly against a path that is now a directory (unreachable through a press). Mutation: the `try`/`except` deleted -> only this test red.
- For the lead, reading confirmed: `dockerfile.py:630-634` (THEIRS: "Point the install at an empty folder, or move that file aside") and `:635-640` (UNREADABLE: "Nothing was touched and nothing was installed"); `:617-621` is the rendered text's own marker, unreachable from disk; suggested "Move that file aside and press again, or point a new install at an empty folder." and dropping "and nothing was installed".
- Deviations: an `OSError` in the restore is now reported, not raised (beyond the two must-fixes, on the reviewer's note); rollback contract untouched (twelve tests unchanged); black reformatted once.

## Review, round 3 (cold Fable reviewer, 2026-09-09 12:42 CEST) -- ACCEPT

Substantive round; nothing left for a round 4. `_recipe_ground` (`native.py:2274-2283`) three-valued; `_put_recipe_back` (`:2314-2366`) never touches an unreadable file, guards `unlink` with `exists`, keeps every call inside the per-file `try`, so a first-file failure still attempts the second; the state write wrapped; flow reaches `_let_go`/`_restore_rollback` unchanged; the `isinstance` narrowing claim correct. The clause at `installer.py:409-416` true against `_look` (`startswith(GENERATED_MARKER)`) and `write()`. The directory stand-in faithful for both readers; each test red at round 2 for the reason claimed and red under its mutation. Item 4 confirmed: `dockerfile.py:630-634` and `:635-640` reachable from Rebuild via `native.py:1883`; the suggested sentence true on both paths. Four files, ledger right, trailer clean.
Notes: the "nothing was removed" sentence is loose for a `write_bytes` that failed mid-write ("may be partly written" truer); "put back exactly as it was" suppressed for the whole press when any file is left, the right call; the user-owns lines assertion is a guard, not RED-first, as asked; "is replaced" loose for an already-current file, same as the accepted "is written again".
