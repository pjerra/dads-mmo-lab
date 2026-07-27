# DML Rust CLI Workspace Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reshape the ported Rust `dml` surface (today inside `launcher/src-tauri/src/dml/`) into a cargo workspace — `dml-core` (game-agnostic library) + `dml-wow` (WoW library) + `dml-wow-cli` (standalone `dml-wow` binary) — so any frontend or script can attach to the server without the launcher, per the approved spec `docs/superpowers/specs/2026-07-26-rust-cli-workspace-design.md`.

**Architecture:** Move-and-re-export refactor. Files move (git mv) into the two library crates; the old module paths keep working via `pub use` shims so the launcher, the 17 live parity suites, and intra-module `super::` references never see a broken path. The launcher's `#[tauri::command]` wrappers stay in `lib.rs` and shrink to Channel→closure adapters over `dml-wow` functions. The CLI is a thin clap binary printing the same JSON envelopes / NDJSON event stream the bash `dml --json` contract defines.

**Tech Stack:** Rust 2021 (toolchain 1.97 MSVC on this box), cargo workspace, clap 4 (derive) — the ONLY new dependency. Existing deps redistribute: `mysql`/`flate2`/`sha1`/`base64` → dml-wow, `serde_yaml_ng` → dml-core, `tauri*` stays launcher-only.

## Global Constraints

- Branch: `feat/rust-cli-workspace` (already exists, off `spike/docker-desktop-native`). NO merge to main — standing user policy.
- `crates/*` MUST NOT depend on `tauri` (that is the point). No `tokio` either — everything stays synchronous (the `mysql` crate is the sync one; launcher wraps calls in `tauri::async_runtime::spawn_blocking`).
- Only ONE new external dependency across the whole plan: `clap = { version = "4", features = ["derive"] }` in dml-wow-cli.
- Byte-identical behavior gate: the 17 parity test files keep passing (they self-skip without a live env; the final task runs them against the live snapshot at `C:\Users\perzi\dml-native`). Any envelope/NDJSON shape change is a defect.
- WSL mode untouched: `DmlRunner::default()` (wsl.exe path), the bash CLI under `cli/`, and all non-`_native` tauri commands keep exact behavior.
- Frontend untouched except zero-diff expectations: no changes under `launcher/src/` in this plan. Tauri command NAMES and signatures must not change (the frontend invokes them by name).
- After EVERY task: `cargo test --workspace` green from repo root (Windows), then commit. Run cargo via `%USERPROFILE%\.cargo\bin\cargo` if PATH lacks it.
- Baselines that must not regress: cargo tests currently 818 lib + 17 parity files (self-skipping), vitest 385, svelte-check 0 errors 0 warnings, bats 750 (untouched by this plan).
- New shell files (`.sh` fixtures) must be LF — `.gitattributes` already forces `*.sh` LF; do not add CRLF fixtures.
- Windows shell quirks: PowerShell 5.1 — no `&&`; use `git commit -F <file>` for multi-line messages.
- Only one controller session may execute this plan (check `.superpowers/sdd/progress.md` + `git log` before dispatching).
- Commit messages: conventional commits (`refactor:`, `feat:`, `test:`, `ci:`, `docs:`), each ending with the standard Claude trailers used on this repo.

## File Structure (end state)

```
Cargo.toml                     # NEW workspace root (members below); Cargo.lock moves here
crates/
  dml-core/
    Cargo.toml                 # serde, serde_json, serde_yaml_ng
    src/lib.rs                 # pub mod backend, conf, compose, engine, envelope, error, events, proc, runner, util
    tests/fixtures/*.cmd,*.sh  # runner/proc fixtures, cross-platform
  dml-wow/
    Cargo.toml                 # dml-core, serde, serde_json, mysql, reqwest, base64, flate2, sha1
    src/lib.rs                 # old dml/mod.rs content: pub mod accountwide … tuning (+ registry)
    src/<the 30 wow modules>.rs
    data/config-registry.json  # NEW embedded registry snapshots (Task 8)
    data/tuning-registry.json
    data/module-catalog.json
    tests/*_parity.rs          # all 17 parity suites move here
    tests/fixtures/            # their fixtures
  dml-wow-cli/
    Cargo.toml                 # dml-core, dml-wow, clap, serde_json; [[bin]] name = "dml-wow"
    src/main.rs                # entry: parse → dispatch → exit code
    src/cli.rs                 # clap derive tree
    src/out.rs                 # envelope emit + stream printer + exit codes
    src/run.rs                 # dispatch: one match arm per subcommand → dml-wow call
launcher/src-tauri/            # becomes workspace member; src/dml/ DELETED; lib.rs imports dml_wow
.github/workflows/rust.yml    # NEW CI
docs/cli-contract.md          # NEW attach-a-frontend contract
docs/rust-cli-pitch.md        # NEW community pitch
```

---

### Task 1: Workspace scaffold

**Files:**
- Create: `Cargo.toml` (repo root), `crates/dml-core/{Cargo.toml,src/lib.rs}`, `crates/dml-wow/{Cargo.toml,src/lib.rs}`, `crates/dml-wow-cli/{Cargo.toml,src/main.rs}`
- Modify: `.gitignore` (root `/target/`)
- Move: `launcher/src-tauri/Cargo.lock` → `Cargo.lock` (git mv)

**Interfaces:**
- Produces: workspace where `cargo test --workspace` runs launcher + 3 crates; crate names `dml-core`/`dml-wow`/`dml-wow-cli`, lib names `dml_core`/`dml_wow`, bin name `dml-wow`.

- [ ] **Step 1: Write root `Cargo.toml`**

```toml
[workspace]
resolver = "2"
members = [
    "crates/dml-core",
    "crates/dml-wow",
    "crates/dml-wow-cli",
    "launcher/src-tauri",
]
```

- [ ] **Step 2: Write the three crate skeletons**

`crates/dml-core/Cargo.toml`:
```toml
[package]
name = "dml-core"
version = "0.1.0"
description = "Game-agnostic core for DML per-game tooling: docker engine/compose attach, conf-file engine, JSON envelope + NDJSON stream contract"
license = "AGPL-3.0-only"
edition = "2021"

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```
`crates/dml-core/src/lib.rs`: `//! Game-agnostic DML core.` (empty for now).

`crates/dml-wow/Cargo.toml`: same header pattern, `name = "dml-wow"`, description "WoW (AzerothCore + playerbots) game library for DML", deps `dml-core = { path = "../dml-core" }`, serde, serde_json. `src/lib.rs`: doc comment only.

`crates/dml-wow-cli/Cargo.toml`: `name = "dml-wow-cli"`, deps `dml-core`, `dml-wow` (path), serde_json, plus:
```toml
[[bin]]
name = "dml-wow"
path = "src/main.rs"
```
`src/main.rs`: `fn main() { println!("{{\"ok\":true,\"data\":{{}}}}"); }` placeholder (replaced in Task 10).

