<script lang="ts">
  import type { TermState } from "./terminal-state";
  import { termText } from "./term-store.svelte";
  import { saveTextFile } from "./api";

  // Destructured to `termState` (not `state`): a local binding literally
  // named `state` collides with the `$state` rune (Svelte's legacy `$store`
  // auto-subscription syntax treats `$state(...)` as "subscribe to store
  // `state`" whenever a same-named variable is in scope), which svelte-check
  // flags as 3 hard errors. The external prop name (`state`) is unchanged.
  let {
    state: termState,
    onclear,
    logName = "dml",
  }: { state: TermState; onclear?: () => void; logName?: string } = $props();

  let box: HTMLDivElement | undefined = $state();
  let autoScroll = $state(true);
  let elapsed = $state(0);

  const running = $derived(termState.startedAt !== null && termState.finished === null);

  $effect(() => {
    if (!running) return;
    const t = setInterval(() => {
      if (termState.startedAt) elapsed = Math.floor((Date.now() - termState.startedAt) / 1000);
    }, 1000);
    return () => clearInterval(t);
  });

  // autoscroll on new lines unless the user scrolled up
  $effect(() => {
    void termState.totalLines;
    if (autoScroll && box) box.scrollTop = box.scrollHeight;
  });

  // Bring the terminal into view when a new run starts. `prevStarted` is a
  // plain (non-reactive) closure variable used purely as a previous-value
  // guard, so this fires once on the null->set transition rather than on
  // every subsequent line (which would fight the autoscroll effect above).
  let prevStarted: number | null = null;
  $effect(() => {
    const started = termState.startedAt;
    if (started !== null && prevStarted === null) box?.scrollIntoView({ block: "end" });
    prevStarted = started;
  });

  let saveErr: string | null = $state(null);
  async function download() {
    saveErr = null;
    const stamp = new Date().toISOString().slice(0, 19).replace(/[:T]/g, "-");
    try {
      await saveTextFile(`${logName}-${stamp}.log`, termText(termState));
    } catch (e) {
      saveErr = String(e);
    }
  }

  function onScroll() {
    if (!box) return;
    autoScroll = box.scrollTop + box.clientHeight >= box.scrollHeight - 8;
  }

  function jump() {
    autoScroll = true;
    if (box) box.scrollTop = box.scrollHeight;
  }

  const fmt = (s: number) =>
    `${String(Math.floor(s / 60)).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}`;
</script>

<div class="term">
  <div class="term-head">
    {#if running}
      <span class="spinner" aria-label="working"></span>
      <span class="runtime">{fmt(elapsed)}</span>
    {:else if termState.finished?.kind === "done"}
      <span class="ok">✔ complete</span>
    {:else if termState.finished?.kind === "error"}
      <span class="err">✖ {termState.finished.error.code}</span>
    {/if}
    {#if onclear}
      <button class="head-btn" onclick={onclear} disabled={running}>Clear</button>
    {/if}
    <button class="head-btn" onclick={download} disabled={termState.totalLines === 0}>
      Download
    </button>
    {#if saveErr}<span class="err">✖ save: {saveErr}</span>{/if}
  </div>

  <div class="term-body" bind:this={box} onscroll={onScroll}>
    {#each termState.sections as sec (sec.name + sec.status)}
      <details open={!sec.collapsed}>
        <summary class={sec.status}>
          {sec.name}
          {#if sec.status === "running"}<span class="spinner small"></span>{/if}
        </summary>
        {#each sec.lines as l}
          <div class="line {l.level}">{l.text}</div>
        {/each}
      </details>
    {/each}
    {#if termState.finished?.kind === "error"}
      <div class="line error">{termState.finished.error.message}</div>
      {#if termState.finished.error.hint}
        <div class="line hint">Hint: {termState.finished.error.hint}</div>
      {/if}
    {/if}
  </div>

  {#if !autoScroll}
    <button class="jump" onclick={jump}>Jump to latest ↓</button>
  {/if}
</div>

<style>
  .term {
    position: relative;
    display: flex;
    flex-direction: column;
    background: #0d1117;
    border: 1px solid #30363d;
    border-radius: 8px;
    min-height: 220px;
    max-height: calc(100vh - 220px);
    overflow: hidden;
    font-family: ui-monospace, Consolas, monospace;
    font-size: 12px;
  }
  .term-head {
    display: flex;
    gap: 8px;
    align-items: center;
    justify-content: flex-end;
    padding: 6px 10px;
    border-bottom: 1px solid #30363d;
    color: #8b949e;
    min-height: 28px;
  }
  .term-body {
    overflow-y: auto;
    padding: 8px 10px;
    flex: 1;
  }
  .line { white-space: pre-wrap; color: #c9d1d9; }
  .line.warn { color: #d29922; }
  .line.error { color: #f85149; }
  .line.hint { color: #58a6ff; }
  summary { cursor: pointer; color: #8b949e; }
  summary.ok { color: #3fb950; }
  summary.error { color: #f85149; }
  .ok { color: #3fb950; }
  .err { color: #f85149; }
  .runtime { font-variant-numeric: tabular-nums; }
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
  .jump {
    position: absolute;
    bottom: 10px;
    right: 14px;
    background: #1f6feb;
    color: white;
    border: none;
    border-radius: 6px;
    padding: 4px 10px;
    cursor: pointer;
  }
  .spinner {
    width: 12px;
    height: 12px;
    border: 2px solid #30363d;
    border-top-color: #58a6ff;
    border-radius: 50%;
    animation: spin 0.9s linear infinite;
    display: inline-block;
  }
  .spinner.small { width: 9px; height: 9px; margin-left: 6px; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
