# T29 Half 2 — Resume under gamescope: it works, and Yu'lon needs no change (`yulon-arch`, 2026-09-11)

**8.8's one open item is closed.** The 8.8 gate
(`pyplan/gates/8.8-steam-resume-yulon-arch-2026-09-10/`) proved that on a plain X11 desktop
Big Picture's **Resume** raises nothing, traced it to Steam rather than to Yu'lon, and left one
claim unproven: that under gamescope — the Deck's Gaming Mode — Resume brings Yu'lon back.
It does.

```
--- immediately BEFORE Resume  (2026-09-10T23:07:34+00:00)
    GAMESCOPE_FOCUSED_APP(CARDINAL) = 769                  <- Steam
    GAMESCOPE_KEYBOARD_FOCUS_DISPLAY(CARDINAL) = 12602, 0, 47
--- immediately AFTER Resume  (2026-09-10T23:07:41+00:00)
    GAMESCOPE_FOCUSED_APP(CARDINAL) = 4102716320           <- the "Turtle WoW Server" entry
    GAMESCOPE_KEYBOARD_FOCUS_DISPLAY(CARDINAL) = 12858, 0, 49
```

**Yu'lon was not modified for this.** The window gamescope focused, `6291463`, is the plain Qt
top-level `Yu'lon — Dad's MMO Lab launcher 0.6.59` with `_NET_WM_PID` set — the same window the
8.8 run photographed Steam ignoring on X11. Nothing was stamped on it, no OpenGL surface was
added, and the Steam overlay still never hooks it. gamescope focuses by PID, and that is
enough.

**gamescope cannot open a *visible* backend on this GPU-less VM.** Section 1 has the exact
error and every line tried. The run above used its **headless** backend, which starts here;
section 2 says what that costs and what it does not.

Every stamp was read from the box's own clock (UTC); every action was announced first with
`~/bin/claude-say`. **The Steam account is named only by its userdata id `18347166`**; no
process environment and no `loginusers.vdf` was captured.

## 1. gamescope will not composite here — `21-`, `22-`, `23-`, `24-`

Installed with the ticket's own line (exit 0, `21-pacman-gamescope.txt`):

```
printf 'yulon\n' | sudo -S -p '' pacman -Sy --noconfirm --needed gamescope vulkan-swrast
```

giving **gamescope 3.16.28**, **vulkan-swrast (lavapipe) 1:26.2.2**, `xorg-xwayland 24.1.13`
(`22-gamescope-installed.txt`). The box has no display adapter on `lspci`, one DRM node
(`/dev/dri/card1`, held by `Xorg :0`), and two Vulkan ICDs on disk of which only lavapipe is
real.

`23-gamescope-attempts.txt` is **seven** command lines, each with its exit status and the tail
of its own output. The ticket's line first:

```
$ gamescope -W 1920 -H 1080 -e -- sleep 10
exit: 1
    [gamescope] [Info]  vulkan: selecting physical device 'llvmpipe (LLVM 22.1.8, 256 bits)' …
    [gamescope] [Warn]  vulkan: physical device doesn't support VK_EXT_physical_device_drm
    [gamescope] [Error] vulkan: Missing required extension: VK_KHR_present_id
    [gamescope] [Error] vulkan: Missing required extension: VK_KHR_present_wait
    Failed to create backend.
```

The same two lines end `--backend sdl`, `--backend wayland`, `--backend sdl --xwayland-count 2`
and `-b` without `-e`. `--backend drm` fails one step earlier (`vulkan: not a valid physical
device`, `Failed to initialize Vulkan`) because there is no DRM-capable device to be master of.
`--backend headless` is the only one that returns **exit 0**.

`24-vulkan-device.txt` checks gamescope's complaint against the driver rather than taking its
word. The only device the loader offers is
`llvmpipe (LLVM 22.1.8, 256 bits)`, `deviceType = PHYSICAL_DEVICE_TYPE_CPU`, and `vulkaninfo`
mentions `VK_KHR_swapchain` three times and **`VK_KHR_present_id` and `VK_KHR_present_wait`
zero times each**. So this is not a gamescope bug or a missing flag: a CPU rasteriser with no
present-timing extensions cannot drive gamescope's compositor, and no `--backend`, ICD or
option on this VM changes that. **A real Deck, or any box with a GPU, is not affected by
this.**

## 2. The headless backend, and what it does and does not cost — `25-`, `frame-01`

