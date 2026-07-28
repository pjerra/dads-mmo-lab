import { describe, it, expect } from "vitest";
import { isValidCharName, recipientStatus, canSendTo } from "./mail-recipient";

// Picking from a dropdown made a bad recipient impossible. Typing makes typos
// the normal failure -- and mail to a character that doesn't exist is lost
// in-game with no bounce. These rules decide what the page may send and what
// it must warn about first.

describe("isValidCharName", () => {
  it("accepts an ordinary character name", () => {
    expect(isValidCharName("Gasino")).toBe(true);
  });

  it("accepts digits and underscores", () => {
    expect(isValidCharName("Alt_2")).toBe(true);
  });

  it("accepts a name at the 12-character limit", () => {
    expect(isValidCharName("Abcdefghijkl")).toBe(true);
  });

  it("rejects a name past the 12-character limit", () => {
    expect(isValidCharName("Abcdefghijklm")).toBe(false);
  });

  it("rejects an empty name", () => {
    expect(isValidCharName("")).toBe(false);
  });

  it("rejects spaces", () => {
    expect(isValidCharName("Two Words")).toBe(false);
  });

  it("rejects extended-Latin and Cyrillic names the action verbs refuse", () => {
    // The accounts read path allows these, but every action verb enforces
    // ^[A-Za-z0-9_]{1,12}$ -- sending one produces an opaque BAD_ARG.
    expect(isValidCharName("Bjørn")).toBe(false);
    expect(isValidCharName("Спартак")).toBe(false);
  });

  it("rejects quoting and command characters outright", () => {
    expect(isValidCharName("bob'; --")).toBe(false);
    expect(isValidCharName('say "hi"')).toBe(false);
    expect(isValidCharName("a\nb")).toBe(false);
  });
});

describe("recipientStatus", () => {
  const known = ["Gasino", "Perzi", "Alt_2"];

  it("reports an empty box as empty, not invalid", () => {
    expect(recipientStatus("", known)).toBe("empty");
    expect(recipientStatus("   ", known)).toBe("empty");
  });

  it("reports a malformed name as invalid", () => {
    expect(recipientStatus("Two Words", known)).toBe("invalid");
  });

  it("reports a known character as known", () => {
    expect(recipientStatus("Gasino", known)).toBe("known");
  });

  it("matches known characters regardless of typed case", () => {
    expect(recipientStatus("gAsInO", known)).toBe("known");
  });

  it("ignores surrounding whitespace", () => {
    expect(recipientStatus("  Perzi  ", known)).toBe("known");
  });

  it("reports a well-formed name nobody recognises as unknown", () => {
    // Deliberately NOT invalid: bots and freshly-created characters are
    // legitimately absent from the account list.
    expect(recipientStatus("Newbie", known)).toBe("unknown");
  });

  it("treats every well-formed name as unknown when no list has loaded", () => {
    expect(recipientStatus("Gasino", [])).toBe("unknown");
  });
});

describe("canSendTo", () => {
  const known = ["Gasino"];

  it("allows sending to a known character", () => {
    expect(canSendTo("Gasino", known)).toBe(true);
  });

  it("allows sending to an unrecognised but well-formed name", () => {
    expect(canSendTo("Newbie", known)).toBe(true);
  });

  it("blocks an empty recipient", () => {
    expect(canSendTo("", known)).toBe(false);
  });

  it("blocks a malformed recipient before it reaches the CLI", () => {
    expect(canSendTo("Two Words", known)).toBe(false);
  });
});
