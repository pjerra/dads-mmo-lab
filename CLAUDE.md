# dads-mmo-lab — Claude Code notes

Dad's MMO Lab: self-hosted MMO private-server tooling (WoW WotLK via AzerothCore + mod-playerbots, and other titles) targeting WSL2 on Windows. Fork of the upstream dads-mmo-lab repo (remotes: `origin` = pjerra fork, `upstream`). License AGPL-3.0.

## Current work: DML Launcher (branch `feat/dml-launcher-windows`)

Open-source cross-platform GUI (Tauri 2, Windows-first) replacing the closed-source "The Lab". Spec: `docs/superpowers/specs/2026-07-14-dml-launcher-windows-design.md`. Plans in `docs/superpowers/plans/`:
- Plan 1 (dml CLI JSON foundation) — **complete**, final review verdict READY TO MERGE (merge = user decision, not done)
- Plan 2 (launcher shell, Tauri 2 + Svelte 5) — **code-complete + reviewed**; two USER-SUPERVISED gates remain: live `tauri dev` smoke (plan Task 7 Step 4) and one launch of the release exe
- Plan 3 (WoW SOAP+MySQL features) — **code-complete + final-reviewed (SAFE TO SHIP)**; adds the `dml wow` namespace (see cli/ section). USER GATES remain: create the `dmlsoap` GM3 SOAP account (worldserver console), then SOAP end-to-end verify + live mutating smokes (mail-item/teleport on a throwaway char)
- Plan 4 (My Party, `docs/superpowers/plans/2026-07-16-my-party.md`) — built via SDD, pending final review + user live gate
- Launcher-pages plan (Item DB/Teleport/Dashboard/Config editor) — **built via SDD, all 12 tasks + gates green**; pending final whole-branch review + the USER-SUPERVISED live click-through (Task 12 Step 5) before merge consideration
- Only ONE controller session may execute a plan on this checkout at a time (a Task-6 double-dispatch already happened once; check `.superpowers/sdd/progress.md` and `git log` before dispatching anything).

## launcher/ — DML Launcher (Plan 2 output)

