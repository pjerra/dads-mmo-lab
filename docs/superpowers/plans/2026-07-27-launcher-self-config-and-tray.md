# Launcher Self-Configuration + System Tray Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the installed launcher configure itself (correct backend and paths with no env vars set by hand) and give it a system tray it can live in.

**Architecture:** One resolver runs ONCE at startup, before the Tauri builder, and exports its answers into the process environment — but only for variables that are not already set. Because `backend::selected()` and the three path readers already read the environment fresh on every call, and because native child processes inherit the launcher's environment, this leaves every existing consumer untouched and fixes the bash children at the same time. The persisted answers live in a Rust-readable `~/.dml/launcher.json`. Phase 2 then builds a tray on top: the frontend PUSHES status to Rust (Rust has no status poller of its own), the window hides instead of exiting, and a watchdog covers the keep-awake guarantee that process-exit used to provide.

**Tech Stack:** Rust 2021, Tauri 2.11.5, Svelte 5 (runes) + SvelteKit, serde/serde_json. One new Cargo feature (`tauri/tray-icon`). **No new crate dependencies.**

## Global Constraints

- Spec: `docs/superpowers/specs/2026-07-27-launcher-self-config-and-tray-design.md`. Read it before starting.
- Branch: work on `feat/rust-cli-workspace` (or a branch off it). NO merge to main — standing user policy.
- **Precedence is `DML_* env var` → `~/.dml/launcher.json` → auto-detect, and env MUST stay highest.** All 18 parity suites, the bats suite and the CLI integration tests inject these variables as override seams; if the file outranked env, tests would read a developer's persisted config.
- **No new crate dependencies.** Autostart is a direct `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` write from Rust, not `tauri-plugin-autostart`.
- The only build-config change permitted is `tauri = { version = "2", features = ["tray-icon"] }` in `launcher/src-tauri/Cargo.toml`. NOTE: `tray-icon 0.24.1` already appears in `Cargo.lock` as an *optional* dependency — that is NOT evidence the feature is on, and `TrayIconBuilder` will not compile without this change.
- `std::env::set_var` must run BEFORE any thread is spawned (the setup hook spawns the interval-backup watcher). Resolution therefore happens as the first statement of `run()`, before `tauri::Builder::default()`.
- After EVERY task: `cargo test --workspace` green from the repo root, `cd launcher; npm test` and `npm run check` green when frontend files changed, then commit.
- Baselines that must not regress: cargo 1063 passed / 0 failed / 2 ignored (Windows, server stack down), vitest 385, svelte-check 0 errors 0 warnings, bats 750.
- Windows shell: PowerShell 5.1 — no `&&`; use `git commit -F <file>` for multi-line messages.
- Commit messages: conventional commits, each ending with the repo's standard Claude trailers.
- Do NOT migrate existing `localStorage` preferences. The documented split is: frontend-only preferences stay in `localStorage`; anything Rust must know before a window exists goes in `launcher.json`.
- Changing the backend in Settings does NOT take effect live — `AppState`'s runner is built once at startup from `selected()`. The UI must say "applies after restart" and offer a relaunch, never imply a live switch.

## File Structure (end state)

```
crates/dml-core/src/
  launcher_config.rs     # NEW: LauncherConfig model, load/save, per-field tolerance, atomic write
  backend.rs             # MODIFY: + detect() and resolve_backend() pure decision fns
  lib.rs                 # MODIFY: pub mod launcher_config;

crates/dml-wow/src/
  bridge.rs              # MODIFY: deploy-nothing becomes a real error, not a success

launcher/src-tauri/
  Cargo.toml             # MODIFY: tauri features = ["tray-icon"]
  src/
    startup.rs           # NEW: resolve_and_export() — the one place env vars are written
    tray.rs              # NEW: tray build, menu handlers, verdict->tooltip mapping, show/hide
    autostart.rs         # NEW: HKCU Run read/write/remove
    lib.rs               # MODIFY: call startup::resolve_and_export() first in run(); build the
                         #   tray in setup(); .on_window_event for close-to-tray; new commands;
                         #   keep-awake watchdog; single-instance guard

launcher/src/lib/
  api.ts                 # MODIFY: wrappers for the new commands
  server-status.svelte.ts# MODIFY: push verdict to Rust on transition
  pages/Config.svelte    # MODIFY: the Launcher settings block (backend, paths, tray, autostart)
```

**Why these boundaries:** `startup.rs`, `tray.rs` and `autostart.rs` are new files rather than more `lib.rs` — `lib.rs` is already ~6000 lines, and each of these has one clear responsibility with a small surface (`resolve_and_export()`, `build_tray()`, `set_autostart()/autostart_enabled()`). The pure decision logic lives in `dml-core` so it is unit-testable with no filesystem and no Tauri, matching the seam `backend.rs` already has between `from_override` (pure) and `selected` (reads env).

---

# PHASE 1 — Self-configuration

### Task 1: `dml_core::launcher_config` — the settings file

**Files:**
- Create: `crates/dml-core/src/launcher_config.rs`
- Modify: `crates/dml-core/src/lib.rs` (add `pub mod launcher_config;`)

**Interfaces:**
- Consumes: nothing.
- Produces `dml_core::launcher_config::{LauncherConfig, config_path, load, save}`:
  - `LauncherConfig { backend: Option<String>, games_dir: Option<String>, dml_script: Option<String>, yq_bin: Option<String>, close_to_tray: bool, start_with_windows: bool }`, serialized camelCase
  - `config_path(dml_home: &Path) -> PathBuf`
  - `load(dml_home: &Path) -> LauncherConfig` — never fails
  - `save(dml_home: &Path, cfg: &LauncherConfig) -> std::io::Result<()>` — atomic

- [ ] **Step 1: Write the failing tests**

