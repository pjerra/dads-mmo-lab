<script lang="ts">
  import { onMount } from "svelte";
  import {
    wowConfigList,
    wowConfigSet,
    wowConfigRawRead,
    wowConfigRawWrite,
    wowConsoleSend,
    gamesRestart,
    type ConfigSetting,
    type RawFileName,
  } from "$lib/api";
  import { dirtyKeys, requiredSaveFlags } from "$lib/config-diff";
  import { applyEvent } from "$lib/terminal-state";
  import { restartState } from "$lib/restart-state.svelte";
  import Terminal from "$lib/Terminal.svelte";
  import { termBuf, beginRun, clearBuf } from "$lib/term-store.svelte";
  import CharPicker from "$lib/CharPicker.svelte";
  import { featureLocked, LOCKED_HINT, testingModeOn, setTestingMode } from "$lib/features.svelte";

  const WOW_ID = "wow-server-playerbots";
  const FILES: RawFileName[] = [
    ".env",
    "docker-compose.override.yml",
    "playerbots.conf",
    "mod_ahbot.conf",
    "mod_ale.conf",
  ];
  // UI mirror of the CLI's raw-write lock (cli rejects these two names).
  const READONLY_FILES: RawFileName[] = [".env", "docker-compose.override.yml"];

  let { tab = "settings" }: { tab?: "settings" | "files" } = $props();
  let settings: ConfigSetting[] = $state([]);
  let edits: Record<string, string> = $state({});
  let error: string | null = $state(null);
  let saving = $state(false);

  let file: RawFileName = $state(".env");
  let fileContent = $state("");
  let fileLoaded = $state(false);
  let loadingFile = $state(false);
  let lastBackup: string | null = $state(null);

  const buf = termBuf("config");

  let aleNote: string | null = $state(null);

  const groups = $derived([...new Set(settings.map((s) => s.group))]);
  const dirty = $derived(dirtyKeys(settings, edits));
  const fileReadonly = $derived(READONLY_FILES.includes(file));
  // Conf-file rows (Batch 1) are a new save mechanism gated behind their own
  // flags -- the Save button locks when ANY dirty row's flag is still locked.
  const saveLocked = $derived(requiredSaveFlags(settings, dirty).some((f) => featureLocked(f)));
  let liveNote = $state(false);

  async function load() {
    error = null;
    try {
      settings = await wowConfigList();
      edits = {};
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    }
  }
  onMount(load);

  async function saveSettings(): Promise<boolean> {
    saving = true;
    error = null;
    aleNote = null;
    liveNote = false;
    try {
      const toSave = dirty;
      let anyLive = false;
      let anyRestart = false;
      for (const key of toSave) {
        const r = await wowConfigSet(key, edits[key]);
        if (r.restart_required) {
          restartState.needed = true;
          anyRestart = true;
        } else if (r.applied === "live") {
          anyLive = true;
        }
      }
      // Conf rows the running server picked up over SOAP -- show the calm
      // "applied live" note instead of the restart banner (only when NO
      // saved row still needs a restart).
      liveNote = anyLive && !anyRestart;
      await load();
      return true;
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
      return false;
    } finally {
      saving = false;
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
      restartState.needed = true;
      return true;
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
      return false;
    } finally {
      saving = false;
    }
  }

  async function reloadAle() {
    saving = true;
    error = null;
    aleNote = null;
    try {
      const r = await wowConsoleSend("reload ale");
      aleNote = r.result.trim();
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    } finally {
      saving = false;
    }
  }

  let confirmingRestart = $state(false);
  // Switching between the Settings and Modules sidebar entries changes `tab`
  // without remounting -- an armed "sure?" confirmation must not survive that.
  $effect(() => {
    void tab;
    confirmingRestart = false;
    aleNote = null;
    liveNote = false;
  });
  async function saveAndRestart(saveFn: () => Promise<boolean>) {
    if (!confirmingRestart) {
      confirmingRestart = true;
      return;
    }
    confirmingRestart = false;
    if (!(await saveFn())) return;
    restartState.restarting = true;
    beginRun("config");
    try {
      await gamesRestart(WOW_ID, (e) => {
        buf.term = applyEvent(buf.term, e);
      });
      restartState.needed = false;
    } catch (e) {
      const err = e as { code?: string; message?: string; hint?: string };
      buf.term = applyEvent(buf.term, {
        event: "error",
        error: { code: err.code ?? "IPC", message: err.message ?? String(e), hint: err.hint ?? "" },
      });
    } finally {
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
    aleNote = null;
  }
</script>

<section class="content" class:fill={tab === "files" && fileLoaded}>
  <header class="bar">
    <h2>{tab === "settings" ? "Settings" : "Modules"}</h2>
  </header>

  {#if error}<div class="error-card"><p>{error}</p></div>{/if}
  {#if restartState.needed}
    <div class="warn-card"><p>Saved — restart the server to apply the changes.</p></div>
  {:else if liveNote}
    <div class="live-card"><p>Applied live ✓ — the running server picked the change up, no restart needed.</p></div>
  {/if}

  {#if tab === "settings"}
    <div class="card testing-card">
      <label class="row">
        <input
          type="checkbox"
          checked={testingModeOn()}
          onchange={(e) => setTestingMode(e.currentTarget.checked)}
        />
        Enable untested features (for smoke testing)
      </label>
      <p class="muted">
        Untested features stay disabled until their smoke test passes. The checklist lives in
        docs/SMOKE-TESTS.md.
      </p>
    </div>
    {#each groups as g (g)}
      <h3>{g}</h3>
      {#each settings.filter((s) => s.group === g) as s (s.key)}
        <div class="setting" class:dirty={dirty.includes(s.key)}>
          <div class="meta">
            <strong>{s.label}</strong>
            <span class="muted">{s.explain}</span>
          </div>
          {#if s.type === "bool"}
            <input
              type="checkbox"
              checked={(edits[s.key] ?? s.value) === "1"}
              disabled={saving || restartState.restarting}
              onchange={(e) => {
                edits[s.key] = e.currentTarget.checked ? "1" : "0";
                confirmingRestart = false;
              }}
            />
          {:else if s.type === "float" || s.type === "int"}
            <input
              type="number"
              min={s.min}
              max={s.max}
              step={s.type === "float" ? "0.5" : "1"}
              value={edits[s.key] ?? s.value}
              disabled={saving || restartState.restarting}
              oninput={(e) => {
                edits[s.key] = e.currentTarget.value;
                confirmingRestart = false;
              }}
            />
          {:else if s.type === "char"}
            <div class="charwrap">
              <span class="muted">current id: {s.value}</span>
              <CharPicker
                selected={edits[s.key] ?? ""}
                disabled={saving || restartState.restarting}
                onpick={(v: string) => {
                  edits[s.key] = v;
                  confirmingRestart = false;
                }}
              />
            </div>
          {:else}
            <input
              value={edits[s.key] ?? s.value}
              disabled={saving || restartState.restarting}
              oninput={(e) => {
                edits[s.key] = e.currentTarget.value;
                confirmingRestart = false;
              }}
            />
          {/if}
        </div>
      {/each}
    {/each}
    <div class="row">
      <button
        class="primary"
        onclick={saveSettings}
        disabled={dirty.length === 0 || saving || restartState.restarting || saveLocked}
        title={saveLocked ? LOCKED_HINT : undefined}
      >
        Save {dirty.length > 0 ? `(${dirty.length})` : ""}
      </button>
      <button
        onclick={() => saveAndRestart(saveSettings)}
        disabled={dirty.length === 0 || saving || restartState.restarting || saveLocked}
        title={saveLocked ? LOCKED_HINT : undefined}
      >
        {confirmingRestart ? "This disconnects players — sure?" : "Save & Restart"}
      </button>
    </div>

  {:else}
    <div class="row">
      <select bind:value={file} onchange={onFileSelect} disabled={saving || restartState.restarting || loadingFile}>
        {#each FILES as f (f)}<option value={f}>{f}</option>{/each}
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
        oninput={() => (confirmingRestart = false)}
        readonly={fileReadonly}
        disabled={saving || restartState.restarting}
      ></textarea>
      {#if fileReadonly}
        <p class="muted">Read-only — locked so a bad edit can't run commands on your PC. Change these via the Settings page.</p>
      {:else}
        {#if lastBackup}<p class="muted">Previous version kept as {lastBackup}</p>{/if}
        <div class="row">
          <button
            class="primary"
            onclick={saveFile}
            disabled={saving || restartState.restarting || featureLocked("config-edit")}
            title={featureLocked("config-edit") ? LOCKED_HINT : undefined}
          >
            Save
          </button>
          <button
            onclick={() => saveAndRestart(saveFile)}
            disabled={saving || restartState.restarting || featureLocked("config-edit")}
            title={featureLocked("config-edit") ? LOCKED_HINT : undefined}
          >
            {confirmingRestart ? "This disconnects players — sure?" : "Save & Restart"}
          </button>
        </div>
      {/if}
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
  h3 { margin: 10px 0 0; font-size: 15px; color: #58a6ff; }
  .setting { display: flex; justify-content: space-between; align-items: center; gap: 16px; background: #0d1117; border: 1px solid #30363d; border-radius: 8px; padding: 10px 14px; }
  .setting.dirty { border-color: #d29922; }
  .card { background: #0d1117; border: 1px solid #30363d; border-radius: 8px; padding: 12px 16px; display: flex; flex-direction: column; gap: 6px; }
  .testing-card { margin-top: 6px; }
  .meta { display: flex; flex-direction: column; gap: 2px; }
  .charwrap { display: flex; gap: 8px; align-items: center; }
  input, select, textarea { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 8px; }
  textarea { font-family: Consolas, monospace; font-size: 13px; width: 100%; box-sizing: border-box; }
  .row { display: flex; gap: 10px; align-items: center; }
  button { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 14px; cursor: pointer; }
  button.primary { background: #238636; border-color: #2ea043; color: white; }
  button:disabled { opacity: 0.5; cursor: default; }
  .muted { color: #8b949e; font-size: 13px; }
  .error-card { background: #161b22; border: 1px solid #f85149; border-radius: 8px; padding: 12px 16px; }
  .warn-card { background: #161b22; border: 1px solid #d29922; border-radius: 8px; padding: 12px 16px; }
  .live-card { background: #161b22; border: 1px solid #2ea043; border-radius: 8px; padding: 12px 16px; }
</style>
