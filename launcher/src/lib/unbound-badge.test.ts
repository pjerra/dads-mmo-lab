import { describe, it, expect } from "vitest";
import { unboundBadge } from "./unbound-badge";
import type { UnboundStatus } from "./api";

const base: UnboundStatus = {
  server_dir: "C:/games/wow",
  state_present: true,
  addon_version: null,
  completed: [],
  next_stage: null,
  phase: "absent",
  patch: "absent",
  module_staged: false,
  last_error: null,
};
const s = (o: Partial<UnboundStatus>): UnboundStatus => ({ ...base, ...o });

describe("what Home and Library say about the add-on", () => {
  it("names the version when it is installed", () => {
    const b = unboundBadge(s({ phase: "installed", addon_version: "1.4.0", patch: "applied" }));
    expect(b?.tone).toBe("ok");
    expect(b?.text).toContain("1.4.0");
  });

  it("still says installed when the version is unknown", () => {
    // A bash-scripted install leaves no state file, so the version can be
    // absent while the add-on demonstrably is not.
    const b = unboundBadge(s({ phase: "installed", addon_version: null, patch: "applied" }));
    expect(b?.tone).toBe("ok");
    expect(b?.text).toMatch(/installed/i);
  });

  /**
   * The state the user actually hit: a launcher closed mid-rebuild leaves the
   * install recorded through `up` and never through `ready`. Until this badge
   * existed, the only way to discover that was to open the page you would only
   * open if you already suspected something.
   */
  it("flags an unfinished install and names the stage to resume from", () => {
    const b = unboundBadge(s({ phase: "installing", next_stage: "ready" }));
    expect(b?.tone).toBe("warn");
    expect(b?.text).toMatch(/unfinished/i);
    expect(b?.detail).toContain("ready");
  });

  it("flags an unfinished uninstall too", () => {
    const b = unboundBadge(s({ phase: "uninstalling" }));
    expect(b?.tone).toBe("warn");
    expect(b?.detail).toMatch(/uninstall/i);
  });

  it("a half-applied patch OUTRANKS an installed phase", () => {
    // This is the condition that makes install refuse and uninstall decline to
    // reverse. A server showing "Unbound 1.4.0" while carrying it would be
    // telling the user the opposite of what they need to act on.
    const b = unboundBadge(s({ phase: "installed", addon_version: "1.4.0", patch: "MIXED" }));
    expect(b?.tone).toBe("warn");
    expect(b?.text).toMatch(/patch/i);
    expect(b?.text).not.toContain("1.4.0");
    expect(b?.detail).toMatch(/six/);
  });

  it("says NOTHING when the add-on is absent", () => {
    expect(unboundBadge(s({ phase: "absent" }))).toBeNull();
  });

  it("says nothing when we could not find out, rather than 'not installed'", () => {
    // null covers WSL mode (the command refuses), a failed probe, and
    // not-fetched-yet. Rendering "not installed" for any of those would be a
    // claim nothing checked.
    expect(unboundBadge(null)).toBeNull();
    expect(unboundBadge(undefined)).toBeNull();
  });

  it("ignores a phase a newer engine invented", () => {
    // Same forward-compat rule this project applies to every union: an unknown
    // value must never crash or invent a claim.
    expect(unboundBadge(s({ phase: "quantum-superposition" }))).toBeNull();
  });
});
