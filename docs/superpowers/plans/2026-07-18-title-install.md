# Title Install & Remove Implementation Plan (Round D)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `dml games catalog|install|remove` + an interactive install terminal in the launcher (runs the DML's interactive installer scripts unchanged) + Library page catalog/remove.

**Architecture:** New `cli/src/80-titles.sh` registry (6 titles) + three `games` arms. `install` is TEXT-MODE ONLY — plain stdin/stdout passthrough of `bash <installer> 2>&1` plus a post-install symlink for legacy `$HOME/<id>` layouts. The Rust runner gains `command_raw` (no `--json`) + `spawn_interactive` (piped stdin/stdout child); `games_install` streams raw chunks over a Channel while the child's stdin handle lives in `AppState` for `games_install_input`/`games_install_cancel` (taskkill by PID). Library gets Available-titles rows, a typed remove confirm, and the InstallTerminal component.

**Tech Stack:** bash + bats, Rust (std::process, Tauri Channel), Svelte 5.

## Global Constraints

- Branch `feat/dml-launcher-windows`; NO merge. `cli/dml` committed artifact (build.sh; never hand-edit). `set -euo pipefail` discipline; NO `local` in dispatch arms.
- Installer scripts run UNCHANGED — no edits under `guides/`. They ship to the distro at `${DML_INSTALLERS_DIR:-/usr/local/share/dml/installers}` via `cli/dev-install.ps1`.
- `games install` REJECTS `--json` (`BAD_ARG` "games install is interactive" / hint "Run it from the launcher's install terminal or a real terminal (no --json)."). Text-mode errors go to stderr as `[dml] ERROR: …` lines, exit 1. Exit code of the installer passes through. The `home`-kind symlink (`ln -sfn "$HOME/<id>" "$GAMES_DIR/<id>"`) is created ONLY when the installer exited 0 AND `$HOME/<id>` exists.
- `games remove` requires `--yes`; without it → `CONFIRM_REQUIRED` whose message lists every path that would be deleted. With it: `docker compose down` when a compose file exists (failure tolerated), then delete — for a `$GAMES_DIR/<id>` symlink: BOTH the link and its resolved target; any plain `$GAMES_DIR/<id>` or `$HOME/<id>` dir; `$HOME/<launcher-file>` when the registry names one. `~/.dml` (backups etc.) is NEVER touched. All ids are registry-validated (closed set) before ANY path use.
- Registry rows verbatim (id|name|script|kind|launcher-file):
  `wow-server-playerbots|WoW WotLK (Playerbots)|install-wow-wotlk.sh|games|wow-playerbots-launcher.sh`, `wow-vanilla-server|WoW Vanilla|install-wow-vanilla.sh|home|wow-vanilla-launcher.sh`, `wow-tbc-server|WoW TBC|install-wow-tbc.sh|home|wow-tbc-launcher.sh`, `maplestory-server|MapleStory v83|install-maplestory.sh|home|maplestory-launcher.sh`, `runescape-server|RuneScape|install-runescape.sh|home|runescape-launcher.sh`, `muonline-server|MU Online|install-muonline.sh|home|muonline-launcher.sh`.
