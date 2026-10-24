# Install

One download per OS from the [releases page](https://github.com/DadsMmoLab/dads-mmo-lab/releases).
The file names carry the version: `Yulon-<version>-x86_64.AppImage`,
`Yulon-<version>-x86_64.tar.gz`, `Yulon-<version>-windows-x64.zip`,
`Yulon-<version>-macos.dmg`, plus `SHA256SUMS` to check them against.

## Windows 10/11

1. Download `Yulon-<version>-windows-x64.zip` and unzip it to a folder you own.
2. Run `yulon.exe`. SmartScreen warns because the builds are unsigned — check the file's SHA-256
   against the release's `SHA256SUMS` (`Get-FileHash`), then More info → Run anyway.
3. Yu'lon sets up WSL2 + Docker Desktop for you. It asks for UAC, and Windows needs one reboot
   after WSL2 is enabled — Yu'lon tells you when.

## Linux

1. Download the `Yulon-<version>-x86_64.AppImage`, make it executable, run it.
2. Yu'lon installs Docker Engine via your distro's package manager (asks for sudo).

If the AppImage refuses to start with `Error: No suitable fusermount binary found`, your distro
ships no FUSE (Arch installs neither `fuse2` nor `fuse3` by default). Take the
`Yulon-<version>-x86_64.tar.gz` instead — same build, needs no FUSE:

```bash
tar -xzf Yulon-*-x86_64.tar.gz && ./yulon/yulon
```

Or run the AppImage with `--appimage-extract-and-run`, or install a FUSE helper
(`sudo pacman -S fuse2` on Arch).

### Steam Deck

The Deck takes the Linux files above. After downloading, add Yu'lon to your Steam library as a
non-Steam game. Yu'lon briefly unlocks SteamOS's read-only filesystem (`steamos-readonly disable`)
to install Docker Engine, then relocks it.

## macOS (Apple silicon only)

1. Download the `Yulon-<version>-macos.dmg`, open it, drag Yu'lon to Applications.
2. Right-click → Open (Gatekeeper warns because the builds are unsigned).
3. Install Docker Desktop if asked — containers run in its Linux VM.

There is no Intel Mac build, and only WotLK and Unbound install on macOS (see the games table in
the [front page](../README.md#supported-games)).

## First run

Pick a game from the Catalog and click **Install**. The app builds the open-source server software
in Docker and gives you Server, Console, Accounts, Modules and Networking tabs.

- You bring your own game client — Yu'lon never downloads or ships one. WotLK fetches
  server-side map data only, not the client you log in with.
- Friends on your local network join via the Networking tab; internet play needs router port
  forwarding you set up yourself.
