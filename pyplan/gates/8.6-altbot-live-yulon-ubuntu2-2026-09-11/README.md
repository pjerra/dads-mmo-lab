# T26 live half — a named character added as a bot, from a real player's own session
### yulon-ubuntu2 and the Hyper-V host, 2026-09-11, 01:30:06–02:29:10 CEST (the box's own clock, read by `date` inside the script that wrote each line — `lib.sh:32`)

**Is the ticket's live half done? Yes, and it took two rounds, because the first
one found a defect in the unit half that no unit test could have found.** All
four claims are pressed through the app's own controls, from a real 3.3.5a
client logged in as a throwaway character on a throwaway account, with the
module's own words photographed in that character's game window. Round 1
(02:04–02:09) proved the *module* accepts all three permission rules and proved
the *app* could not see it: `add_named` reported "no bot by that name joined the
party within 6 seconds" for characters that had joined. Round 2 (02:18–02:21),
after a fix with four named mutations and a green gate, reports
`Tsixalt joined the party.`, `Tsixmate joined the party.` and
`Tsixfriend joined the party.`

**No world restart was needed and none was made.** `restarts=0` on all three
containers at the start (`01-ground.log:15-17`) and the same values at the end
(`09-box-as-found.log`, the container block), with one `StartedAt`,
`2026-09-10T21:46:31Z`, throughout. Section 2 says why the sixth script needed
no restart and how it was proved loaded.

**Every sentence here names the file and line that shows it.** A claim with no
`file:line` after it is a claim this run did not make. Every `docker logs
--since` window was opened with `date -u +%Y-%m-%dT%H:%M:%SZ` (`lib.sh:33`) and
queried BEFORE the action to prove it empty of the pattern — the T3 trap of
2026-09-09 (memory note `docker-logs-since-is-client-local`). Every box action
was announced in the box's own Claude activity terminal before it was taken
(`lib.sh:35`, `10-activity-terminal.txt`, 30 lines, one window).

The box answers to `ssh yulon-ubuntu2`; its own `hostname` is `yulon-ubuntu`,
which is what every capture header prints (`01-ground.log:7`). One box, two names.

**The code this was pressed from** is the tip of `hand-t26`, copied to the box
as `~/t26-live` so every press is this ticket's tree and not the box clone's
older one. Steps 01–06 round 1 ran against `party.py` as the unit half merged
it; step 07 prints the md5 of the file round 2 pressed
(`07-reset.log:66`, `9151f8cc7d2bce32af12f6b869aa66c0`) beside the three lines
the fix added.

## 0. What was pressed, in order

| # | at | what | capture |
|---|---|---|---|
| 01 | 01:48:28–01:49:35 | the box before anything, and the module conf | `01-ground.log` |
| 02 | 01:51:24 and 01:52:50 | three throwaway accounts and four throwaway characters | `02-stage.log` |
| 03 | 01:53:55–01:58:52 | **the client seat**: realmlist, launch, login, into the world | `t26client.log`, `shots/01b`–`03`, `03a-win11-probe.log`, `03b-client-before.txt` |
| 04 | 02:04:21 | **round 1**: the picker, the alt, the online refusal | `04-alt-round1.log`, `shots/09-after-claim1-client.png` |
| 05 | 02:06:59 | **round 1**: the guild mate | `05-guild-round1.log`, `shots/10-after-claim2-client.png` |
| 06 | 02:09:32 | **round 1**: the linked account | `06-link-round1.log` |
| — | 02:12–02:16 | the fix, its four tests, its four mutations, the gate | `12-mutations.txt`, `13-checks-m910q.txt` |
| 07 | 02:17:41–02:18:05 | the box put back to where step 02 left it | `07-reset.log` |
| 04 | 02:18:22 / 02:19:39 | **round 2**: the picker, the alt, the online refusal | `04-alt-round2a.log`, `04-alt.log`, `shots/11-claim1-and-4-client.png` |
| 05 | 02:20:22 | **round 2**: the guild mate | `05-guild.log`, `shots/12-claim2-client.png` |
| 06 | 02:21:02 | **round 2**: the linked account | `06-link.log`, `shots/13-claim3-client.png` |
| 08 | 02:22:45–02:23:03 | the bots, the guild and the link undone | `08-teardown.log`, `shots/14-after-teardown-client.png` |
| 09 | 02:27:14–02:27:23 | the throwaway accounts gone, and every step-01 number read back | `09-box-as-found.log` |
| 10 | 02:27:41 / 02:28:35 | this lane's directories off the box; the box's own activity log | `10-cleanup.log`, `10-activity-terminal.txt` |
| 11 | 02:28:08–02:28:47 | the Hyper-V host put back | `11-vmhost-as-found.log` |
| 14 | 02:29:10 | this folder grepped for secrets | `14-no-secrets.txt` |

