# T5 — My Party: chosen spec, chosen level, dismiss all — the three things 8.6's own line promises and the panel says it lacks

**Status:** OPEN
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
