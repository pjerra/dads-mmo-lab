import { describe, it, expect } from "vitest";
import { code, sourceFinder, blockAfter, blockOf, normalizeEol } from "./source-scan";

/**
 * "AWAIT RETURNED" IS NOT SUCCESS.
 *
 * `dml_core::runner::run_stream` returns `Ok(code)` for EVERY exit code: on a
 * non-zero exit it synthesizes an `error` event and still resolves Ok.
 * `stream_action`/`stream_args` in lib.rs then throw the code away
 * (`.map(|_exit| ())`), and the native path is worse still —
 * `run_games_lifecycle_native` resolves `Ok(())` BY DESIGN, its own doc
 * comment saying "domain failures already traveled in the event stream".
 *
 * So on the frontend a lifecycle promise resolves whether the restart worked,
 * whether compose refused, or whether the DB never became healthy and
 * `dml-start.sh` bailed before `compose up -d`. The ONLY honest signal is the
 * terminal `done` event; a failure arrives as `error` instead.
 *
 * Five call sites currently get this right, each with a comment saying why.
 * NOTHING PINNED IT. Every pure test in this repo — restart-state, apply-needed,
 * term-buffer, the lot — stays green under a wholesale revert to
 * promise-derived success, because the promise-vs-event distinction lives in
 * WIRING and pure tests structurally cannot see wiring. The failure it lets
 * through is silent and it is the mirror image of the bug the pending-apply
 * banner exists to prevent: the banner is cleared for a restart that never
 * happened, so the user is told their change is live when it is not, and there
 * is no signal anywhere that anything went wrong.
 *
 * Two traps this repo has recorded, which every scanner below has to survive:
 *   1. matching a string inside a COMMENT rather than in real code (bitten
 *      twice on 2026-08-01) — hence `code()` strips first, and all three files
 *      scanned here discuss this very invariant, by name, in the comments the
 *      scan must ignore;
 *   2. passing on an EMPTY match set — hence `sourceFinder`/`blockOf` THROW on
 *      a miss, the scanners have their own non-vacuity suite below, and the
 *      call-site inventory is asserted before anything is asserted about it.
 */

const SOURCES = import.meta.glob(
  ["./pages/Home.svelte", "./pages/Config.svelte", "./ModuleFiles.svelte"],
  { query: "?raw", import: "default", eager: true },
) as Record<string, string>;

const find = sourceFinder(SOURCES);

/**
 * The gate that makes a call honest: `if (<ident>.event === "done")`, with an
 * optional extra conjunct, immediately before the call it guards.
 *
 * Anchored with `$` so it describes the code that IMMEDIATELY precedes the
 * call. "the words appear somewhere above" is satisfied by a `done` gate
 * around something else entirely, three statements up.
 */
