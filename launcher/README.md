# DML Launcher

A free, open-source desktop app for running your own World of Warcraft
(Wrath of the Lich King) private server with ~2000 AI playerbots on Windows —
a full replacement for the closed-source "The Lab", built on
[Dad's MMO Lab](https://github.com/DadsMmoLab/dads-mmo-lab) and
[AzerothCore](https://www.azerothcore.org/) + mod-playerbots.

Start/stop your server, watch it live, manage characters, accounts, bots,
modules and backups — all from one window, no terminal needed.

## Getting it

**As a user — no repo, no Rust, no Node.**

1. Run `Install-DML.ps1` once from an elevated PowerShell. It enables WSL2 and
   creates the `dml-arch` distro; that step needs admin, so it stays a script.
2. Install the launcher (`DML Launcher_x.y.z_x64-setup.exe`). It is unsigned,
   so SmartScreen will warn — *More info* then *Run anyway*.
3. Open it. The first-run screen walks the rest: it ships the `dml` CLI, the
   lua bridges and the title installers inside it, and provisions them into the
   distro from a **Set up backend** button. There is no `dev-install.ps1` step.

Requirements: Windows 10/11 (WSL2 capable). In a VM, nested virtualization must
be enabled on the host or WSL2 cannot start.

**As a developer — from source.**

```
git clone --branch release/dml-launcher https://github.com/pjerra/dads-mmo-lab.git
cd dads-mmo-lab\launcher
npm install
npm run tauri dev
```

`cli\dev-install.ps1` still exists for the dev loop, but it hardcodes a repo
path and is no longer any user's route. Requirements for building: Node.js 18+
and Rust.

## What it does

The sidebar is grouped into five collapsible sections — **Server**,
**Characters**, **Items & Bots**, **Config**, **Help** — each a
dropdown/accordion; the group holding your current page opens automatically.
Tabs remain in exactly one place: the character view (Character / Talents /
Achievements).

**Server**
- **Home** — live server card: world up/down/starting/restarting with real-time
  updates, players, uptime, latency, bots online / max. Start, Stop and
  Restart with streamed output. Restart applies changed settings and, by
  default, saves every character first; an optional toggle skips that extra
  save for a faster restart (the graceful shutdown still saves normally).
- **Live status chip** — current server state always visible in the sidebar,
  on every page, updating every few seconds.
- **Library** — install complete game servers (WotLK Playerbots, Vanilla, TBC,
  MapleStory, RuneScape, Mu Online) through their interactive installers, with
  a real terminal for answering prompts, or from a trusted repo URL;
  start/stop/remove per title, optionally also deleting a removed title's
  downloaded server images to free disk space.
- **Console** — live worldserver log that fills the window, with send-command
  support (GM console), command history with Up/Down recall and autocomplete
  (GM catalog + your favorites), ERROR/WARN log coloring, clear and download.
- **Tools** — LAN play (let other PCs on your network join), "Play over the
  internet" (guided router port-forward + optional DuckDNS), "Play Together"
  over Tailscale (friends join without any port-forwarding), the Wrath
  Unbound multi-class addon (install/uninstall), a realmlist status-and-fix
  card, a LAN/database diagnostic with a generated script to expose MySQL to
  HeidiSQL on your LAN, auto-shutdown when WoW closes, keep-PC-awake while
  the server's online, disk & performance tools (WSL memory/CPU limits,
  restart WSL, a disk-shrink script, a Windows Defender exclusion hint),
  cache maintenance, an environment doctor, and a one-click WSL shell.
- **Accounts** — create accounts, set passwords and GM levels, delete
  accounts (with protection for the admin account).
- **Modules** — a tabbed page: **Modules** (38 server modules across three
  families — C++ / Lua / SQL — with one-line descriptions and GitHub links:
  transmog, auction house bot (or the Auction House Bot Plus fork,
  auto-detected), solocraft, autobalance, 1v1 arena, hardcore modes and more;
  each installed C++ module shows its current version (commit sha + date);
  handles the worldserver rebuild, config activation, install-state repair,
  Docker disk cleanup, and AzerothCore + playerbots server source updates;
  modules that add a service NPC (e.g. NPC Beastmaster, Black Market AH) get
  a one-click "place NPC in capitals" button, and some modules show an
  inline advisory, e.g. Paragon's unguarded `.test` command), **Tuning**
  (guided per-module settings, formerly a Config page tab), and **Config
  files** (full-window conf editor with automatic backups, formerly a Config
  page tab). A "Check for updates" button fetches every installed module's
  origin and shows how many commits behind it is; once behind, a locked
  per-module Update button pulls that module's latest source (stash-safe)
  and marks it rebuild-required until the next rebuild compiles it in —
  except mod-arac, which is data-only and needs a client patch + restart
  instead of a rebuild. mod-playerbots always updates together with the
  server core via the same tab's Server update card, never on its own.

**Characters**
- **Character** — an in-game-style character sheet in tabs, auto-loading
  when you pick a character from the sidebar's "playing as" switcher
  (**Reload gear** re-fetches on demand):
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
- **GM Tools** — revive, heal, set level, set gold, or send a character home
  (unstuck — works offline too) on any character; summon service NPCs
  (banker, auctioneer, repair bot, …); at-login flags (rename, customize).

**Items & Bots**
- **Item Database** — search the item database and mail items to characters;
  save a character's equipped gear as a reusable "gear set", mail whole sets
  to other characters, and export/import sets as shareable TOML text.
- **Playerbots (My Party)** — build a bot party by role/class/spec (spec list
  pulled live from the server), gear them up, fix their talents, set their
  level, see who's online at a glance; save and load full party presets;
  export/import presets as text.
- **Browse Bots** — search all ~2500 world bots by name/class/level/online
  status, star favorites, inspect a bot's gear/talents/achievement count, and
  invite one to your party or set its level.
- **Commands** — a cheat-sheet page with the always-available core GM
  commands plus the in-game commands of every installed module.

**Config**
- **Settings** — curated server settings (XP/gold rates, bot population,
  AHBot, message of the day), each showing its default and safe range with a
  per-row Reset; most apply on restart (motd is instant, and some rate
  changes can apply live).
- **Bot World** — the same curated bot-population controls plus a searchable
  browser of every `playerbots.conf` key, and a guarded "flush & rebuild
  bots" danger-zone action.
- **Auction House** — repair/configure the Auction House Bot, or the Auction
  House Bot Plus fork (auto-detected), so the in-game auction house fills
  with bot listings.
- **Account-wide** — once the Accountwide Systems module is installed, turn
  on account-wide sharing per system: achievements, currency, gold, mounts,
  pets, playtime, professions, PvP rank, flight paths, titles.
  (Module tuning and Module files moved into the Modules page as its Tuning
  and Config files tabs — see the Server section above.)
- **Backups** — one-click snapshots of all characters/accounts/bots (works
  while running, always saves characters first), each with a short content
  summary and a Verify button that checks archive integrity without
  restoring; restore rolls everything back with an automatic safety backup.

**Help**
- **Help & FAQ** — accordion of setup/troubleshooting help with copyable
  commands and deep links into the rest of the app.

**Quality of life throughout**: every terminal keeps its transcript when you
switch pages (streams keep running), Clear/Download buttons everywhere, save
dialogs are native, item/spell/achievement data is cached for offline use
after first view, Windows taskbar progress during long-running operations,
consistent loading/empty states, and themed scrollbars/text selection
matching the dark UI.

Many mutating actions — not just the newest additions but also things like
installing/removing titles, module installs/rebuilds, backups, accounts, and
LAN/Unbound-addon play — ship behind an "Enable untested features" toggle in
Settings until each one passes a live smoke test by hand. Read-only views and
a core set (server start/stop/restart, console, teleport, GM actions,
item mail, My Party, curated Settings) are already verified; everything
else is grey + locked until tested. See `docs/SMOKE-TESTS.md` for the full,
up-to-date list of what's verified and what's still pending.

## Architecture

The GUI is a thin Tauri 2 + Svelte 5 shell — no server logic lives in it. It
has two backends, and both speak the same JSON envelopes / NDJSON event
streams documented in `../cli/README.md` and `../docs/cli-contract.md`:

* **WSL mode** shells the bash `dml` CLI inside the `dml-arch` distro as
  `wsl.exe -d dml-arch -u dml -- dml <cmd> --json`.
* **Native mode** calls the `dml-wow` Rust library in-process. That code used
  to live under `src-tauri/src/dml/`; it now lives in the repo-root cargo
  workspace (`../crates/dml-core`, `../crates/dml-wow`) and this crate is a
  workspace member holding only the `#[tauri::command]` adapters. The same
  library also ships as a standalone `dml-wow` binary
  (`../crates/dml-wow-cli`), so a frontend does not have to be this one.

## Dev loop

    powershell -File ..\cli\dev-install.ps1   # install/refresh the dml CLI in WSL
    npm install
    npm run tauri dev        # run the app
    npm test                 # vitest
    npm run check            # svelte-check

Rust tests run from the REPO ROOT, not here — the cargo root is the workspace:

    cargo test --workspace   # launcher + dml-core + dml-wow + dml-wow-cli

CLI tests run in the distro: `wsl -d dml-arch -u dml -- bash -lc "cd /mnt/.../cli && bash build.sh && bats tests/"`

## Release build

    npm run tauri build      # NSIS+MSI under the ROOT ../target/release/bundle/

Builds are currently unsigned (SmartScreen warning expected).

## Layout

    src/lib/api.ts             typed invoke wrappers (Channel-based streaming)
    src/lib/terminal-state.ts  pure NDJSON→terminal-state reducer (vitest)
    src/lib/Terminal.svelte    embedded terminal (sections, runtime, jump-to-latest)
    src/lib/pages/             one Svelte component per sidebar page
    src/routes/+page.svelte    sidebar + status chip + page shell
    src-tauri/src/lib.rs       tauri commands (validated IPC surface)
    ../crates/dml-core/        envelope parsing + process runner + docker/compose
    ../crates/dml-wow/         the WoW library (SOAP, DB reads, config, backups)
    ../crates/dml-wow-cli/     the standalone `dml-wow` binary

## License

AGPL — same as the Dad's MMO Lab project this builds on. The 3D model viewer
adapter ports invocation logic from
[wow-model-viewer](https://github.com/Miorey/wow-model-viewer) (ISC).
