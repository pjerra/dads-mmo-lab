# 8.9a live: uninstall and purge, WoW WotLK, on yulon-ubuntu

**Box:** yulon-ubuntu, a Hyper-V VM on `vmhost`. AzerothCore WotLK at `/home/pk/wowserver`
(2.3 GB, install id `243c46e3`, compose project `yulon-wow-wotlk-243c46e3`), stack up.
**Date:** 2026-09-08, 06:16–06:47 VM local (CEST). **Tree:** `pylauncher` at `80963a7b`,
exported with `git archive`, unpacked to `/home/pk/gate89a`.
**Python:** `/home/pk/laneA/.venv` (3.12.3, PySide6 6.11.2), `QT_QPA_PLATFORM=offscreen`.
**Restores:** three. **Result: all four clauses PASSED, plus the visible effect.**

Every press went through `main.build_window()` — the real window, the real tab, the real
buttons, the real `state.json` — so the record, the tab teardown and the catalog tile are
the app's own and not the driver's. No seam below the view was faked anywhere.

## The one deviation from the checklist, said plainly

8.9a's gate line reads: *"`yulon-ubuntu` on a **throwaway** install, never the finished 7.2
one"*. **This gate used the finished 7.2 install.** The owner made that call on 2026-09-08.

The rule existed so a gate could not destroy something we still need, and a Hyper-V
checkpoint serves that better than a second install nobody has hours to compile: the
install is destroyed for real, three times, and comes back byte-for-byte. The transcript's
**RESTORE 1/3** block is the evidence that the rollback is real rather than assumed — the
driver tree and the seeded `state.json`, both written *after* the checkpoint was taken,
were gone after the restore, while the three containers came back **Up** rather than cold
(a Standard checkpoint carries memory state).

This is a deviation, not the rule being ignored, and the substitute protection is named
above so a later reader can judge it.

## Setup, and why it counted as setup rather than as the gate

`/home/pk/wowserver` was built by the 7.2 gate, outside the launcher, so `state.json` did
not exist on this box and the app had no memory of the install: no tab to press, and a
Catalog tile already reading "Install" — which would have made clause 2's visible effect
true before any press. The driver's `seed` stage wrote that one record through the app's
own `AppState.remember` / `save_state`, and every stage re-read it and printed it before
pressing. Screenshot `01` is the tile that record produces: **Installed**, greyed.

## The four clauses

Each was pressed from a state its own action does not produce. The ground was read and
printed immediately before the press, never inferred.

| # | Clause | Ground before the press | What the press produced |
| --- | --- | --- | --- |
| 3 | a purge of a RUNNING server refuses and says to stop it first | `ac-worldserver`, `ac-authserver`, `ac-database` **Up 9 hours**; folder present; both volumes; four images; record present | `plan()` and `run()` **both** refused: *"ac-worldserver, ac-authserver, ac-database: still running. Stop the server first, then uninstall. Nothing was removed."* No confirm button offered |
| 4 | an install whose ownership cannot be proved is refused rather than removed | two decoy folders built for this clause, each holding `MY-PRECIOUS-SAVES.txt`; `server_dir_claim` answered **UNKNOWN** for the one with an unreadable `.yulon-install.json` and **UNCLAIMED** for the one with none | both refused, with **different** advice; both folders and both canaries intact; `forget()` never called; the real install in another folder untouched |
| 1 | ticked — the characters survive | stack **Up**, folder 2.3 GB, **both** volumes, four images, record present, **1001 characters** in `acore_characters` | folder gone, `_client-data` gone, four images gone, five containers gone, record forgotten — and `_db-data` **kept**, then opened and read: **1001 characters, 105 accounts** |
| 2 | unticked — nothing remains | restored: stack **Up**, folder 2.3 GB, **both** volumes, four images, record present | `docker volume ls` lists **nothing at all**; `docker ps -a` is a bare header; no `yulon.local/*` image; folder gone; `state.json installs = []` |
| — | visible effect: the catalog tile reads **Install** again | tile read **Installed**, disabled, tooltip naming `/home/pk/wowserver` (shots `01`, `05`) | tile reads **Install**, enabled, tooltip empty (shots `03`, `07`) |

