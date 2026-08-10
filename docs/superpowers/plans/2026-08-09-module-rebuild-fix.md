# Module Rebuild Fix Implementation Plan

> **STATUS 2026-08-10: EXECUTED, reviewed and live-verified** (commits
> `3b41fae..ba16d53` + the final fix wave; the VM rebuilt with mod-city-bots
> in game). The step checkboxes below were never ticked during execution —
> read `git log` and the SDD ledger for state, not the boxes.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `wow module rebuild` actually compile on native installs, refuse honestly on image-only servers, add a standing Rebuild button, and let Unbound recover from shallow mod-ale clones.

**Architecture:** A new shared `buildcap` module in `dml-wow` holds the build-capability primitives (the `-f` file set and the compose-config build check) that `unbound.rs` already proved out; `modmgr`'s rebuild/install/update streams consume them; bash mirrors the guard with an awk-scoped compose-config scan; the launcher reads a new `can_build` list field (fail-open) and gains an always-visible rebuild card.

**Tech Stack:** Rust (dml-wow), bash (cli/src), Svelte 5 + vitest (launcher), bats (cli/tests).

**Spec:** `docs/superpowers/specs/2026-08-09-module-rebuild-fix-design.md`

## Global Constraints

- Branch: `feat/core-family`. NO merge to `main` (standing policy).
- Error code (both surfaces, exact): `MODULE_NO_BUILD_CONFIG`.
- Refusal message (rebuild, both surfaces, byte-identical): `This server has no build configuration for ac-worldserver -- it runs from prebuilt images, so C++ modules cannot be compiled into it.` Hint: `A rebuild needs a server built from source (a native install or a WSL install). A migrated image-only server cannot rebuild yet.`
- Refusal message (cpp install/update, both surfaces, byte-identical): `This server runs from prebuilt images -- a C++ module can never be compiled into it, so installing it would do nothing.` Hint: `Lua and SQL modules still work on this server.`
- Tri-state everywhere: a compose that cannot answer is evidence of NOTHING — warn and proceed, never refuse on silence. Warn line (both surfaces, byte-identical): `could not read the compose configuration -- proceeding without the build-config check.`
- A fix on ONE surface only half-ships: every bash change mirrors Rust and vice versa, except where the spec records the exception (unbound is Rust-only; the explicit-overlay build step is native-only because WSL base composes carry `build:`).
- Shell files LF (`.gitattributes` enforces; never CRLF). NEVER edit `cli/dml` directly — edit `cli/src/*.sh`, run `bash cli/build.sh`.
- Never run bats and cargo suites at the same time (bats rewrites `cli/dml` that parity suites read).
- Run bats inside the distro: `wsl -d dml-arch -u dml --exec bash -lc 'cd /mnt/c/Users/perzi/dads-mmo-lab && bats cli/tests/<file> > /tmp/bats.out 2>&1; echo EXIT=$?'` then read `/tmp/bats.out` in a separate call. Never judge by a piped tail. Use `--exec`, never `--`.
- cargo from Windows: `%USERPROFILE%\.cargo\bin\cargo.exe` may need its full path. Verification runs need `--no-fail-fast` and the clean-env rule (`env -u DML_GAMES_DIR -u DML_BACKEND -u DML_SCRIPT -u DML_YQ_BIN` on POSIX; on PowerShell, `Remove-Item Env:DML_GAMES_DIR,Env:DML_BACKEND,Env:DML_SCRIPT,Env:DML_YQ_BIN -ErrorAction SilentlyContinue` first).
- No mid-test bare `!` in bats (use `run cmd` + status assert). No raw greps over YAML for structure (service-scoped awk only).
- Commit after every task with a descriptive message ending in the standard Co-Authored-By/Claude-Session trailer.

---

### Task 1: `buildcap.rs` — shared build-capability primitives

**Files:**
- Create: `crates/dml-wow/src/buildcap.rs`
- Modify: `crates/dml-wow/src/lib.rs` (add `pub mod buildcap;` in alphabetical position among the existing `pub mod` lines)

**Interfaces:**
- Produces: `buildcap::build_files(sdir: &Path) -> Vec<String>` — the `-f` argument set for a compose call that must see build config. `buildcap::worldserver_has_build(config_json: &str) -> Option<bool>` — parse of `docker compose config --format json` output; `None` = unparseable (tri-state "could not tell").
- Consumes: `crate::composegen::{BASE_FILE, OVERRIDE_FILE, BUILD_FILE}` (existing consts: `docker-compose.yml`, `docker-compose.override.yml`, `docker-compose.build.yml`).

- [ ] **Step 1: Write the failing tests** — create `buildcap.rs` with ONLY the test module first:

```rust
//! Build-capability primitives shared by the Unbound engine and the module
//! subsystem: which `-f` files a build-aware compose call needs, and whether
//! the effective config can build ac-worldserver at all.
//!
//! Extracted from `unbound.rs` (review CRITICAL 2026-08-02: `build:` lives in
//! `docker-compose.build.yml`, which compose NEVER auto-loads — a bare
//! `compose build` there builds NOTHING and exits 0) so `modmgr`'s rebuild
//! cannot drift from the engine that already got this right.

use std::path::Path;

use super::composegen::{BASE_FILE, BUILD_FILE, OVERRIDE_FILE};

#[cfg(test)]
mod tests {
    use super::*;

    fn tdir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("dml-buildcap-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_composegen_server_gets_all_three_files() {
        let d = tdir("three");
        for f in [BASE_FILE, OVERRIDE_FILE, BUILD_FILE] {
            std::fs::write(d.join(f), "").unwrap();
        }
        assert_eq!(
            build_files(&d),
            vec!["-f", BASE_FILE, "-f", OVERRIDE_FILE, "-f", BUILD_FILE]
        );
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_missing_override_is_skipped_but_base_and_build_survive() {
        let d = tdir("noover");
        std::fs::write(d.join(BASE_FILE), "").unwrap();
        std::fs::write(d.join(BUILD_FILE), "").unwrap();
        assert_eq!(build_files(&d), vec!["-f", BASE_FILE, "-f", BUILD_FILE]);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_server_without_the_build_overlay_needs_no_flags() {
        // bash-era/WSL servers keep build: in the base compose — an empty -f
        // set makes compose auto-load base+override, which is correct there.
        let d = tdir("wsl");
        std::fs::write(d.join(BASE_FILE), "").unwrap();
        assert_eq!(build_files(&d), Vec::<String>::new());
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn build_key_present_is_some_true() {
        let j = r#"{"services":{"ac-worldserver":{"build":{"context":"."},"image":"x"}}}"#;
        assert_eq!(worldserver_has_build(j), Some(true));
    }

    #[test]
    fn build_key_absent_is_some_false() {
        let j = r#"{"services":{"ac-worldserver":{"image":"dml.local/x:migrated"}}}"#;
        assert_eq!(worldserver_has_build(j), Some(false));
    }

    #[test]
    fn missing_service_is_some_false_and_garbage_is_none() {
        assert_eq!(worldserver_has_build(r#"{"services":{}}"#), Some(false));
        assert_eq!(worldserver_has_build("not json at all"), None);
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p dml-wow buildcap -- --nocapture`
Expected: COMPILE ERROR — `build_files`/`worldserver_has_build` not found. (Remember to add `pub mod buildcap;` to `lib.rs` first so the test module itself is reached.)

