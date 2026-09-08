# 8.4d — Play, WoW Tortoise — **RUN 2026-09-08 07:23Z–08:05Z on m910q + vmhost**

> **This file is a record, not a plan.** The plan it executes is
> [`../8.4d-tortoise-m910q-PLAN.md`](../8.4d-tortoise-m910q-PLAN.md), checked in
> the day before with nothing run. Every stage below ran against the **live**
> Tortoise stack on `m910q` (`tortoise-db`, `tortoise-realmd`, `tortoise-mangosd`,
> up 2 h 52 m when the first stage started), through the **`docker attach`
> console** — this build has no SOAP. The client half ran on `vmhost` against
> the same server, in the same sessions that closed **8.5d's clause (4)**, which
> is why the two boxes were one job.
>
> Output with the clock on every line: `transcript.txt`. App frames: `1-*` …
> `10-*`. Client frames: `client-1` … `client-3`, with their driver logs in
> `../8.5d-tortoise-m910q-2026-09-08/`.

**The box.** `pyplan/checklist.md:2495` — *"Play, WoW Tortoise — as 8.4c, with
one item per message, this fork's top-level rename command, and **no set-level
control drawn** … the group is replaced by a sentence naming what does exist
rather than one implying nothing does. **Definition of done:** as 8.4c for the
actions this tree has, and the absent group shows its reason."*

**The tree that ran** is the committed `yulon-phase8b` tip `7220a0ab`, exported
with `git archive` and unpacked to `~/gate84d/pylauncher` at 07:18Z — deliberately
not a copy of the shared checkout and not `~/gate85d/pylauncher`, which is a
different commit.

```
4e30dd615706d9d2e7eecf529d2b6db0  ~/gate84d/pylauncher/yulon/play.py
a14d7a0d3982bbfc3e19fe4d12e96119  ~/gate84d/pylauncher/yulon/ui/controller_view.py
14feb669d0411c911ff1dc66c81a9106  ~/gate84d/pylauncher/yulon/catalog/catalog.json   (before the fix)
11a5755352fde8ca796f19e8e7504f7e  ~/gate84d/pylauncher/yulon/catalog/catalog.json   (after it)
```

**One file changed during the run** and is committed with this folder:
`catalog.json`'s absent-group sentence. See *The defect the box found*.

`gate84d.py` in this folder is byte-for-byte the file that ran, LF-normalised:
`md5 8de0f69e47927609a1825351cf33b5bb`, the same on both sides.

---

## The clauses

| clause | verdict | where |
|---|---|---|
| the set-level group is not drawn, and a sentence stands in its place | **PASSED** | `absent` 07:32Z — `2-no-set-level-and-its-reason.png` against `1-ground-a-tree-that-has-it.png` |
| that sentence is TRUE: nothing moves an existing character to a level you pick | **PASSED**, five routes pressed | `levelroute` 07:26Z, `initprobe` 07:28Z, `levelcreate` 07:29Z |
| `reset level`, the thing the sentence says the console CAN do — both halves | **PASSED** | `resetlevel` 07:35–07:36Z |
| teleport, on this tree's own verb, and a place the server does not know | **PASSED** | `teleport` 07:36Z — `4-a-place-the-server-does-not-know.png` |
| this fork's **top-level** `rename`, sent by the app, on an ONLINE character | **PASSED** | `rename` 07:33Z |
| the offline rename withheld — at the button AND at the seam | **PASSED** | `renameoff` 07:37Z — `6-offline-rename-withheld.png` |
| revive works offline here, and a watched corpse goes | **PASSED** | `revive` 07:37Z |
| **one item per message** | **PASSED** | `gear` 07:37Z — `8-the-promise.png`, `8b-dorta-the-promise.png` |
| gold, and a successful send reported as one | **PASSED** | `gold` 07:37Z |
| the equipped read is this fork's FLAT one, and the ids are real | **PASSED** | `flatread` 07:38Z |
| the list catches up | **PASSED, and NARROWED** — see below | `catchup` 07:39–07:40Z — `10-the-list-is-live.png` |
| a channel that cannot be asked is reported as that | **PASSED** | `deadchannel` 08:03–08:05Z — `12-the-channel-could-not-be-asked.png` |
| **the client:** the character standing where 8.4d teleported it | **PASSED** | `client-1`, `client-3` |
| **the client:** the rename prompt at the next login | **PASSED** | `client-2` |
| **the client:** the mail 8.4d sent, one item per letter | **partly** — six letters exist in `tw_char.mail`; the mailbox itself was not opened. See **The client half**. |

