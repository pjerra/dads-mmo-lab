# Changelog

Every release of Yu'lon, newest first. The **Unreleased** section is titled by the lead when a release is cut; until then it collects what has landed on the working branch since the last tag. Entries say what a player or operator can now do, and cite the gate folder under `pyplan/gates/` that proved it on a real machine — a line here without a folder behind it is a claim, not a change.

## Unreleased

_Everything below landed on `yulon-phase8b` after `v0.6.59Public` (cut from `Yulon` on 2026-08-29, `57c74600`). Phase 8 of the plan: the launcher operates a running server, not only installs one._

### Operate a running server
- **Observability** — the Server tab shows whether the world is up, restarting, or looping, from the container's own events rather than a poll; one kill reads as a restart, three read as a loop. Gated on WotLK, TBC, Vanilla and Tortoise. (`8.1a-…`, `8.1b-…`, `8.1c-…`, `8.1d-…`)
- **A command channel** — the app talks to the world through SOAP (AzerothCore, CMaNGOS) or the console (where a tree has no SOAP), sets it up on an existing install, repairs a refused channel, and says which of the two it is using. Tortoise joined the SOAP trees on its 2026-09-08 pin. (`8.2a-…` to `8.2e-…`, `tortoise-soap-yulon-arch-2026-09-09/`)
- **Accounts** — create, set a password, grant and revoke GM rank, through the tree's own writer; each tree's password scheme and rank scale measured rather than inherited. (`8.3a-…` to `8.3d-…`)
- **Play** — launch the game client at this server, revive a character, rename one online, on all four trees and on Windows. (`8.4a-…` to `8.4d-…`, `8.9a-wotlk-yulon-win11-…`)
- **Browse bots** — list the online bots the way the in-game who-list sees them, with a marker that refuses when blank and warns when it matches nothing. (`8.5a-…` to `8.5d-…`)
- **My Party (WotLK)** — a panel that adds a bot of a chosen class to your party through the server-side Lua bridge, shows the party as the group table reports it, and dismisses; every refusal in the app's own words. Proved with a real client and then through the panel itself. (`8.6-wotlk-yulon-ubuntu2-2026-09-09/`, `8.6-panel-live-yulon-ubuntu2-2026-09-09/`)
- **Modules** — how far behind each installed module is, apply an update, activate a module's config, and refuse module SQL aimed at a running world's databases; on the CMaNGOS trees a module is a configuration key or a SQL mod. (`8.7a-…` to `8.7d-…`)
- **A module from a link or a folder** — paste a repository link or point at a folder on this computer and it becomes an installed module through the same path the shipped manifests use, listed afterwards as custom. (`8.7e-module-from-link-m910q-2026-09-08/`)
- **Uninstall and purge** — one action scoped to the server folder, its Docker project and the launcher's record, with "Keep my characters"; the kept volume reopens with the recalled password. (`8.9a-…`, `8.9b-…`)
- **Rebuild with rollback** — a rebuild keeps the build it is about to overwrite, puts it back by itself when the new one does not come up, and says what it does not put back (the database). All three arms pressed live. A rebuild now renders the build recipe (Dockerfile and .dockerignore) again from this version's templates before it compiles, refuses a file that no longer carries the line Yu'lon wrote at its top, and puts both back as they were if it stops before the server is replaced -- unit-proven, the live press with a stale recipe still owed. (`rebuild-live-yulon-ubuntu2-2026-09-09/`, `rollback-restore-yulon-ubuntu2-2026-09-09/`)

### Fixed
- A log panel's Stop now reaches a child blocked in a quiet read; a worker driven on the GUI thread no longer quits the GUI thread's event loop. The 7.10 re-run's one clause that compared a plan to itself was corrected and watched failing; the box-restoring trap around it verifies every step it restores. (`7.10-rerun-ubuntu2-2026-09-09/`, `7.10-clause35-ubuntu2-2026-09-09/`)
- A rebuild waits for the realm line, not for the address a fresh install had — on a server whose address had changed it would otherwise wait six hours and restore the old build.
- The install-time ready wait asks the install for its realm pair instead of typing one.
- An account writer handed a scheme it does not know refuses by name -- password reset, account row, GM grant and GM read -- instead of writing AzerothCore's columns; an entry that declares no scheme is refused in the app, not defaulted. Unit-proven; no live folder.
- Tortoise: the fork's one self-colliding world migration is made idempotent at image build time; the reimport that upgrades an existing install is rehearsed, and its live procedure corrected after the first press.

### Known, named rather than hidden
- Every module the app installs is silent: a logger declared in a module's own config is not read by the server (bug-checklist 47).
- Tortoise: the fork's `character_updates/` directory is now a catalog phase, applied on a fresh install and re-applied on an install that already carries the import marker (idempotent files, no marker rewritten) -- but only through the engine's install route; the button that reaches an established install from the app is T14, not yet built. (`tortoise-upgrade-m910q-2026-09-09/`, the m910q press of the re-run route still owed)
- 8.8 (Steam / Deck) has no evidence yet; fullscreen Steam is the agreed stand-in.

## v0.6.59Public — 2026-08-29

The last release before this log existed. Cut from `Yulon` at `57c74600`; its contents are the commit history up to that tag.
