<script lang="ts">
  import { onMount, tick } from "svelte";
  import { wowConsoleTail, wowConsoleSend, saveTextFile } from "$lib/api";
  import { featureLocked, LOCKED_HINT } from "$lib/features.svelte";
  import { consoleStore, tailAfterAnchor } from "$lib/term-store.svelte";
  import { serverStatus, containersExist } from "$lib/server-status.svelte";
  import { stepRecall, logSeverity, consoleCommands, commandSuggestions } from "$lib/console-input";
  import { CORE_COMMANDS } from "$lib/gm-commands";

  let available = $state(true);
  let lines: string[] = $state([]);
  let tailError: string | null = $state(null);
  let refreshing = $state(false);
  let auto = $state(true);

  let command = $state("");
  let sending = $state(false);

  // Command favorites (Batch 3 F11c): starred console commands, persisted in
  // localStorage, rendered as chips above the input. Clicking a chip FILLS
  // the input (never auto-sends). The star next to the send box adds/removes
  // the current input text. Guarded storage access, same idiom as
  // features.svelte.ts (vitest's node env has no localStorage).
  const FAVS_KEY = "dml.consoleFavs";
  function readFavs(): string[] {
    try {
      if (typeof localStorage === "undefined") return [];
      const raw = localStorage.getItem(FAVS_KEY);
      if (!raw) return [];
      const arr = JSON.parse(raw);
      return Array.isArray(arr) ? arr.filter((x) => typeof x === "string") : [];
    } catch {
      return [];
    }
  }
  function writeFavs(favs: string[]): void {
    try {
      if (typeof localStorage !== "undefined") localStorage.setItem(FAVS_KEY, JSON.stringify(favs));
    } catch {
      // In-memory list still applies this session.
    }
  }
  let favs: string[] = $state(readFavs());
  const isFav = $derived(favs.includes(command.trim()));
  function toggleFav() {
    const cmd = command.trim();
    if (!cmd) return;
    favs = favs.includes(cmd) ? favs.filter((f) => f !== cmd) : [...favs, cmd];
    writeFavs(favs);
  }

  // --- Input helpers: history recall (F2) + autocomplete (F3) --------------
  // `histCursor`/`histDraft` are plain (non-reactive) locals -- nothing renders
  // them; they only thread state between keydowns. Recall + suggestion logic is
  // pure (console-input.ts) so it's unit-tested away from the DOM.
  let inputEl: HTMLInputElement | undefined = $state();
  let histCursor: number | null = null;
  let histDraft = "";

  // Autocomplete pool: the GM cheat-sheet command stems plus the user's saved
  // favorites. The catalog is static; favorites are reactive.
  const catalogStems = consoleCommands(CORE_COMMANDS);
  const pool = $derived([...catalogStems, ...favs]);
  const suggestions = $derived(commandSuggestions(pool, command));
  let suggestOpen = $state(false);
  let suggIndex = $state(-1);
  const showSuggest = $derived(suggestOpen && suggestions.length > 0);

  function caretToEnd() {
    void tick().then(() => {
      inputEl?.focus();
      inputEl?.setSelectionRange(command.length, command.length);
    });
  }

  function acceptSuggestion(s: string) {
    // Complete in place; trailing space both readies any args and (being an
    // exact-match prefix) collapses the dropdown on the next derived pass.
    command = `${s} `;
    suggestOpen = false;
    suggIndex = -1;
    histCursor = null;
    caretToEnd();
  }

  function onCommandKey(e: KeyboardEvent) {
    // While the suggestion dropdown is open, the arrows/enter/tab drive it;
    // otherwise the arrows do history recall.
    if (showSuggest) {
      if (e.key === "ArrowDown") {
        e.preventDefault();
        suggIndex = (suggIndex + 1) % suggestions.length;
        return;
      }
      if (e.key === "ArrowUp") {
        e.preventDefault();
        suggIndex = suggIndex <= 0 ? suggestions.length - 1 : suggIndex - 1;
        return;
      }
      if (e.key === "Tab") {
        e.preventDefault();
        acceptSuggestion(suggestions[suggIndex >= 0 ? suggIndex : 0]);
        return;
      }
      if (e.key === "Enter" && suggIndex >= 0) {
        // A highlighted suggestion completes instead of sending; with none
        // highlighted, Enter falls through to the form's submit (send).
        e.preventDefault();
        acceptSuggestion(suggestions[suggIndex]);
        return;
      }
      if (e.key === "Escape") {
        e.preventDefault();
        suggestOpen = false;
        suggIndex = -1;
        return;
      }
    }

    if (e.key !== "ArrowUp" && e.key !== "ArrowDown") return;
    const hist = consoleStore.hist.map((h) => h.command);
    if (hist.length === 0) return;
    e.preventDefault();
    const dir = e.key === "ArrowUp" ? "up" : "down";
    // stepRecall captures the live draft whenever we're not already walking
    // history, so a Down press with no prior Up is a no-op instead of wiping
    // the typed command with a stale draft.
    const r = stepRecall(hist, histCursor, dir, command, histDraft);
    command = r.value;
    histCursor = r.cursor;
    histDraft = r.draft;
    caretToEnd();
  }

  // Real typing (not a programmatic recall/accept assignment) reopens the
  // suggestion list and drops out of recall.
  function onCommandInput() {
    histCursor = null;
    suggestOpen = true;
    suggIndex = -1;
  }

  let logEl: HTMLDivElement | undefined = $state();

  async function refreshLogs() {
    if (refreshing) return;
    refreshing = true;
    const nearBottom =
      !logEl || logEl.scrollHeight - logEl.scrollTop - logEl.clientHeight < 40;
    try {
      const t = await wowConsoleTail();
      available = t.available;
      if (consoleStore.clearAnchor) {
        const after = tailAfterAnchor(t.lines, consoleStore.clearAnchor);
        if (after === null) {
          // Anchor scrolled out of the tail window -- everything fetched is
          // newer than the clear point.
          consoleStore.clearAnchor = null;
          lines = t.lines;
        } else {
          lines = after;
        }
      } else {
        lines = t.lines;
      }
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
      histCursor = null;
      histDraft = "";
      suggestOpen = false;
      suggIndex = -1;
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

  function clearConsole() {
    // "Clear" clears what the user SEES: the log view (via the anchor --
    // the tail refills from the server every poll, so an anchor marks the
    // clear point and only newer lines render) plus the command history.
    consoleStore.hist = [];
    consoleStore.clearAnchor = lines.slice(-3);
    lines = [];
    histCursor = null;
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
      <button onclick={clearConsole} disabled={sending}>Clear</button>
      <button onclick={downloadLog}>Download</button>
      {#if saveErr}<span class="save-err">save failed: {saveErr}</span>{/if}
    </div>
  </header>

  {#if tailError}
    <div class="error-card"><strong>Couldn't read the server log.</strong><p>{tailError}</p></div>
  {:else if !available}
    {#if containersExist(serverStatus.detail)}
      <p class="muted">The server looks stopped — start it from Home to see live logs.</p>
    {:else}
      <p class="muted">No server logs — is the server installed?</p>
    {/if}
  {:else}
    <div class="log" bind:this={logEl}>
      {#each lines as line, i (i)}
        <div class="logline {logSeverity(line)}">{line}</div>
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

  {#if favs.length > 0}
    <div class="favrow">
      {#each favs as f (f)}
        <button class="fav-chip" title="Fill the command box (does not send)" onclick={() => (command = f)}>{f}</button>
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
    <div class="inputwrap">
      {#if showSuggest}
        <div class="suggest" role="listbox">
          {#each suggestions as s, i (s)}
            <button
              type="button"
              role="option"
              aria-selected={i === suggIndex}
              class="suggest-item"
              class:active={i === suggIndex}
              onmousedown={(e) => e.preventDefault()}
              onclick={() => acceptSuggestion(s)}
            >{s}</button>
          {/each}
        </div>
      {/if}
      <input
        type="text"
        placeholder="Console command, e.g. server info"
        bind:value={command}
        bind:this={inputEl}
        onkeydown={onCommandKey}
        oninput={onCommandInput}
        onblur={() => (suggestOpen = false)}
        disabled={sending || featureLocked("console-send")}
        title={featureLocked("console-send") ? LOCKED_HINT : undefined}
      />
    </div>
    <button
      type="button"
      class="starbtn"
      class:faved={isFav}
      onclick={toggleFav}
      disabled={command.trim() === ""}
      title={isFav ? "Remove this command from favorites" : "Save this command as a favorite"}
    >
      {isFav ? "★" : "☆"}
    </button>
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
  .logline.warn { color: #d29922; }
  .logline.error { color: #f85149; }
  .sendrow { display: flex; gap: 8px; flex-shrink: 0; }
  .favrow { display: flex; gap: 6px; flex-wrap: wrap; flex-shrink: 0; }
  .fav-chip { background: #161b22; border: 1px solid #30363d; border-radius: 12px; color: #c9d1d9; font-family: Consolas, monospace; font-size: 12px; padding: 3px 10px; cursor: pointer; }
  .fav-chip:hover { border-color: #58a6ff; }
  .starbtn { font-size: 16px; padding: 6px 10px; }
  .starbtn.faved { color: #d29922; border-color: #d29922; }
  .inputwrap { position: relative; flex: 1; display: flex; }
  .sendrow input { flex: 1; background: #0d1117; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 8px 10px; font-family: Consolas, monospace; font-size: 13px; }
  .suggest { position: absolute; bottom: calc(100% + 4px); left: 0; right: 0; z-index: 5; display: flex; flex-direction: column; max-height: 200px; overflow-y: auto; background: #161b22; border: 1px solid #30363d; border-radius: 6px; box-shadow: 0 6px 18px rgba(0, 0, 0, 0.45); }
  .suggest-item { text-align: left; background: transparent; border: none; border-radius: 0; color: #c9d1d9; font-family: Consolas, monospace; font-size: 13px; padding: 6px 10px; cursor: pointer; }
  .suggest-item:hover, .suggest-item.active { background: #1f2937; color: #f0f6fc; }
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