---

## The defect the box found, and the fix

**The shipped sentence named a command in an argument order this server
refuses.** It read

> *…while `.rndbot <bot> level` ignores the number it is handed and re-rolls
> that bot at the level it already has…*

and on the live console (07:27:33Z and 07:30:36Z):

```
`rndbot Aniel level 60`   ->  Aniel: level - character not found
`rndbot Aniel levelup 60` ->  Aniel: levelup - character not found
`rndbot level Aniel 60`   ->  level: Aniel - ok        (Aniel stayed at 18)
`rndbot levelup Aniel 60` ->  levelup: Aniel - ok      (Aniel stayed at 18)
```

**The verb comes first on this fork.** So the sentence was *right about what
happens* and *wrong about what to type*: somebody following it gets `character
not found`, reads that as the feature, and never sees the behaviour being
described. The plan carried the same mistake — its S1 step 3 spells it
`rndbot <onlinebot> level 60` — and so did `initprobe`'s first run, which is how
it was caught at all.

Fixed with a test first. RED, `test_the_sentence_in_place_of_the_level_control_names_what_this_fork_has`:

```
E       AssertionError: No set-level control on this server. Nothing its console can run
        moves an existing character to a level you pick: `.levelup` is the one that would,
        and the console is not allowed to run it, while `.rndbot <bot> level` ignores the
        number it is handed …
E       assert '.rndbot level <bot>' in 'No set-level control on this server. …'
tests\test_play_actions.py:462: AssertionError
```

then one word moved in `catalog.json`. The two new assertions are
`".rndbot level <bot>" in said` and `".rndbot <bot> level" not in said`, with the
two console answers quoted above written into the test as the reason.

---

## S1 — the clause the absent group rests on, pressed five ways

The point of this box is a *sentence about an absence*, and a sentence about an
absence is only as good as the search behind it. Five console routes were sent,
each against a subject whose level was **not** the level being asked for:

| sent | the console answered | the row |
|---|---|---|
| `levelup Aniel 60` | `This command is not available to you.` | 22 → 22 |
| `character level Aniel 60` | `There is no such subcommand` + the ten subcommands it does have | 22 → 22 |
| `rndbot level Aniel 60` | `level: Aniel - ok` | 18 → 18 |
| `rndbot init Zetom` | `init applied to Zetom` | **60 → 12** |
| `rndbot create level=30 class=1 race=1 name=Gate52534` | `Bot created: Gate52534` | **a new row, 903, at level 30** |

Two of those refuted a prediction the plan made:

1. **`character`'s subcommand list has ten rows here, not eleven.** The console
   printed `deleted, erase, getname, diffitems, reputation, hasitem, fillflys,
   clean, itemlog, mail` — the plan's list added `inactivity`, which this build
   does not carry. No `level` and no `rename`, which is the part that matters.
2. **`rndbot init` DOES move an existing character's level.** The plan predicted
   a no-op (`master ? master->GetLevel() : bot->GetLevel()`, and a console call
   has no master). It moved `Aniel` 22 → 18 and `Zetom` 60 → 12, on two separate
   presses. **The shipped sentence survives that** — and only just: `init` takes
   *no level argument*, so the number it writes is the module's and not an
   operator's, and "nothing moves a character to a level **you pick**" is still
   true. It is the command that comes closest to refuting this box, and it is
   named here so the next person does not have to find it twice.

And the one that works is the point: the only console route to a chosen level
**made a character that was not there before** (901 → 902 → 903). That is not
what a "Set level" button offers, which is why the group is a sentence.

**Two throwaway characters are on this server because of it:** `Gate52286`
(guid 902, level 30) and `Gate52534` (guid 903, level 30), both from `rndbot
create`. They are recorded rather than deleted — a delete is a write nobody
asked for, and 8.5d's totals are re-read live rather than pinned.

---

## S2 — the absent group, and the ground that makes the photograph mean something

`2-no-set-level-and-its-reason.png` is the box's own frame: the Characters tab
on the Tortoise install with `Zetom` selected, **no Level row, no Set level
button**, and the sentence in the cell they would have taken — wrapped and fully
readable at the window's real width, with every other button naming the
character:

