# T16 live half — when a chosen level holds, and what T5 was actually looking at
### yulon-ubuntu2, 2026-09-10, 23:02:32–23:50:09 CEST (the box's own clock, read by `date` inside the script that wrote each line — `lib.sh:26`)

**Is the ticket's question answered? Yes, and the answer is not the one the ticket expected.**
There is no window in which the module takes a chosen level back. A level sent at +0, +5, +10, +20
and +30 seconds after the group row appeared landed **every time and stayed landed** — the world's
own `.pinfo` read the chosen level within a second of the send and never stopped reading it. What T5
photographed is `characters.level` **not having been written yet**: on this core
`.character level` on an ONLINE character calls `Player::GiveLevel` and writes no row at all
(`01b-source.log:15-45`), and a character writes its own row at `PlayerSaveInterval`, which is
900 000 ms here (`01b-source.log:92`). T5's panel read the row half a second after the press, got the
value of the last save, and reported it as the level.

**So what closes the window is a SAVE, not a wait and not a resend.** `.saveall` is
`ObjectAccessor::SaveAllPlayers` (`01b-source.log:81-87`), which walks every `Player` in the world —
bots included, though a bot holds no session — and the row agreed 2.0 to 9.2 seconds after it went
out in all seven trials. The code half sends it and reads `characters.level` afterwards, and only
then does a row that still disagrees earn a second send.

**Every sentence here names the file and line that shows it.** A claim with no `file:line` after it is
a claim this run did not make. Every `docker logs --since` window was opened with
`date -u +%Y-%m-%dT%H:%M:%SZ` (`lib.sh:27`) and queried BEFORE the action to prove it empty of the
pattern — the T3 trap of 2026-09-09 (memory note `docker-logs-since-is-client-local`). Every box
action was announced in the box's own Claude activity terminal before it was taken (`lib.sh:29`,
`10-activity-terminal.txt`, 31 lines).

The box answers to `ssh yulon-ubuntu2`; its own `hostname` is `yulon-ubuntu`, which is what every
capture header prints (`01-ground.log:2`). One box, two names.

**The code this was pressed from** is the tip of `hand-t16`, copied to the box as `~/t16-live` so
every press is this ticket's tree and not the box clone's older one (`02-harness.log:7`,
`lua_root = /home/pk/t16-live/pylauncher/lua`). Steps 01–03b ran against `party.py` as `hand-t16`
inherited it; step 04 ran against the code half, and prints the three lines it added and the file's
md5 before pressing them (`04-second-press.log:7-11`).

## 0. What was pressed, in order

| # | at | what | capture |
|---|---|---|---|
| 01 | 23:02:32–23:02:43 | the box before anything, and the MODULE's own level paths | `01-ground.log` |
| 01b | 23:03:22–23:03:29 | **the CORE's own source** for `.character level` and `.saveall` | `01b-source.log` |
| 02 | 23:03:44–23:05:59 | **the wall the panel's Add hits on this box**, then the harness on and a restart through the app | `02-harness.log` |
| 03 | 23:17:11–23:33:00 | **the timing table**, three of seven rows staged | `03-trials.log`, `trial-1.txt`…`trial-7.txt` |
| 03b | 23:34:37–23:41:35 | the four rows step 03 could not stage | `03b-fill.log`, `fill-1.txt`…`fill-4.txt` |
| 04 | 23:42:25–23:45:36 | **the second press** — `party._level_step`, three times | `04-second-press.log`, `press-a.txt`, `press-a2.txt`, `press-b.txt` |
| 05 | 23:46:01–23:47:31 | teardown: the harness off, the world restarted without it | `05-teardown.log` |
| 06 | 23:49:18–23:49:47 | the box read back against step 01's own numbers | `06-box-as-found.log` |
| 07 | 23:50:09 | this lane's directories off the box | `07-cleanup.log` |
| 12 | 23:21 | the mutation table for the code half | `12-mutations.txt` |

