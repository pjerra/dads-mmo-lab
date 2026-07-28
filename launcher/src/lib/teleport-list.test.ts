import { describe, expect, it } from "vitest";
import { locListNotice } from "./teleport-list";

describe("locListNotice", () => {
  it("reports the in-flight load ahead of whatever the last one did", () => {
    expect(locListNotice(true, false, 0)?.kind).toBe("loading");
    expect(locListNotice(true, true, 12)?.kind).toBe("loading");
  });

  it("blames the filter only when a load succeeded and returned nothing", () => {
    const n = locListNotice(false, false, 0);
    expect(n?.kind).toBe("empty");
    expect(n?.text).toBe("No locations match the filter.");
  });

  it("says the load failed rather than blaming the filter", () => {
    const n = locListNotice(false, true, 0);
    expect(n?.kind).toBe("failed");
    expect(n?.text).not.toMatch(/filter/i);
  });

  it("flags a failed refresh that left the previous locations on screen", () => {
    const n = locListNotice(false, true, 12);
    expect(n?.kind).toBe("stale");
    expect(n?.text).toMatch(/last/i);
  });

  it("has nothing to say about a healthy non-empty list", () => {
    expect(locListNotice(false, false, 12)).toBeNull();
  });
});
