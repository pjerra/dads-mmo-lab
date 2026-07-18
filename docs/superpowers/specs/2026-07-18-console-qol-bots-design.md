# Console QoL + Bots Visibility — Design Spec + Plan (Round N)

**Date:** 2026-07-18 · **Branch:** `feat/dml-launcher-windows` · Design review waived (standing).
User request: (1) console text must survive tab changes on ALL consoles; (2) a clear-console
option; (3) download console logs; (4) consoles fill the window where possible / resize so the
page never needs scrolling *because of a console*; (5) Home shows bots online + how many bots
can be loaded; bots-online alone elsewhere where useful (Playerbots page).

**No feature locks this round**: every feature is local-UI or read-only server data (downloads
write a user-chosen local file — not server state). Smoke rows are still added.

## Ground truth (from exploration, verified live)

- Pages are destroyed on nav (`routes/+page.svelte` `{#if page === ...}` blocks); only
  module-level `.svelte.ts` stores survive (pattern: `restart-state.svelte.ts`,
  `features.svelte.ts`). In-flight Tauri `Channel` streams keep firing after unmount into
  orphaned local `term` state — output is lost visually today.
- 7 pages use the shared `Terminal.svelte` + `TermState`/`applyEvent` pair with identical
  local `term = $state(initialTermState())` / `showTerm = $state(false)`: Home(187),
  Library(248), ModuleManager(841), Backups(165), Playerbots(388), GMTools(241), Config(342).
- `Console.svelte` is bespoke: snapshot log tail (poll 3s, replace-wholesale, `.log` capped
  48vh) + command `hist` (local, lost on nav). `InstallTerminal.svelte` (Library installs
  only) accumulates a raw `output` string (ANSI-stripped, `\r`-collapsed), `.scrollback`
  capped 40vh.
- Layout: app shell is `100vh` grid; each page `<section class="content">` is the scroll
  container (`overflow-y:auto`). `Terminal.svelte` `.term` is `min-height:220px;
  max-height:45vh` with inner `.term-body` scrolling.
- Tauri: ONLY `tauri-plugin-opener` installed. No dialog/fs plugin. Adding save dialog =
  Cargo.toml `tauri-plugin-dialog = "2"` + `.plugin(tauri_plugin_dialog::init())` in lib.rs
  builder (next to opener at L895) + `"dialog:allow-save"` in `capabilities/default.json` +
  npm `@tauri-apps/plugin-dialog: ^2` + `npm install`. Writing the picked path needs a new
  rust command (std::fs::write) — do NOT add tauri-plugin-fs.
- Bots online (exact, live-verified = 1602): `db_chars_query` with the My Party idiom
  inverted — `SELECT COUNT(*) FROM characters WHERE online = 1 AND account IN (SELECT
  account_id FROM acore_playerbots.playerbots_account_type WHERE account_type IN (1,2))`.
  (`characters.online=1` INCLUDES bots; type 1=RNDbot 2=AddClass; cross-schema subquery from
  the chars connection is the established idiom, e.g. 90-main.sh:2020-2042.)
- Bots max (live = 2000): env override `AC_AI_PLAYERBOT_MAX_RANDOM_BOTS` in
  `docker-compose.override.yml` (read via `_cfg_env_read`, needs `_cfg_preamble`) beats the
  conf file value (500). Resolution: env override → grep
  `env/dist/etc/modules/playerbots.conf` `AiPlayerbot.MaxRandomBots` → null. All failures →
  JSON null; UI shows `?`.

## Design

### N1. CLI: `wow server-detail` gains `"bots":{"online":N|null,"max":N|null}`

