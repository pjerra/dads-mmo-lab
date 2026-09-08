# 8.4c — Play, WoW Vanilla — server side gated live on m910q, 2026-09-07

Every server-side clause is green against the running stack. The client half is
a separate lane and this folder ends with what that lane needs.

Server: the Vanilla install on `m910q` (`~/vanilla-75b`, CMaNGOS Classic 0.18,
Classic DB 1.12.1 "Melting Pot v2", world DB z2815). 901 characters, 500 of them
bots that were logged in when the gate ran. `transcript.txt` is one end-to-end
run of every stage, each with its own exit code; the screenshots are the real
`ControllerView`, rendered offscreen against this server.

## The reason this box exists: the mail cap, measured

The catalog carried `mail_item_cap: 1` as a **source-cited prediction** — this
tree's own `src/game/Mails/Mail.h:49`, where the TBC install's identical header
reads 12. A prediction the server has never been asked is not a measurement.
Three sends, because on this tree an unknown item id and an over-cap send take
the same exit — a `return false` that reaches SOAP as a closed connection with
nothing in it, so two probes could not tell "the cap is one" from "the second id
does not exist":

    1 item  (2589:1)        -> yes: 'Mail sent to Asdff'      mails 21 -> 22
    1 item  (3110:1)        -> yes: 'Mail sent to Asdff'      mails 22 -> 23
    2 items (2589:1 3110:1) -> connection closed, no answer   mails 23 -> 23

(the numbers are the shipped transcript's, on a mailbox this gate had already
filled; the first run of the same probe, at 21:06Z on an empty one, read
0 -> 1, 1 -> 2, 2 -> 2)

**The cap is 1, and the prediction holds.** Both ids exist in this server's
shipped world DB (Linen Cloth, Tunnel Rat Ear), and each arrived alone, so
nothing about the ids explains the refusal of the pair. The number is not only
written down: the app's own command builder refuses the two-item line with *"this
server carries at most 1 item in one mail"* before anything is sent.

## The clauses

| clause | reading |
| --- | --- |
| every action on an OFFLINE character (`Asdff`) | `tele name` moved it map 0 → map 1, level 53 → 60, `at_login` 0 → 1, mails 23 → 24 |
| every action on an ONLINE character | the same four, each after a `saveall`, on a bot that had never been renamed |
| revive, ONLINE, with a corpse | the corpse sat still for 12 s untouched and was gone within 12 s of the command — reproduced four times, on `Stelle`, `Sekihr` (twice) and `Calene` |
| revive, OFFLINE, with a corpse | **it works here**, which refutes the app — see below |
| a name typed in the wrong case | `asdff`, `ASDFF`, `aSDFF` each answered *"You changed level of Asdff to 60"*; a name nobody has was refused before the server was asked |
| the gear arithmetic | 12 pieces → the button promised 12 mails → 12 arrived, and the fullest of them holds one item |
| the same on the client lane's character | 3 pieces → 3 promised → 3 arrived |
| the equipped read | 12 of 12 ids resolve in `mangos.item_template`; 0 of the 12 instance guids do |
| the realm and the password | a loopback plan, then a lan plan, then a password — three presses of two other features |

### The two-mail proof this box's line asks for

The box says *"a two-item send arrives as two mails and the button says so before
the press"*. Both halves are in the capture. The button, before anything is
pressed: **"Send Atheata's 12 worn items (12 mails)"** (`3-the-promise.png`).
The press: `12 items sent to Atheata in 12 mails.` Then the count, from the
database: 12 mails arrived, 12 of them carry items, and

    SELECT MAX(n) FROM (SELECT COUNT(*) AS n FROM mail_items ... GROUP BY mail_id)  ->  1

so no mail on this tree holds two of anything.

**Nineteen pieces do not exist on this server.** 8.4b's clause names nineteen
because that is what a WotLK-shaped bot wears; the best-dressed character here
wears twelve, and nothing can raise that without writing inventory rows by hand,
which would be the gate inventing its own evidence. What the clause is about is a
set that does not fit in one mail, and on a cap of one that is any set of two or
more. It was pressed at twelve and at three.

## Measured on this tree rather than inherited

**The teleport verb, by use and not by reading.** Both were sent, from a
character whose position neither send could have produced:

    'teleport name Asdff Stormwind' -> closed the connection; the row did not move
    'help teleport'                 -> 'There is no such command'
    'tele name Asdff Stormwind'     -> 'You are teleporting Asdff (offline) to Stormwind.'
                                       (1629.36, -4373.39) map 1 -> (-8833.38, 628.628) map 0

