# T46 — mod-arac's server DBCs are never copied, so ARAC characters start naked

**Status:** FILED — diagnosed, parked for later (owner, 2026-09-14: "add this to do later")
**Filed:** 2026-09-14 by the lead, from the owner: "can you find a version of https://github.com/heyitsbench/mod-arac that works ?"

## What is wrong

The trouble is not which mod-arac version we use. On WotLK, Yu'lon never copies any
module's `server_dbc` step:

- `pylauncher/yulon/ui/controller_view.py:1198` builds `wotlk_modules.applier(...)` with no
  `dbc=`.
- `Applier._dbc` (`pylauncher/yulon/apply.py:2214`) therefore logs
  `server_dbc … : no DBC copier configured` as *skipped* and moves on.
- `DbcCopier.copy_dbc_dir` (`apply.py:494`) is a Protocol only. Outside the tests, nothing
  implements it (grep on 2026-09-14).

So `CharBaseInfo.dbc`, `CharStartOutfit.dbc` and `SkillRaceClassInfo.dbc` never reach the
server. The SQL is applied and the client's `Patch-A.MPQ` is copied, so the creation screen
offers the new combinations. The server then disagrees. What that looks like was reported
upstream as heyitsbench/mod-arac#49 and #50 (2026-08-01, mod-playerbots core 190184a):

- new race/class combinations start with no items
- `Player X (GUID: N), has skill (54) that is invalid for the race/class combination (Race: 3, Class: 11). Will be deleted.`

The reporter closed both after putting the DBC files where the worldserver reads them.

Not yet confirmed on our own install. The owner still has to say whether this is the
symptom they saw.

## Which version to ship (surveyed 2026-09-13)

Each fork's SQL was checked statically: every INSERT targets a table that exists, and every
row has as many values as that table has columns, in the core's
`data/sql/base/db_world` at mod-playerbots `Playerbot` @06234df. All four passed. Upstream's
`arac.sql` was also applied for real in the 8.7a press
(`pyplan/gates/8.7a-direct-sql-yulon-ubuntu2-2026-09-09/`).

| fork | head | notes |
|---|---|---|
| **ChromWolf/mod-arac-updated** (recommended) | a8d57e0, 2025-11-20 | Only fork with a user report of it working on a recent mod-playerbots core. Data-only, no rebuild. Adds missing racials and weapon skills. Ships edited `Spell.dbc` (49 MB) and `SkillLineAbility.dbc`, and a 4.7 MB `Patch-A.MPQ`. SQL is split into six `ARAC_*.sql` files; `arac.sql` is renamed `.OLD`. |
| heyitsbench/mod-arac (current pin) | 3f605e0, 2025-07-25 | Works once the DBCs land. Known gaps: some spells tied to race (Seal of Vengeance/Corruption, Heroism/Bloodlust, upstream PR #46), no trainers (#26). |
| TheSCREWEDSoftware/mod-arac (upstream PR #48) | 45a776c, 2026-04-09 | Adds shapeshift models and an optional trainers SQL. Renames `DBFilesContent` to `DBFilesClient`. Its C++ file is an empty stub. |
| AlsoNotMehh/mod-arac-enhanced | ab6e05d, 2026-09-04 | New, needs a C++ rebuild, no user reports. Skip. |

## To do

1. Owner confirms the symptom (naked start, or the "will be deleted" skill lines).
2. Find where our WotLK worldserver reads DBCs (its `DataDir` inside the container / the
   client-data volume). Measure it on a box; do not copy the answer from the upstream issue
   thread, which names two different paths.
3. Implement a `DbcCopier` for WotLK and pass it at `controller_view.py:1198`. Grep the
   caller afterwards, not only the definition.
4. Switch `manifests/wow-wotlk/modules/mod-arac.json` to `ChromWolf/mod-arac-updated`: six
   SQL steps instead of `arac.sql`, and check the `server_dbc` / `client` paths against
   that tree.
5. Live gate on a test box with a real 3.3.5a client: make one new combination (e.g. Human
   Hunter). It must start with its gear, and the worldserver log must have no "invalid for
   the race/class combination" lines. Stop the server afterwards.

Note for step 4: the manifest says removing the module does not revert the SQL, DBCs or
MPQ, and with ChromWolf that includes a replaced server `Spell.dbc`. A remove path that
promises a clean core would need to restore stock DBCs.