Scripts, and they are what ran: `lib.sh`, `01-ground.sh`, `01b-source.sh`, `02-harness.sh`,
`03-trials.sh`, `03b-fill.sh`, `04-second-press.sh`, `05-teardown.sh`, `06-box-as-found.sh`,
`07-cleanup.sh`, `08-no-secrets.sh`, the driver `t16press.py`, and the staging harness
`t16_stage.lua`.

Two abandoned runs are kept rather than deleted, and section 7 says why: `03a-trials-abandoned.log`
and `03b-trials-abandoned.log`.

## 1. THE WALL: the panel's own Add cannot be pressed on this box at all

This is the first finding and it bounds everything else. `party.add_bot` adds a bot with
`dml_addclass`, which runs `.playerbots bot addclass <class>` inside the MASTER's own session. That
command reaches its handler only through `PlayerbotMgr::HandlePlayerbotMgrCommand`, whose third guard
is quoted from the box at `02-harness.log:48-53`:

```
    Player* player = m_session->GetPlayer();
    PlayerbotMgr* mgr = GET_PLAYERBOT_MGR(player);
    if (!mgr)
    {
        handler->PSendSysMessage("You cannot control bots yet");
        return false;
    }
```

`GET_PLAYERBOT_MGR` answers only for a NON-bot session: a playerbot is given a `PlayerbotAI` and no
`PlayerbotMgr`. And **every character in the world on this box is a playerbot** — the server's own
`server info` says `Connected players: 0. Characters in world: 500.` (`02-harness.log:13-15`). There is
no wine on the box and the Hyper-V host T5 drove a client from is not this lane's, so no non-bot
session could be made.

Pressed twice, and both presses are the finding:

| press | what went out | what happened | capture |
|---|---|---|---|
| 1 | `dml_addclass Jurnaar mage` — `party.add_command` over the app's own channel | `outcome=yes`, the world logged `[dml_addclass] Jurnaar ran: .playerbots bot addclass mage`, and after 30 s: online characters **500 → 500**, `group_member` **0 → 0** | `02-harness.log:23-28` |
| 2 | `InstallParty.add('Jurnaar', 'mage', level=42)` — the panel's own Add | *"the server accepted the command and no bot joined the party within 6 seconds. It may still arrive; nothing here says it will."* `added=True joined=False bot=None`, `group_member` still 0 | `02-harness.log:35-42` |

T26 met the same wall from the other side on the same box the same evening
(`8.6-altbot-measure-yulon-ubuntu2-2026-09-10/README.md`, and `05-relay.log`, where three
`dml_login` presses through a bot master logged nothing in and formed no group).

**So the party in this run is staged**, the one way a bot CAN get a bot master: a real core group
invite. `AcceptInvitationAction.cpp:51` — `if (sRandomPlayerbotMgr.IsRandomBot(bot))
botAI->SetMaster(inviter);` — is the single line in the module that sets a master without a real
player, and it is reached only from a real invite packet. `t16_stage.lua`'s header carries that
argument with the source lines; it is T18's `t18_stage.lua`, re-headed. Section 8 says what staging
costs.

## 2. Step 1 of the ticket, part one — the MODULE's own source: what runs after a bot joins

Read on the box from the pinned clone, `HEAD b949b50bfcdd4fab937781bac2d7765e39330e4b`, nothing
modified (`01-ground.log:12-13`).

