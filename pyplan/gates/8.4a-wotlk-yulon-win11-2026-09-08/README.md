# 8.4a — Play, WoW WotLK — the WINDOWS half

**Where and when.** `yulon-win11`, 2026-09-08 15:22Z – 16:11Z, against a real AzerothCore-with-
playerbots install at `D:\wow-server` (rev `47960183bb03`, 1000 characters, 500 in the world). Code
at `7a3a603f` in `C:\gate\src84a`, venv `C:\gate\venv84a`. The client half ran on the Hyper-V host
with its own 3.3.5a client at `C:\clients\WoW-WotLK-3.3.5a-min`, against this VM at
`172.30.55.155`, driven through `schtasks /it`.

8.4a's line declares the Windows half its own press. `gate84a_win.py` is the Linux gate's script
with its constants changed **plus the ground-clearing the adversarial review of that box demanded
after its script was committed**: the committed Linux script still sets level 60 on whatever level
the character happens to be and sets the rename flag on whatever `at_login` happens to be, so both
assertions can be true before their command runs. Every stage below reads the ground, prints it,
and refuses to run a step whose result is its start state.

**The install had to be brought back before any of this** — 8.9a's Windows half had purged it that
afternoon, and building another would be a source build. The route, the checkpoint, and the two
per-box traps that came back with it are written up in the 8.5a half's README, which ran on the
same restored server an hour earlier. Nothing was built.

## The clauses

| Definition of done | Reading |
|---|---|
| every action on an **offline** character changes the named row | `Aiza`: map **609 → 0**, level **55 → 60**, `at_login` **0 → 1**, mails **0 → 1**. Ground printed before each (`logs/log-gate84a_win-offline.txt`) |
| every action on an **online** character, and its effect in the client | `Amvezuan`, table below |
| a name typed in the wrong case still finds the character | `aiza`, `AIZA`, `aIZA` all found `Aiza`; `NoSuchPersonHere` refused **before the server was asked** — *"there is no character called NoSuchPersonHere on this server"* |
| a nineteen-piece gear set sends the number of mails the button promised | `Alanda`: 19 pieces, the button promised **2**, *"19 items sent to Alanda in 2 mails."*, **2 arrived** |
| the tab draws only what this tree has, and names the character | `Teleport Aiza`, `Set level Aiza`, `Rename at next login Aiza`, `Send gold to Aiza`, `Send Aiza's 12 worn items (1 mail)` — and **`Aiza has to be logged in to be revived`, disabled** |

Tab screenshots: `1-characters.png` (1000 characters, live), `2-chosen.png` (the buttons naming the
chosen one), `3-teleported.png` (the server's own sentence read back), `4-revive-withheld-offline.png`
(the Revive button withheld, with its reason).

## The online arm, in the client

Every one of these was sent from the VM while the host's client had the character in the world.

| what the tab sent | what the world and the client said |
|---|---|
| `teleport name Amvezuan Stormwind` | `pinfo`: **Durotar, Razor Hill → Stormwind City**; the client in the **Trade District** (`5-`, `7-`) |
| `character level Amvezuan 55` | `pinfo`: **60 → 55**; the client's chat *"Congratulations, you have reached level 55!"* and *"server console command level down you to (55)"* (`7-`) |
| `send money Amvezuan "A gift" … 50000` | mail rows **6 → 7**; the ground was read as both, `pinfo` saying **Mails: 0 Read/6 Total** before the press. The mail table afterwards carries four `A gift` rows at `money = 50000` copper each — the app's 5 gold, converted once (`logs/the-mail-rows.txt`) |
| `revive Amvezuan` | `pinfo`: **Alive ?: No → Yes**. The ground is real: the character was **dead** first (`6-`) |
| `character rename Amvezuan` | *"Forced rename for player Amvezuan will be requested at next login."*, and at the next Enter World **"Your name has been flagged for rename — Please enter a new name"** over a panel reading *Amvezuan, Level 55 Death Knight, Stormwind City* (`9-`). Answering it renamed the character: the portrait then read **Amvezgate** (`10-`) |

The rename is **last**, because answering the prompt consumes it and the character cannot enter the
world until it is answered.

## Four things this box measured rather than inherited

**1. `saveall` answers "All players saved." and does not write a live player's level.** This is the
one that would have made the gate fail on the app's behalf. The Linux box's script nudges the row
along with `saveall` and asserts on it; here the world reported level 61, **four `saveall`s over
twenty seconds left the row at 60**, and the row caught up **178 seconds later with nothing asked of
the server at all** — at `logout_time`. `the-row-and-the-world.py` and `watch_row.py` are those two
probes, `logs/log-row-and-world.txt` and `logs/log-watch-row.txt` their output. So the online arm's
live reading is `pinfo` — the server answering about its own world — and the row is read after the
logout that writes it (`stage_logout`, which **refuses to run while the character is still online**;
it did refuse once).

That refusal really happened, and **the log does not carry it**. The first attempt ran at 15:59:55Z,
four seconds after the client was killed, and stopped with `AssertionError: the character is still
logged in; the row is not written yet`; the second run, two minutes later, wrote over the same log
file, so `logs/log-gate84a_win-logout.txt` holds only the pass. It is recorded here in the session's
own words rather than pointed at in a file that has not got it — the runner takes a `-Label` for
exactly this and it was not used on that pair.

The row after the logout: **level 55, map 0, at_login 1, `logout_time 2026-09-08 15:59:57`.** Every
online action, in the row, written at the logout.

**2. `die <name>` is not a command on this tree, and the client's `.die` needs a selection.** The
console answered *"Command 'die Amvezuan' does not exist"* — a SOAP session selects nothing, so the
one command that kills cannot be reached from the console. The kill is therefore the client's: F1 to
target self, then `.die` from the character's own chat line, on an account the **app's own 8.3a
button** put at GM level 3 (`Account(id=102, username='GATE84W', gm_level=0)` → `set_gm_level: True
You change security level of account GATE84W to 3.` → `gm_level=3`; that press was a one-liner
rather than a gate stage, so its exchange is in `logs/gm-level-for-the-die.txt` and named as a
console capture). One run sent `.die` with
nothing selected and the server answered *"You should select a character or a creature."* — which
photographs exactly like a character that refused to die, and is why the F1 is in the script.

**The `online` stage's log carries the line `revive : killing the character first`.** That wording
is the script's older one and it is wrong — the client had already done the killing, and the
`die Amvezuan` that follows it is the probe whose refusal is the measurement above. The script now
says so; the log is left as it ran rather than re-recorded.

**3. This client shows the rename prompt at Enter World, NOT at the character screen.** Both were
photographed: `8-client-character-screen-no-prompt.png` is the character screen with the flag
already set and no prompt on it; `9-client-rename-prompt.png` is the same login one press later.
8.4b found the same on its 2.4.3 client; this box asked rather than inherited, and asked it the
other way round as well.

**4. There is no character here that is not a bot, and that had to be made.** 1000 characters, 1000
of them bots — the 8.5a half counted it an hour earlier. 8.4a already measured why a bot is no good
for the online arm (a taken-over bot account never reaches the world, and an online bot's level is
rewritten by the module within the minute), so `gate84a_win_player.py` makes one: the account
`GATE84W` through the **app's own `create_account`**, and then one offline bot character's `account`
column moved onto it **by hand**. That move is the box's setup and not one of its clauses, and it is
written down because it changes what the 8.5a half counted: 1000 of 1000 became **999 of 1000**, and
that re-read is in the other folder.

## What is NOT proved here

* **The mail's client-visible effect was not photographed.** The mails are delivered (seven rows,
  four of them `A gift` at 50000 copper, `deliver_time` in the past) and the world's own `pinfo`
  counts them, but no minimap envelope appears in any frame of the runs that were taken. The
  reading for this clause on this box is the mail rows and the server's own count, not a picture.
  8.4a's Linux half has the envelope; this one does not, and says so. The delivery timer was ruled
  out (`logs/the-mail-rows.txt`); what is left unexplained is the client's, not the app's.
