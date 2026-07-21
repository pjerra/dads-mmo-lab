<script lang="ts">
  import { onMount } from "svelte";
  import { wowStats, type WowStats } from "$lib/api";
  import { serverStatus, containersExist } from "$lib/server-status.svelte";
  import { className } from "$lib/wow";
  import zoneNamesJson from "$lib/zone-names-wotlk.json";
  import {
    avgGuildSize,
    continentName,
    fillLevelBuckets,
    formatBootDate,
    formatGold,
    formatLastSeen,
    formatPlaytime,
    formatYears,
    levelBucketLabel,
    pct,
    zoneName,
  } from "$lib/stats";

  const zoneNames = zoneNamesJson as Record<string, string>;

  let stats: WowStats | null = $state(null);
  let loading = $state(false);
  let error: string | null = $state(null);

  // One `dml wow stats` call per page visit + the Refresh button --
  // deliberately NOT hooked to the 7s status poll (16 queries per tick
  // would be pointless load on the game DB).
  async function refresh() {
    if (loading) return;
    loading = true;
    try {
      stats = await wowStats();
      error = null;
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    void refresh();
  });

  // $derived.by closures (not inline $derived expressions): at the top level
  // TS flow-narrows `stats` to null right after its $state(null) init, which
  // makes an inline `stats ? ...` truthy branch type to `never`. Inside a
  // closure the declared union type applies and the guard narrows normally.
  const levels = $derived.by(() => (stats ? fillLevelBuckets(stats.population.levels) : []));
  const levelMax = $derived.by(() => Math.max(1, ...levels.map((l) => l.family + l.bots)));
  const classMax = $derived.by(() =>
    stats ? Math.max(1, ...stats.population.classes.map((c) => c.count)) : 1,
  );
  const zoneMax = $derived.by(() =>
    stats ? Math.max(1, ...stats.botwatch.zones.map((z) => z.count)) : 1,
  );
  const bootMax = $derived.by(() =>
    stats ? Math.max(1, ...stats.history.recent.map((b) => b.uptime)) : 1,
  );
  const factionTotal = $derived.by(() =>
    stats ? stats.population.factions.alliance + stats.population.factions.horde : 0,
  );
</script>