Create `crates/dml-core/src/launcher_config.rs` containing ONLY this test module for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Each test MUST pass a distinct name literal: cargo runs tests as threads
    // of ONE process, so the pid alone does not make these unique and two
    // tests sharing a name would remove_dir_all each other's directory mid-run.
    fn tmp_dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("dml-core-lcfg-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn missing_file_is_all_defaults() {
        let d = tmp_dir("missing");
        let cfg = load(&d);
        assert_eq!(cfg, LauncherConfig::default());
        assert_eq!(cfg.backend, None);
        assert!(cfg.close_to_tray, "close-to-tray defaults ON");
        assert!(!cfg.start_with_windows);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn corrupt_file_degrades_to_defaults() {
        let d = tmp_dir("corrupt");
        std::fs::write(config_path(&d), "{ this is not json").unwrap();
        assert_eq!(load(&d), LauncherConfig::default());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn partial_file_keeps_known_fields_and_defaults_the_rest() {
        let d = tmp_dir("partial");
        std::fs::write(config_path(&d), "{\"backend\":\"native\",\"unknownKey\":123}").unwrap();
        let cfg = load(&d);
        assert_eq!(cfg.backend.as_deref(), Some("native"));
        assert!(cfg.close_to_tray, "an absent field takes its default, not false");
        assert_eq!(cfg.games_dir, None);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn round_trips_through_save_with_camel_case_keys() {
        let d = tmp_dir("roundtrip");
        let cfg = LauncherConfig {
            backend: Some("wsl".into()),
            games_dir: Some("C:/games".into()),
            dml_script: None,
            yq_bin: None,
            close_to_tray: false,
            start_with_windows: true,
        };
        save(&d, &cfg).unwrap();
        let raw = std::fs::read_to_string(config_path(&d)).unwrap();
        assert!(raw.contains("gamesDir"), "on-disk keys are camelCase: {raw}");
        assert!(raw.contains("startWithWindows"), "on-disk keys are camelCase: {raw}");
        assert_eq!(load(&d), cfg);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn save_leaves_no_temp_file_behind() {
        let d = tmp_dir("atomic");
        save(&d, &LauncherConfig::default()).unwrap();
        let strays: Vec<String> = std::fs::read_dir(&d)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".tmp"))
            .collect();
        assert!(strays.is_empty(), "temp file survived the rename: {strays:?}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn save_creates_the_home_dir_when_absent() {
        let d = tmp_dir("mkdir");
        let nested = d.join("does-not-exist-yet");
        save(&nested, &LauncherConfig::default()).unwrap();
        assert!(config_path(&nested).is_file());
        let _ = std::fs::remove_dir_all(&d);
    }
}
```

Add to `crates/dml-core/src/lib.rs`, in the existing `pub mod` list (alphabetical, between `error` and `proc`):

```rust
pub mod launcher_config;
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p dml-core launcher_config`
Expected: FAIL to COMPILE — `cannot find type LauncherConfig in this scope`, `cannot find function load`, `cannot find function config_path`, `cannot find function save`.

- [ ] **Step 3: Write the implementation**

Insert ABOVE the `#[cfg(test)]` block in `crates/dml-core/src/launcher_config.rs`:

```rust
//! Launcher-owned settings, persisted at `~/.dml/launcher.json`.
//!
//! This file exists because the launcher must know its backend BEFORE any
//! window exists: the tray shows server status while minimised, and every
//! Rust command needs the mode. Frontend `localStorage` — where every other
//! launcher preference lives — cannot answer that question at startup.
//!
//! Tolerance is deliberate and matches this directory's neighbours
//! (`soap.env`, `client-path`): a missing file is the normal first-run state,
//! and a corrupt one degrades to defaults rather than bricking startup.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

fn default_close_to_tray() -> bool {
    true
}

/// The persisted launcher settings. Every field is optional on disk; absent
/// means "work it out". `backend` is `auto` | `native` | `wsl` and records
/// INTENT, not a frozen answer — so a machine that gains Docker later
/// re-resolves correctly instead of going stale.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherConfig {
    #[serde(default)]
    pub backend: Option<String>,
    #[serde(default)]
    pub games_dir: Option<String>,
    #[serde(default)]
    pub dml_script: Option<String>,
    #[serde(default)]
    pub yq_bin: Option<String>,
    #[serde(default = "default_close_to_tray")]
    pub close_to_tray: bool,
    #[serde(default)]
    pub start_with_windows: bool,
}

impl Default for LauncherConfig {
    fn default() -> Self {
        Self {
            backend: None,
            games_dir: None,
            dml_script: None,
            yq_bin: None,
            close_to_tray: true,
            start_with_windows: false,
        }
    }
}

/// `<dml_home>/launcher.json`.
pub fn config_path(dml_home: &Path) -> PathBuf {
    dml_home.join("launcher.json")
}

/// Read the settings. NEVER fails: an unreadable or unparseable file yields
/// defaults, because a broken config must not stop the app from starting.
pub fn load(dml_home: &Path) -> LauncherConfig {
    let Ok(raw) = std::fs::read_to_string(config_path(dml_home)) else {
        return LauncherConfig::default();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

/// Write the settings via temp-file + rename, so a crash mid-write cannot
/// leave a truncated config behind.
pub fn save(dml_home: &Path, cfg: &LauncherConfig) -> std::io::Result<()> {
    std::fs::create_dir_all(dml_home)?;
    let path = config_path(dml_home);
    let tmp = path.with_extension("json.tmp");
    let body = serde_json::to_string_pretty(cfg).map_err(std::io::Error::other)?;
    std::fs::write(&tmp, body)?;
    std::fs::rename(&tmp, &path)
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p dml-core launcher_config`
Expected: PASS, 6 tests.

- [ ] **Step 5: Run the whole workspace**

Run: `cargo test --workspace`
Expected: 1069 passed / 0 failed / 2 ignored (the 1063 baseline plus these 6).

**Running total for later tasks:** 1063 baseline → T1 +6 = 1069 → T2 +6 = 1075
→ T3 +2 = 1077 → T6 +1 (two tests replace one) = 1078 → T9 +2 = **1080**.
Tasks 4, 5, 7, 8, 10–13 add no cargo tests.

- [ ] **Step 6: Commit**

```bash
git add crates/dml-core/src/launcher_config.rs crates/dml-core/src/lib.rs
git commit -F <message-file>
```

Message subject: `feat(dml-core): launcher.json settings model with tolerant load and atomic save`

---

### Task 2: `dml_core::backend` — auto-detect and the precedence resolver

**Files:**
- Modify: `crates/dml-core/src/backend.rs` — add two pure functions and their tests. Do NOT change `from_override` or `selected`; every existing caller depends on them unchanged.

**Interfaces:**
- Consumes: `Backend`, `from_override` (already in this file).
- Produces:
  - `detect(native_dir_exists: bool, docker_present: bool) -> Backend`
  - `resolve(env_value: Option<&str>, file_value: Option<&str>, native_dir_exists: bool, docker_present: bool) -> Backend`

- [ ] **Step 1: Write the failing tests**

Append INSIDE the existing `#[cfg(test)] mod tests` block in `crates/dml-core/src/backend.rs`:

```rust
    #[test]
    fn detect_prefers_native_only_when_both_signals_present() {
        assert_eq!(detect(true, true), Backend::Native);
        assert_eq!(detect(true, false), Backend::Wsl);
        assert_eq!(detect(false, true), Backend::Wsl);
        assert_eq!(detect(false, false), Backend::Wsl);
    }

    #[test]
    fn resolve_env_outranks_everything() {
        // Load-bearing: the 18 parity suites, the bats suite and the CLI
        // integration tests all inject these vars as override seams. If the
        // file outranked env, those tests would start reading a developer's
        // persisted launcher.json.
        assert_eq!(resolve(Some("wsl"), Some("native"), true, true), Backend::Wsl);
        assert_eq!(resolve(Some("native"), Some("wsl"), false, false), Backend::Native);
    }

    #[test]
    fn resolve_ignores_empty_env_and_falls_through_to_file() {
        assert_eq!(resolve(Some(""), Some("native"), false, false), Backend::Native);
        assert_eq!(resolve(None, Some("native"), false, false), Backend::Native);
    }

    #[test]
    fn resolve_auto_in_file_means_detect_not_wsl() {
        // "auto" is NOT a value from_override understands -- passing it
        // straight through would silently mean Wsl and defeat detection.
        assert_eq!(resolve(None, Some("auto"), true, true), Backend::Native);
        assert_eq!(resolve(None, Some("  AUTO "), true, true), Backend::Native);
        assert_eq!(resolve(None, Some("auto"), false, false), Backend::Wsl);
    }

    #[test]
    fn resolve_absent_file_value_means_detect() {
        assert_eq!(resolve(None, None, true, true), Backend::Native);
        assert_eq!(resolve(None, Some(""), true, true), Backend::Native);
    }

    #[test]
    fn resolve_typo_in_file_is_wsl_not_detect() {
        // Same doctrine as from_override: a typo must never silently strand
        // the user on an unfinished backend, so it resolves to Wsl rather
        // than being treated as "auto".
        assert_eq!(resolve(None, Some("natve"), true, true), Backend::Wsl);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p dml-core backend`
Expected: FAIL to COMPILE — `cannot find function detect in this scope`, `cannot find function resolve in this scope`.

- [ ] **Step 3: Write the implementation**

Add to `crates/dml-core/src/backend.rs`, immediately after `selected()` and before the `#[cfg(test)]` block:

```rust
/// Which backend a machine looks like it wants, from two probe results.
///
/// Native wins only when BOTH signals are present. It is the faster path and
/// the one the tester docs recommend, but guessing Native without Docker
/// would strand the user on a backend that cannot start anything.
pub fn detect(native_dir_exists: bool, docker_present: bool) -> Backend {
    if native_dir_exists && docker_present {
        Backend::Native
    } else {
        Backend::Wsl
    }
}

/// Full precedence: `DML_BACKEND` env → `launcher.json` → auto-detect.
///
/// `file_value` is the persisted setting; `"auto"` (its default) means "fall
/// through to detection", which is why it cannot simply be handed to
/// [`from_override`] — that maps every unrecognized string to `Wsl`.
pub fn resolve(
    env_value: Option<&str>,
    file_value: Option<&str>,
    native_dir_exists: bool,
    docker_present: bool,
) -> Backend {
    if let Some(v) = env_value.map(str::trim).filter(|v| !v.is_empty()) {
        return from_override(Some(v));
    }
    match file_value.map(str::trim).filter(|v| !v.is_empty()) {
        Some(v) if !v.eq_ignore_ascii_case("auto") => from_override(Some(v)),
        _ => detect(native_dir_exists, docker_present),
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p dml-core backend`
Expected: PASS — the 6 new tests plus the file's existing `from_override` tests.
Then `cargo test --workspace` → 1075 passed / 0 failed / 2 ignored.

- [ ] **Step 5: Commit**

```bash
git add crates/dml-core/src/backend.rs
git commit -F <message-file>
```

Message subject: `feat(dml-core): backend auto-detection and env-over-file-over-detect resolver`

---

### Task 3: Startup wiring — resolve once, export into the process environment

This is the task that makes an installed app work. It deliberately changes NO
existing consumer: `backend::selected()` and the three path readers already
read the environment fresh on every call, and native child processes inherit
the launcher's environment, so setting the variables at startup fixes Rust and
bash together.

**Files:**
- Create: `launcher/src-tauri/src/startup.rs`
- Modify: `launcher/src-tauri/src/lib.rs` (declare the module; call it as the FIRST statement of `run()`)

**Interfaces:**
- Consumes: `dml_core::launcher_config::{load, LauncherConfig}`, `dml_core::backend::{resolve, Backend}`, `dml_core::util::dml_home_dir`, `dml_core::engine::docker_desktop_program`.
- Produces:
  - `startup::value_to_export(env_value: Option<&str>, resolved: Option<&str>) -> Option<String>` (pure)
  - `startup::default_games_dir() -> Option<PathBuf>`
  - `startup::resolve_and_export()` — the only place in the app that writes these env vars.

- [ ] **Step 1: Write the failing tests**

Create `launcher/src-tauri/src/startup.rs` with ONLY this test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_only_when_env_is_absent_or_empty() {
        // Env wins: a set value is never overwritten.
        assert_eq!(value_to_export(Some("C:/set-by-user"), Some("C:/resolved")), None);
        // Unset or empty: the resolved value fills in.
        assert_eq!(value_to_export(None, Some("C:/resolved")), Some("C:/resolved".to_string()));
        assert_eq!(value_to_export(Some(""), Some("C:/resolved")), Some("C:/resolved".to_string()));
        assert_eq!(value_to_export(Some("   "), Some("C:/resolved")), Some("C:/resolved".to_string()));
    }

    #[test]
    fn export_nothing_when_there_is_nothing_to_resolve() {
        // No env AND no resolved value: leave it unset so downstream failures
        // stay honest ("not found") instead of pointing at an invented path.
        assert_eq!(value_to_export(None, None), None);
        assert_eq!(value_to_export(Some(""), None), None);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p launcher startup`
Expected: FAIL to COMPILE — `cannot find function value_to_export in this scope`.

- [ ] **Step 3: Write the implementation**

Insert ABOVE the `#[cfg(test)]` block in `launcher/src-tauri/src/startup.rs`:

```rust
//! One-shot startup resolution of the four `DML_*` variables.
//!
//! WHY THE ENVIRONMENT. `backend::selected()` and the three path readers
//! (`games_dir_from_env`, `ConfigReader::title_dir_from_env`,
//! `find_dml_script`) read the process environment fresh on EVERY call, and
//! native children inherit it (`DmlRunner` only prepends PATH). Writing the
//! resolved values here therefore fixes ~60 native command gates and the
//! bash children at once, without threading a resolver through any of them.
//!
//! WHY ONLY-IF-UNSET. Precedence is `env → launcher.json → auto-detect`, and
//! env must stay highest: the parity, bats and CLI integration suites all
//! inject these variables as override seams.
//!
//! ORDERING. `std::env::set_var` is only sound before other threads exist, so
//! `resolve_and_export()` MUST be the first statement of `run()` — before
//! `tauri::Builder::default()`, whose `.setup()` spawns the interval-backup
//! watcher thread.

use std::path::PathBuf;

/// Pure: what to write for one variable, or `None` to leave it alone.
pub fn value_to_export(env_value: Option<&str>, resolved: Option<&str>) -> Option<String> {
    if env_value.map(str::trim).is_some_and(|v| !v.is_empty()) {
        return None; // the user set it; never overwrite
    }
    resolved.map(str::to_string)
}

/// The conventional native install location, used when neither the
/// environment nor `launcher.json` names one.
pub fn default_games_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(|u| PathBuf::from(u).join("dml-native"))
}

/// Resolve the backend and the three paths, then export whatever the user has
/// not already set. Call FIRST in `run()`.
pub fn resolve_and_export() {
    let home = match dml_core::util::dml_home_dir() {
        Some(h) => h,
        None => return, // no USERPROFILE/HOME: nothing to read, nothing to write
    };
    let cfg = dml_core::launcher_config::load(&home);

    // --- games dir -------------------------------------------------------
    let games_dir: Option<PathBuf> = cfg
        .games_dir
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .or_else(default_games_dir);

    // --- probes for auto-detection ---------------------------------------
    let native_dir_exists = games_dir
        .as_ref()
        .map(|g| g.join("wow-server-playerbots").is_dir())
        .unwrap_or(false);
    // `docker_desktop_program` has NO bare-name fallback, so `Some` means a
    // real Docker Desktop executable was found on disk.
    let docker_present = dml_core::engine::docker_desktop_program().is_some();

    let backend = dml_core::backend::resolve(
        std::env::var("DML_BACKEND").ok().as_deref(),
        cfg.backend.as_deref(),
        native_dir_exists,
        docker_present,
    );
    let backend_str = match backend {
        dml_core::backend::Backend::Native => "native",
        dml_core::backend::Backend::Wsl => "wsl",
    };

    // --- yq: default to the path the one-click installer downloads to -----
    let yq: Option<String> = cfg
        .yq_bin
        .clone()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            games_dir
                .as_ref()
                .map(|g| g.join("tools").join("yq.exe").to_string_lossy().into_owned())
        });

    // --- script: NO invented default. Absent means absent. ---------------
    let script: Option<String> = cfg.dml_script.clone().filter(|s| !s.trim().is_empty());

    let exports: Vec<(&str, Option<String>)> = vec![
        ("DML_BACKEND", value_to_export(std::env::var("DML_BACKEND").ok().as_deref(), Some(backend_str))),
        (
            "DML_GAMES_DIR",
            value_to_export(
                std::env::var("DML_GAMES_DIR").ok().as_deref(),
                games_dir.as_ref().map(|g| g.to_string_lossy().into_owned()).as_deref(),
            ),
        ),
        ("DML_SCRIPT", value_to_export(std::env::var("DML_SCRIPT").ok().as_deref(), script.as_deref())),
        ("DML_YQ_BIN", value_to_export(std::env::var("DML_YQ_BIN").ok().as_deref(), yq.as_deref())),
    ];

    for (name, value) in exports {
        if let Some(v) = value {
            std::env::set_var(name, v);
        }
    }
}
```

In `launcher/src-tauri/src/lib.rs`, add the module declaration next to the existing `mod power;` / `mod watch;` lines:

```rust
mod startup;
```

and make it the first statement of `run()` — the function carrying
`#[cfg_attr(mobile, tauri::mobile_entry_point)]`, immediately before
`tauri::Builder::default()`:

```rust
    // MUST run before any thread exists (Builder::setup spawns one) and
    // before AppState captures backend::selected().
    startup::resolve_and_export();
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p launcher startup`
Expected: PASS, 2 tests.

- [ ] **Step 5: Verify the whole workspace still passes**

Run: `cargo test --workspace`
Expected: 1077 passed / 0 failed / 2 ignored.

- [ ] **Step 6: Manual smoke — the actual bug this fixes**

With the native server RUNNING and NO `DML_*` variables set in the shell:

```powershell
Remove-Item Env:DML_BACKEND, Env:DML_GAMES_DIR, Env:DML_SCRIPT, Env:DML_YQ_BIN -ErrorAction SilentlyContinue
cargo run -p launcher
```

Expected: the status card shows the server ONLINE (before this task it showed offline).
NOTE: the user's machine has these four set as PERSISTENT user variables, so a
fresh terminal re-inherits them — `Remove-Item Env:` clears them for this
process only, which is exactly the unconfigured-install condition to test.

- [ ] **Step 7: Commit**

```bash
git add launcher/src-tauri/src/startup.rs launcher/src-tauri/src/lib.rs
git commit -F <message-file>
```

Message subject: `feat(launcher): resolve backend and paths at startup, export only what is unset`

---

### Task 4: Tauri commands for reading and writing the settings

**Files:**
- Modify: `launcher/src-tauri/src/lib.rs` (two commands + two `generate_handler!` entries)
- Modify: `launcher/src/lib/api.ts` (two wrappers + one interface)

**Interfaces:**
- Consumes: `dml_core::launcher_config::{load, save, LauncherConfig}`, `dml_core::util::dml_home_dir`.
- Produces:
  - Rust: `launcher_config_read() -> Result<serde_json::Value, CmdError>`, `launcher_config_write(cfg: LauncherConfig) -> Result<(), CmdError>`
  - TS: `launcherConfigRead(): Promise<LauncherSettings>`, `launcherConfigWrite(cfg: LauncherConfig): Promise<void>`

- [ ] **Step 1: Write the Rust commands**

Add near the other small commands in `launcher/src-tauri/src/lib.rs` (e.g. beside `backend_mode`):

```rust
/// The launcher's own settings, plus which source currently WINS for the
/// backend. The UI needs `backendSource` to explain why its dropdown is
/// read-only when an env var overrides the file.
#[tauri::command]
fn launcher_config_read() -> Result<serde_json::Value, CmdError> {
    let home = dml_core::util::dml_home_dir().ok_or_else(|| CmdError {
        code: "NO_HOME".into(),
        message: "Could not resolve the home directory".into(),
        hint: "Set USERPROFILE or HOME.".into(),
    })?;
    let cfg = dml_core::launcher_config::load(&home);
    let env_backend = std::env::var("DML_BACKEND").ok().filter(|v| !v.trim().is_empty());
    let source = if env_backend.is_some() {
        "env"
    } else if cfg.backend.as_deref().is_some_and(|v| !v.eq_ignore_ascii_case("auto")) {
        "file"
    } else {
        "auto"
    };
    Ok(serde_json::json!({
        "config": cfg,
        "backendSource": source,
        "effectiveBackend": match dml_wow::backend::selected() {
            dml_wow::backend::Backend::Native => "native",
            dml_wow::backend::Backend::Wsl => "wsl",
        },
        "envBackend": env_backend,
    }))
}

/// Persist the settings. Backend changes apply on the NEXT launch — AppState's
/// runner is built once at startup — so the UI must say so rather than imply
/// a live switch.
#[tauri::command]
fn launcher_config_write(cfg: dml_core::launcher_config::LauncherConfig) -> Result<(), CmdError> {
    let home = dml_core::util::dml_home_dir().ok_or_else(|| CmdError {
        code: "NO_HOME".into(),
        message: "Could not resolve the home directory".into(),
        hint: "Set USERPROFILE or HOME.".into(),
    })?;
    dml_core::launcher_config::save(&home, &cfg).map_err(|e| CmdError {
        code: "WRITE_FAILED".into(),
        message: format!("Could not write launcher.json: {e}"),
        hint: String::new(),
    })
}
```

Register both by appending to the `tauri::generate_handler![...]` list. The
list's last entry is `realmlist_lock` and has NO trailing comma — add one to it,
then append:

```rust
            realmlist_lock,
            launcher_config_read,
            launcher_config_write
```

- [ ] **Step 2: Write the TypeScript wrappers**

Add to `launcher/src/lib/api.ts`:

```ts
export interface LauncherConfig {
  backend: string | null;
  gamesDir: string | null;
  dmlScript: string | null;
  yqBin: string | null;
  closeToTray: boolean;
  startWithWindows: boolean;
}
export interface LauncherSettings {
  config: LauncherConfig;
  backendSource: "env" | "file" | "auto";
  effectiveBackend: BackendMode;
  envBackend: string | null;
}

export async function launcherConfigRead(): Promise<LauncherSettings> {
  return await invoke<LauncherSettings>("launcher_config_read");
}

// The Rust parameter is `cfg`, so the invoke key is `cfg`. Getting this wrong
// fails SILENTLY for optional params -- always camelCase the exact Rust
// parameter name.
export async function launcherConfigWrite(cfg: LauncherConfig): Promise<void> {
  return await invoke("launcher_config_write", { cfg });
}
```

- [ ] **Step 3: Verify it compiles and type-checks**

Run: `cargo test --workspace`
Expected: PASS (no new tests; must still be 1077 / 0 / 2).
Run: `cd launcher; npm run check`
Expected: 0 errors, 0 warnings.

- [ ] **Step 4: Manual smoke**

Start the app with `cargo run -p launcher`, open the devtools console and run:

```js
await window.__TAURI__.core.invoke("launcher_config_read")
```

Expected: an object with `config`, `backendSource` (`"env"` on the user's
machine, since `DML_BACKEND` is set persistently), `effectiveBackend`, `envBackend`.

- [ ] **Step 5: Commit**

```bash
git add launcher/src-tauri/src/lib.rs launcher/src/lib/api.ts
git commit -F <message-file>
```

Message subject: `feat(launcher): launcher_config read/write commands`

---

### Task 5: Settings UI — the Launcher card

**Files:**
- Modify: `launcher/src/lib/pages/Config.svelte`

**Interfaces:**
- Consumes: `launcherConfigRead`, `launcherConfigWrite`, `LauncherSettings` (Task 4).
- Produces: no exports — UI only.

The AC-registry rows are data-driven from `ConfigSetting[]` and must NOT be
hand-written. These are purely-local launcher settings, so they follow the
`testing-card` precedent: a hand-written card inside `{#if tab === "settings"}`,
placed BEFORE the `{#each visibleGroups as g (g)}` loop.

- [ ] **Step 1: Add the state and loader**

In the `<script>` block of `Config.svelte`, beside the other `$state`
declarations:

```ts
let launcher: LauncherSettings | null = $state(null);
let launcherSaving = $state(false);
let launcherNote: string | null = $state(null);

async function loadLauncherSettings(): Promise<void> {
  try {
    launcher = await launcherConfigRead();
  } catch {
    launcher = null; // a missing/broken config must not break the page
  }
}

$effect(() => {
  if (tab === "settings" && launcher === null) void loadLauncherSettings();
});

async function saveLauncherBackend(choice: string): Promise<void> {
  if (!launcher) return;
  launcherSaving = true;
  try {
    const next = { ...launcher.config, backend: choice };
    await launcherConfigWrite(next);
    launcher = { ...launcher, config: next };
    launcherNote = "Saved. Restart the launcher to switch backend.";
  } catch (e) {
    const err = e as { message?: string };
    error = err.message ?? "Could not save launcher settings";
  } finally {
    launcherSaving = false;
  }
}
```

Add `launcherConfigRead`, `launcherConfigWrite` and the `LauncherSettings` type
to the existing `api` import at the top of the file.

- [ ] **Step 2: Add the markup**

Immediately AFTER the closing `</div>` of the `testing-card` block and BEFORE
`{#each visibleGroups as g (g)}`:

```svelte
    {#if launcher}
      <div class="card">
        <h3>Launcher</h3>
        <label class="row">
          Server backend
          <select
            value={launcher.config.backend ?? "auto"}
            disabled={launcherSaving || launcher.backendSource === "env"}
            onchange={(e) => saveLauncherBackend(e.currentTarget.value)}
          >
            <option value="auto">Detect automatically</option>
            <option value="native">Docker Desktop (native)</option>
            <option value="wsl">WSL (dml-arch distro)</option>
          </select>
        </label>
        <p class="muted">
          Currently using <strong>{launcher.effectiveBackend}</strong>.
          {#if launcher.backendSource === "env"}
            Locked by the DML_BACKEND environment variable
            (<code>{launcher.envBackend}</code>), which overrides this setting.
            Clear it to choose here.
          {:else if launcher.backendSource === "auto"}
            Detected automatically. Native is chosen when a title folder and
            Docker Desktop are both present.
          {/if}
        </p>
        {#if launcherNote}<p class="muted">{launcherNote}</p>{/if}
      </div>
    {/if}
```

- [ ] **Step 3: Type-check and test**

Run: `cd launcher; npm run check`
Expected: 0 errors, 0 warnings.
Run: `cd launcher; npm test`
Expected: 385 passed (this task adds no vitest coverage — the card is markup
plus two IPC calls; its logic lives in the Rust functions already tested).

- [ ] **Step 4: Manual smoke**

`cargo run -p launcher` → Config ▸ Settings. Expected on the user's machine:
the dropdown is DISABLED with the "Locked by the DML_BACKEND environment
variable (native)" note, because they set it persistently. Then:

```powershell
Remove-Item Env:DML_BACKEND -ErrorAction SilentlyContinue
cargo run -p launcher
```

Expected: the dropdown is now enabled and reads "Detect automatically";
changing it writes `~/.dml/launcher.json` and shows the restart note.

- [ ] **Step 5: Commit**

```bash
git add launcher/src/lib/pages/Config.svelte
git commit -F <message-file>
```

Message subject: `feat(launcher): backend selector in Settings, read-only under an env override`

---

### Task 6: Stop the Eluna bridge reporting success when it deploys nothing

**Files:**
- Modify: `crates/dml-wow/src/bridge.rs`

**Interfaces:**
- Changes the behaviour of `deploy_scripts(root, dest)`: "no families found" becomes an error instead of `Ok(false)`.

This is a real bug, not polish: with `DML_SCRIPT` unset, "Enable My Party"
emits `done{changed:false}` — reporting SUCCESS while deploying zero lua files —
so My Party then silently does not work with nothing pointing at the cause.

**Note there are TWO no-families paths**, and both must be covered: the
`read_dir` escape for a missing/unreadable root, AND the silent fall-through
when the root exists but contains no family directories.

- [ ] **Step 1: Update the test that pins the old behaviour, and add the second case**

In `crates/dml-wow/src/bridge.rs`, REPLACE the existing test
`deploy_scripts_missing_root_creates_dest_and_reports_unchanged` with:

```rust
    #[test]
    fn deploy_scripts_missing_root_is_an_error_not_a_silent_success() {
        let root = tmp_dir("deploy_missing_root_never_created");
        std::fs::remove_dir_all(&root).unwrap(); // now genuinely absent
        let dest = tmp_dir("deploy_missing_dest");

        let err = deploy_scripts(&root, &dest).unwrap_err();
        assert!(err.contains("no bridge scripts"), "err={err}");
        // dest is still created -- only the reporting changed.
        assert!(dest.is_dir());

        let _ = std::fs::remove_dir_all(&dest);
    }

    #[test]
    fn deploy_scripts_empty_root_is_also_an_error() {
        // The second path: the root EXISTS but holds no family dirs, so the
        // copy loop never runs. Previously this also returned Ok(false).
        let root = tmp_dir("deploy_empty_root");
        let dest = tmp_dir("deploy_empty_dest");

        let err = deploy_scripts(&root, &dest).unwrap_err();
        assert!(err.contains("no bridge scripts"), "err={err}");

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&dest);
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p dml-wow bridge`
Expected: FAIL — `called Result::unwrap_err() on an Ok value: false` for both.

- [ ] **Step 3: Change `deploy_scripts`**

In `deploy_scripts`, replace the `read_dir` early-return so a missing root is an
error, and add a families-found check before returning. Update the doc comment,
which currently documents the old degradation as deliberate:

```rust
    // A root with no families means nothing was deployed. Reporting Ok here
    // let bridge-setup emit done{changed:false} -- a SUCCESS envelope for a
    // no-op -- and My Party then failed with nothing pointing at the cause.
    let Ok(read) = std::fs::read_dir(root) else {
        return Err(format!(
            "no bridge scripts found at {} -- is DML_SCRIPT set? (the lua dir must be its sibling)",
            root.display()
        ));
    };
```

and after the family loop, before the final `Ok(changed)`:

```rust
    if families == 0 {
        return Err(format!(
            "no bridge scripts found at {} -- is DML_SCRIPT set? (the lua dir must be its sibling)",
            root.display()
        ));
    }
```

adding a `let mut families = 0usize;` before the loop and `families += 1;` inside it.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p dml-wow bridge`
Expected: PASS. The surviving tests that deploy real fixture families must
still pass unchanged — if one now fails, it was relying on the empty-root
degradation and its fixture needs a family directory.

- [ ] **Step 5: Confirm the stream reports it**

No change is needed in `bridge_setup_stream`: it already maps
`Err(e)` to `section_end("error")` + `bs_event_error("WRITE_FAILED", ...)`.
Read that arm and confirm the new message flows through it.

- [ ] **Step 6: Run the workspace suite and commit**

Run: `cargo test --workspace`
Expected: 1072 passed / 0 failed / 2 ignored (1071 + one net new test).

```bash
git add crates/dml-wow/src/bridge.rs
git commit -F <message-file>
```

Message subject: `fix(dml-wow): bridge deploy with no scripts is an error, not a silent success`

---

# PHASE 2 — System tray

### Task 7: Enable the tray feature and build a minimal tray

**Files:**
- Modify: `launcher/src-tauri/Cargo.toml`
- Create: `launcher/src-tauri/src/tray.rs`
- Modify: `launcher/src-tauri/src/lib.rs` (declare `mod tray;`, call `tray::build(app)?` in `.setup()`)

**Interfaces:**
- Produces: `tray::build(app: &tauri::AppHandle) -> tauri::Result<()>`, `tray::show_main_window(app: &tauri::AppHandle)`

- [ ] **Step 1: Enable the Cargo feature**

In `launcher/src-tauri/Cargo.toml` change:

```toml
tauri = { version = "2", features = [] }
```
to
```toml
tauri = { version = "2", features = ["tray-icon"] }
```

`tray-icon 0.24.1` is ALREADY in `Cargo.lock` as an OPTIONAL dependency — that
is not evidence the feature is on, and `TrayIconBuilder` will not compile
without this change. No `capabilities/default.json` change is needed:
`core:default` already includes `core:tray:default` and `core:menu:default`,
and a tray built in Rust is not subject to the capability system at all.

- [ ] **Step 2: Write the tray module**

Create `launcher/src-tauri/src/tray.rs`:

```rust
//! System tray. Built in `.setup()`; owns show/hide of the main window.
//!
//! The icon needs no extra Cargo feature: `icons/icon.ico` is embedded at
//! COMPILE time by tauri-codegen (first `bundle.icon` entry ending in `.ico`
//! on Windows), so `default_window_icon()` hands us a decoded image. Loading
//! one at runtime instead would need the non-default `image-ico` feature.

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::Manager;

/// The window label is "main" in tauri.conf.json (implicit default) and in
/// capabilities/default.json. Reuse it rather than guessing.
pub const MAIN_WINDOW: &str = "main";

/// Show, unminimize and focus the main window.
pub fn show_main_window(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window(MAIN_WINDOW) {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

pub fn build(app: &tauri::AppHandle) -> tauri::Result<()> {
    let open_i = MenuItem::with_id(app, "tray_open", "Open DML Launcher", true, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, "tray_quit", "Exit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open_i, &quit_i])?;

    TrayIconBuilder::with_id("dml-tray")
        .icon(app.default_window_icon().expect("bundle.icon provides icon.ico").clone())
        .tooltip("DML Launcher")
        .menu(&menu)
        // Left click opens the window; the menu is the right-click surface.
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "tray_open" => show_main_window(app),
            // MUST go through app.exit() so the existing RunEvent::Exit arm
            // still fires and clears the keep-awake execution state. A window
            // destroy would bypass it and leave the PC pinned awake.
            "tray_quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}
```

- [ ] **Step 3: Wire it into `.setup()`**

In `launcher/src-tauri/src/lib.rs` add `mod tray;` beside the other module
declarations. The setup hook currently binds its argument as `_app` and returns
`Ok(())`; rename it to `app` and add the tray build before the `Ok(())`:

```rust
        .setup(|app| {
            // ... existing native-mode interval-backup watcher block ...
            tray::build(app.handle())?;
            Ok(())
        })
```

- [ ] **Step 4: Build and smoke**

Run: `cargo test --workspace`
Expected: still 1078 / 0 / 2 (no new tests yet).
Run: `cargo run -p launcher`
Expected: a DML Launcher icon appears in the notification area. Left-click
focuses the window. Right-click shows Open / Exit. Exit quits the app.

- [ ] **Step 5: Commit**

```bash
git add launcher/src-tauri/Cargo.toml launcher/src-tauri/src/tray.rs launcher/src-tauri/src/lib.rs Cargo.lock
git commit -F <message-file>
```

Message subject: `feat(launcher): system tray icon with Open and Exit`

---

### Task 8: Close hides to tray

**Files:**
- Modify: `launcher/src-tauri/src/lib.rs` (add `.on_window_event`)

**Interfaces:**
- Consumes: `dml_core::launcher_config::load`, `tray::MAIN_WINDOW`.

- [ ] **Step 1: Add the handler**

There is currently NO `.on_window_event` in the builder chain. Add it on
`tauri::Builder::default()` BEFORE `.build(...)` — anywhere among
`.manage`/`.setup`/`.plugin`/`.invoke_handler`:

```rust
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() != tray::MAIN_WINDOW {
                    return;
                }
                // Read the preference fresh: the user can change it in
                // Settings without restarting, and this runs rarely.
                let hide = dml_core::util::dml_home_dir()
                    .map(|h| dml_core::launcher_config::load(&h).close_to_tray)
                    .unwrap_or(true);
                if hide {
                    api.prevent_close();
                    // HIDE, never destroy: the webview must keep running.
                    // It owns the 7s status poll that feeds the tray, and the
                    // auto-shutdown toggle is re-asserted to Rust from its
                    // onMount -- destroying it would silently kill both.
                    let _ = window.hide();
                }
            }
        })
```

- [ ] **Step 2: Smoke it**

Run: `cargo run -p launcher`
Expected: clicking X hides the window and leaves the tray icon; left-clicking
the tray icon brings it back with its state intact (navigate to a page before
hiding and confirm you return to it, proving the webview was not destroyed);
tray ▸ Exit quits.

- [ ] **Step 3: Verify the keep-awake guarantee still holds on Exit**

With the server ONLINE (so keep-awake is engaged), quit via tray ▸ Exit, then:

```powershell
powercfg /requests
```

Expected: no SYSTEM request remains from launcher.exe.

- [ ] **Step 4: Commit**

```bash
git add launcher/src-tauri/src/lib.rs
git commit -F <message-file>
```

Message subject: `feat(launcher): close hides to tray instead of exiting`

---

### Task 9: Push server status into the tray

**Files:**
- Create tests + code in: `launcher/src-tauri/src/tray.rs`
- Modify: `launcher/src-tauri/src/lib.rs` (one command + handler entry)
- Modify: `launcher/src/lib/api.ts`, `launcher/src/lib/server-status.svelte.ts`

**Interfaces:**
- Produces: `tray::tooltip_for(verdict: &str) -> String` (pure), Rust command `tray_set_status(verdict: String)`, TS `traySetStatus(verdict: string)`.

Rust has NO status poller — polling is entirely frontend-driven (a 7s
`setInterval` in `server-status.svelte.ts`). The frontend therefore PUSHES.
This also inherits the webview-only `restarting` suppression, so the tray does
not flap while containers cycle.

- [ ] **Step 1: Write the failing test for the pure mapping**

Append to `launcher/src-tauri/src/tray.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tooltip_names_the_app_and_the_state() {
        assert_eq!(tooltip_for("online"), "DML Launcher — server online");
        assert_eq!(tooltip_for("stopped"), "DML Launcher — server stopped");
        assert_eq!(tooltip_for("starting"), "DML Launcher — server starting");
        assert_eq!(tooltip_for("crashed"), "DML Launcher — server crashed");
        assert_eq!(tooltip_for("soap_unreachable"), "DML Launcher — server unreachable");
    }

    #[test]
    fn tooltip_falls_back_for_an_unknown_verdict() {
        // The verdict union may grow; an unknown value must not panic or
        // produce an empty tooltip.
        assert_eq!(tooltip_for("nonsense"), "DML Launcher");
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p launcher tray`
Expected: FAIL to COMPILE — `cannot find function tooltip_for`.

- [ ] **Step 3: Implement**

Add to `launcher/src-tauri/src/tray.rs`:

```rust
/// Pure: tray tooltip for a `ServerDetail.verdict`.
pub fn tooltip_for(verdict: &str) -> String {
    let tail = match verdict {
        "online" => "server online",
        "stopped" => "server stopped",
        "starting" => "server starting",
        "crashed" => "server crashed",
        "soap_unreachable" => "server unreachable",
        _ => return "DML Launcher".to_string(),
    };
    format!("DML Launcher — {tail}")
}

/// Apply a pushed verdict to the tray icon.
pub fn apply_status(app: &tauri::AppHandle, verdict: &str) {
    if let Some(tray) = app.tray_by_id("dml-tray") {
        let _ = tray.set_tooltip(Some(tooltip_for(verdict)));
    }
}
```

Add the command in `launcher/src-tauri/src/lib.rs` (sync and infallible, same
doctrine as `set_keep_awake` — there is nothing useful to report on failure):

```rust
/// The frontend pushes the polled verdict here so the tray can show it while
/// the window is hidden. Rust has no status poller of its own.
#[tauri::command]
fn tray_set_status(app: tauri::AppHandle, verdict: String) {
    tray::apply_status(&app, &verdict);
}
```

Register `tray_set_status` in `tauri::generate_handler![...]`.

- [ ] **Step 4: Push from the frontend**

Add to `launcher/src/lib/api.ts`:

```ts
export async function traySetStatus(verdict: string): Promise<void> {
  return await invoke("tray_set_status", { verdict });
}
```

In `launcher/src/lib/server-status.svelte.ts`, add to the END of
`runTransitionActions`, after the `azerothReadyTransition` line:

```ts
  // Tray tooltip. runTransitionActions runs on EVERY successful poll, not
  // only on changes, so guard explicitly or this fires every 7 seconds.
  if (prev !== next) void traySetStatus(next).catch(() => {});
```

Import `traySetStatus` from `./api` alongside the existing imports.

- [ ] **Step 5: Run everything**

Run: `cargo test -p launcher tray` → PASS (2 tests).
Run: `cargo test --workspace` → 1080 / 0 / 2.
Run: `cd launcher; npm test` → 385 passed. `npm run check` → 0/0.

- [ ] **Step 6: Manual smoke**

Start the app with the server stopped, hover the tray icon (expect "server
stopped"), start the server from Home, and watch the tooltip become "server
online" without opening any window.

- [ ] **Step 7: Commit**

```bash
git add launcher/src-tauri/src/tray.rs launcher/src-tauri/src/lib.rs launcher/src/lib/api.ts launcher/src/lib/server-status.svelte.ts
git commit -F <message-file>
```

Message subject: `feat(launcher): tray tooltip reflects server status, pushed from the poll loop`

---

### Task 10: Keep-awake watchdog

**Files:**
- Modify: `launcher/src-tauri/src/lib.rs` (AppState field, watchdog thread, timestamp update)

Hide-to-tray removes the guarantee that process exit clears the Windows
execution state. The keep-awake block is engaged by the WEBVIEW poll loop; if
WebView2 throttles a hidden window's timers, the poll stops, the release never
fires, and the machine stays awake indefinitely.

**Interfaces:**
- Produces: `AppState.last_status_push: Arc<Mutex<Option<std::time::Instant>>>`

- [ ] **Step 1: Add the state field**

Add to the `AppState` struct and to its inline `.manage(AppState { ... })`
literal (there is no `AppState::new()`):

```rust
    /// When the frontend last pushed a status. The keep-awake watchdog uses
    /// this to detect a stalled webview poll.
    pub last_status_push: Arc<Mutex<Option<std::time::Instant>>>,
```
```rust
            last_status_push: Arc::new(Mutex::new(None)),
```

- [ ] **Step 2: Stamp it on every push**

Change `tray_set_status` to take state and record the time:

```rust
#[tauri::command]
fn tray_set_status(app: tauri::AppHandle, verdict: String, state: State<'_, AppState>) {
    if let Ok(mut t) = state.last_status_push.lock() {
        *t = Some(std::time::Instant::now());
    }
    tray::apply_status(&app, &verdict);
}
```

- [ ] **Step 3: Spawn the watchdog in `.setup()`**

Add inside the setup hook, after `tray::build(app.handle())?;`:

```rust
            // Keep-awake safety net. Engagement is driven by the webview poll
            // loop; a hidden window whose timers get throttled would otherwise
            // hold the sleep block forever. Two minutes is ~17 missed 7s polls
            // -- long enough never to fight a briefly-busy poll, short enough
            // that a stalled webview cannot keep the PC awake unnoticed.
            let pushes = app.state::<AppState>().last_status_push.clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_secs(30));
                let stale = pushes
                    .lock()
                    .ok()
                    .and_then(|t| *t)
                    .map(|t| t.elapsed() > std::time::Duration::from_secs(120))
                    .unwrap_or(false);
                if stale {
                    power::keep_awake(false);
                }
            });
```

- [ ] **Step 4: Verify**

Run: `cargo test --workspace` → 1080 / 0 / 2.

Manual: start the server (keep-awake engages — confirm with `powercfg /requests`),
hide to tray, and leave it for three minutes. Expected: if the webview poll
stalls, the SYSTEM request disappears; if the poll keeps running (the normal
case), the request stays. Either outcome is correct — the point is that a
stalled poll can no longer pin the machine awake.

- [ ] **Step 5: Commit**

```bash
git add launcher/src-tauri/src/lib.rs
git commit -F <message-file>
```

Message subject: `feat(launcher): release keep-awake when the status poll stalls`

---

### Task 11: Start / Stop from the tray menu

**Files:**
- Modify: `launcher/src-tauri/src/tray.rs`, `launcher/src/lib/api.ts`, `launcher/src-tauri/src/lib.rs`

Rather than duplicating lifecycle orchestration in the tray, the menu items
open the window and ask the frontend to run the SAME flow the Home card uses,
including its confirmation. A destructive-ish action must not be one unguarded
click away with no window open.

**Interfaces:**
- Produces: a `tray://action` event payload `{ action: "start" | "stop" }` emitted to the frontend.

- [ ] **Step 1: Add the menu items and emit**

In `tray::build`, extend the menu:

```rust
    let start_i = MenuItem::with_id(app, "tray_start", "Start server", true, None::<&str>)?;
    let stop_i = MenuItem::with_id(app, "tray_stop", "Stop server", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open_i, &start_i, &stop_i, &quit_i])?;
```

and in `on_menu_event`, before the `_ => {}` arm:

```rust
            "tray_start" | "tray_stop" => {
                // Surface the window first: the action runs through the same
                // confirmed flow the Home card uses, which needs a UI.
                show_main_window(app);
                let action = if event.id.as_ref() == "tray_start" { "start" } else { "stop" };
                let _ = app.emit("tray-action", action);
            }
```

Add `use tauri::Emitter;` to the imports in `tray.rs`.

- [ ] **Step 2: Read Home's existing confirmation before wiring anything**

This step is research, not code. Open `launcher/src/lib/pages/Home.svelte` and
find how its Start and Stop buttons arm their confirmation (the codebase uses
an armed-boolean pattern — e.g. `confirmingRestart` in `Config.svelte`, and
`{confirmSetup ? "Deploy the bot bridge scripts?" : "Enable My Party"}` in
`Playerbots.svelte`). Note the exact state variable names. The tray must set
that SAME state, not call the lifecycle API — routing round the confirmation
is the one thing this task must not do.

- [ ] **Step 3: Handle the event in the shell**

In `launcher/src/routes/+page.svelte`, inside the existing `onMount` beside
`startStatusPolling()`, add a listener that navigates Home and stores the
request in the module-level store the Home page reads:

```ts
  void listen<string>("tray-action", (e) => {
    trayAction.pending = e.payload as "start" | "stop";
    // Navigate to Home using this file's existing page-switching mechanism
    // (the same call its sidebar buttons make) -- do not invent a new one.
  });
```

Import `listen` from `@tauri-apps/api/event`.

Create `launcher/src/lib/tray-action.svelte.ts`, following the module-level
runes-store pattern of `restart-state.svelte.ts` so the value survives
navigation without prop-drilling:

```ts
// A start/stop request arriving from the tray menu. Home consumes it by
// ARMING its own confirmation -- never by running the action directly.
export const trayAction = $state({ pending: null as "start" | "stop" | null });
```

In `Home.svelte`, add an `$effect` that consumes it by setting the confirmation
state you identified in Step 2, then clears `trayAction.pending` so the same
request cannot re-arm on the next navigation.

- [ ] **Step 3: Verify**

Run: `cd launcher; npm run check` → 0/0. `npm test` → 385 passed.
Run: `cargo test --workspace` → 1080 / 0 / 2.

Manual: hide to tray, right-click ▸ Start server. Expected: the window appears
on Home with the start action armed and awaiting confirmation — NOT a server
already starting.

- [ ] **Step 4: Commit**

```bash
git add launcher/src-tauri/src/tray.rs launcher/src/routes/+page.svelte
git commit -F <message-file>
```

Message subject: `feat(launcher): tray Start/Stop route through the Home confirmation`

---

### Task 12: Single-instance guard

**Files:**
- Create: `launcher/src-tauri/src/single_instance.rs`
- Modify: `launcher/src-tauri/src/lib.rs`

Once the app survives window close, launching the exe again (taskbar, Start
menu, autostart) would start a SECOND app fighting over the same server.
`tauri-plugin-single-instance` is a new crate and the plan forbids new
dependencies, so this uses a loopback TCP bind — dependency-free, and the
same socket doubles as the "focus the existing window" channel.

**Interfaces:**
- Produces: `single_instance::acquire() -> Option<std::net::TcpListener>` and `single_instance::serve(listener, app)`.

- [ ] **Step 1: Write the module**

Create `launcher/src-tauri/src/single_instance.rs`:

```rust
//! Dependency-free single-instance guard.
//!
//! Binding a fixed loopback port is atomic: exactly one process can hold it.
//! A second launch fails to bind, connects instead (which wakes the primary
//! into focusing its window), and exits. No new crate, and no stale-lock-file
//! problem — the port is released by the OS when the process dies.

use std::net::{TcpListener, TcpStream};

/// Arbitrary high port, loopback only. Changing it strands running instances.
const PORT: u16 = 51789;

/// `Some(listener)` if we are the first instance; `None` if another is live
/// (after poking it so it surfaces its window).
pub fn acquire() -> Option<TcpListener> {
    match TcpListener::bind(("127.0.0.1", PORT)) {
        Ok(l) => Some(l),
        Err(_) => {
            let _ = TcpStream::connect(("127.0.0.1", PORT));
            None
        }
    }
}

/// Focus the window whenever another launch pokes us.
pub fn serve(listener: TcpListener, app: tauri::AppHandle) {
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            drop(stream);
            crate::tray::show_main_window(&app);
        }
    });
}
```

- [ ] **Step 2: Wire it in**

In `run()`, immediately AFTER `startup::resolve_and_export();`:

```rust
    let instance_lock = match single_instance::acquire() {
        Some(l) => l,
        None => return, // another instance is live and has been focused
    };
```

and in `.setup()`, after the tray build:

```rust
            single_instance::serve(instance_lock, app.handle().clone());
```

(move `instance_lock` into the setup closure). Add `mod single_instance;`.

- [ ] **Step 3: Verify**

Run: `cargo test --workspace` → 1080 / 0 / 2.

Manual: start the app, hide it to tray, then run `.\target\debug\launcher.exe`
again. Expected: no second window or second tray icon; the hidden window is
restored and focused.

- [ ] **Step 4: Commit**

```bash
git add launcher/src-tauri/src/single_instance.rs launcher/src-tauri/src/lib.rs
git commit -F <message-file>
```

Message subject: `feat(launcher): single-instance guard focuses the running app`

---

### Task 13: Start with Windows

**Files:**
- Create: `launcher/src-tauri/src/autostart.rs`
- Modify: `launcher/src-tauri/src/lib.rs`, `launcher/src/lib/api.ts`, `launcher/src/lib/pages/Config.svelte`

No new crate: this shells `reg.exe`, matching the repo's existing registry
access (a read-only `reg query` for WSL detection, run with `CREATE_NO_WINDOW`).

**Interfaces:**
- Produces: `autostart::{enabled() -> bool, set(on: bool) -> Result<(), String>}`; commands `autostart_get`/`autostart_set`; TS `autostartGet`/`autostartSet`.

- [ ] **Step 1: Write the module**

Create `launcher/src-tauri/src/autostart.rs`:

```rust
//! Start-with-Windows via HKCU\...\Run, using reg.exe.
//!
//! No new crate: the repo already shells `reg query` (wslconfig.rs) rather
//! than depending on winreg, and this keeps the toggle on the same Rust path
//! as every other launcher.json setting. Chosen over
//! tauri-plugin-autostart, whose JS API would also need a permission in
//! capabilities/default.json — a failure with no compile-time signal.

const KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE: &str = "DML Launcher";

fn reg(args: &[&str]) -> std::io::Result<std::process::Output> {
    let mut cmd = std::process::Command::new("reg");
    cmd.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.output()
}

/// True when a Run entry exists AND still points at a file that exists — a
/// stale entry from a deleted build counts as disabled, not enabled.
pub fn enabled() -> bool {
    let Ok(out) = reg(&["query", KEY, "/v", VALUE]) else {
        return false;
    };
    if !out.status.success() {
        return false;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let Some(line) = text.lines().find(|l| l.contains(VALUE)) else {
        return false;
    };
    // `    DML Launcher    REG_SZ    C:\path\launcher.exe`
    let Some(path) = line.split("REG_SZ").nth(1).map(str::trim) else {
        return false;
    };
    std::path::Path::new(path.trim_matches('"')).is_file()
}

pub fn set(on: bool) -> Result<(), String> {
    if !on {
        let _ = reg(&["delete", KEY, "/v", VALUE, "/f"]);
        return Ok(());
    }
    let exe = std::env::current_exe().map_err(|e| format!("could not resolve the exe path: {e}"))?;
    let exe = exe.to_string_lossy().into_owned();
    let out = reg(&["add", KEY, "/v", VALUE, "/t", "REG_SZ", "/d", &exe, "/f"])
        .map_err(|e| format!("could not run reg.exe: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}
```

- [ ] **Step 2: Commands and wrappers**

In `lib.rs` (add `mod autostart;` and register both):

```rust
#[tauri::command]
fn autostart_get() -> bool {
    autostart::enabled()
}

#[tauri::command]
fn autostart_set(on: bool) -> Result<(), CmdError> {
    autostart::set(on).map_err(|e| CmdError {
        code: "AUTOSTART_FAILED".into(),
        message: e,
        hint: String::new(),
    })
}
```

In `api.ts`:

```ts
export async function autostartGet(): Promise<boolean> {
  return await invoke<boolean>("autostart_get");
}
export async function autostartSet(on: boolean): Promise<void> {
  return await invoke("autostart_set", { on });
}
```

- [ ] **Step 3: Add the toggles to the Launcher card**

In the `{#if launcher}` card from Task 5, after the backend `<label class="row">`:

```svelte
        <label class="row">
          <input
            type="checkbox"
            checked={launcher.config.closeToTray}
            disabled={launcherSaving}
            onchange={(e) => saveLauncherFlag("closeToTray", e.currentTarget.checked)}
          />
          Closing the window keeps DML Launcher running in the system tray
        </label>
        <label class="row">
          <input
            type="checkbox"
            checked={autostartOn}
            disabled={launcherSaving}
            onchange={(e) => setAutostart(e.currentTarget.checked)}
          />
          Start DML Launcher when Windows starts
        </label>
```

with, in the script block:

```ts
let autostartOn = $state(false);

async function saveLauncherFlag(key: "closeToTray", on: boolean): Promise<void> {
  if (!launcher) return;
  launcherSaving = true;
  try {
    const next = { ...launcher.config, [key]: on };
    await launcherConfigWrite(next);
    launcher = { ...launcher, config: next };
  } finally {
    launcherSaving = false;
  }
}

async function setAutostart(on: boolean): Promise<void> {
  launcherSaving = true;
  try {
    await autostartSet(on);
    autostartOn = await autostartGet();
  } catch (e) {
    const err = e as { message?: string };
    error = err.message ?? "Could not change the Windows startup setting";
    autostartOn = await autostartGet();
  } finally {
    launcherSaving = false;
  }
}
```

and extend the existing `loadLauncherSettings()` to also set
`autostartOn = await autostartGet();`.

- [ ] **Step 4: Verify**

Run: `cargo test --workspace` → 1080 / 0 / 2. `npm run check` → 0/0. `npm test` → 385.

Manual: toggle it on, then confirm the entry exists and points at the running exe:

```powershell
reg query "HKCU\Software\Microsoft\Windows\CurrentVersion\Run" /v "DML Launcher"
```

Toggle it off and confirm the value is gone.

- [ ] **Step 5: Commit**

```bash
git add launcher/src-tauri/src/autostart.rs launcher/src-tauri/src/lib.rs launcher/src/lib/api.ts launcher/src/lib/pages/Config.svelte
git commit -F <message-file>
```

Message subject: `feat(launcher): start-with-Windows toggle via HKCU Run`

---

### Task 14: Notify when the server goes down unexpectedly

**Files:**
- Modify: `launcher/src/lib/server-status.svelte.ts`, `launcher/src/lib/server-status.test.ts`

Today exactly one state-change notification exists: `azerothReadyTransition`
fires the "AZEROTH IS READY!" toast on `starting|stopped|crashed → online`.
The requested "notify when the server changes state" needs the other
direction — the server going down while you are not looking is precisely what
a tray user wants to be told about.

**Interfaces:**
- Produces: `serverWentDownTransition(prev, next): boolean` (pure, exported, vitest-pinned).

- [ ] **Step 1: Write the failing test**

Add to `launcher/src/lib/server-status.test.ts`:

```ts
describe("serverWentDownTransition", () => {
  it("fires when a running server stops or crashes", () => {
    expect(serverWentDownTransition("online", "stopped")).toBe(true);
    expect(serverWentDownTransition("online", "crashed")).toBe(true);
  });

  it("does not fire on the first poll", () => {
    // prev === null means app launch -- a server that was already down is
    // not news, and notifying on startup would be noise every launch.
    expect(serverWentDownTransition(null, "stopped")).toBe(false);
    expect(serverWentDownTransition(null, "crashed")).toBe(false);
  });

  it("does not fire for a transient SOAP hiccup or a restart", () => {
    // soap_unreachable is frequently transient; starting is a deliberate
    // user action. Neither is an unexpected shutdown.
    expect(serverWentDownTransition("online", "soap_unreachable")).toBe(false);
    expect(serverWentDownTransition("online", "starting")).toBe(false);
    expect(serverWentDownTransition("soap_unreachable", "stopped")).toBe(false);
  });

  it("does not fire when nothing changed", () => {
    expect(serverWentDownTransition("stopped", "stopped")).toBe(false);
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd launcher; npm test`
Expected: FAIL — `serverWentDownTransition is not a function` / not exported.

- [ ] **Step 3: Implement**

In `launcher/src/lib/server-status.svelte.ts`, beside `azerothReadyTransition`:

```ts
/// True when a server we KNEW was up has gone down. Deliberately narrow:
/// only from `online`, only to a settled down-state, and never on the first
/// poll (prev === null), so this cannot fire as launch noise.
export function serverWentDownTransition(
  prev: ServerDetail["verdict"] | null,
  next: ServerDetail["verdict"] | null,
): boolean {
  if (prev !== "online") return false;
  return next === "stopped" || next === "crashed";
}
```

and fire it in `runTransitionActions`, beside the existing ready notification:

```ts
  if (serverWentDownTransition(prev, next)) fireServerDownNotification(next);
```

with an impure executor modelled on the existing `fireReadyNotification`
(same permission check, same plugin import):

```ts
function fireServerDownNotification(next: ServerDetail["verdict"]): void {
  const body =
    next === "crashed" ? "The world server crashed." : "The server has stopped.";
  void (async () => {
    try {
      if (!(await isPermissionGranted())) return;
      sendNotification({ title: "DML Launcher", body });
    } catch {
      // Notifications are a courtesy; never let one break the poll loop.
    }
  })();
}
```

Read `fireReadyNotification` first and mirror its exact permission-request
handling rather than assuming — it already solved this.

- [ ] **Step 4: Run to verify it passes**

Run: `cd launcher; npm test`
Expected: 389 passed (385 + 4). `npm run check` → 0 errors, 0 warnings.

- [ ] **Step 5: Manual smoke**

Hide the app to tray with the server online, then stop the server from a
terminal (`dml-wow stop --no-stop-engine`). Expected: a Windows notification
saying the server has stopped, with no window open.

- [ ] **Step 6: Commit**

```bash
git add launcher/src/lib/server-status.svelte.ts launcher/src/lib/server-status.test.ts
git commit -F <message-file>
```

Message subject: `feat(launcher): notify when a running server stops or crashes`

---

### Task 15: Final verification gate

- [ ] `cargo test --workspace` — record the total; expect 1080 passed / 0 failed / 2 ignored (1063 baseline + 17 new).
- [ ] `cd launcher; npm test` — expect 389 passed (385 baseline + Task 14's 4) — and `npm run check` (0 errors, 0 warnings).
- [ ] `bats` unchanged: `wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bats tests/"` → 750 passed. (Phase 1 touches no bash, but `deploy_scripts` has a bash oracle.)
- [ ] **The bug this plan exists to fix**, tested as an unconfigured install: clear all four vars from the process (`Remove-Item Env:DML_BACKEND, Env:DML_GAMES_DIR, Env:DML_SCRIPT, Env:DML_YQ_BIN -ErrorAction SilentlyContinue`), start the server, launch the app, and confirm the status card shows ONLINE.
- [ ] Live parity gate still green with the server up: `cargo test -p dml-wow --tests -- --nocapture` → all 18 suites run with ZERO skips.
- [ ] `npm run tauri build`; launch `target/release/launcher.exe` with no env vars set and repeat the tray checks: hide to tray, reopen, tray Start/Stop, Exit clears keep-awake (`powercfg /requests`), second launch focuses rather than duplicating.
- [ ] Bridge check: with `DML_SCRIPT` unset, "Enable My Party" now reports a real error naming the cause instead of a silent success.
- [ ] Update `.superpowers/sdd/` ledger and the repo `CLAUDE.md` (crates/ section gains `launcher_config`; launcher section gains the tray and the settings file).
- [ ] Commit stragglers. DO NOT merge. Report remaining user gates.

---

## Notes for the executor

- **The `localStorage` / `launcher.json` split is deliberate.** Frontend-only
  preferences stay in `localStorage`; anything Rust must know before a window
  exists goes in `launcher.json`. Do not migrate the existing preferences.
- **Backend changes are not live.** `AppState`'s runner is built once from
  `selected()`. The UI says "restart to apply" — do not attempt a live switch.
- **Bundling `cli/` is explicitly out of scope** (the user's staged decision).
  `DML_SCRIPT` therefore stays a real setting; the honest failure path is the
  point, not a workaround.