* **Three empty mail rows** landed on the character during the session, one within a second of each
  teleport, and nothing this app sends creates one. `logs/the-mail-rows.txt` records them without
  attributing them to anything.
* The **item search** and **item mail** controls were not pressed separately — the gear set exercises
  the same `mail_items` path with the cap, and that is what was gated.

## The prerequisite: the command channel

On Windows the console cannot send (8.2b: no pty), so the channel is the tab's only route.
`gate84a_win_channel.py` is 8.2b's `press`/`prove` pair against this install: refused while running
with the app's own sentence, then `AC_SOAP_ENABLED "1"`, `AC_SOAP_IP "0.0.0.0"`, `AC_RA_ENABLE "0"`,
`AC_AI_PLAYERBOT_COMMAND_SERVER_PORT "0"`, then a start and a verify.

**It walked into the trap `ensure()` documents, and the repair is what got it out.** The first
`prove` ran three tries in fifteen seconds against a world that needs a minute and a half, gave up
correctly and saved nothing; the run after that generated a new password while `create` kept the
existing account's, so the round trip was a 401 — `Refused`, not silence — and `repair()` rotated the
password and verified at once. `logs/log-gate84a_win_channel-*.txt` has all three runs.

## The files

* `gate84a_win.py` — the gate. `pick | offline | online | logout | case | gear | withheld | takeover | shots`
* `gate84a_win_channel.py` — the command channel: `press | prove | settle | state`
* `gate84a_win_player.py` — the box's setup: `account | adopt | state`
* `gate84a_win_realm.py` — the realm address, through the app's Networking plan/apply
* `the-row-and-the-world.py`, `logs/log-watch-row.txt` — the two `saveall` probes
* `client-play.ps1` — the client driver: focus by handle, every `Data\<locale>\realmlist.wtf`, a
  holding loop that photographs at a fixed interval and **logs whether the client is alive at each
  capture**, and the optional `-Die` and `-AnswerRenameWith` steps
* `run.ps1` — the runner (`PYTHONIOENCODING=utf-8`; without it a `→` in the app's own output kills
  the script on this box's cp1252 console)
* `logs/`, `shots/`

## How to re-run it

```
powershell -File C:\gate\run.ps1 -Driver gate84a_win -Stage <stage> [-Extra <name>]
powershell -File C:\Users\PK\client-play.ps1 -ClientDir C:\clients\WoW-WotLK-3.3.5a-min `
    -Realmlist 172.30.55.155 -Account GATE84W -Password "gate84w-p@ss" -Label <label> [-Die]
```
