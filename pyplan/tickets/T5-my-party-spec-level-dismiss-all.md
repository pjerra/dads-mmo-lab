# T5 — My Party: chosen spec, chosen level, dismiss all — the three things 8.6's own line promises and the panel says it lacks

**Status:** CODE HALF ACCEPTED and MERGED (lead, 2026-09-09 16:40, `e593d3d0`); LIVE HALF pending -- the box is T3's until its round 3 reports
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

## Review 2, code half (cold Fable reviewer, 2026-09-09 12:46) -- REWORK, two

1. A stale dismiss-all arm can fire against a different character (`party_panel.py:505-532`): the first press names the master and count but records nothing, and only `returnPressed` disarms -- arm on Pakka, retype Anmi, press -> `remove_all("Anmi")`. Remember the armed master and count, disarm on `textChanged`, test it.
2. The seam's three new hand-offs in `InstallParty.add` (`party.py:1573-1580`: `specs=`, `max_level=`, `set_level=`) are under no test -- every spec/level test calls `add_bot` directly with those supplied; mutations `specs=()`, `max_level=None`, `set_level=None` are caught by nothing. One seam-level test through `_install(...).add(...)` closes all three.
Notes: the seams are the same ones (spec whisper replaces autopick and gear follows; level through the same `InstallPlay` the factory builds; dismiss-all continues past a refusal); five weakened-assertion cases analysed, none true before its action; the eight mutations are not enumerated (two named); the new-test count is 32+11+1, not 28; spec-name behaviour matches the cited module claims, and refusing an unlisted spec in the app is sound given the in-game-only refusal (caveat: an inline `# comment` after a value is kept where the core would drop it); the level readback is from `characters.level`; threads clean; **the live half must prove** `characters.level` on an online bot after `.character level` (the core's async `CHAR_UPD_LEVEL` can lag -- if seen, poll like the join does), the level-then-spec order, the in-game "Spec not found", and the talent readback.

## Rejection (lead, round 1) -- the union

1. Bind the dismiss-all arm to the master and the exact bot set (both reviewers); disarm on `textChanged`; re-read before the second press; tests for the edited box and for a party that changed underneath.
2. Discard stale spec reads by class or generation (Codex); a reversed-completion test.
3. One seam-level test through `InstallParty.add` with spec and level (Fable).
4. In the report: the eight mutations with the test that caught each; the recount.

## Report, code half round 2 (hand, 2026-09-09 13:58)

- shas `de6b1e77` + `387f43f9` (a second commit, not an amend, so the delta reads on its own); gate ALL GREEN 3826; round-2 delta 4 files +393/-26.
- 1: the arm carries its subject -- the first press snapshots (normalised master, the bots the sentence named) into `_confirmed`; the second recomputes both and fires only on a match; `character.textChanged` stands the arm down; a stand-down caused by the subject moving is remembered and the next press says `DISMISS_ALL_MOVED` rather than silently re-arming; the arming sentence names the bots. No wall-clock expiry, with the reason in the docstring (the two identities are what stale means; offered on request).
- 2: `_spec_generation` bumped per request and carried with the class into the completion; older answers dropped and logged.
- 3: `test_the_seam_hands_the_specs_the_bound_and_the_level_setter_to_the_press` -- one press through `_install(..., level_setter=...).add("Pakka","mage", spec="fire pve", level=60)` over conf files and seven scripted SQL answers; the whisper sent, autopick not, the setter asked `("Newbot", 60)`, `("fire pve", 1, 60)`.
- Two notes fixed: side readings use `_side_failed`/`_read_alongside` (a raising `specs()` no longer re-arms every button under an in-flight add); `_arm` locks the three pickers. The inline-`#` caveat written into `read_spec_names`' docstring; `SPEC_SHAPE` filters such a name out, so the failure is a spec missing rather than one refused unheard.
- Seventeen mutations enumerated, each with its test; round 2's six were fix-then-test with the mutation as the RED evidence. Recount: 52 new test functions (33 + 18 + 1), 57 collected cases; the earlier 28 was an undercount.
- Live half still owes: the async `characters.level` lag, level-then-spec order, the in-game "Spec not found", the talent readback, dismiss-all with two bots.

## Review 3, code half round 2 (Codex adversarial, 2026-09-09 14:20) -- REWORK, one

- [high] `party_panel.py:588-609`: the second press builds its subject from `member_list` -- the last DISPLAYED reading -- and then calls `remove_all(master)`, which does its own fresh read and dismisses whatever is in the party NOW. A bot that joined after arming, with no refresh, passes the stale comparison and is dismissed unconfirmed. The added party-change test edits the widget's rows rather than the seam's state, so it cannot expose this. Must-fix: make confirmation and execution one bound operation -- pass the confirmed identities (GUIDs, not parsed names) into the seam; it reads fresh and refuses unless the current set matches exactly, then dismisses only that snapshot; a test that arms, changes `_StubParty.state_result` with the rows untouched, presses, and proves no dismissal -- and that fails on the current code first.
Fable round-2 verdict pending; the rejection body carries both.

## Review 4, code half round 2 (cold Fable reviewer, 2026-09-09 14:35) -- ACCEPT, seven notes

The arm verified: snapshot of (master, shown bots), equality on the second press, `textChanged` stands down, the moved stand-down remembered so the next press says so and does not re-arm; the three tests each pin a distinct branch. The spec race: generation and class carried, the reversed-completion test genuine. The seam test fails on all three hand-off mutations (traced: `specs=()` -> refusal sentence; `max_level=None` -> refusal; `set_level=None` -> BadRequest). Threads: nothing new. Notes: (1) a stale list under a new name can still be armed -- Show as Pakka, retype Anmi without Show, press: the sentence names Pakka's bots over Anmi's party (the action is right, the sentence wrong); guard: remember the master the rows were drawn for and refuse to arm unless it matches; (2) the bots-changed test is screen-level -- a server-side change between arm and fire is not seen by the panel; the cost is bounded because the seam's fresh read is bots-only; (4) `_level_bound_read` can re-enable the level box under a press (cosmetic); (6) a docstring says four group reads, the script holds three; (7) the live half's list.

## Rejection (lead, round 2) -- Codex's must-fix, with Fable's cheap guard

Both reviewers saw the same window; they weigh it differently. The lead takes the stricter reading, because "dismiss every bot" is a destructive press and the sentence it shows must be the set it sends away.
1. **Bind confirmation and execution in the seam** (Codex, `party_panel.py:588-609`): pass the confirmed identities (GUIDs from the group read, not parsed display names) into a seam call that reads the party fresh and refuses unless the current set matches exactly, then dismisses only that snapshot. Test: arm, change `_StubParty.state_result` with the rows untouched, press -> no dismissal and the moved sentence; and that test fails on the current code first (say the message).
2. **Refuse to arm unless the rows were drawn for this master** (Fable note 1): remember the master in `_state_read`; a press with a different name in the box says "Press Show this character's party first". Test it.
3. Optional: note 4 (`setEnabled(not self._busy)` in `_level_bound_read`); note 6 (the docstring's count).
A third commit is fine; say which. Gate, report.

## Report, code half round 3 (hand, 2026-09-09 15:38)

- A third commit `b8e93fc1` on `de6b1e77` + `387f43f9`; gate ALL GREEN 3832; round-3 delta 6 files +316/-79.
- 1: `InstallParty.remove_all(master, confirmed: tuple[int, ...])` -- `confirmed` required, guids from the group read; the seam re-reads and refuses unless the fresh set matches exactly in both directions (a party that gained or lost a bot is not the one agreed to); the refusal names the party as it is now; the name that reaches `dml_uninvite` comes from the fresh row. The panel snapshots guids at arm time; both Protocols updated. RED recorded first: `TypeError: remove_all() takes 2 positional arguments but 3 were given` and the panel's `assert [] == [('Pakka', (948, 949))]`; eight tests failed before the fix.
- 2: `_state_read` receives `(master, state)` -- the master travels with the answer -- and records `_drawn` + `_drawn_for`, cleared together on a refused read; arming over rows drawn for another character is refused with `PRESS_SHOW_FIRST`; two tests.
- 3: both optional items done.
- Deleted as unreachable: the panel's `_confirmed != subject` branch (every path to `_drawn` stands the arm down first; a changed master is refused above it); its round-2 mutation retired. Removed: the test that mutated `member_list` directly, replaced by `test_a_party_that_moved_on_the_server_is_not_dismissed_unconfirmed` plus two seam tests.
- Mutations: 16 from rounds 1-2 re-run against round-3 code, all caught (the runner rebuilt under `<scratchpad>/T5/` after the collision); round 3: 5 of 6 caught, the sixth an **equivalent mutant** (the arm's snapshot vs a value recomputed at the second press -- indistinguishable because `_drawn` cannot change while an arm stands; the snapshot stays because it records what a person was shown). Two survivors were weaknesses in the tests fixed in the code (one source of truth for "nothing drawn"; the level-bound read completing under a press now exercised).
- Recount against `7ab2caa0`: 58 new test functions net (35 + 22 + 1), 63 collected cases; gate 3818 -> 3826 -> 3832.
- Live half still owes: the async `characters.level` lag, level-then-spec, the in-game "Spec not found", the talent readback, dismiss-all with two bots.

## Review 5, code half round 3 (Codex adversarial, 2026-09-09 16:25) -- REWORK, one

- [high] `party.py:1621-1631`: the stale-set check closes round 2's bug, but validation is not bound to the group `dml_uninvite` mutates -- the seam verifies the confirmed guids belong to `master`, then issues name-only `dml_uninvite` commands, and the Lua resolves the name at execution and removes the bot from whichever group it occupies then. A confirmed bot that leaves and joins another master's party between the read and the command is removed from an unconfirmed group. Recommendation: pass the expected master (and the guid) into the server command and have the Lua verify the bot is still in that master's group at execution, refusing otherwise; a test seam that moves the bot between the SQL validation and the command.
Lead's note: the Lua bridge script (`dml_uninvite.lua`) is outside this ticket's file set; the window is the interval between one SQL read and one whisper. Fable round-3 verdict pending; disposition follows both.

## Review 6, code half round 3 (cold Fable reviewer, 2026-09-09 16:32) -- ACCEPT, six notes

Binding in the seam traced (`remove_all` re-reads, refuses on a set difference in both directions, the fresh row's name reaches the whisper; the panel binds the snapshot to a local before standing down); the RED shape recorded; `_drawn_for` and the paired clear pinned by two tests; the deleted branch proved unreachable by tracing every writer of `_drawn`; the equivalent mutant agreed, with the snapshot's reason sound; threads clean; every `remove_all(` call site on the new arity. Notes: (1) `dismiss_all`'s first docstring paragraph is round-2 text left standing -- **cut by the lead at the merge**; (2) the equivalence rests on a comment-level invariant, cost bounded by the seam's refusal; (3) a `state()` that raises leaves the previous rows and `_drawn` intact, unlike a refused read (pre-existing; a later ticket); (4) inside the seam, a bot that joins after the check is left alone and one that leaves is reported by name; (5)-(6) the live half's list, plus: capture one `_not_the_confirmed_party` refusal live by adding a third bot from the console between the two presses, and record whether the bot manager's own timer moves bots inside a normal confirm interval.

## Lead's disposition (2026-09-09 16:40)

Fable ACCEPT; Codex REWORK on the window between the seam's read and the Lua's execution (`dml_uninvite` resolves the name at execution and removes the bot from whichever group it is in then). That fix is the Lua bridge script's contract and is outside this ticket's file set: filed as **T13**. The code half is merged (`e593d3d0`) with the stale docstring paragraph cut; the hand's worktree and branch stay for the live half, which starts when T3 releases the box.
