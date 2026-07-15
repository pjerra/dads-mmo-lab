# DML Launcher — Design Spec

**Date:** 2026-07-14
**Status:** Draft for review
**Author:** Josh (Dad's MMO Lab) with Claude
**License:** AGPL-3.0 (consistent with the rest of dads-mmo-lab and AzerothCore)

---

## 1. Summary

Build **DML Launcher**, an open-source graphical app that installs, manages, and
plays a local WoW 3.3.5a (Wrath of the Lich King) private server — AzerothCore +
mod-playerbots — with **no terminal and no config-file editing**. It is the
graphical front door to the Dad's MMO Lab tooling that today lives in shell
scripts and a menu-driven TUI.

It is inspired by **The Lab** (github.com/0xVe1L/the-lab), the closed-source
Steam Deck app by Veil (0xVe1L) + alembic.gg. The Lab is Steam-Deck-only, its
source is not published, and its author went quiet in mid-June 2026 immediately
after announcing a Windows `.exe` was "almost done." DML Launcher is an
**independent, open-source, cross-platform** implementation under the Dad's MMO
Lab umbrella — **Windows first**, but architected so the same app later runs on
Steam Deck and Linux.

## 2. Goals

- One windowed app that manages the **whole DML game library** (install / start /
  stop / status), with WoW WotLK getting deep, Lab-style management first.
- **Zero terminal** for the user: the app owns WSL2 enablement, elevation, the
  Arch + Docker substrate, and every server operation.
- An **embedded terminal pane** that streams live install/console output —
  runtime counter, spinner, collapsible sections, jump-to-latest — the way The
  Lab does.
- WoW deep features at v1: **My Party** bot builder, **item database + in-game
  mailer**, **teleports**, and **module toggles**.
- Cross-platform core: no Windows-only assumptions in the shared logic, so a
  Steam Deck / Linux build is a later packaging job, not a rewrite.
- Everything open source; the community can read, script against, and extend it.

## 3. Non-goals (v1)

- **Not** shipping or downloading any game client or Blizzard assets. The user
  supplies a legally obtained 3.3.5a client. (Consistent with `DISCLAIMER.md`.)
- **Not** running public/monetized servers. Personal, offline-first use.
- **Not** reusing or decompiling The Lab's binary. We mirror observable behavior
  and the open server interfaces (SOAP, MySQL), nothing proprietary.
- HD patch installer, in-app auto-updater, controller stack, and **Play Together**
  (P2P mesh) are **deferred past v1** (see §12).

## 4. Positioning vs The Lab

The Lab is a **Tauri** app (confirmed: `TheLab.AppImage` bundles
`libwebkit2gtk-4.1`, uses a `latest.json` updater manifest, and its binary
carries `tauri-runtime-wry` + `tauri-plugin-*` strings and a
`/home/deck/the-lab-source/the-lab/src-tauri` build path). It wraps AzerothCore
+ mod-playerbots in Docker and talks to the worldserver over **SOAP**.

We adopt the same shape (Tauri, SOAP) because it is the correct one, but:

- **Open source, AGPL**, in the `dads-mmo-lab` monorepo.
- **Windows-first**, cross-platform core.
- The "brains" live in a **versioned `dml` CLI**, not baked into the GUI binary,
  so the Deck TUI (`wow-manage.sh`) and the GUI run the same code.

### Reference: The Lab's command surface (recovered, for parity targeting)

The Lab exposes ~140 Tauri IPC commands. This is our behavioral checklist — the
features to reach eventual parity with. Representative clusters:

- **Install / lifecycle:** `greet`, `analyze_install`, `adopt_install`,
  `start_install`, `cancel_install`, `start_uninstall`, `start_server`,
  `stop_server`, `restart_server`, `migrations_status`, `run_migrations`.
- **Embedded terminal:** `console_attach`, `console_send`, `console_detach`.
- **WoW client:** `get_wow_client_state`, `set_wow_directory`, `fix_realmlist`,
  `set_realmlist`.
- **Items / economy:** `search_items`, `get_items_by_entries`,
  `send_item_to_character`, icon/tooltip extraction (`extract_item_icons`,
  `extract_tooltip_data`, `build_talent_data`).
- **Playerbots / party:** `list_playerbots`, `set_playerbot_level`,
  `summon_playerbot_to_character`, `invite_bot_to_party`, `get_user_party`,
  `kick_bot_from_party`, `get_playerbot_settings`, `set_playerbot_settings`,
  `flush_random_bots`, `save_party_preset`, `list_party_presets`,
  `export_party_preset_toml`, `import_party_preset_toml`.
- **World / GM:** `list_teleport_locations`, `teleport_character_to_location`,
  `teleport_character_to_coords`, `get_world_settings`, `set_world_settings`,
  `gm_set_money`, `gm_set_health_pct`, `gm_revive`, `gm_flag_race_change`,
  `summon_transmog_npc`.
- **Modules:** `update_module_conf`, `read_module_conf_raw`,
  `write_module_conf_raw`, `configure_ahbot_character`, `reload_ahbot`,
  `repair_ahbot`.
- **Accounts / characters:** `lookup_account`, `list_account_characters`,
  `backup_characters`, `validate_backup`, `restore_characters`,
  `get_character_paperdoll`, `get_character_talents`.
- **Platform:** `steamos_status`, `is_gaming_mode`, `run_steamos_fix`,
  `add_to_steam`, `get_steam_integration_status`, `install_consoleportlk`,
  `apply_controller_preset`.
- **Play Together (deferred):** `install_tailscale`, `tailscale_login`,
  `start_hosting`, `regenerate_share_code`, `join_via_code`,
  `import_guest_character`, `pull_visit_snapshot`, `return_home`,
  `list_visit_backups`, `merge_visit_backup`.

### Backlog additions (post-approval, user-requested)

- **Multi-function config editor (requested 2026-07-15):** a config-editing
  panel that opens **in the same content slot the embedded Terminal uses**
  (bottom pane of the page area, not a separate OS window). "Multi-function"
  = one editor covering the several config surfaces the stack already has:
  the title's `.env` values, `docker-compose.override.yml` env merges (the
  `soap-setup` write path), and module confs (`playerbots.conf`,
  `mod_ahbot.conf`, `mod_ale.conf` — The Lab's `update_module_conf` /
  `read_module_conf_raw` / `write_module_conf_raw` parity cluster above).
  Needs a matching `dml wow config get|set` CLI verb pair (GUI stays a thin
  shell); mutations that need a worldserver restart must say so in the
  response (`restart_required`, same contract as `soap-setup`). Slot it into
  the launcher-pages plan alongside lighting up the sidebar.

