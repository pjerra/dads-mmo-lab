<script lang="ts">
  import { onMount } from "svelte";
  import {
    wowLan,
    wowLanPublicIp,
    wowTailscale,
    dmlDoctor,
    toolInstall,
    openShell,
    detectLanIp,
    realmlistStatus,
    realmlistFix,
    realmlistLock,
    wslconfigRead,
    wslconfigWrite,
    restartWsl,
    generateCompactScript,
    generateMysqlProxyScript,
    wowPortCheck,
    defenderHint,
    zamCacheStatus,
    zamCacheClear,
    wowCacheStatus,
    wowCacheClean,
    type LanAction,
    type ToolName,
    type RealmlistStatus,
    type WslConfigState,
    type PortCheck,
    type CacheEntry,
  } from "$lib/api";
  import { parseLanStatus } from "$lib/transitions";
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
    // Realmlist status wants the LAN address in its expected-set, so it
    // loads after the LAN status answer has landed.
    lanStatus().then(loadRealmlist);
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

  // --- Play over the internet (Batch 4 F15) --------------------------------
  // Guided stepper, option b (no paid services): detected addresses ->
  // manual router port-forwarding checklist -> optional DuckDNS -> apply
  // (realmlist rewrite via `lan --internet on`) -> what-friends-do -> revert.
  // No automatic router configuration (UPnP) on purpose.
  let inetPublicIp: string | null = $state(null);
  let inetPublicIpLoaded = $state(false);
  let inetAddr = $state("");
  let inetBusy = $state(false);
  let inetError: string | null = $state(null);
  let inetOutput: string | null = $state(null);
  let inetConfirm: "apply" | "revert" | null = $state(null);
  let inetApplied: string | null = $state(null);
  let inetCopied = $state(false);

  onMount(() => {
    wowLanPublicIp()
      .then((ip) => {
        inetPublicIp = ip;
        inetPublicIpLoaded = true;
        // Prefill the address field only if the user hasn't typed one.
        if (ip && !inetAddr) inetAddr = ip;
      })
      .catch(() => {
        inetPublicIpLoaded = true;
      });
  });

  function armInet(action: "apply" | "revert") {
    if (inetConfirm !== action) {
      inetConfirm = action;
      return;
    }
    inetConfirm = null;
    void runInet(action);
  }

  async function runInet(action: "apply" | "revert") {
    inetBusy = true;
    inetError = null;
    try {
      if (action === "apply") {
        inetOutput = await wowLan("on", inetAddr.trim(), true);
        inetApplied = inetAddr.trim();
      } else {
        inetOutput = await wowLan("off");
        inetApplied = null;
      }
      // The LAN card's status shares the same realm address -- refresh it.
      void lanStatus();
    } catch (e) {
      inetError = fmtErr(e);
    } finally {
      inetBusy = false;
    }
  }

  async function copyRealmlistLine() {
    try {
      await navigator.clipboard.writeText(`set realmlist ${inetApplied ?? inetAddr.trim()}`);
      inetCopied = true;
      setTimeout(() => (inetCopied = false), 1500);
    } catch {
      // Clipboard unavailable -- the line is still shown for manual copy.
    }
  }

  // --- Play Together over the internet (Tailscale, Batch 5) ----------------
  // Guided stepper: Install -> Log in (show the one-time auth URL) -> shows the
  // 100.x tailnet address -> point the realm at it so friends' clients reach
  // the world server -> friends set realmlist to it. Every mutating step rides
  // the tailscale-play lock; Refresh status is read-only.
  let tsBusy = $state(false);
  let tsError: string | null = $state(null);
  let tsNote: string | null = $state(null);
  let tsInstalled: boolean | null = $state(null);
  let tsConnected = $state(false);
  let tsIp: string | null = $state(null);
  let tsAuthUrl: string | null = $state(null);
  let tsApplied = $state(false);
  let tsCopied = $state(false);
  let tsAuthCopied = $state(false);
  let tsConfirm: "down" | null = $state(null);

  onMount(() => {
    // Best-effort prefill; NOT_INSTALLED is a normal answer, not an error.
    void tsLoadStatus(true);
  });

  async function tsLoadStatus(silent = false) {
    tsBusy = true;
    if (!silent) tsError = null;
    try {
      const s = await wowTailscale("status");
      tsInstalled = true;
      tsConnected = s.connected;
      tsIp = s.ip;
      tsAuthUrl = null;
    } catch (e) {
      const code = (e as { code?: string }).code;
      if (code === "NOT_INSTALLED") {
        tsInstalled = false;
      } else if (!silent) {
        tsError = fmtErr(e);
      }
    } finally {
      tsBusy = false;
    }
  }

  async function tsInstall() {
    tsBusy = true;
    tsError = null;
    tsNote = null;
    try {
      const r = await wowTailscale("install");
      tsInstalled = true;
      tsNote = r.already ? "Tailscale is already installed." : "Tailscale installed.";
    } catch (e) {
      tsError = fmtErr(e);
    } finally {
      tsBusy = false;
    }
  }

  async function tsLogin() {
    tsBusy = true;
    tsError = null;
    tsNote = null;
    tsAuthUrl = null;
    try {
      const r = await wowTailscale("up");
      tsInstalled = true;
      tsConnected = r.connected;
      tsIp = r.ip;
      tsAuthUrl = r.auth_url;
      if (r.connected) tsNote = "Connected to your tailnet.";
    } catch (e) {
      tsError = fmtErr(e);
    } finally {
      tsBusy = false;
    }
  }

  async function tsApplyRealm() {
    if (!tsIp) return;
    tsBusy = true;
    tsError = null;
    tsNote = null;
    try {
      // 100.x is CGNAT, not a private-LAN range, so the realmlist write goes
      // through the --internet path (internet=true) -- same backend as the
      // "Play over the internet" card.
      const out = await wowLan("on", tsIp, true);
      // wow_lan resolves with the CLI's text even on a non-zero exit (a failed
      // realmlist write comes back as an error string, not a thrown promise),
      // so gate the "applied" state + share line on the CLI's own ok marker
      // instead of on the call merely returning.
      if (!out.includes("[ok] LAN play ENABLED")) {
        tsError = out.trim() || "Could not point the realm at your Tailscale address.";
        return;
      }
      tsApplied = true;
      tsNote = `Your realm now points friends at ${tsIp}.`;
      void lanStatus();
    } catch (e) {
      tsError = fmtErr(e);
    } finally {
      tsBusy = false;
    }
  }

  function armTsDown() {
    if (tsConfirm !== "down") {
      tsConfirm = "down";
      return;
    }
    tsConfirm = null;
    void tsDown();
  }

  async function tsDown() {
    tsBusy = true;
    tsError = null;
    tsNote = null;
    try {
      await wowTailscale("down");
      tsConnected = false;
      tsIp = null;
      tsAuthUrl = null;
      tsApplied = false;
      tsNote = "Disconnected from Tailscale.";
    } catch (e) {
      tsError = fmtErr(e);
    } finally {
      tsBusy = false;
    }
  }

  async function tsCopyRealmline() {
    if (!tsIp) return;
    try {
      await navigator.clipboard.writeText(`set realmlist ${tsIp}`);
      tsCopied = true;
      setTimeout(() => (tsCopied = false), 1500);
    } catch {
      // shown for manual copy either way
    }
  }

  async function tsCopyAuth() {
    if (!tsAuthUrl) return;
    try {
      await navigator.clipboard.writeText(tsAuthUrl);
      tsAuthCopied = true;
      setTimeout(() => (tsAuthCopied = false), 1500);
    } catch {
      // shown for manual copy either way
    }
  }

  // --- Game realmlist (Batch 2 F7) ----------------------------------------
  let rl: RealmlistStatus | null = $state(null);
  let rlError: string | null = $state(null);
  let rlBusy = $state(false);

  // The LAN address, when LAN play is on, extends the set of addresses the
  // realmlist may legitimately point at. Parsed from the same status text
  // the LAN card shows.
  const lanInfo = $derived(parseLanStatus(lanOutput ?? ""));
  const lanFixIp = $derived(lanInfo.on && lanInfo.ip && lanInfo.ip !== "127.0.0.1" ? lanInfo.ip : null);
  // $derived.by: inside a closure TS uses rl's declared type -- the inline
  // form gets narrowed to the initial null by straight-line control flow.
  const rlEffective = $derived.by(() => (rl ? (rl.current ?? rl.config_wtf) : null));

  async function loadRealmlist() {
    rlBusy = true;
    rlError = null;
    try {
      rl = await realmlistStatus(lanFixIp ?? undefined);
    } catch (e) {
      rlError = fmtErr(e);
    } finally {
      rlBusy = false;
    }
  }

  async function fixRealmlist(target: string) {
    rlBusy = true;
    rlError = null;
    try {
      rl = await realmlistFix(target, lanFixIp ?? undefined);
    } catch (e) {
      rlError = fmtErr(e);
    } finally {
      rlBusy = false;
    }
  }

  async function lockRealmlist(locked: boolean) {
    rlBusy = true;
    rlError = null;
    try {
      rl = await realmlistLock(locked, lanFixIp ?? undefined);
    } catch (e) {
      rlError = fmtErr(e);
    } finally {
      rlBusy = false;
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

  // --- Disk & performance (Batch 4 F17) --------------------------------
  // .wslconfig editor + restart WSL + shrink-disk script + Defender hint.
  // All Windows-side (native Rust commands); mutating cards ride the
  // disk-tools flag.
  let wc: WslConfigState | null = $state(null);
  let wcMemory = $state("");
  let wcProcessors = $state("");
  let wcError: string | null = $state(null);
  let wcNote: string | null = $state(null);
  let wcBusy = $state(false);
  const cpuCount = typeof navigator !== "undefined" ? (navigator.hardwareConcurrency ?? 0) : 0;

  let wslRestartInput = $state("");
  let wslRestartBusy = $state(false);
  let wslRestartNote: string | null = $state(null);

  let shrinkBusy = $state(false);
  let shrinkPath: string | null = $state(null);
  let shrinkError: string | null = $state(null);

  let defCmd: string | null = $state(null);
  let defCopied = $state(false);

  onMount(() => {
    wslconfigRead()
      .then((s) => {
        wc = s;
        wcMemory = s.memory ?? "";
        wcProcessors = s.processors ?? "";
      })
      .catch((e) => (wcError = fmtErr(e)));
    defenderHint()
      .then((d) => (defCmd = d.command))
      .catch(() => {});
  });

  async function saveWslconfig() {
    wcBusy = true;
    wcError = null;
    wcNote = null;
    try {
      const s = await wslconfigWrite(
        wcMemory.trim() || undefined,
        wcProcessors.trim() || undefined,
      );
      wc = s;
      wcMemory = s.memory ?? "";
      wcProcessors = s.processors ?? "";
      wcNote = "Saved — takes effect after WSL restarts (use the Restart WSL card below).";
    } catch (e) {
      wcError = fmtErr(e);
    } finally {
      wcBusy = false;
    }
  }

  async function runRestartWsl() {
    if (wslRestartInput !== "restart-wsl") return;
    wslRestartInput = "";
    wslRestartBusy = true;
    wcError = null;
    wslRestartNote = null;
    try {
      const r = await restartWsl();
      wslRestartNote = r.stopped_server
        ? "Server stopped gracefully, WSL shut down. The next Start is a cold start."
        : r.stop_attempted
          ? "WSL shut down. The server was running but couldn't be confirmed stopped first — check it from Home after the next Start. The next Start is a cold start."
          : "WSL shut down (no server was running). The next Start is a cold start.";
    } catch (e) {
      wslRestartNote = fmtErr(e);
    } finally {
      wslRestartBusy = false;
    }
  }

  async function makeShrinkScript() {
    shrinkBusy = true;
    shrinkError = null;
    shrinkPath = null;
    try {
      shrinkPath = await generateCompactScript();
    } catch (e) {
      shrinkError = fmtErr(e);
    } finally {
      shrinkBusy = false;
    }
  }

  async function copyDefenderCmd() {
    if (!defCmd) return;
    try {
      await navigator.clipboard.writeText(defCmd);
      defCopied = true;
      setTimeout(() => (defCopied = false), 1500);
    } catch {
      // shown for manual copy either way
    }
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

  // --- Database access / LAN diagnostic (Batch 5) --------------------
  // Read-only port diagnostic + a generated admin PowerShell script that
  // exposes MySQL to the LAN for HeidiSQL. The 3306 exposure is the mutating
  // part -> locked behind port-proxy; the diagnostic itself is read-only.
  let pcData: PortCheck | null = $state(null);
  let pcError: string | null = $state(null);
  let pcBusy = $state(false);
  let pcScriptPath: string | null = $state(null);
  let pcScriptError: string | null = $state(null);
  let pcScriptBusy = $state(false);

  onMount(() => {
    void runPortCheck().catch(() => {});
  });

  async function runPortCheck() {
    pcBusy = true;
    pcError = null;
    try {
      pcData = await wowPortCheck();
    } catch (e) {
      pcError = fmtErr(e);
    } finally {
      pcBusy = false;
    }
  }

  async function makeMysqlScript() {
    pcScriptBusy = true;
    pcScriptError = null;
    pcScriptPath = null;
    try {
      pcScriptPath = await generateMysqlProxyScript(pcData?.db_host_port);
    } catch (e) {
      pcScriptError = fmtErr(e);
    } finally {
      pcScriptBusy = false;
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

  // --- Cache maintenance (Batch 6 C) ---------------------------------------
  // Status + wipe for the two RUNTIME enrichment caches (safe to clear -- they
  // re-download on demand): the Windows zam 3D-model/icon cache and the WSL
  // wowhead item tooltip/icon cache. The committed talent/achievement datasets
  // ship in the binary and are never touched. Only the wipe is locked
  // (cache-maint); reading sizes is read-only.
  let cacheEntries: CacheEntry[] = $state([]);
  let cacheError: string | null = $state(null);
  let cacheBusy = $state(false);
  let cacheNote: string | null = $state(null);
  let cacheConfirm = $state(false);

  function fmtBytes(n: number): string {
    if (!n || n <= 0) return "0 B";
    const units = ["B", "KB", "MB", "GB"];
    let v = n;
    let i = 0;
    while (v >= 1024 && i < units.length - 1) {
      v /= 1024;
      i += 1;
    }
    return `${i === 0 || v >= 10 ? Math.round(v) : v.toFixed(1)} ${units[i]}`;
  }

  async function loadCaches() {
    cacheBusy = true;
    cacheError = null;
    try {
      const [zam, wh] = await Promise.all([zamCacheStatus(), wowCacheStatus()]);
      cacheEntries = [zam, ...wh.caches];
    } catch (e) {
      cacheError = fmtErr(e);
    } finally {
      cacheBusy = false;
    }
  }

  onMount(() => {
    void loadCaches().catch(() => {});
  });

  function armClearCaches() {
    if (!cacheConfirm) {
      cacheConfirm = true;
      return;
    }
    cacheConfirm = false;
    void clearCaches();
  }

  async function clearCaches() {
    cacheBusy = true;
    cacheError = null;
    cacheNote = null;
    try {
      const [zam, wh] = await Promise.all([zamCacheClear(), wowCacheClean()]);
      const freed = (zam.freed_bytes ?? 0) + (wh.freed_bytes ?? 0);
      cacheNote = `Cleared ${fmtBytes(freed)} of cached tooltips, icons and 3D-model data — it rebuilds itself as you browse.`;
      await loadCaches();
    } catch (e) {
      cacheError = fmtErr(e);
    } finally {
      cacheBusy = false;
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
    <h3>Play over the internet</h3>
    <p class="muted">
      Lets friends OUTSIDE your home network connect to your server. No paid services
      needed — you forward two ports on your router and share your address.
    </p>
    {#if inetError}<p class="inline-error">{inetError}</p>{/if}

    <div class="step">
      <strong>1. Your addresses</strong>
      <p class="muted">
        Public address (what friends connect to):
        <code>{inetPublicIpLoaded ? (inetPublicIp ?? "unknown — are you online?") : "detecting…"}</code>
        &nbsp;·&nbsp; LAN address (for the router rule): <code>{lanIp || "unknown"}</code>
      </p>
    </div>

    <div class="step">
      <strong>2. Forward two ports on your router</strong>
      <p class="muted">
        In your router's admin page, forward <strong>TCP 3724</strong> (login) and
        <strong>TCP 8085</strong> (game world) to <code>{lanIp || "this PC's LAN address"}</code>.
        Every router is different — search the web for "<em>your router model</em> port
        forwarding" for the exact clicks.
      </p>
      <p class="warn-text">
        NEVER forward port 3306 (the database) — that would expose your server's data to the
        whole internet. And only share your address with people you trust.
      </p>
    </div>

    <div class="step">
      <strong>3. Optional: a free DuckDNS name</strong>
      <p class="muted">
        Home internet addresses change now and then. A free name from duckdns.org stays the
        same: create an account there, pick a name (e.g. <code>ourfamilywow</code>), and set it
        to your public address. Then use <code>yourname.duckdns.org</code> below instead of the
        raw address.
      </p>
    </div>

    <div class="step">
      <strong>4. Point the realm at your public address</strong>
      <div class="row">
        <input
          type="text"
          placeholder="84.210.13.37 or yourname.duckdns.org"
          bind:value={inetAddr}
          oninput={() => (inetConfirm = null)}
          disabled={inetBusy}
        />
        <button
          class="primary"
          onclick={() => armInet("apply")}
          disabled={inetBusy || !inetAddr.trim() || featureLocked("internet-play")}
          title={featureLocked("internet-play") ? LOCKED_HINT : undefined}
        >
          {inetConfirm === "apply" ? "Share the realm at this address — sure?" : "Apply"}
        </button>
      </div>
    </div>

    <div class="step">
      <strong>5. What your friends do</strong>
      <p class="muted">
        In their WoW client folder, they put this line in
        <code>Data\enUS\realmlist.wtf</code>:
      </p>
      <div class="row">
        <code class="copyline">set realmlist {inetApplied ?? (inetAddr.trim() || "<your address>")}</code>
        <button onclick={copyRealmlistLine} disabled={!(inetApplied ?? inetAddr.trim())}>
          {inetCopied ? "Copied!" : "Copy"}
        </button>
      </div>
      <p class="muted">
        Each friend also needs their own account on your server — create those on the
        Accounts page.
      </p>
    </div>

    <div class="step">
      <strong>6. Done playing over the internet?</strong>
      <div class="row">
        <button
          onclick={() => armInet("revert")}
          disabled={inetBusy || featureLocked("internet-play")}
          title={featureLocked("internet-play") ? LOCKED_HINT : undefined}
        >
          {inetConfirm === "revert" ? "Back to this-PC-only — sure?" : "Revert to local play"}
        </button>
      </div>
    </div>

    {#if inetOutput}<pre class="usage">{inetOutput}</pre>{/if}
  </div>

  <div class="card">
    <h3>Play Together over the internet (Tailscale)</h3>
    <p class="muted">
      The easy way to let far-away friends join — no router port-forwarding. Tailscale gives
      this PC a private address that friends' clients reach directly. Everyone installs the
      free Tailscale app and joins the same "tailnet" (get it at
      <code>tailscale.com/download</code>).
    </p>
    {#if tsError}<p class="inline-error">{tsError}</p>{/if}

    <div class="step">
      <strong>1. Install Tailscale on this server</strong>
      <div class="row">
        <button
          class="primary"
          onclick={tsInstall}
          disabled={tsBusy || featureLocked("tailscale-play")}
          title={featureLocked("tailscale-play") ? LOCKED_HINT : undefined}
        >
          {tsInstalled ? "Re-install / update" : "Install"}
        </button>
        {#if tsInstalled === true}<span class="notice">Installed.</span>{/if}
        {#if tsInstalled === false}<span class="muted">Not installed yet.</span>{/if}
      </div>
    </div>

    <div class="step">
      <strong>2. Log in</strong>
      <p class="muted">
        This starts a one-time login. Click Log in, then open the link it shows on any device
        (your phone is fine) and sign in — Google, Microsoft and GitHub all work.
      </p>
      <div class="row">
        <button
          class="primary"
          onclick={tsLogin}
          disabled={tsBusy || featureLocked("tailscale-play")}
          title={featureLocked("tailscale-play") ? LOCKED_HINT : undefined}
        >
          {tsConnected ? "Re-check login" : "Log in"}
        </button>
        <button onclick={() => tsLoadStatus(false)} disabled={tsBusy}>Refresh status</button>
      </div>
      {#if tsAuthUrl}
        <p class="notice">Open this link on any device and sign in:</p>
        <div class="row">
          <code class="copyline">{tsAuthUrl}</code>
          <button onclick={tsCopyAuth}>{tsAuthCopied ? "Copied!" : "Copy"}</button>
        </div>
      {/if}
    </div>

    <div class="step">
      <strong>3. Your Tailscale address</strong>
      {#if tsConnected && tsIp}
        <p class="rl-state good">Connected — your address is <code>{tsIp}</code>.</p>
        <div class="row">
          <button
            class="primary"
            onclick={tsApplyRealm}
            disabled={tsBusy || featureLocked("tailscale-play")}
            title={featureLocked("tailscale-play") ? LOCKED_HINT : undefined}
          >
            Point my realm at {tsIp}
          </button>
        </div>
        {#if tsApplied}
          <p class="muted">
            Now each friend puts this line in their <code>Data\enUS\realmlist.wtf</code>:
          </p>
          <div class="row">
            <code class="copyline">set realmlist {tsIp}</code>
            <button onclick={tsCopyRealmline}>{tsCopied ? "Copied!" : "Copy"}</button>
          </div>
          <p class="muted">
            Each friend also needs to join your tailnet in their Tailscale app AND have a game
            account (create those on the Accounts page).
          </p>
        {/if}
      {:else}
        <p class="muted">Log in above to get your shareable Tailscale address.</p>
      {/if}
    </div>

    <div class="step">
      <strong>4. Done playing?</strong>
      <div class="row">
        <button
          onclick={armTsDown}
          disabled={tsBusy || featureLocked("tailscale-play")}
          title={featureLocked("tailscale-play") ? LOCKED_HINT : undefined}
        >
          {tsConfirm === "down" ? "Disconnect Tailscale — sure?" : "Disconnect Tailscale"}
        </button>
      </div>
    </div>

    {#if tsNote}<p class="notice">{tsNote}</p>{/if}
  </div>

  <div class="card">
    <h3>Game realmlist</h3>
    <p class="muted">
      Checks which server address your WoW client logs in to (its "realmlist") and fixes
      it with one click. Changes take effect the next time you start the game.
    </p>
    {#if rlError}<p class="inline-error">{rlError}</p>{/if}
    {#if rl}
      {#if !rl.windows_path}
        <p class="muted">
          {rl.client_path
            ? `The stored client folder (${rl.client_path}) isn't on a Windows drive — its realmlist can't be reached from here.`
            : "No client path set — set your WoW client folder on the Modules page first."}
        </p>
      {:else}
        <p class="rl-state" class:good={rl.matches} class:warn={!rl.matches}>
          {#if rl.matches}
            Points at your server ({rlEffective}).
            {#if !rl.current && rl.config_wtf}
              <span class="muted">(via the game's saved settings — realmlist.wtf itself is empty)</span>
            {/if}
          {:else if rlEffective}
            Points at "{rlEffective}" — not your server.
          {:else}
            No realmlist entry found — the game won't know where your server is.
          {/if}
        </p>
        {#if rl.path}
          <p class="muted">File: {rl.path}{rl.readonly ? " (protected, read-only)" : ""}</p>
        {/if}
        <div class="row">
          <button
            class="primary"
            onclick={() => fixRealmlist("127.0.0.1")}
            disabled={rlBusy || featureLocked("realmlist-fix")}
            title={featureLocked("realmlist-fix") ? LOCKED_HINT : undefined}
          >
            Point at this PC (127.0.0.1)
          </button>
          {#if lanFixIp}
            <button
              onclick={() => lanFixIp && fixRealmlist(lanFixIp)}
              disabled={rlBusy || featureLocked("realmlist-fix")}
              title={featureLocked("realmlist-fix") ? LOCKED_HINT : undefined}
            >
              Point at the LAN address ({lanFixIp})
            </button>
          {/if}
          <button onclick={loadRealmlist} disabled={rlBusy}>Re-check</button>
        </div>
        {#if rl.exists}
          <label class="toggle" title={featureLocked("realmlist-fix") ? LOCKED_HINT : undefined}>
            <input
              type="checkbox"
              checked={rl.readonly}
              disabled={rlBusy || featureLocked("realmlist-fix")}
              onchange={(e) => lockRealmlist(e.currentTarget.checked)}
            />
            Protect the file (read-only — stops other launchers from overwriting it)
          </label>
        {/if}
      {/if}
    {:else if rlBusy}
      <p class="muted">Checking…</p>
    {/if}
  </div>

  <div class="card">
    <h3>Database access / LAN diagnostic</h3>
    <p class="muted">
      Checks whether your server's ports are reachable from other PCs on your network, and
      helps you connect HeidiSQL (a free database viewer) to the game's database.
    </p>
    {#if pcError}<p class="inline-error">{pcError}</p>{/if}
    <div class="row">
      <button class="primary" onclick={runPortCheck} disabled={pcBusy}>
        {pcBusy ? "Checking…" : "Re-check"}
      </button>
    </div>

    {#if pcData}
      {#if !pcData.running}
        <p class="muted">The server isn't running — start it from Home, then Re-check.</p>
      {:else}
        <p class="rl-state" class:good={pcData.game_lan_ready} class:warn={!pcData.game_lan_ready}>
          {pcData.game_lan_ready
            ? "Game ports (login + world) are reachable from other PCs on your network."
            : "Game ports are bound to this PC only — other PCs can't reach them yet."}
        </p>
        <ul class="port-list">
          {#each pcData.ports as p (p.name)}
            <li>
              <span class="mono">{p.service}</span>
              {#if p.published}
                <span class="mono">{p.host_ip}:{p.host_port}</span>
                <span class={p.lan_ready ? "ok-chip" : "warn-chip"}>
                  {p.lan_ready ? "LAN-reachable" : "this PC only"}
                </span>
              {:else}
                <span class="warn-chip">not published</span>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}

      <div class="step">
        <strong>HeidiSQL on THIS PC</strong>
        <p class="muted">
          No setup needed — connect HeidiSQL to Host <code>127.0.0.1</code>, Port
          <code>{pcData.db_host_port}</code>, User <code>root</code>, Password
          <code>password</code>.
        </p>
      </div>

      <div class="step">
        <strong>HeidiSQL from ANOTHER PC on your LAN</strong>
        <p class="warn-text">
          This opens your database (port {pcData.db_host_port}) to your local network. Only do
          this on a network you trust, and NEVER forward it on your router or expose it to the
          internet — it is your whole server's data. The script writes a Windows
          portproxy + firewall rule; run it as Administrator.
        </p>
        {#if pcScriptError}<p class="inline-error">{pcScriptError}</p>{/if}
        <div class="row">
          <button
            onclick={makeMysqlScript}
            disabled={pcScriptBusy || featureLocked("port-proxy")}
            title={featureLocked("port-proxy") ? LOCKED_HINT : undefined}
          >
            Create the LAN-exposure script
          </button>
        </div>
        {#if pcScriptPath}
          <p class="notice">
            Script created: {pcScriptPath} — right-click it → Run with PowerShell (as admin).
          </p>
        {/if}
      </div>
    {/if}
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
          lockFlag="unbound-addon"
          onExit={onToolInstallExit}
        />
      {/key}
    {/if}
  </div>

  <h3 class="section-head">Disk &amp; performance</h3>

  <div class="card">
    <h3>WSL memory &amp; CPU (.wslconfig)</h3>
    <p class="muted">
      How much of this PC the server sandbox (WSL) may use. Stored in
      {wc?.path ?? "%USERPROFILE%\\.wslconfig"} — unrelated settings in that file are left
      untouched. Takes effect after WSL restarts.
    </p>
    {#if wcError}<p class="inline-error">{wcError}</p>{/if}
    <div class="row">
      <label class="fld">Memory
        <input type="text" placeholder="e.g. 8GB" bind:value={wcMemory} disabled={wcBusy} />
      </label>
      {#each ["8GB", "12GB", "16GB"] as p (p)}
        <button onclick={() => (wcMemory = p)} disabled={wcBusy}>{p}</button>
      {/each}
    </div>
    <div class="row">
      <label class="fld">Processors
        <input type="text" placeholder="e.g. 4" bind:value={wcProcessors} disabled={wcBusy} />
      </label>
      {#if cpuCount > 1}
        <button onclick={() => (wcProcessors = String(Math.max(1, Math.floor(cpuCount / 2))))} disabled={wcBusy}>
          Half ({Math.max(1, Math.floor(cpuCount / 2))})
        </button>
        <button onclick={() => (wcProcessors = String(cpuCount))} disabled={wcBusy}>
          All ({cpuCount})
        </button>
      {/if}
    </div>
    <div class="row">
      <button
        class="primary"
        onclick={saveWslconfig}
        disabled={wcBusy || (!wcMemory.trim() && !wcProcessors.trim()) || featureLocked("disk-tools")}
        title={featureLocked("disk-tools") ? LOCKED_HINT : undefined}
      >
        Save
      </button>
      {#if wcNote}<span class="notice">{wcNote}</span>{/if}
    </div>
  </div>

  <div class="card">
    <h3>Restart WSL</h3>
    <p class="muted">
      Stops the server (characters saved, graceful stop) and shuts down ALL of WSL — the next
      server start is a cold start. Needed for .wslconfig changes to take effect, and a good
      "turn it off and on again" when WSL misbehaves.
    </p>
    <div class="row">
      <input
        type="text"
        placeholder={'Type "restart-wsl" to confirm'}
        bind:value={wslRestartInput}
        disabled={wslRestartBusy}
      />
      <button
        onclick={runRestartWsl}
        disabled={wslRestartInput !== "restart-wsl" || wslRestartBusy || featureLocked("disk-tools")}
        title={featureLocked("disk-tools") ? LOCKED_HINT : undefined}
      >
        {wslRestartBusy ? "Restarting…" : "Restart WSL"}
      </button>
    </div>
    {#if wslRestartNote}<p class="notice">{wslRestartNote}</p>{/if}
  </div>

  <div class="card">
    <h3>Shrink the server disk</h3>
    <p class="muted">
      The WSL virtual disk grows but never shrinks on its own. This writes a PowerShell
      script into your Downloads folder and opens Explorer at it — then YOU right-click the
      script and pick "Run with PowerShell" <strong>as Administrator</strong>. Stop the server
      first; compacting can take many minutes.
    </p>
    {#if shrinkError}<p class="inline-error">{shrinkError}</p>{/if}
    <div class="row">
      <button
        onclick={makeShrinkScript}
        disabled={shrinkBusy || featureLocked("disk-tools")}
        title={featureLocked("disk-tools") ? LOCKED_HINT : undefined}
      >
        Create the shrink script
      </button>
    </div>
    {#if shrinkPath}
      <p class="notice">Script created: {shrinkPath} — right-click it → Run with PowerShell (as admin).</p>
    {/if}
  </div>

  <div class="card">
    <h3>Windows Defender exclusion (optional)</h3>
    <p class="muted">
      Excluding the server's disk folder from Defender's real-time scanning makes builds and
      disk-heavy work noticeably faster. Run this one line in an <strong>admin</strong>
      PowerShell — it is entirely optional and easy to undo (Remove-MpPreference).
    </p>
    {#if defCmd}
      <div class="row">
        <code class="copyline">{defCmd}</code>
        <button onclick={copyDefenderCmd}>{defCopied ? "Copied!" : "Copy"}</button>
      </div>
    {:else}
      <p class="muted">
        Couldn't locate the server disk automatically — in an admin PowerShell run
        <code>Add-MpPreference -ExclusionPath "&lt;folder containing dml-arch's ext4.vhdx&gt;"</code>.
      </p>
    {/if}
  </div>

  <div class="card">
    <h3>Cache maintenance</h3>
    <p class="muted">
      The launcher saves item tooltips, icons and 3D-model data it downloads so pages open
      instantly next time. Clearing it is safe — it just re-downloads what you look at. This
      does NOT touch the built-in talent trees or achievements data (those ship inside the
      app and are never cached).
    </p>
    {#if cacheError}<p class="inline-error">{cacheError}</p>{/if}
    <ul class="port-list">
      {#each cacheEntries as c (c.key)}
        <li>
          <span class="mono">{c.label}</span>
          <span class="mono">{fmtBytes(c.bytes)}</span>
          <span class="muted">{c.files} file{c.files === 1 ? "" : "s"}</span>
        </li>
      {/each}
      {#if cacheEntries.length === 0 && !cacheBusy}
        <li><span class="muted">No cache measured yet.</span></li>
      {/if}
    </ul>
    <div class="row">
      <button onclick={loadCaches} disabled={cacheBusy}>Refresh</button>
      <button
        onclick={armClearCaches}
        disabled={cacheBusy || featureLocked("cache-maint")}
        title={featureLocked("cache-maint") ? LOCKED_HINT : undefined}
      >
        {cacheConfirm ? "Clear all caches — sure?" : "Clear caches"}
      </button>
    </div>
    {#if cacheNote}<p class="notice">{cacheNote}</p>{/if}
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
  .section-head { margin: 8px 0 0; font-size: 14px; color: #8b949e; text-transform: uppercase; letter-spacing: 0.06em; }
  .fld { display: flex; align-items: center; gap: 8px; font-size: 13.5px; color: #8b949e; }
  .fld input { min-width: 110px; }
  .step { border-top: 1px solid #21262d; padding-top: 8px; display: flex; flex-direction: column; gap: 6px; }
  .step strong { font-size: 13.5px; color: #f0f6fc; }
  .step .warn-text { color: #f85149; font-weight: 600; }
  code { background: #161b22; border: 1px solid #21262d; border-radius: 4px; padding: 1px 5px; font-size: 12.5px; }
  .copyline { padding: 6px 10px; font-size: 13px; }
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
  .rl-state { margin: 0; font-size: 14px; }
  .rl-state.good { color: #3fb950; }
  .rl-state.warn { color: #d29922; }
  .port-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 4px; font-size: 13px; color: #8b949e; }
  .port-list li { display: flex; gap: 8px; align-items: baseline; flex-wrap: wrap; }
  .port-list .mono { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; color: #c9d1d9; }
  .port-list li .mono:first-child { min-width: 68px; }
  .ok-chip { color: #3fb950; font-size: 12px; }
  .warn-chip { color: #d29922; font-size: 12px; }
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