- [ ] **Step 3: Implement** — above the test module:

```rust
/// The `-f` set a compose call needs to SEE build config, from disk evidence.
/// Composegen servers keep `build:` in [`BUILD_FILE`] (never auto-loaded);
/// bash-era servers keep it in the base compose and need no flags. Body is
/// the former `UnboundEngine::resolve_build_files` verbatim.
pub fn build_files(sdir: &Path) -> Vec<String> {
    let mut files: Vec<String> = Vec::new();
    if sdir.join(BUILD_FILE).is_file() {
        for f in [BASE_FILE, OVERRIDE_FILE, BUILD_FILE] {
            if sdir.join(f).is_file() {
                files.push("-f".into());
                files.push(f.into());
            }
        }
    }
    files
}

/// Does the effective compose config let ac-worldserver build? Input is
/// `docker compose <files> config --format json` stdout. `None` means the
/// answer could not be read — tri-state, callers warn and proceed.
pub fn worldserver_has_build(config_json: &str) -> Option<bool> {
    let cfg: serde_json::Value = serde_json::from_str(config_json).ok()?;
    Some(
        cfg.get("services")
            .and_then(|s| s.get("ac-worldserver"))
            .map(|w| w.get("build").is_some())
            .unwrap_or(false),
    )
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p dml-wow buildcap -- --nocapture`
Expected: 6 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/dml-wow/src/buildcap.rs crates/dml-wow/src/lib.rs
git commit -m "feat(buildcap): shared build-file set + worldserver build-config check"
```

---

### Task 2: unbound.rs consumes buildcap

**Files:**
- Modify: `crates/dml-wow/src/unbound.rs:828-843` (`resolve_build_files`) and `:1081-1113` (the guard's inline JSON parse)

**Interfaces:**
- Consumes: `buildcap::build_files`, `buildcap::worldserver_has_build` (Task 1).
- Produces: nothing new — behaviour must be byte-identical; the existing unbound test suite is the parity proof.

- [ ] **Step 1: Replace `resolve_build_files`'s body** (keep the method — `self.build_files` is used in several places):

```rust
    /// Which `-f` files the BUILD needs, from disk evidence — see
    /// [`crate::buildcap::build_files`], extracted from this method.
    fn resolve_build_files(&mut self) {
        self.build_files = crate::buildcap::build_files(&self.sdir().to_path_buf());
    }