- Rust: interactive spawn uses a command builder WITHOUT `--json`; chunk reader (4 KiB reads, `decode_wsl_output`, `{event:"chunk",text}`), `{event:"exit",code}` at EOF; one session at a time (`BUSY`), input errors `NO_SESSION` when idle; cancel = `taskkill /F /T /PID <pid>` with CREATE_NO_WINDOW; session cleared on natural exit AND after cancel (the reader loop's EOF does it).
- UI: typed remove confirm (must type the title id exactly); install terminal strips ANSI, sticky-autoscrolls, disables input after exit, Cancel has its own confirm copy `Cancelling mid-install can leave a partial install behind. Cancel anyway?`. Streamed remove derives outcome from done/error events (sawDone/streamErr, applied after refresh).
- Gates: full bats in WSL; `npm test`; `npm run check`; `cargo test`. Baselines entering D: bats 320, vitest 20, cargo 17, check 0/0.
- Commits end with `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`.

---

## File Map

- Task 1: Create `cli/src/80-titles.sh`; modify `cli/src/90-main.sh` (three arms in the `games` dispatch); modify `cli/dev-install.ps1`; create `cli/tests/games-titles.bats`
- Task 2: `launcher/src-tauri/src/dml/runner.rs` (+fixture), `launcher/src-tauri/src/lib.rs`, `launcher/src/lib/api.ts`
- Task 3: `launcher/src/lib/pages/Library.svelte`, create `launcher/src/lib/InstallTerminal.svelte`

---

### Task 1: CLI catalog/install/remove + dev-install + bats

**Files:** Create `cli/src/80-titles.sh`; modify `cli/src/90-main.sh` (arms inside the top-level `games`… wait — the games verbs are TOP-LEVEL arms of the main `case "$cmd"` dispatch: `list)`, `status)`, `start)`, `stop)` etc. Add `catalog)`, `install)`, `remove)` as sibling top-level arms — read the dispatch first; they are invoked as `dml games <sub>`? NO: check `cli/src/00-head.sh`/dispatch — the CLI is invoked `dml games start <id>`; the dispatch consumes `games` then subs. Find the existing `start)`/`stop)` arms and place the three new arms beside them, following exactly how `start)` reads `"$1"` as the title id.); modify `cli/dev-install.ps1`; create `cli/tests/games-titles.bats`. Commit regenerated `cli/dml`.

**Interfaces produced:** `_installers_dir`, `_title_registry`, `_title_row <id>` (stdout row or empty), `_title_server_dir <id> <kind>`, `_title_installed <id> <kind>` (exit status); `dml games catalog --json`; `dml games install <id>` (text-mode); `dml games remove <id> --yes --json` (NDJSON).

- [ ] **Step 1: Create `cli/src/80-titles.sh`:**

```bash
# ---------------------------------------------------------------------------
# Title (game) catalog + install/remove support (Round D).
# Installer scripts are the DML's own interactive bash installers, shipped
# unchanged to _installers_dir by cli/dev-install.ps1 and run with plain
# stdin/stdout passthrough (the launcher gives them a real terminal).
# ---------------------------------------------------------------------------

_installers_dir() { echo "${DML_INSTALLERS_DIR:-/usr/local/share/dml/installers}"; }

# id|display name|installer script|kind(games=installer manages ~/games itself,
# home=legacy $HOME/<id> layout needing a post-install symlink)|launcher file
_title_registry() {
cat <<'EOF'
wow-server-playerbots|WoW WotLK (Playerbots)|install-wow-wotlk.sh|games|wow-playerbots-launcher.sh
wow-vanilla-server|WoW Vanilla|install-wow-vanilla.sh|home|wow-vanilla-launcher.sh
wow-tbc-server|WoW TBC|install-wow-tbc.sh|home|wow-tbc-launcher.sh
maplestory-server|MapleStory v83|install-maplestory.sh|home|maplestory-launcher.sh
runescape-server|RuneScape|install-runescape.sh|home|runescape-launcher.sh
muonline-server|MU Online|install-muonline.sh|home|muonline-launcher.sh
EOF
}

# Prints the registry row for an id, or nothing (exact-key match).
_title_row() {
    local row
    row="$(_title_registry | grep -m1 -F "$1|" || true)"
    [[ "$row" == "$1|"* ]] && printf '%s' "$row"
    return 0
}

# Primary server dir for a title by kind.
_title_server_dir() {
    if [[ "$2" == games ]]; then echo "$GAMES_DIR/$1"; else echo "$HOME/$1"; fi
}

# Exit-status helper: is the title present at either location? (wotlk may
# live at the legacy $HOME path with a games/ symlink, or vice versa.)
_title_installed() {
    [[ -d "$GAMES_DIR/$1" || -d "$HOME/$1" ]]
}
```

- [ ] **Step 2: Write the failing bats suite `cli/tests/games-titles.bats`:**

```bash
#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_docker_stub
  export HOME="$FIXTURE"
  export DML_INSTALLERS_DIR="$FIXTURE/installers"
  mkdir -p "$DML_INSTALLERS_DIR"
}
teardown() { teardown_fixture; }

fake_installer() {  # fake_installer <script-name> <server-dir-to-create>
  cat > "$DML_INSTALLERS_DIR/$1" <<EOF
#!/usr/bin/env bash
read -r answer
echo "you said: \$answer"
echo "\$answer" > "$FIXTURE/got-stdin"
mkdir -p "$2"
exit 0
EOF
}

@test "games catalog: registry rows with installed/script_available" {
  mkdir -p "$FIXTURE/maplestory-server"
  fake_installer install-maplestory.sh "$FIXTURE/maplestory-server"
  run bash "$DML" games catalog --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.titles | length')" = "6" ]
  [ "$(echo "$output" | jq -r '.data.titles[] | select(.id=="maplestory-server") | .installed')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.titles[] | select(.id=="maplestory-server") | .script_available')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.titles[] | select(.id=="runescape-server") | .installed')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.titles[] | select(.id=="runescape-server") | .script_available')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.titles[] | select(.id=="runescape-server") | .running')" = "null" ]
}

@test "games install: --json rejected, unknown id rejected, EXISTS when installed" {
  run bash "$DML" games install maplestory-server --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'BAD_ARG'
  run bash "$DML" games install not-a-title
  [ "$status" -eq 1 ]
  mkdir -p "$FIXTURE/maplestory-server"
  run bash "$DML" games install maplestory-server
  [ "$status" -eq 1 ]
  echo "$output" | grep -qi 'already installed'
}

@test "games install: missing script -> NO_SCRIPT-style error" {
  run bash "$DML" games install maplestory-server
  [ "$status" -eq 1 ]
  echo "$output" | grep -qi 'installer script'
}

@test "games install: runs the script with stdin passthrough, symlinks home-kind" {
  fake_installer install-maplestory.sh "$FIXTURE/maplestory-server"
  run bash -c 'echo hello | bash "'"$DML"'" games install maplestory-server'
  [ "$status" -eq 0 ]
  [ "$(cat "$FIXTURE/got-stdin")" = "hello" ]
  echo "$output" | grep -q 'you said: hello'
  [ -L "$FIXTURE/games/maplestory-server" ]
  [ "$(readlink "$FIXTURE/games/maplestory-server")" = "$FIXTURE/maplestory-server" ]
}

@test "games install: installer failure -> exit code passes through, no symlink" {
  cat > "$DML_INSTALLERS_DIR/install-maplestory.sh" <<'EOF'
#!/usr/bin/env bash
echo boom
exit 7
EOF
  run bash "$DML" games install maplestory-server
  [ "$status" -eq 7 ]
  [ ! -e "$FIXTURE/games/maplestory-server" ]
}

@test "games install: declined install (exit 0, no dir) -> no phantom symlink" {
  cat > "$DML_INSTALLERS_DIR/install-maplestory.sh" <<'EOF'
#!/usr/bin/env bash
echo "aborted by user"
exit 0
EOF
  run bash "$DML" games install maplestory-server
  [ "$status" -eq 0 ]
  [ ! -e "$FIXTURE/games/maplestory-server" ]
}

@test "games remove: unknown id / not installed / no --yes" {
  run bash "$DML" games remove not-a-title --json
  [ "$status" -eq 1 ]
  run bash "$DML" games remove maplestory-server --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'NOT_FOUND'
  mkdir -p "$FIXTURE/maplestory-server"
  run bash "$DML" games remove maplestory-server --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'CONFIRM_REQUIRED'
  echo "$output" | grep -q 'maplestory-server'
  [ -d "$FIXTURE/maplestory-server" ]
}

@test "games remove --yes: deletes dir + symlink + launcher file, keeps ~/.dml" {
  mkdir -p "$FIXTURE/maplestory-server" "$FIXTURE/games" "$FIXTURE/.dml/backups"
  touch "$FIXTURE/maplestory-server/docker-compose.yml"
  ln -s "$FIXTURE/maplestory-server" "$FIXTURE/games/maplestory-server"
  touch "$FIXTURE/maplestory-launcher.sh" "$FIXTURE/.dml/backups/keepme"
  run bash "$DML" games remove maplestory-server --yes --json
  [ "$status" -eq 0 ]
  echo "$output" | grep -q '"event":"done"'
  [ ! -e "$FIXTURE/maplestory-server" ]
  [ ! -e "$FIXTURE/games/maplestory-server" ]
  [ ! -e "$FIXTURE/maplestory-launcher.sh" ]
  [ -f "$FIXTURE/.dml/backups/keepme" ]
}

@test "games remove --yes: compose down attempted when compose exists" {
  mkdir -p "$FIXTURE/maplestory-server"
  touch "$FIXTURE/maplestory-server/docker-compose.yml"
  run bash "$DML" games remove maplestory-server --yes --json
  [ "$status" -eq 0 ]
  echo "$output" | grep -qi 'stopping'
}
```

- [ ] **Step 3: run — FAIL.** Step 4: the three arms in `90-main.sh`, placed beside the existing `start)`/`stop)` top-level games arms (READ the dispatch first and follow its `gid="${1:-}"` convention exactly):

```bash
  catalog)
    tout='['; first=1
    while IFS='|' read -r tid tname tscript tkind tlauncher; do
      [[ -z "$tid" ]] && continue
      tinst=false; _title_installed "$tid" && tinst=true
      tscriptok=false; [[ -f "$(_installers_dir)/$tscript" ]] && tscriptok=true
      trun=null
      if [[ "$tinst" == true ]]; then
        tdir="$GAMES_DIR/$tid"; [[ -d "$tdir" ]] || tdir="$HOME/$tid"
        tcompose="$(_resolve_compose_dir "$tdir/")"
        if [[ -n "$tcompose" ]] && [[ "$(_compose_running "$tcompose")" -gt 0 ]]; then
          trun='"running"'
        else
          trun='"stopped"'
        fi
      fi
      [[ $first -eq 0 ]] && tout+=','
      tout+="{\"id\":\"$tid\",\"name\":\"$(json_escape "$tname")\",\"installed\":$tinst,\"running\":$trun,\"script_available\":$tscriptok}"
      first=0
    done < <(_title_registry)
    tout+=']'
    json_ok "{\"titles\":$tout}"
    ;;
  install)
    gid="${1:-}"
    if [[ "$DML_JSON" == 1 ]]; then
      json_err BAD_ARG "games install is interactive" "Run it from the launcher's install terminal or a real terminal (no --json)."
      exit 1
    fi
    trow="$(_title_row "$gid")"
    if [[ -z "$trow" ]]; then
      echo "[dml] ERROR: unknown title: $gid" >&2; exit 1
    fi
    if _title_installed "$gid"; then
      echo "[dml] ERROR: $gid is already installed" >&2; exit 1
    fi
    tscript="$(printf '%s' "$trow" | cut -d'|' -f3)"
    tkind="$(printf '%s' "$trow" | cut -d'|' -f4)"
    tfile="$(_installers_dir)/$tscript"
    if [[ ! -f "$tfile" ]]; then
      echo "[dml] ERROR: installer script not found: $tfile (re-run cli/dev-install.ps1)" >&2; exit 1
    fi
    rc=0
    bash "$tfile" 2>&1 || rc=$?
    if [[ $rc -eq 0 && "$tkind" == home && -d "$HOME/$gid" ]]; then
      mkdir -p "$GAMES_DIR"
      ln -sfn "$HOME/$gid" "$GAMES_DIR/$gid"
      echo "[dml] linked $GAMES_DIR/$gid -> $HOME/$gid"
    fi
    exit "$rc"
    ;;
  remove)
    gid="${1:-}"; shift || true
    confirm=0
    [[ "${1:-}" == "--yes" ]] && confirm=1
    [[ "$DML_JSON" == 1 ]] && ndjson_section_start games-remove
    trow="$(_title_row "$gid")"
    if [[ -z "$trow" ]]; then
      ndjson_section_end games-remove error
      ndjson_error BAD_ARG "Unknown title: $gid" ""; exit 1
    fi
    tkind="$(printf '%s' "$trow" | cut -d'|' -f4)"
    tlauncher="$(printf '%s' "$trow" | cut -d'|' -f5)"
    if ! _title_installed "$gid"; then
      ndjson_section_end games-remove error
      ndjson_error NOT_FOUND "$gid is not installed" ""; exit 1
    fi
    targets=""
    [[ -e "$GAMES_DIR/$gid" || -L "$GAMES_DIR/$gid" ]] && targets+="$GAMES_DIR/$gid "
    [[ -d "$HOME/$gid" && ! -L "$HOME/$gid" ]] && targets+="$HOME/$gid "
    [[ -n "$tlauncher" && -e "$HOME/$tlauncher" ]] && targets+="$HOME/$tlauncher"
    if [[ "$confirm" != 1 ]]; then
      ndjson_section_end games-remove error
      ndjson_error CONFIRM_REQUIRED "Removing $gid deletes: $targets" "Re-run with --yes. Backups under ~/.dml are kept."
      exit 1
    fi
    tdir="$GAMES_DIR/$gid"; [[ -d "$tdir" ]] || tdir="$HOME/$gid"
    tcompose="$(_resolve_compose_dir "$tdir/")"
    if [[ -n "$tcompose" ]]; then
      ndjson_line info "stopping $gid..."
      (cd "$tcompose" && docker compose down >/dev/null 2>&1) || true
    fi
    if [[ -L "$GAMES_DIR/$gid" ]]; then
      ttarget="$(readlink -f "$GAMES_DIR/$gid" 2>/dev/null || true)"
      rm -f "$GAMES_DIR/$gid"
      [[ -n "$ttarget" && -d "$ttarget" ]] && rm -rf "$ttarget"
    elif [[ -d "$GAMES_DIR/$gid" ]]; then
      rm -rf "$GAMES_DIR/$gid"
    fi
    [[ -d "$HOME/$gid" ]] && rm -rf "$HOME/$gid"
    [[ -n "$tlauncher" ]] && rm -f "$HOME/$tlauncher"
    ndjson_line info "removed (backups under ~/.dml are kept)"
    ndjson_section_end games-remove ok
    ndjson_done "{\"id\":\"$(json_escape "$gid")\",\"removed\":true}"
    ;;
```

- [ ] **Step 5: `cli/dev-install.ps1`** — extend the single wsl bash line: after the existing lua installs, add `&& mkdir -p /usr/local/share/dml/installers && install -m 0755 $repoWsl/guides/wow-wotlk/install-wow-wotlk.sh $repoWsl/guides/wow-vanilla/install-wow-vanilla.sh $repoWsl/guides/wow-tbc/install-wow-tbc.sh $repoWsl/guides/Maplestory/install-maplestory.sh $repoWsl/guides/runescape/install-runescape.sh $repoWsl/guides/Mu-online/install-muonline.sh /usr/local/share/dml/installers/` (before `&& dml version`).

- [ ] **Step 6: rebuild; run file (9/9) + full — expect 329 (320 + 9). Step 7: commit** `feat(cli): games catalog/install/remove — title registry + interactive install passthrough`.

---

### Task 2: Rust interactive runner + commands + api.ts

**Files:** `launcher/src-tauri/src/dml/runner.rs` (+ new test + fixture `launcher/src-tauri/tests/fixtures/interactive_echo.cmd`), `launcher/src-tauri/src/lib.rs`, `launcher/src/lib/api.ts`.

**Interfaces produced:** runner `command_raw(&self, args) -> Command` (prefix + args, NO `--json`) and `spawn_interactive(&self, args) -> Result<std::process::Child, RunnerError>` (command_raw + stdin/stdout piped, stderr null, CREATE_NO_WINDOW); Tauri commands `games_catalog`, `games_install(id, on_event)`, `games_install_input(text)`, `games_install_cancel()`, `games_remove(id, on_event)`; api.ts `TitleInfo`, `gamesCatalog()`, `gamesInstall(id, onEvent)` (events `{event:"chunk",text}` / `{event:"exit",code}`), `gamesInstallInput(text)`, `gamesInstallCancel()`, `gamesRemove(id, onEvent)` (TermEvent stream; always sends `--yes`).

- [ ] **Step 1: runner.rs** — add beside `command`:

```rust
    fn command_raw(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.prefix_args).args(args);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        cmd
    }

    /// Interactive spawn for `games install`: raw text passthrough (no --json),
    /// stdin piped so the UI can answer installer prompts, stderr already
    /// merged by the CLI arm (2>&1).
    pub fn spawn_interactive(&self, args: &[&str]) -> Result<std::process::Child, RunnerError> {
        self.command_raw(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| RunnerError::Spawn(e.to_string()))
    }
```

- [ ] **Step 2: fixture `interactive_echo.cmd`:**

```bat
@echo off
<nul set /p=answer me: 
set /p REPLY=
echo you typed %REPLY%
exit /b 0
```

and a runner test following the existing fixture-test pattern (construct `DmlRunner { program: fixture path, prefix_args: vec![] }`):

```rust
    #[test]
    fn spawn_interactive_round_trips_stdin() {
        use std::io::{Read, Write};
        let r = fixture_runner();
        let mut child = r.spawn_interactive(&[&fixture("interactive_echo.cmd")]).unwrap();
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(b"hello\r\n").unwrap();
        drop(stdin);
        let mut out = String::new();
        child.stdout.take().unwrap().read_to_string(&mut out).unwrap();
        let status = child.wait().unwrap();
        assert!(out.contains("answer me:"));
        assert!(out.contains("you typed hello"));
        assert_eq!(status.code(), Some(0));
    }
```

(Adapt `fixture_runner`/`fixture` helper names to whatever the existing tests in the file actually use — read them first.)

- [ ] **Step 3: lib.rs** — extend `AppState` with `install: std::sync::Mutex<Option<InstallSession>>` where

```rust
pub struct InstallSession {
    pub stdin: std::process::ChildStdin,
    pub pid: u32,
}
```

(update the `.manage(AppState { … })` construction with `install: std::sync::Mutex::new(None)`), then the commands:

```rust
#[tauri::command]
async fn games_catalog(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["games".into(), "catalog".into()]).await
}

#[tauri::command]
async fn games_install(
    id: String,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    let runner = state.runner.clone();
    {
        let guard = state.install.lock().unwrap();
        if guard.is_some() {
            return Err(CmdError {
                code: "BUSY".into(),
                message: "An install is already running".into(),
                hint: "Finish or cancel it first.".into(),
            });
        }
    }
    let state_arc = state.install_arc();
    tauri::async_runtime::spawn_blocking(move || {
        use std::io::Read;
        let mut child = match runner.spawn_interactive(&["games", "install", &id]) {
            Ok(c) => c,
            Err(e) => {
                let _ = on_event.send(serde_json::json!({"event":"chunk","text": format!("failed to start: {e}\n")}));
                let _ = on_event.send(serde_json::json!({"event":"exit","code": -1}));
                return;
            }
        };
        let stdin = child.stdin.take().expect("stdin piped");
        let pid = child.id();
        *state_arc.lock().unwrap() = Some(InstallSession { stdin, pid });
        let mut stdout = child.stdout.take().expect("stdout piped");
        let mut buf = [0u8; 4096];
        loop {
            match stdout.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let text = crate::dml::envelope::decode_wsl_output(&buf[..n]);
                    let _ = on_event.send(serde_json::json!({"event":"chunk","text": text}));
                }
                Err(_) => break,
            }
        }
        let code = child.wait().ok().and_then(|s| s.code()).unwrap_or(-1);
        *state_arc.lock().unwrap() = None;
        let _ = on_event.send(serde_json::json!({"event":"exit","code": code}));
    })
    .await
    .map_err(|e| CmdError { code: "IPC".into(), message: e.to_string(), hint: String::new() })?;
    Ok(())
}

#[tauri::command]
async fn games_install_input(text: String, state: State<'_, AppState>) -> Result<(), CmdError> {
    use std::io::Write;
    let mut guard = state.install.lock().unwrap();
    match guard.as_mut() {
        Some(sess) => sess
            .stdin
            .write_all(format!("{text}\n").as_bytes())
            .map_err(|e| CmdError { code: "STDIN".into(), message: e.to_string(), hint: String::new() }),
        None => Err(CmdError {
            code: "NO_SESSION".into(),
            message: "No install is running".into(),
            hint: String::new(),
        }),
    }
}

#[tauri::command]
async fn games_install_cancel(state: State<'_, AppState>) -> Result<(), CmdError> {
    let pid = {
        let guard = state.install.lock().unwrap();
        match guard.as_ref() {
            Some(s) => s.pid,
            None => {
                return Err(CmdError {
                    code: "NO_SESSION".into(),
                    message: "No install is running".into(),
                    hint: String::new(),
                })
            }
        }
    };
    let mut cmd = std::process::Command::new("taskkill");
    cmd.args(["/F", "/T", "/PID", &pid.to_string()]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd.output()
        .map_err(|e| CmdError { code: "KILL".into(), message: e.to_string(), hint: String::new() })?;
    Ok(())
}

#[tauri::command]
async fn games_remove(
    id: String,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    stream_args(vec!["games".into(), "remove".into(), id, "--yes".into()], on_event, state).await
}
```

Implementation notes: `install_arc` — the simplest form is to make `AppState.install` an `Arc<Mutex<Option<InstallSession>>>` (field type `Arc<…>`) so `state.install.clone()` is the arc (adjust the BUSY check + input/cancel accordingly; add `install_arc()` only if you prefer a helper). `decode_wsl_output` visibility: it lives in `dml::envelope` — make sure it's `pub` (it is used by runner.rs already; if it's `pub(crate)` adjust the call path accordingly). Register all five commands.

