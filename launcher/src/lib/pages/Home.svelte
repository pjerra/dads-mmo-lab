<script lang="ts">
  import { onMount } from "svelte";
  import { wowServerDetail, gamesStatus, gamesStart, gamesStop, gamesRestart, type ServerDetail } from "$lib/api";
  import { applyEvent, initialTermState, type TermState } from "$lib/terminal-state";
  import Terminal from "$lib/Terminal.svelte";
  import { featureLocked, LOCKED_HINT } from "$lib/features.svelte";

  const WOW_ID = "wow-server-playerbots";
  const ROLE_LABELS: Record<string, string> = {
    world: "World server",
    auth: "Auth server",
    database: "Database",
  };

  let detail: ServerDetail | null = $state(null);
  let detailError: string | null = $state(null);
  let containerState: "running" | "stopped" | null = $state(null);
  let statusError: string | null = $state(null);
  let refreshing = $state(false);
  let expanded = $state(false);

  let busy = $state(false);
  let term: TermState = $state(initialTermState());
  let showTerm = $state(false);

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
    try {
      detail = await wowServerDetail();
      detailError = null;
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      detailError = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
      detail = null;
    } finally {
      refreshing = false;
    }
  }
  onMount(refresh);

  async function act(action: "start" | "stop" | "restart") {
    busy = true;
    showTerm = true;
    term = initialTermState();
    try {
      const run = action === "start" ? gamesStart : action === "stop" ? gamesStop : gamesRestart;
      await run(WOW_ID, (e) => {
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

  {#if detailError}
    <div class="error-card"><strong>Couldn't read world status.</strong><p>{detailError}</p></div>
  {:else if detail}
    <div class="card status-card" class:warn={detail.verdict === "soap_unreachable"}>
      <div class="card-title">
        <span
          class="dot"
          class:on={detail.verdict === "online"}
          class:mid={detail.verdict === "starting"}
          class:bad={detail.verdict === "soap_unreachable"}
          class:off={detail.verdict === "stopped"}
        ></span>
        <strong>
          {#if detail.verdict === "online"}World is up
          {:else if detail.verdict === "starting"}Starting up…
          {:else if detail.verdict === "soap_unreachable"}World is running, but the launcher can't reach it
          {:else}Server is stopped{/if}
        </strong>
      </div>
      {#if detail.verdict === "online"}
        <div class="stats">
          <span>Players online: <strong>{detail.soap.players ?? "?"}</strong></span>
          <span>Uptime: <strong>{detail.soap.uptime ?? "?"}</strong></span>
          <span>Update time: <strong>{detail.soap.mean_ms ?? "?"} ms avg</strong></span>
        </div>
      {:else if detail.verdict === "starting"}
        <p class="muted">The world is still loading — this takes a couple of minutes while bots spawn.</p>
      {:else if detail.verdict === "soap_unreachable"}
        <p class="muted">
          If this persists for more than a minute, Docker's networking in the distro is likely stuck —
          restarting Docker inside dml-arch usually fixes it.
        </p>
      {:else}
        <p class="muted">Start the server below.</p>
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
          {:else}
            <button class="primary" disabled={busy} onclick={() => act("start")}>Start</button>
          {/if}
        </div>
      </div>
      {#if expanded}
        <div class="health">
          {#if detail}
            {#each detail.containers as c (c.name)}
              <div class="hrow">
                <span class="dot" class:on={c.state === "running"} class:off={c.state !== "running"}></span>
                <span class="hname">{ROLE_LABELS[c.role] ?? c.name}</span>
                <span class="hval">{c.state === "absent" ? "not created" : c.status || c.state}</span>
              </div>
            {/each}
            {#if detail.verdict === "online"}
              <div class="hrow"><span class="hname">Version</span><span class="hval">{detail.soap.version ?? "?"}</span></div>
              <div class="hrow"><span class="hname">Uptime</span><span class="hval">{detail.soap.uptime ?? "?"}</span></div>
              <div class="hrow"><span class="hname">Players online</span><span class="hval">{detail.soap.players ?? "?"}</span></div>
              <div class="hrow">
                <span class="hname">World update time</span>
                <span class="hval">{detail.soap.mean_ms ?? "?"} ms mean · {detail.soap.median_ms ?? "?"} ms median</span>
              </div>
            {/if}
            <div class="hrow">
              <span class="hname">Ports</span>
              <span class="hval">
                game {detail.ports.world ?? "?"} · auth {detail.ports.auth ?? "?"} · SOAP {detail.ports.soap ?? "?"} · DB {detail.ports.db ?? "?"}
              </span>
            </div>
            <div class="hrow">
              <span class="hname">SOAP</span>
              <span class="hval">
                {detail.soap.reachable ? "reachable" : "unreachable"}{detail.soap.auth_ok === false
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

  {#if showTerm}
    <Terminal state={term} />
  {/if}
</section>

<style>
  .content { padding: 20px 24px; overflow-y: auto; display: flex; flex-direction: column; gap: 16px; }
  .bar { display: flex; justify-content: space-between; align-items: center; }
  .bar h2 { margin: 0; font-size: 18px; }
  .card { background: #0d1117; border: 1px solid #30363d; border-radius: 8px; padding: 14px 16px; display: flex; justify-content: space-between; align-items: center; gap: 16px; flex-wrap: wrap; }
  .card.warn { border-color: #f85149; }
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
  .stats { display: flex; gap: 18px; flex-wrap: wrap; }
  .health { margin-top: 12px; border-top: 1px solid #30363d; padding-top: 10px; display: flex; flex-direction: column; gap: 6px; }
  .hrow { display: flex; gap: 10px; align-items: center; font-size: 14px; }
  .hname { min-width: 150px; color: #8b949e; }
  .hval { color: #c9d1d9; }
  button { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 14px; cursor: pointer; }
  button.primary { background: #238636; border-color: #2ea043; color: white; }
  button:disabled { opacity: 0.5; cursor: default; }
  .muted { color: #8b949e; margin: 0; }
  .error-card { background: #161b22; border: 1px solid #f85149; border-radius: 8px; padding: 12px 16px; }
</style>
