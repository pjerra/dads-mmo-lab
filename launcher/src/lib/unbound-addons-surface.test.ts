import { describe, it, expect } from "vitest";

// Source via import.meta.glob(?raw) — the convention feature-keys.test.ts and
// soap-surface.test.ts already use (the app has no @types/node).
const SOURCES = import.meta.glob(["./api.ts", "./pages/Tools.svelte"], {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

function find(suffix: string): string {
  const hit = Object.entries(SOURCES).find(([f]) => f.endsWith(suffix));
  if (!hit) throw new Error(`no source for ${suffix} — the glob is wrong`);
  return hit[1];
}

/** Strip comments before matching — both files EXPLAIN these rules in prose. */
function code(src: string): string {
  return src
    .replace(/<!--[\s\S]*?-->/g, "")
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/(^|[^:])\/\/.*$/gm, "$1");
}

describe("the client-addon surface keeps its two load-bearing properties", () => {
  const api = code(find("api.ts"));
  const tools = code(find("pages/Tools.svelte"));

  it("strips comments rather than grepping raw source", () => {
    // Non-vacuity for the stripper: both files discuss these very rules.
    expect(code("// invoke('x', { dir })")).not.toContain("invoke");
    expect(code("<!-- clientPath -->\n<div/>")).not.toContain("clientPath");
    expect(code("const u = 'https://x/y';")).toContain("https://x/y");
  });

  /**
   * THE SECURITY PROPERTY. The addon install writes 43 files into a directory.
   * That directory is resolved on the RUST side from the saved client path, and
   * the export directory comes from a native folder picker — neither is ever
   * named by the webview. Same rule `save_text_file` was written to,
   * for the same reason: a compromised webview must not get to choose where
   * files land.
   */
  it("never passes a destination path from the webview", () => {
    // Extract each function BODY by name. A regex over the invoke call alone
    // is not enough: the generic type argument carries `;` separators, which
    // is what made the first version of this test match nothing and pass for
    // the wrong reason.
    const body = (name: string): string => {
      const at = api.indexOf(`export async function ${name}(`);
      expect(at, `${name} should exist`).toBeGreaterThan(-1);
      const end = api.indexOf("\n}", at);
      return api.slice(at, end);
    };

    /**
     * The invoke's ARGUMENT LIST only — a balanced-paren scan from the first
     * `(` after `invoke`.
     *
     * Scoping matters more than it looks: a loose match over the whole
     * function hits the RESPONSE type (`{ addons_dir: string; … }`) and the
     * returned object (`{ dir: r.addons_dir, … }`), both of which mention
     * "dir" and neither of which is an argument. That is how this test failed
     * on correct code the first time.
     */
    const invokeArgs = (fn: string): string => {
      const iv = fn.indexOf("invoke");
      const open = fn.indexOf("(", iv);
      let depth = 0;
      for (let i = open; i < fn.length; i++) {
        if (fn[i] === "(") depth++;
        else if (fn[i] === ")") {
          depth--;
          if (depth === 0) return fn.slice(open + 1, i);
        }
      }
      throw new Error("unbalanced invoke call");
    };

    for (const name of ["wowUnboundAddonsInstall", "wowUnboundAddonsExport"]) {
      const fn = body(name);
      expect(fn, `${name} should call invoke`).toContain("invoke");
      const args = invokeArgs(fn);
      // Exactly one argument, the command name. Tauri's second argument is the
      // ONLY way a path could travel from here, so there must not be one.
      expect(args, `${name} must not send an argument object`).not.toContain("{");
      expect(args.split(",").filter((a) => a.trim()).length, `${name} arg count`).toBe(1);
      expect(args).toContain("wow_unbound_addons_");
    }

    // Non-vacuity: the scanner must find an argument object when one IS there.
    expect(invokeArgs('invoke<X>("cmd", { dir })')).toContain("{");
  });

  /**
   * A CANCELLED PICKER IS NOT A FAILURE. `wowUnboundAddonsExport` resolves
   * `null` when the user closes the dialog. Reporting that as "export failed"
   * is a small lie the user cannot correct, and the kind that trains people to
   * distrust every other message on the page.
   */
  it("treats a cancelled export as a non-event, not an error", () => {
    const fn = tools.match(/async function exportAddons\(\)[\s\S]*?\n  \}/)?.[0] ?? "";
    expect(fn, "exportAddons should exist").toBeTruthy();
    // The null case must be handled in the SUCCESS path (a ternary or an if on
    // the result), never by assigning the error field.
    expect(fn).toMatch(/\br\s*\?/);
    // Only the catch may set an error VALUE. The `addonErr = null` reset at
    // the top is not that -- an earlier version of this test forbade every
    // assignment and failed on the reset, which is how a test ends up
    // "fixed" by weakening the thing it was guarding.
    const before = fn.slice(0, fn.indexOf("catch"));
    const sets = before.match(/addonErr\s*=\s*([^;]+);/g) ?? [];
    for (const s of sets) {
      expect(s, "only a null reset may precede the catch").toMatch(/=\s*null\s*;/);
    }
  });

  it("the export button says it opens a picker rather than acting immediately", () => {
    // The ellipsis is the platform convention for "this asks you something".
    expect(tools).toMatch(/Export for players…|Export for players\.\.\./);
  });

  it("both buttons are disabled while a 90-minute install is running", () => {
    // They write into the same client the install's own final step writes to,
    // and the install slot is global -- racing them is a file fight nobody
    // asked for.
    const install = tools.match(/onclick=\{installAddons\}/)?.index ?? -1;
    const exportAt = tools.match(/onclick=\{exportAddons\}/)?.index ?? -1;
    expect(install).toBeGreaterThan(-1);
    expect(exportAt).toBeGreaterThan(-1);
    for (const at of [install, exportAt]) {
      const button = tools.slice(Math.max(0, at - 200), at);
      expect(button).toContain("toolBusy");
    }
  });
});
