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
- `dml games list --json` → `{"games":[{"id","path","running","display_name"}]}`
- `dml games status <id> --json` → `{"id","state":"running"|"stopped"}`
- `dml games start|restart|stop <id> --json` → NDJSON stream
- `dml games catalog --json` → `{"titles":[{"id","name","installed","running","script_available","display_name","custom_name"}],"install_supported":<bool>}`
- `dml games name <id> --set "<name>"` → `{"id","name"}`
- `dml games name <id> --clear` → `{"id","name":null}`
- `dml version --json` → `{"version":"3.0.0"}`

**Display names.** A server's custom name lives in `<title dir>/.dml-name`
(UTF-8, first line) — WITH THE SERVER, not in launcher config: it is a property
of the server, so it survives a launcher reinstall, travels with the directory,
and is readable under either backend. `display_name` is what to render (custom
name → registry name → id, so a server is NEVER a blank label); `custom_name`
(catalog only) is the rename's own value, `null` when unset, which is what a
Rename dialog needs to tell "no name yet" from "named the same as the default".
`name` keeps meaning the built-in title name. Write rules: trimmed, non-empty,
max 40 characters, control characters/CR/LF REFUSED (`BAD_ARG`) rather than
stripped — the file is read back first-line-only, so accepting a newline would
silently store a different name. The reader is defensive about hand edits (first
line, control characters dropped, trimmed, capped at 40). The value is written
as a plain file body and never spliced into a shell command. `games name` needs
the title to be installed (`NOT_FOUND` otherwise) and validates the id
(`^[A-Za-z0-9._-]+$`, no `..`) before touching the filesystem.

`games catalog` answers TWO separate questions, and a consumer must not collapse
them. `script_available` is per title: is that title's installer script shipped
on this backend (`DML_INSTALLERS_DIR`, default `/usr/local/share/dml/installers`)?
`install_supported` is per HOST: can this machine run the installers at all? All
six are Linux scripts (sudo, pacman/apt, systemd, `usermod -aG docker`), so on a
Windows-native host running under Git Bash the answer is `false` and `games
install` refuses BEFORE the file check. Collapsing the two is the bug this field
was added to fix: on native every title reported `script_available:false` and the
launcher told the user to re-run a dev-install step that was already fine and
could never have helped. A `dml` predating the field omits it; treat ABSENT as
supported (fail open), never as blocked.

`dml games list` and `dml games status` are JSON-first: they always emit JSON,
even without `--json`. A JSON-mode `games start|stop|restart|status` call with
a missing title still emits a terminal error (NDJSON `error` event, or the
`status` envelope) with code `NOT_FOUND` — parser-side synthesis of a missing
terminal event is never needed.

`games start|restart` run `<compose_dir>/dml-start.sh <mode>` when present
(staged AzerothCore start that avoids re-running ac-db-import); otherwise
`docker compose up -d` / `down`.

While that hook runs, `games start|restart --json` **watches for a boot loop**.
The hook owns the whole readiness wait (up to `DML_READY_TIMEOUT_SECS`, 30 min
by default) and cannot tell a crash loop from a slow boot — on 2026-07-21 it
narrated ten minutes of `Can't connect to MySQL (110)` crash-retrying as "still
waiting … world is loading". Every `DML_BOOT_LOOP_POLL_SECS` (default 15) the
CLI reads `.State.RestartCount` for the world container **this title's compose
project owns**, and if it has climbed by 3 since the boot began it emits one
latched `warn` line (`boot loop detected: …`) naming the likely cause and the
Restart Docker action. Purely a diagnosis: the stream, the outcome and the exit
code are unchanged, and an unreadable count is evidence of nothing (it never
sets or resets the baseline). A title whose project owns no world container is
never watched, so a non-WoW start is never accused of the WoW world's loop. The
watch lives in the CLI rather than in `dml-start.sh` on purpose — that hook is
a deployed artifact no CLI update refreshes, so a fix inside it would only
reach fresh installs. Text mode is deliberately unwatched (frozen legacy-tray
output). Native mode arms the same watch in `games_lifecycle_stream`.

