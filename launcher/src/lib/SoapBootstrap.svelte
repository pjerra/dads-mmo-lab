<script lang="ts">
  // The guided step that makes GM Tools, My Party, the console's send box and
  // the announcements work -- or says plainly that they do not yet.
  //
  // A freshly installed AzerothCore has NO accounts, so until one exists at GM
  // level 3 every SOAP-backed feature fails with a SOAP_AUTH that names nothing
  // the user did wrong. Skipping this leaves an app where half the buttons
  // quietly do nothing, at the end of a multi-hour install. That is the worst
  // outcome available here, and this card exists to make it unreachable.
  //
  // WHY THE USER TYPES IT: verified 2026-08-01 against a live Docker Desktop,
  // the worldserver console cannot be scripted. `docker attach` refuses piped
  // stdin against a TTY container ("stdin is not a terminal"), and without the
  // tty it accepts the pipe but never returns. The only automated alternative
  // is an SRP6 INSERT into acore_auth, which would be a third sanctioned MySQL
  // write and is the user's decision to make, not ours.
  //
  // The credentials are saved ONLY after a real round-trip succeeds -- see
  // crates/dml-wow/src/soap_bootstrap.rs. Saving first would produce a
  // plausible soap.env full of details that do not work, which is harder to
  // diagnose than having none.
  import { onMount } from "svelte";
  import {
    wowSoapBootstrapInfo,
    wowSoapBootstrapVerify,
    type SoapBootstrapInfo,
    type SoapBootstrapVerdict,
  } from "$lib/api";

  let { onverified = () => {} }: { onverified?: () => void } = $props();

  let info = $state<SoapBootstrapInfo | null>(null);
  let user = $state("");
  let pass = $state("");
  let busy = $state(false);
  let verdict = $state<SoapBootstrapVerdict | null>(null);
  let loadError = $state<string | null>(null);

  onMount(async () => {
    try {
      const i = await wowSoapBootstrapInfo();
      info = i;
      user = i.default_user;
    } catch (e) {
      const err = e as { message?: string };
      loadError = err.message ?? String(e);
    }
  });

  // The commands are recomputed locally as the user edits, so what is on screen
  // is always what their current inputs would create. Asking the backend on
  // every keystroke would send a password over IPC for no gain.
  const commands = $derived(
    user.trim() === "" || pass === ""
      ? []
      : [`account create ${user} ${pass}`, `account set gmlevel ${user} 3 -1`],
  );

  async function verify() {
    busy = true;
    verdict = null;
    try {
      verdict = await wowSoapBootstrapVerify(user, pass);
      if (verdict.status === "ok") onverified();
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      verdict = {
        status: "rejected",
        detail: `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`,
        saved_to: null,
      };
    } finally {
      busy = false;
    }
  }

  async function copyCommands() {
    try {
      await navigator.clipboard.writeText(commands.join("\n"));
    } catch {
      /* clipboard denied -- the commands are on screen anyway */
    }
  }
</script>

<div class="soap-card">
  <h4>Finish setup: let the launcher talk to your server</h4>
  <p class="why">
    Your server has no accounts yet. Until one exists, GM Tools, My Party, the console's
    command box and announcements can't work — they'll fail with an authentication error that
    doesn't explain itself. This takes a minute.
  </p>

  {#if loadError}
    <p class="err">Couldn't load the setup details: {loadError}</p>
  {:else if !info}
    <p class="muted">Loading…</p>
  {:else}
    <div class="fields">
      <label>
        Account name
        <input bind:value={user} disabled={busy} spellcheck="false" autocomplete="off" />
      </label>
      <label>
        Password
        <input bind:value={pass} disabled={busy} type="password" autocomplete="new-password" />
      </label>
    </div>
    <p class="muted small">
      3–20 characters for the name, 4–16 for the password. This is a server admin account,
      not your game login.
    </p>

    <ol class="steps">
      <li>
        Open your server's console:
        <code class="cmd">{info.attach_hint}</code>
      </li>
      <li>
        Type these two lines (both — the first makes the account, the second gives it
        permission):
        {#if commands.length}
          <pre class="cmds">{commands.join("\n")}</pre>
          <button onclick={copyCommands} disabled={busy}>Copy both lines</button>
        {:else}
          <p class="muted small">Fill in a name and password above and the exact lines appear here.</p>
        {/if}
      </li>
      <li>
        Leave the console: <strong>Ctrl-P</strong> then <strong>Ctrl-Q</strong>.
        <span class="warn">{info.detach_warning}</span>
      </li>
    </ol>

    <div class="row">
      <button
        class="primary"
        disabled={busy || user.trim() === "" || pass === ""}
        onclick={verify}
      >
        {busy ? "Checking…" : "Check it worked"}
      </button>
    </div>

    {#if verdict}
      {#if verdict.status === "ok"}
        <p class="ok">
          Done — the launcher can talk to your server. GM Tools, My Party and the console
          are live.
          {#if verdict.saved_to}<span class="muted small">Saved to {verdict.saved_to}</span>{/if}
        </p>
      {:else if verdict.status === "unreachable"}
        <!-- Deliberately NOT phrased as bad credentials: the usual cause is a
             world server that has not finished booting, and blaming the password
             sends the user to delete and recreate a perfectly good account. -->
        <p class="warnbox">{verdict.detail}</p>
      {:else}
        <p class="err">{verdict.detail}</p>
      {/if}
    {/if}
  {/if}
</div>

<style>
  .soap-card {
    border: 1px solid var(--accent, #4a90d9);
    border-radius: 0.5rem;
    padding: 0.9rem 1rem;
    margin-top: 1rem;
    background: rgba(74, 144, 217, 0.06);
  }
  h4 { margin: 0 0 0.35rem; }
  .why { margin: 0 0 0.8rem; font-size: 0.88rem; line-height: 1.45; }
  .fields { display: flex; gap: 0.8rem; flex-wrap: wrap; }
  .fields label { display: flex; flex-direction: column; gap: 0.2rem; font-size: 0.82rem; }
  .fields input { padding: 0.35rem 0.5rem; min-width: 12rem; }
  .steps { margin: 0.8rem 0 0.6rem; padding-left: 1.2rem; font-size: 0.88rem; line-height: 1.5; }
  .steps li { margin-bottom: 0.6rem; }
  .cmd, .cmds {
    display: block;
    margin-top: 0.3rem;
    padding: 0.4rem 0.6rem;
    background: rgba(0, 0, 0, 0.28);
    border-radius: 0.3rem;
    font-family: ui-monospace, Consolas, monospace;
    font-size: 0.82rem;
    overflow-x: auto;
    white-space: pre;
  }
  .warn { display: block; margin-top: 0.25rem; color: var(--warn-fg, #f0c674); font-size: 0.82rem; }
  .row { margin-top: 0.4rem; }
  .ok { color: var(--ok-fg, #8ec07c); margin: 0.7rem 0 0; font-size: 0.88rem; }
  .err { color: var(--err-fg, #fb4934); margin: 0.7rem 0 0; font-size: 0.88rem; }
  .warnbox { color: var(--warn-fg, #f0c674); margin: 0.7rem 0 0; font-size: 0.88rem; }
  .muted { opacity: 0.75; }
  .small { font-size: 0.8rem; }
</style>