The run that produced the result at the top:

```
gamescope --backend headless -W 1920 -H 1080 -e --xwayland-count 2 -- steam -gamepadui
```

`25-gamescope-run-log.txt` is its own log: `Creating headless backend`, `Running compositor on
wayland display 'gamescope-0'`, `Starting Xwayland on :1`, `Starting Xwayland on :2`,
`Updating mode for xwayland server #0: 1920x1080@60`. Two XWayland servers is the Deck's own
shape — Steam on `:1`, the launched app on `:2`.

**What headless costs:** nothing is drawn to `:0`.
`frame-01-vm-console-while-gamescope-ran.png` is the VM console taken while Steam's gamepad UI
and Yu'lon were both running inside gamescope — a bare XFCE desktop. So the VM-console
screenshots the other frames in this project use cannot show the gamepad UI at all, and the
`gs-*` images here are Qt grabs of the windows on gamescope's own XWayland displays
(`grab.py`, `QScreen.grabWindow(<wid>)`) instead. They are real pixels off a real X server; they
are simply not photographs of the monitor, because there is nothing on the monitor to
photograph.

**What headless does not cost:** the compositor, the window management, the two XWayland
servers, the focus stack and the atoms are the same code paths. `GAMESCOPE_FOCUSED_APP`,
`GAMESCOPE_FOCUSED_WINDOW` and `GAMESCOPECTRL_BASELAYER_APPID` are set and updated exactly as
they would be with a display attached — which is why the question 8.8 left open can be answered
here at all. What is *not* proven by a headless run is that the picture on a Deck's screen
follows the focus; only that gamescope's focus does.

## 3. Steam's gamepad UI inside gamescope — `gs-01`, `gs-02`, `gs-03`

`gs-01-gamepadui-inside-gamescope.png` is Steam's gamepad UI at 1920x1080 on gamescope's
XWayland `:1`, signed in, with both entries on the Recent Games row. `26-resume-under-gamescope.txt`
records the atoms at that moment: `GAMESCOPE_FOCUSED_APP = 769` (Steam itself),
`GAMESCOPE_FOCUSED_WINDOW = 25165876` (its Big Picture window), `GAMESCOPECTRL_BASELAYER_APPID =
413091, 769`.

Keyboard input reaches it: `gs-02-server-entry-focused.png` is **"Turtle WoW Server"** focused
after one `Right`, and `gs-03-server-entry-page-play.png` is its page with the green **Play**.

## 4. Launching the entry — gamescope focuses Yu'lon by itself

`Return` on Play, and 25 seconds later:

```
--- after Play on Turtle WoW Server, inside gamescope  (23:04:04)
    GAMESCOPE_FOCUSED_APP(CARDINAL) = 4102716320
    GAMESCOPE_FOCUSED_APP_GFX(CARDINAL) = 4102716320
    GAMESCOPE_FOCUSED_WINDOW(CARDINAL) = 6291463
    GAMESCOPECTRL_BASELAYER_APPID(CARDINAL) = 413091, 4102716320, 769
```

`4102716320` is the "Turtle WoW Server" entry's 32-bit appid — the same one the 8.8 run saw
Steam track. Window `6291463` is on the **second** XWayland (`:2`), which is why it is absent
from `:1`'s window list; on `:2` the windows are `steamcompmgr`, `Qt Selection Owner for
main.py`, `main.py` and `Yu'lon — Dad's MMO Lab launcher 0.6.59`, and the process is
`reaper SteamLaunch AppId=4102716320 -- /home/pk/y8v313/bin/python /home/pk/y8/pylauncher/main.py`.

**gamescope raised Yu'lon with no help from Yu'lon.** On the plain X11 desktop the same launch
left Steam's Big Picture in front with a page and a dead Resume button.

`gs-04-yulon-window-on-xwayland-2-black.png` is an honest failure and is kept as one: a Qt grab
of window `6291463` comes back black, because gamescope redirects its clients and the window's
own contents are not readable that way. The atoms, the window title on `:2` and the process
line are what say Yu'lon is up and focused; that image says nothing and is included so the
attempt is on the record.

## 5. Getting back to Steam — three attempts, `26-`

The ticket asked for the chord gamescope maps, then `steam://open/bigpicture`, then whatever was
tried. All three are in `26-resume-under-gamescope.txt` with their atoms:

