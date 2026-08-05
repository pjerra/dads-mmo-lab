import { describe, it, expect } from "vitest";

// Sources come in via import.meta.glob(?raw) rather than node:fs -- the app
// has no @types/node, so a bare `node:fs` import fails `npm run check`.
// Same technique as soap-surface.test.ts / taskbar-pairing.test.ts.
const SOURCES = import.meta.glob("./pages/Config.svelte", {
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
 * This repo has been bitten TWICE (2026-08-01) by source scans that read an
 * explanation of a thing as the thing itself. A first draft of this test
 * stripped only `<!-- -->` and a naive `//.*$`, which has two holes: a
 * `/* ... *\/` block comment in the `<script>` block can satisfy the match on
 * its own (nothing here strips those), and a naive `//` rule treats the `//`
 * inside `https://` as a comment marker, silently truncating any real text
 * that follows it on the same line. Same shape as soap-surface.test.ts's and
 * status-surface.test.ts's `code()` — kept identical on purpose.
 */
function code(src: string): string {
  return src
    .replace(/<!--[\s\S]*?-->/g, "")
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/(^|[^:])\/\/.*$/gm, "$1");
}

describe("the comment stripper", () => {
  it("cannot be satisfied by a comment, in any of the three forms", () => {
    // Non-vacuity for the stripper itself: each of these plants the phrase
    // ONLY inside a comment. If the assertions below ever pass on a component
    // that only has the copy in a comment, this is the test that must catch
    // it first.
    expect(code("// keeps running when you close it\nconst a = 1;")).not.toMatch(
      /keeps? running when you close/i,
    );
    expect(code("<!-- keeps running when you close, on WSL --><div/>")).not.toMatch(
      /keeps? running when you close/i,
    );
    expect(
      code("/* keeps running when you close, on WSL */\nconst a = 1;"),
    ).not.toMatch(/keeps? running when you close/i);
  });

  it("does not treat the // in a URL as a line comment", () => {
    // Protects real content: a naive `//.*$` stripper would eat everything
    // after `https://` up to the newline, which is exactly the kind of
    // silent-never-matches failure this branch keeps producing.
    expect(code("const u = 'https://example.com/x'; // trailing note")).toContain(
      "https://example.com/x",
    );
  });
});

describe("the backend picker states the survival difference", () => {
  it("says the server does not survive closing the launcher on WSL", () => {
    const src = code(find("pages/Config.svelte"));
    expect(src).toMatch(/keeps? running when you close/i);
    expect(src).toMatch(/WSL/);
  });
});
