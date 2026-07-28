import { describe, it, expect } from "vitest";

// The taskbar cue is wired by hand at every long streamed op (there is no
// central hook -- beginRun() deliberately knows nothing about it), so it
// drifts silently: six call sites shipped without the cue and nothing failed.
// This suite is the missing guard rail. It reads the component sources and
// enforces the pairing convention itself:
//
//   a function that starts a long stream -> it must call taskbarBusy(), and
//   its taskbarIdle() must live in a finally block.
//
// The finally rule is the load-bearing half: taskbarIdle() no-ops at zero
// (taskbar.ts:37) so a stray idle is harmless, but a busy whose idle sits on
// the happy path leaks the cue on for the rest of the session whenever the op
// throws.
//
// Sources come in via import.meta.glob(?raw) rather than node:fs -- the app
// has no @types/node, so a bare `node:fs` import fails `npm run check`.
const SOURCES = import.meta.glob("./**/*.svelte", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

// Top-level function declarations only (indent <= 2 inside the script block) --
// nested callbacks are part of their owner's body, not regions of their own.
const FN_START = /^[ \t]{0,2}(?:export\s+)?(?:async\s+)?function\s+([A-Za-z_$][\w$]*)\s*\(/gm;

/** Split a component's <script> into one text region per top-level function. */
function functionRegions(source: string): { name: string; body: string }[] {
  const script = /<script[^>]*>([\s\S]*)<\/script>/.exec(source)?.[1] ?? "";
  const starts: { name: string; at: number }[] = [];
  FN_START.lastIndex = 0;
  for (let m = FN_START.exec(script); m !== null; m = FN_START.exec(script)) {
    starts.push({ name: m[1], at: m.index });
  }
  return starts.map((s, i) => ({
    name: s.name,
    body: script.slice(s.at, starts[i + 1]?.at ?? script.length),
  }));
}

// What marks a function as "starts a long stream". beginRun() catches every
// call site that owns a Terminal buffer -- but it structurally cannot see the
// LONGEST op in the app. A title install (and install-from-URL, and a tool
// install) streams through InstallTerminal.svelte, which writes to
// installStore instead of a term buffer and gates its single invoke on
// claimInstallInvoke(). Scanning for beginRun() alone is exactly why that
// tens-of-minutes path shipped with no cue while a 5-second bridge-setup got
// one. Both markers sit at the START of the stream, so the same
// busy/finally-idle rule applies unchanged.
const STREAM_MARKERS = ["beginRun(", "claimInstallInvoke("];

// The install stream's owner, pinned by name: the claim guard is what makes
// run() the single instance that actually invokes (a nav-away remount returns
// early and must NOT take a busy it will never release), so the cue has to be
// taken right after it.
const INSTALL_STREAM = "./InstallTerminal.svelte:run";

const streamingFns = Object.entries(SOURCES)
  .sort(([a], [b]) => a.localeCompare(b))
  .flatMap(([file, source]) =>
    functionRegions(source)
      .filter((fn) => STREAM_MARKERS.some((marker) => fn.body.includes(marker)))
      .map((fn) => ({ where: `${file}:${fn.name}`, body: fn.body })),
  );

describe("taskbar cue pairing", () => {
  it("finds the streamed-op call sites to check", () => {
    // Guards against the scan silently matching nothing (a rename of
    // beginRun, or a formatting change that breaks the region split, would
    // otherwise turn every assertion below into a vacuous pass).
    expect(streamingFns.length).toBeGreaterThanOrEqual(15);
  });

  it("covers the install stream, which has no beginRun() to find", () => {
    // Per-marker non-vacuity. The beginRun() sites dominate the count above,
    // so if InstallTerminal's run() is renamed or its claim guard moves, the
    // floor still passes and the two pairing assertions below would silently
    // stop covering the longest-running op in the app.
    expect(streamingFns.map((fn) => fn.where)).toContain(INSTALL_STREAM);
  });

  it("turns the cue on in every function that streams a long op", () => {
    const missing = streamingFns.filter((fn) => !fn.body.includes("taskbarBusy()")).map((fn) => fn.where);
    expect(missing).toEqual([]);
  });

  it("clears the cue from a finally block, never from the happy path", () => {
    const unbalanced = streamingFns
      .filter((fn) => {
        const idle = fn.body.indexOf("taskbarIdle()");
        if (idle === -1) return true; // busy with no idle at all -- leaks forever
        const fin = fn.body.indexOf("finally");
        return fin === -1 || fin > idle; // idle outside/ahead of the finally
      })
      .map((fn) => fn.where);
    expect(unbalanced).toEqual([]);
  });
});
