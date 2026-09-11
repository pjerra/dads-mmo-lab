# 8.4c, the client half — one real 1.12.1 session, 2026-09-07/08

The server side of this box was green when this lane started and its README
ends with a handover. This is what the handover was spent on: one session of the
real 1.12.1 client, driven from `ssh vmhost`, against `m910q`'s running Vanilla
stack. It closes 8.4c's last clause, settles the one thing 8.4c listed as
blocked, and answers 8.5c's clause 4 from the same character (that half is
written up in `../8.5c-vanilla-m910q-2026-09-07/CLIENT-LANE.md`).

Times below are **UTC**. The driver's own log stamps local time and writes a
`Z` on it; `client-transcript.txt` says so at the top.

## What the client proved

| clause | what the client showed | file |
| --- | --- | --- |
| the character is at the destination | the character list read **Orgrimmar**, and the world read **Valley of Strength** — 8.4c's last `tele name … Orgrimmar` | `5-client-level-60-orgrimmar.png`, `8-client-in-world.png` |
| the level is on the character frame | **Level 60 Warrior** on the list, **Level 60 Tauren Warrior** on the character sheet | `5-`, `9-client-character-sheet.png` |
| the mail is in the mailbox | the Orgrimmar mailbox opened on an inbox of `Your gear`, `A gift` and `cap 1` letters — this box's own sends, read as a player reads them | `13-client-mailbox.png` |
| **a two-item send arrives as two mails, and the button says so first** | the promise changed from 3 to 2 because the CLIENT unequipped a piece, then two letters arrived holding one item each | `10-` … `15-` |
| the rename prompt at the next login | twice: once from 8.4c's own offline press, once from a press made in this session | `6-`, `20-` |
| **revive on an OFFLINE character — 8.4c's blocked clause** | died, released, logged out as a **Ghost**, was revived offline, and logged back in **alive** | `16-`, `17-`, `18-` |

And one that nobody asked for but the client volunteered: with the character
standing in the world, the app's presses arrive as sentences in its chat log —
*"server console command level up you to (60)"* and *"You are being teleported
by server console command."* (`19-client-sees-the-commands.png`). 8.4b saw the
effects; this shows the game naming the cause.

## The two-mail clause, and why the client had to be in it

8.4c pressed the gear-set button at twelve pieces and at three. It could not
press it at two: the app has no unequip, and writing `character_inventory` rows
by hand would have been the gate inventing its own evidence. The client can, and
the sequence is the point — every reading has a ground the next step did not
produce.

| time (UTC) | where | reading |
| --- | --- | --- |
| 21:56:30 | m910q | `saveall`; rows `3:6125, 6:139, 15:2361`; the button reads **"Send Asdffrenamed's 3 worn items (3 mails)"** (`10-app-promises-three.png`) |
| 21:58–22:09 | client | the shirt (`Brawler's Harness`, 6125) taken off and put in the backpack (`11-client-shirt-unequipped.png`) |
| 22:09:46 | m910q | `saveall`; rows `6:139, 15:2361`; the button now reads **"Send Asdffrenamed's 2 worn items (2 mails)"** (`12-app-promises-two.png`) |
| 22:09:58 | m910q | pressed. `2 items sent to Asdffrenamed in 2 mails.` mails 27 → 29, and `MAX(items per mail) = 1` |
| 22:20–22:23 | client | the mailbox opened; letter 1 holds **Battleworn Hammer** and nothing else, letter 2 holds **Brawler's Pants** and nothing else (`14-`, `15-`) |

The number on the button is not a constant and not a cache: an ordinary player
action changed it, and the two mails that arrived carry exactly the two pieces
that were still on the character when the button was pressed. `two-mails.py` is
the script; it reads the label off the real `ControllerView` rather than calling
`gear_set_size`, because 8.4c's own defect lived entirely in the gap between the
two.

**A false step on the way, recorded because the screenshot lied.** The first
attempt at the unequip was a RIGHT-click on the equipment slot. The slot drew
empty, the paper doll redrew bare-chested, and the frame looked exactly like a
successful unequip — and the item was still on. The tooltip came back
(`Brawler's Harness / Shirt`), the backpack never received it, and
`character_inventory` still held slot 3 after a `saveall` and again after a
logout. What works on this client is the two-click pick-up: left-click the
equipment slot, then left-click an empty bag slot. Had the gate trusted the
photograph, it would have pressed a "2 mails" button that was still saying 3.

