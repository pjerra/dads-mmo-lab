# 8.8 read half — the Steam layout on `yulon-arch`, the Deck stand-in

Captured 2026-09-10 21:49 CEST on this laptop; the box's own clock read `2026-09-10T19:50:03+00:00` at the first probe.
Box: `yulon-arch` (Hyper-V, Arch Linux, XFCE on `:0`), Steam installed and running in
Big Picture with the owner's account signed in. **The account is referred to here and in
every other record only by its userdata id `18347166`.** No account name, no token and no
byte of `loginusers.vdf` is reproduced in this folder.

Everything below is backed by `layout.txt` (the raw probe output, one section per claim) and
`live-shortcut-entries.json` (the two verbatim entries from the prior art's manifest).

**`claude-say` was not on this box.** The standing order to announce every box action needs a
place to announce to, so the hand installed a two-line `~/bin/claude-say` that stamps
`~/claude-activity.log` from the clock and broadcasts with `wall -n`. It deliberately does
**not** open a terminal window: Steam Big Picture owns `:0` and a window would sit on top of
the frames this gate exists to capture. `~/claude-activity.log` already existed with lines
from earlier work, so the log is shared rather than new.

## 1. The layout on this box

```
/home/pk/.local/share/Steam/userdata                            DIR   -> realpath /home/pk/.local/share/Steam/userdata
/home/pk/.steam/steam/userdata                                  DIR   -> realpath /home/pk/.local/share/Steam/userdata
/home/pk/.var/app/com.valvesoftware.Steam/data/Steam/userdata   absent
```

**Two of the prior art's three roots exist and they are the same directory.** `~/.steam/steam`
is a symlink to `~/.local/share/Steam` (`layout.txt`, "`steam roots probed`" and the `~/.steam`
listing: `steam -> /home/pk/.local/share/Steam`). `dmlpack.steam_config_dir()` iterates the
three roots and appends a candidate per root, so on a box laid out like this one it would
collect **the same profile twice**. That does not matter to an add-only routine that sorts and
takes the first, and it matters a great deal to this ticket, whose refusal is *"more than one
profile"*: a finder that counted candidates rather than resolved directories would refuse on a
perfectly ordinary single-account box. **The finder in `steam.py` de-duplicates by
`Path.resolve()` before it counts.** This is the read's single most load-bearing finding.

The one profile:

```
/home/pk/.local/share/Steam/userdata/18347166
/home/pk/.local/share/Steam/userdata/18347166/7/remotecache.vdf
/home/pk/.local/share/Steam/userdata/18347166/config
/home/pk/.local/share/Steam/userdata/18347166/config/cloudstorage
/home/pk/.local/share/Steam/userdata/18347166/config/compat.vdf
/home/pk/.local/share/Steam/userdata/18347166/config/librarycache
/home/pk/.local/share/Steam/userdata/18347166/config/licensecache
/home/pk/.local/share/Steam/userdata/18347166/config/localconfig.vdf
```

**There is no `shortcuts.vdf` and no `grid/` on this box** (`ls` says *No such file or
directory* for both). The writer therefore has to create both, which is the path the prior art
handles with `root = {"shortcuts": {}}` and a `mkdir`, and it is the path this gate runs.

The prior art's account-detection rule — *the `userdata/<id>/config` that has a
`shortcuts.vdf`, most recently used wins* — cannot fire here for the same reason: no profile
has one yet. The rule that does the work is the one this ticket asks for: **exactly one
resolved `userdata/<id>/config` directory, or refuse by name.**

## 2. The fields of a live entry

From `live-shortcut-entries.json`, packed by `dmlpack.snapshot_live_shortcuts()` off a real
Steam Deck. Eighteen keys, in this order, and the order is preserved on write because the
codec serialises a dict in insertion order:

```
appid (int32)   AppName (str)   Exe (str, QUOTED)      StartDir (str)
icon (str)      ShortcutPath (str)                     LaunchOptions (str)
IsHidden (int)  AllowDesktopConfig (int)               AllowOverlay (int)
OpenVR (int)    Devkit (int)    DevkitGameID (str)     DevkitOverrideAppID (int)
LastPlayTime (int)              FlatpakAppID (str)     sortas (str)   tags (map)
```

The client entry, verbatim:

```json
{"appid": -105858796, "AppName": "Turtle WoW",
 "Exe": "\"/home/deck/Games/TurtleWoW/TurtleWoW/WoW.exe\"",
 "StartDir": "/home/deck/Games/TurtleWoW/TurtleWoW/",
 "icon": "", "ShortcutPath": "", "LaunchOptions": "",
 "IsHidden": 0, "AllowDesktopConfig": 1, "AllowOverlay": 1, "OpenVR": 0,
 "Devkit": 0, "DevkitGameID": "", "DevkitOverrideAppID": 0,
 "LastPlayTime": 1786161145, "FlatpakAppID": "", "sortas": "", "tags": {}}
```

and the server entry:

```json
{"appid": -966953680, "AppName": "Turtle WoW V2 Server",
 "Exe": "\"/usr/bin/konsole\"", "StartDir": "\"/home/deck\"",
 "icon": "", "ShortcutPath": "",
 "LaunchOptions": "--hold -e bash /home/deck/tortoise-wow-v2-launcher.sh",
 "IsHidden": 0, "AllowDesktopConfig": 1, "AllowOverlay": 1, "OpenVR": 0,
 "Devkit": 0, "DevkitGameID": "", "DevkitOverrideAppID": 0,
 "LastPlayTime": 1786160012, "FlatpakAppID": "", "sortas": "", "tags": {}}
```

Three things a writer has to take from those two and not from a tidy summary of them:

* **`Exe` is quoted, `StartDir` sometimes is and sometimes is not.** The server entry's
  `StartDir` carries its own quotes; the client entry's does not. Both were made by Steam.
  `dmlpack`'s own docstring records a third live entry, "Dekaron Server", storing `StartDir`
  unquoted while its siblings quote it. So **`StartDir` quoting is not a rule to enforce**;
  the writer quotes what it creates and leaves what it finds alone.
* **`appid` is a signed int32 in the file** and `crc32(...) | 0x80000000` is unsigned, so the
  value goes through a `to_signed` before it is written. Both stored appids are negative.
* **The stored `appid` is not always `gen_appid(Exe, AppName)`.** Measured, on these two:

  | AppName | stored appid | `gen_appid(Exe, AppName)` | equal? |
  |---|---|---|---|
  | `Turtle WoW` | `-105858796` | `-1169327133` | **no** |
  | `Turtle WoW V2 Server` | `-966953680` | `-966953680` | yes |

  Seven other derivations were tried against the mismatching entry (unquoted exe, name first,
  exe alone, exe+name+startdir, …) and none reproduces `-105858796`; the most likely history
  is that the shortcut was renamed or re-pointed after Steam created it, and Steam keeps the
  appid it first issued. `dmlpack` notices this and prints a warning
  (`dmlpack.py:1553-1557`) — nothing more. **The consequence for this ticket is a rule, not a
  warning:** an upsert that replaces an entry keeps the entry's stored `appid` rather than
  recomputing it, because the grid artwork on disk is named after the appid Steam is actually
  using, and recomputing would leave the art orphaned and the tile blank. `gen_appid` is used
  when an entry is *created*.

## 3. The grid folder naming

`<Steam>/userdata/<id>/config/grid/`, keyed by the appid with no lookup table:

| file | what Steam shows it as |
|---|---|
| `<appid>.png` | the wide header / capsule |
| `<appid>p.png` | the vertical library portrait |
| `<appid>_hero.png` | the page banner behind the title |
| `<appid>_logo.png` | the transparent logo laid over the hero |

This is why `gen_appid` matters beyond identity: art written for an appid follows a restore for
free, and a rename changes the appid and orphans four files. `dmlpack`'s manifest carries
`"grid_note": "neither shortcut has grid artwork on this machine, so extras ships none"` — the
artwork is the half of 8.8 that has **no** prior art at all.

## 4. `CompatToolMapping`

It is **not** in `userdata/<id>/config/compat.vdf`. That file exists on this box and holds

```
"platform_overrides"
{
}
```

which is a different thing. `CompatToolMapping` lives in the *global* text VDF,
`~/.local/share/Steam/config/config.vdf`, as a child of
`InstallConfigStore/Software/Valve/Steam`:

```
"InstallConfigStore"
{
	"Software"
	{
		"Valve"
		{
			"Steam"
			{
				"AutoUpdateWindowEnabled"		"0"
```

Tab-indented, one `"key"\t\t"value"` per line, brace on its own line. **On this box the
section is absent:** `grep -c -i CompatToolMapping` answers `0` against a 17,977-byte file
(sha256 `cf136080…e17c80`, recorded in `layout.txt` so the write half can prove it changed
nothing else). So the writer's compat edit is an *insertion* of a section that does not exist,
not an edit of one that does — and the minimal insertion is four tabs of indent immediately
after the `"Steam"` map opens, which is where Steam itself writes it.

The shape Steam writes, for the record, is

```
				"CompatToolMapping"
				{
					"<appid>"
					{
						"name"		"<tool>"
						"config"		""
						"priority"		"250"
					}
				}
```

## 5. Proton on this box

`~/.steam/steam/steamapps/common/` **does not exist** and `compatibilitytools.d` did not
exist either — this Steam has never installed a game or a tool. A compat mapping naming a
Proton the box does not have is a mapping that fails at launch, so the writer refuses by name
rather than writing a dangling one. To make the write half's compat edit provable rather than
theoretical the hand installed **GE-Proton11-6** (the upstream x86_64 release tarball) into
`~/.steam/root/compatibilitytools.d/`; that is an additive change to the box and is recorded
here and in the write half's README as a deviation from "everything else as found".

## 6. SteamOS's read-only root — the question the checklist asks

No Deck is reachable from this side, so this is answered from the repository's own evidence
rather than from hardware:

* Yu'lon already knows SteamOS's root is read-only until unlocked, and brackets exactly one
  thing with the unlock — installing a package: `platform.py:434-439` wraps `pacman -Sy ufw`
  in `steamos-readonly disable` / `steamos-readonly enable`, and `is_steamos()` at
  `platform.py:132-137` is the test.
* The paths in the Deck-packed manifest are all under `/home/deck`
  (`/home/deck/Games/TurtleWoW/…`, `/home/deck/tortoise-wow-v2-launcher.sh`), and the file
  this feature writes is `~/.local/share/Steam/userdata/<id>/config/shortcuts.vdf` — i.e.
  `/home/deck/.local/share/...`, in the user's home, which is a separate writable partition on
  SteamOS and not part of the read-only root image.
* Corroborating from this box, which is Arch (`ID=arch`, not `ID=steamos`) and so proves only
  the ordinary case: `touch` succeeds in both
  `/home/pk/.local/share/Steam/userdata/18347166/config` and
  `/home/pk/.local/share/Steam/config` as the plain user, on a read-write btrfs root.

**Answer: the userdata folder this box writes to sits outside SteamOS's read-only root, and the
writer must not and does not call `steamos-readonly`.** `steam.py` contains no unlock.

## 7. What could and could not be staged

* **Could not: adding a non-Steam game by hand through Big Picture.** The box is driven over
  ssh; Big Picture's "Add a non-Steam game" flow needs a pointer and a file browser on `:0`,
  and no controller or synthetic-input path is set up on this VM. The optional
  hand-added-shortcut capture was therefore skipped, **and it was optional for a reason**: the
  ticket names the manifest's two entries as the authoritative capture and they are captured
  here in full, field for field, from a real Deck.
* **Could not: a before/after `config.vdf` around Steam setting a compat tool by hand**, for
  the same reason. The *shape* above is recorded from Steam's documented layout and the
  before-file's sha256 is recorded so the write half can show its own edit changed nothing else.
* **Could: everything in §§1-6**, each from a live probe on the box.
* **Not applicable: capturing a `shortcuts.vdf` from this box before the write** — there is
  none. Its absence is itself the capture.

## Files

| file | what it is |
|---|---|
| `layout.txt` | every probe run on the box, in order, unedited |
| `live-shortcut-entries.json` | the two verbatim entries from `origin/rust-main`'s Deck manifest |
| `00-before-bigpicture.png` | the box's screen at the start of the read: Steam in Big Picture, signed in |
