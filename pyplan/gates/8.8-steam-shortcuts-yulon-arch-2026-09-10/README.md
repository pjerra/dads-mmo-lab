# 8.8 write half — the two Steam entries, live on `yulon-arch`

Written 2026-09-10 22:32 CEST on this laptop; every stamp below comes from a clock, either the
box's (`date -Is` at the head of each capture) or the file's own mtime.

Box: `yulon-arch` — Hyper-V, Arch Linux (`ID=arch`, **not** SteamOS), XFCE on `:0`,
Steam installed and the owner's account signed in. **The account is named only by its
userdata id `18347166`.** No account name, no token and no byte of `loginusers.vdf`
appears here.

The read half is `pyplan/gates/8.8-steam-read-yulon-arch-2026-09-10/`; this half
assumes its findings and does not repeat them.

**Every sentence below is backed by a file in this folder.** The order of the files is
the order of the run.

## What was pressed, and how

`ControllerView.add_to_steam` runs `self.services.steam.add()` and nothing else, so the
live half calls `SteamShortcuts.add()` from a small driver on the box with **no fakes**:
the real `platform.detect()`, the real `pgrep -x steam`, the real home directory. The
driver's first three lines are in every press capture and say which of each it got.

**The names in the frames are the driver's, not the button's.** The driver passed
`game="Turtle WoW"`; the button passes `entry.name`, which for this game is
**"WoW Tortoise"** (`catalog.json:949`). So a press through the app produces
*WoW Tortoise* and *WoW Tortoise Server*, and different appids with them. Nothing
else about the run changes — the appid is `crc32(Exe + AppName)` either way, and every
claim below is about the mechanism rather than about those two strings.