`games stop|restart` first write a bounded tail of the worldserver log to
`~/.dml/logs/world-<UTC ts>-<title>-<mode>.log` (a compose recreate destroys
the old container's log, which is how freeze evidence was lost twice during
the 2026-07-21 incident). Strictly best-effort: a failure is one `warn` line
and the stop continues. The container is resolved through the stopping title's
OWN compose project (`docker compose ps -a -q ac-worldserver` in its compose
dir), so a title whose project owns no world container — any non-WoW title —
is skipped silently even while the WoW stack is up; a bare `docker logs
ac-worldserver` would instead file the WoW log under the other title's name
and evict the real evidence from the shared retention pool. Both docker calls
are time-bounded (`DML_LOG_SNAPSHOT_TIMEOUT`, default 20s, for the read; 10s
for the resolution) — evidence capture must never block a stop. Retention
keeps the newest `DML_LOG_SNAPSHOT_KEEP` (default 10) `world-*.log` files and
never prunes the snapshot just written, so the reported name always exists;
`DML_LOG_SNAPSHOT_KEEP=0` turns the feature off entirely. Prunes are silent on
both surfaces. `games start` takes no snapshot — a cold start has no prior run
of its own container to preserve.

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
(the MySQL query against `ac-database` failed), `CHAR_ONLINE`
(`teleport-coords` refused to write an online character's position —
"Character must be logged out."), `EXISTS` (`preset-import` refused to
overwrite an existing preset without `--force`), and the five
`docker-restart` codes `NOT_SUPPORTED` / `NO_SUDO` / `RESTART_FAILED` /
`RESTART_TIMEOUT` / `DOCKER_STILL_DOWN` (see `docker-restart` below). `NOT_FOUND` and
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
- Every SOAP call is serialized under an `flock` on `~/.dml/soap.lock` — the
  lock lives inside the shared `soap_exec` helper, so every verb that talks
  SOAP inherits it. The worldserver console runs on a single thread, so the
  CLI never issues two commands concurrently.
- Mutations almost always go through a SOAP GM console command, never a
  direct database write. The MySQL access used by `items search`,
  `teleport-list`, `characters`, and `paperdoll` is **read-only**. Four
  direct MySQL writes are sanctioned project-wide: the pre-existing `lan`
  toggle's `realmlist` UPDATE, `backup restore`, `teleport-coords`'
  `characters.position_x/y/z/map/orientation` UPDATE (OFFLINE characters
  only — see below), and (new) `module repair`'s INSERT/DELETE on the
  `updates` tracking tables only — never game tables (see `module repair`
  below).
- Any value that ends up spliced into a SOAP console-command string
  (character names, item specs, teleport locations, mail subject/body) is
  allowlist-validated or sanitized first — an unvalidated value would be
  command-injection-equivalent (arbitrary console commands on the
  worldserver, i.e. RCE-equivalent). Character names must match
  `^[A-Za-z0-9_]{1,12}$`; teleport `--to` must be a single clean token
  (`^[A-Za-z0-9_-]+$`) — anything else is `BAD_ARG`; free-form text (mail
  `--subject`/`--body`) has embedded double quotes stripped and CR/LF
  replaced with a space before being quoted into the command. Both guards
  close the AC #2695 `.send items` class of bug, where an embedded newline
  in an argument can be read by the worldserver as a second console command.
  Note (live-confirmed): AC's modern command parser does **not** strip
  double quotes around name/location args — only `#subject`/`#text` style
  QuotedString args take quotes — so the CLI sends names and locations as
  bare tokens and relies on the allowlists above.
- Credentials come from environment variables, not flags: `DML_SOAP_USER` /
  `DML_SOAP_PASS` / `DML_SOAP_URL` (default `admin` / `admin` /
  `http://127.0.0.1:7878/`) and `DML_DB_ROOT_PASSWORD` (default `password`).

