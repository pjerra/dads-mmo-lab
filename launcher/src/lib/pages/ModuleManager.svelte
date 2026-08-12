<script lang="ts">
  import { onMount } from "svelte";
  import {
    wowModuleInstall,
    wowModuleRemove,
    wowModuleRebuild,
    wowModuleConfActivate,
    wowModuleClientPatch,
    wowModuleTracking,
    wowModuleRepair,
    wowModuleFixit,
    wowModulePlaceNpc,
    type PlaceNpcKey,
    wowClientPathGet,
    wowClientPathSet,
    wowClientPathDetect,
    wowDockerUsage,
    wowDockerClean,
    wowUpdateCheck,
    wowServerUpdate,
    wowModuleUpdate,
    type ModuleList,
    type CppModule,
    type LuaModule,
    type SqlModule,
    type ClientPath,
    type TermEvent,
    type ModuleTracking,
    type ModuleRepair,
    type RepairResult,
    type UpdateCheck,
  } from "$lib/api";
  import { applyEvent } from "$lib/terminal-state";
  import Terminal from "$lib/Terminal.svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";

  // Opens the module's project page in the system browser (Round R). The
  // opener plugin + its default capability were already granted for this
  // app; url is registry-sourced (https-only by the CLI's own validator),
  // never user input.
  function openModUrl(url: string | null) {
    if (!url) return;
    openUrl(url).catch(() => {
      // Best-effort -- a failed browser launch shouldn't break the page.
    });
  }
  import { termBuf, beginRun, clearBuf } from "$lib/term-store.svelte";
  import { featureLocked, LOCKED_HINT } from "$lib/features.svelte";
  import { aleInstallOffer } from "$lib/ale-gate";
  import { taskbarBusy, taskbarIdle } from "$lib/taskbar";
  import { moduleBusy } from "$lib/module-busy.svelte";
  import { moduleUpdates, runUpdateCheck, checkBadge, versionLabel } from "$lib/module-updates.svelte";
  import {
    MODULE_TABS,
    MODULE_TAB_LABELS,
    type ModuleTab,
    canOfferUpdate,
    repoAfterUpdate,
    updateDoneNote,
    updatesWithServer,
  } from "$lib/module-tabs";
  import ModuleTuning from "$lib/ModuleTuning.svelte";
  import ModuleFiles from "$lib/ModuleFiles.svelte";
  import { moduleListCache } from "$lib/page-cache.svelte";
  import { canBuild } from "$lib/module-canbuild";
  import { defaultExpanded, luaStatus, luaStatusLabel, splitInstalled } from "$lib/module-split";
  import { requestConfFile, requestTuning } from "$lib/module-nav.svelte";
  import { setupDoneKey, setupFor, type SetupAction } from "$lib/setup-catalog";
  import { buildModuleRow, type ModuleRow, type RowActionId, type RowChip } from "$lib/module-row";
  import { confActivationChip, outcomeFromErrorCode } from "$lib/conf-activation-chip";
  import type { Snippet } from "svelte";

  // In-page tab strip (module-update round): the old ModuleManager content is
  // the Modules tab; the Tuning/Config files tabs host the views extracted
  // from the Config page. All three stay MOUNTED and switch with display:none
  // so staged edits survive tab switches -- the same state-survival behavior
  // the Config accordion gave those views.
  let tab: ModuleTab = $state("modules");

  // Backed by the shared module-list cache (page-cache.svelte) so re-opening
  // the Modules page renders the last-loaded modules INSTANTLY while refresh()
  // updates them in the background. The Tuning tab's server-module cards read
  // the same cache, so an install/remove here is reflected there too.
  const list = $derived<ModuleList | null>(moduleListCache.store.data);
  let error: string | null = $state(null);
  let note: string | null = $state(null);
  // Single flag: disables every Install/Update/Remove/Rebuild/Activate/Save/
  // Detect button. Backed by the SHARED moduleBusy store (not component
  // state) so a mutation started on the co-mounted Tuning tab also disables
  // this tab's buttons, and vice versa -- the tabs would otherwise race two
  // CLI mutations on the same server checkout.
  const busy = $derived(moduleBusy.busy);

  // Installed/Available split (Modules-page round, Task 3): each family
  // card's "Available" catalog collapses once anything in that family is
  // installed, expands on a fresh server with nothing installed yet. The
  // default is computed ONCE, the first time the list loads -- a $derived
  // would recompute (and silently re-collapse an Available section the user
  // just expanded) on every later refresh(), which install/remove trigger
  // constantly.
  let cppExpanded = $state(false);
  let luaExpanded = $state(false);
  let sqlExpanded = $state(false);
  let expansionInited = false;
  $effect(() => {
    if (expansionInited || !list) return;
    expansionInited = true;
    cppExpanded = defaultExpanded(list.families.cpp.filter((m) => m.installed).length);
    luaExpanded = defaultExpanded(list.families.lua.filter((m) => m.cloned || m.deployed).length);
    sqlExpanded = defaultExpanded(list.families.sql.filter((m) => m.installed).length);
  });

  let confirmingRebuild = $state(false);
  let backupChecked = $state(true); // "Back up the server first" defaults ON
  let confirmingRemove: string | null = $state(null); // key of the cpp module armed for removal
  let customUrl = $state("");

  // Repair panel (installed cpp rows): key of the module whose panel is open
  // (one at a time, like confirmingRemove above), its fetched tracking
  // diagnosis, the db/mode picks, the two-step apply confirm, and the last
  // apply's per-file results. repairError is the panel's own inline-error
  // surface -- separate from the page-level `error` so a failed tracking
  // fetch/apply doesn't blow away an unrelated page error (and vice versa).
  let repairOpen: string | null = $state(null);
  let tracking: ModuleTracking | null = $state(null);
  let repairError: string | null = $state(null);
  let repairDb: "world" | "characters" | "auth" = $state("world");
  let repairMode: "mark" | "clear" = $state("mark");
  let confirmingRepair = $state(false);
  let repairResult: ModuleRepair | null = $state(null);
  const DB_ORDER: Array<"world" | "characters" | "auth"> = ["world", "characters", "auth"];

  // Lua (ALE) card: per-row "back up first" checkbox (has_sql rows only,
  // default ON) and the key armed for a two-step remove confirm.
  let luaBackup: Record<string, boolean> = $state({});
  let confirmingLuaRemove: string | null = $state(null);

  // SQL mods card: per-row backup checkbox (default ON, every row), the key
  // armed for a remove confirm, and the two variant-typed rows' inputs.
  let sqlBackup: Record<string, boolean> = $state({});
  let confirmingSqlRemove: string | null = $state(null);
  let hearthstoneVariant = $state("5min");
  let npcTeleporterLevel = $state(80);

  // Client folder card.
  let clientPath: ClientPath | null = $state(null);
  let clientPathInput = $state("");
  let clientError: string | null = $state(null);
  let clientCandidates: string[] | null = $state(null);

  // Server update card: the last `Check for updates` result (fetched only on
  // explicit button click -- it does a `git fetch` per repo, unlike the
  // other cards' data, so it does NOT ride along with every refresh()), its
  // own inline-error surface (same separation pattern as
  // clientError/repairError/dockerUsageError), the backup checkbox (default
  // ON), and the two-step confirm flag.
  let updateCheck: UpdateCheck | null = $state(null);
  let updateCheckError: string | null = $state(null);
  let updateBackup = $state(true);
  let confirmingUpdate = $state(false);

  // Disk cleanup card: usage lines fetched alongside every refresh() (own
  // inline-error surface, separate from the page-level `error`, same
  // separation pattern as clientError/repairError), the level select
  // (default 1), and the two-step confirm flag.
  let dockerUsage: string[] | null = $state(null);
  let dockerUsageError: string | null = $state(null);
  let cleanLevel = $state(1);
  let confirmingClean = $state(false);

  const buf = termBuf("modules");

  function showErr(e: unknown) {
    const err = e as { message?: string; hint?: string };
    error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
  }

  // New keys default their backup checkbox ON without stomping a value the
  // user already toggled (called after every list refresh).
  function ensureBackupDefaults() {
    if (!list) return;
    for (const m of list.families.lua) {
      if (m.has_sql && !(m.key in luaBackup)) luaBackup[m.key] = true;
    }
    for (const m of list.families.sql) {
      if (!(m.key in sqlBackup)) sqlBackup[m.key] = true;
    }
  }

  // Batch 3 F13b: place the missing Battle Pass vendor NPC (entry 90100).
  // Idempotent CLI-side; the NPC only appears after a world restart, which
  // the note passes on.
  async function fixitBattlepassNpc() {
    moduleBusy.busy = true; error = null; note = null;
    try {
      const r = await wowModuleFixit("battlepass-npc");
      note = r.already_placed
        ? "The Battle Pass NPC is already placed in the world."
        : "Battle Pass NPC placed in Stormwind + Orgrimmar. Restart the world server (Home) for it to appear, then talk to the Battle Pass Vendor.";
    } catch (e) {
      showErr(e);
    } finally {
      moduleBusy.busy = false;
    }
  }

  // Batch 2 (overnight): NPC-mods that ship a ready-made capital coord block
  // in the CLI's cheat-sheet (mirrors the CLI/Rust allowlist). Each gets a
  // "Place NPC in capitals" button once installed. Battlepass is NOT here --
  // its NPC needs a creature_template created too, handled by its own fixit.
  const PLACE_NPC_KEYS = new Set<string>([
    "mod-1v1-arena",
    "mod-transmog",
    "mod-npc-beastmaster",
    "bmah",
  ]);

  // Spawns an installed NPC-mod's creature in Stormwind + Orgrimmar. Idempotent
  // per map CLI-side; the NPC only appears after a world restart (the note
  // passes that on). Non-streamed, same shape as fixitBattlepassNpc above.
  async function placeNpc(key: string, label: string) {
    moduleBusy.busy = true; error = null; note = null;
    try {
      const r = await wowModulePlaceNpc(key as PlaceNpcKey);
      note = r.already_placed
        ? `${label} is already placed in Stormwind + Orgrimmar.`
        : `Placed ${label} in ${r.spawns_placed} capital(s). Restart the world server (Home) for it to appear.`;
    } catch (e) {
      showErr(e);
    } finally {
      moduleBusy.busy = false;
    }
  }

  // Needs-setup notices (Modules-page round, Task 5): installed rows whose
  // key has a setup-catalog entry get an amber "Needs setup" chip + a panel
  // (summary, steps, guided actions, "Mark as done"). Dismissal is stored in
  // localStorage (recorded deviation #2 in the plan header, not
  // launcher.json) keyed by server dir + module key -- SETUP_SERVER is a
  // literal placeholder because nothing on this page currently exposes the
  // real server identity; a later round can thread the real one through
  // without changing setupDoneKey's signature.
  const SETUP_SERVER = "native";
  let setupOpen: string | null = $state(null);
  let setupCopied: string | null = $state(null);
  // localStorage reads inside needsSetup() aren't tracked by Svelte's
  // reactivity -- bumping this on every dismissal forces `needsSetup` (and
  // anything that calls it in a template) to re-evaluate.
  let setupDismissals = $state(0);

  function needsSetup(key: string): boolean {
    setupDismissals;
    if (!setupFor(key)) return false;
    try {
      return !localStorage.getItem(setupDoneKey(SETUP_SERVER, key));
    } catch {
      // Storage can be unavailable (e.g. locked down webview) -- fail open
      // so the notice still shows rather than silently vanishing.
      return true;
    }
  }

  function toggleSetup(key: string) {
    setupOpen = setupOpen === key ? null : key;
    setupCopied = null;
  }

  // Conf-activation failure chips (round 2, Task 2): the auto-catchup below
  // no longer swallows a failed activation -- it turns into a clickable
  // "conf not activated" chip on that module's row (module-row.ts's
  // confFailChip ctx slot), keyed by module key. confFailReason holds the
  // CmdError's message + hint for the small panel the chip opens; both are
  // cleared for a key that activates successfully on a later pass.
  let confFailChips: Record<string, RowChip> = $state({});
  let confFailReason: Record<string, { message: string; hint?: string }> = $state({});
  let confFailOpen: string | null = $state(null);

  function toggleConfFail(key: string) {
    confFailOpen = confFailOpen === key ? null : key;
  }

  // Chip-as-action dispatch (round 2, Task 2): shared between cpp/lua rows --
  // the chip IS the click target, wired through RowUi.onChip.
  function onChipClick(chip: RowChip, key: string) {
    if (chip.id === "setup") toggleSetup(key);
    else if (chip.id === "conf-activation") toggleConfFail(key);
  }

  function markSetupDone(key: string) {
    try {
      localStorage.setItem(setupDoneKey(SETUP_SERVER, key), "1");
    } catch {
      // Best-effort -- the chip just won't stay dismissed across reloads.
    }
    setupDismissals++;
    setupOpen = null;
  }

  async function copySetupCommand(command: string) {
    try {
      await navigator.clipboard.writeText(command);
      setupCopied = command;
      setTimeout(() => {
        if (setupCopied === command) setupCopied = null;
      }, 2000);
    } catch {
      // Clipboard access can be denied -- the action just won't flash "copied".
    }
  }

  // Dispatches a setup action to whichever existing machinery it wraps --
  // no new mutation surfaces (plan Task 5 interface).
  function runSetupAction(a: SetupAction) {
    switch (a.type) {
      case "open-tuner":
        requestTuning(a.key);
        tab = "tuning";
        break;
      case "open-files":
        requestConfFile(a.file);
        tab = "files";
        break;
      case "place-npc":
        void placeNpc(a.key, a.label);
        break;
      case "fixit":
        void fixitBattlepassNpc();
        break;
      case "copy-command":
        void copySetupCommand(a.command);
        break;
    }
  }

  async function refresh() {
    error = null; confirmingRebuild = false; confirmingRemove = null;
    confirmingLuaRemove = null; confirmingSqlRemove = null;
    repairOpen = null; confirmingRepair = false;
    confirmingClean = false; confirmingUpdate = false;
    await moduleListCache.refresh();
    if (moduleListCache.store.error) error = moduleListCache.store.error;
    else ensureBackupDefaults();
    try { clientPath = await wowClientPathGet(); } catch (e) { showErr(e); }
    try { dockerUsage = (await wowDockerUsage()).lines; dockerUsageError = null; } catch (e) { dockerUsageErr(e); }
  }

  // Auto-conf catch-up (Modules-page round, Task 4): a server whose modules
  // were installed by an older launcher/CLI (before install auto-activated a
  // module's conf -- Task 1) can still have installed cpp modules sitting at
  // conf === "ready". One pass, once per page mount, activates every such
  // conf so the tuning cards aren't hidden behind a manual "Activate conf"
  // click the user has no reason to know about. Silent on success (round 2,
  // Task 2) -- activation still happens and still logs to the console
  // stream, but no page note. The only surfaced case is failure: a raced
  // EXISTS/NEEDS_REBUILD (or a genuine error) now shows a "conf not
  // activated" chip on that module's row via confActivationChip(), instead
  // of vanishing into a swallowed catch{} the way round 1 left it.
  let catchupDone = false;
  async function autoConfCatchup() {
    if (catchupDone || busy) return;
    catchupDone = true;
    const candidates = (list?.families.cpp ?? []).filter((m) => m.installed && m.conf === "ready");
    if (candidates.length === 0) return;
    let anyActivated = false;
    for (const m of candidates) {
      try {
        const r = await wowModuleConfActivate(m.key);
        if (r.activated) anyActivated = true;
        delete confFailChips[m.key];
        delete confFailReason[m.key];
      } catch (e) {
        const err = e as { code?: string; message?: string; hint?: string };
        const chip = confActivationChip(outcomeFromErrorCode(err.code));
        if (chip) {
          confFailChips[m.key] = chip;
          confFailReason[m.key] = { message: err.message ?? "Conf activation failed.", hint: err.hint };
        }
      }
    }
    if (anyActivated) await refresh();
  }
  onMount(() => {
    void (async () => {
      await refresh();
      await autoConfCatchup();
    })();
  });

  // Streamed operations (install/remove/rebuild) use the sawDone/streamErr
  // contract: the outcome is derived from events captured inside the event
  // callback, then applied AFTER the trailing refresh() -- never from the
  // promise resolving, since the streaming promise resolves even when the
  // underlying CLI step failed.
  async function runStream(
    run: (onEvent: (e: TermEvent) => void) => Promise<void>,
    onDone: (doneData: unknown) => void,
  ) {
    moduleBusy.busy = true; error = null; note = null; beginRun("modules");
    taskbarBusy();
    let sawDone = false;
    let doneData: unknown;
    let streamErr: { message?: string; hint?: string } | null = null;
    let outcomeErr: unknown = null;
    try {
      await run((e) => {
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
      await refresh();
      if (outcomeErr) showErr(outcomeErr);
      else if (streamErr) showErr(streamErr);
      else if (sawDone) onDone(doneData);
    }
  }

  function install(key: string | null, url: string | null, label: string) {
    return runStream(
      (onEvent) => wowModuleInstall("cpp", key, url, onEvent),
      () => {
        note = `Installed ${label}.`;
        if (url) customUrl = "";
      },
    );
  }

  // Batch 5 F2: ARAC's server-DBC + client-MPQ step, streamed into the same
  // terminal as install/remove/rebuild. The done payload's client_patched
  // tells us whether the MPQ landed (false = no client folder saved yet).
  function applyClientPatch(m: CppModule) {
    return runStream(
      (onEvent) => wowModuleClientPatch(m.key, onEvent),
      (doneData) => {
        const d = doneData as { client_patched?: boolean } | undefined;
        note = d?.client_patched
          ? "ARAC patch applied (server DBCs + client Patch-A.MPQ). Restart the server to load it."
          : "ARAC server DBCs installed — set your WoW client folder below, then Apply client patch again for Patch-A.MPQ. Restart the server to load it.";
      },
    );
  }

  function removeModule(m: CppModule) {
    if (confirmingRemove !== m.key) {
      confirmingRemove = m.key;
      return;
    }
    confirmingRemove = null;
    return runStream(
      (onEvent) => wowModuleRemove("cpp", m.key, onEvent),
      () => { note = `Removed ${m.name}.`; },
    );
  }

  function rebuild() {
    if (!confirmingRebuild) {
      confirmingRebuild = true;
      return;
    }
    confirmingRebuild = false;
    return runStream(
      (onEvent) => wowModuleRebuild(backupChecked, onEvent),
      () => { note = "Rebuild complete."; },
    );
  }

  function showUpdateCheckErr(e: unknown) {
    const err = e as { message?: string; hint?: string };
    updateCheckError = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
  }

  async function checkUpdates() {
    moduleBusy.busy = true; updateCheckError = null;
    try {
      updateCheck = await wowUpdateCheck();
    } catch (e) {
      showUpdateCheckErr(e);
    } finally {
      moduleBusy.busy = false;
    }
  }

  function serverUpdate() {
    if (!confirmingUpdate) {
      confirmingUpdate = true;
      return;
    }
    confirmingUpdate = false;
    return runStream(
      (onEvent) => wowServerUpdate(updateBackup, onEvent),
      (doneData) => {
        const d = doneData as { changed?: boolean } | undefined;
        note = d?.changed
          ? "Update pulled — rebuild required (see the banner above)."
          : "Already up to date.";
      },
    );
  }

  // Per-module source update (module-update round): streams the CLI's
  // stash-safe pull through the same runStream contract as install/rebuild.
  // Offered only when the header's Check for updates reported the module a
  // known number of commits behind; mod-playerbots never gets the button
  // (it updates with the server core -- see module-tabs.ts).
  function updateModule(m: CppModule) {
    return runStream(
      (onEvent) => wowModuleUpdate(m.key, onEvent),
      (doneData) => {
        const d = doneData as
          | { key?: string; changed?: boolean; after?: string; pending_rebuild?: boolean }
          | undefined;
        note = updateDoneNote(d);
        // The clone now sits at origin's head -- turn the cached amber chip
        // off (both tabs) without re-running the fetch-everything check.
        if (d?.after && moduleUpdates.repos[m.key]) {
          moduleUpdates.repos[m.key] = repoAfterUpdate(moduleUpdates.repos[m.key], d.after);
        }
      },
    );
  }

  function dockerUsageErr(e: unknown) {
    const err = e as { message?: string; hint?: string };
    dockerUsageError = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
  }

  function clean() {
    if (!confirmingClean) {
      confirmingClean = true;
      return;
    }
    confirmingClean = false;
    return runStream(
      (onEvent) => wowDockerClean(cleanLevel, onEvent),
      (doneData) => {
        const d = doneData as { level?: number } | undefined;
        note = `Docker cleanup complete (level ${d?.level ?? cleanLevel}).`;
      },
    );
  }

  async function activateConf(key: string) {
    moduleBusy.busy = true; error = null; note = null;
    try {
      const r = await wowModuleConfActivate(key);
      note = `Activated ${r.conf_name}.`;
      await refresh();
    } catch (e) { showErr(e); } finally { moduleBusy.busy = false; }
  }

  function installLua(m: LuaModule) {
    const backup = m.has_sql ? luaBackup[m.key] : undefined;
    return runStream(
      (onEvent) => wowModuleInstall("lua", m.key, null, onEvent, backup),
      (doneData) => {
        const d = doneData as { reload?: string } | undefined;
        note = d?.reload ?? `Installed ${m.name}.`;
      },
    );
  }

  function removeLua(m: LuaModule) {
    if (confirmingLuaRemove !== m.key) {
      confirmingLuaRemove = m.key;
      return;
    }
    confirmingLuaRemove = null;
    return runStream(
      (onEvent) => wowModuleRemove("lua", m.key, onEvent),
      () => { note = `Removed ${m.name}.`; },
    );
  }

  function installSql(m: SqlModule) {
    let variant: string | undefined;
    if (m.key === "hearthstone-cd") variant = hearthstoneVariant;
    else if (m.key === "npc-teleporter") variant = String(npcTeleporterLevel);
    return runStream(
      (onEvent) => wowModuleInstall("sql", m.key, null, onEvent, sqlBackup[m.key], variant),
      () => { note = `Installed ${m.name}.`; },
    );
  }

  function removeSql(m: SqlModule) {
    if (confirmingSqlRemove !== m.key) {
      confirmingSqlRemove = m.key;
      return;
    }
    confirmingSqlRemove = null;
    return runStream(
      (onEvent) => wowModuleRemove("sql", m.key, onEvent, sqlBackup[m.key]),
      () => { note = `Removed ${m.name}.`; },
    );
  }

  function clientErr(e: unknown) {
    const err = e as { message?: string; hint?: string };
    clientError = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
  }

  async function saveClientPath() {
    moduleBusy.busy = true; clientError = null; note = null;
    try {
      clientPath = await wowClientPathSet(clientPathInput);
      note = `Client folder set — ${clientPath.path}.`;
      clientPathInput = "";
      clientCandidates = null;
    } catch (e) { clientErr(e); } finally { moduleBusy.busy = false; }
  }

  async function detectClientPath() {
    moduleBusy.busy = true; clientError = null;
    try {
      const r = await wowClientPathDetect();
      clientCandidates = r.candidates;
    } catch (e) { clientErr(e); } finally { moduleBusy.busy = false; }
  }

  async function pickClientCandidate(path: string) {
    moduleBusy.busy = true; clientError = null; note = null;
    try {
      clientPath = await wowClientPathSet(path);
      note = `Client folder set — ${clientPath.path}.`;
      clientCandidates = null;
    } catch (e) { clientErr(e); } finally { moduleBusy.busy = false; }
  }

  // Copy pinned by the brief verbatim -- mod-arac's data is not reverted by
  // a clone removal (SQL rows / DBC files / client patch all stay behind).
  function removeConfirmText(m: CppModule): string {
    if (m.key === "mod-arac") {
      return "mod-arac is data-only — removing does not revert its SQL, DBC or client patch";
    }
    return `Remove ${m.name} — sure?`;
  }

  function statusText(m: CppModule): string {
    if (!m.installed) return "Not installed";
    if (m.pending_rebuild) return "Installed — rebuild pending";
    return "Installed";
  }
  function statusClass(m: CppModule): "on" | "warn" | "off" {
    if (!m.installed) return "off";
    if (m.pending_rebuild) return "warn";
    return "on";
  }

  // Shared row skeleton (Modules-page round 2, Task 1): every family's rows
  // render through ONE snippet fed by buildModuleRow (module-row.ts). RowUi
  // carries the per-family presentation the pure builder doesn't know about
  // -- status badge, link targets, and the action dispatch/disable/label
  // hooks -- so the row MARKUP lives in exactly one place.
  type RowUi = {
    url: string | null;
    desc?: string;
    ver?: string | null;
    statusCls: "on" | "warn" | "off";
    statusText: string;
    dim?: boolean;
    // Installed rows: the module name is a button opening its tuning card.
    onName?: () => void;
    // Round 2, Task 2: the chip IS the click target -- setup/conf-failed
    // chips dispatch here instead of a separate text-link row.
    onChip?: (chip: RowChip) => void;
    onAction: (id: RowActionId) => void;
    actionDisabled?: (id: RowActionId) => boolean;
    actionTitle?: (id: RowActionId) => string | undefined;
    // Two-step remove confirms override the builder's static label.
    actionLabel?: (id: RowActionId) => string | undefined;
  };

  function openTuning(key: string) {
    requestTuning(key);
    tab = "tuning";
  }

  function cppAction(id: RowActionId, m: CppModule) {
    if (id === "install") void install(m.key, null, m.name);
    else if (id === "tune") openTuning(m.key);
    else if (id === "repair") void toggleRepair(m);
    else if (id === "remove") void removeModule(m);
  }
  function cppActionDisabled(id: RowActionId): boolean {
    return (id === "install" || id === "remove") && featureLocked("modules-cpp");
  }
  function cppActionTitle(id: RowActionId): string | undefined {
    if ((id === "install" || id === "remove") && featureLocked("modules-cpp")) return LOCKED_HINT;
    if (id === "tune") return "Open this module's tuning card";
    return undefined;
  }

  function luaAction(id: RowActionId, m: LuaModule) {
    if (id === "install") void installLua(m);
    else if (id === "tune") openTuning(m.key);
    else if (id === "remove") void removeLua(m);
  }
  function luaActionDisabled(id: RowActionId, aleReady: boolean): boolean {
    return (id === "install" || id === "remove") && (!aleReady || featureLocked("modules-lua"));
  }
  function luaActionTitle(id: RowActionId, aleReady: boolean): string | undefined {
    if (id === "install" || id === "remove") {
      if (featureLocked("modules-lua")) return LOCKED_HINT;
      if (!aleReady) return "Install the ALE module first";
    }
    if (id === "tune") return "Open this module's tuning card";
    return undefined;
  }

  function sqlAction(id: RowActionId, m: SqlModule) {
    if (id === "install") void installSql(m);
    else if (id === "remove") void removeSql(m);
  }
  function sqlActionDisabled(id: RowActionId): boolean {
    return (id === "install" || id === "remove") && featureLocked("modules-sql");
  }
  function sqlActionTitle(id: RowActionId, m: SqlModule): string | undefined {
    if (id === "remove" && m.key === "rare-drops") {
      return "No automated reversal — restore a backup instead.";
    }
    if ((id === "install" || id === "remove") && featureLocked("modules-sql")) return LOCKED_HINT;
    return undefined;
  }

  function showRepairErr(e: unknown) {
    const err = e as { message?: string; hint?: string };
    repairError = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
  }

  async function fetchTracking(key: string) {
    moduleBusy.busy = true; repairError = null;
    try {
      tracking = await wowModuleTracking(key);
    } catch (e) {
      showRepairErr(e);
    } finally {
      moduleBusy.busy = false;
    }
  }

  function toggleRepair(m: CppModule) {
    if (repairOpen === m.key) {
      repairOpen = null;
      return;
    }
    repairOpen = m.key;
    tracking = null;
    repairError = null;
    repairResult = null;
    confirmingRepair = false;
    repairDb = "world";
    repairMode = "mark";
    return fetchTracking(m.key);
  }

  function disarmRepair() {
    confirmingRepair = false;
  }

  async function applyRepair(m: CppModule) {
    if (!confirmingRepair) {
      confirmingRepair = true;
      return;
    }
    confirmingRepair = false;
    moduleBusy.busy = true; repairError = null; repairResult = null;
    try {
      repairResult = await wowModuleRepair(m.key, repairDb, repairMode);
      tracking = await wowModuleTracking(m.key);
    } catch (e) {
      showRepairErr(e);
    } finally {
      moduleBusy.busy = false;
    }
  }

  function humanizeResult(r: RepairResult["result"]): string {
    switch (r) {
      case "marked": return "marked";
      case "cleared": return "cleared";
      case "not_tracked": return "not tracked";
      case "file_missing": return "file missing";
    }
  }

  function resultClass(r: RepairResult["result"]): "on" | "warn" | "off" {
    if (r === "marked" || r === "cleared") return "on";
    if (r === "file_missing") return "warn";
    return "off";
  }
</script>

<div class="page">
  <!-- In-page tab strip -- the Dashboard character tabs' visual language
       (horizontal strip, active = bright text + accent underline). -->
  <div class="tabstrip">
    {#each MODULE_TABS as t (t)}
      <button class:active={tab === t} onclick={() => (tab = t)}>{MODULE_TAB_LABELS[t]}</button>
    {/each}
  </div>

<section class="content" style:display={tab === "modules" ? null : "none"}>
  <header class="bar">
    <h2>Modules</h2>
    <div class="row">
      <!-- Read-only check (git fetch per installed clone, no mutation) --
           deliberately never feature-locked. Results feed the per-module
           badges here AND on the Tuning tab via the shared module-updates
           store. -->
      <button onclick={() => void runUpdateCheck()} disabled={moduleUpdates.checking}>
        {#if moduleUpdates.checking}<span class="spinner"></span>Checking…{:else}Check for updates{/if}
      </button>
      <button onclick={refresh} disabled={busy}>Refresh</button>
    </div>
  </header>

  {#if error}<div class="error-card"><p>{error}</p></div>{/if}
  {#if moduleUpdates.lastError}<p class="inline-error">Update check failed: {moduleUpdates.lastError}</p>{/if}
  {#if moduleUpdates.checked && !moduleUpdates.lastError && Object.keys(moduleUpdates.repos).length === 0}
    <!-- A check that found zero git module clones is a valid answer -- say
         so instead of returning the page to its prior state unchanged. -->
    <p class="muted">Checked — no module source checkouts to compare.</p>
  {/if}
  {#if note}<p class="muted">{note}</p>{/if}

  {#if list}
    {@const buildable = canBuild(list)}
    <div class="card {list.rebuild_pending.length > 0 ? 'warn-card' : ''}">
      {#if list.rebuild_pending.length > 0}
        <p><strong>Server rebuild required for: {list.rebuild_pending.join(", ")}</strong></p>
      {:else}
        <p><strong>Server rebuild</strong> — recompiles the worldserver with the currently installed C++ modules.</p>
      {/if}
      {#if !buildable}
        <p class="muted">This server runs prebuilt images — C++ modules can't be compiled into it, so rebuild is unavailable.</p>
      {/if}
      <label class="row">
        <input type="checkbox" bind:checked={backupChecked} disabled={busy || !buildable} />
        Back up the server first (recommended)
      </label>
      <div class="row">
        {#if !confirmingRebuild}
          <button
            class="primary"
            onclick={rebuild}
            disabled={busy || !buildable || featureLocked("modules-rebuild")}
            title={!buildable
              ? "This server runs prebuilt images — rebuild is unavailable."
              : featureLocked("modules-rebuild") ? LOCKED_HINT : undefined}
          >
            {list.rebuild_pending.length > 0 ? "Rebuild now" : "Rebuild server"}
          </button>
        {:else}
          <span>Rebuild takes 30–90 minutes and stops the world while building. Continue?</span>
          <button class="primary" onclick={rebuild} disabled={busy}>Confirm</button>
          <button onclick={() => (confirmingRebuild = false)} disabled={busy}>Cancel</button>
        {/if}
      </div>
    </div>
  {/if}

  <!-- Needs-setup panel (Modules-page round, Task 5; the chip itself moved
       into buildModuleRow's chips list -- round 2, Task 2), shared between
       the cpp and lua rows below -- both families have catalog entries
       (mod-ahbot/mod-arac are cpp; battlepass/bmah/mod-1v1-arena/
       mod-npc-beastmaster/mod-transmog can be lua). -->
  {#snippet setupPanel(key: string)}
    {#if setupOpen === key}
      {@const setup = setupFor(key)}
      {#if setup}
        <div class="setup-panel">
          <p>{setup.summary}</p>
          <ol>
            {#each setup.steps as step (step)}
              <li>{step}</li>
            {/each}
          </ol>
          {#if setup.actions.length > 0}
            <div class="row">
              {#each setup.actions as action (action.label)}
                <button onclick={() => runSetupAction(action)} disabled={busy}>{action.label}</button>
              {/each}
            </div>
          {/if}
          {#if setupCopied}<p class="muted">Copied to clipboard.</p>{/if}
          <div class="row">
            <button onclick={() => markSetupDone(key)}>Mark as done</button>
          </div>
        </div>
      {/if}
    {/if}
  {/snippet}

  <!-- Conf-not-activated panel (round 2, Task 2): the chip's own click
       target -- the reason (CmdError.message) and the manual conf-activate
       hint (CmdError.hint), same visual pattern as setupPanel above. -->
  {#snippet confFailPanel(key: string)}
    {#if confFailOpen === key && confFailReason[key]}
      <div class="setup-panel">
        <p>{confFailReason[key].message}</p>
        {#if confFailReason[key].hint}<p class="muted">{confFailReason[key].hint}</p>{/if}
      </div>
    {/if}
  {/snippet}

  <!-- One row skeleton for every section (round 2, Task 1): name+status
       left, chips after the status badge, actions in a fixed right-aligned
       column (same order: tune · repair · remove / install). The family
       wrappers below feed it via buildModuleRow and supply their extra
       chips/controls through the two snippet slots -- the row MARKUP lives
       only here. -->
  {#snippet moduleRow(row: ModuleRow, ui: RowUi, chips?: Snippet, controls?: Snippet)}
    <div class="modrow" class:dim={ui.dim}>
      <div class="mhead">
        <span class="mtitle">
          {#if ui.onName}
            <button class="mname-link" onclick={ui.onName} title="Open this module's tuning card">{row.title}</button>
          {:else}
            <strong class="mname">{row.title}</strong>
          {/if}
          {#if ui.url}<button class="ghlink" onclick={() => openModUrl(ui.url)} title="Open the project page in your browser">GitHub ↗</button>{/if}
        </span>
        {#if ui.desc}<span class="mdesc">{ui.desc}</span>{/if}
        {#if ui.ver}<span class="mver">{ui.ver}</span>{/if}
      </div>
      <div class="mmid">
        <span class="badge {ui.statusCls}">{ui.statusText}</span>
        {#each row.chips as chip (chip.id)}
          {#if chip.clickable}
            <button class="mchip {chip.kind}" onclick={() => ui.onChip?.(chip)}>{chip.label}</button>
          {:else}
            <span class="mchip {chip.kind}">{chip.label}</span>
          {/if}
        {/each}
        {#if chips}{@render chips()}{/if}
      </div>
      <div class="mactions">
        {#if controls}{@render controls()}{/if}
        {#each row.actions as a (a.id)}
          <button
            class:primary={a.id === "install"}
            onclick={() => ui.onAction(a.id)}
            disabled={busy || a.disabled || ui.actionDisabled?.(a.id)}
            title={ui.actionTitle?.(a.id)}
          >
            {ui.actionLabel?.(a.id) ?? a.label}
          </button>
        {/each}
      </div>
    </div>
  {/snippet}

  {#snippet cppRow(m: CppModule)}
    {@const badge = checkBadge(moduleUpdates.checked, moduleUpdates.repos[m.key])}
    {#snippet cppChips()}
      {#if badge}<span class="badge {badge.cls}">{badge.text}</span>{/if}
    {/snippet}
    {#snippet cppControls()}
      {#if m.conf === "ready"}
        <button
          onclick={() => activateConf(m.key)}
          disabled={busy || featureLocked("modules-conf")}
          title={featureLocked("modules-conf") ? LOCKED_HINT : undefined}
        >
          Activate conf
        </button>
      {:else if m.conf === "active"}
        <span class="muted">conf active</span>
      {/if}
      {#if m.installed}
        <!-- The per-module Update replaces the old always-there reinstall
             button (module-update round): it appears only when the check
             above reported the module behind, and streams the stash-safe
             `module update` instead of install's plain pull. -->
        {#if canOfferUpdate(m.key, moduleUpdates.repos[m.key]?.behind)}
          <button
            class="primary"
            onclick={() => updateModule(m)}
            disabled={busy || featureLocked("module-update")}
            title={featureLocked("module-update")
              ? LOCKED_HINT
              : "Pull the module's latest source — a rebuild compiles it afterwards"}
          >
            Update
          </button>
        {:else if updatesWithServer(m.key)}
          <span class="muted">updates with the server — use the Server update card below</span>
        {/if}
        {#if m.key === "mod-arac"}
          <button
            onclick={() => applyClientPatch(m)}
            disabled={busy || featureLocked("arac-client-patch")}
            title={featureLocked("arac-client-patch")
              ? LOCKED_HINT
              : clientPath?.path
                ? "Copies ARAC's server DBC files into the data volume and Patch-A.MPQ into your WoW client"
                : "Copies the server DBC files now — set your WoW client folder (card below) first to also install Patch-A.MPQ"}
          >
            Apply client patch
          </button>
        {/if}
        {#if PLACE_NPC_KEYS.has(m.key)}
          <button
            onclick={() => placeNpc(m.key, m.name)}
            disabled={busy || featureLocked("place-npc")}
            title={featureLocked("place-npc")
              ? LOCKED_HINT
              : "Spawn this NPC in Stormwind + Orgrimmar (needs a world restart to appear)"}
          >
            Place NPC in capitals
          </button>
        {/if}
      {/if}
    {/snippet}
    {@render moduleRow(
      buildModuleRow(m, {
        family: "cpp",
        needsSetup: m.installed && needsSetup(m.key),
        confFailChip: confFailChips[m.key] ?? null,
      }),
      {
        url: m.url,
        desc: m.desc,
        ver: versionLabel(m.head, m.head_date),
        statusCls: statusClass(m),
        statusText: statusText(m),
        onName: m.installed ? () => openTuning(m.key) : undefined,
        onChip: (chip) => onChipClick(chip, m.key),
        onAction: (id) => cppAction(id, m),
        actionDisabled: cppActionDisabled,
        actionTitle: cppActionTitle,
        actionLabel: (id) =>
          id === "remove" && confirmingRemove === m.key ? removeConfirmText(m) : undefined,
      },
      cppChips,
      cppControls,
    )}
    {@render setupPanel(m.key)}
    {@render confFailPanel(m.key)}
    {#if repairOpen === m.key}
      <div class="repair-panel">
        {#if repairError}<p class="inline-error">{repairError}</p>{/if}
        {#if tracking}
          {#each DB_ORDER as dbName (dbName)}
            {@const dbData = tracking.dbs[dbName]}
            <div class="repair-db">
              <strong class="db-name">{dbName}</strong>
              {#if dbData.files.length === 0 && dbData.tracked_rows.length === 0}
                <p class="muted">nothing found</p>
              {:else}
                {#if dbData.files.length > 0}
                  <div class="row">
                    {#each dbData.files as f (f.name)}
                      <span class="chip {f.tracked ? 'tracked' : 'untracked'}">{f.name}</span>
                    {/each}
                  </div>
                {/if}
                {#if dbData.tracked_rows.length > 0}
                  <div class="row">
                    {#each dbData.tracked_rows as name (name)}
                      <span class="muted">{name}</span>
                    {/each}
                  </div>
                {/if}
              {/if}
            </div>
          {/each}

          <div class="row">
            <label class="row">
              DB
              <select bind:value={repairDb} onchange={disarmRepair} disabled={busy}>
                <option value="world">world</option>
                <option value="characters">characters</option>
                <option value="auth">auth</option>
              </select>
            </label>
            <label class="row">
              Mode
              <select bind:value={repairMode} onchange={disarmRepair} disabled={busy}>
                <option value="mark">Mark as applied — fixes "Table already exists" on start</option>
                <option value="clear">Clear tracking — makes the server re-apply the SQL (only safe if the SQL is re-runnable)</option>
              </select>
            </label>
          </div>
          <div class="row">
            {#if !confirmingRepair}
              <button
                class="primary"
                onclick={() => applyRepair(m)}
                disabled={busy || featureLocked("module-repair")}
                title={featureLocked("module-repair") ? LOCKED_HINT : undefined}
              >
                Apply
              </button>
            {:else}
              <span>This edits the database's update-tracking records. Continue?</span>
              <button class="primary" onclick={() => applyRepair(m)} disabled={busy}>Confirm</button>
              <button onclick={() => (confirmingRepair = false)} disabled={busy}>Cancel</button>
            {/if}
          </div>

          {#if repairResult}
            <div class="repair-results">
              {#each repairResult.results as r (r.file)}
                <div class="row">
                  <span>{r.file}</span>
                  <span class="badge {resultClass(r.result)}">{humanizeResult(r.result)}</span>
                </div>
              {/each}
              <p class="muted">Restart the server to apply.</p>
            </div>
          {/if}
        {/if}
      </div>
    {/if}
  {/snippet}

  <div class="card">
    <h3>C++ modules</h3>
    {#if list}
      {@const parts = splitInstalled(list.families.cpp, (m) => m.installed)}
      <h4>Installed ({parts.installed.length})</h4>
      {#if parts.installed.length === 0}
        <p class="muted">Nothing installed yet — browse Available below.</p>
      {/if}
      {#each parts.installed as m (m.key)}
        {@render cppRow(m)}
      {/each}
      <button class="avail-toggle" onclick={() => (cppExpanded = !cppExpanded)}>
        {cppExpanded ? "▾" : "▸"} Available ({parts.available.length})
      </button>
      {#if cppExpanded}
        {#each parts.available as m (m.key)}
          {@render cppRow(m)}
        {/each}
      {/if}
    {/if}
  </div>

  {#snippet luaRow(m: LuaModule, aleReady: boolean)}
    {@const row = buildModuleRow(m, { family: "lua", needsSetup: m.deployed && needsSetup(m.key) })}
    {#snippet luaControls()}
      {#if m.has_sql}
        <label class="row">
          <input type="checkbox" bind:checked={luaBackup[m.key]} disabled={busy} />
          Back up first (recommended)
        </label>
      {/if}
      {#if m.key === "battlepass" && m.deployed}
        <!-- Batch 3 F13b: the upstream battlepass SQL ships no vendor
             NPC -- this places entry 90100 in both capitals. -->
        <button
          onclick={fixitBattlepassNpc}
          disabled={busy || featureLocked("module-fixit")}
          title={featureLocked("module-fixit")
            ? LOCKED_HINT
            : "Place the missing Battle Pass NPC in Stormwind + Orgrimmar (needs a world restart to appear)"}
        >
          Fix missing NPC
        </button>
      {/if}
      {#if PLACE_NPC_KEYS.has(m.key) && m.deployed}
        <!-- Batch 2 (overnight): spawn the mod's NPC (e.g. BMAH
             Auctioneer) in both capitals from its coord block. -->
        <button
          onclick={() => placeNpc(m.key, m.name)}
          disabled={busy || featureLocked("place-npc")}
          title={featureLocked("place-npc")
            ? LOCKED_HINT
            : "Spawn this NPC in Stormwind + Orgrimmar (needs a world restart to appear)"}
        >
          Place NPC in capitals
        </button>
      {/if}
    {/snippet}
    {@render moduleRow(
      row,
      {
        url: m.url,
        desc: m.desc,
        statusCls: luaStatus(m) === "installed" ? "on" : luaStatus(m) === "cloned" ? "warn" : "off",
        statusText: luaStatusLabel(luaStatus(m)),
        dim: !aleReady,
        onName: row.installed ? () => openTuning(m.key) : undefined,
        onChip: (chip) => onChipClick(chip, m.key),
        onAction: (id) => luaAction(id, m),
        actionDisabled: (id) => luaActionDisabled(id, aleReady),
        actionTitle: (id) => luaActionTitle(id, aleReady),
        actionLabel: (id) =>
          id === "remove" && confirmingLuaRemove === m.key ? `Remove ${m.name} — sure?` : undefined,
      },
      undefined,
      luaControls,
    )}
    {#if m.deployed}{@render setupPanel(m.key)}{/if}
    {#if m.warn}
      <!-- Batch 6 A: read-only advisory (e.g. Paragon's unguarded
           `.test` command), shown only when the CLI reports it. -->
      <p class="mod-warn">⚠ {m.warn}</p>
    {/if}
  {/snippet}

  <div class="card">
    <h3>Lua scripts (ALE)</h3>
    {#if list}
      {@const parts = splitInstalled(list.families.lua, (m) => m.cloned || m.deployed)}
      <h4>Installed ({parts.installed.length})</h4>
      {#if parts.installed.length === 0}
        <p class="muted">Nothing installed yet — browse Available below.</p>
      {/if}
      {#each parts.installed as m (m.key)}
        {@render luaRow(m, list.ale_ready)}
      {/each}
      {#if !list.ale_ready}
        {@const offer = aleInstallOffer(list.families.cpp, list.ale_ready)}
        <!-- Deliberately NOT hiding the list: it's a catalog of what you
             could install, so hiding it hides the reason to install ALE at
             all. Rows render disabled beneath this note instead. Placed
             above the Available section (Task 3): that's the catalog it's
             explaining. -->
        <div class="ale-note">
          <p class="muted">
            These scripts need the ALE module (mod-ale) — the Eluna engine that runs
            them. Install it and everything below becomes available.
          </p>
          {#if offer}
            <button
              class="primary"
              onclick={() => install(offer.key, null, offer.name)}
              disabled={busy || featureLocked("modules-cpp")}
              title={featureLocked("modules-cpp") ? LOCKED_HINT : `Install ${offer.name}`}
            >
              Install ALE
            </button>
          {:else}
            <span class="muted">Find it in the C++ modules list above.</span>
          {/if}
        </div>
      {/if}
      <button class="avail-toggle" onclick={() => (luaExpanded = !luaExpanded)}>
        {luaExpanded ? "▾" : "▸"} Available ({parts.available.length})
      </button>
      {#if luaExpanded}
        {#each parts.available as m (m.key)}
          {@render luaRow(m, list.ale_ready)}
        {/each}
      {/if}
    {/if}
  </div>

  {#snippet sqlRow(m: SqlModule)}
    {#snippet sqlControls()}
      {#if !m.installed}
        {#if m.key === "hearthstone-cd"}
          <label class="row">
            Cooldown
            <select bind:value={hearthstoneVariant} disabled={busy}>
              <option value="1sec">1sec</option>
              <option value="1min">1min</option>
              <option value="5min">5min</option>
              <option value="15min">15min</option>
              <option value="30min">30min</option>
            </select>
          </label>
        {:else if m.key === "npc-teleporter"}
          <label class="row">
            Level
            <input type="number" min="1" max="80" bind:value={npcTeleporterLevel} disabled={busy} />
          </label>
        {/if}
      {/if}
      <label class="row">
        <input type="checkbox" bind:checked={sqlBackup[m.key]} disabled={busy} />
        Back up first (recommended)
      </label>
    {/snippet}
    {@render moduleRow(
      buildModuleRow(m, { family: "sql", removeDisabled: m.key === "rare-drops" }),
      {
        url: m.url,
        desc: m.desc,
        statusCls: m.installed ? "on" : "off",
        statusText: m.installed ? "Installed" : "Not installed",
        onAction: (id) => sqlAction(id, m),
        actionDisabled: sqlActionDisabled,
        actionTitle: (id) => sqlActionTitle(id, m),
        actionLabel: (id) =>
          id === "remove" && confirmingSqlRemove === m.key ? `Remove ${m.name} — sure?` : undefined,
      },
      undefined,
      sqlControls,
    )}
    {#if m.type === "tweak_world"}
      <p class="muted">Tweaks replace each other — installing one removes the active one.</p>
    {/if}
  {/snippet}

  <div class="card">
    <h3>SQL mods</h3>
    {#if list}
      {@const parts = splitInstalled(list.families.sql, (m) => m.installed)}
      <h4>Installed ({parts.installed.length})</h4>
      {#if parts.installed.length === 0}
        <p class="muted">Nothing installed yet — browse Available below.</p>
      {/if}
      {#each parts.installed as m (m.key)}
        {@render sqlRow(m)}
      {/each}
      <button class="avail-toggle" onclick={() => (sqlExpanded = !sqlExpanded)}>
        {sqlExpanded ? "▾" : "▸"} Available ({parts.available.length})
      </button>
      {#if sqlExpanded}
        {#each parts.available as m (m.key)}
          {@render sqlRow(m)}
        {/each}
      {/if}
    {/if}
  </div>

  <div class="card">
    <h3>Install from URL</h3>
    <div class="row">
      <input
        type="text"
        placeholder="https://github.com/.../mod-your-module.git"
        bind:value={customUrl}
        disabled={busy}
      />
      <button
        class="primary"
        onclick={() => install(null, customUrl, customUrl)}
        disabled={busy || !customUrl.trim() || featureLocked("modules-cpp")}
        title={featureLocked("modules-cpp") ? LOCKED_HINT : undefined}
      >
        Install
      </button>
    </div>
    <p class="muted">mod-* repos only</p>
  </div>

  <div class="card">
    <h3>Client folder</h3>
    <div class="row">
      <strong>Current:</strong>
      {#if !clientPath?.path}
        <span class="muted">(not set)</span>
      {:else}
        <span>{clientPath.path}</span>
        {#if !clientPath.valid}
          <span class="warn-text">(saved folder is missing)</span>
        {/if}
      {/if}
    </div>
    {#if clientError}<p class="inline-error">{clientError}</p>{/if}
    <div class="row">
      <input
        type="text"
        placeholder="C:\Games\WoW"
        bind:value={clientPathInput}
        disabled={busy}
      />
      <button
        class="primary"
        onclick={saveClientPath}
        disabled={busy || !clientPathInput.trim() || featureLocked("client-path")}
        title={featureLocked("client-path") ? LOCKED_HINT : undefined}
      >
        Save
      </button>
      <button
        onclick={detectClientPath}
        disabled={busy || featureLocked("client-path")}
        title={featureLocked("client-path") ? LOCKED_HINT : undefined}
      >
        Detect
      </button>
    </div>
    {#if clientCandidates}
      {#if clientCandidates.length === 0}
        <p class="muted">No WoW client folders found.</p>
      {:else}
        <div class="row">
          {#each clientCandidates as c (c)}
            <button
              onclick={() => pickClientCandidate(c)}
              disabled={busy || featureLocked("client-path")}
              title={featureLocked("client-path") ? LOCKED_HINT : undefined}
            >
              {c}
            </button>
          {/each}
        </div>
      {/if}
    {/if}
    <p class="muted">Needed for scripts that ship client-side files (BMAH UI, Paragon, SOD). Windows paths like C:\Games\WoW work.</p>
  </div>

  <div class="card">
    <h3>Server update</h3>
    <div class="row">
      <button onclick={checkUpdates} disabled={busy}>Check for updates</button>
    </div>
    {#if updateCheckError}<p class="inline-error">{updateCheckError}</p>{/if}
    {#if updateCheck}
      {#each updateCheck.repos as r (r.label)}
        <div class="row mrow">
          <strong class="mname">{r.label}</strong>
          <span class="muted">{r.branch}</span>
          <span class="muted">{r.head}</span>
          <span class="spacer"></span>
          {#if r.behind === 0}
            <span class="badge on">up to date</span>
          {:else if r.behind === null}
            <span class="badge off">? behind</span>
          {:else}
            <span class="badge warn">{r.behind} behind</span>
          {/if}
          {#if r.dirty > 0}
            <span class="badge warn">{r.dirty} local edits</span>
          {/if}
        </div>
      {/each}
      {#if updateCheck.note}<p class="muted">{updateCheck.note}</p>{/if}
    {/if}
    <div class="row">
      <label class="row">
        <input type="checkbox" bind:checked={updateBackup} disabled={busy} />
        Back up the server first (recommended)
      </label>
      {#if !confirmingUpdate}
        <button
          class="primary"
          onclick={serverUpdate}
          disabled={busy || featureLocked("server-update")}
          title={featureLocked("server-update") ? LOCKED_HINT : undefined}
        >
          Update
        </button>
      {:else}
        <span>Pulls the latest AzerothCore + mod-playerbots. Local edits are preserved (conflicts saved as patch files). New revisions can run DB migrations at next start. Continue?</span>
        <button class="primary" onclick={serverUpdate} disabled={busy}>Confirm</button>
        <button onclick={() => (confirmingUpdate = false)} disabled={busy}>Cancel</button>
      {/if}
    </div>
  </div>

  <div class="card">
    <h3>Disk cleanup</h3>
    {#if dockerUsageError}
      <p class="inline-error">{dockerUsageError}</p>
    {:else if dockerUsage}
      <pre class="usage">{dockerUsage.join("\n")}</pre>
    {/if}
    <div class="row">
      <label class="row">
        Level
        <select bind:value={cleanLevel} onchange={() => (confirmingClean = false)} disabled={busy}>
          <option value={1}>1 — build cache only (safe)</option>
          <option value={2}>2 — + build volume (CMake artifacts)</option>
          <option value={3}>3 — + unused images (maximum recovery)</option>
        </select>
      </label>
      {#if !confirmingClean}
        <button
          class="primary"
          onclick={clean}
          disabled={busy || featureLocked("docker-clean")}
          title={featureLocked("docker-clean") ? LOCKED_HINT : undefined}
        >
          Clean
        </button>
      {:else}
        <span>Stops the worldserver. The next rebuild after cleaning will be a full 30-90 minute recompile. Continue?</span>
        <button class="primary" onclick={clean} disabled={busy}>Confirm</button>
        <button onclick={() => (confirmingClean = false)} disabled={busy}>Cancel</button>
      {/if}
    </div>
  </div>

  {#if buf.show}
    <Terminal state={buf.term} onclear={() => clearBuf("modules")} logName="dml-modules" />
  {/if}
</section>

  <!-- Both stay MOUNTED while hidden (display:none, not {#if}) so staged
       edits survive tab switches; `active` gates their lazy loads. -->
  <div class="panel" style:display={tab === "tuning" ? null : "none"}>
    <ModuleTuning active={tab === "tuning"} onupdated={refresh} />
  </div>
  <div class="panel" style:display={tab === "files" ? null : "none"}>
    <ModuleFiles active={tab === "files"} />
  </div>
</div>

<style>
  /* The page is a fixed-height column (tab strip + one visible panel) so
     each panel keeps its own scroll -- .content scrolls exactly as it did
     before the strip existed. */
  .page { display: flex; flex-direction: column; height: 100vh; overflow: hidden; }
  .tabstrip { display: flex; gap: 4px; border-bottom: 1px solid #30363d; padding: 12px 24px 0; flex-shrink: 0; }
  .tabstrip button { background: none; border: none; border-bottom: 2px solid transparent; border-radius: 0; padding: 8px 16px; color: #8b949e; font-size: 14px; cursor: pointer; }
  .tabstrip button.active { color: #f0f6fc; border-bottom-color: #58a6ff; }
  .tabstrip button:hover:not(.active) { color: #c9d1d9; }
  .panel { flex: 1; min-height: 0; display: flex; flex-direction: column; overflow: hidden; }
  /* The extracted components' own <section class="content"> must fill the
     panel so their overflow/fill behavior keeps working. */
  .panel > :global(section) { flex: 1; min-height: 0; }
  .content { flex: 1; min-height: 0; padding: 20px 24px; overflow-y: auto; display: flex; flex-direction: column; gap: 12px; }
  .bar { display: flex; justify-content: space-between; align-items: center; }
  .bar h2 { margin: 0; font-size: 18px; }
  .card { background: #0d1117; border: 1px solid #30363d; border-radius: 8px; padding: 12px 16px; display: flex; flex-direction: column; gap: 10px; }
  .card h3 { margin: 0; font-size: 15px; color: #58a6ff; }
  .card h4 { margin: 4px 0 0; font-size: 12px; text-transform: uppercase; letter-spacing: 0.04em; color: #8b949e; }
  .card p { margin: 0; }
  /* Installed/Available split (Modules-page round): the toggle reads as a
     row of its own, not a button competing with the module action buttons. */
  .avail-toggle {
    align-self: flex-start;
    background: none;
    border: none;
    color: #8b949e;
    font-size: 12px;
    padding: 4px 0;
    cursor: pointer;
  }
  .avail-toggle:hover { color: #c9d1d9; }
  .warn-card { border-color: #d29922; }
  .row { display: flex; gap: 10px; align-items: center; flex-wrap: wrap; }
  /* .mrow survives for the Server-update card's repo rows only -- module
     rows all use .modrow below (round 2, Task 1). */
  .mrow { padding: 6px 0; border-top: 1px solid #21262d; }
  .mrow:first-of-type { border-top: none; }

  /* One row skeleton for every module section (round 2, Task 1): a 3-column
     grid -- name block | status+chips | right-aligned action column with a
     shared min-width so the actions line up across rows and sections. One
     row height, one separator, all families. */
  .modrow {
    display: grid;
    grid-template-columns: minmax(260px, 460px) minmax(120px, 1fr) auto;
    gap: 10px;
    align-items: center;
    min-height: 44px;
    padding: 6px 0;
    border-top: 1px solid #21262d;
  }
  .modrow:first-of-type { border-top: none; }
  /* ALE missing: the lua catalog stays readable (that's the point -- it
     shows what you'd get) but reads as unavailable. */
  .modrow.dim { opacity: 0.55; }
  .mmid { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  .mactions {
    display: flex;
    gap: 8px;
    align-items: center;
    justify-content: flex-end;
    flex-wrap: wrap;
    min-width: 300px;
  }
  /* One chip style block (round 2, Task 1): shared size/padding/radius,
     kind -> color token. Rendered from the builder's RowChip list, which
     Tasks 2-4 populate. */
  .mchip {
    font-size: 12px;
    padding: 2px 10px;
    border-radius: 10px;
    border: 1px solid #30363d;
  }
  .mchip.setup, .mchip.rebuild, .mchip.update { color: #d29922; border-color: #d29922; }
  .mchip.conf-failed { color: #f85149; border-color: #f85149; }

  /* ALE missing: the explainer box above the lua catalog. */
  .ale-note {
    display: flex;
    gap: 10px;
    align-items: center;
    flex-wrap: wrap;
    padding: 8px 10px;
    background: #161b22;
    border: 1px solid #30363d;
    border-radius: 6px;
    margin-bottom: 6px;
  }
  .ale-note p { margin: 0; flex: 1 1 260px; }
  .mhead { display: flex; flex-direction: column; gap: 2px; min-width: 260px; max-width: 460px; }
  .mtitle { display: flex; gap: 8px; align-items: baseline; }
  .mname { min-width: 0; }
  /* Click-to-open (Modules-page round, Task 4): installed rows' name opens
     the Tuning tab at that module's card. Plain button styled as
     text-with-hover-underline so it reads as a label, not chrome. (The raw
     .conf filename link left the rows in round 2, Task 1 -- Task 3 re-homes
     config access on the Tuning tab.) */
  .mname-link {
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    font: inherit;
    color: #c9d1d9;
    font-weight: 600;
  }
  .mname-link:hover { text-decoration: underline; }
  .mdesc { color: #8b949e; font-size: 12px; line-height: 1.35; }
  /* Installed clone's last commit (sha · date), from the list arm's additive
     head/head_date fields -- muted version line under the description. */
  .mver { color: #8b949e; font-size: 12px; font-family: Consolas, monospace; }
  .ghlink {
    background: none;
    border: none;
    color: #58a6ff;
    font-size: 12px;
    padding: 0;
    cursor: pointer;
  }
  .ghlink:hover { text-decoration: underline; }
  .spacer { flex: 1; }
  .badge { font-size: 12px; padding: 2px 10px; border-radius: 10px; border: 1px solid #30363d; }
  .badge.on { color: #3fb950; border-color: #3fb950; }
  .badge.warn { color: #d29922; border-color: #d29922; }
  .badge.off { color: #8b949e; }
  input[type="text"] { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 8px; flex: 1; min-width: 260px; }
  input[type="number"] { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 8px; width: 70px; }
  select { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 8px; }
  button { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 14px; cursor: pointer; }
  button.primary { background: #238636; border-color: #2ea043; color: white; }
  button:disabled { opacity: 0.5; cursor: default; }
  .muted { color: #8b949e; font-size: 13px; margin: 0; }
  .warn-text { color: #d29922; font-size: 13px; margin: 0; }
  .mod-warn { color: #d29922; font-size: 12.5px; margin: 0 0 6px 0; line-height: 1.4; }
  .inline-error { color: #f85149; font-size: 13px; margin: 0; }
  .error-card { background: #161b22; border: 1px solid #f85149; border-radius: 8px; padding: 12px 16px; }
  .repair-panel { margin: 0 0 6px 0; padding: 10px 12px; background: #161b22; border: 1px solid #21262d; border-radius: 6px; display: flex; flex-direction: column; gap: 10px; }
  .repair-db { display: flex; flex-direction: column; gap: 6px; }
  .db-name { font-size: 13px; }
  .chip { font-size: 12px; padding: 2px 10px; border-radius: 10px; border: 1px solid #30363d; }
  .chip.tracked { color: #3fb950; border-color: #3fb950; }
  .chip.untracked { color: #8b949e; }
  .repair-results { display: flex; flex-direction: column; gap: 6px; }
  /* Needs-setup / conf-not-activated panels (Modules-page round, Task 5;
     round 2, Task 2): the chip itself is the click target now (.mchip,
     rendered as a real <button> -- see the moduleRow snippet); this is just
     the panel that click opens. */
  .setup-panel { margin: 0 0 6px 0; padding: 10px 12px; background: #161b22; border: 1px solid #21262d; border-radius: 6px; display: flex; flex-direction: column; gap: 10px; }
  .setup-panel ol { margin: 0; padding-left: 20px; display: flex; flex-direction: column; gap: 4px; font-size: 13px; }
  .usage { background: #161b22; border: 1px solid #21262d; border-radius: 6px; padding: 8px 10px; margin: 0; font-size: 12px; color: #8b949e; overflow-x: auto; white-space: pre; }
  /* Check-for-updates busy spinner (Tools page's doctor-button pattern). */
  .spinner {
    width: 12px;
    height: 12px;
    border: 2px solid #30363d;
    border-top-color: #58a6ff;
    border-radius: 50%;
    animation: spin 0.9s linear infinite;
    display: inline-block;
    vertical-align: -2px;
    margin-right: 6px;
  }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
