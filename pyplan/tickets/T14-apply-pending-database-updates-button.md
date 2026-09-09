# T14 — A button applies a re-runnable phase to an established install

**Status:** OPEN
**Filed:** 2026-09-09 12:50 CEST by the lead (Fable), from T11's Fable reviewer (note 1)
**Hand:** Opus, worktree branched from `yulon-phase8b` (ff-merge `origin/yulon-phase8b` first; report the sha)
**File set (yours alone):** `pylauncher/yulon/catalog/native.py` (a new stage tuple beside `rebuild_stages()` at :1768, and its runner), `pylauncher/yulon/catalog/families/cmangos.py` only if the family must expose the flagged phases by name, `pylauncher/yulon/ui/controller_view.py` **only** at the Server-tab action group where `REBUILD_BUTTON_LABEL` (:1738) is wired and the `_rebuild_*` handlers live (:4840-4870) — not the Modules tab (T7 landed there), not `_build_my_party_group` (T5), not the accounts sites (T12); `pylauncher/yulon/ui/catalog_view.py` **not** (the greyed tile is the wrong surface: an established install is operated from its controller view); their test files (name them); `pyplan/write-ledger.md` if a write site is added; and a NEW `pyplan/gates/tortoise-updates-button-m910q-2026-09-09/` for the live half. Not `pyplan/checklist.md`.
**Box:** `m910q` for the live half, **only on the lead's word** (the Tortoise install there is the owner's; one server at a time; stop what you start). Code half first, report DONE for it.

## The fact (T11's reviewer, note 1)

T11 merged the route: `SqlPhase.rerun_on_marked` and `CmangosInstaller._rerun_on_marked()` apply the Tortoise `character updates` phase on an install the probe reads as finished, before `up`, with no marker written. But the route is reached only through `engine.run()`: the catalog tile greys to "Installed" and disables its button once the app knows the folder (`ui/catalog_view.py:412-438`), and `rebuild_stages()` excludes `import` on purpose (`native.py:1763-1786`, "`import` reaches for a database that has a player's characters in it"). So for a GUI user with an established Tortoise install the sentence that opened T11 — *no button in the app applies those three files* — is still true, and exit clause 3 (no capability reachable only from a script) is still open on it. The CLI harness (`install_wiring.py:342`) is the only caller.

## What to build

- **A stage tuple for updates**: `update_stages()` (or the name the family's vocabulary prefers) = the database started alone if it is down (the primitive T7 wired, `docker.start_database()`; reuse, do not re-derive), then `import` — which on a marked install is exactly `_rerun_on_marked()` — and nothing else: no `up`, no `build`, no `write-dockerfile`, no `create_schemas`. The world's state at press time is read through the same seam T7 wired (`docker.world_running`, three-valued) and the press **refuses when the world is up or unreadable**, with the 8.7a sentence shape ("Press Stop, then …"). T11's reviewer note 3 is the reason: the route today writes DDL into `tw_char` under a running world; the button must not.
- **The button**: on the Server tab beside Rebuild, label in the app's voice ("Apply pending database updates…"), enabled only for an entry whose plan carries at least one `rerun_on_marked` phase (read the catalog, do not hard-code Tortoise); a confirmation that names the phases and the files they will stream (`expand()` gives the list) and says the marker is unchanged and the world must be stopped; the report line the route already logs ("… applied. The import marker is unchanged.") surfaces in the panel. A refusal from `sqlplan.apply()` (`on_error: fail`) surfaces as the app's sentence, the world untouched, Start still working.
- **Tests, TDD, each naming its mutation**: the tuple contains exactly the stages named and never `up`/`build`; the world-up and world-unreadable refusals with no SQL run (recorder seam); the button disabled for an entry with no flagged phase and enabled for `wow-tortoise`; the confirmation text pins the file list; a press through `ControllerServices.for_entry()` on a scratch install (T7's pattern in `test_controller_view.py`) reaching the recorder with the flagged phase's statements and nothing else.
- T11's reviewer notes 2 and 4 are facts the confirmation or refusal must state, not fix: `MODIFY money INT(10) UNSIGNED` can refuse under strict mode on a negative `guild_bank_money.money` (the refusal names the file); a clone that predates `sql/character_updates/` refuses at `expand()` and the sentence says the folder is missing from the clone.

## Live half (later, on the lead's word)

On the m910q's established Tortoise install, world stopped: the press through the button; the two log lines; `yulon_install` row and hash unchanged before and after; `tw_char` table count before and after; `character_inventory_copy` present; `guild_bank_money.money` type unchanged; index state on `ai_playerbot_random_bots` before and after; no marker or verify query in the SQL trace; then Start and the world up with its characters. Frames of the button, the confirmation, the report. The fresh-install half is the yulon-arch press already recorded (`pyplan/gates/tortoise-soap-yulon-arch-2026-09-09/`); cite, do not re-prove.

## Definition of done

Code half: `--checks` ALL GREEN from your worktree's `pylauncher/` (announce on `yulon-fedora` first). Live half: the captures above, `test_no_secrets_in_evidence.py` green locally before any log is committed. One commit per half, `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>` only; no push; the ticket file is not yours. Scratch files under `<scratchpad>/T14/`.

## Report format (final message)

`## Report (code half)` — sha, gate last line, diff stat, the stage tuple as written, the button's enabling rule, the confirmation text, the refusal sentences, the tests and their mutations, deviations, status DONE (code half).
