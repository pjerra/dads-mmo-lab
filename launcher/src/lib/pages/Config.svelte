<script lang="ts">
  import { onMount } from "svelte";
  import {
    wowConfigList,
    wowConfigSet,
    wowConfigPbKeys,
    wowConfigFiles,
    wowConfigRawRead,
    wowConfigRawWrite,
    wowConfigRawReset,
    wowConsoleSend,
    gamesRestart,
    wowBotsFlush,
    type ConfFile,
    type ConfigSetting,
    type PbKey,
    type RawFileName,
  } from "$lib/api";
  import { filterPbKeys, stagedPbChanges } from "$lib/pb-keys";
  import { dirtyKeys, requiredSaveFlags } from "$lib/config-diff";
  import { applyEvent } from "$lib/terminal-state";
  import { restartState } from "$lib/restart-state.svelte";
  import Terminal from "$lib/Terminal.svelte";
  import { termBuf, beginRun, clearBuf } from "$lib/term-store.svelte";
  import CharPicker from "$lib/CharPicker.svelte";
  import { featureLocked, LOCKED_HINT, testingModeOn, setTestingMode } from "$lib/features.svelte";

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

  let { tab = "settings" }: { tab?: "settings" | "files" | "botworld" } = $props();
  let settings: ConfigSetting[] = $state([]);
  let edits: Record<string, string> = $state({});
  let error: string | null = $state(null);
  let saving = $state(false);

  let file: RawFileName = $state(".env");
  let fileContent = $state("");
  let fileLoaded = $state(false);
  let loadingFile = $state(false);
  let lastBackup: string | null = $state(null);
  let confFiles: ConfFile[] = $state(FALLBACK_FILES);
  let confFilesLoaded = $state(false);
  let confirmingReset = $state(false);
  let resetting = $state(false);

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
    if (tab === "files" && !confFilesLoaded) void loadConfFiles();
  });

  const currentFileMeta = $derived(confFiles.find((f) => f.name === file));

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
      restartState.needed = true;
      if (file === target) await loadFile();
      lastBackup = r.backup;
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    } finally {
      resetting = false;
    }
  }

  const buf = termBuf("config");

  let aleNote: string | null = $state(null);

  // --- Bot World all-keys browser (Batch 1 F2) -----------------------------
  const PB_RENDER_CAP = 200;
  let pbKeys: PbKey[] = $state([]);
  let pbLoaded = $state(false);
  let pbQuery = $state("");
  let pbEdits: Record<string, string> = $state({});
  let pbSaving = $state(false);
  const pbFiltered = $derived(filterPbKeys(pbKeys, pbQuery));
  const pbShown = $derived(pbFiltered.slice(0, PB_RENDER_CAP));
  const pbStaged = $derived(stagedPbChanges(pbKeys, pbEdits));

  async function loadPbKeys() {
    try {
      const r = await wowConfigPbKeys();
      pbKeys = r.keys;
      pbEdits = {};
      pbLoaded = true;
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    }
  }
  $effect(() => {
    if (tab === "botworld" && !pbLoaded) void loadPbKeys();
  });

  // --- Flush & rebuild bot population (Batch 1 F4) -------------------------
  let flushConfirm = $state("");
  let flushing = $state(false);

  async function runFlush() {
    if (flushConfirm !== "flush") return;
    flushConfirm = "";
    flushing = true;
    restartState.restarting = true; // the flush restarts the server twice
    error = null;
    beginRun("config");
    try {
      await wowBotsFlush((e) => {
        buf.term = applyEvent(buf.term, e);
      });
    } catch (e) {
      const err = e as { code?: string; message?: string; hint?: string };
      buf.term = applyEvent(buf.term, {
        event: "error",
        error: { code: err.code ?? "IPC", message: err.message ?? String(e), hint: err.hint ?? "" },
      });
    } finally {
      flushing = false;
      restartState.restarting = false;
    }
  }

  async function savePbChanges() {
    pbSaving = true;
    error = null;
    try {
      for (const c of pbStaged) {
        const r = await wowConfigSet(`conf:playerbots.conf:${c.key}`, c.value);
        if (r.restart_required) restartState.needed = true;
      }
      await loadPbKeys();
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    } finally {
      pbSaving = false;
    }
  }

  const groups = $derived([...new Set(settings.map((s) => s.group))]);
  // "Bot ..."-prefixed groups render on the Bot World tab, everything else
  // on Settings (AHBot deliberately does NOT match the "Bot " prefix).
  const visibleGroups = $derived(
    tab === "botworld" ? groups.filter((g) => g.startsWith("Bot ")) : groups.filter((g) => !g.startsWith("Bot ")),
  );
  const dirty = $derived(dirtyKeys(settings, edits));
  const fileReadonly = $derived(
    confFiles.find((f) => f.name === file)?.readonly ?? READONLY_FILES.includes(file),
  );
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
    confirmingReset = false;
    aleNote = null;
  }
