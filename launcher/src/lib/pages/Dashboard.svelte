<script module lang="ts">
  import type { ItemInfo } from "$lib/api";

  // Module-level: persists across Dashboard (re)instantiation -- switching
  // sidebar pages unmounts/remounts this component, and this cache must
  // survive that so re-viewing a character within the same app session
  // never re-fetches item info already seen (the CLI's own disk cache
  // covers cross-session persistence).
  const infoCache = new Map<number, ItemInfo>();
</script>

<script lang="ts">
  import { onMount } from "svelte";
  import {
    wowServerDetail,
    wowPaperdoll,
    wowItemInfo,
    type ServerDetail,
    type PaperdollData,
    type PaperdollItem,
  } from "$lib/api";
  import { QUALITY_COLORS, className } from "$lib/wow";
  import { sanitizeTooltipHtml } from "$lib/tooltip";
  import CharPicker from "$lib/CharPicker.svelte";
  import CharacterModel from "$lib/CharacterModel.svelte";

  let detail: ServerDetail | null = $state(null);
  let infoError: string | null = $state(null);
  let loadingInfo = $state(false);

  let charName = $state("");
  let doll = $state<PaperdollData | null>(null);
  let dollError: string | null = $state(null);
  let loadingDoll = $state(false);

  // infoCache is a plain Map (not deeply reactive) -- reads that depend on
  // its contents also read this counter so Svelte's reactivity notices when
  // a batch of item info lands and re-renders the affected slots/tooltip.
  let infoVersion = $state(0);

  function itemInfo(entry: number): ItemInfo | undefined {
    infoVersion;
    return infoCache.get(entry);
  }

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

  function fetchItemInfo(items: PaperdollItem[]) {
    const missing = items.map((it) => it.entry).filter((entry) => !infoCache.has(entry));
    if (missing.length === 0) return;
    // Fire-and-forget: the grid renders immediately with name-only
    // placeholders, icons/tooltips pop in when this lands. Item info must
    // NEVER break the paperdoll -- a failed call just leaves tooltips
    // degraded to name-only.
    wowItemInfo(missing)
      .then((infos) => {
        for (const info of infos) infoCache.set(info.entry, info);
        infoVersion++;
      })
      .catch(() => {});
  }

  async function loadDoll() {
    if (!charName) return;
    loadingDoll = true;
    dollError = null;
    doll = null;
    hovered = null;
    try {
      doll = await wowPaperdoll(charName);
      fetchItemInfo(doll.equipped);
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      dollError = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    } finally {
      loadingDoll = false;
    }
  }

  function bySlotMap(items: PaperdollItem[]): Map<number, PaperdollItem> {
    const map = new Map<number, PaperdollItem>();
    for (const it of items) map.set(it.slot, it);
    return map;
  }
  const bySlot = $derived(bySlotMap(doll?.equipped ?? []));

  const LEFT_SLOTS: [number, string][] = [
    [0, "Head"],
    [1, "Neck"],
    [2, "Shoulders"],
    [14, "Back"],
    [4, "Chest"],
    [3, "Shirt"],
    [18, "Tabard"],
    [8, "Wrists"],
  ];
  const RIGHT_SLOTS: [number, string][] = [
    [9, "Hands"],
    [5, "Waist"],
    [6, "Legs"],
    [7, "Feet"],
    [10, "Ring"],
    [11, "Ring"],
    [12, "Trinket"],
    [13, "Trinket"],
  ];
  const BOTTOM_SLOTS: [number, string][] = [
    [15, "Main Hand"],
    [16, "Off Hand"],
    [17, "Ranged"],
  ];

  interface Hovered {
    item: PaperdollItem;
    top: number;
    left: number | null;
    right: number | null;
  }
  let hovered: Hovered | null = $state(null);
  let tooltipEl: HTMLDivElement | undefined = $state();

  function showTooltip(e: MouseEvent | FocusEvent, item: PaperdollItem) {
    const target = e.currentTarget as HTMLElement;
    const rect = target.getBoundingClientRect();
    const flip = rect.right + 340 > window.innerWidth;
    hovered = {
      item,
      top: rect.top,
      left: flip ? null : rect.right + 8,
      right: flip ? window.innerWidth - rect.left + 8 : null,
    };
  }
  function hideTooltip() {
    hovered = null;
  }

  // Clamp the tooltip vertically once its real (post-render) height is
  // known -- the initial `top` is just the hovered slot's top edge, which
  // can run the tooltip off the bottom of the viewport for slots near the
  // bottom (e.g. Feet, Off Hand).
  $effect(() => {
    if (!hovered || !tooltipEl) return;
    const h = tooltipEl.getBoundingClientRect().height;
    const maxTop = Math.max(8, window.innerHeight - h - 8);
    const clamped = Math.min(Math.max(hovered.top, 8), maxTop);
    if (hovered.top !== clamped) hovered.top = clamped;
  });
</script>