Scripts, and they are what ran: `lib.sh`, `01-ground.sh`, `02-stage.sh`,
`03-client.sh`, `04-alt.sh`, `05-guild.sh`, `06-link.sh`, `07-reset.sh`,
`08-teardown.sh`, `09-box-as-found.sh`, the box-side driver `t26press.py`, the
client-side driver `t26client.ps1` with its launcher `t26run.ps1`, and the
mutation harness `12-mut-t26.py`.

---

## 1. The client seat is the Hyper-V HOST, not `yulon-win11`, and that is a deviation with a capture behind it

The brief names `yulon-win11` as the client seat and gives
`vmshot.ps1 -VMName yulon-win11` as the way to photograph it. Three readings
sent this lane to the host instead, and all three are in this folder:

* **`yulon-win11` has no 3.3.5a client.** The only `wow.exe` anywhere on its two
  drives is `D:\clients\WoW-TBC-2.4.3\WoW-Client-2.4.3\Wow.exe`, `version=2, 4,
  3, 8606` (`03a-win11-probe.log`). `D:\clients` otherwise holds two zips.
* **That box drew no WoW window.** The TBC client was launched there into
  session 1 at 01:34 as a renderer probe; two minutes later
  `MainWindowHandle` was `0` and `Logs\gx.log` was **0 bytes**, which it still
  is (`03a-win11-probe.log`, the Logs block). The in-guest screenshot shows the
  desktop with the client's taskbar button and no client window
  (`shots/win11-probe-in-session.png`).
* **`vmshot.ps1` does not photograph that box's desktop.**
  `ssh vmhost 'C:\Users\PK\vmshot.ps1 -VMName yulon-win11'` returned an
  all-black 1024×768 frame (`shots/win11-probe-vmshot-black.png`, 4.6 KB) at the
  same moment the guest's own `CopyFromScreen` returned a 1920×1080 desktop
  (`shots/win11-probe-in-session.png`, 1.5 MB).

The host has the 3.3.5a client 8.4a and 8.5a drove into the world,
`C:\clients\WoW-WotLK-3.3.5a-min`, and it has a real display. Copying a 16 GB
client onto a box that had just failed to draw a smaller one was not worth the
hour. **What this costs:** the client ran on the owner's own desktop rather than
on a test VM, and every client frame in `shots/` is that desktop.

**The client was left exactly as it was found.** `03b-client-before.txt` is the
three realmlist files and the whole of `WTF\Config.wtf` read before anything was
written; `11-vmhost-as-found.log` shows all three back at
`set realmlist 172.30.48.189` and `Config.wtf` restored line for line.

**One change was made on the host and put back.** Its portproxy listeners for
3724 and 8085 pointed at `172.30.48.189`, a lease `yulon-ubuntu2` no longer
holds — it answers on `172.26.8.248` — so a client that authenticated would have
been sent to a world that never answered. The realm row is
`100.99.204.5|100.99.204.5|255.255.255.0` (`01-ground.log:72`), which is the
host's own Tailscale address, so the world had to be reachable *there*. Both
listeners were pointed at `172.26.8.248` for the run and set back to
`172.30.48.189` afterwards (`11-vmhost-as-found.log`, the before/after blocks).
**The realm row itself was never read for writing and never touched**
(`01-ground.log:72` and `09-box-as-found.log:81` are the same three values).

**The client's own chat log is empty and says nothing.** `/chatlog` was turned on
and the client answered *"Chat being logged to Logs\WoWChatLog.txt"*
(`t26client.log`, the 01:58:47 stage), but the file was still 0 bytes when the
client was closed. The chat evidence in this folder is the client-side frames,
not that file.

