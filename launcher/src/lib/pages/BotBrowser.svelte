<script lang="ts">
  import { onMount } from "svelte";
  import {
    wowPartyOnline,
    wowPartyRelogin,
    wowGmLevel,
    type BotRow,
    type BotsPage,
    type OnlineChar,
  } from "$lib/api";
  import { loadBotsPage } from "$lib/page-cache.svelte";
  import { className } from "$lib/wow";
  import {
    loadFavs,
    saveFavs,
    toggleFav,
    sortWithFavs,
    pageInfo,
    zoneName,
    levelFilter,
    levelValid,
  } from "$lib/bot-browser";
  import { featureLocked, LOCKED_HINT } from "$lib/features.svelte";
  import CharacterSheet from "$lib/CharacterSheet.svelte";

  const CLASS_IDS = [1, 2, 3, 4, 5, 6, 7, 8, 9, 11];
  const PAGE_SIZE = 50;

  // Filters (Items.svelte pattern: form -> search()).
  let name = $state("");
  let classId = $state<string>("");
  let minLevel = $state<string | number>("");
  let maxLevel = $state<string | number>("");
  let onlineOnly = $state(false);

  // Generic-param $state style (see Dashboard.svelte's achievementsData
  // comment): `let x: T | null = $state(null)` defeats TS narrowing under
  // this project's svelte-check version.
  let page = $state<BotsPage | null>(null);
  let offset = $state(0);
  let searched = $state(false);
  let searching = $state(false);
  let error: string | null = $state(null);

  let favs: string[] = $state(loadFavs());

  // Details row (one bot at a time): embeds a COMPACT CharacterSheet --
  // the sheet fetches its own paperdoll/talents/achievements, so no
  // detail-fetching state lives here anymore.
  let detailBot = $state<BotRow | null>(null);

  // In-place full-size character view (user request 2026-07-22: "Open full
  // view" must NOT navigate to the Characters tab). While set, the list is
  // hidden -- but the component (and so every filter, the current page and
  // favorites) stays mounted, and Back simply returns to it intact.
  let fullView = $state<BotRow | null>(null);

  // Actions (Playerbots.svelte pattern: online-player select, busy, note).
  let online: OnlineChar[] = $state([]);
  let master = $state("");
  let busy = $state(false);
  let note: string | null = $state(null);
  let levelInput = $state("");

  function showErr(e: unknown) {
    const err = e as { message?: string; hint?: string };
    error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
  }

  async function search(newOffset = 0) {
    searching = true;
    error = null;
    note = null;
    try {
      page = await loadBotsPage({
        name: name.trim() || undefined,
        class: classId === "" ? undefined : Number(classId),
        minLevel: levelFilter(minLevel),
        maxLevel: levelFilter(maxLevel),
        online: onlineOnly || undefined,
        limit: PAGE_SIZE,
        offset: newOffset,
      });
      offset = newOffset;
      searched = true;
    } catch (e) {
      showErr(e);
    } finally {
      searching = false;
    }
  }

  const info = $derived(page ? pageInfo(page.total, page.limit, page.offset) : null);
  const shownRows = $derived(page ? sortWithFavs(page.bots, favs) : []);

  function starToggle(botName: string) {
    favs = toggleFav(favs, botName);
    saveFavs(favs);
  }

  async function refreshOnline() {
    try {
      online = await wowPartyOnline();
      if (!online.find((o) => o.name === master)) master = online[0]?.name ?? "";
    } catch {
      online = [];
      master = "";
    }
  }
  onMount(() => {
    void refreshOnline();
    // Smoke item 4a: fire the default search on open so the page starts
    // populated -- the Search button stays as the refine action.
    void search(0);
  });

  function toggleDetail(bot: BotRow) {
    detailBot = detailBot?.guid === bot.guid ? null : bot;
    levelInput = "";
  }

  async function invite(bot: BotRow) {
    const p = master;
    if (!p) return;
    busy = true;
    error = null;
    note = null;
    try {
      await wowPartyRelogin(p, bot.name);
      note = `Asked ${bot.name} to log in and join ${p}'s party — give it a moment. If nothing happens, see the permission note in the bot's Details.`;
    } catch (e) {
      showErr(e);
    } finally {
      busy = false;
    }
  }

  async function setLevel(bot: BotRow) {
    const n = Number(levelInput);
    busy = true;
    error = null;
    note = null;
    try {
      await wowGmLevel(bot.name, n);
      note = `${bot.name} is now level ${n} (list data updates on the bot's next save).`;
    } catch (e) {
      showErr(e);
    } finally {
      busy = false;
    }
  }
