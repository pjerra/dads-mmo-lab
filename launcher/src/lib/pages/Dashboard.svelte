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
  import {
    wowPaperdoll,
    wowItemInfo,
    wowCharProgress,
    wowEntityInfo,
    wowAchievements,
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
  import {
    categoryTree,
    scopeAchievements,
    earnedPoints,
    type AchievementCategory,
    type AchievementDef,
  } from "$lib/achievements";
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

  // Task P2: tabbed character window. Default/reset target is always
  // "character" -- a fresh character load must land there, never leave a
  // previous character's Achievements/Talents tab selected.
  let activeTab: "character" | "talents" | "achievements" = $state("character");
  function setTab(tab: "character" | "talents" | "achievements") {
    activeTab = tab;
    if (tab === "achievements") void ensureAchievementsData();
  }

  // The 217KB achievements-wotlk.json is dynamic-imported (not a static
  // top-level import) so its parse cost never lands on app startup or the
  // critical Dashboard bundle chunk -- it's only pulled in the first time
  // someone actually opens the Achievements tab, and cached module-level
  // (survives Dashboard remounts, like infoCache/entityCache above) so
  // later tab switches / character loads never re-import it.
  interface AchievementsData {
    categories: AchievementCategory[];
    achievements: AchievementDef[];
  }
  // Generic-param $state style (matches `doll`/`progress` above) rather
  // than annotating the `let` -- the latter defeats TS narrowing for
  // property access on this variable under this project's svelte-check
  // version (confirmed via isolated repro), the former doesn't.
  let achievementsData = $state<AchievementsData | null>(null);
  let loadingAchData = $state(false);
  // Review finding: silent failure here made a broken tab indistinguishable
  // from "no achievements" -- both paths now surface a muted note, matching
  // this file's dollError/progressError convention.
  let achError: string | null = $state(null);
  async function ensureAchievementsData() {
    if (achievementsData || loadingAchData) return;
    loadingAchData = true;
    try {
      const mod = await import("$lib/achievements-wotlk.json");
      achievementsData = mod.default as unknown as AchievementsData;
    } catch {
      achError = "Couldn't load the achievement list.";
    } finally {
      loadingAchData = false;
    }
  }

  // The character's COMPLETE earned-achievement list (distinct from
  // progress.achievements.recent above, which is only the 10 most recent
  // and stays as-is to keep pre-warming entityCache). Fetched once per
  // character alongside loadProgress.
  let earnedAchievements: AchievementEntry[] = $state([]);
  async function loadEarnedAchievements(name: string) {
    try {
      const r = await wowAchievements(name);
      // Stale-response guard: a slow response for a previous character must
      // not overwrite the current one's earned set (same pattern as
      // Config.svelte's loadFile).
      if (charName !== name) return;
      earnedAchievements = r.earned;
      achError = null;
    } catch {
      if (charName !== name) return;
      earnedAchievements = [];
      achError = "Couldn't load earned achievements — everything may show as unearned.";
    }
  }
  const earnedSet = $derived(new Set(earnedAchievements.map((e) => e.id)));
  const earnedDateById = $derived(new Map(earnedAchievements.map((e) => [e.id, e.date])));

  const totalAchPoints = $derived(
    achievementsData ? earnedPoints(achievementsData.achievements, earnedSet) : 0,
  );
  const earnedAchCount = $derived(
    achievementsData ? achievementsData.achievements.filter((a) => earnedSet.has(a.id)).length : 0,
  );
  const totalAchCount = $derived(achievementsData ? achievementsData.achievements.length : 0);
  const catTree = $derived(achievementsData ? categoryTree(achievementsData.categories) : []);

  // Selected left-rail category; null until the dataset has loaded and a
  // default (first root) has been picked below.
  let selectedCatId = $state<number | null>(null);

  // Achievement rows for the currently selected scope only -- never the
  // full 1320. Faction filtering is a known simplification: the launcher
  // has no way to know the viewed character's faction here, so every row
  // is shown regardless of the `faction` field (matches the brief: -1
  // always shown, 0/1 "show both" rather than guessing/hiding either).
  const scopeList = $derived(
    achievementsData && selectedCatId !== null
      ? scopeAchievements(achievementsData.achievements, selectedCatId, achievementsData.categories)
      : [],
  );

  // Selecting a category fetches icons ONLY for that scope's achievements
  // (chunked ≤25 via the existing loadEntities, which already skips
  // anything already cached) -- never all 1320 up front, and re-selecting
  // a previously-viewed category costs nothing.
  function selectCategory(catId: number) {
    selectedCatId = catId;
    if (!achievementsData) return;
    const scope = scopeAchievements(achievementsData.achievements, catId, achievementsData.categories);
    void loadEntities(
      "achievement",
      scope.map((a) => a.id),
    );
  }

  // Default selection once the dataset has loaded: the first root
  // category, in-game style. Guarded on selectedCatId === null so this
  // only ever fires once per dataset load / per character reset, never on
  // every render.
  $effect(() => {
    if (achievementsData && selectedCatId === null && catTree.length > 0) {
      selectCategory(catTree[0].root.id);
    }
  });

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
    // Task P2: a fresh character load always lands on the Character tab
    // and drops the previous character's earned-achievement state/category
    // selection -- the achievements DATASET itself (categories/defs) is
    // character-independent and stays cached, only the per-character
    // earned list and which category was being browsed reset.
    activeTab = "character";
    earnedAchievements = [];
    selectedCatId = null;
    achError = null;
    try {
      doll = await wowPaperdoll(charName);
      fetchItemInfo(doll.equipped);
      void loadProgress(charName);
      void loadEarnedAchievements(charName);
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

{#snippet achBrowserRow(a: AchievementDef)}
  {@const info = entityInfo("achievement", a.id)}
  {@const earnedDate = earnedDateById.get(a.id)}
  {@const isEarned = earnedSet.has(a.id)}
  <div
    class="abrow"
    class:earned={isEarned}
    role="button"
    tabindex="0"
    aria-label={a.name}
    onmouseenter={(e: MouseEvent) => showTooltip(e, { source: "achievement", id: a.id })}
    onmouseleave={hideTooltip}
    onfocus={(e: FocusEvent) => showTooltip(e, { source: "achievement", id: a.id })}
    onblur={hideTooltip}
  >
    <div class="abrow-icon" class:filled={!!info?.icon_b64}>
      {#if info?.icon_b64}
        <img src="data:image/jpeg;base64,{info.icon_b64}" alt="" />
      {/if}
    </div>
    <div class="abrow-text">
      <span class="abrow-name">{a.name}</span>
      <span class="abrow-desc muted">{a.desc}</span>
    </div>
    <span class="abrow-points">{a.points}</span>
    <span class="abrow-date muted">{isEarned && earnedDate ? formatEpochDate(earnedDate) : ""}</span>
  </div>
{/snippet}

<section class="content">
  <!-- The server-status card that used to sit here moved to the global
       sidebar chip + Home card (Round Q) -- duplicating it on every page
       was noise, per user feedback. -->
  <header class="bar"><h2>Character viewer</h2></header>
  <div class="pickrow">
    <CharPicker bind:selected={charName} />
    <button onclick={loadDoll} disabled={!charName || loadingDoll}>Show gear</button>
  </div>
  {#if dollError}
    <div class="error-card"><strong>Couldn't load character gear.</strong><p>{dollError}</p></div>
  {:else if doll}
    <div class="tabbed-window">
      <div class="tabstrip">
        <button class:active={activeTab === "character"} onclick={() => setTab("character")}>Character</button>
        <button class:active={activeTab === "talents"} onclick={() => setTab("talents")}>Talents</button>
        <button class:active={activeTab === "achievements"} onclick={() => setTab("achievements")}>Achievements</button>
      </div>

      {#if activeTab === "character"}
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
      {:else if activeTab === "talents"}
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
      {:else}
        <div class="card ach-browser">
          <div class="ach-header">
            {#if loadingAchData}
              <p class="muted">Loading achievements…</p>
            {:else if achievementsData}
              <p class="muted">{totalAchPoints} points · {earnedAchCount} of {totalAchCount}</p>
            {/if}
            {#if achError}
              <p class="muted err-note">{achError}</p>
            {/if}
          </div>
          {#if achievementsData}
            <div class="ach-body">
              <div class="ach-rail">
                {#each catTree as node (node.root.id)}
                  <button
                    class="cat-root"
                    class:active={selectedCatId === node.root.id}
                    onclick={() => selectCategory(node.root.id)}
                  >
                    {node.root.name}
                  </button>
                  {#each node.children as child (child.id)}
                    <button
                      class="cat-child"
                      class:active={selectedCatId === child.id}
                      onclick={() => selectCategory(child.id)}
                    >
                      {child.name}
                    </button>
                  {/each}
                {/each}
              </div>
              <div class="ach-list">
                {#each scopeList as a (a.id)}
                  {@render achBrowserRow(a)}
                {/each}
              </div>
            </div>
          {/if}
        </div>
      {/if}
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

  /* Task P2: Character/Talents/Achievements tabbed window -- replaces the
     old side-by-side .doll-row + .progress-row layout. Horizontal tab
     strip in the sidebar nav's visual language (active = bright text +
     accent underline instead of the sidebar's left border, since this
     strip is horizontal); each tab's own content keeps its own card
     chrome below the strip (matching Config.svelte's dark-card/#30363d
     language) rather than the whole window being one big outer card. */
  .tabbed-window { display: flex; flex-direction: column; gap: 12px; }
  .tabstrip { display: flex; gap: 4px; border-bottom: 1px solid #30363d; }
  .tabstrip button {
    background: none;
    border: none;
    border-bottom: 2px solid transparent;
    border-radius: 0;
    padding: 8px 16px;
    color: #8b949e;
    font-size: 14px;
  }
  .tabstrip button.active { color: #f0f6fc; border-bottom-color: #58a6ff; }
  .tabstrip button:hover:not(.active) { color: #c9d1d9; }

  /* 590px = three 172px tree grids + two 20px gaps + 32px card padding --
     the floor at which all three trees genuinely fit side by side. No
     longer needs a flex-basis (it's the sole child of its tab now, not
     racing an achievements card for a shared row). */
  .talents-card { min-width: 590px; }
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

  /* Achievements-tab in-game-style browser: left rail of root/child
     categories, scrollable row list on the right (capped ~60vh so the
     page doesn't grow unbounded -- the character/talents tabs don't need
     this since they're bounded by their own fixed grids). */
  .ach-header p { margin: 0; }
  .err-note { color: #f85149; }
  .ach-body { display: flex; gap: 16px; align-items: flex-start; }
  .ach-rail {
    flex: 0 0 200px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 60vh;
    overflow-y: auto;
  }
  .ach-rail button {
    background: none;
    border: none;
    text-align: left;
    padding: 6px 10px;
    color: #8b949e;
    font-size: 13px;
    border-radius: 4px;
  }
  .ach-rail button.cat-child { padding-left: 22px; font-size: 12px; }
  .ach-rail button:hover:not(.active) { color: #c9d1d9; background: #161b22; }
  .ach-rail button.active { color: #f0f6fc; background: #161b22; }

  .ach-list {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-height: 60vh;
    overflow-y: auto;
  }
  .abrow { display: flex; align-items: center; gap: 10px; padding: 6px 8px; border-radius: 4px; cursor: pointer; }
  .abrow:hover { background: #161b22; }
  /* Unearned rows read as locked/incomplete, in-game style: dimmed row +
     grayscale icon; earned rows are full-color (the default). */
  .abrow:not(.earned) { opacity: 0.45; }
  .abrow-icon {
    box-sizing: border-box;
    width: 32px;
    height: 32px;
    flex-shrink: 0;
    background: #0d1117;
    border: 1px dashed #30363d;
    border-radius: 4px;
  }
  .abrow-icon.filled { border-style: solid; }
  .abrow-icon img { width: 100%; height: 100%; object-fit: cover; border-radius: 2px; }
  .abrow:not(.earned) .abrow-icon img { filter: grayscale(1); }
  .abrow-text { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
  .abrow-name { font-size: 13px; color: #f0f6fc; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  /* Explicit font-size (overrides .muted's 13px, same cascade position
     wins since this rule comes later in the sheet) so the description
     reads visibly smaller than the achievement name, per the in-game
     achievement pane's look. */
  .abrow-desc { font-size: 11px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .abrow-points { flex-shrink: 0; font-size: 12px; color: #d4af37; font-weight: 600; min-width: 24px; text-align: right; }
  .abrow-date { flex-shrink: 0; min-width: 74px; text-align: right; }
</style>