**The GUI was not driven.** Two reasons, and the first was measured rather than assumed:
Qt could not initialise its `xcb` platform plugin on this box at all
(*"This application failed to start because no Qt platform plugin could be
initialized"*) until the hand installed `libxcb`, `xcb-util-{wm,image,keysyms,renderutil,cursor}`
and `libxkbcommon-x11`. After that it starts — and it is what the **server entry opens**,
which is frame 4. But the button itself needs a remembered install with a tab, and
building one on this box would be staging the thing under test. The button's own
behaviour is covered offscreen in `tests/test_controller_view.py`.

## 1. The press, with Steam closed — `press-1-add.txt`, `press-1-on-disk.txt`

Steam was shut down with `steam -shutdown` on `:0` and `pgrep -x steam` went quiet
within four seconds. Then:

```
platform.detect()      : linux
steam.steam_running()  : False
pgrep -x steam         : rc=1
shortcuts.vdf before   : (absent)
```

and the sentence the button shows, verbatim:

> Added two entries in the Steam library: “Turtle WoW” (the game client, through the
> compatibility tool GE-Proton11-6-x86_64) and “Turtle WoW Server” (this launcher).
> Written to /home/pk/.local/share/Steam/userdata/18347166/config/shortcuts.vdf. There
> was no shortcuts.vdf before this, so there was nothing to back up. 6 artwork files
> were written under /home/pk/.local/share/Steam/userdata/18347166/config/grid. The
> compatibility tool was set in /home/pk/.local/share/Steam/config/config.vdf, backed
> up as config.vdf.yulon-bak-20260910-201500. Restart Steam to see them.

The two entries as they landed in the file:

```
[0] appid=-483334338 unsigned=3811632958 AppName='Turtle WoW'
    Exe="/home/pk/clients/TurtleWoW/WoW.exe"  StartDir="/home/pk/clients/TurtleWoW"
    LaunchOptions=''
[1] appid=-192250976 unsigned=4102716320 AppName='Turtle WoW Server'
    Exe="/home/pk/y8v313/bin/python"  StartDir="/home/pk/y8v313/bin"
    LaunchOptions='/home/pk/y8/pylauncher/main.py'
```

The client entry points at the client this box actually has — `~/clients/TurtleWoW`
holds `WoW.exe`, `TurtleWoW.exe` and `TWPatcher.exe`, and `client_executable()` picks
`WoW.exe`. **Nothing here is staged:** the ticket allowed for an entry pointing at where
the app *would* put a client, and it was not needed.

The server entry points at this Yu'lon: `sys.executable` with `main.py` in the launch
options, which is what `launcher_command()` answers from source. Frozen it would be the
`yulon` binary with no launch options.

**The compat edit changed nothing else**, and that is a `diff` rather than a claim
(`press-1-on-disk.txt`): deleting the inserted `CompatToolMapping` block from the new
`config.vdf` gives back the backup **byte for byte** —

```
## compat edit: everything outside the inserted block is byte for byte the backup
IDENTICAL
```

and the block itself, at four tabs, immediately inside the `"Steam"` map:

```
				"CompatToolMapping"
				{
					"3811632958"
					{
						"name"		"GE-Proton11-6-x86_64"
						"config"		""
						"priority"		"250"
					}
				}
```

Six grid files, three slots each, and the two marks are **not** the same picture:

```
b76a02b1e9051dabc3772a23f6b83dfe  grid/3811632958.png   (and p, _hero)   -- jade, the client
519095835a5b4df885f76af26c38454a  grid/4102716320.png   (and p, _hero)   -- ember, the server
```

## 2. Both entries in a real Steam library — `2-library-two-entries.png`, `3-server-entry-page.png`

Steam was started again with `steam -bigpicture`. Big Picture's search finds two, both
`IN LIBRARY`, under `LIBRARY 2`, wearing the artwork this press wrote — one jade, one
ember. Opening the second gives **Turtle WoW Server** with a green **Play**.

**`1-logo-slot-hid-the-name.png` is a failure, kept on purpose.** The first version of
the writer filled all four grid slots. Steam draws `<appid>_logo.png` *instead of* the
entry's name, so the game page showed a Play button, a jade Y, and nothing at all to say
which of the two entries it was — and the two differ by one word. The writer now leaves
`_logo` empty and gives the two entries different colours, and
`test_the_logo_slot_is_left_empty_and_the_two_tiles_are_not_the_same_picture` holds
both halves of that down. Frame 3 is the same page after the fix.

## 3. The server entry launched, for real — `4-server-entry-opened-the-launcher.png`

**Play** was pressed on the Turtle WoW Server page in Big Picture. Steam's own process
list is the decisive line:

```
/home/pk/.local/share/Steam/ubuntu12_32/reaper SteamLaunch AppId=4102716320 -- \
    /home/pk/y8v313/bin/python /home/pk/y8/pylauncher/main.py
```

**What it opens on this box:** the Yu'lon launcher itself — the frame is its window,
titled *"Yu'lon — Dad's MMO Lab launcher 0.6.59"*, showing the Catalog tab with the four
games. This box has no remembered install in its `state.json`, so it opens on the
Catalog rather than on a Server tab; on a Deck with an install it would open on that
install's tab, where **Start** is.

## 4. The client entry launched, for real — `5-client-entry-running-through-proton.png`

**Not staged — it ran.** The frame is the Turtle WoW intro cinematic, full screen, on the
VM's software renderer. The process tree names the compatibility tool this press wrote
into `config.vdf`:

```
reaper SteamLaunch AppId=3811632958 -- .../SteamLinuxRuntime_4/_v2-entry-point \
  --verb=waitforexitandrun -- \
  /home/pk/.local/share/Steam/compatibilitytools.d/GE-Proton11-6-x86_64/proton \
  waitforexitandrun /home/pk/clients/TurtleWoW/WoW.exe
```

and Steam's own log (`steam-own-logs.txt`) says the same thing in its own words:

```
[2026-09-10 20:21:29] Proton: Upgrading prefix from None to GE-Proton11-6
                      (/home/pk/.local/share/Steam/steamapps/compatdata/3811632958/)
[2026-09-10 20:21:27] CGetCompatToolBucketJob for GE-Proton11-6-x86_64 starting.
```

That is the compatibility-tool half of 8.8 proved end to end: Yu'lon wrote
`CompatToolMapping/3811632958 -> GE-Proton11-6-x86_64` and Steam launched a Windows
executable through it.

One caveat, named rather than hidden: it was launched with `steam steam://rungameid/…`
rather than by clicking the tile. Big Picture's pointer could not be steered onto the
right one of two identically-shaped tiles over ssh — three attempts landed on the
server entry — and `rungameid` is Steam launching its own shortcut by its own id, which
is the same code path the tile takes.

## 5. A second press changes nothing — `press-2-replace.txt`, `press-2-on-disk.txt`

Between the two presses Steam had **played both entries and rewritten the file itself**,
putting its own `LastPlayTime` into each (`steam-rewrote-lastplaytime.txt`):

```
0 'Turtle WoW'        LastPlayTime= 1789071688
1 'Turtle WoW Server' LastPlayTime= 1789071433
```

which is the running-Steam hazard this feature refuses over, caught in the act. Steam was
closed again and the button pressed a second time:

> **Replaced** two entries in the Steam library … It was backed up first as
> shortcuts.vdf.yulon-bak-20260910-202555. …

and the file did not change **at all**:

```
1f6b4d7f48d8d2716403f290ebc2c3c0f5538f70d3509e250ae3e07133424d4d  shortcuts.vdf
1f6b4d7f48d8d2716403f290ebc2c3c0f5538f70d3509e250ae3e07133424d4d  shortcuts.vdf.yulon-bak-20260910-202555
cmp: identical
   entries: 2
   0 'Turtle WoW'        LastPlayTime= 1789071688
   1 'Turtle WoW Server' LastPlayTime= 1789071433
```

Same sha256 before and after, the entry count did not grow, and **Steam's own
`LastPlayTime` survived**. The last of those is a defect this live run found: the first
version of `upsert` swapped the whole entry out and put `0` back over both, so every
press erased "last played". A replace now merges over what it finds and holds back
`appid`, `LastPlayTime` and `tags` (`steam.KEPT_ON_REPLACE`), and keeps any field Steam
writes that this module has never heard of.

`compat-tool-mapping.txt` shows the same for `config.vdf`: one `CompatToolMapping`
section, one `"3811632958"` block inside it, still naming GE-Proton11-6-x86_64. That
file's sha256 *did* move — Steam added its own shader-cache and depot rows for the appid
at lines 32 and 784 while it was running. Ours is line 11 and is untouched.

## 6. With Steam running, the button refuses — `press-3-refused-steam-running.txt`

Steam was started in Big Picture and the press repeated:

```
steam.steam_running()  : True
pgrep -x steam         : rc=0
shortcuts.vdf before   : 1f6b4d7f48d8d2716403f290ebc2c3c0f5538f70d3509e250ae3e07133424d4d

REFUSED:
  Steam is running. Steam keeps the shortcut list in memory and rewrites shortcuts.vdf
  when it exits, so anything written now would be thrown away without a word. Quit Steam
  completely — check the tray — and press Add to Steam… again.

shortcuts.vdf after    : 1f6b4d7f48d8d2716403f290ebc2c3c0f5538f70d3509e250ae3e07133424d4d
```

Same sha256 after the refusal as before it: **nothing was written.**

## 7. The tests, and each one's named mutation — `mutations.txt`

Nineteen mutations, each applied to the shipped source, the one test that should catch it
run, then the source restored and the test run again. `__pycache__` is purged **on both
sides of every run** — a stale `.pyc` reports the unmutated module and every mutation
"survives". Every one was caught; the file lists them with the test and the reasoning.

One of them was not caught the first time and the test was strengthened rather than the
mutation dropped: *drop the root's own END* survived
`test_the_codec_round_trips_the_decks_own_entries_byte_for_byte`, because a round trip
through a symmetrically broken codec still round-trips, and `endswith(b"\x08\x08")` is
true of a truncated real file anyway — the last entry's empty `tags` map contributes two
ENDs of its own. It is now caught by
`test_the_empty_document_is_the_exact_bytes_steam_writes`, eleven bytes written out by
hand from the format.

### Round 2, 2026-09-10 — seven code fixes, and why no capture was re-taken

The cold review accepted this evidence and asked for seven code changes. **None of them
changes a byte that anything in this folder records**, so nothing here was re-run and
nothing here was re-taken:

* the two writes go through a `.yulon-tmp` sibling and a rename — same final bytes, and
  the presses above were not interrupted;
* `config.vdf` is now written with `surrogateescape` and read with `newline=""` — this
  box's config is UTF-8 and LF, so both are no-ops on it (`press-1-on-disk.txt`'s
  `IDENTICAL` diff is the proof it was already byte-exact here);
