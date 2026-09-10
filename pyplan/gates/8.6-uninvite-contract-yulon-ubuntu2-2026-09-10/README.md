# T13 live half — `dml_uninvite` removes a bot only from the party of the master that asked — yulon-ubuntu2, 2026-09-10

**Is the ticket's live half met? Yes, and one case more than it asked for.** The negative case, the
honest case, the free guard added in this commit, and the member-but-not-leader edge round 1's
reviewer named were all pressed against a live 500-bot AzerothCore world and are captured here.
**What it does not show is a human at a client**: no WoW client was driven, so both "masters" in
this folder are player characters the playerbots module had online, not people. Section 6 says
exactly what that costs and what it does not.

**Every sentence below names the file that shows it.** The T14 evidence reviewer rejected five
sentences with nothing behind them; a claim here with no `file:line` after it is a claim this run
did not make. Times are the box's own clock, read by `date +"%H:%M:%S %Z"` inside the script that
wrote the line (`lib.sh:22`) — no time in this folder was typed by hand.

The box answers to `ssh yulon-ubuntu2`; its own `hostname` is `yulon-ubuntu`, which is what every
capture header prints (`01-ground.log:2`). One box, two names.

## 0. What was pressed, in order

| # | at | what | capture |
|---|---|---|---|
| 01 | 18:35:49–18:35:58 | read the box before touching it | `01-ground.log` |
| 02 | 18:36:46–18:37:38 | `party.deploy` of the five scripts, restart the world, prove they loaded | `02-deploy-restart.log` |
| 03 | 18:42:32–18:44:15 | the three routes to a party that this box refuses, then a staging harness and two parties | `03-stage.log` |
| 03b | 18:50:37–18:50:38 | a correction to one sentence step 03 printed | `03b-addclass-cause.log` |
| 04 | 18:44:50–18:44:58 | **the negative case** | `04-negative.log`, `04-grouprows-{before,after}.txt` |
| 05 | 18:45:58–18:46:07 | **the honest case** | `05-honest.log`, `05-grouprows-{before,after}.txt` |
| 06 | 18:46:35–18:46:47 | **the free guard** | `06-guard.log`, `06-grouprows-{before,after}.txt` |
| 07 | 18:47:25–18:47:39 | **member, not leader** — and the same two cases through the Python seam | `07-member-not-leader.log`, `07-grouprows-*.txt` |
| 08 | 18:48:23–18:49:53 | teardown, and the box read back | `08-teardown.log` |
| 09 | 18:56:50–18:56:51 | this lane's own directories off the box | `09-cleanup.log` |

The scripts are in this folder and are what ran: `lib.sh`, `01-ground.sh`, `02-deploy-restart.sh`,
`03-stage.sh`, `03b-addclass-cause.sh`, `04-negative.sh`, `05-honest.sh`, `06-guard.sh`,
`07-member-not-leader.sh`, `08-teardown.sh`, `09-cleanup.sh`, the driver `t13press.py`, and the
staging harness `t13_stage.lua`.

## 1. The world was up, and it was running THIS ticket's scripts

| claim | capture |
|---|---|
| all three containers were up before anything was done | `01-ground.log:6-8` — `ac-worldserver`, `ac-authserver`, `ac-database (healthy)` |
| the world was carrying the **old one-argument** `dml_uninvite.lua` — md5 `a52a64e7…`, and its whole text, `dml_uninvite <botName>`, no master check | `01-ground.log:17` and `01-ground.log:26-57` |
| the redeploy went through the app's own seam, `party.deploy(party.lua_root(), party.dest_dir(SERVER))` — the function "Enable My Party" calls — and reported `changed=True`, naming all five it deploys; the one whose bytes differed is `dml_uninvite.lua` (`a52a64e7…` before, `3a8dcdb5…` after), the other four already matched the worktree | `01-ground.log:14-25` vs `02-deploy-restart.log:6-21`; the call is `t13press.py:110-118` |
| after it, the deployed `dml_uninvite.lua` is byte-for-byte the worktree's (md5 `3a8dcdb5…` on both sides) | `02-deploy-restart.log:12-21` |
| the world was restarted, and the bridge answered again 48 s later | `02-deploy-restart.log:33-37` — `DML-BRIDGE-READY dml_bridge_ping` |
| **the line the ticket asks for**, from the world's own log: `[dml_uninvite] loaded -- group-remove relay ready`, with the other four beside it | `02-deploy-restart.log:40-47` |
| every precondition the panel reads was met afterwards: `bridge_answered=True`, `ready: True  blocker: None` | `02-deploy-restart.log:50-52` |

