# DML Launcher — Pages + Config Editor (design)

Date: 2026-07-15 · Status: **approved by user** (brainstorm w/ per-section sign-off)
Depends on: Plan 3 (`dml wow` CLI, live-verified 2026-07-15), Plan 2 (launcher shell).
Explicitly NOT in scope: the Playerbots page (needs Plan 4's bot-join mechanism), item
icons (client DBC extraction), realm rename (DB-only write, no safe command path yet),
coordinate teleport (deferred in Plan 3).

## 1. Goal

Turn the launcher's disabled sidebar pages into real pages on top of the already-working
CLI, and add the user-requested **multi-function config editor** that opens in the same
content slot the embedded Terminal uses. After this plan the launcher is a real app:
Library, Dashboard, Item Database, Teleport, Config — with Playerbots grayed out and
labeled "coming with My Party".

The "Modules" sidebar entry is removed: module settings live inside the Config page.

## 2. Decisions made during brainstorm (user-confirmed)

1. Page scope: **Item DB + Teleport + Dashboard + Config editor**, all in this plan.
2. Config editor style: **curated settings + raw file mode** (The Lab parity).
3. Curated groups: **Rates, Playerbots, AHBot, Server basics** (all four).
4. Item DB actions: **send item to character** (mail), plus search/filters.
5. Character picker: **dropdown of all real characters**, backed by a new read-only
   `dml wow accounts` verb (bot accounts filtered out).
6. Dashboard: **server status + character viewer** (no quick start/stop — Library owns
   lifecycle, avoids two-managers confusion).
7. Apply flow: **Save** (banner: "restart needed") and **Save & Restart** (confirm
   dialog → streams `games restart` into the terminal panel).
8. Architecture: **Approach A — typed Rust commands + curated config registry**
   (rejected: generic CLI passthrough — would let a compromised webview run arbitrary
   GM commands; rejected: splitting pages/config into two plans — they share the
   page-switch work).

## 3. UI changes (launcher)

- `+page.svelte` becomes a thin shell: sidebar (now stateful) + a content area that
  renders the active page component. No SvelteKit routing needed — a `$state` page id
  is enough for 5 pages and keeps the single-route structure Plan 2 shipped.
- New components in `launcher/src/lib/pages/`: `Library.svelte` (today's cards +
  terminal, extracted as-is), `Dashboard.svelte`, `Items.svelte`, `Teleport.svelte`,
  `Config.svelte`. Shared small pieces in `launcher/src/lib/`: `CharPicker.svelte`
  (account → character dropdown, used by Items/Teleport/Dashboard), plus typed
  `api.ts` wrappers per new command.
- The Terminal component and `terminal-state.ts` reducer are **untouched**; Library and
  Config's Save & Restart stream into it exactly like Start/Stop do today.
- Sidebar states: Library/Dashboard/Items/Teleport/Config active; Playerbots rendered
  disabled with the "coming with My Party" note.
- Error envelopes (`{code,message,hint}`) render as a readable error card on every
  page (same pattern as Library's `loadError`).

### Page specs

**Dashboard** — top: status card from `wow server-info` (world up/down, uptime, players
online, mean/percentile update time) with a Refresh button; below: character viewer —
`CharPicker` → `wow paperdoll` result as a clean text list (slot name, item name,
quality, item level). Shows the paperdoll `note:"last_saved"` caveat as a small hint.

**Item Database** — search box (`--name`, required), quality + min/max level filters,
result table (name, quality, item level, required level). Each row has **Send**: a
dialog with `CharPicker`, count (default 1, clamped 1–max stack sanity limit 200),
optional subject → `wow mail-item`. Success shows "sent to <char>".

**Teleport** — search box filtering `wow teleport-list` results (500-row cap noted in
the UI when hit), `CharPicker`, **Teleport** button with a confirm ("Send <char> to
<location>?") → `wow teleport`.

**Config** — two tabs in the terminal-slot panel area:
- **Settings tab**: groups from the registry (see §4). Each setting renders label,
  plain-language explanation, current value, and an input bounded to its safe range
  (number/slider/toggle/text by type). Dirty settings highlighted; Save / Save &
  Restart buttons with the flow from decision 7.
- **Files tab (labeled Advanced)**: dropdown of allowlisted files → text editor →
  Save (+ same restart offer). Save path writes `.bak` first; YAML files are
  syntax-validated before write; validation failure = clean error, file untouched.

## 4. New CLI surface (`cli/src/40-config.sh` + `90-main.sh` wow arm)

All verbs follow the Plan 3 contract: one JSON envelope on stdout, documented error
codes, allowlist validation in the CLI (GUI limits are convenience only), stub-driven
bats tests. Mutations stay SOAP-or-registry-env only; MySQL stays read-only.

- `dml wow accounts --json` →
  `{"accounts":[{"id","username","characters":[{"guid","name","level"}]}]}`
  Read-only over acore_auth + acore_characters. Filters `username LIKE 'RNDBOT%'`
  and `AHBOT` (SOAP-only accounts like `DMLSOAP` appear but simply have no
  characters; harmless). Errors: `DB_UNREACHABLE`.

- `dml wow server-info --json` →
  `{"online":bool,"version":str,"players":int,"uptime":str,"mean_ms":int|null,...}`
  Runs `server info` via `soap_exec` and parses the known line format; unparseable
  fields are null rather than an error. SOAP unreachable → `{"online":false,...}`
  envelope (ok:true — "down" is an answer, not an error) unless auth fails
  (`SOAP_AUTH` stays an error).

- `dml wow config list --json` →
  `{"settings":[{"key","group","label","explain","type","min","max",
  "value","default","restart_required","env"}]}` — the registry with live values.
  Current values read from the title's `docker-compose.override.yml` / `.env`
  (write targets are the source of truth); unset → `default`, marked as such.

- `dml wow config set --key <k> --value <v> --json` →
  `{"changed":bool,"restart_required":true}` — key must exist in the registry
  (`NOT_FOUND` otherwise), value validated against type/range (`BAD_ARG`), written
  via the same yq env-merge path `soap-setup` uses. Idempotent like soap-setup.

- `dml wow config raw-read --file <name> --json` → `{"file","content"}`;
  `dml wow config raw-write --file <name> --json` (content on **stdin**, avoiding
  argv length/quoting issues) → `{"written":true,"backup":"<name>.bak"}`.
  `<name>` must be in the file allowlist (`NOT_FOUND` otherwise): `.env`,
  `docker-compose.override.yml`, and the module confs present on the stack
  (`playerbots.conf`, `mod_ahbot.conf`, `mod_ale.conf`). Mechanism PINNED
  (verified on the real install 2026-07-15): the base compose bind-mounts
  `./env/dist/etc` into the container, so module confs are plain host files at
  `<server dir>/env/dist/etc/modules/` — raw read/write is ordinary file IO, no
  `docker cp`. YAML targets (`docker-compose.override.yml` only) are `yq`-validated pre-write;
  invalid → `BAD_ARG`, file untouched. Every write copies the current file to
  `<file>.bak` first (single-slot backup).

### Curated registry (initial contents)

Registry lives in the CLI as data (bash-embedded table in `40-config.sh`), each row:
key, group, label, explanation, type+range, env var, write target. Env names below
follow the acore-docker convention (config key, dots→underscores, uppercased, `AC_`
prefix); **implementation must pin each exact env name against the running stack /
conf.dist files before shipping** — the mechanism (AC_ env overrides any conf key,
already used by this stack for `AC_AI_PLAYERBOT_*` and `AC_SOAP_*`) is verified.

| Group | Setting | Type/range | Env (to pin) |
|---|---|---|---|
| Rates | XP from kills | float 0.5–20 | `AC_RATE_XP_KILL` |
| Rates | XP from quests | float 0.5–20 | `AC_RATE_XP_QUEST` |
| Rates | Gold drops | float 0.5–20 | `AC_RATE_DROP_MONEY` |
| Playerbots | World bot population — ONE number, written to BOTH min and max (stable population; NB the installed stack currently has min 1600 / max 2000, so read-back shows the max and the first save normalizes min=max) | int 0–3000 | `AC_AI_PLAYERBOT_MIN_RANDOM_BOTS` / `..._MAX_RANDOM_BOTS` (verified: both already present in the stack's override.yml) |
| Playerbots | Bots log in at server start | bool | `AC_AI_PLAYERBOT_RANDOM_BOT_AUTOLOGIN` (verified in override.yml) |
| AHBot | Auction seller bot | bool | `AC_AUCTION_HOUSE_BOT_ENABLE_SELLER` (conf key `AuctionHouseBot.EnableSeller`; mangling rule proven by the worldserver's own log: `Updates.EnableDatabases` → `AC_UPDATES_ENABLE_DATABASES`) |
| AHBot | Auction buyer bot | bool | `AC_AUCTION_HOUSE_BOT_ENABLE_BUYER` |
| AHBot | Seller character — user picks a character; the CLI resolves it read-only to its guid+account and writes BOTH env keys | char name | `AC_AUCTION_HOUSE_BOT_GUID` + `AC_AUCTION_HOUSE_BOT_ACCOUNT` |
| Server | Message of the day | text (quotes/CR/LF stripped) | **AMENDED 2026-07-15 (user-approved):** this AC build has NO `Motd` conf key — MOTD lives in the auth DB (`motd` table, `MotdMgr`) and is set live via the `.server set motd <realmId> <locale> <text>` console command. So `config set server.motd` sends `server set motd 1 enUS <text>` over SOAP (applies INSTANTLY, `restart_required:false`; needs the server running — `SOAP_UNREACHABLE` with a "start it first" hint otherwise), and `config list` reads it back read-only from `acore_auth.motd` (guarded; falls back to the default if the DB is down). The one registry row whose `restart_required` is false. |

Registry is intentionally small; adding a row is the extension path.

## 5. Rust layer (launcher/src-tauri)

One typed command per verb, mirroring `games_*`: `wow_accounts`, `wow_server_info`,
`wow_items_search`, `wow_mail_item`, `wow_teleport_list`, `wow_teleport`,
`wow_paperdoll`, `wow_config_list`, `wow_config_set`, `wow_config_raw_read`,
`wow_config_raw_write` — request/response via the existing `run_json`; only
`games_restart` (already streaming) is used for Save & Restart. (No `wow_characters`
command: the character lists ride inside `wow_accounts`, so a separate verb would be
dead surface in the launcher.)
Raw-write passes content via the child's stdin. Input validation in Rust stays
minimal (arg presence/UTF-8) — the CLI is the authority; Rust's job is only to make
non-allowlisted verbs unreachable from the webview.

## 6. Safety summary

No new privilege beyond Plan 3's live-tested surface: reads are read-only; the only
mutations are mail-item, teleport, registry-validated env writes, allowlisted raw
file writes (with `.bak` + YAML validation), and restart — restart being the only
action affecting a live session, always behind a confirm dialog. All enforcement is
CLI-side. SOAP creds keep coming from `~/.dml/soap.env` (chmod 600). Never
quote-wrap name args in AC console commands (live-confirmed parser behavior; only
`#subject`/`#text` QuotedStrings take quotes).

## 7. Testing & gates

- bats (stub harness) for every new verb: envelope shape, allowlist rejections,
  range validation, `.bak` creation, YAML-invalid rejection, registry read-back,
  accounts bot-filtering, server-info parse (fixture from the real output captured
  2026-07-15).
- cargo tests for new commands over fixture scripts (existing pattern);
  vitest for page-level pure logic (keep pages thin); `svelte-check` clean.
- Full existing suites stay green (96 bats / 16 cargo / 8+ vitest at time of writing).
- **Final gate (user-supervised, live server):** click-through of all four pages —
  search+send an item, teleport the test char, dashboard status+paperdoll, change a
  rate via Settings + a comment via Files, Save & Restart, confirm the rate applied
  in-game.
