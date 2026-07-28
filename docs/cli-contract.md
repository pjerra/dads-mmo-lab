# dml-wow CLI contract (`dml-json-v3`)

As of the `rust-main` branch (renamed from `feat/rust-cli-workspace` 2026-07-28).

`dml-wow` is a clap 4 binary (crate `crates/dml-wow-cli`, library `crates/dml-wow`, shared
plumbing `crates/dml-core`) that drives a WoW (AzerothCore + mod-playerbots) private server and
prints machine-readable JSON on stdout. Any frontend — the Tauri launcher, an Electron app, a
plain script — attaches by spawning the binary and parsing stdout. There is no IPC, no socket,
no config file: stdout is the entire interface, environment variables are the entire
configuration surface.

The bash CLI (`cli/src/*.sh`, contract documented in `cli/README.md`) is the oracle this port
was written against; where the two emit different bytes, the JSON content is identical and this
document describes the Rust binary.

Contract identity: `dml-wow version` emits
`{"version": <crate version>, "contract": "dml-json-v3", "backend": "native"}` — gate on the
`contract` string, not on the crate version.

- stdout: machine contract (envelopes / NDJSON). Parse it as JSON, never byte-match.
- stderr: human side channel, never part of the machine contract.
- Exit code: part of the contract (see the exit-code table).

## 1. Envelopes

Every non-streaming subcommand prints **exactly one** JSON envelope on stdout.
**Exit 0 iff `ok` is `true`.** Exit 2 means a clap usage error, which still prints a machine
envelope (code `BAD_ARGS`) on stdout plus clap's full error text on stderr.

Success — an object with `"ok": true` and `"data": <any JSON value>`; no `error` key:

```json
{"data":{"backend":"native","contract":"dml-json-v3","version":"..."},"ok":true}
```

Failure — `"ok": false` and an `error` object with exactly three string fields; no `data` key:

```json
{"error":{"code":"BAD_ARG","hint":"1-9 or 11.","message":"Invalid class id: 42"},"ok":false}
```

Usage error (exit 2) — `message` is the first line of clap's error text:

```json
{"error":{"code":"BAD_ARGS","hint":"dml-wow --help","message":"..."},"ok":false}
```

Rules:

- `hint` may be `""`. A tolerant parser should default a missing `hint` to `""` (the reference
  parser in `dml-core` does).
- **Key order is NOT part of the contract.** This workspace's serde_json builds without
  `preserve_order`, so Rust emits keys alphabetically (`{"data":...,"ok":true}`) while the bash
  oracle prints `{"ok":true,"data":...}`. Same JSON, different bytes. Parse; never compare raw
  lines.
- String content is standard JSON escaping. (The bash oracle escapes backslash first, then
  quote, `\n`, `\r`, `\t`, and strips remaining ASCII control characters; serde_json produces
  equivalent output for free.)
- The two exceptions to "one envelope per invocation": `install` (raw stdio passthrough — no
  envelope at all on success) and clap usage errors (envelope on stdout + clap text on stderr,
  exit 2). Both are described below.
- `--help` and `--version` exit 0 via clap's own output; no envelope.

## 2. NDJSON streams

Long-running commands print an NDJSON event stream instead of one envelope: one compact JSON
object per line, flushed per line. Exactly 14 subcommands stream (marked in the command table):
`start`, `stop`, `restart`, `backup create`, `backup restore`, `docker-clean`, `bots-flush`,
`games-remove`, `self-update`, `module install`, `module remove`, `module update`,
`module rebuild`, `party preset-load`.

Event vocabulary (one verbatim wire example each, Rust key order):

| Event | Example line |
|---|---|
| `section_start` | `{"event":"section_start","name":"compose-up"}` |
| `line` | `{"event":"line","level":"info","text":"Starting containers"}` |
| `section_end` | `{"event":"section_end","name":"compose-up","status":"ok"}` |
| `done` (terminal) | `{"data":{"state":"running"},"event":"done"}` |
| `error` (terminal) | `{"error":{"code":"CLI_CRASH","hint":"Check WSL: wsl -d dml-arch","message":"dml exited with code 3 before finishing"},"event":"error"}` |

Terminal-event rule:

- An event is terminal iff its `event` field is `"done"` or `"error"`. The stream **ends** at
  its terminal event — nothing is written after it. A consumer may close the pipe there.
