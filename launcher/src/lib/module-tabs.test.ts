import { describe, expect, it } from "vitest";
import {
  MODULE_TABS,
  MODULE_TAB_LABELS,
  canOfferUpdate,
  repoAfterUpdate,
  updateDoneNote,
  updatesWithServer,
} from "./module-tabs";
import type { ModuleCheckRepo } from "./api";

describe("MODULE_TABS", () => {
  it("has the three tabs in strip order", () => {
    expect(MODULE_TABS).toEqual(["modules", "tuning", "files"]);
  });

  it("every tab has a non-empty label", () => {
    for (const t of MODULE_TABS) {
      expect(MODULE_TAB_LABELS[t].length).toBeGreaterThan(0);
    }
  });
});

// mod-playerbots updates together with the server core -- the CLI refuses
// `module update --key mod-playerbots`, so the GUI must never offer it.
describe("updatesWithServer", () => {
  it("is true only for mod-playerbots", () => {
    expect(updatesWithServer("mod-playerbots")).toBe(true);
    expect(updatesWithServer("mod-transmog")).toBe(false);
    expect(updatesWithServer("mod-arac")).toBe(false);
  });
});

describe("canOfferUpdate", () => {
  it("offers the button only for a known behind-count > 0", () => {
    expect(canOfferUpdate("mod-transmog", 1)).toBe(true);
    expect(canOfferUpdate("mod-transmog", 374)).toBe(true);
  });

  it("never offers it when up to date or unknown", () => {
    expect(canOfferUpdate("mod-transmog", 0)).toBe(false);
    expect(canOfferUpdate("mod-transmog", null)).toBe(false);
    expect(canOfferUpdate("mod-transmog", undefined)).toBe(false);
  });

  it("never offers it for mod-playerbots, however far behind", () => {
    expect(canOfferUpdate("mod-playerbots", 2661)).toBe(false);
  });
});

describe("updateDoneNote", () => {
  it("points at the rebuild when the pull changed a compiled module", () => {
    expect(updateDoneNote({ changed: true, pending_rebuild: true })).toBe(
      "Updated — rebuild required before it takes effect.",
    );
  });

  it("skips the rebuild wording for data-only changes (mod-arac)", () => {
    expect(updateDoneNote({ changed: true, pending_rebuild: false, rebuild_required: false })).toBe(
      "Updated — no rebuild needed.",
    );
    // An older CLI sends no rebuild_required at all -- keep the old wording.
    expect(updateDoneNote({ changed: true, pending_rebuild: false })).toBe(
      "Updated — no rebuild needed.",
    );
  });

  it("does not congratulate a compiled module whose rebuild was never queued", () => {
    // changed + !pending_rebuild + rebuild_required = the marker write failed:
    // this module will NOT compile on its own, so "no rebuild needed" would be
    // the exact opposite of the truth.
    expect(updateDoneNote({ changed: true, pending_rebuild: false, rebuild_required: true })).toBe(
      "Updated — but the rebuild could not be queued. Start Server rebuild yourself.",
    );
  });

  it("reports already-up-to-date pulls (and a missing done payload)", () => {
    expect(updateDoneNote({ changed: false, pending_rebuild: false })).toBe("Already up to date.");
    expect(updateDoneNote(undefined)).toBe("Already up to date.");
  });
});

describe("repoAfterUpdate", () => {
  const repo: ModuleCheckRepo = {
    label: "mod-transmog",
    url: "https://github.com/azerothcore/mod-transmog.git",
    branch: "master",
    head: "abc1234",
    dirty: 2,
    behind: 5,
  };

  it("moves head to the pulled sha and zeroes behind, keeping the rest", () => {
    expect(repoAfterUpdate(repo, "def5678")).toEqual({ ...repo, head: "def5678", behind: 0 });
  });

  it("does not mutate the input row", () => {
    repoAfterUpdate(repo, "def5678");
    expect(repo.head).toBe("abc1234");
    expect(repo.behind).toBe(5);
  });
});
