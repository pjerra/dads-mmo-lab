# My Party, driven from the app's own panel against a live world — yulon-ubuntu2, 2026-09-09

**The panel drove a real server.** Read an empty party, added a bot of a chosen class, saw it in the
panel's own list, dismissed it, saw it go — and then three refusals, each in the app's words. Until
this run the capability had only ever been reached from a gate script (`gate86b.py`), which is the
thing the Phase 8 exit line's third clause forbids.

**This README was written by the orchestrator, not by the lane that took the frames.** The session
was cut off at about 06:15 with the folder complete and uncommitted; the evidence below is the
lane's, the summary is mine, and nothing here is claimed beyond what the logs show.

## What the panel did, in its own words

From `panel-transitions.log`, which records the panel's four visible fields after every transition:

| time | summary | report | rows |
|---|---|---|---|
| 06:00:51 | *(empty — the window has just opened)* | | `[]` |
| 06:04:02 | This character's party has no bots in it yet. | | `[]` |
| 06:05:53 | This character's party has no bots in it yet. | **Kiteema joined the party, geared and specced.** | `[]` |
| 06:05:55 | **1 bot in this party.** | Kiteema joined the party, geared and specced. | `['Kiteema — level 1 — class 8']` |
| 06:07:14 | 1 bot in this party. | **Kiteema left the party.** | `['Kiteema — level 1 — class 8']` |
| 06:07:15 | This character's party has no bots in it yet. | Kiteema left the party. | `[]` |

Two details in that table are the point of it, and both are the app being honest rather than quick:

* **the row appears one reading LATE and disappears one reading LATE.** At 06:05:53 the server has
  already said the bot joined and the list is still empty; the row arrives at 06:05:55, on the
  re-read. Same at dismissal. The panel draws what the group table says, never what it just asked
  for — which is the difference between a party frame and a wish.
* **the checks are read every time** (`checks=['ok server_installed', 'ok world_running', …]`), so
  the summary line is a reading, not a cached verdict.

## The refusals, each with a frame

| frame | what was pressed | what the app said |
|---|---|---|
| `panel-5-empty-name-refused.png` | Add, with no character named | *Type the name of the character you are playing first.* The seam was never called. |
| `panel-6-not-in-world-refused.png` | Add, master `Notinworld` | *Notinworld is not logged in. A bot is added to a live session — the server resolves the master by name in the world, so log the character in first.* |
| `panel-7-master-logged-out.png` | the master logged out under a live panel | the same sentence, arriving on a re-read rather than on a press |

The third is the one worth keeping: the refusal is not a guess the panel makes before asking. It is
the server's own answer, and the panel keeps showing it for as long as it stays true (the transcript
repeats it once a second from 06:12:59 to 06:13:23, which is the panel polling and being told no).

## Frames

**The panel** — `panel-0-opened.png` (opened, nothing read yet), `panel-1a-name-typed.png`,
`panel-1-party-read-empty.png`, `panel-2a-class-picker.png`, `panel-2-class-chosen.png`,
`panel-3-bot-in-list.png`, `panel-4a-bot-selected.png` (Dismiss now names the bot),
`panel-4-bot-dismissed.png`, plus the three refusals above.

**The game client**, which is what the checklist's visible effect actually names —
`client-1-in-world-no-party.png`, `client-2-party-frame-with-bot.png`,
`client-3-party-frame-after-dismiss.png`. Also `d-credentials-typed.png` and
`h-character-created.png` from getting a character into the world in the first place.

**Its own account**, not the owner's: master `Panelpress`, bot `Kiteema`. The lane made both. The
owner's `PERZI` and `Pakka` were not touched — a lane on 2026-09-08 rotated his password for its
client seat and locked him out of his own server for an hour, and that is now a standing rule.

## What this does NOT show

* **No screenshot of the panel and the client in one frame.** They were captured separately, so a
  reader takes it on the timestamps that the bot in `client-2` is the row in `panel-3`.
* **The Qt frames are the panel on the VM's desktop, not the whole app.** The surrounding tab, and
  how a user reaches the panel from a cold start, are not photographed here.
* **One class was pressed, not ten.** `party.BOT_CLASSES` carries ten; `class 8` (mage) is the only
  one this press sent.
* **`part-2-party.md:188` in the sibling folder is now false.** It reads *"The Qt widget for My Party
  does not exist."* It does: `pylauncher/yulon/ui/widgets/party_panel.py`, wired at
  `controller_view.py::_build_my_party_group`. The historical page is left as it was written; this
  paragraph is the correction, and a tick citing that folder should cite this one beside it.
* **`part-2-party.md:3` is wrong about the VM's clock (added 2026-09-09).** That page reads *"the VM's
  clock is UTC"*. Measured read-only on the box by the lead at 10:34 local on 2026-09-09: `date`
  answered `CEST`, `timedatectl` answered `Europe/Oslo (+0200)`, and `docker exec ac-worldserver date`
  answered `UTC`. So the VM's shell runs **+0200** and the `ac-worldserver` container — the one
  container that was asked — runs **UTC**; no other container's clock was measured. What that
  settles is the `…Z` stamps on the **`yulon-ubuntu2`** pages that come from Docker itself:
  `8.6-wotlk-yulon-ubuntu2-2026-09-09/part-2-party.md:26` (`worldserver running
  started=2026-09-08T21:15:35Z`) and `part-2-bridge-absent.md:29` and `:33` (the same run, and
  `2026-09-08T22:24:14Z` after the restart) are container readings, two hours behind the shell that
  took them. It settles nothing about `8.6-wotlk-yulon-ubuntu-2026-09-08/`, whose `Z` stamps are the
  gate script's own banners (`1-ground.txt:1`, `4-console.txt:1-2`, `README.md:3`) on the original
  `yulon-ubuntu`, a box whose clock was never measured and which has been OffCritical since its host
  disk left the bus on 2026-09-08.
  `pyplan/gates/8.7a-wotlk-yulon-ubuntu2-2026-09-09/README.md:9` had already written the same offset
  from the other side (*"22:56–23:02 UTC (00:56–01:02 VM local)"*), and the measurement itself is
  recorded in `pyplan/tickets/T1-tick-8.6.md:125`. What it means for this folder: the bare
  `[HH:MM:SS]` stamps in `panel-transitions.log`, `panel-transcript.log` and `panel-transcript-2.log`
  carry no zone, and the panel ran on the VM's desktop, so they are VM-local +0200; `client-agent.log`
  was written on the Hyper-V host, a third clock this paragraph did not measure (`part-2-party.md:3`
  calls it PST), so those two files must not be read against each other without converting. The
  historical page is left as it was written.
* **No commit sha for the code that was pressed (added 2026-09-09).** The lane was cut off before it
  wrote one down, and nothing in this folder names a tree. The nearest commit is **`daeae0a5`**
  (*"The prior-art citations in the panel, read rather than remembered"*, 2026-09-09 03:38:23 +0200):
  the last commit before the 06:00 press to touch either surface file, and nothing has touched them
  since — `git log --oneline daeae0a5..528f219e -- pylauncher/yulon/ui/widgets/party_panel.py
  pylauncher/yulon/ui/controller_view.py` prints nothing, across the 39 commits in that range. That is
  a record of the nearest commit and **not** the sha of the press: what was checked out on the box at
  06:00 was never written down, and this paragraph does not claim to know it.