```

- [ ] **Step 2: Replace the guard's inline parse.** In the guard (around line 1086), the `if outcome.is_ok() { match serde_json::from_str... }` block becomes:

```rust
        if outcome.is_ok() {
            match crate::buildcap::worldserver_has_build(&out) {
                Some(has_build) => {
                    if !has_build {
                        return Err(Fail::new(
                            CODE_NO_BUILD_CONFIG,
                            "This server has no build configuration for ac-worldserver -- it runs from prebuilt images, so mod-unbound cannot be compiled into it.",
                            "Wrath Unbound needs a server whose worldserver is built from source (a native install, or a WSL install with its compose build sections). A migrated image-only server cannot take the add-on.",
                        ));
                    }
                }
                None => self.line(
                    "warn",
                    "could not parse the compose configuration -- proceeding without the build-config check.",
                ),
            }
        } else {
```

(The `else` warn arm below is unchanged. Message strings are unchanged — copy them exactly as they already are in the file.)

- [ ] **Step 3: Run the unbound suite**

Run: `cargo test -p dml-wow unbound -- --nocapture`
Expected: all pass, zero behaviour change. If any test fails, the extraction is NOT faithful — fix `buildcap`, not the test.

- [ ] **Step 4: Commit**

```bash
git add crates/dml-wow/src/unbound.rs
git commit -m "refactor(unbound): consume shared buildcap primitives"
```

---

### Task 3: Rust rebuild arm — guard, explicit overlay build, pct

**Files:**
- Modify: `crates/dml-wow/src/modmgr.rs:2154-2230` (`module_rebuild_stream`)
- Test: same file's `mod tests`

**Interfaces:**
- Consumes: `buildcap::build_files` / `worldserver_has_build` (Task 1), `crate::install_native::BuildProgress` (existing, `observe(&mut self, line) -> Option<u8>`), `dml_core::events::pct_event` (existing — verify the exact name with `grep -n "fn pct_event" crates/dml-core/src/events.rs`; unbound.rs already emits it), `dml_core::proc::run_streamed_lines` (existing, no-tee sibling of `run_streamed_unbounded` — signature at `crates/dml-core/src/proc.rs:506`).
- Produces: `pub fn module_rebuild_stream_with(docker_program: std::ffi::OsString, sdir: std::path::PathBuf, backup: Option<bool>, db_cfg: crate::db::DbConfig, emit: impl Fn(serde_json::Value))` plus the existing `module_rebuild_stream(backup, db_cfg, emit)` as a thin wrapper that resolves `native::docker_program()` and the server dir (keeping the NOT_FOUND/DOCKER_DOWN refusals in the wrapper or the `_with` — executor's choice, but the backup-choice BAD_ARG must stay the first statement either way, per the cli_integration pin). Tests drive `_with` with a fake docker script.

- [ ] **Step 1: Study the fake-binary harness.** Read `crates/dml-wow/src/lifecycle.rs` tests (search `LifecycleEnv` and the fake docker script builder) and copy its per-platform fake-executable helper shape (a `.cmd` on Windows, a shell script on Unix, `#[cfg]`-split — the Test-portability rule). The fake docker for this task must:
  - On `compose <...> config --format json`: print the JSON from env var `FAKE_CONFIG_JSON` and exit `FAKE_CONFIG_EXIT` (default 0).
  - On `compose <...> build ac-worldserver`: print two lines `#26 1.0 [10/100] Building CXX object modules/x.cpp.o` and `#26 2.0 [100/100] Linking CXX executable worldserver`, exit `FAKE_BUILD_EXIT` (default 0).
  - On anything else (`compose stop`, `compose up -d`, `info`): exit 0 silently.
  - Append every argv line to the file named by `FAKE_CALL_LOG` (the read-back-in-order oracle).

- [ ] **Step 2: Write the failing tests** (in `modmgr.rs`'s `mod tests`; `sdir` = tempdir with `docker-compose.yml` + `docker-compose.build.yml` written so `build_files` resolves; `db_cfg` = whatever existing tests construct — search `DbConfig` in this test module or construct via `crate::db::DbConfig::from_env()`-free test constructor used by `module_backup_now` tests):

```rust
    fn rebuild_events(fake_docker: &std::path::Path, sdir_env: &std::path::Path) -> Vec<serde_json::Value> {
        // Point title-dir resolution at the tempdir the same way the
        // cli_integration Sealed tests do: DML_GAMES_DIR + a title dir whose
        // name is config::TITLE containing the compose file. Collect emitted
        // events into a Vec via a RefCell closure.
        todo!("assemble from the existing test helpers in this module")
    }

    #[test]
    fn rebuild_refuses_before_backup_on_a_no_build_config() {
        // FAKE_CONFIG_JSON = {"services":{"ac-worldserver":{"image":"x"}}}
        // Expect: error event code MODULE_NO_BUILD_CONFIG, exit shape error,
        // and the call log contains NO mysqldump call and NO "build" call.
    }

    #[test]
    fn rebuild_builds_through_the_overlay_then_ups_without_it() {
        // FAKE_CONFIG_JSON = {"services":{"ac-worldserver":{"build":{"context":"."}}}}
        // --no-backup. Expect call log, in order:
        //   1. a line containing "config --format json"
        //   2. a line containing "stop"
        //   3. a line containing "-f docker-compose.yml" AND
        //      "-f docker-compose.build.yml" AND ending "build ac-worldserver"
        //      (NB the needle is the OVERLAY FILENAME — the argv never
        //      contains the substring "compose build" with the -f set)
        //   4. a line containing "up -d" and NOT containing "--build" and NOT
        //      containing "docker-compose.build.yml"
        // Expect events to contain at least one {"event":"pct"} (from the
        // fake ninja counters) and a final done {"rebuilt":true}.
    }

    #[test]
    fn rebuild_proceeds_with_a_warn_when_compose_cannot_answer() {
        // FAKE_CONFIG_EXIT=1. Expect a warn line containing "could not read
        // the compose configuration" and the build call still present.
    }
```

Write them as real tests (the `todo!` above is a planning sketch — the executor replaces it with the harness assembled in Step 1; if title-dir env plumbing proves impossible to seal inside a unit test, drive `_with` via a new `sdir: &Path` parameter instead and let the wrapper resolve the dir — prefer this simpler seam: `module_rebuild_stream_with(docker_program, sdir, backup, db_cfg, emit)`).

- [ ] **Step 3: Run to verify failure**

Run: `cargo test -p dml-wow modmgr::tests::rebuild -- --nocapture`
Expected: FAIL (`module_rebuild_stream_with` not found / assertions unmet).

- [ ] **Step 4: Implement.** Rework `module_rebuild_stream` into `module_rebuild_stream_with(docker_program: std::ffi::OsString, sdir: PathBuf, backup: Option<bool>, db_cfg, emit)`; the public `module_rebuild_stream` keeps its exact signature and body prefix (backup-choice gate FIRST, then title-dir/server-dir resolution, then `docker_engine_up` check) and delegates. Inside `_with`, after the engine check and BEFORE the backup block:

```rust
    // Build-config guard (spec 2026-08-09): refuse BEFORE the backup so a
    // server that can never compile does not first sit through a dump. The
    // overlay is never auto-loaded (unbound review CRITICAL 2026-08-02) — the
    // config probe must pass the same -f set the build will use.
    let bfiles = crate::buildcap::build_files(&sdir);
    let mut cfg_args: Vec<&str> = vec!["compose"];
    cfg_args.extend(bfiles.iter().map(String::as_str));
    cfg_args.extend(["config", "--format", "json"]);
    let mut cfg_cmd = Command::new(&docker_program);
    cfg_cmd.current_dir(&sdir).args(&cfg_args);
    windows_no_window(&mut cfg_cmd);
    let cfg_out = output_bounded_draining(cfg_cmd, Duration::from_secs(30));
    match cfg_out {
        Ok(out) if out.status.success() => {
            match crate::buildcap::worldserver_has_build(&String::from_utf8_lossy(&out.stdout)) {
                Some(false) => {
                    emit(modmgr::section_end(MODULE_REBUILD_SECTION, "error"));
                    emit(modmgr::error_event(
                        "MODULE_NO_BUILD_CONFIG",
                        "This server has no build configuration for ac-worldserver -- it runs from prebuilt images, so C++ modules cannot be compiled into it.",
                        "A rebuild needs a server built from source (a native install or a WSL install). A migrated image-only server cannot rebuild yet.",
                    ));
                    return;
                }
                Some(true) => {}
                None => emit(modmgr::line_event("warn", "could not read the compose configuration -- proceeding without the build-config check.")),
            }
        }
        _ => emit(modmgr::line_event("warn", "could not read the compose configuration -- proceeding without the build-config check.")),
    }
```

(Adapt the exact bounded-call shape to what `output_bounded_draining` returns in this file — the stop step at `modmgr.rs:2202-2206` is the local example.)

Then replace the single `up -d --build` block with:

```rust
    emit(modmgr::line_event(
        "info",
        format!("building (this can take 30-90 minutes; full log: {}/rebuild.log)...", sdir.display()),
    ));
    let log_path = sdir.join("rebuild.log");
    let mut build_args: Vec<&str> = vec!["compose"];
    build_args.extend(bfiles.iter().map(String::as_str));
    build_args.extend(["build", "ac-worldserver"]);
    let mut progress = crate::install_native::BuildProgress::default();
    let status = destructive::run_streamed_unbounded(&docker_program, &build_args, &sdir, &log_path, |line| {
        if let Some(pct) = progress.observe(line) {
            emit(dml_core::events::pct_event(pct));
        }
        emit(modmgr::line_event("info", line));
    });
    if !matches!(&status, Some(s) if s.success()) {
        emit(modmgr::section_end(MODULE_REBUILD_SECTION, "error"));
        emit(modmgr::error_event("BUILD_FAILED", "worldserver rebuild failed", &format!("Full log: {}/rebuild.log", sdir.display())));
        return;
    }
    emit(modmgr::line_event("info", "build finished -- starting the stack..."));
    // Plain up, NO overlay and NO --build: the installer's proven shape
    // (build via -f set, then bare `compose up -d`). run_streamed_lines is
    // the no-tee sibling — a second run_streamed_unbounded would TRUNCATE
    // the build log just written.
    let up_status = dml_core::proc::run_streamed_lines(
        &docker_program, &["compose", "up", "-d"], Some(&sdir),
        dml_core::proc::LineSplit::Newline,
        |line| emit(modmgr::line_event("info", line)),
    );
    if !matches!(&up_status, Some(s) if s.success()) {
        emit(modmgr::section_end(MODULE_REBUILD_SECTION, "error"));
        emit(modmgr::error_event("BUILD_FAILED", "the stack did not come back up after the build", &format!("Full log: {}/rebuild.log", sdir.display())));
        return;
    }
```

(Verify `run_streamed_lines`'s exact signature/visibility at `crates/dml-core/src/proc.rs:506` and export it through `destructive.rs`'s existing `pub use dml_core::proc::{...}` line if cleaner.)

- [ ] **Step 5: Run to verify pass**

Run: `cargo test -p dml-wow modmgr -- --nocapture`
Expected: new tests pass, all existing modmgr tests pass.

- [ ] **Step 6: Mutation check (manual, part of this task's evidence).** Comment out the guard block, run `rebuild_refuses_before_backup_on_a_no_build_config` — MUST go red. Restore. Comment the `bfiles` extension in `build_args` only — `rebuild_builds_through_the_overlay_then_ups_without_it` MUST go red. Restore. Record both results in the commit message.

- [ ] **Step 7: cli_integration check**

Run: `cargo test -p dml-wow-cli --test cli_integration module_rebuild -- --nocapture`
Expected: the existing `module_rebuild_without_a_backup_choice_streams_bad_arg_and_exits_1` still passes (the backup gate stayed first).

- [ ] **Step 8: Commit**

```bash
git add crates/dml-wow/src/modmgr.rs
git commit -m "fix(modmgr): rebuild compiles through the build overlay and refuses image-only servers

Mutation-verified: guard deletion reds the refusal test; dropping the -f set reds the overlay-argv test."
```

---

### Task 4: Rust cpp install/update guard

**Files:**
- Modify: `crates/dml-wow/src/modmgr.rs` — `module_install_stream` (:2025) and `module_update_stream` (:2096)
- Test: same file's `mod tests`

**Interfaces:**
- Consumes: `buildcap` (Task 1); the same fake-docker harness (Task 3).
- Produces: `pub fn cpp_build_guard(docker_program: &OsStr, sdir: &Path) -> Option<bool>` — `Some(false)` = refuse, `Some(true)` = fine, `None` = could not tell (warn+proceed). One function, both streams call it.

- [ ] **Step 1: Write the failing tests**

```rust
    #[test]
    fn cpp_install_refuses_on_a_no_build_server_before_cloning() {
        // family "cpp", registry key, fake docker answering config with no
        // build section. Expect error MODULE_NO_BUILD_CONFIG with the
        // install-flavoured message, and NO git call in the call log
        // (assert the modules/<key> dir was never created).
    }

    #[test]
    fn cpp_install_warns_and_proceeds_when_compose_cannot_answer() {
        // FAKE_CONFIG_EXIT=1 → warn line, then the normal clone path runs
        // (it will fail at git in the sealed env — assert the warn line
        // precedes the git failure, proving the guard did not refuse).
    }

    #[test]
    fn lua_and_sql_installs_skip_the_guard_entirely() {
        // family "lua" with the same no-build fake docker: the error must
        // NOT be MODULE_NO_BUILD_CONFIG (it will be a different, later
        // error in the sealed env — NOT_READY for missing mod-ale).
    }
```

- [ ] **Step 2: Run to verify failure** — `cargo test -p dml-wow modmgr::tests::cpp_install_refuses -- --nocapture` → FAIL.

- [ ] **Step 3: Implement.** Add near the other free functions:

```rust
/// Shared cpp-family guard: can this server ever compile a C++ module?
/// `None` = compose could not answer (tri-state; callers warn + proceed).
pub fn cpp_build_guard(docker_program: &OsStr, sdir: &Path) -> Option<bool> {
    let bfiles = crate::buildcap::build_files(sdir);
    let mut args: Vec<&str> = vec!["compose"];
    args.extend(bfiles.iter().map(String::as_str));
    args.extend(["config", "--format", "json"]);
    let mut cmd = Command::new(docker_program);
    cmd.current_dir(sdir).args(&args);
    windows_no_window(&mut cmd);
    match output_bounded_draining(cmd, Duration::from_secs(30)) {
        Ok(out) if out.status.success() => {
            crate::buildcap::worldserver_has_build(&String::from_utf8_lossy(&out.stdout))
        }
        _ => None,
    }
}
```

Refactor Task 3's guard to call this too (one probe implementation). In `module_install_stream`, inside the `"cpp"` match arm BEFORE `modmgr::install_cpp(...)`:

```rust
        "cpp" => {
            match modmgr::cpp_build_guard(docker_program.as_os_str(), &sdir) {
                Some(false) => {
                    emit(modmgr::section_end(modmgr::SECTION_INSTALL, "error"));
                    emit(modmgr::error_event(
                        "MODULE_NO_BUILD_CONFIG",
                        "This server runs from prebuilt images -- a C++ module can never be compiled into it, so installing it would do nothing.",
                        "Lua and SQL modules still work on this server.",
                    ));
                    return;
                }
                None => emit(modmgr::line_event("warn", "could not read the compose configuration -- proceeding without the build-config check.")),
                Some(true) => {}
            }
            modmgr::install_cpp(&git_program, &sdir, key.as_deref(), url.as_deref(), backup, &emit)
        }
```

In `module_update_stream`: same guard, applied only when the key is cpp-shaped AND present as a cpp clone (`modmgr::cpp_row(&key).is_some() || (crate::modules::valid_cpp_key(&key) && sdir.join("modules").join(&key).join(".git").is_dir())`), with `native::docker_program()` added to that stream (update its doc comment: the guard is a compose-file parse, not an engine call — `compose config` works with the engine down, and its failure only warns).

- [ ] **Step 4: Run to verify pass** — `cargo test -p dml-wow modmgr -- --nocapture` → all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/dml-wow/src/modmgr.rs
git commit -m "fix(modmgr): cpp install/update refuse on image-only servers"
```

---

### Task 5: `can_build` in the native module list

**Files:**
- Modify: `crates/dml-wow/src/modules.rs:344-348` (the `json!` assembly)
- Test: same file's tests (the existing assembly test around line 472-495)

**Interfaces:**
- Consumes: `composegen::BUILD_FILE`.
- Produces: `.data.can_build: bool` in the module-list JSON — `true` iff `<sdir>/docker-compose.build.yml` exists (disk evidence only, NO docker call on the list path).

- [ ] **Step 1: Failing test** — extend the existing assembly test: seed the fake sdir WITHOUT the build file, assert `data["can_build"] == false`; add a sibling test that writes `docker-compose.build.yml` and asserts `true`.
- [ ] **Step 2: Run to verify failure** — `cargo test -p dml-wow modules -- --nocapture` → FAIL (missing key).
- [ ] **Step 3: Implement** — in the `json!` block: `"can_build": self.sdir.join(super::composegen::BUILD_FILE).is_file(),` (adapt to the reader struct's actual sdir field name — grep `struct` at the top of `modules.rs`).
- [ ] **Step 4: Run to verify pass.** Also run `cargo test -p dml-wow --test module_parity -- --nocapture` — the parity suite deep-equals bash output, and bash does not emit `can_build` yet: if it FAILS on the new key, note it and proceed — Task 6 restores parity and re-runs this suite (never weaken the deep-equal).
- [ ] **Step 5: Commit**

```bash
git add crates/dml-wow/src/modules.rs
git commit -m "feat(modules): can_build from build-overlay disk evidence in the native list"
```

---

### Task 6: bash mirror — guard + `can_build` + stub + bats

**Files:**
- Modify: `cli/src/70-modules.sh` (new `_module_can_build`), `cli/src/90-main.sh` (rebuild arm ~5818-5859, cpp install arm ~5452-5520, cpp update arm ~5711, list emitter ~5374 and the catalog's static emitter ~5430), `cli/tests/helpers/env.bash` (docker stub `compose config` case)
- Test: `cli/tests/wow-module-rebuild.bats`, `cli/tests/wow-module-cpp.bats`
- Rebuild artifact: `bash cli/build.sh` (regenerates `cli/dml`)

**Interfaces:**
- Produces: `_module_can_build <sdir>` → prints `yes|no|unknown`. List JSON gains `"can_build":<bool>` (from the same helper: `yes`→true, `no`→false, `unknown`→true — fail OPEN on the list path, the refusal lives in the arms). Catalog's placeholder emitter (~line 5430) gains `"can_build":true` beside `"rebuild_pending":[]`.

- [ ] **Step 1: Extend the docker stub.** In `cli/tests/helpers/env.bash`'s `if [[ "${1:-}" == "compose" ]]` block, add a `config` subcase honoring `DML_STUB_COMPOSE_CONFIG`:

```bash
    # `docker compose config` -- canonicalized YAML. DML_STUB_COMPOSE_CONFIG:
    #   build   -> ac-worldserver with a build: section (source-built server)
    #   nobuild -> ac-worldserver without one (image-only server)
    #   fail    -> compose cannot answer (exit 1, nothing printed)
    # Default: build (the shape every existing test's server has).
    if [[ "$2" == "config" || " $* " == *" config "* ]]; then
        case "${DML_STUB_COMPOSE_CONFIG:-build}" in
            fail) exit 1 ;;
            nobuild)
                printf 'services:\n  ac-worldserver:\n    image: dml.local/x:migrated\n'
                ;;
            *)
                printf 'services:\n  ac-worldserver:\n    build:\n      context: .\n    image: acore/x\n'
                ;;
        esac
        exit 0
    fi
