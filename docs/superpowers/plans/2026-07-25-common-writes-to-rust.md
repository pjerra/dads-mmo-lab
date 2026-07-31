# Common Writes → Rust Implementation Plan

> **STATUS (corrected 2026-08-01): SUBSYSTEMS A AND B ARE BUILT, TESTED AND
> PUSHED — this plan is ~90% DONE, not deferred.**
>
> The header this replaces said "TRACKED BUT DEFERRED — do NOT start this before
> the v0.1.0 beta ships", written 2026-07-30. It was wrong, and wrong in a way
> worth recording: the work had already shipped on **2026-07-25**, five days
> BEFORE that header was written. The plan file was rescued out of untracked
> limbo and committed (in `b1fc258`) to stop it being invisible to git — the
> right instinct — but nobody checked whether it had already been executed, so a
> finished subsystem was filed as outstanding. That is the recorded
> `.superpowers/`-invisibility failure in a new flavour: not lost work, but
> **already-done work re-entered as a blocker**. Before deferring a recovered
> plan, check `git log` for its own commits.
>
> Shipped 2026-07-25 (verified by commit): **A** — SOAP client + command layer
> (`81582b6`), 13 native SOAP write commands (`f77fa28`), return-home +
> `db::execute` (`6dcc933`), routing + parity (`cde4e4a`, `c8e085f`).
> **B** — config-write core (`042eb7a`), `wow_config_set_native` (`f531d00`),
> `wow_config_tuning_set_native` (`aa80bf0`), parity + Save routing (`dbccdb8`,
> `b0b5589`). Native Tailscale (`e63c4e2`, `c6e9c3b`) landed the same night.
>
> WHAT ACTUALLY REMAINS is one item out of the three "Adjacent fixes" below:
> **Console always-on (background stream)**. The other two are done — native
> Tailscale shipped as above, and the docker-stop UX hang was fixed by
> `stop_engine_stream_with` (which emits "Stopping Docker Desktop..." before the
> slow call) plus the 2026-08-01 reducer fix that closes a running section on the
> terminal `done` event. Tick them off rather than rebuilding them.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move the *common writes* the launcher does — GM/account/mail/teleport/announce actions, the Console "send command", and the Settings/Tuning **Save** — out of the bash `dml` CLI and into the launcher's Rust core, so native mode performs them directly (and the SOAP path benefits WSL mode too).

**Architecture:** Two independent subsystems. (A) A **Rust SOAP client** that speaks AzerothCore's SOAP over HTTP:7878 — this covers every "action" write, because `dml` already routes account/gm/mail/teleport mutations through SOAP (the *worldserver* performs the DB change + SRP6 hashing; we never touch the DB or hash a password ourselves). (B) **Rust config-file writes** for Settings/Tuning Save — override YAML (`docker-compose.override.yml`) + module `.conf` files, matching `dml`'s exact write semantics. Each Rust path is native-mode-only routing (WSL keeps calling `dml`), EXCEPT the SOAP client, which may serve both modes since SOAP is plain TCP.

**Tech Stack:** Rust (Tauri commands), `reqwest` (blocking, in `spawn_blocking`) for SOAP HTTP, `serde_yaml_ng` (already a dep) for the override write, the existing `dml::config` reader machinery for conf parsing. Svelte 5 frontend routing via the established `backend_mode()` + `page-cache.svelte.ts`/pref pattern.

## Global Constraints

