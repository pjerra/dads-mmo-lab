import { describe, it, expect } from "vitest";
import { code, sourceFinder, blockAfter, blockOf } from "./source-scan";

/**
 * The frontend half of the exit contract (F2, final review 2026-08-05).
 *
 * The Rust half of this feature is pinned by wiring tests inside lib.rs. The
 * frontend half had NONE, and the reviewer proved what that costs: renaming
 * ONE string literal — `listen("exit-requested")` to
 * `listen("exit-requested-TYPO")` — left 769/769 vitest tests green and
 * svelte-check at 0 errors, while reproducing the Task-4 product failure
 * byte for byte. Rust prevents the exit, the window surfaces, no dialog ever
 * appears, and the user reaches for Task Manager — which is the hard WSL cut
 * this entire plan exists to prevent.
 *
 * Nothing pure can catch that. `exitCopy` is a pure function and it is fully
 * covered; the covered thing was never the thing that broke. So every
 * assertion below is about WIRING read out of the real source: the event name,
 * the handler's effect, the dialog's gate, which button reaches which command.
 *
 * Two traps this repo has recorded, and which every scanner here has to
 * survive:
 *   1. matching a string inside a COMMENT rather than in real code (bitten
 *      twice on 2026-08-01) — hence `code()` strips first, and this file's
 *      subject is discussed at length in the very comments it must ignore;
 *   2. passing on an EMPTY match set — hence `sourceFinder`/`blockOf` THROW on
 *      a miss, the scanners have their own non-vacuity suite below, and the
 *      call-site counts are asserted before anything is asserted about them.
 */

const SOURCES = import.meta.glob(["../routes/+page.svelte", "./api.ts"], {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

const find = sourceFinder(SOURCES);

/** Event names passed to `listen(...)`, generic parameter or not. */
function listenedEvents(src: string): string[] {
  const out: string[] = [];
  const re = /\blisten\s*(?:<[^>]*>)?\s*\(\s*"([^"]+)"/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(src)) !== null) out.push(m[1]);
  return out;
}

/** Command names passed to `invoke(...)`. */
function invokedCommands(src: string): string[] {
  const out: string[] = [];
  const re = /\binvoke(?:<[^>]*>)?\s*\(\s*"([^"]+)"/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(src)) !== null) out.push(m[1]);
  return out;
}

/** The shell's markup for the exit dialog, from its gate to the style block. */
function exitModalMarkup(src: string): string {
  const gate = '{#if exitGuard.open}';
  const at = src.indexOf(gate);
  if (at === -1) {
    throw new Error("the exit dialog has no `{#if exitGuard.open}` gate — a listener that sets a flag nothing renders is the Task-4 regression itself");
  }
  const end = src.indexOf("<style>", at);
  if (end === -1) throw new Error("no <style> block to bound the markup scan");
  return src.slice(at, end);
}

// ---------------------------------------------------------------------------

describe("the source scanners themselves", () => {
  // Non-vacuity. A scanner that quietly returned nothing would make every
  // assertion in this file pass against any source at all.
  it("reads event and command names, and only from call shapes", () => {
    expect(listenedEvents('void listen<string>("a", (e) => {});')).toEqual(["a"]);
    expect(listenedEvents('listen("a"); listen<X>( "b" )')).toEqual(["a", "b"]);
    expect(listenedEvents('const s = "listen for exit-requested";')).toEqual([]);
    expect(invokedCommands('invoke("cmd", { x })')).toEqual(["cmd"]);
    expect(invokedCommands("nothing here")).toEqual([]);
  });

  it("extracts a block and is not fooled by braces inside strings", () => {
    expect(blockOf("function f() { a; }", "function f(")).toBe(" a; ");
    expect(blockOf('function f() { s = "}"; a; }', "function f(")).toBe(' s = "}"; a; ');
    expect(blockOf("function f() { if (x) { y; } }", "function f(")).toBe(" if (x) { y; } ");
    expect(blockAfter('listen("e", (x) => { body; })', 0)).toBe(" body; ");
  });

  it("throws rather than returning empty when the anchor is gone", () => {
    expect(() => blockOf("function g() {}", "function f(")).toThrow(/anchor not found/);
    expect(() => find("routes/nope.svelte")).toThrow(/the glob is wrong/);
    // The -1-means-top-of-file trap, closed at the source.
    expect(() => blockAfter("function f() { a; }", -1)).toThrow(/never found/);
  });

  it("matches code, not the prose about the code", () => {
    // This file's own subject appears verbatim in the shell's comments.
    expect(listenedEvents(code('// void listen<string>("exit-requested", f)'))).toEqual([]);
    expect(listenedEvents(code('<!-- listen("exit-requested") -->'))).toEqual([]);
  });
});

// ---------------------------------------------------------------------------