| what | the module's own line | capture |
|---|---|---|
| `addclass` does not create a character: it logs an OFFLINE one in, out of a cache the module builds from its AddClass accounts | `PlayerbotMgr.cpp:1175` — `AddPlayerBot(guid, master->GetSession()->GetAccountId());` | `01-ground.log:61`, and the same lines quoted in full at `:65-83` |
| the login work is QUEUED onto the world thread, so it runs after the command has answered | `PlayerbotMgr.cpp:232-233` — `sRandomPlayerbotMgr.OnPlayerLogin(bot);` then `OnBotLoginOperation` | `01-ground.log:90-91` |
| and `PlayerbotHolder::OnBotLogin` is where a fresh bot is finished | `PlayerbotMgr.cpp:460` | `01-ground.log:92` |
| **an addclass bot IS re-made at the MASTER's level on login** — the one place the module would fight a chosen level | `PlayerbotMgr.cpp:593-594` — `bool addClassBot = sRandomPlayerbotMgr.IsAccountType(accountId, 2);` / `if (addClassBot && master && abs((int)master->GetLevel() - (int)bot->GetLevel()) > 3)` then `PlayerbotFactory factory(bot, master->GetLevel(), …); factory.Randomize(false);` | `01-ground.log:95`, `:119-120` |
| every line in the module that writes a player level — six, and five are the factory | `XpGainAction.cpp:91`, `PlayerbotFactory.cpp:588`, `:650`, `:1440`, `:3615`, `:5356` | `01-ground.log:129-134` |
| the one `GiveLevel` every `Randomize` goes through | `PlayerbotFactory::Prepare` at `:581`, `bot->GiveLevel(level);` at `:588` | `01-ground.log:137`, `:143` |
| the RandomBots loop that could move a level later, and how often it looks | `RandomPlayerbotMgr.cpp:1530` `Randomize(bot)`, `:1551` `Refresh(bot)`; `AiPlayerbot.RandomBotUpdateInterval = 20` | `01-ground.log:160-161`, `:54` |
| and the module's own save, which is the only reason a bot's row ever moves without a `saveall` | `PlayerbotFactory.cpp:873` — `bot->SaveToDB(false, false);` at the end of `Randomize` | `01b-source.log:95`, `:98-102` |