## 2. The sixth bridge script was already deployed and already loaded, so no restart was owed

The brief allows one world restart to load `dml_botadd.lua`. None was needed:

* the file is on the box at `~/wowserver/env/dist/etc/modules/lua_scripts/`,
  `5005` bytes, dated `2026-09-10 21:32`, md5 `958b0966ec795e655868178a1f643ba1`
  (`01-ground.log:33`) — **the same md5 as this ticket's own tree**
  (`01-ground.log:41`), and the other five match too (`:32-37` against `:40-45`);
* the world running now started at `2026-09-10T21:46:31Z`
  (`01-ground.log:15`), which is *after* that, and the log window opened from
  that instant holds all six `loaded` lines including
  `[dml_botadd] loaded -- named-character add relay ready` (`01-ground.log:49`);
* and it answers: every `add_named` press in this folder put a
  `[dml_botadd] …` line in the world's log — `04-alt.log:96`, `05-guild.log:98`,
  `06-link.log:121` — and its refusing branch printed
  `[dml_botadd] Tsixalt was not added to Tsixmaster's party (Tsixalt is logged in)`
  (`04-alt.log:138`).

T16's own teardown restart of 2026-09-10 23:46 is what loaded it; this lane
found it already in the world and pressed it.

The conf the picker reads is the DEPLOYED `playerbots.conf`, md5
`bd8d55ae591adfed443f174ed1311c61`: `MaxAddedBots = 40`,
`AllowAccountBots = 1`, `AllowGuildBots = 1`, `AllowTrustedAccountBots = 1`
(`01-ground.log:60-65`).

## 3. What had to be made, and why

The measure of 2026-09-10 counted what this box offers a master: same-account 0,
guild 0, linked 0, addclass pool 500, online-and-refused 500. **Not one of the
three cases the ticket is about existed here**, and every offline character
except the owner's own `Pakka` is in the addclass pool, which rule 3 lets
anybody add — so an "allowed" there would prove nothing about rules 1, 2 and 4.
`Pakka` is out of bounds. So they were made, and nothing else was:

* **Three throwaway accounts through the app's own Accounts seam** —
  `wotlk_accounts.create_account`, the function `controller_view` wires:
  `T26MASTER` (id 116), `T26MATE` (117), `T26FRIEND` (118), `joindate`
  23:51:25–23:51:28 UTC (`02-stage.log:23-25`). The password was generated on
  the laptop, passed to the box on stdin and never in argv, never written to the
  box, and is in no file here (`14-no-secrets.txt`). **The owner's `PERZI` was
  never used and its `last_login` is the same before and after**
  (`01-ground.log:74-76`, `09-box-as-found.log:83-85`, both
  `2026-09-08 23:24:16`).
