# T13 — `dml_uninvite` removes a bot only from the party of the master that asked

**Status:** code half MERGED `1a446392`; live half DONE by the hand `d4bf2a0e` (2026-09-10 18:59 CEST), under one cold evidence review
**Filed:** 2026-09-09 17:05 by the lead (Fable), from T5's round-3 Codex review
**Hand:** Sonnet (budget rule 2026-09-09; the trailer says Sonnet), worktree branched from `yulon-phase8b` (ff-merge `origin/yulon-phase8b` first; report the sha)
**File set (yours alone):** the bridge script `dml_uninvite.lua` where the app ships it (find it: `git grep -l dml_uninvite -- '*.lua'`; the deploy list in `pylauncher/yulon/party.py` names the five), `pylauncher/yulon/party.py` **only** at `uninvite_command()` and the seam sentence for a refusal (T5's round-3 `remove_all` and its tests are merged and must keep passing), `pylauncher/tests/test_party.py`, and a NEW `pyplan/gates/8.6-uninvite-contract-yulon-ubuntu2-2026-09-09/` for the live half. Not `party_panel.py`, not `controller_view.py`, not `pyplan/checklist.md`.
**Box:** `yulon-ubuntu2` for the live half — only on the lead's word; the box is queued (T3's round 3, then T5's live half, then T7's).

## The window (T5, Codex round 3; the Fable reviewer's note 4 names the same interval)

`InstallParty.remove_all(master, confirmed)` re-reads the group table and refuses unless the fresh guid set equals what the person confirmed. Then it sends `dml_uninvite <player> <bot>` per bot by **name**. `dml_uninvite.lua` resolves the bot's name at execution and calls `RemoveFromGroup()` on whichever group the bot occupies **then**. A confirmed bot that leaves and joins another master's party between the seam's read and the whisper is removed from a group nobody confirmed. The interval is one SQL read to one whisper; the bot manager's own timers move bots on their own schedule, so the window is real and narrow, and no further read in Python narrows it to zero — only the server can check at the moment it acts.

## What to build

