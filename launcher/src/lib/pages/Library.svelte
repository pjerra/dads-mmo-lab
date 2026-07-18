<script lang="ts">
  import { onMount } from "svelte";
  import { gamesCatalog, gamesStart, gamesStop, gamesRemove, type TitleInfo, type TermEvent } from "$lib/api";
  import { applyEvent, initialTermState, type TermState } from "$lib/terminal-state";
  import Terminal from "$lib/Terminal.svelte";
  import InstallTerminal from "$lib/InstallTerminal.svelte";

  let catalog: TitleInfo[] = $state([]);
  let loadError: string | null = $state(null);
  let actionError: string | null = $state(null);
  let note: string | null = $state(null);

  // Start/stop, in flight for this id (existing pattern).
  let busyId: string | null = $state(null);
  let term: TermState = $state(initialTermState());
  let showTerm = $state(false);

  // Typed-confirm remove: which row is armed, its input, and whether the
  // gamesRemove stream is actually running.
  let removingId: string | null = $state(null);
  let removeInput = $state("");
  let removeBusy = $state(false);

  // Install: which title's panel is shown, and whether its session is
  // still active (drives the cross-action busy gate below).
  let installId: string | null = $state(null);
  let installRunning = $state(false);

  // Install session OR a remove stream blocks the other mutating actions
  // (start/stop, arm/confirm remove, open a new install).
  const busy = $derived(busyId !== null || removeBusy || installRunning);

  const installed = $derived(catalog.filter((t) => t.installed));
  const available = $derived(catalog.filter((t) => !t.installed));

  async function refresh() {
    try {
      catalog = await gamesCatalog();
      loadError = null;
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      loadError = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    }
  }
  onMount(refresh);

  function showActionErr(e: unknown) {
    const err = e as { message?: string; hint?: string };
    actionError = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
  }

  async function act(id: string, action: "start" | "stop") {
    busyId = id;
    actionError = null;
    note = null;
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

  function armRemove(id: string) {
    removingId = id;
    removeInput = "";
  }
  function cancelRemoveArm() {
    removingId = null;
    removeInput = "";
  }

  function confirmRemove(id: string) {
    if (removeInput !== id) return;
    removingId = null;
    removeInput = "";
    return runRemove(id);
  }

  // Same sawDone/streamErr contract as ModuleManager's runStream: the
  // outcome is derived from events captured in the callback, then applied
  // AFTER the trailing refresh() -- the streaming promise resolves even
  // when the underlying CLI step failed.
  async function runRemove(id: string) {
    removeBusy = true;
    actionError = null;
    note = null;
    showTerm = true;
    term = initialTermState();
    let sawDone = false;
    let streamErr: { message?: string; hint?: string } | null = null;
    let outcomeErr: unknown = null;
    try {
      await gamesRemove(id, (e: TermEvent) => {
        term = applyEvent(term, e);
        if (e.event === "done") {
          sawDone = true;
        } else if (e.event === "error") {
          streamErr = (e as { error?: { message?: string; hint?: string } }).error ?? {};
        }
      });
    } catch (e) {
      outcomeErr = e;
    } finally {
      removeBusy = false;
      await refresh();
      if (outcomeErr) showActionErr(outcomeErr);
      else if (streamErr) showActionErr(streamErr);
      else if (sawDone) note = `Removed ${id}.`;
    }
  }

  function startInstall(id: string) {
    installId = id;
    installRunning = true;
    actionError = null;
    note = null;
  }

  function onInstallExit(_code: number) {
    installRunning = false;
    void refresh();
  }
</script>

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
  {:else if catalog.length === 0}
    <p class="muted">No titles found.</p>
  {/if}
  {#if actionError}
    <div class="error-card"><p>{actionError}</p></div>
  {/if}
  {#if note}<p class="muted">{note}</p>{/if}

  <h3>Installed</h3>
  {#if installed.length === 0}
    <p class="muted">No titles installed yet.</p>
  {/if}
  <div class="cards">
    {#each installed as t (t.id)}
      <div class="card">
        <div class="card-row">
          <div class="card-title">
            <span class="dot {t.running === 'running' ? 'on' : 'off'}"></span>
            {t.name}
          </div>
          <div class="card-actions">
            {#if t.running === "running"}
              <button disabled={busy} onclick={() => act(t.id, "stop")}>Stop</button>
            {:else}
              <button class="primary" disabled={busy} onclick={() => act(t.id, "start")}>Start</button>
            {/if}
            {#if removingId !== t.id}
              <button disabled={busy} onclick={() => armRemove(t.id)}>Remove</button>
            {/if}
          </div>
        </div>
        {#if removingId === t.id}
          <div class="remove-confirm">
            <p>Removing deletes the server and its data. Backups under ~/.dml are kept. Type the title id to confirm:</p>
            <div class="row">
              <input type="text" placeholder={t.id} bind:value={removeInput} />
              <button disabled={removeInput !== t.id || busy} onclick={() => confirmRemove(t.id)}>Remove</button>
              <button onclick={cancelRemoveArm}>Cancel</button>
            </div>
          </div>
        {/if}
      </div>
    {/each}
  </div>

  <h3>Available titles</h3>
  {#if available.length === 0}
    <p class="muted">No available titles.</p>
  {/if}
  <div class="cards">
    {#each available as t (t.id)}
      <div class="card">
        <div class="card-row">
          <div class="card-title">{t.name}</div>
          <div class="card-actions">
            {#if !busy}
              {#if t.script_available}
                <button class="primary" onclick={() => startInstall(t.id)}>Install</button>
              {:else}
                <button disabled title="Re-run cli/dev-install.ps1 to ship installer scripts">Install</button>
              {/if}
            {/if}
          </div>
        </div>
      </div>
    {/each}
  </div>

  {#if installId}
    {#key installId}
      <InstallTerminal id={installId} onExit={onInstallExit} />
    {/key}
  {/if}

  {#if showTerm}
    <Terminal state={term} />
  {/if}
</section>

<style>
  .content { padding: 20px 24px; overflow-y: auto; display: flex; flex-direction: column; gap: 16px; }
  .bar { display: flex; justify-content: space-between; align-items: center; }
  .bar h2 { margin: 0; font-size: 18px; }
  h3 { margin: 0; font-size: 15px; color: #58a6ff; }
  .cards { display: flex; flex-wrap: wrap; gap: 12px; }
  .card {
    background: #0d1117;
    border: 1px solid #30363d;
    border-radius: 8px;
    padding: 14px 16px;
    min-width: 280px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .card-row { display: flex; justify-content: space-between; align-items: center; gap: 16px; }
  .card-title { display: flex; align-items: center; gap: 8px; font-weight: 600; }
  .card-actions { display: flex; gap: 8px; }
  .dot { width: 9px; height: 9px; border-radius: 50%; display: inline-block; }
  .dot.on { background: #3fb950; }
  .dot.off { background: #6e7681; }
  .remove-confirm { border-top: 1px solid #21262d; padding-top: 10px; display: flex; flex-direction: column; gap: 8px; }
  .remove-confirm p { margin: 0; font-size: 13px; color: #d29922; }
  .row { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  .row input[type="text"] {
    background: #21262d;
    color: #c9d1d9;
    border: 1px solid #30363d;
    border-radius: 6px;
    padding: 6px 8px;
    flex: 1;
    min-width: 160px;
  }
  button { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 14px; cursor: pointer; }
  button.primary { background: #238636; border-color: #2ea043; color: white; }
  button:disabled { opacity: 0.5; cursor: default; }
  .muted { color: #8b949e; margin: 0; }
  .error-card { background: #161b22; border: 1px solid #f85149; border-radius: 8px; padding: 12px 16px; }
</style>
