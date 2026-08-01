import { describe, it, expect } from "vitest";
import { translateNativeEvent } from "./native-install";
import type { TermEvent } from "./api";

describe("translateNativeEvent", () => {
  it("only ever ends the run on a terminal event", () => {
    // The whole reason this translation exists as a pure function: an install
    // is hours long, and any mid-run event that could set an exit code would
    // close the panel over a live build.
    const midRun: TermEvent[] = [
      { event: "section_start", name: "build" },
      { event: "line", level: "info", text: "compiling" },
      { event: "line", level: "warn", text: "low memory" },
      { event: "line", level: "error", text: "a compiler warned" },
      { event: "section_end", name: "build", status: "ok" },
      { event: "section_end", name: "build", status: "error" },
    ];
    for (const e of midRun) {
      expect(translateNativeEvent(e).exit).toBeNull();
    }
    expect(translateNativeEvent({ event: "done", data: {} }).exit).toBe(0);
    expect(translateNativeEvent({ event: "error", error: { code: "X", message: "m", hint: "" } }).exit).toBe(1);
  });

  it("never drops the hint, which is often the only actionable half", () => {
    // INSTALL_STACK_CONFLICT is the case that proves it: the message says a
    // stack is in the way, the hint says WHICH and what to do about it.
    const got = translateNativeEvent({
      event: "error",
      error: {
        code: "INSTALL_STACK_CONFLICT",
        message: "Another server stack is running.",
        hint: "Stop it from Home first.",
      },
    });
    expect(got.text).toContain("Another server stack is running.");
    expect(got.text).toContain("Stop it from Home first.");
  });

  it("survives an error envelope with nothing in it", () => {
    // A failure that renders as "undefined" is worse than a generic sentence.
    const got = translateNativeEvent({ event: "error" } as unknown as TermEvent);
    expect(got.exit).toBe(1);
    expect(got.text).not.toContain("undefined");
    expect(got.text.trim().length).toBeGreaterThan(0);
  });

  it("ignores an unknown event instead of throwing", () => {
    // The standing forward-compat rule for this union -- `pct` is reserved and
    // a future engine event must not be able to crash a live install's panel.
    expect(translateNativeEvent({ event: "pct", value: 40 } as unknown as TermEvent)).toEqual({
      text: "",
      exit: null,
    });
  });

  it("tags warn and error lines but not info", () => {
    // info is the overwhelming majority of an 8-stage build; tagging it would
    // bury the two levels a user scrolls back to find.
    expect(translateNativeEvent({ event: "line", level: "info", text: "cloning" }).text).toBe("cloning\n");
    expect(translateNativeEvent({ event: "line", level: "warn", text: "slow" }).text).toContain("[warn]");
  });

  it("stays quiet on a successful section_end", () => {
    // Eight stages of "ok" would be noise; the next section header already
    // says the last one finished.
    expect(translateNativeEvent({ event: "section_end", name: "up", status: "ok" }).text).toBe("");
    expect(translateNativeEvent({ event: "section_end", name: "up", status: "error" }).text).toContain("up");
  });
});
