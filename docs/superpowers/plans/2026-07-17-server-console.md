# Server Console Implementation Plan (Round B)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Console page — polled read-only worldserver log tail + a SOAP command box — via two new CLI verbs (`wow console-tail`, `wow console-send`).

**Architecture:** `console-tail` wraps `docker logs --tail N ac-worldserver`, strips ANSI/CR, returns a JSON lines array (docker down → `available:false`, exit 0). `console-send` mirrors the existing `soap-exec` arm's rc handling but decodes XML entities in the reply. The Svelte page polls tail every 3 s (sticky auto-scroll) and appends each sent command + reply to a session history.

**Tech Stack:** bash (cli/src, built by `cli/build.sh`), bats + stubs, Tauri 2 Rust, Svelte 5 runes, TypeScript.

## Global Constraints

- Everything stays on `feat/dml-launcher-windows`; NO merge.
- `cli/dml` is a committed build artifact: NEVER hand-edit; regenerate with `bash cli/build.sh` and commit it with the source.
- `set -euo pipefail` active in the built CLI: guard every fallible substitution; helpers use `local` + `return 0` (filter functions whose body is a single pipeline are exempt, documented); NO `local` in the top-level dispatch case.
- `console-tail` is read-only, request-response `json_ok` (NO NDJSON). Docker down / container absent → `{"available":false,"lines":[]}`, exit 0 — down is an answer. `--lines` validated `^[0-9]+$`, normalized `$((10#$lines))` (octal-bypass rule), range 1–1000, default 200; violations → `BAD_ARG`, exit 1.
- `console-send` sends free text to SOAP **by design** (the manual GM console — same capability as the existing public `wow soap-exec` verb; the closed-allowlist rule binds canned/automated UI actions, not the operator console). The command reaches SOAP only via `soap_exec` (XML-escaped, flock-serialized); nothing is shell-interpolated. Empty/whitespace `--command` → `BAD_ARG`. rc mapping identical to `soap-exec`: 0 → `json_ok {"result":...}`, 2 → `SOAP_FAULT` (fault text as message), 3 → `SOAP_AUTH`, 4 → `SOAP_UNREACHABLE` — but result/fault text is entity-decoded (`&#xD;` removed; `&lt; &gt; &quot; &amp;` → literals, `&amp;` LAST). The existing `soap-exec` verb stays UNTOUCHED.
- Bash `${var//pat/repl}` treats a bare `&` in the replacement as a backreference (bash 5.2 patsub_replacement) — every literal `&` in a replacement must be `\&`.
- UI: Auto-refresh default ON, 3 s, tick skipped while a fetch or send is in flight, interval cleaned up via `$effect` teardown. Sticky auto-scroll: scroll to bottom after update only if previously within 40 px of the bottom. Send failures render inline in the history (never a blocking error card). After every send (success or failure) the tail refreshes once. No confirmation dialogs.
- UI copy (exact): unavailable state → "No server logs — is the server installed?"; input placeholder → "Console command, e.g. server info".
- Nav: `{ id: "console", label: "Console" }` appended to the **Server** section (after Library); `nav.test.ts` pins updated in the same task.
- Gates: full bats (`wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bash build.sh && bats tests/"`; DrvFs flake → re-run once), `npm test`, `npm run check`, `cd src-tauri; cargo test`. Baselines entering this round: bats 245, vitest 19, cargo 17, svelte-check 0/0.
- Commits end with `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`.

---

## File Map

- Create: `cli/src/45-console.sh` — `_strip_ansi`, `_soap_text_decode`, `_console_lines_json` (Task 1)
- Modify: `cli/src/90-main.sh` — `console-tail)` + `console-send)` arms after `server-detail)` (Task 1)
- Modify: `cli/tests/helpers/env.bash` — logs stub arm gains `DML_STUB_LOGS_ARGS_LOG` (Task 1)
- Create: `cli/tests/wow-console.bats` — 11 tests (Task 1)
- Modify: `launcher/src-tauri/src/lib.rs` — `wow_console_tail` + `wow_console_send` (Task 2)
- Modify: `launcher/src/lib/api.ts` — `ConsoleTail`, `wowConsoleTail`, `wowConsoleSend` (Task 2)
- Create: `launcher/src/lib/pages/Console.svelte` (Task 3)
- Modify: `launcher/src/lib/nav.ts`, `launcher/src/lib/nav.test.ts`, `launcher/src/routes/+page.svelte` (Task 3)

