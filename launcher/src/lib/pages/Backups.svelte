<script lang="ts">
  import { onMount } from "svelte";
  import { wowBackupCreate, wowBackupList, wowBackupDelete, wowBackupRestore, type BackupInfo } from "$lib/api";
  import { applyEvent, initialTermState, type TermState } from "$lib/terminal-state";
  import Terminal from "$lib/Terminal.svelte";
  import { restartState } from "$lib/restart-state.svelte";

  let backups: BackupInfo[] = $state([]);
  let error: string | null = $state(null);
  let note: string | null = $state(null);
  let busy = $state(false);          // list/delete request-response ops
  let streaming = $state(false);     // create/restore streaming ops
  let confirming: { kind: "restore" | "delete"; file: string } | null = $state(null);
  let includeWorld = $state(false);

  let term: TermState = $state(initialTermState());
  let showTerm = $state(false);

  function showErr(e: unknown) {
    const err = e as { message?: string; hint?: string };
    error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
  }

  function human(n: number): string {
    if (n >= 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
    if (n >= 1024) return `${(n / 1024).toFixed(0)} KB`;
    return `${n} B`;
  }

  async function refresh() {
    error = null; confirming = null;
    try { backups = await wowBackupList(); } catch (e) { showErr(e); }
  }
  onMount(refresh);

  // Streaming outcomes derive from done/error EVENTS, never promise
  // resolution -- streaming promises resolve even when the CLI fails.
  async function backupNow() {
    streaming = true; error = null; note = null; showTerm = true; term = initialTermState();
    let doneFile: string | null = null; let doneSize = 0;
    let streamErr: { message?: string; hint?: string } | null = null;
    let outcomeErr: unknown = null;
    try {
      await wowBackupCreate((e) => {
        term = applyEvent(term, e);
        if (e.event === "done") {
          const d = e.data as { file?: string; size?: number } | undefined;
          doneFile = d?.file ?? null; doneSize = d?.size ?? 0;
        } else if (e.event === "error") {
          streamErr = (e as { error?: { message?: string; hint?: string } }).error ?? {};
        }
      }, includeWorld);
    } catch (e) { outcomeErr = e; }
    finally {
      streaming = false;
      await refresh();
      if (outcomeErr) showErr(outcomeErr);
      else if (streamErr) showErr(streamErr);
      else if (doneFile) note = `Backed up — ${doneFile} (${human(doneSize)}).`;
    }
  }

  async function restoreBackup(file: string) {
    if (confirming?.kind !== "restore" || confirming?.file !== file) {
      confirming = { kind: "restore", file };
      return;
    }
    confirming = null;
    streaming = true; restartState.restarting = true; error = null; note = null; showTerm = true; term = initialTermState();
    let safety: string | null = null; let sawDone = false;
    let streamErr: { message?: string; hint?: string } | null = null;
    let outcomeErr: unknown = null;
    try {
      await wowBackupRestore(file, (e) => {
        term = applyEvent(term, e);
        if (e.event === "done") {
          sawDone = true;
          const d = e.data as { safety_backup?: string } | undefined;
          safety = d?.safety_backup ?? null;
        } else if (e.event === "error") {
          streamErr = (e as { error?: { message?: string; hint?: string } }).error ?? {};
        }
      });
    } catch (e) { outcomeErr = e; }
    finally {
      restartState.restarting = false;
      streaming = false;
      await refresh();
      if (outcomeErr) showErr(outcomeErr);
      else if (streamErr) showErr(streamErr);
      else if (sawDone) note = `Restored — the server is starting back up. Pre-restore state saved as ${safety ?? "a safety backup"}.`;
    }
  }

  async function deleteBackup(file: string) {
    if (confirming?.kind !== "delete" || confirming?.file !== file) {
      confirming = { kind: "delete", file };
      return;
    }
    confirming = null;
    busy = true; error = null; note = null;
    try { await wowBackupDelete(file); await refresh(); }
    catch (e) { showErr(e); } finally { busy = false; }
  }
</script>

<section class="content">
  <header class="bar">
    <h2>Backups</h2>
    <button onclick={refresh} disabled={busy || streaming}>Refresh</button>
  </header>

  <p class="muted">Snapshots of every character, account and bot. Restoring rolls ALL of them back to that moment.</p>

  {#if error}<div class="error-card"><p>{error}</p></div>{/if}
  {#if note}<p class="muted">{note}</p>{/if}

  <div class="row">
    <button class="primary" onclick={backupNow} disabled={busy || streaming || restartState.restarting}>
      Back up now
    </button>
    <span class="muted">Works while the server is running.</span>
  </div>
  <label class="row">
    <input type="checkbox" bind:checked={includeWorld} disabled={busy || streaming} />
    Include world data (bigger file — recommended before installing modules)
  </label>
  <p class="muted">Full backups share the keep-10 pool with regular ones, and restoring an older full backup while a module is still installed re-applies that module's SQL at the next server start.</p>

  {#if backups.length === 0}
    <p class="muted">No backups yet.</p>
  {:else}
    <div class="card">
      {#each backups as b (b.file)}
        <div class="row brow">
          <span>{b.created} <span class="muted">({human(b.size)}{b.file.includes("-prerestore") ? " · safety backup" : ""}{b.world ? " · includes world" : ""})</span></span>
          <button onclick={() => restoreBackup(b.file)} disabled={busy || streaming || restartState.restarting}>
            {confirming?.kind === "restore" && confirming?.file === b.file
              ? `This rolls EVERY character back to ${b.created} and restarts the server — sure?`
              : "Restore"}
          </button>
          <button onclick={() => deleteBackup(b.file)} disabled={busy || streaming}>
            {confirming?.kind === "delete" && confirming?.file === b.file ? "Delete this backup — sure?" : "Delete"}
          </button>
        </div>
      {/each}
    </div>
  {/if}

  {#if showTerm}
    <Terminal state={term} />
  {/if}
</section>

<style>
  .content { padding: 20px 24px; overflow-y: auto; display: flex; flex-direction: column; gap: 12px; }
  .bar { display: flex; justify-content: space-between; align-items: center; }
  .bar h2 { margin: 0; font-size: 18px; }
  .card { background: #0d1117; border: 1px solid #30363d; border-radius: 8px; padding: 8px 16px; }
  .row { display: flex; gap: 10px; align-items: center; flex-wrap: wrap; }
  .brow { padding: 6px 0; }
  button { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 14px; cursor: pointer; }
  button.primary { background: #238636; border-color: #2ea043; color: white; }
  button:disabled { opacity: 0.5; cursor: default; }
  .muted { color: #8b949e; font-size: 13px; margin: 0; }
  .error-card { background: #161b22; border: 1px solid #f85149; border-radius: 8px; padding: 12px 16px; }
</style>
