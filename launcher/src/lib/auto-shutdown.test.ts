import { describe, expect, it } from "vitest";
import { autoShutdownLabel, firedNotice } from "./auto-shutdown.svelte";

// The "fired" event now carries a 4-way outcome instead of a stopped bool, so
// a FAILED graceful stop no longer masquerades as "nothing to stop" -- the
// user is never told the world is off while it keeps running.
describe("firedNotice", () => {
  it("reports a real automatic stop", () => {
    expect(firedNotice("stopped")).toMatch(/stopped automatically/i);
  });

  it("distinguishes a failed stop and tells the user to stop it themselves", () => {
    const n = firedNotice("stop_failed");
    expect(n).toMatch(/could not be stopped|stop it from Home/i);
    // Crucially NOT the "nothing to stop" copy.
    expect(n).not.toMatch(/nothing to stop/i);
  });

  it("distinguishes an unreadable server state", () => {
    expect(firedNotice("unknown")).toMatch(/couldn't be checked|check it from Home/i);
  });

  it("keeps the honest 'nothing to stop' copy only for a truly-down server", () => {
    expect(firedNotice("not_running")).toMatch(/nothing to stop/i);
  });
});

describe("autoShutdownLabel", () => {
  it("is Off when disabled or off-state", () => {
    expect(autoShutdownLabel(false, "off")).toBe("Off.");
    expect(autoShutdownLabel(true, "off")).toBe("Off.");
  });
  it("shows armed and waiting copy when enabled", () => {
    expect(autoShutdownLabel(true, "armed")).toMatch(/Armed/);
    expect(autoShutdownLabel(true, "waiting")).toMatch(/Waiting for WoW/);
  });
});
