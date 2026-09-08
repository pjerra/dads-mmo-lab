# 8.9b — Uninstall and purge, WoW Vanilla. Run on m910q, 2026-09-08

**Box** (`pyplan/checklist.md` line 2507): *Uninstall and purge, WoW Vanilla — as 8.9a.*
**Definition of done** (8.9a, line 2506): *with the box ticked, a reinstall to the same folder
finds the characters on the login screen; with it unticked, no container, volume, image,
folder or record of the install remains; a purge of a running server refuses and says to stop
it first; an install whose ownership cannot be proved is refused rather than removed.*

**Verdict: every clause passed, with two deviations named below.** Ran 10:01:48Z → 10:19:16Z.
Driver `gate89b.py` (nine stages), subject built by `setup89b.py`, corrected mid-run by
`relabel-volume.py`. Plan of record: `pyplan/8.9b-gate-plan.md`, written by the investigation
lane the day before; where the machine disagreed with it, this file says so.

---

## Why this box exists when 8.9a is already ticked

`wow-wotlk`'s database password is a **fixed** value in `catalog.json`, so its ticked purge
kept nothing and the copy-and-recall path in `yulon/dbsecret.py` — added by that very box, to
fix a defect found by reading — was never once pressed. Every CMaNGOS entry **generates** a
password per install into `<server_dir>/.db_password`, inside the folder a ticked purge
deletes, and `composegen.install_id()` is a digest of the absolute path, so the reinstall
comes back to the *same* volume. Without the copy, "Keep my characters" keeps a database
nothing can ever open.

That is the clause this box exists for, and it is `07-key.txt`:

```
10:05:58Z  recalled a password for volume 'yulon-wow-vanilla-e9e233c0_db-data'
10:05:58Z  starting a database on the KEPT volume with nothing but the recalled password
10:06:04Z  the kept volume answered: characters=['901', '108']
10:06:04Z  WRONG-PASSWORD CONTROL: exit=1  ERROR 1045 (28000): Access denied for user 'root'
10:06:04Z  KEY PASSED: 901 characters, reachable only with the password the purge kept
```

901 characters and 108 accounts, read out of a volume whose only key had been inside a folder
that no longer existed, with a wrong-password control so the reading is not a database that
would have opened for anybody.

## Clause by clause

| Clause | Where | Verdict |
|---|---|---|
| ownership unprovable → refused | `02-unknown.txt`, `1-refused-unreadable-record.png` | refused **twice, with two different reasons**: a zero-byte record, and a `version: 99` record whose refusal quotes the version and says the file was left alone |
| a folder with no record → refused | `03-unclaimed.txt`, `2-refused-no-record.png` | *"Nothing here says Yu'lon installed it"*; the folder was still there afterwards |
| a running server → refused | `04-running.txt`, `3-refused-server-running.png` | names `vanilla-mangosd, vanilla-realmd, vanilla-db`, hides the confirm button, and all three were still up |
| the plan reads only | `05-plan.txt`, `4-the-plan.png` | every listing — containers, volumes, images, networks, record — identical before and after |
| **ticked: the characters survive** | `06-ticked.txt`, `5-ticked-what-was-kept.png` | volume kept, image gone, network gone, folder gone, record forgotten, log snapshotted, password copied out |
| **and the password that opens them** | `07-key.txt` | above |
| **and the reinstall finds them** | `08-reinstall.txt`, `6-characters-after-the-reinstall.png` | the `db-password` stage accepted the kept copy instead of refusing; the server came up on the kept volume; the Characters tab lists **901** rows |
| unticked: nothing remains | `09-unticked.txt`, `7-unticked-nothing-remains.png` | container, volume, image, network, folder and record all gone, and the neighbours all intact |

The unticked diff against the baseline, in full:

```
containers: gone=['vanilla-db', 'vanilla-mangosd', 'vanilla-realmd']  new=[]
volumes:    gone=['yulon-wow-vanilla-e9e233c0_db-data']               new=[]
images:     gone=['yulon.local/cmangos-vanilla-server:native-e9e233c0'] new=[]
networks:   gone=['yulon-wow-vanilla-e9e233c0_vanilla-net']           new=[]
record:     gone=['/home/pk/gate89b/server']                          new=[]
```

Nothing else moved. `mariadb:11` — **shared with the TBC install**, which is the thing 8.9a's
box could not see because `mysql:8.4` is nobody else's — survived both presses, as did
`~/vanilla-75b`, its volume and its image, and `yulon-wow-tbc-37f13213_db-data`.

## The two deviations, named rather than left to be noticed

**1. The subject is a clone, not an installer press.** `pyplan/8.9b-gate-plan.md` G1 asks for
a throwaway install made by Yu'lon's own installer and calls it the long pole of the box: on
this tree a press clones, compiles, extracts a 2.4 GB client and builds mmaps, and the owner
runs compiles. So `setup89b.py` copied the finished install to `~/gate89b/server`, which is a
**different absolute path and therefore a different install id, project, volume name and image
tag**, gave it its own claim, regenerated its compose through the shipped
`stage_generate_compose`, retagged the built image rather than compiling one, and filled its
volume from the original's. Every object the purge acted on was genuine — a real 2.8 GB folder
with a real checkout, a real project, a real labelled volume with 901 real characters, a real
built image, a real network, a real row in `state.json`, and a real running mangosd that
reached `Avg Diff:`. What is not genuine is the provenance of the bytes.
**`~/vanilla-75b` was never the subject** and was untouched throughout, which is what the plan
asks for.

