# dml CLI

Canonical source for the `dml` CLI that runs inside the `dml-arch` WSL distro
(and, on Linux/Steam Deck, any bash host). Built as a single file:

    bash build.sh        # cat src/*.sh > dml
    ./dev-install.ps1    # (Windows) install into dml-arch + print version

Bootstrap installs still come from Install-DML.ps1 (embedded v2.6.0); the DML
Launcher dev-installs this newer CLI over it. Do not edit `dml` directly —
edit `src/*.sh` and rebuild.

## Machine-readable contract (--json)

Add `--json` anywhere in argv. Two shapes:

**Envelopes** (single JSON object, one line):
- ok:    `{"ok":true,"data":{...}}` — exit 0
- error: `{"ok":false,"error":{"code":"NOT_FOUND","message":"...","hint":"..."}}` — exit 1

**NDJSON streams** (long-running commands: `games start|stop|restart`):
one JSON object per line — `section_start`, `line` (level: info|warn|error),
`section_end`, then exactly one terminal `done` (exit 0) or `error` (exit 1).
`pct` is reserved for installers.

Error codes: UNKNOWN_COMMAND, NOT_FOUND, NO_COMPOSE, DOCKER_DOWN,
START_FAILED, STOP_FAILED.

Commands:
- `dml games list --json` → `{"games":[{"id","path","running"}]}`
- `dml games status <id> --json` → `{"id","state":"running"|"stopped"}`
- `dml games start|restart|stop <id> --json` → NDJSON stream
- `dml version --json` → `{"version":"3.0.0"}`

`dml games list` and `dml games status` are JSON-first: they always emit JSON,
even without `--json`. A JSON-mode `games start|stop|restart|status` call with
a missing title still emits a terminal error (NDJSON `error` event, or the
`status` envelope) with code `NOT_FOUND` — parser-side synthesis of a missing
terminal event is never needed.

`games start|restart` run `<compose_dir>/dml-start.sh <mode>` when present
(staged AzerothCore start that avoids re-running ac-db-import); otherwise
`docker compose up -d` / `down`.

Tests: `bats tests/` inside the distro; `tests/windows-smoke.ps1` from Windows.

## wow subcommands

`dml wow <subcommand>` talks to an AzerothCore WotLK server (the
`wow-server-playerbots` title) two ways: mutating actions go through the
worldserver's SOAP console, read-only lookups query the `ac-database`
container's MySQL directly. All `wow` subcommands are JSON-first like
`games list`/`games status`: they always emit the `{"ok":...}` envelope
(success or error), whether or not `--json` is passed on the command line.

**Error codes** (in addition to the base list above): `MISSING_DEP` (a
required external tool, currently just `yq`, is not installed),
`BAD_ARG` (missing/invalid flag or value), `SOAP_FAULT` (the worldserver
rejected the console command — a SOAP fault body came back), `SOAP_AUTH`
(SOAP HTTP Basic auth failed — HTTP 401), `SOAP_UNREACHABLE` (curl could
not reach the SOAP endpoint — connection refused, timeout), `DB_UNREACHABLE`
(the MySQL query against `ac-database` failed). `NOT_FOUND` and
`UNKNOWN_COMMAND` are reused from the base list.

**Security posture:**
- SOAP is unreachable from the LAN. The worldserver's *in-container* bind
  (`AC_SOAP_IP=0.0.0.0`, set by `soap-setup`) is required for Docker's
  port-publish NAT to route to it at all — that `0.0.0.0` is internal to the
  container network, not a host-facing bind. The setting that actually
  controls host reachability is the *published* port: `soap-setup` pins
  `DOCKER_SOAP_EXTERNAL_PORT=127.0.0.1:7878` in `.env`, so Docker only maps
  the port onto the host's loopback interface — nothing on the LAN can reach
  it. Auth is HTTP Basic.
