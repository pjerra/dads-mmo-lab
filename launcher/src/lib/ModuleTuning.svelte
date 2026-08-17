<script lang="ts">
  // The Config page's "Module tuning" view, extracted verbatim into the
  // tabbed Modules page (module-update round). Two sections of collapsible
  // per-module cards (collapsed by default, the sidebar-accordion caret
  // style):
  //   Server modules: every INSTALLED C++ module whose conf passes the CLI's
  //     editable-conf allowlist. Curated rows (the old guided knobs) render
  //     first inside the owning module's card, then an "All settings" browser
  //     (the Bot World pb-keys pattern generalized via `config conf-keys`).
  //   Lua scripts: ONLY the curated lua knobs (Unlimited Ammo, Sit Means
  //     Rest) -- deliberately NO generic browser for lua (editing arbitrary
  //     script lines is a footgun), and the section hides entirely when no
  //     curated-lua module is deployed.
  // All writes stay locked behind the single [guided-config] flag; browsing
  // and searching are never locked. Conf changes need a restart unless the
  // CLI knows the module's live-reload console command (mod-transmog); lua
  // changes apply with `.reload ale`.
  // The component stays MOUNTED while the other tabs are shown (the page
  // switches with display:none), so staged edits survive tab switches --
  // `active` gates the lazy loads the old in-Config `tab` checks gated.
  import {
    wowConfigSet,
    wowConsoleSend,
    wowConfigTuningSet,
    wowConfigConfKeys,
    wowModuleUpdate,
    type ConfFile,
    type ConfKey,
    type CppModule,
    type ModuleTuning,
  } from "$lib/api";
  import { configFilesCache, moduleListCache, moduleTuningCache } from "$lib/page-cache.svelte";
  import {
    confKeyHint,
    filterConfKeys,
    installedConfModules,
    stagedConfChanges,
  } from "$lib/conf-keys";
  import { clearSavedEdits } from "$lib/config-diff";
  import { applyEvent } from "$lib/terminal-state";
  import { restartState, noteApplyNeeded } from "$lib/restart-state.svelte";
  import { bannerText, normalizeApplyNeeded } from "$lib/apply-needed";
  import { requestConfFile, takeTuning } from "$lib/module-nav.svelte";
  import Terminal from "$lib/Terminal.svelte";
  import { termBuf, beginRun, clearBuf } from "$lib/term-store.svelte";
  import { featureLocked, LOCKED_HINT } from "$lib/features.svelte";
  import { taskbarBusy, taskbarIdle } from "$lib/taskbar";
  import { moduleBusy } from "$lib/module-busy.svelte";
  import { moduleUpdates, checkBadge, versionLabel } from "$lib/module-updates.svelte";
  import { canOfferUpdate, repoAfterUpdate, updateDoneNote, updatesWithServer } from "$lib/module-tabs";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { untrack } from "svelte";

  // `onupdated` lets the owning Modules page refresh its own module list
  // after a successful per-module update, so the rebuild banner / pending
  // chips light up over there without a manual Refresh.
  // `onOpenFile` (round 2, Task 3) lets the "Open config file" button below
  // switch the owning Modules page's tab strip to Files -- `tab` is
  // ModuleManager.svelte's own state, not something this component can
  // reach directly, so it's a plain callback prop (same shape as onupdated).
  let {
    active = false,
    onupdated,
    onOpenFile,
  }: { active?: boolean; onupdated?: () => void; onOpenFile?: () => void } = $props();

  // Opens the module's project page in the system browser -- same helper as
  // the Modules tab (registry-sourced https url, never user input).
  function openModUrl(url: string | null) {
    if (!url) return;
    openUrl(url).catch(() => {
      // Best-effort -- a failed browser launch shouldn't break the page.
    });
  }

  // Round 2, Task 3: the per-card "Open config file" fallback -- reuses the
  // SAME one-shot nav target + Files-tab click-to-open effect the Modules
  // tab's rows already drive (module-nav.svelte's requestConfFile ->
  // ModuleFiles.svelte's takeConfFile effect), just triggered from here
  // instead. Files outside the raw-write allowlist already open read-only
  // on that surface -- this reuses it verbatim, no new write path.
  function openConfigFile(conf: string) {
    requestConfFile(conf);
    onOpenFile?.();
  }

  const PB_RENDER_CAP = 200;

  let error: string | null = $state(null);
  let note: string | null = $state(null);
  let aleNote: string | null = $state(null);
  let liveNote = $state(false);

  // All three heavy reads are backed by the shared module-level caches
  // (page-cache.svelte) so re-opening the Modules → Tuning tab renders the
  // last-loaded cards INSTANTLY; the activation effects below refresh in the
  // background. `mtEdits` (staged, unsaved input) stays component-local and is
  // NEVER wiped by a background refresh -- only by an explicit save/reset.
  const mtSettings = $derived<ModuleTuning[]>(moduleTuningCache.store.data ?? []);
  const mtLoaded = $derived(moduleTuningCache.store.loaded);
  let mtEdits: Record<string, string> = $state({});
  let mtReloadPending = $state(false); // a lua knob changed -> reload ALE to apply

  // Server-modules inputs: module list (installed + conf_name) x config files
  // (which confs actually exist under env/dist/etc/modules) -- both cached.
  const smCpp = $derived<CppModule[]>(moduleListCache.store.data?.families.cpp ?? []);
  const smFiles = $derived<ConfFile[]>(configFilesCache.store.data ?? []);
  const smLoaded = $derived(moduleListCache.store.loaded && configFilesCache.store.loaded);
  const smModules = $derived(installedConfModules(smCpp, smFiles));
  // key -> full module row, for the version line + GitHub link per card.
  const cppByKey = $derived(new Map(smCpp.map((c) => [c.key, c])));

  // Per-card state, keyed by conf basename (server modules) or by the lua
  // module name prefixed "lua:" (expand map only).
  let mtExpand: Record<string, boolean> = $state({}); // collapsed by default
  let ckKeys: Record<string, ConfKey[]> = $state({});
  let ckSource: Record<string, string> = $state({});
  let ckLoaded: Record<string, boolean> = $state({});
  let ckErr: Record<string, string> = $state({});
  let ckQuery: Record<string, string> = $state({});
  let ckEdits: Record<string, Record<string, string>> = $state({});
  let ckHelpOpen: Record<string, boolean> = $state({}); // "<conf>:<key>" -> inline help shown
  let mtSavingCard = $state<string | null>(null);
  const mtSaving = $derived(mtSavingCard !== null);

  const buf = termBuf("config");

  // Refresh the tuning cache. Used by the activation effect (background) AND
  // by the save flows -- but only the save flows pass `saved` to clear staged
  // edits. A background refresh (no `saved`) must NEVER touch mtEdits, or a
  // tab switch would wipe the user's unsaved input.
  async function loadModuleTuning(saved?: string[]) {
    error = null;
    await moduleTuningCache.refresh();
    if (moduleTuningCache.store.error) {
      error = moduleTuningCache.store.error;
      return;
    }
    // A per-card save must not wipe the OTHER cards' pending curated edits --
    // drop only the just-saved keys (same pattern as the settings tabs).
    if (saved) mtEdits = clearSavedEdits(mtEdits, saved);
  }
  async function loadServerModules() {
    error = null;
    await Promise.all([moduleListCache.refresh(), configFilesCache.refresh()]);
    const e = moduleListCache.store.error ?? configFilesCache.store.error;
    if (e) error = e;
  }
  // First activation loads fresh; a re-open renders cached cards immediately
  // and this refreshes them in the background (single-flight in the cache).
  // untrack() keeps the effect depending ONLY on `active` -- the load helpers
  // read the caches' internal `loading`/`error` state synchronously, and
  // tracking those would turn each refresh into a re-trigger loop.
  $effect(() => {
    if (!active) return;
    untrack(() => {
      void loadModuleTuning();
      void loadServerModules();
    });
  });
  // Switching tabs keeps this component mounted -- one-shot notes must not
  // survive a leave-and-return (mirrors the old in-Config tab-change reset).
  $effect(() => {
    void active;
    aleNote = null;
    liveNote = false;
    note = null;
  });

  // Click-to-open catch-up (Modules-page round, Task 4): a pending tuning
  // target set on the Modules tab is consumed here once the server-module
  // list is actually loaded (takeTuning() clears it, so a later re-activation
  // of this tab is a no-op). Depends on `active` AND `smLoaded` -- if the
  // list is still loading when the tab first activates, `smLoaded` flipping
  // true re-runs this effect and the target is still there to consume.
  $effect(() => {
    if (!active || !smLoaded) return;
    const k = takeTuning();
    if (!k) return;
    const m = smModules.find((mm) => mm.key === k);
    if (!m) return;
    mtExpand[m.conf] = true;
    if (!ckLoaded[m.conf]) void loadConfKeys(m.conf);
    queueMicrotask(() => document.getElementById(`tune-${k}`)?.scrollIntoView({ block: "start" }));
  });

  async function loadConfKeys(conf: string) {
    ckErr[conf] = "";
    try {
      const r = await wowConfigConfKeys(conf);
      ckKeys[conf] = r.keys;
      ckSource[conf] = r.source;
      ckEdits[conf] = {};
      ckLoaded[conf] = true;
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      ckErr[conf] = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    }
  }
  function toggleCard(id: string, conf?: string) {
    mtExpand[id] = !mtExpand[id];
    if (conf && mtExpand[id] && !ckLoaded[conf]) void loadConfKeys(conf);
  }

  // Curated rows grouped where they render: conf-backend rows inside the
  // owning server-module card (matched on the conf file name the CLI reports
  // per row), lua-backend rows as their own cards in the Lua section.
  function curatedRowsFor(conf: string): ModuleTuning[] {
    return mtSettings.filter((s) => s.backend === "conf" && s.file === conf);
  }
  const luaModules = $derived.by(() => {
    const order: string[] = [];
    const byMod = new Map<string, ModuleTuning[]>();
    for (const s of mtSettings) {
      if (s.backend !== "lua") continue;
      if (!byMod.has(s.module)) {
        byMod.set(s.module, []);
        order.push(s.module);
      }
      byMod.get(s.module)!.push(s);
    }
    return order
      .map((m) => ({
        name: m,
        installed: byMod.get(m)!.every((r) => r.installed),
        rows: byMod.get(m)!,
      }))
      .filter((m) => m.installed);
  });

  function curatedDirty(rows: ModuleTuning[]): string[] {
    return rows
      .filter((s) => mtEdits[s.key] !== undefined && mtEdits[s.key] !== s.value)
      .map((s) => s.key);
  }
  function cardStaged(conf: string): { key: string; value: string }[] {
    return stagedConfChanges(ckKeys[conf] ?? [], ckEdits[conf] ?? {});
  }
  function cardDirtyCount(conf: string): number {
    return curatedDirty(curatedRowsFor(conf)).length + cardStaged(conf).length;
  }
  function ckShown(conf: string): ConfKey[] {
    return filterConfKeys(ckKeys[conf] ?? [], ckQuery[conf] ?? "").slice(0, PB_RENDER_CAP);
  }
  function ckMatchCount(conf: string): number {
    return filterConfKeys(ckKeys[conf] ?? [], ckQuery[conf] ?? "").length;
  }

  // One save per card: curated rows first (tuning-set keeps their special
  // validation), then the browser's staged edits (the generalized direct conf
  // route). `applied:"live"` from a save means the CLI fired the module's
  // known live-reload command over SOAP (mod-transmog) -- show the calm green
  // note instead of the restart banner, but only when NO write needs a restart.
  async function saveModuleCard(conf: string) {
    mtSavingCard = conf;
    error = null;
    liveNote = false;
    const curated = curatedDirty(curatedRowsFor(conf));
    try {
      let anyLive = false;
      let anyRestart = false;
      for (const key of curated) {
        const r = await wowConfigTuningSet(key, mtEdits[key]);
        const need = normalizeApplyNeeded(r);
        if (r.changed && need !== "none") {
          noteApplyNeeded(need);
          anyRestart = true;
        }
      }
      for (const c of cardStaged(conf)) {
        const r = await wowConfigSet(`conf:${conf}:${c.key}`, c.value);
        const need = normalizeApplyNeeded(r);
        if (need !== "none") {
          noteApplyNeeded(need);
          anyRestart = true;
        } else if (r.applied === "live") {
          anyLive = true;
        }
      }
      liveNote = anyLive && !anyRestart && !restartState.needed;
      await loadModuleTuning(curated);
      if (ckLoaded[conf]) await loadConfKeys(conf);
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    } finally {
      mtSavingCard = null;
    }
  }

  async function saveLuaCard(mod: { name: string; rows: ModuleTuning[] }) {
    mtSavingCard = `lua:${mod.name}`;
    error = null;
    const dirty = curatedDirty(mod.rows);
    try {
      for (const key of dirty) {
        const r = await wowConfigTuningSet(key, mtEdits[key]);
        if (r.changed) {
          if (r.backend === "lua") mtReloadPending = true;
          noteApplyNeeded(normalizeApplyNeeded(r));
        }
      }
      await loadModuleTuning(dirty);
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    } finally {
      mtSavingCard = null;
    }
  }

  async function reloadAle(): Promise<boolean> {
    error = null;
    aleNote = null;
    try {
      const r = await wowConsoleSend("reload ale");
      aleNote = r.result.trim();
      return true;
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
      return false;
    }
  }

  async function mtReloadAle() {
    // Keep the "reload to apply" banner up when the reload itself failed --
    // clearing it would tell the user the change is live when it isn't.
    if (await reloadAle()) mtReloadPending = false;
  }

  // --- Per-module update (module-update round) ------------------------------
  // Streams `wow module update --key <key>` into this tab's terminal (same
  // sawDone/streamErr contract as the Modules tab's rebuild flow: the outcome
  // comes from captured events, never from the promise resolving). The pull
  // never rebuilds -- on a changed compiled module the CLI marks it
  // rebuild-pending, so the trailing refresh + `onupdated` light the existing
  // rebuild banner up.
  // Gated on the SHARED moduleBusy flag (not component state): the Modules
  // tab stays mounted next to this one, so this pull must also disable its
  // Install/Remove/Rebuild/Update buttons -- and their streams must disable
  // this Update -- or two CLI mutations race on the same checkout.
  async function updateModule(key: string) {
    moduleBusy.busy = true;
    error = null;
    note = null;
    beginRun("config");
    taskbarBusy();
    let sawDone = false;
    let doneData: unknown;
    let streamErr: { message?: string; hint?: string } | null = null;
    let outcomeErr: unknown = null;
    try {
      await wowModuleUpdate(key, (e) => {
        buf.term = applyEvent(buf.term, e);
        if (e.event === "done") {
          sawDone = true;
          doneData = (e as { data?: unknown }).data;
        } else if (e.event === "error") {
          streamErr = (e as { error?: { message?: string; hint?: string } }).error ?? {};
        }
      });
    } catch (e) {
      outcomeErr = e;
    } finally {
      taskbarIdle();
      moduleBusy.busy = false;
      await loadServerModules();
      if (outcomeErr || streamErr) {
        const err = (outcomeErr ?? streamErr) as { message?: string; hint?: string };
        error = `${err.message ?? String(outcomeErr ?? "update failed")}${err.hint ? ` — ${err.hint}` : ""}`;
      } else if (sawDone) {
        const d = doneData as
          | { key?: string; changed?: boolean; after?: string; pending_rebuild?: boolean }
          | undefined;
        note = updateDoneNote(d);
        // The clone now sits at origin's head -- turn the cached chip off in
        // both tabs without re-running the whole check.
        if (d?.after && moduleUpdates.repos[key]) {
          moduleUpdates.repos[key] = repoAfterUpdate(moduleUpdates.repos[key], d.after);
        }
        onupdated?.();
      }
    }
  }