```
Teleport Zetom / Rename at next login Zetom / Revive Zetom / Send gold to Zetom
/ Send Zetom's 12 worn items (12 mails)
```

The stage asserts more than "it is not visible", because `isVisible()` is False
for *every* child of a window that was never shown — the first run asserted it
and failed on the sentence it was there to prove. What it asserts now is the
widget's own state and its **layout cell**:

```
the Level spinbox:     hidden=True  would_be_drawn=False  in_a_layout_cell=False
the Set level button:  hidden=True  would_be_drawn=False  in_a_layout_cell=False
the sentence:          hidden=False would_be_drawn=True   in_a_layout_cell=True
```

plus `form.indexOf(set_level_button) < 0` and `set_level_button not in
character_buttons()` — the control is out of the form, out of the naming and out
of the enabling, not merely invisible.

**`1-ground-a-tree-that-has-it.png` is the ground**, and without it the frame
above is a photograph of an empty space. It is the **same app build, the same
window class**, given the `wow-wotlk` entry: the Level spinbox and the Set level
button are both there. Its character list is empty, and that is expected —
WotLK's schemas do not exist on this Tortoise database — because what the ground
has to show is the **form**, not the list.

> The gate got this wrong on its first attempt in a way worth recording: its
> `view_for()` hard-coded the Tortoise entry, so the "ground" was a **Tortoise**
> view wearing WotLK's services, and it photographed the very absence it was
> meant to contrast with. The stage failed *on the app's behalf*. `view_for` now
> takes the entry as a parameter and the docstring says why.

---

## The other clauses, with their grounds

### `reset level` — the thing the sentence says the console CAN do (07:35–07:36Z)

`StartPlayerLevel` was read off `etc/mangosd.conf` at the time of the run: **1**.

* **online:** `reset level Aniel`, subject at level 18. **The command printed no
  answer at all** — the whole reply window held one unrelated SQL trace line —
  and the row reached level 1 **51 seconds later**, on the seventeenth three-second
  re-read. The first version of this stage slept 3 s, read 18, and would have
  recorded a no-op; the next stage's ground caught it at level 1 twenty-one
  seconds after that. *For an online character the world holds the truth until it
  is asked to save,* and on this tree that queue is about a minute deep.
* **offline:** `reset level Laureo` → `Player not found!`, level 49 → 49. The
  handler takes only a `Player**`, so the sentence's "a character who is logged
  in" is this server's own word.

### Teleport (07:36Z)

`tele name` — this tree's verb, from the entry. `Laureo` (offline) went map 1 →
map 0, x 1629.36 → -8833.38, with `You are teleporting Laureo (offline) to
Stormwind.` Then a place nobody has, **through the tab**:
`4-a-place-the-server-does-not-know.png` shows the report reading exactly

```
Teleport location not found!
```

and the row did not move a second time.

> **Recorded, not asserted:** the seam returned `done=True` for that refusal.
> On this transport it cannot do otherwise — `AttachChannel` answers `yes` for
> every command the console echoes a prompt after, and says so in its own
> docstring, because a console has no fault code. The plan's S4 asked for "the
> refusal drawn as a refusal rather than as a success", which is a SOAP-shaped
> expectation. What the tab actually shows is the **server's own sentence**, and
> that sentence is the refusal. A boolean that could tell the two apart would
> need something this channel does not carry.

### Rename — this fork's top-level command (07:33Z)

Ground: `Zetom`, online, `at_login & 1 == 0`. The app sent `rename` (not
`character rename`) and the console answered

```
Forced rename for player Zetom will be requested at next login.
```

`at_login` 0 → 1, **and the name unchanged** — `'Zetom'` before and after, which
is the assertion that separates this arm from the destructive one.

### The offline rename, withheld (07:37Z)

`6-offline-rename-withheld.png`: `Laureo` selected, the Rename button **greyed**
and reading `Laureo has to be logged in to be renamed`, its tooltip carrying the
entry's full sentence about what the server would have done instead.

Then the seam was pressed anyway — which is what a list read minutes ago would
reach:

```
done=False
problem="Laureo has to be logged in before a rename can be flagged on this server --
         the same command run on an offline character here replaces the name with the
         character's guid instead, so it is not sent"
after: name 'Laureo' -> 'Laureo', at_login 0
```