**The equipped column.** `character_inventory` here is
`guid, bag, slot, item, item_template`, so the read is flat. The claim under test
is not that the two catalog shapes differ — on this tree they cannot — but that
the column NAME is right, and resolution is what shows it: all 12 ids the app
answers resolve in `mangos.item_template`, and all 12 values of the neighbouring
`item` column (154831…154842) resolve as nothing at all.

**`revive` takes a name**, whatever its help says: *"Syntax: .revive — Revive the
selected player. If no player is selected, it will revive you."* — and
`revive <name>` works.

**A save is not optional for an online character.** `character level` answered
*"You changed level of Aliran to 60"* while the row still read 6; the row is the
world's only after `saveall`.

## Two defects this gate found, both fixed

### 1. Two characters, one name

`stage_pick` chose the best-dressed online character and the very next read died:

    ERROR 1242 (21000): Subquery returns more than 1 row

This server holds two characters called **Joleta** (guids 186, 804) and two
called **Dalnaal** (255, 446). `characters.name` here is `idx_name`, a NON-unique
index, and the bot generator collided. `play.equipped` keyed the inventory off a
scalar subquery on that column, so it did not answer a wrong set — MySQL refused
the statement — and `ControllerView._gear_set_size` swallowed that into
**"Dalnaal is wearing nothing"** with the button greyed out, for a character
wearing a full set. A false sentence about the character in place of a true one
about the server.

Fixed: the owner is joined in and its guid comes back beside each item, so the
ambiguity arrives in the answer instead of in the database engine; `equipped`
raises a named `play.Ambiguous`; `send_gear_set` turns that into a refusal rather
than a traceback; and the tab draws the reason. Live, on this server
(`4-two-of-one-name.png`):

    the tab's gear button on Dalnaal reads: '2 characters here are called Dalnaal' (enabled=False)

with the whole sentence — the guids, and why nothing here can pick between them —
in the tooltip (the live stage reads the label; the tooltip is asserted by
`test_a_gear_set_that_cannot_be_read_says_why_rather_than_wearing_nothing`). The first version put all 180 characters on the label and it ran
off the end of the window; that photograph is what made the two lengths.

### 2. The revive button was greyed out for a command that works

The tab disabled Revive for every offline character, citing a measurement:
8.4a and 8.4b read `characters.health` before and after, saw 0 and 0, and
concluded the command does nothing to a character who is not logged in.

It does. This tree's `HandleReviveCommand` has an offline branch
(`Level3.cpp:3281-3297`, in `offline-revive.txt`):

    else
        // will resurrected at login without corpse
        sObjectAccessor.ConvertCorpseForPlayer(target_guid);

`health` is exactly the column that branch does not touch. Measured live at
21:08:27–21:08:40Z on `Koldoum` (guid 407): one corpse row, still one after
twelve untouched seconds, none within twelve seconds of the command — and at
21:14Z, six minutes later, `Koldoum` was still `online = 0` with no corpse, so it
never logged in and nothing but the command removed it.

Fixed as a per-tree fact rather than a constant: `Play.revive_offline` is `true`
on `wow-vanilla`, measured here, and null on every other tree — whose own boxes
have not looked again. The button follows the entry. Live, in the real view:
`button: 'Revive Asdff' enabled=True` on an offline character
(`2-chosen.png`). The same code on the WotLK entry still greys it, for the honest
reason that nobody has looked there —
`test_revive_is_not_offered_offline_on_a_tree_that_has_not_measured_it`.

**The honest limit on that reading.** The ONLINE half is reproduced four times.
The OFFLINE half is **one** reading. This server's only offline corpse was the
one it loaded at start-up, the measurement consumed it, and no other appeared in
about twenty-five minutes of polling (`corpse-watch.py`) — bots die and revive
themselves in seconds, and none of them logged out dead. What would settle it is
the client lane: log `Asdff` in, die, log out, then press the button.

## The identical source on the TBC install

`~/tbc-7.4c`'s `HandleReviveCommand` is byte-for-byte the same function
(`offline-revive.txt`). So 8.4b's conclusion is very likely wrong there too, for
the same reason. It is **not** changed here: that is 8.4b's box, its server was
stopped to make room for this one, and a fact carried across trees on a source
reading is exactly what this phase refuses. The prediction is written down; the
press belongs to whoever restarts that stack.

## Every mistake this gate made

1. **It asserted something that was already true.** The takeover stage began
   `assert REACHABLE not in before` and stopped at once — the realm row already
   advertised `100.78.24.50`, applied by an earlier gate, so the press would have
   "passed" on a state it did not produce. The ground is now made by the same
   feature: a loopback plan first (`realmlist → 127.0.0.1`), then the lan plan.
   Two presses, each grounded by the other.