### Clause 3 — the refusal names all three, not just the world

`ac-worldserver, ac-authserver, ac-database: still running.` The census is scoped to the
project and reports what it found, so the database counts too. Both the read path
(`plan()`, behind the "Uninstall…" button) and the destructive path (`run()`, called
directly so the refusal could not be attributed to a hidden button) gave the same sentence.

**Clause 5's read-only property came free here and is recorded:** the ground was re-read
after both presses and compared field by field — `docker ps -a`, `docker volume ls`,
`docker images`, and the folder — `CLAUSE 5 (plan reads only) + clause 3 left nothing
changed: True`.

### Clause 4 — two refusals, two different sentences, on folders the purge must not touch

Made rather than found, so this clause cost no restore: `/home/pk/decoy-unknown` holds a
`.yulon-install.json` containing `{ this is not json`, and `/home/pk/decoy-unclaimed` holds
no record at all. Both hold a compose file (so nothing else could be the reason for
refusing) and a canary file.

    UNKNOWN   -> "There is an install record in /home/pk/decoy-unknown that Yu'lon cannot
                  read, so it cannot prove this install is its own. Nothing was removed."
    UNCLAIMED -> "Nothing here says Yu'lon installed it: there is no install record in
                  /home/pk/decoy-unclaimed, so this folder is not Yu'lon's to delete.
                  Nothing was removed."

Both are refusals for a delete, and only `OWNED` authorised one. `plan()` resolved nothing
else on either — `project=''`, no containers, no volumes — so the refusal came before any
Docker question was asked. `forget()` was called for **nothing**, so no record was dropped
on the way out of a refusal.

The neighbouring-install exclusion was measured in the same stage: the real WotLK install's
containers, volumes, images and folder were byte-identical before and after both decoy
presses — `the real install in another folder was untouched: True`.

### Clause 1 — ticked, and what "the characters survive" was actually proved to mean

The press: the app's own **Stop** (43.4 s to bring the world down), then "Uninstall…", then
"Uninstall this server" with the box ticked. The plan the user is shown said, before
anything was touched:

    volumes removed: yulon-wow-wotlk-243c46e3_client-data
    volumes KEPT: yulon-wow-wotlk-243c46e3_db-data — your characters. A reinstall to THIS
    SAME FOLDER finds them again; a reinstall anywhere else does not.

The uninstall itself took **7.1 s**. Afterwards `docker ps -a` listed **no containers at
all**, `docker volume ls` listed exactly one — `yulon-wow-wotlk-243c46e3_db-data` — the four
`yulon.local` images were gone, the folder was gone, and `state.json installs` was `[]`.

**A full reinstall was not run, and this is the equivalent that was proved instead** (the
brief allows it; say exactly which). With the folder deleted:

1. the password a reinstall would use was resolved **by the app**, not typed here:
   `installer_for_app(entry).resolve_secrets(/home/pk/wowserver).db_password` — the same
   call stage 1 of a reinstall makes;
2. a **fresh** `mysql:8.4` container was started on the kept volume with that password —
   nothing of the old install was left to serve it;
3. through it: `select count(*), min(name), max(name) from acore_characters.characters`
   → **1001, Aasahi, Zoyom** — identical to the reading taken before the purge — the first
   ten characters with race/class/level, and **105 rows in `acore_auth.account`**, the
   accounts that log in to them.

So: the volume survived, the password that opens it is still resolvable after the folder
holding secrets is gone, and the character rows are readable through it. What is *not*
proved here is the last hop — a rebuilt server binding that volume and drawing the
character list on a real login screen. That needs a full reinstall (a compile), and it is
owed on 8.9b's box.

WotLK is a `fixed`-password family, so `PurgeReport.secret_kept` was `None` and the tab said
nothing about a kept password — correct, and it means **this box exercises only the path
where nothing has to be kept**. The copy-and-recall half that yesterday's blocker fix added
(copy written, folder deleted, reinstall reads it back) is a `generated`-family path and is
owed a press on 8.9b's CMaNGOS box.

