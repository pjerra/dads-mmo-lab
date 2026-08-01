import { describe, expect, it } from "vitest";
import type { TermEvent } from "./api";
import {
  BUILD_STAGE,
  emptyProgress,
  installProgressReduce,
  installStatusText,
  type InstallProgress,
} from "./install-progress.svelte";

/** Fold a whole stream, the way a real install arrives. */
function fold(events: TermEvent[], from: InstallProgress = emptyProgress()): InstallProgress {
  return events.reduce(installProgressReduce, from);
}

const start = (name: string) => ({ event: "section_start", name }) as TermEvent;
const pct = (value: number) => ({ event: "pct", value }) as TermEvent;

describe("installProgressReduce", () => {
  it("goes active on the first section and tracks the stage", () => {
    expect(fold([start("preflight")])).toEqual({ active: true, stage: "preflight", pct: null });
  });

  it("records a percentage", () => {
    expect(fold([start(BUILD_STAGE), pct(62)])).toEqual({
      active: true,
      stage: BUILD_STAGE,
      pct: 62,
    });
  });

  it("drops the percentage when the stage changes", () => {
    // The bug this prevents: a stale 99% from the build hanging over
    // "Starting containers…", which is a different job with no number at all.
    const got = fold([start(BUILD_STAGE), pct(99), start("up")]);
    expect(got).toEqual({ active: true, stage: "up", pct: null });
  });

  it("drops the percentage when the build stage is re-entered", () => {
    // A resume runs `build` again with a step total the old floor does not
    // describe, so carrying the number over would be a number about nothing.
    const got = fold([start(BUILD_STAGE), pct(80), start("up"), start(BUILD_STAGE)]);
    expect(got.pct).toBeNull();
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
      expect(installProgressReduce(before, bad)).toEqual(before);
    }
  });

  it("clamps a value outside 0-100", () => {
    expect(installProgressReduce(emptyProgress(), pct(4000)).pct).toBe(100);
    expect(installProgressReduce(emptyProgress(), pct(-5)).pct).toBe(0);
  });
});

describe("installStatusText", () => {
  it("shows the stage without a number when there is none", () => {
    // Most of an install has no honest percentage -- the clones, apt, and the
    // cmake configure that runs before ninja starts counting.
    expect(installStatusText({ active: true, stage: "clone-core", pct: null })).toBe(
      "Downloading AzerothCore…",
    );
    expect(installStatusText({ active: true, stage: BUILD_STAGE, pct: null })).toBe("Building…");
  });

  it("appends the number when there is one", () => {
    expect(installStatusText({ active: true, stage: BUILD_STAGE, pct: 62 })).toBe("Building… 62%");
    expect(installStatusText({ active: true, stage: BUILD_STAGE, pct: 0 })).toBe("Building… 0%");
  });

  it("falls back to a truthful generic for an unknown stage", () => {
    // A stage added to the engine later must not render as "undefined".
    expect(installStatusText({ active: true, stage: "warp-core-alignment", pct: null })).toBe(
      "Installing…",
    );
    expect(installStatusText({ active: true, stage: null, pct: null })).toBe("Installing…");
  });

  it("covers every stage the engine actually runs", () => {
    // STAGE_ORDER in crates/dml-wow/src/install_native.rs. A stage added there
    // without copy here would silently render as the generic.
    const stages = [
      "preflight",
      "guard",
      "clone-core",
      "clone-module",
      "generate-compose",
      "build",
      "up",
      "ready",
    ];
    for (const s of stages) {
      expect(installStatusText({ active: true, stage: s, pct: null })).not.toBe("Installing…");
    }
  });
});