* **Four throwaway characters with the CORE's own `.pdump write` / `.pdump
  load`** (`cs_character.cpp:57-58`, both `Console::Yes`, so the app's own SOAP
  channel can send them): one offline addclass-pool character, `Rinmu` (guid
  501, race 3, class 1, level 1, in no guild — `02-stage.log:7-10`), dumped to
  `t26src.dump` and loaded back four times under new names and new guids —
  `Tsixmaster` (1002) and `Tsixalt` (1003) on `T26MASTER`, `Tsixmate` (1004) on
  `T26MATE`, `Tsixfriend` (1005) on `T26FRIEND` (`02-stage.log:65-68`). A new
  guid is in no pool and in no guild, which the same step counts: `0` rows in
  `playerbots_account_type` for their accounts and `0` in `guild_member`
  (`:70`, `:72`). `PlayerDump.cpp:956` calls
  `sCharacterCache->AddCharacterCacheEntry(...)` on every load, which is what
  `.playerbots bot add` resolves a name through — so no restart was owed for
  them either, and the client listed both of `T26MASTER`'s characters on its own
  character screen (`shots/02-after-login.png`).

**Step 02 ran twice and only the second run is kept.** The first attempt wrote
the three accounts and then failed at `.pdump write /tmp/t26src.dump` with
*"Failed to open file"*: this server ships `PlayerDump.DisallowPaths = 1`
(`worldserver.conf:4688`) and `PlayerDumpWriter::WriteDumpToFile`
(`PlayerDump.cpp:718-721`) refuses any name carrying a slash before it opens
anything. `02-stage.sh:30-37` carries that as a comment and the bare name. The
first run's own log was overwritten by the second and is not reconstructed here;
what it did is readable in the kept run instead — the three accounts are already
there with `joindate` one minute earlier and `create_account` answers
`created=False … keeping its password` (`02-stage.log:13-21`).

**The client was then logged in.** `T26MASTER` at the login screen
(`shots/01b-login-screen.png`, realm "Yulon ubuntu2", version 3.3.5 (12340)),
its two characters on the character screen (`shots/02-after-login.png`),
`Tsixmaster` standing in Coldridge Valley (`shots/03-in-world.png`) — and the
server agreeing: `Connected players: 1. Characters in world: 501.`
(`04-alt.log:10`), where it had been `Connected players: 0` at the start
(`01-ground.log:118`). **That is the thing the measure said this route needs and
this box did not have.**

## 4. ROUND 1 — the module said yes three times and the app could not see it

Every one of the three adds reached the module, logged the character in and put
it in the master's group. Every one of them came back from `add_named` as
`joined=False`.

| claim | press | what `add_named` said | what the WORLD did |
|---|---|---|---|
| 1, the alt | `add_named('Tsixmaster', 'Tsixalt')` (`04-alt-round1.log:92`) | `added=True joined=False`, *"…no bot by that name joined the party within 6 seconds"* (`:94`) | `Tsixalt` online `0 → 1` (`:13` → `:104`); the module's own `add: Tsixalt - ok` / `Tsixalt joins the party.` in the master's window (`shots/09-after-claim1-client.png`) |
| 2, the guild mate | `add_named('Tsixmaster', 'Tsixmate')` (`05-guild-round1.log:94`) | the same sentence (`:96`) | `Tsixmate` online `0 → 1` (`:107`); `add: Tsixmate - ok` / `Tsixmate joins the party.` and the party frame holding it (`shots/10-after-claim2-client.png`) |
| 3, the linked account | `add_named('Tsixmaster', 'Tsixfriend')` (`06-link-round1.log:142`) | the same sentence (`:144`) | `Tsixfriend` online `0 → 1` (`:157`) and `group_member` holding group 2 with `Tsixmaster`, `Tsixmate` and `Tsixfriend` (`:149-152`) |

**The defect.** `add_named` polled `InstallParty.members()`, which is
`group_rows_sql` — and that query keeps only rows `dbreads.bot_clause`
recognises: a bot-registry entry, or a username beginning with
`RandomBotAccountPrefix`. That is right for `add_bot`, whose characters come out
of the module's own `RNDBOT*` pool. It is wrong for this route **by
definition**: the characters T26 exists to add are the owner's own alt, a guild
mate's character, a friend's — every one of them on a person's account, which no
bot marker will ever match. The poll was looking at a list the answer could not
be in.

**The fix**, `c0149d55` on `hand-t26` with `eb2e24ee` for black:
`party_rows_sql(entry, *, master_guid)` is the same query with that clause left
out and the master dropped by guid instead (the one row the marker was keeping
out on this path); `InstallParty.party_members` reads it; `add_named` is handed
that. `members()` is untouched, and a test says so — it is what the panel draws
and what `remove_all` confirms, and a party list offering to dismiss a real
person's character would be a worse defect than this one.

Four tests, four named mutations, **4/4 caught**, and the mutation table's own
`sha256 before`/`after` are both `ff84f58c7104c990`, which is the committed
file's (`12-mutations.txt`). `--checks` on m910q: **ALL GREEN**, 4153 passed, 6
skipped, mypy on three platforms, ruff and black clean (`13-checks-m910q.txt`).

Round 1 is kept rather than deleted: `04-alt-round1.log`,
`05-guild-round1.log`, `06-link-round1.log`. A folder showing only the run that
worked would hide that it took two.

**The box was put back to where step 02 left it before round 2**
(`07-reset.log`): the two bots in the party dismissed through
`InstallParty.remove` — `removed=True logged_out=True`, *"Tsixmate left the
party."* (`:19-23`) — the third logged out by the app's own whisper, the guild
deleted (`guild` back to 20, `guild_member` back to 300, `:37-39`), the two link
rows deleted (`:46`), and the four characters back to one online and three
offline (`:50-53`) with `group_member` 0 (`:55`).

## 5. CLAIM 1 — an alt on the same account, added as a bot, joining

`04-alt.log`, 02:19:39–02:19:43, master `Tsixmaster` (guid 1002, account 116,
online — `:12`), target `Tsixalt` (guid 1003, **account 116, the same account**,
offline — `:13`).

**The picker offered it and named the rule**: `OFFER Tsixalt level 1 T26MASTER
on this character's own account`, with the other two greyed *"not on this
character's account, not in its guild, not an addclass bot, and not on a linked
account"* (`04-alt.log:81-83`) — the module's four rules in the app's voice, and
the only rule that could have admitted `Tsixalt` is rule 1.

