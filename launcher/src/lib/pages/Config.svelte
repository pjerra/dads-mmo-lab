<script lang="ts">
  import { onMount } from "svelte";
  import {
    wowConfigList,
    wowConfigSet,
    wowConfigPbKeys,
    wowConfigFiles,
    wowConfigRawRead,
    wowConfigRawWrite,
    wowConfigRawReset,
    wowConsoleSend,
    gamesRestart,
    wowBotsFlush,
    wowAhbotRepair,
    wowModuleList,
    wowAccountwideGet,
    wowAccountwideSet,
    wowConfigTuningList,
    wowConfigTuningSet,
    type ConfFile,
    type ConfigSetting,
    type PbKey,
    type RawFileName,
    type AccountwideState,
    type AwSubsystem,
    type ModuleTuning,
  } from "$lib/api";
  import { filterPbKeys, stagedPbChanges } from "$lib/pb-keys";
  import { dirtyKeys, requiredSaveFlags, settingsInGroups } from "$lib/config-diff";
  import { applyEvent } from "$lib/terminal-state";
  import { restartState } from "$lib/restart-state.svelte";
  import Terminal from "$lib/Terminal.svelte";
  import { termBuf, beginRun, clearBuf } from "$lib/term-store.svelte";
  import CharPicker from "$lib/CharPicker.svelte";
  import { featureLocked, LOCKED_HINT, testingModeOn, setTestingMode } from "$lib/features.svelte";

  const WOW_ID = "wow-server-playerbots";
  // Static fallback shown until (or if) `wow config files` answers -- the
  // real list is dynamic since Batch 1 F3 (every installed module conf).
  const FALLBACK_FILES: ConfFile[] = [
    { name: ".env", exists: true, dist: false, readonly: true },
    { name: "docker-compose.override.yml", exists: true, dist: false, readonly: true },
    { name: "playerbots.conf", exists: true, dist: true, readonly: false },
  ];
  // UI mirror of the CLI's raw-write lock (cli rejects these two names).
  const READONLY_FILES: RawFileName[] = [".env", "docker-compose.override.yml"];

  // One Config page, four tabs (was four sidebar entries). `tab` is internal
  // state now; the in-page tab bar switches it, and the lazy-load $effects
  // below fire when a tab first becomes active.
  type ConfigTab = "settings" | "botworld" | "ahbot" | "accountwide" | "moduletuning" | "files";
  let tab = $state<ConfigTab>("settings");
  const TABS: { id: ConfigTab; label: string }[] = [
    { id: "settings", label: "Settings" },
    { id: "botworld", label: "Bot World" },
    { id: "ahbot", label: "Auction House" },
    { id: "accountwide", label: "Account-wide" },
    { id: "moduletuning", label: "Module tuning" },
    { id: "files", label: "Module files" },
  ];
  let settings: ConfigSetting[] = $state([]);
  let edits: Record<string, string> = $state({});
  let error: string | null = $state(null);
  let saving = $state(false);

  let file: RawFileName = $state(".env");
  let fileContent = $state("");
  let fileLoaded = $state(false);
  let loadingFile = $state(false);
  let lastBackup: string | null = $state(null);
  let confFiles: ConfFile[] = $state(FALLBACK_FILES);
  let confFilesLoaded = $state(false);
  let confirmingReset = $state(false);
  let resetting = $state(false);

  async function loadConfFiles() {
    try {
      confFiles = await wowConfigFiles();
      confFilesLoaded = true;
      if (!confFiles.some((f) => f.name === file) && confFiles.length > 0) {
        file = confFiles[0].name;
      }
    } catch {
      // Keep the static fallback -- the picker still works for the basics.
    }
  }
  $effect(() => {
    if (tab === "files" && !confFilesLoaded) void loadConfFiles();
  });

  const currentFileMeta = $derived(confFiles.find((f) => f.name === file));

  async function resetFile() {
    if (!confirmingReset) {
      confirmingReset = true;
      return;
    }
    confirmingReset = false;
    resetting = true;
    error = null;
    try {
      const target = file;
      const r = await wowConfigRawReset(target);
      restartState.needed = true;
      if (file === target) await loadFile();
      lastBackup = r.backup;
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    } finally {
      resetting = false;
    }
  }

  const buf = termBuf("config");

  let aleNote: string | null = $state(null);

  // --- Bot World all-keys browser (Batch 1 F2) -----------------------------
  const PB_RENDER_CAP = 200;
  let pbKeys: PbKey[] = $state([]);
  let pbLoaded = $state(false);
  let pbQuery = $state("");
  let pbEdits: Record<string, string> = $state({});
  let pbSaving = $state(false);
  const pbFiltered = $derived(filterPbKeys(pbKeys, pbQuery));
  const pbShown = $derived(pbFiltered.slice(0, PB_RENDER_CAP));
  const pbStaged = $derived(stagedPbChanges(pbKeys, pbEdits));

  async function loadPbKeys() {
    try {
      const r = await wowConfigPbKeys();
      pbKeys = r.keys;
      pbEdits = {};
      pbLoaded = true;
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    }
  }
  $effect(() => {
    if (tab === "botworld" && !pbLoaded) void loadPbKeys();
  });

  // --- Flush & rebuild bot population (Batch 1 F4) -------------------------
  let flushConfirm = $state("");
  let flushing = $state(false);

  async function runFlush() {
    if (flushConfirm !== "flush") return;
    flushConfirm = "";
    flushing = true;
    restartState.restarting = true; // the flush restarts the server twice
    error = null;
    beginRun("config");
    try {
      await wowBotsFlush((e) => {
        buf.term = applyEvent(buf.term, e);
      });
    } catch (e) {
      const err = e as { code?: string; message?: string; hint?: string };
      buf.term = applyEvent(buf.term, {
        event: "error",
        error: { code: err.code ?? "IPC", message: err.message ?? String(e), hint: err.hint ?? "" },
      });
    } finally {
      flushing = false;
      restartState.restarting = false;
    }
  }

  // --- Auction House repair (Batch 4 F14) ----------------------------------
  // Streams `wow ahbot repair` -- character lookup + mod_ahbot.conf writes.
  // Creating the bot's account/character is a MANUAL step (the CLI's port of
  // wow-manage.sh keeps it manual on purpose); the card explains it.
  let ahRepairChar = $state("");
  let ahRepairConfirm = $state(false);
  let ahRepairing = $state(false);

  // Batch 2 (overnight): detect which AH fork is installed from the module
  // list so the card labels itself + the repair targets the right one. Both
  // forks write mod_ahbot.conf, so the curated conf rows/reads are unchanged
  // -- only the display label depends on this. Neither installed -> the
  // standard "Auction House Bot" label + the existing setup hint.
  let ahModuleKey = $state<"mod-ah-bot-plus" | "mod-ah-bot" | null>(null);
  let ahModuleLoaded = $state(false);
  const ahModuleLabel = $derived(
    ahModuleKey === "mod-ah-bot-plus" ? "Auction House Bot Plus" : "Auction House Bot",
  );

  async function loadAhModule() {
    try {
      const cpp = (await wowModuleList()).families.cpp;
      if (cpp.some((m) => m.key === "mod-ah-bot-plus" && m.installed)) ahModuleKey = "mod-ah-bot-plus";
      else if (cpp.some((m) => m.key === "mod-ah-bot" && m.installed)) ahModuleKey = "mod-ah-bot";
      else ahModuleKey = null;
      ahModuleLoaded = true;
    } catch {
      // Keep the standard label -- detection is a nicety, not a gate.
    }
  }
  $effect(() => {
    if (tab === "ahbot" && !ahModuleLoaded) void loadAhModule();
  });

  async function runAhRepair() {
    if (!ahRepairConfirm) {
      ahRepairConfirm = true;
      return;
    }
    ahRepairConfirm = false;
    if (!ahRepairChar) return;
    ahRepairing = true;
    error = null;
    beginRun("config");
    try {
      await wowAhbotRepair(ahRepairChar, (e) => {
        buf.term = applyEvent(buf.term, e);
        const ev = e as { event?: string; data?: { restart_required?: boolean } };
        if (ev.event === "done" && ev.data?.restart_required) restartState.needed = true;
      });
    } catch (e) {
      const err = e as { code?: string; message?: string; hint?: string };
      buf.term = applyEvent(buf.term, {
        event: "error",
        error: { code: err.code ?? "IPC", message: err.message ?? String(e), hint: err.hint ?? "" },
      });
    } finally {
      ahRepairing = false;
    }
  }

  // --- Account-wide sharing configurator (overnight Batch 1) ---------------
  // Toggles the accountwide module's ENABLE_* flags in the deployed lua
  // files. The whole tab is locked behind [accountwide-config] until its
  // smoke test passes. Only shown as usable when the module is installed.
  let awState = $state<AccountwideState | null>(null);
  let awLoaded = $state(false);
  let awError = $state<string | null>(null); // initial-load failed -> show retry, not a stuck "Loading…"
  let awSaving = $state(false);
  let awReloadPending = $state(false); // a flag changed -> reload ALE to apply

  async function loadAccountwide() {
    error = null;
    awError = null;
    try {
      awState = await wowAccountwideGet();
      awLoaded = true;
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      const msg = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
      error = msg;
      // Without this the tab stays on "Loading…" forever when the very first
      // load fails; awError drives a retry card in that not-yet-loaded state.
      awError = msg;
    }
  }
  $effect(() => {
    if (tab === "accountwide" && !awLoaded) void loadAccountwide();
  });

  const awByKey = $derived(new Map((awState?.subsystems ?? []).map((s) => [s.key, s])));
  function awValueOf(key: string): "on" | "off" {
    return awByKey.get(key)?.value ?? "off";
  }
  // Indent depth = number of ancestors (money -> live sync -> alt-bot sync).
  function awDepth(s: AwSubsystem): number {
    let d = 0;
    let p = s.parent;
    while (p) {
      d++;
      p = awByKey.get(p)?.parent ?? null;
    }
    return d;
  }
  // A sub-toggle has no effect until its parent system is on -- disable it.
  function awParentOff(s: AwSubsystem): boolean {
    return s.parent !== null && awValueOf(s.parent) === "off";
  }

  // Returns true on success so the control's onchange can revert the checkbox/
  // select back to the (unchanged) server value when a write fails -- otherwise
  // the widget keeps showing the new position while the server never moved.
  async function setAwFlag(
    key: string,
    value: "on" | "off",
    variant?: "default" | "custom",
  ): Promise<boolean> {
    awSaving = true;
    error = null;
    try {
      const r = await wowAccountwideSet(key, value, variant);
      if (r.changed) awReloadPending = true;
      await loadAccountwide();
      return true;
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
      return false;
    } finally {
      awSaving = false;
    }
  }

  // Reputation is pick-one: "off" or a variant. Picking a variant deletes the
  // other file server-side (only one may load).
  const repSelect = $derived(
    awState && awState.reputation.value === "on"
      ? (awState.reputation.active ?? "off")
      : "off",
  );
  async function setAwReputation(sel: string): Promise<boolean> {
    if (sel === "off") return await setAwFlag("ENABLE_ACCOUNTWIDE_REPUTATION", "off");
    return await setAwFlag("ENABLE_ACCOUNTWIDE_REPUTATION", "on", sel as "default" | "custom");
  }
  const repVariantLabel = (v: string) =>
    v === "default"
      ? "Default (standard AzerothCore factions)"
      : "Custom (custom race/faction build)";

  async function awReloadAle() {
    // Keep the "reload to apply" banner up when the reload itself failed --
    // clearing it would tell the user the change is live when it isn't.
    if (await reloadAle()) awReloadPending = false;
  }

  // --- Guided module tuning (overnight Batch 3) ----------------------------
  // Curated activator knobs for a few optional modules: NPC Beastmaster +
  // Learn Spells (their .conf) and Unlimited Ammo + Sit Means Rest (their
  // deployed ALE .lua). All writes are locked behind the single [guided-config]
  // flag until its smoke test passes. Conf changes need a restart; lua changes
  // apply with `.reload ale`.
  let mtSettings = $state<ModuleTuning[]>([]);
  let mtLoaded = $state(false);
  let mtEdits: Record<string, string> = $state({});
  let mtSaving = $state(false);
  let mtReloadPending = $state(false); // a lua knob changed -> reload ALE to apply

  async function loadModuleTuning() {
    error = null;
    try {
      mtSettings = await wowConfigTuningList();
      mtEdits = {};
      mtLoaded = true;
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    }
  }
  $effect(() => {
    if (tab === "moduletuning" && !mtLoaded) void loadModuleTuning();
  });

  // Stable module order (first appearance in the list), each with its rows.
  const mtModules = $derived.by(() => {
    const order: string[] = [];
    const byMod = new Map<string, ModuleTuning[]>();
    for (const s of mtSettings) {
      if (!byMod.has(s.module)) {
        byMod.set(s.module, []);
        order.push(s.module);
      }
      byMod.get(s.module)!.push(s);
    }
    return order.map((m) => ({
      name: m,
      installed: byMod.get(m)!.every((r) => r.installed),
      rows: byMod.get(m)!,
    }));
  });
  const mtDirty = $derived(
    mtSettings.filter((s) => mtEdits[s.key] !== undefined && mtEdits[s.key] !== s.value).map((s) => s.key),
  );

  async function saveModuleTuning() {
    mtSaving = true;
    error = null;
    try {
      for (const key of mtDirty) {
        const r = await wowConfigTuningSet(key, mtEdits[key]);
        if (r.changed) {
          if (r.backend === "lua") mtReloadPending = true;
          if (r.restart_required) restartState.needed = true;
        }
      }
      await loadModuleTuning();
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    } finally {
      mtSaving = false;
    }
  }

  async function mtReloadAle() {
    await reloadAle();
    mtReloadPending = false;
  }

  async function savePbChanges() {
    pbSaving = true;
    error = null;
    try {
      for (const c of pbStaged) {
        const r = await wowConfigSet(`conf:playerbots.conf:${c.key}`, c.value);
        if (r.restart_required) restartState.needed = true;
      }
      await loadPbKeys();
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    } finally {
      pbSaving = false;
    }
  }

  const groups = $derived([...new Set(settings.map((s) => s.group))]);
  // "Bot ..."-prefixed groups render on the Bot World tab, "Auction..."
  // groups on the Auction House tab (Batch 4 F14), everything else on
  // Settings.
  const visibleGroups = $derived(
    tab === "botworld"
      ? groups.filter((g) => g.startsWith("Bot "))
      : tab === "ahbot"
        ? groups.filter((g) => g.startsWith("Auction"))
        : groups.filter((g) => !g.startsWith("Bot ") && !g.startsWith("Auction")),
  );
  // Scope dirty/toSave/saveLocked to the rows the CURRENT tab shows: the
  // Settings/Bot World/Auction House tabs share the single settings+edits map,
  // so an unscoped `dirty` would let each tab's Save write (and lock on) the
  // other tabs' dirty rows.
  const visibleSettings = $derived(settingsInGroups(settings, visibleGroups));
  const dirty = $derived(dirtyKeys(visibleSettings, edits));
  const fileReadonly = $derived(
    confFiles.find((f) => f.name === file)?.readonly ?? READONLY_FILES.includes(file),
  );
  // Conf-file rows (Batch 1) are a new save mechanism gated behind their own
  // flags -- the Save button locks when ANY dirty row's flag is still locked.
  const saveLocked = $derived(requiredSaveFlags(visibleSettings, dirty).some((f) => featureLocked(f)));
  let liveNote = $state(false);

  async function load() {
    error = null;
    try {
      settings = await wowConfigList();
      edits = {};
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    }
  }
  onMount(load);

  async function saveSettings(): Promise<boolean> {
    saving = true;
    error = null;
    aleNote = null;
    liveNote = false;
    try {
      const toSave = dirty;
      let anyLive = false;
      let anyRestart = false;
      for (const key of toSave) {
        const r = await wowConfigSet(key, edits[key]);
        if (r.restart_required) {
          restartState.needed = true;
          anyRestart = true;
        } else if (r.applied === "live") {
          anyLive = true;
        }
      }
      // Conf rows the running server picked up over SOAP -- show the calm
      // "applied live" note instead of the restart banner (only when NO
      // saved row still needs a restart).
      liveNote = anyLive && !anyRestart;
      await load();
      return true;
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
      return false;
    } finally {
      saving = false;
    }
  }

  async function loadFile() {
    error = null;
    aleNote = null;
    fileLoaded = false;
    lastBackup = null;
    loadingFile = true;
    const target = file;
    try {
      const r = await wowConfigRawRead(target);
      if (file === target) {
        fileContent = r.content;
        fileLoaded = true;
      }
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      if (file === target) {
        error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
      }
    } finally {
      loadingFile = false;
    }
  }

  async function saveFile(): Promise<boolean> {
    saving = true;
    error = null;
    aleNote = null;
    try {
      const targetFile = file;
      const content = fileContent;
      const r = await wowConfigRawWrite(targetFile, content);
      lastBackup = r.backup;
      restartState.needed = true;
      return true;
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
      return false;
    } finally {
      saving = false;
    }
  }

  async function reloadAle(): Promise<boolean> {
    saving = true;
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
    } finally {
      saving = false;
    }
  }

  let confirmingRestart = $state(false);
  // Switching between the Settings and Modules sidebar entries changes `tab`
  // without remounting -- an armed "sure?" confirmation must not survive that.
  $effect(() => {
    void tab;
    confirmingRestart = false;
    ahRepairConfirm = false;
    aleNote = null;
    liveNote = false;
  });
  async function saveAndRestart(saveFn: () => Promise<boolean>) {
    if (!confirmingRestart) {
      confirmingRestart = true;
      return;
    }
    confirmingRestart = false;
    if (!(await saveFn())) return;
    restartState.restarting = true;
    beginRun("config");
    try {
      // Applying settings is a deliberate, infrequent restart -- always save
      // characters first (false = don't skip). The "faster restart" option
      // lives on Home for the routine restart button.
      await gamesRestart(WOW_ID, false, (e) => {
        buf.term = applyEvent(buf.term, e);
      });
      restartState.needed = false;
    } catch (e) {
      const err = e as { code?: string; message?: string; hint?: string };
      buf.term = applyEvent(buf.term, {
        event: "error",
        error: { code: err.code ?? "IPC", message: err.message ?? String(e), hint: err.hint ?? "" },
      });
    } finally {
      restartState.restarting = false;
    }
  }

  function onFileSelect() {
    // Changing which file is targeted must invalidate whatever was loaded/armed
    // for the previous file -- otherwise a stale `fileContent` could get written
    // to the newly selected `file` (or a stale restart confirmation could fire
    // for content the user never actually confirmed).
    fileLoaded = false;
    fileContent = "";
    lastBackup = null;
    confirmingRestart = false;
    confirmingReset = false;
    aleNote = null;
  }
