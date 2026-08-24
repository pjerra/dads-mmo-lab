import { describe, it, expect } from "vitest";

// Sources via import.meta.glob(?raw) — the convention feature-keys.test.ts and
// soap-surface.test.ts already use (the app has no @types/node).
const SOURCES = import.meta.glob(["./pages/Home.svelte", "./pages/Library.svelte"], {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

function find(suffix: string): string {
  const hit = Object.entries(SOURCES).find(([f]) => f.endsWith(suffix));
  if (!hit) throw new Error(`no source for ${suffix} — the glob is wrong`);
  return hit[1];
}

/**
 * Strip comments before matching.
 *
 * Both files carry long prose about starting the server, and this file's whole
 * point is that Home OFFERS a start rather than DISCUSSING one. The repo has
 * been bitten twice (2026-08-01) by scans that read an explanation as the
 * thing itself.
 */
function code(src: string): string {
  return src
    .replace(/<!--[\s\S]*?-->/g, "")
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/(^|[^:])\/\/.*$/gm, "$1");
}

/** The `{#if …}` / `{:else if …}` / `{:else}` / `{/if}` block a line sits in. */
function branchAround(src: string, needle: string): string {
  const at = src.indexOf(needle);
  if (at < 0) throw new Error(`"${needle}" not found`);
  const before = src.slice(0, at);
  const start = Math.max(
    before.lastIndexOf("{#if "),
    before.lastIndexOf("{:else if "),
    before.lastIndexOf("{:else}"),
  );
  const rest = src.slice(start);
  const endRel = Math.min(
    ...[rest.indexOf("{:else", 1), rest.indexOf("{/if}", 1)].filter((i) => i > 0),
  );
  return rest.slice(0, endRel > 0 ? endRel : rest.length);
}

describe("Home can start the server in the state where you most need it to", () => {
  const home = code(find("Home.svelte"));
  const library = code(find("Library.svelte"));

  it("uses the SAME start call Library uses", () => {
    // Not a lookalike, not a second code path: one wrapper, so a fix to the
    // start behaviour cannot reach one page and miss the other.
    expect(library).toContain("gamesStart");
    expect(home).toContain("gamesStart");
  });

  /**
   * THE BUG THIS FILE EXISTS FOR (reported live, 2026-08-02).
   *
   * `games status` needs a live Docker engine. With Docker Desktop stopped —
   * the normal state of a machine that was just booted — the status call threw,
   * Home fell into its "Couldn't reach the DML backend" branch, and that branch
   * rendered an error message and nothing else. Home became read-only in
   * exactly the situation it exists for, and Library was the only page in the
   * app that could start a server (it lists titles from DISK, so it never
   * depended on the engine answering).
   *
   * Anchored on the ERROR BRANCH specifically, not on "Home contains a Start
   * button somewhere" — the running/stopped branch has always had one, so the
   * loose assertion passed throughout the bug.
   */
  it("offers Start from the backend-unreachable branch, not just when status is readable", () => {
    const branch = branchAround(home, "Couldn't reach the DML backend");
    expect(branch).toMatch(/act\(\s*["']start["']\s*\)/);
  });

  it("keeps Start and Stop on the right sides of the running check", () => {
    // The running/stopped pair, split at its own `{:else}`. Guards the
    // ORIGINAL button too: without this, deleting it would leave only the
    // error-branch button and the test above would still pass.
    const at = home.indexOf('{#if containerState === "running"}');
    expect(at).toBeGreaterThan(-1);
    const block = home.slice(at, home.indexOf("{/if}", at));
    const [running, stopped] = block.split("{:else}");
    expect(stopped, "no {:else} — the stopped branch is gone").toBeTruthy();
    expect(running).toMatch(/act\(\s*["']stop["']\s*\)/);
    expect(running).not.toMatch(/act\(\s*["']start["']\s*\)/);
    expect(stopped).toMatch(/act\(\s*["']start["']\s*\)/);
    expect(stopped).not.toMatch(/act\(\s*["']stop["']\s*\)/);
  });

  it("does not grow a second Docker-only start affordance", () => {
    // `games start` brings the engine up itself before composing — the user's
    // own terminal shows "Docker engine is down. Starting Docker Desktop...".
    // A separate button would be a second way to do what Start already does,
    // and the two would drift.
    expect(home).not.toMatch(/startDockerEngine|ensureEngine\s*\(/);
  });

  it("a not-installed title is an empty state that points at Library, never the error card", () => {
    // On a fresh machine `games status` answers NOT_FOUND: nothing is wrong,
    // nothing is installed. Drawing that as "Couldn't reach the DML backend"
    // with a CLI hint was the first thing a new user saw (VM run, 2026-08-25).
    expect(home).toMatch(/err\.code\s*===\s*["']NOT_FOUND["']/);
    const branch = branchAround(home, "No server installed yet");
    expect(branch).toMatch(/onnav\?\.\(\s*["']library["']\s*\)/);
    expect(branch).not.toContain("Couldn't reach the DML backend");
    // And the branch order: the empty state is tested BEFORE the error card.
    expect(home.indexOf("No server installed yet")).toBeLessThan(home.indexOf("Couldn't reach the DML backend"));
  });

  it("the branch matcher can actually fail", () => {
    // Non-vacuity: branchAround must not return the whole file (which would
    // make every assertion above pass regardless of where the button lives).
    const fake = `{#if a}\n  <p>no button here</p>\n{:else}\n  <button onclick={() => act("start")}>Start</button>\n{/if}`;
    expect(branchAround(fake, "no button here")).not.toMatch(/act\(\s*["']start["']\s*\)/);
    expect(branchAround(fake, "Start<")).toMatch(/act\(\s*["']start["']\s*\)/);
  });
});
