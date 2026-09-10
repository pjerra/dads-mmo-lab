# T29 Half 2 — under gamescope, coming out of the Steam UI puts Yu'lon back in front (`yulon-arch`, 2026-09-11, headless backend)

**What is proven here.** Under gamescope, launching the "Turtle WoW Server" entry from Steam's
gamepad UI makes **gamescope focus Yu'lon by itself**, and coming back out of the Steam UI —
including by activating the running-game card's **Resume game** — leaves Yu'lon focused again.
On the plain X11 desktop (`pyplan/gates/8.8-steam-resume-yulon-arch-2026-09-10/`) neither
happened: Big Picture stayed in front and its menu had no running-game card at all.

**What is NOT proven here, and the title says so.** Two things.

* **This ran on gamescope's *headless* backend**, because no visible backend can start on this
  GPU-less VM (§1). The compositor, the two XWayland servers, the focus stack and the atoms are
  the same code; what a headless run cannot show is that the *picture* on a Deck's screen follows
  the focus.
* **The readings cannot separate "Resume game was activated" from "the overlay was
  dismissed."** gamescope binds `Shift+Tab` itself, and §4's two controls show that *any*
  dismissal — a second `Shift+Tab`, or `Escape` — returns `GAMESCOPE_FOCUSED_APP` to the entry
  with Resume never touched, writing the same two lines to Steam's own log. So the Resume press
  in §5 is evidenced by the *highlight photographed on "Resume game" immediately before the
  Return* and by the card's own state, not by a signature in the atoms. See §6 for what that
  does and does not leave standing.

**Yu'lon was not modified for any of it.** The window gamescope focused is the plain Qt
top-level `Yu'lon — Dad's MMO Lab launcher 0.6.59`, id `6291463` on XWayland `:2`, carrying
`_NET_WM_PID = 18219` — the pid of `main.py` under `reaper SteamLaunch AppId=4102716320`
(`30-run2-readings-and-presses.txt`). Nothing was stamped on it, no OpenGL surface was added,
and the Steam overlay still never hooks it.