- [ ] **Step 3: Move the lockfile and ignore root target**

```powershell
git -C . mv launcher/src-tauri/Cargo.lock Cargo.lock
```
Add `/target/` to root `.gitignore`. (Workspace target dir now lives at repo root; `launcher/src-tauri/target/` becomes stale — leave it, it is already ignored.)

- [ ] **Step 4: Verify the workspace builds and launcher still compiles**

Run from repo root: `cargo test --workspace`
Expected: all existing launcher tests pass (818 lib tests), new crates compile with 0 tests. Also run `cargo check -p launcher` — passes.

- [ ] **Step 5: Commit**

`refactor: cargo workspace scaffold (dml-core, dml-wow, dml-wow-cli crates)`

---

### Task 2: dml-core — envelope, events, error, backend, util

**Files:**
- Create: `crates/dml-core/src/{envelope.rs,events.rs,error.rs,backend.rs,util.rs}`
- Modify: `crates/dml-core/src/lib.rs`, `launcher/src-tauri/src/dml/mod.rs`, `launcher/src-tauri/src/dml/envelope.rs` (deleted), `launcher/src-tauri/src/dml/backend.rs` (deleted), `launcher/src-tauri/src/dml/modmgr.rs`, `launcher/src-tauri/src/lib.rs`, `launcher/src-tauri/Cargo.toml` (dep on dml-core)

