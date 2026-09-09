# The live Tortoise reimport was pressed, failed, and rolled back clean — m910q, 2026-09-09 01:06Z

Owner, 2026-09-09 ~03:00, asked for it in one word: **"Go, unattended"**. So it ran unattended, it
failed, the rollback beside it put the install back, and the counts match the ones taken before the
rehearsal to the row. **Nothing was lost.** The install is exactly where it was, with its server
stopped.

| | before (`counts-before.txt`) | after the rollback |
| --- | --- | --- |
| characters | 903 | **903** |
| accounts | 109 | 109 |
| `tw_world` migrations | 158 | **158** |
| `tw_char` migrations | 1 | **1** |
| image on the tag compose uses | `ff4de2e90500` | **`ff4de2e90500`** |

## What failed, in the order it happened

```
[01:06:34Z] --- 2. db alone; tw_world from their base; the colliding migration ... ---
            base: 191 imported, 0 failed
            UPDATE `spell_template`
            ERROR 1054 (42S22) at line 5: Unknown column 'script_name' in 'SET'
[01:07:15Z] --- 3. the whole stack up on the head image; watching the updater ---
            [DB Auto-Updater] Found 173 possible migrations for world.
            [DB Auto-Updater] Migration 20260903063722_world with hash 34F8…FFED failed to apply.
            DB AutoUpdater FAILED, cancelling server.
VERDICT: FAILED
```

## The two blockers, both measured on the box afterwards

**1. The workaround applies that migration out of order, and it cannot survive that.**
`20260903063722_world.sql` is 14 `INSERT INTO` and **12 `UPDATE`** statements. Twelve of them are
`UPDATE spell_template SET script_name = …`. The fork's own base does not define that column —

```
grep -A40 "CREATE TABLE .spell_template" sql/base/tw_world_spell_template.sql | grep -ci script_name
  → 0
information_schema on the LIVE tw_world (rolled back)                                → 1
```

so `script_name` is added by *one of the other 172 migrations*, and the live script pre-applied
20260903063722 **before** the updater ran any of them. The rehearsal never saw this: its README
already said, under *What is not proved*, that the statements after the failing one "did not run on
the copy in round 2". That caveat was the whole of tonight's failure. **The rehearsal was honest and
the reading of it was not careful enough** — a caveat is a prediction, and this one came true on the
first live press.

**2. The head image predates the fix for the collision this workaround exists to dodge.**
The reason for pre-applying the file at all is the duplicate-primary-key collision the rehearsal
found (`Duplicate entry '44070'`, the file's own second `INSERT` of a row it already inserts). The
image template was fixed for exactly this on 2026-09-08 in `3a1ed6ee`, which rewrites that file's
`INSERT INTO` to `INSERT IGNORE INTO` at build time. The image in hand was built at 04:21Z that day,
**before** the fix:

```
docker run --rm head-2404-2026-09-08  grep -c "INSERT IGNORE INTO" …/20260903063722_world.sql  → 0
docker run --rm head-2404-2026-09-08  grep -c "^INSERT INTO"       …/20260903063722_world.sql  → 14
```

## What actually unblocks it

Let the updater apply all 173 **in order**, and stop pre-applying anything — which works only once
the image carries the `INSERT IGNORE` rewrite. That means **rebuilding the Tortoise image from the
current tree**, and a rebuild is the owner's to start (`ask-before-any-rebuild`). Once that image
exists the live script's step 2 loses its migration half and keeps only the base import and the
character index, and the collision is handled where it should be, inside the file the updater reads.

The alternative — teaching the script to apply that one file *after* the migration that adds
`script_name` — is worse: it hard-codes an ordering between two of their migrations, and the next
sync moves both.

## What this press did prove

* **The rollback works, on the live install, unattended.** Image tag restored, `tw_world` restored
  from the dump, both character-side changes undone, every count back to the row. That had never
  been pressed either.
* The refusal guard held: the script checks for a running server and would have refused.
* `AutoUpdater` cancels the server rather than starting half-migrated, which is the behaviour that
  makes this recoverable at all.

Logs: `../../../../scratchpad/tortoise-reimport-live.log` and `tortoise-rollback.log` in the session
scratchpad (not committed; they carry the generated database password).
