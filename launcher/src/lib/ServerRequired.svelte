<script lang="ts">
  import type { GateState } from "$lib/server-gate";

  // Shown INSTEAD of a page that needs the world server, so the page's own
  // fetches never run (and never spray errors) while the server is down.
  // DECIDED 2026-07-21: a greeting carrying the fixing action, not a
  // greyed-out sidebar entry.
  let { gate, onstart }: { gate: GateState; onstart: () => void } = $props();

  const GLYPH: Record<GateState["kind"], string> = {
    stopped: "🌙",
    crashed: "💥",
    starting: "⏳",
    unreachable: "📡",
  };
</script>

<section class="content">
  <div class="greet">
    <div class="glyph">{GLYPH[gate.kind]}</div>
    <h2>{gate.title}</h2>
    <p>{gate.body}</p>
    {#if gate.canStart}
      <button class="primary" onclick={onstart}>Start server</button>
    {/if}
  </div>
</section>

<style>
  .content { padding: 20px 24px; overflow-y: auto; display: flex; }
  .greet {
    margin: 48px auto 0;
    max-width: 460px;
    text-align: center;
    background: #0d1117;
    border: 1px solid #30363d;
    border-radius: 8px;
    padding: 28px 24px;
  }
  .glyph { font-size: 34px; line-height: 1; }
  h2 { margin: 12px 0 6px; font-size: 18px; color: #f0f6fc; }
  p { margin: 0 0 16px; color: #8b949e; font-size: 14px; line-height: 1.5; }
  button {
    background: #21262d;
    color: #c9d1d9;
    border: 1px solid #30363d;
    border-radius: 6px;
    padding: 7px 16px;
    cursor: pointer;
  }
  button.primary { background: #238636; border-color: #2ea043; color: #ffffff; }
</style>
