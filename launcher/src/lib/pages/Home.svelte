<script lang="ts">
  import { onMount } from "svelte";
  import { gamesStatus, gamesStart, gamesStop, gamesRestart, wowPlayersOnline, wowWorldRestart, type PlayerOnline } from "$lib/api";
  import { className } from "$lib/wow";
  import { applyEvent } from "$lib/terminal-state";
  import Terminal from "$lib/Terminal.svelte";
  import { termBuf, beginRun, clearBuf } from "$lib/term-store.svelte";
  import { featureLocked, LOCKED_HINT } from "$lib/features.svelte";
  import { chipStart, serverStatus, refreshServerStatus } from "$lib/server-status.svelte";
  import { restartState } from "$lib/restart-state.svelte";

  const WOW_ID = "wow-server-playerbots";
  const ROLE_LABELS: Record<string, string> = {
    world: "World server",
    auth: "Auth server",
    database: "Database",
  };

  let containerState: "running" | "stopped" | null = $state(null);
  let statusError: string | null = $state(null);
  let refreshing = $state(false);
  let expanded = $state(false);

  let busy = $state(false);
  const buf = termBuf("home");

  // `detail`/`detailError` now live in the server-status store (server-status.svelte.ts)
  // so the sidebar chip and Console see the same last-known data instantly on
  // remount instead of going blank until this page's own fetch lands.
  async function refresh() {
    refreshing = true;
    try {
      containerState = (await gamesStatus(WOW_ID)).state;
      statusError = null;
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      statusError = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
      containerState = null;
    }
    await refreshServerStatus();
    void refreshPlayers();
    refreshing = false;
  }
  onMount(refresh);

  // Players-online card (Batch 3 F11a): fetched only while the world is
  // actually up (the DB query would just error otherwise). Refetches on
  // page Refresh and whenever the polled verdict flips to online.
  let players: PlayerOnline[] = $state([]);
  let playersLoaded = $state(false);
  async function refreshPlayers() {
    if (serverStatus.detail?.verdict !== "online") {
      players = [];
      playersLoaded = false;
      return;
    }
    try {
      players = await wowPlayersOnline();
      playersLoaded = true;
    } catch {
      // Best-effort card -- a transient DB error just keeps the last list.
    }
  }
  let lastVerdict: string | null = null;
  $effect(() => {
    const v = serverStatus.detail?.verdict ?? null;
    if (v !== lastVerdict) {
      lastVerdict = v;
      void refreshPlayers();
    }
  });

  // Chip quick-start consumer (Batch 2 F8): the sidebar ▶ sets the request
  // and navigates here; this effect runs on mount AND when the request flips
  // while Home is already the active page. Consumed before starting so a
  // re-render can't double-fire.
  $effect(() => {
    if (chipStart.requested && !busy) {
      chipStart.requested = false;
      act("start");
    }
  });

  async function act(action: "start" | "stop" | "restart") {
    busy = true;
    beginRun("home");
    // The shared restarting flag drives the amber "Restarting…" override on
    // the card and the sidebar chip -- without it, polling mid-restart flaps
    // through stopped/starting. Config/Backups set it for their flows; this
    // covers the Home buttons (a start after a stop reads as "starting" via
    // the polled verdict, so only restart needs the explicit flag).
    if (action === "restart") restartState.restarting = true;
    try {
      const run = action === "start" ? gamesStart : action === "stop" ? gamesStop : gamesRestart;
      await run(WOW_ID, (e) => {
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
      if (action === "restart") restartState.restarting = false;
      busy = false;
      await refresh();
    }
  }

  // Fast world-only restart (Batch 3 F11f): restarts ONLY the worldserver
  // container. Faster than a full Restart, but docker restart keeps
  // creation-time env -- settings changes do NOT apply (the stream repeats
  // that caveat). Shares the restarting flag so the chip/card show amber.
  async function worldRestart() {
    busy = true;
    beginRun("home");
    restartState.restarting = true;
    try {
      await wowWorldRestart((e) => {
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
      restartState.restarting = false;
      busy = false;
      await refresh();
    }
  }
</script>

<section class="content">
  <header class="bar">
    <h2>Home</h2>
    <button onclick={refresh} disabled={refreshing || busy}>Refresh</button>
  </header>

  {#if serverStatus.detail}
    {@const d = serverStatus.detail}
    <div
      class="card status-card"
      class:warn={!restartState.restarting && d.verdict === "soap_unreachable"}
      class:crash={!restartState.restarting && d.verdict === "crashed"}
    >
      <div class="card-title">
        <span
          class="dot status-dot"
          class:on={!restartState.restarting && d.verdict === "online"}
          class:mid={restartState.restarting || d.verdict === "starting"}
          class:bad={!restartState.restarting && d.verdict === "soap_unreachable"}
          class:off={!restartState.restarting && d.verdict === "stopped"}
          class:crash={!restartState.restarting && d.verdict === "crashed"}
        ></span>
        <strong class:crash-text={!restartState.restarting && d.verdict === "crashed"}>
          {#if restartState.restarting}Restarting…
          {:else if d.verdict === "online"}World is up
          {:else if d.verdict === "starting"}Starting up…
          {:else if d.verdict === "soap_unreachable"}World is running, but the launcher can't reach it
          {:else if d.verdict === "crashed"}Server crashed
          {:else}Server is stopped{/if}
        </strong>
      </div>
      {#if restartState.restarting}
        <p class="muted">Restarting — this takes a minute or two while the world reloads.</p>
      {:else if d.verdict === "online"}
        <div class="stats">
          <span>Players online: <strong>{d.soap.players ?? "?"}</strong></span>
          <span>Uptime: <strong>{d.soap.uptime ?? "?"}</strong></span>
          <span>Update time: <strong>{d.soap.mean_ms ?? "?"} ms avg</strong></span>
          <span>Bots: <strong>{d.bots.online ?? "?"} / {d.bots.max ?? "?"}</strong></span>
        </div>
      {:else if d.verdict === "starting"}
        <p class="muted">The world is still loading — this takes a couple of minutes while bots spawn.</p>
      {:else if d.verdict === "soap_unreachable"}
        <p class="muted">
          If this persists for more than a minute, Docker's networking in the distro is likely stuck —
          restarting Docker inside dml-arch usually fixes it.
        </p>
      {:else if d.verdict === "crashed"}
        <div class="recover-row">
          <p class="muted">
            The world server stopped unexpectedly{d.exit_code !== null ? ` (exit code ${d.exit_code})` : ""}.
            Recover starts the server again; the crash usually leaves no damage — characters
            save every 15 minutes.
          </p>
          <button class="primary" disabled={busy} onclick={() => act("start")}>Recover</button>
        </div>
      {:else}
        <p class="muted">Start the server below.</p>
      {/if}
    </div>
    {#if serverStatus.lastError}
      <p class="muted refresh-warn">Last refresh failed ({serverStatus.lastError}) — showing the last known status.</p>
    {/if}
  {:else if serverStatus.lastError}
    <div class="error-card"><strong>Couldn't read world status.</strong><p>{serverStatus.lastError}</p></div>
  {/if}

  {#if serverStatus.detail?.verdict === "online" && playersLoaded}
    <div class="card players-card">
      <div class="card-title"><strong>Players online</strong></div>
      {#if players.length === 0}
        <p class="muted">Nobody online right now.</p>
      {:else}
        <div class="players">
          {#each players as p (p.name)}
            <span class="player">
              <strong>{p.name}</strong>
              <span class="muted">lvl {p.level} {className(p.class)}</span>
            </span>
          {/each}
        </div>
      {/if}
    </div>
  {/if}

  <header class="bar"><h2>WoW server</h2></header>
  {#if statusError}
    <div class="error-card"><strong>Couldn't reach the DML backend.</strong><p>{statusError}</p></div>
  {:else if containerState}
    <div class="card server-card">
      <div class="row">
        <button class="expander" onclick={() => (expanded = !expanded)} aria-expanded={expanded}>
          <span class="chev">{expanded ? "▾" : "▸"}</span>
          <span class="dot" class:on={containerState === "running"} class:off={containerState !== "running"}></span>
          {WOW_ID}
        </button>
        <div>
          {#if containerState === "running"}
            <button disabled={busy} onclick={() => act("stop")}>Stop</button>
            <button
              disabled={busy || featureLocked("restart")}
              title={featureLocked("restart") ? LOCKED_HINT : undefined}
              onclick={() => act("restart")}
            >
              Restart
            </button>
            <button
              disabled={busy || featureLocked("world-restart")}
              title={featureLocked("world-restart")
                ? LOCKED_HINT
                : "Faster: restarts only the world server. Does NOT apply settings changes — use Restart for that."}
              onclick={worldRestart}
            >
              Restart world only
            </button>
          {:else}
            <button class="primary" disabled={busy} onclick={() => act("start")}>Start</button>
          {/if}
        </div>
      </div>
      {#if expanded}
        <div class="health">
          {#if serverStatus.detail}
            {@const d = serverStatus.detail}
            {#each d.containers as c (c.name)}
              <div class="hrow">
                <span class="dot" class:on={c.state === "running"} class:off={c.state !== "running"}></span>
                <span class="hname">{ROLE_LABELS[c.role] ?? c.name}</span>
                <span class="hval">{c.state === "absent" ? "not created" : c.status || c.state}</span>
              </div>
            {/each}
            {#if d.verdict === "online"}
              <div class="hrow"><span class="hname">Version</span><span class="hval">{d.soap.version ?? "?"}</span></div>
              <div class="hrow"><span class="hname">Uptime</span><span class="hval">{d.soap.uptime ?? "?"}</span></div>
              <div class="hrow"><span class="hname">Players online</span><span class="hval">{d.soap.players ?? "?"}</span></div>
              <div class="hrow"><span class="hname">Bots online</span><span class="hval">{d.bots.online ?? "?"} of {d.bots.max ?? "?"} max</span></div>
              <div class="hrow">
                <span class="hname">World update time</span>
                <span class="hval">{d.soap.mean_ms ?? "?"} ms mean · {d.soap.median_ms ?? "?"} ms median</span>
              </div>
            {/if}
            <div class="hrow">
              <span class="hname">Ports</span>
              <span class="hval">
                game {d.ports.world ?? "?"} · auth {d.ports.auth ?? "?"} · SOAP {d.ports.soap ?? "?"} · DB {d.ports.db ?? "?"}
              </span>
            </div>
            <div class="hrow">
              <span class="hname">SOAP</span>
              <span class="hval">
                {d.soap.reachable ? "reachable" : "unreachable"}{d.soap.auth_ok === false
                  ? " — authentication failing, check ~/.dml/soap.env"
                  : ""}
              </span>
            </div>
          {:else}
            <p class="muted">No health data — hit Refresh.</p>
          {/if}
        </div>
      {/if}
    </div>
  {/if}

  {#if buf.show}
    <Terminal state={buf.term} onclear={() => clearBuf("home")} logName="dml-home" />
  {/if}
</section>

<style>
  .content { padding: 20px 24px; overflow-y: auto; display: flex; flex-direction: column; gap: 16px; }
  .bar { display: flex; justify-content: space-between; align-items: center; }
  .bar h2 { margin: 0; font-size: 18px; }
  .card { background: #0d1117; border: 1px solid #30363d; border-radius: 8px; padding: 14px 16px; display: flex; justify-content: space-between; align-items: center; gap: 16px; flex-wrap: wrap; }
  .card.warn { border-color: #f85149; }
  /* Distinct crash styling (Batch 2 F8): red border + red title, pulsing dot. */
  .card.crash { border-color: #f85149; background: #160b0e; }
  .crash-text { color: #f85149; }
  .recover-row { display: flex; align-items: center; gap: 14px; flex-wrap: wrap; }
  .server-card { flex-direction: column; align-items: stretch; }
  .row { display: flex; justify-content: space-between; align-items: center; gap: 16px; }
  .card-title { display: flex; align-items: center; gap: 8px; font-weight: 600; }
  .expander { background: none; border: none; padding: 0; display: flex; align-items: center; gap: 8px; font-weight: 600; font-size: inherit; color: inherit; cursor: pointer; }
  .chev { color: #8b949e; width: 12px; }
  .dot { width: 9px; height: 9px; border-radius: 50%; display: inline-block; flex-shrink: 0; }
  .dot.on { background: #3fb950; }
  .dot.off { background: #6e7681; }
  .dot.mid { background: #d29922; }
  .dot.bad { background: #f85149; }
  /* The top overview card's dot tracks live verdict colors (Round Q): online
     green, starting/restarting amber (pulsing), stopped red, soap_unreachable
     orange. Scoped to .status-dot so the health panel's per-container
     running/not-running dots above keep their original green/gray meaning. */
  .dot.status-dot.on { background: #3fb950; }
  .dot.status-dot.mid { background: #d29922; animation: dot-pulse 1.4s ease-in-out infinite; }
  .dot.status-dot.off { background: #f85149; }
  .dot.status-dot.bad { background: #ffa657; }
  .dot.status-dot.crash { background: #f85149; animation: dot-pulse 0.9s ease-in-out infinite; }
  @keyframes dot-pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.35; } }
  .stats { display: flex; gap: 18px; flex-wrap: wrap; }
  .players-card { flex-direction: column; align-items: stretch; gap: 8px; }
  .players { display: flex; gap: 8px; flex-wrap: wrap; }
  .player { background: #161b22; border: 1px solid #30363d; border-radius: 12px; padding: 3px 12px; font-size: 13px; display: inline-flex; gap: 8px; align-items: baseline; }
  .health { margin-top: 12px; border-top: 1px solid #30363d; padding-top: 10px; display: flex; flex-direction: column; gap: 6px; }
  .hrow { display: flex; gap: 10px; align-items: center; font-size: 14px; }
  .hname { min-width: 150px; color: #8b949e; }
  .hval { color: #c9d1d9; }
  button { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 14px; cursor: pointer; }
  button.primary { background: #238636; border-color: #2ea043; color: white; }
  button:disabled { opacity: 0.5; cursor: default; }
  .muted { color: #8b949e; margin: 0; }
  .refresh-warn { font-size: 12.5px; }
  .error-card { background: #161b22; border: 1px solid #f85149; border-radius: 8px; padding: 12px 16px; }
</style>