Note for GUI authors: `characters`, `teleport-list`, `paperdoll`, and
`config raw-read`/`config raw-write` only recognize their flag
(`--account`/`--search`/`--char`/`--file`) as the very first argument — a
first-arg-only check, not a `while`-loop flag parser like `items search`/
`mail-item`/`teleport`/`config set`. Put the flag anywhere else and it's
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
  Mutating: sends `teleport name <char> <to>` over SOAP (both tokens
  deliberately unquoted — see the parser note above). `--char` is validated
  the same way as `mail-item`'s `--to`. `--to` must be a single token
  matching `^[A-Za-z0-9_-]+$` (`BAD_ARG` otherwise); nearly all stock
  `game_tele` names fit, and the handful containing a space remain
  reachable via the server's partial-name match on their first word.
  **`--to` is not pre-validated against `game_tele`** — there is no
  existence check before the SOAP call, so an unknown/misspelled location
  surfaces as `SOAP_FAULT` from the worldserver, not a friendlier
  "location not found." **`--coords` on this verb is rejected as `BAD_ARG`**
  with a hint pointing at `teleport-coords` below.

- `dml wow teleport-coords --char <char> --map <id> --x <n> --y <n> --z <n> --json`
  → `{"teleported":true,"char":"<char>","map":N,"x":N,"y":N,"z":N}`
  Coordinate teleport for an **OFFLINE** character: writes
  `characters.position_x/y/z/map` (`orientation` reset to `0`) directly via
  MySQL (`_chars_write_stmt`, `30-db.sh`) — this is one of the four
  sanctioned direct writes (see the security posture note above), used
  instead of SOAP because AC's `teleport` console command only works on an
  online player. `--map` is 1-3 digits; `--x`/`--y`/`--z` are plain numbers
  with at most 5 integer digits and a magnitude cap of 20000 (`BAD_ARG`
  otherwise, checked before any SQL is built). The character is looked up
  first (`NOT_FOUND` if unknown); an **online** character is rejected as
  `CHAR_ONLINE` ("Character must be logged out.") — a live worldserver holds
  its own in-memory position and would clobber this write on the
  character's next auto-save/logout. Errors: `BAD_ARG`, `NOT_FOUND`,
  `CHAR_ONLINE`, `DB_UNREACHABLE`.

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

- `dml wow accounts --json` →
  `{"accounts":[{"id","username","characters":[{"guid","name","level"}]}]}`
  Read-only list of real player accounts and their characters (the GUI's
  character picker). Ambient-bot accounts (`RNDBOT*`) and `AHBOT` are
  filtered out; accounts with no characters (e.g. a SOAP-only account) come
  back with an empty `characters` array. Errors: `DB_UNREACHABLE`.

- `dml wow stats --json` →
  `{"population":{"family":{"total","online"},"bots":{"total","online"},"levels":[{"bucket","family","bots"}],"classes":[{"class","count"}],"factions":{"alliance","horde"},"top_levels":[{"name","level","family"}],"guilds":{"count","members"}},"economy":{"copper":{"total","family","bots"},"richest":[{"name","copper","family"}],"auction":{"count","buyout"},"mail":{"total","to_family"}},"journey":[{"name","level","class","playtime","last_seen","kills","achievements","quests"}],"history":{"boots","total_uptime","longest","peak","realm","recent":[{"start","uptime"}]},"botwatch":{"zones":[{"zone","count"}],"continents":[{"map","count"}],"playtime"}}`
  Read-only statistics envelope for the launcher's Statistics page — every
  number in one call (16 fixed-order queries, see `src/48-stats.sh`).
  "Family" = non-bot accounts excluding the `AHBOT`/`DMLSOAP` system
  accounts; bots via the `playerbots_account_type` idiom. Money fields are
  COPPER; times are seconds; `last_seen`/`start` are unix seconds. An empty
  DB answers zeros/empty arrays with `ok:true`; errors: `DB_UNREACHABLE`.

