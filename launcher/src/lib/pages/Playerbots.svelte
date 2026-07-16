<script lang="ts">
  import { onMount } from "svelte";
  import {
    wowPartyOnline, wowPartyAdd, wowPartyList, wowPartyKick, wowPartyRelogin, wowPartySetup,
    type OnlineChar, type PartyMember,
  } from "$lib/api";
  import { className } from "$lib/wow";
  import { applyEvent, initialTermState, type TermState } from "$lib/terminal-state";
  import Terminal from "$lib/Terminal.svelte";
  import { restartState } from "$lib/restart-state.svelte";

  const CLASSES = ["warrior","paladin","hunter","rogue","priest","shaman","mage","warlock","druid"];

  let online: OnlineChar[] = $state([]);
  let player = $state("");           // the chosen online player's name
  let members: PartyMember[] = $state([]);
  let error: string | null = $state(null);
  let busy = $state(false);
  let note: string | null = $state(null);

  let term: TermState = $state(initialTermState());
  let showTerm = $state(false);
  let setting = $state(false);
  let confirmSetup = $state(false);

  function showErr(e: unknown) {
    const err = e as { message?: string; hint?: string };
    error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
  }

  async function refresh() {
    error = null;
    confirmSetup = false;
    note = null;
    try {
      online = await wowPartyOnline();
      if (!online.find((o) => o.name === player)) player = online[0]?.name ?? "";
      if (player) members = await wowPartyList(player); else members = [];
    } catch (e) { showErr(e); }
  }
  onMount(refresh);

  // add/kick/resummon snapshot `player` into a local before their first await.
  // The player <select> is also disabled while busy/setting (below), but the
  // snapshot means these handlers stay correct even if that guard is ever
  // loosened -- a live re-read of `player` after an await could otherwise
  // send a follow-up call (or a "note" message) to the wrong character if the
  // selection changed mid-flight.
  async function add(cls: string) {
    const p = player;
    if (!p) return;
    busy = true; error = null; note = null;
    try {
      const r = await wowPartyAdd(p, cls);
      note = r.joined ? `Added a ${cls} to your party.` : (r.note ?? "Adding…");
      members = await wowPartyList(p);
    } catch (e) { showErr(e); } finally { busy = false; }
  }
  async function kick(bot: string) {
    const p = player;
    busy = true; error = null;
    try { await wowPartyKick(bot); members = await wowPartyList(p); }
    catch (e) { showErr(e); } finally { busy = false; }
  }
  async function resummon(bot: string) {
    const p = player;
    busy = true; error = null;
    try { await wowPartyRelogin(p, bot); members = await wowPartyList(p); }
    catch (e) { showErr(e); } finally { busy = false; }
  }
  async function enableMyParty() {
    if (!confirmSetup) { confirmSetup = true; return; }
    confirmSetup = false; setting = true; showTerm = true; term = initialTermState();
    try {
      await wowPartySetup((e) => {
        term = applyEvent(term, e);
        if (e.event === "done") {
          const d = e.data as { restart_required?: boolean } | undefined;
          if (d?.restart_required) restartState.needed = true;
        }
      });
    } catch (e) {
      const err = e as { code?: string; message?: string; hint?: string };
      term = applyEvent(term, { event: "error", error: { code: err.code ?? "IPC", message: err.message ?? String(e), hint: err.hint ?? "" } });
    } finally { setting = false; }
  }
</script>

<section class="content">
  <header class="bar"><h2>My Party</h2><button onclick={refresh} disabled={busy || setting}>Refresh</button></header>

  {#if error}<div class="error-card"><p>{error}</p></div>{/if}

  {#if online.length === 0}
    <div class="card">
      <p class="muted">No character is logged into the game. Log one in, then Refresh.</p>
      <p class="muted">First time? <button onclick={enableMyParty} disabled={setting}>
        {confirmSetup ? "Deploy the bot bridge scripts?" : "Enable My Party"}</button>
        <span class="muted">— one-time setup; afterward stop and start the server (Home or Library) to load the scripts.</span></p>
    </div>
  {:else}
    <div class="card">
      <strong>Building a party for
        {#if online.length > 1}
          <select bind:value={player} onchange={() => refresh()} disabled={busy || setting}>
            {#each online as o (o.guid)}<option value={o.name}>{o.name}</option>{/each}
          </select>
        {:else}{player}{/if}
      </strong>
    </div>

    <div class="addrow">
      {#each CLASSES as c (c)}
        <button class="cls" onclick={() => add(c)} disabled={busy || setting}>{c[0].toUpperCase() + c.slice(1)}</button>
      {/each}
    </div>
    {#if note}<p class="muted">{note}</p>{/if}

    <header class="bar"><h3>Current party</h3></header>
    {#if members.length <= 1}
      <p class="muted">Just you so far — click a class above to add a bot.</p>
    {:else}
      <table>
        <tbody>
          {#each members as m (m.guid)}
            <tr>
              <td>{m.name}</td><td class="muted">{className(m.class)} · lvl {m.level}</td>
              <td>{#if m.is_bot}<button onclick={() => kick(m.name)} disabled={busy}>Kick</button>
                  <button onclick={() => resummon(m.name)} disabled={busy}>Re-summon</button>{:else}<span class="muted">you</span>{/if}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  {/if}

  {#if showTerm}<Terminal state={term} />{/if}
</section>

<style>
  .content { padding: 20px 24px; overflow-y: auto; display: flex; flex-direction: column; gap: 14px; }
  .bar { display: flex; justify-content: space-between; align-items: center; }
  .bar h2, .bar h3 { margin: 0; }
  .card { background: #0d1117; border: 1px solid #30363d; border-radius: 8px; padding: 12px 16px; }
  .addrow { display: flex; flex-wrap: wrap; gap: 8px; }
  .cls { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 8px 14px; cursor: pointer; }
  table { border-collapse: collapse; }
  td { padding: 4px 12px 4px 0; font-size: 14px; }
  button { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 5px 12px; cursor: pointer; }
  button:disabled { opacity: 0.5; cursor: default; }
  .muted { color: #8b949e; font-size: 13px; }
  .error-card { background: #161b22; border: 1px solid #f85149; border-radius: 8px; padding: 12px 16px; }
</style>
