import { describe, expect, it } from "vitest";
import { canBuild } from "./module-canbuild";

describe("canBuild", () => {
  it("fails open when the field is missing (older CLI)", () => {
    expect(canBuild({} as never)).toBe(true);
    expect(canBuild(null)).toBe(true);
  });
  it("honours an explicit false", () => {
    expect(canBuild({ can_build: false })).toBe(false);
  });
  it("honours an explicit true", () => {
    expect(canBuild({ can_build: true })).toBe(true);
  });
});