- Tauri 2 (2.11.5) + Svelte 5 **SvelteKit** app: UI lives in `launcher/src/routes/+page.svelte` (there is NO App.svelte); shared code in `src/lib/` (api.ts invoke wrappers, terminal-state.ts pure reducer, Terminal.svelte); Rust shell in `src-tauri/src/` (`dml/envelope.rs`, `dml/runner.rs`, commands in `lib.rs`).
- Sidebar is grouped (Server: Home/Library · Characters: Dashboard/Teleport · Items & Bots: Item Database/Playerbots · Config: Settings/Modules) — data module `launcher/src/lib/nav.ts` (vitest-pinned), shell in `+page.svelte`; **Home is the default page** (status card + start/stop). Settings and Modules are ONE `Config.svelte` mount driven by a `tab` prop (single `{#if}` block in the shell — do not split it, hopping between them must keep unsaved edits). Future Lab-parity entries (GM Tools, Summon, Backups) get added to nav.ts only when their page ships. The config editor writes `AC_*` env vars via `dml wow config` (registry in `cli/src/40-config.sh`) — every setting is restart-to-apply EXCEPT the message of the day, which has no env/conf key and is instead set live over SOAP while the server keeps running.
- My Party (Playerbots page) adds bots via SOAP → Eluna bridge scripts (`cli/lua/party/*.lua`, deployed by `dml wow party-setup`) → `.playerbots bot addclass` in the player's session; needs the character online; party-setup deploys the scripts and the server must then be restarted once (Library/Config) to load them.
- **SECURITY**: the Modules page can raw-**read** all 5 files but raw-**write** ONLY the 3 module confs (the UI also opens the two protected names read-only — `READONLY_FILES` in Config.svelte mirrors the CLI lock) (`playerbots.conf`, `mod_ahbot.conf`, `mod_ale.conf`). `.env` and `docker-compose.override.yml` are read-only in raw-write (they'd let the editor + restart drive host command execution) — change those via the curated Settings tab / `config set`. The `raw-write)` arm rejects the two protected names (cli/src/90-main.sh) after the tmp write but before any `mv`; do NOT loosen this allowlist.
- The pending-restart banner state lives in a module-level runes store `launcher/src/lib/restart-state.svelte.ts` (NOT component-local) so it survives sidebar navigation. CharPicker fires `onpick` only on user interaction (never on mount — mount-time staging false-dirtied Config's one-way AHBot char row).
- Dev loop (from `launcher/`): `npm run tauri dev` / `npm test` (vitest, reducer) / `npm run check` (svelte-check) / `cd src-tauri; cargo test` (fixture-driven runner tests use cmd.exe scripts under `src-tauri/tests/fixtures/`).
- Production spawn is exactly `wsl.exe -d dml-arch -u dml -- dml <cmd> --json`; game ids validated `[A-Za-z0-9._-]+` in Rust before any spawn.
- Release: `npm run tauri build` → NSIS+MSI under `src-tauri/target/release/bundle/`; bare exe is `launcher.exe` (crate name), installers use productName "DML Launcher". Unsigned — SmartScreen warning expected.
- Terminal event contract: TermEvent union in api.ts must stay in sync with `cli/src/10-json.sh` emitters; unknown events (e.g. reserved `pct`) must be IGNORED, never crash.

## cli/ — the dml CLI (Plan 1 + Plan 3 output)

- `cli/dml` is a **committed build artifact**: `bash cli/build.sh` concatenates `cli/src/*.sh` in glob order (00-head, 10-json, 20-soap, 30-db, 90-main). NEVER edit `cli/dml` directly; edit `cli/src/*.sh` and rebuild.
- `dml wow` namespace (Plan 3): `soap-setup`, `soap-exec`, `items search`, `mail-item`, `teleport`/`teleport-list`, `characters`/`paperdoll` — documented in `cli/README.md` "wow subcommands". Security posture: SOAP host-published on 127.0.0.1:7878 only (pinned via the title's `.env` `DOCKER_SOAP_EXTERNAL_PORT` — base compose already publishes the port; never add a second `ports:` entry, Compose concatenates lists); SOAP calls flock-serialized (`~/.dml/soap.lock`); MySQL access strictly read-only (mutations via SOAP GM commands only); creds via `DML_SOAP_URL/USER/PASS` env (default admin/admin).
- Sanitization invariants (tested; keep them): `_xml_escape` escapes `&` FIRST; `sql_escape` escapes `\` then `'`; SOAP-bound free text strips `"`/CR/LF (AC #2695 second-command surface); numeric values are `^[0-9]+$`-whitelisted before SQL splice; every value-taking flag calls `_need_flag_val` before reading `$2` (else `set -u` aborts with no JSON envelope).
- bash while-read loops fed from command substitution need `|| [[ -n "$var" ]]` (trailing-newline strip silently drops the last row otherwise) — see `_items_rows_to_json`.
- Runs inside WSL2 distro `dml-arch` as user `dml`; games in `$HOME/games` (override for tests: `DML_GAMES_DIR`). Repo inside WSL: `/mnt/c/Users/perzi/dads-mmo-lab`.
- `--json` contract (envelopes + NDJSON streams, error codes) is documented in `cli/README.md` — the Tauri GUI and Plan 2+ build against it; changing it is a breaking change.
- Tests: `wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bats tests/"` (bats + jq installed in the distro; jq is test-only, never a runtime dep). Windows call-path smoke: `powershell -File cli\tests\windows-smoke.ps1` (also dev-installs the built CLI into the distro via `cli/dev-install.ps1`).
- Test harness `cli/tests/helpers/env.bash` stubs `docker` (env: `DML_STUB_RUNNING`, `DML_STUB_DOCKER_DOWN`, `DML_STUB_COMPOSE_EXIT`) and the port-scan tool so tests never touch the real host.
- `guides/DML-Windows/Install-DML.ps1` still embeds the OLD CLI v2.6.0 as bootstrap (here-string, lines ~836–1633) — do NOT edit it casually; installer↔cli sync is a dedicated later plan (must bump `$ExpectedCliVersion` at ~line 813 together with the embedded script).
- `set -euo pipefail` is load-bearing in the built CLI (`${1:?}` usage, `_stream_cmd` pipefail exit-code propagation).

## Gotchas

- **Line endings:** bash inside WSL chokes on CRLF. `.gitattributes` forces LF for `*.sh`, `*.bats`, `*.bash`, and `cli/dml`; keep new shell files LF.
- **PowerShell 5.1 + UTF-8:** BOM-less `.ps1` files containing non-ASCII (em dashes) mis-parse under the ANSI codepage. `cli/tests/windows-smoke.ps1` carries a UTF-8 BOM on purpose; keep BOMs on any non-ASCII `.ps1`.
- **Rust toolchain PATH:** Rust 1.97 MSVC is installed per-user but `%USERPROFILE%\.cargo\bin` may be missing from a fresh Claude shell's PATH — use the full path or prepend it per call. MSVC Build Tools 2022 (C++ workload) and Node 22 are installed; WebView2 present.
- The `games` namespace is JSON-first: `games list`/`status` always emit JSON; only `games start|stop|restart` have a text mode. Legacy top-level commands (`list`, `status`, `start`, …) keep their exact text output — the old C# tray parses it.
- `dml-start.sh` hook: `games start|restart` run `<compose_dir>/dml-start.sh <mode>` when present+executable (avoids re-running ac-db-import on restarts).

## SDD bookkeeping

`.superpowers/sdd/` (untracked) holds the progress ledger, task briefs/reports, and review packages for plan execution. Ledger: `.superpowers/sdd/progress.md`.