In the `server-detail` arm (90-main.sh:1221-1261): compute only when the world container is
`running`, else both null. `online` from the COUNT query above via `db_chars_query`
(non-numeric/failed → null). `max`: `_cfg_env_read AC_AI_PLAYERBOT_MAX_RANDOM_BOTS` (guarded
— server dir may be missing), fallback conf grep, fallback null; numeric-validate. Add helper
`_bots_counts()` in 40-config.sh next to `_world_ready` emitting the JSON fragment. `server-info`
(back-compat public API) is NOT touched. bats (`wow-server-detail.bats`): (a) world running →
bots.online from stubbed mysql + bots.max from stubbed override env read; (b) db failure →
online null, verdict unchanged; (c) world stopped → both null, no mysql call; (d) max
fallback to conf grep when no override. Reuse/extend the existing mysql + yq stub seams used
by party/config bats (see helpers/env.bash).

### N2. Tauri: save dialog + `save_text_file` + api plumbing

- `Cargo.toml`: `tauri-plugin-dialog = "2"`. `package.json`: `"@tauri-apps/plugin-dialog":
  "^2"` + run npm install.
- lib.rs: `.plugin(tauri_plugin_dialog::init())` after opener; new command
  `save_text_file(path: String, content: String) -> Result<(), String>` = `std::fs::write`
  mapped err→string; register in generate_handler.
- `capabilities/default.json`: add `"dialog:allow-save"`.
- api.ts: `export async function saveTextFile(defaultName: string, content: string):
  Promise<boolean>` — `save({ defaultPath: defaultName })` from `@tauri-apps/plugin-dialog`;
  null (cancel) → false; else `invoke("save_text_file", ...)` → true.
- api.ts types: `ServerDetail` gains `bots: { online: number | null; max: number | null }`.

### N3. `term-store.svelte.ts` + Terminal.svelte UX

New `launcher/src/lib/term-store.svelte.ts` (pattern copied from features.svelte.ts):
```ts
export interface TermBuf { term: TermState; show: boolean }
const store = $state({ bufs: {} as Record<string, TermBuf> });
export function termBuf(key: string): TermBuf   // lazily creates {initialTermState(), show:false}
export function beginRun(key: string): TermBuf  // fresh term + show=true, returns buf
export function clearBuf(key: string): void     // fresh term + show=false
export function termText(t: TermState): string  // PURE: flatten sections -> "== name ==\n" + lines
export const consoleStore = $state({ hist: [] as ConsoleHistEntry[] });  // Console page history
export const installStore = $state({ text: "" });                        // Library install output
```
(`ConsoleHistEntry` type moves here from Console.svelte.)

`Terminal.svelte` changes:
- New optional props: `onclear?: () => void`, `logName?: string` (default `"dml"`).
- Header gains two small buttons right of the status: `Clear` (rendered only when `onclear`
  given; `disabled={running}`) and `Download` (always; flattens via `termText(termState)`,
  calls `saveTextFile(`${logName}-${stamp()}.log`, text)`; `stamp()` =
  `new Date().toISOString().slice(0,19).replace(/[:T]/g, "-")`).
- Fill: `.term` `max-height: 45vh` → `max-height: calc(100vh - 220px)`. On a new run
  (`startedAt` transitions null→set), `box.scrollIntoView({ block: "end" })` so the page
  auto-scrolls the terminal into view (its internal autoscroll then follows output).
- vitest: `term-store.test.ts` — termBuf lazy-create/idempotent key, beginRun resets+shows,
  clearBuf hides+empties, termText flattening truth table (sections, empty state).

### N4. Migrate the 7 Terminal pages to the store

Per page: delete local `term`/`showTerm`; `const buf = termBuf("<key>")`; stream callbacks
`buf.term = applyEvent(buf.term, e)`; reset-before-run → `beginRun("<key>")`;
`{#if buf.show}<Terminal state={buf.term} onclear={() => clearBuf("<key>")}
logName="dml-<key>" />{/if}`. Keys: `home`, `library`, `modules`, `backups`, `playerbots`,
`gmtools`, `config`. NOTE Config.svelte is shared by settings+modules nav ids — one key
`config` (single instance already survives settings↔modules hops; the store now survives
full unmounts too). No other behavior changes. Gates: check 0/0, vitest green.

