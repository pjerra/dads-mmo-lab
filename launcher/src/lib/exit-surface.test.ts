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

  /**
   * AND THE TERNARY POINTS THE RIGHT WAY (M10, final review 2026-08-05).
   *
   * Inverting `e.payload === "prompt_unknown" ? "prompt_unknown" : …` left 18
   * tests green across both files: `exitGuard.kind` is still assigned, the
   * payload is still read, and every assertion above still holds. The result
   * is an Unknown close that asserts "Your server is running" as settled fact
   * — the exact overclaim `exit-guard.test.ts`'s own fix round exists to
   * forbid, reintroduced one level up where that suite structurally cannot
   * see it.
   */
  it("maps the unknown payload to the unknown copy, not to its opposite", () => {
    const body = handler();
    expect(
      body,
      'the "prompt_unknown" payload must select the prompt_unknown copy — inverted, the ' +
        "dialog tells a user whose server state we could NOT read that it is definitely running",
    ).toMatch(/e\.payload\s*===\s*"prompt_unknown"\s*\?\s*"prompt_unknown"/);
  });

  /**
   * H3 (final review 2026-08-05). THE ASSERTION THAT WAS POLARITY-BLIND.
   *
   * This used to be `toMatch(/exitGuard\.busy\)\s*return/)`, which matches
   * `!exitGuard.busy) return` verbatim. One character — `if (exitGuard.busy)`
   * to `if (!exitGuard.busy)` — restored the whole Task-4 product failure with
   * 784 tests green and svelte-check at 0 errors: `busy` is false on every
   * fresh close, so the handler returns before touching `exitGuard`, the
   * dialog never opens, Rust has already called `prevent_exit`, and the window
   * surfaces with no explanation.
   *
   * So: the SHAPE (which rejects the negation), and the POSITION (nothing else
   * may bail out before the dialog opens). The position half also closes
   * M10's first mutation — prepending `if (firstRun) return;` to this handler
   * was invisible to the whole suite, and the identical guard sits twelve
   * lines below in the tray-action listener WITH a comment recommending it.
   */
  it("ignores a re-emit while a confirmed stop is already running", () => {
    // Tray Quit clicked twice mid-stop re-emits for real. Without this the
    // second one resets the terminal and the progress out from under a stop
    // that is genuinely in flight.
    const body = handler();
    expect(body, "the handler must bail out while busy").toMatch(
      /if\s*\(\s*exitGuard\.busy\s*\)\s*return/,
    );
    expect(
      body,
      "the busy guard is NEGATED — it now bails out on every fresh close, which is the " +
        "dialog never opening at all",
    ).not.toMatch(/if\s*\(\s*!\s*exitGuard\.busy\s*\)/);
  });

  it("has no other way to bail out before the dialog opens", () => {
    const body = handler();
    const returns = body.match(/\breturn\b/g) ?? [];
    expect(
      returns.length,
      "every `return` in this handler is a path on which the user clicks Exit and NOTHING " +
        "happens, while Rust has already vetoed the exit. There must be exactly one, and " +
        "it must be the busy guard.",
    ).toBe(1);
    const bail = body.indexOf("return");
    const opens = body.indexOf("exitGuard.open = true");
    expect(opens, "the handler no longer opens the dialog at all").toBeGreaterThan(-1);
    expect(
      bail,
      "a return sits AFTER the dialog is opened — then it is not the busy guard and the " +
        "handler has an exit path this test cannot account for",
    ).toBeLessThan(opens);
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

  /**
   * M8's wiring half (final review 2026-08-05). An empty `catch { }` with the
   * note and `busy = false` moved into the success path left 15 tests green —
   * F3's frontend half was pinned in name only. The note now comes from a
   * shared constant `exit-guard.test.ts` owns the wording of, so this side
   * only has to prove the catch actually uses it.
   */
  it("reports the failure from the one constant whose wording is pinned", () => {
    const confirm = blockOf(shell(), "async function confirmExit(");
    const caught = blockOf(confirm, "catch (e)");
    expect(
      caught,
      "the catch is empty or writes an ad-hoc string. An inline literal is deletable — " +
        "changing this note to \"\" left 784 tests green — so it lives in EXIT_STOP_FAILED_NOTE",
    ).toContain("EXIT_STOP_FAILED_NOTE");
    expect(caught, "the failure must reach the terminal too").toContain("applyEvent(");
  });

  /**
   * M7 (final review 2026-08-05). THE ESCAPE HATCH VANISHED EXACTLY WHEN IT
   * WAS NEEDED. `{#if exitGuard.busy}` gated the "Close anyway" button, and
   * the failure arm clears `busy` — correctly, that run is over — so after a
   * failed stop the user was left with Cancel (does nothing about a server
   * that may still be up) and a Confirm that had just failed. Spec line 117:
   * "if the stop fails, report the failure and offer to close anyway."
   *
   * Both halves proved by mutation: an assertion forbidding the `busy`-only
   * gate went RED against the old markup, and wrapping the button in
   * `{#if false}` stayed green — the gate itself was unpinned.
   */
  it("keeps the escape hatch on screen after a failed stop", () => {
    const markup = exitModalMarkup(shell());
    const gate = markup.match(/\{#if\s+([^}]*)\}\s*<button class="exit-force"/);
    expect(
      gate,
      'the "Close anyway" button is no longer behind a readable {#if} immediately above ' +
        "it — if it is now ungated that is fine, but this assertion has to be rewritten " +
        "deliberately rather than silently stop checking",
    ).not.toBeNull();
    const condition = gate![1];
    expect(
      condition,
      "the escape hatch is gated on `busy` alone. The failure arm clears `busy`, so the " +
        "one control that still works after a failed stop disappears at that exact " +
        "moment, and the user is left with Cancel and a Confirm that just failed (M7).",
    ).toContain("exitGuard.failed");
    expect(condition, "it must still show during the stop itself").toContain("exitGuard.busy");
    expect(condition, "the gate must be a disjunction, not a conjunction").toContain("||");
  });

  it("latches the failure flag the escape hatch depends on, and clears it on a fresh ask", () => {
    const src = shell();
    expect(
      blockOf(src, "async function confirmExit("),
      "nothing sets exitGuard.failed, so the M7 gate above can never become true",
    ).toContain("exitGuard.failed = true");
    expect(
      blockOf(src, '"exit-requested"'),
      "a fresh exit-requested must clear the previous run's failure, or the escape hatch " +
        "renders on a dialog that has not failed yet",
    ).toContain("exitGuard.failed = false");
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

  /**
   * H4 (final review 2026-08-05). A RENAME WAS CAUGHT; THE SWAP WAS NOT.
   *
   * The assertions above check that both literals appear somewhere in the file
   * and that both export names exist. Exchanging the two literals satisfies
   * every one of them: 15 passed, `npm run check` 0 errors. Under the swap
   * "Stop server and close" invokes `exit_anyway` and HARD-CUTS a live server
   * with ~2,000 bots on it — the incident this plan was written for — while
   * "Close anyway", the control whose entire purpose is to work when the stop
   * is hung, blocks on that stop instead.
   *
   * The membership test above is about the file; this one is about each
   * wrapper's own body.
   */
  it("binds each wrapper to its own command, so a swap is not invisible", () => {
    const api = code(find("/api.ts"));
    const stopAndClose = blockOf(api, "export const exitStopAndClose");
    const anyway = blockOf(api, "export async function exitAnyway");

    expect(stopAndClose, "exitStopAndClose must invoke exit_stop_and_close").toContain(
      '"exit_stop_and_close"',
    );
    expect(
      stopAndClose,
      'exitStopAndClose invokes exit_anyway — the button labelled "Stop server and close" ' +
        "now hard-cuts a live server instead of stopping it",
    ).not.toContain('"exit_anyway"');

    expect(anyway, "exitAnyway must invoke exit_anyway").toContain('"exit_anyway"');
    expect(
      anyway,
      "exitAnyway invokes exit_stop_and_close — the escape hatch now blocks on the very " +
        "stop it exists to escape",
    ).not.toContain('"exit_stop_and_close"');
  });
});
