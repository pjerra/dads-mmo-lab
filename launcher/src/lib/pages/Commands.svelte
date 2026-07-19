<script lang="ts">
  import { onMount } from "svelte";
  import { wowCommands, type ModCommands } from "$lib/api";

  let mods: ModCommands[] = $state([]);
  let error: string | null = $state(null);
  let busy = $state(false);

  async function refresh() {
    busy = true;
    error = null;
    try {
      mods = await wowCommands();
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    } finally {
      busy = false;
    }
  }
  onMount(refresh);

  // Batch 3 F11b: core AzerothCore GM commands -- always available, no
  // module required. Static frontend data (they never change), rendered
  // ABOVE the per-module blocks.
  const CORE_COMMANDS: { cmd: string; what: string }[] = [
    { cmd: ".tele <place>", what: "Teleport yourself to a named place (e.g. .tele stormwind)." },
    { cmd: ".levelup <n>", what: "Raise your (or your target's) level by n." },
    { cmd: ".additem <id>", what: "Put an item straight into your bags (find ids on the Item Database page)." },
    { cmd: ".modify money <copper>", what: "Give money — 10000 copper = 1 gold." },
    { cmd: ".modify speed <1-10>", what: "Run faster (1 = normal; wears off on relog)." },
    { cmd: ".revive", what: "Bring yourself (or your target) back to life." },
    { cmd: ".summon <name>", what: "Pull a player to you." },
    { cmd: ".appear <name>", what: "Jump to a player." },
    { cmd: ".gm on / .gm off", what: "Turn GM mode on/off (invulnerable + invisible to mobs while on)." },
    { cmd: ".server info", what: "Show server version, uptime and who's online." },
    { cmd: ".saveall", what: "Save every online character right now." },
  ];
</script>

<section class="content">
  <header class="bar">
    <h2>In-Game Commands</h2>
    <button onclick={refresh} disabled={busy}>Refresh</button>
  </header>

  <div class="card">
    <h3>Core commands (always available)</h3>
    <p class="muted corenote">Type these in the in-game chat box on a GM account (or send them from the Console page without the leading dot).</p>
    <table class="coretable">
      <tbody>
        {#each CORE_COMMANDS as c (c.cmd)}
          <tr><td class="corecmd">{c.cmd}</td><td class="corewhat">{c.what}</td></tr>
        {/each}
      </tbody>
    </table>
  </div>

  {#if error}
    <div class="error-card"><strong>Couldn't load commands.</strong><p>{error}</p></div>
  {:else if mods.length === 0}
    <p class="muted">No installed mods with commands yet — install mods on the Modules page.</p>
  {:else}
    {#each mods as m (m.key)}
      <div class="card">
        <h3>{m.name}</h3>
        <pre class="cmds">{m.text}</pre>
      </div>
    {/each}
  {/if}
</section>

<style>
  .content { padding: 20px 24px; overflow-y: auto; display: flex; flex-direction: column; gap: 14px; box-sizing: border-box; }
  .bar { display: flex; justify-content: space-between; align-items: center; }
  .bar h2 { margin: 0; font-size: 18px; }
  .card { background: #0d1117; border: 1px solid #30363d; border-radius: 8px; padding: 12px 16px; }
  .card h3 { margin: 0 0 8px; font-size: 15px; }
  .cmds { margin: 0; color: #c9d1d9; font-family: Consolas, monospace; font-size: 12.5px; line-height: 1.45; white-space: pre-wrap; word-break: break-word; }
  .corenote { font-size: 12.5px; margin: 0 0 8px; }
  .coretable { border-collapse: collapse; }
  .coretable td { padding: 3px 14px 3px 0; font-size: 13px; vertical-align: top; }
  .corecmd { font-family: Consolas, monospace; color: #58a6ff; white-space: nowrap; }
  .corewhat { color: #c9d1d9; }
  button { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 14px; cursor: pointer; }
  button:disabled { opacity: 0.5; cursor: default; }
  .muted { color: #8b949e; margin: 0; }
  .error-card { background: #161b22; border: 1px solid #f85149; border-radius: 8px; padding: 12px 16px; }
</style>