### N5. Console page + Library install surfaces

Console.svelte:
- `hist` → `consoleStore.hist` (survives nav; type import from term-store).
- Fill layout: `.content { overflow: hidden }` (page no longer scrolls); `.log { flex: 1;
  min-height: 200px; max-height: none }`; `.history { max-height: 22vh; overflow-y: auto }`;
  input row stays last/visible. Result: log tail fills all free height, only inner regions
  scroll.
- Header gains `Clear` (clears `consoleStore.hist`; log tail is a server snapshot — refills
  on next poll, so Clear targets history; disabled while `sending`) and `Download` (tail
  lines + blank + history rendered as `> cmd` / reply blocks; `saveTextFile("dml-console-...")`).
Library/InstallTerminal.svelte:
- `output` string → `installStore.text` (read+write through the store so installs stream on
  across nav; InstallTerminal keeps its ANSI/`\r` processing).
- Header gains `Clear` (disabled while the install session is running) + `Download`
  (`dml-install-...`).
- `.scrollback` `max-height: 40vh` → `calc(100vh - 260px)`.

### N6. Bots UI

- Home online-card stats row (Home.svelte ~100-105): 4th span `Bots:
  <strong>{detail.bots.online ?? "?"} / {detail.bots.max ?? "?"}</strong>`.
- Home health panel (~155-163): after Players online row, `<div class="hrow"><span
  class="hname">Bots online</span><span class="hval">{detail.bots.online ?? "?"} of
  {detail.bots.max ?? "?"} max</span></div>`.
- Playerbots.svelte header bar (232): chip `<span class="chip">Bots online:
  {botsOnline ?? "?"}</span>` between h2 and Refresh; `botsOnline` fetched in `refresh()` via
  `wowServerDetail()` (`.bots.online`, failure → null). Chip CSS matches existing muted-chip
  styles on the page (or minimal new `.chip`).

### N7. Docs + smoke rows

SMOKE-TESTS.md additions (no locks — read-only/local): §1 row "Bots line" (Home shows
`Bots: <n> / <max>` matching `.playerbots` reality); §2 rows: "Console persistence" (run a
stream on Modules, hop to Home and back — transcript intact + still streaming), "Clear
button", "Download log" (file saves where chosen, content matches), "Console fill" (Console
page: log fills window, page itself never scrolls; other pages: terminal auto-scrolls into
view on run start and is viewport-capped); §3 row: install output survives nav. README:
launcher feature list gains one line (console persistence/clear/download + bots display).
Dev note: after this round `npm install` must run once and the `npm run tauri dev` app must
be RESTARTED (new rust plugin) — put in the report to the user, not docs.

## Tasks (SDD; commit per task; gates per task)

1. **N1 CLI bots block** — 90-main.sh server-detail arm + `_bots_counts` in 40-config.sh +
   4 bats + rebuild. Gate: full bats green (406+4=410).
2. **N2 Tauri dialog+save+types** — Cargo/npm/capabilities/lib.rs/api.ts. Gates: cargo test
   (25), `npm run check` 0/0, vitest green. (cargo build via `npm run tauri dev` happens on
   the user's side; CI-style `cargo test` runs headless.)
3. **N3 term-store + Terminal UX** — new store + tests + Terminal buttons/fill. Gates:
   vitest (41+~6), check 0/0.
4. **N4 migrate 7 pages** — mechanical store migration. Gates: vitest, check 0/0.
5. **N5 Console + Library surfaces** — bespoke migrations + fill layout + buttons. Gates:
   vitest, check 0/0.
6. **N6 bots UI** — Home + Playerbots. Gates: vitest, check 0/0.
7. **N7 docs + smoke rows** — SMOKE-TESTS + README. Then final whole-round review (opus) on
   the round diff; fix findings; redeploy (`dev-install.ps1`), remind user re npm install +
   tauri dev restart.
