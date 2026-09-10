# 8.8 — "Resume" in Big Picture does not raise the running server entry (`yulon-arch`)

Written 2026-09-10 23:58 CEST on this laptop. The owner's report: *"when I clicked play on the
server and went back to steam and tried to click resume on the server inside steam nothing
happened, it should open the yulon app again when clicking resume."* Reproduced, and traced
to where Steam stops, not to Yu'lon.

Box: `yulon-arch` — Hyper-V, Arch Linux, XFCE on `:0` (1920x1080), **no gamescope**, Steam in
Big Picture ("gamepad UI") with the owner signed in. **The account is named only by its
userdata id `18347166`.** The two entries are the ones T17's live run left
(`8.8-steam-shortcuts-yulon-arch-2026-09-10/`): "Turtle WoW" (the client under Proton) and
"Turtle WoW Server" (the venv's python running `~/y8/pylauncher/main.py`, 0.6.59 at `803a686`).

**Every sentence below is backed by a file in this folder**; `run-notes.txt` is the run in
order with every measured value, `steam-own-logs.txt` is Steam's own two logs for the window,
and the frames are numbered in the order they were taken.

## 1. Reproduced — frames 1 and 2, `run-notes.txt` 21:36-21:38

The server entry launched from Steam opens Yu'lon in front (Steam's `reaper SteamLaunch
AppId=4102716320` line in `steam-own-logs.txt`). Back in Big Picture the entry's page shows a
blue **Resume** (frame 1). Pressing it — Return on the focused button, then a mouse click with
the pointer on it (frame 2) — leaves Big Picture in front. Steam writes **no line** to
`console-linux.txt` or `gameprocess_log.txt` for the click. Yu'lon keeps running the whole
time (Steam still tracks its PID; the app exited only when killed at the end).

## 2. What does raise it — frame 3

`steam steam://rungameid/17621032419199025152` issued while the entry runs brings the Yu'lon
window to the front (frame 3, `run-notes.txt` 21:37:07). So Steam *can* raise the window; the
Resume button's path simply does not go there. In Steam's UI bundle the launch button calls
`SteamClient.Apps.RunGame(gameid, "", -1, <launch source>)` and, when the gamepad UI runs inside
gamescope, a different "switch to the app" path instead — `run-notes.txt` names the grep. The
desktop `rungameid` URL and the gamepad-UI button therefore reach different native code.

## 3. Where Steam stops — `relaunch-overlay-check.txt`, frames 4-7

Steam's way of knowing a game's window on X11 is its overlay: `gameoverlayrenderer.so` is
`LD_PRELOAD`ed into the entry (it is mapped into Yu'lon's process, 4 mappings), and when the
overlay hooks the game's window it spawns `gameoverlayui`. **For Yu'lon it never does** — a Qt
6 raster window created through xcb gives the overlay nothing to hook: no `gameoverlayui`
process (`relaunch-overlay-check.txt`), no `STEAM_GAME` property on the window (the overlay
stamps one on windows it knows; Steam's own window carries `STEAM_GAME = 769`). Stamping the
property by hand changed nothing (21:39).

Three controls separate the causes:

- **A Qt window with an OpenGL surface** (`glprobe.py`, entry "yulon-glprobe-gl"): the overlay
  now hooks it — `gameoverlayui` starts — but Big Picture still shows the page with Resume
  (frame 4) and the click still does nothing (frame 5). So "make the overlay hook Yu'lon" is not
  a fix.
- **An Xlib/GLX program** (`/usr/bin/glxgears`, entry "yulon-glprobe-xlib") and **the Proton
  client**: Steam recognises the window and Big Picture steps aside into its spinner screen
  (frame 6) — and from there *nothing* in Big Picture raises the game either: the spinner,
  Return and the STEAM menu do nothing, and the menu has no running-game card (frame 7), which on
  a Deck is where "Resume" lives.

So on a plain X11 desktop Steam's gamepad UI hands window focus to nobody: on the Deck that job
is **gamescope's** (it focuses the launched app by PID), and this box has no gamescope. Yu'lon's
window is a normal top-level window with `_NET_WM_PID` set — which is exactly what gamescope
keys on — so the Deck path is not contradicted by anything here, but **it is not proven here
either**: no gamescope, no Deck. That is the one open item.

## 4. What Yu'lon could do — nothing that helps

- Setting `STEAM_GAME`: tried, no effect.
- Rendering through OpenGL so the overlay hooks: tried, no effect on Resume.
- Intercepting the click: Steam sends the app nothing (no log line, no IPC the overlay would
  carry), so there is nothing to intercept.

The environment Steam gives the entry (`SteamAppId`, `SteamGameId`, `SteamOverlayGameId`,
`SteamTenfoot=1`, `SteamGamepadUI=1`, `SteamClientLaunch=1`) tells Yu'lon it was launched
from the gamepad UI, which is enough for a future "launched from Steam" behaviour (for
instance staying on top, or opening full-screen the way the Deck expects) but not for a
Resume that Steam never delivers.

## 5. Box as left — `steam-own-logs.txt` (tail), `run-notes.txt` 21:54-21:56

The three probe entries were added and removed with Steam closed, using Yu'lon's own codec
(`yulon.steam.vdf_parse` / `new_entry` / `upsert` / `vdf_dump`); `shortcuts.vdf` was then
restored from the copy taken before the first probe and the two sha256 sums match; the probes'
`shadercache/<appid>` folders are gone; `mesa-utils` (for `glxgears`) stays installed;
`~/probe/` holds the scripts and the pre-probe copy. Steam is back in Big Picture, signed in,
with the two entries. No Yu'lon process runs. Nothing under `~/y8` or the owner's client
folder was touched.

## Servers on this box

Yu'lon's state on `yulon-arch` (`~/.local/share/yulon/`) holds only `yulon.log` — **no
`state.json`, so the catalog shows every tile as "Install / Use existing…"**. Two server trees
from earlier phases are still on disk with their (exited) containers: `~/tortoise-vm` (4.1 GB,
compose project `yulon-wow-tortoise-54fa64f0`, containers created 2026-09-08) and
`~/wow-server-playerbots` (2.3 GB, `yulon-wow-wotlk-056ed20d`, 2026-09-01). The Tortoise tree
matches the client entry Steam has, and "Use existing…" on the WoW Tortoise tile is the
adopt press T19 built — that is the way to get a runnable server behind the Steam entry on
this box without a rebuild.
