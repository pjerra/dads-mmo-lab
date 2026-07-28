import { describe, expect, it } from "vitest";
import { parseLanStatus, shouldEngageKeepAwake, verdictTransitionActions } from "./transitions";

const allowed = { lanAutoAllowed: true };
const denied = { lanAutoAllowed: false };

// Engaging the sleep block is a per-poll assertion, NOT a transition: two
// release paths (the 3-failed-poll release and the Rust 120s watchdog) drop
// the block with no transition left to recover on. See the wiring cover in
// keep-awake.test.ts.
describe("shouldEngageKeepAwake", () => {
  it("engages on every online poll, not just the transition into online", () => {
    expect(shouldEngageKeepAwake("online", true)).toBe(true);
  });

  it("never engages when the flag is locked or the toggle is off", () => {
    expect(shouldEngageKeepAwake("online", false)).toBe(false);
  });

  it("never engages for a non-online verdict", () => {
    for (const v of ["starting", "stopped", "crashed", "soap_unreachable"] as const) {
      expect(shouldEngageKeepAwake(v, true)).toBe(false);
    }
    expect(shouldEngageKeepAwake(null, true)).toBe(false);
  });
});

describe("verdictTransitionActions — keep-awake", () => {
  it("does not decide engagement at all (that is shouldEngageKeepAwake's job)", () => {
    expect(verdictTransitionActions("stopped", "online", allowed).keepAwake).toBeNull();
    expect(verdictTransitionActions("starting", "online", allowed).keepAwake).toBeNull();
    expect(verdictTransitionActions(null, "online", allowed).keepAwake).toBeNull();
  });

  it("releases on transitions into stopped and crashed, even when no longer allowed", () => {
    expect(verdictTransitionActions("online", "stopped", allowed).keepAwake).toBe("off");
    expect(verdictTransitionActions("online", "crashed", allowed).keepAwake).toBe("off");
    // Safety: releasing must not depend on the flag still being unlocked.
    expect(verdictTransitionActions("online", "stopped", denied).keepAwake).toBe("off");
  });

  it("does nothing when the verdict does not change", () => {
    expect(verdictTransitionActions("online", "online", allowed).keepAwake).toBeNull();
    expect(verdictTransitionActions("stopped", "stopped", allowed).keepAwake).toBeNull();
  });

  it("skips the release on the very first poll (nothing was ever engaged)", () => {
    expect(verdictTransitionActions(null, "stopped", allowed).keepAwake).toBeNull();
  });

  it("leaves keep-awake alone on transitions into intermediate states", () => {
    expect(verdictTransitionActions("stopped", "starting", allowed).keepAwake).toBeNull();
    expect(verdictTransitionActions("online", "soap_unreachable", allowed).keepAwake).toBeNull();
  });
});

describe("verdictTransitionActions — LAN auto-refresh", () => {
  it("fires exactly on starting→online when allowed", () => {
    expect(verdictTransitionActions("starting", "online", allowed).lanRefresh).toBe(true);
  });

  it("does not fire on other paths into online", () => {
    expect(verdictTransitionActions("stopped", "online", allowed).lanRefresh).toBe(false);
    expect(verdictTransitionActions("soap_unreachable", "online", allowed).lanRefresh).toBe(false);
    expect(verdictTransitionActions(null, "online", allowed).lanRefresh).toBe(false);
    expect(verdictTransitionActions("online", "online", allowed).lanRefresh).toBe(false);
  });

  it("never fires when locked/disabled", () => {
    expect(verdictTransitionActions("starting", "online", denied).lanRefresh).toBe(false);
  });
});

describe("parseLanStatus", () => {
  it("parses the ON line with its realm address", () => {
    expect(parseLanStatus("LAN play: ON  (realm address 192.168.1.50)\nOther PCs use: set realmlist 192.168.1.50\n")).toEqual({
      on: true,
      ip: "192.168.1.50",
    });
  });

  it("parses the OFF line (address is localhost, reported but off)", () => {
    expect(parseLanStatus("LAN play: OFF (realm address 127.0.0.1 -- this PC only)\n")).toEqual({
      on: false,
      ip: "127.0.0.1",
    });
  });

  it("treats error/garbage output as off with no ip", () => {
    expect(parseLanStatus("[dml] ERROR: Could not read the realm address from the database.")).toEqual({
      on: false,
      ip: null,
    });
    expect(parseLanStatus("")).toEqual({ on: false, ip: null });
  });
});
