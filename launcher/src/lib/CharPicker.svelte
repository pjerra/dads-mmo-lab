<script lang="ts">
  import { onMount } from "svelte";
  import { wowAccounts, type Account, type CharacterSummary } from "$lib/api";
  import {
    charStore,
    findStoredChar,
    setSelectedChar,
    type SelectedChar,
  } from "$lib/char-store.svelte";

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
      // Mount staging is only a DEFAULT for an empty selection: if the
      // parent already put a name into the bound `selected` (Dashboard
      // adopting a Bot Browser "Open full view" request -- a bot name that
      // is deliberately NOT in the accounts list), overwriting it here
      // would clobber that request the moment this fetch resolves. The
      // account dropdown then simply stays unpicked until the user picks.
      if (selected) return;
      // Persistent selection (Batch 3 F12): prefer the stored character when
      // it still exists (and is actionable); otherwise the old first-account
      // default. Neither path calls onpick -- mount-time staging must not
      // commit itself (see the comment below).
      const stored = findStoredChar(accounts, charStore.selected);
      if (stored && ACTIONABLE_NAME.test(stored.char.name)) {
        accountName = stored.account;
        selected = stored.char.name;
        return;
      }
      const first = accounts.find((a) => actionable(a.characters).length > 0);
      if (first) {
        accountName = first.username;
        selected = actionable(first.characters)[0].name;
        // NB: deliberately NOT calling onpick here. Mount-time staging is wrong
        // for one-way consumers (Config's AHBot seller row): it would mark the
        // row dirty on page load and sweep a silent seller reassignment into any
        // unrelated save. A displayed default must not commit itself -- the user
        // must actively pick (onCharChange/onAccountChange fire onpick).
      }
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    }
  });

  // Sidebar "playing as" dropdown (Batch 3 F12): when the shared store
  // changes EXTERNALLY (the sidebar chip or another CharPicker) while this
  // picker is mounted, adopt the new selection so pages follow. Adoption does
  // NOT fire onpick (same commit-itself rationale as mount staging) -- the
  // bound `selected` still updates for consumers.
  //
  // Gate on a real store change, NOT on local `selected`/`accountName` drift.
  // Selecting an account with no actionable characters sets selected="" and
  // deliberately does NOT persist (the store keeps the previous char); an
  // effect that also tracked the local vars would then re-find that stored
  // char and snap the dropdown back to the old account, making empty accounts
  // unselectable. Reference-comparing against the last store value we saw
  // means only an external store mutation (always a fresh object) triggers
  // adoption; local interaction that leaves the store alone is ignored.
  let lastStoreObserved: SelectedChar | null = charStore.selected;
  $effect(() => {
    const sel = charStore.selected;
    if (accounts.length === 0) return; // wait for accounts; don't record yet
    if (sel === lastStoreObserved) return; // no external change
    lastStoreObserved = sel;
    if (!sel) return;
    const hit = findStoredChar(accounts, sel);
    if (hit && ACTIONABLE_NAME.test(hit.char.name)) {
      accountName = hit.account;
      selected = hit.char.name;
    }
  });

  // Write-back on a USER change in this picker (never on mount staging):
  // the store then drives the sidebar chip and every other CharPicker.
  function persistPick() {
    const c = currentChars.find((ch) => ch.name === selected);
    if (c) setSelectedChar({ guid: c.guid, name: c.name, account: accountName });
  }

  function onAccountChange() {
    selected = currentChars[0]?.name ?? "";
    if (selected) persistPick();
    onpick?.(selected);
  }

  function onCharChange() {
    persistPick();
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