**The destruction itself was not run**, deliberately: the command it would send
is `UPDATE characters SET name = guid, at_login = at_login | '1'`, and a gate
that proves a refusal by first proving the thing refused has spent a character to
learn what the source already says. What *was* measured, because the plan warns
about the restore path: **`rename <char> cancel` did not clear the flag on an
OFFLINE character** (07:55:34Z — prompted, no answer printed, `at_login` stayed
1). So the plan's suggested restore is not available offline either, and the only
route back is a login or a direct `UPDATE`.

### Revive (07:37Z)

`Bramerm`, offline, one corpse row, **watched for 12 s over four readings** so
its going is the command's doing and not decay. Then `Bramerm revived
(offline)`, corpse rows 1 → 0. Health is exactly the column this branch does not
touch, which is why the corpse is the oracle — 8.4c's finding, holding on a
fourth tree.

### One item per message (07:37Z)

`8-the-promise.png` is the button **before** the press: `Send Siristera's 5 worn
items (5 mails)`. After: `5 items sent to Siristera in 5 mails.`, mail rows
0 → 5, and

```
MAX(items per mail) for this receiver: 1
```

Repeated on `Dorta` for the client half (`8b-dorta-the-promise.png`): 5 promised,
5 arrived, one item each. And on a bigger wearer the same tab reads
**`Send Laureo's 19 worn items (19 mails)`** — the nineteen-piece set 8.4a and
8.4b sent as two mails, on a tree whose cap is one.

### Gold (07:37Z)

`Mail sent to Laureo`, mails 5 → 6, newest mail `money = 50000` — five gold, the
multiplication done once and in one place.

### The flat equipped read (07:38Z)

`item_template` as a column of the inventory row, no `item_instance`. `Zetom`:
the SQL count says 12, the app read 12, **all 12 ids exist in
`tw_world.item_template`**, and the same 12 as inventory instance guids match
**0** rows — which is the assertion that a read answering instance guids would
have passed on count alone.

### The list's catch-up — narrowed, honestly (07:39–07:40Z)

The first run of this stage pressed Teleport and watched the row text not change
over six re-reads. **That is not a defect, and the stage now says why.**
`play.characters()` selects `guid, name, level, online, account`, so the list
line prints exactly those — and every action this *tree* draws writes something
else:

```
Teleport                writes (map, position_x, position_y, position_z) -- overlap: NONE
Rename at next login    writes (at_login,)                               -- overlap: NONE
Revive                  writes (corpse rows,)                            -- overlap: NONE
Send gold to            writes (mail,)                                   -- overlap: NONE
Send everything worn by writes (mail, mail_items)                        -- overlap: NONE
```

The intersection is empty **because of this box**: the one drawn action whose
effect the line shows is the set-level control, and 8.4d is the box that removes
it. So the clause is proved in the two halves that stay honest:

* the machinery runs after a press and **stops** — the tab's own bound is 4
  re-reads at 750 ms, and the row did move (map 1 → 0);
* the list is **live** — `reset level Deronimo` sent on the console, by another
  hand, and the line went `Deronimo — level 10 — online — RNDBOT0` →
  `Deronimo — level 1 — online — RNDBOT0` on the fourteenth re-read, **42 s
  later** (`10-the-list-is-live.png`). Without this half, the empty intersection
  above would be indistinguishable from a list that never re-reads anything.

That 42 s is worth keeping: it is the same save-queue depth `resetlevel` measured
at 51 s, and the tab's bound is 3 seconds. **A set-level control on this tree
would answer before its row landed** — one more reason the group is absent rather
than disabled.

---

## The client half

Three sessions on `vmhost`, all driven through `schtasks /it` and all deleting
their own task. **The client was alive at every capture**, logged beside each
frame in `../8.5d-tortoise-m910q-2026-09-08/client-t84d-*.log`.

**`client-1-realm-flow-is-the-character-screen.png`** — one `-DryRealm` run bought
what "everything past the realm list on 1.18.1 is UNMEASURED" had been costing:
**this client has no realm-selection dialog and no language panel.** Login goes
straight to the character screen. And the screen itself already carries this
box's teleport:

```
Dorta
Level 1 Warrior
Orgrimmar
```

— the character panel naming the destination 8.4d's Teleport button sent it to,
before the world is even entered.

**`client-2-rename-prompt-at-enter-world.png`** — the rename this box's own
button flagged, arriving in the client:

