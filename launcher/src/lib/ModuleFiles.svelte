<script lang="ts">
  // The Config page's "Module files" raw-editor view, extracted verbatim into
  // the tabbed Modules page (module-update round): file picker over every
  // conf the CLI reports (dynamic since Batch 1 F3), raw textarea editor with
  // the conf-lint save gate, reset-from-dist, and the ALE reload shortcut.
  // The component stays MOUNTED while the other tabs are shown (the page
  // switches with display:none), so an in-progress edit survives tab
  // switches -- `active` gates the lazy load the old in-Config `tab` check
  // gated.
  import {
    wowConfigFiles,
    wowConfigRawRead,
    wowConfigRawWrite,
    wowConfigRawReset,
    wowConsoleSend,
    gamesRestart,
    type ConfFile,
    type RawFileName,
  } from "$lib/api";
  import { lintConfContent } from "$lib/conf-lint";
  import { applyEvent } from "$lib/terminal-state";
  import { restartState, noteApplyNeeded, clearApplyNeeded } from "$lib/restart-state.svelte";
  import { bannerText } from "$lib/apply-needed";
  import Terminal from "$lib/Terminal.svelte";
  import { termBuf, beginRun, clearBuf } from "$lib/term-store.svelte";
  import { featureLocked, LOCKED_HINT } from "$lib/features.svelte";
  import { taskbarBusy, taskbarIdle } from "$lib/taskbar";

  let { active = false }: { active?: boolean } = $props();

  const WOW_ID = "wow-server-playerbots";
  // Static fallback shown until (or if) `wow config files` answers -- the
  // real list is dynamic since Batch 1 F3 (every installed module conf).
  const FALLBACK_FILES: ConfFile[] = [
    { name: ".env", exists: true, dist: false, readonly: true },
    { name: "docker-compose.override.yml", exists: true, dist: false, readonly: true },
    { name: "playerbots.conf", exists: true, dist: true, readonly: false },
  ];
  // UI mirror of the CLI's raw-write lock (cli rejects these two names).
  const READONLY_FILES: RawFileName[] = [".env", "docker-compose.override.yml"];

  let error: string | null = $state(null);
  let saving = $state(false);
  let aleNote: string | null = $state(null);

  let file: RawFileName = $state(".env");
  let fileContent = $state("");
  let fileLoaded = $state(false);
  let loadingFile = $state(false);
  let lastBackup: string | null = $state(null);
  let confFiles: ConfFile[] = $state(FALLBACK_FILES);
  let confFilesLoaded = $state(false);
  let confirmingReset = $state(false);
  let resetting = $state(false);
  let confirmingRestart = $state(false);
  let lintConfirm = $state(false);

  const buf = termBuf("config");

  async function loadConfFiles() {
    try {
      confFiles = await wowConfigFiles();
      confFilesLoaded = true;
      if (!confFiles.some((f) => f.name === file) && confFiles.length > 0) {
        file = confFiles[0].name;
      }
    } catch {
      // Keep the static fallback -- the picker still works for the basics.
    }
  }
  $effect(() => {
    if (active && !confFilesLoaded) void loadConfFiles();
  });
  // Switching tabs keeps this component mounted -- an armed "sure?"
  // confirmation or a one-shot note must not survive a leave-and-return
  // (mirrors the old in-Config tab-change reset).
  $effect(() => {
    void active;
    confirmingRestart = false;
    confirmingReset = false;
    lintConfirm = false;
    aleNote = null;
  });

  const currentFileMeta = $derived(confFiles.find((f) => f.name === file));
  const fileReadonly = $derived(
    confFiles.find((f) => f.name === file)?.readonly ?? READONLY_FILES.includes(file),
  );
  // Improvements Batch 3 F4: cheap "does this still look like a .conf?" check
  // shown live while editing an editable .conf, and gating Save with a one-off
  // "save anyway" confirm. Only .conf files use Key = Value syntax (the .env /
  // compose files are read-only), so scope the check to them.
  const lintIssues = $derived(
    fileLoaded && !fileReadonly && file.toLowerCase().endsWith(".conf")
      ? lintConfContent(fileContent)
      : [],
  );

  async function resetFile() {
    if (!confirmingReset) {
      confirmingReset = true;
      return;
    }
    confirmingReset = false;
    resetting = true;
    error = null;
    try {
      const target = file;
      const r = await wowConfigRawReset(target);
      // `recreate`, not `world-restart`. A raw rewrite is a bind-mounted conf
      // that the world re-reads at process start -- but unlike `config set`, this
      // route removes NO shadowing AC_* env key, and its allowlist includes
      // playerbots.conf, which is exactly where those keys live. Promising "the
      // fast world-only restart is enough" would be a promise we cannot keep.
      noteApplyNeeded("recreate");
      if (file === target) await loadFile();
      lastBackup = r.backup;
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    } finally {
      resetting = false;
    }
  }

  async function loadFile() {
    error = null;
    aleNote = null;
    fileLoaded = false;
    lastBackup = null;
    loadingFile = true;
    const target = file;
    try {
      const r = await wowConfigRawRead(target);
      if (file === target) {
        fileContent = r.content;
        fileLoaded = true;
      }
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      if (file === target) {
        error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
      }
    } finally {
      loadingFile = false;
    }
  }

  async function saveFile(): Promise<boolean> {
    saving = true;
    error = null;
    aleNote = null;
    try {
      const targetFile = file;
      const content = fileContent;
      const r = await wowConfigRawWrite(targetFile, content);
      lastBackup = r.backup;
      // See the raw-reset comment above: this route never un-shadows an AC_* env
      // key, so the weaker apply must not be advertised.
      noteApplyNeeded("recreate");
      return true;
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
      return false;
    } finally {
      saving = false;
    }
  }

  // Gate the raw-file Save behind the lint check: the first click on a file
  // with suspicious lines arms a "save anyway" confirm instead of writing.
  async function saveFileChecked(): Promise<boolean> {
    if (lintIssues.length > 0 && !lintConfirm) {
      lintConfirm = true;
      return false;
    }
    lintConfirm = false;
    return await saveFile();
  }

  function saveAndRestartFile(): void {
    // Arm the lint confirm together with the restart confirm on the FIRST
    // click, so the restart's second click isn't silently swallowed by an
    // unconfirmed lint gate (saveAndRestart would call saveFileChecked, which
    // would otherwise just arm the lint confirm and abort the restart).
    if (!confirmingRestart && lintIssues.length > 0) lintConfirm = true;
    void saveAndRestart(saveFileChecked);
  }

  async function reloadAle(): Promise<boolean> {
    saving = true;
    error = null;
    aleNote = null;
    try {
      const r = await wowConsoleSend("reload ale");
      aleNote = r.result.trim();
      return true;
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
      return false;
    } finally {
      saving = false;
    }
  }

  async function saveAndRestart(saveFn: () => Promise<boolean>) {
    if (!confirmingRestart) {
      confirmingRestart = true;
      return;
    }
    confirmingRestart = false;
    if (!(await saveFn())) return;
    restartState.restarting = true;
    beginRun("config");
    taskbarBusy();
    try {
      // Applying settings is a deliberate, infrequent restart -- always save
      // characters first (false = don't skip). The "faster restart" option
      // lives on Home for the routine restart button.
      // Derived from the terminal `done` event, never from the promise: a
      // streaming command resolves Ok even when the CLI exits non-zero (the
      // failure arrives as an `error` event), so clearing the banner here would
      // announce a restart that did not happen.
      await gamesRestart(WOW_ID, false, (e) => {
        buf.term = applyEvent(buf.term, e);
        if (e.event === "done") clearApplyNeeded();
      });
    } catch (e) {
      const err = e as { code?: string; message?: string; hint?: string };
      buf.term = applyEvent(buf.term, {
        event: "error",
        error: { code: err.code ?? "IPC", message: err.message ?? String(e), hint: err.hint ?? "" },
      });
    } finally {
      taskbarIdle();
      restartState.restarting = false;
    }
  }

  function onFileSelect() {
    // Changing which file is targeted must invalidate whatever was loaded/armed
    // for the previous file -- otherwise a stale `fileContent` could get written
    // to the newly selected `file` (or a stale restart confirmation could fire
    // for content the user never actually confirmed).
    fileLoaded = false;
    fileContent = "";
    lastBackup = null;
    confirmingRestart = false;
    confirmingReset = false;
    lintConfirm = false;
    aleNote = null;
  }