## 5. Architecture

```
┌─ Windows (native — WebView2, NOT bundled WebKit) ──────────────┐
│  DML Launcher — Tauri 2                                         │
│  ├─ UI: TypeScript + Svelte                                    │
│  │    Sidebar: Dashboard · Item DB · Playerbots / My Party ·   │
│  │    Teleport · Modules · Settings                            │
│  │    Embedded Terminal pane: NDJSON → live log, runtime       │
│  │    counter + spinner, collapsible sections, jump-to-latest  │
│  └─ Rust shell (thin):                                         │
│       • spawns  wsl.exe -d dml-arch -u dml dml <cmd> --json    │
│       • first-run substrate wizard (elevated Install-DML       │
│         logic: WSL2 enable, UAC, reboot-and-resume, Arch+Docker)│
│       • tray icon, WSL keepalive (job object),                 │
│         SetThreadExecutionState sleep-block                    │
│       • all of the above behind a `platform` trait so the      │
│         Deck/Linux build swaps them for no-ops / systemd       │
└───────────────┬────────────────────────────────────────────────┘
                │ JSON (request/response) + NDJSON (progress) over stdio
┌─ WSL2: dml-arch distro ────────────────────────────────────────┐
│  dml CLI (promoted from Install-DML here-doc to versioned cli/)│
│  ├─ dml games list | install | start | stop            --json  │
│  ├─ dml wow items search | mail-item                   --json  │
│  ├─ dml wow party create | preset | list-bots          --json  │
│  ├─ dml wow teleport | modules | account               --json  │
│  └─ shared libs refactored out of wow-manage.sh                │
│      (Deck TUI menu + GUI call the same code paths)            │
│            │                        │                           │
│         Docker                 AzerothCore                      │
│      (all game stacks)     SOAP :7878  +  MySQL (acore_*)       │
└────────────────────────────────────────────────────────────────┘
```

### 5.1 Load-bearing decisions

1. **The GUI is a thin shell; the brains are the `dml` CLI.** Every GUI feature
   must exist first as a `dml … --json` command. This keeps Deck/Linux parity
   automatic (they invoke the same CLI), makes each feature testable without a
   GUI, and gives the community a scriptable API.