describe("the shell hears the exit request", () => {
  const shell = () => code(find("routes/+page.svelte"));

  it("registers listeners at all", () => {
    expect(listenedEvents(shell()).length, "the shell listens for nothing").toBeGreaterThan(0);
  });

  /**
   * THE ONE THE REVIEWER MUTATED.
   *
   * The name is a wire literal shared with `emit("exit-requested", …)` in
   * lib.rs. A typo on either side type-checks, compiles, and silently removes
   * the only dialog the user gets before their server is stopped.
   */
  it('listens for exactly "exit-requested"', () => {
    expect(
      listenedEvents(shell()),
      'the exit dialog is driven by the "exit-requested" event emitted from lib.rs',
    ).toContain("exit-requested");
  });

  /**
   * The handler body, anchored on the literal itself.
   *
   * `blockOf` THROWS when the anchor is gone. The first version of this used
   * `blockAfter(src, src.indexOf(...))` and `indexOf` returns -1 on a miss,
   * which `blockAfter` read as "start at the top of the file" — so under the
   * renamed-literal mutation these three assertions went red against
   * `onMount`'s body instead of against the handler, i.e. red for a reason
   * that had nothing to do with what they test. Right verdict, wrong evidence,
   * and one plausible source edit away from the wrong verdict.
   */
  const handler = () => blockOf(shell(), '"exit-requested"');

  it("opens the dialog when it fires", () => {
    // Registering a listener that writes nothing is the same product failure
    // as not registering one: the window surfaces and nothing explains why.
    expect(handler(), "the handler must raise the dialog").toContain("exitGuard.open = true");
  });

  it("takes the prompt kind from the payload instead of assuming one", () => {
    // A hardcoded kind makes an Unknown close assert "Your server is running"
    // as settled fact — the overclaim exit-guard.test.ts guards the copy
    // against, reintroduced one level up where that suite cannot see it.
    const body = handler();
    expect(body).toContain("exitGuard.kind");
    expect(body).toContain("prompt_unknown");
    expect(body, "the kind must be derived from the event payload").toMatch(/e\.payload/);
  });

  it("ignores a re-emit while a confirmed stop is already running", () => {
    // Tray Quit clicked twice mid-stop re-emits for real. Without this the
    // second one resets the terminal and the progress out from under a stop
    // that is genuinely in flight.
    expect(handler(), "the handler must bail out while busy").toMatch(/exitGuard\.busy\)\s*return/);
  });
});

// ---------------------------------------------------------------------------

describe("the dialog's three buttons reach three different places", () => {
  const shell = () => code(find("routes/+page.svelte"));

  it("renders the dialog behind the guard's own open flag", () => {
    const markup = exitModalMarkup(shell()); // throws if the gate is gone
    expect(markup).toContain("exitCopy(exitGuard.kind)");
  });

  it("wires each button to its own handler", () => {
    const markup = exitModalMarkup(shell());
    for (const [handler, what] of [
      ["cancelExit", "Cancel"],
      ["confirmExit", "the confirm button"],
      ["closeAnyway", "the escape hatch"],
    ] as const) {
      expect(markup, `${what} is not wired to ${handler}`).toContain(`onclick={${handler}}`);
    }
    // The escape hatch specifically: it is the ONLY control available while a
    // stop is running, so it must be the force-close button and not, say, the
    // one that starts another stop.
    expect(markup, "the exit-force button must be the escape hatch").toMatch(
      /class="exit-force"[^>]*onclick=\{closeAnyway\}/,
    );
  });

  it("disables the confirm and cancel buttons while a stop is in flight", () => {
    const markup = exitModalMarkup(shell());
    expect(markup.match(/disabled=\{exitGuard\.busy\}/g)?.length ?? 0).toBeGreaterThanOrEqual(2);
  });

  /**
   * The swap is the dangerous mutation here. `closeAnyway` skips the stop
   * entirely; `confirmExit` performs it. Exchanging them makes the button
   * labelled "Stop server and close" hard-cut a live server with ~2,000 bots
   * on it — precisely the incident this plan was written for — while every
   * pure test, the type checker and the rendered dialog all stay identical.
   */
  it("confirm stops the server and the escape hatch does not", () => {
    const src = shell();
    const confirm = blockOf(src, "async function confirmExit(");
    const anyway = blockOf(src, "async function closeAnyway(");
    const cancel = blockOf(src, "function cancelExit(");

    expect(confirm, "confirm must run the ordinary stop").toContain("exitStopAndClose(");
    expect(confirm, "confirm must not take the escape hatch").not.toContain("exitAnyway(");

    expect(anyway, "the escape hatch must close without stopping").toContain("exitAnyway(");
    expect(anyway, "the escape hatch must not start a stop").not.toContain("exitStopAndClose(");

    // Cancel means "don't close and don't touch the server". It has exactly
    // one job and both commands are wrong answers to it.
    expect(cancel).not.toContain("exitStopAndClose(");
    expect(cancel).not.toContain("exitAnyway(");
    expect(cancel, "cancel must retract the dialog").toContain("exitGuard.open = false");
  });

  /**
   * F3's frontend half. Wording is deliberately NOT pinned — exit-guard's own
   * suite owns copy, and this note's text is in flux while the Rust side stops
   * exiting on a failed stop. What must hold is the SHAPE: a failure is
   * surfaced and the dialog is not left stuck mid-run.
   */
  it("surfaces a failed stop rather than swallowing it", () => {
    const confirm = blockOf(shell(), "async function confirmExit(");
    expect(confirm, "confirm must catch the stop's failure").toMatch(/\bcatch\b/);
    expect(confirm, "a failure must leave a note for the user").toMatch(/exitGuard\.note\s*=/);
    expect(confirm, "a failure must release the busy flag").toContain("exitGuard.busy = false");
  });
});

// ---------------------------------------------------------------------------

describe("the IPC wrappers name the commands lib.rs registers", () => {
  // Same literal-string class as the event name above, one layer down: a
  // rename here type-checks and fails only at runtime, inside a dialog that
  // only appears when the user is closing a launcher with a live server.
  it("wraps exit_stop_and_close and exit_anyway", () => {
    const api = code(find("/api.ts"));
    const commands = invokedCommands(api);
    expect(commands.length, "api.ts invokes nothing").toBeGreaterThan(0);
    expect(commands).toContain("exit_stop_and_close");
    expect(commands).toContain("exit_anyway");
    expect(api).toMatch(/export\s+(const|async function)\s+exitStopAndClose/);
    expect(api).toMatch(/export\s+(const|async function)\s+exitAnyway/);
  });
});