- **Parity is the gate.** Every new Rust write path has an integration test proving it produces the SAME effect/output as the corresponding `dml wow <cmd>` — for SOAP, the same `<result>`/fault classification on the live server; for config, byte-identical file contents + the same `changed`/`restart_required`/`applied` envelope. The live server is UP at SOAP `127.0.0.1:7878` (creds in `~/.dml/soap.env`) and the native files live under `C:/Users/perzi/dml-native/wow-server-playerbots/`.
- **Input validation must match the CLI exactly.** Port the bash allowlists verbatim: `_valid_account_user` (3-20 `[A-Za-z0-9_]`), `_valid_account_pass` (4-16, `[A-Za-z0-9_@#%+=!-]`), `_valid_charname` (`^[A-Za-z0-9_]{1,12}$`), gm level `[0-3]`, etc. Reject BEFORE building any command string. SOAP command strings are built from already-validated tokens (never raw user text spliced arbitrarily).
- **WSL mode behavior unchanged** unless a task explicitly routes SOAP through Rust in both modes (Task A4 decision). Native-only routing is gated on `backend_mode()==native`.
- **Never edit** `launcher/src/lib/features.svelte.ts` or `docs/SMOKE-TESTS.md` (controller-owned; the controller adds smoke rows/flags).
- **No new mutating feature ships unlocked** if it lacks a smoke-tested gate — but these REPLACE existing dml-backed writes (GM tools, account page, config save already exist and are already gated/available), so they inherit the existing gating; they are not new user-facing capabilities, just a faster/native backend for them. Flag the controller if any becomes a genuinely new action.
- **Baselines that must stay green:** bats 750, cargo (191 lib + parity), vitest 382, svelte-check 0/0.
- The refusal rules `dml` enforces MUST be preserved: refuse to delete the `admin` account; the `AiPlayerbot.DeleteRandomBotAccounts` config denylist; etc.

---

## File Structure

- **Create** `launcher/src-tauri/src/dml/soap.rs` — the SOAP client (config load, envelope build, POST, response classification) + its unit tests.
- **Modify** `launcher/src-tauri/src/dml/config.rs` — add the write side (override-env write, conf write) beside the existing readers.
- **Modify** `launcher/src-tauri/src/lib.rs` — new Tauri commands (`wow_soap_exec` + typed write wrappers; `wow_config_write`/`wow_tuning_write`); register them; `require_native_backend()` where native-only.
- **Modify** `launcher/src/lib/api.ts` — wrappers + types for the new commands.
- **Modify** `launcher/src/lib/page-cache.svelte.ts` (or a small `writes.ts`) — native-vs-wsl routing for the write calls, mirroring the reader routing.
- **Modify** the write-issuing pages: `GMTools.svelte`, `Accounts.svelte`, `Console.svelte` (send), the announcements control, `Config.svelte`/`ModuleTuning.svelte` (Save).
- **Create** parity tests: `launcher/src-tauri/tests/soap_parity.rs`, `launcher/src-tauri/tests/config_write_parity.rs`.
- **Reference** (read, don't duplicate): `cli/src/20-soap.rs`→`20-soap.sh` (`soap_url/user/pass`, `soap_envelope`, `soap_exec`, `soap_parse_result`, the rc codes 0/2/3/other), `cli/src/40-config.sh` (`_cfg_env_write`, `_cfg_conf_write`, `_cfg_conf_route`, the denylist + live-reload map), `cli/src/90-main.sh` arms `soap-exec)`, `account)`, `gm ...`, `mail-item)`, `config)`.

---

## Subsystem A — Rust SOAP client

### Task A1: SOAP client core (`soap.rs`)

**Files:** Create `launcher/src-tauri/src/dml/soap.rs`; Test: same file `#[cfg(test)]`.

**Interfaces (Produces):**
- `pub struct SoapConfig { url: String, user: String, pass: String }` with `SoapConfig::load()` reading env `DML_SOAP_URL/USER/PASS` then `~/.dml/soap.env` (shell-assignment lines, CR-stripped), defaults `http://127.0.0.1:7878/`, `admin`/`admin`.
- `pub enum SoapOutcome { Ok(String), Fault(String), Auth, Unreachable(String) }` — mirrors `dml`'s rc 0/2/3/other.
- `pub fn exec(cfg:&SoapConfig, command:&str) -> SoapOutcome` — builds the `executeCommand` XML envelope (XML-escaping the command), HTTP POSTs with Basic auth + a bounded timeout (connect 5s, read/write 30s), classifies the response: HTTP 401 → `Auth`; a SOAP `<faultstring>` body → `Fault(text)`; a `<result>` body → `Ok(inner_text, entity-decoded)`; connect/timeout error → `Unreachable(msg)`.
- Pure helpers (unit-tested without a network): `build_envelope(command)`, `parse_response(http_status, body) -> SoapOutcome`, `xml_escape`, `entity_decode` (for `&#xD;` etc. that AC emits).

**Steps:** write failing unit tests first (envelope contains the escaped command; `parse_response` maps a fault body / a result body / a 401 correctly; entity-decode of a real AC result fixture), then implement, then verify. Match `20-soap.sh`'s `soap_parse_result` semantics exactly (it's the parity oracle).

