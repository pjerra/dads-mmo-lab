import { describe, it, expect } from "vitest";

/**
 * One verdict-to-appearance mapping, used by everything that shows it.
 *
 * `statusLabel()` is tested thoroughly in server-status.test.ts. This file
 * tests something that suite structurally cannot: whether the surfaces
 * actually GO THROUGH it.
 *
 * They did not. Home's status card re-derived the whole chain inline behind ten
 * separate `!restartState.restarting &&` guards, while its own comment claimed
 * the card and the chip "share statusLabel() so the two can never disagree" --
 * true only of the install branch. So when a stop-in-flight flag was added, the
 * sidebar chip learned about it and the card did not, and a server on its way
 * down announced "World is running, but the launcher can't reach it" to a user
 * who had just pressed Stop (reported live, 2026-08-03).
 *
 * Every assertion here is about a CALL SHAPE rather than a rendered string, so
 * it stays green when the wording changes and goes red when a surface starts
 * answering the question by itself again.
 */

const SOURCES = import.meta.glob(
  ["./pages/Home.svelte", "../routes/+page.svelte"],
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
 * The repo rule, learned twice on 2026-08-01 and then AGAIN here: this very
 * file's subject is documented in a long HTML comment that names the removed
 * `class:warn={!restartState.restarting && d.verdict === ...}` shape verbatim,
 * so a raw scan would read the explanation as the thing and fail on correct
 * code. Shared shape with soap-surface.test.ts's `code()`.
 */
function code(src: string): string {
  return src
    .replace(/<!--[\s\S]*?-->/g, "")
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/(^|[^:])\/\/.*$/gm, "$1");
}

/** Argument lists of every `statusLabel(...)` call, split at top-level commas. */
function statusLabelCalls(src: string): string[][] {
  const calls: string[][] = [];
  const needle = "statusLabel(";
  let at = src.indexOf(needle);
  while (at !== -1) {
    let depth = 0;
    let i = at + needle.length - 1;
    const args: string[] = [];
    let cur = "";
    for (; i < src.length; i++) {
      const c = src[i];
      if (c === "(" || c === "[" || c === "{") depth++;
      else if (c === ")" || c === "]" || c === "}") {
        depth--;
        if (depth === 0) break;
      }
      if (depth === 1 && c === ",") {
        args.push(cur.trim());
        cur = "";
        continue;
      }
      if (!(depth === 1 && c === "(" && i === at + needle.length - 1)) cur += c;
    }
    if (cur.trim()) args.push(cur.trim());
    calls.push(args);
    at = src.indexOf(needle, i);
  }
  return calls;
}

describe("the argument-list scanner", () => {
  it("counts top-level arguments and ignores nested punctuation", () => {
    // Non-vacuity. A scanner that silently found nothing would make every
    // assertion below pass on any source at all -- the exact failure this
    // repo has recorded twice.
    expect(statusLabelCalls("statusLabel(a, b, c, d)")).toEqual([["a", "b", "c", "d"]]);
    expect(statusLabelCalls("statusLabel(f(x, y), {p: 1, q: 2}, c, d)")).toEqual([
      ["f(x, y)", "{p: 1, q: 2}", "c", "d"],
    ]);
    expect(statusLabelCalls("statusLabel(a ?? null, b)")).toEqual([["a ?? null", "b"]]);
    expect(statusLabelCalls("statusLabel(a) + statusLabel(b, c)")).toEqual([["a"], ["b", "c"]]);
    expect(statusLabelCalls("nothing here")).toEqual([]);
  });
});

describe("every surface that shows server state goes through statusLabel()", () => {
  const FILES = ["pages/Home.svelte", "routes/+page.svelte"] as const;

  it("finds the call sites at all", () => {
    for (const f of FILES) {
      expect(statusLabelCalls(code(find(f))).length, `${f} calls statusLabel`).toBeGreaterThan(0);
    }
  });

  /**
   * The load-bearing one.
   *
   * `stopping` has a DEFAULT, so a three-argument call still compiles and still
   * type-checks -- which is precisely how the card silently kept its old
   * behaviour while the chip got the fix. The default exists for the tests and
   * for honesty about which parameter is rare; it must not become a way for a
   * real surface to opt out. Same reasoning that made `install` required.
   */
  it("passes ALL FOUR arguments at every call site, never relying on the default", () => {
    for (const f of FILES) {
      for (const args of statusLabelCalls(code(find(f)))) {
        expect(args.length, `${f}: statusLabel(${args.join(", ")})`).toBe(4);
        expect(args[3], `${f}: 4th arg is the stop flag`).toContain("stopping");
      }
    }
  });
});

describe("Home's status card does not re-derive appearance from the verdict", () => {
  /** `class:name={expr}` directives, as [name, expr] pairs. */
  function classDirectives(src: string): [string, string][] {
    const out: [string, string][] = [];
    const re = /class:([A-Za-z-]+)=\{/g;
    let m: RegExpExecArray | null;
    while ((m = re.exec(src)) !== null) {
      let depth = 1;
      let i = m.index + m[0].length;
      let expr = "";
      for (; i < src.length && depth > 0; i++) {
        const c = src[i];
        if (c === "{") depth++;
        else if (c === "}") {
          depth--;
          if (depth === 0) break;
        }
        expr += c;
      }
      out.push([m[1], expr]);
    }
    return out;
  }

  it("reads class directives", () => {
    // Non-vacuity again, including the nested-brace case that would otherwise
    // truncate an expression and hide a `verdict` mention inside it.
    expect(classDirectives('<b class:warn={a === "x"}></b>')).toEqual([["warn", 'a === "x"']]);
    expect(classDirectives("<b class:crash={f({k: 1})}></b>")).toEqual([["crash", "f({k: 1})"]]);
  });

  /**
   * The alarming styling -- the red "warn" card, the pulsing "crash" dot -- is
   * now keyed on `s.dot`, and that indirection is the fix rather than an
   * incidental refactor. `statusLabel()` returns "mid" for any transient state,
   * so "a server that is settling never renders as a fault" becomes a property
   * of one function instead of a convention every directive has to restate.
   *
   * A directive that consults `verdict` directly has opted back out of it.
   */
  it("keys its status classes on the label's dot, not on the raw verdict", () => {
    const src = code(find("pages/Home.svelte"));
    const offenders = classDirectives(src).filter(([, expr]) => expr.includes("verdict"));
    expect(
      offenders.map(([n, e]) => `class:${n}={${e}}`),
      "status appearance must come from statusLabel()'s dot",
    ).toEqual([]);
  });
});
