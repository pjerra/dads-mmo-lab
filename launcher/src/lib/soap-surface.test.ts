import { describe, it, expect } from "vitest";

// Sources come in via import.meta.glob(?raw), the same technique
// feature-keys.test.ts uses (the app has no @types/node).
const SOURCES = import.meta.glob(
  ["./pages/Library.svelte", "../routes/+page.svelte"],
  { query: "?raw", import: "default", eager: true },
) as Record<string, string>;

function find(suffix: string): string {
  const hit = Object.entries(SOURCES).find(([f]) => f.endsWith(suffix));
  if (!hit) throw new Error(`no source for ${suffix} — the glob is wrong`);
  return hit[1];
}

/**
 * Strip comments before matching.
 *
 * This repo was bitten TWICE on 2026-08-01 by source scans that read an
 * explanation of a thing as the thing itself. Library.svelte is dense with
 * `// … soap …` prose about why the step worked the way it did, and a raw grep
 * would report the surface as still present after it was removed — a red test
 * on correct code, which is how a scan like this gets deleted.
 */
function code(src: string): string {
  return src
    .replace(/<!--[\s\S]*?-->/g, "")
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/(^|[^:])\/\/.*$/gm, "$1");
}

describe("the SOAP account step is a shell surface, not a Library one", () => {
  it("strips comments rather than grepping raw source", () => {
    // Non-vacuity for the stripper itself.
    expect(code("// SoapBootstrap\nconst a = 1;")).not.toContain("SoapBootstrap");
    expect(code("<!-- soapSetupState -->\n<div/>")).not.toContain("soapSetupState");
    expect(code("import X from './SoapBootstrap.svelte';")).toContain("SoapBootstrap");
    // A protocol-relative URL must survive the line-comment rule.
    expect(code("const u = 'https://x/y';")).toContain("https://x/y");
  });

  it("Library.svelte has no SOAP surface left", () => {
    const src = code(find("pages/Library.svelte"));
    for (const token of [
      "SoapBootstrap",
      "soapSetupState",
      "wowSoapStatus",
      "refreshSoapNeed",
      "clearSoapSetup",
    ]) {
      expect(src, `Library still references ${token}`).not.toContain(token);
    }
  });

  it("the shell carries both the banner and the fallback card", () => {
    // A fallback reachable from one page only is the same bug this change
    // removes; it must not survive in the failure path.
    //
    // Every marker here is MARKUP, never a bare identifier. The first version
    // of this test asserted "SoapBootstrap" and "soapSetupState", both of which
    // the two import lines at the top of the shell satisfy on their own -- so
    // deleting the whole banner block left the test named for the banner green,
    // and nothing else would have caught it either (tsconfig sets no
    // noUnusedLocals, so `npm run check` is happy with an orphaned import). The
    // shipped bug that hides behind that: the launcher creates a GM3 account on
    // the user's server and tells them nothing.
    const src = code(find("routes/+page.svelte"));
    for (const [marker, what] of [
      ["soapSetupState.autoResult", "the success banner"],
      ["soapSetupState.needed", "the gate on the manual fallback card"],
      ["<SoapBootstrap", "the manual fallback card itself"],
    ] as const) {
      expect(src, `the shell has lost ${what} (${marker})`).toContain(marker);
    }
  });

  it("dismissing the fallback card hides it without resolving it", () => {
    // ONE WORD is the whole difference, and getting it wrong is a regression
    // this project already shipped once. `ondismiss={clearSoapSetup}` drops
    // `soapSetupState.needed`, which is now raised by exactly one thing -- the
    // gave_up arm -- reachable only through a poll that the module-level
    // `autosetupSettled` flag has already switched off, with Library's
    // on-mount re-probe deleted. So "Later" put the step out of reach for the
    // rest of the process, on the one path where setup had already failed.
    //
    // The store's own suite proves dismissSoapSetup and clearSoapSetup differ;
    // nothing proved the shell hands over the right one. Swap the word back and
    // every other test here stays green.
    const src = code(find("routes/+page.svelte"));
    expect(src, "the fallback card must dismiss, not resolve").toContain(
      "ondismiss={dismissSoapSetup}",
    );
    expect(src, "the card is gated on dismissal too, or Later does nothing").toContain(
      "!soapSetupState.dismissed",
    );
  });
});