{#snippet slotBox(slotNum: number, label: string)}
  {@const item = bySlot.get(slotNum)}
  {@const info = item ? itemInfo(item.entry) : undefined}
  <div
    class="slot"
    class:filled={!!item}
    role="button"
    style={item ? `border-color: ${QUALITY_COLORS[item.quality] ?? "#c9d1d9"}` : ""}
    tabindex={item ? 0 : -1}
    aria-label={item ? item.name : label}
    title={item ? undefined : label}
    onmouseenter={item ? (e: MouseEvent) => showTooltip(e, item) : undefined}
    onmouseleave={item ? hideTooltip : undefined}
    onfocus={item ? (e: FocusEvent) => showTooltip(e, item) : undefined}
    onblur={item ? hideTooltip : undefined}
  >
    {#if item}
      {#if info?.icon_b64}
        <img class="icon" src="data:image/jpeg;base64,{info.icon_b64}" alt={item.name} />
      {:else}
        <span class="letter" style="background: {QUALITY_COLORS[item.quality] ?? '#c9d1d9'}">
          {item.name.charAt(0).toUpperCase()}
        </span>
      {/if}
    {/if}
  </div>
{/snippet}

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
    <div class="doll-row">
      <CharacterModel {doll} />
      <div class="card doll">
        <div class="paperdoll">
          <div class="col">
            {#each LEFT_SLOTS as [slotNum, label] (slotNum)}
              {@render slotBox(slotNum, label)}
            {/each}
          </div>
          <div class="summary">
            <div class="charname">{doll.name}</div>
            <div class="muted">Level {doll.level} {className(doll.class)}</div>
            <div class="gold">{doll.gold} gold</div>
          </div>
          <div class="col">
            {#each RIGHT_SLOTS as [slotNum, label] (slotNum)}
              {@render slotBox(slotNum, label)}
            {/each}
          </div>
          <div class="bottom-row">
            {#each BOTTOM_SLOTS as [slotNum, label] (slotNum)}
              {@render slotBox(slotNum, label)}
            {/each}
          </div>
        </div>
        <p class="muted">Shown as of the character's last save — an online character can lag a little.</p>
      </div>
    </div>
  {/if}
</section>

{#if hovered}
  {@const info = itemInfo(hovered.item.entry)}
  <div
    class="wow-tooltip"
    bind:this={tooltipEl}
    style="top: {hovered.top}px; {hovered.left !== null ? `left: ${hovered.left}px;` : `right: ${hovered.right}px;`}"
  >
    {#if info?.source === "wowhead" && info.wowhead}
      {@html sanitizeTooltipHtml(info.wowhead.tooltip)}
    {:else if info?.source === "local" && info.tooltip_html}
      {@html sanitizeTooltipHtml(info.tooltip_html)}
    {:else}
      <b style="color: {QUALITY_COLORS[hovered.item.quality] ?? '#c9d1d9'}">{hovered.item.name}</b>
      <div class="whtt-extra">ilvl {hovered.item.item_level}</div>
    {/if}
  </div>
{/if}

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
  button { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 14px; cursor: pointer; }
  button:disabled { opacity: 0.5; cursor: default; }
  .muted { color: #8b949e; font-size: 13px; }
  .error-card { background: #161b22; border: 1px solid #f85149; border-radius: 8px; padding: 12px 16px; }

  /* Model card + paperdoll grid side by side; wraps to stacked on narrow
     windows since the model card has a fixed 300x380 size. */
  .doll-row { display: flex; gap: 16px; align-items: flex-start; flex-wrap: wrap; }

  /* Paperdoll grid: left slot column / summary / right slot column, bottom
     weapon row spans underneath -- no visible per-slot text labels, matching
     the in-game character pane's look (identity comes from position + the
     hover tooltip + aria-label). */
  .paperdoll {
    display: grid;
    grid-template-columns: auto 1fr auto;
    gap: 20px 32px;
    align-items: start;
    margin-top: 4px;
  }
  .col { display: flex; flex-direction: column; gap: 6px; }
  .summary { text-align: center; padding-top: 10px; }
  .charname { font-size: 15px; font-weight: 600; color: #f0f6fc; }
  .gold { color: #d4af37; font-size: 13px; margin-top: 6px; }
  .bottom-row { grid-column: 1 / -1; display: flex; justify-content: center; gap: 10px; margin-top: 2px; }

  .slot {
    box-sizing: border-box;
    width: 40px;
    height: 40px;
    background: #0d1117;
    border: 1px dashed #30363d;
    border-radius: 4px;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .slot.filled { border-style: solid; cursor: pointer; }
  .icon { width: 36px; height: 36px; object-fit: cover; border-radius: 2px; }
  .letter {
    width: 36px;
    height: 36px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 2px;
    font-size: 15px;
    font-weight: 700;
    color: #0d1117;
  }

  /* In-game-style hover tooltip -- fixed position, rendered only while a
     slot is hovered/focused, so it never shifts the paperdoll layout. */
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
</style>