**The press**: `InstallParty.add_named('Tsixmaster', 'Tsixalt')` →
`added=True joined=True`, sentence **"Tsixalt joined the party."**, in 2.4
seconds (`:92-94`). The world logged
`[dml_botadd] Tsixmaster ran: .playerbots bot add Tsixalt` in a window proved to
hold none before the press (`:87`, `:96`). `Tsixalt` went online `0 → 1`
(`:13` → `:104`), and `group_member` held group 4 with guids 1002 and 1003
(`:99-101`).

**The two reads side by side, which is the fix in one line**:
`InstallParty.members('Tsixmaster') -> ()` and
`InstallParty.party_members('Tsixmaster') -> (Member(name='Tsixalt', guid=1003,
klass=1, level=1),)` (`:108-109`).

**And in the master's own game window** (`shots/11-claim1-and-4-client.png`):
`add: Tsixalt - ok` / `[Tsixalt] whispers: Hello` / `Tsixalt joins the party.`,
with `Tsixalt` standing beside `Tsixmaster` in the world.

A third press of the same name, while it was in the party, was stopped before
anything was sent: *"Tsixalt is already in Tsixmaster's party, so nothing was
sent. Adding a character that is already there could not be shown to have done
anything."* (`:114-116`).

## 6. CLAIM 2 — a guild mate on ANOTHER account, added as a bot

`05-guild.log`, 02:20:22–02:20:46.

The guild is this lane's staging, made with the core's own two commands over the
app's channel: `.guild create Tsixmaster T26Kinfolk` (`cs_guild.cpp:35`,
`Console::Yes`, and the target must be connected — which is why the master had
to be in the world first) and `.guild invite Tsixmate T26Kinfolk` (`:37`, which
takes a `PlayerIdentifier` and calls `Guild::AddMember(guid)` with no connected
check, so an **offline** character can be put in a guild). Guild 22
`T26Kinfolk`, leader guid 1002 (`05-guild.log:24`), members 1002 and 1004 —
**1004 is on account 117, not the master's 116, and it was offline when it was
put in** (`:26-27`); guild count 20 → 21 and `guild_member` 300 → 302
(`:8-10` → `:29-31`). It is guild 22 and not 21 because round 1 made and deleted
one at 02:06 (`05-guild-round1.log:24`).

**The picker's row changed and nothing else did**: `Tsixmate` moved from *"not
on this character's account, not in its guild, not an addclass bot, and not on a
linked account"* (`04-alt.log:83`) to `OFFER Tsixmate level 1 T26MATE —
T26Kinfolk **in this character's guild**` (`05-guild.log:85`).

**The press**: `add_named('Tsixmaster', 'Tsixmate')` → `added=True joined=True`,
**"Tsixmate joined the party."**, 2.4 seconds (`:94-96`), the world's
`[dml_botadd] Tsixmaster ran: .playerbots bot add Tsixmate` in a window proved
empty first (`:98`), `Tsixmate` online `0 → 1` (`:107`), group 5 holding 1002 and
1004 (`:101-103`), and `party_members` reporting exactly `Tsixmate`
(`:110-111`).

**In the master's window** (`shots/12-claim2-client.png`, and the same frame
again at `shots/13-claim3-client.png`): `Tsixmaster has joined the guild.` /
`Tsixmate has joined the guild.` / `add: Tsixmate - ok` /
`[Tsixmate] whispers: Hi!` / `Tsixmate joins the party.`, with
`Tsixmate <T26Kinfolk>` in the world and in the party frame.

