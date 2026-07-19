# DML Launcher — Feature List

*A free, open-source Windows app for running your own WotLK private server
with ~2000 AI playerbots. One window, no terminal needed.*

**Server control**
- **Live server dashboard** — start, stop and restart with real-time status
  (players, uptime, latency, bots online), streamed logs, and a status light
  visible on every page. Restarts save all characters first and apply your
  settings changes.
- **Server console** — the live worldserver log in-app, plus a GM command
  line with history (send `.commands` like you would in-game).
- **Game installer** — install complete servers (WotLK Playerbots, Vanilla,
  TBC, MapleStory, RuneScape, Mu Online) by answering the installer's
  questions right in the app.
- **LAN play** — one click to let other PCs on your home network join your
  server.
- **Doctor & shell** — environment health checks and instant access to the
  server's Linux shell when you want it.

**Your characters**
- **Character sheet** — an in-game-style window: your gear with wowhead
  tooltips, a rotatable 3D model wearing your equipment, your class's three
  talent trees exactly as in-game, and the full achievement browser (1320
  achievements with categories, points and earned dates).
- **Teleport** — send any character to ~2000 named locations or exact
  coordinates.
- **GM tools** — revive, heal, set level or gold on any character; summon a
  banker/auctioneer/repair bot anywhere; rename or re-customize on next
  login.
- **Item mail** — search the item database and mail anything to any
  character.

**Playerbots**
- **~2000 AI bots** populate the world, level, trade and fill the auction
  house (via Dad's MMO Lab + mod-playerbots).
- **My Party** — build your own bot party by class, gear them, fix their
  talents, set their levels — and save whole party lineups as presets you
  can reload or share as text.

**Server customization**
- **37 server modules**, one click each, with descriptions and GitHub links:
  transmog, auction house bot, solocraft, autobalance (group-size scaling),
  1v1 arena, hardcore/iron-man modes, all-races-all-classes, mount at level
  1, and more. The app handles the server rebuild and config activation.
- **Settings with guardrails** — XP/gold rates, bot population, message of
  the day; safe ranges, applied automatically on restart.
- **Config editor** — full-window editor for module configs with automatic
  backups.
- **Accounts** — create logins for family/friends, set passwords and GM
  levels, delete safely.
- **Backups** — one-click snapshots of every character/account/bot, restore
  with an automatic safety net.
- **Self-updating** — pull the latest AzerothCore + playerbots source and
  rebuild from inside the app; Docker disk cleanup included.

**Get it**

```
git clone --branch feat/dml-launcher-windows https://github.com/pjerra/dads-mmo-lab.git
```

Requires Windows 10/11 with WSL2 and the Dad's MMO Lab server environment —
see the repo's launcher/README.md for setup.
