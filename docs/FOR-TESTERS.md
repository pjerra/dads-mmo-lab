# DML Launcher — Notes for Testers

Thanks for helping test! Here's the honest state of things so you know what to expect.

## What it is
A Windows desktop app for running your **own** AzerothCore WotLK + Playerbots server locally — install, start/stop, config, GM tools, teleport, bots, backups — all from a GUI instead of the command line.

It's a **manager/launcher layered on top of AzerothCore's official Docker install** — it uses AzerothCore's official images and upstream community modules (playerbots, etc.). It is **not** a custom server or a fork, and it's not affiliated with AzerothCore.

## Status (read this)
Active development, **pre-release**. The app's core is being rewritten to native Rust for speed — most of it is now fast and native, but a few **install/maintenance** operations still run through the bash CLI under the hood (being ported). It works end-to-end today; expect some rough edges, especially around first-time install.

## Getting it
> _hypeer: keep whichever line applies — a built installer you send, or build-from-source._
- **Installer:** run the `DML Launcher` setup I send you.
- **From source:** clone the repo (branch `release/dml-launcher` — the launcher-only branch; see the root README for prerequisites), then in `launcher/`: `npm install` → `npm run tauri dev` (or `npm run tauri build` for an installer).

**Requirements:**
- **Docker Desktop** (Windows). The first server install pulls AzerothCore's official images (a few GB) — one-time.
- Two run modes:
  - **Native (recommended — faster):** Docker Desktop + Git Bash. Launch via *"Start DML Launcher (native experiment)"*.
  - **WSL:** WSL2 + the DML environment. Launch via *"Start DML Launcher"*.
- Tip: some mutating features ship **locked** until they've been smoke-tested. If a button looks disabled, there's a **testing-mode toggle in Settings** that unlocks them.

## What's solid — please hammer on these
- Start / stop the server + the Home status dashboard
- Accounts: create / set password / GM level / delete
- GM tools: level, gold, heal, revive, summon, teleport, send-home
- Teleport, mail items, Console (send commands + live log)
- Settings + Module tuning (save & apply)
- Character sheet: paperdoll, talents, achievements, 3D model
- Bot Browser, My Party, Statistics, Item database

## Still being ported / known rough edges
- **Install, module install/update, backup/restore, world-restart, LAN setup, self-update** still run through the CLI (they work — just not native yet; this is the part actively being finished).
- **Native mode** is newer than WSL mode — if something behaves oddly, note **which mode** you're in.
- **3D models** load from Wowhead's CDN; a few custom/GM items have no model data and show a "can't be shown in 3D" note (expected).
- Browsing lots of 3D character models used to spin the fans up — just fixed; if you still notice it, flag it.

## Feedback that helps most
- **Which mode** (native vs WSL) + what you did → what happened.
- Anything that **hangs, spins the fans, or fails silently**.
- **Install / first-run friction** — that's exactly what's being improved right now.
- **Your feature ideas** — genuinely want them for upcoming releases.

Best way to send feedback: [hypeer: Discord channel / DM / GitHub issues — pick one].

Thanks again! 🙏