2. **WebView2 is a hard requirement, proven by experiment.** On 2026-07-14 we ran
   the real `TheLab.AppImage` v0.0.7 inside a throwaway Ubuntu WSL distro. The
   Tauri backend booted (it seeded presets and wrote its config), but the webview
   died every time with `Could not create default EGL display: EGL_BAD_PARAMETER`.
   Root cause: bundled **WebKitGTK 2.46.5 requires a GBM/DRM render node**, and
   WSL kernels expose none (`/dev/dri` absent; WSLg routes GPU through `/dev/dxg`
   + d3d12, which WebKit can't use). The Lab's own `safe_graphics` fallback also
   failed, and an LD_PRELOAD GBM→surfaceless shim didn't help. Conclusion:
   "run the Linux AppImage on Windows" is a dead end; a **native Windows build
   using the system WebView2** is required. WebView2 ships on Windows 10/11.

3. **WoW deep features use open interfaces only:** AzerothCore **SOAP** (enabled
   in `worldserver.conf` at install) for live commands (accounts, mail, teleport,
   bots), replacing today's fragile `docker attach` where Ctrl+C kills the
   server; and **read-only MySQL** against `acore_world` / `acore_characters` for
   the item DB, talents, and character views.

4. **The C# tray app is absorbed, then retired.** Its two hard-won tricks — WSL
   keepalive via a kill-on-close job object, and sleep-blocking via
   `SetThreadExecutionState` — move into the Rust shell. One launcher binary.

## 6. Components

Each is independently understandable and testable.

- **`cli/` — the `dml` CLI (bash, in `dml-arch`).** Promote the CLI out of its
  ~2,800-line here-doc in `Install-DML.ps1` into versioned files. Add a stable
  `--json` mode: request/response JSON on success, `{code, message, hint}` on
  error, and **NDJSON** event streams for long operations. Refactor the reusable
  guts of `wow-manage.sh` into shared libs so the TUI and CLI share one
  implementation. *Depends on:* Docker, the game installers, SOAP, MySQL.
- **`launcher/` — the Tauri app.**
  - *Rust shell:* command runner (`wsl.exe … dml … --json`), NDJSON stream
    parser, substrate wizard (elevated Install-DML logic), tray + keepalive +
    sleep-block, all behind a `platform` trait. *Depends on:* `cli/`, WSL, Windows
    APIs (Windows build only).
  - *Svelte UI:* sidebar navigation, the embedded terminal component, and the
    v1 feature pages. *Depends on:* the Rust shell's Tauri commands.
- **Substrate installer.** Reuses the phased, reboot-resume, re-run-safe logic
  already in `Install-DML.ps1` (WSL2 + Arch `dml-arch` + rootful Docker), invoked
  as an elevated child process and surfaced in the embedded terminal.
- **WoW feature layer (inside `cli/`).** SOAP client + MySQL reader that back the
  `dml wow …` subcommands.

## 7. Data flow (v1)

**Install (embedded terminal).** Library → "Install WoW WotLK." First run triggers
the substrate wizard (elevated: enable WSL2, reboot-and-resume, import Arch +
Docker). Then the shell spawns
`wsl.exe -d dml-arch -u dml dml install wow-server-playerbots --json`. The CLI
emits NDJSON — `{section_start}`, `{line, level}`, `{pct}`, `{section_end}`,
`{done, admin_user, admin_pass}` — and the Svelte pane renders it as a live
terminal (runtime counter, spinner, collapsible sub-script sections,
jump-to-latest). On `done`, the sidebar unlocks. (Mirrors `start_install`.)

**Party create (My Party).** Pick role → class → spec → level, or paste a Wowhead
talent code. UI calls
`dml wow party invite --class mage --spec frost --level 19 --talents … --json`
(= `invite_bot_to_party`); the CLI drives the playerbot over SOAP and reads back
with `get_user_party`. Presets save as TOML under
`~/.config/dads-mmo-lab/party-presets/*.toml` (`save_party_preset`).

**Item mail.** Item DB search (`search_items` reads `acore_world.item_template`;
icons/tooltips scraped from the client) → Send → `send_item_to_character` issues
the GM mail command over SOAP → toast confirms → item appears in the in-game
mailbox.

## 8. Data & config formats

- **Settings:** `~/.config/dads-mmo-lab/settings.json` — the **same namespace The
  Lab writes**, so the two can coexist and a user can migrate between them. Keys
  observed in the running app include `wow_client_dir`, `admin_user`,
  `admin_pass`, `active_install_path`, `active_soap_url`,
  `auto_shutdown_on_client_exit`, `selected_character_guid`, `cursor_faction`,
  `safe_graphics`, `hd_patch_installed`, and the Play Together fields
  (`away_share_code`, `host_token`, `snapshot_keep_depth`).
- **Party presets:** TOML, `schema_version = 1`, with `[preset_info]`, a
  `target`, `[party.player]`, and `[[party.bots]]` entries carrying
  `role/class/level/spec` plus a **Wowhead-compatible `talents` code** per bot.
  (Schema captured verbatim from The Lab's seeded examples.)

## 9. Error handling (dad-friendly)

CLI commands return `{code, message, hint}` on failure. The GUI maps each `code`
to a plain-English card with a one-click fix — e.g. `DOCKER_DOWN` → "Docker isn't
running" + **Run the fix** (`run_steamos_fix`); `CLIENT_MISSING` → "Point me at
your WoW folder." The embedded terminal is always one click away for the full
log. Half-finished installs get a **Finish setup / repair** path (mirrors The
Lab's `analyze_install` + `adopt_install`).

## 10. Testing

- **CLI contract:** `bats` tests pin each `dml … --json` command's schema and use
  golden-file NDJSON streams for progress output.
- **Rust shell:** unit tests for the command runner and NDJSON parser.
- **UI:** `tauri-driver` + WebdriverIO for the core flows (install view, party
  create, item mail).
- **Substrate:** a **Win10 + Win11 clean-VM matrix** exercising the installer
  end-to-end (enable WSL2, reboot-resume, install a server, start it).

Because the UI is thin, the CLI contract tests catch most regressions without a
running GUI.

## 11. Distribution

Tauri bundler → `.exe` / `.msi` (NSIS + WiX). Unsigned initially (a documented
SmartScreen warning), with a path to code signing + a `winget` manifest later.
The in-app updater (v1.1) reuses the same `latest.json` pattern The Lab ships.

## 12. Scope & phasing

- **v1 — "Lab-parity core":** substrate wizard + full game library
  (install/start/stop) + embedded terminal + WoW **My Party**, **item DB +
  mailer**, **teleports**, **module toggles**, **account manager**, health/repair,
  LAN toggle.
- **v1.1:** HD patch installer (resumable download) + in-app Tauri auto-updater +
  Add-to-Steam / shortcuts.
- **Later:** controller stack (ConsolePortLK/WoWPadX), **Play Together** (Tailscale
  mesh + character snapshot/merge; note The Lab's depends on the proprietary
  `alembic.gg` auth service — we will design an open equivalent), Deck/Linux
  packaging of the same app.

## 13. Repository layout (monorepo)

```
dads-mmo-lab/
├─ cli/                     # the `dml` CLI, promoted to versioned files + libs
│   ├─ dml                  # entrypoint
│   ├─ lib/                 # shared with wow-manage.sh
│   └─ tests/               # bats contract tests
├─ launcher/                # Tauri 2 app
│   ├─ src/                 # Svelte UI
│   └─ src-tauri/           # Rust shell
└─ docs/superpowers/specs/  # this spec
```

The existing `guides/DML-Windows/Install-DML.ps1` substrate logic is reused by
the launcher's first-run wizard; the C# tray app is retired once the Rust shell
absorbs keepalive + sleep-block.

## 14. Prerequisites (developer machine — checked 2026-07-14)

Present: **VS Code** 1.127.0, **Node.js** 22.21.1, **WebView2 Runtime** v150.
Still to install before scaffolding: **Rust** (via `rustup`) and the **MSVC C++
Build Tools** ("Visual Studio Build Tools" with the Desktop C++ workload — the
linker Rust needs; not the full Visual Studio IDE).

## 15. Open questions / risks

1. **mod-playerbots control interface.** The repo today documents NPCBot chat
   commands only; ambient playerbots are configured via env vars and party
   control is delegated to The Lab. We must confirm the exact SOAP/chat commands
   (or DB writes) mod-playerbots accepts for summon / invite / level / talents.
   This is the highest-risk unknown for My Party.
2. **SOAP enablement on existing installs.** New installs enable SOAP at setup;
   adopted/migrated installs may need a guided `worldserver.conf` edit + restart.
3. **Play Together auth.** The Lab depends on `alembic.gg`. An open re-implementation
   (or a self-hostable auth) is a design project of its own — deferred.
4. **Item DB enrichment.** Icons/tooltips are scraped from the user's client
   (DBC/MPQ). We need a reliable extraction path on Windows via the WSL side.
