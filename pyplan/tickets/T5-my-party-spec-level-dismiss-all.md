# T5 — My Party: chosen spec, chosen level, dismiss all — the three things 8.6's own line promises and the panel says it lacks

**Status:** DONE, code half (hand reported 2026-09-09 12:02; awaiting review); live half queued for the box
**Filed:** 2026-09-09 09:55 by the lead (Fable), from T1's round-2 Codex finding
**Hand:** Opus, worktree branched from `yulon-phase8b` (ff-merge `origin/yulon-phase8b` first; report the sha)
**File set (yours alone):** `pylauncher/yulon/party.py`, `pylauncher/yulon/ui/widgets/party_panel.py`, `pylauncher/yulon/ui/controller_view.py` (only where the panel is built, `_build_my_party_group`), `pylauncher/tests/test_party.py`, `pylauncher/tests/test_party_panel.py`, `pylauncher/tests/test_controller_view.py`, `pyplan/write-ledger.md` only if you add a write site, and a NEW `pyplan/gates/8.6-spec-level-dismiss-yulon-ubuntu2-2026-09-09/`. Not `pyplan/checklist.md`.
**Box:** `yulon-ubuntu2` for the live half — **only when the lead says the box is yours** (T2 and T3 hold it first). Do the code half now, report DONE for it, and wait.

## Why

8.6's line (`pyplan/checklist.md:2508` on T1's branch) defines done as *"add a bot by class, spec and level to an online character's party, kick, and dismiss all, with each precondition drawing its own sentence when it is not met."* The design (`pyplan/phase8-designs/b-users-surface.md:422`) says *"Add bot (class, premade spec, level) … puts a named bot of the chosen class, spec and level into the online character's party frame … within 30 s"* and its gate reads `characters.level` on the box. The panel shipped last night (`party_panel.py:33`, honestly): *"No spec, no level, no 'dismiss all' … `add()` takes a class and a gender; there is no spec whisper in `party.py` at all … the level is 8.4a's Characters tab and a different seam, and dismiss-all is a loop the seam does not have."* A Codex adversarial reviewer caught the tick certifying that gap; the tick is withheld until this ticket lands.

## What exists, so you build on it and not beside it