const DONE_GATE = /\bif\s*\(\s*\w+\.event\s*===\s*"done"(?:\s*&&[^)]*)?\)\s*\{?\s*$/;

/**
 * Home's `act()` runs three different lifecycle calls through one `finally`,
 * so it routes the verdict through a local flag instead of calling from inside
 * the handler. That is fine ONLY because the flag itself is done-derived, which
 * `home act()` below pins separately.
 */
const APPLIED_GATE = /\bif\s*\(\s*applied\s*\)\s*$/;

/** Every `clearApplyNeeded(` call, with the code that immediately precedes it. */
function applyClearSites(src: string): string[] {
  const leads: string[] = [];
  const needle = "clearApplyNeeded(";
  let at = src.indexOf(needle);
  while (at !== -1) {
    leads.push(src.slice(Math.max(0, at - 240), at));
    at = src.indexOf(needle, at + needle.length);
  }
  return leads;
}

/** Occurrences of `re` in `src`. */
function count(src: string, re: RegExp): number {
  return (src.match(new RegExp(re.source, re.flags.includes("g") ? re.flags : re.flags + "g")) ?? [])
    .length;
}

/**
 * The per-file inventory the whole suite is built on, in one place so the
 * CRLF-equivalence test can compare two flavours of the same source without
 * duplicating a single assertion.
 */
function inventory(src: string): { clears: number; gated: number; ungated: string[] } {
  const leads = applyClearSites(src);
  const ungated = leads.filter((l) => !DONE_GATE.test(l) && !APPLIED_GATE.test(l));
  return { clears: leads.length, gated: leads.length - ungated.length, ungated };
}

// ---------------------------------------------------------------------------

describe("the source scanners themselves", () => {
  // Non-vacuity. A scanner that quietly returned nothing would make every
  // assertion in this file pass against any source at all.
  it("extracts a block and is not fooled by braces inside strings", () => {
    expect(blockOf("function f() { a; }", "function f(")).toBe(" a; ");
    expect(blockOf('function f() { s = "}"; a; }', "function f(")).toBe(' s = "}"; a; ');
    expect(blockOf("function f() { if (x) { y; } }", "function f(")).toBe(" if (x) { y; } ");
    // The real shape: the anchor is a CALL head and the block is the callback
    // passed to it, several arguments in.
    expect(blockAfter('await gamesRestart(ID, false, (e) => { body; })', 0)).toBe(" body; ");
    // ...including the multi-line spelling `gamesStop` actually uses.
    expect(blockOf("await gamesStop(\n  ID,\n  (e) => {\n    x;\n  },\n  p,\n);", "await gamesStop(")).toContain("x;");
  });

  it("throws rather than returning empty when the anchor is gone", () => {
    expect(() => blockOf("function g() {}", "async function act(")).toThrow(/anchor not found/);
    expect(() => find("pages/Nope.svelte")).toThrow(/the glob is wrong/);
    // The -1-means-top-of-file trap, closed at the source.
    expect(() => blockAfter("function f() { a; }", -1)).toThrow(/never found/);
  });

  it("finds every clearApplyNeeded call and nothing else", () => {
    expect(applyClearSites("a; clearApplyNeeded(); b; clearApplyNeeded();").length).toBe(2);
    expect(applyClearSites("nothing here")).toEqual([]);
    // The import names the function without calling it. A scanner that counted
    // it would report an ungated site in every file and be deleted as noisy.
    expect(applyClearSites("import { noteApplyNeeded, clearApplyNeeded } from '$lib/x';")).toEqual([]);
  });

  it("recognises the honest gates and rejects the mutations they exist to catch", () => {
    expect(DONE_GATE.test('if (e.event === "done") ')).toBe(true);
    expect(DONE_GATE.test('if (ev.event === "done") ')).toBe(true);
    expect(DONE_GATE.test('if (e.event === "done" && restartState.apply === "world-restart") {\n  ')).toBe(true);
    // Wrong event: a `line` event fires for every progress line, so this
    // clears the banner the moment the command says anything at all.
    expect(DONE_GATE.test('if (e.event === "line") ')).toBe(false);
    // Wrong event: `error` IS the failure signal.
    expect(DONE_GATE.test('if (e.event === "error") ')).toBe(false);
    // Promise-derived: no gate at all in front of the call.
    expect(DONE_GATE.test("      });\n    ")).toBe(false);
    expect(APPLIED_GATE.test("      });\n    ")).toBe(false);
    // A `done` gate that closed three statements earlier does NOT gate this
    // call, and the `$` anchor is what says so.
    expect(DONE_GATE.test('if (e.event === "done") { x = 1; }\n    ')).toBe(false);
    expect(APPLIED_GATE.test("if (applied) ")).toBe(true);
    expect(APPLIED_GATE.test("if (!applied) ")).toBe(false);
  });

  it("matches code, not the prose about the code", () => {
    // All three scanned files explain this invariant in comments that quote
    // the exact shapes below.
    expect(applyClearSites(code('// if (e.event === "done") clearApplyNeeded();'))).toEqual([]);
    expect(applyClearSites(code('<!-- if (e.event === "done") clearApplyNeeded(); -->'))).toEqual([]);
    expect(applyClearSites(code('/* clearApplyNeeded() on done */'))).toEqual([]);
    // ...and a comment must not be able to SUPPLY a gate either: stripping it
    // leaves the real call ungated, which is the reading we want.
    const faked = code('// if (e.event === "done")\n    clearApplyNeeded();');
    expect(faked).not.toContain("done");
    expect(inventory(faked).ungated.length).toBe(1);
    // A protocol-relative URL must survive the line-comment rule.
    expect(code("const u = 'https://x/y';")).toContain("https://x/y");
  });

  it("reads the same inventory out of CRLF and LF flavours of the same file", () => {
    // Working-tree files here are CRLF and committed blobs are LF, so the same
    // source reaches this scan two ways depending on how it got to disk.
    for (const raw of Object.values(SOURCES)) {
      const lf = normalizeEol(raw);
      const crlf = lf.replace(/\n/g, "\r\n");
      const a = inventory(code(lf));
      const b = inventory(code(crlf));
      expect(b.clears).toBe(a.clears);
      expect(b.gated).toBe(a.gated);
    }
  });
});

// ---------------------------------------------------------------------------

describe("no surface clears the pending-apply banner outside a done gate", () => {
  /**
   * The whole-file sweep. The per-function tests below say the five known call
   * sites are right; this one says there is no SIXTH that is wrong, which is
   * the shape a regression actually arrives in — a new page copies the
   * `await …; clearApplyNeeded();` idiom because the promise resolving looks
   * like success everywhere else in the codebase.
   *
   * The counts are asserted first: a scanner that found nothing would make the
   * gate check below vacuously true.
   */
  const FILES: [string, number][] = [
    ["pages/Home.svelte", 2],
    ["pages/Config.svelte", 2],
    ["ModuleFiles.svelte", 1],
  ];

  it.each(FILES)("%s clears the banner only from a terminal event", (file, expected) => {
    const inv = inventory(code(find(file)));
    expect(
      inv.clears,
      `${file} has ${inv.clears} clearApplyNeeded() call sites, this suite knows about ` +
        `${expected}. A new one is not automatically wrong — but it has to be READ and ` +
        `this number bumped deliberately, because the whole point of this file is that ` +
        `an ungated clear is invisible to every other test in the repo.`,
    ).toBe(expected);
    expect(
      inv.ungated,
      `${file} clears the pending-apply banner without a \`done\` gate. The promise ` +
        `resolving is NOT success — run_stream returns Ok(code) for every exit code and ` +
        `run_games_lifecycle_native resolves Ok(()) by design — so this tells the user ` +
        `their change is live after a restart that may never have happened.`,
    ).toEqual([]);
  });
});

// ---------------------------------------------------------------------------

describe("Home act() derives its verdict from the done event", () => {
  const home = () => code(find("pages/Home.svelte"));
  const act = () => blockOf(home(), "async function act(");

  it("starts pessimistic", () => {
    // `let applied = true` would clear the banner on every run including the
    // ones that threw, and reads as a harmless initialiser.
    expect(act(), "the verdict flag must default to NOT applied").toMatch(
      /let\s+applied\s*=\s*false\s*;/,
    );
  });

  /**
   * THE MUTATION THIS EXISTS FOR.
   *
   * Both counts are needed and neither is enough alone. The gated count alone
   * passes when someone ADDS an ungated `applied = true` after the await; the
   * total alone passes when the gate is changed to `"line"`. Together they say
   * every assignment in the function is done-gated.
   */
  it("sets the flag ONLY from inside a stream handler, on the terminal event", () => {
    const body = act();
    const assigns = count(body, /\bapplied\s*=\s*true\b/);
    const gated = count(body, /if\s*\(\s*e\.event\s*===\s*"done"\s*\)\s*applied\s*=\s*true\b/);
    expect(assigns, "act() no longer records a verdict at all").toBe(2);
    expect(
      gated,
      "an `applied = true` in act() is not gated on the terminal `done` event. Awaiting a " +
        "lifecycle stream resolves Ok even when the CLI exited non-zero, so this clears the " +
        "pending-apply banner for a restart that failed.",
    ).toBe(assigns);
  });

  it("puts one gated assignment inside each of the restart and start handlers", () => {
    // Position, not just shape: both assignments could satisfy the counts above
    // while living in the same handler, leaving the other lifecycle path silent.
    const src = home();
    for (const anchor of ["await gamesRestart(", "await gamesStart("]) {
      const handler = blockOf(src, anchor); // throws if the call is gone
      expect(
        count(handler, /if\s*\(\s*e\.event\s*===\s*"done"\s*\)\s*applied\s*=\s*true\b/),
        `${anchor}…) does not record its verdict on the done event`,
      ).toBe(1);
    }
  });

  it("stops without claiming a restart happened", () => {
    // A stop recreates nothing, so it must NOT satisfy a pending apply. This
    // is the assertion that would go red if someone "unified" the three
    // handlers by copying the restart one.
    const handler = blockOf(home(), "await gamesStop(");
    expect(handler, "a stop must not clear the pending-apply banner").not.toMatch(
      /\bapplied\s*=\s*true\b/,
    );
    expect(handler, "a stop must not clear the pending-apply banner").not.toContain(
      "clearApplyNeeded(",
    );
  });

  it("consumes the flag exactly once, in the finally", () => {
    const body = act();
    expect(count(body, /clearApplyNeeded\(/), "act() clears the banner more than once").toBe(1);
    expect(
      body,
      "the clear is no longer gated on the done-derived flag — an unconditional clear in " +
        "`finally` fires on the failure path too, which is exactly the promise-derived bug",
    ).toMatch(/if\s*\(\s*applied\s*\)\s*clearApplyNeeded\(\)/);
  });
});

// ---------------------------------------------------------------------------

describe("Home worldRestart() clears only what a world restart can satisfy", () => {
  const home = () => code(find("pages/Home.svelte"));

  it("gates the clear on the done event AND on the pending kind", () => {
    const handler = blockOf(home(), "await wowWorldRestart(");
    expect(handler, "the fast restart no longer clears anything").toContain("clearApplyNeeded(");
    expect(
      handler,
      "the fast restart's clear is not gated on the terminal event — `docker restart` " +
        "failing resolves Ok just like every other stream",
    ).toMatch(/\be\.event\s*===\s*"done"/);
    expect(
      handler,
      "the fast restart clears ANY pending apply. It restarts the world PROCESS only, so a " +
        "pending `recreate` (settings that need creation-time env) is NOT satisfied by it — " +
        "dropping this conjunct tells the user a setting is live that the container never saw.",
    ).toMatch(/restartState\.apply\s*===\s*"world-restart"/);
  });

  it("has no second, ungated clear next to it", () => {
    expect(count(blockOf(home(), "async function worldRestart("), /clearApplyNeeded\(/)).toBe(1);
  });
});

// ---------------------------------------------------------------------------

describe("the settings-apply restarts clear the banner from the done event", () => {
  /**
   * Config.svelte and ModuleFiles.svelte carry byte-identical `saveAndRestart`
   * bodies. Both are scanned, because "a fix on ONE surface only half-ships"
   * applies to duplicated frontend code just as it does to bash-vs-Rust: an
   * edit to one of these two is exactly the change that leaves the other
   * behind, and nothing else in the suite would notice.
   */
  const FILES = ["pages/Config.svelte", "ModuleFiles.svelte"] as const;

  it.each(FILES)("%s saveAndRestart clears only on done", (file) => {
    const src = code(find(file));
    const fn = blockOf(src, "async function saveAndRestart(");
    expect(count(fn, /clearApplyNeeded\(/), `${file}: saveAndRestart clears more than once`).toBe(1);
    const handler = blockOf(fn, "await gamesRestart(");
    expect(
      handler,
      `${file}: the clear left the stream handler. Applying settings is the ONE flow whose ` +
        `whole purpose is "the change is now live" — deriving that from the promise announces ` +
        `a restart that did not happen.`,
    ).toMatch(/if\s*\(\s*e\.event\s*===\s*"done"\s*\)\s*clearApplyNeeded\(\)/);
  });

  it("Config runFlush clears only on done", () => {
    const src = code(find("pages/Config.svelte"));
    const fn = blockOf(src, "async function runFlush(");
    expect(count(fn, /clearApplyNeeded\(/), "runFlush clears more than once").toBe(1);
    const handler = blockOf(fn, "await wowBotsFlush(");
    expect(
      handler,
      "the flush restarts the server twice, which is why it may clear the banner at all — " +
        "but only if it actually finished. A flush that died mid-way resolves Ok and would " +
        "otherwise clear a banner nothing applied.",
    ).toMatch(/if\s*\(\s*\w+\.event\s*===\s*"done"\s*\)\s*clearApplyNeeded\(\)/);
  });

  /**
   * The opposite direction, same file: `runAhRepair` NOTES a pending apply
   * rather than clearing one, and it too must read the terminal event — a
   * repair that failed has not made a restart necessary.
   */
  it("Config runAhRepair notes a pending apply only on done", () => {
    const handler = blockOf(code(find("pages/Config.svelte")), "await wowAhbotRepair(");
    expect(handler, "the repair no longer raises the banner at all").toContain("noteApplyNeeded(");
    expect(
      handler,
      "the repair raises the restart banner without waiting for the terminal event",
    ).toMatch(/\bif\s*\(\s*\w+\.event\s*===\s*"done"\s*&&/);
  });
});
