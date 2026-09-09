# T8 — A Rebuild that recompiles the Dockerfile on disk can never carry a template fix to an existing install

**Status:** DONE (hand reported 2026-09-09 15:35; awaiting review)
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
