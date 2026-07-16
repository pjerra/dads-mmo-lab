# DML Launcher

Tauri 2 desktop shell for Dad's MMO Lab. Windows-first; the core is
cross-platform (all Windows specifics live behind cfg(windows)).

The GUI is a thin shell: every feature calls the `dml` CLI inside the
`dml-arch` WSL distro as `wsl.exe -d dml-arch -u dml -- dml <cmd> --json`
and renders the JSON envelopes / NDJSON event streams documented in
`../cli/README.md`. No server logic lives in the GUI.

## Pages

- **Library** — install status per game, Start/Stop with live terminal output.
- **Dashboard** — world up/down, uptime, players online, update-time stats;
  character viewer (level, gold, equipped gear as of the last save).
- **Item Database** — search `item_template` by name/quality/level; send any
  item to a character by in-game mail.
- **Teleport** — pick a character and one of the ~2000 named locations
  (two-step confirm).
- **Config** — Settings tab: curated server settings (XP/gold rates, bot
  population, bot autologin, AHBot, message of the day) with safe ranges.
  Every setting except the message of the day writes an `AC_*` env var into
  the wow title's compose override (restart-to-apply); the message of the
  day has no env/conf key in this AC build, so it is instead sent over SOAP
  and applies **instantly** while the server keeps running — no restart.
  Files tab (Advanced): direct editor for `.env`, the compose override
  (YAML-validated before save), and the module confs — every save keeps a
  `.bak`. Both tabs offer **Save** (shows a restart-needed banner) and
  **Save & Restart** (confirm → streams the restart into the terminal
  panel).
- **Playerbots** — disabled until the My Party feature (Plan 4).

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
