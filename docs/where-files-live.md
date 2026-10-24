# Where files live

## App state and logs

Yu'lon keeps its state (remembered server paths, cached manifests, the log file) in the
platform's conventional per-user data directory:

- Linux: `~/.local/share/yulon/` (honors `XDG_DATA_HOME` if set)
- Windows: `%APPDATA%\yulon\`
- macOS: `~/Library/Application Support/yulon/`

This is app state, not server data.

## Your servers

Server files stay wherever you chose at install time — never inside the app folder. The default
for each game is `~/yulon-<game>` (`~/yulon-wotlk` for WotLK, `~/yulon-tbc` for TBC,
`~/yulon-vanilla`, `~/yulon-tortoise`, `~/yulon-centurion`, `~/yulon-unbound`, `~/yulon-ddon`,
`~/yulon-dfo`); a server installed by an earlier version keeps its folder.

## How the servers run

Game servers run as Docker containers, and Docker needs a Linux kernel. Yu'lon hides that for
you, but it does not remove it:

| You are on | What runs the servers |
|---|---|
| Linux | Docker Engine, via your distro's package manager |
| Windows 10/11 | Docker Desktop on WSL2 — a small Linux VM managed by Windows; hidden, not removed |
| macOS | Docker Desktop — containers run in its own Linux VM |

Things the app asks you for, in plain words:

- **A password** when your system needs one to install software (Linux `sudo`, Windows UAC,
  macOS).
- **A reboot** after enabling WSL2 on Windows.
- **Router settings** for internet play (a DHCP reservation and TCP port forwarding) — the app
  detects and explains them; it cannot log in to your router.
- **Your own game client.** Yu'lon never bundles, sells or distributes a game client or game
  files. Two details worth stating plainly: WoW TBC, Vanilla and Tortoise ask you to point at a
  client folder you already have, and WotLK does not — its installer never asks, because the
  server fetches AzerothCore's own client-data archive (maps, vmaps, mmaps, DBC) into a Docker
  volume. That is server-side data the server cannot run without. It is not the client you log
  in with, and you still have to supply that yourself.
