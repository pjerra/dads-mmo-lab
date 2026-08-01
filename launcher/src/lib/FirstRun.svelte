<script lang="ts">
  // The first thing a stranger sees (SHIP-LIST 4.4). Rendered INSTEAD of Home
  // while the backend probe chain says this machine cannot run a server yet,
  // so nobody lands on a status card for a server that does not exist.
  //
  // Deliberately shaped like ServerRequired.svelte: one glyph, one sentence,
  // one button, centred and calm. This is not an error screen -- a new user
  // has done nothing wrong, they simply have one step left.
  //
  // NO DECISIONS LIVE HERE. Which state, which copy and which button all come
  // from first-run.ts (vitest-pinned); this component only renders the answer
  // and performs the action.
  import { openUrl } from "@tauri-apps/plugin-opener";
  import type { PageId } from "$lib/nav";
  import { firstRunButton, type FirstRunKind, type FirstRunState } from "$lib/first-run";
  import { backendSetup } from "$lib/api";
  import { applyEvent } from "$lib/terminal-state";
  import Terminal from "$lib/Terminal.svelte";
  import { termBuf, beginRun, clearBuf } from "$lib/term-store.svelte";
  import { backendSetupRun } from "$lib/backend-setup.svelte";
  import { featureLocked, LOCKED_HINT } from "$lib/features.svelte";
  import { taskbarBusy, taskbarIdle } from "$lib/taskbar";

  let {
    state: fr,
    onnav,
    onrecheck,
    rechecking,
  }: {
    state: FirstRunState;
    onnav: (p: PageId) => void;
    /** Re-run the backend probe -- the screen's whole job is to stop being true. */
    onrecheck: () => void;
    /**
     * Whether a probe is in flight RIGHT NOW. Owned by the shell, which owns the
     * probe: the retry can cost the full cold-start budget, and a button that
     * neither disables nor changes text for that long -- while the shell's
     * re-entry guard drops every re-click -- reads as broken.
     *
     * REQUIRED ON PURPOSE, no default. A default of `false` makes an unwired
     * screen look exactly like an idle one, so deleting `rechecking={probing}`
     * from the shell would silently reinstate that button with every test still
     * green. Without one, the omission is a svelte-check error -- and
     * first-run-pairing.test.ts pins the attribute itself.
     */
    rechecking: boolean;
  } = $props();

  // Exhaustive by type: adding a FirstRunKind without a glyph is a
  // svelte-check error rather than a blank square in front of a new user.
  const GLYPH: Record<FirstRunKind, string> = {
    "no-docker": "🐳",
    "docker-stopped": "🐳",
    "no-wsl": "🧰",
    "no-distro": "🧰",
    "no-cli": "📦",
    "cli-outdated": "📦",
    "no-titles": "🎮",
    "payload-missing": "🧩",
    "payload-unknown": "🧩",
    unknown: "🔎",
  };

  // Transcript lives in the shared store so navigating away mid-setup doesn't
  // throw away the output of the one job a new user most needs to read.
  const buf = termBuf("first-run");

  // [backend-setup] is registered UNLOCKED on purpose -- locking it would lock
  // a new user out of their own setup -- but it is wired like every other
  // mutating control so the registry stays the single switch.
  const setupLocked = $derived(featureLocked("backend-setup"));
  const busy = $derived(backendSetupRun.running);
  // Label + disabled together, decided in first-run.ts so both slow arms
  // (setup streaming, probe in flight) are vitest-pinned rather than clicked.
  const btn = $derived(
    firstRunButton(fr.action, { setupRunning: busy, rechecking, setupLocked }),
  );

  async function runSetup() {
    if (backendSetupRun.running) return;
    backendSetupRun.running = true;
    beginRun("first-run");
    taskbarBusy();
    try {
      await backendSetup((e) => {
        buf.term = applyEvent(buf.term, e);
      });
    } catch (e) {
      const err = e as { code?: string; message?: string; hint?: string };
      buf.term = applyEvent(buf.term, {
        event: "error",
        error: {
          code: err.code ?? "IPC",
          message: err.message ?? String(e),
          hint: err.hint ?? "",
        },
      });
    } finally {
      taskbarIdle();
      backendSetupRun.running = false;
      // Always re-probe, including after a failed run: the screen must reflect
      // what the machine looks like NOW, not what it looked like on mount.
      //
      // Clearing `running` first is safe but not sufficient. Safe: onrecheck()
      // raises the shell's `probing` synchronously (probeBackend sets it before
      // its first await), so both writes land in one batch and no frame renders
      // between them. Not sufficient: the probe then OUTLIVES this block by up
      // to the cold-start budget, and the button is rendered from `rechecking`
      // for all of it -- which is why firstRunButton's setup arm has to honour
      // that flag, not just `running`.
      onrecheck();
    }
  }

  function act() {
    const a = fr.action;
    switch (a.kind) {
      case "setup":
        void runSetup();
        break;
      case "nav":
        onnav(a.page);
        break;
      case "link":
        openUrl(a.url).catch(() => {
          // Best-effort -- a failed browser launch must not break the screen
          // that is meant to be someone's calm first impression.
        });
        break;
      case "retry":
        onrecheck();
        break;
    }
  }
</script>

<section class="content">
  <div class="greet">
    <div class="glyph">{GLYPH[fr.kind]}</div>
    <h2>{fr.title}</h2>
    <p>{fr.body}</p>
    <button
      class="primary"
      onclick={act}
      disabled={btn.disabled}
      title={fr.action.kind === "setup" && setupLocked ? LOCKED_HINT : undefined}
    >
      {btn.label}
    </button>
    {#if fr.detail}
      <p class="detail">{fr.detail}</p>
    {/if}
  </div>

  {#if buf.show}
    <div class="term-wrap">
      <Terminal
        state={buf.term}
        logName="dml-backend-setup"
        onclear={() => clearBuf("first-run")}
      />
    </div>
  {/if}
</section>

<style>
  /* Column (not ServerRequired's plain flex row): the setup stream renders
     under the greeting, and both scroll together. */
  .content { padding: 20px 24px; overflow-y: auto; display: flex; flex-direction: column; box-sizing: border-box; }
  .greet {
    margin: 48px auto 0;
    max-width: 460px;
    text-align: center;
    background: #0d1117;
    border: 1px solid #30363d;
    border-radius: 8px;
    padding: 28px 24px;
  }
  .glyph { font-size: 34px; line-height: 1; }
  h2 { margin: 12px 0 6px; font-size: 18px; color: #f0f6fc; }
  p { margin: 0 0 16px; color: #8b949e; font-size: 14px; line-height: 1.5; }
  /* Diagnostics (which files are missing, which probe errored). Quiet on
     purpose: useful when you need it, never the headline. */
  .detail { margin: 14px 0 0; font-size: 12px; color: #6e7681; word-break: break-word; }
  .term-wrap { max-width: 860px; width: 100%; margin: 20px auto 0; }
  button {
    background: #21262d;
    color: #c9d1d9;
    border: 1px solid #30363d;
    border-radius: 6px;
    padding: 7px 16px;
    cursor: pointer;
  }
  button.primary { background: #238636; border-color: #2ea043; color: #ffffff; }
  button:disabled { opacity: 0.6; cursor: default; }
</style>