- Stream ends with `done` → exit 0. Stream ends with `error` → exit 1.
- A stream that dies with **neither** is a crash: the process exits 1 ("silently dying without
  done/error is itself a failure to report"). **Treat the exit code as truth**, and derive
  success/failure from the terminal event, never from process/promise state alone.
- Failure is sticky: once an `error` event has been observed, a later `done` can never flip the
  exit code back to 0 (protects multi-stream arms like `start`/`stop` that drive several
  internal streams onto one tracker).
- `stop` buffers its terminal event and re-emits it **last**, so engine-stop `line` events never
  follow the terminal event.
- Confirmation gates and the CLI's own argument checks run **before** the stream opens — those
  rejections are one ordinary error envelope + exit 1, never a half-emitted stream. That covers
  the four `--yes` gates, the `start`/`stop`/`restart`/`install` game-id rule, and
  `party preset-load`'s name guards. **Library-owned** validation is different: it is emitted
  *inside* the stream as `section_start` + `error` events (e.g. `docker-clean --level 9 --yes`,
  `games-remove` with an unknown title, the module family allowlist and backup-flag rules,
  `backup restore`'s name/existence checks). Either way the stream still terminates in an
  `error` event and exit 1 — a consumer that treats a streaming command as a stream is
  unaffected — but do not expect every rejection to arrive as a bare envelope.
- Unknown event types must be ignored by consumers, never crash on them.

Section-name constants live with each owning module in `dml-wow` (e.g. the module manager), not
in `dml-core`. Not yet documented: the full inventory of section names per streaming command.

### Consuming through `dml-core` (what the launcher does)

`dml_core::runner::DmlRunner::run_stream`, as a consumer of a child CLI's stream, adds two
synthesized behaviors a hand-rolled consumer should copy:

- Child exits non-zero with no terminal event seen → it synthesizes
  `{"event":"error","error":{"code":"CLI_CRASH","message":"dml exited with code {code} before finishing","hint":<runner host hint>}}`.
- Non-JSON stdout lines are wrapped as `{"event":"line","level":"warn","text":<raw line>}`
  rather than dropped.

For envelopes, `dml_core::envelope::parse_envelope` on non-JSON input returns
`Err("unparseable dml output ({e}): {raw}")`, and `envelope_to_result` maps `ok=false` with no
`error` object to a synthesized code `CLI_BAD_OUTPUT`, message `ok=false with no error object`,
hint `""`. `CLI_CRASH` and `CLI_BAD_OUTPUT` are consumer-side codes — `dml-wow` itself never
prints them.

## 3. Exit codes

| Code | Meaning |
|---|---|
| 0 | ok envelope; or a stream that ended in `done`; or `--help`/`--version`; or a broken pipe mid-write (consumer closed the pipe — always exit 0, see caveats) |
| 1 | error envelope; or a stream that ended in `error`; or a stream that ended with **no** terminal event; or a non-pipe stdout write failure (after a best-effort stderr line `dml-wow: stdout write failed: {msg}`) |
| 2 | clap usage error (bad flags / unknown subcommand) — `BAD_ARGS` envelope on stdout, clap text on stderr |
| (install) | `dml-wow install` exits with the child installer's exit code verbatim (`status.code().unwrap_or(1)`) — see caveats for the collision with 2 |

## 4. Commands

74 subcommands, in definition order (`crates/dml-wow-cli/src/cli.rs`, dispatch in
`src/run.rs`). Output: **envelope** = one JSON envelope; **stream** = NDJSON;
**passthrough** = inherited stdio, no JSON.

| Command | Arguments | Output | Description |
|---|---|---|---|
| `version` | — | envelope | CLI + contract version (`{"version":...,"contract":"dml-json-v3","backend":"native"}`) |
| `status` | — | envelope | Full server status (containers, SOAP, bots, ports) |
| `server-info` | — | envelope | SOAP server-info fields only |
| `console-tail` | `[--lines <N>]` u32, clap-ranged `1..=1000`, default 200; out-of-range is a clap usage error (exit 2) | envelope | Last worldserver console lines |
| `config list` | — | envelope | Every curated setting with its live value |
| `config get` | `<KEY>` registry key, e.g. `rates.xp_kill`; unknown key → `NOT_FOUND` | envelope | One curated setting with its live value (no bash equivalent) |
| `config set` | `<KEY> <VALUE>`; a `conf:<file>.conf:<Key>` key takes the direct module-conf route | envelope | Change one setting |
| `config registry` | — | envelope | The static registry only — no values read, no files touched |
| `config files` | — | envelope | Which files the raw editor may open |
| `config read` | `<NAME>` file name as reported by `config files` | envelope | Print one editable file's contents (bash: `config raw-read`) |
| `config write` | `<NAME>` — body read from **stdin**; `.env` and `docker-compose.override.yml` are rejected (library allowlist) | envelope | Overwrite one editable file (bash: `config raw-write`) |
| `tuning list` | — | envelope | Every tuning knob with its live value + installed state |
| `tuning set` | `<KEY> <VALUE>`, e.g. `sitmeansrest.duration` | envelope | Change one tuning knob (conf-backed or lua-backed) |
| `module list` | — | envelope | Every module with its live install/deploy/rebuild state |
| `module catalog` | — | envelope | The static catalog only — no state read, no files touched |
| `module install` | `--family <cpp\|lua\|sql>` (required); `[--key <KEY> \| --url <URL>]` (mutually exclusive, enforced in-library); `[--variant <V>]`; `[--backup \| --no-backup]` (see BackupChoice note) | stream | Install (or pull) one module |
| `module remove` | `--family <F> --key <KEY>` (both required); `[--backup \| --no-backup]` | stream | Remove one module |
| `module update` | `--key <KEY>` (required) | stream | `git pull` one installed C++ module |
| `module rebuild` | `[--backup \| --no-backup]`; neither → library `BAD_ARG` "Pick --backup or --no-backup" | stream | Recompile the server with pending module changes (30–90 min cold) |
| `module repair` | `--key <KEY>` (valid cpp key) `--db <world\|characters\|auth>` `--mode <mark\|clear>` (all checked in run.rs — load-bearing, the library panics otherwise) `[--files <FILES>]` (space-separated .sql names; omitted = discover) | envelope | Mark/clear a module's rows in a database's updates tracking table |
| `players-online` | — | envelope | Real (non-bot) characters currently online |
| `accounts` | — | envelope | Every real account + its characters |
| `bots` | `[--name <PREFIX>]` (1–12 letters/digits/underscore) `[--class <1-9\|11>]` `[--min-level] [--max-level] [--online]` `[--limit <N>]` (clamped in-library to `1..=200`, default 50) `[--offset <N>]` (default 0) | envelope | Filtered playerbots browser page |
| `teleport-list` | `[--search <S>]` | envelope | `game_tele` locations, optionally filtered by name |
| `items-search` | `--name <NAME>` (required; empty/whitespace-only → `BAD_ARG`) `[--quality <N>] [--min-level] [--max-level]` | envelope | `item_template` search (LIKE-wrapped substring) |
| `paperdoll` | `<NAME>` (1–12 letters/digits/underscore) | envelope | One character's equipped gear + appearance |
| `char-progress` | `<NAME>` | envelope | One character's achievement/talent summary |
| `achievements` | `<NAME>` | envelope | One character's full earned-achievement list |
| `stats` | — | envelope | The Statistics page envelope — 19 queries, 18 concurrent |
| `item-info` | `<IDS>` one token of comma-separated item entry ids (e.g. `25,116,6948`); format checked by clap (exit 2 on malformed), the max-25 cap is a run.rs `BAD_ARG` (exit 1) | envelope | Wowhead tooltip/icon info for one or more item entries |
| `console` | `<COMMAND>...` (1+ tokens, hyphen values allowed; joined with single spaces; whitespace-only → `BAD_ARG`) | envelope | Run one raw worldserver console command over SOAP |
| `account create` | `<USER> <PASS>` (3–20 char user, 4–16 char password — validated by the library) | envelope | Create a game account over SOAP |
| `account set-password` | `<USER> <PASS>` | envelope | Change an account's password |
| `account set-gm` | `<USER> <LEVEL>` (LEVEL is a **string**; literal `0\|1\|2\|3` matched by the library — out-of-range earns `BAD_ARG` "--level must be 0-3", not a clap error) | envelope | Set an account's GM level |
| `account delete` | `<USER>` (refuses `admin`, the launcher's own SOAP account) | envelope | Delete an account |
| `gm level` | `<PLAYER> <LEVEL>` (i32, negatives reach the library; 1–255 checked in-library; works offline) | envelope | Set a character's level |
| `gm gold` | `<PLAYER> <GOLD>` (i32; 0–214748 checked in-library; character must be online) | envelope | Give whole gold via the DML bridge |
| `gm heal` | `<PLAYER>` (must be online) | envelope | Heal to full via the DML bridge |
| `gm revive` | `<PLAYER>` (must be online) | envelope | Revive via the DML bridge |
| `gm summon` | `<PLAYER> <ENTRY>` (i32; `1..=999999` checked in-library; `creature_template` lookup then online check) | envelope | Summon a creature next to the character |
| `gm at-login` | `<PLAYER> <FLAG>` (allowlist `rename\|customize\|changerace\|changefaction`; works offline) | envelope | Set an at-next-login flag |
| `mail-item` | `<TO> <ITEMS>...` (`itemid:count` specs — separate tokens and/or comma-separated) `[--subject <S>]` (default `Dad's MMO Lab`) `[--body <B>]` (alias `--text`, default `Enjoy!`) | envelope | Send in-game mail with item attachments |
| `teleport` | `<CHAR_NAME> <TO>` (destination token as listed by `teleport-list`) | envelope | Teleport a character to a named `game_tele` location |
| `motd` | `<TEXT>` (hyphen values allowed; routed through `config_set`'s `server.motd` special case for sanitization) | envelope | Set the message of the day — applies live, no restart |
| `party add` | `<PLAYER> <CLASS>` (`warrior\|paladin\|hunter\|rogue\|priest\|shaman\|mage\|warlock\|druid`) `[--gender <G>] [--spec <S>]` (premade spec name, e.g. `frost pve`; empty treated as absent; checked against deployed playerbots.conf spec names) | envelope | Spawn a bot of class and invite it to player's party |
| `party kick` | `<PLAYER> <BOT>` (logout whisper is best-effort — failure only flips `dismissed`, never fails the kick) | envelope | Kick one bot from the party and send it to log out |
| `party relogin` | `<PLAYER> <BOT>` | envelope | Log a bot back in and re-invite it |
| `party botcmd` | `<PLAYER> <BOT> <ACTION>` (closed allowlist `gear\|talents\|maintain\|spec`) `[--spec <S>]` (required when action is `spec`; both parties must be online) | envelope | Whisper one bot a maintenance action |
| `party preset-save` | `<PLAYER> <NAME>` (preset name: letters, digits, `-`, `_`, max 32 — path-traversal guard) | envelope | Save the caller's current bot party as a named class-list preset |
| `party preset-list` | — | envelope | List saved presets |
| `party preset-delete` | `<NAME>` (same preset-name guard) | envelope | Delete a saved preset |
| `party preset-load` | `<PLAYER> <NAME>` (both guards run **before** the stream opens) | stream | Replace the current party with a preset |
| `start` | `[--id <ID>]` (default `wow-server-playerbots`; validated `[A-Za-z0-9._-]+`, else `BAD_ID`) | stream | Start the server — brings the Docker Desktop engine up first |
| `stop` | `[--id <ID>]` `[--stop-engine \| --no-stop-engine]` (conflicting; **neither = engine-stop ON**, the library default) | stream | Stop the server; also stops Docker Desktop unless `--no-stop-engine` |
| `restart` | `[--id <ID>]` `[--no-saveall]` (skip the pre-stop saveall; a no-op on the native compose path) | stream | Restart the server — no engine wrapping |
| `backup create` | `[--include-world]` (also dump acore_world) `[--name <NAME>]` (display name; sanitized/bounded in-library, empty/absent gets an auto name) | stream | Take a new gzipped mysqldump |
| `backup list` | — | envelope | Every backup in `~/.dml/backups`, newest first |
| `backup validate` | `<FILE>` (name as reported by `backup list`; bad name → `BAD_ARG`, absent → `NOT_FOUND`) | envelope | Check one backup's gzip integrity and SQL markers |
| `backup delete` | `<FILE>` (same name/existence gates; also drops the `.meta` sidecar) | envelope | Delete one backup file (and its sidecar) |
| `backup restore` | `<FILE> --yes` (no name/existence pre-check — the stream runs both as its first two steps) | stream, guarded | Overwrite the live databases from a backup. **DESTRUCTIVE** |
| `docker-clean` | `--level <1\|2\|3>` (u8, not clap-ranged — the library's in-stream check owns the `BAD_ARG` wording) `--yes` | stream, guarded | Reclaim Docker disk space (build cache, dangling images, stale volumes). **DESTRUCTIVE** |
| `bots-flush` | `--yes --ack flush` (`--ack` must be exactly `flush`; defaults to `""`, so omission is a `CONFIRM_REQUIRED` refusal/exit 1, not exit 2) | stream, guarded | Delete every random playerbot and rebuild the population. **DESTRUCTIVE** |
| `games-remove` | `<ID>` (**required** positional, never defaulted — typing it is part of the confirmation) `[--keep-data]` (keep the ~6 GB client-data volume) `[--remove-images]` (also delete the AzerothCore/MySQL images, ~3–5 GB) `--yes` | stream, guarded | Uninstall a title: its containers, its directory and its launcher. **DESTRUCTIVE** |
| `self-update` | `[--backup \| --no-backup]` (neither → fail-closed `BAD_ARG` "Pick --backup or --no-backup") | stream | Update AzerothCore + mod-playerbots from git |
| `lan` | `<on\|off\|status\|refresh>` (allowlist owned by the library; unknown = `BAD_ARG`/exit 1) `[IP]` (required for on/refresh; ignored for off/status) `[--internet]` (only honored when action is `on`) `[--local <LAN-IP>]` (private/loopback IPv4 only — `BAD_ARG` otherwise; honored on `on`, ignored by `status`/`refresh`, and always forced to `127.0.0.1` by `off`) | envelope | LAN address control for this CLI's fixed AC title |
| `cache status` | — | envelope | Wowhead item-info cache size |
| `cache clean` | — | envelope | Wipe the wowhead item-info cache |
| `client-path get` | — | envelope | The saved client folder, if any |
| `client-path set` | `<DIR>` (unvalidated by clap — the library owns `BAD_PATH`/`NOT_CLIENT` wording) | envelope | Save a new client folder |
| `client-path detect` | — | envelope | Scan common install locations for a WoW client |
| `accountwide get` | — | envelope | Installed state + every subsystem's on/off value |
| `accountwide set` | `<KEY>` (e.g. `ENABLE_ACCOUNTWIDE_MOUNTS`) `<on\|off>` `[--variant <V>]` (for the reputation pick-one) | envelope | Flip one account-wide sharing flag |
| `commands` | — | envelope | The in-game `.` commands cheat sheet (`NOT_FOUND` if the server is not installed) |
| `install` | `[ID]` (positional, default `wow-server-playerbots`; validated `[A-Za-z0-9._-]+`) | passthrough | Interactively install a title — stdio passthrough, no envelope on success |

Not yet documented: the `data` payload schema of each command's ok envelope (known payloads:
`version` as above; `lan` and `console` return `{"result": "<text>"}` on success).

### Confirmation gates

Four irreversible subcommands are guarded: `backup restore`, `docker-clean`, `games-remove`
require `--yes`; `bots-flush` requires `--yes` **and** `--ack flush` (exact string). Without the
gate the CLI emits code `CONFIRM_REQUIRED`, message `<cmd> is destructive; re-run with --yes`
(for `bots-flush` with `--yes` but no ack: `...re-run with --yes --ack flush`), hint `""`,
exit 1. Gates run **first** — no stream is opened and nothing is touched on refusal.

### Argument-parsing doctrine

clap is deliberately loose; the `dml-wow` library owns validation and its exact `BAD_ARG`
wording. Details a frontend should know:

- `console-tail --lines` is the **only** clap-range-gated numeric (`1..=1000`; out-of-range =
  exit 2). Everything else reaches the library (e.g. `account set-gm` LEVEL is a string,
  `docker-clean --level` is un-ranged, `lan` ACTION is a raw string) so that out-of-range values
  earn the library's `BAD_ARG` and exit 1.
- `item-info` id format is a clap custom parser (parse error:
  `not a valid item id: {part:?} (want comma-separated ids, e.g. 25,116,6948)`, exit 2); the
  25-id cap is a run.rs `BAD_ARG` `--entries max 25 ids per call` (exit 1).
- `console` and `motd` allow hyphen-leading values; `gm level`/`gold`/`summon` allow negative
  numbers (so negatives reach the library's range check).
- `mail-item --body` has alias `--text`.
- `--backup`/`--no-backup` (clap-conflicting pair, shared by `module install`/`remove`/
  `rebuild` and `self-update`): **neither flag** is a load-bearing third state, and the rule is
  per-family — it follows whether the operation touches the **database**, not the family alone.
  All wording is the library's, emitted in-stream:

  | Operation | Neither flag | A flag present |
  |---|---|---|
  | `module install --family cpp` | accepted | rejected: `cpp installs don't take backup flags` |
  | `module install --family lua`, module **ships SQL** (`lua_has_sql`) | `BAD_ARG` `Pick --backup or --no-backup`, hint `This script applies SQL to the database.` | accepted |
  | `module install --family lua`, module ships **no** SQL | accepted | rejected: `{key} applies no SQL — backup flags don't apply` |
  | `module install --family sql` | `BAD_ARG` `Pick --backup or --no-backup`, hint `SQL mods change the world database.` | accepted |
  | `module remove --family cpp` | accepted | rejected: `cpp removals don't take backup flags` |
  | `module remove --family lua` | accepted | rejected: `lua removal never touches the database — backup flags don't apply` |
  | `module remove --family sql` | `BAD_ARG` `Pick --backup or --no-backup`, hint `Removal changes the world database.` | accepted |
  | `module rebuild`, `self-update` | `BAD_ARG` `Pick --backup or --no-backup` | accepted |

  A frontend that unconditionally passes a backup flag therefore breaks on cpp installs/removes,
  lua removes, and no-SQL lua installs; one that never passes it breaks on sql installs/removes,
  SQL-shipping lua installs, `rebuild` and `self-update`. Decide per operation.
- `stop`: `--stop-engine`/`--no-stop-engine` conflict; neither = the library default, which is
  engine-stop **ON**.
- Game-id rule for `start`/`stop`/`restart`/`install`: `[A-Za-z0-9._-]+`, else code `BAD_ID`,
  message `invalid game id: {id:?}` (Debug-quoted), hint `Game ids come from games_list`.
  Reproduced launcher wart: bare `.` and `..` pass the regex.
- Default title id for `start`/`stop`/`restart` (`--id`) and `install` (positional) is
  `wow-server-playerbots` (`dml_wow::config::TITLE`); `games-remove`'s ID is required and never
  defaulted, by design.

### `install` passthrough

`dml-wow install` is the one deliberate non-JSON, non-NDJSON command: installers prompt, so all
three stdio streams are inherited and the installer's raw output IS the output. Envelopes appear
only on failure paths (exit 1): `BAD_ID` (before anything runs); `INSTALL_PREREQS` with message
`Git Bash (or bash on PATH) and the dml script (DML_SCRIPT) are required for install` (hint
`""`) when the preflight fails; `INSTALL_SPAWN_FAILED` `failed to launch the installer: {e}`;
`INSTALL_WAIT_FAILED` `the installer started but could not be waited on: {e}`. On success the
process exits with the installer's own exit code (`status.code().unwrap_or(1)` — a signal-killed
child maps to 1).

### Deliberately not ported

These exist in the bash CLI or launcher but have **no** `dml-wow` arm: `party dismiss-all`,
`party preset-show`, `party preset-import` (launcher-only), `gm return-home`. Bash spellings
`config raw-read`/`raw-write`/`tuning-list` are not accepted — this CLI renamed them
`config read`/`config write`/`tuning list`. Also launcher-only with no CLI arm: `lan public-ip`,
the tailscale family, the realmlist (client-side `realmlist.wtf`) family, and `bridge-setup`
(the Eluna bridge deploy is wired only from the launcher on this branch). The CLI's `lan`
actions are exactly `on|off|status|refresh`.

## 5. Error codes

Codes constructed by the CLI crate itself (`crates/dml-wow-cli/src`), with verbatim wording:

| Code | Exit | Where / message (verbatim) |
|---|---|---|
| `BAD_ARGS` | 2 | Any clap parse failure; message = first line of clap's error text; hint `dml-wow --help` |
| `BAD_ARG` | 1 | Library-owned validation. Literal CLI-crate messages include: `Invalid name prefix: {n}` (hint `1-12 letters/digits/underscore.`); `Invalid class id: {c}` (hint `1-9 or 11.`); `items search requires a non-empty --name` (hint `Example: dml-wow items-search --name hearthstone`); `Invalid character name: {name}`; `--entries max 25 ids per call`; `console-send requires a non-empty --command` (hint `Example: dml wow console-send --command "server info" --json`); `Action spec requires --spec <name>` (hint `e.g. --spec 'frost pve'`); `Invalid action: {action}` (hint `One of: gear talents maintain spec`); `Could not read the new file contents from stdin: {e}` (hint `Pipe the body in, e.g. dml-wow config write mod_x.conf < mod_x.conf`); `Invalid module key: {key}`; `Invalid --db: {db}` (hint `Use world, characters, or auth.`); `Invalid --mode: {mode}` (hint `Use mark or clear.`); `Invalid backup name: {file}` |
| `NOT_FOUND` | 1 | `No such character or no equipped items: {name}` (paperdoll); `No such character: {name}` (char-progress/achievements); `WoW Playerbots server not installed` (hint `Install it first.` — commands, accountwide get/set); `Unknown setting: {key}` (hint `See: dml-wow config list` — config get); `No backup named {file}` (backup validate/delete) |
| `BAD_ID` | 1 | `invalid game id: {id:?}`, hint `Game ids come from games_list` (start/stop/restart/install) |
| `SOAP_AUTH` | 1 | `SOAP authentication failed`, hint `Check ~/.dml/soap.env` — only on HTTP 401; a down server is ok data, not an error |
| `LAN_ERROR` | 1 | message = the lan action's own `[dml] ERROR: ...` text verbatim, hint `""` (emitted when the `[dml] ERROR:` prefix classifier matches — reproduces the bash oracle's exit-1/exit-0 split) |
| `CONFIRM_REQUIRED` | 1 | `{cmd} is destructive; re-run with --yes` (see confirmation gates) |
| `INTERNAL` | 1 | hint `""` in all cases: `Could not resolve the wowhead cache directory` (item-info); cache clean guard message; client-path set Io message; `Could not resolve the backups directory` (backup list/validate/delete) |
| `WIPE_FAILED` | 1 | cache clean wipe-failure message, hint `""` |
| `BAD_PATH` | 1 | client-path set bad-path message, hint `Check the folder exists and try again.` |
| `NOT_CLIENT` | 1 | client-path set not-a-client message, hint `Expected Wow.exe or an Interface folder inside it.` |
| `INSTALL_PREREQS` | 1 | `Git Bash (or bash on PATH) and the dml script (DML_SCRIPT) are required for install`, hint `""` |
| `INSTALL_SPAWN_FAILED` | 1 | `failed to launch the installer: {e}`, hint `""` |
| `INSTALL_WAIT_FAILED` | 1 | `the installer started but could not be waited on: {e}`, hint `""` |

Pass-through codes: most error call sites forward a library `CmdError` verbatim — its three
fields ARE the envelope's three fields, and the CLI never invents a code for a failure the
library already described. Examples that surface on the wire this way: `DB_UNREACHABLE` (via
`dml_wow::db::db_err_to_cmd`, the same mapper the launcher uses) and `DOCKER_DESKTOP_MISSING`
(from `start`'s engine-ensure, hint
`Install Docker Desktop, or set DML_DOCKER_DESKTOP to its exe.`). Streaming commands' domain
errors (module family allowlist, not-installed, party invalid-name/not-online, ...) arrive as
in-stream `error` events emitted by the `dml-wow` library.

Consumer-side only (synthesized by `dml-core` when it consumes a child CLI, never printed by
`dml-wow`): `CLI_CRASH`, `CLI_BAD_OUTPUT` (section 2).

Public error helpers: `dml_core::error`'s `bad_arg` / `not_found_err` / `io_internal_err` are
public API (generic wording; AC-specific hints stay in `dml-wow`).

Not yet documented: the full inventory of error-code strings constructed inside the `dml-wow`
library crate (only the CLI-crate literals and the named pass-through examples above are
enumerated here).

## 6. Environment variables

Everything has a working default — nothing panics on unset. The only effectively required
settings: `DML_GAMES_DIR` whenever the process cwd is not the games directory; `USERPROFILE` or
`HOME` for anything under `~/.dml`; and `DML_SCRIPT` (plus a resolvable bash) for
`dml-wow install`. "Set but empty" is treated as unset for every `DML_*` string variable.

| Variable | Default | Used by / notes |
|---|---|---|
| `DML_GAMES_DIR` | `.` (current directory) | Games root. Title dir = `$DML_GAMES_DIR/wow-server-playerbots`; also `games_dir/<id>` for start/stop/restart/status of an arbitrary id. **No scan, no `%USERPROFILE%\dml-native` fallback in the CLI path** (that fallback is launcher-only). With it unset, reads simply miss (registry defaults / not-installed-class errors), no panic. |
| `DOCKER_DB_EXTERNAL_PORT` / `DB_EXTERNAL_PORT` | 3306 | MySQL port. Precedence (first non-empty value that parses as u16 and is non-zero): env `DOCKER_DB_EXTERNAL_PORT` → env `DB_EXTERNAL_PORT` → title-`.env` `DOCKER_DB_EXTERNAL_PORT` → title-`.env` `DB_EXTERNAL_PORT` → 3306. Trimmed before parse; 0 rejected. |
| `DB_ROOT_PASSWORD` | `password` | MySQL password: env → title `.env` file → default; empty filtered. Host is hardcoded `127.0.0.1`, user hardcoded `root`, schemas fixed (`acore_world`, `acore_characters`, `acore_auth`, `acore_playerbots`) — none env-overridable. |
| `DML_SOAP_URL` | `http://127.0.0.1:7878/` | SOAP endpoint. Per-key precedence: env (set and non-empty) → `~/.dml/soap.env` → default. |
| `DML_SOAP_USER` | `admin` | SOAP account. Same precedence. |
| `DML_SOAP_PASS` | `admin` | SOAP password. Same precedence. Wrong creds surface as `SOAP_AUTH`. |
| `DML_BASH` | Git Bash discovery | Override taken **verbatim** when set non-empty (no existence check); else `C:\Program Files\Git\bin\bash.exe`, then `C:\Program Files\Git\usr\bin\bash.exe` (existence-checked), else bare `bash` off PATH. In this CLI consumed only by `install`. |
| `DML_SCRIPT` | bare `dml` | Path to the bash `dml` script. `install` requires the resolved value to be an existing file (else `INSTALL_PREREQS`) — effectively **required** for `install` (repo `cli/dml` in dev). Second consumer: the Eluna bridge lua source root is `<parent of DML_SCRIPT>/lua` (launcher-wired only on this branch). |
| `DML_DOCKER` | docker.exe discovery | Override used verbatim when set non-empty (even if nonexistent — intentional); else first existing of `%LOCALAPPDATA%\Programs\DockerDesktop\resources\bin\docker.exe`, `%ProgramFiles%\Docker\Docker\resources\bin\docker.exe`, `%ProgramFiles(x86)%\...`; else bare `docker` off PATH. Used by essentially every engine-touching arm. |
| `DML_DOCKER_DESKTOP` | Docker Desktop.exe discovery | Override verbatim; else the three standard `Docker Desktop.exe` locations; **no bare-name fallback**. Engine down + exe not found → `DOCKER_DESKTOP_MISSING` with hint `Install Docker Desktop, or set DML_DOCKER_DESKTOP to its exe.` |
| `DML_BACKEND` | (unset → Wsl) | **Launcher-only switch** on this branch: `native`/`docker` (case-insensitive, trimmed) → Native, else Wsl. `dml-wow` does not read it — its `version` envelope hardcodes `"backend":"native"`. |
| `DML_READY_TIMEOUT_SECS` | 1800 | World-ready wait timeout (seconds); unparseable → 1800. Bounds the world-ready waits in the launcher-invoked world-restart stream **and** in `dml-wow bots-flush`, which restarts auth+world twice. The world-restart wait can also end EARLY, well inside this budget: if `ac-worldserver` is observed not running on 5 consecutive 2s polls it fails fast with the arm's existing `RESTART_FAILED` code (`The world server exited instead of coming back up`) instead of waiting out the timeout. |
| `DML_BACKUP_KEEP` | 10 | Keep-newest-N retention for `~/.dml/backups`; trimmed usize; unparseable → 10. `prune` runs after every `backup create`. |
| `DML_PARTY_POLL_TRIES` | 12 | Party add-bot new-member poll retries (u32, trimmed). |
| `DML_PARTY_POLL_SLEEP` | 0.5 | Poll sleep in fractional seconds (f64 → Duration; default 500 ms). |
| `DML_CLIENT_SCAN_ROOTS` | built-in root list | `client-path detect` roots, split with the platform path-list separator (`;` on Windows). Absent: `home\Games`, `home\wow wotlk`, home itself, then per existing drive letter A–Z: `<drive>:\Games`, `<drive>:\Program Files (x86)`, `<drive>:\wow wotlk`, `<drive>:\`. One level deep, dirs only, capped at 10 candidates. |
| `DML_WOWHEAD_BASE` | `https://nether.wowhead.com` | `item-info` fetch base; every fetch degrades gracefully — never required. |
| `DML_ZAMIMG_BASE` | `https://wow.zamimg.com` | `item-info` icon base. |
| `DML_WOWHEAD_XML_BASE` | `https://www.wowhead.com` | `item-info` XML base. |
| `USERPROFILE` / `HOME` | — | At least one required for any `~/.dml` feature (`USERPROFILE` wins when both set). Both unset: `item-info` errors `INTERNAL` `Could not resolve the wowhead cache directory`; `soap.env` is skipped (SOAP falls back to env/defaults). |
| `LOCALAPPDATA`, `ProgramFiles`, `ProgramFiles(x86)` | — | Only build the docker.exe / Docker Desktop.exe discovery candidate lists; missing vars just shrink the list. |
| `PATH` | — | Resolved docker bin dir is prepended for spawned children (so docker + credential helpers resolve); `install`'s preflight walks PATH manually for bare `bash` (with `.exe` fallback on Windows, **no PATHEXT** — fails closed). |

Not read by the Rust CLI crates: `DML_YQ_BIN` and `DML_AWK` (parity-test harnesses and the
launcher only), `DML_LUA_DIR` (deliberately not ported — the lua root derives from
`DML_SCRIPT`). `git` is a hardcoded bare `git` off PATH — no `DML_GIT` override exists.

Title/DB/SOAP configuration is env-vars only, deliberately: no config file and no CLI flags for
any of it.

### Files under `~/.dml`

`~/.dml` = `%USERPROFILE%\.dml` on Windows, `$HOME/.dml` otherwise. Children used by the CLI:
`soap.env`, `backups/`, `wowhead-cache/`, `client-path`, `party-presets/`.

Two `KEY=VALUE` file parsers exist with **opposite** duplicate-key rules:

- Title `.env` (`$DML_GAMES_DIR/wow-server-playerbots/.env`, read by the DB config): optional
  `export ` prefix stripped, `#` comments and blanks ignored, surrounding single/double quotes
  stripped, **first** match wins (Compose semantics). Process env always beats the file.
- `~/.dml/soap.env`: only the three keys `DML_SOAP_URL`/`DML_SOAP_USER`/`DML_SOAP_PASS` are
  recognized; trailing `\r` stripped (CRLF tolerated), keys/values trimmed, quotes stripped,
  non-`KEY=VALUE` and unrecognized lines ignored, **last** occurrence wins (shell re-assignment
  semantics).

Backups: `~/.dml/backups`, on-disk format shared with the WSL/bash `dml` in both directions.
Retention keeps the newest N `.sql.gz` (N = `DML_BACKUP_KEEP`, default 10); `prune` sweeps
**every** `.sql.gz` beyond the window (even mis-named strays, matching bash `_backup_prune`),
deleting each file plus its `.meta` sidecar, best-effort. Automatic backups (pre-stop/restart
safety dumps, the launcher's 6 h interval dump) are standard `wow-<ts>.sql.gz` files feeding
the same prune pool.

## 7. Caveats

Wire and behavior:

- **JSON key order is unspecified.** Rust emits alphabetical, bash emits insertion order. Parse.
- **Broken pipe always exits 0** — including while emitting an error envelope. A consumer that
  closes the pipe early can observe exit 0 for a command that was reporting an error. (Standard
  SIGPIPE-race behavior, deliberate; `dml-wow console-tail --lines 1000 | head -1` is a normal
  interaction.) A non-pipe stdout write failure exits 1 after a best-effort stderr line.
- **`install` exit-code collision:** the child installer's exit code passes through verbatim, so
  an installer exiting 2 is indistinguishable by exit code from the CLI's own "2 = usage". No
  envelope on success.
- **`install` preflight approximates `CreateProcess` search:** PATH plus a bare `.exe` fallback,
  no `PATHEXT` (`.bat`/`.cmd` etc.). The gap fails closed — a PATHEXT-only match reports "not
  found", never the reverse. A `DML_BASH` pointing at a real non-executable file passes
  preflight and then fails `INSTALL_SPAWN_FAILED`.
- **No cross-process SOAP serialization.** Each invocation creates a fresh single-use lock; two
  concurrent `dml-wow` invocations (or one alongside the GUI or the bash CLI) can interleave
  SOAP calls. Nuance: the bash CLI flocks `~/.dml/soap.lock` only where `flock(1)` exists (the
  Linux distro) — Git Bash on Windows ships no flock, so bash skips the lock there too. The
  Rust CLI provides no cross-process serialization on any platform.
- **`server.motd` is never read back.** The native config reader never reads the DB for
  `server.motd` — `config list`/`config get server.motd` always show the registry default, even
  when the DB is reachable and holds a custom MOTD (bash would show the custom value from
  `acore_auth.motd`). Setting a custom MOTD via `dml-wow motd`/`config set` succeeds over SOAP
  but is never reflected by this CLI's own read arms.
- **`lan`/`console` payload:** success data is `{"result": "<text>"}` (adopted from bash's
  `console-send --json`). `lan` domain failures are classified by the `[dml] ERROR:` prefix
  (`LAN_ERROR`, exit 1); refresh no-ops start with bare `[dml] ` and correctly exit 0 — the
  `ERROR:` token in the classifier is load-bearing (14/14 exit-code agreement measured against
  the bash oracle).
- **`lan --local` writes a second column, and reports nothing extra.** `on --local <LAN-IP>`
  updates `realmlist.localAddress` (and pins `localSubnetMask` to `255.255.255.0`) in addition
  to `address`; `off` always resets `localAddress` to `127.0.0.1`. The success text is
  deliberately UNCHANGED from the no-`--local` case, so the bash/Rust text-parity fixtures
  keep matching — the only observable difference is in the database. Unlike `address`, the
  local write is not read back and verified: it is a companion write on the same row, and a
  silent no-op there cannot mislead the way one on `address` would. Rationale for the column
  at all: AzerothCore hands `localAddress` to clients inside that subnet, so without it every
  player on the home LAN is routed out to the public `address` and reaches the world server
  only if the router hairpins NAT.
- **Two hints cite bash-CLI syntax that does not exist in this binary** (kept for launcher
  byte-parity): the empty `console` command's hint
  `Example: dml wow console-send --command "server info" --json`, and `accountwide set`'s
  validator messages `--value must be on or off` (no `--value` flag exists here) and
  `Flags look like ENABLE_ACCOUNTWIDE_MOUNTS -- see: dml wow accountwide get --json`.
- **`stop` stops Docker Desktop by default** (product decision, parity with the launcher's
  default-checked toggle); `--no-stop-engine` is the opt-out.
- **`games-remove` no longer lists its removal targets** (the bash oracle told the operator
  exactly what would be deleted; the port executes nothing pre-confirmation, so the probe was
  dropped — a `--dry-run` is a possible future amendment). The gate itself is a static 6-entry
  registry lookup: `games-remove ..` earns `BAD_ARG` and builds no path.
- **Embedded registries can go silently stale.** Config/tuning/module registries are baked into
  `dml-wow` at compile time (config 66 rows, tuning 13, catalog cpp 19 / lua 9 / sql 10). A user
  who git-pulls a newer `cli/` but keeps an older binary gets silently stale registry data
  (previously this surfaced as `CLI_BAD_OUTPUT` with an "is the CLI up to date?" hint). Parity
  suites that deep-equal a live bash run guard this, but they self-skip on boxes without the
  prereqs — including CI.
- **CRLF/LF write divergence:** the Rust writers preserve CRLF byte-identically (matching the
  oracle's source semantics and Linux behavior); bash under Git Bash on Windows flattens a CRLF
  config/lua file entirely to LF on any write. Impact is round-trip/diff noise, not corruption.
  CRLF `.lua` files are realistic (ALE lua modules cloned under `core.autocrlf=true`).
- **Panicking library API (known limitation D1):** several public `dml-wow` functions panic on
  unvalidated input because validation lives in caller wrappers — `lan::lan_action` has
  `.expect("validated: ip required for on")` / `.expect("validated: ip required for refresh")` /
  `unreachable!("action pre-validated by validate_lan_request_native")`; moduletail has
  `database_for_short(&db).expect("validated above")`; `backup::delete_backup` does zero
  validation (the CLI's name check is load-bearing against `backup delete ../../anything`). The
  CLI dispatch runs the same validators the launcher wrappers run; a third-party consumer of the
  **library** must validate first. Typed errors instead of panics is the recorded long-term fix.
- **`config files` is not exactly `ls -1`.** It emits dotfiles, and emits a *directory* named
  e.g. `foo.conf` because the check is `exists()`, not `is_file()`. Neither case occurs in a
  normal server layout, and this arm has no committed parity row, so treat its listing as
  best-effort rather than a guaranteed file set.
- **Dependency note for embedders:** the crates take no `tauri` dependency, but "no tokio" is
  overstated — `reqwest`'s blocking feature pulls tokio in transitively. `dml-wow` also
  re-exports `dml_core::{backend, envelope, runner}` on its public surface.
- **List order is not part of the contract.** The Rust reader sorts by byte value
  (`Vec::sort`); the bash oracle sorts with `sort(1)`, whose collation follows the caller's
  locale — under `en_US.UTF-8` that is case-insensitive, under `LC_ALL=C` it is byte order. A
  module SQL listing containing both `..._VendorItems.sql` and `..._texts.sql` therefore comes
  back in a different order from the two backends. Sort client-side if order matters to you.

Scope and status:

- Design intent: reshape the launcher-internal Rust into `dml-core` + `dml-wow` + a thin CLI so
  any frontend (Tauri, Electron, plain scripts) can drive the server; offered to the DML
  maintainers as a branch on this fork.
- Recorded spec deviations (in the plan, not the spec): (1) `install` is interactive stdio
  passthrough, not NDJSON-wrapped (installers prompt; NDJSON capture would deadlock them);
  (2) the static config/tuning/module registries are baked into `dml-wow` (also removes the
  launcher's one-time ~2 s bash registry spawn in native mode); (3) Linux CI tests the three
  crates, not the launcher package (Tauri needs webkit system libs there; the Linux promise is
  the CLI).
- Tests: `cargo test --workspace` on Windows is 1063 passed / 0 failed / 2 ignored with the
  whole server stack **down** (2026-07-27) — dml-core 107, dml-wow 648, the 18 parity suites 38
  while self-skipping, dml-wow-cli 91 + 54, launcher 125.
- CI (`.github/workflows/rust.yml`): the Windows job builds+tests the whole workspace (launcher
  included); the Linux job builds+tests only `dml-core`/`dml-wow`/`dml-wow-cli` and runs 933
  tests. Both green. The Linux job runs with `--nocapture` on purpose, so its log states which
  parity suites actually executed rather than hiding a silent skip.
- **CI green does not mean the byte-parity suites ran.** Each suite self-skips unless its own
  prerequisites exist, and the tiers differ: six suites (cache-status, client-path,
  config-write, tuning-write, item-info, lan-public-ip) need only bash + the committed `cli/dml`
  (+ yq, + network for the last two) and do execute on Linux CI; the rest need a real title
  directory, a reachable MySQL, or a live server, and genuinely run only against an installed
  server.
- Live gates still owed: the mutating happy paths are unverified against a live server (account
  create/set-password/set-gm/delete; gm level/gold/heal/revive/summon/at-login; mail-item;
  teleport; motd; party add/kick/relogin/botcmd/preset-save/preset-load — evidence is code
  reading plus sealed-endpoint probes). The five lua-backend tuning rows read `installed:false`
  on the reference box, so the lua writer has not run against a live runtime. Every Rust exit-0
  path for `lan` needs a real MySQL, so its success half is covered compositionally (27 unit
  tests) rather than end-to-end.

## 8. Attach a frontend — quickstart

Value command: spawn, read all of stdout, parse one JSON envelope. Streaming command: spawn,
parse stdout line by line, act on the terminal event.

```text
# one-shot value command
out  = spawn(["dml-wow", "status"]).stdout          # exactly one JSON envelope
env  = json_parse(out)
if env.ok:  render(env.data)
else:       show_error(env.error.code, env.error.message, env.error.hint)

# streaming command
proc = spawn(["dml-wow", "start"])
terminal = null
for raw in proc.stdout.lines():
    ev = try_json(raw)
    if ev == null:            log_warn(raw); continue        # non-JSON: warn-level text
    if ev.event in ("done", "error"): terminal = ev          # stream ends here
    handle(ev)                                               # ignore unknown event types
code = proc.wait()
if terminal == null:          fail("CLI crashed", code)      # no terminal event: exit code is truth
elif terminal.event == "done": succeed(terminal.data)        # code will be 0
else:                          fail(terminal.error)          # code will be 1
```

Practical notes for the spawner: pass the environment variables from section 6 (at minimum
`DML_GAMES_DIR` unless cwd is the games directory); read stdout only for the contract and
surface stderr to humans if at all; check `dml-wow version` once and gate on
`contract == "dml-json-v3"`.
