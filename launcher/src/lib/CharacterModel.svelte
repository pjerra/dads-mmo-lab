<script lang="ts">
  import type { PaperdollData } from "$lib/api";
  import { createCharacterViewer, loadViewerScripts } from "$lib/model-viewer";

  let { doll }: { doll: PaperdollData } = $props();

  // Only one CharacterModel is ever mounted at a time (Dashboard), but a
  // unique id still avoids any chance of colliding with a leftover
  // previous-instance container if Svelte keeps the old DOM node around
  // for a tick during teardown.
  const containerId = `character-model-${crypto.randomUUID()}`;

  let status: "loading" | "ready" | "error" = $state("loading");

  // Teardown has no documented API in the recon (ZamModelViewer's own
  // destroy/dispose method was never captured in the source extracts) --
  // this is a best-effort guarded call: if the viewer instance doesn't
  // expose a `destroy` function, or calling it throws, we just drop the
  // reference. Must never break switching to the next character.
  function destroyViewer(viewer: unknown): void {
    if (!viewer || typeof viewer !== "object") return;
    const maybeDestroy = (viewer as { destroy?: unknown }).destroy;
    if (typeof maybeDestroy !== "function") return;
    try {
      (maybeDestroy as () => void).call(viewer);
    } catch {
      // best-effort teardown only
    }
  }

  $effect(() => {
    // Track doll.name specifically -- everything else this effect touches
    // is read inside async continuations (after the initial synchronous
    // pass), which Svelte's effect tracking doesn't subscribe to anyway.
    void doll.name;

    status = "loading";
    let cancelled = false;
    let created: unknown = null;

    loadViewerScripts()
      .then(() => createCharacterViewer(containerId, doll))
      .then((viewer) => {
        if (cancelled) return;
        created = viewer;
        status = "ready";
      })
      .catch(() => {
        if (cancelled) return;
        status = "error";
      });

    // Runs right before the next effect execution (i.e. destroy previous,
    // then the body above loads+creates the next one) and on unmount.
    return () => {
      cancelled = true;
      destroyViewer(created);
      // destroyViewer is best-effort (no documented destroy API upstream --
      // see above), so it can silently no-op and leave the old canvas/WebGL
      // context sitting in the DOM. Forcibly clearing the container's
      // children guarantees the next createCharacterViewer() call starts
      // from an empty container instead of stacking a new canvas on top of
      // a leaked one across character switches.
      document.getElementById(containerId)?.replaceChildren();
    };
  });
</script>

<div class="model-card">
  <div id={containerId} class="model-container"></div>
  {#if status === "loading"}
    <p class="muted">Loading model…</p>
  {:else if status === "error"}
    <p class="muted">3D model unavailable (needs internet on first view)</p>
  {/if}
</div>

<style>
  .model-card {
    box-sizing: border-box;
    width: 300px;
    height: 380px;
    flex-shrink: 0;
    background: #0d1117;
    border: 1px solid #30363d;
    border-radius: 8px;
    display: flex;
    align-items: center;
    justify-content: center;
    position: relative;
    overflow: hidden;
  }
  .model-container {
    width: 100%;
    height: 100%;
  }
  .muted {
    position: absolute;
    color: #8b949e;
    font-size: 13px;
    text-align: center;
    padding: 0 16px;
    pointer-events: none;
  }
</style>