## The offline revive, settled

8.4c's README says what would settle its one-reading clause: *"log Asdff in,
die, log out, then press Revive."* Done, and every step of the ground was made
rather than found.

| time (UTC) | step | reading |
| --- | --- | --- |
| 22:25:40 | ground | level 60, health 2760, online, **0 corpses** — so a corpse afterwards cannot be somebody else's leftovers |
| 22:25:56 | app press | `set level 1` then `tele name … BlackrockStronghold`; the client printed both back (`16-client-dead.png`) |
| 22:26:13 | died | health 0 at Blackrock Stronghold, 17 seconds after arriving |
| 22:27:01 | client | **Release Spirit** pressed — an ordinary player action, and the only thing that makes a corpse row |
| 22:27:16 | row | `corpses = 1`, `corpse_type = 1` |
| 22:28:16 | client | `/logout`; **online = 0**, corpse still there. The character screen reads **"Level 1 Warrior (Ghost)"** (`17-client-ghost-offline.png`) |
| 22:28:26–39 | watch | the corpse sat still for **12 untouched seconds** — the control 8.4c added after a bot's corpse vanished on its own |
| 22:28:42 | app press | Revive, on the offline character: `done=True`, and `corpses 1 → 0` within twelve seconds, with `online` still 0 |
| 22:30:54 | client | logged back in: **alive**, solid, health 84/84, no Release Spirit dialog, no ghost-blue world (`18-client-alive-after-offline-revive.png`) |

So the app was right and 8.4a and 8.4b were wrong, and now for a reason anybody
can see rather than a source reading: `revive` on a character who is not logged
in removes the corpse and resurrects them at their next login. `characters.health`
is untouched by that branch, which is why the health-only readings on the other
two trees concluded the opposite. `Play.revive_offline = true` on `wow-vanilla`
is now measured twice on two different characters, the second one made to order.

The corpse for it did not have to be waited for. 8.4c polled for twenty-five
minutes and never saw another, because bots die and revive themselves in
seconds. Two presses of Yu'lon's own buttons produce one on demand: drop the
level to 1, teleport somewhere lethal, and the world does the rest in under
twenty seconds. `offline-revive-live.py` is that, with the refusals in it — it
will not run the watch stage without a corpse, and it will not run the revive
stage on a character that is online.

## The rename, twice, and a race worth knowing about

The first prompt was 8.4c's: it had pressed `character rename` on the offline
character earlier in the evening, and the prompt was waiting at the first
**Enter World**; it was photographed at 21:51:24Z (`6-client-rename-prompt.png`). Answering it renamed `Asdff` to
`Asdffrenamed`, and the row read `at_login 0` immediately afterwards.

The plan says leave the rename for last. On this client it cannot be deferred —
the prompt stands between Enter World and the world — so it was spent to get in
and then **made again at the end**, which is the stronger reading anyway because
both halves are inside one session:

    before press      : Asdffrenamed|60|1|0        <- online, at_login 0
    rename            : True 'Forced rename for player Asdffrenamed will be
                              requested at next login.'
    row, NO saveall   : Asdffrenamed|60|1|1
    row, AFTER saveall: Asdffrenamed|60|1|1        <- it survives the save

and then, at 22:33:53Z, Enter World put the prompt up
(`20-client-rename-prompt-second.png`). The row is read both before and after
the `saveall` on purpose; the reason is the next paragraph.

**A race that makes the button lie, found by accident.** The first press of the
three-press restore (`set level 60`, `tele name … Orgrimmar`, `rename`) answered
*"Forced rename for player Asdffrenamed (GUID #901) will be requested at next
login."* — and the row read `at_login 0` after the next save. The second press,
a minute later, answered the same sentence **without the GUID**, and that one
stuck. The two sentences are this core's two branches: with a live player object
it names the player, without one it names the guid, and only the first branch
sets the flag on the object that the next save writes. A `rename` sent within a
second of a `tele` takes the offline branch, because a far teleport takes the
player out of the world while the client loads — so it writes the row, and the
session's next save quietly puts it back. **The button reports success either
way.** The only tell is which sentence came back, and Yu'lon does not read it.

