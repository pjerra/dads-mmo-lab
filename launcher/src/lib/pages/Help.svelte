<script lang="ts">
  // Help & FAQ (Batch 3 F9). Read-only page distilled from the two guide
  // docs (guides/wow-wotlk/WoW-Playerbots-Windows-HOWTO.md and
  // WoW-Wotlk-NETWORKING.md) for a non-technical parent: scannable
  // accordion sections, copyable command blocks, deep links to the Tools /
  // Backups pages. No feature flag -- nothing here mutates anything.
  //
  // NB: the guides mention "join the discord" but contain NO actual Discord
  // URL (verified 2026-07-19 against both guide files), so the Community
  // section deliberately links only the upstream GitHub project -- never
  // invent a URL.
  import { openUrl } from "@tauri-apps/plugin-opener";
  import type { PageId } from "$lib/nav";

  let { onnav }: { onnav?: (p: PageId) => void } = $props();

  function community() {
    openUrl("https://github.com/DadsMmoLab/dads-mmo-lab").catch(() => {
      // Best-effort -- a failed browser launch shouldn't break the page.
    });
  }

  // Which command block was just copied (index key), for the brief "Copied!"
  // feedback on its button.
  let copied: string | null = $state(null);
  let copyTimer: ReturnType<typeof setTimeout> | undefined;
  function copy(key: string, text: string) {
    navigator.clipboard
      .writeText(text)
      .then(() => {
        copied = key;
        clearTimeout(copyTimer);
        copyTimer = setTimeout(() => (copied = null), 2000);
      })
      .catch(() => {
        // Clipboard unavailable -- the text is still visible to copy by hand.
      });
  }
</script>