2. **It picked a character with an ambiguous name**, which is how defect 1 was
   found — and then kept doing it, because `row()` reads `WHERE name = ...` and
   would have mashed two rows into one dict. Every pick now excludes names more
   than one character has, and the ambiguity has a stage of its own.
3. **It cleared a rename flag in the row on a character the world was holding.**
   The next `saveall` wrote the player object's own flag straight back and the
   clause failed with *"the rename flag is ALREADY set"*. For an online
   character the row is not the ground; there is no command that clears the
   flag, so the online stage now asks the server for a character that has never
   been renamed, and cannot be re-run on the same one.
4. **It read a corpse by the wrong column.** `corpse.guid` is the CORPSE's guid;
   the column naming the player is `player`. Caught by the adversarial review
   before it ran, not by the server.
5. **It nearly believed a corpse that vanished on its own.** The first ONLINE
   attempt after the fix found `Arteric`, and the corpse was gone before the
   command was sent — a bot revives itself. Every corpse is now watched for the
   same window before the command as after it, and a corpse that goes early
   makes the stage say NOT RUN rather than pass.
6. **It scp'd four files into the wrong directory**, dropping `catalog.py` and
   `catalog.json` beside the `yulon/catalog/` package where they would shadow it.
   Removed before anything imported them.
7. **It shipped another session's half-finished file to the box.** This checkout
   is shared, and `controller_view.py` had 300 lines of somebody else's
   in-flight manifest-prompt work in it; copying the whole file to `m910q` broke
   the import (`cannot import name 'PendingSql'`). The live proof was re-taken
   against `HEAD` plus this gate's own three edits and nothing else.

## Tests

`pytest -m "not integration"` on the laptop: **3291 passed, 5 failed**, and none
of the five is this gate's. `test_controller_wow_tbc` and
`test_controller_wow_vanilla` fail inside another session's uncommitted
`docker.py` (314 lines added), shelling out to the real docker CLI through a
seam the conftest guard refuses; `test_patch` and `test_extract` fail on CRLF, in
files nobody has modified. The files this gate changed —
`test_play_reads.py`, `test_characters_tab.py`, `test_play_actions.py`,
`test_catalog*.py`, `test_controller_view.py` — are 314 passed, 0 failed.

Each new test was checked against the mutation it exists for: with
`if len(worn) > 1` neutered, `test_two_characters_with_one_name_are_named_rather_than_guessed_between`
fails "DID NOT RAISE"; with the two view branches neutered, all three new tab
tests fail. `__pycache__` was purged on both sides of each mutation.

## What the client lane is handed

The realm advertises **100.78.24.50** (m910q's Tailscale address; the LAN address
`192.168.10.134` is not reachable from the Hyper-V host, and that trap cost 8.4b
an evening). Both the loopback and the lan plan reported `restart required:
True`; realmd was not restarted and the row is what the client reads.

| what | value |
| --- | --- |
| account | `VANGATE` |
| password | `gate84c-p@ss` (set through the app's own 8.3a button; the stored verifier changed) |
| character | `Asdff`, guid 901 — the one character here a person owns |
| where it stands | level 60, map 1 (Orgrimmar), `at_login = 1` so the rename prompt is waiting, 27 mails in its mailbox including a gold mail and three one-item gear mails |

A bot's account is not a spare seat: the playerbots module owns those sessions
and a taken-over bot account authenticates and never reaches the world.

Three things for that lane to measure, none of them assumed here:

1. **The 1.12.1 path from the realm list to the character screen is unmeasured.**
   8.3c drove this client only as far as the realm list, and 2.4.3's
   Development / Suggest Realm / Accept panel is a TBC thing that may not exist
   here.
2. **The two-mail proof, seen.** The mailbox should show separate letters with
   one item each, not one letter with three.
3. **The offline revive, deterministically.** Die, log out, press Revive on the
   offline row, log back in — the one clause here that rests on a single reading.

## Files

* `gate84c.py` — the gate, twelve stages, run one at a time
* `transcript.txt` — one end-to-end run of all twelve, each with its exit code
* `offline-revive.txt` — the offline branch of `.revive` on both CMaNGOS
  checkouts, and `Koldoum`'s row six minutes after the command
* `corpse-watch.py` — the bounded poll that looked for another offline corpse
* `1-characters.png`, `2-chosen.png` — the tab, and `Asdff` chosen with Revive
  enabled
* `3-the-promise.png` — the gear button promising twelve mails before the press
* `4-two-of-one-name.png` — the button on `Dalnaal`
