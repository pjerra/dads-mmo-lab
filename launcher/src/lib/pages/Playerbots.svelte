<script lang="ts">
  import { onMount } from "svelte";
  import {
    wowPartyOnline, wowPartyAdd, wowPartyList, wowPartyKick, wowPartyDismissAll, wowPartyRelogin, wowPartySetup,
    wowPartyBotcmd, wowPartyPresetSave, wowPartyPresetList, wowPartyPresetDelete, wowPartyPresetLoad,
    wowPartyPresetShow, wowPartyPresetImport, wowGmLevel, wowServerDetail, wowPartySpecs,
    type OnlineChar, type PartyMember, type PresetInfo,
  } from "$lib/api";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { className } from "$lib/wow";
  import { applyEvent } from "$lib/terminal-state";
  import Terminal from "$lib/Terminal.svelte";
  import { termBuf, beginRun, clearBuf } from "$lib/term-store.svelte";
  import { restartState } from "$lib/restart-state.svelte";
  import { featureLocked, LOCKED_HINT } from "$lib/features.svelte";
  import {
    ROLES,
    ROLE_MAP,
    PVE_SPECS_BY_CLASS_ID,
    VALID_BOT_CLASSES,
    buildSpecIndex,
    type Role,
    type SpecIndex,
  } from "$lib/party-specs";

  // Shown whenever an Add / Re-summon does not confirm a join in the short
  // poll window -- the bot is usually just still spawning in-game (Batch 5 F5).
  const SPAWNING_NOTE = "The bot may still be spawning — check in-game, then Refresh.";

  let online: OnlineChar[] = $state([]);
  let player = $state("");           // the chosen online player's name
  let members: PartyMember[] = $state([]);
  let error: string | null = $state(null);
  let busy = $state(false);
  let note: string | null = $state(null);
  let botsOnline: number | null = $state(null);

  // Batch 5 F5: live spec picker index, built from the deployed playerbots.conf
  // (`wow party specs`). Empty when the server isn't installed -> the static
  // ROLE_MAP / PVE_SPECS_BY_CLASS_ID fallbacks below take over.
  let specIndex: SpecIndex = $state({ byName: {}, byId: {} });

  const buf = termBuf("playerbots");
  let setting = $state(false);
  let confirmSetup = $state(false);

  let presets: PresetInfo[] = $state([]);
  let presetName = $state("");
  let loadingPreset = $state(false);
  let confirmingPreset: { kind: "load" | "delete"; name: string } | null = $state(null);

  let botLevel: Record<string, string> = $state({});

  let exportName: string | null = $state(null);
  let exportText = $state("");

  let importName = $state("");
  let importClasses = $state("");
  let importOverwrite = $state(false);

  function showErr(e: unknown) {
    const err = e as { message?: string; hint?: string };
    error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
  }

  async function refresh() {
    error = null;
    confirmSetup = false;
    confirmingPreset = null;
    confirmDismissAll = false;
    note = null;
    try {
      online = await wowPartyOnline();
      if (!online.find((o) => o.name === player)) player = online[0]?.name ?? "";
      if (player) members = await wowPartyList(player); else members = [];
    } catch (e) { showErr(e); }
    try {
      const d = await wowServerDetail();
      botsOnline = d.bots.online;
    } catch {
      // Decorative — do not fail refresh() over this. Null (not the stale
      // previous count) so the chip disappears instead of lying.
      botsOnline = null;
    }
  }
  async function refreshPresets() {
    try { presets = await wowPartyPresetList(); } catch (e) { showErr(e); }
  }
  // Live spec list (read-only). Never fails the page: on error (server not
  // installed / conf missing) the index stays empty and the static maps drive
  // the picker instead.
  async function refreshSpecs() {
    try { specIndex = buildSpecIndex(await wowPartySpecs()); } catch { /* static fallback */ }
  }
  onMount(() => { refresh(); refreshPresets(); refreshSpecs(); });

  // Batch 5 F5: role -> class -> spec picker state. "Any role" shows all 9
  // classes; "Any spec" ("" here) keeps today's no---spec behavior exactly.
  let pickRole = $state<Role | "">("");
  let pickClass = $state("");
  let pickSpec = $state("");
  const roleClasses = $derived(
    pickRole === ""
      ? VALID_BOT_CLASSES.map((c) => ({ class: c, spec: "" }))
      : ROLE_MAP[pickRole].map((p) => ({ class: p.class, spec: p.spec })),
  );
  function onRoleChange() {
    pickClass = "";
    pickSpec = "";
  }
  function onClassChange() {
    // Default the spec to the role's pick for that class; Any-role -> any.
    pickSpec = roleClasses.find((c) => c.class === pickClass)?.spec ?? "";
  }
  // Batch 5 F5: the ACTUAL spec options for the picked class come from the live
  // conf; empty means "server not installed" -> the single role spec is offered
  // as a fallback (see the template). pickSpecMeta drives the build preview.
  const liveSpecsForPick = $derived(pickClass ? (specIndex.byName[pickClass] ?? []) : []);
  const pickSpecMeta = $derived(
    pickSpec ? (liveSpecsForPick.find((s) => s.name === pickSpec) ?? null) : null,
  );
  // Per-bot Change-spec options by characters.class id: live names when known,
  // else the static pve fallback list.
  function specOptionsForClassId(cid: number): string[] {
    const live = specIndex.byId[cid];
    if (live && live.length) return live.map((s) => s.name);
    return PVE_SPECS_BY_CLASS_ID[cid] ?? [];
  }
  // Open the Wowhead talent-calc preview for a live spec (its class name is a
  // wowhead class slug; the link is the talent fragment). Opener plugin +
  // capability are already granted (used by Items/Help/Modules).
  function openSpecPreview(cls: string, link: string) {
    openUrl(`https://www.wowhead.com/wotlk/talent-calc/${cls}/${link}`).catch(() => {});
  }
  const addLocked = $derived(
    featureLocked("party-ops") || (pickSpec !== "" && featureLocked("party-spec")),
  );

  // Per-bot Change-spec picks (Batch 5 F5), keyed by bot name.
  let botSpec: Record<string, string> = $state({});

  // add/kick/resummon snapshot `player` into a local before their first await.
  // The player <select> is also disabled while busy/setting (below), but the
  // snapshot means these handlers stay correct even if that guard is ever
  // loosened -- a live re-read of `player` after an await could otherwise
  // send a follow-up call (or a "note" message) to the wrong character if the
  // selection changed mid-flight.
  async function add(cls: string, spec?: string) {
    const p = player;
    if (!p) return;
    busy = true; error = null; note = null;
    try {
      const r = await wowPartyAdd(p, cls, undefined, spec);
      // The CLI already polled a short window for the join. Confirmed -> a
      // success note; not confirmed -> the spawning guidance (not "Adding…").
      if (r.joined) {
        if (spec) {
          note = r.spec_applied
            ? `Added a ${spec} ${cls} to your party (talents + gear applied).`
            : (r.note ?? `Added a ${cls} — spec not applied.`);
        } else {
          note = `Added a ${cls} to your party.`;
        }
      } else {
        note = SPAWNING_NOTE;
      }
      members = await wowPartyList(p);
    } catch (e) { showErr(e); } finally { busy = false; }
  }

  // Poll the party list a few times for a bot to come (back) online. Updates
  // `members` on each pass and resolves true as soon as the bot shows online.
  async function waitForBotOnline(p: string, bot: string): Promise<boolean> {
    const TRIES = 4, DELAY_MS = 600;
    for (let i = 0; i < TRIES; i++) {
      members = await wowPartyList(p);
      if (members.some((m) => m.name === bot && m.online)) return true;
      if (i < TRIES - 1) await new Promise((r) => setTimeout(r, DELAY_MS));
    }
    return false;
  }

  async function changeSpec(bot: string) {
    const p = player;
    const s = botSpec[bot];
    if (!s) return;
    busy = true; error = null; note = null;
    try {
      await wowPartyBotcmd(p, bot, "spec", s);
      note = `Told ${bot} to respec to ${s} — give it a moment, then Gear up.`;
    } catch (e) { showErr(e); } finally { busy = false; }
  }
  async function kick(bot: string) {
    const p = player;
    busy = true; error = null;
    try { await wowPartyKick(p, bot); members = await wowPartyList(p); }
    catch (e) { showErr(e); } finally { busy = false; }
  }
  // Dismiss ALL party bots (uninvite + logout each) -- two-step confirm like
  // the preset load/delete buttons above.
  let confirmDismissAll = $state(false);
  async function dismissAll() {
    if (!confirmDismissAll) { confirmDismissAll = true; return; }
    confirmDismissAll = false;
    const p = player;
    busy = true; error = null; note = null;
    try {
      const r = await wowPartyDismissAll(p);
      note = r.dismissed === 0
        ? "No bots to dismiss."
        : `Dismissed ${r.dismissed} bot${r.dismissed === 1 ? "" : "s"}.`;
      members = await wowPartyList(p);
    } catch (e) { showErr(e); } finally { busy = false; }
  }
  async function resummon(bot: string) {
    const p = player;
    busy = true; error = null; note = null;
    try {
      await wowPartyRelogin(p, bot);
      // Relogin fires and returns immediately; confirm the bot actually comes
      // back online in a short window, else surface the spawning guidance.
      const online = await waitForBotOnline(p, bot);
      note = online ? `Re-summoned ${bot}.` : SPAWNING_NOTE;
    }
    catch (e) { showErr(e); } finally { busy = false; }
  }
  const BOTCMD_PHRASE = { gear: "gear up", talents: "fix its talents", maintain: "do maintenance" } as const;
  async function botcmd(bot: string, action: "gear" | "talents" | "maintain") {
    const p = player;
    busy = true; error = null; note = null;
    try {
      await wowPartyBotcmd(p, bot, action);
      note = `Told ${bot} to ${BOTCMD_PHRASE[action]} — give it a moment.`;
    } catch (e) { showErr(e); } finally { busy = false; }
  }
  function levelValid(v: string | undefined): boolean {
    const t = (v ?? "").trim();
    if (!t) return false;
    const n = Number(t);
    return Number.isInteger(n) && n >= 1 && n <= 255;
  }
  async function setBotLevel(bot: string) {
    const p = player;
    const n = Number(botLevel[bot]);
    busy = true; error = null; note = null;
    try {
      await wowGmLevel(bot, n);
      note = `${bot} is now level ${n}.`;
      members = await wowPartyList(p);
    } catch (e) { showErr(e); } finally { busy = false; }
  }

  async function savePreset() {
    const p = player; const n = presetName.trim();
    if (!n) return;
    busy = true; error = null; note = null;
    try {
      const r = await wowPartyPresetSave(p, n);
      note = `Saved preset "${r.name}" (${r.bots.length} bots${r.overwrote ? ", replaced the old one" : ""}).`;
      await refreshPresets();
    } catch (e) { showErr(e); } finally { busy = false; }
  }

  async function loadPreset(name: string) {
    if (confirmingPreset?.kind !== "load" || confirmingPreset?.name !== name) {
      confirmingPreset = { kind: "load", name };
      return;
    }
    confirmingPreset = null;
    const p = player;
    loadingPreset = true; error = null; note = null; beginRun("playerbots");
    let requested = 0, joined = 0;
    let sawDone = false;
    let streamErr: { message?: string; hint?: string } | null = null;
    let outcomeErr: unknown = null;
    try {
      await wowPartyPresetLoad(p, name, (e) => {
        buf.term = applyEvent(buf.term, e);
        if (e.event === "done") {
          sawDone = true;
          const d = e.data as { requested?: number; joined?: number } | undefined;
          requested = d?.requested ?? 0; joined = d?.joined ?? 0;
        } else if (e.event === "error") {
          streamErr = (e as { error?: { message?: string; hint?: string } }).error ?? {};
        }
      });
    } catch (e) { outcomeErr = e; }
    finally {
      loadingPreset = false;
      await refresh();
      await refreshPresets();
      // Apply the outcome AFTER refresh() so its note/error reset can't clobber
      // it. The stream promise resolves even when the CLI fails (an NDJSON
      // `error` event is terminal) -- only a seen `done` event means success.
      if (outcomeErr) showErr(outcomeErr);
      else if (streamErr) showErr(streamErr);
      else if (sawDone) note = `Loaded "${name}" — ${joined} of ${requested} bots joined.`;
    }
  }

  async function deletePreset(name: string) {
    if (confirmingPreset?.kind !== "delete" || confirmingPreset?.name !== name) {
      confirmingPreset = { kind: "delete", name };
      return;
    }
    confirmingPreset = null;
    busy = true; error = null;
    try { await wowPartyPresetDelete(name); await refreshPresets(); }
    catch (e) { showErr(e); } finally { busy = false; }
  }

  async function toggleExport(name: string) {
    if (exportName === name) { exportName = null; return; }
    busy = true; error = null; note = null;
    try {
      const r = await wowPartyPresetShow(name);
      exportName = name;
      exportText = r.classes.join(",");
    } catch (e) { showErr(e); } finally { busy = false; }
  }

  // On a fresh EXISTS, arm a second confirm click instead of surfacing the raw
  // error -- mirrors the two-step confirm pattern used for load/delete above.
  // Any edit to name/classes disarms it (inputs' oninput), so a stale confirm
  // can't silently overwrite a preset the user has since renamed.
  async function importPreset() {
    const name = importName.trim();
    const classes = importClasses.trim();
    if (!name || !classes) return;
    busy = true; error = null; note = null;
    try {
      const r = await wowPartyPresetImport(name, classes, importOverwrite);
      note = `Imported preset "${r.name}" (${r.classes.length} bots).`;
      importOverwrite = false;
      importName = "";
      importClasses = "";
      await refreshPresets();
    } catch (e) {
      const err = e as { code?: string; message?: string; hint?: string };
      if (err.code === "EXISTS" && !importOverwrite) {
        importOverwrite = true;
      } else {
        importOverwrite = false;
        showErr(e);
      }
    } finally { busy = false; }
  }
  async function enableMyParty() {
    if (!confirmSetup) { confirmSetup = true; return; }
    confirmSetup = false; setting = true; beginRun("playerbots");
    try {
      await wowPartySetup((e) => {
        buf.term = applyEvent(buf.term, e);
        if (e.event === "done") {
          const d = e.data as { restart_required?: boolean } | undefined;
          if (d?.restart_required) restartState.needed = true;
        }
      });
    } catch (e) {
      const err = e as { code?: string; message?: string; hint?: string };
      buf.term = applyEvent(buf.term, { event: "error", error: { code: err.code ?? "IPC", message: err.message ?? String(e), hint: err.hint ?? "" } });
    } finally { setting = false; }
  }