**Interfaces:**
- Produces (all re-exported by launcher's `dml/mod.rs` so existing paths keep compiling):
  - `dml_core::envelope::{Envelope, ErrorInfo, parse_envelope, decode_wsl_output}` (moved verbatim)
  - `dml_core::envelope::{ok_envelope(data: serde_json::Value) -> serde_json::Value, error_envelope(code: &str, message: &str, hint: &str) -> serde_json::Value}` (NEW — emit side)
  - `dml_core::events::{line_event, section_start, section_end, done_event, error_event}` (moved from `modmgr.rs:306-332`, signatures unchanged)
  - `dml_core::error::CmdError { code, message, hint }` (moved from launcher `lib.rs:78-82`, keeps `#[derive(Debug, Serialize)]`)
  - `dml_core::backend::{Backend, from_override, selected, BACKEND_ENV}` (moved verbatim)
  - `dml_core::util::{home_dir, dml_home_dir}` (moved from `dml/mod.rs:40-49`)

- [ ] **Step 1: Write failing tests for the NEW emit-side envelope helpers** (in `crates/dml-core/src/envelope.rs` after pasting the moved code):

```rust
#[test]
fn ok_envelope_shape() {
    let v = ok_envelope(serde_json::json!({"version": "0.1.0"}));
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["version"], "0.1.0");
    assert!(v.get("error").is_none());
}

#[test]
fn error_envelope_shape_matches_bash_contract() {
    let v = error_envelope("NOT_FOUND", "Title not found: nope", "Run games list");
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"]["code"], "NOT_FOUND");
    assert_eq!(v["error"]["message"], "Title not found: nope");
    assert_eq!(v["error"]["hint"], "Run games list");
    assert!(v.get("data").is_none());
}

#[test]
fn round_trip_error_envelope_through_parser() {
    let v = error_envelope("X", "y", "");
    let env = parse_envelope(&v.to_string()).unwrap();
    assert!(!env.ok);
    assert_eq!(env.error.unwrap().code, "X");
}
```

- [ ] **Step 2: Run to verify failure** — `cargo test -p dml-core` fails: functions not defined.

- [ ] **Step 3: Do the moves + minimal implementations**

1. `git mv launcher/src-tauri/src/dml/envelope.rs crates/dml-core/src/envelope.rs`; append:
```rust
use serde_json::Value;

/// Emit-side `{ok:true,data}` envelope — the exact shape `cli/src/10-json.sh`
/// emits and `parse_envelope` reads.
pub fn ok_envelope(data: Value) -> Value {
    serde_json::json!({ "ok": true, "data": data })
}

/// Emit-side `{ok:false,error:{code,message,hint}}` envelope.
pub fn error_envelope(code: &str, message: &str, hint: &str) -> Value {
    serde_json::json!({ "ok": false, "error": { "code": code, "message": message, "hint": hint } })
}
```
2. `git mv launcher/src-tauri/src/dml/backend.rs crates/dml-core/src/backend.rs`.
3. Create `crates/dml-core/src/events.rs`: CUT `line_event`, `section_start`, `section_end`, `done_event`, `error_event` (modmgr.rs:306-332) and paste verbatim (they only use serde_json). The `SECTION_*` string constants STAY in modmgr (wow-specific names). In `modmgr.rs` add `pub use dml_core::events::{line_event, section_start, section_end, done_event, error_event};` so every existing `modmgr::section_start(...)` call site (including other modules and lib.rs) still resolves.
4. Create `crates/dml-core/src/error.rs`: CUT `CmdError` struct (launcher lib.rs:77-82) and paste. Do NOT move `From<RunnerError>` yet (runner moves in Task 3). In launcher lib.rs add `pub use dml_core::error::CmdError;` at the old location (tests and dml modules reference `crate::CmdError`).
5. Create `crates/dml-core/src/util.rs`: CUT `home_dir`/`dml_home_dir` from `dml/mod.rs`; in `dml/mod.rs` add `pub use dml_core::util::{home_dir, dml_home_dir};`.
6. `dml/mod.rs`: replace `pub mod envelope;` with `pub use dml_core::envelope;` and `pub mod backend;` with `pub use dml_core::backend;`.
7. dml-core `lib.rs`: `pub mod backend; pub mod envelope; pub mod error; pub mod events; pub mod util;`
8. launcher `Cargo.toml`: add `dml-core = { path = "../../crates/dml-core" }`.

- [ ] **Step 4: Run** — `cargo test --workspace`: dml-core tests pass (moved envelope tests + 3 new), launcher 818 still pass (the count shifts down by exactly the moved envelope tests — record the new split in the commit message).

- [ ] **Step 5: Commit** — `refactor: move envelope/events/error/backend/util into dml-core`

---

### Task 3: dml-core — runner + docker discovery + cross-platform fixtures

**Files:**
- Move: `launcher/src-tauri/src/dml/runner.rs` → `crates/dml-core/src/runner.rs`
- Create: `crates/dml-core/src/docker.rs` (docker/desktop program discovery, cut from `native.rs`), `crates/dml-core/tests/fixtures/*.sh` (9 files), fixtures `.cmd` copies
- Modify: `crates/dml-core/src/{lib.rs,error.rs}`, `launcher/src-tauri/src/dml/{mod.rs,native.rs}`

**Interfaces:**
- Produces: `dml_core::runner::{DmlRunner, RunnerError, DISTRO, USER}` (verbatim; launcher re-exports via `pub use dml_core::runner;` in dml/mod.rs)
- Produces: `dml_core::docker::{docker_program() -> OsString, docker_desktop_program() -> Option<OsString>}` (cut from native.rs:82,137; native.rs re-exports them)
- `From<RunnerError> for CmdError` impl moves to `dml-core/src/error.rs` VERBATIM (launcher lib.rs loses it).

- [ ] **Step 1: Move the files**

1. `git mv launcher/src-tauri/src/dml/runner.rs crates/dml-core/src/runner.rs`; fix `use super::backend::Backend;` → `use crate::backend::Backend;`, `use super::envelope::…` → `use crate::envelope::…`, and `super::native::docker_program` → `crate::docker::docker_program`.
2. CUT `docker_program` (native.rs:82) and `docker_desktop_program` (native.rs:137) + their private helpers/tests into `crates/dml-core/src/docker.rs`; add `pub use dml_core::docker::{docker_program, docker_desktop_program};` in native.rs.
3. Move `impl From<RunnerError> for CmdError` (launcher lib.rs:84-101) verbatim into `dml-core/src/error.rs` (`use crate::runner::RunnerError;`).
4. dml/mod.rs: `pub use dml_core::runner;`. lib.rs `use` lines: `use dml_core::runner::{DmlRunner, RunnerError};` replacing old paths.

- [ ] **Step 2: Make the runner's in-file tests cross-platform**

The `#[cfg(test)]` tests spawn `cmd.exe /C <fixture>.cmd` — they fail on Linux. Rework in `runner.rs`:

```rust
#[cfg(windows)]
fn fixture_runner() -> DmlRunner {
    DmlRunner { program: "cmd.exe".into(), prefix_args: vec!["/C".into()], path_prepend: None, host_label: "wsl", host_hint: "Check WSL: wsl -d dml-arch" }
}
#[cfg(not(windows))]
fn fixture_runner() -> DmlRunner {
    DmlRunner { program: "sh".into(), prefix_args: vec![], path_prepend: None, host_label: "wsl", host_hint: "Check WSL: wsl -d dml-arch" }
}
#[cfg(windows)]
const FIXTURE_EXT: &str = "cmd";
#[cfg(not(windows))]
const FIXTURE_EXT: &str = "sh";
fn fixture(name: &str) -> String {
    format!("{}/tests/fixtures/{}.{}", env!("CARGO_MANIFEST_DIR"), name, FIXTURE_EXT)
}
```
Change every `fixture("ok.cmd")` call to `fixture("ok")` (extension appended by the helper).

- [ ] **Step 3: Create the fixtures**

COPY (plain copy, not git mv — other launcher tests may share the directory; Task 7 sweeps leftovers) these 9 from `launcher/src-tauri/tests/fixtures/` into `crates/dml-core/tests/fixtures/`: `ok.cmd, err.cmd, garbage.cmd, wsl_down.cmd, stream_ok.cmd, stream_crash.cmd, stdin_echo.cmd, interactive_echo.cmd, captured_mixed.cmd`. Then author a `.sh` sibling for each by opening the `.cmd` and porting its output VERBATIM (same JSON bytes, same exit code). Pattern examples:

`ok.sh`:
```sh
#!/bin/sh
echo '{"ok":true,"data":{"games":[{"id":"wow-server-playerbots"}]}}'
```
`wsl_down.sh` (empty stdout, nonzero exit):
```sh
#!/bin/sh
echo 'Error: no distro named dml-arch' >&2
exit 1
```
`stdin_echo.sh`:
```sh
#!/bin/sh
input=$(cat)
printf '{"ok":true,"data":{"echo":"%s"}}\n' "$input"
```
The others follow the same rule: replicate each `echo`/`exit` line of the `.cmd`. `stream_*.sh` print the same NDJSON event lines; `interactive_echo.sh` prints `answer me:`, reads a line, prints `you typed <line>`; `captured_mixed.sh` echoes one line to stdout and one to stderr. Keep JSON payloads byte-identical to the `.cmd` versions.

- [ ] **Step 4: Run** — `cargo test --workspace` on Windows: green (Windows still exercises the `.cmd` path). The `.sh` path is validated by CI in Task 16.

- [ ] **Step 5: Commit** — `refactor: move DmlRunner + docker discovery into dml-core; cross-platform test fixtures`

---

### Task 4: dml-core — bounded/streamed subprocess helpers (`proc.rs`)

**Files:**
- Create: `crates/dml-core/src/proc.rs`
- Modify: `launcher/src-tauri/src/dml/destructive.rs`, `crates/dml-core/src/lib.rs`

**Interfaces:**
- Produces `dml_core::proc::{CapturedRun, run_captured(program,&args,timeout), drain_lines, run_streamed_unbounded, combined_nonempty_lines}` — cut VERBATIM from destructive.rs:463-566 region (the generic subprocess half; everything mentioning titles/volumes/compose stays put). destructive.rs re-exports: `pub use dml_core::proc::{CapturedRun, run_captured, drain_lines, run_streamed_unbounded, combined_nonempty_lines};`

- [ ] **Step 1: Cut the functions + their unit tests** into proc.rs. Their tests that spawn fixture scripts get the same `#[cfg(windows)]`/`sh` treatment as Task 3 (reuse the Task 3 fixture helper pattern locally; add `.sh` siblings for any fixture these tests use that isn't already ported).
- [ ] **Step 2: Run** — `cargo test --workspace` green.
- [ ] **Step 3: Commit** — `refactor: move bounded/draining subprocess helpers into dml-core proc`

---

### Task 5: dml-core — conf-file engine (`conf.rs`)

**Files:**
- Create: `crates/dml-core/src/conf.rs`
- Modify: `launcher/src-tauri/src/dml/config.rs`, `crates/dml-core/{Cargo.toml,src/lib.rs}` (add `serde_yaml_ng = "0.10.0"`)

**Interfaces:**
- Produces `dml_core::conf::{parse_conf, strip_conf_quotes, conf_write, conf_ensure, override_env_write, override_env_remove, parse_override_env, kv_rows, key_browser_rows, KeyBrowserRow, conf_help_lines, dist_sibling, bak_sibling, is_single_line, within_max_len, float_in_range, int_in_range, is_bool01, sanitize_text_value}` — cut VERBATIM from config.rs with their unit tests (especially the byte-parity `conf_write` tests).
- config.rs re-exports ALL of them under the old names (`pub use dml_core::conf::{…};`) — the AC-specific half (`env_name_for`, `route_conf`, `ConfigReader`, `conf_path_in`, `cfg_file_path`, `wow_server_installed`, `is_core_conf_file`, `is_module_conf_name`, `direct_conf_path`, direct-key validators, `conf_reload_cmd`) stays in config.rs untouched.

- [ ] **Step 1: Cut functions + tests** into conf.rs; fix intra-file references (moved fns referencing each other use `crate::conf::` inside dml-core; config.rs's remaining fns reach them through the re-export, no call-site edits).
- [ ] **Step 2: Run** — `cargo test --workspace` green; the conf_write byte-parity tests now run in dml-core.
- [ ] **Step 3: Commit** — `refactor: move conf-file engine into dml-core (byte-parity conf_write et al)`

---

### Task 6: dml-core — engine + compose (`engine.rs`, `compose.rs`)

**Files:**
- Create: `crates/dml-core/src/engine.rs`, `crates/dml-core/src/compose.rs`
- Modify: `launcher/src-tauri/src/dml/native.rs` (shrinks to re-exports + anything wow-specific), `launcher/src-tauri/src/dml/lifecycle.rs`, `crates/dml-core/src/{lib.rs,docker.rs}` (docker.rs folds INTO engine.rs)

**Interfaces:**
- `dml_core::engine`: everything remaining in native.rs — `PsRow, parse_ps_json, game_state, docker_info_args, engine_running, docker_desktop_stop_args, stop_engine, launch_detached, EnsureDecision, ensure_decision, stop_engine_enabled, poll_until_ready` + the Task-3 `docker_program`/`docker_desktop_program` (delete docker.rs, move content here, update the two `pub use` sites).
- `dml_core::compose`: cut from lifecycle.rs — `games_dir_from_env, title_dir_for_id, resolve_compose_dir, compose_file_name, count_running_ids, compose_up_argv, compose_down_argv, is_compose_down, compose_sequence_for_mode, port_listening`.
- STAYS in wow lifecycle.rs (playerbots/WoW-specific): `flush_marker_path, flush_conf_path, flush_heal_flag, bots_flush_confirmed, env_has_db_external_port, db_port_conflict_message, game_port_conflict_lines, check_port_conflicts` (hardcodes WoW ports).
- native.rs / lifecycle.rs re-export every moved name so no call site changes.

- [ ] **Step 1: Cut + re-export + move tests.** If native.rs ends up as pure re-exports, keep the file anyway (path stability until Task 7).
- [ ] **Step 2: Run** — `cargo test --workspace` green.
- [ ] **Step 3: Commit** — `refactor: move docker engine + compose lifecycle into dml-core`

---

### Task 7: Create dml-wow — move the module tree, the parity suites, and re-point the launcher

This is the risk-peak task. It is a MOVE, not a rewrite: no logic changes allowed.

**Files:**
- Move: every remaining `launcher/src-tauri/src/dml/*.rs` → `crates/dml-wow/src/*.rs`; `dml/mod.rs` content merges into `crates/dml-wow/src/lib.rs`
- Move: all 17 `launcher/src-tauri/tests/*_parity.rs` → `crates/dml-wow/tests/`; `launcher/src-tauri/tests/fixtures/` → `crates/dml-wow/tests/fixtures/`
- Modify: `launcher/src-tauri/src/lib.rs` (+ `nativesetup.rs`, `realmlist.rs`, `watch.rs`, `zam.rs`, `wslconfig.rs`, `power.rs` where they mention `crate::dml`), both Cargo.tomls

**Interfaces:**
- Produces crate `dml_wow` whose module paths mirror the old tree exactly: `dml_wow::config::ConfigReader`, `dml_wow::db::DbConfig`, `dml_wow::soap::{exec, SoapConfig}`, `dml_wow::status::read_server_detail`, etc.
- Consumes dml-core via the re-export shims created in Tasks 2-6 (they move along and now live inside dml-wow, still pointing at dml-core — correct and final).

- [ ] **Step 1: Move the sources**

```powershell
git mv launcher/src-tauri/src/dml/accountwide.rs crates/dml-wow/src/accountwide.rs
# … repeat for every remaining file: ahbot, backup, bridge, cachestatus, clientpath,
# commands, config, db, destructive, iteminfo, lan, lanip, lifecycle, maint, modmgr,
# moduletail, modules, native, pages, paperdoll, party, party_specs, restore, soap,
# soap_cmds, stats, status, tuning
```
`crates/dml-wow/src/lib.rs` = the old `dml/mod.rs` module list (the moved helpers already became `pub use dml_core::util::…` in Task 2 — keep those lines). Delete `launcher/src-tauri/src/dml/` entirely.

- [ ] **Step 2: Fix cross-crate references inside the moved modules**

- `rg "crate::" crates/dml-wow/src` — expected hits: `crate::CmdError` (becomes `dml_core::error::CmdError`; add `use dml_core::error::CmdError;` per file) and `crate::dml::…` self-references (become `crate::…`). Fix each; NO other edits.
- db.rs: DELETE `query_async` (db.rs:343-366, the only tauri reference). `rg "query_async" launcher crates` and rewrite each caller in launcher lib.rs to `tauri::async_runtime::spawn_blocking(move || dml_wow::db::query(&cfg, db, &sql))` — the same thing query_async did.
- Doc comments mentioning tauri stay (they're prose).

- [ ] **Step 3: Re-point the launcher**

- launcher Cargo.toml: add `dml-wow = { path = "../../crates/dml-wow" }`; REMOVE deps now unused by launcher: `mysql`, `base64`, `flate2`, `sha1`, `serde_yaml_ng` (keep `reqwest` — zam.rs uses it; verify with `rg "reqwest" launcher/src-tauri/src --files-with-matches`); dml-wow Cargo.toml gains `mysql` (same feature set: `default-features = false, features = ["minimal-rust", "derive", "buffer-pool"]`), `reqwest` (same features), `base64 = "0.22.1"`, `flate2 = "1"`, `sha1 = "0.10"`, `serde_yaml_ng` if any wow module uses it directly (check with rg).
- lib.rs: delete `mod dml;`; PowerShell rewrite then hand-check:
```powershell
(Get-Content launcher/src-tauri/src/lib.rs -Raw) -replace 'crate::dml::', 'dml_wow::' | Set-Content launcher/src-tauri/src/lib.rs -Encoding utf8
```
  Same rewrite in nativesetup.rs, realmlist.rs, watch.rs, zam.rs, wslconfig.rs, power.rs (run `rg "crate::dml" launcher/src-tauri/src` until zero). `use crate::dml::…` lines become `use dml_wow::…`.
- Keep `pub use dml_core::error::CmdError;` in lib.rs (command signatures unchanged).

- [ ] **Step 4: Move the parity suites**

`git mv` all 17 `tests/*_parity.rs` + `tests/fixtures/` to `crates/dml-wow/tests/`. In each: `use launcher_lib::dml::` → `use dml_wow::`. Their `CARGO_MANIFEST_DIR/../../` repo-root hops still resolve (crates/dml-wow is the same depth as launcher/src-tauri). If any launcher in-file test still references `tests/fixtures/`, copy just those fixtures back under `launcher/src-tauri/tests/fixtures/` (rg for `tests/fixtures` in launcher src after the move).

- [ ] **Step 5: Run** — `cargo test --workspace`: total test count across dml-core+dml-wow+launcher equals the old 818 + parity files all compile (self-skip locally without env vars, pass with them). `cargo check -p launcher` clean.

- [ ] **Step 6: Commit** — `refactor: extract dml module tree into dml-wow crate; parity suites move with it` (several intermediate commits during this task are fine — each must build).

---

### Task 8: Embed the three static registries in dml-wow

Kills the last bash-`dml` dependency on the native manage path (launcher native mode currently shells `dml wow config registry --json` etc. once per session; the CLI must not need the bash script for manage commands).

**Files:**
- Create: `crates/dml-wow/src/registry.rs`, `crates/dml-wow/data/{config-registry.json,tuning-registry.json,module-catalog.json}`
- Modify: `crates/dml-wow/src/lib.rs`, `launcher/src-tauri/src/lib.rs` (AppState loses the three cache fields + the bash fetch/prefetch code)

**Interfaces:**
- Produces:
```rust
pub fn config_registry_rows() -> &'static [serde_json::Value]   // .data.settings of `dml wow config registry --json`
pub fn tuning_registry_rows() -> &'static [serde_json::Value]   // .data.settings of tuning-registry (13 rows)
pub fn module_catalog() -> &'static serde_json::Value           // .data of `dml wow module catalog --json`
```
backed by `include_str!("../data/…")` + `std::sync::LazyLock` (std, Rust ≥1.80 — no new dep).

- [ ] **Step 1: Generate the snapshots from the bash oracle** (Git Bash, repo root):

```bash
"/c/Program Files/Git/bin/bash.exe" cli/dml wow config registry --json | jq .data.settings > crates/dml-wow/data/config-registry.json
bash cli/dml wow config tuning-registry --json | jq .data.settings > crates/dml-wow/data/tuning-registry.json
bash cli/dml wow module catalog --json | jq .data > crates/dml-wow/data/module-catalog.json
```
(jq is available inside the `dml-arch` WSL distro if not on the host: `wsl -d dml-arch -u dml -- bash -lc 'cd /mnt/c/Users/perzi/dads-mmo-lab && …'`. Files must be LF, pretty-printed by jq is fine — the parser doesn't care.)

- [ ] **Step 2: Failing tests** in registry.rs:

```rust
#[test]
fn config_registry_parses_and_has_known_key() {
    let rows = config_registry_rows();
    assert!(!rows.is_empty());
    assert!(rows.iter().all(|r| r.get("key").is_some()));
    assert!(rows.iter().any(|r| r["key"] == "server.motd"));
}
#[test]
fn tuning_registry_is_13_rows() { assert_eq!(tuning_registry_rows().len(), 13); }
#[test]
fn module_catalog_parses() { assert!(module_catalog().is_object() || module_catalog().is_array()); }
```
(If generation shows tuning ≠ 13 rows, fix the assertion to the real count — the memory value is 13.)

- [ ] **Step 3: Implement** registry.rs with LazyLock statics; `pub mod registry;` in lib.rs.

- [ ] **Step 4: Switch the launcher to the embedded registries**

In launcher lib.rs: find every consumer of `AppState.config_registry`/`tuning_registry`/`module_catalog` (rg the field names). Replace the "fetch via runner if None, then cache" logic with direct `dml_wow::registry::…()` calls. Delete: the three AppState fields, their `Arc<Mutex<…>>` init in `run()`, and the startup prefetch block that spawned bash for them. The bash CLI's registry arms stay (WSL mode + oracle for parity).

- [ ] **Step 5: Run** — `cargo test --workspace`; then with the live snapshot env set (see Task 18 for the exact env), `cargo test -p dml-wow --test config_parity --test tuning_parity --test module_parity` — these deep-equal the embedded-registry-fed readers against live bash output. If they diverge, the snapshot was stale → regenerate Step 1.

- [ ] **Step 6: Commit** — `feat: embed static config/tuning/module registries in dml-wow (native path no longer shells bash)`

---

### Task 9: Hoist the remaining `*_blocking` orchestration bodies from launcher lib.rs into dml-wow

lib.rs holds Tauri-free functions like `wow_module_remove_native_blocking(family, key, backup, db_cfg, emit)` (lib.rs:1049-1083) next to their `#[tauri::command]` adapters. The CLI needs them, so they move to the library.

**Files:**
- Modify: `launcher/src-tauri/src/lib.rs` (shrinks), the matching `crates/dml-wow/src/*.rs` modules

**Interfaces:**
- Produces, per family, `pub fn`s in dml-wow named by stripping the `wow_`/`games_` prefix and `_native_blocking` suffix, with `_stream` appended when they take an emit callback. Signatures otherwise VERBATIM (same params, same `impl Fn(serde_json::Value)` emit). Examples:
  - lib.rs `wow_module_remove_native_blocking(family, key, backup, db_cfg, emit)` → `dml_wow::modmgr::module_remove_stream(family: String, key: String, backup: Option<bool>, db_cfg: DbConfig, emit: impl Fn(serde_json::Value))`
  - lib.rs `wow_module_update_native_blocking(key, emit)` → `dml_wow::modmgr::module_update_stream(key, emit)`
  - the games start/stop/restart native orchestration bodies → `dml_wow::lifecycle::{games_start_stream, games_stop_stream, games_restart_stream}` (whatever args they already take)
  - backup/restore/destructive (docker-clean, bots-flush, self-update, games-remove) bodies → their family modules, same rule.

- [ ] **Step 1: Enumerate** — `rg "fn \w+_blocking" launcher/src-tauri/src/lib.rs` (also `rg "spawn_blocking" -A2` for closures with inline bodies >10 lines). Build the full move list before cutting anything.
- [ ] **Step 2: Move family by family** (modules → lifecycle → backup/restore → destructive → any stragglers), one commit per family. The `#[tauri::command]` wrapper keeps its name and signature and becomes: resolve inputs (e.g. `DbConfig::from_env()`), `spawn_blocking(move || dml_wow::modmgr::module_remove_stream(…, |v| { let _ = ch.send(v); }))`. Rule: a wrapper body over ~6 lines means something didn't move.
- [ ] **Step 3: Guard**: `rg "fn .*_blocking" launcher/src-tauri/src/lib.rs` returns nothing at the end; `#[tauri::command]` count unchanged (243-line grep baseline from planning: the exact number of `#[tauri::command]` attributes before == after).
- [ ] **Step 4: Run** — `cargo test --workspace` after each family; commit per family: `refactor: hoist <family> native orchestration into dml-wow`.

---

### Task 10: CLI scaffold (`dml-wow-cli`)

**Files:**
- Create: `crates/dml-wow-cli/src/{main.rs,cli.rs,out.rs,run.rs}`
- Modify: `crates/dml-wow-cli/Cargo.toml` (add `clap = { version = "4", features = ["derive"] }`)

**Interfaces:**
- Produces binary `dml-wow`; every command prints EXACTLY ONE envelope (or an NDJSON stream) on stdout; exit 0 iff ok. Consumes `dml_core::envelope::{ok_envelope, error_envelope}`, `dml_wow::status::…`, `dml_wow::config::ConfigReader`, `dml_wow::db::DbConfig`, `dml_wow::soap::SoapConfig`, `dml_core::engine::docker_program`.
- Title/config resolution = IDENTICAL to launcher native mode: `ConfigReader::title_dir_from_env()` / `*::from_env()` readers / `DbConfig::from_env()` / the same `~/.dml/soap.env` SoapConfig loading lib.rs uses today (find it with `rg "SoapConfig" launcher/src-tauri/src/lib.rs` and call the same constructor). Env vars are the interface: `DML_GAMES_DIR`, `DML_BASH`, `DML_SCRIPT` (install only), DB/SOAP env — no new config file.

- [ ] **Step 1: Failing tests** (in `cli.rs` / `out.rs`):

```rust
#[test]
fn parses_version() {
    let c = Cli::try_parse_from(["dml-wow", "version"]).unwrap();
    assert!(matches!(c.command, Cmd::Version));
}
#[test]
fn unknown_subcommand_is_usage_error() {
    assert!(Cli::try_parse_from(["dml-wow", "definitely-not-a-cmd"]).is_err());
}
#[test]
fn console_tail_takes_lines() {
    let c = Cli::try_parse_from(["dml-wow", "console-tail", "--lines", "50"]).unwrap();
    assert!(matches!(c.command, Cmd::ConsoleTail { lines: 50 }));
}
```
out.rs tests: `emit_ok(json!({"x":1}))` returns exit code 0 and the serialized envelope string equals `dml_core::envelope::ok_envelope(...)`'s serialization; `emit_err("X","y","")` → code 1.

- [ ] **Step 2: Implement**

`cli.rs`:
```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "dml-wow", version, about = "DML per-game CLI for the WoW (AzerothCore + playerbots) server — JSON envelopes on stdout")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Cmd,
}

#[derive(Subcommand)]
pub enum Cmd {
    /// CLI + contract version envelope
    Version,
    /// Full server status (containers, SOAP, bots, ports)
    Status,
    /// SOAP server-info fields only
    ServerInfo,
    /// Last worldserver console lines
    ConsoleTail {
        // ADDED 2026-07-27: the 1..=1000 bound is NOT optional — both the
        // launcher (lib.rs wow_console_tail_read) and the bash CLI enforce it
        // before calling read_console_tail. Omitting it is a GUI-vs-CLI parity
        // gap and lets an unbounded value reach `docker logs --tail N`.
        #[arg(long, default_value_t = 200, value_parser = clap::value_parser!(u32).range(1..=1000))]
        lines: u32,
    },
}
```
`out.rs`: `pub fn emit_ok(data: Value) -> i32` (println envelope, 0), `pub fn emit_err(code,&str…) -> i32` (println error envelope, 1), `pub fn stream_sink() -> impl Fn(Value)` (println each event, flush stdout after each line), plus `pub fn stream_exit(saw: &TerminalSeen) -> i32` — track whether a `done` (0) or `error` (1) event ended the stream (same terminal-event rule as `runner.rs::is_terminal`).
`run.rs`: match on Cmd →
  - Version → `emit_ok(json!({"version": env!("CARGO_PKG_VERSION"), "contract": "dml-json-v3", "backend": "native"}))`
  - Status → `dml_wow::status::read_server_detail(&docker_program(), &soap_cfg, &db_cfg, &mut ConfigReader::from_env())` → emit_ok
  - ServerInfo → `read_server_info(&soap_cfg)`. CORRECTED 2026-07-27: `Err(())` means **auth failure only** — an unreachable/down server returns `Ok(server_info_down())` (down is data, not an error). Map it the way the launcher already does: `emit_err("SOAP_AUTH", "SOAP authentication failed", "Check ~/.dml/soap.env")`. The original "SOAP_UNREACHABLE / Is the server running?" wording was wrong and would misdirect a user whose server is up with bad credentials.
  - ConsoleTail → `read_console_tail(&docker_program(), lines)` → emit_ok
`main.rs`: `Cli::try_parse()` — on clap error, print clap's message to stderr AND `error_envelope("BAD_ARGS", <first line of clap error>, "dml-wow --help")` to stdout, exit 2. Otherwise `std::process::exit(run::dispatch(cli))`.

- [ ] **Step 3: Run** — `cargo test -p dml-wow-cli`; then manual smoke against the live snapshot: `cargo run -p dml-wow-cli -- version` and (server up) `cargo run -p dml-wow-cli -- status`.
- [ ] **Step 4: Commit** — `feat: dml-wow CLI scaffold (version/status/server-info/console-tail, envelope contract)`

---

### Task 11: CLI — config, tuning, modules(read), registries

**Files:** Modify `crates/dml-wow-cli/src/{cli.rs,run.rs}` (this shape repeats for Tasks 12-15).

Subcommand → library mapping (args mirror the bash `dml wow …` arms; look each function's exact signature up in its module — all verified to exist):

| Subcommand | Call |
|---|---|
| `config list` | `ConfigReader::from_env()` value-read path used by launcher's `wow_config_read` (registry rows + live values) |
| `config get <KEY>` | same reader, single row (error `NOT_FOUND` if key absent from `registry::config_registry_rows()`) |
| `config set <KEY> <VALUE>` | the hoisted config-set path (route_conf/env vs conf file, validation identical to launcher) |
| `config registry` | `emit_ok(json!({"settings": registry::config_registry_rows()}))` |
| `tuning list` | `TuningReader::from_env()` (launcher `wow_tuning_read` path) |
| `tuning set <KEY> <VALUE>` | hoisted tuning-set (validates via `tuning::validate_tuning_value`) |
| `module list` | `ModuleReader::from_env()` (launcher `wow_module_read` path) |
| `module catalog` | `emit_ok` of `registry::module_catalog()` |
| `config files` / `config read <NAME>` / `config write <NAME>` (stdin body) | the raw file list/read/write paths; `write` MUST enforce the same allowlist as the launcher/bash (`is_module_conf_name`, the two protected names rejected) — test this: `config write .env` → error envelope, no file touched |

- [ ] Steps: failing parse tests per subcommand (same style as Task 10) → implement → `cargo test --workspace` → manual smoke (`config list`, `tuning list`, `module list` against snapshot; deep-diff `config list` output `.data` against `bash cli/dml wow config list --json | jq .data` — should match except documented server.motd caveat) → commit `feat: dml-wow CLI config/tuning/module read+write commands`.

---

### Task 12: CLI — database page reads

| Subcommand | Call (all take `DbConfig::from_env()`) |
|---|---|
| `players-online` | `pages::read_players_online` |
| `accounts` | `pages::read_accounts` |
| `bots [--class N] [--limit N] [--offset N] …]` | `pages::read_bots(cfg, &BotFilters{…})` (flags mirror BotFilters fields — read the struct) |
| `teleport-list [--search S]` | `pages::read_teleport_list(cfg, search)` |
| `items-search <opts>` | `pages::read_items_search(cfg, &ItemSearchOpts{…})` (flags mirror struct) |
| `paperdoll <NAME>` | `paperdoll::read_paperdoll(cfg, name)` — validate with `paperdoll::valid_charname` first; None → `NOT_FOUND` envelope |
| `char-progress <NAME>` | `pages::read_char_progress` |
| `achievements <NAME>` | `pages::read_achievements` |
| `stats` | `stats::read_stats` |
| `item-info <ID>[,<ID>…]` | `iteminfo::read_item_info(cache_dir()…, Some(&cfg), &ids)` |

DbError mapping: every `Err(DbError)` → `emit_err("DB_UNREACHABLE", <display>, …)`. CORRECTED 2026-07-27: the hint string originally written here was invented and matched nothing. Do NOT hand-copy a string — reuse the launcher's single source, `db_err_to_cmd` (crates/dml-wow/src/db.rs), whose real hint is "Is ac-database running? (native mode reads MySQL directly on 127.0.0.1)" (the bash CLI uses the shorter "Is ac-database running?"). Making that helper public is preferable to a second copy that can drift.

- [ ] Steps: parse tests → implement → workspace tests → smoke vs live snapshot (compare `stats` against `bash cli/dml wow stats --json`) → commit `feat: dml-wow CLI database page reads`.

---

### Task 13: CLI — SOAP actions, accounts, GM, party

All build a command string via the existing validated builders, then `soap::exec(&cfg, &cmd)` under the outcome mappers (`soap_cmds::outcome_to_result_decoded` / `_raw`) — CLI is a one-shot process so no in-process soap lock is needed; cross-process serialization is NOT provided (same as launcher-vs-bash today; documented in the contract doc caveats).

| Subcommand | Builder |
|---|---|
| `console <COMMAND…>` | raw `soap::exec` with the joined string (the launcher's console-send path) |
| `account create <USER> <PASS>` | `soap_cmds::account_create_cmd` |
| `account set-password <USER> <PASS>` | `account_set_password_cmd` |
| `account set-gm <USER> <LEVEL>` | `account_set_gm_cmd` |
| `account delete <USER>` | `account_delete_cmd` |
| `gm level/gold/heal/revive/summon/at-login …` | the six `gm_*_cmd` builders |
| `mail-item <CHAR> <ITEMSPEC…> [--subject S] [--text T]` | `soap_cmds::mail_items_cmd` |
| `teleport <CHAR> <DEST>` | `teleport_name_cmd` |
| `motd <TEXT>` | `motd_cmd` |
| `party add/kick/relogin/botcmd/preset-save/preset-list/preset-delete/preset-load …` | `party::*` builders + the hoisted party orchestration fns from Task 9 (preset-load streams via `stream_sink()`) |

- [ ] Steps: parse tests (include one per validator rejection: e.g. `gm level` with a bad name → error envelope BEFORE any SOAP call) → implement → workspace tests → smoke: `console server info`, and one mutating smoke on a throwaway char if the server is up → commit `feat: dml-wow CLI SOAP/account/GM/party commands`.

---

### Task 14: CLI — lifecycle, module mutations, backup/restore, destructive (with `--yes`)

Streaming commands print NDJSON events via `stream_sink()`; exit code from the terminal event.

| Subcommand | Call | Guard |
|---|---|---|
| `start` / `stop` / `restart` | Task 9's `lifecycle::games_lifecycle_stream(mode, id, skip_saveall, emit)` — ONE mode-dispatched fn, not three (amended 2026-07-27: splitting it would have been a rewrite, so Task 9 moved it whole; the launcher's own `games_start/stop/restart` commands already dispatch by mode the same way). Engine wrapping is available as `native::ensure_engine_up_stream` / `native::stop_engine_stream`. | — |
| `module install/remove/update/rebuild/repair …` | Task 9's `modmgr::module_*_stream` | `rebuild`,`remove` prompt-free but honor `--backup`/`--no-backup` flags mirroring the launcher args |
| `backup create [--include-world]` / `backup list` / `backup delete <FILE>` / `backup validate <FILE>` | backup module fns (`dump_to`, `list_backups`, `delete_backup`, `validate_backup` — via the Task 9 hoisted orchestrations where they exist) | — |
| `backup restore <FILE>` | hoisted restore orchestration (stop → prerestore safety dump → stream import → restart) | `--yes` REQUIRED |
| `docker-clean --level <1..3>` | hoisted docker-clean stream | `--yes` |
| `bots-flush --ack <TEXT>` | hoisted flush (uses `lifecycle::bots_flush_confirmed(confirm, ack)` — pass `confirm=true` only when `--yes` given, ack from the flag) | `--yes` + `--ack` |
| `games-remove` | hoisted removal | `--yes` |
| `self-update` | hoisted self-update stream | — |

`--yes` missing on a guarded command → `error_envelope("CONFIRM_REQUIRED", "<cmd> is destructive; re-run with --yes", "")`, exit 1, NOTHING executed — write a test for at least `backup restore` and `bots-flush` asserting the guard fires before any side effect (unit-test the dispatch fn with a mock emit; no server needed).

- [ ] Steps: parse+guard tests → implement → workspace tests → smoke (start/stop stream against snapshot server; `backup create` + `backup list`) → commit `feat: dml-wow CLI lifecycle/module/backup/destructive commands with --yes guards`.

---

### Task 15: CLI — misc reads + `install` passthrough

| Subcommand | Call |
|---|---|
| `lan status` / `lan on` / `lan off` / `lan refresh` | the hoisted lan/realmlist fns (launcher realmlist path — realmlist.rs consumes dml_wow; the underlying fns live in dml-wow after Task 9's sweep; if realmlist logic still lives in launcher `realmlist.rs`, move its Tauri-free core to `dml_wow::lan` first, same hoist rule) |
| `cache status` / `cache clean` | `cachestatus::{read_cache_status, clean_cache}` |
| `client-path get/set <DIR>/detect` | `clientpath::{read_client_path, set_client_path, detect_client(default_scan_roots())}` |
| `accountwide get/set <FLAG> <VALUE> [--variant V]` | `accountwide::{build_get, set_flag}` (server dir via `maint::resolve_server_dir`) |
| `commands` | `commands::assemble_commands(registry::module_catalog(), &ModuleReader::from_env(), <modules dir>)` (mirror the launcher call site) |
| `install [<TITLE-ID>]` | interactive passthrough — see below |

`install` (spec deviation, deliberate): the installers are interactive bash scripts; capturing them as NDJSON would starve their prompts. Implementation: preflight — resolve bash via the same lookup `DmlRunner::native()` uses (factor `find_bash`/`find_dml_script` in `dml_core::runner` to `pub(crate)`→`pub`) and check the script exists; missing → `error_envelope("INSTALL_PREREQS", "Git Bash (or bash on PATH) and the dml script (DML_SCRIPT) are required for install", …)`. Then spawn `bash <script> games install <id>` with ALL stdio inherited (`Command::spawn` + `wait`, no pipes), and exit with the child's code. Print NO envelope on the success path (raw installer passthrough is the output). Title id validated with the same `[A-Za-z0-9._-]+` rule as the launcher (`validate_game_id` — move it from launcher lib.rs to `dml_core::util` and re-export in launcher).

- [ ] Steps: parse tests + preflight test (unset DML_SCRIPT + fake bash → INSTALL_PREREQS envelope, nothing spawned) → implement → workspace tests → commit `feat: dml-wow CLI misc commands + interactive install passthrough`.

---

### Task 16: CI — build + test both platforms

**Files:**
- Create: `.github/workflows/rust.yml`

```yaml
name: rust
on:
  push:
    branches: ["feat/rust-cli-workspace", "spike/**"]
  pull_request:
    paths: ["crates/**", "launcher/src-tauri/**", "Cargo.*", ".github/workflows/rust.yml"]
jobs:
  windows:
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo build --workspace --locked
      - run: cargo test --workspace --locked
  linux:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      # The launcher crate is Tauri/Windows-focused and needs webkit system libs;
      # the community CLI story on Linux is the three crates — test exactly those.
      - run: cargo build -p dml-core -p dml-wow -p dml-wow-cli --locked
      - run: cargo test -p dml-core -p dml-wow -p dml-wow-cli --locked
```

- [ ] Step 1: Write the workflow. Step 2: commit `ci: rust build+test on windows and linux`, push the branch, and WATCH the first run (`"C:\Program Files\GitHub CLI\gh.exe" run watch` or `gh run list --workflow rust.yml`). Expected first-run findings: `.sh` fixture bugs (exit codes, quoting) — fix until both jobs are green. Parity tests self-skip on CI by design (no live server). Do not merge/PR anything — push to the feature branch only.

---

### Task 17: Docs — contract, pitch, README/CLAUDE.md refresh

**Files:**
- Create: `docs/cli-contract.md`, `docs/rust-cli-pitch.md`
- Modify: `README.md` (workspace + CLI section), `CLAUDE.md` (dev-loop: root `cargo test --workspace`, crate map, CLI smoke command), `launcher/README.md` if present

`docs/cli-contract.md` must contain, concretely:
1. Envelope shapes with verbatim JSON examples (ok, error) and the rule "exactly one envelope on stdout, exit 0 iff ok=true; exit 2 = usage (BAD_ARGS envelope)".
2. The NDJSON stream event vocabulary — one example line each for `section_start`, `line` (with `level`), `section_end`, `done`, `error` — and the terminal-event rule (stream ends with done→0 or error→1; a stream that dies without either is a crash, treat exit code as truth).
3. The full command table (name, args, one-line description, value vs stream) — generate the list from `cli.rs` while writing, keep in command order.
4. Environment variables: `DML_GAMES_DIR` (games root; default `~` scan used by `ConfigReader::title_dir_from_env` — copy the real default from the code), DB variables consumed by `DbConfig::from_env` (copy names from db.rs), SOAP variables / `~/.dml/soap.env`, `DML_BASH`/`DML_SCRIPT` (install only).
5. Caveats: `server.motd` shows the registry default when the DB holds a custom MOTD and the DB is unreachable; concurrent CLI processes do not serialize SOAP calls cross-process; backups default to `~/.dml/backups` keep-10.
6. "Attach a frontend" quickstart: spawn `dml-wow status`, parse stdout as one JSON envelope; spawn `dml-wow start` and read NDJSON lines — 10-line pseudo-code example.

`docs/rust-cli-pitch.md`: why per-game binaries on a shared core (Baerthe's 2026-07-23 direction, quoted), what exists today (crate map, test counts, parity methodology vs the bash oracle), how Veil Lab / an Electron app / a plain script attaches (points at the contract doc), what a second game crate would look like (dml-core surface list), honest status: Windows fully exercised against a live server; Linux built+unit-tested in CI, needs a community smoke.

- [ ] Also: refresh `README.md`/`docs/FEATURES.md` via a **sonnet** subagent per the repo's living-docs rule (launcher behavior mostly unchanged — registry prefetch removal and workspace layout are the notable entries).
- [ ] Commit `docs: CLI contract, community pitch, workspace docs refresh`.

---

### Task 18: Final verification gate (evidence before claims)

- [ ] `cargo test --workspace` (Windows) — record total; compare against pre-refactor 818+parity: no lost tests (moved counts must add up; new CLI/core tests on top).
- [ ] Live parity run (start the snapshot server first: Docker Desktop up, then `cargo run -p dml-wow-cli -- start` — eat our own dogfood): run all 17 parity suites with the live env (`$env:DML_GAMES_DIR="C:\Users\perzi\dml-native"`; DML_BASH/DML_SCRIPT/DML_YQ_BIN as in the suites' headers): `cargo test -p dml-wow --tests` — every suite must print PASS (not SKIP) for the file-gated ones whose inputs exist. Any SKIP that used to run = a broken gate, investigate.
- [ ] Launcher regression: `cd launcher; npm test` (vitest 385), `npm run check` (0/0), then `npm run tauri build` — the workspace-Tauri risk gate; confirm the NSIS/MSI bundle and `launcher.exe` appear under the ROOT `target/release/` (workspace target). Launch the exe once, open Settings (registry now embedded — first open should be instant), start/stop the server from Home.
- [ ] CLI release build: `cargo build --release -p dml-wow-cli`; run `target/release/dml-wow.exe version`, `status`, `config list`, `backup list` against the live snapshot.
- [ ] CI green on both jobs (Task 16 pushed; re-check `gh run list`).
- [ ] Update `.superpowers/sdd/progress.md` ledger; update repo CLAUDE.md "Current work" line for the workspace.
- [ ] Commit any doc/ledger stragglers. DO NOT merge. Report the user-facing gates that remain: (a) user click-through of the launcher release exe, (b) a Linux community smoke of the CLI, (c) the pre-existing NATIVE-TAIL-SMOKE checklist (unchanged by this plan).

---

## Task dependency notes for the dispatcher

Strict order: 1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 → 9 → 10 → {11, 12, 13} (parallelizable AFTER 10 if separate worktrees are NOT needed — they all touch cli.rs/run.rs, so run them sequentially in practice) → 14 → 15 → 16 → 17 → 18.

## Spec deviations decided in this plan (already reflected above)

1. `install` is interactive stdio passthrough, not NDJSON-wrapped (installers prompt; NDJSON capture would deadlock them). The spec file gets a one-line amendment.
2. Registry embedding (Task 8) implements the spec's "config/tuning/module registries" line by baking the static rows into dml-wow — this also removes the launcher's one-time ~2s bash registry spawn in native mode.
3. Linux CI tests the three crates, not the launcher package (Tauri needs webkit system libs there; the Linux promise is the CLI).
