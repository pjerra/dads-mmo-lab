<script lang="ts">
  import { onMount } from "svelte";
  import {
    wowPartyOnline, wowGmLevel, wowGmGold, wowGmHeal, wowGmRevive, wowGmSummon, wowGmAtLogin, wowGmReturnHome, wowBridgeSetup,
    type OnlineChar,
  } from "$lib/api";
  import { applyEvent } from "$lib/terminal-state";
  import Terminal from "$lib/Terminal.svelte";
  import { termBuf, beginRun, clearBuf } from "$lib/term-store.svelte";
  import { restartState, noteApplyNeeded } from "$lib/restart-state.svelte";
  import { bannerText, normalizeApplyNeeded } from "$lib/apply-needed";
  import CharPicker from "$lib/CharPicker.svelte";
  import { featureLocked, LOCKED_HINT } from "$lib/features.svelte";
  import { summonModuleHint } from "$lib/gm-summon";
  import { taskbarBusy, taskbarIdle } from "$lib/taskbar";

  let charName = $state("");
  let online: OnlineChar[] = $state([]);
  let error: string | null = $state(null);
  let note: string | null = $state(null);
  let busy = $state(false);

  let level = $state(80);
  let gold = $state(1000);
  let customEntry = $state(990000);
  let confirming: "level" | "gold" | "atlogin" | null = $state(null);

  const AT_LOGIN_FLAGS: { value: "rename" | "customize" | "changerace" | "changefaction"; label: string }[] = [
    { value: "rename", label: "Rename" },
    { value: "customize", label: "Customize appearance" },
    { value: "changerace", label: "Change race" },
    { value: "changefaction", label: "Change faction" },
  ];
  let atLoginFlag: "rename" | "customize" | "changerace" | "changefaction" = $state("rename");

  const buf = termBuf("gmtools");
  let deploying = $state(false);
  let confirmDeploy = $state(false);

  const isOnline = $derived(online.some((o) => o.name === charName));

  const NPCS = [
    { entry: 8661, label: "Auctioneer" },
    { entry: 5060, label: "Banker" },
    { entry: 6272, label: "Innkeeper" },
    { entry: 9896, label: "Stable Master" },
    { entry: 14337, label: "Repair Bot" },
    { entry: 990000, label: "Casino" },
  ];

  function showErr(e: unknown) {
    const err = e as { message?: string; hint?: string };
    error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
  }

  async function refresh() {
    error = null; note = null; confirming = null;
    try { online = await wowPartyOnline(); } catch (e) { showErr(e); }
  }
  onMount(refresh);

  // Every action snapshots charName before its first await so a mid-flight
  // picker change can't retarget the call or the success note.
  async function act(fn: () => Promise<unknown>, okNote: string) {
    busy = true; error = null; note = null;
    try { await fn(); note = okNote; }
    catch (e) { showErr(e); }
    finally { busy = false; }
  }

  function revive() { const p = charName; act(() => wowGmRevive(p), `Revived ${p}.`); }
  function heal() { const p = charName; act(() => wowGmHeal(p), `Healed ${p} to full.`); }
  // Works OFFLINE too (online -> console teleport, offline -> DB position
  // write), so unlike revive/heal it is not gated on the character being
  // online. Destination is the character's faction capital (smoke-findings
  // fix: the old hearth/`.unstuck` target could itself be the stuck spot).
  function returnHome() { const p = charName; act(() => wowGmReturnHome(p), `Sent ${p} to their faction capital.`); }
  function applyLevel() {
    if (confirming !== "level") { confirming = "level"; return; }
    confirming = null;
    const p = charName; const l = level;
    act(() => wowGmLevel(p, l), `${p} is now level ${l}.`);
  }
  function applyGold() {
    if (confirming !== "gold") { confirming = "gold"; return; }
    confirming = null;
    const p = charName; const g = gold;
    act(() => wowGmGold(p, g), `${p} now has ${g} gold.`);
  }
  function applyAtLogin() {
    if (confirming !== "atlogin") { confirming = "atlogin"; return; }
    confirming = null;
    const p = charName; const f = atLoginFlag;
    const label = AT_LOGIN_FLAGS.find((a) => a.value === f)?.label ?? f;
    act(() => wowGmAtLogin(p, f), `${label} will apply next time ${p} logs in.`);
  }

  async function summon(entry: number) {
    const p = charName;
    busy = true; error = null; note = null;
    try {
      const r = await wowGmSummon(p, entry);
      note = `Summoned ${r.npc} — despawns in 5 minutes.`;
    } catch (e) {
      // summonModuleHint only fires for a genuinely-missing module NPC; an
      // offline-character NOT_FOUND (same code, different message) falls
      // through to the plain error.
      const hint = summonModuleHint(entry, e as { code?: string; message?: string });
      if (hint) error = hint;
      else showErr(e);
    } finally { busy = false; }
  }

  async function deployBridges() {
    if (!confirmDeploy) { confirmDeploy = true; return; }
    confirmDeploy = false; deploying = true; error = null; note = null; beginRun("gmtools"); taskbarBusy();
    try {
      await wowBridgeSetup((e) => {
        buf.term = applyEvent(buf.term, e);
        if (e.event === "done") {
          const d = e.data as { restart_required?: boolean; apply_needed?: string } | undefined;
          if (d?.restart_required) noteApplyNeeded(normalizeApplyNeeded(d));
        }
      });
    } catch (e) {
      const err = e as { code?: string; message?: string; hint?: string };
      buf.term = applyEvent(buf.term, { event: "error", error: { code: err.code ?? "IPC", message: err.message ?? String(e), hint: err.hint ?? "" } });
    } finally { taskbarIdle(); deploying = false; }
  }