- **The Lua verifies the master at execution**: it already receives the player's name; before `RemoveFromGroup()` it checks the bot's current group is the group that player is in (or led by that player — read how the fork's playerbots resolve the master; `dml_addclass.lua` and `dml_whisper.lua` show the idioms this bridge uses) and answers with a sentence the seam can recognise when it is not (`… is not in <player>'s party now`). Keep the success reply the seam already parses (`party.py`'s `dismiss()` reads it — read that first).
- **The seam recognises the new refusal** as `Dismissal(..., left=False, sentence)` naming the bot and the party it is in now, so `_mass_sentence` reports it beside the ones that went.
- TDD on the Python side (the reply parsed into the right `Dismissal`; a mutation that treats the new sentence as success is caught). The Lua cannot be unit-tested here: its proof is the live half.
- **Live half** (later): with two bots in your own character's party and a second character of your own in the world, move one bot into the second character's party by hand (the console can invite it, or `dml_addclass` as the second master) between the seam's read and the whisper is not stageable — so prove the contract directly: whisper `dml_uninvite <first> <bot>` for a bot that is in the **second** master's party and show it stays, with the refusal in the world log; then the honest case, and show it goes. Frames and the world's own log lines, the process alive at each capture; the owner's things untouched; realm row and authserver as every ticket before.

## Definition of done

Code half: `--checks` ALL GREEN (announce on `yulon-fedora` first); the parsing test fails first. Live half: the two whispers above with their log lines. One commit per half, `Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>` only; no push; the ticket file is not yours. Scratch files under `<scratchpad>/T13/`.

## Report format (final message)

`## Report` — sha, gate last line, diff stat, the Lua check as written (quoted), the refusal sentence, the tests and their mutations, deviations, status DONE (code half).

## Report, code half (hand, 2026-09-09 14:23 CEST)

- Base `75127959` (ff, contains `f7881372`); one commit `b9531de9`; gate ALL GREEN (3897); 3 files +151/-16 (`pylauncher/lua/party/dml_uninvite.lua` +58, `test_party.py` +60, `party.py` +49).
- **The Lua check**: `local g = b:GetGroup(); if g == nil or g:GetLeaderGUID() ~= p:GetGUID() then` -> `"<bot> is not in <player>'s party now"` sent through `handler:SendSysMessage()` (so it reaches the SOAP `<result>`) and printed, `return false`. Master resolution reasoned from the Eluna Group API the bridge already uses (no pinned playerbots/mod-ale source on the laptop). **The wire grammar became `dml_uninvite <playerName> <botName>`** (was `dml_uninvite <botName>` -- the ticket's "it already receives the player's name" was wrong; `origin/rust-main`'s `cli/lua/party/dml_uninvite.lua` is byte-identical to the old script, no master check there either).
- Python: `party.py`'s `_uninvite_moved_marker(player, bot)` builds and recognises the exact string inside `dismiss()`; on a match `Dismissal(removed=False, logged_out=False, sentence="{bot} was not removed: ...", bot=bot)` with no logout whisper and no poll. The hook returns `false` regardless, so `answer.outcome` is always `"yes"` and the refusal is legible only in `answer.text` (the same reasoning `read_probe` uses).
- Tests: the parsing test RED first; mutation `if False and moved in answer.text:` caught (the logout whisper sent); a bad-master-name refusal test; five wire-string assertions updated to the two-arg form; T5's `remove_all` tests untouched and green.
- Deviations: trailer `Co-Authored-By: Claude Sonnet 5` (the hand is Sonnet, as briefed); the wire format widened, not added to; five laptop-only failures.

## Review, code half (cold Opus reviewer, 2026-09-09 14:28 CEST) -- REWORK, three

The wire change, the seam and the marker-not-outcome reasoning are right; `handler` on the SOAP path is proved (hook 42 pushes `(player, text, handler)`, `dml_bridge_ping.lua:36-38`); the grammar fails closed in both directions (old Lua + two args -> `LANG_CMD_INVALID` -> `outcome == "no"`); the only sender is `party.py:1320`; marker exactness safe (`_check_name` runs first). Must-fix:

1. **`dml_uninvite.lua:66`: the leader test contradicts the feature's own definition of "<player>'s party".** `g:GetLeaderGUID() ~= p:GetGUID()` asks whether the player *leads*; the Python half asks *membership* (`group_rows_sql`, `party.py:931-959`: the bot members of the group the master is IN, no leadership clause). A master grouped with another human -- the multi-master world this ticket is premised on -- or any leader hand-off: the panel lists and confirms the bots, the Lua refuses every one with a false sentence, and `dismiss()` escalates it to an invented cause. Use membership (`g:IsMember(p:GetGUID())`, name checked against the fork's ALE on the box) or match `group_rows_sql` exactly; soften the Python sentence to what the server reported.
2. **`dml_uninvite.lua:55-63`: the player-not-found branches are silent refusals the seam reads as success.** `print()` + `return false` -> empty `<result>` -> `outcome == "yes"`, no marker -> `dismiss()` whispers logout and polls; `InstallParty.remove` (`:1720-1727`) has no online check, `members()` returns the not-online string, `_rows_only` makes it `()`, and the press reports "Newbot left the party" while the bot is still there -- a new false success, the failure T13 was filed to delete. Every non-removing branch answers over `handler` in words `dismiss()` treats as a refusal.
3. **Nothing pins the two halves of the protocol**: the sentence template and the wire grammar each exist twice (Lua and `party.py`) with no test binding them; `test_party.py:300` already does this for the probe command. Add a pin over `resources.lua_dir()/"party"/"dml_uninvite.lua"` for the grammar pattern and the format string.
Notes: `GetLeaderGUID() ~= GetGUID()` on userdata without `__eq` would refuse always -- the live half's honest case is load-bearing, the negative capture alone is not enough; the live half must redeploy the five scripts and restart the world first; the new test's mutation is caught by one assertion only.

## Rejection (lead, round 1)

1. Membership, not leadership (must-fix 1), with the Python sentence bounded to the server's words.
2. Every non-removing branch (player not found, bot not found, no group, not a member) replies over `handler` with a sentence the seam recognises as a refusal, each its own `Dismissal(removed=False, ...)` wording or the one marker with the reason -- your choice, say which; a test per branch that the logout whisper is not sent.
3. The Lua/Python pin test (must-fix 3).
Round 2 is the last under the owner's cap; the lead closes what remains by hand. Add a commit; gate; report in the same format.

## Report, code half round 2 (the hand's work, committed by the lead, 2026-09-10 14:35 CEST)

- The Sonnet hand died on the limit with round 2 uncommitted in its worktree; the lead read the diff, ran it, and committed it unchanged as `0d832391` on top of `b9531de9`. 3 files +177/-52 (`dml_uninvite.lua` +62/-36, `test_party.py` +80/-3, `party.py` +35/-13).
- Gate: `YULON_TEST_BOX=m910q` (the Hyper-V host `desktop-fp27auv` is offline on the tailnet, last seen ~20 h before; `yulon-fedora` and `yulon-ubuntu2` unreachable) `--checks` -> `=== --checks: ALL GREEN ===`, 3903 passed 6 skipped, mypy x3, ruff, black.
- **Must-fix 1**: `local g = b:GetGroup(); if g == nil then ... end; if not g:IsMember(p:GetGUID()) then` -> membership, the relationship `group_rows_sql` reads. `Group:IsMember` checked against `azerothcore/mod-ale` master `GroupMethods.h` (the engine `rebuild-live-yulon-ubuntu2-2026-09-09` cloned): `ObjectGuid guid = ALE::CHECKVAL<ObjectGuid>(L, 2); ALE::Push(L, group->IsMember(guid));`, and `Player:GetGUID()` pushes an ObjectGuid, so the comparison is C++-side, no Lua `__eq` involved. The Python sentence is now `"<bot> was not removed: the server says <answer.text>"`, nothing invented.
- **Must-fix 2**: one `refuse(reason)` closure; all four non-removing branches (player not found/offline, bot not found/offline, ungrouped, grouped with someone else) go through it: `handler:SendSysMessage(msg)` when `handler ~= nil`, then `print`, `return false`. Marker `"<bot> is not in <player>'s party now (<reason>)"` -- the hand chose the one-marker-with-reason option. A parametrised test per branch asserts `removed is False`, `logged_out is False`, and `chan.sent == ["dml_uninvite Pakka Newbot"]` (no logout whisper, no poll).
- **Must-fix 3**: `UNINVITE_COMMAND_PATTERN` and `UNINVITE_REFUSAL_FORMAT` live in `party.py`; `test_the_uninvite_grammar_and_refusal_sentence_are_the_shipped_scripts_own` asserts both are substrings of the shipped `lua/party/dml_uninvite.lua`; `_uninvite_refusal_marker` builds from the format constant.
- Mutations (lead, `__pycache__` purged each side): marker never matches (`if False and marker in answer.text`) -> 6 fail; command pattern drifts -> 1 fail; refusal format drifts -> 7 fail; sentence invents a mechanism again -> 1 fail. Restored: 121 pass.
- Deviations: gate box m910q not `yulon-fedora` (host offline); the commit trailer names Sonnet (the hand wrote every line); the live half cannot start until the host is up.

## Review, code half round 2 (cold Opus reviewer, 2026-09-10 14:41 CEST) -- ACCEPT, no must-fixes

All three delivered; no new defect. Must-fix 1: `IsMember` on the bot's group is the same predicate as `group_rows_sql` (a player is in at most one group); the C++-side compare sidesteps the `__eq` hazard; the sentence relays the server's words, pinned by test. Must-fix 2: all four branches go through `refuse()`; the only other early returns are the in-game origin and the pattern miss (falls to the core, `LANG_CMD_INVALID`, `outcome == "no"`); `bname` cannot be nil there; `handler` is non-nil on SOAP per `dml_bridge_ping.lua:34-36`. Must-fix 3: both pinned literals occur exactly once in the Lua, so a drift in either file fails the pin; `%` formatting is correct with the `'s`.

Notes (non-blocking, recorded for follow-up):
1. `UNINVITE_COMMAND_PATTERN` is referenced only by the pin test; the builder's own drift is caught by the `chan.sent` assertions, so weaker, not broken.
2. `answer.text.strip()` is relayed whole; a world that put a second line into the same `<result>` would land it in the panel label and `_mass_sentence`; `splitlines()[0]` would bound it.
3. Regex-special names cannot reach either side (`valid_name` is `isalpha()`).
4. `dml_uninvite Pakka Pakka` passes `IsMember` and would remove the master from their own party; unreachable from the app (`group_rows_sql`'s `bot_clause` excludes the master) and not new; `if b == p then return refuse(...)` would be free -- for the live half's commit.
5. Outside this file set: `_rows_only` turns a FAILED group read into `()` and the poll reads that as `removed=True`; round 2 removed the Lua-silence route into it, a genuine uninvite whose follow-up read errors remains. Its own ticket.
6. The Lua header cites the Eluna docs, not ALE; the live half's honest case stays load-bearing.

## Report, live half (hand, Opus, 2026-09-10 18:59 CEST)

- `d4bf2a0e` on `hand-t13-live` (base `ffc3c222`); 34 files +1861, the Lua +13; folder `pyplan/gates/8.6-uninvite-contract-yulon-ubuntu2-2026-09-10/` (README, captures 01-09 and 03b, nine scripts, `t13press.py`, the staging harness `t13_stage.lua`, eight `grouprows` before/after files).
- Deploy first: `party.deploy()` reported all five changed, the deployed `dml_uninvite.lua` md5 became the worktree's, the world restarted behind an empty `--since` window, `[dml_uninvite] loaded -- group-remove relay ready` (`02-deploy-restart.log:47`).
- Case 1 negative (`04-negative.log`): `dml_uninvite Jurnaar Zarraden` with Zarraden in Grirmirn's party -> reply and world log "Zarraden is not in Jurnaar's party now (Zarraden is grouped with someone else)"; group rows byte-identical (same md5). Case 2 honest (`05-honest.log`): `dml_uninvite Jurnaar Dalio` -> `text=''`, `[dml_uninvite] removed Dalio from group`, the rows gone. Case 3 guard (`06-guard.log`): `dml_uninvite Jurnaar Jurnaar` -> "(Jurnaar is the master, not a bot in the party)", rows identical. Case 4 member-not-leader (`07-member-not-leader.log`): leadership handed to Dalio (`IsMember(Jurnaar)=true`, leader false); through `InstallParty.remove`: `Dismissal(removed=True, logged_out=True, 'Nore left the party.')` with the removed and logout log lines; the refusal through the same seam `Dismissal(removed=False, logged_out=False, ...)` with no `[dml_whisper]` line.
- The guard as written, after the bot lookup and before `GetGroup`: `if b == p then return refuse(string.format("%s is the master, not a bot in the party", bname)) end` -- fires on case 3, silent on case 2, which also settles round 1's `__eq` worry on this fork.
- Could not be staged: no human master (all 500 online characters are random bots, no client driven); `.group join` refuses at gm 3; `dml_addclass Jurnaar mage` created nothing (cause not determined -- `RNDBOT0` at 10 characters with `CharactersPerRealm = 10` fits as well as "the master is a bot"; `03b` and README §6 say so). The parties came from `t13_stage.lua`, loaded by a second restart and removed by a third. Not shown: the panel, an app-built party, `remove_all`, the race itself.
- Box as left: world up on the five scripts, three restarts, auth and database never restarted, staged parties disbanded (group tables at 0), the five characters online at unchanged levels, 103 accounts unchanged, no throwaway made, PERZI/Pakka/`LootPet2.lua`/`Logger.ALE`/the realm row identical to step 01, the box's clone untouched, `~/t13-live`, `~/t13-venv`, `~/t13-out` removed. `test_party.py` 127, `test_no_secrets_in_evidence.py` 4, the folder grepped for the password and the credential fields.
- Deviations: folder dated the 10th; bot-driven masters; a non-shipped harness on the server for two restarts; a throwaway venv built and deleted on the box; trailer Opus 5. Status DONE.