The log window for that restart was opened with `date -u +%Y-%m-%dT%H:%M:%SZ` and the value is
printed as Docker reads it (`02-deploy-restart.log:24`), because a zone-less stamp is read in the
client's local zone and would open the window two hours early on this +02:00 box — the T3 trap of
2026-09-09. The same window was queried **before** the restart and held zero `dml_uninvite` lines
(`02-deploy-restart.log:26-28`), so the load line above is this restart's and not a leftover.

## 2. The negative case — the bot stays in the second master's party

`dml_uninvite Jurnaar Zarraden`, while `Zarraden` is in `Grirmirn`'s party.

| claim | capture |
|---|---|
| before: two parties, `Grirmirn`+`Zarraden` (group 1) and `Jurnaar`+`Dalio` (group 2) | `04-negative.log:6-10`, saved as `04-grouprows-before.txt` |
| the window was proved empty of `[dml_uninvite]` lines before the whisper (grep exit 1, count 0) | `04-negative.log:19-21` |
| the command string was built by `party.uninvite_command()`, not typed: `dml_uninvite Jurnaar Zarraden` | `04-negative.log:26`; the call is `t13press.py:173-174` |
| **the SOAP reply, verbatim**: `Zarraden is not in Jurnaar's party now (Zarraden is grouped with someone else)` | `04-negative.log:28` |
| **the world's own log line**: `[dml_uninvite] Zarraden is not in Jurnaar's party now (Zarraden is grouped with someone else)` | `04-negative.log:31` |
| the group table after is **byte-identical** to before — same md5 `cb56b55d…`, `diff` silent, `IDENTICAL` | `04-negative.log:34-41`, `04-grouprows-after.txt` |
| the server agrees it never moved: `IsMember(Zarraden)=true` in `Grirmirn`'s group afterwards | `04-negative.log:51` |

Note the reply's `outcome=yes` (`04-negative.log:27`). That is the shape `party.py` was built for: this
hook never trips the core's error flag, so a refusal and a success are both "yes" on the wire and the
refusal is legible only in the text — which is why `dismiss()` reads the marker and not the outcome.

## 3. The honest case — the bot goes

`dml_uninvite Jurnaar Dalio`, while `Dalio` is in `Jurnaar`'s own party. Round 1's reviewer:
"the live half's honest case is load-bearing, the negative capture alone is not enough" — a check
that refused everything would produce section 2 unchanged.

| claim | capture |
|---|---|
| before: `Jurnaar` leads a group of 2 and `IsMember(Dalio)=true` | `05-honest.log:15`, `05-grouprows-before.txt` |
| the window was proved empty before the whisper | `05-honest.log:19-21` |
| the reply is an **empty** `<result>` (`text=''`) — the success shape, the one `dismiss()` already parsed before this ticket | `05-honest.log:27` |
| **the world's own log line**: `[dml_uninvite] removed Dalio from group` | `05-honest.log:32` |
| the group table lost both rows of group 2 | `05-honest.log:37-40` (`diff` shows `< 2 1 Jurnaar` and `< 2 4 Dalio` gone), `05-grouprows-after.txt` |
| the seam's own read agrees: `seam.members('Jurnaar') -> ()`, and the server says `Jurnaar is in no group` | `05-honest.log:46-50` |
| the second master's party was untouched by it | `05-honest.log:56` |

A two-player group whose second member leaves is disbanded by the core, which is why the leader's own
row went with it (`05-honest.log:41-42`); step 07 repeats the honest case in a three-member group
where only the bot's row disappears.

## 4. The free guard — `dml_uninvite <master> <master>`

Round 2's reviewer, note 4: this passes `IsMember` (a player is a member of his own group) and would
remove the master from his own party; unreachable from the app, reachable from this channel, free to
refuse. **Added in this commit** (`pylauncher/lua/party/dml_uninvite.lua`), and proved live rather
than reasoned:

```lua
    if b == p then
        return refuse(string.format("%s is the master, not a bot in the party", bname))
    end
