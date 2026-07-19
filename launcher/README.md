# DML Launcher

A free, open-source desktop app for running your own World of Warcraft
(Wrath of the Lich King) private server with ~2000 AI playerbots on Windows —
a full replacement for the closed-source "The Lab", built on
[Dad's MMO Lab](https://github.com/DadsMmoLab/dads-mmo-lab) and
[AzerothCore](https://www.azerothcore.org/) + mod-playerbots.

Start/stop your server, watch it live, manage characters, accounts, bots,
modules and backups — all from one window, no terminal needed.

## Getting it

```
git clone --branch feat/dml-launcher-windows https://github.com/pjerra/dads-mmo-lab.git
cd dads-mmo-lab\launcher
powershell -File ..\cli\dev-install.ps1
npm install
npm run tauri dev
```

Requirements: Windows 10/11 with WSL2 and the Dad's MMO Lab `dml-arch` distro
(set up by the [DML installer](https://github.com/DadsMmoLab/dads-mmo-lab)),
Node.js 18+, and Rust (for building the app shell).

## What it does

**Server**
- **Home** — live server card: world up/down/starting/restarting with real-time
  updates, players, uptime, latency, bots online / max. Start, Stop and
  Restart with streamed output. Restarts save every character first and apply
  changed settings.
- **Live status chip** — current server state always visible in the sidebar,
  on every page, updating every few seconds.
- **Library** — install complete game servers (WotLK Playerbots, Vanilla, TBC,
  MapleStory, RuneScape, Mu Online) through their interactive installers, with
  a real terminal for answering prompts; start/stop/remove per title.
- **Console** — live worldserver log that fills the window, with send-command
  support (GM console), command history, clear and download.
- **Tools** — LAN play (let other PCs on your network join), the Wrath Unbound
  multi-class addon (install/uninstall), environment doctor, and a one-click
  WSL shell.

**Characters**
- **Dashboard** — an in-game-style character sheet in tabs:
  - *Character*: equipment grid with wowhead tooltips and item icons, and a
    rotatable 3D model of your character wearing their gear, centered in the
    pane like the in-game window.
  - *Talents*: your class's three talent trees laid out exactly as in-game,
    learned talents lit with rank badges, generated from the server's own
    game data.
  - *Achievements*: the full achievement browser — categories, points,
    earned dates, icons and tooltips (1320 achievements).
- **Teleport** — send any character to any of ~2000 named locations, or exact
  coordinates.
- **GM Tools** — revive, heal, set level, set gold on any character; summon
  service NPCs (banker, auctioneer, repair bot, …); at-login flags (rename,
  customize).

**Items & Bots**
- **Item Database** — search the item database and mail items to characters.
- **Playerbots (My Party)** — build a bot party by class, gear them up, fix
  their talents, set their level; save and load full party presets;
  export/import presets as text.
- **Commands** — a cheat-sheet page with the in-game commands of every
  installed module.

**Config**
- **Accounts** — create accounts, set passwords and GM levels, delete
  accounts (with protection for the admin account).
- **Settings** — curated server settings (XP/gold rates, bot population,
  AHBot, message of the day) with safe ranges; applied on restart (motd is
  instant).
- **Modules** — 37 server modules across three families (C++ / Lua / SQL)
  with one-line descriptions and GitHub links: transmog, auction house bot,
  solocraft, autobalance, 1v1 arena, hardcore modes and more. Handles the
  worldserver rebuild, config activation, install-state repair, Docker disk
  cleanup and server source updates.
- **Module Configs** — full-window text editor for module conf files with
  automatic backups.
- **Backups** — one-click snapshots of all characters/accounts/bots (works
  while running, always saves characters first); restore rolls everything
  back with an automatic safety backup.

**Quality of life throughout**: every terminal keeps its transcript when you
switch pages (streams keep running), Clear/Download buttons everywhere,
save dialogs are native, item/spell/achievement data is cached for offline
use after first view, and untested features ship locked until they pass a
live smoke test (`docs/SMOKE-TESTS.md`).

## Architecture

The GUI is a thin Tauri 2 + Svelte 5 shell: every feature calls the `dml` CLI
inside the `dml-arch` WSL distro as
`wsl.exe -d dml-arch -u dml -- dml <cmd> --json` and renders the JSON
envelopes / NDJSON event streams documented in `../cli/README.md`. No server
logic lives in the GUI.

## Dev loop

    powershell -File ..\cli\dev-install.ps1   # install/refresh the dml CLI in WSL
    npm install
    npm run tauri dev        # run the app
    npm test                 # vitest
    npm run check            # svelte-check
    cd src-tauri; cargo test # runner + envelope + command tests

CLI tests run in the distro: `wsl -d dml-arch -u dml -- bash -lc "cd /mnt/.../cli && bash build.sh && bats tests/"`

## Release build

    npm run tauri build      # NSIS installer under src-tauri/target/release/bundle/

Builds are currently unsigned (SmartScreen warning expected).

## Layout

    src/lib/api.ts             typed invoke wrappers (Channel-based streaming)
    src/lib/terminal-state.ts  pure NDJSON→terminal-state reducer (vitest)
    src/lib/Terminal.svelte    embedded terminal (sections, runtime, jump-to-latest)
    src/lib/pages/             one Svelte component per sidebar page
    src/routes/+page.svelte    sidebar + status chip + page shell
    src-tauri/src/dml/         envelope parsing + WSL process runner (cargo tests)
    src-tauri/src/lib.rs       tauri commands (validated IPC surface)

## License

AGPL — same as the Dad's MMO Lab project this builds on. The 3D model viewer
adapter ports invocation logic from
[wow-model-viewer](https://github.com/Miorey/wow-model-viewer) (ISC).
