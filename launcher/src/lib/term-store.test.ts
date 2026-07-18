import { describe, expect, it } from "vitest";
import { beginRun, claimInstallInvoke, clearBuf, termBuf, termText } from "./term-store.svelte";
import { initialTermState, type TermState } from "./terminal-state";

describe("termBuf", () => {
  it("creates lazily with an empty term and show=false", () => {
    const buf = termBuf("fresh-key");
    expect(buf.term).toEqual(initialTermState());
    expect(buf.show).toBe(false);
  });

  it("returns the same object for the same key", () => {
    const a = termBuf("same-key");
    const b = termBuf("same-key");
    expect(a).toBe(b);
  });

  it("keeps different keys independent", () => {
    const a = termBuf("key-a");
    const b = termBuf("key-b");
    a.show = true;
    expect(b.show).toBe(false);
    expect(a).not.toBe(b);
  });
});

describe("beginRun", () => {
  it("resets an existing buf's term and sets show=true", () => {
    const buf = termBuf("run-key");
    buf.term.totalLines = 5;
    buf.show = false;

    const returned = beginRun("run-key");

    expect(returned).toBe(buf);
    expect(buf.term).toEqual(initialTermState());
    expect(buf.show).toBe(true);
  });
});

describe("clearBuf", () => {
  it("empties the term and hides it", () => {
    const buf = termBuf("clear-key");
    buf.term.totalLines = 3;
    buf.show = true;

    clearBuf("clear-key");

    expect(buf.term).toEqual(initialTermState());
    expect(buf.show).toBe(false);
  });
});

// Module-singleton state: these cases run in order within this file and use
// nonces disjoint from each other so the shared `invokedNonce` can't leak
// meaning between tests.
describe("claimInstallInvoke", () => {
  it("grants the first claim of a nonce (fresh install invokes)", () => {
    expect(claimInstallInvoke(101)).toBe(true);
  });

  it("denies a repeat claim of the same nonce (remount must not re-invoke)", () => {
    expect(claimInstallInvoke(101)).toBe(false);
    expect(claimInstallInvoke(101)).toBe(false);
  });

  it("grants a new nonce after a previous one was claimed (next install runs)", () => {
    expect(claimInstallInvoke(102)).toBe(true);
    // and the old nonce stays spent even after a newer one was claimed
    expect(claimInstallInvoke(102)).toBe(false);
    expect(claimInstallInvoke(103)).toBe(true);
  });
});

describe("termText", () => {
  it("returns an empty string for the empty state", () => {
    expect(termText(initialTermState())).toBe("");
  });

  it("formats one section with two lines", () => {
    const state: TermState = {
      ...initialTermState(),
      sections: [
        {
          name: "install",
          status: "ok",
          collapsed: true,
          lines: [
            { level: "info", text: "cloning repo" },
            { level: "info", text: "done" },
          ],
        },
      ],
    };
    expect(termText(state)).toBe("== install ==\ncloning repo\ndone");
  });

  it("separates two sections with a blank line", () => {
    const state: TermState = {
      ...initialTermState(),
      sections: [
        { name: "one", status: "ok", collapsed: true, lines: [{ level: "info", text: "a" }] },
        { name: "two", status: "ok", collapsed: true, lines: [{ level: "info", text: "b" }] },
      ],
    };
    expect(termText(state)).toBe("== one ==\na\n\n== two ==\nb");
  });
});