**About the master-level branch, and what is and is not claimed for it.** It is the one place in
the module that would fight a chosen level, so it is named — but it never ran in this folder, because
no addclass bot was logged in at all (section 1). Two readings bear on it and **neither is captured
here**: that the offline addclass pool on this box is at level 1 (read at the console during the
probe that found the wall, and never written to a file), and that T5's master `Tfivepress` was
itself a level-1 character (`8.6-spec-level-dismiss-…`'s frames show its bots joining at level 1). If
both hold then `abs(1 - 1) > 3` is false and this factory never ran for T5's presses either — which
would be one more reason T5's rows were about the DATABASE and not about the module. Nothing in the
code half rests on it.

## 3. Step 1 of the ticket, part two — the CORE's own source, which is the actual mechanism

AzerothCore `413bea61a85e20d9caef7d66fc601a661fdddd9d`, the tree the image is built from, on the box
at `~/wowserver/src` (`01b-source.log:9-10`).

**`.character level` on an ONLINE character writes no row.** `cs_character.cpp:252-281`, quoted from
the box (`01b-source.log:15-45`):

```cpp
    static void HandleCharacterLevel(Player* player, ObjectGuid playerGuid, uint32 oldLevel, uint32 newLevel, ChatHandler* handler)
    {
        if (player)
        {
            player->GiveLevel(newLevel);
            player->InitTalentForLevel();
            player->SetUInt32Value(PLAYER_XP, 0);
            ...
        }
        else
        {
            // Update level and reset XP, everything else will be updated at login
            auto* stmt = CharacterDatabase.GetPreparedStatement(CHAR_UPD_LEVEL);
            ...
        }
    }
```

The online arm changes the live `Player` and stops. The row is written only on the **offline** arm.
That is the whole of T16: the app was reading a column the command it had just sent does not touch.

The rest, each with its line:

| | capture |
|---|---|
| the sentence the app reads back is `acore_string` 127, `You changed level of {} to {}.` — a message about the command, not about the row | `01b-source.log:67` |
| `.saveall` is `ObjectAccessor::SaveAllPlayers()` | `cs_misc.cpp:1402-1407`, `01b-source.log:71-78` |
| which walks every `Player` in the world and calls `SaveToDB(false, false)` on each — bots are in that map | `ObjectAccessor.cpp:286-293`, `01b-source.log:80-88` |
| how long a character takes to save itself with nobody asking: `PlayerSaveInterval = 900000` | `01b-source.log:92` |
| this server's own cap, which bounds the level the panel may ask for: `MaxPlayerLevel = 80` | `01-ground.log:58` |

## 4. THE TIMING TABLE

Seven trials. Each staged a fresh join, waited the row's delay, sent level **42**, and read the LIVE
level (`.pinfo`) and `characters.level` on the same clock about once a second — through a `saveall`
sent ten seconds after the send, and out the far end. `hand` is `character level <bot> 42` over the
seam's own channel with no app function in the way; `seam` is `play.InstallPlay.set_level`, which is
the function `party.add_bot` is handed.

| delay | how | bot (guid) | level at join | live at the send | row at the send | **live 10 s after** | **row 10 s after** | row 3 s after `.saveall` | **row first agreed** | last reading | capture |
|---|---|---|---|---|---|---|---|---|---|---|---|
| **+0 s** | hand | Hikond (39) | 29 | 29 | 29 | **42** | 29 | 29 | **+19.2 s** | live 42, row 42 at t0+19.9 s | `fill-1.txt:83` |
| **+5 s** | hand | Kagal (22) | 6 | 6 | 6 | **42** | 6 | 42 | **+12.9 s** | live 42, row 42 at t0+96.1 s | `trial-2.txt:227` |
| **+10 s** | hand | Gnabrimkonk (54) | 17 | 17 | 17 | **42** | 17 | 17 | **+16.7 s** | live 42, row 42 at t0+27.4 s | `fill-2.txt:86` |
| **+20 s** | hand | Sandrin (8) | 34 | 34 | 34 | **42** | 34 | 42 | **+12.7 s** | live 42, row 42 at t0+111.0 s | `trial-4.txt:266` |
| **+30 s** | hand | Orgurthorm (13) | 47 | 47 | 47 | **42** | 47 | 42 | **+12.0 s** | live 42, row 42 at t0+120.8 s | `trial-5.txt:289` |
| **+0 s** | **seam** | Boldameg (65) | 9 | 9 | 9 | **42** | 9 | 42 | **+14.7 s** | live 42, row 42 at t0+15.5 s | `fill-3.txt:46` |
| **+30 s** | **seam** | Guirrurigg (74) | 8 | 8 | 8 | **42** | 8 | 8 | **+15.3 s** | live 42, row 42 at t0+46.1 s | `fill-4.txt:113` |

Read the table like this:

* **The delay does not matter.** +0 s behaves exactly like +30 s. There is no window.
* **The live level moves within one sample of the send and never moves back.** In the row for +5 s
  the level went out at t0+5.5 s (`trial-2.txt:19`), the world read 42 at t0+5.7 s — the very next
  sample, two tenths of a second later (`trial-2.txt:20`) — and still read 42 at t0+96.1 s, ninety
  seconds later, with every sample in between reading 42 (`trial-2.txt:20-226`). +20 s and +30 s ran the same ninety-second tail
  (`trial-4.txt`, `trial-5.txt`); the four `fill` rows broke off earlier, at 15 to 46 seconds.
* **`characters.level` does not move on its own in that time at all** — the "row at the send" and
  "row 10 s after" columns are the same number in all seven rows.
* **`saveall` is what moves it**, 2.0 to 9.2 seconds later (the `saveall` goes out at send+10, so
  "row first agreed" minus 10). The three rows whose 3-second reading still showed the old value are
  the slow end of that range, not a different outcome: all seven ended with the row reading 42.
* **The app's own seam behaves exactly like the command by hand.** Both `seam` rows are
  indistinguishable from their `hand` neighbours, and `play.set_level`'s own `Outcome` carries the
  server's sentence — `Outcome(done=True, text='You changed level of Boldameg to 42.…')`
  (`fill-3.txt:9`, and `fill-4.txt:75` for the other seam row).

**Nothing in the module moved a level in any of the seven tails.** That is the negative claim this
folder makes, and its bound is the tails' own lengths: 90 seconds in three rows, 15 to 46 in four.
`AiPlayerbot.RandomBotUpdateInterval = 20` (`01-ground.log:54`) means the RandomBots loop looked at
its population four times inside the longest tail and moved nothing; a per-bot randomize timer is
hours (`RandomizeFirst` writes `minRandomBotRandomizeTime`…`max` into `playerbots.random_bots`), and
a tail long enough to catch one of those was not run.

## 5. Step 2 of the ticket — what the code does now

`pylauncher/yulon/party.py`, at `add_bot`'s level step. The whole of it is `_level_step`, and the
order is **send, save, read, resend, save, read**:

```python
    outcome = set_level(bot, level)
    if not outcome.done:
        return LevelStep(_row_level(members, guid), (outcome.problem or outcome.text).strip())
    after = _row_level(members, guid)
    if after == level or after is None:
        return LevelStep(after)
    after = _saved_row_level(...)
    if after == level or after is None:
        return LevelStep(after, saved=True)
    outcome = set_level(bot, level)
    ...
    after = _saved_row_level(...)
    return LevelStep(after, resent=True, saved=True)
```

and `_saved_row_level` is the save plus the join poll's own machinery:

```python
    send(SAVE_COMMAND)
    after: int | None = None
    for attempt in range(tries):
        if attempt:
            sleep(pause)
        after = _row_level(members, guid)
        if after == level or after is None:
            return after
    return after
```

* **`SAVE_COMMAND = "saveall"`** is what closes the window, and its docstring carries section 3's two
  citations. It is the server's own command and what the world does to itself every quarter of an
  hour.
* **`LEVEL_TRIES = 30`, `LEVEL_SLEEP = 1.0`** — a ceiling of thirty seconds against the 2.0–9.2 s
  this folder measured, and a ceiling rather than a wait: the poll returns the moment the row agrees.
  On the second press the whole step took 3.2 s and then 4.4 s.
* **The resend is what a real disagreement earns and nothing else.** Before the save, a disagreeing
  row is a row nobody has written; after it, a disagreeing row is the world disagreeing. A resend on
  the first reading would fire on every press and prove nothing.
* **`None` is a third answer throughout.** A guid that is not in the group table has not proved
  anything about its level, and it earns neither a save nor a resend.
* **The "still reads" sentence survives, rewritten.** It used to say the row "has been seen to stay
  unwritten for as long as this panel watched it"; it now says *"…still reads N, not L, after the row
  was written and the level was sent a second time. Take the level as not set."* The arm is still
  reachable and now means something narrower and true.

## 6. THE SECOND PRESS — `party._level_step` against this server

Pressed from the tree with the code half in it: `04-second-press.log:7-11` prints
`698:SAVE_COMMAND = "saveall"`, `720:LEVEL_TRIES = 30`, `1322:def _level_step(` and the file's md5
before anything is sent. The step is handed the app's own `play.InstallPlay.set_level`, the app's own
channel, and the app's own group read (`t16press.py`, `levelpress`).

| # | what | what came back | capture |
|---|---|---|---|
| A | `Olena` (guid 45), in the master's party at level 8, asked for **42** | `LevelStep(after=42, problem='', resent=False, saved=True)` in **3.2 s**; the panel's own sentence *" characters.level read 8 before the press and 42 after."*; and ten seconds later, and after a further `.saveall`, live 42 and `characters.level` 42 | `04-second-press.log:30-36`, `press-a.txt` |
| A2 | the SAME bot, asked for **42** again — the "already" arm | `LevelStep(after=42, resent=False, saved=False)` in **0.4 s**, no save sent, and *" characters.level already read 42 before the press, so nothing about the level changed and this says nothing about whether it would have."* | `04-second-press.log:46-52`, `press-a2.txt` |
| B | `Ryshar` (guid 73), at level 5, asked for **55** — a second bot and a second level, so the claim is not one press | `LevelStep(after=55, resent=False, saved=True)` in **4.4 s**, *" characters.level read 5 before the press and 55 after."*, live 55 and row 55 ten seconds later and after a `.saveall` | `04-second-press.log:88-94`, `press-b.txt` |

**The chosen level holds and the panel can say so.** `resent=False` in all three: the row agreed as
soon as it was written, which is what section 4 predicted and is why the resend is a guard rather
than the mechanism. A2 is the row that proves the first reading is not skipped — it sent nothing, it
saved nothing, and it said so.

## 7. What went wrong in this lane's own driver, and why two logs are abandoned

Kept rather than deleted, because a folder showing only the run that worked would hide that it took
three.

1. **`03a-trials-abandoned.log` — the driver levelled the MASTER.** The first version took "the row
   that was not in the party before" as the newly joined bot. On this box the master is itself a
   playerbot, so `group_rows_sql`'s clause — which is a BOT clause, and in the app filters out a real
   player master — returns him too; in the +20 s row `Jurnaar` sorted before the invited bot and the
   trial levelled the master to 42 (`03a-trials-abandoned.log`, `RESULT … bot=Jurnaar guid=1`). The
   run was killed, the trial thrown away, and the driver now selects by NAME (`t16press.py`, and its
   comment says so).
2. **`03b-trials-abandoned.log` — and the damage from (1) then refused every invite.** With the
   master left at level 42, `PlayerbotSecurity::LevelFor` returns `PLAYERBOT_SECURITY_TALK` — below
   `PLAYERBOT_SECURITY_INVITE` — for any invitee more than five levels above the inviter
   (`GroupInvitationPermission = 1` on this install, `03-trials.log`), and
   `AcceptInvitationAction::Execute` declines in silence. `Jurnaar` was set back to the level 80 it
   had in step 01 and the run restarted.
3. **An invite is answered on the bot's own terms, so roughly one candidate in three came.** Step 03
   staged three of its seven rows; `03b-fill.sh` re-ran exactly the other four with longer candidate
   lists and nothing else changed. Four trials in this folder report `NO GROUP ROW from any of […] --
   nothing measured` and measured nothing, which is the right outcome for a step that could not
   stage its own precondition.
4. **The orphan-`account_access` read in step 06 named a column this core does not have** and FAILED
   rather than answering, on its first run. The column is `id`, not `AccountID`; the script now
   `DESCRIBE`s the table beside the count (`06-box-as-found.log:29-35`, `0` orphans). A probe that
   cannot answer is not evidence of anything (memory note `incomplete-artifact-reads-as-fact`).

## 8. What this does NOT show

* **The panel's own Add finishing a press.** Section 1: it cannot, on this box, for a reason in the
  module's own source. What was pressed is `party._level_step` — the step under it — with the app's
  own seam, channel and group read, the way T18 pressed `party.spec_command` where Add was out of
  reach.
* **A person at a client.** No `Wow.exe` was launched and no party frame was photographed. Both
  "players" in every trial are bots the module had online, and the master had to be made by a
  harness.
* **An addclass bot, which is what T5's presses were.** Every bot here was already in the world and
  joined a party by invite, so the login work an addclass bot goes through — including
  `PlayerbotMgr.cpp:593-594`'s factory at the master's level — did not run in any trial. Section 2
  reads that branch instead of pressing it, and names why it was harmless for T5.
* **A tail long enough to catch a per-bot randomize timer.** Ninety seconds in three rows; the
  module's own timers for that are hours.
* **The resend arm firing for real.** `resent=True` is measured only in the unit tests
  (`12-mutations.txt`, M3/M4): no press in this run produced a written row that disagreed, because
  the level always landed.
* **More than one level, and more than one class.** 42 in the trials, 42 and 55 in the second press.
  Eight of the ten `party.BOT_CLASSES` never came up: the invitees are whatever the module had
  online.
* **A dismissal through the app.** `InstallParty.remove` was never reached — the bots left their
  parties through `playerbots rndbot init`, which is how they were handed back (section 9).

## 9. The box as left

Read in step 01 and read back in step 06, and identical in both:

| thing | value | step 01 | step 06 |
|---|---|---|---|
| `PERZI.last_login` | `2026-09-08 23:24:16` | `01-ground.log:190` | `06-box-as-found.log:13` |
| `Pakka` | level 6, offline | `01-ground.log:192` | `06-box-as-found.log:15` |
| the realm row, all three columns | `100.99.204.5`, `100.99.204.5`, `255.255.255.0` | `01-ground.log:194` | `06-box-as-found.log:17` |
| `LootPet2.lua` | 37 294 bytes, 2026-09-09 02:06 | `01-ground.log:195` | `06-box-as-found.log:18` |
| `Logger.ALE` | `706:Logger.ALE=4,Console Server` | `01-ground.log:196` | `06-box-as-found.log:19` |
| accounts | 103, with `101 YULON_243C46E3` and `103 YULONADMIN` | `01-ground.log:208-211` | `06-box-as-found.log:23-26` |
| `account_access` rows, and orphans | 3, and **0** | `01-ground.log` / step 06's own read | `06-box-as-found.log:28`, `:35` |
| characters, and online | 1001, of which 500 | `01-ground.log:200`, `:202` | `06-box-as-found.log:39`, `:41` |
| `group_member` / `groups` | 0 / 0 | `01-ground.log:204`, `:206` | `06-box-as-found.log:43`, `:45` |
| the deployed `playerbots.conf` T18 left | 119 723 bytes, md5 `bd8d55ae591adfed443f174ed1311c61` | `01-ground.log:29-30` | `06-box-as-found.log:56-57` |
| the bridge | the same six scripts | `01-ground.log:16-21` | `06-box-as-found.log:48-53` |
| the module clone | `b949b50b…`, nothing modified | `01-ground.log:12-13` | `06-box-as-found.log:60-61` |

The realm row was never edited, so no authserver restart was owed or made. **No account was created
and none was touched**; `PERZI` was never used. The world was restarted twice, both times through the
app's own Stop and Start (`docker.stop_staged` / `docker.start_staged`, the Server tab's two
buttons), and all three containers are up at the end (`06-box-as-found.log:6-8`).

