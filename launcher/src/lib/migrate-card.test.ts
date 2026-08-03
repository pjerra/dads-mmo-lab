import { describe, it, expect } from "vitest";
import { migrateCard, migrateErrorHint } from "./migrate-card";
import type { MigrateStatus } from "./api";

const base: MigrateStatus = {
  title_dir: "C:/games/wow-server-playerbots",
  export_present: false,
  missing: [],
  state_present: false,
  completed: [],
  next_stage: null,
  last_error: null,
};
const s = (o: Partial<MigrateStatus>): MigrateStatus => ({ ...base, ...o });

describe("what the migration card says before anything is pressed", () => {
  it("offers the import when a complete export is sitting there", () => {
    const c = migrateCard(s({ export_present: true }));
    expect(c.action).toBe("import");
    expect(c.disabled).toBe(false);
  });

  it("names what is missing rather than just refusing", () => {
    // "Something is wrong with your export" sends someone to re-run a
    // 40-minute export when one file is absent.
    const c = migrateCard(s({ missing: ["db-dump.sql.gz", "img-worldserver.tar.gz"] }));
    expect(c.action).toBe("blocked");
    expect(c.disabled).toBe(true);
    expect(c.detail).toContain("db-dump.sql.gz");
    expect(c.detail).toContain("img-worldserver.tar.gz");
  });

  it("says where the export has to go, and why the folder name matters", () => {
    const c = migrateCard(s({ missing: ["etc/"] }));
    expect(c.detail).toContain("C:/games/wow-server-playerbots");
    expect(c.detail).toMatch(/name/i);
  });

  /**
   * The button must not say "Import" when pressing it resumes. Same rule the
   * Library's native-install button already follows: a label that describes
   * something the app is not about to do matters most to the user who just lost
   * hours of work and needs to know they are not paying for it twice.
   */
  it("says RESUME when a previous run got somewhere", () => {
    const c = migrateCard(s({ state_present: true, next_stage: "db-restore" }));
    expect(c.action).toBe("resume");
    expect(c.label).toMatch(/resume/i);
    expect(c.body).toContain("db-restore");
    expect(c.disabled).toBe(false);
  });

  it("carries the reason a previous run stopped", () => {
    const c = migrateCard(
      s({ state_present: true, next_stage: "up", last_error: "MIGRATE_UP_FAILED: exit 1" }),
    );
    expect(c.detail).toContain("MIGRATE_UP_FAILED");
  });

  /**
   * A resume OUTRANKS the payload check, and this is not cosmetic ordering:
   * past `load-images` the tarballs may legitimately be gone, so a
   * payload-first card would tell someone whose migration is nearly finished
   * that their export is incomplete.
   */
  it("prefers resuming over complaining about a payload that has been consumed", () => {
    const c = migrateCard(
      s({ state_present: true, next_stage: "up", export_present: false, missing: ["client-data.tar"] }),
    );
    expect(c.action).toBe("resume");
    expect(c.detail).not.toContain("client-data.tar");
  });

  it("distinguishes 'no export' from 'could not look'", () => {
    // The tri-state rule in UI form. Claiming "no export found" when the probe
    // merely failed sends the user to redo work they already have.
    const cant = migrateCard(null);
    expect(cant.action).toBe("blocked");
    expect(cant.body).toMatch(/couldn't check/i);
    expect(cant.body).not.toMatch(/no complete export/i);

    const none = migrateCard(s({ missing: ["etc/"] }));
    expect(none.body).toMatch(/no complete export/i);
  });

  it("warns that a migration is a snapshot, before the click and not after", () => {
    // The one thing that is not discoverable afterwards: the old server keeps
    // running and keeps diverging, so a week of play on it is a week lost.
    const c = migrateCard(s({ export_present: true }));
    expect(c.detail).toMatch(/snapshot/i);
  });

  it("warns that a non-empty target is refused, before the click", () => {
    const c = migrateCard(s({ export_present: true }));
    expect(c.detail).toMatch(/refuses|characters/i);
  });
});

describe("refusal copy", () => {
  /**
   * The two emptiness refusals must NOT read the same. They need opposite
   * actions: one means "you pointed at somebody's server", the other means "the
   * database did not answer". Collapsing them would send half the users to do
   * the wrong thing — and the engine keeps them as separate codes precisely so
   * the UI can tell them apart.
   */
  it("keeps the two emptiness refusals apart", () => {
    const occupied = migrateErrorHint("MIGRATE_TARGET_NOT_EMPTY");
    const unknown = migrateErrorHint("MIGRATE_TARGET_UNKNOWN");
    expect(occupied).not.toBe(unknown);
    expect(occupied).toMatch(/already has characters/i);
    expect(unknown).toMatch(/didn't answer/i);
    // And the one that wrote nothing must SAY it wrote nothing.
    expect(unknown).toMatch(/nothing was written/i);
  });

  it("points a replace-my-server user at the tool that has a safety copy", () => {
    // There is no --replace, deliberately. The card has to name the thing that
    // does the job rather than leave a dead end.
    expect(migrateErrorHint("MIGRATE_TARGET_NOT_EMPTY")).toMatch(/restore/i);
  });

  it("has copy for every refusal the engine can emit at the user", () => {
    for (const code of [
      "MIGRATE_TARGET_NOT_EMPTY",
      "MIGRATE_TARGET_UNKNOWN",
      "MIGRATE_NO_OVERRIDE",
      "MIGRATE_INCOMPLETE_EXPORT",
      "MIGRATE_COMPOSE_EXISTS",
      "MIGRATE_STACK_CONFLICT",
      "MIGRATE_ENGINE_DOWN",
      "MIGRATE_READY_TIMEOUT",
    ]) {
      expect(migrateErrorHint(code), code).not.toBe("");
    }
  });

  it("says nothing rather than guessing at a code it does not know", () => {
    // Forward-compat: a newer engine's code must not produce invented advice.
    expect(migrateErrorHint("MIGRATE_SOMETHING_NEW")).toBe("");
  });
});