- [ ] **Step 4: `cargo test`** — expect 18 (17 + 1 new; adjust if the file grows more).

- [ ] **Step 5: api.ts additions** (after the console/module wrappers):

```ts
export interface TitleInfo {
  id: string;
  name: string;
  installed: boolean;
  running: "running" | "stopped" | null;
  script_available: boolean;
}
export async function gamesCatalog(): Promise<TitleInfo[]> {
  const d = await invoke<{ titles: TitleInfo[] }>("games_catalog");
  return d.titles;
}
export interface InstallEvent {
  event: "chunk" | "exit";
  text?: string;
  code?: number;
}
export const gamesInstall = (id: string, onEvent: (e: InstallEvent) => void): Promise<void> => {
  const ch = new Channel<InstallEvent>();
  ch.onmessage = onEvent;
  return invoke("games_install", { id, onEvent: ch });
};
export async function gamesInstallInput(text: string): Promise<void> {
  return await invoke("games_install_input", { text });
}
export async function gamesInstallCancel(): Promise<void> {
  return await invoke("games_install_cancel");
}
export const gamesRemove = (id: string, onEvent: (e: TermEvent) => void): Promise<void> => {
  const ch = new Channel<TermEvent>();
  ch.onmessage = onEvent;
  return invoke("games_remove", { id, onEvent: ch });
};
```

