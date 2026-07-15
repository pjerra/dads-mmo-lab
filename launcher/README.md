# DML Launcher

Tauri 2 desktop shell for Dad's MMO Lab. Windows-first; the core is
cross-platform (all Windows specifics live behind cfg(windows)).

The GUI is a thin shell: every feature calls the `dml` CLI inside the
`dml-arch` WSL distro as `wsl.exe -d dml-arch -u dml -- dml <cmd> --json`
and renders the JSON envelopes / NDJSON event streams documented in
`../cli/README.md`. No server logic lives in the GUI.

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
