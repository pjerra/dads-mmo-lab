import { describe, expect, it } from "vitest";
import { confActivationChip, outcomeFromErrorCode } from "./conf-activation-chip";

// Round 2, Task 2: conf-activation is silent on success (activated /
// already-active -- a user's own edit already there); the ONLY surfaced case
// is failure (no-dist / no-conf / error), which gets a clickable
// "conf not activated" chip on that row instead of vanishing silently.

describe("confActivationChip", () => {
  it("activated -- silent, no chip", () => {
    expect(confActivationChip("activated")).toBeNull();
  });

  it("already-active -- silent, no chip (the user's own edit is already there)", () => {
    expect(confActivationChip("already-active")).toBeNull();
  });

  it("no-dist -- conf-failed chip", () => {
    expect(confActivationChip("no-dist")).toEqual({
      id: "conf-activation",
      kind: "conf-failed",
      label: "conf not activated",
      clickable: true,
    });
  });

  it("no-conf -- conf-failed chip", () => {
    expect(confActivationChip("no-conf")).toEqual({
      id: "conf-activation",
      kind: "conf-failed",
      label: "conf not activated",
      clickable: true,
    });
  });

  it("error -- conf-failed chip", () => {
    expect(confActivationChip("error")).toEqual({
      id: "conf-activation",
      kind: "conf-failed",
      label: "conf not activated",
      clickable: true,
    });
  });
});

describe("outcomeFromErrorCode", () => {
  it("maps the three known conf-activate error codes onto their outcome", () => {
    expect(outcomeFromErrorCode("EXISTS")).toBe("already-active");
    expect(outcomeFromErrorCode("NEEDS_REBUILD")).toBe("no-dist");
    expect(outcomeFromErrorCode("NO_CONF")).toBe("no-conf");
  });

  it("anything else (including missing code) is a generic error", () => {
    expect(outcomeFromErrorCode(undefined)).toBe("error");
    expect(outcomeFromErrorCode("INTERNAL")).toBe("error");
  });
});