```
Your character has been marked for a rename
Please enter a new name
[            ]      Confirm    Cancel
```

On this client the prompt appears at **Enter World**, as on 2.4.3 and unlike
3.3.5a. The flag was set **through the app, while the character was in the
world**, because this tree's rename only works on a logged-in character —
`Forced rename for player Dorta will be requested at next login.` at 07:49:45Z,
`at_login` 0 → 1.

**`client-3-renamed-in-orgrimmar-and-who-found-sietta.png`** — one frame carrying
three clauses at once:

* the portrait reads **`Dortagate`**: the rename was answered and the row now
  holds the new name (guid 901, `at_login` back to 0);
* the zone reads **Valley of Strength**: the character is standing where the
  Teleport button put it;
* the chat carries **8.5d's clause (4)**:
  `[Sietta]: Level 5 Goblin Rogue <Tainted Bunnies> - Durotar / 1 player total`.

Two more names were found in the same session and are in 8.5d's record.

**What the client half did NOT do:** the mailbox was never opened. Six letters
are in `tw_char.mail` for guid 901 — one holding `money = 70000` and five holding
one item each, which is this box's "one item per message" arriving — but walking a
level-1 character to a mailbox and opening each letter is the twenty-frame mouse
sequence 8.4c spent an evening on, and this lane spent its client time on the
clause that had never been proved at all. The mail is proved in the row and in
the button's promise, not in the client.

---

## What was left on each box

* **`m910q`** — the **Tortoise stack is up**, as this lane found it. The
  `deadchannel` stage stopped `tortoise-mangosd` and started it again, and the
  stage does not finish until `server info` answers. `~/gate84d/pylauncher` (the
  exported tree), `~/gate84d.py`, `~/gate84d-transcript.txt` and
  `~/gate84d-tortoise-shots/` are this run's; nothing under `~/tortoise-server`
  was edited.
  * **Server state this run left behind, all through the app or the console:**
    `Dorta` is now **`Dortagate`**; `Laureo` sits in Stormwind with 6 mails;
    `Siristera` has 5; `Zetom`, `Aniel` and `Deronimo` have had their levels
    moved by `reset level`/`rndbot init` (the bot manager owns them anyway);
    `Bramerm`'s corpse is gone; `Gate52286` and `Gate52534` are two new bot
    characters.
* **`vmhost`** — no client running, **no scheduled task** (`yulon-who-turtle`
  deleted by the driver's `finally`, confirmed absent). 21 frames and three logs
  under `C:\Users\PK\client-login\` with the `t84d-` prefix. The twelve stale
  `yulon-*` tasks already on that box are **other lanes' and were not touched**.

## What this record does not claim

* The mailbox clause is proved in the database and in the button, **not in the
  client** (above).
* Every source line number in `../8.4d-tortoise-m910q-PLAN.md` is a claim like
  any other and this run checked only the ones its stages pressed. Two of the
  plan's predictions were refuted (`character`'s subcommand list, and `rndbot
  init` being a no-op) and one of its command spellings was wrong; re-read the
  rest before quoting them.
* `deadchannel` is the only stage that stopped a container. It is last for that
  reason, and the box is not finished until the world answers again.

---

## The dead channel, in full (08:03–08:05Z)

`12-the-channel-could-not-be-asked.png`. Ground first: the console answered
(`prompted=True`) at 08:03:14Z, so the stage is not proving a channel that was
already dead. Then `tortoise-mangosd` was stopped and Send gold pressed:

```
done=False
problem: "the console printed no prompt inside the reply window, so nothing in it is
          this command's answer. The change may already have been made, so check
          before trying it again."
```

The tab shows that sentence. It does **not** say `Done.`, and it does **not**
say `It did not work.` — which are the two things the box exists to exclude,
because one invites a person to believe a command ran and the other invites them
to send it twice.

The container was started again in the stage's own `finally` and the stage does
not return until `server info` answers: attempts at 08:03:48Z, 08:04:07Z and
08:04:25Z came back `prompted=False`, and **08:04:44Z came back True** — about
75 seconds to load maps. Re-checked at 08:06:41Z: `Server uptime: 3 Minutes 7
Seconds`.

One incidental control fell out of it: the frame's character list reads
**`offline` on every row**, because the world had just come up and no bot had
logged in yet. The same list read a mix of online and offline in every earlier
frame, so the list is reading the server rather than a cache.
