import { describe, expect, it } from "vitest";
import { backupSummaryLine } from "./backup-summary";

describe("backupSummaryLine", () => {
  it("pluralizes each count independently", () => {
    expect(backupSummaryLine({ characters: 3, accounts: 2, bots: 5 })).toBe(
      "3 characters · 2 accounts · 5 bots",
    );
  });

  it("uses the singular at a count of one -- bots included", () => {
    expect(backupSummaryLine({ characters: 1, accounts: 1, bots: 1 })).toBe(
      "1 character · 1 account · 1 bot",
    );
  });

  it("keeps the bots clause silent for none and for an unreadable schema", () => {
    expect(backupSummaryLine({ characters: 4, accounts: 1, bots: 0 })).toBe("4 characters · 1 account");
    expect(backupSummaryLine({ characters: 4, accounts: 1, bots: null })).toBe("4 characters · 1 account");
  });

  it("pluralizes a zero character/account count", () => {
    expect(backupSummaryLine({ characters: 0, accounts: 0, bots: null })).toBe("0 characters · 0 accounts");
  });
});