</script>

<section class="content">
  <header class="bar">
    <h2>GM Tools</h2>
    <button onclick={refresh} disabled={busy || deploying}>Refresh</button>
  </header>

  {#if error}<div class="error-card"><p>{error}</p></div>{/if}
  {#if restartState.needed}
    <div class="warn-card"><p>{bannerText(restartState.apply)}</p></div>
  {/if}

  <div class="card row">
    <strong>Character</strong>
    <CharPicker bind:selected={charName} disabled={busy || deploying}
      onpick={() => { confirming = null; note = null; error = null; }} />
    {#if charName}
      <span class="badge {isOnline ? 'on' : 'off'}">{isOnline ? "Online" : "Offline"}</span>
      {#if !isOnline}<span class="muted">Revive, heal and gold need the character logged in. Set level works offline.</span>{/if}
    {/if}
  </div>

  {#if note}<p class="muted">{note}</p>{/if}

  <div class="card row">
    <strong>Rescue</strong>
    <button
      onclick={revive}
      disabled={!charName || !isOnline || busy || featureLocked("gm-actions")}
      title={featureLocked("gm-actions") ? LOCKED_HINT : undefined}
    >
      Revive
    </button>
    <button
      onclick={heal}
      disabled={!charName || !isOnline || busy || featureLocked("gm-actions")}
      title={featureLocked("gm-actions") ? LOCKED_HINT : undefined}
    >
      Full heal
    </button>
    <button
      onclick={returnHome}
      disabled={!charName || busy || featureLocked("gm-return-home")}
      title={featureLocked("gm-return-home") ? LOCKED_HINT : "Teleports the character to their faction capital (Stormwind or Orgrimmar) — fixes a character stuck in the world; works even while they're offline"}
    >
      Send to capital (unstuck)
    </button>
    <span class="muted">Send to capital works offline too.</span>
  </div>

  <div class="card row">
    <strong>Set level</strong>
    <input type="number" min="1" max="255" bind:value={level}
      oninput={() => (confirming = null)} disabled={busy} />
    <button
      onclick={applyLevel}
      disabled={!charName || busy || !Number.isInteger(level) || level < 1 || level > 255 || featureLocked("gm-actions")}
      title={featureLocked("gm-actions") ? LOCKED_HINT : undefined}
    >
      {confirming === "level" ? "This can lower the level — sure?" : "Apply"}
    </button>
    <span class="muted">1–255; your server's max level applies. Works offline.</span>
  </div>

  <div class="card row">
    <strong>Set gold</strong>
    <input type="number" min="0" max="214748" bind:value={gold}
      oninput={() => (confirming = null)} disabled={busy} />
    <button
      onclick={applyGold}
      disabled={!charName || !isOnline || busy || !Number.isInteger(gold) || gold < 0 || gold > 214748 || featureLocked("gm-actions")}
      title={featureLocked("gm-actions") ? LOCKED_HINT : undefined}
    >
      {confirming === "gold" ? "This replaces their current money — sure?" : "Apply"}
    </button>
    <span class="muted">Sets the total (not adds). Max 214,748 gold.</span>
  </div>

  <div class="card row">
    <strong>At next login</strong>
    <select bind:value={atLoginFlag} onchange={() => (confirming = null)} disabled={busy}>
      {#each AT_LOGIN_FLAGS as f (f.value)}<option value={f.value}>{f.label}</option>{/each}
    </select>
    <button
      onclick={applyAtLogin}
      disabled={!charName || busy || featureLocked("gm-atlogin")}
      title={featureLocked("gm-atlogin") ? LOCKED_HINT : undefined}
    >
      {confirming === "atlogin" ? "Applies at that character's next login. Continue?" : "Apply"}
    </button>
    <span class="muted">Takes effect the next time this character logs in. Works offline.</span>
  </div>

  <div class="card">
    <div class="row">
      <strong>Summon an NPC</strong>
      {#each NPCS as n (n.entry)}
        <button
          onclick={() => summon(n.entry)}
          disabled={!charName || !isOnline || busy || featureLocked("gm-summon")}
          title={featureLocked("gm-summon") ? LOCKED_HINT : undefined}
        >
          {n.label}
        </button>
      {/each}
    </div>
    <div class="row" style="margin-top: 8px;">
      <span class="muted">Custom entry id:</span>
      <input type="number" min="1" max="999999" bind:value={customEntry} disabled={busy} />
      <button
        onclick={() => summon(customEntry)}
        disabled={!charName || !isOnline || busy || !Number.isInteger(customEntry) || customEntry < 1 || customEntry > 999999 || featureLocked("gm-summon")}
        title={featureLocked("gm-summon") ? LOCKED_HINT : undefined}
      >
        Summon
      </button>
    </div>
    <p class="muted" style="margin-top: 8px;">Temporary — the NPC despawns after 5 minutes. Needs the character online.</p>
  </div>

  <p class="muted">
    Bridge scripts missing or outdated?
    <button
      onclick={deployBridges}
      disabled={deploying || featureLocked("bridge-setup")}
      title={featureLocked("bridge-setup") ? LOCKED_HINT : undefined}
    >
      {confirmDeploy ? "Deploy the server bridge scripts?" : "Deploy server bridges"}
    </button>
    — then stop and start the server (Home or Library) to load them.
  </p>

  {#if buf.show}
    <Terminal state={buf.term} onclear={() => clearBuf("gmtools")} logName="dml-gmtools" />
  {/if}
</section>

<style>
  .content { padding: 20px 24px; overflow-y: auto; display: flex; flex-direction: column; gap: 12px; }
  .bar { display: flex; justify-content: space-between; align-items: center; }
  .bar h2 { margin: 0; font-size: 18px; }
  .card { background: #0d1117; border: 1px solid #30363d; border-radius: 8px; padding: 12px 16px; }
  .row { display: flex; gap: 12px; align-items: center; flex-wrap: wrap; }
  .badge { font-size: 12px; padding: 2px 10px; border-radius: 10px; border: 1px solid #30363d; }
  .badge.on { color: #3fb950; border-color: #3fb950; }
  .badge.off { color: #8b949e; }
  input { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 8px; width: 110px; }
  select { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 8px; }
  button { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 14px; cursor: pointer; }
  button:disabled { opacity: 0.5; cursor: default; }
  .muted { color: #8b949e; font-size: 13px; margin: 0; }
  .error-card { background: #161b22; border: 1px solid #f85149; border-radius: 8px; padding: 12px 16px; }
  .warn-card { background: #161b22; border: 1px solid #d29922; border-radius: 8px; padding: 12px 16px; }
</style>
