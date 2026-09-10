# T29 — the Deck path for Steam's Resume (gamescope on `yulon-arch`) and a server behind the entries

Filed by the lead 2026-09-11 00:20 CEST from the owner's two answers: "Try gamescope on the arch VM" and "Yes, adopt it now" (`~/tortoise-vm`). Live-box work, no product code expected. Hand: Opus. Box: `yulon-arch` only.

## Why

`pyplan/gates/8.8-steam-resume-yulon-arch-2026-09-10/` shows Big Picture's **Resume** raises nothing on a plain X11 desktop and that the switch is gamescope's job on a Deck. That leaves the 8.8 stand-in with one unproven claim: that under gamescope (the Deck's Gaming Mode) Resume brings Yu'lon back. And the box has no server Yu'lon knows, so the Steam entries have nothing to play against.

## Definition of done

Two halves, **in this order** (both drive display `:0`; never at once).

### Half 1 — adopt `~/tortoise-vm`

- Yu'lon (from `~/y8`, venv `~/y8v313`, on `:0`) adopts the Tortoise tree at `/home/pk/tortoise-vm` through the **WoW Tortoise** tile's "Use existing…" and the adopt press T19 built (`catalog/native.py: stage_adopt`, `ADOPT_BUTTON_LABEL`). Read `pyplan/tickets/T19-*.md` for what the press does and refuses, and the m910q evidence `pyplan/gates/tortoise-adopt-and-updates-m910q-2026-09-10/` for a run that worked. If a headless path exists (`yulon.install_wiring`), it is acceptable; otherwise drive the GUI with `xdotool` on `:0` and prove each step with a frame.
- After the press: `state.json` names the install; the server starts from Yu'lon's Server tab; `tortoise-db`, `tortoise-realmd`, `tortoise-mangosd` up; the realmd/world ports listening; a frame of the running tab.
- The client entry in Steam ("Turtle WoW", `~/clients/TurtleWoW`) reaches the login screen against it: set the client's realmlist to `127.0.0.1` the way Yu'lon does it for this family (find it in the code; do not hand-edit if Yu'lon has a press for it), launch the entry from Big Picture, frame of the login screen. **Do not create accounts with real names; do not use the owner's account or characters.** A throwaway account made by the server's own command is fine and its name goes in the README.

### Half 2 — Resume under gamescope

- Install `gamescope` (and `vulkan-swrast` / `mesa`'s lavapipe if gamescope needs a Vulkan device; the box has no GPU). `printf 'yulon\n' | sudo -S -p '' pacman -S --noconfirm --needed …`.
- Close Steam (`steam -shutdown`, wait for no `steam` process). Start Steam **inside** gamescope on `:0`, the Deck way: `gamescope -W 1920 -H 1080 -e -- steam -gamepadui` (try `-steam`/`--steam` and `--xwayland-count` variants if the first refuses; record the exact line that worked or the exact error that stopped it). If gamescope cannot run on this VM at all, the half's result is that fact with the error text, and the ticket still closes.
- Launch "Turtle WoW Server" from the gamepad UI; Yu'lon opens; return to Steam (in gamescope the Steam button is the guide button — try the keyboard chord gamescope maps, else `steam steam://open/bigpicture`, else record what was tried); press **Resume**; frame before, frame after; `xdotool getactivewindow` is not meaningful inside gamescope — use frames and gamescope's own `GAMESCOPE_FOCUSED_APP`/`GAMESCOPE_FOCUSED_WINDOW` root atoms on the gamescope X display, or its log.
- Leave the box the way it is now: Steam back in ordinary Big Picture on XFCE (`steam -bigpicture`), signed in, the two entries present, `shortcuts.vdf` untouched (sha256 before and after), no Yu'lon process, no server running unless Half 1 leaves it stopped cleanly.

## Rules (the same as every live half)

- Evidence in `pyplan/gates/8.8-steam-gamescope-yulon-arch-2026-09-11/` and `pyplan/gates/tortoise-adopt-yulon-arch-2026-09-11/`: numbered frames, every README sentence backed by a file, stamps from the box's clock (`date -Is` at the head of every capture). Frames: `ssh vmhost 'C:\Users\PK\vmshot.ps1 -VMName yulon-arch'` then `scp vmhost:C:/Users/PK/vmshot-yulon-arch.png <target>`.
- The Steam account is named **only** by its userdata id `18347166`; never write the account name (it appears in the process environment Steam gives a launched entry and in `loginusers.vdf` — do not capture either). Passwords via environment, never printed. `tests/test_no_secrets_in_evidence.py` green before committing.
- `yulon-arch`: `DISPLAY=:0 XAUTHORITY=/home/pk/.Xauthority`; sudo wants the password `yulon` (`printf 'yulon\n' | sudo -S -p '' …`); announce first with `~/bin/claude-say "…"`. `pkill -f` patterns must not match your own shell (`pkill -f "[g]lxgears"`).
- Never touch `~/clients/TurtleWoW` beyond the realmlist, never `~/y8`'s files, never the owner's Steam entries. No pushes. Commit on `hand-t29` in the worktree; the lead merges.
- Report on the final message: sha, both folders' file lists, what each half proved in one paragraph each, deviations, box as left, Status.
