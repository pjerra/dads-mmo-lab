<script lang="ts">
  import { onMount } from "svelte";
  import { wowAccounts, type Account } from "$lib/api";

  let { selected = $bindable("") }: { selected?: string } = $props();
  let accounts: Account[] = $state([]);
  let accountName = $state("");
  let error: string | null = $state(null);

  const current = $derived(accounts.find((a) => a.username === accountName));

  onMount(async () => {
    try {
      accounts = await wowAccounts();
      const first = accounts.find((a) => a.characters.length > 0);
      if (first) {
        accountName = first.username;
        selected = first.characters[0].name;
      }
    } catch (e) {
      const err = e as { message?: string };
      error = err.message ?? String(e);
    }
  });

  function onAccountChange() {
    selected = current?.characters[0]?.name ?? "";
  }
</script>

{#if error}
  <span class="err">Couldn't load characters: {error}</span>
{:else}
  <select bind:value={accountName} onchange={onAccountChange}>
    {#each accounts as a (a.id)}
      <option value={a.username}>{a.username}</option>
    {/each}
  </select>
  <select bind:value={selected} disabled={!current || current.characters.length === 0}>
    {#each current?.characters ?? [] as c (c.guid)}
      <option value={c.name}>{c.name} (lvl {c.level})</option>
    {/each}
  </select>
{/if}

<style>
  select { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 5px 8px; }
  .err { color: #f85149; font-size: 13px; }
</style>
