# DML Launcher Shell Implementation Plan (Plan 2)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **Prerequisite:** Plan 1 (`docs/superpowers/plans/2026-07-14-dml-cli-json-foundation.md`) must be fully complete — the launcher consumes the `dml … --json` contract it delivers, and the dev machine must have CLI v3.0.0 dev-installed (`powershell -File cli\dev-install.ps1`). Do not execute two plans (or two controller sessions) concurrently on this checkout.

**Goal:** Scaffold the Tauri 2 desktop app (`launcher/`) with a thin Rust shell that runs `wsl.exe -d dml-arch -u dml -- dml … --json`, and a Svelte UI with a game library page and The Lab-style embedded terminal streaming NDJSON install/start output.

**Architecture:** GUI is a thin shell (spec §5.1): every feature calls the `dml` CLI. Rust owns process spawning, envelope/NDJSON parsing, and streaming to the frontend over a Tauri `Channel`; Svelte owns rendering. Terminal display logic is a pure TypeScript reducer (`applyEvent`) so it is unit-testable without a DOM.

**Tech Stack:** Tauri 2.x (Rust 1.97 MSVC, WebView2), Svelte 5 + TypeScript + Vite (create-tauri-app template), vitest for TS unit tests, cargo test for Rust. No extra Tauri plugins — process spawning uses `std::process` in the app core.

## Global Constraints

- Repo: `C:\Users\perzi\dads-mmo-lab`, branch `feat/dml-launcher-windows`. Commit after every task.
- New code lives in `launcher/` (spec §13). App identifier `com.dadsmmolab.launcher`, product name `DML Launcher`. License AGPL-3.0 like the rest of the repo.
- Production CLI invocation is exactly: program `wsl.exe`, args `-d dml-arch -u dml -- dml <subcommand…> --json` (spec §5; proven by `cli/tests/windows-smoke.ps1`).
- CLI contract (from `cli/README.md`, Plan 1): success envelope `{"ok":true,"data":<object>}`; error envelope `{"ok":false,"error":{"code","message","hint"}}` with exit 1; NDJSON streams for `games start|stop|restart` — `section_start`, `line` (level info|warn|error), `section_end`, one terminal `done`/`error`; `pct` reserved (the UI must IGNORE unknown events, not crash).
- Windows-only APIs (`CREATE_NO_WINDOW`, the `wsl.exe` program name) are confined to `cfg(windows)` blocks / the runner's construction site — the parsing and streaming core stays cross-platform (spec: "no Windows-only APIs in the core").
- Game ids passed to the CLI must be validated in Rust against `[A-Za-z0-9._-]+` before spawning (defense against argument injection).
- Do not modify anything under `cli/` or `guides/` in this plan.
- Dev-loop commands run from repo root in PowerShell unless stated. Rust tests: `cd launcher\src-tauri; cargo test`. TS tests: `cd launcher; npm test`. Type/lint check: `cd launcher; npm run check`.
- TDD is required for every task that adds logic (Tasks 2–6). Scaffold (Task 1) and pure-UI wiring (Task 7) verify by build/check + scripted smoke instead.
- Automated UI driving (tauri-driver + WebdriverIO, spec §10) is deliberately DEFERRED to a later plan — this plan covers the UI with the pure reducer tests plus the scripted live smoke in Task 7. Do not add tauri-driver here.

---

### Task 1: Scaffold the Tauri app in `launcher/`

**Files:**
- Create: `launcher/` (entire create-tauri-app scaffold: `src/`, `src-tauri/`, `package.json`, `vite.config.ts`, …)
- Modify: `launcher/src-tauri/tauri.conf.json` (identifier, productName, window)
- Modify: `launcher/package.json` (name)

**Interfaces:**
- Produces: a building Tauri 2 + Svelte 5 + TS app at `launcher/`; `npm run check` (svelte-check) and `cargo check` both clean. Later tasks add files under `launcher/src/lib/` and `launcher/src-tauri/src/`.
- Consumes: toolchain already on the machine (Rust 1.97 MSVC, Node 22, WebView2 — verified 2026-07-14).

- [ ] **Step 1: Generate the scaffold**

From repo root:
```powershell
npm create tauri-app@latest launcher -- --template svelte-ts --manager npm --yes
cd launcher
npm install
```
Expected: scaffold created under `launcher/` with `src-tauri/` inside; `npm install` completes without errors. If `--yes` is not accepted by the current create-tauri-app version, answer prompts: name `launcher`, identifier `com.dadsmmolab.launcher`, frontend `TypeScript / JavaScript` → `npm` → `Svelte` → `TypeScript`.

- [ ] **Step 2: Set identity and window config**

In `launcher/src-tauri/tauri.conf.json` set these keys (leave the rest as generated):
```json
{
  "productName": "DML Launcher",
  "identifier": "com.dadsmmolab.launcher",
  "app": {
    "windows": [
      {
        "title": "DML Launcher",
        "width": 1100,
        "height": 750,
        "minWidth": 900,
        "minHeight": 600
      }
    ]
  }
}
```
In `launcher/package.json` set `"name": "dml-launcher"`.

- [ ] **Step 3: Verify it builds**