- Every SOAP call (`soap-exec`, `mail-item`, `teleport`) is serialized under
  an `flock` on `~/.dml/soap.lock` — the worldserver console runs on a single
  thread, so the CLI never issues two commands concurrently.
- Mutations always go through a SOAP GM console command, never a direct
  database write. The MySQL access used by `items search`, `teleport-list`,
  `characters`, and `paperdoll` is **read-only**.
- Any value that ends up spliced into a SOAP console-command string
  (character names, item specs, teleport locations, mail subject/body) is
  allowlist-validated or sanitized first — an unvalidated value would be
  command-injection-equivalent (arbitrary console commands on the
  worldserver, i.e. RCE-equivalent). Character names must match
  `^[A-Za-z0-9_]{1,12}$`; free-form text (teleport `--to`, mail
  `--subject`/`--body`) has embedded double quotes stripped and CR/LF
  replaced with a space before being quoted into the command — this closes
  the AC #2695 `.send items` class of bug, where an embedded newline in an
  argument can be read by the worldserver as a second console command.
- Credentials come from environment variables, not flags: `DML_SOAP_USER` /
  `DML_SOAP_PASS` / `DML_SOAP_URL` (default `admin` / `admin` /
  `http://127.0.0.1:7878/`) and `DML_DB_ROOT_PASSWORD` (default `password`).

Note for GUI authors: `characters`, `teleport-list`, and `paperdoll` only
recognize their flag (`--account`/`--search`/`--char`) as the very first
argument — a first-arg-only check, not a `while`-loop flag parser like
`items search`/`mail-item`/`teleport`. Put the flag anywhere else and it's
silently ignored (treated as if omitted), rather than rejected.

**Commands:**

- `dml wow soap-setup --json` → `{"changed":bool,"restart_required":bool}`
  Enables SOAP on the WoW Playerbots title: sets
  `DOCKER_SOAP_EXTERNAL_PORT=127.0.0.1:7878` in that title's `.env` (pins the
  base compose file's existing port mapping to localhost — it never adds a
  second `ports:` entry, since Compose concatenates `ports:` lists across
  base+override) and merges `AC_SOAP_ENABLED=1` / `AC_SOAP_IP=0.0.0.0` /
  `AC_SOAP_PORT=7878` into `docker-compose.override.yml` via `yq`.
  Idempotent — a second run with nothing to change reports
  `changed:false, restart_required:false`. Requires `yq` (mikefarah v4)
  inside dml-arch; errors `MISSING_DEP` if it's missing. Errors `NOT_FOUND`
  if the WoW Playerbots title isn't installed. `restart_required` is `true`
  whenever `changed` is `true` — the running worldserver must be restarted
  for the new SOAP settings to take effect.

- `dml wow soap-exec "<console command>" --json` → `{"result":"<text>"}`
  Runs an arbitrary GM console command over SOAP and returns its result text
  verbatim. No argument sanitization is applied here — the caller supplies
  the whole command, so this endpoint is only as trusted as the SOAP
  account itself. Errors: `SOAP_FAULT`, `SOAP_AUTH`, `SOAP_UNREACHABLE`.