</script>

<section class="content" class:fill={tab === "files" && fileLoaded}>
  <header class="bar">
    <div class="subtabs" role="tablist">
      {#each TABS as t (t.id)}
        <button
          role="tab"
          aria-selected={tab === t.id}
          class:active={tab === t.id}
          onclick={() => (tab = t.id)}
        >{t.label}</button>
      {/each}
    </div>
  </header>

  {#if error}<div class="error-card"><p>{error}</p></div>{/if}
  {#if restartState.needed}
    <div class="warn-card"><p>Saved — restart the server to apply the changes.</p></div>
  {:else if liveNote}
    <div class="live-card"><p>Applied live ✓ — the running server picked the change up, no restart needed.</p></div>
  {/if}

  {#if tab === "settings" || tab === "botworld" || tab === "ahbot"}
    {#if tab === "settings"}
      <div class="card testing-card">
        <label class="row">
          <input
            type="checkbox"
            checked={testingModeOn()}
            onchange={(e) => setTestingMode(e.currentTarget.checked)}
          />
          Enable untested features (for smoke testing)
        </label>
        <p class="muted">
          Untested features stay disabled until their smoke test passes. The checklist lives in
          docs/SMOKE-TESTS.md.
        </p>
      </div>
    {/if}
    {#each visibleGroups as g (g)}
      <h3>{g}</h3>
      {#each settings.filter((s) => s.group === g) as s (s.key)}
        <div class="setting" class:dirty={dirty.includes(s.key)}>
          <div class="meta">
            <strong>{s.label}</strong>
            <span class="muted">{s.explain}</span>
          </div>
          {#if s.type === "bool"}
            <input
              type="checkbox"
              checked={(edits[s.key] ?? s.value) === "1"}
              disabled={saving || restartState.restarting}
              onchange={(e) => {
                edits[s.key] = e.currentTarget.checked ? "1" : "0";
                confirmingRestart = false;
              }}
            />
          {:else if s.type === "float" || s.type === "int"}
            <input
              type="number"
              min={s.min}
              max={s.max}
              step={s.type === "float" ? "0.5" : "1"}
              value={edits[s.key] ?? s.value}
              disabled={saving || restartState.restarting}
              oninput={(e) => {
                edits[s.key] = e.currentTarget.value;
                confirmingRestart = false;
              }}
            />
          {:else if s.type === "char"}
            <div class="charwrap">
              <span class="muted">current id: {s.value}</span>
              <CharPicker
                selected={edits[s.key] ?? ""}
                disabled={saving || restartState.restarting}
                onpick={(v: string) => {
                  edits[s.key] = v;
                  confirmingRestart = false;
                }}
              />
            </div>
          {:else}
            <input
              value={edits[s.key] ?? s.value}
              disabled={saving || restartState.restarting}
              oninput={(e) => {
                edits[s.key] = e.currentTarget.value;
                confirmingRestart = false;
              }}
            />
          {/if}
        </div>
      {/each}
    {/each}
    <div class="row">
      <button
        class="primary"
        onclick={saveSettings}
        disabled={dirty.length === 0 || saving || restartState.restarting || saveLocked}
        title={saveLocked ? LOCKED_HINT : undefined}
      >
        Save {dirty.length > 0 ? `(${dirty.length})` : ""}
      </button>
      <button
        onclick={() => saveAndRestart(saveSettings)}
        disabled={dirty.length === 0 || saving || restartState.restarting || saveLocked}
        title={saveLocked ? LOCKED_HINT : undefined}
      >
        {confirmingRestart ? "This disconnects players — sure?" : "Save & Restart"}
      </button>
    </div>

    {#if tab === "ahbot"}
      <h3>Repair {ahModuleLabel}</h3>
      <div class="card">
        {#if ahModuleLoaded && ahModuleKey === "mod-ah-bot-plus"}
          <p class="muted">
            Detected the <strong>Auction House Bot Plus</strong> fork (blizzlike pricing).
            Repair and the conf settings both target it — both forks use the same
            mod_ahbot.conf, so nothing below changes.
          </p>
        {:else if ahModuleLoaded && ahModuleKey === null}
          <p class="muted">
            No Auction House Bot module is installed yet — install
            <strong>Auction House Bot</strong> (or <strong>Auction House Bot Plus</strong>)
            from the Modules page first.
          </p>
        {/if}
        <p class="muted">
          The auction bot lists and bids as a real character. Give it its own dedicated
          account so <strong>you</strong> can buy the bot's auctions — auctions from your own
          account would be invisible to you in-game.
        </p>
        <ol class="ah-steps">
          <li>Create a separate account for the bot on the <strong>Accounts</strong> page.</li>
          <li>Log into the game with that account once and create <strong>one</strong> character
            (race and class don't matter — it will never be played).</li>
          <li>Log out of the game completely.</li>
          <li>Pick that character below and click Repair.</li>
        </ol>
        <p class="muted">
          The bot character should not be used for play — it is busy running the auction
          house around the clock.
        </p>
        <div class="row">
          <CharPicker
            selected={ahRepairChar}
            disabled={ahRepairing || restartState.restarting}
            onpick={(v: string) => {
              ahRepairChar = v;
              ahRepairConfirm = false;
            }}
          />
          <button
            class="primary"
            onclick={runAhRepair}
            disabled={!ahRepairChar || ahRepairing || restartState.restarting || featureLocked("ahbot-page")}
            title={featureLocked("ahbot-page") ? LOCKED_HINT : undefined}
          >
            {ahRepairConfirm ? `Make ${ahRepairChar} the auction bot — sure?` : `Repair ${ahModuleLabel}`}
          </button>
        </div>
      </div>
    {/if}

    {#if tab === "botworld"}
      <h3>All playerbots.conf keys</h3>
      <div class="card">
        <p class="muted">
          Every setting the bots module knows, straight from playerbots.conf. Changes apply after a
          server restart. Hover a key for its default value.
        </p>
        <input placeholder="Search keys… (e.g. broadcast, teleport, revive)" bind:value={pbQuery} />
        {#if pbLoaded}
          <div class="pb-list">
            {#each pbShown as k (k.key)}
              <div class="pbrow" class:dirty={pbEdits[k.key] !== undefined && pbEdits[k.key] !== k.value}>
                <span class="pbkey" title={k.default !== null ? `Default: ${k.default}` : "No default recorded"}>{k.key}</span>
                <input
                  class="pbval"
                  value={pbEdits[k.key] ?? k.value}
                  disabled={pbSaving || restartState.restarting}
                  oninput={(e) => (pbEdits[k.key] = e.currentTarget.value)}
                />
              </div>
            {/each}
          </div>
          {#if pbFiltered.length > PB_RENDER_CAP}
            <p class="muted">Showing the first {PB_RENDER_CAP} of {pbFiltered.length} matches — narrow the search.</p>
          {:else if pbFiltered.length === 0}
            <p class="muted">No keys match.</p>
          {/if}
          <div class="row">
            <button
              class="primary"
              onclick={savePbChanges}
              disabled={pbStaged.length === 0 || pbSaving || restartState.restarting || featureLocked("bots-world")}
              title={featureLocked("bots-world") ? LOCKED_HINT : undefined}
            >
              Save {pbStaged.length} change{pbStaged.length === 1 ? "" : "s"}
            </button>
          </div>
        {:else}
          <p class="muted">Loading keys…</p>
        {/if}
      </div>

      <h3>Danger zone</h3>
      <div class="card danger-card">
        <strong>Flush &amp; rebuild the bot population</strong>
        <p class="muted">
          Deletes ALL ~{settings.find((s) => s.key === "bots.population")?.value ?? "2000"} random
          bots' characters, auctions and mail, then rebuilds the population from your settings above.
          Your own characters and party bots on real accounts are untouched. A character backup is
          taken first. The server restarts twice — this takes several minutes.
        </p>
        <div class="row">
          <input
            placeholder={'Type "flush" to confirm'}
            bind:value={flushConfirm}
            disabled={flushing || restartState.restarting}
          />
          <button
            class="danger"
            onclick={runFlush}
            disabled={flushConfirm !== "flush" || flushing || restartState.restarting || featureLocked("bots-flush")}
            title={featureLocked("bots-flush") ? LOCKED_HINT : undefined}
          >
            Flush &amp; rebuild
          </button>
        </div>
      </div>
    {/if}

  {:else if tab === "accountwide"}
    {#if !awLoaded}
      {#if awError}
        <div class="card">
          <strong>Couldn't load the account-wide settings</strong>
          <p class="muted">{awError}</p>
          <div class="row">
            <button onclick={loadAccountwide}>Try again</button>
          </div>
        </div>
      {:else}
        <p class="muted">Loading…</p>
      {/if}
    {:else if !awState?.installed}
      <div class="card">
        <strong>Account-wide sharing isn't installed yet</strong>
        <p class="muted">
          This shares things like achievements, mounts, pets, gold and titles across every
          character on the same account. Install <strong>Accountwide Systems</strong> from the
          <strong>Modules</strong> page (Lua scripts), then reopen this tab to turn each system on.
        </p>
      </div>
    {:else}
      <div class="card testing-card">
        <p class="muted">
          Turn on any system below to share it across all characters on an account. Everything
          ships off. Changes are written to the server's script files — click
          <strong>Reload account-wide scripts</strong> (or restart the server) to make them take
          effect in-game.
        </p>
      </div>

      {#if awReloadPending}
        <div class="warn-card">
          <p>Saved — reload the account-wide scripts to apply the change in-game.</p>
          <div class="row">
            <button
              class="primary"
              onclick={awReloadAle}
              disabled={awSaving || restartState.restarting || featureLocked("accountwide-config")}
              title={featureLocked("accountwide-config") ? LOCKED_HINT : undefined}
            >
              Reload account-wide scripts
            </button>
          </div>
        </div>
      {/if}
      {#if aleNote}<p class="muted">{aleNote}</p>{/if}

      {#each awState.subsystems as s (s.key)}
        <div class="setting aw-row" style={`margin-left:${awDepth(s) * 22}px`}>
          <div class="meta">
            <strong>{s.label}</strong>
            <span class="muted">{s.explain}</span>
            {#if awParentOff(s)}
              <span class="muted aw-hint">Turn on the system above for this to have any effect.</span>
            {/if}
          </div>
          <input
            type="checkbox"
            checked={s.value === "on"}
            disabled={awSaving || restartState.restarting || featureLocked("accountwide-config")}
            title={featureLocked("accountwide-config") ? LOCKED_HINT : undefined}
            onchange={(e) => {
              const el = e.currentTarget;
              void setAwFlag(s.key, el.checked ? "on" : "off").then((ok) => {
                // On failure the server value never moved -- snap the checkbox
                // back to it (Svelte won't, the bound value didn't change).
                if (!ok) el.checked = s.value === "on";
              });
            }}
          />
        </div>
      {/each}

      {#if awState.reputation.present}
        <div class="setting aw-row">
          <div class="meta">
            <strong>Reputation</strong>
            <span class="muted">
              Share faction reputation across characters of the same faction. Two versions may be
              installed — only one can run, so picking one removes the other.
            </span>
          </div>
          <select
            value={repSelect}
            disabled={awSaving || restartState.restarting || featureLocked("accountwide-config")}
            title={featureLocked("accountwide-config") ? LOCKED_HINT : undefined}
            onchange={(e) => {
              const el = e.currentTarget;
              void setAwReputation(el.value).then((ok) => {
                // Revert the dropdown to the server's value on a failed write.
                if (!ok) el.value = repSelect;
              });
            }}
          >
            <option value="off">Off</option>
            {#each awState.reputation.variants as v (v)}
              <option value={v}>{repVariantLabel(v)}</option>
            {/each}
          </select>
        </div>
      {/if}
    {/if}

  {:else if tab === "moduletuning"}
    {#if !mtLoaded}
      <p class="muted">Loading…</p>
    {:else}
      <div class="card testing-card">
        <p class="muted">
          Plain-language switches for a few optional modules. Turn a knob and click <strong>Save</strong>.
          Install the module first from the <strong>Modules</strong> page if a card says it isn't installed —
          the settings still show here but won't take effect until it is.
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

      {#each mtModules as m (m.name)}
        <h3>{m.name}</h3>
        {#if !m.installed}
          <p class="muted">
            Not installed — install <strong>{m.name}</strong> from the Modules page first, then reopen this tab.
          </p>
        {/if}
        {#each m.rows as s (s.key)}
          <div class="setting" class:dirty={mtDirty.includes(s.key)}>
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
      {/each}

      <div class="row">
        <button
          class="primary"
          onclick={saveModuleTuning}
          disabled={mtDirty.length === 0 || mtSaving || restartState.restarting || featureLocked("guided-config")}
          title={featureLocked("guided-config") ? LOCKED_HINT : undefined}
        >
          Save {mtDirty.length > 0 ? `(${mtDirty.length})` : ""}
        </button>
      </div>
    {/if}

  {:else}
    <div class="row">
      <select bind:value={file} onchange={onFileSelect} disabled={saving || restartState.restarting || loadingFile}>
        {#each confFiles as f (f.name)}
          <option value={f.name}>{f.name}{!f.exists && f.dist ? " (new — starts from defaults)" : ""}</option>
        {/each}
      </select>
      <button onclick={loadFile} disabled={saving || restartState.restarting || loadingFile}>Open</button>
    </div>
    <p class="muted">
      Edited mod_ale.conf or its Lua scripts on disk?
      <button
        onclick={reloadAle}
        disabled={saving || restartState.restarting || loadingFile || featureLocked("ale-reload")}
        title={featureLocked("ale-reload") ? LOCKED_HINT : undefined}
      >
        Reload ALE scripts
      </button>
    </p>
    {#if aleNote}<p class="muted">{aleNote}</p>{/if}
    {#if fileLoaded}
      <textarea
        rows="18"
        spellcheck="false"
        bind:value={fileContent}
        oninput={() => {
          confirmingRestart = false;
          confirmingReset = false;
        }}
        readonly={fileReadonly}
        disabled={saving || restartState.restarting}
      ></textarea>
      {#if fileReadonly}
        <p class="muted">Read-only — locked so a bad edit can't run commands on your PC. Change these via the Settings page.</p>
      {:else}
        {#if lastBackup}<p class="muted">Previous version kept as {lastBackup}</p>{/if}
        <div class="row">
          <button
            class="primary"
            onclick={saveFile}
            disabled={saving || restartState.restarting || resetting || featureLocked("config-edit")}
            title={featureLocked("config-edit") ? LOCKED_HINT : undefined}
          >
            Save
          </button>
          <button
            onclick={() => saveAndRestart(saveFile)}
            disabled={saving || restartState.restarting || resetting || featureLocked("config-edit")}
            title={featureLocked("config-edit") ? LOCKED_HINT : undefined}
          >
            {confirmingRestart ? "This disconnects players — sure?" : "Save & Restart"}
          </button>
          {#if currentFileMeta?.dist}
            <button
              class="danger"
              onclick={resetFile}
              disabled={saving || restartState.restarting || resetting || featureLocked("config-reset")}
              title={featureLocked("config-reset") ? LOCKED_HINT : undefined}
            >
              {confirmingReset ? `Overwrite ${file} with its defaults — sure?` : "Reset to defaults"}
            </button>
          {/if}
        </div>
      {/if}
    {/if}
  {/if}

  {#if buf.show}
    <Terminal state={buf.term} onclear={() => clearBuf("config")} logName="dml-config" />
  {/if}
</section>

<style>
  .content { padding: 20px 24px; overflow-y: auto; display: flex; flex-direction: column; gap: 12px; }
  /* Module Configs editor fills the window (user request): with a file
     open, the page stops scrolling and the textarea takes all free height
     -- the save/restart rows below it stay pinned and visible. */
  .content.fill { overflow: hidden; box-sizing: border-box; }
  .content.fill textarea { flex: 1; min-height: 240px; resize: none; }
  .bar { display: flex; justify-content: space-between; align-items: center; }
  /* In-page tab bar (was four sidebar entries). */
  .subtabs { display: flex; gap: 4px; flex-wrap: wrap; }
  .subtabs button {
    padding: 6px 14px;
    background: #161b22;
    border: 1px solid #30363d;
    border-radius: 6px;
    color: #8b949e;
    font-size: 13.5px;
    cursor: pointer;
  }
  .subtabs button:hover { border-color: #58a6ff; color: #c9d1d9; }
  .subtabs button.active { background: #1f2937; border-color: #58a6ff; color: #f0f6fc; }
  h3 { margin: 10px 0 0; font-size: 15px; color: #58a6ff; }
  .setting { display: flex; justify-content: space-between; align-items: center; gap: 16px; background: #0d1117; border: 1px solid #30363d; border-radius: 8px; padding: 10px 14px; }
  .setting.dirty { border-color: #d29922; }
  .card { background: #0d1117; border: 1px solid #30363d; border-radius: 8px; padding: 12px 16px; display: flex; flex-direction: column; gap: 6px; }
  .testing-card { margin-top: 6px; }
  .meta { display: flex; flex-direction: column; gap: 2px; }
  .charwrap { display: flex; gap: 8px; align-items: center; }
  input, select, textarea { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 8px; }
  textarea { font-family: Consolas, monospace; font-size: 13px; width: 100%; box-sizing: border-box; }
  .row { display: flex; gap: 10px; align-items: center; }
  button { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 14px; cursor: pointer; }
  button.primary { background: #238636; border-color: #2ea043; color: white; }
  button.danger { border-color: #f85149; color: #f85149; }
  button:disabled { opacity: 0.5; cursor: default; }
  .muted { color: #8b949e; font-size: 13px; }
  .error-card { background: #161b22; border: 1px solid #f85149; border-radius: 8px; padding: 12px 16px; }
  .warn-card { background: #161b22; border: 1px solid #d29922; border-radius: 8px; padding: 12px 16px; }
  .live-card { background: #161b22; border: 1px solid #2ea043; border-radius: 8px; padding: 12px 16px; }
  .danger-card { border-color: #f85149; }
  .pb-list { display: flex; flex-direction: column; gap: 4px; max-height: 420px; overflow-y: auto; }
  .pbrow { display: flex; justify-content: space-between; align-items: center; gap: 12px; padding: 3px 6px; border-radius: 6px; }
  .pbrow.dirty { background: #1c1a10; outline: 1px solid #d29922; }
  .pbkey { font-family: Consolas, monospace; font-size: 12.5px; color: #c9d1d9; overflow-wrap: anywhere; }
  .pbval { width: 220px; flex-shrink: 0; font-family: Consolas, monospace; font-size: 12.5px; }
  .ah-steps { margin: 0; padding-left: 20px; color: #c9d1d9; font-size: 13.5px; display: flex; flex-direction: column; gap: 4px; }
  .aw-hint { color: #d29922; }
  .warn-card .row { margin-top: 8px; }
</style>
