import { describe, expect, it } from "vitest";
import { recallHistory, logSeverity, consoleCommands, commandSuggestions } from "./console-input";
import { CORE_COMMANDS } from "./gm-commands";

describe("recallHistory", () => {
  const hist = ["server info", "saveall", "gm on"]; // oldest -> newest

  it("Up from the draft recalls the newest command", () => {
    expect(recallHistory(hist, null, "up", "typed")).toEqual({ value: "gm on", cursor: 2 });
  });

  it("Up steps toward older commands and clamps at the oldest", () => {
    expect(recallHistory(hist, 2, "up", "typed")).toEqual({ value: "saveall", cursor: 1 });
    expect(recallHistory(hist, 1, "up", "typed")).toEqual({ value: "server info", cursor: 0 });
    expect(recallHistory(hist, 0, "up", "typed")).toEqual({ value: "server info", cursor: 0 });
  });

  it("Down steps toward newer commands", () => {
    expect(recallHistory(hist, 0, "down", "typed")).toEqual({ value: "saveall", cursor: 1 });
    expect(recallHistory(hist, 1, "down", "typed")).toEqual({ value: "gm on", cursor: 2 });
  });

  it("Down past the newest restores the saved draft and leaves recall", () => {
    expect(recallHistory(hist, 2, "down", "typed")).toEqual({ value: "typed", cursor: null });
  });

  it("Down while already at the draft is a no-op", () => {
    expect(recallHistory(hist, null, "down", "typed")).toEqual({ value: "typed", cursor: null });
  });

  it("empty history keeps the draft in place", () => {
    expect(recallHistory([], null, "up", "typed")).toEqual({ value: "typed", cursor: null });
    expect(recallHistory([], null, "down", "typed")).toEqual({ value: "typed", cursor: null });
  });

  it("a full up-then-down round trip returns to the draft", () => {
    let r = recallHistory(hist, null, "up", "draft"); // -> gm on (2)
    r = recallHistory(hist, r.cursor, "up", "draft"); // -> saveall (1)
    r = recallHistory(hist, r.cursor, "down", "draft"); // -> gm on (2)
    r = recallHistory(hist, r.cursor, "down", "draft"); // -> draft (null)
    expect(r).toEqual({ value: "draft", cursor: null });
  });
});

describe("logSeverity", () => {
  it("flags ERROR/FATAL lines as error", () => {
    expect(logSeverity("2026-07-20 ERROR: could not bind socket")).toBe("error");
    expect(logSeverity(">> FATAL - database offline")).toBe("error");
  });

  it("flags WARN/WARNING lines as warn", () => {
    expect(logSeverity("WARN: config value out of range")).toBe("warn");
    expect(logSeverity("WARNING deprecated setting used")).toBe("warn");
  });

  it("leaves ordinary lines normal", () => {
    expect(logSeverity("World initialized in 12 seconds")).toBe("normal");
    expect(logSeverity(">> Loading spells...")).toBe("normal");
  });

  it("does not miscolour lower-case prose containing 'error'", () => {
    expect(logSeverity("Player reported an error in chat")).toBe("normal");
  });

  it("error outranks warn when both markers appear", () => {
    expect(logSeverity("WARN then ERROR on the same line")).toBe("error");
  });
});

describe("consoleCommands", () => {
  it("strips the leading dot and placeholder args", () => {
    const stems = consoleCommands([
      { cmd: ".tele <place>", what: "" },
      { cmd: ".saveall", what: "" },
    ]);
    expect(stems).toEqual(["tele", "saveall"]);
  });

  it("splits slash-separated variants into separate stems", () => {
    expect(consoleCommands([{ cmd: ".gm on / .gm off", what: "" }])).toEqual(["gm on", "gm off"]);
  });

  it("keeps multi-word command stems intact", () => {
    expect(consoleCommands([{ cmd: ".modify money <copper>", what: "" }])).toEqual(["modify money"]);
  });

  it("derives dot-less stems from the real catalog", () => {
    const stems = consoleCommands(CORE_COMMANDS);
    expect(stems).toContain("tele");
    expect(stems).toContain("server info");
    expect(stems).toContain("gm off");
    expect(stems.every((s) => !s.startsWith("."))).toBe(true);
    expect(stems.every((s) => !s.includes("<"))).toBe(true);
  });
});

describe("commandSuggestions", () => {
  const pool = ["tele", "levelup", "modify money", "modify speed", "server info", "saveall"];

  it("returns nothing for empty input", () => {
    expect(commandSuggestions(pool, "")).toEqual([]);
    expect(commandSuggestions(pool, "   ")).toEqual([]);
  });

  it("prefix-matches case-insensitively", () => {
    expect(commandSuggestions(pool, "mod")).toEqual(["modify money", "modify speed"]);
    expect(commandSuggestions(pool, "SERV")).toEqual(["server info"]);
  });

  it("drops an exact match (nothing left to complete)", () => {
    expect(commandSuggestions(pool, "tele")).toEqual([]);
  });

  it("de-duplicates favorites that repeat catalog stems", () => {
    expect(commandSuggestions([...pool, "saveall", "sadface"], "sa")).toEqual(["saveall", "sadface"]);
  });

  it("honors the cap", () => {
    const many = ["ta", "tb", "tc", "td", "te"];
    expect(commandSuggestions(many, "t", 3)).toEqual(["ta", "tb", "tc"]);
  });
});
