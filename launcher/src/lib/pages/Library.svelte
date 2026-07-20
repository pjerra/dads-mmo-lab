<script lang="ts">
  import { onMount } from "svelte";
  import {
    gamesCatalog,
    gamesStart,
    gamesStop,
    gamesRemove,
    urlInstall,
    type TitleInfo,
    type TermEvent,
  } from "$lib/api";
  import { applyEvent } from "$lib/terminal-state";
  import Terminal from "$lib/Terminal.svelte";
  import InstallTerminal from "$lib/InstallTerminal.svelte";
  import { termBuf, beginRun, clearBuf, installStore } from "$lib/term-store.svelte";
  import { featureLocked, LOCKED_HINT } from "$lib/features.svelte";

  let catalog: TitleInfo[] = $state([]);
  let loadError: string | null = $state(null);
  let actionError: string | null = $state(null);
  let note: string | null = $state(null);

  // Start/stop, in flight for this id (existing pattern).
  let busyId: string | null = $state(null);
  const buf = termBuf("library");

  // Typed-confirm remove: which row is armed, its input, and whether the
  // gamesRemove stream is actually running.
  let removingId: string | null = $state(null);
  let removeInput = $state("");
  let removeBusy = $state(false);
  // Batch 3 F13c: keep the ~6 GB client-data volume for a faster reinstall.
  let removeKeepData = $state(false);
  // Batch 6 B: also delete the AzerothCore/MySQL server images (~3-5 GB).
  let removeImages = $state(false);

  // Install: which title's panel is shown, and whether its session is still
  // active. This lives in installStore (term-store.svelte.ts), NOT local
  // $state -- Library.svelte is destroyed on nav-away, but a RUNNING
  // interactive install (and the backend's single global install slot) keeps
  // going regardless, and the transcript already lived in installStore.text.
  // Gating the panel on local state meant nav-away-and-back left the panel
  // gone, the reply input (the only channel for gamesInstallInput) and
  // Cancel unreachable, with no way back short of an app restart. Reading
  // installStore.id/.running/.nonce here instead means the panel, reply
  // input and Cancel all survive nav intact.
  // installStore.nonce is bumped on every startInstall() so the {#key} block
  // below always remounts InstallTerminal, even when re-installing the same
  // title id after a failed/cancelled run (installStore.id alone wouldn't
  // change -> no remount -> onMount(run) never re-fires -> no exit event ->
  // installStore.running stuck true -> every control disabled forever until
  // nav-away).

  // Install session OR a remove stream blocks the other mutating actions
  // (start/stop, arm/confirm remove). installStore is also the backing
  // store for Tools.svelte's tool:-prefixed sessions (Round Q) -- a running
  // Unbound install must not soft-lock Library's start/stop/remove for its
  // 30-90 minute duration (those never touch the install slot), so only a
  // Library-owned (non "tool:"-prefixed) install session counts here.
  const busy = $derived(
    busyId !== null || removeBusy || (installStore.running && !installStore.id?.startsWith("tool:")),
  );
  // STARTING a new install is different: games_install and tool_install
  // share the backend's single InstallSlot, so the Install button must be
  // blocked by ANY running session -- tool-owned included. Review-caught:
  // gating it on the filtered `busy` above would let a mid-Unbound Install
  // click clobber installStore out from under the live session (orphaning
  // its reply/Cancel UI) and then BUSY-fail anyway.
  const installBlocked = $derived(busy || installStore.running);

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
    beginRun("library");
    try {
      const run = action === "start" ? gamesStart : gamesStop;
      await run(id, (e) => {
        buf.term = applyEvent(buf.term, e);
      });
    } catch (e) {
      const err = e as { code?: string; message?: string; hint?: string };
      buf.term = applyEvent(buf.term, {
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
    removeKeepData = false;
    removeImages = false;
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
    beginRun("library");
    let sawDone = false;
    let streamErr: { message?: string; hint?: string } | null = null;
    let outcomeErr: unknown = null;
    try {
      await gamesRemove(
        id,
        (e: TermEvent) => {
          buf.term = applyEvent(buf.term, e);
          if (e.event === "done") {
            sawDone = true;
          } else if (e.event === "error") {
            streamErr = (e as { error?: { message?: string; hint?: string } }).error ?? {};
          }
        },
        removeKeepData,
        removeImages,
      );
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
    // Always a FRESH session (the Install button that calls this is only
    // rendered while !busy, i.e. no session is already running -- see the
    // {#if !busy} guard around it below), so resetting the transcript here
    // is safe: resuming an already-running session never re-enters this
    // function, it just reads the store's existing state on remount.
    installStore.id = id;
    installStore.nonce += 1;
    installStore.running = true;
    installStore.exitCode = null;
    actionError = null;
    note = null;
    installStore.text = "";
  }

  // --- Install from URL (Batch 4 F16) --------------------------------------
  // Streams the EXISTING interactive `dml run <url>` CLI arm through the
  // same InstallTerminal/install-slot machinery as regular game installs.
  // The id carries the URL ("url:<https link>") so the runner closure and a
  // nav-away remount both know what to (re)connect to.
  let urlValue = $state("");
  let urlArmed = $state(false);
  let urlConfirmInput = $state("");

  function armUrlInstall() {
    urlArmed = true;
    urlConfirmInput = "";
  }
  function cancelUrlArm() {
    urlArmed = false;
    urlConfirmInput = "";
  }
  function confirmUrlInstall() {
    if (urlConfirmInput !== "install") return;
    urlArmed = false;
    urlConfirmInput = "";
    startInstall(`url:${urlValue.trim()}`);
  }

  // installStore.running is flipped false directly by InstallTerminal's own
  // exit/error handling (so it stays truthful even if the instance that
  // witnesses the exit event is an orphaned one from before a nav-away) --
  // this callback's only remaining job is refreshing the catalog so the
  // "installed" flag updates promptly while this page is mounted.
  function onInstallExit(_code: number) {
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
              <button
                disabled={busy || featureLocked("title-remove")}
                title={featureLocked("title-remove") ? LOCKED_HINT : undefined}
                onclick={() => armRemove(t.id)}
              >
                Remove
              </button>
            {/if}
          </div>
        </div>
        {#if removingId === t.id}
          <div class="remove-confirm">
            <p>Removing deletes the server and its data. Backups under ~/.dml are kept. Type the title id to confirm:</p>
            <label class="keep-data">
              <input type="checkbox" bind:checked={removeKeepData} disabled={busy} />
              Keep downloaded game data (~6 GB) for faster reinstall
            </label>
            <label class="keep-data">
              <input type="checkbox" bind:checked={removeImages} disabled={busy} />
              Also delete downloaded server images (~3-5 GB) — slower to reinstall
            </label>
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
            {#if !installBlocked}
              {#if t.script_available}
                <button
                  class="primary"
                  disabled={featureLocked("title-install")}
                  title={featureLocked("title-install") ? LOCKED_HINT : undefined}
                  onclick={() => startInstall(t.id)}
                >
                  Install
                </button>
              {:else}
                <button disabled title="Re-run cli/dev-install.ps1 to ship installer scripts">Install</button>
              {/if}
            {/if}
          </div>
        </div>
      </div>
    {/each}
  </div>

  <h3>Install from URL / community titles</h3>
  <div class="card url-card">
    <p class="muted">
      Paste a project's git link (https) to install a community-made server that follows the
      DML layout (an install.sh at the repo root).
    </p>
    <p class="url-warn">
      This runs the project's own install script on your machine — only use sources you
      trust.
    </p>
    <div class="row">
      <input
        type="text"
        placeholder="https://github.com/user/some-game-server.git"
        bind:value={urlValue}
        oninput={cancelUrlArm}
        disabled={installBlocked}
      />
      {#if !urlArmed}
        <button
          class="primary"
          disabled={installBlocked || !urlValue.trim().startsWith("https://") || featureLocked("title-url-install")}
          title={featureLocked("title-url-install") ? LOCKED_HINT : undefined}
          onclick={armUrlInstall}
        >
          Install
        </button>
      {/if}
    </div>
    {#if urlArmed}
      <div class="remove-confirm">
        <p>About to run the installer from {urlValue.trim()}. Type "install" to confirm:</p>
        <div class="row">
          <input type="text" placeholder="install" bind:value={urlConfirmInput} />
          <button
            class="primary"
            disabled={urlConfirmInput !== "install" || installBlocked}
            onclick={confirmUrlInstall}
          >
            Run installer
          </button>
          <button onclick={cancelUrlArm}>Cancel</button>
        </div>
      </div>
    {/if}
  </div>

  {#if installStore.id && !installStore.id.startsWith("tool:")}
    {#key installStore.nonce}
      {#if installStore.id.startsWith("url:")}
        <InstallTerminal
          id={installStore.id}
          runner={(id, onEvent) => urlInstall(id.slice("url:".length), onEvent)}
          lockFlag="title-url-install"
          onExit={onInstallExit}
        />
      {:else}
        <InstallTerminal id={installStore.id} onExit={onInstallExit} />
      {/if}
    {/key}
  {/if}

  {#if buf.show}
    <Terminal state={buf.term} onclear={() => clearBuf("library")} logName="dml-library" />
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
  .url-card { max-width: 640px; }
  .url-warn { margin: 0; font-size: 13px; font-weight: 600; color: #f85149; }
  .keep-data { display: flex; gap: 6px; align-items: center; font-size: 13px; color: #8b949e; }
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