---

### Task 1: CLI console verbs + bats

**Files:**
- Create: `cli/src/45-console.sh`
- Modify: `cli/src/90-main.sh` (insert both arms immediately after the `server-detail)` arm's closing `;;`)
- Modify: `cli/tests/helpers/env.bash` (one line added to the `logs` stub arm)
- Create: `cli/tests/wow-console.bats`
- Commit also: regenerated `cli/dml`

**Interfaces:**
- Consumes: `soap_exec` (rc 0/2/3/4), `json_ok`/`json_err`/`json_escape`, `_need_flag_val`, curl-stub seams (`DML_STUB_SOAP_RESPONSE`/`DML_STUB_CAPTURE`/`DML_STUB_CURL_EXIT`), docker-stub logs seams (`DML_STUB_LOGS_FILE`, `DML_STUB_DOCKER_DOWN`).
- Produces: `dml wow console-tail [--lines N] --json` → `{"ok":true,"data":{"available":bool,"lines":[string...]}}`; `dml wow console-send --command "<text>" --json` → `{"ok":true,"data":{"result":"<decoded>"}}` or `SOAP_FAULT`/`SOAP_AUTH`/`SOAP_UNREACHABLE`/`BAD_ARG` error envelopes; stub seam `DML_STUB_LOGS_ARGS_LOG` (file capturing each `docker logs` argv line).

- [ ] **Step 1: Add the args-log seam to the logs stub arm in `cli/tests/helpers/env.bash`**

Inside `use_docker_stub`'s heredoc, in the `if [[ "${1:-}" == "logs" ]]; then` arm, insert directly after its `[[ "${DML_STUB_DOCKER_DOWN:-0}" == 1 ]] && exit 1` line:

```bash
  [[ -n "${DML_STUB_LOGS_ARGS_LOG:-}" ]] && printf '%s\n' "$*" >> "$DML_STUB_LOGS_ARGS_LOG"
```

- [ ] **Step 2: Write the failing bats suite `cli/tests/wow-console.bats`**

```bash
#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_docker_stub
  use_curl_stub
  export HOME="$FIXTURE"
}
teardown() { teardown_fixture; }

@test "console-tail: default asks docker for --tail 200" {
  printf 'line one\nline two\n' > "$FIXTURE/log.txt"
  export DML_STUB_LOGS_FILE="$FIXTURE/log.txt"
  export DML_STUB_LOGS_ARGS_LOG="$FIXTURE/args.log"
  run bash "$DML" wow console-tail --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.ok')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.available')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.lines | length')" = "2" ]
  [ "$(echo "$output" | jq -r '.data.lines[1]')" = "line two" ]
  grep -q -- '--tail 200' "$FIXTURE/args.log"
}

@test "console-tail: --lines 50 passes --tail 50" {
  printf 'x\n' > "$FIXTURE/log.txt"
  export DML_STUB_LOGS_FILE="$FIXTURE/log.txt"
  export DML_STUB_LOGS_ARGS_LOG="$FIXTURE/args.log"
  run bash "$DML" wow console-tail --lines 50 --json
  [ "$status" -eq 0 ]
  grep -q -- '--tail 50' "$FIXTURE/args.log"
}

@test "console-tail: leading-zero --lines normalizes to base-10" {
  printf 'x\n' > "$FIXTURE/log.txt"
  export DML_STUB_LOGS_FILE="$FIXTURE/log.txt"
  export DML_STUB_LOGS_ARGS_LOG="$FIXTURE/args.log"
  run bash "$DML" wow console-tail --lines 050 --json
  [ "$status" -eq 0 ]
  grep -q -- '--tail 50' "$FIXTURE/args.log"
}

@test "console-tail: bad --lines values are BAD_ARG" {
  for v in 0 1001 abc; do
    run bash "$DML" wow console-tail --lines "$v" --json
    [ "$status" -eq 1 ]
    [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  done
}

@test "console-tail: ANSI escapes and CRs are stripped" {
  printf '\033[0m\033[36mWORLD: World Initialized\033[0m\r\n\033[?2004hAC> hello\r\n' > "$FIXTURE/log.txt"
  export DML_STUB_LOGS_FILE="$FIXTURE/log.txt"
  run bash "$DML" wow console-tail --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.lines[0]')" = "WORLD: World Initialized" ]
  [ "$(echo "$output" | jq -r '.data.lines[1]')" = "AC> hello" ]
}

@test "console-tail: docker down -> available:false, exit 0" {
  export DML_STUB_DOCKER_DOWN=1
  run bash "$DML" wow console-tail --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.available')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.lines | length')" = "0" ]
}

@test "console-send: command text reaches the SOAP body" {
  cat > "$FIXTURE/resp.xml" <<'EOF'
<?xml version="1.0"?><SOAP-ENV:Envelope><SOAP-ENV:Body><ns1:executeCommandResponse><result>ok</result></ns1:executeCommandResponse></SOAP-ENV:Body></SOAP-ENV:Envelope>
EOF
  export DML_STUB_SOAP_RESPONSE="$FIXTURE/resp.xml"
  export DML_STUB_CAPTURE="$FIXTURE/sent.xml"
  run bash "$DML" wow console-send --command "server info" --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.result')" = "ok" ]
  grep -q 'server info' "$FIXTURE/sent.xml"
}

@test "console-send: XML entities in the result are decoded" {
  cat > "$FIXTURE/resp.xml" <<'EOF'
<?xml version="1.0"?><SOAP-ENV:Envelope><SOAP-ENV:Body><ns1:executeCommandResponse><result>a &lt;b&gt; &quot;c&quot; &amp;d&#xD;
next</result></ns1:executeCommandResponse></SOAP-ENV:Body></SOAP-ENV:Envelope>
EOF
  export DML_STUB_SOAP_RESPONSE="$FIXTURE/resp.xml"
  run bash "$DML" wow console-send --command "x" --json
  [ "$status" -eq 0 ]
  result="$(echo "$output" | jq -r '.data.result')"
  [[ "$result" == *'a <b> "c" &d'* ]]
  [[ "$result" == *'next'* ]]
}

@test "console-send: empty command is BAD_ARG" {
  run bash "$DML" wow console-send --command "   " --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}

@test "console-send: fault -> SOAP_FAULT" {
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-fault.xml"
  run bash "$DML" wow console-send --command "bogus" --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "SOAP_FAULT" ]
}

@test "console-send: unreachable -> SOAP_UNREACHABLE" {
  printf 'x' > "$FIXTURE/resp.xml"
  export DML_STUB_SOAP_RESPONSE="$FIXTURE/resp.xml"
  export DML_STUB_CURL_EXIT=7
  run bash "$DML" wow console-send --command "server info" --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "SOAP_UNREACHABLE" ]
}
```

- [ ] **Step 3: Run the new suite to verify it fails**

Run: `wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bash build.sh && bats tests/wow-console.bats"`
Expected: FAIL — unknown subcommand for both verbs.

- [ ] **Step 4: Create `cli/src/45-console.sh`**

```bash
# ---------------------------------------------------------------------------
# Server console helpers: log-tail sanitizing + SOAP reply decoding.
# All read-only text filters -- no docker/SOAP calls live here.
# ---------------------------------------------------------------------------

# Filter: strips ANSI CSI escape sequences (color codes, ESC[?2004h
# bracketed-paste noise) and carriage returns from stdin. The $'…' quoting
# embeds a real ESC byte so this works on any sed, not just ones that
# understand \x1b in patterns.
_strip_ansi() {
    sed -E $'s/\x1b\\[[0-9;?]*[a-zA-Z]//g' | tr -d '\r'
}

# Decodes the XML entities soap_parse_result leaves in <result> text so the
# console shows real characters. &amp; is decoded LAST so "&amp;lt;" cannot
# double-decode. NB: a bare & in a ${var//pat/repl} replacement is a
# backreference (bash 5.2 patsub_replacement) -- hence the \&.
_soap_text_decode() {
    local s="${1-}"
    s=${s//&#xD;/}
    s=${s//&lt;/<}
    s=${s//&gt;/>}
    s=${s//&quot;/\"}
    s=${s//&amp;/\&}
    printf '%s' "$s"
    return 0
}

# stdin: sanitized log lines. stdout: a JSON array of strings.
_console_lines_json() {
    local line out="" first=1
    while IFS= read -r line; do
        if [[ $first == 1 ]]; then first=0; else out="$out,"; fi
        out="$out\"$(json_escape "$line")\""
    done
    printf '[%s]' "$out"
    return 0
}
```

- [ ] **Step 5: Add both arms in `cli/src/90-main.sh`, directly after the `server-detail)` arm's closing `;;`**

```bash
      console-tail)
        # Read-only worldserver log tail for the Console page. Down is an
        # answer: docker/container unavailable -> available:false, exit 0.
        lines=200
        while [[ $# -gt 0 ]]; do
          case "$1" in
            --lines) _need_flag_val "$1" $#; lines="$2"; shift 2 ;;
            *) json_err BAD_ARG "Unknown flag: $1" "Usage: dml wow console-tail [--lines N] --json"; exit 1 ;;
          esac
        done
        if [[ ! "$lines" =~ ^[0-9]+$ ]]; then
          json_err BAD_ARG "--lines must be a number" "Usage: dml wow console-tail [--lines N] --json"; exit 1
        fi
        lines=$((10#$lines))
        if (( lines < 1 || lines > 1000 )); then
          json_err BAD_ARG "--lines must be 1-1000" "Usage: dml wow console-tail [--lines N] --json"; exit 1
        fi
        if raw="$(docker logs --tail "$lines" ac-worldserver 2>&1)"; then
          if [[ -n "$raw" ]]; then
            arr="$(printf '%s\n' "$raw" | _strip_ansi | _console_lines_json)"
          else
            arr="[]"
          fi
          json_ok "{\"available\":true,\"lines\":$arr}"
        else
          json_ok '{"available":false,"lines":[]}'
        fi
        ;;
      console-send)
        # The manual GM console: free text is DELIBERATE here (same
        # capability as the public `wow soap-exec`; the closed-allowlist
        # rule binds canned/automated actions, not the operator console).
        # The text reaches SOAP only via soap_exec's XML escaping.
        cmd=""
        while [[ $# -gt 0 ]]; do
          case "$1" in
            --command) _need_flag_val "$1" $#; cmd="$2"; shift 2 ;;
            *) json_err BAD_ARG "Unknown flag: $1" "Usage: dml wow console-send --command \"<text>\" --json"; exit 1 ;;
          esac
        done
        if [[ -z "${cmd//[[:space:]]/}" ]]; then
          json_err BAD_ARG "console-send requires a non-empty --command" "Example: dml wow console-send --command \"server info\" --json"; exit 1
        fi
        if out="$(soap_exec "$cmd")"; then rc=0; else rc=$?; fi
        case "$rc" in
          0) json_ok "{\"result\":\"$(json_escape "$(_soap_text_decode "$out")")\"}" ;;
          2) json_err SOAP_FAULT "$(_soap_text_decode "$out")" "The worldserver rejected the command." ; exit 1 ;;
          3) json_err SOAP_AUTH "SOAP authentication failed" "Check ~/.dml/soap.env" ; exit 1 ;;
          *) json_err SOAP_UNREACHABLE "Could not reach SOAP at $(soap_url)" "Is the worldserver running with SOAP enabled? Run: dml wow soap-setup" ; exit 1 ;;
        esac
        ;;
```

- [ ] **Step 6: Rebuild + run the new suite**

Run: `wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bash build.sh && bats tests/wow-console.bats"`
Expected: 11/11 PASS.

- [ ] **Step 7: Run the FULL bats suite**

Run: `wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bash build.sh && bats tests/"`
Expected: 256 tests, 0 failures (245 baseline + 11 new).

- [ ] **Step 8: Commit**

```bash
git add cli/src/45-console.sh cli/src/90-main.sh cli/tests/helpers/env.bash cli/tests/wow-console.bats cli/dml
git commit -m "feat(cli): wow console-tail + console-send

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 2: Rust commands + api.ts

**Files:**
- Modify: `launcher/src-tauri/src/lib.rs` (two commands after `wow_server_detail`; both registered after `wow_server_detail,` in `generate_handler![...]`)
- Modify: `launcher/src/lib/api.ts` (type + two wrappers after `wowServerDetail`)

**Interfaces:**
- Consumes: `run_json_cmd`; Task 1's CLI verbs.
- Produces (Task 3 imports these exact names): `ConsoleTail { available: boolean; lines: string[] }`, `wowConsoleTail(lines?: number): Promise<ConsoleTail>`, `wowConsoleSend(command: string): Promise<{ result: string }>`.

- [ ] **Step 1: Rust commands in `lib.rs`** (after `wow_server_detail`)

```rust
#[tauri::command]
async fn wow_console_tail(
    lines: Option<u32>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    let mut args: Vec<String> = vec!["wow".into(), "console-tail".into()];
    if let Some(l) = lines {
        args.extend(["--lines".into(), l.to_string()]);
    }
    run_json_cmd(state, args).await
}

#[tauri::command]
async fn wow_console_send(
    command: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "console-send".into(), "--command".into(), command]).await
}
```

Add `wow_console_tail,` and `wow_console_send,` to `generate_handler![...]` immediately after `wow_server_detail,`.

- [ ] **Step 2: `cargo test`** (from launcher/src-tauri) — expect 17 passed.

- [ ] **Step 3: api.ts additions** (after `wowServerDetail`)

```ts
export interface ConsoleTail {
  available: boolean;
  lines: string[];
}
export async function wowConsoleTail(lines?: number): Promise<ConsoleTail> {
  return await invoke("wow_console_tail", { lines });
}
export async function wowConsoleSend(command: string): Promise<{ result: string }> {
  return await invoke("wow_console_send", { command });
}
```

- [ ] **Step 4: Gates** — from launcher/: `npm test` (19 passed) and `npm run check` (0/0).

- [ ] **Step 5: Commit**

```bash
git add launcher/src-tauri/src/lib.rs launcher/src/lib/api.ts
git commit -m "feat(launcher): console tail/send commands + api wrappers

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 3: Console page + nav