- [ ] **Step 6: `npm test` (20) + `npm run check` (0/0). Step 7: commit** `feat(launcher): interactive install runner + games catalog/install/remove commands`.

---

### Task 3: Library page + InstallTerminal

**Files:** `launcher/src/lib/pages/Library.svelte` (extend), create `launcher/src/lib/InstallTerminal.svelte`.

**Binding requirements** (read Library.svelte + Console.svelte + ModuleManager.svelte first — reuse their patterns):
- Library loses the "(Install flows arrive in a later release.)" placeholder. Two sections: **Installed** (existing cards, from `gamesCatalog` rows where installed — keep start/stop via the EXISTING gamesList-driven logic OR switch the whole page to catalog rows with `running` driving start/stop; implementer's choice, but one data source, no double-fetch) each gaining a `Remove` button; **Available titles** — catalog rows where `!installed`: name + `Install` button (disabled when `!script_available` with title `Re-run cli/dev-install.ps1 to ship installer scripts`).
- Remove: two-step typed confirm inline on the card — text `Removing deletes the server and its data. Backups under ~/.dml are kept. Type the title id to confirm:` + input + Remove button enabled only when the input equals the id exactly → streams `gamesRemove` into the existing Terminal with the sawDone/streamErr contract (outcome applied after a catalog refresh).
- Install: opens the **InstallTerminal** panel (one at a time — hide Install buttons while a session runs): props `{ id, onExit(code) }`; internally: calls `gamesInstall(id, cb)`; scrollback `<div>` of accumulated chunk text with ANSI escape sequences stripped (`text.replace(/\x1b\[[0-9;?]*[A-Za-z]/g, "")`), sticky autoscroll (Console's 40px pattern); input row (text input + Send, Enter submits) → `gamesInstallInput`, input+Send disabled after the exit event; `Cancel install` button with its own two-step confirm copy exactly `Cancelling mid-install can leave a partial install behind. Cancel anyway?` → `gamesInstallCancel()`; on exit event: note `Installer finished (exit 0).` or `Installer failed (exit <code>).` and the parent refreshes the catalog.
- All fetch errors inline (error-card style); busy discipline: install session OR a remove stream blocks the other actions.
- Gates: `npm test` (20) + `npm run check` (0/0). Commit `feat(launcher): Library — install terminal + title catalog + typed remove`.
