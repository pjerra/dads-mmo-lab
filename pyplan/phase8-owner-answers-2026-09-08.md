# Owner answers, 2026-09-08

Four questions put to the owner on the morning of 2026-09-08, after the overnight run raised them.
Each was asked with options and a recommendation; three answers took the recommendation and one
took part of it. Recorded here because `phase8-decisions.md` is the design of record and was silent
on all four — and because a decision nobody wrote down is a decision the next lane re-makes
differently.

---

## 1. Tortoise cannot take their current head

**Answer: reimport `tw_world` at their baseline.**

The fork's own migrations are not idempotent — `DROP INDEX` without `IF EXISTS`, `INSERT` without
`IGNORE` — and their updater cancels the whole world on a single failure. 173 world migrations are
outstanding on the m910q install, so the failures are a queue rather than an incident (measured
2026-09-08; see `catalog/installers/wow-tortoise/native/Dockerfile.tmpl` and the memory note).

What that means in practice, and the reason it is safe: **the world database is the only one being
replaced.** Characters live in `tw_char` and accounts in `tw_logon`, which are separate databases
and are never touched — the 901 characters and 108 accounts survive. Any hand-made edit to the world
database would be lost, and there are none.

**Tested on a checkpointed VM before m910q is touched** (owner's instruction, and the same one he
gave for the purge box). The rollback recipe from the failed rebuild stands: tag the working image,
dump all three databases, record the row counts, and compare afterwards.

Rejected, with reasons, so nobody revisits them cheaply:

* *Patching all 173 migrations idempotent at build time* — 173 files of somebody else's SQL that
  rot on every upstream change, and an `INSERT IGNORE` can silently skip a migration that genuinely
  needed to change a row.
* *A fresh install on a VM only* — proves the new-install path, which was never broken, and leaves
  the actual blocked install blocked.
* *Waiting for upstream* — leaves Tortoise with no SOAP channel and an `account set password` that
  still bricks any account that has logged in, which blocks the client half of every Tortoise box.

## 2. What the rebuild button does when a rebuild breaks the server

**Answer: always keep a rollback, and restore it automatically if the world does not come up.**

Tag the working image before building; if the worldserver does not reach ready within its window,
retag, bring the old one back, and show what the log said.

The reason this beats "check for outstanding migrations and refuse": it is not specific to this
failure. A migration queue is what bit us on 2026-09-08, but a rebuild can fail to start for
reasons nobody has enumerated, and only the rollback helps then. It is also tree-agnostic, so it
protects all four games rather than the one whose defect we happen to know about. The cost is one
image's worth of disk while a rebuild runs.

It is exactly the recipe that saved m910q that night, done by hand. This makes it the app's.

## 3. What an unticked purge removes

**Answer: add the client-data volume. Nothing else.**

| Artefact | Unticked purge | Why |
|---|---|---|
| `<project>_client-data` (3.2 GB) | **now removed** | Downloaded map and DBC data, no player data at all, and a reinstall re-fetches it. Leaving 3.2 GB behind after "remove everything" is the surprise nobody wants. |
| `credentials/<game>-<install_id>.json` | **kept** | Not selected. The dialog names it as left behind, and it stays named. |
| `logs/<game>-<install_id>-*.log` | **kept** | The only record of why a server died; someone purging a broken install may still want them. |
| The server folder, on a TICKED purge | **still deleted** | "Uninstall" that leaves a 40 GB checkout is not what the word means. The kept volume plus `dbsecret`'s copy of its password is what makes the reinstall work. |

A **ticked** purge is unchanged: it keeps the database volume and the password that opens it, and
now also keeps the client-data volume, because a reinstall that keeps the characters should not
re-download 3.2 GB.

## 4. My Party (8.6)

**Answer: investigate up to the rebuild line; the owner runs the rebuild, if it is still worth
running.**

The box's own text allows a negative answer, and one failed attempt is already on record from
2026-08-20 — a deploy that reported success while every bridge command answered *"Command does not
exist"*. Repeating that experiment properly is what decides whether the rebuild is worth the
owner's time at all:

* Is the Lua engine in the image on `yulon-ubuntu`? Read the binary, not a conf file.
* Does our manifest name the configuration key the module actually has? The box says it does not.
* Does the bridge command answer when the server is asked?

If it fails for a reason a rebuild cannot fix, the rebuild is saved entirely, and My Party goes back
to the owner as the box says it may. Behind a checkpoint, because the experiment writes into the
server folder.
