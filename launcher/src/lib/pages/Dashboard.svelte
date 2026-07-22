<script lang="ts">
  // Character-page host: CharPicker + manual reload around the reusable
  // CharacterSheet (the tabbed sheet itself -- paperdoll, 3D model,
  // talents, achievements -- was extracted to $lib/CharacterSheet.svelte
  // 2026-07-22 so Bot Browser can embed the same view; the module-level
  // item/entity caches moved with it and are shared by every host).
  import CharPicker from "$lib/CharPicker.svelte";
  import CharacterSheet from "$lib/CharacterSheet.svelte";
  import { charView } from "$lib/char-store.svelte";

  let charName = $state("");

  // The sheet auto-loads whenever charName changes (its own effect); this
  // handle only powers the manual "Reload gear" button. Structural type:
  // exactly the sheet's exported host seam. isLoading() reads the sheet's
  // reactive loading flag, so the disabled binding below re-evaluates when
  // loading flips.
  let sheet = $state<{ reload: () => void; isLoading: () => boolean } | null>(null);

  // Cross-page full-character-view request (smoke item 4b): adopt the
  // requested name into charName -- the sheet's auto-load effect then
  // fetches it. The request is cleared immediately so revisiting the page
  // later doesn't replay it. Runs both on mount (request set before
  // navigation) and while mounted (already on this page when the request
  // arrives). Bot Browser no longer uses this (it embeds CharacterSheet
  // in place); the hook stays for any other caller.
  $effect(() => {
    const req = charView.requestedName;
    if (req) {
      charView.requestedName = null;
      charName = req;
    }
  });
</script>

<section class="content">
  <!-- The server-status card that used to sit here moved to the global
       sidebar chip + Home card (Round Q) -- duplicating it on every page
       was noise, per user feedback. -->
  <header class="bar"><h2>Character viewer</h2></header>
  <div class="pickrow">
    <CharPicker bind:selected={charName} />
    <!-- Gear auto-loads when the character changes (CharacterSheet's own
         effect); this button is a manual refresh. -->
    <button onclick={() => sheet?.reload()} disabled={!charName || !sheet || sheet.isLoading()}>
      Reload gear
    </button>
  </div>
  {#if charName}
    <CharacterSheet bind:this={sheet} name={charName} />
  {/if}
</section>

<style>
  .content { padding: 20px 24px; overflow-y: auto; display: flex; flex-direction: column; gap: 16px; }
  .bar { display: flex; justify-content: space-between; align-items: center; }
  .bar h2 { margin: 0; font-size: 18px; }
  .pickrow { display: flex; gap: 8px; align-items: center; }
  button { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 14px; cursor: pointer; }
  button:disabled { opacity: 0.5; cursor: default; }
</style>
