# FAQ

- **"Windows protected my PC" / Defender flags Yu'lon.** The downloads are unsigned for now —
  there is no certificate. Check the file's SHA-256 against the release's
  `SHA256SUMS` (`Get-FileHash`), then More info → Run anyway. Never turn Defender off to run
  Yu'lon. If Defender quarantines it, report it with the version (Discord or the issue tracker).
  On macOS the same unsigned warning comes from Gatekeeper: right-click → Open.
- **Do I need internet?** Yes, to install and update: the install fetches sources and Docker
  images, and WotLK fetches its server-side map data. Playing solo on your own machine runs
  locally.
- **Do I need my own game?** Yes. Yu'lon distributes no Blizzard game files and no game client;
  the installs fetch open-source server software, and some installs (Centurion, Unbound) also
  fetch the client patch files or addons of their own makers. It cannot get you a client —
  WotLK's server-side data download is not the client, and you still supply that yourself.
- **Is there an Intel Mac build?** No. The `.dmg` is Apple silicon only.
- **The AppImage won't start?** Your distro ships no FUSE. Take the `.tar.gz` instead — see
  [Install](install.md#linux).
- **Why does Windows need a reboot?** Enabling WSL2 requires one; Yu'lon tells you when.
- **Can friends join my server?** On your local network: the Networking tab walks you through it.
  Over the internet needs router port forwarding you set up yourself — the app detects and
  explains the settings but cannot log in to your router.
- **Why is the GM console's Send button disabled?** The command box and Send are disabled when
  Yu'lon has no usable way to reach the server console here — the console needs a terminal
  (a pseudo-terminal on Linux and macOS, or a recorded WSL distro that opens one on Windows).
  The tab shows the reason, and following the log stays enabled.
- **Is this piracy? Is it affiliated with the game makers?** No. Yu'lon is not affiliated with
  or endorsed by Blizzard Entertainment or the makers of any game it can install a server for.
  Yu'lon distributes no Blizzard game files and no game client; the installs fetch open-source
  server software, and some installs (Centurion, Unbound) also fetch the client patch files or
  addons of their own makers. Read [DISCLAIMER.md](../DISCLAIMER.md) before running a server.