### Clause 2 — the negative, proved by asking Docker

    raw `docker volume ls`:
    DRIVER    VOLUME NAME
    raw `docker ps -a`:
    CONTAINER ID   IMAGE     COMMAND   CREATED   STATUS    PORTS     NAMES

Not a report saying it removed things: the daemon listing nothing. `docker images` was left
holding exactly two, and they are exactly the two the module argues must survive —
`mysql:8.4` (shared with any second install) and the sha256-pinned `alpine/git` that shows
up untagged and that a prune would take. Neither is this install's. The folder was gone,
`state.json installs` was `[]`, and `server_dir_claim` on the vanished folder answered
`UNCLAIMED`. The whole press, after Stop, took **4.5 s**.

## Restores: three, and what each press started from

| # | When | Why | What the next press started from |
| --- | --- | --- | --- |
| 1 | 06:20 | a **driver** bug abandoned the first clause-1 attempt after the app's Stop had run. Nothing destructive had happened, but the box was no longer in a state clause 1's own action had not produced | the checkpoint: stack **Up 9 hours**, both volumes, folder 2.3 GB, four images, no `state.json`, no driver |
| 2 | 06:34 | the ticked press had destroyed the install | the same checkpoint state, verified item by item at 06:39:21 before pressing |
| 3 | 06:41 | the unticked press had destroyed the install | nothing — this one is the hand-back |

After each restore the tree and driver were pushed again and the record re-seeded, because
the checkpoint predates both. Both checkpoints — `8.9a-before-purge-2026-09-08` (the spare)
and `8.9a-purge-gate-2026-09-08b` (this gate's) — are intact and were not deleted.

**The box was left as it was found**, read rather than assumed at 06:46:56: three containers
Up, both volumes, four `yulon.local` images, `/home/pk/wowserver` back, 1001 characters
answering through `ac-database`, and no gate leftovers (`/home/pk/gate89a`, `/home/pk/decoy-*`
and `state.json` all absent, exactly as the checkpoint has it).

## The driver bug, and the one line in the transcript that is wrong

Two defects were found, both in **the gate driver**, neither in the feature. They are named
here because a transcript that contains a false-looking line and no explanation is worse
than one with a defect admitted.

1. **`isVisible()` on a child of a window that was never shown is False.** The first
   clause-1 attempt read the app's confirm button as absent and stopped, after the app had
   already produced the correct plan. Fixed by `window.show()` (offscreen, but really
   shown) plus a check on the view's own `_uninstall_plan`. Cost: restore 1/3.
2. **`logs/stage1.log` contains the line `the kept volume opened with that password:
   False`, and that line is wrong.** The readiness probe tested
   `probe.strip().endswith("1")`, but `mysql`'s "Using a password on the command line
   interface can be insecure" warning arrives *after* the value, so the flag never went
   true and the loop ran its full 240 s. The three queries that follow it — 1001 characters,
   ten named rows, 105 accounts, all read out of the kept volume through that password —
   are the actual evidence, and they only ran because the volume *did* open. The driver has
   been left exactly as it ran so the code and the transcript agree; the flag is wrong, the
   queries under it are not.

## Files

| file | what it is |
| --- | --- |
| `gate89a.py` | the driver, exactly as it ran. `seed`, `ground`, `stage3`, `stage4`, `stage1`, `stage2` |
| `transcript.txt` | the whole run, clock-stamped, with the restores and the final hand-back |
| `logs/stage3.log`, `stage4.log`, `stage1.log`, `stage2.log` | the raw per-stage output, unfiltered |
| `logs/stage1-attempt1-driver-bug.log` | the abandoned first attempt, kept rather than deleted |
| `shots/01`, `05` | the tile before each press: **Installed**, greyed, tooltip naming the folder |
| `shots/03`, `07` | the tile after each press: **Install**, enabled — the visible effect |
| `shots/02`, `04`, `06`, `08` | the whole Catalog pane around those tiles |