</script>

<section class="content">
  {#if fullView}
    <!-- In-place full-size character view: the list below stays mounted
         (its state intact), Back just clears this. -->
    <header class="bar fullbar">
      <button onclick={() => (fullView = null)}>‹ Back to list</button>
      <h2>{fullView.name}</h2>
      <span class="muted">Level {fullView.level} {className(fullView.class)}</span>
    </header>
    <CharacterSheet name={fullView.name} />
  {:else}
  <header class="bar"><h2>Bot Browser</h2></header>

  <form class="filters" onsubmit={(e) => { e.preventDefault(); search(0); }}>
    <input placeholder="Name prefix" maxlength="12" bind:value={name} />
    <select bind:value={classId}>
      <option value="">Any class</option>
      {#each CLASS_IDS as c (c)}
        <option value={String(c)}>{className(c)}</option>
      {/each}
    </select>
    <input placeholder="Min lvl" size="6" type="number" min="1" max="255" step="1" bind:value={minLevel} />
    <input placeholder="Max lvl" size="6" type="number" min="1" max="255" step="1" bind:value={maxLevel} />
    <label class="chk"><input type="checkbox" bind:checked={onlineOnly} /> online only</label>
    <button class="primary" type="submit" disabled={searching}>Search</button>
  </form>

  {#if error}<div class="error-card"><p>{error}</p></div>{/if}
  {#if note}<p class="muted">{note}</p>{/if}

  {#if searched && page && page.total === 0 && !error}
    <p class="muted">No bots matched.</p>
  {/if}

  {#if page && page.total > 0 && info}
    <div class="pagerow">
      <span class="muted">{page.total} bots — page {info.page} of {info.pages}</span>
      <button onclick={() => search(Math.max(0, offset - PAGE_SIZE))} disabled={searching || !info.hasPrev}>‹ Prev</button>
      <button onclick={() => search(offset + PAGE_SIZE)} disabled={searching || !info.hasNext}>Next ›</button>
    </div>
    <table>
      <thead><tr><th></th><th>Name</th><th>Class</th><th>Level</th><th>Zone</th><th>Online</th><th></th></tr></thead>
      <tbody>
        {#each shownRows as b (b.guid)}
          <tr class:favrow={favs.includes(b.name)}>
            <td><button class="star" class:faved={favs.includes(b.name)} title={favs.includes(b.name) ? "Remove favorite" : "Add favorite"} onclick={() => starToggle(b.name)}>{favs.includes(b.name) ? "★" : "☆"}</button></td>
            <td>{b.name}</td>
            <td class="muted">{className(b.class)}</td>
            <td>{b.level}</td>
            <td class="muted">{zoneName(b.zone)}</td>
            <td>
              <span
                class="dot"
                class:on={b.online}
                class:off={!b.online}
                role="img"
                aria-label={b.online ? "online" : "offline"}
                title={b.online ? "Online" : "Offline"}
              ></span>
            </td>
            <td><button onclick={() => toggleDetail(b)}>{detailBot?.guid === b.guid ? "Hide" : "Details"}</button></td>
          </tr>
          {#if detailBot?.guid === b.guid}
            <tr><td colspan="7">
              <div class="card detail">
                <!-- Compact embedded character sheet (user request
                     2026-07-22): the same tabbed view as the Character
                     page, sized down -- it fetches and renders its own
                     gear/model/talents/achievements. -->
                <CharacterSheet name={b.name} compact />

                <div class="actions">
                  <button
                    onclick={() => (fullView = b)}
                    title="Show the full-size character view — stays on this page; Back returns to the list"
                  >
                    Open full view
                  </button>
                  <span class="sep"></span>
                  <label class="muted">Invite to
                    <select bind:value={master} disabled={busy || online.length === 0}>
                      {#each online as o (o.guid)}<option value={o.name}>{o.name}</option>{/each}
                    </select>
                  </label>
                  <button
                    onclick={() => invite(b)}
                    disabled={busy || !master || featureLocked("bot-browser")}
                    title={featureLocked("bot-browser")
                      ? LOCKED_HINT
                      : online.length === 0
                        ? "Log a character into the game first"
                        : "Logs this bot in and adds it to the selected player's party (needs the bot bridge deployed)"}
                  >
                    Invite to party
                  </button>
                  {#if online.length === 0}<span class="muted small">log a character in first</span>{/if}
                  <span class="sep"></span>
                  <input class="lvl-input" type="number" min="1" max="255" placeholder="lvl" bind:value={levelInput} disabled={busy} />
                  <button
                    onclick={() => setLevel(b)}
                    disabled={busy || !levelValid(levelInput) || featureLocked("bot-browser")}
                    title={featureLocked("bot-browser") ? LOCKED_HINT : "Works even while the bot is offline"}
                  >
                    Set level
                  </button>
                </div>
                <!-- Smoke item 4c: the in-game "not allowed to control" denial is
                     invisible to the launcher (it lands in the master's chat), so
                     name the real mod-playerbots gate up front instead of leaving
                     a mystery. -->
                <p class="muted small">
                  If the game whispers "you are not allowed to control bot …", that's the
                  server's playerbots permission gate: only bots made by My Party's Add bot,
                  bots on your own account, or bots in your guild can be invited this way
                  (mod-playerbots AiPlayerbot.AllowGuildBots / AllowAccountBots).
                </p>
              </div>
            </td></tr>
          {/if}
        {/each}
      </tbody>
    </table>
  {/if}

  {#if !searched}
    <p class="muted">{searching ? "Loading the bot list…" : "Browse the ~2500 ambient bots: filter by name, class or level, star your favorites, open Details for gear and talents."}</p>
  {/if}
  {/if}
</section>

<style>
  .content { padding: 20px 24px; overflow-y: auto; display: flex; flex-direction: column; gap: 16px; }
  .bar h2 { margin: 0; font-size: 18px; }
  .filters { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  input, select { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 8px; }
  label.chk { color: #8b949e; font-size: 14px; display: flex; gap: 6px; align-items: center; }
  .pagerow { display: flex; gap: 10px; align-items: center; }
  table { border-collapse: collapse; }
  th { text-align: left; color: #8b949e; font-size: 13px; padding: 4px 14px 4px 0; }
  td { padding: 4px 14px 4px 0; font-size: 14px; border-top: 1px solid #21262d; }
  tr.favrow td { background: #10161d; }
  button { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 14px; cursor: pointer; }
  button.primary { background: #238636; border-color: #2ea043; color: white; }
  button:disabled { opacity: 0.5; cursor: default; }
  button.star { padding: 2px 8px; font-size: 15px; border: none; background: none; color: #6e7681; }
  button.star.faved { color: #d29922; }
  /* Row online-status dot: same visual language as the sidebar status
     chip's dot (+page.svelte) -- green = online, red = offline. */
  .dot { width: 9px; height: 9px; border-radius: 50%; display: inline-block; flex-shrink: 0; }
  .dot.on { background: #3fb950; }
  .dot.off { background: #f85149; }
  .card.detail { background: #0d1117; border: 1px solid #30363d; border-radius: 8px; padding: 14px 16px; display: flex; flex-direction: column; gap: 10px; margin: 6px 0; }
  /* Full-view header: Back button + name + level/class on one line. */
  .fullbar { display: flex; gap: 12px; align-items: center; }
  .actions { display: flex; gap: 10px; align-items: center; flex-wrap: wrap; }
  .actions .sep { width: 12px; }
  input.lvl-input { width: 56px; }
  .muted { color: #8b949e; }
  .muted.small { font-size: 12px; }
  .error-card { background: #161b22; border: 1px solid #f85149; border-radius: 8px; padding: 12px 16px; }
</style>