## 7. CLAIM 3 — a character of a LINKED account, with the link written by the panel

`06-link.log`, 02:21:02–02:21:16. The link table was empty before (`:9`).

**Both presses of "Link an account", in the order the panel makes them.** The
first writes nothing and names the consequence:

> This links the account T26MASTER to the account T26FRIEND, both ways. From then
> on this server lets T26MASTER add every character of T26FRIEND as a bot, and
> lets T26FRIEND add every character of T26MASTER. It is two rows in
> playerbots_account_links and nothing else; press again to write them.

(`:18-20`, and the table still `0` at `:22`.) The second writes:
`linked=True`, *"T26MASTER and T26FRIEND are linked: this press wrote both rows…"*
(`:40-42`) — and the table, read by hand and not by the app, holds
`116 118` and `118 116`, and the count is 2 (`:43-47`). A third press: *"…already linked on this
server, so nothing was written."* (`:52-54`).

**Its two refusals were pressed rather than described**: the master's own
account — *"T26MASTER is Tsixmaster's own account, so there is nothing to link…"*
(`:27-29`) — and a name that is not an account — *"there is no account called
NOSUCHACCOUNT on this server… It is the account name, not a character's name."*
(`:32-34`).

**The picker's row changed**: `Tsixfriend` moved from the four-rule refusal to
`OFFER Tsixfriend level 1 T26FRIEND **on an account linked to this one**`
(`:107`), with `Tsixmate` now greyed *"logged in"* — the same two rows this
folder has been watching, each answering for a different rule (`:106-108`).

**The press**: `add_named('Tsixmaster', 'Tsixfriend')` → `added=True
joined=True`, **"Tsixfriend joined the party."**, 2.5 seconds (`:117-119`), the
world's own `[dml_botadd]` line (`:121`), `Tsixfriend` online `0 → 1` (`:132`),
group 5 holding 1002, 1004 and 1005 (`:124-127`), and `party_members` reporting
both bots (`:134-135`). `T26FRIEND` is account 118 — not the master's account, not in the
guild, not in the addclass pool. **The only rule that could have admitted it is
the link this app wrote a moment earlier.**

**In the master's window** (`shots/13-claim3-client.png`): `add: Tsixfriend - ok`
/ `[Tsixfriend] whispers: Hi` / `Tsixfriend joins the party.`, with both bots in
the party frame.

## 8. CLAIM 4 — an online character refused, in the words the code gives

`04-alt.log`, 02:19:47–02:19:54, and the character is the one rule 1 admitted
fifteen seconds earlier.

The order is what makes it a claim about being ONLINE and not about a permission
rule: `Tsixalt` joined (§5), then the bridge's own `dml_uninvite` took it out of
the group and **left it logged in** — `group_member` 0, `Tsixalt` online 1
(`:123-127`), which is exactly why `InstallParty.remove` sends a logout whisper
after it (T13). Same character, same guid, same account, same rule; the only
thing that changed is that it is in the world.

**The press**: `add_named('Tsixmaster', 'Tsixalt')` → `added=False joined=False`
and the sentence

> Tsixalt was not added: the server says Tsixalt was not added to Tsixmaster's
> party (Tsixalt is logged in)

(`:134-136`). That is `add_named`'s refusal arm quoting `dml_botadd.lua`'s own
words verbatim; the script's line is in the world's log for the same window,
proved to hold none before the press: `[dml_botadd] Tsixalt was not added to
Tsixmaster's party (Tsixalt is logged in)` (`:131`, `:138`), and `group_member`
stayed 0 (`:140`).

**And the picker agrees, for the same reason and in its own words**: `GREY
Tsixalt level 1 T26MASTER logged in — the module refuses a character that is in
the world` (`:192`) — `PlayerbotMgr.cpp:685-686`, the short-circuit before any
permission rule.

## 9. The box as left

`09-box-as-found.log` against `01-ground.log`, every row.

