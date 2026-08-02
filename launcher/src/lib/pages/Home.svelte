<script lang="ts">
  import { onMount } from "svelte";
  import { gamesStatus, gamesStart, gamesStop, gamesRestart, wowPlayersOnline, wowWorldRestart, nativeSetupStatus, wowSoapCredentials, wowUnboundStatus, type PlayerOnline, type NativeSetupStatus, type SoapCredentials, type UnboundStatus } from "$lib/api";
  import { dockerRestartCardVisible } from "$lib/docker-restart";
  import { className } from "$lib/wow";
  import { applyEvent } from "$lib/terminal-state";
  import Terminal from "$lib/Terminal.svelte";
  import { termBuf, beginRun, clearBuf } from "$lib/term-store.svelte";
  import { featureLocked, LOCKED_HINT } from "$lib/features.svelte";
  import { chipStart, serverStatus, refreshServerStatus, statusLabel } from "$lib/server-status.svelte";
  import { restartState, clearApplyNeeded } from "$lib/restart-state.svelte";
  import { installProgress, installDetailText } from "$lib/install-progress.svelte";
  import { unboundBadge } from "$lib/unbound-badge";
  import { bannerText, fastRestartBlockedReason } from "$lib/apply-needed";
  import { trayAction } from "$lib/tray-action.svelte";
  import { taskbarBusy, taskbarIdle } from "$lib/taskbar";
  import { toolPrefs } from "$lib/tool-prefs.svelte";
  import { saveallPref, setSaveBeforeRestart, wouldSkipSaveall, saveallBoxChecked } from "$lib/saveall-pref.svelte";
  import { soapSetupState, showSoapSetup } from "$lib/soap-setup-state.svelte";
  import type { PageId } from "$lib/nav";

  // Cross-page nav, wired by the shell exactly like Help's (+page.svelte).
  // Home needs it for one thing only: the soap_unreachable card's advice used
  // to be prose with nothing to click -- it now hands the user to the Tools
  // card that actually performs the fix.
  let { onnav }: { onnav?: (p: PageId) => void } = $props();

  const WOW_ID = "wow-server-playerbots";
  const ROLE_LABELS: Record<string, string> = {
    world: "World server",
    auth: "Auth server",
    database: "Database",
  };

  // Backend probe, for ONE decision: whether the soap_unreachable card may
  // offer the jump to Tools' "Restart Docker in the distro" card. That card is
  // WSL-only (native mode drives Docker Desktop and has no dml-arch to
  // restart), so an ungated jump button would navigate to a page where the
  // promised card does not render. The visibility rule is not restated here --
  // Home asks dockerRestartCardVisible(), the same predicate the card itself
  // uses, so the two can never disagree (including on a null probe, which
  // hides rather than guesses).
  let nativeStatus: NativeSetupStatus | null = $state(null);

  let containerState: "running" | "stopped" | null = $state(null);
  // The add-on's on-disk state. Fetched on refresh rather than on every poll
  // tick: it reads a state file plus six source files, and none of that
  // changes between 3-second polls. A failure (WSL mode refuses outright)
  // leaves it null, which the badge renders as SILENCE rather than as
  // "not installed" -- a claim nothing checked.
  let unbound: UnboundStatus | null = $state(null);
  const unboundTag = $derived.by(() => unboundBadge(unbound));
  let statusError: string | null = $state(null);
  let refreshing = $state(false);
  let expanded = $state(false);

  // The SOAP credentials row in the health panel. Nobody types this password
  // any more -- the launcher generates it -- so this is the only place a user
  // can read it back.
  //
  // Both pieces of state live HERE, at component scope, not inside the health
  // markup: those rows re-render on every status poll, and state declared
  // alongside them would be reset (and a fetch re-fired) a few seconds after
  // every reveal. The only thing that calls the backend is the click handler.
  let soapCreds: SoapCredentials | null = $state(null);
  let soapPassShown = $state(false);

  // "Is anything actually configured?" is the BACKEND's answer (`configured`),
  // never a comparison here.
  //
  // This row used to decide it by testing the returned pair against the literal
  // strings "admin"/"admin" -- the resolver's compiled-in fallback when neither
  // DML_SOAP_* nor ~/.dml/soap.env supplies anything. That cannot distinguish
  // the fallback from a REAL account: admin/admin is the pair every AzerothCore
  // tutorial tells people to create, it passes the launcher's own account
  // validator, and CLAUDE.md documents the env vars as defaulting to it. A user
  // with a working admin account was told "No credentials saved yet" directly
  // under a row reporting SOAP healthy, with nothing in the UI able to settle
  // which was true.

  // Fetched on demand, never on render: an always-fetched password is a
  // password in every devtools trace of every poll.
  async function toggleSoapPass() {
    if (soapPassShown) {
      soapCreds = null;
      soapPassShown = false;
      return;
    }
    try {
      soapCreds = await wowSoapCredentials(true);
      soapPassShown = true;
    } catch {
      soapCreds = null;
    }
  }

  let busy = $state(false);
  const buf = termBuf("home");

  // "Save characters before restart". Default ON = safe, and NOT persisted --
  // saveall-pref.svelte.ts re-asserts saving on every app start after the
  // 2026-07-21 incident, where an unticked box outlived the session that
  // armed it. The pre-stop saveall is a redundant FIRST save -- the graceful
  // `docker stop -t 300` also saves everyone on shutdown -- so turning this
  // OFF just skips the redundant save for a faster restart. Gated by
  // [skip-saveall]: while that feature is locked we never skip, and the box
  // shows checked+disabled so the display matches the forced-save behavior.
  const saveallLocked = $derived(featureLocked("skip-saveall"));
  const skipSaveall = $derived(wouldSkipSaveall(saveallPref.saveBeforeRestart, saveallLocked));

  // `detail`/`detailError` now live in the server-status store (server-status.svelte.ts)
  // so the sidebar chip and Console see the same last-known data instantly on
  // remount instead of going blank until this page's own fetch lands.
  async function refresh() {
    refreshing = true;
    try {
      containerState = (await gamesStatus(WOW_ID)).state;
      statusError = null;
      // Deliberately after the status call and deliberately swallowed: the
      // add-on badge is a nicety, and it must never be able to turn a healthy
      // server card into an error card.
      wowUnboundStatus().then(
        (u) => (unbound = u),
        () => (unbound = null),
      );
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      statusError = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
      containerState = null;
    }
    await refreshServerStatus();
    void refreshPlayers();
    refreshing = false;
  }
  onMount(refresh);

  // Read once on mount, off the status path: the backend cannot change while
  // the app runs (AppState builds its runner once, so a backend switch needs a
  // relaunch). A failed probe leaves it null, which hides the jump button --
  // the same "don't offer it if we can't confirm the backend" stance the card
  // takes.
  onMount(async () => {
    try {
      nativeStatus = await nativeSetupStatus();
    } catch {
      // Best-effort: the status card's own error handling already tells the
      // user when the bridge is unhappy.
    }
  });

  // Players-online card (Batch 3 F11a): fetched only while the world is
  // actually up (the DB query would just error otherwise). Refetches on
  // page Refresh and whenever the polled verdict flips to online.
  let players: PlayerOnline[] = $state([]);
  let playersLoaded = $state(false);
  async function refreshPlayers() {
    if (serverStatus.detail?.verdict !== "online") {
      players = [];
      playersLoaded = false;
      return;
    }
    try {
      players = await wowPlayersOnline();
      playersLoaded = true;
    } catch {
      // Best-effort card -- a transient DB error just keeps the last list.
    }
  }
  // Refetched on EVERY successful poll while the world is up, not only on a
  // verdict flip: this list is now also the SOURCE of the "Players online"
  // number (SOAP's "Connected players" counts bot sessions, so it cannot be),
  // and a count that only moves when the server starts or stops is worse than
  // no count. `serverStatus.detail` is replaced by each successful poll and
  // left untouched by a failed one, so its identity IS the poll heartbeat.
  let lastDetail: unknown = null;
  $effect(() => {
    const d = serverStatus.detail;
    if (d !== lastDetail) {
      lastDetail = d;
      void refreshPlayers();
    }
  });

  // The bot-free count behind the "Players online" rows. `null` (rendered
  // "?") until the list has actually loaded once -- never 0, which would
  // claim an empty world on a failed read.
  const humansOnline = $derived(playersLoaded ? players.length : null);

  // Chip quick-start consumer (Batch 2 F8): the sidebar ▶ sets the request
  // and navigates here; this effect runs on mount AND when the request flips
  // while Home is already the active page. Consumed before starting so a
  // re-render can't double-fire.
  $effect(() => {
    if (chipStart.requested && !busy) {
      chipStart.requested = false;
      act("start");
    }
  });

  // Tray menu Start/Stop consumer. Same shape as the chip consumer above:
  // the tray sets the request and Rust surfaces the window, the shell lands
  // on Home, and this runs the ordinary act() path -- so the tray shares one
  // implementation with the buttons instead of driving the lifecycle API
  // itself. Consumed before acting so a re-render cannot double-fire.
  $effect(() => {
    if (!trayAction.pending) return;
    // DROP the request if we are mid-operation rather than queueing it: a
    // stop that fires minutes later, when the user has moved on, is worse
    // than one that visibly did nothing.
    const action = trayAction.pending;
    trayAction.pending = null;
    if (!busy) act(action);
  });

  const fastRestartBlocked = $derived(
    restartState.needed ? fastRestartBlockedReason(restartState.apply) : "",
  );

  async function act(action: "start" | "stop" | "restart") {
    busy = true;
    beginRun("home");
    taskbarBusy();
    // The shared restarting flag drives the amber "Restarting…" override on
    // the card and the sidebar chip -- without it, polling mid-restart flaps
    // through stopped/starting. Config/Backups set it for their flows; this
    // covers the Home buttons (a start after a stop reads as "starting" via
    // the polled verdict, so only restart needs the explicit flag).
    if (action === "restart") restartState.restarting = true;
    // Only a run that actually SUCCEEDED may clear the pending-apply banner.
    //
    // "await returned" is not success: a streaming command resolves Ok even when
    // the CLI exits non-zero (run_stream returns Ok(code) for any code, and
    // stream_args maps it to Ok(())) -- the failure travels as an `error` event
    // instead. Deriving this from the promise cleared the banner for a restart
    // that never happened, which is the mirror image of the bug the banner
    // exists to prevent: a DB that never became healthy means `dml-start.sh`
    // exits before `compose up -d`, so the env removal is not applied, and the
    // user would be left with no signal at all. Same rule as the bot-flush
    // stream below.
    let applied = false;
    try {
      if (action === "restart") {
        await gamesRestart(WOW_ID, skipSaveall, (e) => {
          buf.term = applyEvent(buf.term, e);
          // A full restart is a compose down->up: the containers are RECREATED,
          // so every pending save is applied -- including the ones the fast
          // world-only restart could not touch.
          if (e.event === "done") applied = true;
        });
      } else if (action === "start") {
        await gamesStart(WOW_ID, (e) => {
          buf.term = applyEvent(buf.term, e);
          // A cold start creates the containers from the current compose +
          // confs, so it applies pending saves for the same reason.
          if (e.event === "done") applied = true;
        });
      } else {
        // manageDocker (native mode only, review finding #6): the persisted
        // Tools-page preference for whether stopping the server also stops
        // Docker Desktop. WSL mode ignores it (stop_engine_enabled gates on
        // native regardless of this value).
        await gamesStop(
          WOW_ID,
          (e) => {
            buf.term = applyEvent(buf.term, e);
          },
          toolPrefs.manageDocker,
        );
      }
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
      if (action === "restart") restartState.restarting = false;
      if (applied) clearApplyNeeded();
      busy = false;
      await refresh();
    }
  }

  // Fast world-only restart (Batch 3 F11f): restarts ONLY the worldserver
  // container. Faster than a full Restart, but docker restart keeps
  // creation-time env -- settings changes do NOT apply (the stream repeats
  // that caveat). Shares the restarting flag so the chip/card show amber.
  async function worldRestart() {
    busy = true;
    beginRun("home");
    restartState.restarting = true;
    taskbarBusy();
    try {
      await wowWorldRestart(skipSaveall, (e) => {
        buf.term = applyEvent(buf.term, e);
        // This restarts the world PROCESS, so it satisfies a pending
        // `world-restart` and nothing stronger. It is disabled while a
        // `recreate` is pending, so reaching here with one is not expected --
        // the explicit check keeps that true if the gate ever changes.
        if (e.event === "done" && restartState.apply === "world-restart") {
          clearApplyNeeded();
        }
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
      restartState.restarting = false;
      busy = false;
      await refresh();
    }
  }
</script>

<section class="content">
  <header class="bar">
    <h2>Home</h2>
    <button onclick={refresh} disabled={refreshing || busy}>Refresh</button>
  </header>

  <!-- An install in flight is answered ABOVE the `serverStatus.detail` gate, not
       inside the verdict chain, and that placement is the whole point: on a
       FIRST install there is no compose file yet, so `server-detail` errors,
       `detail` stays null, and this page fell through to the "Couldn't read
       world status" card below -- for the entire multi-hour build. The status
       chip and this card share statusLabel() so the two can never disagree
       about which state wins. -->
  {#if installProgress.active}
    {@const s = statusLabel(serverStatus.detail?.verdict ?? null, restartState.restarting, installProgress)}
    <div class="card status-card">
      <div class="card-title">
        <span class="dot status-dot mid"></span>
        <strong>{s.label}</strong>
      </div>
      <p class="muted">
        {installDetailText(installProgress) ??
          "Installing the server. This takes hours and the Library page has the full output — you can leave this window open, or close it and come back."}
      </p>
    </div>
  {:else if serverStatus.detail}
    {@const d = serverStatus.detail}
    <div
      class="card status-card"
      class:warn={!restartState.restarting && d.verdict === "soap_unreachable"}
      class:crash={!restartState.restarting && d.verdict === "crashed"}
    >
      <div class="card-title">
        <span
          class="dot status-dot"
          class:on={!restartState.restarting && d.verdict === "online"}
          class:mid={restartState.restarting || d.verdict === "starting"}
          class:bad={!restartState.restarting && d.verdict === "soap_unreachable"}
          class:off={!restartState.restarting && d.verdict === "stopped"}
          class:crash={!restartState.restarting && d.verdict === "crashed"}
        ></span>
        <strong class:crash-text={!restartState.restarting && d.verdict === "crashed"}>
          {#if restartState.restarting}Restarting…
          {:else if d.verdict === "online"}World is up
          {:else if d.verdict === "starting"}Starting up…
          {:else if d.verdict === "soap_unreachable"}World is running, but the launcher can't reach it
          {:else if d.verdict === "crashed"}Server crashed
          {:else}Server is stopped{/if}
        </strong>
      </div>
      {#if restartState.restarting}
        <p class="muted">Restarting — this takes a minute or two while the world reloads.</p>
      {:else if d.verdict === "online"}
        <div class="stats">
          <span>Players online: <strong>{humansOnline ?? "?"}</strong></span>
          <span>Uptime: <strong>{d.soap.uptime ?? "?"}</strong></span>
          <span>Update time: <strong>{d.soap.mean_ms ?? "?"} ms avg</strong></span>
          <span>Bots: <strong>{d.bots.online ?? "?"} / {d.bots.max ?? "?"}</strong></span>
        </div>
      {:else if d.verdict === "starting"}
        <p class="muted">The world is still loading — this takes a couple of minutes while bots spawn.</p>
      {:else if d.verdict === "soap_unreachable"}
        <div class="recover-row">
          {#if dockerRestartCardVisible(nativeStatus)}
            <p class="muted">
              If this persists for more than a minute, Docker's networking in the distro is likely stuck —
              restarting Docker inside dml-arch usually fixes it.
            </p>
            <button onclick={() => onnav?.("tools")} title="Opens the Restart Docker card on the Tools page">
              Restart Docker in the distro…
            </button>
          {:else}
            <p class="muted">
              If this persists for more than a minute, the world is running but isn't answering the
              launcher — restart the server below, and check that Docker itself is healthy if that
              doesn't help.
            </p>
          {/if}
        </div>
      {:else if d.verdict === "crashed"}
        <div class="recover-row">
          <p class="muted">
            The world server stopped unexpectedly{d.exit_code !== null ? ` (exit code ${d.exit_code})` : ""}.
            Recover starts the server again; the crash usually leaves no damage — characters
            save every 15 minutes.
          </p>
          <button class="primary" disabled={busy} onclick={() => act("start")}>Recover</button>
        </div>
      {:else}
        <p class="muted">Start the server below.</p>
      {/if}
    </div>
    {#if serverStatus.lastError}
      <p class="muted refresh-warn">Last refresh failed ({serverStatus.lastError}) — showing the last known status.</p>
    {/if}
  {:else if serverStatus.lastError}
    <div class="error-card"><strong>Couldn't read world status.</strong><p>{serverStatus.lastError}</p></div>
  {/if}

  {#if serverStatus.detail?.verdict === "online" && playersLoaded}
    <div class="card players-card">
      <div class="card-title"><strong>Players online</strong></div>
      {#if players.length === 0}
        <p class="muted">Nobody online right now.</p>
      {:else}
        <div class="players">
          {#each players as p (p.name)}
            <span class="player">
              <strong>{p.name}</strong>
              <span class="muted">lvl {p.level} {className(p.class)}</span>
            </span>
          {/each}
        </div>
      {/if}
    </div>
  {/if}

  <header class="bar"><h2>WoW server</h2></header>
  <!-- The pending-apply banner belongs HERE, not only on the pages that save:
       the restart buttons live in this section, so before this the user had to
       navigate away from the advice to reach the button -- and the advice never
       said WHICH button. See lib/apply-needed.ts. -->
  {#if restartState.needed}
    <div class="warn-card"><p>{bannerText(restartState.apply)}</p></div>
  {/if}
  {#if statusError}
    <!--
      A DEAD END UNTIL 2026-08-02. `games status` needs a live Docker engine,
      so a stopped Docker Desktop -- the single most common state on a machine
      that was just booted -- sent this branch, and this branch had no Start
      button. Home became read-only in exactly the situation it exists for,
      and Library was the only page that could start anything (it lists titles
      from DISK, so it never depended on the engine answering).

      Start is offered here because `games start` brings the engine up itself
      before composing, through the same path Library's button takes: the
      user's own terminal shows it -- "Docker engine is down. Starting Docker
      Desktop... Docker Desktop engine is ready." A separate "start Docker"
      affordance would be a second way to do what this button already does.
    -->
    <div class="error-card">
      <strong>Couldn't reach the DML backend.</strong>
      <p>{statusError}</p>
      <div class="recover-row">
        <p class="muted">
          If Docker Desktop isn't running yet, Start will launch it and then start the server.
        </p>
        <button class="primary" disabled={busy} onclick={() => act("start")}>Start</button>
      </div>
    </div>
  {:else if containerState}
    <div class="card server-card">
      <div class="row">
        <button class="expander" onclick={() => (expanded = !expanded)} aria-expanded={expanded}>
          <span class="chev">{expanded ? "▾" : "▸"}</span>
          <span class="dot" class:on={containerState === "running"} class:off={containerState !== "running"}></span>
          {WOW_ID}
        </button>
        {#if unboundTag}
          <span class="addon-tag" class:warn={unboundTag.tone === "warn"} title={unboundTag.detail}>
            {unboundTag.text}
          </span>
        {/if}
        <div>
          {#if containerState === "running"}
            <button disabled={busy} onclick={() => act("stop")}>Stop</button>
            <button
              disabled={busy || featureLocked("restart")}
              title={featureLocked("restart") ? LOCKED_HINT : undefined}
              onclick={() => act("restart")}
            >
              Restart
            </button>
            <button
              disabled={busy || featureLocked("world-restart") || fastRestartBlocked !== ""}
              title={featureLocked("world-restart")
                ? LOCKED_HINT
                : fastRestartBlocked !== ""
                  ? fastRestartBlocked
                  : "Faster: restarts only the world server. Does NOT apply settings changes — use Restart for that."}
              onclick={worldRestart}
            >
              Restart world only
            </button>
          {:else}
            <button class="primary" disabled={busy} onclick={() => act("start")}>Start</button>
          {/if}
        </div>
      </div>
      {#if containerState === "running"}
        <label
          class="saveall-opt"
          title={saveallLocked
            ? LOCKED_HINT
            : "On (safe): saves every character before restarting. Off: skips that extra save for a faster restart — the graceful stop still saves your characters on shutdown, so only turn this off if restarts feel slow."}
        >
          <input
            type="checkbox"
            checked={saveallBoxChecked(saveallPref.saveBeforeRestart, saveallLocked)}
            disabled={busy || saveallLocked}
            onchange={(e) => setSaveBeforeRestart(e.currentTarget.checked)}
          />
          Save characters before restart <span class="opt-sub">(safer; off = faster)</span>
        </label>
        <!-- The incident's other half: the risky state was invisible. While
             the skip is actually armed, say so in amber right under the box. -->
        {#if skipSaveall}
          <p class="saveall-warn">⚠ Faster mode — characters will NOT be saved before a restart (resets on next launch).</p>
        {/if}
      {/if}
      {#if expanded}
        <div class="health">
          {#if serverStatus.detail}
            {@const d = serverStatus.detail}
            {#each d.containers as c (c.name)}
              <div class="hrow">
                <span class="dot" class:on={c.state === "running"} class:off={c.state !== "running"}></span>
                <span class="hname">{ROLE_LABELS[c.role] ?? c.name}</span>
                <span class="hval">{c.state === "absent" ? "not created" : c.status || c.state}</span>
              </div>
            {/each}
            {#if d.verdict === "online"}
              <div class="hrow"><span class="hname">Version</span><span class="hval">{d.soap.version ?? "?"}</span></div>
              <div class="hrow"><span class="hname">Uptime</span><span class="hval">{d.soap.uptime ?? "?"}</span></div>
              <div class="hrow"><span class="hname">Players online</span><span class="hval">{humansOnline ?? "?"}</span></div>
              <div class="hrow"><span class="hname">Bots online</span><span class="hval">{d.bots.online ?? "?"} of {d.bots.max ?? "?"} max</span></div>
              <!-- The raw SOAP figure, kept ONLY here and honestly labelled:
                   `.server info`'s "Connected players" counts bot sessions
                   too, which is exactly why it can no longer be the "Players
                   online" number above. -->
              <div class="hrow"><span class="hname">Sessions (incl. bots)</span><span class="hval">{d.soap.players ?? "?"}</span></div>
              <div class="hrow">
                <span class="hname">World update time</span>
                <span class="hval">{d.soap.mean_ms ?? "?"} ms mean · {d.soap.median_ms ?? "?"} ms median</span>
              </div>
            {/if}
            <div class="hrow">
              <span class="hname">Ports</span>
              <span class="hval">
                game {d.ports.world ?? "?"} · auth {d.ports.auth ?? "?"} · SOAP {d.ports.soap ?? "?"} · DB {d.ports.db ?? "?"}
              </span>
            </div>
            <div class="hrow">
              <span class="hname">SOAP</span>
              <span class="hval">
                {d.soap.reachable ? "reachable" : "unreachable"}{d.soap.auth_ok === false
                  ? " — authentication failing, check ~/.dml/soap.env"
                  : ""}
                <button class="linky" onclick={toggleSoapPass}>
                  {soapPassShown ? "Hide account" : "Show account"}
                </button>
                <!-- The way back into the account step, in the one row that
                     already reports the problem. "Later" on the fallback card
                     only HIDES it, and nothing else re-raises it: the gave_up
                     verdict that would is behind a settled-once latch, and the
                     Library re-check that used to be the second route was
                     deleted with the old surface. Without a control here,
                     "Later" is a one-way door out of GM Tools, My Party and the
                     console. Gated on `needed` alone -- the card is unresolved
                     either way, and it renders at the bottom of the window, so
                     pointing at it is useful even when it is already there. -->
                {#if soapSetupState.needed}
                  <button class="linky" onclick={showSoapSetup}>Set up server access…</button>
                {/if}
                {#if soapPassShown && soapCreds}
                  {#if !soapCreds.configured}
                    <!-- Says what IS true and promises only what this app will
                         actually do. The old copy ("the launcher sets this up
                         automatically") is a promise it may never keep:
                         autosetup fires only when SOAP is reachable AND auth is
                         failing, so with the server down, or with a hand-made
                         admin account that works, nothing is ever created and
                         the user waits for a step that is not coming. -->
                    <span class="creds muted">
                      Nothing saved in ~/.dml/soap.env — the launcher is trying the built-in default
                      account. If the server refuses it, the launcher creates a proper one.
                    </span>
                  {:else}
                    <span class="creds">
                      {soapCreds.user} / <code>{soapCreds.pass ?? "?"}</code>
                    </span>
                  {/if}
                {/if}
              </span>
            </div>
          {:else}
            <p class="muted">No health data — hit Refresh.</p>
          {/if}
        </div>
      {/if}
    </div>
  {/if}

  {#if buf.show}
    <Terminal state={buf.term} onclear={() => clearBuf("home")} logName="dml-home" />
  {/if}
</section>

<style>
  .content { padding: 20px 24px; overflow-y: auto; display: flex; flex-direction: column; gap: 16px; }
  .bar { display: flex; justify-content: space-between; align-items: center; }
  .bar h2 { margin: 0; font-size: 18px; }
  .card { background: #0d1117; border: 1px solid #30363d; border-radius: 8px; padding: 14px 16px; display: flex; justify-content: space-between; align-items: center; gap: 16px; flex-wrap: wrap; }
  .card.warn { border-color: #f85149; }
  /* Distinct crash styling (Batch 2 F8): red border + red title, pulsing dot. */
  .card.crash { border-color: #f85149; background: #160b0e; }
  .crash-text { color: #f85149; }
  .recover-row { display: flex; align-items: center; gap: 14px; flex-wrap: wrap; }
  .server-card { flex-direction: column; align-items: stretch; }
  /* "Save characters before restart" option, under the lifecycle buttons. */
  .saveall-opt { display: flex; align-items: center; gap: 7px; margin: 10px 2px 0; font-size: 12.5px; color: #8b949e; cursor: pointer; user-select: none; }
  .saveall-opt input { cursor: pointer; margin: 0; }
  .saveall-opt input:disabled { cursor: default; }
  .saveall-opt .opt-sub { color: #6e7681; }
  /* Amber = the card's existing "mid/attention" color (.dot.mid). */
  .saveall-warn { margin: 6px 2px 0; font-size: 12.5px; color: #d29922; }
  .row { display: flex; justify-content: space-between; align-items: center; gap: 16px; }
  .card-title { display: flex; align-items: center; gap: 8px; font-weight: 600; }
  .expander { background: none; border: none; padding: 0; display: flex; align-items: center; gap: 8px; font-weight: 600; font-size: inherit; color: inherit; cursor: pointer; }
  .chev { color: #8b949e; width: 12px; }
  .dot { width: 9px; height: 9px; border-radius: 50%; display: inline-block; flex-shrink: 0; }
  /* The add-on tag sits with the server name, not in the actions: it is a
     fact ABOUT this server, not something to click. */
  .addon-tag { font-size: 11px; padding: 2px 7px; border-radius: 10px; border: 1px solid #2ea04326;
               background: #2ea0431a; color: #3fb950; white-space: nowrap; }
  .addon-tag.warn { border-color: #d2992240; background: #d299221a; color: #d29922; }
  .dot.on { background: #3fb950; }
  .dot.off { background: #6e7681; }
  .dot.mid { background: #d29922; }
  .dot.bad { background: #f85149; }
  /* The top overview card's dot tracks live verdict colors (Round Q): online
     green, starting/restarting amber (pulsing), stopped red, soap_unreachable
     orange. Scoped to .status-dot so the health panel's per-container
     running/not-running dots above keep their original green/gray meaning. */
  .dot.status-dot.on { background: #3fb950; }
  .dot.status-dot.mid { background: #d29922; animation: dot-pulse 1.4s ease-in-out infinite; }
  .dot.status-dot.off { background: #f85149; }
  .dot.status-dot.bad { background: #ffa657; }
  .dot.status-dot.crash { background: #f85149; animation: dot-pulse 0.9s ease-in-out infinite; }
  @keyframes dot-pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.35; } }
  .stats { display: flex; gap: 18px; flex-wrap: wrap; }
  .players-card { flex-direction: column; align-items: stretch; gap: 8px; }
  .players { display: flex; gap: 8px; flex-wrap: wrap; }
  .player { background: #161b22; border: 1px solid #30363d; border-radius: 12px; padding: 3px 12px; font-size: 13px; display: inline-flex; gap: 8px; align-items: baseline; }
  .health { margin-top: 12px; border-top: 1px solid #30363d; padding-top: 10px; display: flex; flex-direction: column; gap: 6px; }
  .hrow { display: flex; gap: 10px; align-items: center; font-size: 14px; }
  .hname { min-width: 150px; color: #8b949e; }
  .hval { color: #c9d1d9; }
  /* The "Show account" toggle: a link, not a button, because it reveals rather
     than acts -- the health panel's other rows are read-only too. */
  .linky {
    background: none;
    border: none;
    padding: 0;
    margin-left: 0.4rem;
    color: var(--accent, #4a90d9);
    font: inherit;
    font-size: 0.8rem;
    text-decoration: underline;
    cursor: pointer;
  }
  .creds { display: block; font-size: 0.8rem; opacity: 0.85; }
  button { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 14px; cursor: pointer; }
  button.primary { background: #238636; border-color: #2ea043; color: white; }
  button:disabled { opacity: 0.5; cursor: default; }
  .muted { color: #8b949e; margin: 0; }
  .refresh-warn { font-size: 12.5px; }
  .error-card { background: #161b22; border: 1px solid #f85149; border-radius: 8px; padding: 12px 16px; }
  .warn-card { background: #161b22; border: 1px solid #d29922; border-radius: 8px; padding: 12px 16px; }
</style>
