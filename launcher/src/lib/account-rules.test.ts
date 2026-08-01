import { describe, it, expect } from "vitest";
import { validAccountName, validAccountPass, accountRuleError } from "./account-rules";

describe("account rules mirrored from the CLI", () => {
  it("accepts what the server accepts", () => {
    expect(validAccountName("dmlsoap")).toBe(true);
    expect(validAccountName("abc")).toBe(true);
    expect(validAccountName("a".repeat(20))).toBe(true);
    expect(validAccountPass("hunter2")).toBe(true);
    expect(validAccountPass("Q7QoLBsg12NgZTOl")).toBe(true); // 16, the maximum
  });

  it("rejects the boundaries the CLI rejects", () => {
    expect(validAccountName("ab")).toBe(false); // 2 chars
    expect(validAccountName("a".repeat(21))).toBe(false);
    expect(validAccountName("dml soap")).toBe(false); // space
    expect(validAccountName("dml-soap")).toBe(false); // hyphen is NOT allowed in a name
    expect(validAccountPass("abc")).toBe(false); // 3 chars
    expect(validAccountPass("a".repeat(17))).toBe(false);
  });

  /// THE CASE THAT COSTS A REAL ACCOUNT. A password the launcher will reject is
  /// only discovered AFTER the user has pasted `account create` into their own
  /// server -- and the retry re-emits the same command, which AzerothCore
  /// refuses as "already exists", so the password is never updated and the
  /// failure blames the wrong command.
  it("rejects the punctuation people actually put in passwords", () => {
    for (const p of ["P@ssw0rd$", "my.password", "pass,word", "a/b", "back\\slash", "with space"]) {
      expect(validAccountPass(p), p).toBe(false);
    }
    // ...while the set AzerothCore does accept still passes.
    for (const p of ["P@ssw0rd", "a_b-c", "x#y%z", "q+r=s", "hey!"]) {
      expect(validAccountPass(p), p).toBe(true);
    }
  });

  it("explains the punctuation rather than restating the length rule", () => {
    const msg = accountRuleError("dmlsoap", "P@ssw0rd$");
    expect(msg).toBeTruthy();
    // The surprise is always the character set, so the message has to show it.
    expect(msg).toMatch(/\$/);
  });

  it("reports no error for a usable pair", () => {
    expect(accountRuleError("dmlsoap", "hunter2")).toBeNull();
  });

  it("checks the name before the password, so one message is enough", () => {
    expect(accountRuleError("ab", "alsobad$")).toMatch(/Account name/);
  });
});
