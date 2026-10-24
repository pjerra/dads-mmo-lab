# FAQ

- **"Windows protected my PC" / Defender flags Yu'lon.** The downloads are not code-signed yet —
  there is no certificate, and none is promised. Check the file's SHA-256 against the release's
  `SHA256SUMS` (`Get-FileHash`), then More info → Run anyway. Never turn Defender off to run
  Yu'lon. If Defender quarantines it, report it with the version (Discord or the issue tracker).
  On macOS the same unsigned warning comes from Gatekeeper: right-click → Open.
- **Do I need internet?** Yes, to install and update: the install fetches sources and Docker
  images, and WotLK fetches its server-side map data. Playing solo on your own machine runs
  locally.
- **Do I need my own game?** Yes. Yu'lon never bundles, sells or distributes a game client, and
  it cannot get you one. WotLK's server-side data download is not the client — you still supply
  that yourself.
- **Is there an Intel Mac build?** No. The `.dmg` is Apple silicon only.
- **The AppImage won't start?** Your distro ships no FUSE. Take the `.tar.gz` instead — see
  [Install](install.md#linux).
- **Why does Windows need a reboot?** Enabling WSL2 requires one; Yu'lon tells you when.
- **Can friends join my server?** On your local network: the Networking tab walks you through it.
  Over the internet needs router port forwarding you set up yourself — the app detects and
  explains the settings but cannot log in to your router.
- **The GM console is disabled on Windows?** The console attaches over a pseudo-terminal, which
  the code only provides on POSIX. On Windows the command box and Send button are disabled with
  the reason shown; following the log stays enabled.
- **Is this piracy? Is it affiliated with Blizzard, Capcom or Neople?** No. Yu'lon builds
  open-source server software and distributes no game files. Read
  [DISCLAIMER.md](../DISCLAIMER.md) before running a server.