**Files:**
- Create: `launcher/src/lib/pages/Console.svelte`
- Modify: `launcher/src/lib/nav.ts` (Server section gains `{ id: "console", label: "Console" }` after Library)
- Modify: `launcher/src/lib/nav.test.ts` (ids pin gains `"console"` after `"library"`)
- Modify: `launcher/src/routes/+page.svelte` (import + mount)

**Interfaces:**
- Consumes: `wowConsoleTail`, `wowConsoleSend`, `ConsoleTail` (Task 2).
- Produces: nothing.

- [ ] **Step 1: nav.ts** — Server section becomes:

```ts
  {
    section: "Server",
    pages: [
      { id: "home", label: "Home" },
      { id: "library", label: "Library" },
      { id: "console", label: "Console" },
    ],
  },
```

- [ ] **Step 2: nav.test.ts** — the ids pin becomes:

```ts
    expect(ids).toEqual([
      "home",
      "library",
      "console",
      "dashboard",
      "teleport",
      "gmtools",
      "items",
      "playerbots",
      "settings",
      "modules",
      "backups",
    ]);
```

- [ ] **Step 3: Run `npm test` — the nav pin test must FAIL before nav.ts is edited if done test-first; after both edits expect 19 passed.**

- [ ] **Step 4: `+page.svelte`** — add `import Console from "$lib/pages/Console.svelte";` with the other page imports, and after the library mount line add:

