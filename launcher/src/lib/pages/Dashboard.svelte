<script lang="ts">
  import { onMount } from "svelte";
  import { wowServerDetail, wowPaperdoll, type ServerDetail, type PaperdollData } from "$lib/api";
  import { qualityName, QUALITY_COLORS } from "$lib/wow";
  import CharPicker from "$lib/CharPicker.svelte";

  let detail: ServerDetail | null = $state(null);
  let infoError: string | null = $state(null);
  let loadingInfo = $state(false);

  let charName = $state("");
  let doll: PaperdollData | null = $state(null);
  let dollError: string | null = $state(null);
  let loadingDoll = $state(false);

  async function refreshInfo() {
    loadingInfo = true;
    infoError = null;
    try {
      detail = await wowServerDetail();
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      infoError = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    } finally {
      loadingInfo = false;
    }
  }
  onMount(refreshInfo);

  async function loadDoll() {
    if (!charName) return;
    loadingDoll = true;
    dollError = null;
    doll = null;
    try {
      doll = await wowPaperdoll(charName);
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      dollError = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    } finally {
      loadingDoll = false;
    }
  }
</script>

<section class="content">
  <header class="bar">
    <h2>Dashboard</h2>
    <button onclick={refreshInfo} disabled={loadingInfo}>Refresh</button>
  </header>

  {#if infoError}
    <div class="error-card"><strong>Couldn't read server status.</strong><p>{infoError}</p></div>
  {:else if detail}
    <div class="card status" class:warn={detail.verdict === "soap_unreachable"}>
      <div>
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
        <p class="muted">Start it from the Library page.</p>
      {/if}
    </div>
  {/if}

  <header class="bar"><h2>Character viewer</h2></header>
  <div class="pickrow">
    <CharPicker bind:selected={charName} />
    <button onclick={loadDoll} disabled={!charName || loadingDoll}>Show gear</button>
  </div>
  {#if dollError}
    <div class="error-card"><strong>Couldn't load character gear.</strong><p>{dollError}</p></div>
  {:else if doll}
    <div class="card doll">
      <strong>{doll.name}</strong> — level {doll.level}, {doll.gold} gold
      <table>
        <tbody>
          {#each doll.equipped as it (it.slot)}
            <tr>
              <td style="color: {QUALITY_COLORS[it.quality] ?? '#c9d1d9'}">{it.name}</td>
              <td>{qualityName(it.quality)}</td>
              <td>ilvl {it.item_level}</td>
            </tr>
          {/each}
        </tbody>
      </table>
      <p class="muted">Shown as of the character's last save — an online character can lag a little.</p>
    </div>
  {/if}
</section>

<style>
  .content { padding: 20px 24px; overflow-y: auto; display: flex; flex-direction: column; gap: 16px; }
  .bar { display: flex; justify-content: space-between; align-items: center; }
  .bar h2 { margin: 0; font-size: 18px; }
  .card { background: #0d1117; border: 1px solid #30363d; border-radius: 8px; padding: 14px 16px; }
  .status .stats { display: flex; gap: 24px; margin-top: 8px; }
  .dot { width: 9px; height: 9px; border-radius: 50%; display: inline-block; margin-right: 6px; }
  .dot.on { background: #3fb950; }
  .dot.off { background: #6e7681; }
  .dot.mid { background: #d29922; }
  .dot.bad { background: #f85149; }
  .card.warn { border-color: #f85149; }
  .pickrow { display: flex; gap: 8px; align-items: center; }
  table { border-collapse: collapse; margin-top: 10px; }
  td { padding: 3px 12px 3px 0; font-size: 14px; }
  button { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 14px; cursor: pointer; }
  button:disabled { opacity: 0.5; cursor: default; }
  .muted { color: #8b949e; font-size: 13px; }
  .error-card { background: #161b22; border: 1px solid #f85149; border-radius: 8px; padding: 12px 16px; }
</style>
