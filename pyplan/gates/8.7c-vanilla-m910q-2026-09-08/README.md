# 8.7c — Modules, WoW Vanilla. Run on m910q, 2026-09-08

**Box** (`pyplan/checklist.md` line 2503): *Modules, WoW Vanilla — as 8.7b.*
**Definition of done** (8.7b, line 2502): *at least one manifest installs and its key is
reported by the running server after the restart it asks for.*

**Verdict: every clause passed.** Ran 09:51:18Z → 09:54:57Z against `~/vanilla-75b`, the
Vanilla install already on that box, with the stack up and the world loaded. The driver is
`gate87c.py` (six stages) plus `sqlmod-restart.py`; each stage's stdout is the `NN-*.txt`
file it names, and each screenshot's subject is in its filename.

---

## What the box asked for, and where it is

| Clause | Where | Verdict |
|---|---|---|
| a manifest set for this game | `00-ground.txt`, `1-modules-tab.png` | five mods, all `wow-vanilla` |
| and a binding | `00-ground.txt` | `store.game=wow-vanilla`, applier at `/home/pk/vanilla-75b`, world → **`mangos`**, client `mariadb` in `vanilla-db` |
| **at least one manifest installs** | `01-install.txt`, `2-installed.png` | `motd`: one key set, exactly one changed line |
| **and its key is reported by the running server** | `03-restart.txt`, `3-server-reports-the-new-motd.png` | `Current Message of the day: Welcome!` |
| **after the restart it asks for** | `02-unrestarted.txt` then `03-restart.txt` | the running server reported the OLD line until the restart |
| a mod is a conf activation **or a SQL mod** | `05-sqlmod.txt`, `06-sqlmod-restart.txt` | 2319 rows changed in `mangos.item_template` and put back |
| reversible | `04-remove.txt` | conf byte-identical: `334915dd…` before and after |

The three console readings, in order, are the whole gate:

```
09:51:22Z  the RUNNING server reports Motd: 'Welcome to the Continued Massive Network Game Object Server.'
09:51:39Z  without a restart, the server still reports: 'Welcome to the Continued Massive Network Game Object Server.'
09:52:55Z  after the restart the server reports: 'Welcome!'
```

The middle line is the one that makes `build.restart` load-bearing rather than decorative:
the file said `Welcome!` while the server said the old line, so the restart is what carried
the value and not a re-read.

## Ground, recorded before every step

`etc/mangosd.conf` was copied to `etc/mangosd.conf.before-8.7c` and hashed before anything
ran, and every stage refuses if its assertion is already true — `stage_install` will not run
when the server is already reporting the value it would write, and `stage_sqlmod` will not
run when every stackable row is already at 200 (it was 68 of 2319). The containers running at
the moment of every capture are printed beside it, because a screenshot of a dead server
photographs exactly like a refusal.

## Per-tree facts, measured on this tree and not inherited from 8.7b

Read in `~/vanilla-75b` and `~/vanilla-75b/src/mangos-classic` on 2026-09-08:

* **`.server motd` is `src/game/Chat/Chat.cpp:797`** here, bound to
  `HandleServerMotdCommand` with `allowConsole` true, answering from
  `src/game/Chat/Level0.cpp:280-282` via `sWorld.GetMotd()`. The TBC box cites `Chat.cpp:816`
  in a different checkout; the line numbers do not transfer.
  `Chat.cpp:785` binds a second `motd` under `server set` which changes the running value
  without writing the conf — the gate never uses it, because a value that does not survive a
  restart proves nothing about a file.
* **`AllowTwoSide.Interaction.Trade` exists on this fork** (`World.cpp:525`,
  `etc/mangosd.conf:928`) and does not exist on mangos-tbc, whose manifest says so. So
  `cross-faction` is **ten** key/patch pairs here and nine there. A copied manifest would
  have switched nine things on, left trading off, and reported success.
