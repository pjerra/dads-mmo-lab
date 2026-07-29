import { describe, it, expect } from "vitest";

// first-run.test.ts pins firstRunButton as a pure function: given
// `rechecking: true`, the button is disabled and says so. That is only half the
// guarantee, and the weaker half. The flag has to REACH it.
//
// It reaches it through exactly one wire: the shell owns `probing` (it owns the
// probe) and hands it to FirstRun.svelte as `rechecking`. Nothing pinned that
// wire, and the component declared the prop with a default (`rechecking =
// false`), so deleting the attribute from +page.svelte restored the original
// bug -- a Check-again button that stays enabled and unchanged for the full
// cold-start budget while every re-click is dropped -- with all 53 unit tests
// still green. A defaulted prop is a silent-failure shape: an omission renders
// as "no probe is running", forever.
//
// So this suite pins the wire itself, in the same source-reading style and for
// the same reason as taskbar-pairing.test.ts and docker-restart-pairing.test.ts
// (a convention that drifts silently needs a guard that reads the convention).
// Sources come in via import.meta.glob(?raw) rather than node:fs -- the app has
// no @types/node, so a bare `node:fs` import fails `npm run check`.
const SOURCES = import.meta.glob("../**/*.svelte", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

// Vite normalises glob keys against the importing module, so a sibling in
// src/lib comes back as "./X.svelte" even under a "../**" pattern.
const SHELL = "../routes/+page.svelte";
const COMPONENT = "./FirstRun.svelte";

/** The `<FirstRun ... />` element as written in the shell, or "". */
function firstRunTag(source: string): string {
  const at = source.indexOf("<FirstRun");
  if (at === -1) return "";
  const end = source.indexOf("/>", at);
  return end === -1 ? "" : source.slice(at, end + 2);
}

/** A named top-level function's body text, or "". */
function fnBody(source: string, name: string): string {
  const at = source.search(new RegExp(`(?:async\\s+)?function\\s+${name}\\s*\\(`));
  if (at === -1) return "";
  const next = source.slice(at + 1).search(/^[ \t]{0,2}(?:async\s+)?function\s/m);
  return next === -1 ? source.slice(at) : source.slice(at, at + 1 + next);
}

/** `[destructuring, type]` of the component's single `$props()` declaration. */
function propsDecl(source: string): [string, string] {
  const m = /let\s*\{([\s\S]*?)\}\s*:\s*\{([\s\S]*?)\}\s*=\s*\$props\(\)/.exec(source);
  return m ? [m[1], m[2]] : ["", ""];
}

describe("the first-run screen's busy flag is actually wired", () => {
  it("finds both sources to check", () => {
    // Non-vacuity: a file move or a glob change must not turn the assertions
    // below into a scan over nothing.
    expect(Object.keys(SOURCES)).toContain(SHELL);
    expect(Object.keys(SOURCES)).toContain(COMPONENT);
  });

  it("finds the FirstRun element the shell renders", () => {
    // Second non-vacuity guard: reshape the tag (or drop the screen entirely)
    // and this fires, instead of the wiring check passing over an empty string.
    expect(firstRunTag(SOURCES[SHELL])).toContain("state={firstRun}");
  });

  it("hands the shell's probe flag to the screen as `rechecking`", () => {
    // THE assertion. Delete `rechecking={probing}` from +page.svelte and this
    // is what fails.
    expect(firstRunTag(SOURCES[SHELL])).toMatch(/\brechecking=\{probing\}/);
  });

  it("makes that prop required, so an omission cannot pass silently", () => {
    // Belt to the assertion above's braces, at a different gate: with no
    // default, dropping the attribute is a svelte-check error rather than a
    // permanent `false`. `rechecking?: boolean` would be the same silent shape
    // by another spelling, so the type is pinned too.
    const [destructured, types] = propsDecl(SOURCES[COMPONENT]);
    expect(destructured).toMatch(/\brechecking\b/); // non-vacuity
    expect(destructured).not.toMatch(/\brechecking\s*=/);
    expect(types).toMatch(/\brechecking\s*:\s*boolean/);
    expect(types).not.toMatch(/\brechecking\s*\?/);
  });

  it("keeps `probing` meaning what the button thinks it means", () => {
    // The wire is only worth pinning if the flag on the other end of it is the
    // probe's own in-flight state. A `probing` that is set but never cleared
    // (or cleared on the happy path only) would jam the button on forever after
    // one thrown probe, which is the same dead end in the opposite direction.
    const body = fnBody(SOURCES[SHELL], "probeBackend");
    expect(body).toMatch(/probing\s*=\s*true/); // non-vacuity + it is raised
    const cleared = body.indexOf("probing = false");
    const fin = body.indexOf("finally");
    expect(cleared).toBeGreaterThan(-1);
    expect(fin).toBeGreaterThan(-1);
    expect(fin).toBeLessThan(cleared);
  });
});
