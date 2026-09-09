# T8 — A Rebuild that recompiles the Dockerfile on disk can never carry a template fix to an existing install

**Status:** OPEN
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
