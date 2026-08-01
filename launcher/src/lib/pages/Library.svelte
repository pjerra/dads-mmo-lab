<script lang="ts">
  import { onMount } from "svelte";
  import {
    gamesCatalog,
    gamesStart,
    gamesStop,
    gamesRemove,
    urlInstall,
    type TitleInfo,
    type TermEvent,
  } from "$lib/api";
  import { applyEvent } from "$lib/terminal-state";
  import Terminal from "$lib/Terminal.svelte";
  import InstallTerminal from "$lib/InstallTerminal.svelte";
  import { nativeInstallRunner } from "$lib/native-install";
  import SoapBootstrap from "$lib/SoapBootstrap.svelte";
  import { gamesInstallNativeState } from "$lib/api";
  import { resolveBackendMode } from "$lib/page-cache.svelte";
  import { termBuf, beginRun, clearBuf, installStore } from "$lib/term-store.svelte";
  import { featureLocked, LOCKED_HINT } from "$lib/features.svelte";
  import { taskbarBusy, taskbarIdle } from "$lib/taskbar";
  import { titleInstallGate, installUnavailableNotice, urlInstallGate } from "$lib/title-install";

  let catalog: TitleInfo[] = $state([]);
  // Whether the backend's HOST can run the title installers at all (the CLI's
  // `install_supported`). Starts true so a slow/failed first catalog read
  // never flashes "you can't install here" -- the honest default is the one
  // that matches every WSL install.
  let installSupported = $state(true);
  // Which backend drives Install. Starts "wsl" -- the historical behaviour --
  // so a slow resolve never briefly arms a native-only control on a WSL box.
  let backendMode = $state<"wsl" | "native">("wsl");
  // Title ids with a half-finished native install on disk. Drives the button
  // label only -- the engine resumes whether or not the UI knows.
  let resumable = $state<Set<string>>(new Set());
  // Which title is showing the native install-consent panel. A native install
  // is hours long, tens of GB, and CANNOT be cancelled (its children are the
  // launcher's own processes), so it gets the same "say what this costs before
  // you do it" treatment Remove and URL-install already have -- and the
  // registry can go on truthfully calling the wiring untested without locking
  // the button and dead-ending onboarding.
  let confirmingInstall = $state<string | null>(null);
  // Shown after a native install finishes. A server with no account leaves GM
  // Tools, My Party, the console's send box and announcements all dead with a
  // SOAP_AUTH that explains nothing -- the worst thing that can happen at the
  // end of a multi-hour install, and the reason this is surfaced rather than
  // left as a documented step.
  let showSoapSetup = $state(false);
  // The subset that the catalog nonetheless calls INSTALLED -- i.e. a title dir
  // that exists but whose install never finished. This is the state the catalog
  // cannot express, because its `installed` test is only "does the directory
  // exist" and the engine creates that directory in stage 3 of 8.
  const unfinished = $derived(new Set([...resumable].filter((id) => installedIds.has(id))));
  let loadError: string | null = $state(null);
  let actionError: string | null = $state(null);
  let note: string | null = $state(null);

  // Start/stop, in flight for this id (existing pattern).
  let busyId: string | null = $state(null);
  const buf = termBuf("library");

  // Typed-confirm remove: which row is armed, its input, and whether the
  // gamesRemove stream is actually running.
  let removingId: string | null = $state(null);
  let removeInput = $state("");
  let removeBusy = $state(false);
  // Batch 3 F13c: keep the ~6 GB client-data volume for a faster reinstall.
  let removeKeepData = $state(false);
  // Batch 6 B: also delete the AzerothCore/MySQL server images (~3-5 GB).
  let removeImages = $state(false);

  // Install: which title's panel is shown, and whether its session is still
  // active. This lives in installStore (term-store.svelte.ts), NOT local
  // $state -- Library.svelte is destroyed on nav-away, but a RUNNING
  // interactive install (and the backend's single global install slot) keeps
  // going regardless, and the transcript already lived in installStore.text.
  // Gating the panel on local state meant nav-away-and-back left the panel
  // gone, the reply input (the only channel for gamesInstallInput) and
  // Cancel unreachable, with no way back short of an app restart. Reading
  // installStore.id/.running/.nonce here instead means the panel, reply
  // input and Cancel all survive nav intact.
  // installStore.nonce is bumped on every startInstall() so the {#key} block
  // below always remounts InstallTerminal, even when re-installing the same
  // title id after a failed/cancelled run (installStore.id alone wouldn't
  // change -> no remount -> onMount(run) never re-fires -> no exit event ->
  // installStore.running stuck true -> every control disabled forever until
  // nav-away).

  // Install session OR a remove stream blocks the other mutating actions
  // (start/stop, arm/confirm remove). installStore is also the backing
  // store for Tools.svelte's tool:-prefixed sessions (Round Q) -- a running
  // Unbound install must not soft-lock Library's start/stop/remove for its
  // 30-90 minute duration (those never touch the install slot), so only a
  // Library-owned (non "tool:"-prefixed) install session counts here.
  const busy = $derived(
    busyId !== null || removeBusy || (installStore.running && !installStore.id?.startsWith("tool:")),
  );
  // STARTING a new install is different: games_install and tool_install
  // share the backend's single InstallSlot, so the Install button must be
  // blocked by ANY running session -- tool-owned included. Review-caught:
  // gating it on the filtered `busy` above would let a mid-Unbound Install
  // click clobber installStore out from under the live session (orphaning
  // its reply/Cancel UI) and then BUSY-fail anyway.
  const installBlocked = $derived(busy || installStore.running);

  const installed = $derived(catalog.filter((t) => t.installed));
  const installedIds = $derived(new Set(installed.map((t) => t.id)));
  const available = $derived(catalog.filter((t) => !t.installed));
  const installNotice = $derived(
    installUnavailableNotice({ installSupported, availableCount: available.length, backendMode }),
  );
  // A URL install runs a community installer the same way on the same host, so
  // the host verdict binds it too -- it is only exempt from the shipped-scripts
  // half of the question.
  const urlGate = $derived(urlInstallGate({ installSupported }));

  async function refresh() {
    try {
      const c = await gamesCatalog();
      catalog = c.titles;
      installSupported = c.installSupported;
      loadError = null;
      // Recomputed HERE rather than at mount, so the Refresh button and the
      // post-install refresh both update the labels. A resumable set that is
      // only ever computed once is stale the moment an install starts.
      if (backendMode === "native") await refreshResumable();
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      loadError = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    }
  }
  // ONE onMount, and the ORDER is the fix. These were two, and the second
  // awaited only `backend_mode` -- an in-process, memoized IPC -- while the
  // first awaited `games catalog`, which spawns a bash subprocess. The cheap
  // read always won, so `refreshResumable()` iterated an empty `catalog`,
  // assigned an empty Set, and was never recomputed by anything: not `refresh`,
  // not the Refresh button, not `onInstallExit`. The entire
  // games_install_native_state -> resumable -> "Resume install" chain was dead
  // code, and fixing the catalog's `installed` rule alone would have LOOKED
  // like it worked while still saying "Install".
  onMount(async () => {
    // Failure leaves the "wsl" default in place: a backend probe that fell over
    // must not arm the native engine on a machine we could not identify.
    try {
      backendMode = (await resolveBackendMode()) === "native" ? "native" : "wsl";
    } catch {
      backendMode = "wsl";
    }
    await refresh();
  });

  // EVERY title, not just the ones the catalog calls available -- and that is
  // the whole point.
  //
  // `games catalog` decides `installed` with `[[ -d "$GAMES_DIR/$1" ]]`
  // (cli/src/80-titles.sh) -- the directory merely EXISTING. The native engine
  // creates that directory at stage 3 of 8, so from the first minute of a
  // multi-hour build the title reports installed:true. Scanning only the
  // available list therefore missed a half-finished install ENTIRELY: it is
  // never in that list, which made the Resume affordance dead code on exactly
  // the path it exists for, and left the user looking at a Start button for a
  // server that was never compiled.
  //
  // Best-effort throughout: a title we cannot ask about keeps the plain
  // labels. Getting this wrong in the safe direction costs a word; blocking
  // the page on it would cost the whole Library.
  async function refreshResumable() {
    const found = new Set<string>();
    await Promise.all(
      catalog.map(async (t) => {
        try {
          if ((await gamesInstallNativeState(t.id)).in_progress) found.add(t.id);
        } catch {
          /* leave it out */
        }
      }),
    );
    resumable = found;
  }

  function showActionErr(e: unknown) {
    const err = e as { message?: string; hint?: string };
    actionError = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
  }

  async function act(id: string, action: "start" | "stop") {
    busyId = id;
    actionError = null;
    note = null;
    beginRun("library");
    taskbarBusy();
    try {
      const run = action === "start" ? gamesStart : gamesStop;
      await run(id, (e) => {
        buf.term = applyEvent(buf.term, e);
      });
    } catch (e) {
      const err = e as { code?: string; message?: string; hint?: string };
      buf.term = applyEvent(buf.term, {
        event: "error",
        error: {
          code: err.code ?? "IPC",
          message: err.message ?? String(e),
          hint: err.hint ?? "",
        },
      });
    } finally {
      taskbarIdle();
      busyId = null;
      await refresh();
    }
  }

  function armRemove(id: string) {
    removingId = id;
    removeInput = "";
    removeKeepData = false;
    removeImages = false;
  }
  function cancelRemoveArm() {
    removingId = null;
    removeInput = "";
  }

  function confirmRemove(id: string) {
    if (removeInput !== id) return;
    removingId = null;
    removeInput = "";
    return runRemove(id);
  }

  // Same sawDone/streamErr contract as ModuleManager's runStream: the
  // outcome is derived from events captured in the callback, then applied
  // AFTER the trailing refresh() -- the streaming promise resolves even
  // when the underlying CLI step failed.
  async function runRemove(id: string) {
    removeBusy = true;
    actionError = null;
    note = null;
    beginRun("library");
    taskbarBusy();
    let sawDone = false;
    let streamErr: { message?: string; hint?: string } | null = null;
    let outcomeErr: unknown = null;
    try {
      await gamesRemove(
        id,
        (e: TermEvent) => {
          buf.term = applyEvent(buf.term, e);
          if (e.event === "done") {
            sawDone = true;
          } else if (e.event === "error") {
            streamErr = (e as { error?: { message?: string; hint?: string } }).error ?? {};
          }
        },
        removeKeepData,
        removeImages,
      );
    } catch (e) {
      outcomeErr = e;
    } finally {
      taskbarIdle();
      removeBusy = false;
      await refresh();
      if (outcomeErr) showActionErr(outcomeErr);
      else if (streamErr) showActionErr(streamErr);
      else if (sawDone) note = `Removed ${id}.`;
    }
  }

  // The native route asks first; the WSL route is unchanged (its installer is
  // interactive and cancellable, and it already asks its own questions).
  function requestInstall(id: string) {
    if (backendMode === "native") {
      confirmingInstall = id;
      return;
    }
    startInstall(id);
  }

  function confirmInstall(id: string) {
    confirmingInstall = null;
    startInstall(id);
  }

  function startInstall(id: string) {
    // Always a FRESH session (the Install button that calls this is only
    // rendered while !busy, i.e. no session is already running -- see the
    // {#if !busy} guard around it below), so resetting the transcript here
    // is safe: resuming an already-running session never re-enters this
    // function, it just reads the store's existing state on remount.
    installStore.id = id;
    installStore.nonce += 1;
    installStore.running = true;
    installStore.exitCode = null;
    actionError = null;
    note = null;
    installStore.text = "";
  }

  // --- Install from URL (Batch 4 F16) --------------------------------------
  // Streams the EXISTING interactive `dml run <url>` CLI arm through the
  // same InstallTerminal/install-slot machinery as regular game installs.
  // The id carries the URL ("url:<https link>") so the runner closure and a
  // nav-away remount both know what to (re)connect to.
  let urlValue = $state("");
  let urlArmed = $state(false);
  let urlConfirmInput = $state("");

  function armUrlInstall() {
    urlArmed = true;
    urlConfirmInput = "";
  }
  function cancelUrlArm() {
    urlArmed = false;
    urlConfirmInput = "";
  }
  function confirmUrlInstall() {
    if (urlConfirmInput !== "install") return;
    urlArmed = false;
    urlConfirmInput = "";
    startInstall(`url:${urlValue.trim()}`);
  }

  // installStore.running is flipped false directly by InstallTerminal's own
  // exit/error handling (so it stays truthful even if the instance that
  // witnesses the exit event is an orphaned one from before a nav-away) --
  // this callback's only remaining job is refreshing the catalog so the
  // "installed" flag updates promptly while this page is mounted.
  function onInstallExit(code: number) {
    // Only on success, and only on native: the WSL installers run their own
    // account step interactively, so raising this there would ask the user to
    // do a thing they were just walked through.
    if (code === 0 && backendMode === "native") showSoapSetup = true;
    void refresh();
  }
