# 8.5a — Browse Bots, WoW WotLK — the WINDOWS half

**Where and when.** `yulon-win11`, 2026-09-08 15:09Z – 16:09Z, against a real AzerothCore-with-
playerbots install at `D:\wow-server` (1000 characters, 500 of them in the world). Code at
`7a3a603f` in `C:\gate\src84a`, venv `C:\gate\venv84a` (Python 3.11.9, PySide6 6.11.2). The client
half ran on the Hyper-V host (`DESKTOP-FP27AUV`) with its own 3.3.5a client at
`C:\clients\WoW-WotLK-3.3.5a-min`, against this VM at `172.30.55.155`.

8.5a's line declares the Windows half its own press. `gate85a_win.py` is the Linux gate's own
script with four constants changed — the source root, the server dir, the shots directory and an
activity log — and **one stage rewritten**, because the committed Linux script is stale: its
`stage_page` calls `browser.page(offset=…)`, and that argument became a `(name, guid)` cursor when
the adversarial review of that box replaced `OFFSET` with keyset paging. A gate calling the old
signature does not fail loudly; it dies with a `TypeError` before reading anything.

## The server this half ran on had to be brought back first

**The box's WotLK install did not exist when this session started.** 8.9a's Windows half purged it
that afternoon — that was the clause, and it worked. `D:\gate\wotlk-server77` (8.2b's and 8.3a's
server) was gone too, and `docker images` held nothing but the TBC stack.

Installing a new one would be a **source build**, which the session is not allowed to start. The
route taken instead was the Hyper-V checkpoint 8.9a left behind, `8.9a-win-before-2026-09-08`
(15:14 local, made before its own purge). Restoring it brought back `D:\wow-server`, both volumes
and all four `yulon.local/ac-wotlk-*:native-643123e5` images. The ground before the restore is in
`ground-before-restore.txt`: `D:\wow-server exists: False`, one volume, no WotLK images. Nothing was
built. 8.9a's own evidence was already committed, so nothing was lost by rolling the box back.

Two per-box facts came back with the checkpoint and were fixed the way 8.3a's README says:

* the two enabled **`Docker Desktop Backend` BLOCK rules** were back on Private and Public, so the
  published ports were unreachable from the host until they were disabled;
* the realm row read `172.30.52.116` from an older lease. Set to `172.30.55.155` **through the
  app's own Networking plan/apply** (`../8.4a-wotlk-yulon-win11-2026-09-08/gate84a_win_realm.py`),
  not by writing the row.

The box's clock reads **CEST now, not PST** as 8.2b's recipe says. Measured, not inherited.

## The clauses

| Definition of done | Reading |
|---|---|
| the total equals the same clause run by hand | **1000 of 1000** at 15:09Z (`logs/log-gate85a_win-total.txt`), and **999 of 1000** at 16:08Z after the 8.4a half moved one character off the bot roster (`logs/log-gate85a_win-after-adopt-total.txt`). Both equal the same clause run by hand |
| the split by marker type is shown | `registry=1000 prefix=0`, and `999/0` after; each half counted separately against the server, and `has_registry(entry)` is True on this tree so the split is drawn |
| an unreadable marker refuses to answer | `total=None`, *"this install's bot marker could not be read, and an empty marker would match every account on the server"* |
| a readable marker matching nothing warns rather than reporting zero | **not reachable on this tree, and that is the honest answer** — see below |
| a listed online bot is found by the in-game who-list | **`[Aegli]: Level 66 Dwarf Death Knight - Shattrath City` / `1 player total`**, `shots/6-client-who-list.png`, asked from `Amvezuan` — a bot the app's own list had shown as online |

Screenshots, all from the real `ControllerView` rendered offscreen with `QT_QPA_FONTDIR` set (8.9a's
measurement on this box: without it every glyph is a tofu box):
`1-bots.png` the tab, `2-page-two.png` Next, `3-back-to-page-one.png` Previous returning the same
rows, `4-filtered.png` a filter that matches one, `5-filtered-to-nothing.png` a filter that matches
nothing.

## What this tree refuses to prove, and says so

The zero-warning clause **cannot be reached here**, exactly as 8.5a found on Linux and for the same
reason: the marker clause has two arms and the registry answers whether or not the prefix does. A
marker matching nothing still returned the full total (`1000`, then `999`) with `by_prefix=0` and no
warning at all — because there is nothing to warn about. That path is a one-signal tree's, and it is
gated live on 8.5b (TBC) and 8.5c (Vanilla). This half asserts the behaviour it actually has:
`warning == ""` and the total unchanged.

**The filtered zero is a different sentence and it was checked**: filtering to `Zzzznosuchbotname`
reads *"no bot's name begins with 'Zzzznosuchbotname' — this server has 999 bots"* and never mentions
the marker. That is 8.5b's defect, re-proved on the tree 8.5a is ticked on.

## The paging stage, written against the cursor

The stale `offset=` stage was replaced by one that asserts what a cursor gives and an offset cannot:

* `next_after` is a two-element tuple and it **is the last row of the page it came from** — the name
  from the page and the guid asked of the server (`('Amormin', 969)`, and `SELECT guid … WHERE name
  = 'Amormin'` → `969`). `Bot` carries no guid, so the second half is checked against the database
  rather than against the object;
* page two begins after that key and shares no name with page one;
* **the same cursor read twice returns the same page** — the property an `OFFSET` loses the moment a
  bot logs out ahead of the boundary;
* a filter narrows the total (`Aax` → 1 of 999) rather than matching everything.

## `/who` is faction-filtered here too

`Aegli` is race 3 (Dwarf) and the asking character is a Dwarf Death Knight, so both are Alliance and
the answer came back. That is 8.5a's, 8.5b's and 8.5c's finding holding on a fourth press; this box
did not re-derive the faction map, it chose an Alliance target from the races the server reports for
the online bots on page one (`logs/log-gate85a_win-page.txt` lists them).

## The files

* `gate85a_win.py` — the Linux gate's script, four constants changed and `stage_page` rewritten.
* `logs/log-gate85a_win-*.txt` — every stage, in the order it ran. The `after-adopt-*` set is the
  same five stages re-run once the 8.4a half had taken `Amvezuan` off the bot roster.
* `ground-before-restore.txt` — the box as it was before the checkpoint restore.
* `shots/` — the tab, and the client's who-list.

## How to re-run it

```
powershell -File C:\gate\run.ps1 -Driver gate85a_win -Stage total|split|refuse|page|shots
```