```

(Match the stub's existing arg-dispatch style — read the surrounding cases first; the stub exits 64 on argv drift, so mimic how `ps` subcases test membership.)

- [ ] **Step 2: Write the failing bats tests.** In `cli/tests/wow-module-rebuild.bats` (follow the file's existing setup/helpers):

```bash
@test "module rebuild refuses an image-only server before the backup" {
    export DML_STUB_COMPOSE_CONFIG=nobuild
    run "$DML" wow module rebuild --backup --json
    [ "$status" -eq 1 ]
    echo "$output" | grep -q '"code":"MODULE_NO_BUILD_CONFIG"'
    # Refusal precedes the backup: no dump narration may appear.
    run bash -c "echo '$output' | grep -c 'backing up'"
    [ "$output" = "0" ]
}

@test "module rebuild warns and proceeds when compose config cannot answer" {
    export DML_STUB_COMPOSE_CONFIG=fail
    run "$DML" wow module rebuild --no-backup --json
    echo "$output" | grep -q 'could not read the compose configuration'
    run bash -c "echo '$output' | grep -c '\"code\":\"MODULE_NO_BUILD_CONFIG\"'"
    [ "$output" = "0" ]
}
```

In `cli/tests/wow-module-cpp.bats`:

```bash
@test "cpp install refuses an image-only server before cloning" {
    export DML_STUB_COMPOSE_CONFIG=nobuild
    run "$DML" wow module install --family cpp --key mod-transmog --json
    [ "$status" -eq 1 ]
    echo "$output" | grep -q '"code":"MODULE_NO_BUILD_CONFIG"'
    [ ! -d "$SERVER_DIR/modules/mod-transmog" ]
}

