<script lang="ts">
  import { onMount } from "svelte";
  import { wowAccounts, type Account } from "$lib/api";
  import { isValidCharName, recipientStatus } from "$lib/mail-recipient";

  // Mail recipient: type a name (with autocomplete over every character on
  // the server) instead of hunting through account dropdowns. Deliberately
  // NOT CharPicker -- that component is mounted on eight other pages and
  // means "who am I playing as", which is a different question from "who am
  // I mailing". A recipient may legitimately be a character this launcher
  // has never listed (a bot, or one created since the page loaded).
  let {
    selected = $bindable(""),
    valid = $bindable(false),
    disabled = false,
  }: { selected?: string; valid?: boolean; disabled?: boolean } = $props();

  let accounts: Account[] = $state([]);
  let loaded = $state(false);
  let loadError: string | null = $state(null);

  // Names the action verbs would refuse are filtered out of the suggestions:
  // the picker must never offer something that then fails with an opaque
  // BAD_ARG (the same filter CharPicker applies).
  const knownNames = $derived(
    accounts
      .flatMap((a) => a.characters.map((c) => c.name))
      .filter(isValidCharName)
      .sort((a, b) => a.localeCompare(b)),
  );

  const status = $derived(recipientStatus(selected, knownNames));

  // Unknown-but-well-formed is sendable; only empty/malformed blocks the send.
  $effect(() => {
    valid = status === "known" || status === "unknown";
  });

  onMount(async () => {
    try {
      accounts = await wowAccounts();
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      loadError = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    } finally {
      loaded = true;
    }
  });
</script>

<div class="recipient">
  <div class="line">
    <input
      list="dml-recipient-names"
      placeholder="Character name"
      autocomplete="off"
      spellcheck="false"
      aria-label="Mail recipient"
      bind:value={selected}
      {disabled}
    />
    <datalist id="dml-recipient-names">
      {#each knownNames as n (n)}
        <option value={n}></option>
      {/each}
    </datalist>
    {#if status === "known"}
      <span class="ok-text">Known character</span>
    {/if}
  </div>

  {#if loadError}
    <span class="warn-text">
      Couldn't load the character list ({loadError}) — you can still type a name.
    </span>
  {:else if status === "invalid"}
    <span class="err-text">
      Names are 1–12 characters, letters/digits/underscore only.
    </span>
  {:else if status === "unknown" && loaded}
    <span class="warn-text">
      No character called "{selected.trim()}" found. You can still send — bots and
      newly-created characters may not be listed — but mail to a name that doesn't
      exist is lost, so check the spelling.
    </span>
  {/if}
</div>

<style>
  .recipient { display: flex; flex-direction: column; gap: 4px; }
  .line { display: flex; gap: 8px; align-items: center; }
  input {
    background: #21262d;
    color: #c9d1d9;
    border: 1px solid #30363d;
    border-radius: 6px;
    padding: 5px 8px;
    min-width: 200px;
  }
  input:disabled { opacity: 0.5; }
  .ok-text { color: #3fb950; font-size: 13px; }
  .warn-text { color: #d29922; font-size: 13px; }
  .err-text { color: #f85149; font-size: 13px; }
</style>
