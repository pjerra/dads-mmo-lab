<script lang="ts">
  import { wowItemsSearch, wowMailItem, type ItemRow } from "$lib/api";
  import { qualityName, QUALITY_COLORS, className } from "$lib/wow";
  import CharPicker from "$lib/CharPicker.svelte";
  import { featureLocked, LOCKED_HINT } from "$lib/features.svelte";
  import { listGearSets, deleteGearSet, mailGearSet, type GearSet } from "$lib/gearsets.svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";

  // Batch 3 F11e: open the item's Wowhead (WotLK) page in the system
  // browser. Entry ids are numbers from our own DB query -- the URL is never
  // user-typed text. Best-effort, same as ModuleManager's openModUrl.
  function openWowhead(entry: number) {
    openUrl(`https://www.wowhead.com/wotlk/item=${entry}`).catch(() => {});
  }

  let name = $state("");
  let quality = $state<string>("");
  let minLevel = $state<string | number>("");
  let maxLevel = $state<string | number>("");
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
        minLevel: minLevel === "" ? undefined : Math.max(0, Math.floor(Number(minLevel)) || 0),
        maxLevel: maxLevel === "" ? undefined : Math.max(0, Math.floor(Number(maxLevel)) || 0),
      });
      searched = true;
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    } finally {
      searching = false;
    }
  }

  // Gear sets (Batch 5 F4): saved on the Dashboard's Character tab, mailed
  // from here (mail machinery/CharPicker/featureLocked already live here).
  // Mailing is sequential, ≤12 items per mail (server cap) -- see
  // gearsets.svelte.ts for the plan/chunk/failure contract.
  let mailSetName = $state<string | null>(null);
  let mailSetTo = $state("");
  let mailingSet = $state(false);
  let confirmDeleteSet = $state<string | null>(null);

  function toggleMailSet(name: string) {
    mailSetName = mailSetName === name ? null : name;
    confirmDeleteSet = null;
  }

  function removeSet(name: string) {
    if (confirmDeleteSet !== name) {
      confirmDeleteSet = name;
      return;
    }
    confirmDeleteSet = null;
    if (mailSetName === name) mailSetName = null;
    deleteGearSet(name);
  }

  async function sendSet(set: GearSet) {
    const to = mailSetTo;
    if (!to) return;
    mailingSet = true;
    error = null;
    sentMsg = null;
    try {
      const out = await mailGearSet(to, set);
      if (out.error) {
        error = out.error;
      } else {
        sentMsg = `Mailed "${set.name}" to ${to} in ${out.total} mail${out.total === 1 ? "" : "s"} (check the mailbox). Items are copies — the receiver may not be able to wear them.`;
        mailSetName = null;
      }
    } finally {
      mailingSet = false;
    }
  }

  async function send() {
    const item = sendItem;
    const to = sendTo;
    if (!item || !to) return;
    const count = Math.min(200, Math.max(1, Math.floor(sendCount) || 1));
    sending = true;
    error = null;
    try {
      await wowMailItem({
        to,
        items: `${item.entry}:${count}`,
        subject: sendSubject.trim() || undefined,
      });
      sentMsg = `Sent ${count}x ${item.name} to ${to} (check the mailbox).`;
      if (sendItem === item) sendItem = null;
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
    <input placeholder="Min lvl" size="6" type="number" min="0" step="1" bind:value={minLevel} />
    <input placeholder="Max lvl" size="6" type="number" min="0" step="1" bind:value={maxLevel} />
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
            <td>
              <button class="wh" title="View on Wowhead" onclick={() => openWowhead(it.entry)}>🔗</button>
              <button disabled={sending} onclick={() => { sendItem = it; sentMsg = null; }}>Send</button>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}

  <div class="card">
    <strong>Gear sets</strong>
    {#if listGearSets().length === 0}
      <p class="muted">None saved yet — open a character on the Dashboard and click "Save gear set".</p>
    {:else}
      {#each listGearSets() as gs (gs.name)}
        <div class="row">
          <span>
            {gs.name}
            <span class="muted">— {gs.sourceChar} (lvl {gs.level} {className(gs.class)}), {gs.items.length} items</span>
          </span>
          <button onclick={() => toggleMailSet(gs.name)} disabled={mailingSet}>
            {mailSetName === gs.name ? "Hide" : "Mail to…"}
          </button>
          <button onclick={() => removeSet(gs.name)} disabled={mailingSet}>
            {confirmDeleteSet === gs.name ? `Delete "${gs.name}" — sure?` : "Delete"}
          </button>
        </div>
        {#if mailSetName === gs.name}
          <div class="row">
            <CharPicker bind:selected={mailSetTo} />
            <button
              class="primary"
              onclick={() => sendSet(gs)}
              disabled={!mailSetTo || mailingSet || featureLocked("gear-sets")}
              title={featureLocked("gear-sets") ? LOCKED_HINT : undefined}
            >
              {mailingSet ? "Mailing…" : `Mail ${gs.items.length} items`}
            </button>
            <span class="muted">Fresh copies by in-game mail{gs.items.length > 12 ? `, split into 2 mails` : ""} — the receiver may not be able to wear cross-class gear.</span>
          </div>
        {/if}
      {/each}
    {/if}
  </div>

  {#if sendItem}
    <div class="card sendbox">
      <strong>
        Send {sendItem.name}
        <button class="wh" title="View on Wowhead" onclick={() => sendItem && openWowhead(sendItem.entry)}>🔗</button>
      </strong>
      <div class="row">
        <CharPicker bind:selected={sendTo} />
        <label>Count <input type="number" min="1" max="200" bind:value={sendCount} /></label>
      </div>
      <input placeholder="Mail subject (optional)" bind:value={sendSubject} />
      <div class="row">
        <button
          class="primary"
          onclick={send}
          disabled={!sendTo || sending || featureLocked("mail-item")}
          title={featureLocked("mail-item") ? LOCKED_HINT : undefined}
        >
          Send mail
        </button>
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
  button.wh { padding: 6px 8px; font-size: 12px; }
  button.wh:hover { border-color: #58a6ff; }
  button.primary { background: #238636; border-color: #2ea043; color: white; }
  button:disabled { opacity: 0.5; cursor: default; }
  .muted { color: #8b949e; }
  .error-card { background: #161b22; border: 1px solid #f85149; border-radius: 8px; padding: 12px 16px; }
  .ok-card { background: #161b22; border: 1px solid #2ea043; border-radius: 8px; padding: 12px 16px; }
</style>
