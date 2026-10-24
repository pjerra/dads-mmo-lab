# Yu'lon

<!-- OWNER: tagline — plan draft is "Your own private MMO server, with buttons instead of a terminal." Replace with the owner's wording. -->
> Your own private MMO server, with buttons instead of a terminal.

[![Latest release](https://img.shields.io/github/v/release/DadsMmoLab/dads-mmo-lab)](https://github.com/DadsMmoLab/dads-mmo-lab/releases)
[![Downloads](https://img.shields.io/github/downloads/DadsMmoLab/dads-mmo-lab/total)](https://github.com/DadsMmoLab/dads-mmo-lab/releases)
[![CI](https://github.com/DadsMmoLab/dads-mmo-lab/actions/workflows/ci.yml/badge.svg?branch=Yulon)](https://github.com/DadsMmoLab/dads-mmo-lab/actions/workflows/ci.yml)
[![License: AGPL-3.0](https://img.shields.io/badge/License-AGPL--3.0-blue.svg)](LICENSE-AGPL)

![Yu'lon icon](pylauncher/assets/yulon_512.png)

<!-- OWNER: hero screenshot of the Catalog — save as docs/img/catalog.png (does not exist yet), then replace this line with ![Yu'lon Catalog](docs/img/catalog.png). -->

## What it is

Yu'lon is a desktop app that installs and runs your own private MMO server. Pick a game from
the Catalog, click **Install**, and the app builds open-source server software in Docker and gives
you Server, Console, Accounts, Modules and Networking tabs to run it with.

You play with bots beside you, solo or with friends on your own network. You bring your own game
client — Yu'lon never downloads or ships one.

## Supported games

Generated from `pylauncher/yulon/catalog/catalog.json` by `tools/readme_games_table.py` — that
script prints this table, so it cannot drift from the catalog:

| Game | Status | Server software | Your client | Linux | Windows | macOS |
|---|---|---|---|---|---|---|
| WoW WotLK | stable | AzerothCore (mod-playerbots fork) + mod-playerbots | 3.3.5a (12340) | yes | yes | yes |
| WoW TBC | beta | CMaNGOS mangos-tbc + cmangos/playerbots + tbc-db | 2.4.3 (8606) | yes | yes | no |
| WoW Vanilla | beta | CMaNGOS mangos-classic + cmangos/playerbots + classic-db | 1.12.1 (5875) | yes | yes | no |
| WoW Tortoise | beta | tortoise-wow/tortoise-wow @ main (the Penqle core) + Sagiroth/TortoiseBots | 1.18.1 (7272) | yes | yes | no |
| Centurion | beta | Centurion (TrinityCore 3.3.5 fork with its own playerbots) | 3.3.5a (12342) | yes | yes | no |
| WoW Unbound | beta | AzerothCore (mod-playerbots fork) + mod-playerbots + mod-ale + Wrath Unbound | 3.3.5a (12340) | yes | yes | yes |
| DDON (Other games) | beta | Arrowgene DDON server | 03.04.003 | yes | yes | no |
| DFO (Other games) | beta | 115US DFO server | US 2.38.3.25 | yes | yes | no |

You bring the client; Yu'lon never downloads or ships one.

## Install in 3 steps

Per-OS detail lives in [docs/install.md](docs/install.md).

- **Windows 10/11:** download `Yulon-<version>-windows-x64.zip`, unzip it to a folder you own,
  run `yulon.exe` (SmartScreen: More info → Run anyway). Yu'lon sets up WSL2 + Docker Desktop,
  asks you for UAC, and needs one reboot after WSL2 is enabled.
- **Linux / Steam Deck:** download the `Yulon-<version>-x86_64.AppImage` (or the
  `Yulon-<version>-x86_64.tar.gz` if FUSE is missing, e.g. on Arch), make it executable, run it.
  Yu'lon installs Docker Engine (asks for sudo; on SteamOS it briefly unlocks and relocks the
  read-only filesystem). On Deck, add it to your Steam library as a non-Steam game.
- **macOS (Apple silicon only):** open the `Yulon-<version>-macos.dmg`, drag Yu'lon to
  Applications, right-click → Open. Needs Docker Desktop. Only WotLK and Unbound install on macOS.

## Screenshots

<!-- OWNER: screenshots — save PNGs under 300 KB as docs/img/catalog.png (hero, Catalog), docs/img/server.png (WotLK Server tab running), docs/img/modules.png (Modules tab), docs/img/accounts.png (Accounts tab), docs/img/small-window.png (1280x800 small-window layout); then replace these lines with the images. Check each for leaked names, IPs and passwords first. -->
- Catalog (hero): `docs/img/catalog.png`
- WotLK Server tab running: `docs/img/server.png`
- Modules tab: `docs/img/modules.png`
- Accounts tab: `docs/img/accounts.png`
- Small-window layout (1280×800): `docs/img/small-window.png`

## How it runs / where files live

Game servers run in Docker containers; what provides Docker depends on your system
([details](docs/where-files-live.md)):

| You are on | What runs the servers |
|---|---|
| Linux | Docker Engine, installed by your distro's package manager |
| Windows 10/11 | Docker Desktop on WSL2 (a small Linux VM managed by Windows; hidden, not removed) |
| macOS | Docker Desktop (runs containers in its own Linux VM) |

App state and logs live in `~/.local/share/yulon/` (Linux), `%APPDATA%\yulon\` (Windows), or
`~/Library/Application Support/yulon/` (macOS). Your server files live wherever you chose at
install time (default `~/yulon-wotlk` for WotLK; each game has its own default folder).

Updates: on launch, Yu'lon checks GitHub Releases for a newer `-Public` release. **Update now**
downloads it and checks it against the release's `SHA256SUMS` before touching your install
([details](docs/updates.md)).

## FAQ

Short answers here; more in [docs/faq.md](docs/faq.md).

- **"Windows protected your PC" / Defender flags Yu'lon:** the builds are unsigned — there is no
  code-signing certificate yet, and none is promised. Check the file's SHA-256 against the
  release's `SHA256SUMS` (`Get-FileHash`), then More info → Run anyway. Never turn Defender off;
  if Defender quarantines Yu'lon, report it with the version. On macOS: right-click → Open.
- **Do I need internet?** Yes, to install and update (sources, Docker images, client data). Playing
  solo on your own machine runs locally.
- **Do I need my own game?** Yes. WotLK fetches server-side map data only, never the client you log
  in with.
- **Intel Mac?** No build — the `.dmg` is Apple silicon only. **AppImage won't start?** Take the
  `.tar.gz` instead (missing FUSE). **Why does Windows need a reboot?** WSL2.
- **Can friends join?** On your local network, via the Networking tab. Over the internet needs
  router port forwarding you set up yourself.
- **Is this piracy / affiliated with Blizzard, Capcom or Neople?** No — see
  [Legal](#legal) below.

## Legal

Yu'lon is not affiliated with, endorsed by, or associated with Blizzard Entertainment, Capcom, or
Neople. It distributes no game files: you supply your own legally obtained game client. Read
[DISCLAIMER.md](DISCLAIMER.md) before running a server.

## Contributing

Setup and conventions: [pyplan/contribution.md](pyplan/contribution.md). One rule that reaches
players: every change adds one line to `CHANGELOG.md` under `## Unreleased`. This README is written
on a branch and merged to `main` later; Yu'lon is [Project Yu'lon](pyplan/README.md), the plan it
implements.

## Community & links

- Releases (the downloads): <https://github.com/DadsMmoLab/dads-mmo-lab/releases>
- YouTube: [youtube.com/@DadsMmoLab](https://youtube.com/@DadsMmoLab)
- Support: [ko-fi.com/dadsmmolab](https://ko-fi.com/dadsmmolab)
- <!-- OWNER: Discord invite URL — paste the invite link here once approved; a Discord badge can go in the hero row then. -->
- <!-- OWNER: credits wording — who to name and how (founder, maintainer). -->

What the app does under the hood — evidence, per-OS run tables, rollback steps — stays in
[pylauncher/README.md](pylauncher/README.md), written for developers.
