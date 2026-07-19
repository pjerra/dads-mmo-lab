<script lang="ts">
  import { onMount } from "svelte";
  import {
    wowLan,
    dmlDoctor,
    toolInstall,
    openShell,
    detectLanIp,
    type LanAction,
    type ToolName,
  } from "$lib/api";
  import { featureLocked, LOCKED_HINT } from "$lib/features.svelte";
  import { installStore } from "$lib/term-store.svelte";
  import InstallTerminal from "$lib/InstallTerminal.svelte";
  import {
    autoShutdown,
    autoShutdownLabel,
    setAutoShutdownEnabled,
  } from "$lib/auto-shutdown.svelte";
  import { toolPrefs, setKeepAwakePref, setLanAutoRefreshPref } from "$lib/tool-prefs.svelte";
  import { setKeepAwake } from "$lib/api";
  import { serverStatus } from "$lib/server-status.svelte";

  function fmtErr(e: unknown): string {
    const err = e as { message?: string; hint?: string };
    return `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
  }

  // --- LAN play ------------------------------------------------------------
  let lanIp = $state("");
  let lanOutput: string | null = $state(null);
  let lanError: string | null = $state(null);
  let lanBusy = $state(false);
  // Which button is armed for its second (confirming) click -- one shared
  // field for all three actions, same "click again to confirm" pattern as
  // GMTools' level/gold/at-login controls.
  let lanConfirm: "on" | "off" | "refresh" | null = $state(null);

  async function lanStatus() {
    lanBusy = true;
    lanError = null;
    try {
      lanOutput = await wowLan("status");
    } catch (e) {
      lanError = fmtErr(e);
    } finally {
      lanBusy = false;
    }
  }

  onMount(() => {
    lanStatus();
    // Prefill only -- the field stays editable, and a failed detect just
    // leaves it blank rather than erroring the card.
    detectLanIp()
      .then((ip) => {
        if (ip) lanIp = ip;
      })
      .catch(() => {});
  });

  function armLan(action: "on" | "off" | "refresh") {
    if (lanConfirm !== action) {
      lanConfirm = action;
      return;
    }
    lanConfirm = null;
    runLan(action);
  }

  async function runLan(action: LanAction) {
    lanBusy = true;
    lanError = null;
    try {
      lanOutput = await wowLan(action, action === "off" ? undefined : lanIp);
    } catch (e) {
      lanError = fmtErr(e);
    } finally {
      lanBusy = false;
    }
  }

  // Keep-awake pref changes take effect immediately when the server is
  // already online -- transition-driven engagement alone would otherwise wait
  // for the next stop/start cycle.
  function onKeepAwakeToggle(on: boolean) {
    setKeepAwakePref(on);
    if (!on && serverStatus.keepAwakeActive) {
      setKeepAwake(false)
        .then(() => (serverStatus.keepAwakeActive = false))
        .catch(() => {});
    } else if (on && !featureLocked("keep-awake") && serverStatus.detail?.verdict === "online") {
      setKeepAwake(true)
        .then(() => (serverStatus.keepAwakeActive = true))
        .catch(() => {});
    }
  }

  // --- Wrath Unbound addon ---------------------------------------------
  let unboundNote: string | null = $state(null);
  let uninstallArmed = $state(false);
  let uninstallInput = $state("");

  // installStore is the single global interactive-install slot shared with
  // Library's game installs (same backend InstallSlot) -- Tools' own
  // Install/Uninstall buttons must stay disabled while ANY session (game or
  // tool) is running, since starting a second one would just hit BUSY.
  const toolBusy = $derived(installStore.running);
  // The panel only renders for a session THIS page started (installStore.id
  // is "tool:<name>") -- a running Library game install must not render its
  // terminal here (mirrors the one-line exclusion added to Library's own
  // gate so a tool session can't render there either).
  const toolInstallId = $derived(
    installStore.id?.startsWith("tool:") ? installStore.id : null,
  );

  function toolFromId(id: string): ToolName {
    return id.slice("tool:".length) as ToolName;
  }

  function startToolInstall(tool: ToolName) {
    installStore.id = `tool:${tool}`;
    installStore.nonce += 1;
    installStore.running = true;
    installStore.exitCode = null;
    installStore.text = "";
    unboundNote = null;
  }

  function armUninstall() {
    uninstallArmed = true;
    uninstallInput = "";
  }
  function cancelUninstallArm() {
    uninstallArmed = false;
    uninstallInput = "";
  }
  function confirmUninstall() {
    if (uninstallInput !== "unbound") return;
    uninstallArmed = false;
    uninstallInput = "";
    startToolInstall("unbound-remove");
  }

  function onToolInstallExit(code: number) {
    unboundNote = code === 0 ? "Finished." : `Exited with code ${code}.`;
  }

  // --- Doctor ---------------------------------------------------------
  let doctorOutput: string | null = $state(null);
  let doctorError: string | null = $state(null);
  let doctorBusy = $state(false);

  async function runDoctor() {
    doctorBusy = true;
    doctorError = null;
    try {
      doctorOutput = await dmlDoctor();
    } catch (e) {
      doctorError = fmtErr(e);
    } finally {
      doctorBusy = false;
    }
  }

  // --- DML shell -----------------------------------------------------
  let shellError: string | null = $state(null);
  let shellBusy = $state(false);

  async function launchShell() {
    shellBusy = true;
    shellError = null;
    try {
      await openShell();
    } catch (e) {
      shellError = fmtErr(e);
    } finally {
      shellBusy = false;
    }
  }
</script>

<section class="content">
  <header class="bar">
    <h2>Tools</h2>
  </header>

  <div class="card">
    <h3>LAN play</h3>
    <p class="muted">
      Points the realmlist at this PC's LAN address so other devices on your network can
      connect to your server. Other PCs still need the Windows firewall/portproxy rule
      from the DML install to actually reach it — the output below prints the specifics
      for your setup.
    </p>
    {#if lanError}<p class="inline-error">{lanError}</p>{/if}
    <div class="row">
      <input
        type="text"
        placeholder="192.168.1.50"
        bind:value={lanIp}
        oninput={() => (lanConfirm = null)}
        disabled={lanBusy}
      />
      <button
        class="primary"
        onclick={() => armLan("on")}
        disabled={lanBusy || !lanIp.trim() || featureLocked("lan-play")}
        title={featureLocked("lan-play") ? LOCKED_HINT : undefined}
      >
        {lanConfirm === "on" ? "Enable at this address — sure?" : "Enable"}
      </button>
      <button
        onclick={() => armLan("off")}
        disabled={lanBusy || featureLocked("lan-play")}
        title={featureLocked("lan-play") ? LOCKED_HINT : undefined}
      >
        {lanConfirm === "off" ? "Disable — back to localhost?" : "Disable"}
      </button>
      <button
        onclick={() => armLan("refresh")}
        disabled={lanBusy || !lanIp.trim() || featureLocked("lan-play")}
        title={featureLocked("lan-play") ? LOCKED_HINT : undefined}
      >
        {lanConfirm === "refresh" ? "Re-apply the rule — sure?" : "Re-apply"}
      </button>
      <button onclick={lanStatus} disabled={lanBusy}>Refresh</button>
    </div>
    <pre class="usage">{lanOutput ?? "No status yet."}</pre>
    <div class="pref-rows">
      <label
        class="toggle"
        title={featureLocked("keep-awake") ? LOCKED_HINT : undefined}
      >
        <input
          type="checkbox"
          checked={toolPrefs.keepAwake}
          disabled={featureLocked("keep-awake")}
          onchange={(e) => onKeepAwakeToggle(e.currentTarget.checked)}
        />
        Keep this PC awake while the server is online
      </label>
      <label
        class="toggle"
        title={featureLocked("lan-auto-refresh") ? LOCKED_HINT : undefined}
      >
        <input
          type="checkbox"
          checked={toolPrefs.lanAutoRefresh}
          disabled={featureLocked("lan-auto-refresh")}
          onchange={(e) => setLanAutoRefreshPref(e.currentTarget.checked)}
        />
        Re-point the LAN address automatically after every server start
      </label>
      {#if serverStatus.lanNotice}<p class="notice">{serverStatus.lanNotice}</p>{/if}
    </div>
  </div>

  <div class="card">
    <h3>Auto-shutdown</h3>
    <p class="muted">
      Stops the server automatically (characters saved, graceful stop) a few seconds after
      you close the WoW game window — so the PC isn't left running a world nobody is in.
      It arms itself when WoW starts and does nothing while WoW was never opened.
    </p>
    {#if autoShutdown.error}<p class="inline-error">{autoShutdown.error}</p>{/if}
    <div class="row">
      <label
        class="toggle"
        title={featureLocked("auto-shutdown") ? LOCKED_HINT : undefined}
      >
        <input
          type="checkbox"
          checked={autoShutdown.enabled}
          disabled={featureLocked("auto-shutdown")}
          onchange={(e) => setAutoShutdownEnabled(e.currentTarget.checked)}
        />
        Stop the server when WoW closes
      </label>
      <span class="muted">{autoShutdownLabel(autoShutdown.enabled, autoShutdown.state)}</span>
    </div>
    {#if autoShutdown.notice}<p class="notice">{autoShutdown.notice}</p>{/if}
  </div>

  <div class="card">
    <h3>Wrath Unbound addon</h3>
    <p class="muted">
      Layers the Wrath Unbound multi-class addon onto this server and force-rebuilds the
      worldserver to match — the worldserver will be down for roughly 30–90 minutes while
      it recompiles. Uninstalling reverses this: it drops the addon's tables and rebuilds
      again, so it takes just as long.
    </p>
    {#if unboundNote}<p class="muted">{unboundNote}</p>{/if}
    <div class="row">
      <button
        class="primary"
        onclick={() => startToolInstall("unbound")}
        disabled={toolBusy || featureLocked("unbound-addon")}
        title={featureLocked("unbound-addon") ? LOCKED_HINT : undefined}
      >
        Install / Update
      </button>
      {#if !uninstallArmed}
        <button
          onclick={armUninstall}
          disabled={toolBusy || featureLocked("unbound-addon")}
          title={featureLocked("unbound-addon") ? LOCKED_HINT : undefined}
        >
          Uninstall
        </button>
      {/if}
    </div>
    {#if uninstallArmed}
      <div class="confirm-row">
        <p class="warn-text">
          This drops the Unbound addon's tables and rebuilds the worldserver again. Type
          "unbound" to confirm:
        </p>
        <div class="row">
          <input type="text" placeholder="unbound" bind:value={uninstallInput} />
          <button class="primary" disabled={uninstallInput !== "unbound" || toolBusy} onclick={confirmUninstall}>
            Uninstall
          </button>
          <button onclick={cancelUninstallArm}>Cancel</button>
        </div>
      </div>
    {/if}

    {#if toolInstallId}
      {#key installStore.nonce}
        <InstallTerminal
          id={toolInstallId}
          runner={(_id, onEvent) => toolInstall(toolFromId(toolInstallId), onEvent)}
          onExit={onToolInstallExit}
        />
      {/key}
    {/if}
  </div>

  <div class="card">
    <h3>Doctor</h3>
    <p class="muted">
      Runs the DML environment checks (Docker, disk space, network, WSL) and reports what
      it finds.
    </p>
    {#if doctorError}<p class="inline-error">{doctorError}</p>{/if}
    <div class="row">
      <button class="primary" onclick={runDoctor} disabled={doctorBusy}>
        {#if doctorBusy}<span class="spinner"></span>Running…{:else}Run{/if}
      </button>
    </div>
    {#if doctorOutput}<pre class="usage">{doctorOutput}</pre>{/if}
  </div>

  <div class="card">
    <h3>DML shell</h3>
    <p class="muted">
      Opens a Windows terminal inside the dml-arch distro (the same shell the CLI runs
      in) — handy for poking around by hand.
    </p>
    {#if shellError}<p class="inline-error">{shellError}</p>{/if}
    <div class="row">
      <button onclick={launchShell} disabled={shellBusy}>Open shell</button>
    </div>
  </div>
</section>

<style>
  .content { padding: 20px 24px; overflow-y: auto; display: flex; flex-direction: column; gap: 16px; }
  .bar { display: flex; justify-content: space-between; align-items: center; }
  .bar h2 { margin: 0; font-size: 18px; }
  .card { background: #0d1117; border: 1px solid #30363d; border-radius: 8px; padding: 14px 16px; display: flex; flex-direction: column; gap: 10px; }
  .card h3 { margin: 0; font-size: 15px; color: #58a6ff; }
  .card p { margin: 0; }
  .row { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  .confirm-row { border-top: 1px solid #21262d; padding-top: 10px; display: flex; flex-direction: column; gap: 8px; }
  .warn-text { color: #d29922; font-size: 13px; margin: 0; }
  input[type="text"] {
    background: #21262d;
    color: #c9d1d9;
    border: 1px solid #30363d;
    border-radius: 6px;
    padding: 6px 8px;
    min-width: 160px;
  }
  button { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 14px; cursor: pointer; }
  button.primary { background: #238636; border-color: #2ea043; color: white; }
  button:disabled { opacity: 0.5; cursor: default; }
  .muted { color: #8b949e; font-size: 13px; margin: 0; }
  .inline-error { color: #f85149; font-size: 13px; margin: 0; }
  .toggle { display: flex; align-items: center; gap: 8px; font-size: 14px; cursor: pointer; }
  .toggle input { accent-color: #238636; }
  .notice { color: #3fb950; font-size: 13px; margin: 0; }
  .pref-rows { border-top: 1px solid #21262d; padding-top: 10px; display: flex; flex-direction: column; gap: 8px; }
  .usage {
    background: #161b22;
    border: 1px solid #21262d;
    border-radius: 6px;
    padding: 8px 10px;
    margin: 0;
    font-size: 12px;
    color: #8b949e;
    white-space: pre-wrap;
    word-break: break-word;
    max-height: 300px;
    overflow-y: auto;
  }
  .spinner {
    width: 12px;
    height: 12px;
    border: 2px solid #30363d;
    border-top-color: #58a6ff;
    border-radius: 50%;
    animation: spin 0.9s linear infinite;
    display: inline-block;
    vertical-align: middle;
    margin-right: 6px;
  }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