</script>

<section class="content" class:fill={tab === "files" && fileLoaded}>
  <header class="bar">
    <h2>{tab === "settings" ? "Settings" : tab === "botworld" ? "Bot World" : "Modules"}</h2>
  </header>

  {#if error}<div class="error-card"><p>{error}</p></div>{/if}
  {#if restartState.needed}
    <div class="warn-card"><p>Saved — restart the server to apply the changes.</p></div>
  {:else if liveNote}
    <div class="live-card"><p>Applied live ✓ — the running server picked the change up, no restart needed.</p></div>
  {/if}

  {#if tab === "settings" || tab === "botworld"}
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
    {/if}
    {#each visibleGroups as g (g)}
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

    {#if tab === "botworld"}
      <h3>All playerbots.conf keys</h3>
      <div class="card">
        <p class="muted">
          Every setting the bots module knows, straight from playerbots.conf. Changes apply after a
          server restart. Hover a key for its default value.
        </p>
        <input placeholder="Search keys… (e.g. broadcast, teleport, revive)" bind:value={pbQuery} />
        {#if pbLoaded}
          <div class="pb-list">
            {#each pbShown as k (k.key)}
              <div class="pbrow" class:dirty={pbEdits[k.key] !== undefined && pbEdits[k.key] !== k.value}>
                <span class="pbkey" title={k.default !== null ? `Default: ${k.default}` : "No default recorded"}>{k.key}</span>
                <input
                  class="pbval"
                  value={pbEdits[k.key] ?? k.value}
                  disabled={pbSaving || restartState.restarting}
                  oninput={(e) => (pbEdits[k.key] = e.currentTarget.value)}
                />
              </div>
            {/each}
          </div>
          {#if pbFiltered.length > PB_RENDER_CAP}
            <p class="muted">Showing the first {PB_RENDER_CAP} of {pbFiltered.length} matches — narrow the search.</p>
          {:else if pbFiltered.length === 0}
            <p class="muted">No keys match.</p>
          {/if}
          <div class="row">
            <button
              class="primary"
              onclick={savePbChanges}
              disabled={pbStaged.length === 0 || pbSaving || restartState.restarting || featureLocked("bots-world")}
              title={featureLocked("bots-world") ? LOCKED_HINT : undefined}
            >
              Save {pbStaged.length} change{pbStaged.length === 1 ? "" : "s"}
            </button>
          </div>
        {:else}
          <p class="muted">Loading keys…</p>
        {/if}
      </div>

      <h3>Danger zone</h3>
      <div class="card danger-card">
        <strong>Flush &amp; rebuild the bot population</strong>
        <p class="muted">
          Deletes ALL ~{settings.find((s) => s.key === "bots.population")?.value ?? "2000"} random
          bots' characters, auctions and mail, then rebuilds the population from your settings above.
          Your own characters and party bots on real accounts are untouched. A character backup is
          taken first. The server restarts twice — this takes several minutes.
        </p>
        <div class="row">
          <input
            placeholder={'Type "flush" to confirm'}
            bind:value={flushConfirm}
            disabled={flushing || restartState.restarting}
          />
          <button
            class="danger"
            onclick={runFlush}
            disabled={flushConfirm !== "flush" || flushing || restartState.restarting || featureLocked("bots-flush")}
            title={featureLocked("bots-flush") ? LOCKED_HINT : undefined}
          >
            Flush &amp; rebuild
          </button>
        </div>
      </div>
    {/if}

  {:else}
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
        }}
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
            disabled={saving || restartState.restarting || resetting || featureLocked("config-edit")}
            title={featureLocked("config-edit") ? LOCKED_HINT : undefined}
          >
            Save
          </button>
          <button
            onclick={() => saveAndRestart(saveFile)}
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
  button.danger { border-color: #f85149; color: #f85149; }
  button:disabled { opacity: 0.5; cursor: default; }
  .muted { color: #8b949e; font-size: 13px; }
  .error-card { background: #161b22; border: 1px solid #f85149; border-radius: 8px; padding: 12px 16px; }
  .warn-card { background: #161b22; border: 1px solid #d29922; border-radius: 8px; padding: 12px 16px; }
  .live-card { background: #161b22; border: 1px solid #2ea043; border-radius: 8px; padding: 12px 16px; }
  .danger-card { border-color: #f85149; }
  .pb-list { display: flex; flex-direction: column; gap: 4px; max-height: 420px; overflow-y: auto; }
  .pbrow { display: flex; justify-content: space-between; align-items: center; gap: 12px; padding: 3px 6px; border-radius: 6px; }
  .pbrow.dirty { background: #1c1a10; outline: 1px solid #d29922; }
  .pbkey { font-family: Consolas, monospace; font-size: 12.5px; color: #c9d1d9; overflow-wrap: anywhere; }
  .pbval { width: 220px; flex-shrink: 0; font-family: Consolas, monospace; font-size: 12.5px; }
</style>
