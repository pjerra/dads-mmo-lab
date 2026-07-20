import { describe, expect, it } from "vitest";
import { lintConfContent } from "./conf-lint";

describe("lintConfContent", () => {
  it("accepts a clean conf (assignments, comments, blanks)", () => {
    const conf = [
      "# playerbots.conf",
      "",
      "AiPlayerbot.RandomBotAutologin = 1",
      "AiPlayerbot.MinRandomBots = 50",
      "   # indented comment",
    ].join("\n");
    expect(lintConfContent(conf)).toEqual([]);
  });

  it("flags a line with no '=' assignment", () => {
    const conf = "Key = 1\nthis is not a setting\nOther = 2";
    expect(lintConfContent(conf)).toEqual([{ line: 2, text: "this is not a setting" }]);
  });

  it("flags a line whose key is empty (leading '=')", () => {
    expect(lintConfContent("= orphan value")).toEqual([{ line: 1, text: "= orphan value" }]);
  });

  it("allows an empty value (key with nothing after '=')", () => {
    expect(lintConfContent("Motd =")).toEqual([]);
  });

  it("allows values that themselves contain '='", () => {
    expect(lintConfContent('Greeting = a = b')).toEqual([]);
  });

  it("reports 1-indexed line numbers across CRLF and LF", () => {
    const conf = "Good = 1\r\nbad line\r\nAlso = 2\r\nanother bad";
    expect(lintConfContent(conf)).toEqual([
      { line: 2, text: "bad line" },
      { line: 4, text: "another bad" },
    ]);
  });

  it("ignores trailing whitespace when classifying", () => {
    expect(lintConfContent("Key = 1   \n   ")).toEqual([]);
  });
});
