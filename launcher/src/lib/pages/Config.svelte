<script lang="ts">
  import { onMount } from "svelte";
  import {
    wowConfigList,
    wowConfigSet,
    wowConfigRawRead,
    wowConfigRawWrite,
    gamesRestart,
    type ConfigSetting,
    type RawFileName,
  } from "$lib/api";
  import { dirtyKeys } from "$lib/config-diff";
  import { applyEvent, initialTermState, type TermState } from "$lib/terminal-state";
  import { restartState } from "$lib/restart-state.svelte";
  import Terminal from "$lib/Terminal.svelte";
  import CharPicker from "$lib/CharPicker.svelte";

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

  let term: TermState = $state(initialTermState());
  let showTerm = $state(false);

  const groups = $derived([...new Set(settings.map((s) => s.group))]);
  const dirty = $derived(dirtyKeys(settings, edits));
  const fileReadonly = $derived(READONLY_FILES.includes(file));

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
    try {
      const toSave = dirty;
      for (const key of toSave) {
        const r = await wowConfigSet(key, edits[key]);
        if (r.restart_required) restartState.needed = true;
      }
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

  let confirmingRestart = $state(false);
  // Switching between the Settings and Modules sidebar entries changes `tab`
  // without remounting -- an armed "sure?" confirmation must not survive that.
  $effect(() => {
    void tab;
    confirmingRestart = false;
  });
  async function saveAndRestart(saveFn: () => Promise<boolean>) {
    if (!confirmingRestart) {
      confirmingRestart = true;
      return;
    }
    confirmingRestart = false;
    if (!(await saveFn())) return;
    restartState.restarting = true;
    showTerm = true;
    term = initialTermState();
    try {
      await gamesRestart(WOW_ID, (e) => {
        term = applyEvent(term, e);
      });
      restartState.needed = false;
    } catch (e) {
      const err = e as { code?: string; message?: string; hint?: string };
      term = applyEvent(term, {
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
  }
</script>

<section class="content">
  <header class="bar">
    <h2>{tab === "settings" ? "Settings" : "Modules"}</h2>
  </header>

  {#if error}<div class="error-card"><p>{error}</p></div>{/if}
  {#if restartState.needed}
    <div class="warn-card"><p>Saved — restart the server to apply the changes.</p></div>
  {/if}

  {#if tab === "settings"}
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
        disabled={dirty.length === 0 || saving || restartState.restarting}
      >
        Save {dirty.length > 0 ? `(${dirty.length})` : ""}
      </button>
      <button
        onclick={() => saveAndRestart(saveSettings)}
        disabled={dirty.length === 0 || saving || restartState.restarting}
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
          <button class="primary" onclick={saveFile} disabled={saving || restartState.restarting}>Save</button>
          <button onclick={() => saveAndRestart(saveFile)} disabled={saving || restartState.restarting}>
            {confirmingRestart ? "This disconnects players — sure?" : "Save & Restart"}
          </button>
        </div>
      {/if}
    {/if}
  {/if}

  {#if showTerm}
    <Terminal state={term} />
  {/if}
</section>

<style>
  .content { padding: 20px 24px; overflow-y: auto; display: flex; flex-direction: column; gap: 12px; }
  .bar { display: flex; justify-content: space-between; align-items: center; }
  .bar h2 { margin: 0; font-size: 18px; }
  h3 { margin: 10px 0 0; font-size: 15px; color: #58a6ff; }
  .setting { display: flex; justify-content: space-between; align-items: center; gap: 16px; background: #0d1117; border: 1px solid #30363d; border-radius: 8px; padding: 10px 14px; }
  .setting.dirty { border-color: #d29922; }
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
</style>
