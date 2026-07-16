<script lang="ts">
  import { onMount } from "svelte";
  import {
    wowPartyOnline, wowGmLevel, wowGmGold, wowGmHeal, wowGmRevive, wowBridgeSetup,
    type OnlineChar,
  } from "$lib/api";
  import { applyEvent, initialTermState, type TermState } from "$lib/terminal-state";
  import Terminal from "$lib/Terminal.svelte";
  import { restartState } from "$lib/restart-state.svelte";
  import CharPicker from "$lib/CharPicker.svelte";

  let charName = $state("");
  let online: OnlineChar[] = $state([]);
  let error: string | null = $state(null);
  let note: string | null = $state(null);
  let busy = $state(false);

  let level = $state(80);
  let gold = $state(1000);
  let confirming: "level" | "gold" | null = $state(null);

  let term: TermState = $state(initialTermState());
  let showTerm = $state(false);
  let deploying = $state(false);
  let confirmDeploy = $state(false);

  const isOnline = $derived(online.some((o) => o.name === charName));

  function showErr(e: unknown) {
    const err = e as { message?: string; hint?: string };
    error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
  }

  async function refresh() {
    error = null; note = null; confirming = null;
    try { online = await wowPartyOnline(); } catch (e) { showErr(e); }
  }
  onMount(refresh);

  // Every action snapshots charName before its first await so a mid-flight
  // picker change can't retarget the call or the success note.
  async function act(fn: () => Promise<unknown>, okNote: string) {
    busy = true; error = null; note = null;
    try { await fn(); note = okNote; }
    catch (e) { showErr(e); }
    finally { busy = false; }
  }

  function revive() { const p = charName; act(() => wowGmRevive(p), `Revived ${p}.`); }
  function heal() { const p = charName; act(() => wowGmHeal(p), `Healed ${p} to full.`); }
  function applyLevel() {
    if (confirming !== "level") { confirming = "level"; return; }
    confirming = null;
    const p = charName; const l = level;
    act(() => wowGmLevel(p, l), `${p} is now level ${l}.`);
  }
  function applyGold() {
    if (confirming !== "gold") { confirming = "gold"; return; }
    confirming = null;
    const p = charName; const g = gold;
    act(() => wowGmGold(p, g), `${p} now has ${g} gold.`);
  }

  async function deployBridges() {
    if (!confirmDeploy) { confirmDeploy = true; return; }
    confirmDeploy = false; deploying = true; showTerm = true; term = initialTermState();
    try {
      await wowBridgeSetup((e) => {
        term = applyEvent(term, e);
        if (e.event === "done") {
          const d = e.data as { restart_required?: boolean } | undefined;
          if (d?.restart_required) restartState.needed = true;
        }
      });
    } catch (e) {
      const err = e as { code?: string; message?: string; hint?: string };
      term = applyEvent(term, { event: "error", error: { code: err.code ?? "IPC", message: err.message ?? String(e), hint: err.hint ?? "" } });
    } finally { deploying = false; }
  }
</script>

<section class="content">
  <header class="bar">
    <h2>GM Tools</h2>
    <button onclick={refresh} disabled={busy || deploying}>Refresh</button>
  </header>

  {#if error}<div class="error-card"><p>{error}</p></div>{/if}
  {#if restartState.needed}
    <div class="warn-card"><p>Saved — restart the server to apply the changes.</p></div>
  {/if}

  <div class="card row">
    <strong>Character</strong>
    <CharPicker bind:selected={charName} disabled={busy || deploying}
      onpick={() => { confirming = null; note = null; error = null; }} />
    {#if charName}
      <span class="badge {isOnline ? 'on' : 'off'}">{isOnline ? "Online" : "Offline"}</span>
      {#if !isOnline}<span class="muted">Revive, heal and gold need the character logged in. Set level works offline.</span>{/if}
    {/if}
  </div>

  {#if note}<p class="muted">{note}</p>{/if}

  <div class="card row">
    <strong>Rescue</strong>
    <button onclick={revive} disabled={!charName || !isOnline || busy}>Revive</button>
    <button onclick={heal} disabled={!charName || !isOnline || busy}>Full heal</button>
  </div>

  <div class="card row">
    <strong>Set level</strong>
    <input type="number" min="1" max="255" bind:value={level}
      oninput={() => (confirming = null)} disabled={busy} />
    <button onclick={applyLevel} disabled={!charName || busy || level < 1 || level > 255}>
      {confirming === "level" ? "This can lower the level — sure?" : "Apply"}
    </button>
    <span class="muted">1–255; your server's max level applies. Works offline.</span>
  </div>

  <div class="card row">
    <strong>Set gold</strong>
    <input type="number" min="0" max="214748" bind:value={gold}
      oninput={() => (confirming = null)} disabled={busy} />
    <button onclick={applyGold} disabled={!charName || !isOnline || busy || gold < 0 || gold > 214748}>
      {confirming === "gold" ? "This replaces their current money — sure?" : "Apply"}
    </button>
    <span class="muted">Sets the total (not adds). Max 214,748 gold.</span>
  </div>

  <p class="muted">
    Bridge scripts missing or outdated?
    <button onclick={deployBridges} disabled={deploying}>
      {confirmDeploy ? "Deploy the server bridge scripts?" : "Deploy server bridges"}
    </button>
    — then stop and start the server (Home or Library) to load them.
  </p>

  {#if showTerm}
    <Terminal state={term} />
  {/if}
</section>

<style>
  .content { padding: 20px 24px; overflow-y: auto; display: flex; flex-direction: column; gap: 12px; }
  .bar { display: flex; justify-content: space-between; align-items: center; }
  .bar h2 { margin: 0; font-size: 18px; }
  .card { background: #0d1117; border: 1px solid #30363d; border-radius: 8px; padding: 12px 16px; }
  .row { display: flex; gap: 12px; align-items: center; flex-wrap: wrap; }
  .badge { font-size: 12px; padding: 2px 10px; border-radius: 10px; border: 1px solid #30363d; }
  .badge.on { color: #3fb950; border-color: #3fb950; }
  .badge.off { color: #8b949e; }
  input { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 8px; width: 110px; }
  button { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 14px; cursor: pointer; }
  button:disabled { opacity: 0.5; cursor: default; }
  .muted { color: #8b949e; font-size: 13px; margin: 0; }
  .error-card { background: #161b22; border: 1px solid #f85149; border-radius: 8px; padding: 12px 16px; }
  .warn-card { background: #161b22; border: 1px solid #d29922; border-radius: 8px; padding: 12px 16px; }
</style>