- `dml wow server-info --json` →
  `{"online","version","players","uptime","mean_ms","median_ms"}`
  Parsed `server info` over SOAP. A down/unreachable worldserver is
  `online:false` with `ok:true` — down is an answer, not an error — and a
  SOAP fault response is folded into the same `online:false` bucket; only
  bad credentials stay an error (`SOAP_AUTH`). Unparseable fields are
  `null`.

- `dml wow config list --json` →
  `{"settings":[{"key","group","label","explain","type","min","max","value",
  "default","restart_required","env"}]}`
  The curated settings registry with live values. Values are read from the
  wow title's `docker-compose.override.yml` environment (the write target is
  the source of truth); an unset key shows its default. `type` is one of
  `float|int|bool|text|char`; `value`/`default` are always JSON strings
  (bools are `"1"`/`"0"`; the AHBot seller character's value is the stored
  character GUID). `restart_required` is per-row: `true` for the eight
  env-backed settings, `false` for `server.motd` — its row's `env` is the
  sentinel `-` and its `value` is instead read live (read-only) from
  `acore_auth.motd` (realm 1), falling back to the registry default if the
  DB is down or the row is empty. Errors: `NOT_FOUND` (wow title not
  installed), `MISSING_DEP` (yq).

- `dml wow config set --key <k> --value <v> --json` →
  `{"changed":bool,"restart_required":bool}` (mirrors `changed`, like
  soap-setup). The value is validated against the registry (type + range) —
  `BAD_ARG` otherwise; unknown key is `NOT_FOUND`. For most keys this writes
  the mapped `AC_*` env var into the override via yq (same proven merge path
  as soap-setup; never a second top-level `services:` block). Special cases:
  `bots.population` writes BOTH `AC_AI_PLAYERBOT_MIN_RANDOM_BOTS` and
  `..._MAX_RANDOM_BOTS` to the one number; `ahbot.character` resolves the
  character name read-only to its guid+account and writes
  `AC_AUCTION_HOUSE_BOT_GUID` + `AC_AUCTION_HOUSE_BOT_ACCOUNT`
  (`NOT_FOUND` if no such character). `server.motd` is not env-backed at
  all — this AC build has no Motd conf/env key, so instead the CLI strips
  double quotes and CR/LF (replaced with a space) from the value and sends
  `server set motd 1 enUS <text>` over SOAP, applying it **instantly**:
  `{"changed":true,"restart_required":false}` on success. That call needs a
  *running* worldserver — `SOAP_UNREACHABLE` (hint: start it first) if it
  isn't, plus the usual `SOAP_AUTH`/`SOAP_FAULT` for this key only.
  Additional errors: `NOT_FOUND` (wow title not installed) and `MISSING_DEP`
  (yq) — the same preamble as `config list`; `DB_UNREACHABLE` for the
  `ahbot.character` branch when the character lookup can't reach the DB (or
  returns garbage).

- Direct conf route: `dml wow config set --key conf:<file>.conf:<Key> --value <v> --json`
  → `{"changed","restart_required","applied":"live"|"restart"|"none"}`.
  Writes one key of an editable **module** conf in place (comment-preserving;
  created from its `.dist` on first write). Any conf passing the dynamic
  module-conf allowlist (see `config files`) is accepted; `worldserver.conf`
  and `authserver.conf` stay curated-rows-only (`BAD_ARG`), unknown confs are
  `NOT_FOUND`. Keys are validated `^[A-Za-z0-9_.]+$`, values single-line and
  ≤200 chars; `AiPlayerbot.DeleteRandomBotAccounts` is denylisted (owned by
  `wow bots flush`). A matching legacy `AC_*` override is cleaned off
  override.yml on save. `applied:"live"` only when the module has a VERIFIED
  live-reload console command (currently mod-transmog → `transmog reload`,
  fired over SOAP best-effort) and no legacy env still beats the conf;
  everything else reports `"restart"`.

- `dml wow config pb-keys --json` → `{"source","keys":[{"key","value",
  "default","line"}]}` — every active `Key = value` line of playerbots.conf
  (falling back to its `.dist`), the Bot World all-keys browser.