```svelte
  {#if page === "console"}<Console />{/if}
```

- [ ] **Step 5: Create `launcher/src/lib/pages/Console.svelte`**

```svelte
<script lang="ts">
  import { onMount, tick } from "svelte";
  import { wowConsoleTail, wowConsoleSend } from "$lib/api";

  interface HistoryEntry {
    command: string;
    result: string | null;
    error: string | null;
  }

  let available = $state(true);
  let lines: string[] = $state([]);
  let tailError: string | null = $state(null);
  let refreshing = $state(false);
  let auto = $state(true);

  let command = $state("");
  let sending = $state(false);
  let history: HistoryEntry[] = $state([]);

  let logEl: HTMLDivElement | undefined = $state();

  async function refreshLogs() {
    if (refreshing) return;
    refreshing = true;
    const nearBottom =
      !logEl || logEl.scrollHeight - logEl.scrollTop - logEl.clientHeight < 40;
    try {
      const t = await wowConsoleTail();
      available = t.available;
      lines = t.lines;
      tailError = null;
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      tailError = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    } finally {
      refreshing = false;
    }
    if (nearBottom) {
      await tick();
      if (logEl) logEl.scrollTop = logEl.scrollHeight;
    }
  }
  onMount(refreshLogs);

  $effect(() => {
    if (!auto) return;
    const t = setInterval(() => {
      if (!refreshing && !sending) refreshLogs();
    }, 3000);
    return () => clearInterval(t);
  });

  async function send() {
    const cmd = command.trim();
    if (!cmd || sending) return;
    sending = true;
    try {
      const r = await wowConsoleSend(cmd);
      history = [...history, { command: cmd, result: r.result, error: null }];
      command = "";
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      history = [
        ...history,
        {
          command: cmd,
          result: null,
          error: `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`,
        },
      ];
    } finally {
      sending = false;
      await refreshLogs();
    }
  }
</script>

<section class="content">
  <header class="bar">
    <h2>Console</h2>
    <div class="controls">
      <label class="autolabel">
        <input type="checkbox" bind:checked={auto} /> Auto-refresh
      </label>
      <button onclick={refreshLogs} disabled={refreshing}>Refresh</button>
    </div>
  </header>

  {#if tailError}
    <div class="error-card"><strong>Couldn't read the server log.</strong><p>{tailError}</p></div>
  {:else if !available}
    <p class="muted">No server logs — is the server installed?</p>
  {:else}
    <div class="log" bind:this={logEl}>
      {#each lines as line, i (i)}
        <div class="logline">{line}</div>
      {/each}
    </div>
  {/if}

  <form
    class="sendrow"
    onsubmit={(e) => {
      e.preventDefault();
      send();
    }}
  >
    <input
      type="text"
      placeholder="Console command, e.g. server info"
      bind:value={command}
      disabled={sending}
    />
    <button class="primary" type="submit" disabled={sending || command.trim() === ""}>Send</button>
  </form>

  {#if history.length > 0}
    <div class="history">
      {#each history as h, i (i)}
        <div class="entry">
          <div class="cmd">&gt; {h.command}</div>
          {#if h.error}
            <pre class="reply err">{h.error}</pre>
          {:else}
            <pre class="reply">{h.result}</pre>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</section>

<style>
  .content { padding: 20px 24px; overflow-y: auto; display: flex; flex-direction: column; gap: 14px; box-sizing: border-box; }
  .bar { display: flex; justify-content: space-between; align-items: center; }
  .bar h2 { margin: 0; font-size: 18px; }
  .controls { display: flex; gap: 10px; align-items: center; }
  .autolabel { color: #8b949e; font-size: 13px; display: flex; gap: 6px; align-items: center; }
  .log { background: #0d1117; border: 1px solid #30363d; border-radius: 8px; padding: 10px 12px; font-family: Consolas, monospace; font-size: 12.5px; line-height: 1.45; overflow-y: auto; min-height: 200px; max-height: 48vh; }
  .logline { white-space: pre-wrap; word-break: break-all; color: #c9d1d9; }
  .sendrow { display: flex; gap: 8px; }
  .sendrow input { flex: 1; background: #0d1117; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 8px 10px; font-family: Consolas, monospace; font-size: 13px; }
  .history { display: flex; flex-direction: column; gap: 10px; }
  .entry { border-left: 2px solid #30363d; padding-left: 10px; }
  .cmd { color: #58a6ff; font-family: Consolas, monospace; font-size: 13px; }
  .reply { margin: 4px 0 0; color: #c9d1d9; font-family: Consolas, monospace; font-size: 12.5px; white-space: pre-wrap; word-break: break-word; }
  .reply.err { color: #f85149; }
  button { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 14px; cursor: pointer; }
  button.primary { background: #238636; border-color: #2ea043; color: white; }
  button:disabled { opacity: 0.5; cursor: default; }
  .muted { color: #8b949e; margin: 0; }
  .error-card { background: #161b22; border: 1px solid #f85149; border-radius: 8px; padding: 12px 16px; }
</style>
```

- [ ] **Step 6: Gates** — from launcher/: `npm test` (19 passed, nav pin now green) and `npm run check` (0 errors 0 warnings).

- [ ] **Step 7: Commit**

```bash
git add launcher/src/lib/pages/Console.svelte launcher/src/lib/nav.ts launcher/src/lib/nav.test.ts launcher/src/routes/+page.svelte
git commit -m "feat(launcher): Console page — polled log tail + SOAP command box

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```