@test "module list emits can_build" {
    run "$DML" wow module list --json
    [ "$status" -eq 0 ]
    echo "$output" | jq -e '.data.can_build == true'
}
```

(Adapt variable names `$DML`/`$SERVER_DIR` to what the two files already use; NO mid-test bare `!` — the `[ ! -d ... ]` bracket form asserts, a bare `! cmd` line does not.)

- [ ] **Step 3: Run to verify failure**

Run: `wsl -d dml-arch -u dml --exec bash -lc 'cd /mnt/c/Users/perzi/dads-mmo-lab && bats cli/tests/wow-module-rebuild.bats cli/tests/wow-module-cpp.bats > /tmp/bats.out 2>&1; echo EXIT=$?'` then read `/tmp/bats.out`.
Expected: the new tests FAIL (no guard yet), existing ones pass.

- [ ] **Step 4: Implement.** `cli/src/70-modules.sh`, next to `_rebuild_pending_*`:

```bash
# Build-capability probe (spec 2026-08-09). Asks `docker compose config`
# (client-side: works with the engine down) whether ac-worldserver can build.
# Service-scoped awk over the CANONICALIZED yaml -- never a raw grep (the
# _stack_is_ac lesson). Prints yes|no|unknown; unknown must never refuse.
_module_can_build() {
    local cfg
    cfg="$(cd "$1" 2>/dev/null && docker compose config 2>/dev/null)" || { echo unknown; return 0; }
    [[ -z "$cfg" ]] && { echo unknown; return 0; }
    if printf '%s\n' "$cfg" | awk '
        /^services:/ { insvc=1; next }
        insvc && /^[^ ]/ { insvc=0 }
        insvc && /^  ac-worldserver:/ { inws=1; next }
        inws && /^  [^ ]/ { inws=0 }
        inws && /^    build:/ { found=1; exit }
        END { exit found?0:1 }
    '; then echo yes; else echo no; fi
}
```

`90-main.sh` rebuild arm — after the `docker info` check, before the backup block:

```bash
            case "$(_module_can_build "$sdir")" in
              no)
                ndjson_section_end module-rebuild error
                ndjson_error MODULE_NO_BUILD_CONFIG "This server has no build configuration for ac-worldserver -- it runs from prebuilt images, so C++ modules cannot be compiled into it." "A rebuild needs a server built from source (a native install or a WSL install). A migrated image-only server cannot rebuild yet."; exit 1
                ;;
              unknown) ndjson_line warn "could not read the compose configuration -- proceeding without the build-config check." ;;
            esac
