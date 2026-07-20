<script lang="ts">
  // Router wrapper for the two bot views, now selected from the sidebar (no
  // in-page tab bar). `view` is the nav page id: "party" (My Party) or
  // "browse" (the ~2500-bot browser). Each child renders its own full-page
  // content; lazy {#if} means the browser DB query only runs when opened.
  import Playerbots from "./Playerbots.svelte";
  import BotBrowser from "./BotBrowser.svelte";

  let { view = "party" }: { view?: string } = $props();
</script>

{#if view === "browse"}
  <BotBrowser />
{:else}
  <Playerbots />
{/if}
