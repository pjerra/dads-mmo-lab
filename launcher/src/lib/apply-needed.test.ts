import { describe, expect, it } from "vitest";
import {
  bannerText,
  escalate,
  fastRestartBlockedReason,
  normalizeApplyNeeded,
  type ApplyNeeded,
} from "./apply-needed";

describe("escalate", () => {
  it("keeps the strongest apply across a multi-row save", () => {
    // The order the rows happen to save in must not decide the answer.
    expect(escalate("world-restart", "recreate")).toBe("recreate");
    expect(escalate("recreate", "world-restart")).toBe("recreate");
  });

  it("never downgrades a pending recreate", () => {
    // THE REGRESSION SHAPE: row 1 removed a shadowing env key (recreate), row 2
    // only rewrote a conf (world-restart). Taking the later answer would tell
    // the user the fast restart is enough, and their first change would revert.
    let a: ApplyNeeded = "none";
    a = escalate(a, "recreate");
    a = escalate(a, "world-restart");
    a = escalate(a, "none");
    expect(a).toBe("recreate");
  });

  it("treats none as the floor, not as a reset", () => {
    expect(escalate("none", "none")).toBe("none");
    expect(escalate("none", "world-restart")).toBe("world-restart");
    expect(escalate("recreate", "none")).toBe("recreate");
  });
});

describe("normalizeApplyNeeded", () => {
  it("takes the CLI's answer when it gives one", () => {
    expect(normalizeApplyNeeded({ apply_needed: "none" })).toBe("none");
    expect(normalizeApplyNeeded({ apply_needed: "world-restart" })).toBe("world-restart");
    expect(normalizeApplyNeeded({ apply_needed: "recreate" })).toBe("recreate");
  });

  it("prefers the CLI's answer over restart_required when they disagree", () => {
    // A live-applied save reports restart_required:false AND apply_needed:none;
    // the field is authoritative, so an odd combination must not resurrect the
    // banner.
    expect(normalizeApplyNeeded({ restart_required: true, apply_needed: "none" })).toBe("none");
  });

  it("assumes the STRONGER apply when the field is missing", () => {
    // An older `dml` in the distro. Guessing world-restart here would reintroduce
    // the silent-revert bug; a needless recreate is merely slower.
    expect(normalizeApplyNeeded({ restart_required: true })).toBe("recreate");
    expect(normalizeApplyNeeded({ restart_required: true, applied: "restart" })).toBe("recreate");
  });

  it("assumes the stronger apply for an unknown future value too", () => {
    expect(normalizeApplyNeeded({ restart_required: true, apply_needed: "warp-drive" })).toBe(
      "recreate",
    );
  });

  it("stays quiet when nothing changed", () => {
    expect(normalizeApplyNeeded({})).toBe("none");
    expect(normalizeApplyNeeded({ restart_required: false })).toBe("none");
    expect(normalizeApplyNeeded({ restart_required: false, applied: "live" })).toBe("none");
  });
});

describe("user-facing copy", () => {
  it("names which button applies a recreate, and says the fast one will not", () => {
    const t = bannerText("recreate");
    expect(t).toMatch(/Restart on the Home page/);
    // Not merely the word "cannot" somewhere: the sentence must name the fast
    // restart as the thing that cannot do it, and say why (same container).
    expect(t).toMatch(/fast world-only restart/);
    expect(t).toMatch(/cannot/);
    expect(t).toMatch(/same container/);
  });

  it("tells the user the fast restart suffices when it does", () => {
    expect(bannerText("world-restart")).toMatch(/fast world-only restart on Home is enough/);
  });

  it("says nothing when no apply is pending", () => {
    expect(bannerText("none")).toBe("");
  });

  it("blocks the fast restart only for a pending recreate", () => {
    // This reason IS the user's only explanation for a disabled button, so it
    // must explain itself -- "x" is non-empty and useless.
    const why = fastRestartBlockedReason("recreate");
    expect(why).toMatch(/full Restart/);
    expect(why).toMatch(/running container/);
    expect(why.length).toBeGreaterThan(40);
    expect(fastRestartBlockedReason("world-restart")).toBe("");
    expect(fastRestartBlockedReason("none")).toBe("");
  });
});
