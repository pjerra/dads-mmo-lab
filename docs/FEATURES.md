# DML Launcher — Feature List

*A free, open-source Windows app for running your own WotLK private server
with ~2000 AI playerbots. One window, no terminal needed.*

**Server control**
- **Live server dashboard** — start, stop and restart with real-time status
  (players, uptime, latency, bots online), streamed logs, and a status light
  visible on every page. Restarts apply your settings changes and save all
  characters first (there's an optional toggle for a faster restart — it's
  still safe, a normal shutdown always saves).
- **Server console** — the live worldserver log in-app, plus a GM command
  line with history and autocomplete (send `.commands` like you would
  in-game).
- **Game installer** — install complete servers (WotLK Playerbots, Vanilla,
  TBC, MapleStory, RuneScape, Mu Online) by answering the installer's
  questions right in the app.
- **LAN play** — one click to let other PCs on your home network join your
  server.
- **Play together over the internet** — friends outside your home network can
  join too, either through a guided router-forwarding setup or, even easier,
  over Tailscale with no router setup at all.
- **Auto-stop** — optionally have the server stop itself automatically when
  you close WoW, so it's never left running by accident.
- **Doctor & shell** — environment health checks, network/database
  troubleshooting tools, and instant access to the server's Linux shell when
  you want it.

**Your characters**
- **Character sheet** — an in-game-style window: your gear with wowhead
  tooltips, a rotatable 3D model wearing your equipment, your class's three
  talent trees exactly as in-game, and the full achievement browser (1320
  achievements with categories, points and earned dates). Pick a character
  and it all loads on its own.
- **Teleport** — send any character to ~2000 named locations or exact
  coordinates.
- **GM tools** — revive, heal, set level or gold on any character; send a
  stuck character home; summon a banker/auctioneer/repair bot anywhere;
  rename or re-customize on next login.
- **Item mail** — search the item database and mail anything to any
  character.
- **Gear sets** — save a character's full outfit as a reusable preset, mail
  whole sets to other characters, or export/import a set as text to trade
  with friends.

**Playerbots**
- **~2000 AI bots** populate the world, level, trade and fill the auction
  house (via Dad's MMO Lab + mod-playerbots).
- **My Party** — build your own bot party by role, class and exact spec,
  gear them, fix their talents, set their levels, see which ones are online
  at a glance — and save whole party lineups as presets you can reload or
  share as text.
- **Browse Bots** — search and star favorites among all ~2500 world bots,
  and peek at any bot's gear, talents and achievements.

**Server customization**
- **38 server modules**, one click each, with descriptions and GitHub links:
  transmog, auction house bot, solocraft, autobalance (group-size scaling),
  1v1 arena, hardcore/iron-man modes, all-races-all-classes, mount at level
  1, and more. The app handles the server rebuild and config activation, and
  can drop a module's extra NPC straight into both capital cities for you.
- **Settings with guardrails** — XP/gold rates, bot population, message of
  the day; sensible defaults and safe ranges shown for every setting, with a
  one-click reset.
- **Module tuning** — every installed server module with a config file gets
  its own card: friendly switches for the popular knobs first, then a
  searchable "All settings" list of every key the module knows, with the
  module author's own notes shown inline. Transmog changes even apply live,
  no restart. Lua script mods keep their simple curated switches (like
  auto-refilling ammo) — no raw script editing.
- **Account-wide sharing** — once installed, share things like achievements,
  gold, mounts, pets and titles across every character on the same account.
- **Config editor** — full-window editor for module configs with automatic
  backups.
- **Accounts** — create logins for family/friends, set passwords and GM
  levels, delete safely.
- **Backups** — one-click snapshots of every character/account/bot, a quick
  summary of what's inside each one, a health check you can run before
  trusting a backup, and restore with an automatic safety net.
- **Self-updating** — pull the latest AzerothCore + playerbots source and
  rebuild from inside the app; Docker disk cleanup included.

*This is under active development. The core stuff — starting/stopping the
server, characters, teleporting, GM tools, mailing items, your bot party —
is up and running. A lot of the rest (installing titles/modules, backups,
accounts, LAN/internet play, and all the newest features above) is built but
still being tested by hand, and unlocks gradually as each piece is verified.*

**Get it**

```
git clone --branch feat/dml-launcher-windows https://github.com/pjerra/dads-mmo-lab.git
```

Requires Windows 10/11 with WSL2 and the Dad's MMO Lab server environment —
see the repo's launcher/README.md for setup.
