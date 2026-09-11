# 8.4b's owed re-press — `character rename` on an ONLINE character, on TBC

**Box:** `m910q`, the native CMaNGOS TBC install at `~/tbc-7.4c` (`tbc-db`, `tbc-realmd`,
`tbc-mangosd`). **Client:** the real 2.4.3 client on the Hyper-V host, `C:\clients\WoW-TBC-2.4.3\WoW-Client-2.4.3`,
driven through `schtasks /it`. **Date:** 2026-09-08, 13:19Z – 13:32Z (every clock in this folder
is UTC; the host's own is CET, and the driver was changed to say so — see *What was wrong with the
driver* below). **Tree:** this lane's worktree on `yulon-phase8b`, tarred to `~/gate84b-repress/pylauncher`
and run under `~/gate81b-venv/bin/python`.

## Why this run exists

`checklist.md:2493` says it: 8.4b was ticked with a clause it had not pressed. The retrospective
audit of 2026-09-08 found that its `live-actions.py` sends *teleport*, *set level* and *send gold*
and **not** rename; that the only rename in that folder is on `Bimokk`, offline; and that the
prompt screenshot there came from a separate, untranscribed client run. The entry now names the
gap and says the online rename is owed a re-press on TBC. This is it.

## The clause, and the ground it was pressed against

> *each action performed once on an offline character and **once on an online one** changes the
> named row and shows its effect in the game client — … the rename prompt at the next login.*

The row was read **three times before anything was sent**, because a rename press whose `at_login`
bit is already set is a step whose assertion is true before its action runs, and proves nothing.
`repress84b.py` refuses outright in that case, with that sentence.

| when | reading | where |
| --- | --- | --- |
| 13:19:02Z, before the client was even started | `Ddsgate level=25 online=0 at_login=0` — **rename bit clear** | `transcript.txt` |
| 13:23:33Z, the moment the world said logged in | `online=1 at_login=0` — still clear | `transcript.txt` |
| 13:24:13Z, immediately before the press | `online=1 at_login=0` — still clear | `transcript.txt` |

Between the second and third of those the script holds for 40 s on purpose, so that the client's
own timer takes frames of a character standing in the world **before** the command is sent. It
photographs on a timer and cannot see the other machine; two UTC clocks are what order the two
sides, not a file somebody named "before".

## Every clause of this re-press

| clause | reading | evidence |
| --- | --- | --- |
| the character is logged in, `at_login` bit 1 **clear**, read as ground | `online=1 at_login=0` at 13:23:33Z and again at 13:24:13Z | `transcript.txt`, `1-in-world-nine-seconds-before-the-press.png` (13:24:04Z, `Ddsgate` level 25 in Elwynn Forest, client alive) |
| the press goes through the app | `tab.rename("Ddsgate")` → `done=True` `'Forced rename for player Ddsgate will be requested at next login.'` at 13:24:13Z | `transcript.txt` |
| the row read **while still online** | `at_login` **0 → 1 in the same second**, and still 1 at +5 s, +15 s, +45 s and +105 s, `online=1` at every one | `transcript.txt`, `2-in-world-seven-seconds-after-the-press.png` (13:24:20Z) |
| logged out, and read again | `/logout` typed in the client's own chat line at 13:27:22Z; the row at 13:27:46Z reads `online=0 at_login=1` | `3-character-screen-after-the-logout.png` |
| logged in for the prompt | **"Your name has been flagged for rename — Please enter a new name"**, over a panel reading `Ddsgate / Level 25 Warrior (Ghost) / Elwynn Forest` | `4-the-rename-prompt-at-enter-world.png` (13:30:48Z) |
| and the rename actually completes | answered `Ddsgateb`; the unit frame reads `Ddsgateb`, and the row is `901 Ddsgateb 25 0 0 0` — the flag consumed | `5-in-world-under-the-new-name.png`, `transcript.txt` |

Every screenshot's log line records whether the client process was alive when the shutter fired,
at **every** capture and not only at the last one. It was alive at all of them, under one process
of that name. A client that has died photographs exactly like a client refusing to do something.

## What this run REFUTES, and it is 8.4b's own sentence

8.4b's entry says, of its three online actions, that *"the row moves at logout, not at the press"*,
and offers that as the 8.4a finding holding here. **For rename on this tree it does not hold.** The
row moved **at the press**, while the character was standing in the world, and the logout changed
nothing about it.

The mechanism was read on this box rather than guessed, and it is specific to this command:

    ~/tbc-7.4c/src/mangos-tbc/src/game/Chat/Level2.cpp:3639-3644  (the ONLINE arm)
        PSendSysMessage(LANG_RENAME_PLAYER, GetNameLink(target).c_str());
        target->SetAtLoginFlag(AT_LOGIN_RENAME);
        CharacterDatabase.PExecute("UPDATE characters SET at_login = at_login | '1' WHERE guid = '%u'", …);

