<script lang="ts">
  import { tick, onMount } from "svelte";
  import { gamesInstall, gamesInstallInput, gamesInstallCancel, type InstallEvent } from "$lib/api";

  let { id, onExit }: { id: string; onExit: (code: number) => void } = $props();

  // Strip ANSI escape sequences (cursor moves, colors) out of installer
  // output before it lands in the scrollback -- the CLI runs an interactive
  // child process and some tools (npm, git) emit control codes even when
  // piped.
  const ANSI_RE = /\x1b\[[0-9;?]*[A-Za-z]/g;

  let output = $state("");
  let exited = $state(false);
  let exitCode: number | null = $state(null);
  let note: string | null = $state(null);
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
    output += text;
    tick().then(() => {
      if (nearBottom && box) box.scrollTop = box.scrollHeight;
    });
  }

  async function run() {
    try {
      await gamesInstall(id, (e: InstallEvent) => {
        if (e.event === "chunk") {
          append(e.text ?? "");
        } else if (e.event === "exit") {
          const code = e.code ?? -1;
          exitCode = code;
          exited = true;
          note = code === 0 ? "Installer finished (exit 0)." : `Installer failed (exit ${code}).`;
          onExit(code);
        }
      });
    } catch (e) {
      showErr(e);
      exited = true;
      exitCode = -1;
      onExit(-1);
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
  {#if error}
    <div class="error-card"><p>{error}</p></div>
  {/if}

  <div class="scrollback" bind:this={box}>{output}</div>

  {#if note}
    <div class="exit-note {exitCode === 0 ? 'ok' : 'err'}">{note}</div>
  {/if}

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
      disabled={sending || exited}
    />
    <button class="primary" type="submit" disabled={sending || exited || command.trim() === ""}>
      Send
    </button>
  </form>

  {#if !exited}
    <div class="row">
      {#if !confirmingCancel}
        <button onclick={cancel} disabled={cancelling}>Cancel install</button>
      {:else}
        <span class="warn-text">Cancelling mid-install can leave a partial install behind. Cancel anyway?</span>
        <button onclick={cancel} disabled={cancelling}>Confirm</button>
        <button onclick={() => (confirmingCancel = false)} disabled={cancelling}>Back</button>
      {/if}
    </div>
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
    max-height: 40vh;
  }
  .exit-note { font-size: 13px; font-weight: 600; }
  .exit-note.ok { color: #3fb950; }
  .exit-note.err { color: #f85149; }
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
