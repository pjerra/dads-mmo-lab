<script lang="ts">
  import { wowItemsSearch, wowMailItem, type ItemRow } from "$lib/api";
  import { qualityName, QUALITY_COLORS } from "$lib/wow";
  import CharPicker from "$lib/CharPicker.svelte";

  let name = $state("");
  let quality = $state<string>("");
  let minLevel = $state<string>("");
  let maxLevel = $state<string>("");
  let rows: ItemRow[] = $state([]);
  let searched = $state(false);
  let searching = $state(false);
  let error: string | null = $state(null);

  let sendItem: ItemRow | null = $state(null);
  let sendTo = $state("");
  let sendCount = $state(1);
  let sendSubject = $state("");
  let sending = $state(false);
  let sentMsg: string | null = $state(null);

  async function search() {
    if (!name.trim()) return;
    searching = true;
    error = null;
    sentMsg = null;
    try {
      rows = await wowItemsSearch({
        name: name.trim(),
        quality: quality === "" ? undefined : Number(quality),
        minLevel: minLevel === "" ? undefined : Number(minLevel),
        maxLevel: maxLevel === "" ? undefined : Number(maxLevel),
      });
      searched = true;
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    } finally {
      searching = false;
    }
  }

  async function send() {
    if (!sendItem || !sendTo) return;
    const count = Math.min(200, Math.max(1, Math.floor(sendCount) || 1));
    sending = true;
    error = null;
    try {
      await wowMailItem({
        to: sendTo,
        items: `${sendItem.entry}:${count}`,
        subject: sendSubject.trim() || undefined,
      });
      sentMsg = `Sent ${count}x ${sendItem.name} to ${sendTo} (check the mailbox).`;
      sendItem = null;
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    } finally {
      sending = false;
    }
  }
</script>

<section class="content">
  <header class="bar"><h2>Item Database</h2></header>

  <form class="filters" onsubmit={(e) => { e.preventDefault(); search(); }}>
    <input placeholder="Item name (required)" bind:value={name} />
    <select bind:value={quality}>
      <option value="">Any quality</option>
      {#each [0, 1, 2, 3, 4, 5] as q}
        <option value={String(q)}>{qualityName(q)}</option>
      {/each}
    </select>
    <input placeholder="Min lvl" size="6" bind:value={minLevel} />
    <input placeholder="Max lvl" size="6" bind:value={maxLevel} />
    <button class="primary" type="submit" disabled={!name.trim() || searching}>Search</button>
  </form>

  {#if error}<div class="error-card"><p>{error}</p></div>{/if}
  {#if sentMsg}<div class="ok-card"><p>{sentMsg}</p></div>{/if}

  {#if searched && rows.length === 0 && !error}
    <p class="muted">No items matched.</p>
  {/if}

  {#if rows.length > 0}
    <table>
      <thead><tr><th>Name</th><th>Quality</th><th>Item lvl</th><th>Req lvl</th><th></th></tr></thead>
      <tbody>
        {#each rows as it (it.entry)}
          <tr>
            <td style="color: {QUALITY_COLORS[it.quality] ?? '#c9d1d9'}">{it.name}</td>
            <td>{qualityName(it.quality)}</td>
            <td>{it.item_level}</td>
            <td>{it.required_level}</td>
            <td><button onclick={() => { sendItem = it; sentMsg = null; }}>Send</button></td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}

  {#if sendItem}
    <div class="card sendbox">
      <strong>Send {sendItem.name}</strong>
      <div class="row">
        <CharPicker bind:selected={sendTo} />
        <label>Count <input type="number" min="1" max="200" bind:value={sendCount} /></label>
      </div>
      <input placeholder="Mail subject (optional)" bind:value={sendSubject} />
      <div class="row">
        <button class="primary" onclick={send} disabled={!sendTo || sending}>Send mail</button>
        <button onclick={() => (sendItem = null)} disabled={sending}>Cancel</button>
      </div>
    </div>
  {/if}
</section>

<style>
  .content { padding: 20px 24px; overflow-y: auto; display: flex; flex-direction: column; gap: 16px; }
  .bar h2 { margin: 0; font-size: 18px; }
  .filters { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  input, select { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 8px; }
  table { border-collapse: collapse; }
  th { text-align: left; color: #8b949e; font-size: 13px; padding: 4px 14px 4px 0; }
  td { padding: 4px 14px 4px 0; font-size: 14px; border-top: 1px solid #21262d; }
  .card, .sendbox { background: #0d1117; border: 1px solid #30363d; border-radius: 8px; padding: 14px 16px; display: flex; flex-direction: column; gap: 10px; }
  .row { display: flex; gap: 10px; align-items: center; }
  label { font-size: 14px; color: #8b949e; }
  button { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 14px; cursor: pointer; }
  button.primary { background: #238636; border-color: #2ea043; color: white; }
  button:disabled { opacity: 0.5; cursor: default; }
  .muted { color: #8b949e; }
  .error-card { background: #161b22; border: 1px solid #f85149; border-radius: 8px; padding: 12px 16px; }
  .ok-card { background: #161b22; border: 1px solid #2ea043; border-radius: 8px; padding: 12px 16px; }
</style>
