# DML Launcher

Tauri 2 desktop shell for Dad's MMO Lab. Windows-first; the core is
cross-platform (all Windows specifics live behind cfg(windows)).

The GUI is a thin shell: every feature calls the `dml` CLI inside the
`dml-arch` WSL distro as `wsl.exe -d dml-arch -u dml -- dml <cmd> --json`
and renders the JSON envelopes / NDJSON event streams documented in
`../cli/README.md`. No server logic lives in the GUI.

## Pages

The sidebar is grouped into sections; entries for upcoming features appear
as they ship.

**Server**
- **Home** — landing page: world up/down card (players, uptime, update-time
  stats, bots online / max loadable) plus Start/Stop for the WoW server with
  live terminal output.
- **Library** — install status per game, Start/Stop with live terminal output.

Every terminal panel keeps its transcript when you switch pages (streams keep
writing while you're away), and has Clear + Download buttons — Download opens
a native save dialog. The Console page fills the window; on other pages the
terminal auto-scrolls into view when a run starts.

**Characters**
- **Dashboard** — world up/down, uptime, players online, update-time stats;
  character viewer (level, gold, equipped gear as of the last save).
- **Teleport** — pick a character and one of the ~2000 named locations
  (two-step confirm).
- **GM Tools** — pick any character: Revive / Full heal / Set level / Set
  gold. Level works even while the character is offline; the other three
  need them logged in (instant, no relog — applied through the same server
  bridge My Party uses). Level and gold ask for confirmation before
  applying. First use may need **Deploy server bridges** + a server restart.
  Also summons temporary service NPCs (auctioneer, banker, innkeeper,
  stable master, repair bot, casino — or any creature entry id); they
  despawn after 5 minutes.

**Items & Bots**
- **Item Database** — search `item_template` by name/quality/level; send any
  item to a character by in-game mail.
- **Playerbots (My Party)** — auto-detects your online character and builds a
  party of playerbots: click a class to add a bot, see your group, kick or
  re-summon bots. First use shows **Enable My Party** (one-time: deploys the
  Eluna bridge scripts — then stop and start the server from Home or Library
  to load them). Requires the character online.
  Each bot row also has **Gear up** / **Fix talents** / **Maintain**
  (whispered to the bot as if you typed it), and a **Party presets** card
  saves your current lineup under a name and loads it back later —
  loading replaces your current bots and re-gears/re-talents the new
  ones automatically.

**Config** (Settings and Modules are one editor split across two entries; a
save on either shows the restart-needed banner on both)
- **Settings** — curated server settings (XP/gold rates, bot population, bot
  autologin, AHBot, message of the day) with safe ranges. Every setting
  except the message of the day writes an `AC_*` env var into the wow
  title's compose override (restart-to-apply); the message of the day has no
  env/conf key in this AC build, so it is instead sent over SOAP and applies
  **instantly** while the server keeps running — no restart.
- **Modules** — direct editor for the module confs (`playerbots.conf`,
  `mod_ahbot.conf`, `mod_ale.conf`, YAML/`.bak` semantics unchanged: every
  save keeps a `.bak`). `.env` and the compose override open **read-only** —
  a bad edit there could run commands on the host, so they are locked; change
  them via Settings. Save shows a restart-needed banner; Save & Restart
  streams the restart into the terminal panel.
- **Backups** — one-click snapshots of every character, account and bot
  (works while the server runs). Restoring rolls everything back to that
  moment: the server stops, a safety backup is taken automatically, the
  snapshot is imported, and the server restarts. Keeps the newest 10.

## Dev loop

    powershell -File ..\cli\dev-install.ps1   # install/refresh the dml CLI in WSL
    npm install
    npm run tauri dev        # run the app
    npm test                 # vitest (terminal-state reducer)
    npm run check            # svelte-check
    cd src-tauri; cargo test # runner + envelope + command tests

## Release build

    npm run tauri build      # NSIS installer under src-tauri/target/release/bundle/

Builds are currently unsigned (SmartScreen warning expected).

## Layout

    src/lib/api.ts             typed invoke wrappers (Channel-based streaming)
    src/lib/terminal-state.ts  pure NDJSON→terminal-state reducer (vitest)
    src/lib/Terminal.svelte    embedded terminal (sections, runtime, jump-to-latest)
    src/routes/+page.svelte    sidebar + game library
    src-tauri/src/dml/         envelope parsing + WSL process runner (cargo tests)
    src-tauri/src/lib.rs       tauri commands: games_list/status/start/stop, dml_version