### Task A2: generic + validated write commands (`lib.rs`)

**Interfaces (Consumes A1; Produces for A3/frontend):**
- `wow_soap_exec(command:String) -> Result<String,CmdError>` — the generic escape hatch the Console "send" uses; maps `SoapOutcome` to the existing `SOAP_FAULT`/`SOAP_AUTH`/`SOAP_UNREACHABLE` `CmdError` codes (same strings as the CLI).
- Typed wrappers, each validating inputs (ported allowlists) BEFORE building the command, then calling `soap::exec`:
  - `wow_gm(action, char, value?)` → `.<gm command>` (level/gold/heal/revive/summon/teleport/at-login/return-home — copy each command string from the `gm` arm in `90-main.sh`).
  - `wow_account(op, user, pass?, level?)` → `account create|set password|set gmlevel|delete` (copy verbatim from the `account)` arm, INCLUDING the refuse-to-delete-`admin` guard and the exact `set password <u> <p> <p>` / `set gmlevel <u> <l> -1` forms).
  - `wow_mail_send(...)` and `wow_announce(text)` → their SOAP command strings from `90-main.sh`.

**Steps:** TDD each wrapper's validation (a bad user/pass/level/name returns `BAD_ARG` with the CLI's exact message; the `admin`-delete refusal fires) with unit tests that stub `soap::exec`; then wire the real client.

### Task A3: parity + frontend routing

**Files:** Create `launcher/src-tauri/tests/soap_parity.rs`; Modify `api.ts`, the routing module, `GMTools.svelte`/`Accounts.svelte`/`Console.svelte`/announcements control.

**Steps:**
- Parity test (live server up): for a safe, reversible command set (e.g. `server info`, an announce, a get-level, set-gm then set back), assert `soap::exec` returns the SAME classification + `<result>` text as `dml wow soap-exec "<cmd>"`. Do NOT test destructive ops against the live server without cleanup; prefer idempotent/queryable commands + one create-then-delete of a throwaway account.
- `api.ts` wrappers + types; route the write calls to the Rust commands when `backend_mode()==native` (see A4), else the existing `dml`-backed calls.
- Point GM Tools, Accounts, Console-send, Announcements at the routed calls. No UI/behavior change beyond the backend.

### Task A4 (decision, do first in A3): native-only vs universal SOAP

**Interfaces:** a one-line routing predicate.

SOAP is plain TCP reachable in both modes, so the Rust client COULD serve WSL too (removing a `dml` spawn there). **Default: route native→Rust, WSL→dml** (consistent with the readers, lowest risk, keeps WSL byte-identical). Leave a comment noting universal routing is a safe future flip. If the controller prefers universal, it's a one-line change + extending the parity test to assert identical behavior in WSL mode.

---

## Subsystem B — Rust config writes

### Task B1: config write core (`config.rs`)

**Files:** Modify `launcher/src-tauri/src/dml/config.rs`; Test: same file.