{#snippet cmd(key: string, text: string, note: string)}
  <div class="cmdblock">
    <pre>{text}</pre>
    <button class="copy" onclick={() => copy(key, text)}>{copied === key ? "Copied!" : "Copy"}</button>
  </div>
  {#if note}<p class="cmdnote">{note}</p>{/if}
{/snippet}

<section class="content">
  <header class="bar"><h2>Help &amp; FAQ</h2></header>
  <p class="muted">
    The most common problems and their fixes. Click a question to expand it.
  </p>

  <details class="card">
    <summary>Another PC in the house can't connect</summary>
    <div class="body">
      <p>Work down this list on the <strong>server PC</strong> — it's ordered by how often each one is the culprit:</p>
      <ol>
        <li>
          <strong>LAN play must be turned on.</strong> It's off by default (the server only talks to this
          PC). Turn it on from
          <button class="navlink" onclick={() => onnav?.("tools")}>the Tools page</button> — it also shows
          this PC's network address (something like <code>192.168.1.50</code>).
        </li>
        <li>
          <strong>The other PC's realmlist must point at this PC.</strong> On the other PC, open
          <code>realmlist.wtf</code> in its WoW folder (often under <code>Data\enUS\</code>) with Notepad
          and make it say the server PC's address from step 1:
          {@render cmd("lan-realmlist", "set realmlist 192.168.1.50", "Replace 192.168.1.50 with YOUR server PC's address from the Tools page.")}
        </li>
        <li>
          <strong>Windows must treat your network as Private.</strong> Windows blocks incoming game
          traffic on <em>Public</em> networks by design. On the server PC: Settings → Network &amp;
          internet → your connection → Network profile type → <strong>Private</strong>. This is the #1
          reason a LAN client can't connect.
        </li>
        <li>
          <strong>The one-time Windows plumbing (firewall + port forwarding into WSL2)</strong> is set up
          by <code>Install-DML.ps1</code>. If you installed before LAN play existed, or your PC got a new
          address from the router, re-run it once. To check the rules exist (PowerShell):
          {@render cmd("lan-portproxy", "netsh interface portproxy show v4tov4", "You should see your PC's current address listed for ports 3724 and 8085, each pointing at 127.0.0.1.")}
        </li>
      </ol>
    </div>
  </details>

  <details class="card">
    <summary>Can't connect on this PC</summary>
    <div class="body">
      <ol>
        <li>
          <strong>The realmlist must say 127.0.0.1.</strong> Open <code>realmlist.wtf</code> in your WoW
          client folder (often under <code>Data\enUS\</code>) and make sure it contains exactly:
          {@render cmd("local-realmlist", "set realmlist 127.0.0.1", "The Tools page can check and fix this for you when your client folder is set.")}
        </li>
        <li>
          <strong>The server must actually be running.</strong> The sidebar chip should say
          "World is up". If it doesn't, start the server from Home and give it a couple of minutes.
        </li>
      </ol>
    </div>
  </details>

  <details class="card">
    <summary>The server won't start</summary>
    <div class="body">
      <ol>
        <li>
          <strong>Be patient first.</strong> A cold start can take several minutes while the database
          warms up, and the world may crash-and-retry once early on — that self-heals and is normal.
        </li>
        <li>
          <strong>Run the health check.</strong>
          <button class="navlink" onclick={() => onnav?.("tools")}>The Tools page</button> has a Doctor
          button that checks Docker, disk space and the server folders, and tells you what's wrong in
          plain language.
        </li>
        <li>
          <strong>The big hammer:</strong> shut the whole Linux environment down and start fresh. In
          PowerShell:
          {@render cmd("wsl-shutdown", "wsl --shutdown", "Completely safe — nothing is deleted. Wait ~10 seconds, then start the server again from Home.")}
        </li>
      </ol>
    </div>
  </details>

  <details class="card">
    <summary>Windows Update broke it</summary>
    <div class="body">
      <p>
        A Windows update (or a Docker Desktop update) can leave the Linux environment in a confused
        state. The fix is the same big hammer as above:
      </p>
      <ol>
        <li>
          Run in PowerShell:
          {@render cmd("wsl-shutdown-2", "wsl --shutdown", "")}
        </li>
        <li>If you use Docker Desktop, start it again and wait for "Engine running".</li>
        <li>
          Run Doctor from
          <button class="navlink" onclick={() => onnav?.("tools")}>the Tools page</button>, then start
          the server from Home.
        </li>
      </ol>
    </div>
  </details>

  <details class="card">
    <summary>Where is my stuff? (characters, backups)</summary>
    <div class="body">
      <ul>
        <li>
          <strong>Characters save automatically</strong> every 15 minutes while playing, and again every
          time the server stops normally.
        </li>
        <li>
          <strong>Backups</strong> are managed from
          <button class="navlink" onclick={() => onnav?.("backups")}>the Backups page</button> (create,
          restore, download). On disk they live inside the Linux environment at
          <code>~/.dml/backups</code> — from Windows Explorer that's:
          {@render cmd("backup-path", "\\\\wsl$\\dml-arch\\home\\dml\\.dml\\backups", "Paste into the Explorer address bar (Windows key + R also works).")}
        </li>
        <li>
          Backups survive removing a game from the Library — only the server itself is deleted.
        </li>
      </ul>
    </div>
  </details>

  <details class="card">
    <summary>Community</summary>
    <div class="body">
      <p>
        This launcher builds on the free <strong>Dad's MMO Lab</strong> project — guides, installers and
        a community of parents running their own little MMO servers.
      </p>
      <button class="navlink" onclick={community}>Dad's MMO Lab on GitHub ↗</button>
    </div>
  </details>
</section>

<style>
  .content { padding: 20px 24px; overflow-y: auto; display: flex; flex-direction: column; gap: 12px; box-sizing: border-box; }
  .bar h2 { margin: 0; font-size: 18px; }
  .card { background: #0d1117; border: 1px solid #30363d; border-radius: 8px; padding: 0; }
  summary { cursor: pointer; padding: 12px 16px; font-weight: 600; font-size: 15px; color: #f0f6fc; user-select: none; }
  summary:hover { color: #58a6ff; }
  .body { padding: 0 16px 14px; font-size: 14px; line-height: 1.55; color: #c9d1d9; }
  .body p, .body li { margin: 0 0 8px; }
  .body ol, .body ul { margin: 0; padding-left: 22px; }
  code { background: #161b22; border: 1px solid #21262d; border-radius: 4px; padding: 1px 5px; font-family: Consolas, monospace; font-size: 12.5px; }
  .cmdblock { display: flex; gap: 8px; align-items: stretch; margin: 8px 0 4px; }
  .cmdblock pre { flex: 1; margin: 0; background: #161b22; border: 1px solid #30363d; border-radius: 6px; padding: 8px 10px; font-family: Consolas, monospace; font-size: 12.5px; color: #c9d1d9; overflow-x: auto; white-space: pre; }
  .copy { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 0 12px; cursor: pointer; font-size: 12.5px; flex-shrink: 0; }
  .copy:hover { border-color: #58a6ff; }
  .cmdnote { color: #8b949e; font-size: 12.5px; margin: 0 0 8px; }
  .navlink { background: none; border: none; padding: 0; color: #58a6ff; cursor: pointer; font-size: inherit; text-decoration: underline; }
  .muted { color: #8b949e; margin: 0; }
</style>