1. **`Super`, on `:2` and then on `:1`** — no change; `GAMESCOPE_FOCUSED_APP` stays
   `4102716320`. gamescope 3.16.28's own `Super` chords are display filters and screenshots
   (`Super+F/N/U/Y/S/G`); none of them is a switch-to-Steam.
2. **`steam steam://open/bigpicture`** — no change; still `4102716320`. Unlike the desktop, where
   8.8 showed `steam://rungameid/…` raising the window, this URL does not move gamescope's focus.
3. **`Shift+Tab`** — **this is the one.** Steam's own UI bundle names it: the localisation key
   `GuidedTour_BPM_SteamButton_Description_Keyboard` reads `"Shift + Tab"`, i.e. it is Steam's
   published keyboard equivalent of the Deck's STEAM button. Sent on `:2`, the display Yu'lon
   held:

```
--- after Shift+Tab on :2  (23:06:41)
    GAMESCOPE_FOCUSED_APP(CARDINAL) = 769          <- Steam is in front
    GAMESCOPE_FOCUSED_APP_GFX(CARDINAL) = 4102716320  <- Yu'lon is still what is being rendered under it
    GAMESCOPE_KEYBOARD_FOCUS_DISPLAY(CARDINAL) = 12602, 0, 47
```

`gs-05-steam-menu-running-game-card.png` is what that produced, and it is the frame 8.8 could
not get: **the STEAM menu with a running-game card** — `Turtle WoW Server` at the top of the
rail, and under it **Resume game**, Controller settings, View game details, Guides, Notes, Game
Recording, Exit game. 8.8's frame 7 recorded the same menu on plain X11 with **no running-game
card at all** — "which on a Deck is where Resume lives". It lives there here.

## 6. Resume — `gs-06`, `gs-07`, `26-`

`gs-06-resume-game-focused.png` is **Resume game** highlighted after one `Right`. `Return`, and
the two readings quoted at the top of this file: `GAMESCOPE_FOCUSED_APP` **769 → 4102716320**
and `GAMESCOPE_KEYBOARD_FOCUS_DISPLAY` **12602 → 12858**, i.e. keyboard focus moved from Steam's
window on `:1` back to Yu'lon's on `:2`. A third reading six seconds later is identical, so it is
a settled state and not a flicker. `GAMESCOPECTRL_BASELAYER_APPID` is `413091, 4102716320, 769`
throughout — the stack never changed; what changed is which member of it gamescope focuses.

`gs-07-steam-after-resume-blank.png` is Steam's own window grabbed straight after: blank. Steam
stops painting the gamepad UI once gamescope has taken focus away from it, which is the same
"Steam steps aside" the 8.8 run saw for `glxgears` and the Proton client — except that here the
step-aside is *toward Yu'lon* rather than toward nothing.

**So the 8.8 stand-in's last claim holds.** On a Deck, the entry launches, the STEAM button
brings the menu, and Resume returns to Yu'lon. Nothing in Yu'lon has to change for it, and
nothing 8.8 proposed and rejected (stamping `STEAM_GAME`, rendering through OpenGL,
intercepting the click) was needed.

## 7. The box as left — `27-`, `28-`, `frame-02`

Steam was shut down from inside gamescope (`steam -shutdown`), gamescope exited with it, and no
Yu'lon process survived. Steam was then started again the ordinary way on `:0`:

```
DISPLAY=:0 XAUTHORITY=/home/pk/.Xauthority setsid nohup steam -bigpicture …
```

`28-half2-box-as-left.txt` at 23:10:22: **steam runs and `-bigpicture` is on its argv**, the
window in front on `:0` is `Steam Big Picture Mode`, **no gamescope process, no Yu'lon process,
no client process**, all three Tortoise containers `Exited (0)` and **nothing listening on 3724
or 8090** (Half 1 stopped the server cleanly through Yu'lon's own Stop). `frame-02-vm-console-box-as-left.png`
is the VM console showing exactly that: ordinary Big Picture on XFCE, signed in, both entries.

