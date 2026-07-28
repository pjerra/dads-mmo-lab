<script module lang="ts">
  import type { ItemInfo } from "$lib/api";

  // Module-level (app-session lifetime), same rationale as CharacterSheet's
  // paperdoll cache: navigating away from Item Database and back must not
  // re-fetch icons/tooltips already seen. Cross-session persistence is the
  // CLI's own disk cache (~/.dml/wowhead-cache).
  const infoCache = new Map<number, ItemInfo>();
</script>

<script lang="ts">
  import { wowItemsSearch, wowMailItem, wowItemInfo, type ItemRow } from "$lib/api";
  import { qualityName, QUALITY_COLORS, className } from "$lib/wow";
  import { cacheSet, missingEntries } from "$lib/item-info-cache";
  import { anchorTooltip, clampTooltipTop, resolveItemHover } from "$lib/item-tooltip";
  import { sanitizeTooltipHtml } from "$lib/tooltip";
  import { isValidCharName } from "$lib/mail-recipient";
  import { chunkIds } from "$lib/progress";
  import RecipientPicker from "$lib/RecipientPicker.svelte";
  import { featureLocked, LOCKED_HINT } from "$lib/features.svelte";
  import { listGearSets, deleteGearSet, mailGearSet, addGearSet, type GearSet } from "$lib/gearsets.svelte";
  import { gearSetToToml, gearSetFromToml } from "$lib/gearset-toml";
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

  // Icons + hover tooltips for the result rows (Round E's `wow item-info`,
  // the same machinery the paperdoll uses). infoCache is a plain Map, so
  // reads bump through this counter to stay reactive when a batch lands.
  let infoVersion = $state(0);

  function itemInfo(entry: number): ItemInfo | undefined {
    infoVersion;
    return infoCache.get(entry);
  }

  // A search returns up to 50 rows but `wow item-info` rejects more than 25
  // ids per call (BAD_ARG), so chunk and let each chunk paint as it lands.
  // Entirely best-effort: offline or a failed chunk leaves plain rows, it
  // must never turn a working search into an error.
  async function loadInfos(entries: number[]) {
    const missing = missingEntries(infoCache, entries);
    if (missing.length === 0) return;
    try {
      for (const chunk of chunkIds(missing)) {
        const infos = await wowItemInfo(chunk);
        for (const info of infos) cacheSet(infoCache, info.entry, info);
        infoVersion++;
      }
    } catch {
      // leave whatever didn't land as plain rows
    }
  }

  interface Hovered {
    row: ItemRow;
    top: number;
    left: number | null;
    right: number | null;
  }
  let hovered: Hovered | null = $state(null);
  let tooltipEl: HTMLDivElement | undefined = $state();

  function showTooltip(e: MouseEvent | FocusEvent, row: ItemRow) {
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const at = anchorTooltip(rect, { width: window.innerWidth, height: window.innerHeight });
    hovered = { row, top: at.top, left: at.left, right: at.right };
  }
  function hideTooltip() {
    hovered = null;
  }

  // Pull the box back on-screen once its rendered height is known -- rows near
  // the bottom of a long result list would otherwise run off the viewport.
  $effect(() => {
    if (!hovered || !tooltipEl) return;
    const h = tooltipEl.getBoundingClientRect().height;
    const clamped = clampTooltipTop(hovered.top, h, window.innerHeight);
    if (hovered.top !== clamped) hovered.top = clamped;
  });

  // ONE recipient for the whole page (top of the tab), shared by single-item
  // sends and gear-set mails. Previously each send box carried its own
  // picker, so mailing five items to the same character meant picking them
  // five times.
  let recipient = $state("");
  let recipientValid = $state(false);

  let sendItem: ItemRow | null = $state(null);
  let sendCount = $state(1);
  let sendSubject = $state("");
  let sending = $state(false);
  let sentMsg: string | null = $state(null);

  async function search() {
    if (!name.trim()) return;
    searching = true;
    error = null;
    sentMsg = null;
    hovered = null;
    try {
      rows = await wowItemsSearch({
        name: name.trim(),
        quality: quality === "" ? undefined : Number(quality),
        minLevel: minLevel === "" ? undefined : Math.max(0, Math.floor(Number(minLevel)) || 0),
        maxLevel: maxLevel === "" ? undefined : Math.max(0, Math.floor(Number(maxLevel)) || 0),
      });
      searched = true;
      // Fire-and-forget: the table renders immediately with plain rows and
      // upgrades to icons/tooltips as the batches land.
      void loadInfos(rows.map((r) => r.entry));
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    } finally {
      searching = false;
    }
  }

  // Gear sets (Batch 5 F4): saved on the Dashboard's Character tab, mailed
  // from here (mail machinery/recipient/featureLocked already live here).
  // Mailing is sequential, ≤12 items per mail (server cap) -- see
  // gearsets.svelte.ts for the plan/chunk/failure contract. The recipient is
  // the page-level one above, not a second picker.
  let mailSetName = $state<string | null>(null);
  let mailingSet = $state(false);
  let confirmDeleteSet = $state<string | null>(null);

  // TOML export/import (Batch 4 D) -- pure frontend, localStorage only.
  let exportSetName = $state<string | null>(null);
  let exportText = $state("");
  let exportCopied = $state(false);
  let importText = $state("");
  let importMsg = $state<string | null>(null);
  let importErr = $state<string | null>(null);

  function toggleMailSet(name: string) {
    mailSetName = mailSetName === name ? null : name;
    confirmDeleteSet = null;
    exportSetName = null;
  }

  function toggleExport(gs: GearSet) {
    if (exportSetName === gs.name) { exportSetName = null; return; }
    exportSetName = gs.name;
    exportText = gearSetToToml(gs);
    exportCopied = false;
    mailSetName = null;
    confirmDeleteSet = null;
  }

  async function copyExport() {
    try {
      await navigator.clipboard.writeText(exportText);
      exportCopied = true;
      setTimeout(() => (exportCopied = false), 1500);
    } catch {
      // Clipboard unavailable -- the read-only textarea is the fallback.
    }
  }

  function importSet() {
    importErr = null; importMsg = null;
    try {
      const s = addGearSet(gearSetFromToml(importText));
      importMsg = `Imported "${s.name}" — ${s.items.length} items.`;
      importText = "";
    } catch (e) {
      importErr = (e as Error)?.message ?? String(e);
    }
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
    const to = recipient.trim();
    // Shape guard at the call site; the picker already decided sendability
    // (an unrecognised but well-formed name is deliberately allowed).
    if (!isValidCharName(to)) return;
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
    const to = recipient.trim();
    if (!item || !isValidCharName(to)) return;
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

  <div class="card mailto">
    <strong>Mail to</strong>
    <RecipientPicker bind:selected={recipient} bind:valid={recipientValid} disabled={sending || mailingSet} />
    <span class="muted">
      Everything mailed from this page — single items and gear sets — goes to this character.
    </span>
  </div>

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
          {@const info = itemInfo(it.entry)}
          <tr>
            <td>
              <div
                class="itemcell"
                role="button"
                tabindex="0"
                aria-label={it.name}
                onmouseenter={(e: MouseEvent) => showTooltip(e, it)}
                onmouseleave={hideTooltip}
                onfocus={(e: FocusEvent) => showTooltip(e, it)}
                onblur={hideTooltip}
              >
                {#if info?.icon_b64}
                  <img class="icon" src="data:image/jpeg;base64,{info.icon_b64}" alt="" />
                {:else}
                  <span
                    class="icon pending"
                    style="background: {QUALITY_COLORS[it.quality] ?? '#c9d1d9'}"
                  ></span>
                {/if}
                <span style="color: {QUALITY_COLORS[it.quality] ?? '#c9d1d9'}">{it.name}</span>
              </div>
            </td>
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
      <p class="muted">None saved yet — open a character on the Dashboard and click "Save gear set", or paste one below.</p>
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
          <button onclick={() => toggleExport(gs)} disabled={mailingSet}>
            {exportSetName === gs.name ? "Hide" : "Export"}
          </button>
          <button onclick={() => removeSet(gs.name)} disabled={mailingSet}>
            {confirmDeleteSet === gs.name ? `Delete "${gs.name}" — sure?` : "Delete"}
          </button>
        </div>
        {#if mailSetName === gs.name}
          <div class="row">
            <button
              class="primary"
              onclick={() => sendSet(gs)}
              disabled={!recipientValid || mailingSet || featureLocked("gear-sets")}
              title={featureLocked("gear-sets")
                ? LOCKED_HINT
                : !recipientValid
                  ? "Enter a recipient at the top of the page first"
                  : undefined}
            >
              {mailingSet ? "Mailing…" : `Mail ${gs.items.length} items to ${recipient.trim() || "…"}`}
            </button>
            <span class="muted">Fresh copies by in-game mail{gs.items.length > 12 ? `, split into 2 mails` : ""} — the receiver may not be able to wear cross-class gear.</span>
          </div>
        {/if}
        {#if exportSetName === gs.name}
          <div class="iocol">
            <textarea class="toml" readonly rows="6" value={exportText}></textarea>
            <div class="row">
              <button onclick={copyExport}>{exportCopied ? "Copied" : "Copy"}</button>
              <span class="muted">Copy this whole block and paste it into Import (below) on another launcher.</span>
            </div>
          </div>
        {/if}
      {/each}
    {/if}

    <div class="iocol import">
      <strong>Import a gear set</strong>
      <textarea class="toml" bind:value={importText} rows="5"
        placeholder="Paste an exported gear-set TOML block here…"></textarea>
      <div class="row">
        <button
          class="primary"
          onclick={importSet}
          disabled={!importText.trim() || featureLocked("gear-sets-io")}
          title={featureLocked("gear-sets-io") ? LOCKED_HINT : undefined}
        >
          Import
        </button>
        {#if importMsg}<span class="ok-text">{importMsg}</span>{/if}
        {#if importErr}<span class="err-text">{importErr}</span>{/if}
      </div>
    </div>
  </div>

  {#if sendItem}
    <div class="card sendbox">
      <strong>
        Send {sendItem.name}
        <button class="wh" title="View on Wowhead" onclick={() => sendItem && openWowhead(sendItem.entry)}>🔗</button>
      </strong>
      <div class="row">
        <span class="muted">To <strong>{recipient.trim() || "— set a recipient at the top"}</strong></span>
        <label>Count <input type="number" min="1" max="200" bind:value={sendCount} /></label>
      </div>
      <input placeholder="Mail subject (optional)" bind:value={sendSubject} />
      <div class="row">
        <button
          class="primary"
          onclick={send}
          disabled={!recipientValid || sending || featureLocked("mail-item")}
          title={featureLocked("mail-item")
            ? LOCKED_HINT
            : !recipientValid
              ? "Enter a recipient at the top of the page first"
              : undefined}
        >
          Send mail
        </button>
        <button onclick={() => (sendItem = null)} disabled={sending}>Cancel</button>
      </div>
    </div>
  {/if}
</section>

{#if hovered}
  {@const view = resolveItemHover(itemInfo(hovered.row.entry), hovered.row)}
  <div
    class="wow-tooltip"
    bind:this={tooltipEl}
    style="top: {hovered.top}px; {hovered.left !== null
      ? `left: ${hovered.left}px;`
      : `right: ${hovered.right}px;`}"
  >
    {#if view.wowhead}
      {@html sanitizeTooltipHtml(view.wowhead.tooltip)}
    {:else if view.localHtml}
      {@html sanitizeTooltipHtml(view.localHtml)}
    {:else}
      <b style="color: {view.color}">{view.label}</b>
      {#if view.sub}<div class="whtt-extra">{view.sub}</div>{/if}
    {/if}
  </div>
{/if}

<style>
  .content { padding: 20px 24px; overflow-y: auto; display: flex; flex-direction: column; gap: 16px; }
  .bar h2 { margin: 0; font-size: 18px; }
  .filters { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  input, select { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 8px; }
  table { border-collapse: collapse; }
  th { text-align: left; color: #8b949e; font-size: 13px; padding: 4px 14px 4px 0; }
  td { padding: 4px 14px 4px 0; font-size: 14px; border-top: 1px solid #21262d; }

  /* Result row: icon + quality-coloured name, and the hover target for the
     shared item tooltip. The icon slot keeps its size before the info batch
     lands so rows don't reflow as icons stream in. */
  .itemcell { display: flex; align-items: center; gap: 8px; cursor: default; }
  .itemcell:focus-visible { outline: 1px solid #58a6ff; outline-offset: 2px; }
  .icon { width: 24px; height: 24px; border-radius: 3px; flex: none; object-fit: cover; }
  .icon.pending { opacity: 0.3; }

  /* In-game-style hover tooltip -- identical treatment to the paperdoll's
     (CharacterSheet); wowhead markup arrives sanitized, hence the :global
     quality classes it carries. */
  .wow-tooltip {
    position: fixed;
    z-index: 1000;
    pointer-events: none;
    background: linear-gradient(#0a0a14f2, #10102af2);
    border: 1px solid #8f8f66;
    border-radius: 5px;
    padding: 10px 12px;
    max-width: 320px;
    font-size: 13px;
    color: #ffffff;
  }
  .wow-tooltip :global(.q) { color: #ffd100; }
  .wow-tooltip :global(.q0) { color: #9d9d9d; }
  .wow-tooltip :global(.q1) { color: #ffffff; }
  .wow-tooltip :global(.q2) { color: #1eff00; }
  .wow-tooltip :global(.q3) { color: #0070dd; }
  .wow-tooltip :global(.q4) { color: #a335ee; }
  .wow-tooltip :global(.q5) { color: #ff8000; }
  .wow-tooltip :global(.q6) { color: #e6cc80; }
  .wow-tooltip :global(.q7) { color: #00ccff; }
  .wow-tooltip :global(table) { border-collapse: collapse; }
  .wow-tooltip :global(td),
  .wow-tooltip :global(th) { padding: 0; text-align: left; }
  .wow-tooltip :global(th) { text-align: right; color: #9d9d9d; font-weight: normal; }
  .wow-tooltip :global(.whtt-extra) { color: #9d9d9d; }
  .card, .sendbox { background: #0d1117; border: 1px solid #30363d; border-radius: 8px; padding: 14px 16px; display: flex; flex-direction: column; gap: 10px; }
  .row { display: flex; gap: 10px; align-items: center; }
  .iocol { display: flex; flex-direction: column; gap: 6px; }
  .import { border-top: 1px solid #21262d; padding-top: 12px; margin-top: 4px; }
  textarea.toml { background: #010409; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 8px; font-family: ui-monospace, monospace; font-size: 12px; resize: vertical; width: 100%; box-sizing: border-box; }
  .ok-text { color: #3fb950; font-size: 13px; }
  .err-text { color: #f85149; font-size: 13px; }
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