```powershell
cd launcher; npm run check
cd src-tauri; cargo check
```
Expected: svelte-check reports 0 errors; `cargo check` finishes (first run compiles the Tauri dependency tree — several minutes is normal). Then confirm git hygiene:
```powershell
cd ..\..; git status --short
```
Expected: new `launcher/` files listed, but NO `node_modules/` and NO `src-tauri/target/` entries (the scaffold's .gitignore files must cover both — if either appears, stop and fix the .gitignore before committing).

- [ ] **Step 4: Commit**

```powershell
git add launcher
git commit -m "feat(launcher): scaffold Tauri 2 + Svelte 5 app shell (com.dadsmmolab.launcher)"
```

---

### Task 2: Rust envelope parsing + WSL output decoding — TDD

**Files:**
- Create: `launcher/src-tauri/src/dml/mod.rs`
- Create: `launcher/src-tauri/src/dml/envelope.rs`
- Modify: `launcher/src-tauri/src/lib.rs` (add `mod dml;`)

**Interfaces:**
- Produces (Rust, `crate::dml::envelope`):
  - `pub struct ErrorInfo { pub code: String, pub message: String, pub hint: String }` (Deserialize + Serialize + Clone + Debug + PartialEq; `hint` defaults to `""`)
  - `pub struct Envelope { pub ok: bool, pub data: serde_json::Value, pub error: Option<ErrorInfo> }` (Deserialize + Debug; `data` defaults to `Value::Null`)
  - `pub fn parse_envelope(s: &str) -> Result<Envelope, String>` (Err carries a description including the raw text)
  - `pub fn decode_wsl_output(bytes: &[u8]) -> String` — UTF-8 by default; detects UTF-16LE (wsl.exe emits UTF-16 for its OWN error messages, e.g. unknown distro) and decodes it; lossy on invalid sequences.
- Consumes: `serde`, `serde_json` (present in the template's Cargo.toml; add `serde` `features = ["derive"]` if missing).

- [ ] **Step 1: Write the failing tests**

Create `launcher/src-tauri/src/dml/mod.rs`:
```rust
pub mod envelope;
```

Create `launcher/src-tauri/src/dml/envelope.rs` with ONLY the test module first:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ok_envelope() {
        let env = parse_envelope(r#"{"ok":true,"data":{"version":"3.0.0"}}"#).unwrap();
        assert!(env.ok);
        assert_eq!(env.data["version"], "3.0.0");
        assert!(env.error.is_none());
    }

    #[test]
    fn parses_error_envelope_with_default_hint() {
        let env = parse_envelope(
            r#"{"ok":false,"error":{"code":"NOT_FOUND","message":"Title not found: nope"}}"#,
        )
        .unwrap();
        assert!(!env.ok);
        let e = env.error.unwrap();
        assert_eq!(e.code, "NOT_FOUND");
        assert_eq!(e.hint, "");
    }

    #[test]
    fn garbage_is_err_and_carries_raw_text() {
        let err = parse_envelope("wsl: unknown distro").unwrap_err();
        assert!(err.contains("wsl: unknown distro"));
    }

    #[test]
    fn decodes_plain_utf8() {
        assert_eq!(decode_wsl_output(b"dml v3.0.0\n"), "dml v3.0.0\n");
    }

    #[test]
    fn decodes_utf16le_from_wsl_exe() {
        // "hi" as UTF-16LE
        let bytes: &[u8] = &[b'h', 0, b'i', 0];
        assert_eq!(decode_wsl_output(bytes), "hi");
    }
}
```
(Add `use serde::{Deserialize, Serialize};` etc. as part of Step 3 — right now the module intentionally fails to compile.)

- [ ] **Step 2: Run tests to verify they fail**

Add `mod dml;` near the top of `launcher/src-tauri/src/lib.rs`. Then:
```powershell
cd launcher\src-tauri; cargo test
```
Expected: FAIL to compile — `parse_envelope` / `decode_wsl_output` / types not found.

- [ ] **Step 3: Implement**

Prepend to `launcher/src-tauri/src/dml/envelope.rs` (above the test module):
```rust
use serde::{Deserialize, Serialize};

fn default_hint() -> String {
    String::new()
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ErrorInfo {
    pub code: String,
    pub message: String,
    #[serde(default = "default_hint")]
    pub hint: String,
}

#[derive(Debug, Deserialize)]
pub struct Envelope {
    pub ok: bool,
    #[serde(default)]
    pub data: serde_json::Value,
    #[serde(default)]
    pub error: Option<ErrorInfo>,
}

pub fn parse_envelope(s: &str) -> Result<Envelope, String> {
    serde_json::from_str(s.trim())
        .map_err(|e| format!("unparseable dml output ({e}): {}", s.trim()))
}

/// wsl.exe relays the guest's UTF-8 bytes, but its OWN messages (bad distro,
/// WSL not installed) are UTF-16LE. Detect the NUL pattern and decode.
pub fn decode_wsl_output(bytes: &[u8]) -> String {
    let looks_utf16 = bytes.len() >= 2
        && bytes.len() % 2 == 0
        && bytes.iter().skip(1).step_by(2).filter(|b| **b == 0).count() > bytes.len() / 4;
    if looks_utf16 {
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

```powershell
cd launcher\src-tauri; cargo test
```
Expected: `test result: ok. 5 passed` (plus any template tests). Output pristine — fix any warnings (unused imports etc.) now.

- [ ] **Step 5: Commit**

```powershell
git add launcher/src-tauri/src
git commit -m "feat(launcher): dml envelope parser + UTF-16-aware wsl output decoding"
```

---

### Task 3: Rust runner `run_json` (blocking, fixture-tested) — TDD

**Files:**
- Create: `launcher/src-tauri/src/dml/runner.rs`
- Modify: `launcher/src-tauri/src/dml/mod.rs` (add `pub mod runner;`)
- Create: `launcher/src-tauri/tests/fixtures/ok.cmd`
- Create: `launcher/src-tauri/tests/fixtures/err.cmd`
- Create: `launcher/src-tauri/tests/fixtures/garbage.cmd`

**Interfaces:**
- Consumes: `parse_envelope`, `decode_wsl_output`, `Envelope` (Task 2).
- Produces (Rust, `crate::dml::runner`):
  - `pub struct DmlRunner { pub program: std::ffi::OsString, pub prefix_args: Vec<String> }`
  - `impl Default for DmlRunner` → program `wsl.exe`, prefix `["-d","dml-arch","-u","dml","--","dml"]`
  - `pub fn run_json(&self, args: &[&str]) -> Result<Envelope, RunnerError>` — appends `--json`, spawns, decodes stdout, parses envelope. An error ENVELOPE is `Ok(env)` with `env.ok == false` (the CLI's exit 1 is expected there); unparseable output is `Err(RunnerError::BadOutput{..})`.
  - `pub enum RunnerError { Spawn(String), BadOutput { raw: String } }` (Debug + Display)
  - Windows child processes are spawned with `CREATE_NO_WINDOW` (no console flash).
- **Test seam (used by Tasks 3–5):** tests construct `DmlRunner { program: "cmd.exe".into(), prefix_args: vec!["/C".into()] }` and pass a fixture path as the only arg — the runner is agnostic about what it spawns. Fixtures must therefore tolerate the appended `--json` arg (cmd ignores extra args to a script).

- [ ] **Step 1: Write fixtures and failing tests**

Create `launcher/src-tauri/tests/fixtures/ok.cmd`:
```bat
@echo {"ok":true,"data":{"games":[{"id":"wow-server-playerbots","path":"/home/dml/games/wow-server-playerbots","running":false}]}}
```
Create `launcher/src-tauri/tests/fixtures/err.cmd`:
```bat
@echo {"ok":false,"error":{"code":"NOT_FOUND","message":"Title not found: nope","hint":"Run: dml games list --json"}}
@exit /b 1
```
Create `launcher/src-tauri/tests/fixtures/garbage.cmd`:
```bat
@echo not json at all
```

Create `launcher/src-tauri/src/dml/runner.rs` with the test module first:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_runner() -> DmlRunner {
        DmlRunner {
            program: "cmd.exe".into(),
            prefix_args: vec!["/C".into()],
        }
    }

    fn fixture(name: &str) -> String {
        format!("{}/tests/fixtures/{}", env!("CARGO_MANIFEST_DIR"), name)
    }

    #[test]
    fn run_json_parses_ok_envelope() {
        let env = fixture_runner().run_json(&[&fixture("ok.cmd")]).unwrap();
        assert!(env.ok);
        assert_eq!(env.data["games"][0]["id"], "wow-server-playerbots");
    }

    #[test]
    fn run_json_returns_error_envelope_as_ok_false() {
        let env = fixture_runner().run_json(&[&fixture("err.cmd")]).unwrap();
        assert!(!env.ok);
        assert_eq!(env.error.unwrap().code, "NOT_FOUND");
    }

    #[test]
    fn run_json_garbage_is_bad_output() {
        match fixture_runner().run_json(&[&fixture("garbage.cmd")]) {
            Err(RunnerError::BadOutput { raw }) => assert!(raw.contains("not json")),
            other => panic!("expected BadOutput, got {other:?}"),
        }
    }

    #[test]
    fn run_json_missing_program_is_spawn_error() {
        let r = DmlRunner { program: "definitely-not-a-real-exe-9f2.exe".into(), prefix_args: vec![] };
        assert!(matches!(r.run_json(&["x"]), Err(RunnerError::Spawn(_))));
    }

    #[test]
    fn default_runner_targets_wsl_dml() {
        let r = DmlRunner::default();
        assert_eq!(r.program, std::ffi::OsString::from("wsl.exe"));
        assert_eq!(r.prefix_args, vec!["-d", "dml-arch", "-u", "dml", "--", "dml"]);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Add `pub mod runner;` to `launcher/src-tauri/src/dml/mod.rs`, then:
```powershell
cd launcher\src-tauri; cargo test
```
Expected: compile FAIL — `DmlRunner`/`RunnerError` not defined.

- [ ] **Step 3: Implement**

Prepend to `launcher/src-tauri/src/dml/runner.rs`:
```rust
use std::ffi::OsString;
use std::process::Command;

use super::envelope::{decode_wsl_output, parse_envelope, Envelope};

#[derive(Debug)]
pub enum RunnerError {
    Spawn(String),
    BadOutput { raw: String },
}

impl std::fmt::Display for RunnerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunnerError::Spawn(e) => write!(f, "failed to run dml via WSL: {e}"),
            RunnerError::BadOutput { raw } => write!(f, "dml produced unexpected output: {raw}"),
        }
    }
}

pub struct DmlRunner {
    pub program: OsString,
    pub prefix_args: Vec<String>,
}

impl Default for DmlRunner {
    fn default() -> Self {
        DmlRunner {
            program: "wsl.exe".into(),
            prefix_args: ["-d", "dml-arch", "-u", "dml", "--", "dml"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
        }
    }
}

impl DmlRunner {
    fn command(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.prefix_args).args(args).arg("--json");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        cmd
    }

    pub fn run_json(&self, args: &[&str]) -> Result<Envelope, RunnerError> {
        let out = self
            .command(args)
            .output()
            .map_err(|e| RunnerError::Spawn(e.to_string()))?;
        let stdout = decode_wsl_output(&out.stdout);
        parse_envelope(&stdout).map_err(|_| RunnerError::BadOutput { raw: stdout.clone() })
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

```powershell
cd launcher\src-tauri; cargo test
```
Expected: 10 passed (5 from Task 2 + 5 new), zero warnings.

- [ ] **Step 5: Commit**

```powershell
git add launcher/src-tauri/src launcher/src-tauri/tests
git commit -m "feat(launcher): DmlRunner.run_json with cmd-fixture tests and CREATE_NO_WINDOW"
```

---

### Task 4: Rust runner `run_stream` (NDJSON line streaming + crash synthesis) — TDD

**Files:**
- Modify: `launcher/src-tauri/src/dml/runner.rs`
- Create: `launcher/src-tauri/tests/fixtures/stream_ok.cmd`
- Create: `launcher/src-tauri/tests/fixtures/stream_crash.cmd`

**Interfaces:**
- Consumes: Task 3's `DmlRunner`, `RunnerError`; Task 2's `decode_wsl_output`.
- Produces:
  - `pub fn run_stream(&self, args: &[&str], on_event: impl FnMut(serde_json::Value)) -> Result<i32, RunnerError>` — spawns with piped stdout+stderr, forwards each parseable stdout JSON line to `on_event` in order, wraps non-JSON lines as `{"event":"line","level":"warn","text":<line>}`, skips blank lines. If the process exits non-zero WITHOUT having emitted a terminal `done`/`error` event, synthesizes `{"event":"error","error":{"code":"CLI_CRASH","message":"dml exited with code <n> before finishing","hint":"Check WSL: wsl -d dml-arch"}}`. Returns the exit code.
  - Terminal detection helper `fn is_terminal(v: &serde_json::Value) -> bool` (`event` == `done` or `error`).
- Task 5 calls `run_stream` from a Tauri command and forwards each `Value` over a `Channel`.

- [ ] **Step 1: Write fixtures and failing tests**

Create `launcher/src-tauri/tests/fixtures/stream_ok.cmd`:
```bat
@echo {"event":"section_start","name":"start"}
@echo {"event":"line","level":"info","text":"[dml] staged start: mode=start"}
@echo {"event":"section_end","name":"start","status":"ok"}
@echo {"event":"done","data":{"id":"wow","state":"running"}}
```
Create `launcher/src-tauri/tests/fixtures/stream_crash.cmd`:
```bat
@echo {"event":"line","level":"info","text":"partial work"}
@exit /b 3
```

Append to the `tests` module in `launcher/src-tauri/src/dml/runner.rs`:
```rust
    #[test]
    fn run_stream_forwards_events_in_order() {
        let mut seen: Vec<serde_json::Value> = vec![];
        let code = fixture_runner()
            .run_stream(&[&fixture("stream_ok.cmd")], |v| seen.push(v))
            .unwrap();
        assert_eq!(code, 0);
        assert_eq!(seen.len(), 4);
        assert_eq!(seen[0]["event"], "section_start");
        assert_eq!(seen[3]["event"], "done");
        assert_eq!(seen[3]["data"]["state"], "running");
    }

    #[test]
    fn run_stream_synthesizes_error_on_silent_crash() {
        let mut seen: Vec<serde_json::Value> = vec![];
        let code = fixture_runner()
            .run_stream(&[&fixture("stream_crash.cmd")], |v| seen.push(v))
            .unwrap();
        assert_eq!(code, 3);
        let last = seen.last().unwrap();
        assert_eq!(last["event"], "error");
        assert_eq!(last["error"]["code"], "CLI_CRASH");
        assert!(last["error"]["message"].as_str().unwrap().contains("3"));
    }

    #[test]
    fn run_stream_wraps_non_json_lines_as_warn() {
        // garbage.cmd prints a non-JSON line and exits 0 → wrapped line + CLI_CRASH-free
        let mut seen: Vec<serde_json::Value> = vec![];
        let code = fixture_runner()
            .run_stream(&[&fixture("garbage.cmd")], |v| seen.push(v))
            .unwrap();
        assert_eq!(code, 0);
        assert_eq!(seen[0]["event"], "line");
        assert_eq!(seen[0]["level"], "warn");
        assert!(seen[0]["text"].as_str().unwrap().contains("not json"));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

```powershell
cd launcher\src-tauri; cargo test
```
Expected: compile FAIL — `run_stream` not defined.

- [ ] **Step 3: Implement**

Add to `impl DmlRunner` in `runner.rs` (plus `use std::io::{BufRead, BufReader};` and `use std::process::Stdio;` at the top):
```rust
    pub fn run_stream(
        &self,
        args: &[&str],
        mut on_event: impl FnMut(serde_json::Value),
    ) -> Result<i32, RunnerError> {
        let mut child = self
            .command(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| RunnerError::Spawn(e.to_string()))?;

        let stdout = child.stdout.take().expect("stdout piped above");
        let mut saw_terminal = false;
        for line in BufReader::new(stdout).split(b'\n') {
            let bytes = line.map_err(|e| RunnerError::Spawn(e.to_string()))?;
            let text = decode_wsl_output(&bytes);
            let text = text.trim_end_matches('\r').trim();
            if text.is_empty() {
                continue;
            }
            let value: serde_json::Value = match serde_json::from_str(text) {
                Ok(v) => v,
                Err(_) => serde_json::json!({"event":"line","level":"warn","text": text}),
            };
            if is_terminal(&value) {
                saw_terminal = true;
            }
            on_event(value);
        }

        let status = child.wait().map_err(|e| RunnerError::Spawn(e.to_string()))?;
        let code = status.code().unwrap_or(-1);
        if code != 0 && !saw_terminal {
            on_event(serde_json::json!({
                "event": "error",
                "error": {
                    "code": "CLI_CRASH",
                    "message": format!("dml exited with code {code} before finishing"),
                    "hint": "Check WSL: wsl -d dml-arch"
                }
            }));
        }
        Ok(code)
    }
```
And at module level:
```rust
fn is_terminal(v: &serde_json::Value) -> bool {
    matches!(v["event"].as_str(), Some("done") | Some("error"))
}
```

- [ ] **Step 4: Run tests to verify they pass**

```powershell
cd launcher\src-tauri; cargo test
```
Expected: 13 passed, zero warnings.

- [ ] **Step 5: Commit**

```powershell
git add launcher/src-tauri/src launcher/src-tauri/tests
git commit -m "feat(launcher): DmlRunner.run_stream NDJSON forwarding with CLI_CRASH synthesis"
```

---

### Task 5: Tauri commands + frontend API wrapper

**Files:**
- Modify: `launcher/src-tauri/src/lib.rs` (state, commands, invoke_handler; remove the scaffold's `greet` demo command)
- Create: `launcher/src/lib/api.ts`

**Interfaces:**
- Consumes: `DmlRunner` (Tasks 3–4), `ErrorInfo` (Task 2).
- Produces (Rust commands, all registered in `invoke_handler`):
  - `games_list() -> Result<serde_json::Value, CmdError>` (returns the envelope's `data`, i.e. `{"games":[...]}`)
  - `games_status(id: String) -> Result<serde_json::Value, CmdError>`
  - `dml_version() -> Result<serde_json::Value, CmdError>`
  - `games_start(id: String, on_event: Channel<serde_json::Value>) -> Result<(), CmdError>` and `games_stop(...)` — stream every NDJSON event over the channel; the terminal `done`/`error` arrives as an event (the Result only reports spawn-level failures)
  - `#[derive(Serialize)] pub struct CmdError { code, message, hint }` — what the frontend receives as the rejected value
  - `pub fn validate_game_id(id: &str) -> bool` (chars in `[A-Za-z0-9._-]`, non-empty) — invalid ids reject with code `BAD_ID` before any spawn
- Produces (TS, `launcher/src/lib/api.ts`): `Game`, `DmlErr`, `TermEvent` types; `gamesList(): Promise<Game[]>`; `gamesStatus(id)`; `gamesStart(id, onEvent)`; `gamesStop(id, onEvent)` — thin `invoke` wrappers using `Channel` from `@tauri-apps/api/core`.

- [ ] **Step 1: Write the failing test (id validation — the one pure unit here)**

Append to `launcher/src-tauri/src/lib.rs` (bottom):
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_id_validation() {
        assert!(validate_game_id("wow-server-playerbots"));
        assert!(validate_game_id("Mu_Online.v2"));
        assert!(!validate_game_id(""));
        assert!(!validate_game_id("wow; rm -rf /"));
        assert!(!validate_game_id("wow server"));
        assert!(!validate_game_id("../escape"));
    }
}
```
(`../escape` fails because `/` is not in the allowed set.)

- [ ] **Step 2: Run to verify failure**

```powershell
cd launcher\src-tauri; cargo test
```
Expected: compile FAIL — `validate_game_id` not defined.

- [ ] **Step 3: Implement the Rust side**

Replace the generated command section of `launcher/src-tauri/src/lib.rs` with the code below. Keep the existing `mod dml;`, and IMPORTANT: if the template's `run()` registers plugins (e.g. `.plugin(tauri_plugin_opener::init())`), keep those `.plugin(...)` lines in the builder chain below — only the demo `greet` command and its `generate_handler!` entry are removed:
```rust
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::State;

use crate::dml::envelope::Envelope;
use crate::dml::runner::{DmlRunner, RunnerError};

pub struct AppState {
    pub runner: std::sync::Arc<DmlRunner>,
}

#[derive(Debug, Serialize)]
pub struct CmdError {
    pub code: String,
    pub message: String,
    pub hint: String,
}

impl From<RunnerError> for CmdError {
    fn from(e: RunnerError) -> Self {
        match e {
            RunnerError::Spawn(m) => CmdError {
                code: "WSL_SPAWN".into(),
                message: m,
                hint: "Is WSL installed and the dml-arch distro present? Try: wsl -d dml-arch".into(),
            },
            RunnerError::BadOutput { raw } => CmdError {
                code: "CLI_BAD_OUTPUT".into(),
                message: raw,
                hint: "Is the dml CLI v3.0.0 installed? Run: powershell -File cli\\dev-install.ps1".into(),
            },
        }
    }
}

fn envelope_to_result(env: Envelope) -> Result<serde_json::Value, CmdError> {
    if env.ok {
        Ok(env.data)
    } else {
        let e = env.error.unwrap_or(crate::dml::envelope::ErrorInfo {
            code: "CLI_BAD_OUTPUT".into(),
            message: "ok=false with no error object".into(),
            hint: String::new(),
        });
        Err(CmdError { code: e.code, message: e.message, hint: e.hint })
    }
}

pub fn validate_game_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

fn bad_id(id: &str) -> CmdError {
    CmdError {
        code: "BAD_ID".into(),
        message: format!("invalid game id: {id:?}"),
        hint: "Game ids come from games_list".into(),
    }
}

#[tauri::command]
async fn dml_version(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    let runner = state.runner.clone();
    tauri::async_runtime::spawn_blocking(move || runner.run_json(&["version"]))
        .await
        .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
        .map_err(CmdError::from)
        .and_then(envelope_to_result)
}

#[tauri::command]
async fn games_list(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    let runner = state.runner.clone();
    tauri::async_runtime::spawn_blocking(move || runner.run_json(&["games", "list"]))
        .await
        .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
        .map_err(CmdError::from)
        .and_then(envelope_to_result)
}

#[tauri::command]
async fn games_status(id: String, state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    if !validate_game_id(&id) {
        return Err(bad_id(&id));
    }
    let runner = state.runner.clone();
    tauri::async_runtime::spawn_blocking(move || runner.run_json(&["games", "status", &id]))
        .await
        .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
        .map_err(CmdError::from)
        .and_then(envelope_to_result)
}

async fn stream_action(
    action: &'static str,
    id: String,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    if !validate_game_id(&id) {
        return Err(bad_id(&id));
    }
    let runner = state.runner.clone();
    tauri::async_runtime::spawn_blocking(move || {
        runner.run_stream(&["games", action, &id], |v| {
            let _ = on_event.send(v);
        })
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
    .map(|_exit| ())
    .map_err(CmdError::from)
}

#[tauri::command]
async fn games_start(
    id: String,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    stream_action("start", id, on_event, state).await
}

#[tauri::command]
async fn games_stop(
    id: String,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    stream_action("stop", id, on_event, state).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState { runner: std::sync::Arc::new(DmlRunner::default()) })
        .invoke_handler(tauri::generate_handler![
            dml_version,
            games_list,
            games_status,
            games_start,
            games_stop
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

Create `launcher/src/lib/api.ts`:
```typescript
import { invoke, Channel } from "@tauri-apps/api/core";

export interface DmlErr {
  code: string;
  message: string;
  hint: string;
}

export interface Game {
  id: string;
  path: string;
  running: boolean;
}

export type TermEvent =
  | { event: "section_start"; name: string }
  | { event: "line"; level: "info" | "warn" | "error"; text: string }
  | { event: "section_end"; name: string; status: "ok" | "error" }
  | { event: "done"; data: unknown }
  | { event: "error"; error: DmlErr }
  | { event: string; [key: string]: unknown }; // forward-compat: pct etc.

export async function gamesList(): Promise<Game[]> {
  const data = await invoke<{ games: Game[] }>("games_list");
  return data.games;
}

export async function gamesStatus(id: string): Promise<{ id: string; state: "running" | "stopped" }> {
  return await invoke("games_status", { id });
}

function streamAction(cmd: "games_start" | "games_stop") {
  return (id: string, onEvent: (e: TermEvent) => void): Promise<void> => {
    const ch = new Channel<TermEvent>();
    ch.onmessage = onEvent;
    return invoke(cmd, { id, onEvent: ch });
  };
}

export const gamesStart = streamAction("games_start");
export const gamesStop = streamAction("games_stop");
```

- [ ] **Step 4: Verify**

```powershell
cd launcher\src-tauri; cargo test
cd ..; npm run check
```
Expected: cargo 14 passed (13 + validation test), zero warnings; svelte-check 0 errors (api.ts compiles; the old `greet` import in `App.svelte`/`+page.svelte` may need deleting if the template referenced it — remove the demo form markup too).

- [ ] **Step 5: Commit**

```powershell
git add launcher/src-tauri/src launcher/src/lib/api.ts launcher/src
git commit -m "feat(launcher): tauri commands (list/status/version + streamed start/stop) and typed frontend API"
```

---

### Task 6: Terminal state reducer (pure TS) — TDD with vitest

**Files:**
- Modify: `launcher/package.json` (add vitest devDependency + `"test": "vitest run"` script)
- Create: `launcher/src/lib/terminal-state.ts`
- Create: `launcher/src/lib/terminal-state.test.ts`

**Interfaces:**
- Consumes: `TermEvent`, `DmlErr` types (Task 5).
- Produces (TS, consumed by Task 7's `Terminal.svelte`):
  - `interface TermLine { level: string; text: string }`
  - `interface Section { name: string; lines: TermLine[]; status: "running" | "ok" | "error"; collapsed: boolean }`
  - `interface TermState { sections: Section[]; startedAt: number | null; finished: null | { kind: "done"; data: unknown } | { kind: "error"; error: DmlErr }; totalLines: number }`
  - `initialTermState(): TermState`
  - `applyEvent(s: TermState, e: TermEvent, now?: number): TermState` — pure (never mutates `s`). Behavior: first event stamps `startedAt`; `section_start` appends a running section; `line` appends to the last running section (creating an implicit `"output"` section if none); `section_end` marks the matching running section `ok`/`error` and auto-collapses it when `ok` (The Lab's collapsible-finished-scripts behavior); `done`/`error` set `finished` (and `error` flips any still-running sections to `error`); unknown events are ignored.

- [ ] **Step 1: Add vitest and write the failing tests**

```powershell
cd launcher; npm install -D vitest
```
Add to `launcher/package.json` scripts: `"test": "vitest run"`.

Create `launcher/src/lib/terminal-state.test.ts`:
```typescript
import { describe, expect, it } from "vitest";
import { applyEvent, initialTermState } from "./terminal-state";

const T0 = 1_000_000;

describe("terminal state reducer", () => {
  it("stamps startedAt on the first event", () => {
    const s = applyEvent(initialTermState(), { event: "section_start", name: "start" }, T0);
    expect(s.startedAt).toBe(T0);
    expect(s.sections).toHaveLength(1);
    expect(s.sections[0]).toMatchObject({ name: "start", status: "running", collapsed: false });
  });

  it("appends lines to the running section and counts them", () => {
    let s = applyEvent(initialTermState(), { event: "section_start", name: "start" }, T0);
    s = applyEvent(s, { event: "line", level: "info", text: "one" });
    s = applyEvent(s, { event: "line", level: "warn", text: "two" });
    expect(s.sections[0].lines).toEqual([
      { level: "info", text: "one" },
      { level: "warn", text: "two" },
    ]);
    expect(s.totalLines).toBe(2);
  });

  it("creates an implicit output section for orphan lines", () => {
    const s = applyEvent(initialTermState(), { event: "line", level: "info", text: "hello" });
    expect(s.sections[0].name).toBe("output");
    expect(s.sections[0].lines[0].text).toBe("hello");
  });

  it("section_end ok collapses the section", () => {
    let s = applyEvent(initialTermState(), { event: "section_start", name: "start" });
    s = applyEvent(s, { event: "section_end", name: "start", status: "ok" });
    expect(s.sections[0]).toMatchObject({ status: "ok", collapsed: true });
  });

  it("done finishes the run", () => {
    let s = applyEvent(initialTermState(), { event: "section_start", name: "start" });
    s = applyEvent(s, { event: "done", data: { id: "wow", state: "running" } });
    expect(s.finished).toEqual({ kind: "done", data: { id: "wow", state: "running" } });
  });

  it("error finishes the run and fails running sections", () => {
    let s = applyEvent(initialTermState(), { event: "section_start", name: "start" });
    const err = { code: "START_FAILED", message: "boom", hint: "" };
    s = applyEvent(s, { event: "error", error: err });
    expect(s.finished).toEqual({ kind: "error", error: err });
    expect(s.sections[0].status).toBe("error");
  });

  it("ignores unknown events (pct is reserved)", () => {
    const s0 = applyEvent(initialTermState(), { event: "section_start", name: "x" }, T0);
    const s1 = applyEvent(s0, { event: "pct", value: 42 } as never);
    expect(s1.sections).toEqual(s0.sections);
    expect(s1.finished).toBeNull();
  });

  it("never mutates its input", () => {
    const s0 = applyEvent(initialTermState(), { event: "section_start", name: "x" }, T0);
    const frozen = JSON.stringify(s0);
    applyEvent(s0, { event: "line", level: "info", text: "y" });
    applyEvent(s0, { event: "section_end", name: "x", status: "ok" });
    expect(JSON.stringify(s0)).toBe(frozen);
  });
});
```

- [ ] **Step 2: Run to verify failure**

```powershell
cd launcher; npm test
```
Expected: FAIL — cannot resolve `./terminal-state`.

- [ ] **Step 3: Implement**

Create `launcher/src/lib/terminal-state.ts`:
```typescript
import type { DmlErr, TermEvent } from "./api";

export interface TermLine {
  level: string;
  text: string;
}

export interface Section {
  name: string;
  lines: TermLine[];
  status: "running" | "ok" | "error";
  collapsed: boolean;
}

export interface TermState {
  sections: Section[];
  startedAt: number | null;
  finished: null | { kind: "done"; data: unknown } | { kind: "error"; error: DmlErr };
  totalLines: number;
}

export function initialTermState(): TermState {
  return { sections: [], startedAt: null, finished: null, totalLines: 0 };
}

export function applyEvent(s: TermState, e: TermEvent, now: number = Date.now()): TermState {
  const st: TermState = {
    ...s,
    sections: s.sections.map((sec) => ({ ...sec, lines: sec.lines })),
    startedAt: s.startedAt ?? now,
  };

  switch (e.event) {
    case "section_start":
      st.sections = [
        ...st.sections,
        { name: String(e.name), lines: [], status: "running", collapsed: false },
      ];
      break;

    case "line": {
      let cur = st.sections[st.sections.length - 1];
      if (!cur || cur.status !== "running") {
        cur = { name: "output", lines: [], status: "running", collapsed: false };
        st.sections = [...st.sections, cur];
      }
      cur.lines = [...cur.lines, { level: String(e.level), text: String(e.text) }];
      st.totalLines += 1;
      break;
    }

    case "section_end":
      st.sections = st.sections.map((sec) =>
        sec.name === e.name && sec.status === "running"
          ? { ...sec, status: e.status === "ok" ? "ok" : "error", collapsed: e.status === "ok" }
          : sec,
      );
      break;

    case "done":
      st.finished = { kind: "done", data: (e as { data: unknown }).data };
      break;

    case "error": {
      st.finished = { kind: "error", error: (e as { error: DmlErr }).error };
      st.sections = st.sections.map((sec) =>
        sec.status === "running" ? { ...sec, status: "error" } : sec,
      );
      break;
    }

    default:
      // Unknown events (e.g. reserved "pct") are intentionally ignored.
      break;
  }
  return st;
}
```

- [ ] **Step 4: Run tests to verify they pass**

```powershell
cd launcher; npm test
```
Expected: 8 passed. Then `npm run check` → 0 errors.

- [ ] **Step 5: Commit**

```powershell
git add launcher/src/lib/terminal-state.ts launcher/src/lib/terminal-state.test.ts launcher/package.json launcher/package-lock.json
git commit -m "feat(launcher): pure terminal-state reducer for NDJSON streams with vitest coverage"
```

---

### Task 7: Library UI + embedded Terminal component + live smoke

**Files:**
- Create: `launcher/src/lib/Terminal.svelte`
- Modify: `launcher/src/App.svelte` (replace template demo with sidebar + library + terminal; if the template used `src/routes/+page.svelte`, apply the same content there instead and say so in the report)

**Interfaces:**
- Consumes: `api.ts` (Task 5), `terminal-state.ts` (Task 6).
- Produces: the v0 launcher window — dark sidebar ("Library" active; disabled placeholders for Dashboard/Items/Bots/Teleport/Modules per spec §5), game cards with running badge + Start/Stop buttons, and the embedded terminal pane with: live line streaming, per-section `<details>` collapse (finished-ok sections auto-collapsed), runtime `mm:ss` counter + spinner while running, and a "Jump to latest" button that appears when the user scrolls up (mirrors The Lab's install view).

- [ ] **Step 1: Terminal component**

Create `launcher/src/lib/Terminal.svelte`:
```svelte
<script lang="ts">
  import type { TermState } from "./terminal-state";

  let { state }: { state: TermState } = $props();

  let box: HTMLDivElement | undefined = $state();
  let autoScroll = $state(true);
  let elapsed = $state(0);

  const running = $derived(state.startedAt !== null && state.finished === null);

  $effect(() => {
    if (!running) return;
    const t = setInterval(() => {
      if (state.startedAt) elapsed = Math.floor((Date.now() - state.startedAt) / 1000);
    }, 1000);
    return () => clearInterval(t);
  });

  // autoscroll on new lines unless the user scrolled up
  $effect(() => {
    void state.totalLines;
    if (autoScroll && box) box.scrollTop = box.scrollHeight;
  });

  function onScroll() {
    if (!box) return;
    autoScroll = box.scrollTop + box.clientHeight >= box.scrollHeight - 8;
  }

  function jump() {
    autoScroll = true;
    if (box) box.scrollTop = box.scrollHeight;
  }

  const fmt = (s: number) =>
    `${String(Math.floor(s / 60)).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}`;
</script>

<div class="term">
  <div class="term-head">
    {#if running}
      <span class="spinner" aria-label="working"></span>
      <span class="runtime">{fmt(elapsed)}</span>
    {:else if state.finished?.kind === "done"}
      <span class="ok">✔ complete</span>
    {:else if state.finished?.kind === "error"}
      <span class="err">✖ {state.finished.error.code}</span>
    {/if}
  </div>

  <div class="term-body" bind:this={box} onscroll={onScroll}>
    {#each state.sections as sec (sec.name + sec.status)}
      <details open={!sec.collapsed}>
        <summary class={sec.status}>
          {sec.name}
          {#if sec.status === "running"}<span class="spinner small"></span>{/if}
        </summary>
        {#each sec.lines as l}
          <div class="line {l.level}">{l.text}</div>
        {/each}
      </details>
    {/each}
    {#if state.finished?.kind === "error"}
      <div class="line error">{state.finished.error.message}</div>
      {#if state.finished.error.hint}
        <div class="line hint">Hint: {state.finished.error.hint}</div>
      {/if}
    {/if}
  </div>

  {#if !autoScroll}
    <button class="jump" onclick={jump}>Jump to latest ↓</button>
  {/if}
</div>

<style>
  .term {
    position: relative;
    display: flex;
    flex-direction: column;
    background: #0d1117;
    border: 1px solid #30363d;
    border-radius: 8px;
    min-height: 220px;
    max-height: 45vh;
    overflow: hidden;
    font-family: ui-monospace, Consolas, monospace;
    font-size: 12px;
  }
  .term-head {
    display: flex;
    gap: 8px;
    align-items: center;
    justify-content: flex-end;
    padding: 6px 10px;
    border-bottom: 1px solid #30363d;
    color: #8b949e;
    min-height: 28px;
  }
  .term-body {
    overflow-y: auto;
    padding: 8px 10px;
    flex: 1;
  }
  .line { white-space: pre-wrap; color: #c9d1d9; }
  .line.warn { color: #d29922; }
  .line.error { color: #f85149; }
  .line.hint { color: #58a6ff; }
  summary { cursor: pointer; color: #8b949e; }
  summary.ok { color: #3fb950; }
  summary.error { color: #f85149; }
  .ok { color: #3fb950; }
  .err { color: #f85149; }
  .runtime { font-variant-numeric: tabular-nums; }
  .jump {
    position: absolute;
    bottom: 10px;
    right: 14px;
    background: #1f6feb;
    color: white;
    border: none;
    border-radius: 6px;
    padding: 4px 10px;
    cursor: pointer;
  }
  .spinner {
    width: 12px;
    height: 12px;
    border: 2px solid #30363d;
    border-top-color: #58a6ff;
    border-radius: 50%;
    animation: spin 0.9s linear infinite;
    display: inline-block;
  }
  .spinner.small { width: 9px; height: 9px; margin-left: 6px; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
```

- [ ] **Step 2: Library page**

Replace the template's demo content in `launcher/src/App.svelte` with:
```svelte
<script lang="ts">
  import { onMount } from "svelte";
  import { gamesList, gamesStart, gamesStop, type Game } from "./lib/api";
  import { applyEvent, initialTermState, type TermState } from "./lib/terminal-state";
  import Terminal from "./lib/Terminal.svelte";

  let games: Game[] = $state([]);
  let loadError: string | null = $state(null);
  let busyId: string | null = $state(null);
  let term: TermState = $state(initialTermState());
  let showTerm = $state(false);

  async function refresh() {
    try {
      games = await gamesList();
      loadError = null;
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      loadError = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    }
  }
  onMount(refresh);

  async function act(id: string, action: "start" | "stop") {
    busyId = id;
    showTerm = true;
    term = initialTermState();
    try {
      const run = action === "start" ? gamesStart : gamesStop;
      await run(id, (e) => {
        term = applyEvent(term, e);
      });
    } catch (e) {
      const err = e as { code?: string; message?: string; hint?: string };
      term = applyEvent(term, {
        event: "error",
        error: {
          code: err.code ?? "IPC",
          message: err.message ?? String(e),
          hint: err.hint ?? "",
        },
      });
    } finally {
      busyId = null;
      await refresh();
    }
  }
</script>

<main class="shell">
  <nav class="sidebar">
    <h1>DML<span>Launcher</span></h1>
    <a class="active" href="#library">Library</a>
    <a class="disabled" href="#dashboard" aria-disabled="true">Dashboard</a>
    <a class="disabled" href="#items" aria-disabled="true">Item Database</a>
    <a class="disabled" href="#bots" aria-disabled="true">Playerbots</a>
    <a class="disabled" href="#teleport" aria-disabled="true">Teleport</a>
    <a class="disabled" href="#modules" aria-disabled="true">Modules</a>
  </nav>

  <section class="content">
    <header class="bar">
      <h2>Game Library</h2>
      <button onclick={refresh}>Refresh</button>
    </header>

    {#if loadError}
      <div class="error-card">
        <strong>Couldn't reach the DML backend.</strong>
        <p>{loadError}</p>
      </div>
    {:else if games.length === 0}
      <p class="muted">No games installed yet. (Install flows arrive in a later release.)</p>
    {/if}

    <div class="cards">
      {#each games as g (g.id)}
        <div class="card">
          <div class="card-title">
            <span class="dot {g.running ? 'on' : 'off'}"></span>
            {g.id}
          </div>
          <div class="card-actions">
            {#if g.running}
              <button disabled={busyId !== null} onclick={() => act(g.id, "stop")}>Stop</button>
            {:else}
              <button class="primary" disabled={busyId !== null} onclick={() => act(g.id, "start")}>
                Start
              </button>
            {/if}
          </div>
        </div>
      {/each}
    </div>

    {#if showTerm}
      <Terminal state={term} />
    {/if}
  </section>
</main>

<style>
  :global(body) { margin: 0; background: #010409; color: #c9d1d9; font-family: "Segoe UI", system-ui, sans-serif; }
  .shell { display: grid; grid-template-columns: 200px 1fr; height: 100vh; }
  .sidebar { background: #0d1117; border-right: 1px solid #30363d; padding: 16px 0; display: flex; flex-direction: column; gap: 2px; }
  .sidebar h1 { font-size: 16px; margin: 0 16px 14px; color: #58a6ff; }
  .sidebar h1 span { color: #c9d1d9; font-weight: 300; margin-left: 4px; }
  .sidebar a { padding: 8px 16px; color: #8b949e; text-decoration: none; font-size: 14px; }
  .sidebar a.active { color: #f0f6fc; background: #161b22; border-left: 2px solid #58a6ff; }
  .sidebar a.disabled { opacity: 0.35; pointer-events: none; }
  .content { padding: 20px 24px; overflow-y: auto; display: flex; flex-direction: column; gap: 16px; }
  .bar { display: flex; justify-content: space-between; align-items: center; }
  .bar h2 { margin: 0; font-size: 18px; }
  .cards { display: flex; flex-wrap: wrap; gap: 12px; }
  .card { background: #0d1117; border: 1px solid #30363d; border-radius: 8px; padding: 14px 16px; min-width: 260px; display: flex; justify-content: space-between; align-items: center; gap: 16px; }
  .card-title { display: flex; align-items: center; gap: 8px; font-weight: 600; }
  .dot { width: 9px; height: 9px; border-radius: 50%; display: inline-block; }
  .dot.on { background: #3fb950; }
  .dot.off { background: #6e7681; }
  button { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 14px; cursor: pointer; }
  button.primary { background: #238636; border-color: #2ea043; color: white; }
  button:disabled { opacity: 0.5; cursor: default; }
  .muted { color: #8b949e; }
  .error-card { background: #161b22; border: 1px solid #f85149; border-radius: 8px; padding: 12px 16px; }
</style>
```
(If the scaffold routes through `src/routes/+page.svelte` instead of `App.svelte`, put this content there and note it in the report.)

- [ ] **Step 3: Static verification**

```powershell
cd launcher; npm run check; npm test
cd src-tauri; cargo test
```
Expected: 0 svelte-check errors, 8 vitest passed, 14 cargo passed.

- [ ] **Step 4: Live smoke against a no-docker fixture game**

Ensure the v3.0.0 CLI is installed, then create a harmless fixture game whose `dml-start.sh` hook streams lines WITHOUT touching Docker or the real WoW server:
```powershell
powershell -File cli\dev-install.ps1
wsl -d dml-arch -u dml -- bash -lc "mkdir -p ~/games/dml-smoke && touch ~/games/dml-smoke/docker-compose.yml && printf '#!/usr/bin/env bash\necho \"[dml] smoke line one\"\nsleep 1\necho \"[dml] smoke line two\"\nexit 0\n' > ~/games/dml-smoke/dml-start.sh && chmod +x ~/games/dml-smoke/dml-start.sh && echo FIXTURE_READY"
cd launcher; npm run tauri dev
```
Expected observations (record each in the report):
1. Window "DML Launcher" opens with the dark sidebar; Library lists `dml-smoke` (plus any real installs like `wow-server-playerbots`) with grey dots.
2. Click **Start** on `dml-smoke` → terminal pane appears; a `start` section streams `[dml] smoke line one` / `two`; spinner + counting `mm:ss` timer visible; on completion the section collapses, ✔ complete shows, and the library refreshes.
3. Scroll up in the terminal mid-stream (re-run Start if needed) → "Jump to latest ↓" appears; clicking it resumes autoscroll.
4. Stop the dev server, then remove the fixture:
```powershell
wsl -d dml-arch -u dml -- bash -lc "rm -rf ~/games/dml-smoke && echo FIXTURE_REMOVED"
```
**Do NOT start `wow-server-playerbots` during this smoke** — booting the real server is heavyweight and unnecessary here.

- [ ] **Step 5: Commit**

```powershell
git add launcher/src
git commit -m "feat(launcher): game library UI + embedded NDJSON terminal (sections, runtime, jump-to-latest)"
```

---

### Task 8: Release build + launcher README

**Files:**
- Create: `launcher/README.md`

**Interfaces:**
- Consumes: everything above.
- Produces: a distributable Windows build artifact and the developer README; closes Plan 2.

- [ ] **Step 1: Produce a release build**

```powershell
cd launcher; npm run tauri build
```
Expected: build succeeds; artifacts under `launcher\src-tauri\target\release\bundle\` (NSIS `*-setup.exe` and/or `.msi`) plus the bare exe at `launcher\src-tauri\target\release\dml-launcher.exe`. Note: unsigned — SmartScreen will warn on other machines (documented, expected; signing is a later plan). Launch the bare exe once and confirm the library loads.

- [ ] **Step 2: Write the README**

Create `launcher/README.md`:
```markdown
# DML Launcher

Tauri 2 desktop shell for Dad's MMO Lab. Windows-first; the core is
cross-platform (all Windows specifics live behind cfg(windows)).

The GUI is a thin shell: every feature calls the `dml` CLI inside the
`dml-arch` WSL distro as `wsl.exe -d dml-arch -u dml -- dml <cmd> --json`
and renders the JSON envelopes / NDJSON event streams documented in
`../cli/README.md`. No server logic lives in the GUI.

## Dev loop

    powershell -File ..\cli\dev-install.ps1   # install/refresh the dml CLI in WSL
    npm install
    npm run tauri dev        # run the app
    npm test                 # vitest (terminal-state reducer)
    npm run check            # svelte-check
    cd src-tauri; cargo test # runner + envelope + command tests

## Release build

    npm run tauri build      # NSIS installer under src-tauri/target/release/bundle/

Builds are currently unsigned (SmartScreen warning expected).

## Layout

    src/lib/api.ts             typed invoke wrappers (Channel-based streaming)
    src/lib/terminal-state.ts  pure NDJSON→terminal-state reducer (vitest)
    src/lib/Terminal.svelte    embedded terminal (sections, runtime, jump-to-latest)
    src/App.svelte             sidebar + game library
    src-tauri/src/dml/         envelope parsing + WSL process runner (cargo tests)
    src-tauri/src/lib.rs       tauri commands: games_list/status/start/stop, dml_version
```

- [ ] **Step 3: Final full check + commit**

```powershell
cd launcher; npm test; npm run check
cd src-tauri; cargo test
cd ..\..
git add launcher/README.md
git commit -m "feat(launcher): release build verified + developer README"
```
Expected: all suites green, then commit lands.
