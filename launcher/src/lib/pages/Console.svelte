<script lang="ts">
  import { onMount, tick } from "svelte";
  import { wowConsoleTail, wowConsoleSend, saveTextFile } from "$lib/api";
  import { featureLocked, LOCKED_HINT } from "$lib/features.svelte";
  import { consoleStore } from "$lib/term-store.svelte";

  let available = $state(true);
  let lines: string[] = $state([]);
  let tailError: string | null = $state(null);
  let refreshing = $state(false);
  let auto = $state(true);

  let command = $state("");
  let sending = $state(false);

  let logEl: HTMLDivElement | undefined = $state();

  async function refreshLogs() {
    if (refreshing) return;
    refreshing = true;
    const nearBottom =
      !logEl || logEl.scrollHeight - logEl.scrollTop - logEl.clientHeight < 40;
    try {
      const t = await wowConsoleTail();
      available = t.available;
      lines = t.lines;
      tailError = null;
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      tailError = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    } finally {
      refreshing = false;
    }
    if (nearBottom) {
      await tick();
      if (logEl) logEl.scrollTop = logEl.scrollHeight;
    }
  }
  onMount(refreshLogs);

  $effect(() => {
    if (!auto) return;
    const t = setInterval(() => {
      if (!refreshing && !sending) refreshLogs();
    }, 3000);
    return () => clearInterval(t);
  });

  async function send() {
    const cmd = command.trim();
    if (!cmd || sending) return;
    sending = true;
    try {
      const r = await wowConsoleSend(cmd);
      consoleStore.hist = [...consoleStore.hist, { command: cmd, result: r.result, error: null }];
      command = "";
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      consoleStore.hist = [
        ...consoleStore.hist,
        {
          command: cmd,
          result: null,
          error: `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`,
        },
      ];
    } finally {
      sending = false;
      await refreshLogs();
    }
  }

  function clearHistory() {
    consoleStore.hist = [];
  }

  let saveErr: string | null = $state(null);
  async function downloadLog() {
    saveErr = null;
    const stamp = new Date().toISOString().slice(0, 19).replace(/[:T]/g, "-");
    const parts: string[] = [lines.join("\n"), ""];
    for (const h of consoleStore.hist) {
      parts.push(`> ${h.command}`, h.error ?? h.result ?? "");
    }
    try {
      await saveTextFile(`dml-console-${stamp}.log`, parts.join("\n"));
    } catch (e) {
      saveErr = String(e);
    }
  }
</script>

<section class="content">
  <header class="bar">
    <h2>Console</h2>
    <div class="controls">
      <label class="autolabel">
        <input type="checkbox" bind:checked={auto} /> Auto-refresh
      </label>
      <button onclick={refreshLogs} disabled={refreshing}>Refresh</button>
      <button onclick={clearHistory} disabled={sending}>Clear</button>
      <button onclick={downloadLog}>Download</button>
      {#if saveErr}<span class="save-err">save failed: {saveErr}</span>{/if}
    </div>
  </header>

  {#if tailError}
    <div class="error-card"><strong>Couldn't read the server log.</strong><p>{tailError}</p></div>
  {:else if !available}
    <p class="muted">No server logs — is the server installed?</p>
  {:else}
    <div class="log" bind:this={logEl}>
      {#each lines as line, i (i)}
        <div class="logline">{line}</div>
      {/each}
    </div>
  {/if}

  {#if consoleStore.hist.length > 0}
    <div class="history">
      {#each consoleStore.hist as h, i (i)}
        <div class="entry">
          <div class="cmd">&gt; {h.command}</div>
          {#if h.error}
            <pre class="reply err">{h.error}</pre>
          {:else}
            <pre class="reply">{h.result}</pre>
          {/if}
        </div>
      {/each}
    </div>
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
      placeholder="Console command, e.g. server info"
      bind:value={command}
      disabled={sending || featureLocked("console-send")}
      title={featureLocked("console-send") ? LOCKED_HINT : undefined}
    />
    <button
      class="primary"
      type="submit"
      disabled={sending || command.trim() === "" || featureLocked("console-send")}
      title={featureLocked("console-send") ? LOCKED_HINT : undefined}
    >
      Send
    </button>
  </form>
</section>

<style>
  .content { padding: 20px 24px; overflow: hidden; display: flex; flex-direction: column; gap: 14px; box-sizing: border-box; }
  .bar { display: flex; justify-content: space-between; align-items: center; }
  .bar h2 { margin: 0; font-size: 18px; }
  .controls { display: flex; gap: 10px; align-items: center; }
  .autolabel { color: #8b949e; font-size: 13px; display: flex; gap: 6px; align-items: center; }
  .save-err { color: #f85149; font-size: 12.5px; align-self: center; }
  .log { background: #0d1117; border: 1px solid #30363d; border-radius: 8px; padding: 10px 12px; font-family: Consolas, monospace; font-size: 12.5px; line-height: 1.45; overflow-y: auto; flex: 1; min-height: 200px; }
  .logline { white-space: pre-wrap; word-break: break-all; color: #c9d1d9; }
  .sendrow { display: flex; gap: 8px; flex-shrink: 0; }
  .sendrow input { flex: 1; background: #0d1117; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 8px 10px; font-family: Consolas, monospace; font-size: 13px; }
  .history { display: flex; flex-direction: column; gap: 10px; max-height: 22vh; overflow-y: auto; flex-shrink: 0; }
  .entry { border-left: 2px solid #30363d; padding-left: 10px; }
  .cmd { color: #58a6ff; font-family: Consolas, monospace; font-size: 13px; }
  .reply { margin: 4px 0 0; color: #c9d1d9; font-family: Consolas, monospace; font-size: 12.5px; white-space: pre-wrap; word-break: break-word; }
  .reply.err { color: #f85149; }
  button { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 14px; cursor: pointer; }
  button.primary { background: #238636; border-color: #2ea043; color: white; }
  button:disabled { opacity: 0.5; cursor: default; }
  .muted { color: #8b949e; margin: 0; }
  .error-card { background: #161b22; border: 1px solid #f85149; border-radius: 8px; padding: 12px 16px; }
</style>