- **Add** — `party.py:1109` `InstallParty.add(master, klass, *, gender="")` → `add_bot()` → after the join, `send(autogear_command(...))` and `send(talents_command(...))` (`:861`, `talents autopick`). The bot arrives geared and auto-specced.
- **Spec, the prior art** — `origin/rust-main:crates/dml-wow/src/party.rs` (`git show origin/rust-main:crates/dml-wow/src/party.rs | sed -n 240,275p`): `spec_whisper_cmd(player, botname, spec)` = `dml_whisper {player} {botname} talents spec {spec}`, "the optional post-`party add` spec whisper pair (`90-main.sh:3109-3111`)". Read the bash lines too: they say which spec names the playerbots module accepts per class. **Measure it on the box**, do not trust the list: whisper an invalid spec and record the module's own refusal.
- **Level** — `pylauncher/yulon/play.py:415` `set_level(character, level)` is 8.4a's seam and a bot is a character. Reuse it; do not write a second level path. The design wants `characters.level` **read back from the database** after the press, not the whisper's reply.
- **Dismiss all** — `InstallParty.remove(master, bot)` per member of `state(master).members`, behind a confirm (the prior art's `Playerbots.svelte:204-220` is two-step). Each dismissal reported by name; one refusal must not hide the others.
- **Panel** — `party_panel.py` (19 tests, headless, `run_inline` in tests). Add a spec picker filtered by the chosen class, a level field bounded by the tree's max (read it from the catalog entry the view already has; do not hard-code 80), and a "Dismiss all" button that names the count it would send away and asks once.
- **Seam type** — `MyPartySeam` (Protocol in `controller_view.py`) gains what the panel needs; `ControllerServices.my_party` still wires `InstallParty`. If the Protocol must grow, that is inside your file set; say so.

## Rules that cost a night each

- TDD: the failing test first, its exact message recorded, then the minimum code; for each new test say which mutation it catches. Purge `__pycache__` on both sides of any mutation you run.
- A gate step whose assertion is already true before its action proves nothing: read the ground (the bot's level and spec before), record it, refuse a step whose result is its start state. Chosen level must differ from the level the server gave; chosen spec must differ from what autopick chose (read it back — the playerbots module can be asked, and `characters` / the bot's talent table can be read).
- Per-tree facts are measured per tree; this box is WotLK only and the panel is drawn only where `InstallParty.for_entry_is_possible`.
- On the box: **make your own account and character**; the owner plays as `PERZI`/`Pakka` and his `LootPet.lua` is in `lua_scripts` — touch none of it. Announce every action: `ssh yulon-ubuntu2 claude-say "T5: …"`. Leave the realm row at `100.99.204.5` on both address columns, mask `255.255.255.0`, and `docker restart ac-authserver` after. `worldserver.conf` must keep `Logger.ALE=4,Console Server`.

## Definition of done

- **Code half:** `--checks` ALL GREEN from your worktree's `pylauncher/` (announce on `yulon-fedora` first); new tests fail first; the panel drives spec, level and dismiss-all through the seam with the app's own sentence for each refusal (unknown spec, level out of range, no bots to dismiss, the confirm declined).
- **Live half:** on `yulon-ubuntu2`, through the panel on the VM's desktop (the 8.6-panel-live folder shows how it was rendered and captured): a bot of a chosen class **and chosen spec and chosen level** in the party, `characters.level` read on the box before and after, the spec read back, then two bots and Dismiss all with both gone from the group table; one frame per clause, the process logged alive at each capture. README leads with whether 8.6's definition of done is now met in full, and names what it still does not show.
- One commit per half is fine; `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>` only; no push; `test_no_secrets_in_evidence.py` green locally before any log is committed.

## Report format (final message; the lead appends it here)

`## Report` — sha(s), gate last line, diff stat, the spec names the module accepted and refused (measured), the level readback pair, deviations flagged, status DONE (code half) / DONE (live half).

## Report, code half (hand, 2026-09-09 12:02)

- sha `de6b1e77` on `worktree-agent-a61dd0f02a5c31520` (base `7ab2caa0`); gate ALL GREEN, 3818 passed; diff 6 files +1680/-44 (`party.py` +519, `party_panel.py` +244, `controller_view.py` +25 Protocol only, three test files).
- Spec: `read_spec_names`/`spec_names`/`spec_command` in `party.py`, seam `InstallParty.specs()`, a panel picker; `add_bot` sends `talents spec <name>` instead of `autopick`, `autogear` after. **Measured, not guessed**: the names come from the install's own `playerbots.conf` (`AiPlayerbot.PremadeSpecName.<class>.<n>`, read at press time; `.dist` fallback), loaded by `PlayerbotAIConfig.cpp:487-493`, compared with `==` in `ChangeTalentsAction.cpp:144`, refused at `:157` **into the game window** (invisible over SOAP -- so the app must refuse an unlisted spec itself), contiguous from 0 (`:138-142` breaks at the first empty). 63 names on the box; the prior art's static list is wrong for this tree (paladin has prot, DK has seven). No hard-coded list.
- Level: `max_player_level` reads `MaxPlayerLevel` from the install's `worldserver.conf`; absent -> `None` and the control withheld (not the compiled 80); `InstallParty.level_setter` is 8.4a's `InstallPlay.set_level` over the same four seams; `Addition` carries `level_before`/`level_after` read from `characters.level`.
- Dismiss all: `MassDismissal`/`dismiss_all`, seam `remove_all()`, a two-press arm naming the count; a refusal does not hide the others.
- Tests: 28 new, RED first (messages recorded); 8 mutations run with `__pycache__` purged both sides, 8/8 caught.
- Deviations: no static spec fallback; `MaxPlayerLevel` absent reads `None`; no catalog level-cap field (finding for `catalog.py`'s owner); no factory edit -- `InstallParty` builds its own `InstallPlay`; no spec readback in code (live half's); `_row` still shows the class number (follow-up); 5 laptop-only pre-existing failures in untouched files, green on fedora; `claude-say` is not installed on ubuntu2 (read-only ssh only).
- Live half still owes: the module's in-game refusal for an invalid spec, the talents read back, `characters.level` before/after, dismiss-all with two bots.

## Review 1, code half (Codex adversarial, 2026-09-09 12:24) -- REWORK, two

- [high] Dismiss-all's confirmation is a global boolean (`party_panel.py:514-532`): armed, the user can change the master name or wait while the party changes, and the second press dismisses whatever `remove_all(master)` reads then -- a different set, or a different master, than the count the user confirmed. Must-fix: snapshot the normalised master and the exact bot identities when arming; before executing, re-read and require the same master and an unchanged set, else disarm and ask again; disarm on character-field edits; consider an expiry.
- [medium] Spec reads race (`party_panel.py:340-361`): each class change starts an independent read whose completion carries only the names, so a slower mage read landing after a druid read fills the druid picker with mage specs, and the next Add sends a spec the seam refuses. Must-fix: carry the requested class (or a generation) into the completion and discard stale results; a deferred-job test that completes reads in reverse order.
Fable verdict pending; the rejection body carries both.
