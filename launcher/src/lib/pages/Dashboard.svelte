<script module lang="ts">
  import type { ItemInfo, EntityInfo } from "$lib/api";

  // Module-level: persists across Dashboard (re)instantiation -- switching
  // sidebar pages unmounts/remounts this component, and this cache must
  // survive that so re-viewing a character within the same app session
  // never re-fetches item info already seen (the CLI's own disk cache
  // covers cross-session persistence).
  const infoCache = new Map<number, ItemInfo>();

  // Round G: same idea for spell/achievement entity-info, keyed `kind:id`
  // (a separate map -- items keep their existing by-entry-number key
  // untouched, so Round E's paperdoll/tooltip behavior is unchanged).
  const entityCache = new Map<string, EntityInfo>();
</script>

<script lang="ts">
  import { onMount } from "svelte";
  import {
    wowServerDetail,
    wowPaperdoll,
    wowItemInfo,
    wowCharProgress,
    wowEntityInfo,
    type ServerDetail,
    type PaperdollData,
    type PaperdollItem,
    type CharProgress,
    type AchievementEntry,
    type WowheadTooltip,
  } from "$lib/api";
  import { QUALITY_COLORS, className } from "$lib/wow";
  import { sanitizeTooltipHtml } from "$lib/tooltip";
  import { chunkIds, formatEpochDate } from "$lib/progress";
  import { learnedRank, treePoints, treeRows, type Tree, type Talent } from "$lib/talent-trees";
  import talentTreesJson from "$lib/talent-trees-wotlk.json";
  import CharPicker from "$lib/CharPicker.svelte";
  import CharacterModel from "$lib/CharacterModel.svelte";

  // Keyed by class id (as a string, matching the JSON's object keys) -- cast
  // once here rather than at every lookup site. The raw JSON's inferred
  // literal-key type doesn't accept a computed string index, and this data
  // is static/trusted (checked into the repo, not user input).
  const talentTreesByClass = talentTreesJson as unknown as Record<string, Tree[]>;
  function talentTreesForClass(classId: number): Tree[] {
    return talentTreesByClass[String(classId)] ?? [];
  }

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

  // Same pattern as infoVersion, for the entityCache (Round G: talents +
  // achievements).
  let entityVersion = $state(0);

  function entityInfo(kind: "spell" | "achievement", id: number): EntityInfo | undefined {
    entityVersion;
    return entityCache.get(`${kind}:${id}`);
  }

  let progress = $state<CharProgress | null>(null);
  let progressError: string | null = $state(null);
  let loadingProgress = $state(false);

  // Progressive load for a batch of spell/achievement ids: skip anything
  // already cached, split what's left into ≤25-id chunks (entity-info's
  // server-side cap), and fetch sequentially so partial results stream in
  // as tiles/rows instead of waiting on one giant call. Entirely
  // best-effort -- a failed chunk just leaves the remaining tiles as
  // pending/unavailable placeholders, it never surfaces as a card error.
  async function loadEntities(kind: "spell" | "achievement", ids: number[]) {
    const missing = ids.filter((id) => !entityCache.has(`${kind}:${id}`));
    if (missing.length === 0) return;
    try {
      for (const chunk of chunkIds(missing)) {
        const infos = await wowEntityInfo(kind, chunk);
        for (const info of infos) entityCache.set(`${kind}:${info.id}`, info);
        entityVersion++;
      }
    } catch {
      // best-effort: leave whatever didn't land as pending/unavailable
    }
  }

  async function loadProgress(name: string) {
    loadingProgress = true;
    progressError = null;
    try {
      const p = await wowCharProgress(name);
      progress = p;
      void loadEntities("spell", p.talents.spells);
      void loadEntities(
        "achievement",
        p.achievements.recent.map((a) => a.id),
      );
    } catch {
      progress = null;
      progressError = "Couldn't load progress.";
    } finally {
      loadingProgress = false;
    }
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
    progress = null;
    progressError = null;
    try {
      doll = await wowPaperdoll(charName);
      fetchItemInfo(doll.equipped);
      void loadProgress(charName);
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

  // A hover target is either a paperdoll item or a spell/achievement
  // entity tile -- the tooltip machinery below (positioning/flip, vertical
  // clamp, sanitized-HTML render) is shared across all three; only the
  // lookup + fallback text differs, resolved by `resolveHover` below.
  type HoverSource =
    | { source: "item"; item: PaperdollItem }
    | { source: "spell" | "achievement"; id: number };

  interface Hovered {
    target: HoverSource;
    top: number;
    left: number | null;
    right: number | null;
  }
  let hovered: Hovered | null = $state(null);
  let tooltipEl: HTMLDivElement | undefined = $state();

  function showTooltip(e: MouseEvent | FocusEvent, target: HoverSource) {
    const el = e.currentTarget as HTMLElement;
    const rect = el.getBoundingClientRect();
    const flip = rect.right + 340 > window.innerWidth;
    hovered = {
      target,
      top: rect.top,
      left: flip ? null : rect.right + 8,
      right: flip ? window.innerWidth - rect.left + 8 : null,
    };
  }
  function hideTooltip() {
    hovered = null;
  }

  // Resolved, render-ready view of whatever `hovered.target` currently
  // points at. Reads itemInfo()/entityInfo() (both version-gated) so it
  // stays live: if the tooltip data streams in *after* the hover started,
  // the tooltip upgrades from the plain-text fallback without needing a
  // re-hover.
  interface ResolvedHover {
    wowhead: WowheadTooltip | null;
    localHtml: string | null;
    label: string;
    color: string;
    sub: string | null;
  }
  function resolveHover(target: HoverSource): ResolvedHover {
    if (target.source === "item") {
      const info = itemInfo(target.item.entry);
      return {
        wowhead: info?.source === "wowhead" ? (info.wowhead ?? null) : null,
        localHtml: info?.source === "local" ? (info.tooltip_html ?? null) : null,
        label: target.item.name,
        color: QUALITY_COLORS[target.item.quality] ?? "#c9d1d9",
        sub: `ilvl ${target.item.item_level}`,
      };
    }
    const info = entityInfo(target.source, target.id);
    return {
      wowhead: info?.source === "wowhead" ? (info.wowhead ?? null) : null,
      localHtml: null,
      label: info?.wowhead?.name ?? `#${target.id}`,
      color: "#c9d1d9",
      sub: null,
    };
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
    onmouseenter={item ? (e: MouseEvent) => showTooltip(e, { source: "item", item }) : undefined}
    onmouseleave={item ? hideTooltip : undefined}
    onfocus={item ? (e: FocusEvent) => showTooltip(e, { source: "item", item }) : undefined}
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

{#snippet talentTile(talent: Talent, rank: number)}
  {@const spellId = talent.ranks[rank - 1]}
  {@const info = entityInfo("spell", spellId)}
  <div
    class="tile tree-tile"
    class:filled={!!info?.icon_b64}
    style="grid-row: {talent.row + 1}; grid-column: {talent.col + 1};"
    role="button"
    tabindex="0"
    aria-label={info?.wowhead?.name ?? `Spell #${spellId}`}
    title={info?.icon_b64 ? undefined : String(spellId)}
    onmouseenter={(e: MouseEvent) => showTooltip(e, { source: "spell", id: spellId })}
    onmouseleave={hideTooltip}
    onfocus={(e: FocusEvent) => showTooltip(e, { source: "spell", id: spellId })}
    onblur={hideTooltip}
  >
    {#if info?.icon_b64}
      <img class="tile-icon" src="data:image/jpeg;base64,{info.icon_b64}" alt={info.wowhead?.name ?? String(spellId)} />
    {/if}
    <span class="rank-badge" class:maxed={rank === talent.ranks.length}>{rank}/{talent.ranks.length}</span>
  </div>
{/snippet}

{#snippet talentCell(talent: Talent, rank: number)}
  {#if rank > 0}
    {@render talentTile(talent, rank)}
  {:else}
    <div
      class="tile tree-tile empty"
      style="grid-row: {talent.row + 1}; grid-column: {talent.col + 1};"
      aria-hidden="true"
    ></div>
  {/if}
{/snippet}

{#snippet treePanel(tree: Tree, learnedSet: Set<number>)}
  <div class="tree-panel">
    <div class="tree-head">{tree.name} ({treePoints(tree, learnedSet)})</div>
    <div
      class="tree-grid"
      style="grid-template-rows: repeat({Math.max(treeRows(tree), 1)}, 40px);"
    >
      {#each tree.talents as talent (talent.id)}
        {@render talentCell(talent, learnedRank(talent, learnedSet))}
      {/each}
    </div>
  </div>
{/snippet}

{#snippet achievementRow(entry: AchievementEntry)}
  {@const info = entityInfo("achievement", entry.id)}
  <div
    class="arow"
    role="button"
    tabindex="0"
    aria-label={info?.wowhead?.name ?? `Achievement #${entry.id}`}
    onmouseenter={(e: MouseEvent) => showTooltip(e, { source: "achievement", id: entry.id })}
    onmouseleave={hideTooltip}
    onfocus={(e: FocusEvent) => showTooltip(e, { source: "achievement", id: entry.id })}
    onblur={hideTooltip}
  >
    <div class="arow-icon" class:filled={!!info?.icon_b64}>
      {#if info?.icon_b64}
        <img src="data:image/jpeg;base64,{info.icon_b64}" alt="" />
      {/if}
    </div>
    <span class="arow-name">{info?.wowhead?.name ?? `#${entry.id}`}</span>
    <span class="arow-date muted">{formatEpochDate(entry.date)}</span>
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
            <CharacterModel {doll} />
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

    <div class="progress-row">
      <div class="card talents-card">
        <div class="card-head">
          <h3>Talents</h3>
          {#if progress && progress.talents.groups_count > 1}
            <span class="badge">Dual spec</span>
          {/if}
        </div>
        {#if progressError}
          <p class="muted">{progressError}</p>
        {:else if loadingProgress && !progress}
          <p class="muted">Loading…</p>
        {:else if progress}
          {@const learnedSet = new Set(progress.talents.spells)}
          {@const trees = talentTreesForClass(doll.class)}
          {@const pointsPerTree = trees.map((t) => treePoints(t, learnedSet))}
          <p class="muted">
            {pointsPerTree.reduce((a, b) => a + b, 0)} points — {pointsPerTree.join("/")}
          </p>
          <div class="tree-row">
            {#each trees as tree (tree.id)}
              {@render treePanel(tree, learnedSet)}
            {/each}
          </div>
        {/if}
      </div>

      <div class="card">
        <div class="card-head">
          <h3>Achievements</h3>
        </div>
        {#if progressError}
          <p class="muted">{progressError}</p>
        {:else if loadingProgress && !progress}
          <p class="muted">Loading…</p>
        {:else if progress}
          <p class="muted">{progress.achievements.total} earned</p>
          <div class="arow-list">
            {#each progress.achievements.recent.slice(0, 10) as entry (entry.id)}
              {@render achievementRow(entry)}
            {/each}
          </div>
        {/if}
      </div>
    </div>
  {/if}
</section>

{#if hovered}
  {@const view = resolveHover(hovered.target)}
  <div
    class="wow-tooltip"
    bind:this={tooltipEl}
    style="top: {hovered.top}px; {hovered.left !== null ? `left: ${hovered.left}px;` : `right: ${hovered.right}px;`}"
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

  /* The 3D model lives INSIDE the gear window's center cell (like the
     in-game character pane); this row is just the card's flex home. */
  .doll-row { display: flex; gap: 16px; align-items: flex-start; flex-wrap: wrap; }
  .summary :global(.model-card) { margin: 10px auto 0; }

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

  /* Talents/Achievements cards, side by side beneath the model+paperdoll
     row; wraps to stacked on narrow windows like .doll-row. */
  .progress-row { display: flex; gap: 16px; align-items: flex-start; flex-wrap: wrap; }
  .progress-row .card { flex: 1 1 320px; min-width: 280px; }
  /* Three 4-col/40px tree panels need more room than the achievements
     card's single icon+text list -- give it a bigger share of the row. */
  .progress-row .talents-card { flex: 2 1 560px; min-width: 560px; }
  .card-head { display: flex; align-items: center; gap: 8px; margin-bottom: 4px; }
  .card-head h3 { margin: 0; font-size: 14px; color: #f0f6fc; }
  .badge {
    font-size: 11px;
    color: #d4af37;
    border: 1px solid #d4af37;
    border-radius: 4px;
    padding: 1px 6px;
  }

  /* Talent tiles: same visual language as paperdoll slots (dashed border
     placeholder / solid once an icon lands). Base size is the achievement-
     row icon size; the in-game-style talent tree grid below sizes its
     tiles up to 40px via .tree-tile. */
  .tile {
    box-sizing: border-box;
    width: 28px;
    height: 28px;
    background: #0d1117;
    border: 1px dashed #30363d;
    border-radius: 4px;
    cursor: pointer;
  }
  .tile.filled { border-style: solid; }
  .tile.empty { cursor: default; }
  .tile-icon { width: 100%; height: 100%; object-fit: cover; border-radius: 2px; }

  /* In-game-style talent trees: three tree panels side by side (wrap on
     narrow), each a 4-column CSS grid positioned by the data's row/col
     (not element order), matching the paperdoll slot's visual language at
     a slightly larger (40px) size to read like the in-game panel. */
  .tree-row { display: flex; gap: 20px; flex-wrap: wrap; margin-top: 8px; }
  .tree-head { font-size: 12px; font-weight: 600; color: #f0f6fc; margin-bottom: 6px; }
  .tree-grid { display: grid; grid-template-columns: repeat(4, 40px); gap: 4px; }
  .tree-tile { width: 40px; height: 40px; position: relative; }
  .rank-badge {
    position: absolute;
    bottom: -3px;
    right: -3px;
    font-size: 9px;
    font-weight: 700;
    line-height: 1;
    color: #3fb950;
    background: #0d1117;
    border: 1px solid #3fb950;
    border-radius: 3px;
    padding: 1px 3px;
  }
  .rank-badge.maxed { color: #d4af37; border-color: #d4af37; }

  .arow-list { display: flex; flex-direction: column; gap: 6px; margin-top: 8px; }
  .arow { display: flex; align-items: center; gap: 8px; cursor: pointer; }
  .arow-icon {
    box-sizing: border-box;
    width: 20px;
    height: 20px;
    flex-shrink: 0;
    background: #0d1117;
    border: 1px dashed #30363d;
    border-radius: 4px;
  }
  .arow-icon.filled { border-style: solid; }
  .arow-icon img { width: 100%; height: 100%; object-fit: cover; border-radius: 2px; }
  .arow-name { flex: 1; font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .arow-date { flex-shrink: 0; text-align: right; }
</style>