<section class="content">
  <header class="bar">
    <h2>Statistics</h2>
    <button onclick={refresh} disabled={loading}>Refresh</button>
  </header>

  {#if !stats}
    {#if loading}
      <p class="muted">Reading the record books…</p>
    {:else if error}
      {#if serverStatus.detail?.verdict === "online"}
        <div class="error-card"><strong>Couldn't read statistics.</strong><p>{error}</p></div>
      {:else if containersExist(serverStatus.detail)}
        <p class="muted">The server looks stopped — start it from Home to see statistics.</p>
      {:else}
        <p class="muted">No statistics yet — is the server installed?</p>
      {/if}
    {/if}
  {:else}
    {#if error}
      <p class="muted stale-note">Refresh failed ({error}) — showing the last loaded numbers.</p>
    {/if}

    <!-- 1: World Population -->
    <div class="card">
      <div class="card-title"><strong>World population</strong></div>
      <div class="tiles">
        <div class="tile">
          <span class="big">{stats.population.family.total}</span>
          <span class="tlabel">family characters</span>
          <span class="tsub">{stats.population.family.online} online now</span>
        </div>
        <div class="tile">
          <span class="big">{stats.population.bots.total}</span>
          <span class="tlabel">bot characters</span>
          <span class="tsub">{stats.population.bots.online} online now</span>
        </div>
        <div class="tile">
          <span class="big">{stats.population.guilds.count}</span>
          <span class="tlabel">guilds</span>
          <span class="tsub">about {avgGuildSize(stats.population.guilds.members, stats.population.guilds.count)} members each</span>
        </div>
      </div>

      <h4>Levels across the world</h4>
      <div class="colchart">
        {#each levels as l (l.bucket)}
          <div class="col" title="{levelBucketLabel(l.bucket)}: {l.family} family, {l.bots} bots">
            <span class="colcount">{l.family + l.bots}</span>
            <div class="colbar">
              <div class="colfill bots" style="height: {pct(l.bots, levelMax)}%"></div>
              <div class="colfill fam" style="height: {pct(l.family, levelMax)}%"></div>
            </div>
            <span class="collabel">{levelBucketLabel(l.bucket)}</span>
          </div>
        {/each}
      </div>
      <p class="legend">
        <span class="swatch fam"></span> family
        <span class="swatch bots"></span> bots
      </p>

      <h4>Classes</h4>
      <div class="hbars">
        {#each stats.population.classes as c (c.class)}
          <div class="hbar-row">
            <span class="hbar-name">{className(c.class)}</span>
            <div class="hbar-track"><div class="hbar-fill" style="width: {pct(c.count, classMax)}%"></div></div>
            <span class="hbar-val">{c.count}</span>
          </div>
        {/each}
      </div>

      {#if factionTotal > 0}
        <h4>Factions</h4>
        <div class="faction-bar">
          <div class="fside alliance" style="width: {pct(stats.population.factions.alliance, factionTotal)}%"></div>
          <div class="fside horde" style="width: {pct(stats.population.factions.horde, factionTotal)}%"></div>
        </div>
        <div class="faction-labels">
          <span class="alliance-text">Alliance · {stats.population.factions.alliance}</span>
          <span class="horde-text">Horde · {stats.population.factions.horde}</span>
        </div>
      {/if}

      {#if stats.population.top_levels.length > 0}
        <h4>Highest levels</h4>
        <div class="chips">
          {#each stats.population.top_levels as t (t.name)}
            <span class="chip">
              <strong>{t.name}</strong>
              <span class="muted">lvl {t.level}</span>
              {#if t.family}<span class="fam-badge">family</span>{/if}
            </span>
          {/each}
        </div>
      {/if}
    </div>

    <!-- 2: Economy -->
    <div class="card">
      <div class="card-title"><strong>Economy</strong></div>
      <div class="tiles">
        <div class="tile">
          <span class="big">{formatGold(stats.economy.copper.family)}</span>
          <span class="tlabel">family gold</span>
        </div>
        <div class="tile">
          <span class="big">{formatGold(stats.economy.copper.bots)}</span>
          <span class="tlabel">bot gold</span>
        </div>
        <div class="tile">
          <span class="big">{formatGold(stats.economy.copper.total)}</span>
          <span class="tlabel">everyone together</span>
        </div>
      </div>

      {#if stats.economy.richest.length > 0}
        <h4>Richest characters</h4>
        <div class="chips">
          {#each stats.economy.richest as r (r.name)}
            <span class="chip">
              <strong>{r.name}</strong>
              <span class="muted">{formatGold(r.copper)}</span>
              {#if r.family}<span class="fam-badge">family</span>{/if}
            </span>
          {/each}
        </div>
      {/if}

      <!-- The AH on this server is stocked entirely by the auction-house
           bot -- this is shop stock, never player listings. -->
      <div class="factline">
        Auction house shop stock: <strong>{stats.economy.auction.count.toLocaleString("en-US")}</strong> items,
        asking <strong>{formatGold(stats.economy.auction.buyout)}</strong> in total.
      </div>
      <div class="factline">
        Post office: <strong>{stats.economy.mail.total.toLocaleString("en-US")}</strong> letters waiting,
        <strong>{stats.economy.mail.to_family}</strong> addressed to the family.
      </div>
    </div>

    <!-- 3: Family's Journey -->
    <div class="card">
      <div class="card-title"><strong>The family's journey</strong></div>
      {#if stats.journey.length === 0}
        <p class="muted">No family characters yet — make one and it shows up here.</p>
      {:else}
        <div class="tablewrap">
          <table class="journey">
            <thead>
              <tr>
                <th>Name</th><th>Level</th><th>Class</th><th>Playtime</th><th>Last seen</th>
                <th>Achievements</th><th>Quests done</th><th>Honorable kills</th>
              </tr>
            </thead>
            <tbody>
              {#each stats.journey as j (j.name)}
                <tr>
                  <td class="jname">{j.name}</td>
                  <td>{j.level}</td>
                  <td>{className(j.class)}</td>
                  <td>{formatPlaytime(j.playtime)}</td>
                  <td>{formatLastSeen(j.last_seen)}</td>
                  <td>{j.achievements.toLocaleString("en-US")}</td>
                  <td>{j.quests.toLocaleString("en-US")}</td>
                  <td>{j.kills.toLocaleString("en-US")}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </div>

    <!-- 4: Server History -->
    <div class="card">
      <div class="card-title">
        <strong>Server history{stats.history.realm ? ` — ${stats.history.realm}` : ""}</strong>
      </div>
      <div class="tiles">
        <div class="tile">
          <span class="big">{stats.history.boots.toLocaleString("en-US")}</span>
          <span class="tlabel">times the world has started</span>
        </div>
        <div class="tile">
          <span class="big">{formatPlaytime(stats.history.total_uptime)}</span>
          <span class="tlabel">lifetime running time</span>
        </div>
        <div class="tile">
          <span class="big">{formatPlaytime(stats.history.longest)}</span>
          <span class="tlabel">longest session</span>
        </div>
        <div class="tile">
          <span class="big">{stats.history.peak.toLocaleString("en-US")}</span>
          <span class="tlabel">most connections at once</span>
          <span class="tsub">all-time peak — includes bot connections</span>
        </div>
      </div>

      {#if stats.history.recent.length > 0}
        <h4>Recent boots</h4>
        <div class="colchart boots">
          {#each stats.history.recent as b (b.start)}
            <div class="col" title="{formatBootDate(b.start)} — up {formatPlaytime(b.uptime)}">
              <div class="colbar">
                <div class="colfill up" style="height: {pct(b.uptime, bootMax)}%"></div>
              </div>
              <span class="collabel tiny">{formatBootDate(b.start)}</span>
            </div>
          {/each}
        </div>
      {/if}
    </div>

    <!-- 5: Bot Watch -->
    <div class="card">
      <div class="card-title"><strong>Bot watch</strong></div>
      {#if stats.botwatch.zones.length === 0}
        <p class="muted">No bots online right now.</p>
      {:else}
        <h4>Where the online bots are</h4>
        <div class="hbars">
          {#each stats.botwatch.zones as z (z.zone)}
            <div class="hbar-row">
              <span class="hbar-name wide">{zoneName(z.zone, zoneNames)}</span>
              <div class="hbar-track"><div class="hbar-fill" style="width: {pct(z.count, zoneMax)}%"></div></div>
              <span class="hbar-val">{z.count}</span>
            </div>
          {/each}
        </div>

        <h4>By continent</h4>
        <div class="chips">
          {#each stats.botwatch.continents as c (c.map)}
            <span class="chip"><strong>{continentName(c.map)}</strong> <span class="muted">{c.count}</span></span>
          {/each}
        </div>
      {/if}
      <div class="factline">
        Combined bot playtime: <strong>{formatYears(stats.botwatch.playtime)}</strong> — they never sleep.
      </div>
    </div>
  {/if}
</section>

<style>
  .content { padding: 20px 24px; overflow-y: auto; display: flex; flex-direction: column; gap: 16px; }
  .bar { display: flex; justify-content: space-between; align-items: center; }
  .bar h2 { margin: 0; font-size: 18px; }
  button { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 14px; cursor: pointer; }
  button:disabled { opacity: 0.5; cursor: default; }
  .muted { color: #8b949e; margin: 0; }
  .stale-note { font-size: 12.5px; }
  .error-card { background: #161b22; border: 1px solid #f85149; border-radius: 8px; padding: 12px 16px; }
  .card { background: #0d1117; border: 1px solid #30363d; border-radius: 8px; padding: 14px 16px; display: flex; flex-direction: column; gap: 12px; }
  .card-title { display: flex; align-items: center; gap: 8px; font-weight: 600; }
  h4 { margin: 4px 0 0; font-size: 12px; letter-spacing: 0.06em; text-transform: uppercase; color: #8b949e; }

  /* Stat tiles */
  .tiles { display: flex; gap: 12px; flex-wrap: wrap; }
  .tile { background: #161b22; border: 1px solid #30363d; border-radius: 8px; padding: 10px 14px; display: flex; flex-direction: column; gap: 2px; min-width: 130px; }
  .big { font-size: 22px; font-weight: 700; color: #f0f6fc; }
  .tlabel { font-size: 12.5px; color: #8b949e; }
  .tsub { font-size: 11.5px; color: #6e7681; }

  /* Column chart (level spread + per-boot history) -- proportional divs,
     no chart library (CSP forbids external assets). */
  .colchart { display: flex; gap: 8px; align-items: flex-end; }
  .colchart.boots { gap: 5px; }
  .col { display: flex; flex-direction: column; align-items: center; gap: 3px; flex: 1; max-width: 64px; }
  .colcount { font-size: 11px; color: #8b949e; }
  .colbar { display: flex; flex-direction: column; justify-content: flex-end; height: 90px; width: 100%; background: #161b22; border-radius: 4px 4px 0 0; overflow: hidden; }
  .colfill.bots { background: #1f6feb; opacity: 0.55; }
  .colfill.fam { background: #3fb950; }
  .colfill.up { background: #58a6ff; opacity: 0.8; }
  .collabel { font-size: 10.5px; color: #6e7681; }
  .collabel.tiny { font-size: 9px; }
  .legend { display: flex; align-items: center; gap: 8px; font-size: 11.5px; color: #8b949e; margin: 0; }
  .swatch { width: 10px; height: 10px; border-radius: 2px; display: inline-block; }
  .swatch.fam { background: #3fb950; }
  .swatch.bots { background: #1f6feb; opacity: 0.55; margin-left: 8px; }

  /* Horizontal bars (classes, bot zones) */
  .hbars { display: flex; flex-direction: column; gap: 5px; }
  .hbar-row { display: flex; align-items: center; gap: 10px; font-size: 13px; }
  .hbar-name { min-width: 105px; color: #c9d1d9; }
  .hbar-name.wide { min-width: 150px; }
  .hbar-track { flex: 1; height: 12px; background: #161b22; border-radius: 6px; overflow: hidden; }
  .hbar-fill { height: 100%; background: #58a6ff; opacity: 0.75; border-radius: 6px; }
  .hbar-val { min-width: 40px; text-align: right; color: #8b949e; font-size: 12.5px; }

  /* Faction split bar */
  .faction-bar { display: flex; height: 14px; border-radius: 7px; overflow: hidden; background: #161b22; }
  .fside.alliance { background: #1f6feb; }
  .fside.horde { background: #da3633; }
  .faction-labels { display: flex; justify-content: space-between; font-size: 12.5px; }
  .alliance-text { color: #58a6ff; }
  .horde-text { color: #f85149; }

  /* Name chips (top levels, richest, continents) */
  .chips { display: flex; gap: 8px; flex-wrap: wrap; }
  .chip { background: #161b22; border: 1px solid #30363d; border-radius: 12px; padding: 3px 12px; font-size: 13px; display: inline-flex; gap: 8px; align-items: baseline; }
  .fam-badge { background: #12261a; border: 1px solid #2ea043; color: #3fb950; border-radius: 8px; padding: 0 7px; font-size: 10.5px; }

  .factline { font-size: 13.5px; color: #8b949e; }
  .factline strong { color: #c9d1d9; }

  /* Family journey table */
  .tablewrap { overflow-x: auto; }
  table.journey { border-collapse: collapse; width: 100%; font-size: 13px; }
  .journey th { text-align: left; color: #8b949e; font-weight: 500; font-size: 11.5px; text-transform: uppercase; letter-spacing: 0.05em; padding: 4px 10px 6px 0; border-bottom: 1px solid #30363d; }
  .journey td { padding: 6px 10px 6px 0; border-bottom: 1px solid #21262d; color: #c9d1d9; white-space: nowrap; }
  .journey tr:last-child td { border-bottom: none; }
  .jname { font-weight: 600; color: #f0f6fc; }
</style>