- `dml wow config conf-keys --file <name>.conf --json` →
  `{"file","source":"conf"|"dist","keys":[{"key","value","default","line",
  "help"}]}` — pb-keys generalized to any editable module conf (the Module
  tuning per-module browsers). `default` comes from the `.dist` when both
  files exist; `help` is the key's comment-block doc parsed from the `.dist`
  (both the adjacent-block and the shared-doc-block-with-`#    Key.Name`-
  headers styles; collapsed to one line, capped at 400 chars, `""` when the
  author documented nothing). Rejects `.env`/the compose override/
  `worldserver.conf`/`authserver.conf` (`BAD_ARG`) and unknown confs
  (`NOT_FOUND`).

- `dml wow config tuning-list --json` / `tuning-set --key <module.knob>
  --value <v> --json` — the curated guided-tuning knobs (conf- and
  lua-backed); rows carry an additive `file` field naming their backing
  file. `module list` cpp rows likewise carry an additive `conf_name`, plus
  (module-update round) `head`/`head_date` — the installed clone's last
  commit (short sha + `YYYY-MM-DD`), both `null` when not installed / no
  `.git`; a local git read, offline (`module update-check` below owns
  fetching).

- `dml wow module update-check --json` → `{"repos":[{"label","url","branch",
  "head","dirty","behind"}]}` — the Modules page's "Check for updates"
  button: one git-fetch probe per installed cpp module clone (registry +
  custom, deduped by key like `module list`'s walk), same shape as the
  server-level `wow update-check`. Read-only (fetch only, never a
  pull/stash); `behind` is the commit count behind `origin/<branch>`, `null`
  when the fetch failed. lua/sql module families are absent — they aren't
  git checkouts under `modules/` (lua deploys copies, sql applies
  statements), so there's nothing to probe. Errors: `NOT_FOUND` (WoW
  Playerbots server not installed).

- `dml wow module update --key <mod-key> --json` → NDJSON stream, terminal
  `done` data `{"key","changed","before","after","pending_rebuild"}` — the
  Modules page's per-module Update button (offered only once
  `update-check` reports it behind): the same patch-backup + stash +
  ff-only-pull + stash-pop idiom as the server-level `wow update`, with
  every gate (key shape, installed, has `.git`, has an origin remote)
  checked before any mutation. No automatic rebuild — a changed pull marks
  the module rebuild-pending (the existing rebuild banner covers compiling
  it) **except** `mod-arac`, which ships no C++ (data-only: SQL + DBC +
  MPQ) and instead needs a client-patch + restart, never a rebuild.
  `mod-playerbots` always refuses (`BAD_ARG`) — it tracks the custom
  AzerothCore fork and updates together with the server core via `wow
  update`, never on its own. Errors: `BAD_ARG` (invalid/refused key),
  `NOT_FOUND` (server or module not installed), `GIT_MISSING` (module dir
  has no `.git`), `REMOTE_MISSING` (no origin remote).

- `dml wow config raw-read --file <name> --json` → `{"file","content"}` and
  `dml wow config raw-write --file <name> --json` (new content on stdin) →
  `{"written":true,"backup":"<name>.bak"|null}`
  The Advanced files editor. `<name>` must be one of `.env`,
  `docker-compose.override.yml`, `playerbots.conf`, `mod_ahbot.conf`,
  `mod_ale.conf` (`NOT_FOUND` otherwise — the literal-name allowlist is
  also the path-traversal guard; module confs are host files because the
  base compose bind-mounts `./env/dist/etc`). **`.env` and
  `docker-compose.override.yml` are read-only in `raw-write`** — both are
  still readable via `raw-read`, but overwriting either one, combined with
  `games restart`, would let this editor drive host command execution
  (env/volume/entrypoint injection into Docker Compose), so `raw-write`
  rejects them as `BAD_ARG` before touching the real file. Change those
  settings from the Settings tab (the curated `config set` keys) instead —
  `raw-write` only actually persists the three module confs
  (`playerbots.conf`, `mod_ahbot.conf`, `mod_ale.conf`). Every successful
  overwrite keeps a single-slot `.bak` of the previous content. The
  compose override is still YAML-validated on submit for diagnostic
  purposes — invalid YAML is `BAD_ARG` and the file is untouched (moot in
  practice now that the file can't be written at all, but keeps the error
  message specific if malformed content is ever submitted). Both verbs run
  the shared config preamble first, so `NOT_FOUND` (wow title not
  installed) and `MISSING_DEP` (yq) apply here too.