* an unknown VDF type byte becomes a refusal — this box's file has never had one;
* `add()` catches `OSError` rather than `PermissionError` — nothing here hit either;
* `compatibilitytools.d` is ordered by version — this box has exactly one tool, and it
  is the one every capture names, `GE-Proton11-6-x86_64`;
* one docstring cited a frame by a name it does not have; it now cites
  `1-logo-slot-hid-the-name.png`, which is in this folder.

The mutation count in §7 went from twelve to nineteen, and `mutations.txt` is the
re-run of all of them.

## 8. The gate — `checks-all-green.txt`

`YULON_TEST_BOX=m910q run-tests-vm.sh --checks` on the finished branch:
`=== --checks: ALL GREEN ===`.

## What could not be staged

* **A press through the GUI button.** Reasoned above: the button needs a remembered
  install with a tab, and building one here would stage the thing under test.
* **Clicking the client tile in Big Picture** (see §4). Launched by `rungameid` instead.
* **A hand-added non-Steam shortcut through Big Picture's own flow**, which the read half
  also could not stage — the same pointer problem. It was optional there and remains so:
  the authoritative capture is the Deck manifest's two entries.
* **An official Valve Proton.** This box has none, so `find_compat_tool()`'s
  `steamapps/common` branch and its `proton_9`/`proton_experimental` name table are
  covered by tests and **not** by a live launch. The tool that launched the client is the
  `compatibilitytools.d` branch, whose name is read off the tool's own manifest and is
  therefore the one this prefers.

## What this run changed on the box, beyond the two entries

Recorded because "everything else as found" is a promise:

| change | why |
|---|---|
| `~/bin/claude-say` written | this box had none, and every action here had to be announced |
| `GE-Proton11-6-x86_64` unpacked into `~/.steam/root/compatibilitytools.d/` | the box had **no** Proton at all, so the compat half would otherwise only be a refusal |
| `libxcb`, `xcb-util-{wm,image,keysyms,renderutil,cursor}`, `libxkbcommon-x11` installed | Qt could not start; without them the server entry opens nothing |
| `SteamLinuxRuntime_4` downloaded, `steamapps/compatdata/3811632958` created | Steam did this itself the first time the client entry ran |
| `~/clients/TurtleWoW/Logs`, `WTF` | the client wrote them when it ran |
| `config.vdf` has one `CompatToolMapping` block, and a `.yulon-bak-` beside it | the feature under test |

`~/y8` is as it was found: the `steam.py` copied in for the driver was removed and
`git status` shows the same four modified files as before T17.

**The box is left with Steam running in Big Picture, the owner's account signed in, and
the two entries present** — `6-box-as-left.png`.
