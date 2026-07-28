import { describe, it, expect } from "vitest";

// "Restart Docker in the distro" exists in TWO places: the card that performs
// it (Tools) and the jump button on Home's soap_unreachable status card. The
// card is deliberately WSL-only -- native mode drives Docker Desktop and has
// no dml-arch distro to restart -- so a jump button that renders in native
// mode is a dead end: it navigates to a page where the promised card does not
// exist.
//
// docker-restart.test.ts pins the predicate itself (dockerRestartCardVisible)
// in isolation; nothing pinned that both UI sites are gated on it. This suite
// is that pairing guard. It reads the component sources -- same technique and
// same reason as taskbar-pairing.test.ts, and via import.meta.glob(?raw)
// rather than node:fs because the app has no @types/node.
const SOURCES = import.meta.glob("./**/*.svelte", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

// The user-visible string both sites share. Home's is the jump button's
// label, Tools' is the card heading.
const PHRASE = "Restart Docker in the distro";
const PREDICATE = "dockerRestartCardVisible(";
const HOME = "./pages/Home.svelte";
const TOOLS = "./pages/Tools.svelte";

/**
 * Markup only -- everything after the last `</script>`. Both components
 * mention the feature in script comments too, and a comment is not a rendered
 * site; scanning them would report noise instead of dead-end buttons.
 */
function markup(source: string): string {
  const close = source.lastIndexOf("</script>");
  return close === -1 ? source : source.slice(close + "</script>".length);
}

/**
 * The `{#if}` conditions that enclose `index`, outermost first. Tracks Svelte
 * block tokens with a stack: `{#if C}` pushes C, `{:else if C}` / `{:else}`
 * REPLACE the top (that branch is no longer guarded by the previous
 * condition), `{/if}` pops. Only if-blocks are tracked -- each/await/key close
 * with their own tags, so they cannot unbalance this.
 */
function enclosingConditions(source: string, index: number): string[] {
  const stack: string[] = [];
  const re = /\{#if\s([^}]*)\}|\{:else if\s([^}]*)\}|\{:else\}|\{\/if\}/g;
  for (let m = re.exec(source); m !== null && m.index < index; m = re.exec(source)) {
    if (m[0].startsWith("{#if")) stack.push(m[1]);
    else if (m[0].startsWith("{:else if")) stack[stack.length - 1] = m[2];
    else if (m[0] === "{:else}") stack[stack.length - 1] = "(else branch)";
    else stack.pop();
  }
  return stack;
}

/** Every offset of `needle` in `hay`. */
function occurrences(hay: string, needle: string): number[] {
  const at: number[] = [];
  for (let i = hay.indexOf(needle); i !== -1; i = hay.indexOf(needle, i + 1)) at.push(i);
  return at;
}

describe("docker-restart UI is gated on one shared predicate", () => {
  it("finds both components to check", () => {
    // Non-vacuity: a page move or a glob change must not turn the assertions
    // below into a scan over nothing.
    expect(Object.keys(SOURCES)).toContain(HOME);
    expect(Object.keys(SOURCES)).toContain(TOOLS);
  });

  it("finds the shared user-visible string rendered by both components", () => {
    // Second non-vacuity guard: reword the button/heading and this fires
    // instead of the pairing check silently passing over zero sites.
    expect(occurrences(markup(SOURCES[HOME]), PHRASE).length).toBeGreaterThanOrEqual(1);
    expect(occurrences(markup(SOURCES[TOOLS]), PHRASE).length).toBeGreaterThanOrEqual(1);
  });

  it("renders every docker-restart site only under dockerRestartCardVisible()", () => {
    const unguarded: string[] = [];
    for (const file of [HOME, TOOLS]) {
      const body = markup(SOURCES[file]);
      for (const at of occurrences(body, PHRASE)) {
        const conds = enclosingConditions(body, at);
        if (!conds.some((c) => c.includes(PREDICATE))) {
          unguarded.push(`${file} @${at} [${conds.join(" > ") || "no {#if} at all"}]`);
        }
      }
    }
    expect(unguarded).toEqual([]);
  });

  it("imports that predicate rather than re-deriving the backend on Home", () => {
    // "Reuse the same predicate" is the whole point: an inline
    // `!nativeStatus.native` on Home would satisfy the check above while
    // drifting the moment the card's rule changes (e.g. the null-status case).
    expect(SOURCES[HOME]).toContain("dockerRestartCardVisible");
    expect(SOURCES[HOME]).toContain("$lib/docker-restart");
    expect(SOURCES[HOME]).not.toMatch(/\.native\b/);
  });
});