```

cpp install arm — after the `cpp)` line's backup-flag check, before the url/key validation:

```bash
                case "$(_module_can_build "$sdir")" in
                  no)
                    ndjson_section_end module-install error
                    ndjson_error MODULE_NO_BUILD_CONFIG "This server runs from prebuilt images -- a C++ module can never be compiled into it, so installing it would do nothing." "Lua and SQL modules still work on this server."; exit 1
                    ;;
                  unknown) ndjson_line warn "could not read the compose configuration -- proceeding without the build-config check." ;;
                esac
```

cpp update arm (~5711): the identical block (section name `module-update` in the two ndjson calls). List emitter: before the final `json_ok`, `canb=true; [[ "$(_module_can_build "$sdir")" == no ]] && canb=false` and add `,\"can_build\":$canb` after `\"ale_ready\":$aleready`. Catalog arm: add `,\"can_build\":true` beside its `"rebuild_pending":[]` placeholder.

- [ ] **Step 5: Rebuild the artifact and run**

```bash
bash cli/build.sh
```

Then the Step 3 bats command again. Expected: all pass. Then the FULL module bats set: `bats cli/tests/wow-module-*.bats` (same wsl pattern) — no regressions.

- [ ] **Step 6: Restore Rust↔bash list parity.** Run `cargo test -p dml-wow --test module_parity -- --nocapture` (AFTER bats — never overlapped). Expected: deep-equal passes again with both sides emitting `can_build`. NB the parity suite runs bash `dml` against a native-shaped dir where `docker` may be the REAL docker — if `_module_can_build` answers `unknown`, the fail-open `true` matches Rust's `BUILD_FILE`-derived `true` on that fixture; if the suite fixture has no BUILD_FILE, check what bash answers there and reconcile (the honest reconciliation: bash fail-open true, Rust disk-evidence — if they disagree on the parity fixture, the fixture gets a BUILD_FILE).
- [ ] **Step 7: Commit**

```bash
git add cli/src/70-modules.sh cli/src/90-main.sh cli/dml cli/tests/helpers/env.bash cli/tests/wow-module-rebuild.bats cli/tests/wow-module-cpp.bats
git commit -m "fix(cli): mirror the build-config guard + can_build on the bash surface"
```

---

### Task 7: Unbound fetches the pin before refusing

**Files:**
- Modify: `crates/dml-wow/src/unbound.rs:1244-1258` (the re-pin path in `do_clone_ale`)
- Test: same file's tests (sibling of `the_ale_pin_failing_is_a_refusal_not_a_warning`, :3094)

**Interfaces:**
- Consumes: the engine's existing `git_probe` Call builder and `run_collect`; `MOD_ALE_COMMIT`.
- Produces: no new API — behaviour: pin mismatch → fetch the pin → retry checkout → only then refuse.

- [ ] **Step 1: Failing tests**