**The harness is off and the world does not know it.** `t16_stage.lua` was deleted, the world
restarted through the app, and the count of `t16_stage` lines in that restart's window is **0**
(`05-teardown.log:36`, `:49`); the world now answers the harness's own command with
`Command 'dml_t16_stage show Jurnaar' does not exist`. The panel's nine preconditions read `ok` with
`ready: True` after it (`05-teardown.log:68`).

**The sixteen bots this lane levelled were handed back to the module's own randomiser** — `playerbots
rndbot init <bot>`, which is `RandomPlayerbotMgr::RandomizeFirst`, the module's own chooser, and
which also takes the bot out of the group. Their levels at the end are the module's choices and not
this lane's: `Jurnaar 50, Farere 61, Dawhiceabu 60, Grirmirn 27, Zarraden 39, Orgurthorm 36, Evini 4,
Losco 70, Kagal 80, Sandrin 59, Hikond 24, Gnabrimkonk 66, Boldameg 50, Guirrurigg 12, Olena 65,
Ryshar 56` (`06-box-as-found.log:65-95`) — **not one of them is 42 or 55**. It is not the build or
the level each had before; a random bot's level is the module's to choose and it re-rolls it itself.
What the teardown did is hand the choice back, and this sentence is the whole of the claim.

`~/t16-live` and `~/t16-out` are gone (`07-cleanup.log`, and the directories read back as absent).

## 10. Secrets

Nothing in this folder `cat`s a conf file or prints a container environment: steps 01 and 01b print
named `AiPlayerbot.*` and `Logger`/`MaxPlayerLevel`/`PlayerSaveInterval` key lines and file listings
only, and the worldserver container's environment — which carries the database password in its
`AC_*_DATABASE_INFO` values — is never printed by any script here (memory note
`gate-logs-carry-generated-passwords`). `.pinfo` output is printed for bots only, never for `Pakka`
or any character on `PERZI`. `tests/test_no_secrets_in_evidence.py` was run against this worktree
before the commit and this folder was grepped by hand: `09-no-secrets.txt`.

## 11. The unit half, and the mutation each test catches

Six mutations, each applied to `party.py` with the anchor asserted to match exactly once, the mutant
read back out of the file, `__pycache__` purged on both sides before and after (memory note
`mutation-testing-pycache-trap`), and the source restored from bytes held in memory inside the same
script run. `party.py`'s sha256 is printed before the first row and after the last and is the same
both times. The harness lives in this lane's own scratch directory, never a shared path, and its
output is `12-mutations.txt`:

| # | mutation | test it kills | mutated | restored |
|---|---|---|---|---|
| M1 | the save is not sent before the row is read | `test_the_level_readback_is_the_row_the_server_wrote_and_not_the_commands_own_yes` | 1 failed | 1 passed |
| M2 | the readback is the level that was asked for, not `characters.level` | the same test | 1 failed | 1 passed |
| M3 | the level is resent whether or not the written row disagrees | `test_a_level_the_written_row_agrees_with_is_never_sent_a_second_time` | 1 failed | 1 passed |
| M4 | a press that needed a second send does not say so | `test_a_level_that_needed_a_second_send_says_so_in_the_panels_own_sentence` | 1 failed | 1 passed |
| M5 | the did-NOT-take sentence is replaced by the one that says it landed | `test_a_level_the_written_row_still_disagrees_with_is_not_reported_as_set` | 1 failed | 1 passed |
| M6 | a bot that left the group is treated as a level that disagreed | `test_a_bot_that_left_the_group_before_the_row_was_read_is_not_sent_the_level_again` | 1 failed | 1 passed |

T5's own level tests are all still green, and one of them was **rewritten rather than kept**:
`test_a_level_the_group_table_does_not_show_afterwards_is_not_reported_as_set` pinned the sentence
this folder overturns, so it is now
`test_a_level_the_written_row_still_disagrees_with_is_not_reported_as_set` and pins the new one, with
the reason in its own docstring. That is a deviation and section 12 names it.

`--checks` was run on m910q from this worktree's `pylauncher/` and came back **ALL GREEN** — 4099
passed, 6 skipped, mypy on three platforms, ruff and black clean: `13-checks-m910q.txt`.

**One earlier pass of the same gate was RED and it is named here rather than smoothed over.** It
failed `tests/test_catalog_view.py::test_a_script_that_exits_0_without_installing_is_not_remembered`,
a test in a file this ticket does not touch. It passes alone on this laptop, it passes with its whole
file on m910q (55 passed), and it passed on the two subsequent `--checks` runs — so it is a flake
under `xdist`, not a consequence of this change. The green transcript is the one committed; this
paragraph is the record that it was the second one.

## 12. Deviations from the ticket, named

1. **The folder is dated 2026-09-10, not 09-09** — the ticket names the 9th; the run happened on the
   10th and the folder carries the day it was measured, like every other folder in `pyplan/gates/`.
2. **The measurement is a STAGED join, not `dml_addclass`.** Section 1 is why, and section 8 says
   what it costs. The ticket's own sentence is "after the group row appears", which is what was
   measured; the login work an addclass bot does is read in section 2 rather than pressed.
3. **A staging and chat-capture harness was loaded onto the server and removed again**, with the
   world restarted through the app so it is not loaded (section 9). It is not part of the app and
   answers in no words `party.py` parses.
4. **Sixteen random bots were levelled and then re-randomised**, and one of them — the master — was
   levelled by this lane's own defect and set back to the level step 01 read (section 7).
5. **`.saveall` was sent many times.** It is the server's own console command, it saves every online
   character, and it is now part of what `add_bot`'s level step does — which is the finding, not a
   side effect of the run.
6. **`playerbots rndbot init` is a module console command, not an app seam.** There is no app route
   that hands a bot's level back to the randomiser, and leaving sixteen bots at 42 would have been
   worse.
7. **Four trials measured nothing** and are reported as such (section 7, item 3).
8. **One of T5's tests was rewritten, not merely kept green.** Its assertions pinned the sentence
   this folder overturns. Section 11 says which, and the test's own docstring carries the reason.