The online arm writes the row **itself**, immediately, in addition to setting the in-memory flag.
Level, position and money do not — they live in the player object until a save — which is why
8.4b's three actions left the row unmoved and this one does not. So "the row moves at logout" is a
fact about *those commands*, not about *online characters*, and generalising it would have been
wrong. Where the definition of done asks that the action "changes the named row", **on this tree
this one changes it at the press.**

**The server's own answer says which arm ran**, independently of the row. `LANG_RENAME_PLAYER`
(`mangos_string` 253, *"Forced rename for player %s will be requested at next login."*) is the
online branch; the offline branch answers `LANG_RENAME_PLAYER_GUID` (254), which carries
*"(GUID #%u)"*. The transcript recorded 253 — no GUID — so the online branch is what the server
executed, and that is true even if every row read had been unavailable.

## Two things this gate got wrong first, and how each was caught

**1. The driver typed an apostrophe into the account box.** Inherited from `drive-login-tbc.ps1`:
the command it writes into its `.cmd` file spelled `-NextField '{TAB}'`, and Windows command-line
parsing knows only the double quote, so the script received the literal seven-character string
`'{TAB}'`. SendKeys then typed `'` into the account name, TAB'd, and typed another `'` in front of
the password. The login screen photographed **`TBCGATE'`** and a password one character too long
(`x-the-drivers-stray-quote-in-the-account-box.png`), realmd logged nothing, `Sessions online: 0`
never moved, and the watcher sat in its login wait for ten minutes. This is the same family as
8.5b's unquoted `-Who` arriving as one string, and it fails the same way: it looks exactly like a
wrong password. Fixed by double-quoting; the reason is a comment in `drive-rename-repress.ps1`.

**2. The driver printed a complete transcript of the previous run.** Its wait loop looks for
`closed the client` in the log, and the client script clears that log a few seconds after the task
starts — so the very first check matched the *previous* run's log and the driver returned, three
seconds after asking for a new run, with a plausible seven-minute transcript of a client that had
been shut down twenty minutes earlier. Nothing in it was false; all of it was about a different
run. The driver now deletes the log before `schtasks /run`. An artefact left in place reads as this
run's own answer.

Neither was found by the gate asserting something. Both were found by reading what actually came
back — the account box in the picture, and the clock on the log lines.

## What was left where

* **`m910q` is running TBC** (`~/tbc-7.4c`, `tbc-db`/`tbc-realmd`/`tbc-mangosd` up since 13:01Z),
  and **Tortoise is STOPPED**. `~/tortoise-server` was up when this lane arrived and was stopped at
  13:00Z with `docker compose stop` to make room, one server at a time; it was **not** started
  again, because this box's evidence is about TBC and a swap back would take the server this
  folder's re-run depends on. Whoever wants Tortoise next starts it and stops this one — that is
  the swap, not a restore.
* **`guid 901` is now `Ddsgateb`**, offline, `at_login 0`. It was `Ddsgate` (and `Ddsasd` before
  8.5b). The flag had to be answered: a flagged character cannot enter the world, which is exactly
  what stranded `Ddsasd` after 8.4b. **A later box on this tree looks for `Ddsgateb`.**
* **`TBCGATE`'s password was set through the app's own Accounts button** to `Repress84b1`, verified
  by the app's own reader (*"TBCGATE's stored credential is the one this password makes"*).
  8.4b set it the same way and did not write it down; this folder does (`setpw.py`).
* **The `yulon-rename-repress` scheduled task on `vmhost` was deleted.** No `Wow.exe` is running
  there.

## How to re-run it

On `m910q`, with this worktree's `pylauncher/` copied to `~/gate84b-repress`:

```
~/gate81b-venv/bin/python setpw.py '<a password>'
~/gate81b-venv/bin/python -u repress84b.py --login-wait 600 --logout-wait 600 --press-delay 40
```

and on the Hyper-V host, after copying `client-rename-repress.ps1` and `drive-rename-repress.ps1`
into `C:\Users\PK` — start the watcher **first**, because it reads its ground before the client
exists:

```
powershell -File C:\Users\PK\drive-rename-repress.ps1 `
  -ClientDir 'C:\clients\WoW-TBC-2.4.3\WoW-Client-2.4.3' -Realmlist 100.78.24.50 `
  -Account TBCGATE -Password '<the one above>' -Label repress84b `
  -EnterWorld -RealmStyle -LogoutFromChat -WorldSeconds 195 -WaitSeconds 800
```

then again with `-Label repress84b-prompt -RenameTo <a new name> -WorldSeconds 45` for the prompt.
The character must be **offline with `at_login` bit 1 clear** before the first of those, or the
watcher refuses to press.