</script>

<section class="content">
  <header class="bar">
    <h2>Game Library</h2>
    <button onclick={refresh}>Refresh</button>
  </header>

  {#if loadError}
    <div class="error-card">
      <strong>Couldn't reach the DML backend.</strong>
      <p>{loadError}</p>
    </div>
  {:else if catalog.length === 0}
    <p class="muted">No titles found.</p>
  {/if}
  {#if actionError}
    <div class="error-card"><p>{actionError}</p></div>
  {/if}
  {#if note}<p class="muted">{note}</p>{/if}

  <h3>Installed</h3>
  {#if installed.length === 0}
    <p class="muted">No titles installed yet.</p>
  {/if}
  <div class="cards">
    {#each installed as t (t.id)}
      <div class="card">
        <div class="card-row">
          <div class="card-title">
            <span class="dot {unfinished.has(t.id) ? 'off' : t.running === 'running' ? 'on' : 'off'}"></span>
            {t.name}
            {#if unfinished.has(t.id)}<span class="badge-unfinished">unfinished install</span>{/if}
          </div>
          <div class="card-actions">
            {#if unfinished.has(t.id)}
              <!-- The install never finished, so there is nothing to start:
                   the image was never built. Offering Start here sends the user
                   into a compose up that fails for reasons nothing on screen
                   explains. Resume continues from the last finished stage. -->
              <button
                class="primary"
                disabled={installBlocked || featureLocked("native-install")}
                title={featureLocked("native-install") ? LOCKED_HINT : undefined}
                onclick={() => requestInstall(t.id)}
              >
                Resume install
              </button>
            {:else if t.running === "running"}
              <button disabled={busy} onclick={() => act(t.id, "stop")}>Stop</button>
            {:else}
              <button class="primary" disabled={busy} onclick={() => act(t.id, "start")}>Start</button>
            {/if}
            {#if removingId !== t.id}
              <button
                disabled={busy || featureLocked("title-remove")}
                title={featureLocked("title-remove") ? LOCKED_HINT : undefined}
                onclick={() => armRemove(t.id)}
              >
                Remove
              </button>
            {/if}
          </div>
        </div>
        {#if confirmingInstall === t.id}
          <div class="install-confirm">
            <p><strong>Before you start — this one is a big job.</strong></p>
            <ul>
              <li>It builds the server from source. On this kind of PC that is <strong>hours</strong>, not minutes.</li>
              <li>It downloads and writes <strong>tens of gigabytes</strong>.</li>
              <li>It <strong>can't be cancelled</strong> once started. You can close this page or the launcher — the build carries on, and running the install again later continues from the last finished step rather than starting over.</li>
              <li>The launcher's own install flow hasn't been through a full click-through yet. The engine behind it has completed a real build on this machine, but expect rough edges and please report them.</li>
            </ul>
            <div class="row">
              <button class="primary" onclick={() => confirmInstall(t.id)}>Start the build</button>
              <button onclick={() => (confirmingInstall = null)}>Not now</button>
            </div>
          </div>
        {/if}
        {#if unfinished.has(t.id)}
          <p class="unfinished-note">
            This server's install stopped before it finished, so it can't start yet —
            nothing was built. Resuming continues from the last completed step and
            reuses Docker's build cache, so you don't pay for it twice.
          </p>
        {/if}
        {#if removingId === t.id}
          <div class="remove-confirm">
            <p>Removing deletes the server and its data. Backups under ~/.dml are kept. Type the title id to confirm:</p>
            <label class="keep-data">
              <input type="checkbox" bind:checked={removeKeepData} disabled={busy} />
              Keep downloaded game data (~6 GB) for faster reinstall
            </label>
            <label class="keep-data">
              <input type="checkbox" bind:checked={removeImages} disabled={busy} />
              Also delete downloaded server images (~3-5 GB) — slower to reinstall
            </label>
            <div class="row">
              <input type="text" placeholder={t.id} bind:value={removeInput} />
              <button disabled={removeInput !== t.id || busy} onclick={() => confirmRemove(t.id)}>Remove</button>
              <button onclick={cancelRemoveArm}>Cancel</button>
            </div>
          </div>
        {/if}
      </div>
    {/each}
  </div>

  <h3>Available titles</h3>
  {#if available.length === 0}
    <p class="muted">No available titles.</p>
  {/if}
  {#if installNotice}
    <div class="notice-card">
      <strong>{installNotice.title}</strong>
      <p>{installNotice.body}</p>
    </div>
  {/if}
  <div class="cards">
    {#each available as t (t.id)}
      {@const gate = titleInstallGate({ installSupported, scriptAvailable: t.script_available, backendMode, id: t.id })}
      <div class="card">
        <div class="card-row">
          <div class="card-title">{t.name}</div>
          <div class="card-actions">
            {#if !installBlocked}
              {#if gate.canInstall}
                <!-- Two branches rather than one computed key, so each feature
                     key appears as a LITERAL. feature-keys.test.ts scans the UI
                     for lock calls with a quoted key, and a key hidden behind a
                     variable is invisible to the one test whose job is catching
                     registry drift -- exactly how "native-install" came to be
                     registered and wired to nothing. -->
                {#if gate.route === "native-engine"}
                  <button
                    class="primary"
                    disabled={featureLocked("native-install")}
                    title={featureLocked("native-install") ? LOCKED_HINT : undefined}
                    onclick={() => requestInstall(t.id)}
                  >
                    {resumable.has(t.id) ? "Resume install" : "Install"}
                  </button>
                {:else}
                  <button
                    class="primary"
                    disabled={featureLocked("title-install")}
                    title={featureLocked("title-install") ? LOCKED_HINT : undefined}
                    onclick={() => requestInstall(t.id)}
                  >
                    Install
                  </button>
                {/if}
              {:else}
                <!-- One hint per REASON (title-install.ts). The old copy named
                     cli/dev-install.ps1 unconditionally, which sent every
                     native-backend user to re-run a step that was already
                     fine and could never have helped. -->
                <button disabled title={gate.hint}>Install</button>
              {/if}
            {/if}
          </div>
        </div>
        {#if confirmingInstall === t.id}
          <div class="install-confirm">
            <p><strong>Before you start &mdash; this one is a big job.</strong></p>
            <ul>
              <li>It builds the server from source. On this kind of PC that is <strong>hours</strong>, not minutes.</li>
              <li>It downloads and writes <strong>tens of gigabytes</strong>.</li>
              <li>It <strong>can't be cancelled</strong> once started. You can close this page or the launcher &mdash; the build carries on, and running the install again later continues from the last finished step rather than starting over.</li>
              <li>The launcher's own install flow hasn't been through a full click-through yet. The engine behind it has completed a real build on this machine, but expect rough edges and please report them.</li>
            </ul>
            <div class="row">
              <button class="primary" onclick={() => confirmInstall(t.id)}>Start the build</button>
              <button onclick={() => (confirmingInstall = null)}>Not now</button>
            </div>
          </div>
        {/if}

      </div>
    {/each}
  </div>

  <h3>Install from URL / community titles</h3>
  <div class="card url-card">
    <p class="muted">
      Paste a project's git link (https) to install a community-made server that follows the
      DML layout (an install.sh at the repo root).
    </p>
    <p class="url-warn">
      This runs the project's own install script on your machine — only use sources you
      trust.
    </p>
    <div class="row">
      <input
        type="text"
        placeholder="https://github.com/user/some-game-server.git"
        bind:value={urlValue}
        oninput={cancelUrlArm}
        disabled={installBlocked || !urlGate.canInstall}
      />
      {#if !urlArmed}
        <button
          class="primary"
          disabled={installBlocked || !urlGate.canInstall || !urlValue.trim().startsWith("https://") || featureLocked("title-url-install")}
          title={!urlGate.canInstall ? urlGate.hint : featureLocked("title-url-install") ? LOCKED_HINT : undefined}
          onclick={armUrlInstall}
        >
          Install
        </button>
      {/if}
    </div>
    {#if urlArmed}
      <div class="remove-confirm">
        <p>About to run the installer from {urlValue.trim()}. Type "install" to confirm:</p>
        <div class="row">
          <input type="text" placeholder="install" bind:value={urlConfirmInput} />
          <button
            class="primary"
            disabled={urlConfirmInput !== "install" || installBlocked}
            onclick={confirmUrlInstall}
          >
            Run installer
          </button>
          <button onclick={cancelUrlArm}>Cancel</button>
        </div>
      </div>
    {/if}
  </div>

  {#if installStore.id && !installStore.id.startsWith("tool:")}
    {#key installStore.nonce}
      {#if installStore.id.startsWith("url:")}
        <InstallTerminal
          id={installStore.id}
          runner={(id, onEvent) => urlInstall(id.slice("url:".length), onEvent)}
          lockFlag="title-url-install"
          onExit={onInstallExit}
        />
      {:else if backendMode === "native"}
        <!-- The native engine speaks the project's NDJSON vocabulary rather
             than a pty's chunk/exit, so it goes through the translating runner
             in native-install.ts. InstallTerminal itself is untouched. -->
        <InstallTerminal
          id={installStore.id}
          runner={nativeInstallRunner}
          interactive={false}
          onExit={onInstallExit}
        />
      {:else}
        <InstallTerminal id={installStore.id} onExit={onInstallExit} />
      {/if}
    {/key}
  {/if}

  {#if showSoapSetup}
    <SoapBootstrap onverified={() => (showSoapSetup = false)} />
  {/if}

  {#if buf.show}
    <Terminal state={buf.term} onclear={() => clearBuf("library")} logName="dml-library" />
  {/if}
</section>

<style>
  .badge-unfinished {
    margin-left: 0.5rem;
    font-size: 0.72rem;
    font-weight: 600;
    padding: 0.1rem 0.4rem;
    border-radius: 0.35rem;
    background: var(--warn-bg, #4a3a12);
    color: var(--warn-fg, #f0c674);
    vertical-align: middle;
  }
  .install-confirm {
    margin-top: 0.6rem;
    padding: 0.7rem 0.85rem;
    border: 1px solid var(--warn-fg, #f0c674);
    border-radius: 0.4rem;
    background: var(--warn-bg-soft, rgba(240, 198, 116, 0.08));
    font-size: 0.86rem;
    line-height: 1.45;
  }
  .install-confirm p { margin: 0 0 0.4rem; }
  .install-confirm ul { margin: 0 0 0.6rem; padding-left: 1.1rem; }
  .install-confirm li { margin-bottom: 0.25rem; }
  .unfinished-note {
    margin: 0.5rem 0 0;
    font-size: 0.85rem;
    opacity: 0.85;
    line-height: 1.4;
  }
  .content { padding: 20px 24px; overflow-y: auto; display: flex; flex-direction: column; gap: 16px; }
  .bar { display: flex; justify-content: space-between; align-items: center; }
  .bar h2 { margin: 0; font-size: 18px; }
  h3 { margin: 0; font-size: 15px; color: #58a6ff; }
  .cards { display: flex; flex-wrap: wrap; gap: 12px; }
  .card {
    background: #0d1117;
    border: 1px solid #30363d;
    border-radius: 8px;
    padding: 14px 16px;
    min-width: 280px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .card-row { display: flex; justify-content: space-between; align-items: center; gap: 16px; }
  .card-title { display: flex; align-items: center; gap: 8px; font-weight: 600; }
  .card-actions { display: flex; gap: 8px; }
  .dot { width: 9px; height: 9px; border-radius: 50%; display: inline-block; }
  .dot.on { background: #3fb950; }
  .dot.off { background: #6e7681; }
  .remove-confirm { border-top: 1px solid #21262d; padding-top: 10px; display: flex; flex-direction: column; gap: 8px; }
  .remove-confirm p { margin: 0; font-size: 13px; color: #d29922; }
  .url-card { max-width: 640px; }
  .url-warn { margin: 0; font-size: 13px; font-weight: 600; color: #f85149; }
  .keep-data { display: flex; gap: 6px; align-items: center; font-size: 13px; color: #8b949e; }
  .row { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  .row input[type="text"] {
    background: #21262d;
    color: #c9d1d9;
    border: 1px solid #30363d;
    border-radius: 6px;
    padding: 6px 8px;
    flex: 1;
    min-width: 160px;
  }
  button { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 14px; cursor: pointer; }
  button.primary { background: #238636; border-color: #2ea043; color: white; }
  button:disabled { opacity: 0.5; cursor: default; }
  .muted { color: #8b949e; margin: 0; }
  .error-card { background: #161b22; border: 1px solid #f85149; border-radius: 8px; padding: 12px 16px; }
  /* Explanation, not a failure: this is the state of the backend, nothing
     went wrong. Blue-grey, matching the informational cards elsewhere. */
  .notice-card {
    background: #161b22;
    border: 1px solid #30363d;
    border-left: 3px solid #58a6ff;
    border-radius: 8px;
    padding: 12px 16px;
    max-width: 640px;
  }
  .notice-card p { margin: 6px 0 0; color: #8b949e; font-size: 13px; line-height: 1.5; }
</style>
