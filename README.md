# DML Launcher

A free, open-source desktop app that installs and runs your own **World of
Warcraft: Wrath of the Lich King** private server with ~2000 AI playerbots —
start/stop, live console, accounts, characters, bots, modules, backups, LAN and
internet play — all from one window, no terminal needed.

Built on [Dad's MMO Lab](https://github.com/DadsMmoLab/dads-mmo-lab),
[AzerothCore](https://www.azerothcore.org/) and mod-playerbots. Open-source
emulators only: **you supply your own game client**; nothing copyrighted is
downloaded or distributed by this project (see [DISCLAIMER.md](DISCLAIMER.md)).

This branch (`release/dml-launcher`) contains only what is needed to build and
run the launcher. The full feature list is in [docs/FEATURES.md](docs/FEATURES.md).

---

## 1. Requirements

| | Windows 10/11 (primary) | Linux (Debian/Ubuntu) |
|---|---|---|
| Containers | **Docker Desktop** (WSL2 backend) — recommended "native" mode | Docker Engine (`scripts/install-dml.sh` installs it) |
| Also needed | Git for Windows (Git Bash) | — |
| Disk | ~30 GB free (server images + build) | same |
| RAM | 8 GB minimum, 16 GB if you compile modules | same |
| CPU | Virtualization enabled in BIOS | — |

The first server install pulls AzerothCore's official images (a few GB) — one time.

---

## 2. Run it (users)

### Windows — native mode (recommended)

1. Install [Docker Desktop](https://www.docker.com/products/docker-desktop/) and
   [Git for Windows](https://git-scm.com/download/win). Start Docker Desktop once.
2. Optional check: run `guides\DML-Windows\Install-DML-Native.ps1` in PowerShell —
   it verifies Docker Desktop and Git Bash and tells you what is missing.
3. Get `DML Launcher_x.y.z_x64-setup.exe` (from a Release, or build it — section 3).
   It is unsigned: SmartScreen will warn → *More info* → *Run anyway*.
4. Open **DML Launcher** → **Library** → install *WoW WotLK Playerbots* and answer
   the installer's questions in the built-in terminal.
5. Point your 3.3.5a client's `realmlist.wtf` at `127.0.0.1` and log in.

### Windows — WSL2 mode (alternative)

Runs the server inside an Arch Linux WSL2 distro (`dml-arch`) instead of Docker
Desktop. Run `guides\DML-Windows\Install-DML.ps1` from an **elevated** PowerShell
(it enables WSL2 — a reboot may be needed), then install the launcher as above.
On first run, click **Set up backend**; the launcher provisions the distro from
files bundled inside it. Full guide: [guides/DML-Windows/DML-Windows-HOWTO.md](guides/DML-Windows/DML-Windows-HOWTO.md).

### Linux

```bash
./scripts/install-dml.sh        # installs Docker Engine, adds you to the docker group
# log out and back in if the script tells you to
```
Then build and start the launcher (section 3). Linux support is newer than
Windows — please report anything odd.

---

## 3. Build from source (developers)

Prerequisites:

- **Rust** (stable) — https://rustup.rs
- **Node.js 18+** — https://nodejs.org
- **Tauri 2 system deps** — https://v2.tauri.app/start/prerequisites/
  - Windows: Visual Studio Build Tools (C++ workload) + WebView2 (already on Win 11)
  - Linux: `libwebkit2gtk-4.1-dev build-essential libssl-dev libayatana-appindicator3-dev librsvg2-dev`

```bash
git clone --branch release/dml-launcher https://github.com/pjerra/dads-mmo-lab.git
cd dads-mmo-lab/launcher
npm install
npm run tauri dev          # dev mode with hot reload
npm run tauri build        # installers land in ../target/release/bundle/
```

On Windows you can also double-click `start-launcher.bat` at the repo root to
run dev mode.

Tests (from the repo root unless noted):

```bash
cargo test --workspace     # Rust crates + launcher backend
cd launcher && npm test    # frontend (vitest)
cd launcher && npm run check
```

The bash `dml` CLI (`cli/`) is built with `bash cli/build.sh` — edit `cli/src/*.sh`,
not `cli/dml`. Its `--json` contract is in [docs/cli-contract.md](docs/cli-contract.md).

---

## 4. What is in this branch

| Path | What |
|---|---|
| `launcher/` | The desktop app — Tauri 2 + Svelte 5 (frontend in `src/`, Rust backend in `src-tauri/`) |
| `crates/` | Rust workspace: `dml-core`, `dml-wow` (server driver), `dml-wow-cli` |
| `cli/` | The bash `dml` CLI used by the WSL2 backend, plus the Eluna Lua bridges |
| `guides/` | Upstream title installers (`install-*.sh`) bundled into the launcher, and the Windows setup/uninstall scripts |
| `scripts/install-dml.sh` | Linux prerequisites installer |
| `docs/` | Feature list, tester notes, CLI contract |

Where things live at runtime: launcher settings and logs in `~/.dml/`
(`%USERPROFILE%\.dml\` on Windows); the installed server in `%USERPROFILE%\dml-native\`
(native) or inside the `dml-arch` distro (WSL2).

---

## 5. Help

- [docs/FOR-TESTERS.md](docs/FOR-TESTERS.md) — current state, known rough edges, what feedback helps
- [docs/FEATURES.md](docs/FEATURES.md) — everything the app can do
- In-app **Help & FAQ** page
- Issues: https://github.com/pjerra/dads-mmo-lab/issues

License: [AGPL-3.0](LICENSE-AGPL). Contributions welcome — see [CONTRIBUTING.md](CONTRIBUTING.md).