`27-shortcuts-as-left.txt` handles `shortcuts.vdf`. Its sha256 moved twice —
`e4068f33…` (as found) → `244a23ed…` (after the client entry ran) → `351bae71…` (after the server
entry ran in gamescope) — and **the entries never changed**. Read with the app's own codec
(`yulon.steam.vdf_parse`), the file is 681 bytes at every reading and holds exactly `Turtle WoW`
(`"/home/pk/clients/TurtleWoW/WoW.exe"`) and `Turtle WoW Server` (`"/home/pk/y8v313/bin/python"`
with `LaunchOptions: /home/pk/y8/pylauncher/main.py`); the only field that differs is
`LastPlayTime`, which Steam writes itself whenever a shortcut is launched — and launching both
entries is what this ticket asked for.

**Left installed on the box:** `gamescope 3.16.28-1`, `vulkan-swrast 1:26.2.2-1`,
`vulkan-tools 1.4.357.0-1` (and the transaction's own dependencies: `sdl3`, `sdl2-compat`,
`seatd`, `xcb-util-errors`, `xorg-xwayland`), the way 8.8 left `mesa-utils`. The run's scratch
tree is `~/t29`; the throwaway account's password file in it was shredded and the working copy
of `shortcuts.vdf` removed.

## Deviations, stated

1. **The half ran on gamescope's headless backend**, because no visible backend can start on
   this VM (section 1, with the error text and seven attempted lines). The ticket's fallback was
   "record the exact error and still close the half"; this goes one better by answering the
   actual question through the atoms, and section 2 states plainly what a headless run does and
   does not prove.
2. **The `gs-*` frames are Qt window grabs off gamescope's XWayland displays, not VM-console
   screenshots**, for the reason `frame-01` shows: with a headless backend the monitor is blank.
   The two `frame-*` images here are ordinary VM-console shots and bracket the run.
3. **`gs-04` is black.** Yu'lon's window inside gamescope cannot be grabbed that way. Kept as a
   recorded failure; the claim it would have supported rests on the atoms, the window title on
   `:2` and the process line instead.
4. **Steam was returned to with `Shift+Tab`, not a controller's STEAM button** — no controller
   is attached (`gs-05` says `No controller connected.`). `Shift+Tab` is Steam's own published
   keyboard equivalent, quoted from its UI bundle.
5. **The VM was hard-reset earlier in the ticket, before this half**, and restarted with 20 GB
   instead of 12 — see Half 1's deviation 1. gamescope was installed and run only after that,
   with the server stopped and the client closed, and `free -m` taken before
   (`20-mem-before-gamescope.txt`: 18994 MB available) and during the run (17172 MB available
   with Steam and Yu'lon both up inside gamescope). Nothing here went near the memory ceiling.
6. `vulkan-tools` was installed beyond the ticket's list, to check gamescope's complaint against
   the driver instead of repeating it (`24-vulkan-device.txt`).

## Files

`20-mem-before-gamescope.txt` · `21-pacman-gamescope.txt` · `22-gamescope-installed.txt` ·
`23-gamescope-attempts.txt` (seven command lines, each with its exit status and output) ·
`24-vulkan-device.txt` · `25-gamescope-run-log.txt` (gamescope's own log for the run) ·
`26-resume-under-gamescope.txt` (**the run in order: every atom reading with its stamp**) ·
`27-shortcuts-as-left.txt` · `28-half2-box-as-left.txt` · the scripts (`gsfacts.sh`, `gstry.sh`,
`atoms.sh`, `grab.py`, `asleft2.sh`) · and nine images:

| image | what it shows |
| --- | --- |
| `gs-01-gamepadui-inside-gamescope.png` | Steam's gamepad UI at 1920x1080 inside gamescope, both entries |
| `gs-02-server-entry-focused.png` | "Turtle WoW Server" focused — keyboard input reaches gamescope |
| `gs-03-server-entry-page-play.png` | its page, green Play |
| `gs-04-yulon-window-on-xwayland-2-black.png` | the failed grab of Yu'lon's window (kept as a recorded failure) |
| `gs-05-steam-menu-running-game-card.png` | **the STEAM menu WITH a running-game card and Resume game** — what 8.8's frame 7 lacked |
| `gs-06-resume-game-focused.png` | "Resume game" highlighted, the moment before the press |
| `gs-07-steam-after-resume-blank.png` | Steam's window straight after Resume: blank, focus given away |
| `frame-01-vm-console-while-gamescope-ran.png` | the real screen while all of the above ran: a bare XFCE desktop |
| `frame-02-vm-console-box-as-left.png` | the box as left — ordinary Big Picture on XFCE, signed in, two entries |

## Checks

`tests/test_no_secrets_in_evidence.py` was run against this worktree before the commit.