That is the same mechanism 8.4c recorded from the other side ("a row UPDATE
clearing it is written straight back by the next save"), now seen setting rather
than clearing, and it is a real hazard for anyone who presses two of these
buttons in a row.

## Where the state was left

`Asdff` again — the prompt at the end was answered with the original name, so
the handover in `README.md` still reads true. Read back at 22:41:22Z: account
`VANGATE`, password `gate84c-p@ss`, character `Asdff` (guid 901), **level 60,
map 1 (Orgrimmar), `at_login = 0`, online = 0, health 2760/2760**, 29 mails, 0
corpses, and **still wearing two pieces** — see below. The realm still
advertises `100.78.24.50`, checked reachable from the client box at about 21:41Z
before anything was launched (`Test-NetConnection … 3724` and `… 8085`, both
True) — so 8.4b's trap was not present tonight and no Networking press was
needed.

### The gear button copies rather than moves, and its label does not say so

Worth writing down because this lane nearly wrote the opposite: after
*"2 items sent to Asdffrenamed in 2 mails"*, the character was **still wearing
both pieces**. `character_inventory` reads `equipped = 2` at 22:41:22Z, hours
later, and the mailbox holds a Battleworn Hammer and a pair of Brawler's Pants
that did not come off anybody.

The reason is the command underneath: `send items <name> <subject> <body>
<itemid>:<count>` builds items from TEMPLATE ids, so what arrives is a fresh
copy of each worn item and the worn ones are untouched. The button reads
**"Send Asdffrenamed's 2 worn items (2 mails)"**, which a person reading it
would take to mean their gear is going into the post — and this run only noticed
because the row was re-read at the end. Nothing here is broken and every count
in every clause is right; it is the sentence that is misleading, and the arithmetic
this box is about (N pieces → N mails → one item each) is unaffected. Flagged
for whoever owns the Characters tab's wording; **not changed here**, because the
label is 8.4a's and 8.4b's as much as this tree's and one lane should not
rename a button on three trees' behalf.

## Files

* `client-login-vanilla.ps1` — the driver. **Not** 8.4b's or 8.5c's: those are
  fire-and-forget, and this clause needs a client action, then a press on
  another machine, then a look back at the client. It launches the client and
  then executes commands from a queue directory one file at a time, answering
  each with a `.done`, so the ssh side sees every frame before choosing the next
  move. Its header carries every coordinate and every client fact measured here.
* `two-mails.py` — the gear-set clause with the client in the loop
* `offline-revive-live.py` — the corpse made to order, and the offline press
* `client-transcript.txt` — the driver's log for the second run

## What this lane could not do

* **The first run's log is gone.** Three stale scheduled tasks on the client box
  (`yulon-login-tbc`, `yulon-who-tbc` and this lane's own `yulon-van84c`, every
  one of them created with `schtasks /sc once /st 23:59`, the recipe copied from
  gate to gate all evening) FIRED at 23:59 local and each started a second copy
  of its client, killing the first mid-session. The driver truncates its log on
  start, so the first run's lines went with it. Its screenshots did not, and
  `5-`, `6-` and `7-` are that run's — the character list, the first rename
  prompt and the answer to it. Those three therefore rest on the photographs and
  on the row readings taken from m910q either side of them (`901|Asdff|…|1` at
  21:41:07Z with the flag set, `901|Asdffrenamed|60|1|0|0` at 21:52:24Z after the
  prompt was answered), not on a driver log. The rename clause was in any case
  re-made from scratch in the second run (`20-`), which is why it was worth
  making twice. The fix for the tasks is one flag — `/sd 01/01/2030` — and it is
  in the driver's header.
* **TAB between the login boxes is still unmeasured on this client.** Every
  attempt here CLICKED both boxes. 8.3c's run used `{TAB}` but its refusal
  ("The information you have entered is not valid") is what a wrong account and
  a wrong password both produce, so it cannot say which box took the password.
  8.5c's flag against `client-who-vanilla.ps1`'s comment therefore stands
  unresolved, not refuted.
* **The Vanilla stack is still up on m910q** — three healthy containers, 509
  characters online at 22:41:22Z. 8.5c left it up on purpose because this lane
  needed it and noted that somebody should stop it afterwards. This lane did not
  stop it either: the standing rule says no idle servers on m910q, but this
  checkout is shared with other live sessions tonight and stopping a server
  another lane is mid-way through is worse than leaving one running. Somebody
  who knows no other lane wants it should take it down.
* **Nothing was committed or pushed**, and `pyplan/checklist.md` was not
  touched. No server was started, stopped or rebuilt, and nothing under
  `pylauncher/` was edited — this lane's only writes are the files in these two
  gate folders.