- `dml wow items search --name <text> [--quality N] [--min-level N] [--max-level N] [--limit N] --json`
  → `{"items":[{"entry","name","quality","item_level","required_level","class","subclass","inventory_type","displayid"}]}`
  Read-only query against `acore_world.item_template`. `--name` is
  **required** and must be non-empty — an empty name would otherwise
  silently fall through to browsing the whole table — `BAD_ARG` otherwise.
  `--name` is matched with `LIKE '%...%'`, SQL-escaped (same treatment as
  `teleport-list`'s `--search`). `--quality`/`--min-level`/`--max-level`/
  `--limit` must be pure digits (or omitted); any non-numeric value is
  `BAD_ARG` (these are inlined unquoted into the SQL, so this doubles as the
  injection guard). `--limit` defaults to 50. Icons are **not** included —
  `displayid` is the raw display id; turning that into an icon path needs
  client DBC (`ItemDisplayInfo.dbc`) enrichment, which is a follow-up, not
  built here. Errors: `BAD_ARG` (missing/invalid `--name` or a non-numeric
  flag), `DB_UNREACHABLE` (if `ac-database` can't be queried).

- `dml wow mail-item --to <char> --items <id:count>[,<id:count>...] [--subject <s>] [--body <s>] --json`
  → `{"sent":true,"to":"<char>","attachments":N}`
  Sends items via the SOAP `.send items` GM command — mutating, so it goes
  through SOAP, never a direct database write. `--to` must match
  `^[A-Za-z0-9_]{1,12}$`. `--items` takes 1-12 comma-separated `id:count`
  pairs (each `^[0-9]+:[0-9]+$`); 0 or more than 12 is `BAD_ARG`.
  `--subject`/`--body` default to `"Dad's MMO Lab"` / `"Enjoy!"`; both are
  sanitized (quotes stripped, CR/LF replaced with a space) before being
  spliced into the console command. Errors: `BAD_ARG` (invalid `--to`,
  malformed/out-of-range `--items`, or an unknown flag), `SOAP_FAULT`,
  `SOAP_AUTH`, `SOAP_UNREACHABLE`.

- `dml wow teleport-list [--search <text>] --json` →
  `{"locations":[{"name","x","y","z","map"}]}`
  Read-only query against `acore_world.game_tele` (up to 500 rows, ordered
  by name). The filter flag is `--search` (not `--name`) and does a
  `name LIKE '%...%'` match, SQL-escaped. Errors `DB_UNREACHABLE` if
  `ac-database` can't be queried.

- `dml wow teleport --char <char> --to <location> --json` →
  `{"teleported":true,"char":"<char>","to":"<location>"}`
  Mutating: sends `teleport name "<char>" "<to>"` over SOAP. `--char` is
  validated the same way as `mail-item`'s `--to`. `--to` is free-form
  (`game_tele` location names aren't restricted to the charname allowlist)
  but has embedded quotes stripped and CR/LF neutralized to a space before
  being spliced into the console command (same guard as `mail-item`).
  **`--to` is not pre-validated against `game_tele`** — there is no
  existence check before the SOAP call, so an unknown/misspelled location
  surfaces as `SOAP_FAULT` from the worldserver, not a friendlier
  "location not found." **`--coords` is deferred**: it always returns
  `BAD_ARG` ("Coordinate teleport is not available yet") — coordinate-based
  teleport needs an offline DB path that isn't built yet.

- `dml wow characters --account <name> --json` →
  `{"characters":[{"guid","name","level","class","race","gender","gold"}]}`
  Read-only: resolves the account id from `acore_auth.account`, then lists
  `acore_characters.characters` for that account, ordered by level
  descending. `gold` is `money/10000` (copper→gold, integer-truncated — any
  silver/copper remainder is dropped, not reported separately). Errors:
  `BAD_ARG` (missing `--account`), `NOT_FOUND` (no such account),
  `DB_UNREACHABLE` (either query fails).

- `dml wow paperdoll --char <name> --json` →
  `{"name","level","class","gold","note":"last_saved","equipped":[{"slot","entry","name","quality","item_level","displayid"}]}`
  Read-only: joins `characters` + `character_inventory` (bag 0, slots 0-18 —
  the equipped-gear slots) + `item_instance` + `acore_world.item_template`.
  `note:"last_saved"` is a standing caveat, not an error: the row reflects
  the characters table as of its last DB save, which can lag an online
  character's true live state until their next auto-save/logout — a
  live-accurate view would need a SOAP `.pinfo`-style call, not built here.
  Each equipped item's `displayid` is likewise the raw display id, not an
  icon (same caveat as `items search` — client DBC enrichment is a
  follow-up, not built here). Errors: `BAD_ARG` (invalid `--char`),
  `NOT_FOUND` (no such character or nothing equipped), `DB_UNREACHABLE`.