* **There are no `Rate.XP.Kill.Vanilla` / `.BC` twins.** `World.cpp:422` sets `Rate.XP.Kill`
  and nothing else of that family, so the TBC manifest's per-tier caveat is not a fact here.
* **`Rate.Pet.XP.Kill` ships two lines above `Rate.XP.Kill`** (`mangosd.conf:1509-1510`), so
  the anchored patterns are load-bearing on this tree in a way they are not on the other.
  `test_the_xp_mod_leaves_the_pet_rate_alone` fails on an unanchored one.
* `item_template.stackable` is `smallint(5) unsigned` in this tree's own base schema
  (`src/mangos-classic/sql/base/mangos.sql:2682`).
* The installer's conf table for this entry owns three `AiPlayerbot.SyncLevel*` keys its TBC
  sibling does not; no manifest here writes anything in it
  (`test_no_manifest_writes_a_key_the_installer_rewrites_on_every_run`).

## Findings

1. **The Modules tab never asks for the one value the mod exists to set.** `motd`'s whole
   point is a sentence the operator chooses, and pressing Install writes the manifest's
   default (`Welcome!`) without a prompt. `ControllerView._module_values()` opens the dialog
   only when some prompt has **no** default — deliberately, and reasonably, for the 39
   manifests where a default is a good answer — but for `motd`, `xp-rates` and
   `all-stackables` the default is a placeholder and the tab offers no way past it. Found by
   the first run of `stage_install`, whose gate value was never written because the
   `_prompt_asker` it replaced is never called. Not previously in `pyplan/bug-checklist.md`;
   filed there now. It is not a Vanilla defect — it is the same on `wow-tbc` and `wow-wotlk`.
   The gate then used the manifest's own default, which is what a user gets, and the clause
   still holds because that value differs from the line the server was running.
2. **A gate-driver defect, recorded rather than quietly fixed.** `stage_install` first
   asserted the word "restart" appeared in the report, and failed on a report that had asked
   for one correctly: the app's own words are *"Press Stop and then Start on the Server tab to
   apply this."* The assertion now checks that sentence. Nothing about the app changed.

## What this gate does NOT prove

* **The Modules tab as a widget.** The presses go through the real `ControllerView` and its
  real buttons (`install_module_button.click()`), with jobs run inline; what is not driven is
  a mouse or the prompt dialog, which finding 1 explains is never opened for these items.
* **`xp-rates`, `all-flight-paths`, `cross-faction` live.** Same mechanism against different
  keys; each is pinned by `test_vanilla_modules.py`, and `cross-faction`'s tenth key has its
  own test. Only `motd` was pressed against the running server, because only `motd` is
  reported back by it.
* **That a player sees any of it.** No client was driven. `.server motd` is the server's own
  answer about its own state, which is what the clause asks for.

## Files

| File | What it is |
|---|---|
| `gate87c.py` | the driver: `ground`, `install`, `unrestarted`, `restart`, `remove`, `sqlmod` |
| `sqlmod-restart.py` | step 6: the world still loads with the SQL mod applied |
| `00-ground.txt` | the conf checksum, the binding, and the Motd before anything was pressed |
| `01-install.txt` | the install report and the one-line diff |
| `02-unrestarted.txt` | the running server still on the old value |
| `03-before-motd.txt` | the first console reading, taken by hand before the driver existed |
| `03-restart.txt` | **the definition of done** |
| `04-remove.txt` | the conf back, byte for byte |
| `05-sqlmod.txt`, `06-sqlmod-restart.txt` | the SQL half |
| `1-modules-tab.png` … `6-sql-mod-removed.png` | the app, at each step |

## The box afterwards

`~/vanilla-75b/etc/mangosd.conf` is byte-identical to what this gate found (`334915dd…`), and
`etc/mangosd.conf.before-8.7c` is left beside it as the reference — the same convention the
earlier `mangosd.conf.before-channel` follows. The Vanilla stack was left running for 8.9b,
which ran next on the same box; see that gate's README for what the box was left in.
