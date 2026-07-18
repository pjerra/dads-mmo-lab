<script lang="ts">
  import { onMount } from "svelte";
  import {
    wowModuleList,
    wowModuleInstall,
    wowModuleRemove,
    wowModuleRebuild,
    wowModuleConfActivate,
    wowModuleTracking,
    wowModuleRepair,
    wowClientPathGet,
    wowClientPathSet,
    wowClientPathDetect,
    type ModuleList,
    type CppModule,
    type LuaModule,
    type SqlModule,
    type ClientPath,
    type TermEvent,
    type ModuleTracking,
    type ModuleRepair,
    type RepairResult,
  } from "$lib/api";
  import { applyEvent, initialTermState, type TermState } from "$lib/terminal-state";
  import Terminal from "$lib/Terminal.svelte";

  let list: ModuleList | null = $state(null);
  let error: string | null = $state(null);
  let note: string | null = $state(null);
  let busy = $state(false); // single flag: disables every Install/Update/Remove/Rebuild/Activate/Save/Detect button

  let confirmingRebuild = $state(false);
  let backupChecked = $state(true); // "Back up the server first" defaults ON
  let confirmingRemove: string | null = $state(null); // key of the cpp module armed for removal
  let customUrl = $state("");

  // Repair panel (installed cpp rows): key of the module whose panel is open
  // (one at a time, like confirmingRemove above), its fetched tracking
  // diagnosis, the db/mode picks, the two-step apply confirm, and the last
  // apply's per-file results. repairError is the panel's own inline-error
  // surface -- separate from the page-level `error` so a failed tracking
  // fetch/apply doesn't blow away an unrelated page error (and vice versa).
  let repairOpen: string | null = $state(null);
  let tracking: ModuleTracking | null = $state(null);
  let repairError: string | null = $state(null);
  let repairDb: "world" | "characters" | "auth" = $state("world");
  let repairMode: "mark" | "clear" = $state("mark");
  let confirmingRepair = $state(false);
  let repairResult: ModuleRepair | null = $state(null);
  const DB_ORDER: Array<"world" | "characters" | "auth"> = ["world", "characters", "auth"];

  // Lua (ALE) card: per-row "back up first" checkbox (has_sql rows only,
  // default ON) and the key armed for a two-step remove confirm.
  let luaBackup: Record<string, boolean> = $state({});
  let confirmingLuaRemove: string | null = $state(null);

  // SQL mods card: per-row backup checkbox (default ON, every row), the key
  // armed for a remove confirm, and the two variant-typed rows' inputs.
  let sqlBackup: Record<string, boolean> = $state({});
  let confirmingSqlRemove: string | null = $state(null);
  let hearthstoneVariant = $state("5min");
  let npcTeleporterLevel = $state(80);

  // Client folder card.
  let clientPath: ClientPath | null = $state(null);
  let clientPathInput = $state("");
  let clientError: string | null = $state(null);
  let clientCandidates: string[] | null = $state(null);

  let term: TermState = $state(initialTermState());
  let showTerm = $state(false);

  function showErr(e: unknown) {
    const err = e as { message?: string; hint?: string };
    error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
  }

  // New keys default their backup checkbox ON without stomping a value the
  // user already toggled (called after every list refresh).
  function ensureBackupDefaults() {
    if (!list) return;
    for (const m of list.families.lua) {
      if (m.has_sql && !(m.key in luaBackup)) luaBackup[m.key] = true;
    }
    for (const m of list.families.sql) {
      if (!(m.key in sqlBackup)) sqlBackup[m.key] = true;
    }
  }

  async function refresh() {
    error = null; confirmingRebuild = false; confirmingRemove = null;
    confirmingLuaRemove = null; confirmingSqlRemove = null;
    repairOpen = null; confirmingRepair = false;
    try { list = await wowModuleList(); ensureBackupDefaults(); } catch (e) { showErr(e); }
    try { clientPath = await wowClientPathGet(); } catch (e) { showErr(e); }
  }
  onMount(refresh);

  // Streamed operations (install/remove/rebuild) use the sawDone/streamErr
  // contract: the outcome is derived from events captured inside the event
  // callback, then applied AFTER the trailing refresh() -- never from the
  // promise resolving, since the streaming promise resolves even when the
  // underlying CLI step failed.
  async function runStream(
    run: (onEvent: (e: TermEvent) => void) => Promise<void>,
    onDone: (doneData: unknown) => void,
  ) {
    busy = true; error = null; note = null; showTerm = true; term = initialTermState();
    let sawDone = false;
    let doneData: unknown;
    let streamErr: { message?: string; hint?: string } | null = null;
    let outcomeErr: unknown = null;
    try {
      await run((e) => {
        term = applyEvent(term, e);
        if (e.event === "done") {
          sawDone = true;
          doneData = (e as { data?: unknown }).data;
        } else if (e.event === "error") {
          streamErr = (e as { error?: { message?: string; hint?: string } }).error ?? {};
        }
      });
    } catch (e) {
      outcomeErr = e;
    } finally {
      busy = false;
      await refresh();
      if (outcomeErr) showErr(outcomeErr);
      else if (streamErr) showErr(streamErr);
      else if (sawDone) onDone(doneData);
    }
  }

  function install(key: string | null, url: string | null, label: string) {
    return runStream(
      (onEvent) => wowModuleInstall("cpp", key, url, onEvent),
      () => {
        note = `Installed ${label}.`;
        if (url) customUrl = "";
      },
    );
  }

  function removeModule(m: CppModule) {
    if (confirmingRemove !== m.key) {
      confirmingRemove = m.key;
      return;
    }
    confirmingRemove = null;
    return runStream(
      (onEvent) => wowModuleRemove("cpp", m.key, onEvent),
      () => { note = `Removed ${m.name}.`; },
    );
  }

  function rebuild() {
    if (!confirmingRebuild) {
      confirmingRebuild = true;
      return;
    }
    confirmingRebuild = false;
    return runStream(
      (onEvent) => wowModuleRebuild(backupChecked, onEvent),
      () => { note = "Rebuild complete."; },
    );
  }

  async function activateConf(key: string) {
    busy = true; error = null; note = null;
    try {
      const r = await wowModuleConfActivate(key);
      note = `Activated ${r.conf_name}.`;
      await refresh();
    } catch (e) { showErr(e); } finally { busy = false; }
  }

  function installLua(m: LuaModule) {
    const backup = m.has_sql ? luaBackup[m.key] : undefined;
    return runStream(
      (onEvent) => wowModuleInstall("lua", m.key, null, onEvent, backup),
      (doneData) => {
        const d = doneData as { reload?: string } | undefined;
        note = d?.reload ?? `Installed ${m.name}.`;
      },
    );
  }

  function removeLua(m: LuaModule) {
    if (confirmingLuaRemove !== m.key) {
      confirmingLuaRemove = m.key;
      return;
    }
    confirmingLuaRemove = null;
    return runStream(
      (onEvent) => wowModuleRemove("lua", m.key, onEvent),
      () => { note = `Removed ${m.name}.`; },
    );
  }

  function installSql(m: SqlModule) {
    let variant: string | undefined;
    if (m.key === "hearthstone-cd") variant = hearthstoneVariant;
    else if (m.key === "npc-teleporter") variant = String(npcTeleporterLevel);
    return runStream(
      (onEvent) => wowModuleInstall("sql", m.key, null, onEvent, sqlBackup[m.key], variant),
      () => { note = `Installed ${m.name}.`; },
    );
  }

  function removeSql(m: SqlModule) {
    if (confirmingSqlRemove !== m.key) {
      confirmingSqlRemove = m.key;
      return;
    }
    confirmingSqlRemove = null;
    return runStream(
      (onEvent) => wowModuleRemove("sql", m.key, onEvent, sqlBackup[m.key]),
      () => { note = `Removed ${m.name}.`; },
    );
  }

  function clientErr(e: unknown) {
    const err = e as { message?: string; hint?: string };
    clientError = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
  }

  async function saveClientPath() {
    busy = true; clientError = null; note = null;
    try {
      clientPath = await wowClientPathSet(clientPathInput);
      note = `Client folder set — ${clientPath.path}.`;
      clientPathInput = "";
      clientCandidates = null;
    } catch (e) { clientErr(e); } finally { busy = false; }
  }

  async function detectClientPath() {
    busy = true; clientError = null;
    try {
      const r = await wowClientPathDetect();
      clientCandidates = r.candidates;
    } catch (e) { clientErr(e); } finally { busy = false; }
  }

  async function pickClientCandidate(path: string) {
    busy = true; clientError = null; note = null;
    try {
      clientPath = await wowClientPathSet(path);
      note = `Client folder set — ${clientPath.path}.`;
      clientCandidates = null;
    } catch (e) { clientErr(e); } finally { busy = false; }
  }

  // Copy pinned by the brief verbatim -- mod-arac's data is not reverted by
  // a clone removal (SQL rows / DBC files / client patch all stay behind).
  function removeConfirmText(m: CppModule): string {
    if (m.key === "mod-arac") {
      return "mod-arac is data-only — removing does not revert its SQL, DBC or client patch";
    }
    return `Remove ${m.name} — sure?`;
  }

  function statusText(m: CppModule): string {
    if (!m.installed) return "Not installed";
    if (m.pending_rebuild) return "Installed — rebuild pending";
    return "Installed";
  }
  function statusClass(m: CppModule): "on" | "warn" | "off" {
    if (!m.installed) return "off";
    if (m.pending_rebuild) return "warn";
    return "on";
  }

  function showRepairErr(e: unknown) {
    const err = e as { message?: string; hint?: string };
    repairError = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
  }

  async function fetchTracking(key: string) {
    busy = true; repairError = null;
    try {
      tracking = await wowModuleTracking(key);
    } catch (e) {
      showRepairErr(e);
    } finally {
      busy = false;
    }
  }

  function toggleRepair(m: CppModule) {
    if (repairOpen === m.key) {
      repairOpen = null;
      return;
    }
    repairOpen = m.key;
    tracking = null;
    repairError = null;
    repairResult = null;
    confirmingRepair = false;
    repairDb = "world";
    repairMode = "mark";
    return fetchTracking(m.key);
  }

  function disarmRepair() {
    confirmingRepair = false;
  }

  async function applyRepair(m: CppModule) {
    if (!confirmingRepair) {
      confirmingRepair = true;
      return;
    }
    confirmingRepair = false;
    busy = true; repairError = null; repairResult = null;
    try {
      repairResult = await wowModuleRepair(m.key, repairDb, repairMode);
      tracking = await wowModuleTracking(m.key);
    } catch (e) {
      showRepairErr(e);
    } finally {
      busy = false;
    }
  }

  function humanizeResult(r: RepairResult["result"]): string {
    switch (r) {
      case "marked": return "marked";
      case "cleared": return "cleared";
      case "not_tracked": return "not tracked";
      case "file_missing": return "file missing";
    }
  }

  function resultClass(r: RepairResult["result"]): "on" | "warn" | "off" {
    if (r === "marked" || r === "cleared") return "on";
    if (r === "file_missing") return "warn";
    return "off";
  }