```rust
    #[test]
    fn a_shallow_ale_clone_is_rescued_by_fetching_the_pin() {
        let (games, sdir) = fake_server("aleshallow");
        // First checkout fails (shallow clone lacks the commit), the fetch
        // succeeds, the RETRY checkout succeeds: FakeIo::set replaces
        // same-key entries, so script the sequence with distinct keys —
        // checkout fails, "fetch origin" succeeds (default 0), and after a
        // fetch the retry uses the same "checkout --quiet" key: use the
        // call-count reply form if FakeIo supports it, else assert via the
        // refusal NOT happening when fetch is scripted ok but checkout
        // stays failing is impossible — so instead script checkout to fail
        // ONCE (FakeIo's first-match semantics were replaced by set();
        // check FakeIo for a fail-N-times helper and use the simplest
        // available shape that lets the retry succeed).
        let io = FakeIo::happy(&sdir).reply_once("checkout --quiet", 1, &["error: pathspec"]);
        let (code, events) = run(&io, &opts_for(&games));
        // The install proceeds past clone-ale (it will stop at some LATER
        // stage in this fixture or complete — assert NO ALE_PIN_MISMATCH
        // error and that the log contains a "fetch" git call).
        assert_ne!(error_code(&events), CODE_ALE_PIN_MISMATCH, "{events:?}");
        assert!(io.log().iter().any(|l| l.contains("fetch origin")), "{:#?}", io.log());
        let _ = code;
    }

    #[test]
    fn a_failing_fetch_still_refuses_with_the_pin_mismatch() {
        let (games, sdir) = fake_server("alefetchfail");
        let io = FakeIo::happy(&sdir)
            .reply("checkout --quiet", 1, &["error: pathspec"])
            .reply("fetch origin", 1, &["fatal: could not read from remote"]);
        let (code, events) = run(&io, &opts_for(&games));
        assert_eq!(code, 1);
        assert_eq!(error_code(&events), CODE_ALE_PIN_MISMATCH);
        assert!(!io.log().iter().any(|l| l.contains("compose build")));
    }
```

