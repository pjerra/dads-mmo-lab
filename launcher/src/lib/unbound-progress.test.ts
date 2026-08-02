import { describe, it, expect, beforeEach, vi } from "vitest";

// Mock the API module, not Tauri: the runner's contract with the progress store
// is what is under test, and stubbing `wowUnboundInstall` lets the test PLAY an
// engine event stream instead of asserting that some source text exists.
const { wowUnboundInstall, wowUnboundUninstall } = vi.hoisted(() => ({
  wowUnboundInstall: vi.fn(),
  wowUnboundUninstall: vi.fn(),
}));
vi.mock("./api", () => ({ wowUnboundInstall, wowUnboundUninstall }));

import { unboundRunner } from "./unbound-run";
import { installProgress, setInstallActive } from "./install-progress.svelte";
import type { TermEvent } from "./api";

/**
 * THE BUG THIS FILE EXISTS FOR (found live, 2026-08-03, mid-rebuild).
 *
 * `unboundRunner` was written by reusing `native-install.ts`'s event
 * TRANSLATOR, and reused none of its STORE wiring. One omission, two symptoms,
 * and the second is worse than the first:
 *
 *   * no `noteInstallEvent` -> `pct` never reached the store -> no progress bar
 *     through a 1877-object compile.
 *   * no `setInstallActive` -> `statusLabel` fell through to the POLLED
 *     verdict. During a rebuild the OLD worldserver is still running, so the
 *     app cheerfully read "World is up" for an hour while the machine
 *     compiled. Wrong in the most reassuring possible direction.
 *
 * The old `unbound-run.test.ts` covered the translator thoroughly and could
 * never have caught this, because the translator was never the broken part.
 * So this drives the REAL runner and asserts against the REAL store.
 */
describe("the Unbound runner drives the install-progress store", () => {
  /**
   * Play a scripted stream and snapshot the store WHILE the run is still open.
   *
   * The snapshot matters. A stream that ends without a terminal event is a
   * contract violation the runner deliberately cleans up after, so reading the
   * store AFTER the run always sees the cleared state -- which is how the
   * first version of these tests failed against correct code.
   */
  let play: (events: TermEvent[], mode?: "install" | "uninstall") => Promise<Snapshot>;
  type Snapshot = { active: boolean; stage: string | null; pct: number | null; limitSecs: number | null };

  const snap = (): Snapshot => ({
    active: installProgress.active,
    stage: installProgress.stage,
    pct: installProgress.pct,
    limitSecs: installProgress.limitSecs,
  });

  beforeEach(() => {
    setInstallActive(false);
    wowUnboundInstall.mockReset();
    wowUnboundUninstall.mockReset();
    play = async (events, mode = "install") => {
      let mid: Snapshot | null = null;
      const impl = async (_accept: boolean, onEvent: (e: TermEvent) => void) => {
        for (const e of events) onEvent(e);
        mid = snap();
      };
      (mode === "install" ? wowUnboundInstall : wowUnboundUninstall).mockImplementation(impl);
      await unboundRunner(mode, true)("id", () => {});
      return mid!;
    };
  });

  it("marks an install ACTIVE before the first event arrives", async () => {
    // Not after the first event: the engine's preflight takes seconds, and a
    // status that reads "Stopped" (or worse, "World is up") for those seconds
    // and then jumps is the flicker this ordering exists to prevent.
    let activeAtFirstEvent: boolean | null = null;
    wowUnboundInstall.mockImplementation(
      async (_a: boolean, onEvent: (e: TermEvent) => void) => {
        activeAtFirstEvent = installProgress.active;
        onEvent({ event: "section_start", name: "preflight" } as unknown as TermEvent);
      },
    );
    await unboundRunner("install", true)("id", () => {});
    expect(activeAtFirstEvent).toBe(true);
  });

  it("feeds pct through to the store during the build", async () => {
    const mid = await play([
      { event: "section_start", name: "build" },
      { event: "pct", value: 62 },
    ] as unknown as TermEvent[]);
    expect(mid.stage).toBe("build");
    expect(mid.pct).toBe(62);
    expect(mid.active).toBe(true);
  });

  it("carries the ready stage's limit so the wait can render elapsed", async () => {
    // `ready` deliberately reports no percentage -- it is a bounded WAIT, and
    // elapsed-over-timeout measures the clock, not the work.
    const mid = await play([
      { event: "section_start", name: "ready", limit_secs: 1800 },
    ] as unknown as TermEvent[]);
    expect(mid.stage).toBe("ready");
    expect(mid.limitSecs).toBe(1800);
    expect(mid.pct).toBeNull();
  });

  it("clears active on done, so the chip stops claiming an install", async () => {
    await play([
      { event: "section_start", name: "ready" },
      { event: "done", data: { addon_version: "1.4.0" } },
    ] as unknown as TermEvent[]);
    expect(installProgress.active).toBe(false);
  });

  it("clears active on a failed run too", async () => {
    await play([
      { event: "section_start", name: "build" },
      { event: "error", error: { code: "UNBOUND_BUILD_FAILED", message: "m", hint: "h" } },
    ] as unknown as TermEvent[]);
    expect(installProgress.active).toBe(false);
  });

  it("clears active when the IPC itself rejects", async () => {
    // A dead session must not pin the chip to "Installing…" until the app is
    // restarted -- there would be no run left to end it.
    wowUnboundInstall.mockRejectedValue({ message: "ipc gone" });
    await unboundRunner("install", true)("id", () => {});
    expect(installProgress.active).toBe(false);
  });

  it("clears active when a stream ends with no terminal event", async () => {
    // A contract violation upstream, but leaving the UI permanently
    // "installing" is strictly worse than an honest close.
    await play([{ event: "section_start", name: "build" }] as unknown as TermEvent[]);
    expect(installProgress.active).toBe(false);
  });

  it("drives the store on an uninstall as well", async () => {
    // The uninstall rebuilds too -- 30-90 minutes of the same silence.
    const mid = await play(
      [
        { event: "section_start", name: "build" },
        { event: "pct", value: 41 },
      ] as unknown as TermEvent[],
      "uninstall",
    );
    expect(wowUnboundUninstall).toHaveBeenCalled();
    expect(mid.stage).toBe("build");
    expect(mid.pct).toBe(41);
  });

  it("still delivers terminal text alongside the store updates", async () => {
    // Non-vacuity for the whole file: the store wiring must not have replaced
    // the terminal output it sits next to.
    const chunks: string[] = [];
    let midPct: number | null = null;
    wowUnboundInstall.mockImplementation(
      async (_a: boolean, onEvent: (e: TermEvent) => void) => {
        onEvent({ event: "pct", value: 5 } as unknown as TermEvent);
        midPct = installProgress.pct;
        onEvent({
          event: "done",
          data: { manual_step: ".npc add 900001" },
        } as unknown as TermEvent);
      },
    );
    await unboundRunner("install", true)("id", (e) => {
      if (e.event === "chunk") chunks.push((e as { text: string }).text);
    });
    expect(chunks.join("")).toContain(".npc add 900001");
    expect(midPct).toBe(5);
    // ...and the run closed cleanly afterwards.
    expect(installProgress.active).toBe(false);
  });
});
