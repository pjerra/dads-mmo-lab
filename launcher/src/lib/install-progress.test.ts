import { describe, expect, it } from "vitest";
import type { TermEvent } from "./api";
import {
  BUILD_STAGE,
  READY_STAGE,
  emptyProgress,
  formatElapsed,
  installDetailText,
  installProgressReduce,
  installStatusText,
  type InstallProgress,
} from "./install-progress.svelte";

const T0 = 1_700_000_000_000;

/** Fold a whole stream, the way a real install arrives. */
function fold(
  events: TermEvent[],
  from: InstallProgress = emptyProgress(),
  now: number = T0,
): InstallProgress {
  return events.reduce((acc, e) => installProgressReduce(acc, e, now), from);
}

const start = (name: string, limit_secs?: number) =>
  ({ event: "section_start", name, ...(limit_secs === undefined ? {} : { limit_secs }) }) as TermEvent;
const pct = (value: number) => ({ event: "pct", value }) as TermEvent;

/** A state as it would look mid-`ready`, `secs` into the wait. */
function waiting(secs: number, limitSecs: number | null = 1800): InstallProgress {
  return {
    active: true,
    stage: READY_STAGE,
    pct: null,
    limitSecs,
    stageStartedAt: T0,
    nowMs: T0 + secs * 1000,
  };
}

describe("installProgressReduce", () => {
  it("goes active on the first section and tracks the stage", () => {
    const got = fold([start("preflight")]);
    expect(got.active).toBe(true);
    expect(got.stage).toBe("preflight");
    expect(got.stageStartedAt).toBe(T0);
  });

  it("records a percentage", () => {
    expect(fold([start(BUILD_STAGE), pct(62)]).pct).toBe(62);
  });

  it("drops the percentage when the stage changes", () => {
    // The bug this prevents: a stale 99% from the build hanging over
    // "Starting containers…", which is a different job with a different total.
    const got = fold([start(BUILD_STAGE), pct(99), start("up")]);
    expect(got.stage).toBe("up");
    expect(got.pct).toBeNull();
  });

  it("drops the percentage when a stage is re-entered on a resume", () => {
    expect(fold([start(BUILD_STAGE), pct(80), start("up"), start(BUILD_STAGE)]).pct).toBeNull();
  });

  it("restarts the stage clock on every section", () => {
    const first = fold([start("clone-core")], emptyProgress(), T0);
    const second = installProgressReduce(first, start(BUILD_STAGE), T0 + 60_000);
    expect(second.stageStartedAt).toBe(T0 + 60_000);
  });

  it("keeps a section's wait ceiling and forgets it on a section without one", () => {
    const ready = fold([start(READY_STAGE, 1800)]);
    expect(ready.limitSecs).toBe(1800);
    // Only `ready` carries one; a later section must not inherit it.
    expect(installProgressReduce(ready, start("up"), T0).limitSecs).toBeNull();
  });

  it("goes inactive on both terminal events", () => {
    for (const terminal of [
      { event: "done", data: {} } as TermEvent,
      { event: "error", error: { code: "X", message: "m", hint: "h" } } as TermEvent,
    ]) {
      expect(fold([start(BUILD_STAGE), pct(50), terminal])).toEqual(emptyProgress());
    }
  });

  it("ignores lines, section_end and unknown events", () => {
    const before = fold([start(BUILD_STAGE), pct(40)]);
    const after = fold(
      [
        { event: "line", level: "info", text: "#26 1.0 [900/1808] Building CXX object x.cpp.o" } as TermEvent,
        { event: "section_end", name: BUILD_STAGE, status: "ok" } as TermEvent,
        { event: "something-from-the-future", data: 1 } as unknown as TermEvent,
      ],
      before,
    );
    expect(after).toEqual(before);
  });

  it("refuses a non-numeric percentage instead of rendering NaN", () => {
    const before = fold([start(BUILD_STAGE), pct(40)]);
    for (const bad of [
      { event: "pct", value: "62" } as unknown as TermEvent,
      { event: "pct" } as unknown as TermEvent,
      { event: "pct", value: Number.NaN } as unknown as TermEvent,
    ]) {
      expect(installProgressReduce(before, bad, T0)).toEqual(before);
    }
  });

  it("clamps a value outside 0-100", () => {
    expect(installProgressReduce(emptyProgress(), pct(4000), T0).pct).toBe(100);
    expect(installProgressReduce(emptyProgress(), pct(-5), T0).pct).toBe(0);
  });
});

describe("formatElapsed", () => {
  it("is mm:ss under an hour and h:mm:ss over it", () => {
    expect(formatElapsed(0)).toBe("0:00");
    expect(formatElapsed(9)).toBe("0:09");
    expect(formatElapsed(271)).toBe("4:31");
    expect(formatElapsed(1800)).toBe("30:00");
    expect(formatElapsed(3871)).toBe("1:04:31");
  });
});

describe("installStatusText", () => {
  it("shows the stage without a number when there is none", () => {
    // Parts of an install have no honest percentage -- apt, the cmake
    // configure, git's server-side phases.
    expect(installStatusText({ ...emptyProgress(), active: true, stage: "clone-core" })).toBe(
      "Downloading AzerothCore…",
    );
  });

  it("appends the number when there is one", () => {
    expect(
      installStatusText({ ...emptyProgress(), active: true, stage: BUILD_STAGE, pct: 62 }),
    ).toBe("Building… 62%");
    expect(installStatusText({ ...emptyProgress(), active: true, stage: "up", pct: 40 })).toBe(
      "Starting containers… 40%",
    );
  });

  it("shows the ready wait as elapsed time, never a percentage", () => {
    // The whole point of the decision: elapsed-over-timeout measures the clock,
    // not the work. The world can be up at 20% or at 99%.
    expect(installStatusText(waiting(271))).toBe("Waiting for the world… 4:31");
    expect(installStatusText(waiting(271))).not.toContain("%");
  });

  it("ignores a percentage on the ready stage if one ever arrived", () => {
    // Belt and braces: the engine does not emit one there, and if a future one
    // did, the wait must still not render as progress.
    expect(installStatusText({ ...waiting(271), pct: 97 })).toBe("Waiting for the world… 4:31");
  });

  it("falls back to a truthful generic for an unknown stage", () => {
    expect(
      installStatusText({ ...emptyProgress(), active: true, stage: "warp-core-alignment" }),
    ).toBe("Installing…");
    expect(installStatusText({ ...emptyProgress(), active: true })).toBe("Installing…");
  });

  it("covers every stage the engine actually runs", () => {
    // STAGE_ORDER in crates/dml-wow/src/install_native.rs. A stage added there
    // without copy here would silently render as the generic.
    for (const s of [
      "preflight",
      "guard",
      "clone-core",
      "clone-module",
      "generate-compose",
      "build",
      "up",
      "ready",
    ]) {
      expect(installStatusText({ ...emptyProgress(), active: true, stage: s })).not.toBe(
        "Installing…",
      );
    }
  });
});

describe("installDetailText", () => {
  it("names both the wait so far and the ceiling", () => {
    expect(installDetailText(waiting(271))).toBe(
      "First boot imports the world database. Waited 4:31 of up to 30:00.",
    );
  });

  it("says nothing without a ceiling, rather than inventing one", () => {
    expect(installDetailText(waiting(271, null))).toBeNull();
  });

  it("says nothing for the stages that speak for themselves", () => {
    expect(
      installDetailText({ ...emptyProgress(), active: true, stage: BUILD_STAGE, pct: 62 }),
    ).toBeNull();
  });
});