(If `FakeIo` has no `reply_once`, add one in the test-support impl following `set`'s replace-in-place semantics — a scripted reply consumed on first match, falling back to default afterwards. The 2026-07-29 gotcha is the reason `set` exists; `reply_once` must be added with the same care: prove it takes effect by the fetch test going red when the production fetch is removed.)

- [ ] **Step 2: Run to verify failure** — `cargo test -p dml-wow unbound::tests::a_shallow -- --nocapture` → FAIL (no fetch in production yet).

- [ ] **Step 3: Implement.** In the mismatch path, replace the single checkout-and-refuse with:

```rust
                if !head.starts_with(MOD_ALE_COMMIT) {
                    self.line("info", "mod-ale is present but not at the pinned commit -- re-pinning it.");
                    let mut co = self.checkout_ale_pin(&ale);
                    if !co {
                        // A Modules-page install is a --depth 1 clone: the
                        // pin is simply absent locally. Fetch exactly it,
                        // then retry, before concluding anything.
                        self.line("info", "the pinned commit is not in the local clone (shallow module-page clone) -- fetching it...");
                        let (f, _) = self.run_collect(&self.git_probe(vec![
                            "-C".into(), ale.display().to_string(),
                            "fetch".into(), "origin".into(), MOD_ALE_COMMIT.into(),
                        ]));
                        if f.is_ok() {
                            co = self.checkout_ale_pin(&ale);
                        }
                    }
                    if !co {
                        return Err(Fail::new(
                            CODE_ALE_PIN_MISMATCH,
                            format!("mod-ale is checked out at {head}, not the pinned commit, and re-pinning failed."),
                            "The add-on is tested against exactly that commit. Shallow clones from the Modules page lack it; the automatic fetch also failed -- check the network, or remove modules/mod-ale and run this again.",
                        ));
                    }
                }
```

with a small private helper (the checkout call appears twice now):

```rust
    fn checkout_ale_pin(&self, ale: &Path) -> bool {
        let (co, _) = self.run_collect(&self.git_probe(vec![
            "-C".into(), ale.display().to_string(),
            "checkout".into(), "--quiet".into(), MOD_ALE_COMMIT.into(),
        ]));
        co.is_ok()
    }
```

NB `git_probe` is a bounded probe — a fetch of ONE commit is small but does hit the network; check `git_probe`'s timeout constant and, if it is under 30s, build the fetch `Call` with the crate's git network timeout instead (grep `GIT_NET_TIMEOUT` in `modmgr.rs` for the convention).

- [ ] **Step 4: Run to verify pass** — `cargo test -p dml-wow unbound -- --nocapture` → all pass, including the two pre-existing pin tests.
- [ ] **Step 5: Commit**

```bash
git add crates/dml-wow/src/unbound.rs
git commit -m "fix(unbound): fetch the mod-ale pin before refusing shallow clones"
```

---

### Task 8: Launcher — `can_build` fail-open + standing rebuild card

**Files:**
- Modify: `launcher/src/lib/api.ts:452-456` (ModuleList), `launcher/src/lib/pages/ModuleManager.svelte:590-614` (the pending banner becomes the always-visible rebuild card)
- Create: `launcher/src/lib/module-canbuild.ts`, `launcher/src/lib/module-canbuild.test.ts`

**Interfaces:**
- Consumes: `ModuleList` (gains `can_build?: boolean`), existing `wowModuleRebuild`, `featureLocked`, `backupChecked`, `confirmingRebuild`, `rebuild()`.
- Produces: `canBuild(list: { can_build?: boolean } | null): boolean` — `false` ONLY on an explicit `false`; missing/undefined/null list → `true` (fail open, the `install_supported` pattern).

**Design note (deviation from spec §6, flagged for review):** the spec said "banner stays + new standing button", but two rebuild buttons sharing one confirm flow is worse UX and duplicated state. This task merges them: ONE always-visible "Server rebuild" card that strengthens to the warn style with the "required for: X" line when `rebuild_pending` is non-empty. Same information, one control.

- [ ] **Step 1: Failing vitest** — `module-canbuild.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { canBuild } from "./module-canbuild";

describe("canBuild", () => {
  it("fails open when the field is missing (older CLI)", () => {
    expect(canBuild({} as never)).toBe(true);
    expect(canBuild(null)).toBe(true);
  });
  it("honours an explicit false", () => {
    expect(canBuild({ can_build: false })).toBe(false);
  });
  it("honours an explicit true", () => {
    expect(canBuild({ can_build: true })).toBe(true);
  });
});
```

- [ ] **Step 2: Run to verify failure** — `cd launcher && npx vitest run src/lib/module-canbuild.test.ts` → FAIL (module missing).
- [ ] **Step 3: Implement** — `module-canbuild.ts`:

```ts
// can_build is additive (2026-08-09): an older CLI omits it, and a missing
// answer must never disable the rebuild button on a server that can build —
// the authoritative refusal lives in the CLI arm. Fail OPEN, exactly like
// normalizeCatalog's install_supported.
export function canBuild(list: { can_build?: boolean } | null): boolean {
  return list?.can_build !== false;
}
```

`api.ts`: add `can_build?: boolean;` to `ModuleList`.

`ModuleManager.svelte`: replace the `{#if list && list.rebuild_pending.length > 0}` block with:

```svelte
  {#if list}
    {@const buildable = canBuild(list)}
    <div class="card {list.rebuild_pending.length > 0 ? 'warn-card' : ''}">
      {#if list.rebuild_pending.length > 0}
        <p><strong>Server rebuild required for: {list.rebuild_pending.join(", ")}</strong></p>
      {:else}
        <p><strong>Server rebuild</strong> — recompiles the worldserver with the currently installed C++ modules.</p>
      {/if}
      {#if !buildable}
        <p class="muted">This server runs prebuilt images — C++ modules can't be compiled into it, so rebuild is unavailable.</p>
      {/if}
      <label class="row">
        <input type="checkbox" bind:checked={backupChecked} disabled={busy || !buildable} />
        Back up the server first (recommended)
      </label>
      <div class="row">
        {#if !confirmingRebuild}
          <button
            class="primary"
            onclick={rebuild}
            disabled={busy || !buildable || featureLocked("modules-rebuild")}
            title={!buildable
              ? "This server runs prebuilt images — rebuild is unavailable."
              : featureLocked("modules-rebuild") ? LOCKED_HINT : undefined}
          >
            {list.rebuild_pending.length > 0 ? "Rebuild now" : "Rebuild server"}
          </button>
        {:else}
          <span>Rebuild takes 30–90 minutes and stops the world while building. Continue?</span>
          <button class="primary" onclick={rebuild} disabled={busy}>Confirm</button>
          <button onclick={() => (confirmingRebuild = false)} disabled={busy}>Cancel</button>
        {/if}
      </div>
    </div>
  {/if}
```

Add `import { canBuild } from "../module-canbuild";` beside the file's existing lib imports (check the relative path against its neighbours).

- [ ] **Step 4: Run to verify pass** — `npx vitest run` (whole lib suite) → all pass. `npx svelte-check --threshold error` if the repo's launcher check script exists (`grep '"check"' launcher/package.json` — run whatever it defines).
- [ ] **Step 5: Commit**

```bash
git add launcher/src/lib/api.ts launcher/src/lib/module-canbuild.ts launcher/src/lib/module-canbuild.test.ts launcher/src/lib/pages/ModuleManager.svelte
git commit -m "feat(launcher): standing rebuild card, disabled honestly on image-only servers"
```

---

### Task 9: Contract docs + CLAUDE.md + full verification battery

**Files:**
- Modify: `docs/cli-contract.md` (module list `.data` gains `can_build`; `module rebuild`/`module install` error tables gain `MODULE_NO_BUILD_CONFIG`; rebuild's build-step description now names the overlay build + `pct` events), `cli/README.md` (same two additions in the wow subcommands section), `crates/CLAUDE.md` (one line under the modmgr/incident notes: rebuild builds through the overlay since 2026-08-09; bare `up -d --build` on a composegen server compiles nothing), `cli/CLAUDE.md` (mirror note: `_module_can_build` tri-state guard)

- [ ] **Step 1: Make the doc edits.** Keep each addition to the existing document's own style; the error-code rows copy the exact strings from Global Constraints.
- [ ] **Step 2: Full Rust battery** (clean env, no bats running):

Run: `env -u DML_GAMES_DIR -u DML_BACKEND -u DML_SCRIPT -u DML_YQ_BIN cargo test --workspace --no-fail-fast` (from Git Bash; on PowerShell clear the vars first as in Global Constraints)
Expected: totals identical to a run WITH the ambient vars (except the live-gated parity suites, which may skip without the snapshot server — record skip counts). Sum the `test result:` lines; zero failures.
- [ ] **Step 3: Full bats battery** (after cargo finishes):

Run: `wsl -d dml-arch -u dml --exec bash -lc 'cd /mnt/c/Users/perzi/dads-mmo-lab && bats cli/tests/ > /tmp/bats.out 2>&1; echo EXIT=$?'` then read `/tmp/bats.out` counts (`grep -c '^not ok'` must be 0).
- [ ] **Step 4: Launcher battery** — `cd launcher && npx vitest run` → zero failures.
- [ ] **Step 5: Mirror review** — dispatch the `dml-mirror-reviewer` agent over the branch diff (`git diff main...HEAD -- cli/src crates launcher/src-tauri` scope note: this round touched cli/src, crates, launcher/src). Address any mirror-completeness findings before closing.
- [ ] **Step 6: Commit**

```bash
git add docs/cli-contract.md cli/README.md crates/CLAUDE.md cli/CLAUDE.md
git commit -m "docs: contract + notes for the module-rebuild fix"
```

---

## User live gates (after all tasks — not agent work)

1. VM: update the launcher build, copy `mod-city-bots` into the server's `modules/`, click **Rebuild server**, watch compile lines + the progress bar, then `mod-city-bots: stage cast loaded: 400 roster entries` in the world log.
2. This machine (migrated install): the rebuild card renders disabled with the prebuilt-images hint; `dml-wow module rebuild --no-backup` refuses with `MODULE_NO_BUILD_CONFIG`.
3. VM: `modules/mod-ale` deleted + Unbound **Resume install** (already unblocked manually; the Task 7 fix makes the manual step unnecessary for the next person).