**Interfaces (Produces):**
- `override_env_write(override_path, key, value)` — set `.services.ac-worldserver.environment[key] = value` in the override YAML, creating the service/environment maps if absent, preserving the rest of the file — matching `_cfg_env_write` (uses `serde_yaml_ng`; round-trip must not reorder/strip unrelated keys — verify against a real override fixture).
- `override_env_remove(override_path, key)` — matching `_cfg_env_remove` (the legacy-env cleanup the conf-write path does).
- `conf_write(conf_path, key, value)` — comment-preserving in-place edit: replace every active `Key = ...` line (or append `Key = value` if absent), symmetric quote-normalization on compare, tmp-file+rename so a failure never truncates — matching `_cfg_conf_write` EXACTLY (it's the parity oracle; copy its quote/whitespace rules).
- Return a `changed: bool` from each (the effective-value-changed signal `dml` uses for `restart_required`).

**Steps:** TDD against fixtures copied from the real override + a real `.conf`: writing an existing key changes only that line; a pure quote toggle is a no-op (`changed=false`); appending a missing key; the tmp+rename never truncates on a simulated failure. Byte-compare Rust output vs the file `dml` produces for the same edit.

### Task B2: config-write commands (`lib.rs`)

**Interfaces (Consumes B1):**
- `wow_config_write(key, value)` — mirror the `config)` `set` arm: registry lookup + range/type validation for curated rows; the `conf:<file>:<Key>` direct route with its allowlist + the `AiPlayerbot.DeleteRandomBotAccounts` denylist + core-conf rejection; env-vs-conf routing; the `bots.population` writes-both-min-and-max special case; the legacy-env cleanup; and the live-reload map (`transmog reload` → `applied:"live"` via `soap::exec` from Subsystem A, else `restart`). Return the same `{changed,restart_required,applied}` envelope.
- `wow_tuning_write(key, value)` — the `tuning-set` equivalent for module-tuning rows.
- Both native-mode-only (`require_native_backend()`), since they write the Windows-side files.

**Steps:** TDD the routing/validation (denylist rejects; a curated float out of range is `BAD_ARG`; a conf key routes to the right file; transmog key reports `applied:"live"` when SOAP reload succeeds) with the real fixtures.

### Task B3: config-write parity + Save routing

**Files:** Create `launcher/src-tauri/tests/config_write_parity.rs`; Modify `api.ts`, routing, `Config.svelte`/`ModuleTuning.svelte` Save handlers.

**Steps:**
- Parity test: for a representative curated env row, a conf row, and the transmog live-reload row, apply the SAME edit via Rust and via `dml wow config set` on COPIES of the files and assert byte-identical results + identical envelopes. Restore the files after (or work on temp copies).
- Route the Settings + Module-tuning Save to the Rust commands when native; WSL unchanged.

---

## Adjacent fixes (fold in or do as small side-tasks — NOT blockers)

- [ ] **Console always-on (background stream):** replace the tab-open-only poller with a shared store fed by a background `docker logs --follow ac-worldserver` (Rust streaming task → an event/store), so the Console tab is instant + live regardless of which page is open. Bounded buffer (last N lines). Native + WSL (the follow command differs by backend).
- [x] **Native Tailscale:** DONE 2026-07-25 (`e63c4e2`, bounded in `c6e9c3b`). the current feature is distro-shaped (`sudo -n pacman/systemctl/tailscaled/iptables`) and errors on Windows (`sudo.exe` rejects `-n`). Native path: detect the Windows Tailscale app, `tailscale.exe up` (browser login, no sudo), status via `tailscale.exe status`. STOPGAP: gate the whole Tailscale card out of native mode (show "use the Windows Tailscale app" note) so it stops throwing the confusing error until the native path lands.
- [x] **Docker-stop UX hang (from commit 0f53fef):** DONE — `stop_engine_stream_with` emits the notice before the slow call; the terminal-`done` reducer fix (2026-08-01) clears the spinner. when native `games_stop` stops Docker Desktop, the UI just spins the loading wheel with no feedback and (apparently) never completes. `docker desktop stop` can take 20-60s to bring the engine + WSL VM down, and nothing is streamed meanwhile. Fix the streaming in `lib.rs::games_stop` (native branch): emit **"Stopping Docker Desktop…"** BEFORE the (slow) stop call, run `docker desktop stop` with a bounded timeout (~90s) + optionally poll `docker info` until the engine is actually down, then emit **"Docker Desktop stopped."** and a terminal `done` event so the spinner resolves. On timeout/failure emit a clear warning + `done` (best-effort — the server stop already succeeded). Verify the front-end Home Stop flow actually consumes the terminal event and clears its busy state.

---

## Verification
- After each task: `cargo test` (with native env vars + server up for parity), `npm run check`, `npx vitest run`; bats untouched (no CLI edits expected — flag if any).
- Live smoke is the controller's, later, via new SMOKE-TESTS rows: create+delete a throwaway account, a GM level change, an announce, a Settings save + confirm the override/conf file changed and (transmog) applied live.
- End state: in native mode, GM Tools / Accounts / Console-send / Announcements / Settings-Save all run through Rust — `dml` is idle during a normal play+admin session except for the rare install/rebuild/backup operations.
