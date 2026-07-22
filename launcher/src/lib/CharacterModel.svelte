<script lang="ts">
  import type { PaperdollData } from "$lib/api";
  import {
    applyViewerSheath,
    createCharacterViewer,
    loadViewerScripts,
    readSheathedPref,
    skippedItemsNote,
    writeSheathedPref,
    type SheathValues,
  } from "$lib/model-viewer";

  // displayIds: CharacterSheet's item-info batch as a promise of
  // entry -> wowhead display_id overrides. createCharacterViewer awaits it
  // with its own bounded timeout (never rejects, never hangs the model);
  // hosts without item info simply omit it and get server ids.
  let {
    doll,
    compact = false,
    displayIds = null,
  }: {
    doll: PaperdollData;
    compact?: boolean;
    displayIds?: Promise<Map<number, number>> | null;
  } = $props();

  // Only one CharacterModel is ever mounted at a time (Dashboard), but a
  // unique id still avoids any chance of colliding with a leftover
  // previous-instance container if Svelte keeps the old DOM node around
  // for a tick during teardown.
  const containerId = `character-model-${crypto.randomUUID()}`;

  let status: "loading" | "ready" | "error" = $state("loading");
  // The real rejection reason -- surfaced in the UI because "needs internet"
  // turned out to cover at least three unrelated failure modes in practice.
  let errDetail = $state("");
  // Smoke item 6: items the CDN has no geometry for (custom/GM gear) are
  // silently dropped by the retry ladder -- without this note that read as
  // "the model viewer lost my gear" in live testing.
  let skipNote = $state<string | null>(null);

  // Sheathe/draw toggle: one global persisted pref (default = weapons in
  // hands). The engine call is live -- no viewer rebuild -- and the pref
  // re-applies on every fresh construction below. `canSheathe` gates the
  // button to dolls that actually rendered something sheathable.
  let sheathed = $state(readSheathedPref());
  let canSheathe = $state(false);
  let viewerRef: unknown = null;
  let sheathValues: SheathValues | null = null;

  function toggleSheathed(): void {
    sheathed = !sheathed;
    writeSheathedPref(sheathed);
    applyViewerSheath(viewerRef, sheathValues, sheathed);
  }

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
    skipNote = null;
    canSheathe = false;
    let cancelled = false;
    let created: unknown = null;

    loadViewerScripts()
      .then(() => createCharacterViewer(containerId, doll, displayIds ?? undefined))
      .then((res) => {
        if (cancelled) return;
        created = res.viewer;
        viewerRef = res.viewer;
        sheathValues = res.sheath;
        canSheathe = res.sheath.main >= 0 || res.sheath.off >= 0;
        skipNote = skippedItemsNote(res.totalItems, res.shownItems);
        status = "ready";
        // A persisted "sheathed" pref must land on the FRESH viewer too --
        // the engine queues the call while the actor is still loading, so
        // firing immediately after construction is safe.
        if (sheathed) applyViewerSheath(res.viewer, res.sheath, true);
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        const err = e as { message?: string };
        errDetail = err?.message ?? String(e);
        status = "error";
      });

    // Runs right before the next effect execution (i.e. destroy previous,
    // then the body above loads+creates the next one) and on unmount.
    return () => {
      cancelled = true;
      viewerRef = null;
      sheathValues = null;
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

<div class="model-card" class:compact>
  <div id={containerId} class="model-container"></div>
  {#if status === "ready" && canSheathe}
    <button class="sheath-btn" onclick={toggleSheathed}>
      {sheathed ? "Draw weapons" : "Sheathe weapons"}
    </button>
  {/if}
  {#if status === "loading"}
    <p class="muted">Loading model…</p>
  {:else if status === "error"}
    <p class="muted">
      3D model unavailable{#if errDetail}<br /><span class="errdetail">{errDetail}</span>{/if}
    </p>
  {:else if skipNote}
    <p class="skipnote">{skipNote}</p>
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
  /* Compact variant (CharacterSheet's Bot Browser embed): 225x285 keeps
     the full card's exact 15:19 ratio, so model-viewer's VIEWER_ASPECT
     (300/380) stays correct for the internal canvas sizing. */
  .model-card.compact {
    width: 225px;
    height: 285px;
  }
  .model-container {
    width: 100%;
    height: 100%;
  }
  /* Sheathe/draw toggle: quiet chip pinned to the card's top-right corner,
     ABOVE the canvas (a sibling overlay -- clicks on it never reach the
     model's drag-rotate). Same palette as the sheet's standard buttons,
     translucent so it stays unobtrusive over the render. */
  .sheath-btn {
    position: absolute;
    top: 6px;
    right: 6px;
    background: rgba(33, 38, 45, 0.85);
    color: #c9d1d9;
    border: 1px solid #30363d;
    border-radius: 6px;
    padding: 3px 8px;
    font-size: 11px;
    cursor: pointer;
  }
  .sheath-btn:hover {
    background: #30363d;
  }
  .muted {
    position: absolute;
    color: #8b949e;
    font-size: 13px;
    text-align: center;
    padding: 0 16px;
    pointer-events: none;
  }
  .errdetail {
    color: #f85149;
    font-size: 11.5px;
    word-break: break-word;
  }
  /* Skipped-items caption: pinned under the canvas, deliberately quiet --
     informational, not an error state. */
  .skipnote {
    position: absolute;
    bottom: 4px;
    left: 8px;
    right: 8px;
    margin: 0;
    color: #8b949e;
    font-size: 11px;
    text-align: center;
    pointer-events: none;
  }
</style>
