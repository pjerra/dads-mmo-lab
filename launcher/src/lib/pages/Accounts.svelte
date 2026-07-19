<script lang="ts">
  import { onMount } from "svelte";
  import {
    wowAccounts,
    wowAccountCreate,
    wowAccountSetPassword,
    wowAccountSetGm,
    wowAccountDelete,
    type Account,
  } from "$lib/api";
  import { featureLocked, LOCKED_HINT } from "$lib/features.svelte";

  const USER_RE = /^[A-Za-z0-9_]{3,20}$/;
  const PASS_RE = /^[A-Za-z0-9_@#%+=!-]{4,16}$/;

  let accounts: Account[] | null = $state(null);
  let error: string | null = $state(null); // initial/refresh list-load failure only -> error-card
  let note: string | null = $state(null);
  let busy = $state(false); // single flag: disables every Create/Apply button

  // Create-account card.
  let newUser = $state("");
  let newPass = $state("");
  let createError: string | null = $state(null);
  const userValid = $derived(USER_RE.test(newUser));
  const passValid = $derived(PASS_RE.test(newPass));

  // Per-row state, keyed by username. GM level is kept as the select's own
  // string value (mirrors Items.svelte's quality select) and converted to a
  // number only when calling the API.
  let gmLevels: Record<string, string> = $state({});
  let confirmingGm: string | null = $state(null); // username armed for the level-3 confirm
  let revealPassword: string | null = $state(null); // username with the password field open
  let passwordInputs: Record<string, string> = $state({});
  let rowError: Record<string, string> = $state({});

  function fmtErr(e: unknown): string {
    const err = e as { message?: string; hint?: string };
    return `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
  }

  // New usernames default their GM select to the account's current level
  // without stomping a value the user already picked (called after every refresh).
  function ensureGmDefaults() {
    if (!accounts) return;
    for (const a of accounts) {
      if (!(a.username in gmLevels)) gmLevels[a.username] = String(a.gm_level);
    }
  }

  async function refresh() {
    error = null; confirmingGm = null;
    try {
      accounts = await wowAccounts();
      ensureGmDefaults();
    } catch (e) { error = fmtErr(e); }
  }
  onMount(refresh);

  async function createAccount() {
    if (!userValid || !passValid) return;
    busy = true; createError = null; note = null;
    try {
      const r = await wowAccountCreate(newUser, newPass);
      note = `Account ${r.user} created.`;
      newUser = ""; newPass = "";
      await refresh();
    } catch (e) { createError = fmtErr(e); } finally { busy = false; }
  }

  function toggleReveal(username: string) {
    if (revealPassword === username) {
      revealPassword = null;
      return;
    }
    revealPassword = username;
    if (!(username in passwordInputs)) passwordInputs[username] = "";
    rowError[username] = "";
  }

  async function applyPassword(username: string) {
    const pass = passwordInputs[username] ?? "";
    if (!PASS_RE.test(pass)) return;
    busy = true; rowError[username] = ""; note = null;
    try {
      const r = await wowAccountSetPassword(username, pass);
      note = `Password updated for ${r.user}.`;
      passwordInputs[username] = "";
      revealPassword = null;
      await refresh();
    } catch (e) { rowError[username] = fmtErr(e); } finally { busy = false; }
  }

  function applyGm(username: string) {
    const level = Number(gmLevels[username] ?? "0");
    if (level === 3 && confirmingGm !== username) {
      confirmingGm = username;
      return;
    }
    confirmingGm = null;
    return setGm(username, level);
  }

  async function setGm(username: string, level: number) {
    busy = true; rowError[username] = ""; note = null;
    try {
      const r = await wowAccountSetGm(username, level);
      note = `${r.user} is now GM level ${r.level}.`;
      await refresh();
    } catch (e) { rowError[username] = fmtErr(e); } finally { busy = false; }
  }

  // Delete: typed-confirm (retype the username) because it takes every
  // character on the account with it. The CLI refuses the admin account
  // outright; the UI mirrors that by not offering Delete on it at all.
  let deletingUser: string | null = $state(null); // username with the confirm field open
  let deleteInput = $state("");
  function armDelete(username: string) {
    if (deletingUser === username) {
      deletingUser = null;
      return;
    }
    deletingUser = username;
    deleteInput = "";
    rowError[username] = "";
  }
  async function confirmDelete(username: string) {
    if (deleteInput !== username) return;
    busy = true; rowError[username] = ""; note = null;
    try {
      const r = await wowAccountDelete(username);
      note = `Account ${r.user} deleted (with all its characters).`;
      deletingUser = null;
      await refresh();
    } catch (e) { rowError[username] = fmtErr(e); } finally { busy = false; }
  }
</script>

<section class="content">
  <header class="bar">
    <h2>Accounts</h2>
    <button onclick={refresh} disabled={busy}>Refresh</button>
  </header>

  {#if error}<div class="error-card"><p>{error}</p></div>{/if}
  {#if note}<p class="muted">{note}</p>{/if}

  <div class="card">
    <h3>Create account</h3>
    <div class="row">
      <div class="field">
        <input type="text" placeholder="Username" bind:value={newUser} disabled={busy} />
        <span class="muted">3-20 letters, digits or _</span>
      </div>
      <div class="field">
        <input type="password" placeholder="Password" bind:value={newPass} disabled={busy} />
        <span class="muted">4-16 chars; no spaces</span>
      </div>
      <button
        class="primary"
        onclick={createAccount}
        disabled={busy || !userValid || !passValid || featureLocked("accounts")}
        title={featureLocked("accounts") ? LOCKED_HINT : undefined}
      >
        Create
      </button>
    </div>
    {#if createError}<p class="inline-error">{createError}</p>{/if}
  </div>

  <div class="card">
    <h3>All accounts</h3>
    {#if accounts}
      {#if accounts.length === 0}
        <p class="muted">No accounts yet.</p>
      {:else}
        {#each accounts as a (a.id)}
          <div class="arow">
            <div class="row">
              <strong>{a.username}</strong>
              <span class="muted">#{a.id}</span>
              {#if a.gm_level > 0}<span class="badge gm">GM {a.gm_level}</span>{/if}
            </div>
            <p class="muted">
              {a.characters.length > 0 ? a.characters.map((c) => c.name).join(", ") : "No characters"}
            </p>
            <div class="row">
              {#if revealPassword === a.username}
                <input
                  type="password"
                  placeholder="New password"
                  bind:value={passwordInputs[a.username]}
                  disabled={busy}
                />
                <span class="muted">4-16 chars; no spaces</span>
                <button
                  class="primary"
                  onclick={() => applyPassword(a.username)}
                  disabled={busy || !PASS_RE.test(passwordInputs[a.username] ?? "") || featureLocked("accounts")}
                  title={featureLocked("accounts") ? LOCKED_HINT : undefined}
                >
                  Apply
                </button>
              {/if}
              <button onclick={() => toggleReveal(a.username)} disabled={busy}>Set password</button>

              <label class="row">
                GM level
                <select
                  bind:value={gmLevels[a.username]}
                  disabled={busy}
                  onchange={() => { if (confirmingGm === a.username) confirmingGm = null; }}
                >
                  <option value="0">0</option>
                  <option value="1">1</option>
                  <option value="2">2</option>
                  <option value="3">3</option>
                </select>
              </label>
              <button
                onclick={() => applyGm(a.username)}
                disabled={busy || featureLocked("accounts")}
                title={featureLocked("accounts") ? LOCKED_HINT : undefined}
              >
                {confirmingGm === a.username ? "Level 3 grants full admin including SOAP. Continue?" : "Apply"}
              </button>

              {#if a.username.toLowerCase() !== "admin"}
                <button
                  class="danger"
                  onclick={() => armDelete(a.username)}
                  disabled={busy || featureLocked("account-delete")}
                  title={featureLocked("account-delete") ? LOCKED_HINT : undefined}
                >
                  {deletingUser === a.username ? "Cancel" : "Delete"}
                </button>
              {/if}
            </div>
            {#if deletingUser === a.username}
              <div class="row">
                <span class="inline-error">
                  Deletes the account AND all its characters permanently. Type the account name to confirm:
                </span>
                <input type="text" placeholder={a.username} bind:value={deleteInput} disabled={busy} />
                <button
                  class="danger"
                  onclick={() => confirmDelete(a.username)}
                  disabled={busy || deleteInput !== a.username || featureLocked("account-delete")}
                >
                  Delete forever
                </button>
              </div>
            {/if}
            {#if rowError[a.username]}<p class="inline-error">{rowError[a.username]}</p>{/if}
          </div>
        {/each}
      {/if}
    {/if}
  </div>
</section>

<style>
  .content { padding: 20px 24px; overflow-y: auto; display: flex; flex-direction: column; gap: 12px; }
  .bar { display: flex; justify-content: space-between; align-items: center; }
  .bar h2 { margin: 0; font-size: 18px; }
  .card { background: #0d1117; border: 1px solid #30363d; border-radius: 8px; padding: 12px 16px; display: flex; flex-direction: column; gap: 10px; }
  .card h3 { margin: 0; font-size: 15px; color: #58a6ff; }
  .row { display: flex; gap: 10px; align-items: center; flex-wrap: wrap; }
  .field { display: flex; flex-direction: column; gap: 4px; }
  .arow { padding: 8px 0; border-top: 1px solid #21262d; display: flex; flex-direction: column; gap: 6px; }
  .arow:first-of-type { border-top: none; }
  .badge { font-size: 12px; padding: 2px 10px; border-radius: 10px; border: 1px solid #30363d; }
  .badge.gm { color: #d29922; border-color: #d29922; }
  input[type="text"], input[type="password"] { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 8px; }
  select { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 8px; }
  button { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 14px; cursor: pointer; }
  button.primary { background: #238636; border-color: #2ea043; color: white; }
  button.danger { border-color: #f85149; color: #f85149; }
  button:disabled { opacity: 0.5; cursor: default; }
  .muted { color: #8b949e; font-size: 13px; margin: 0; }
  .inline-error { color: #f85149; font-size: 13px; margin: 0; }
  .error-card { background: #161b22; border: 1px solid #f85149; border-radius: 8px; padding: 12px 16px; }
</style>