**2. The reinstall's folder came from a tar, and its image was retagged.** `stage_reinstall`
restores the folder, deletes `.db_password` — which is exactly the state a reinstall to that
folder starts in — and runs the shipped `_db_password` stage. That stage is the one that would
have refused, and the whole point of the fix; what a real press would add around it is eight
stages of build time it does not change. See finding 2 for the image.

## Findings

1. **A ticked purge removes the built image, so "reinstall to find those characters again"
   costs a full recompile.** The first run of `stage_reinstall` hung: `docker compose up`
   found no image and tried to **pull** it from `yulon.local`, a registry that does not exist —

   ```
   failed to resolve reference "yulon.local/cmangos-vanilla-server:native-e9e233c0":
   dial tcp: lookup yulon.local: no such host
   ```

   Removing the image is right — it is this install's alone — but the dialog's sentence
   ("Kept … — reinstall to the same folder to find those characters again") reads as cheap and
   on this tree is not: the folder held `src/` and a 2.4 GB `data/`, and the image is gone, so
   the reinstall re-clones, recompiles, re-extracts and rebuilds mmaps. This is the same
   territory as the gate plan's open question 2 and it now has a measured shape. Filed in
   `pyplan/bug-checklist.md`.

2. **Every purge plan logged a warning that was not true — and it is fixed in this commit.**
   `purge._default_reason()` reads the claim with `valid=()` because it wants the *version*,
   and `apply.server_dir_claim()` does the same because it wants the *identity*; both say so
   in as many words. `_parse_state` measured `completed` against that empty tuple anyway, so
   every recorded stage came out "unknown" and the log said *"records stages this build does
   not know … this is usually an older Yu'lon opening an install a newer one created"* — about
   an install this build had just written. Advice about a version mismatch that is not
   happening, printed on the one action that cannot be undone. It is family-neutral, so 8.9a's
   WotLK gate produced it too and nobody read the log. Fixed by warning only when the caller
   actually named the stages it knows; pinned by
   `test_planning_a_purge_does_not_warn_that_this_installs_stages_are_unknown`.

3. **A defect in this gate's own subject, and the reason it is worth writing down.**
   `setup89b.py` created the clone's volume with `docker volume create`, which produces a
   volume carrying **no labels**. The purge enumerates volumes by `com.docker.compose.project`
   — never by name, correctly, because `docker volume rm` exits non-zero on a name that is not
   there and the volume sets differ by family (gate plan defect 4). So the plan printed
   **"volumes removed: none"** while `docker volume ls` showed the volume plainly, and every
   later step would have "passed" without the purge ever having seen it. Caught by
   `stage_plan`, whose assertion is that the plan *names* the volume; the ground checks in
   `stage_ticked` would have passed, because they read `docker volume ls` rather than the
   label filter. `relabel-volume.py` rebuilt the volume through `compose create` so its labels
   are compose's own. A worked example of the rule the plan opens with: an assertion read off
   the wrong listing answers the same before and after.

## Where the machine disagreed with `8.9b-gate-plan.md`

* **Defect 1 was already fixed** before this gate ran — `yulon/dbsecret.py` landed with 8.9a
  on the strength of that lane's reading. What 8.9b owed was the press, and that is `07-key`
  and `08-reinstall`.
* **Defect 5** (a missing `.env` makes every compose command fail on this tree) was not
  exercised: `.env` was present throughout, and removing it to force the fallback would have
  been a step about a warning line rather than about the box's clauses. Still unpressed.
* **Defect 7** (a `--dry-run` transcript on this box prints network removals that do not
  happen) was avoided by construction: every listing here comes from `docker network ls` and
  friends, never from a transcript.
* **Open question 1** (where the throwaway comes from) is answered by deviation 1 above, and
  the answer is a workaround, not a resolution: a genuine fresh Vanilla install press on
  m910q is still owed, and it is still the owner's to start.

## What is not proved

* **The last hop: a real 1.12.1 client drawing those characters on a login screen.** 8.9a's
  tick carries the same gap. What is proved here is stronger than 8.9a's in one respect — the
  volume was opened with the recalled password and the rows counted, and then the app's own
  Characters tab listed all 901 after the server came back up — and weaker in none.
* **A reinstall that actually recompiles.** See deviation 2 and finding 1.
* **Windows.** This is the Linux half only, as the box says.

## Files

| File | What it is |
|---|---|
| `setup89b.py` | builds the subject; read its docstring for what is genuine and what is copied |
| `relabel-volume.py` | finding 3's correction |
| `gate89b.py` | the nine stages |
| `00-setup.txt`, `05a-relabel.txt` | the subject being built and corrected |
| `00-baseline.json`, `01-baseline.txt` | the machine at rest — every later diff is against this |
| `02-unknown.txt` … `09-unticked.txt` | one file per clause |
| `1-…png` … `7-…png` | the app at each clause; the filename is the subject |

## The box afterwards

`~/gate89b` and everything in it is deleted, including the kept password copy for the
throwaway install (`db-secrets/wow-vanilla-e9e233c0.json`). `~/vanilla-75b` is exactly as it
was found: folder intact, `yulon-wow-vanilla-06ced116_db-data` and
`yulon.local/cmangos-vanilla-server:native-06ced116` present, `etc/mangosd.conf` byte-identical
(`334915dd…`), and **no containers** — which is the state it was in when this lane took the
box. `mariadb:11`, `mariadb:10.6` and `alpine/git` are untouched. Disk is back to 31 GB free.

**Nothing is running on m910q** except the unrelated `r6` container that was already there.
The Tortoise stack, which was up when this lane took the box, was stopped for 8.7c (one server
at a time) and left stopped; the TBC stack was already stopped and still is. A lane that needs
Tortoise back starts it with `docker compose up -d` in `~/tortoise-server`.