</script>

<section class="content">
  <header class="bar"><h2>My Party</h2>{#if botsOnline !== null}<span class="chip">Bots online: {botsOnline}</span>{/if}<button onclick={refresh} disabled={busy || setting || loadingPreset}>Refresh</button></header>

  {#if error}<div class="error-card"><p>{error}</p></div>{/if}

  {#if online.length === 0}
    <div class="card">
      <p class="muted">No character is logged into the game. Log one in, then Refresh.</p>
      <p class="muted">First time? <button
        onclick={enableMyParty}
        disabled={setting || featureLocked("party-ops")}
        title={featureLocked("party-ops") ? LOCKED_HINT : undefined}
      >
        {confirmSetup ? "Deploy the bot bridge scripts?" : "Enable My Party"}</button>
        <span class="muted">— one-time setup; afterward stop and start the server (Home or Library) to load the scripts.</span></p>
    </div>
  {:else}
    <div class="card">
      <strong>Building a party for
        {#if online.length > 1}
          <select bind:value={player} onchange={() => refresh()} disabled={busy || setting || loadingPreset}>
            {#each online as o (o.guid)}<option value={o.name}>{o.name}</option>{/each}
          </select>
        {:else}{player}{/if}
      </strong>
    </div>

    <!-- Batch 5 F5: role -> class -> spec picker (replaces the old class
         button row). "Any spec" = today's behavior (no spec whisper). -->
    <div class="addrow">
      <select bind:value={pickRole} onchange={onRoleChange} disabled={busy || setting || loadingPreset}>
        <option value="">Any role</option>
        {#each ROLES as r (r)}<option value={r}>{r}</option>{/each}
      </select>
      <select bind:value={pickClass} onchange={onClassChange} disabled={busy || setting || loadingPreset}>
        <option value="">Pick a class…</option>
        {#each roleClasses as c (c.class)}
          <option value={c.class}>{c.class[0].toUpperCase() + c.class.slice(1)}</option>
        {/each}
      </select>
      <select bind:value={pickSpec} disabled={busy || setting || loadingPreset || !pickClass}>
        <option value="">Any spec</option>
        {#if pickClass}
          {#if liveSpecsForPick.length > 0}
            <!-- Live options from the deployed playerbots.conf (party specs). -->
            {#each liveSpecsForPick as s (s.name)}<option value={s.name}>{s.name}</option>{/each}
          {:else}
            <!-- Fallback (server not installed): the single role spec only. -->
            {@const roleSpec = roleClasses.find((c) => c.class === pickClass)?.spec}
            {#if roleSpec}<option value={roleSpec}>{roleSpec}</option>{/if}
          {/if}
        {/if}
      </select>
      <button
        class="cls"
        onclick={() => add(pickClass, pickSpec || undefined)}
        disabled={!pickClass || busy || setting || loadingPreset || addLocked}
        title={addLocked ? LOCKED_HINT : undefined}
      >
        Add bot
      </button>
    </div>
    {#if pickSpecMeta}
      <p class="muted">
        {pickSpec} build{#if pickSpecMeta.tree}: talents {pickSpecMeta.tree}{/if}
        {#if pickSpecMeta.link}
          · <button class="link" onclick={() => openSpecPreview(pickSpecMeta.class, pickSpecMeta.link!)}>preview on Wowhead</button>
        {/if}
      </p>
    {/if}
    {#if note}<p class="muted">{note}</p>{/if}

    <header class="bar"><h3>Current party</h3>
      {#if members.filter((m) => m.is_bot).length > 0}
        <button
          onclick={dismissAll}
          disabled={busy || loadingPreset || featureLocked("party-ops")}
          title={featureLocked("party-ops") ? LOCKED_HINT : "Kicks every bot from the party and sends it away"}
        >
          {confirmDismissAll ? "Send every bot away — sure?" : "Dismiss all bots"}
        </button>
      {/if}
    </header>
    {#if members.length <= 1}
      <p class="muted">Just you so far — click a class above to add a bot.</p>
    {:else}
      <table>
        <tbody>
          {#each members as m (m.guid)}
            <tr>
              <td>{#if m.is_bot}<span class="dot" class:on={m.online} title={m.online ? "online" : "offline"}></span>{/if}{m.name}</td><td class="muted">{className(m.class)} · lvl {m.level}</td>
              <td>{#if m.is_bot}<button
                    onclick={() => kick(m.name)}
                    disabled={busy || loadingPreset || featureLocked("party-ops")}
                    title={featureLocked("party-ops") ? LOCKED_HINT : undefined}
                  >
                    Kick
                  </button>
                  <button
                    onclick={() => resummon(m.name)}
                    disabled={busy || loadingPreset || featureLocked("party-ops")}
                    title={featureLocked("party-ops") ? LOCKED_HINT : undefined}
                  >
                    Re-summon
                  </button>
                  <button
                    onclick={() => botcmd(m.name, "gear")}
                    disabled={busy || loadingPreset || featureLocked("party-botcmd")}
                    title={featureLocked("party-botcmd") ? LOCKED_HINT : undefined}
                  >
                    Gear up
                  </button>
                  <button
                    onclick={() => botcmd(m.name, "talents")}
                    disabled={busy || loadingPreset || featureLocked("party-botcmd")}
                    title={featureLocked("party-botcmd") ? LOCKED_HINT : undefined}
                  >
                    Fix talents
                  </button>
                  <button
                    onclick={() => botcmd(m.name, "maintain")}
                    disabled={busy || loadingPreset || featureLocked("party-botcmd")}
                    title={featureLocked("party-botcmd") ? LOCKED_HINT : undefined}
                  >
                    Maintain
                  </button>
                  <input type="number" min="1" max="255" class="lvl-input" placeholder="lvl"
                    value={botLevel[m.name] ?? ""}
                    oninput={(e) => (botLevel[m.name] = e.currentTarget.value)}
                    disabled={busy || loadingPreset} />
                  <button
                    onclick={() => setBotLevel(m.name)}
                    disabled={busy || loadingPreset || !levelValid(botLevel[m.name]) || featureLocked("bot-level")}
                    title={featureLocked("bot-level") ? LOCKED_HINT : undefined}
                  >
                    Set level
                  </button>
                  {@const specOpts = specOptionsForClassId(m.class)}
                  {#if specOpts.length > 0}
                    <select
                      value={botSpec[m.name] ?? ""}
                      onchange={(e) => (botSpec[m.name] = e.currentTarget.value)}
                      disabled={busy || loadingPreset}
                    >
                      <option value="">spec…</option>
                      {#each specOpts as s (s)}
                        <option value={s}>{s}</option>
                      {/each}
                    </select>
                    <button
                      onclick={() => changeSpec(m.name)}
                      disabled={busy || loadingPreset || !botSpec[m.name] || featureLocked("party-spec")}
                      title={featureLocked("party-spec") ? LOCKED_HINT : undefined}
                    >
                      Change spec
                    </button>
                  {/if}
                  {:else}<span class="muted">you</span>{/if}</td>
            </tr>
          {/each}
        </tbody>
      </table>
      <!-- Batch 5 F5: set-level guidance, consistent with GM Tools. -->
      <p class="muted">Set level: 1–255; your server's max level applies.</p>
    {/if}

    <header class="bar"><h3>Party presets</h3></header>
    <div class="card">
      <div class="prow">
        <input placeholder="preset name" maxlength="32" bind:value={presetName}
          disabled={busy || setting || loadingPreset} />
        <button onclick={savePreset}
          disabled={!presetName.trim() || busy || setting || loadingPreset || members.filter((m) => m.is_bot).length === 0 || featureLocked("party-presets")}
          title={featureLocked("party-presets") ? LOCKED_HINT : undefined}>
          Save current party
        </button>
      </div>
      <div class="prow">
        <input placeholder="import as…" maxlength="32" bind:value={importName}
          oninput={() => (importOverwrite = false)} disabled={busy || setting || loadingPreset} />
        <input placeholder="warrior,mage,priest,…" bind:value={importClasses}
          oninput={() => (importOverwrite = false)} disabled={busy || setting || loadingPreset} />
        <button onclick={importPreset}
          disabled={!importName.trim() || !importClasses.trim() || busy || setting || loadingPreset || featureLocked("preset-io")}
          title={featureLocked("preset-io") ? LOCKED_HINT : undefined}>
          {importOverwrite ? `Preset "${importName.trim()}" exists — overwrite?` : "Import"}
        </button>
      </div>
      {#if presets.length === 0}
        <p class="muted">No presets saved yet — build a party and save it.</p>
      {:else}
        {#each presets as pr (pr.name)}
          <div class="prow">
            <span>{pr.name} <span class="muted">({pr.bots} bots)</span></span>
            <button onclick={() => loadPreset(pr.name)}
              disabled={busy || setting || loadingPreset || featureLocked("party-presets")}
              title={featureLocked("party-presets") ? LOCKED_HINT : undefined}>
              {confirmingPreset?.kind === "load" && confirmingPreset?.name === pr.name ? "Replaces your current bots — sure?" : "Load"}
            </button>
            <button onclick={() => deletePreset(pr.name)}
              disabled={busy || setting || loadingPreset || featureLocked("party-presets")}
              title={featureLocked("party-presets") ? LOCKED_HINT : undefined}>
              {confirmingPreset?.kind === "delete" && confirmingPreset?.name === pr.name ? `Delete "${pr.name}" — sure?` : "Delete"}
            </button>
            <button onclick={() => toggleExport(pr.name)}
              disabled={busy || setting || loadingPreset || featureLocked("preset-io")}
              title={featureLocked("preset-io") ? LOCKED_HINT : undefined}>
              {exportName === pr.name ? "Hide" : "Export"}
            </button>
          </div>
          {#if exportName === pr.name}
            <div class="prow">
              <textarea class="export-box" rows="2" readonly value={exportText}></textarea>
            </div>
          {/if}
        {/each}
      {/if}
    </div>
  {/if}

  {#if buf.show}<Terminal state={buf.term} onclear={() => clearBuf("playerbots")} logName="dml-playerbots" />{/if}
</section>

<style>
  .content { padding: 20px 24px; overflow-y: auto; display: flex; flex-direction: column; gap: 14px; }
  .bar { display: flex; justify-content: space-between; align-items: center; }
  .bar h2, .bar h3 { margin: 0; }
  .card { background: #0d1117; border: 1px solid #30363d; border-radius: 8px; padding: 12px 16px; }
  .addrow { display: flex; flex-wrap: wrap; gap: 8px; }
  .prow { display: flex; gap: 10px; align-items: center; flex-wrap: wrap; padding: 4px 0; }
  .cls { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 8px 14px; cursor: pointer; }
  input, textarea { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 8px; }
  input.lvl-input { width: 52px; }
  textarea.export-box { width: 100%; box-sizing: border-box; font-family: Consolas, monospace; font-size: 13px; resize: vertical; }
  table { border-collapse: collapse; }
  td { padding: 4px 12px 4px 0; font-size: 14px; }
  button { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 5px 12px; cursor: pointer; }
  button:disabled { opacity: 0.5; cursor: default; }
  .muted { color: #8b949e; font-size: 13px; }
  .chip { font-size: .85em; color: #8b949e; border: 1px solid #30363d; border-radius: 999px; padding: 2px 10px; }
  /* Per-bot online dot: grey by default, green when the bot is online. */
  .dot { display: inline-block; width: 8px; height: 8px; border-radius: 50%; margin-right: 6px; background: #8b949e; vertical-align: middle; }
  .dot.on { background: #3fb950; }
  /* Inline text-button styling for the Wowhead build preview. */
  button.link { background: none; border: none; padding: 0; color: #58a6ff; text-decoration: underline; cursor: pointer; font: inherit; }
  .error-card { background: #161b22; border: 1px solid #f85149; border-radius: 8px; padding: 12px 16px; }
</style>