**This is the ordinary desktop Steam client run inside gamescope by hand — not SteamOS's own
build in a `gamescope-session`.** A Deck runs a Valve-patched session with a controller, a real
GPU and its own session manager; what is reproduced here is the mechanism (gamescope focusing a
launched app by PID, and the gamepad UI's running-game card), not the Deck's whole environment.

Two gamescope runs are in this folder. **Run 2 (`29-`, `30-`, `gs2-*`) is the one every claim
above rests on**; run 1 (`25-`, `26-`, `gs-*`) is kept because it is a real run and because the
three attempts at getting back to Steam are recorded there, but its Resume press has no recorded
command and its window readings cover `:1` only — which is why run 2 exists.

Every stamp was read from the box's own clock (UTC); every action was announced first with
`~/bin/claude-say`. **The Steam account is named only by its userdata id `18347166`**; no
process environment and no `loginusers.vdf` was captured.

## 1. gamescope will not composite here — `21-`, `22-`, `23-`, `24-`

Installed with the ticket's own line (exit 0, `21-pacman-gamescope.txt`):

```
printf 'yulon\n' | sudo -S -p '' pacman -Sy --noconfirm --needed gamescope vulkan-swrast
```

giving **gamescope 3.16.28**, **vulkan-swrast (lavapipe) 1:26.2.2**, `xorg-xwayland 24.1.13`
(`22-gamescope-installed.txt`). No display adapter on `lspci`, one DRM node (`/dev/dri/card1`,
held by `Xorg :0`), and of the two Vulkan ICDs on disk only lavapipe is real.

`23-gamescope-attempts.txt` is **seven** command lines, each with its exit status and the tail of
its own output. The ticket's line first:

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
device`) because there is no DRM-capable device to be master of. `--backend headless` is the only
one returning **exit 0**.

`24-vulkan-device.txt` checks that complaint against the driver rather than taking gamescope's
word: the only device the loader offers is `llvmpipe (LLVM 22.1.8, 256 bits)`,
`deviceType = PHYSICAL_DEVICE_TYPE_CPU`, and `vulkaninfo` mentions `VK_KHR_swapchain` three times
and **`VK_KHR_present_id` and `VK_KHR_present_wait` zero times each**. Not a gamescope bug and not
a missing flag: a CPU rasteriser without present-timing extensions cannot drive gamescope's
compositor. **A real Deck, or any box with a GPU, is not affected.**

## 2. The headless run, and what it costs — `29-`, `frame-01`

```
gamescope --backend headless -W 1920 -H 1080 -e --xwayland-count 2 -- steam -gamepadui
```

`29-gamescope-run-2-log.txt` is its own log: `Creating headless backend`, `Starting Xwayland on
:1`, `Starting Xwayland on :2`, and — the line §4 turns on —

```
29-:58  [gamescope] [Info]  binding: (GuideKeyboardHotkey) -> Adding new trigger [Tab + Shift_L].
29-:59  [gamescope] [Info]  binding: (QAMKeyboardHotkey) -> Adding new trigger [Tab + Control_L + Shift_L].
```

so **`Shift+Tab` is gamescope's own guide hotkey**, not something Steam receives as a keystroke.
Two XWayland servers is the Deck's own shape: Steam on `:1`, the launched app on `:2`.

**What headless costs:** nothing is drawn to `:0`.
`frame-01-vm-console-while-gamescope-ran.png` is the VM console taken while Steam's gamepad UI and
Yu'lon were both running inside gamescope — a bare XFCE desktop. So the VM-console screenshots
this project uses elsewhere cannot show the gamepad UI at all, and the `gs-*` / `gs2-*` images are
Qt grabs of the windows on gamescope's own XWayland displays (`grab.py`,
`QScreen.grabWindow(<wid>)`). Real pixels off a real X server; simply not photographs of a monitor,
because there is nothing on the monitor to photograph.

## 3. Launch — gamescope focuses Yu'lon by itself. `30-` §1-2, `gs2-01`, `gs2-02`

`30-run2-readings-and-presses.txt` is the run in order. Every press in it goes through `press.sh`,
which echoes the display, the key, a stamp and `xdotool`'s exit status; every reading goes through
`atoms.sh`, which reads the atoms on `:1` and lists the windows on **both** XWayland displays with
their `_NET_WM_PID`, the entry's own process line, and `free -m`.

Steam alone in gamescope (`gs2-01-gamepadui-server-entry-focused.png`, the Recent Games row with
"Turtle WoW Server" focused):

```
--- Steam alone in gamescope  (23:33:31)
    GAMESCOPE_FOCUSED_APP = 769                 GAMESCOPE_FOCUSED_WINDOW = 25165876
    GAMESCOPECTRL_BASELAYER_APPID = 413091, 769
  [windows on :1]  25165876  pid=17153  Steam Big Picture Mode   (and steam, steamwebhelper ×2, …)
  [windows on :2]  2097153   pid=<none> steamcompmgr             — nothing else yet
```

`Return` opens the entry page (`gs2-02-…-play.png`, the green **Play**), `Return` again is Play,
and thirty seconds later:

```
--- after Play: gamescope has focused the launched entry  (23:34:38)
    GAMESCOPE_FOCUSED_APP = 4102716320          GAMESCOPE_FOCUSED_APP_GFX = 4102716320
    GAMESCOPE_FOCUSED_WINDOW = 6291463
    GAMESCOPECTRL_BASELAYER_APPID = 413091, 4102716320, 769
    GAMESCOPE_KEYBOARD_FOCUS_DISPLAY = 12858, 0, 49
  [windows on :2]  6291463  pid=18219  Yu'lon — Dad's MMO Lab launcher 0.6.59
                   6291465  pid=<none> main.py
                   6291461  pid=<none> Qt Selection Owner for main.py
  [the entry's own process]
    18218  …/reaper SteamLaunch AppId=4102716320 -- /home/pk/y8v313/bin/python /home/pk/y8/pylauncher/main.py
    18219  /home/pk/y8v313/bin/python /home/pk/y8/pylauncher/main.py
  [memory]  Mem: 19990 total, 17283 available
```

`4102716320` is the entry's 32-bit appid; window `6291463` is on `:2` and its `_NET_WM_PID` is
`18219`, the pid Steam's own reaper launched. **This step involves no overlay and no dismissal**,
so it is the unambiguous half of the result: gamescope raised Yu'lon with no help from Yu'lon,
where the plain X11 desktop left Big Picture in front with a dead Resume button.

`gs-04-yulon-window-on-xwayland-2-black.png` (run 1) is an honest failure kept as one: a Qt grab of
window `6291463` comes back black, because gamescope redirects its clients. The atoms, the window
title, the `_NET_WM_PID` and the process line above are what say Yu'lon is up and focused.

## 4. The control: dismissal alone moves the same atoms — `30-` §3, `gs2-03`, `gs2-04`

Because `Shift+Tab` is gamescope's guide hotkey (§2), the round-1 evidence could not tell "Resume
was activated" from "the overlay went away". Run 2 takes the control twice, **with Resume never
touched**:

| | after opening the guide | after dismissing it |
| --- | --- | --- |
| **control A** — dismissed by a second `Shift+Tab` | `FOCUSED_APP = 769`, `..._GFX = 4102716320`, keyboard display `12602` | `FOCUSED_APP = 4102716320`, keyboard display `12858` |
| **control B** — dismissed by `Escape` | `FOCUSED_APP = 769`, keyboard display `12602` | `FOCUSED_APP = 4102716320`, keyboard display `12858` |

and control B's dismissal added **two** lines to Steam's `console-linux.txt`:
`Adding process 19072 for gameID 17621032419199025152` and one more like it. **That is exactly the
transition, and exactly the log delta, that the Resume press produces in §5.** So neither the
atoms nor Steam's own log is a signature of Resume.

`gs2-03-control-guide-open-running-game-card.png` is the guide open — **the running-game card**
with `Turtle WoW Server` in the rail and `Resume game`, Controller settings, View game details,
Guides, Notes, Game Recording, Exit game. 8.8's frame 7 recorded the same menu on plain X11 with
**no running-game card at all** ("which on a Deck is where Resume lives"). It exists here, and
that fact is independent of the ambiguity above.
`gs2-04-control-guide-dismissed-no-resume.png` is the same window after the dismissal.

## 5. The press — `30-` §4-5, `gs2-05` … `gs2-09`

**A misfire first, recorded rather than hidden.** The card remembers where the rail was left, so
one `Right` after re-opening the guide landed on **Exit game**
(`gs2-05-misfire-exit-game-highlighted.png`, grabbed between the `Right` and the `Return`). The
`Return` therefore activated Exit game — which did **not** exit anything: it raised an
`Exit game? / Unsaved game data may be lost. / Confirm / Cancel` modal
(`gs2-06-exit-game-confirmation.png`), which is why focus stayed on Steam and Yu'lon kept running.
It was cancelled on **Cancel** (`gs2-07-card-after-cancel.png`, the card back with Exit game
highlighted), and the rail was then walked up six times with the highlight photographed **before**
the Return:

`gs2-08-resume-game-highlighted-before-the-press.png` — **the highlight is on "Resume game"**.
Then, from `30-`:

```
--- immediately BEFORE Resume, guide open, Resume game highlighted  (23:39:06)
    GAMESCOPE_FOCUSED_APP = 769      ..._GFX = 4102716320   keyboard display = 12602
  $ DISPLAY=:1 xdotool key Return        # ACTIVATE Resume game
    stamp: 2026-09-10T23:39:06+00:00
    exit: 0
--- immediately AFTER Resume  (23:39:12)
    GAMESCOPE_FOCUSED_APP = 4102716320   ..._GFX = 4102716320   keyboard display = 12858
    lines console-linux.txt gained across the Resume activation: 2
      | [2026-09-10 23:39:09] Adding process 20796 for gameID 17621032419199025152
      | [2026-09-10 23:39:09] Adding process 20795 for gameID 17621032419199025152
--- fourteen seconds after Resume  (23:39:20)
    GAMESCOPE_FOCUSED_APP = 4102716320   keyboard display = 12858
```

Settled, not a flicker. `GAMESCOPECTRL_BASELAYER_APPID` is `413091, 4102716320, 769` throughout —
the stack never changed; what changed is which member gamescope focuses.
`gs2-09-steam-window-after-resume.png` is Steam's own window grabbed straight after: blank.
**Do not read that as "Steam stopped painting"** — `gs-04` shows the same grab method returning
black for a window gamescope has redirected, so the blankness is at least as likely to be the
grab as the client.

## 6. What §5 leaves standing, stated plainly

* The atoms after Resume are **identical** to the atoms after a bare dismissal (§4). Steam's log
  delta is identical too. **Nothing measured here distinguishes the two.**
* What the Resume press *is* evidenced by: `gs2-08`, the highlight photographed on "Resume game"
  immediately before a `Return` whose command, display, stamp and exit status are in `30-`; and
  the card's own state, which offers Resume as the way back.
* What is unaffected by the ambiguity: **§3.** The launch focuses Yu'lon with no overlay in the
  picture at all, and §4's card exists under gamescope where it did not on X11.
* So the Deck-shaped claim this half can make is: *under gamescope, the entry launches into
  focus, the STEAM button brings a running-game card, and leaving that card — by Resume or by any
  dismissal — returns to Yu'lon.* The narrower claim "the **Resume item specifically** is what
  raises it" is **not** separable with these instruments, and 8.8's stand-in should be read
  against the first sentence, not the second.
* Either way Yu'lon needs no change, and nothing 8.8 proposed and rejected (stamping
  `STEAM_GAME`, rendering through OpenGL, intercepting the click) was needed.

## 7. The box as left — `27-`, `28-`, `frame-02`

Steam was shut down from inside gamescope (`steam -shutdown`); gamescope exited with it and no
Yu'lon process survived. Steam was then started the ordinary way on `:0` with `-bigpicture`.

`28-half2-box-as-left.txt` at 23:42:44: **steam runs and `-bigpicture` is on its argv**, the window
in front on `:0` is `Steam Big Picture Mode`, **no gamescope process, no Yu'lon process, no client
process**, all three Tortoise containers `Exited (0)`, **`nothing listening on either port`**, and
Yu'lon's source tree `~/y8` at `679d2df` with `git status --short: clean, nothing modified or
untracked`. `frame-02-vm-console-box-as-left.png` shows it: ordinary Big Picture on XFCE, signed
in, both entries.

`27-shortcuts-as-left.txt` handles `shortcuts.vdf` — see deviation 1.

**Left installed on the box:** `gamescope 3.16.28-1`, `vulkan-swrast 1:26.2.2-1`,
`vulkan-tools 1.4.357.0-1` and the transaction's dependencies (`sdl3`, `sdl2-compat`, `seatd`,
`xcb-util-errors`, `xorg-xwayland`), the way 8.8 left `mesa-utils`. The run's scratch tree is
`~/t29`; the throwaway account's password file in it was shredded and the working copy of
`shortcuts.vdf` removed.

## Deviations, stated

1. **`shortcuts.vdf`'s sha256 moved, three times, against the ticket's "untouched (sha256 before
   and after)".** `e4068f33…` (as found) → `244a23ed…` (after the client entry ran) → `351bae71…`
   (after run 1) → `03adf146…` (as left). Steam writes `LastPlayTime` into the file whenever a
   shortcut is launched, and launching both entries is what the ticket asked for. Read with the
   app's own codec the file is **681 bytes at every reading** and holds exactly `Turtle WoW`
   (`"/home/pk/clients/TurtleWoW/WoW.exe"`) and `Turtle WoW Server`
   (`"/home/pk/y8v313/bin/python"` + `LaunchOptions: /home/pk/y8/pylauncher/main.py`); only
   `LastPlayTime` differs. **Honesty note, also in `27-`: the as-found file was only hashed, never
   parsed** — no copy of it was kept — so "the entries never changed" rests on the parses of the
   later readings, the constant 681 bytes, and T17's and 8.8's own listings, not on a
   parse-to-parse diff against the as-found bytes.
2. **The half ran on gamescope's headless backend** (§1-2), with §2 stating what that does and
   does not show.
3. **The `gs-*` and `gs2-*` frames are Qt window grabs off gamescope's XWayland displays, not
   VM-console screenshots**, for the reason `frame-01` shows. The two `frame-*` images are
   ordinary VM-console shots and bracket the run.
4. **`gs-04` is black** — Yu'lon's window inside gamescope cannot be grabbed that way. Kept as a
   recorded failure; the claim it would have supported rests on the atoms, the window title, the
   `_NET_WM_PID` and the process line.
5. **Steam was returned to with `Shift+Tab`, not a controller's STEAM button** — no controller is
   attached (`gs2-03` says `No controller connected.`). `Shift+Tab` is gamescope's own guide
   binding, quoted from its log at `29-:58`, which is also why §4's control was necessary.
6. **A press in run 2 landed on Exit game rather than Resume game** (§5). It raised a confirmation,
   which was cancelled; nothing exited, and the press was redone with the highlight verified first.
   Both are in `30-` and in the frames.
7. **The VM was hard-reset earlier in the ticket, before this half**, and restarted with 20 GB
   instead of 12 — the host's worker process was killed at about 22:41 on the box's clock. See
   Half 1's deviation 1 for the numbers. gamescope was installed and run only after that, with the
   server stopped and the client closed; `free -m` before (`20-`: 18994 MB available) and inside the
   run (`30-`: 17283 MB available with Steam and Yu'lon both up). Nothing here went near the ceiling.
8. **`vulkan-tools` was installed beyond the ticket's list**, to check gamescope's complaint against
   the driver instead of repeating it (`24-`).
9. **Run 1 is superseded but kept** (`25-`, `26-`, `gs-*`). Its Resume press has no recorded command
   and its readings cover `:1` only; `26-`'s head says so.

## Files

| file | what it is |
| --- | --- |
| `20-mem-before-gamescope.txt` | `free -m` immediately before gamescope was first started |
| `21-pacman-gamescope.txt` | the install transaction |
| `22-gamescope-installed.txt` | versions, ICDs, the machine, no GPU |
| `23-gamescope-attempts.txt` | seven command lines, each with exit status and output |
| `24-vulkan-device.txt` | `vulkaninfo`: llvmpipe, and the two extensions absent |
| `25-gamescope-run-log.txt` | run 1's gamescope log |
| `26-resume-under-gamescope.txt` | run 1's readings and the three attempts at returning to Steam |
| `29-gamescope-run-2-log.txt` | run 2's gamescope log, with the two hotkey bindings at `:58-59` |
| `30-run2-readings-and-presses.txt` | **run 2 in order: every press with its command, display, stamp and exit status; every reading with both displays' windows, their `_NET_WM_PID`, the reaper line and `free -m`** |
| `27-shortcuts-as-left.txt` | the four sha256 readings, the parse as left, and the honesty note |
| `28-half2-box-as-left.txt` | the box as left, including `~/y8`'s HEAD and clean status |
| `atoms.sh`, `press.sh`, `grab.py`, `x.sh`, `gsfacts.sh`, `gstry.sh`, `asleft2.sh`, `vdf2.sh` | the scripts that produced the above |

| image | run | what it shows |
| --- | --- | --- |
| `gs2-01-gamepadui-server-entry-focused.png` | 2 | the gamepad UI inside gamescope, server entry focused |
| `gs2-02-server-entry-page-play.png` | 2 | its page, green Play |
| `gs2-03-control-guide-open-running-game-card.png` | 2 | **the running-game card** — what plain X11 never had |
| `gs2-04-control-guide-dismissed-no-resume.png` | 2 | the same, dismissed, Resume never touched |
| `gs2-05-misfire-exit-game-highlighted.png` | 2 | the misfire: the highlight on Exit game |
| `gs2-06-exit-game-confirmation.png` | 2 | the modal it raised — nothing exited |
| `gs2-07-card-after-cancel.png` | 2 | cancelled, back on the card |
| `gs2-08-resume-game-highlighted-before-the-press.png` | 2 | **the highlight on "Resume game", immediately before the Return** |
| `gs2-09-steam-window-after-resume.png` | 2 | Steam's window after Resume: blank (see §5's caveat) |
| `gs-01` … `gs-07` | 1 | run 1's equivalents, kept; `gs-04` is the black Yu'lon grab |
| `frame-01-vm-console-while-gamescope-ran.png` | — | the real screen while all of it ran: a bare XFCE desktop |
| `frame-02-vm-console-box-as-left.png` | — | the box as left: ordinary Big Picture on XFCE, two entries |

## Checks

`tests/test_no_secrets_in_evidence.py` was run against this worktree before the commit.