- `dml wow docker-restart --json` →
  `{"restarted":true,"waited_seconds":<int>}`
  Restarts the Docker **daemon** inside `dml-arch`
  (`sudo -n systemctl restart docker`). Incident follow-up 1 (2026-07-21):
  that outage was a wedged Docker network in the distro and the whole fix
  was this one command, which nothing in the launcher could click.
  **DESTRUCTIVE** — this is the daemon, not a container: every running
  container goes down with it, and dockerd only grants them its own short
  shutdown grace (the worldserver needs minutes to save a live world), so
  the GUI gates it behind a typed confirmation. One envelope, never a
  stream. Two properties are load-bearing:
  - **Every blocking call is bounded, so it always settles.** The CLI runs
    as the unprivileged `dml` user, so the restart goes through `sudo -n`
    (the tailscaled precedent): a box without the passwordless-sudo rule
    fails immediately with `NO_SUDO` instead of sitting on a password
    prompt with no tty while the GUI spins. That is not enough on its own:
    `docker.service` in `dml-arch` is `Type=notify` with
    `TimeoutStartSec=0`, so `systemctl restart docker` waits *indefinitely*
    for a `READY=1` that a dockerd wedged during startup never sends — the
    exact failure this button exists for. So the restart itself runs under
    `timeout` (90s; `DML_DOCKER_RESTART_CMD_TIMEOUT` overrides), the
    systemd probe under 10s, and each `docker info` poll under 5s (the
    socket is socket-activated, so it *accepts* and then blocks). A bound
    that is hit is a `RESTART_TIMEOUT` envelope — never a spinner that
    never settles.
  - **It waits for real evidence.** `systemctl` exiting 0 means the unit
    was *told* to restart — a daemon that stopped answering is exactly what
    brought the user here — so the arm then polls `docker info` until
    Docker answers again, bounded (30s; `DML_DOCKER_RESTART_TIMEOUT`
    overrides, and `DML_SYSTEMCTL_BIN` is a test-only seam for the
    no-systemd path). The budget is measured in *elapsed* time, so a probe
    that burns its own bound spends the budget instead of multiplying it.
    `waited_seconds` is how long that took.

  The systemd probe reads the *state* `systemctl is-system-running` prints,
  never its exit code: it exits nonzero for everything except `running`,
  and `degraded` (some unrelated unit failed at boot) is the normal state
  inside a WSL distro, so exit-code gating would refuse on most real boxes.
  Errors: `NOT_SUPPORTED` (no `systemctl`, or systemd is not running —
  nothing was restarted), `NO_SUDO` (no `sudo`, or `sudo -n` was refused),
  `RESTART_FAILED` (systemctl itself failed; the hint carries its stderr),
  `RESTART_TIMEOUT` (a bounded call was killed at its cap — the systemd
  probe or the restart command itself; the restart may still be in
  progress, so the hint says to re-check and offers Restart WSL),
  `DOCKER_STILL_DOWN` (restarted, but Docker never answered again within
  the wait), `BAD_ARG` (unknown flag).

  **No `dml-wow` (Rust) twin, by design.** `dml-wow` is the native Windows
  binary — it never runs inside `dml-arch`, and Windows has no systemd,
  no `sudo` and no `docker.service` to restart. The native answer to the
  same problem is a different action entirely (launch Docker Desktop, the
  launcher's `start_docker_desktop`). See "Deliberately not ported" in
  `docs/cli-contract.md`.

## party subcommands (My Party)

`dml wow party …` builds a playerbot party for a logged-in player via
SOAP-triggered Eluna bridge scripts (deployed by `bridge-setup`; `party-setup` is a legacy alias). Every op
needs the player's character **online**. Mutations go through the bridges
(`dml_addclass`/`dml_uninvite`/`dml_login`) over SOAP; reads are read-only
MySQL. Ambient random bots are excluded from `party online` and flagged in
`party list` via `acore_playerbots.playerbots_account_type`.

- `dml wow bridge-setup --json` → NDJSON stream, terminal `done` data
  `{"changed":bool,"restart_required":bool}`. `party-setup` and `setup`
  remain as aliases; deploys ALL bridge script families (party/, gm/) into
  `<server dir>/env/dist/etc/modules/lua_scripts/` and preflights SOAP.
  `restart_required` is true whenever any script changed — Eluna loads them
  at worldserver startup (this build has no live Lua reload), so a one-time
  restart is needed. Errors: `NOT_FOUND` (server not installed), `SOAP_AUTH`,
  `SOAP_UNREACHABLE`.
- `dml wow party online --json` → `{"online":[{"guid","name","class","level"}]}`.
  Read-only: human characters currently online (bots excluded). `class` is the
  numeric class id (1=Warrior … 11=Druid). Errors: `DB_UNREACHABLE`.
- `dml wow party add --player <name> --class <c> [--gender male|female] --json`
  → `{"added":true,"joined":bool,"bot":<name|null>,"note":<str|null>}`.
  `<c>` ∈ warrior/paladin/hunter/rogue/priest/shaman/mage/warlock/druid
  (`BAD_ARG` otherwise). Online-guarded (`NOT_FOUND` if the player isn't
  online). Fires `dml_addclass` then polls `group_member` (~6 s) for a new
  member: `joined:true` + the bot's name, or `joined:false` + a soft note.
  A `SOAP_FAULT` here usually means the bridge isn't loaded — run `bridge-setup`
  and restart. Errors: `BAD_ARG`, `NOT_FOUND`, `SOAP_AUTH`, `SOAP_FAULT`,
  `SOAP_UNREACHABLE`.
- `dml wow party list --player <name> --json` →
  `{"members":[{"guid","name","class","level","is_bot"}]}`. Read-only group
  members (empty if solo). Online-guarded. Errors: `BAD_ARG`, `NOT_FOUND`,
  `DB_UNREACHABLE`.
- `dml wow party kick --bot <name> --json` → `{"kicked":true}` (fires
  `dml_uninvite`). `dml wow party relogin --player <name> --bot <name> --json`
  → `{"relogged":true}` (fires `dml_login`). Names allowlisted
  `^[A-Za-z0-9_]{1,12}$`. Errors: `BAD_ARG`, `SOAP_AUTH`, `SOAP_FAULT`,
  `SOAP_UNREACHABLE`.

    dml wow party botcmd --player <name> --bot <name> --action gear|talents|maintain --json
    dml wow party preset-save   --player <name> --name <preset> --json
    dml wow party preset-list   --json
    dml wow party preset-delete --name <preset> --json
    dml wow party preset-load   --player <name> --name <preset> --json
    dml wow party preset-show   --name <preset> --json
    dml wow party preset-import --name <preset> --classes <c1,c2,...> [--force] --json

`botcmd` whispers a fixed command to the bot as if the player typed it
(`gear` → autogear, `talents` → talents autopick, `maintain` →
maintenance) — a closed allowlist; there is no free-text whisper.
Presets live under `~/.dml/party-presets/<name>` (one class name per
line). `preset-save` snapshots the LIVE party's bots (`overwrote:true`
when replacing). `preset-load` streams NDJSON and REPLACES the party:
kicks every current bot, then per saved class adds a bot, waits for the
join, and whispers `talents autopick` + `autogear` to the newcomer
(maintenance is deliberately not auto-run — it can walk bots to
trainers mid-load); `done` reports `{requested, joined}`.
`preset-show --name <preset> --json` → `{"name","classes":[...]}` reads a
preset file back (e.g. for a GUI export box) — `NOT_FOUND` if it doesn't
exist. `preset-import --name <preset> --classes <c1,c2,...> [--force] --json`
→ `{"imported":true,"name","classes":[...]}` writes a preset from a
comma-separated class list (the GUI's paste-in-a-list import) — every
token is validated against the SAME class allowlist as `party add
--class`/`preset-load` (`_valid_bot_class`, `50-party.sh`) **before
anything is written**, so one bad token leaves the file untouched.
An existing preset name without `--force` is rejected as `EXISTS` (file
untouched); with `--force` it's overwritten.
Errors: BAD_ARG (names/action/preset name/class token), NOT_FOUND (offline player/bot, unknown preset, party has no bots to save), EXISTS (preset-import without --force), DB_UNREACHABLE (party reads). botcmd can additionally raise SOAP_AUTH / SOAP_FAULT (bridge-setup hint) / SOAP_UNREACHABLE; preset-load never hard-fails on SOAP — kick/add/whisper failures become warn lines and the done payload just shows fewer joined (bridge not deployed => joined:0).

## gm subcommands (GM character tools)

    dml wow gm level    --player <name> --level <1-255> --json
    dml wow gm gold     --player <name> --gold <0-214748> --json
    dml wow gm heal     --player <name> --json
    dml wow gm revive   --player <name> --json
    dml wow gm summon   --player <name> --entry <1-999999> --json
    dml wow gm at-login --player <name> --flag rename|customize|changerace|changefaction --json

`level` uses the stock `.character level` command and works for OFFLINE
characters (absolute value — it can lower a level). `gold` (sets the total,
in whole gold), `heal` (100% HP) and `revive` (full HP, no resurrection
sickness) go through the `dml_gm_*` Eluna bridge and need the character
ONLINE (`NOT_FOUND` otherwise). Bridge ops need `bridge-setup` + one server
restart first (`SOAP_FAULT` with a bridge-setup hint until then).
`summon` temp-spawns the creature next to the ONLINE player (5-minute
self-despawn) after checking the entry exists in `creature_template`
(read-only) — unknown entry → `NOT_FOUND`; the payload carries the
creature's name.
`at-login` uses the stock `character <flag>` console command family
(`rename`/`customize`/`changerace`/`changefaction`, a closed allowlist —
`BAD_ARG` otherwise) to flag a character for that action at its **next
login**; works for OFFLINE characters too (same family as `.character
level` — no `.` prefix on this command family, though). Returns
`{"applied":true,"player","flag"}`.
Errors: `BAD_ARG` (name/range/flag), `NOT_FOUND` (offline character, or unknown creature entry for summon), `DB_UNREACHABLE` (summon's existence check), `SOAP_AUTH`, `SOAP_FAULT`, `SOAP_UNREACHABLE`.

## backup subcommands (whole-server snapshots)

    dml wow backup create  --json
    dml wow backup list    --json
    dml wow backup delete  --file <wow-YYYYMMDD-HHMMSS.sql.gz> --json
    dml wow backup restore --file <wow-YYYYMMDD-HHMMSS.sql.gz> --json

`create` dumps `acore_characters` + `acore_playerbots` + `acore_auth`
(`--single-transaction`, safe while the server runs) to
`~/.dml/backups/wow-<UTC>.sql.gz`, keeping the newest
`DML_BACKUP_KEEP` (default 10) and reporting every pruned file.
`restore` is the project's one sanctioned write path for whole CHARACTER-DB
snapshots (the LAN toggle's realmlist update, `teleport-coords`' position
update, and `module repair`'s updates-table INSERT/DELETE are the other
three sanctioned direct MySQL writes — see the security posture note under
`wow subcommands` above): it
stops ac-worldserver+ac-authserver, takes an automatic `-prerestore`
safety backup, imports the snapshot, and restarts the server. If the
import fails the server is deliberately LEFT STOPPED and the error names
the safety file. Errors: BAD_ARG (name), NOT_FOUND (missing backup/server),
DOCKER_DOWN, BACKUP_FAILED (dump/stop/import failures).
