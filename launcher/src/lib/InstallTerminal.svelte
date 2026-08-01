<script lang="ts">
  import { tick, onMount } from "svelte";
  import { gamesInstall, gamesInstallInput, gamesInstallCancel, saveTextFile, type InstallEvent } from "$lib/api";
  import { featureLocked, LOCKED_HINT } from "$lib/features.svelte";
  import { installStore, claimInstallInvoke } from "$lib/term-store.svelte";
  import { taskbarBusy, taskbarIdle } from "$lib/taskbar";

  // `runner` defaults to gamesInstall (Library's game installs) but can be
  // swapped for another Channel-streamed InstallEvent call with the same
  // shape -- e.g. Tools.svelte passes a closure around toolInstall() for
  // `tool:`-prefixed installStore.ids (Round Q). Everything else here
  // (claimInstallInvoke, cancel/reply wiring, exit handling) is shared
  // unchanged between callers.
  // `lockFlag` is the feature flag that gates the reply/cancel controls. It
  // MUST follow the runner: the default game-install path is "title-install",
  // but the URL-install flow (Library) and the tool-install flow (Tools) run
  // their own execs behind different flags. Hardcoding "title-install" meant a
  // URL install whose own flag was flipped tested-but-title-install-still-
  // locked left the reply box and Cancel disabled -- the single global install
  // slot stuck on an unanswerable prompt until an app restart.
  // `interactive` says whether the run behind `runner` can be TYPED AT and
  // KILLED. Default true, because that is what every existing caller wraps: an
  // interactive bash installer in a pty.
  //
  // The native engine is neither. It asks no questions (nothing to reply to)
  // and its git/docker children are the launcher's own, so `taskkill /F /T` on
  // our pid would close the app -- the backend refuses both with
  // NOT_INTERACTIVE and NOT_CANCELLABLE. Rendering the controls anyway meant
  // the two-step confirm warned "Cancelling mid-install can leave a partial
  // install behind. Cancel anyway?", the user accepted that outcome, and then
  // the app refused -- leaving an error card parked above the scrollback for
  // the rest of an hours-long build.
  let {
    id,
    onExit,
    runner = gamesInstall,
    lockFlag = "title-install",
    interactive = true,
  }: {
    id: string;
    onExit: (code: number) => void;
    runner?: (id: string, onEvent: (e: InstallEvent) => void) => Promise<void>;
    lockFlag?: string;
    interactive?: boolean;
  } = $props();

  // Strip ANSI escape sequences (cursor moves, colors) out of installer
  // output before it lands in the scrollback -- the CLI runs an interactive
  // child process and some tools (npm, git) emit control codes even when
  // piped.
  const ANSI_RE = /\x1b\[[0-9;?]*[A-Za-z]/g;

  // `exited` used to be local $state, reset to false on every mount -- fine
  // while the panel and its backing gamesInstall() call shared one component
  // instance, but nav-away-and-back destroys and recreates this component
  // while the interactive install session (and the backend's single global
  // install slot) keeps going. Deriving straight off installStore.running
  // means a remounted instance immediately reflects the session's true
  // state instead of starting falsely "exited" -- which would hide Cancel
  // and disable the reply input for a session that's actually still alive.
  const exited = $derived(!installStore.running);
  const note = $derived(
    exited && installStore.exitCode !== null
      ? installStore.exitCode === 0
        ? "Installer finished (exit 0)."
        : `Installer failed (exit ${installStore.exitCode}).`
      : null,
  );

  let error: string | null = $state(null);

  let command = $state("");
  let sending = $state(false);

  let confirmingCancel = $state(false);
  let cancelling = $state(false);

  let box: HTMLDivElement | undefined = $state();

  function showErr(e: unknown) {
    const err = e as { message?: string; hint?: string };
    error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
  }

  // Sticky autoscroll: same 40px "near bottom" pattern as Console.svelte --
  // only follow new output if the user hadn't scrolled up to read back.
  function append(text: string) {
    const nearBottom = !box || box.scrollHeight - box.scrollTop - box.clientHeight < 40;
    // Strip ANSI sequences, then collapse carriage-return progress redraws: per-chunk,
    // keep only text after the last lone \r on each line (final redraw wins).
    // Best-effort per chunk; redraws split across chunks may leave occasional stale lines.
    text = text.replace(ANSI_RE, "").replace(/^.*\r(?!\n)/gm, "");
    installStore.text += text;
    tick().then(() => {
      if (nearBottom && box) box.scrollTop = box.scrollHeight;
    });
  }

  function clearOutput() {
    installStore.text = "";
  }

  async function downloadOutput() {
    const stamp = new Date().toISOString().slice(0, 19).replace(/[:T]/g, "-");
    try {
      await saveTextFile(`dml-install-${stamp}.log`, installStore.text);
    } catch (e) {
      showErr(e);
    }
  }

  // Only the mount that actually claims this session's nonce may invoke
  // games_install -- a nav-away-and-back remount reuses the SAME nonce (see
  // installStore.nonce / claimInstallInvoke in term-store.svelte.ts), and
  // the backend allows exactly one concurrent install. A second invoke
  // there would hit its BUSY error and (pre-fix) falsely flip a still-
  // running session to "exited", hiding Cancel for good. A remount that
  // loses the claim just renders installStore reactively instead.
  // The taskbar cue is taken HERE, not at Library's startInstall(), because
  // this function is where the stream actually begins and ends -- an install
  // is the longest op in the app (tens of minutes) and the one a minimized
  // user most needs a cue for. Placement rules, both load-bearing:
  //   * AFTER the claim guard. A nav-away-and-back remount loses the claim and
  //     returns early; a busy taken above that return would never be released
  //     and would pin the cue on for the rest of the session.
  //   * idle in a finally around the awaited runner(), which is the true end
  //     of the stream: games_install/url_install/tool_install all .await their
  //     spawn_blocking, so the invoke promise settles only after the child
  //     exits (and after the exit event was sent) -- and it settles on the
  //     reject path too (e.g. the backend's BUSY error). Unmounting this
  //     panel by navigating away does NOT abort this async function, so the
  //     pair stays balanced across the exact nav the cue exists for.
  async function run() {
    if (!claimInstallInvoke(installStore.nonce)) return;
    taskbarBusy();
    try {
      await runner(id, (e: InstallEvent) => {
        if (e.event === "chunk") {
          append(e.text ?? "");
        } else if (e.event === "exit") {
          const code = e.code ?? -1;
          installStore.exitCode = code;
          installStore.running = false;
          onExit(code);
        }
      });
    } catch (e) {
      showErr(e);
      installStore.exitCode = -1;
      installStore.running = false;
      onExit(-1);
    } finally {
      taskbarIdle();
    }
  }
  onMount(run);

  async function send() {
    const text = command.trim();
    if (!text || sending || exited) return;
    sending = true;
    try {
      await gamesInstallInput(text);
      command = "";
    } catch (e) {
      showErr(e);
    } finally {
      sending = false;
    }
  }

  // Two-step: first click arms the confirm, second click (same handler)
  // actually cancels -- matches ModuleManager's rebuild()/removeModule().
  function cancel() {
    if (!confirmingCancel) {
      confirmingCancel = true;
      return;
    }
    confirmingCancel = false;
    doCancel();
  }

  async function doCancel() {
    cancelling = true;
    try {
      await gamesInstallCancel();
    } catch (e) {
      showErr(e);
    } finally {
      cancelling = false;
    }
  }
</script>

<div class="install-term">
  <div class="term-head">
    <button class="head-btn" onclick={clearOutput} disabled={!exited}>Clear</button>
    <button class="head-btn" onclick={downloadOutput} disabled={installStore.text === ""}>
      Download
    </button>
  </div>

  {#if error}
    <div class="error-card"><p>{error}</p></div>
  {/if}

  <div class="scrollback" bind:this={box}>{installStore.text}</div>

  {#if note}
    <div class="exit-note {installStore.exitCode === 0 ? 'ok' : 'err'}">{note}</div>
  {/if}

  {#if interactive}
  <form
    class="sendrow"
    onsubmit={(e) => {
      e.preventDefault();
      send();
    }}
  >
    <input
      type="text"
      placeholder="Reply to the installer…"
      bind:value={command}
      disabled={sending || exited || featureLocked(lockFlag)}
      title={featureLocked(lockFlag) ? LOCKED_HINT : undefined}
    />
    <button
      class="primary"
      type="submit"
      disabled={sending || exited || command.trim() === "" || featureLocked(lockFlag)}
      title={featureLocked(lockFlag) ? LOCKED_HINT : undefined}
    >
      Send
    </button>
  </form>
  {/if}

  {#if !exited}
    {#if interactive}
      <div class="row">
        {#if !confirmingCancel}
          <button
            onclick={cancel}
            disabled={cancelling || featureLocked(lockFlag)}
            title={featureLocked(lockFlag) ? LOCKED_HINT : undefined}
          >
            Cancel install
          </button>
        {:else}
          <span class="warn-text">Cancelling mid-install can leave a partial install behind. Cancel anyway?</span>
          <button onclick={cancel} disabled={cancelling}>Confirm</button>
          <button onclick={() => (confirmingCancel = false)} disabled={cancelling}>Back</button>
        {/if}
      </div>
    {:else}
      <!-- Static copy in place of a Cancel that would be refused. Deliberately
           does NOT claim closing the window stops the build: close-to-tray is
           ON by default, so the X hides the app and leaves the build running.
           Promising a stop we do not perform is the error this replaces. -->
      <p class="noncancel-note">
        This build keeps going on its own. Leaving this page is fine — it carries on, and
        running the install again later continues from the last finished step, reusing
        Docker's build cache, so nothing is paid for twice.
      </p>
    {/if}
  {/if}
</div>

<style>
  .install-term {
    background: #0d1117;
    border: 1px solid #30363d;
    border-radius: 8px;
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .scrollback {
    font-family: ui-monospace, Consolas, monospace;
    font-size: 12.5px;
    line-height: 1.45;
    color: #c9d1d9;
    white-space: pre-wrap;
    word-break: break-word;
    overflow-y: auto;
    min-height: 160px;
    max-height: calc(100vh - 260px);
  }
  .term-head {
    display: flex;
    gap: 8px;
    align-items: center;
    justify-content: flex-end;
  }
  .head-btn {
    background: transparent;
    color: #8b949e;
    border: 1px solid #30363d;
    border-radius: 4px;
    padding: 2px 8px;
    font-size: 11px;
    line-height: 1.4;
    cursor: pointer;
  }
  .head-btn:hover:not(:disabled) { color: #c9d1d9; border-color: #8b949e; }
  .head-btn:disabled { opacity: 0.5; cursor: default; }
  .exit-note { font-size: 13px; font-weight: 600; }
  .exit-note.ok { color: #3fb950; }
  .exit-note.err { color: #f85149; }
  .noncancel-note {
    margin: 0.6rem 0 0;
    font-size: 0.85rem;
    opacity: 0.8;
    line-height: 1.45;
  }
  .sendrow { display: flex; gap: 8px; }
  .sendrow input {
    flex: 1;
    background: #21262d;
    color: #c9d1d9;
    border: 1px solid #30363d;
    border-radius: 6px;
    padding: 8px 10px;
    font-family: ui-monospace, Consolas, monospace;
    font-size: 13px;
  }
  .row { display: flex; gap: 10px; align-items: center; flex-wrap: wrap; }
  .warn-text { color: #d29922; font-size: 13px; }
  button {
    background: #21262d;
    color: #c9d1d9;
    border: 1px solid #30363d;
    border-radius: 6px;
    padding: 6px 14px;
    cursor: pointer;
  }
  button.primary { background: #238636; border-color: #2ea043; color: white; }
  button:disabled { opacity: 0.5; cursor: default; }
  .error-card { background: #161b22; border: 1px solid #f85149; border-radius: 8px; padding: 12px 16px; }
  .error-card p { margin: 0; color: #f85149; font-size: 13px; }
</style>