</script>

<section class="content">
  <header class="bar">
    <h2>Module tuning</h2>
  </header>

  {#if error}<div class="error-card"><p>{error}</p></div>{/if}
  {#if note}<p class="muted">{note}</p>{/if}
  {#if restartState.needed}
    <div class="warn-card"><p>{bannerText(restartState.apply)}</p></div>
  {:else if liveNote}
    <div class="live-card"><p>Applied live ✓ — the running server picked the change up, no restart needed.</p></div>
  {/if}

  {#if !mtLoaded || !smLoaded}
    <p class="muted">Loading…</p>
  {:else}
    <div class="card testing-card">
      <p class="muted">
        Every installed server module with a config file gets a card below — expand one to tune
        it. Friendly switches come first; <strong>All settings</strong> lists every key the
        module knows, with the author's own notes. Changes apply after a restart unless the card
        says otherwise.
      </p>
    </div>

    {#if mtReloadPending}
      <div class="warn-card">
        <p>Saved — reload the Lua scripts to apply the script (Lua) changes in-game.</p>
        <div class="row">
          <button
            class="primary"
            onclick={mtReloadAle}
            disabled={mtSaving || restartState.restarting || featureLocked("guided-config")}
            title={featureLocked("guided-config") ? LOCKED_HINT : undefined}
          >
            Reload Lua scripts
          </button>
        </div>
      </div>
    {/if}
    {#if aleNote}<p class="muted">{aleNote}</p>{/if}

    {#if smModules.length > 0}
      <h3>Server modules</h3>
      {#each smModules as m (m.key)}
        {@const curated = curatedRowsFor(m.conf)}
        {@const nDirty = cardDirtyCount(m.conf)}
        {@const cpp = cppByKey.get(m.key)}
        {@const ver = versionLabel(cpp?.head, cpp?.head_date)}
        {@const badge = checkBadge(moduleUpdates.checked, moduleUpdates.repos[m.key])}
        <div class="card mod-card" id="tune-{m.key}">
          <button
            class="mod-head"
            aria-expanded={!!mtExpand[m.conf]}
            onclick={() => toggleCard(m.conf, m.conf)}
          >
            <span class="sec-caret">{mtExpand[m.conf] ? "▾" : "▸"}</span>
            <strong>{m.name}</strong>
            <span class="muted mod-conf">{m.conf}</span>
            {#if ver}<span class="muted mod-ver">{ver}</span>{/if}
            {#if nDirty > 0}<span class="mod-dirty">{nDirty} unsaved</span>{/if}
          </button>
          {#if badge}
            <div class="row upd-row">
              <span class="upd-chip {badge.cls}">{badge.text}</span>
              {#if canOfferUpdate(m.key, moduleUpdates.repos[m.key]?.behind)}
                <button
                  class="primary"
                  onclick={() => updateModule(m.key)}
                  disabled={moduleBusy.busy || restartState.restarting || featureLocked("module-update")}
                  title={featureLocked("module-update")
                    ? LOCKED_HINT
                    : "Pull the module's latest source — a rebuild compiles it afterwards"}
                >
                  Update
                </button>
              {:else if updatesWithServer(m.key)}
                <span class="muted">updates with the server — use Server update on the Modules tab</span>
              {/if}
            </div>
          {/if}
          {#if mtExpand[m.conf]}
            {#if m.desc}<p class="muted mod-desc">{m.desc}</p>{/if}
            <div class="row">
              {#if cpp?.url}
                <button class="ghlink" onclick={() => openModUrl(cpp?.url ?? null)} title="Open the project page in your browser">GitHub ↗</button>
              {/if}
              <!-- Round 2, Task 3: the raw-edit fallback -- every card gets
                   this regardless of whether it has curated rows below, so
                   an installed module with nothing but "All settings" is
                   still one click from its conf. -->
              <button
                class="ghlink"
                onclick={() => openConfigFile(m.conf)}
                title="Edit {m.conf} directly on the Module files tab"
              >
                Open config file
              </button>
            </div>

            {#each curated as s (s.key)}
              <div class="setting" class:dirty={mtEdits[s.key] !== undefined && mtEdits[s.key] !== s.value}>
                <div class="meta">
                  <strong>{s.label}</strong>
                  <span class="muted">{s.explain}</span>
                </div>
                {#if s.type === "bool"}
                  <input
                    type="checkbox"
                    checked={(mtEdits[s.key] ?? s.value) === "1"}
                    disabled={mtSaving || restartState.restarting}
                    onchange={(e) => (mtEdits[s.key] = e.currentTarget.checked ? "1" : "0")}
                  />
                {:else if s.type === "int"}
                  <input
                    type="number"
                    min={s.min}
                    max={s.max}
                    step="1"
                    value={mtEdits[s.key] ?? s.value}
                    disabled={mtSaving || restartState.restarting}
                    oninput={(e) => (mtEdits[s.key] = e.currentTarget.value)}
                  />
                {:else}
                  <input
                    value={mtEdits[s.key] ?? s.value}
                    placeholder="e.g. 3,8 (0 = all)"
                    disabled={mtSaving || restartState.restarting}
                    oninput={(e) => (mtEdits[s.key] = e.currentTarget.value)}
                  />
                {/if}
              </div>
            {/each}

            <h4>All settings</h4>
            {#if ckErr[m.conf]}
              <p class="muted">Couldn't read {m.conf}: {ckErr[m.conf]}</p>
              <div class="row"><button onclick={() => loadConfKeys(m.conf)}>Try again</button></div>
            {:else if !ckLoaded[m.conf]}
              <p class="muted">Loading keys…</p>
            {:else}
              {#if ckSource[m.conf] === "dist"}
                <p class="muted">Showing the module's defaults — the first save creates {m.conf}.</p>
              {/if}
              <input
                placeholder="Search keys…"
                value={ckQuery[m.conf] ?? ""}
                oninput={(e) => (ckQuery[m.conf] = e.currentTarget.value)}
              />
              <div class="pb-list">
                {#each ckShown(m.conf) as k (k.key)}
                  <div
                    class="pbrow"
                    class:dirty={ckEdits[m.conf]?.[k.key] !== undefined && ckEdits[m.conf][k.key] !== k.value}
                  >
                    <span class="pbkey" title={confKeyHint(k)}>
                      {k.key}
                      {#if k.help}
                        <button
                          class="help-toggle"
                          type="button"
                          title="What does this do?"
                          onclick={() => (ckHelpOpen[`${m.conf}:${k.key}`] = !ckHelpOpen[`${m.conf}:${k.key}`])}
                        >?</button>
                      {/if}
                    </span>
                    <input
                      class="pbval"
                      value={ckEdits[m.conf]?.[k.key] ?? k.value}
                      disabled={mtSaving || restartState.restarting}
                      oninput={(e) => {
                        if (!ckEdits[m.conf]) ckEdits[m.conf] = {};
                        ckEdits[m.conf][k.key] = e.currentTarget.value;
                      }}
                    />
                  </div>
                  {#if k.help && ckHelpOpen[`${m.conf}:${k.key}`]}
                    <p class="muted key-help">
                      {k.help}{#if k.default !== null}&nbsp;(default {k.default}){/if}
                    </p>
                  {/if}
                {/each}
              </div>
              {#if ckMatchCount(m.conf) > PB_RENDER_CAP}
                <p class="muted">
                  Showing the first {PB_RENDER_CAP} of {ckMatchCount(m.conf)} matches — narrow the search.
                </p>
              {:else if ckMatchCount(m.conf) === 0}
                <p class="muted">No keys match.</p>
              {/if}
            {/if}

            <div class="row">
              <button
                class="primary"
                onclick={() => saveModuleCard(m.conf)}
                disabled={nDirty === 0 || mtSaving || restartState.restarting || featureLocked("guided-config")}
                title={featureLocked("guided-config") ? LOCKED_HINT : undefined}
              >
                Save {nDirty} change{nDirty === 1 ? "" : "s"}
              </button>
            </div>
          {/if}
        </div>
      {/each}
    {/if}

    {#if luaModules.length > 0}
      <h3>Lua scripts</h3>
      {#each luaModules as m (m.name)}
        {@const luaId = `lua:${m.name}`}
        {@const nDirty = curatedDirty(m.rows).length}
        <div class="card mod-card">
          <button
            class="mod-head"
            aria-expanded={!!mtExpand[luaId]}
            onclick={() => toggleCard(luaId)}
          >
            <span class="sec-caret">{mtExpand[luaId] ? "▾" : "▸"}</span>
            <strong>{m.name}</strong>
            <span class="muted mod-conf">{m.rows[0]?.file ?? ""}</span>
            {#if nDirty > 0}<span class="mod-dirty">{nDirty} unsaved</span>{/if}
          </button>
          {#if mtExpand[luaId]}
            {#each m.rows as s (s.key)}
              <div class="setting" class:dirty={mtEdits[s.key] !== undefined && mtEdits[s.key] !== s.value}>
                <div class="meta">
                  <strong>{s.label}</strong>
                  <span class="muted">{s.explain}</span>
                </div>
                {#if s.type === "bool"}
                  <input
                    type="checkbox"
                    checked={(mtEdits[s.key] ?? s.value) === "1"}
                    disabled={mtSaving || restartState.restarting}
                    onchange={(e) => (mtEdits[s.key] = e.currentTarget.checked ? "1" : "0")}
                  />
                {:else}
                  <input
                    type="number"
                    min={s.min}
                    max={s.max}
                    step="1"
                    value={mtEdits[s.key] ?? s.value}
                    disabled={mtSaving || restartState.restarting}
                    oninput={(e) => (mtEdits[s.key] = e.currentTarget.value)}
                  />
                {/if}
              </div>
            {/each}
            <div class="row">
              <button
                class="primary"
                onclick={() => saveLuaCard(m)}
                disabled={nDirty === 0 || mtSaving || restartState.restarting || featureLocked("guided-config")}
                title={featureLocked("guided-config") ? LOCKED_HINT : undefined}
              >
                Save {nDirty} change{nDirty === 1 ? "" : "s"}
              </button>
            </div>
          {/if}
        </div>
      {/each}
    {/if}

    {#if smModules.length === 0 && luaModules.length === 0}
      <div class="card">
        <strong>Nothing to tune yet</strong>
        <p class="muted">
          No installed module has a config file. Install modules on the
          <strong>Modules</strong> tab, then come back here to tune them.
        </p>
      </div>
    {/if}
  {/if}

  {#if buf.show}
    <Terminal state={buf.term} onclear={() => clearBuf("config")} logName="dml-config" />
  {/if}
</section>

<style>
  .content { padding: 20px 24px; overflow-y: auto; display: flex; flex-direction: column; gap: 12px; }
  .bar { display: flex; justify-content: space-between; align-items: center; }
  .bar h2 { margin: 0; font-size: 18px; }
  h3 { margin: 10px 0 0; font-size: 15px; color: #58a6ff; }
  .setting { display: flex; justify-content: space-between; align-items: center; gap: 16px; background: #0d1117; border: 1px solid #30363d; border-radius: 8px; padding: 10px 14px; }
  .setting.dirty { border-color: #d29922; }
  .card { background: #0d1117; border: 1px solid #30363d; border-radius: 8px; padding: 12px 16px; display: flex; flex-direction: column; gap: 6px; }
  .testing-card { margin-top: 6px; }
  .meta { display: flex; flex-direction: column; gap: 2px; }
  input { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 8px; }
  .row { display: flex; gap: 10px; align-items: center; }
  button { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 14px; cursor: pointer; }
  button.primary { background: #238636; border-color: #2ea043; color: white; }
  button:disabled { opacity: 0.5; cursor: default; }
  .muted { color: #8b949e; font-size: 13px; }
  .error-card { background: #161b22; border: 1px solid #f85149; border-radius: 8px; padding: 12px 16px; }
  .warn-card { background: #161b22; border: 1px solid #d29922; border-radius: 8px; padding: 12px 16px; }
  .live-card { background: #161b22; border: 1px solid #2ea043; border-radius: 8px; padding: 12px 16px; }
  /* Module tuning rework: collapsible per-module cards (sidebar-accordion
     caret style). The header is a full-width transparent button so the whole
     row toggles; content renders inside the same .card below it. */
  .mod-card { padding: 0; gap: 0; }
  .mod-card > :global(*) { margin: 0 14px; }
  .mod-head { display: flex; align-items: center; gap: 8px; width: 100%; margin: 0; background: transparent; border: none; padding: 12px 14px; text-align: left; cursor: pointer; color: #c9d1d9; font-size: 14px; border-radius: 8px; }
  .mod-head:hover { background: #161b22; }
  .sec-caret { font-size: 10px; width: 10px; display: inline-block; color: #8b949e; }
  .mod-conf { font-family: Consolas, monospace; font-size: 12px; }
  .mod-ver { font-family: Consolas, monospace; font-size: 12px; white-space: nowrap; }
  .mod-dirty { margin-left: auto; color: #d29922; font-size: 12px; white-space: nowrap; }
  .mod-desc { margin-top: 0; }
  .mod-card h4 { margin: 10px 14px 0; font-size: 13.5px; color: #58a6ff; }
  .mod-card .setting, .mod-card .row, .mod-card > input, .mod-card .pb-list { margin-left: 14px; margin-right: 14px; }
  .mod-card .row:last-child { margin-bottom: 12px; }
  .mod-card > p.muted { margin: 4px 14px 0; }
  /* Update-check badge row (module-update round): the Modules tab's badge
     language ("up to date" / "? behind" / amber "Update available") + the
     gated Update button. */
  .upd-row { margin-top: 2px; margin-bottom: 8px; }
  .upd-chip { font-size: 12px; padding: 2px 10px; border-radius: 10px; border: 1px solid #30363d; }
  .upd-chip.on { color: #3fb950; border-color: #3fb950; }
  .upd-chip.warn { color: #d29922; border-color: #d29922; }
  .upd-chip.off { color: #8b949e; }
  .ghlink { background: none; border: none; color: #58a6ff; font-size: 12px; padding: 0; cursor: pointer; }
  .ghlink:hover { text-decoration: underline; }
  .help-toggle { background: transparent; border: 1px solid #30363d; color: #8b949e; border-radius: 50%; width: 16px; height: 16px; line-height: 1; padding: 0; font-size: 10.5px; margin-left: 6px; cursor: pointer; }
  .help-toggle:hover { border-color: #58a6ff; color: #c9d1d9; }
  .key-help { margin: 0 6px 4px; padding-left: 6px; border-left: 2px solid #30363d; font-size: 12.5px; }
  .pb-list { display: flex; flex-direction: column; gap: 4px; max-height: 420px; overflow-y: auto; }
  .pbrow { display: flex; justify-content: space-between; align-items: center; gap: 12px; padding: 3px 6px; border-radius: 6px; }
  .pbrow.dirty { background: #1c1a10; outline: 1px solid #d29922; }
  .pbkey { font-family: Consolas, monospace; font-size: 12.5px; color: #c9d1d9; overflow-wrap: anywhere; }
  .pbval { width: 220px; flex-shrink: 0; font-family: Consolas, monospace; font-size: 12.5px; }
  .warn-card .row { margin-top: 8px; }
</style>
