<script lang="ts">
  import { onMount } from "svelte";
  import {
    wowConfigSet,
    wowConfigPbKeys,
    wowConsoleSend,
    gamesRestart,
    wowBotsFlush,
    wowAhbotRepair,
    wowModuleList,
    wowAccountwideGet,
    wowAccountwideSet,
    launcherConfigRead,
    launcherConfigWrite,
    autostartGet,
    autostartSet,
    type ConfigSetting,
    type PbKey,
    type AccountwideState,
    type AwSubsystem,
    type LauncherSettings,
  } from "$lib/api";
  import { configSettingsCache } from "$lib/page-cache.svelte";
  import { filterPbKeys, stagedPbChanges } from "$lib/pb-keys";
  import { dirtyKeys, requiredSaveFlags, settingsInGroups, clearSavedEdits } from "$lib/config-diff";
  import { applyEvent } from "$lib/terminal-state";
  import { restartState, noteApplyNeeded, clearApplyNeeded } from "$lib/restart-state.svelte";
  import { bannerText, normalizeApplyNeeded } from "$lib/apply-needed";
  import Terminal from "$lib/Terminal.svelte";
  import { termBuf, beginRun, clearBuf } from "$lib/term-store.svelte";
  import CharPicker from "$lib/CharPicker.svelte";
  import { featureLocked, LOCKED_HINT, testingModeOn, setTestingMode } from "$lib/features.svelte";
  import { taskbarBusy, taskbarIdle } from "$lib/taskbar";

  const WOW_ID = "wow-server-playerbots";

  // The view is now driven by the sidebar (this page has no in-page tab bar).
  // `view` comes from the router as a plain string (the nav page id); `tab`
  // narrows it to a known ConfigTab so all the existing tab logic + lazy-load
  // $effects are unchanged. Switching sidebar items keeps this component
  // mounted (one router {#if}), so edits/lazy-loads persist across views.
  // (The old moduletuning/files views moved to the tabbed Modules page --
  // see ModuleTuning.svelte / ModuleFiles.svelte.)
  type ConfigTab = "settings" | "botworld" | "ahbot" | "accountwide";
  const CONFIG_TABS: ConfigTab[] = ["settings", "botworld", "ahbot", "accountwide"];
  const TAB_LABELS: Record<ConfigTab, string> = {
    settings: "Settings",
    botworld: "Bot World",
    ahbot: "Auction House",
    accountwide: "Account-wide",
  };
  let { view = "settings" }: { view?: string } = $props();
  let tab = $derived<ConfigTab>(
    (CONFIG_TABS as string[]).includes(view) ? (view as ConfigTab) : "settings",
  );
  let tabLabel = $derived(TAB_LABELS[tab]);
  // Backed by the shared module-level cache (page-cache.svelte): re-opening
  // this page renders the last-loaded rows INSTANTLY, while load() refreshes
  // in the background. In native mode the cache's refresh routes through the
  // fast in-process Rust read (wowConfigRead); in WSL mode it stays on the
  // CLI (wowConfigList) — identical to today.
  const settings = $derived<ConfigSetting[]>(configSettingsCache.store.data ?? []);
  let edits: Record<string, string> = $state({});
  let error: string | null = $state(null);
  let saving = $state(false);

  // --- Launcher's own settings (~/.dml/launcher.json) ---------------------
  // NOT part of the AC config registry above: these are read by Rust at
  // startup, before any window exists, so they cannot live in localStorage
  // like the launcher's other preferences.
  let launcher: LauncherSettings | null = $state(null);
  let launcherSaving = $state(false);
  let launcherNote: string | null = $state(null);

  let autostartOn = $state(false);

  async function loadLauncherSettings(): Promise<void> {
    try {
      launcher = await launcherConfigRead();
    } catch {
      // A missing or broken launcher.json must not break the Settings page.
      launcher = null;
      return;
    }
    // Separate try: a failing autostart probe must NOT null `launcher` and
    // hide the whole card. The $effect only re-runs while launcher === null,
    // so that would remove the card for the rest of the session.
    try {
      autostartOn = await autostartGet();
    } catch {
      autostartOn = false;
    }
  }

  async function saveLauncherFlag(key: "closeToTray", on: boolean): Promise<void> {
    if (!launcher) return;
    launcherSaving = true;
    try {
      const next = { ...launcher.config, [key]: on };
      await launcherConfigWrite(next);
      launcher = { ...launcher, config: next };
    } catch (e) {
      const err = e as { message?: string };
      error = err.message ?? "Could not save launcher settings";
    } finally {
      launcherSaving = false;
    }
  }

  async function setAutostart(on: boolean): Promise<void> {
    launcherSaving = true;
    try {
      await autostartSet(on);
    } catch (e) {
      const err = e as { message?: string };
      error = err.message ?? "Could not change the Windows startup setting";
    }
    // Re-read either way: the registry is the source of truth, so a failed
    // write must not leave the checkbox showing what the user clicked. NOT in
    // a `finally` that can itself throw — that would skip the reset below and
    // leave all three launcher controls disabled for the session.
    try {
      autostartOn = await autostartGet();
    } catch {
      // keep the last known value
    }
    launcherSaving = false;
  }

  async function saveLauncherBackend(choice: string): Promise<void> {
    if (!launcher) return;
    launcherSaving = true;
    try {
      const next = { ...launcher.config, backend: choice };
      await launcherConfigWrite(next);
      launcher = { ...launcher, config: next };
      // AppState builds its runner once at startup from selected(), so this
      // genuinely cannot take effect until the next launch. Say so rather
      // than implying a live switch.
      launcherNote = "Saved. Restart the launcher to switch backend.";
    } catch (e) {
      const err = e as { message?: string };
      error = err.message ?? "Could not save launcher settings";
    } finally {
      launcherSaving = false;
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
  $effect(() => {
    if (tab === "settings" && launcher === null) void loadLauncherSettings();
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
    taskbarBusy();
    try {
      await wowBotsFlush((e) => {
        buf.term = applyEvent(buf.term, e);
        // The flush restarts the server twice, so any pending "restart to
        // apply" banner is now stale -- its changes were applied by those
        // restarts. Clear it on the successful terminal event (a failure
        // arrives as an "error" event instead, which leaves the banner).
        const ev = e as { event?: string };
        if (ev.event === "done") clearApplyNeeded();
      });
    } catch (e) {
      const err = e as { code?: string; message?: string; hint?: string };
      buf.term = applyEvent(buf.term, {
        event: "error",
        error: { code: err.code ?? "IPC", message: err.message ?? String(e), hint: err.hint ?? "" },
      });
    } finally {
      taskbarIdle();
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
    taskbarBusy();
    try {
      await wowAhbotRepair(ahRepairChar, (e) => {
        buf.term = applyEvent(buf.term, e);
        const ev = e as {
          event?: string;
          data?: { restart_required?: boolean; apply_needed?: string };
        };
        if (ev.event === "done" && ev.data?.restart_required) {
          noteApplyNeeded(normalizeApplyNeeded(ev.data));
        }
      });
    } catch (e) {
      const err = e as { code?: string; message?: string; hint?: string };
      buf.term = applyEvent(buf.term, {
        event: "error",
        error: { code: err.code ?? "IPC", message: err.message ?? String(e), hint: err.hint ?? "" },
      });
    } finally {
      taskbarIdle();
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

  async function savePbChanges() {
    pbSaving = true;
    error = null;
    try {
      for (const c of pbStaged) {
        const r = await wowConfigSet(`conf:playerbots.conf:${c.key}`, c.value);
        noteApplyNeeded(normalizeApplyNeeded(r));
      }
      await loadPbKeys();
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    } finally {
      pbSaving = false;
    }
  }

  // Improvements Batch 3 F1: surface the default + valid range each curated
  // row already carries (ConfigSetting.default/min/max) and give every row a
  // one-click Reset. Mirrors the Bot World tab, which already tooltips each
  // key's default.
  function defaultLabel(s: ConfigSetting): string {
    if (s.type === "bool") return s.default === "1" ? "on" : "off";
    return s.default;
  }
  function atDefault(s: ConfigSetting): boolean {
    return (edits[s.key] ?? s.value) === s.default;
  }
  function resetSetting(s: ConfigSetting): void {
    edits[s.key] = s.default;
    confirmingRestart = false;
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
  // Conf-file rows (Batch 1) are a new save mechanism gated behind their own
  // flags -- the Save button locks when ANY dirty row's flag is still locked.
  const saveLocked = $derived(requiredSaveFlags(visibleSettings, dirty).some((f) => featureLocked(f)));
  let liveNote = $state(false);

  // `saved` (passed only by a per-tab Save) keeps the OTHER shared-map tabs'
  // pending edits and drops just the keys that were written; a plain load
  // (mount / manual) starts from a clean edits map.
  async function load(saved?: string[]) {
    error = null;
    await configSettingsCache.refresh();
    if (configSettingsCache.store.error) {
      // A failed reload keeps the previous rows (the cache retains them) and
      // leaves `edits` untouched -- same as the old try/catch, which never
      // reached the edits reset on error.
      error = configSettingsCache.store.error;
      return;
    }
    edits = saved ? clearSavedEdits(edits, saved) : {};
  }
  // First open loads fresh; a re-open renders the cached rows immediately and
  // this refreshes them in the background.
  onMount(() => void load());

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
        // apply_needed is the single source of truth: gating on restart_required
        // first would discard an envelope that says WHICH apply is needed without
        // repeating THAT one is.
        const need = normalizeApplyNeeded(r);
        if (need !== "none") {
          noteApplyNeeded(need);
          anyRestart = true;
        } else if (r.applied === "live") {
          anyLive = true;
        }
      }
      // Conf rows the running server picked up over SOAP -- show the calm
      // "applied live" note instead of the restart banner (only when NO
      // saved row still needs a restart).
      liveNote = anyLive && !anyRestart;
      // Reload live values but KEEP edits on the other shared-map tabs
      // (Settings/Bot World/Auction House share one `edits` map) -- only the
      // just-saved keys are cleared, so switching tabs mid-edit and saving
      // one no longer silently discards the other's typed input.
      await load(toSave);
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
    taskbarBusy();
    try {
      // Applying settings is a deliberate, infrequent restart -- always save
      // characters first (false = don't skip). The "faster restart" option
      // lives on Home for the routine restart button.
      // Derived from the terminal `done` event, never from the promise: a
      // streaming command resolves Ok even when the CLI exits non-zero (the
      // failure arrives as an `error` event), so clearing the banner here would
      // announce a restart that did not happen.
      await gamesRestart(WOW_ID, false, (e) => {
        buf.term = applyEvent(buf.term, e);
        if (e.event === "done") clearApplyNeeded();
      });
    } catch (e) {
      const err = e as { code?: string; message?: string; hint?: string };
      buf.term = applyEvent(buf.term, {
        event: "error",
        error: { code: err.code ?? "IPC", message: err.message ?? String(e), hint: err.hint ?? "" },
      });
    } finally {
      taskbarIdle();
      restartState.restarting = false;
    }
  }

</script>

<section class="content">
  <header class="bar">
    <h2>{tabLabel}</h2>
  </header>

  {#if error}<div class="error-card"><p>{error}</p></div>{/if}
  {#if restartState.needed}
    <div class="warn-card"><p>{bannerText(restartState.apply)}</p></div>
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
      {#if launcher}
        <div class="card">
          <h3>Launcher</h3>
          <label class="row">
            Server backend
            <select
              value={launcher.config.backend ?? "auto"}
              disabled={launcherSaving || launcher.backendSource === "env"}
              onchange={(e) => saveLauncherBackend(e.currentTarget.value)}
            >
              <option value="auto">Detect automatically</option>
              <option value="native">Docker Desktop (native)</option>
              <option value="wsl">WSL (dml-arch distro)</option>
            </select>
          </label>
          <p class="muted">
            On Docker Desktop your server keeps running when you close the launcher. On WSL it
            cannot — Windows shuts the distro down shortly after, so the launcher stops the
            server for you.
          </p>
          <p class="muted">
            Currently using <strong>{launcher.effectiveBackend}</strong>.
            {#if launcher.backendSource === "env"}
              Locked by the DML_BACKEND environment variable
              (<code>{launcher.envBackend}</code>), which overrides this setting. Clear it to
              choose here.
            {:else if launcher.backendSource === "auto"}
              Detected automatically — native is chosen when a title folder and Docker Desktop are
              both present.
            {/if}
          </p>
          <label class="row">
            <input
              type="checkbox"
              checked={launcher.config.closeToTray}
              disabled={launcherSaving}
              onchange={(e) => saveLauncherFlag("closeToTray", e.currentTarget.checked)}
            />
            Closing the window keeps DML Launcher running in the system tray
          </label>
          <label class="row">
            <input
              type="checkbox"
              checked={autostartOn}
              disabled={launcherSaving}
              onchange={(e) => setAutostart(e.currentTarget.checked)}
            />
            Start DML Launcher when Windows starts
          </label>
          {#if launcherNote}<p class="muted">{launcherNote}</p>{/if}
        </div>
      {/if}
    {/if}
    {#each visibleGroups as g (g)}
      <h3>{g}</h3>
      {#each settings.filter((s) => s.group === g) as s (s.key)}
        <div class="setting" class:dirty={dirty.includes(s.key)}>
          <div class="meta">
            <strong>{s.label}</strong>
            <span class="muted">{s.explain}</span>
            <span class="muted defaults">
              default {defaultLabel(s)}{#if s.min !== null && s.max !== null} · range {s.min}–{s.max}{/if}
              <button
                class="reset-link"
                type="button"
                onclick={() => resetSetting(s)}
                disabled={saving || restartState.restarting || atDefault(s)}
                title={`Set this back to its default (${defaultLabel(s)})`}
              >Reset</button>
            </span>
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
  .defaults { display: flex; align-items: center; gap: 8px; font-size: 12px; }
  .reset-link { background: transparent; border: 1px solid #30363d; color: #8b949e; border-radius: 4px; padding: 1px 8px; font-size: 11.5px; cursor: pointer; }
  .reset-link:hover:not(:disabled) { border-color: #58a6ff; color: #c9d1d9; }
  .reset-link:disabled { opacity: 0.4; cursor: default; }
  .charwrap { display: flex; gap: 8px; align-items: center; }
  input, select { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 8px; }
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