| claim | before | after |
|---|---|---|
| accounts | 103 (`01:82`) | 103 (`09:57`) |
| `account_access` | 3 (`01:84`) | 3 (`09:59`) |
| characters | 1001 (`01:86`) | 1001 (`09:61`) |
| online | 500 (`01:88`) | 500 (`09:63`) |
| guilds / `guild_member` | 20 / 300 (`01:90-92`) | 20 / 300 (`09:65-67`) |
| `groups` / `group_member` | 0 / 0 (`01:93-96`) | 0 / 0 (`09:68-71`) |
| `playerbots_account_links` / `_keys` | 0 / 0 (`01:98-100`) | 0 / 0 (`09:38-41`) |
| the addclass and random pools | 50 / 50 (`01:101-103`) | 50 / 50 (`09:72-74`) |
| worldserver / authserver / database | `started=2026-09-10T21:46:31.042646621Z restarts=0` and its two siblings (`01:15-17`) | the same three values, `restarts=0` (`09`, the container block) |
| `PERZI.last_login` | `2026-09-08 23:24:16` (`01:76`) | identical (`09:85`) |
| `Pakka` | guid 1001, account 102, level 6, offline (`01:78`) | identical (`09:87`) |
| the realm row | `100.99.204.5 / 100.99.204.5 / 255.255.255.0` (`01:72`) | identical (`09:81`) |
| `LootPet2.lua` | 37 294 bytes, md5 `06190339…` (`01:68-69`) | identical (`09:78`) |
| `Logger.ALE` | `706:Logger.ALE=4,Console Server` (`01:70`) | identical (`09`, same block) |
| the deployed `playerbots.conf` | md5 `bd8d55ae…` (`01`, §2) | identical (`09:88`) |
| the six bridge scripts | six md5s (`01:32-37`) | the same six (`09`, the bridge block, `dml_botadd.lua` at `:101`) |
| the world answers | `DML-BRIDGE-READY` (`01`, §"the world answers") | `DML-BRIDGE-READY` (`09:109`) |
| the module clone | `b949b50b…`, unmodified (`01:20`) | identical (`09:115`) |
| the panel's nine preconditions | all `ok`, `ready: True` (`01:126-127`) | all `ok`, `ready: True` (`09`, last block) |

**Everything this lane made was removed.** The three throwaway accounts were
deleted with the server's own `.account delete`, which takes their characters
with them — `Account deleted: T26MASTER`, `T26MATE`, `T26FRIEND`
(`09-box-as-found.log:21`, `:25`, `:29`) — and the reads after it find no
`T26%` account and no `Tsix%` character at all (`:30-32`). **No orphaned rows**:
`group_member`, `guild_member`, `account_access`, `realmcharacters` and
`characters` all return `0` for guids 1002–1005 and accounts 116–118
(`:34-47`), and both playerbots tables are empty (`:38-41`).

**Nothing was restarted.** `reload ale` was never sent; the world, the
authserver and the database are the same processes that were running at 01:48,
with `restarts=0`.

`~/t26-live` (410 MB) and `~/t26-out` are gone and the read-back is in the same
capture: `ls` answers *"No such file or directory"* for both and `ls ~/t26*`
finds nothing at all (`10-cleanup.log`). The box's own clone `~/dads-mmo-lab`
still reads `182fc92a` with the one pre-existing modification to
`pylauncher/yulon/party.py` that T13, T16 and the T26 measure all recorded
(`10-cleanup.log`, last two lines).

