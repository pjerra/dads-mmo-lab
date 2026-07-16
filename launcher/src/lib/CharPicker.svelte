<script lang="ts">
  import { onMount } from "svelte";
  import { wowAccounts, type Account, type CharacterSummary } from "$lib/api";

  let {
    selected = $bindable(""),
    disabled = false,
    onpick,
  }: { selected?: string; disabled?: boolean; onpick?: (name: string) => void } = $props();
  let accounts: Account[] = $state([]);
  let accountName = $state("");
  let error: string | null = $state(null);

  // Every action verb (teleport/mail/etc.) enforces ^[A-Za-z0-9_]{1,12}$ on char
  // names, but the accounts read path doesn't -- so extended-Latin/Cyrillic
  // names would otherwise get listed here and then fail every action with an
  // opaque BAD_ARG. Filter them out before they're ever offered as a choice.
  const ACTIONABLE_NAME = /^[A-Za-z0-9_]{1,12}$/;
  function actionable(chars: CharacterSummary[]): CharacterSummary[] {
    return chars.filter((c) => ACTIONABLE_NAME.test(c.name));
  }

  const current = $derived(accounts.find((a) => a.username === accountName));
  const currentChars = $derived(actionable(current?.characters ?? []));

  onMount(async () => {
    try {
      accounts = await wowAccounts();
      const first = accounts.find((a) => actionable(a.characters).length > 0);
      if (first) {
        accountName = first.username;
        selected = actionable(first.characters)[0].name;
        onpick?.(selected);
      }
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    }
  });

  function onAccountChange() {
    selected = currentChars[0]?.name ?? "";
    onpick?.(selected);
  }

  function onCharChange() {
    onpick?.(selected);
  }
</script>

{#if error}
  <span class="err">Couldn't load characters: {error}</span>
{:else}
  <select bind:value={accountName} onchange={onAccountChange} {disabled}>
    {#each accounts as a (a.id)}
      <option value={a.username}>{a.username}</option>
    {/each}
  </select>
  <select
    bind:value={selected}
    onchange={onCharChange}
    disabled={disabled || !current || currentChars.length === 0}
  >
    {#each currentChars as c (c.guid)}
      <option value={c.name}>{c.name} (lvl {c.level})</option>
    {/each}
  </select>
{/if}

<style>
  select { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 5px 8px; }
  .err { color: #f85149; font-size: 13px; }
</style>