```

| claim | capture |
|---|---|
| `Jurnaar`'s party was re-staged to three members, `Jurnaar` leading, `IsMember(Jurnaar)=true` | `06-guard.log:7-17`, `06-grouprows-before.txt` |
| the window was proved empty before the whisper | `06-guard.log:30-32` |
| **the SOAP reply, verbatim**: `Jurnaar is not in Jurnaar's party now (Jurnaar is the master, not a bot in the party)` | `06-guard.log:38` |
| **the world's own log line**: `[dml_uninvite] Jurnaar is not in Jurnaar's party now (Jurnaar is the master, not a bot in the party)` | `06-guard.log:41` |
| the group table after is byte-identical — same md5 `eb721f0d…`, `IDENTICAL`, and the master is still in his own group of three | `06-guard.log:45-57` |

**This settles the round-1 review's `__eq` worry for `Player` userdata, in the direction that
mattered.** That review warned that Eluna GUID userdata has no guaranteed `__eq`, so a `~=`
comparison could silently refuse everything; the guard here compares two *Player* userdata with
`==`, and the refusal fired (`06-guard.log:38`) while section 3's honest removal, which reaches the
same lookup, did not (`05-honest.log:32`). So on this fork `b == p` distinguishes the two cases
rather than being always-false or always-true. Measured on this build only.

## 5. The master is a MEMBER, not the LEADER — the edge round 1 named

Round 1, must-fix 1: the first version of the check asked whether the master *leads*; the feature's
own definition (`group_rows_sql`) asks whether he is *in* it.

| claim | capture |
|---|---|
| leadership was handed from `Jurnaar` to `Dalio`; the `groups` row's `leaderGuid` moves from 1 to 4 | `07-member-not-leader.log:11-12` (from 1), `:16-18` and `:29-31` |
| the server then reports `leaderGUID=4 Jurnaar-is-leader=false; IsMember(Jurnaar)=true` | `07-member-not-leader.log:27` |
| the window was proved empty before the press | `07-member-not-leader.log:40-41` |
| **the honest case still succeeds for a non-leader master**, pressed through `InstallParty.remove` — the whole Python half, `dismiss()` included: `Dismissal(removed=True, logged_out=True, sentence='Nore left the party.', …)` | `07-member-not-leader.log:46` |
| **the world's own log lines**: `[dml_uninvite] removed Nore from group` and then `[dml_whisper] Jurnaar -> Nore: logout` | `07-member-not-leader.log:50-51` |
| only `Nore`'s row went; `Jurnaar` is still in the group he does not lead | `07-member-not-leader.log:57-59`, `07-grouprows-after-honest.txt` |

And the negative case through the same seam, in the same run:

| claim | capture |
|---|---|
| the second window was proved empty first | `07-member-not-leader.log:64` |
| `InstallParty.remove('Jurnaar','Zarraden')` returns `Dismissal(removed=False, logged_out=False, sentence="Zarraden was not removed: the server says Zarraden is not in Jurnaar's party now (Zarraden is grouped with someone else)", …)` | `07-member-not-leader.log:67` |
| the world logged the refusal and **no `[dml_whisper]` line follows it** — a bot the bridge refused to remove is not then told to log out | `07-member-not-leader.log:71-73` |
| the group table is byte-identical across that press (`IDENTICAL`), and `Grirmirn`'s party still holds `Zarraden` | `07-member-not-leader.log:78-84` |

That row is the Python half of T13 working live: the marker recognised, the sentence bounded to the
server's own words, no logout whisper, no poll.

## 6. What had to be staged, and what that costs

**No WoW client was driven.** Every one of the 500 characters online on this install is a random bot
(`01-ground.log:83` — 500 online; `03-stage.log:23` — 1001 characters exist, `08-teardown.log:112` —
still 1001 at the end), so the two "masters" here, `Jurnaar` and `Grirmirn`, are bot-driven player
characters. `dml_uninvite.lua` asks `GetPlayerByName` and `Group:IsMember` and cannot tell a
bot-driven `Player` from a client-driven one; what it was tested against is a real core `Group`,
written to `group_member` by the core, which is what every capture above reads. **What is therefore
NOT shown: a person's own character, a party built by the panel, and the panel's own surface.**
Those are T5's folder next door, against the old bridge.

Three routes to a party were tried first and are captured failing, which is why a harness exists:

| route | what happened | capture |
|---|---|---|
| the core's own `.group` commands | `.help group` offers this SOAP caller `group list` alone; `.group join Jurnaar Dalio` answers with the same USAGE and `outcome=no` | `03-stage.log:7-14` |
| — and it is not a rights problem | the channel's account is gm level 3 | `03-stage.log:17-19` |
| the app's own `dml_addclass`, with a bot as master | it ran (`[dml_addclass] Jurnaar ran: .playerbots bot addclass mage`) and made nothing: 1001 characters before, 1001 after, group table still 0 | `03-stage.log:23-34` |
| `.playerbots bot …` directly | Console::No by design — that is the whole reason `dml_addclass.lua` exists | `pylauncher/lua/party/dml_addclass.lua:29-34` |

**A correction.** `03-stage.log:35-36` prints "the module's addclass will not serve one [a bot
master]". That is more than the run measured. What was measured is that it ran and created nothing.
`03b-addclass-cause.log:7-16` shows the second explanation that fits just as well: `RNDBOT0` already
holds 10 characters and this world's `CharactersPerRealm = 10`, so `addclass` had nowhere to put an
alt either way. **The cause was not determined; only the unavailability of the route was.** The same overreach stood in `t13_stage.lua`'s header (lines 16-18 as committed by the hand) and is cut back there too by the lead.

So the two parties were built by `t13_stage.lua`, a harness that is not part of the app, is not in
`party.BRIDGE_SCRIPTS`, registers a command name nothing in the app ever sends, and never calls
`RemoveFromGroup`. It reaches the same Eluna Group API the bridge under test reads. It was copied
in beside the deployed five (`03-stage.log:39-40`, md5 `04f36462…` on both sides), the world was
restarted a second time so it would load (`03-stage.log:58-63` shows the five **and**
`[t13_stage] loaded`), and step 08 removed it and restarted a third time: `08-teardown.log:54-58`
shows the five loading with no `t13_stage` among them, and the world answers
`Command 'dml_t13_stage show Jurnaar Jurnaar' does not exist` (`08-teardown.log:63`).

One more thing a reader should not be surprised by: `seam.members('Jurnaar')` lists `Jurnaar`
himself (`03-stage.log:102`). `group_rows_sql`'s `bot_clause` excludes bots by account prefix, and on
this install the *master* is on an `RNDBOT` account too. That is an artefact of using a bot as a
master, not a defect: for a real master the clause excludes him, which is exactly why the guard in
section 4 is unreachable from the app.

## 7. The box, left as it was

| claim | capture |
|---|---|
| both staged parties were disbanded; `group_member` and `groups` both read 0 | `08-teardown.log:11-24`, and again `:76` after the restart |
| the harness file was deleted from the server directory | `08-teardown.log:26-34` (the listing no longer holds `t13_stage.lua`) |
| the five the app ships are still deployed, and are this ticket's bytes | `08-teardown.log:35-45` |
| the world is up, the bridge answers, all three containers running | `08-teardown.log:52`, `:78-84` |
| the five characters this run touched are all back online, level unchanged | `08-teardown.log:70-74` — `Jurnaar 80`, `Nore 74`, `Dalio 24`, `Grirmirn 69`, `Zarraden 78`, all `online=1` |
| the realm row is exactly what step 01 read — `100.99.204.5` on both address columns, mask `255.255.255.0`, port 8085, flag 0 — never edited, so no authserver restart was owed or made | `01-ground.log:61` vs `08-teardown.log:88`; `ac-authserver` still `started=2026-09-10T10:48:01.8042228Z`, the same value step 01 read (`01-ground.log:64` vs `08-teardown.log:82`) |
| **no account was created, deleted or changed.** 103 accounts at the end, the same three non-bot ids (101 `YULON_243C46E3`, 102 `PERZI`, 103 `YULONADMIN`) with the same gm levels | the before-read of the three ids and their gm levels is `03-stage.log:17-19`; the count and the same three at the end `08-teardown.log:92-102` (no before-count was taken, so "103" rests on the end read plus those three ids) |
| **no throwaway account was needed**: the second master is an existing random-bot character, so nothing had to be created and nothing had to be removed | the account counts above; `03-stage.log:77-93` shows both parties made from existing characters |
| `PERZI`'s `last_login` is unchanged at `2026-09-08 23:24:16`; `PERZI` was never used | `01-ground.log:69` vs `08-teardown.log:106` |
| `Pakka` is unchanged: guid 1001, level 6, offline | `01-ground.log:72` vs `08-teardown.log:108` |
| `LootPet2.lua` is unchanged: 37294 bytes, `Sep 9 02:06` | `01-ground.log:73` vs `08-teardown.log:109` |
| `Logger.ALE=4,Console Server` still at line 706 of `worldserver.conf` | `01-ground.log:74` vs `08-teardown.log:110` |
| the character count is unchanged at 1001 | `03-stage.log:23` vs `08-teardown.log:112` |

The world was restarted three times (steps 02, 03, 08) — allowed for this ticket, and each restart's
own log window is captured. `ac-database` and `ac-authserver` were never restarted
(`08-teardown.log:82-84`).

Every action on the box was announced first in its Claude activity terminal (`lib.sh:22`, called at
the top and bottom of each step script); the window was launched at 18:29 and there is exactly one.

**This lane's own three directories are off the box** (`09-cleanup.log`, 18:56:50): `~/t13-live`
(a copy of this worktree, 12 M), `~/t13-venv` (a throwaway venv holding `pydantic`, `certifi` and
`PySide6`, 676 M, because this box had none of them installed) and `~/t13-out` (the capture files,
92 K, copied into this folder first). All three now answer `No such file or directory`
(`09-cleanup.log:12-14`). Nothing under `~/wowserver` was removed: the five bridge scripts,
`LootPet2.lua` and the pre-existing `LootPet.lua.bak-20260909-002427` are all still there
(`09-cleanup.log:25-32`), and all three containers are up (`09-cleanup.log:22-24`).

**The box's own clone `~/dads-mmo-lab` was never touched**: it still reads `182fc92a` with the one
modification to `pylauncher/yulon/party.py` that was already there before this run
(`09-cleanup.log:18-19`, and the same two lines were read at 18:30 before any work began — that
earlier read is not captured, which is why this row cites the after-state only).

One line in `09-cleanup.log` should not be misread: the `20` at `09-cleanup.log:33` counts
`t13_stage` lines in the world's last thirty minutes, and that window still contains steps 03–08,
when the harness was loaded on purpose. The proof that it is gone is the third restart's own load
list (`08-teardown.log:54-58`) and the world answering
`Command 'dml_t13_stage show Jurnaar Jurnaar' does not exist` (`08-teardown.log:63`).

## 8. Secrets

No password is in this folder. Checked three ways rather than assumed: the folder holds no
`<game>-<16 hex>` shape and no line matching `passw|secret|token|sha_pass|verifier|MYSQL_|_PASS`
except `t13press.py`'s own three variable names; the install's database password (16 characters, read
through `controller_view._db_password` and never printed) appears in none of the capture files; and
of the six fields in the channel's credential file, only `account` and `host` occur in any capture —
`password` occurs in none. The SOAP admin's password was never read into a shell, never echoed, and
`t13press.py` never prints it: the channel is built by `channel_setup.InstallChannel.live_channel`,
which reads it itself (`t13press.py:53-68`).

`tests/test_no_secrets_in_evidence.py` was run against this worktree by the hand before its commit and
again by the lead after this rework; the lead's run is `10-no-secrets.txt`.

## 9. What this folder does NOT show

* **A human master.** Section 6. No client was driven; the win11 VM was not used.
* **The My Party panel.** No panel frame is here — the presses are the seam and the wire, which is
  what the ticket asked for ("the whispers go over SOAP"). T5's folder next door has the panel.
* **A party built by the app.** `dml_addclass` could not make one on this install (section 6), so
  the parties came from the staging harness.
* **Why `dml_addclass` made nothing.** Two explanations fit and neither was excluded (section 6).
* **The bot manager moving a bot across a confirm interval.** Still not stageable, still the reason
  the check lives in the Lua at all; what is proved here is the check, not the race.
* **`remove_all`.** T5's mass path was not pressed in this run; only `remove`, and the wire beneath it.
* **Any of this on another core.** WotLK/AzerothCore with ALE, on this box, on this build.