**The Hyper-V host is as it was found** (`11-vmhost-as-found.log`): both
portproxy entries back at `172.30.48.189`, `pw.txt` gone, the 16 GB tar this lane
made while it was still considering `yulon-win11` deleted, the `T26Client`
scheduled task deleted (`schtasks /query` answers *"The system cannot find the
file specified"*), `C:\Users\PK\t26` removed, and no `wow.exe` running. The
client's realmlist and `Config.wtf` are restored, and `yulon-win11` has nothing
of this lane's on it either: the probe client was closed at 01:37 and
`03a-win11-probe.log` reads no running process.

The Claude activity terminal on `yulon-ubuntu2` was launched into the desktop
session at 01:30 (there was none; three earlier stale `tail` children were
killed so exactly one window remains) and the whole of this lane is
`10-activity-terminal.txt` — 30 lines, 01:30:06 to 02:28:35, and the file's last
line counts the windows: **1**.

## 10. Secrets

No password is in this folder. `14-no-secrets.txt` is the grep: the
`<word>-<16 hex>` credential-file shape returns nothing; `passw|secret|token|
sha_pass|verifier|MYSQL_|_PASS` matches only the word "password" in comments and
in the app's own log line *"keeping its password"*, never a value; and the
throwaway password this lane generated is searched for **by its own value** and
is not here. It was generated on the laptop, handed to the box on stdin
(`02-stage.sh`'s header, and `t26press.py:172` reads `$T26_PW` from the
environment), and written to the host only as `C:\Users\PK\t26\pw.txt`, which
`t26client.ps1` reads once and step 11 deleted. The database root password is
never named: both SQL helpers dereference `$MYSQL_ROOT_PASSWORD` **inside** the
container (`lib.sh:45`), so the value never reaches this shell or any capture
(memory note `gate-logs-carry-generated-passwords`). No conf file was ever
`cat`'d into a log; only key names and their non-secret values were grepped.
`tests/test_no_secrets_in_evidence.py` was run against this worktree before the
commit: 4 passed.

## 11. What this folder does NOT show

* **The panel's own button.** What was pressed is `InstallParty.candidates`,
  `add_named`, `link_plan`, `link_account` and `remove` — the methods
  `party_panel.py` calls — with the app's own channel, SQL reader and SQL writer,
  assembled by `t26press.py:build()` from the same inputs
  `controller_view._for_wotlk` uses. No Qt widget was clicked and no panel was
  rendered; 8.6's earlier folders photograph the panel, this one photographs the
  game.
* **The cap.** `MaxAddedBots = 40` is read and reported and decides nothing —
  the picker's own note says so in every capture, and this lane never had more
  than two bots added at once.
* **Rule 3 and rule 5.** The addclass pool is 500 rows in every picker reading
  and the route through it is 8.6's existing `add_bot`; the random-bot rule is
  never a person's. Neither was pressed here.
* **Cross-faction.** All four throwaway characters are race 3 (Dwarf), copied
  from one source, so every rule was tested within one faction. Nothing on the
  module's add path checks team — read in the 2026-09-10 measure, not measured
  here.
* **A character made by a person at a character-creation screen.** All four were
  made with `.pdump load` (§3). The client logged one of them in and played it;
  it did not create any of them.
* **`joined=True` on a bot from the addclass pool through this route.** Every
  add here is a character on a person's account, which is the case the fix is
  about; the pool case goes through `add_bot` and is 8.6's.
* **A second install, a second core, or any other tree.** WotLK/AzerothCore
  `413bea61`, `mod-playerbots` `b949b50b`, `mod-ale`, this box, this build.

## 12. Deviations, named

1. **The client seat is the Hyper-V host, not `yulon-win11`.** §1, with three
   captures behind it.
2. **The host's two portproxy listeners were repointed for the run and set
   back.** §1. The realm row itself was never touched.
3. **Four characters and three accounts were made and then deleted.** §3 and §9.
   The accounts were made through the app's own Accounts seam; the characters
   with the core's own `.pdump`, because a character can otherwise only be made
   at a client's creation screen.
4. **A guild was made and deleted**, with the core's own `.guild create` /
   `.guild invite` / `.guild delete`. There is no guild control in Yu'lon; this
   is staging, like T16's.
5. **Two rows were deleted from `playerbots_account_links` by hand.** The panel
   writes the link and the module reads it; there is no unlink control, so
   taking it away again is this lane's cleanup and is a `DELETE` of exactly the
   two pairs the write's own readback named (`07-reset.log:46`,
   `08-teardown.log`).
6. **A code fix was made mid-lane**, so the four claims were pressed twice.
   Round 1's three logs are kept and §4 is about them.
7. **Step 04 of round 2 ran twice**, 02:18:22 and 02:19:39.
   `04-alt-round2a.log` is the first: the press is identical and reports
   `joined=True`, but the script's raw group-table read joined `group_member.guid`
   (which is the GROUP id) to `characters.guid` and so printed a stranger's name
   beside the right row count. The query was corrected and the step re-pressed;
   `04-alt.log` is that run and is the one this README cites.
8. **Step 02 ran twice and the first run's log was overwritten.** §3.
9. **The client's chat log is empty.** §1. The chat evidence is the frames.
10. **The trailer on this lane's commits is `Co-Authored-By: Claude Opus 5`**,
    as the brief asks, with no session line and no generated-with footer.