</script>

<section class="content" class:fill={fileLoaded}>
  <header class="bar">
    <h2>Module files</h2>
  </header>

  {#if error}<div class="error-card"><p>{error}</p></div>{/if}
  {#if restartState.needed}
    <div class="warn-card"><p>{bannerText(restartState.apply)}</p></div>
  {/if}

  <div class="row">
    <select bind:value={file} onchange={onFileSelect} disabled={saving || restartState.restarting || loadingFile}>
      {#each confFiles as f (f.name)}
        <option value={f.name}>{f.name}{!f.exists && f.dist ? " (new — starts from defaults)" : ""}</option>
      {/each}
    </select>
    <button onclick={loadFile} disabled={saving || restartState.restarting || loadingFile}>Open</button>
  </div>
  <p class="muted">
    Edited mod_ale.conf or its Lua scripts on disk?
    <button
      onclick={reloadAle}
      disabled={saving || restartState.restarting || loadingFile || featureLocked("ale-reload")}
      title={featureLocked("ale-reload") ? LOCKED_HINT : undefined}
    >
      Reload ALE scripts
    </button>
  </p>
  {#if aleNote}<p class="muted">{aleNote}</p>{/if}
  {#if fileLoaded}
    <textarea
      rows="18"
      spellcheck="false"
      bind:value={fileContent}
      oninput={() => {
        confirmingRestart = false;
        confirmingReset = false;
        lintConfirm = false;
      }}
      readonly={fileReadonly}
      disabled={saving || restartState.restarting}
    ></textarea>
    {#if fileReadonly}
      <p class="muted">Read-only — locked so a bad edit can't run commands on your PC. Change these via the Settings page.</p>
    {:else}
      {#if lintIssues.length > 0}
        <div class="warn-card">
          <p>
            {lintIssues.length} line{lintIssues.length === 1 ? "" : "s"} don't look like
            <code>Key = Value</code> (line{lintIssues.length === 1 ? "" : "s"}
            {lintIssues.slice(0, 10).map((i) => i.line).join(", ")}{lintIssues.length > 10 ? "…" : ""}).
            Check for typos before saving — you can still save if this is intentional.
          </p>
        </div>
      {/if}
      {#if lastBackup}<p class="muted">Previous version kept as {lastBackup}</p>{/if}
      <div class="row">
        <button
          class="primary"
          onclick={saveFileChecked}
          disabled={saving || restartState.restarting || resetting || featureLocked("config-edit")}
          title={featureLocked("config-edit") ? LOCKED_HINT : undefined}
        >
          {lintConfirm && lintIssues.length > 0 ? "Save anyway — sure?" : "Save"}
        </button>
        <button
          onclick={saveAndRestartFile}
          disabled={saving || restartState.restarting || resetting || featureLocked("config-edit")}
          title={featureLocked("config-edit") ? LOCKED_HINT : undefined}
        >
          {confirmingRestart ? "This disconnects players — sure?" : "Save & Restart"}
        </button>
        {#if currentFileMeta?.dist}
          <button
            class="danger"
            onclick={resetFile}
            disabled={saving || restartState.restarting || resetting || featureLocked("config-reset")}
            title={featureLocked("config-reset") ? LOCKED_HINT : undefined}
          >
            {confirmingReset ? `Overwrite ${file} with its defaults — sure?` : "Reset to defaults"}
          </button>
        {/if}
      </div>
    {/if}
  {/if}

  {#if buf.show}
    <Terminal state={buf.term} onclear={() => clearBuf("config")} logName="dml-config" />
  {/if}
</section>

<style>
  .content { padding: 20px 24px; overflow-y: auto; display: flex; flex-direction: column; gap: 12px; }
  /* Module Configs editor fills the window (user request): with a file
     open, the page stops scrolling and the textarea takes all free height
     -- the save/restart rows below it stay pinned and visible. */
  .content.fill { overflow: hidden; box-sizing: border-box; }
  .content.fill textarea { flex: 1; min-height: 240px; resize: none; }
  .bar { display: flex; justify-content: space-between; align-items: center; }
  .bar h2 { margin: 0; font-size: 18px; }
  select, textarea { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 8px; }
  textarea { font-family: Consolas, monospace; font-size: 13px; width: 100%; box-sizing: border-box; }
  .row { display: flex; gap: 10px; align-items: center; }
  button { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 14px; cursor: pointer; }
  button.primary { background: #238636; border-color: #2ea043; color: white; }
  button.danger { border-color: #f85149; color: #f85149; }
  button:disabled { opacity: 0.5; cursor: default; }
  .muted { color: #8b949e; font-size: 13px; }
  .error-card { background: #161b22; border: 1px solid #f85149; border-radius: 8px; padding: 12px 16px; }
  .warn-card { background: #161b22; border: 1px solid #d29922; border-radius: 8px; padding: 12px 16px; }
  .warn-card code { font-family: Consolas, monospace; font-size: 12.5px; background: #21262d; border-radius: 4px; padding: 1px 5px; }
</style>