</script>

<section class="content">
  <header class="bar">
    <h2>Modules</h2>
    <button onclick={refresh} disabled={busy}>Refresh</button>
  </header>

  {#if error}<div class="error-card"><p>{error}</p></div>{/if}
  {#if note}<p class="muted">{note}</p>{/if}

  {#if list && list.rebuild_pending.length > 0}
    <div class="card warn-card">
      <p><strong>Server rebuild required for: {list.rebuild_pending.join(", ")}</strong></p>
      <label class="row">
        <input type="checkbox" bind:checked={backupChecked} disabled={busy} />
        Back up the server first (recommended)
      </label>
      <div class="row">
        {#if !confirmingRebuild}
          <button class="primary" onclick={rebuild} disabled={busy}>Rebuild now</button>
        {:else}
          <span>Rebuild takes 30–90 minutes and stops the world while building. Continue?</span>
          <button class="primary" onclick={rebuild} disabled={busy}>Confirm</button>
          <button onclick={() => (confirmingRebuild = false)} disabled={busy}>Cancel</button>
        {/if}
      </div>
    </div>
  {/if}

  <div class="card">
    <h3>C++ modules</h3>
    {#if list}
      {#each list.families.cpp as m (m.key)}
        <div class="row mrow">
          <strong class="mname">{m.name}</strong>
          <span class="badge {statusClass(m)}">{statusText(m)}</span>
          {#if m.conf === "ready"}
            <button onclick={() => activateConf(m.key)} disabled={busy}>Activate conf</button>
          {:else if m.conf === "active"}
            <span class="muted">conf active</span>
          {/if}
          <span class="spacer"></span>
          {#if !m.installed}
            <button class="primary" onclick={() => install(m.key, null, m.name)} disabled={busy}>Install</button>
          {:else}
            <button onclick={() => install(m.key, null, m.name)} disabled={busy}>Update</button>
            <button onclick={() => toggleRepair(m)} disabled={busy}>Repair…</button>
            <button onclick={() => removeModule(m)} disabled={busy}>
              {confirmingRemove === m.key ? removeConfirmText(m) : "Remove"}
            </button>
          {/if}
        </div>
        {#if repairOpen === m.key}
          <div class="repair-panel">
            {#if repairError}<p class="inline-error">{repairError}</p>{/if}
            {#if tracking}
              {#each DB_ORDER as dbName (dbName)}
                {@const dbData = tracking.dbs[dbName]}
                <div class="repair-db">
                  <strong class="db-name">{dbName}</strong>
                  {#if dbData.files.length === 0 && dbData.tracked_rows.length === 0}
                    <p class="muted">nothing found</p>
                  {:else}
                    {#if dbData.files.length > 0}
                      <div class="row">
                        {#each dbData.files as f (f.name)}
                          <span class="chip {f.tracked ? 'tracked' : 'untracked'}">{f.name}</span>
                        {/each}
                      </div>
                    {/if}
                    {#if dbData.tracked_rows.length > 0}
                      <div class="row">
                        {#each dbData.tracked_rows as name (name)}
                          <span class="muted">{name}</span>
                        {/each}
                      </div>
                    {/if}
                  {/if}
                </div>
              {/each}

              <div class="row">
                <label class="row">
                  DB
                  <select bind:value={repairDb} onchange={disarmRepair} disabled={busy}>
                    <option value="world">world</option>
                    <option value="characters">characters</option>
                    <option value="auth">auth</option>
                  </select>
                </label>
                <label class="row">
                  Mode
                  <select bind:value={repairMode} onchange={disarmRepair} disabled={busy}>
                    <option value="mark">Mark as applied — fixes "Table already exists" on start</option>
                    <option value="clear">Clear tracking — makes the server re-apply the SQL (only safe if the SQL is re-runnable)</option>
                  </select>
                </label>
              </div>
              <div class="row">
                {#if !confirmingRepair}
                  <button class="primary" onclick={() => applyRepair(m)} disabled={busy}>Apply</button>
                {:else}
                  <span>This edits the database's update-tracking records. Continue?</span>
                  <button class="primary" onclick={() => applyRepair(m)} disabled={busy}>Confirm</button>
                  <button onclick={() => (confirmingRepair = false)} disabled={busy}>Cancel</button>
                {/if}
              </div>

              {#if repairResult}
                <div class="repair-results">
                  {#each repairResult.results as r (r.file)}
                    <div class="row">
                      <span>{r.file}</span>
                      <span class="badge {resultClass(r.result)}">{humanizeResult(r.result)}</span>
                    </div>
                  {/each}
                  <p class="muted">Restart the server to apply.</p>
                </div>
              {/if}
            {/if}
          </div>
        {/if}
      {/each}
    {/if}
  </div>

  <div class="card">
    <h3>Lua scripts (ALE)</h3>
    {#if list}
      {#if !list.ale_ready}
        <p class="muted">Install the ALE module (mod-ale) first — it's in the C++ modules list above.</p>
      {:else}
        {#each list.families.lua as m (m.key)}
          <div class="row mrow">
            <strong class="mname">{m.name}</strong>
            <span class="badge {m.cloned ? 'on' : 'off'}">Cloned</span>
            <span class="badge {m.deployed ? 'on' : 'off'}">Deployed</span>
            <span class="spacer"></span>
            {#if m.has_sql}
              <label class="row">
                <input type="checkbox" bind:checked={luaBackup[m.key]} disabled={busy} />
                Back up first (recommended)
              </label>
            {/if}
            <button class="primary" onclick={() => installLua(m)} disabled={busy}>Install</button>
            <button onclick={() => removeLua(m)} disabled={busy}>
              {confirmingLuaRemove === m.key ? `Remove ${m.name} — sure?` : "Remove"}
            </button>
          </div>
        {/each}
      {/if}
    {/if}
  </div>

  <div class="card">
    <h3>SQL mods</h3>
    {#if list}
      {#each list.families.sql as m (m.key)}
        <div class="row mrow">
          <strong class="mname">{m.name}</strong>
          <span class="badge {m.installed ? 'on' : 'off'}">Installed</span>
          {#if m.key === "hearthstone-cd"}
            <label class="row">
              Cooldown
              <select bind:value={hearthstoneVariant} disabled={busy}>
                <option value="1sec">1sec</option>
                <option value="1min">1min</option>
                <option value="5min">5min</option>
                <option value="15min">15min</option>
                <option value="30min">30min</option>
              </select>
            </label>
          {:else if m.key === "npc-teleporter"}
            <label class="row">
              Level
              <input type="number" min="1" max="80" bind:value={npcTeleporterLevel} disabled={busy} />
            </label>
          {/if}
          <span class="spacer"></span>
          <label class="row">
            <input type="checkbox" bind:checked={sqlBackup[m.key]} disabled={busy} />
            Back up first (recommended)
          </label>
          <button class="primary" onclick={() => installSql(m)} disabled={busy}>Install</button>
          {#if m.key === "rare-drops"}
            <button disabled title="No automated reversal — restore a backup instead.">Remove</button>
          {:else}
            <button onclick={() => removeSql(m)} disabled={busy}>
              {confirmingSqlRemove === m.key ? `Remove ${m.name} — sure?` : "Remove"}
            </button>
          {/if}
        </div>
        {#if m.type === "tweak_world"}
          <p class="muted">Tweaks replace each other — installing one removes the active one.</p>
        {/if}
      {/each}
    {/if}
  </div>

  <div class="card">
    <h3>Install from URL</h3>
    <div class="row">
      <input
        type="text"
        placeholder="https://github.com/.../mod-your-module.git"
        bind:value={customUrl}
        disabled={busy}
      />
      <button
        class="primary"
        onclick={() => install(null, customUrl, customUrl)}
        disabled={busy || !customUrl.trim()}
      >
        Install
      </button>
    </div>
    <p class="muted">mod-* repos only</p>
  </div>

  <div class="card">
    <h3>Client folder</h3>
    <div class="row">
      <strong>Current:</strong>
      {#if !clientPath?.path}
        <span class="muted">(not set)</span>
      {:else}
        <span>{clientPath.path}</span>
        {#if !clientPath.valid}
          <span class="warn-text">(saved folder is missing)</span>
        {/if}
      {/if}
    </div>
    {#if clientError}<p class="inline-error">{clientError}</p>{/if}
    <div class="row">
      <input
        type="text"
        placeholder="C:\Games\WoW"
        bind:value={clientPathInput}
        disabled={busy}
      />
      <button class="primary" onclick={saveClientPath} disabled={busy || !clientPathInput.trim()}>Save</button>
      <button onclick={detectClientPath} disabled={busy}>Detect</button>
    </div>
    {#if clientCandidates}
      {#if clientCandidates.length === 0}
        <p class="muted">No WoW client folders found.</p>
      {:else}
        <div class="row">
          {#each clientCandidates as c (c)}
            <button onclick={() => pickClientCandidate(c)} disabled={busy}>{c}</button>
          {/each}
        </div>
      {/if}
    {/if}
    <p class="muted">Needed for scripts that ship client-side files (BMAH UI, Paragon, SOD). Windows paths like C:\Games\WoW work.</p>
  </div>

  {#if showTerm}
    <Terminal state={term} />
  {/if}
</section>

<style>
  .content { padding: 20px 24px; overflow-y: auto; display: flex; flex-direction: column; gap: 12px; }
  .bar { display: flex; justify-content: space-between; align-items: center; }
  .bar h2 { margin: 0; font-size: 18px; }
  .card { background: #0d1117; border: 1px solid #30363d; border-radius: 8px; padding: 12px 16px; display: flex; flex-direction: column; gap: 10px; }
  .card h3 { margin: 0; font-size: 15px; color: #58a6ff; }
  .card p { margin: 0; }
  .warn-card { border-color: #d29922; }
  .row { display: flex; gap: 10px; align-items: center; flex-wrap: wrap; }
  .mrow { padding: 6px 0; border-top: 1px solid #21262d; }
  .mrow:first-of-type { border-top: none; }
  .mname { min-width: 220px; }
  .spacer { flex: 1; }
  .badge { font-size: 12px; padding: 2px 10px; border-radius: 10px; border: 1px solid #30363d; }
  .badge.on { color: #3fb950; border-color: #3fb950; }
  .badge.warn { color: #d29922; border-color: #d29922; }
  .badge.off { color: #8b949e; }
  input[type="text"] { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 8px; flex: 1; min-width: 260px; }
  input[type="number"] { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 8px; width: 70px; }
  select { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 8px; }
  button { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 14px; cursor: pointer; }
  button.primary { background: #238636; border-color: #2ea043; color: white; }
  button:disabled { opacity: 0.5; cursor: default; }
  .muted { color: #8b949e; font-size: 13px; margin: 0; }
  .warn-text { color: #d29922; font-size: 13px; margin: 0; }
  .inline-error { color: #f85149; font-size: 13px; margin: 0; }
  .error-card { background: #161b22; border: 1px solid #f85149; border-radius: 8px; padding: 12px 16px; }
  .repair-panel { margin: 0 0 6px 0; padding: 10px 12px; background: #161b22; border: 1px solid #21262d; border-radius: 6px; display: flex; flex-direction: column; gap: 10px; }
  .repair-db { display: flex; flex-direction: column; gap: 6px; }
  .db-name { font-size: 13px; }
  .chip { font-size: 12px; padding: 2px 10px; border-radius: 10px; border: 1px solid #30363d; }
  .chip.tracked { color: #3fb950; border-color: #3fb950; }
  .chip.untracked { color: #8b949e; }
  .repair-results { display: flex; flex-direction: column; gap: 6px; }
</style>
