import { describe, it, expect } from "vitest";
import { translateUnboundEvent } from "./unbound-run";
import type { TermEvent } from "./api";

const ev = (o: Record<string, unknown>) => o as unknown as TermEvent;

describe("the Unbound done payload reaches the user", () => {
  /**
   * THE REASON THIS FILE EXISTS. The generic translator renders `done` as the
   * single line "Install finished." Reusing it here would swallow the one
   * instruction the engine deliberately does not carry out — spawning the
   * Mentor needs a GM in-game — leaving a rebuilt server with no Mentor and
   * nothing on screen saying why. That is the bash banner's lie, which this
   * whole port exists to remove.
   */
  it("prints the manual step after an install", () => {
    const t = translateUnboundEvent(
      ev({
        event: "done",
        data: {
          addon_version: "1.2.2",
          backup: "wow-20260802-full.sql.gz",
          manual_step: "In game as a GM, stand where the Mentor should appear and run: .npc add 900001",
        },
      }),
      "install",
    );
    expect(t.exit).toBe(0);
    expect(t.text).toContain(".npc add 900001");
    expect(t.text).toContain("wow-20260802-full.sql.gz");
    expect(t.text).toContain("1.2.2");
    // Never the generic wording, which is what dropping the payload looks like.
    expect(t.text).not.toContain("Install finished.");
  });

  it("lists every residue line after an uninstall", () => {
    const residue = [
      "modules/mod-ale left in place (shared Lua engine, harmless without the Mentor script)",
      "characters keep already-learned cross-class spells until their next login",
      "database revert failed: universal skill access rows removed (ERROR 1142)",
    ];
    const t = translateUnboundEvent(
      ev({
        event: "done",
        data: { residue, mentor_stone_cleanup_sql: "DELETE ci FROM character_inventory ci ..." },
      }),
      "uninstall",
    );
    expect(t.exit).toBe(0);
    for (const r of residue) expect(t.text).toContain(r);
    // A FAILED revert must be visible, not just the by-design leftovers.
    expect(t.text).toContain("ERROR 1142");
    // The cleanup SQL is shown as text to run, never executed for them.
    expect(t.text).toContain("character_inventory");
  });

  it("says so when the engine reports no residue at all", () => {
    // The engine always appends the permanent classes, so an empty array means
    // it did not report -- claiming a clean reversal there would invent a fact.
    const t = translateUnboundEvent(ev({ event: "done", data: { residue: [] } }), "uninstall");
    expect(t.text).toContain("no residue list");
    expect(t.text).not.toMatch(/Not reverted/);
  });

  it("does not leak install wording into an uninstall, or vice versa", () => {
    const inst = translateUnboundEvent(ev({ event: "done", data: {} }), "install");
    const unin = translateUnboundEvent(ev({ event: "done", data: {} }), "uninstall");
    expect(inst.text).toContain("installed");
    expect(inst.text).not.toContain("removed");
    expect(unin.text).toContain("removed");
    expect(unin.text).not.toMatch(/\binstalled\b/);
  });

  it("delegates every non-done event to the shared translator", () => {
    // Stages, warnings and failures must render identically to the title
    // install -- two engines showing the same thing two ways is how a user
    // learns to distrust both.
    const sec = translateUnboundEvent(ev({ event: "section_start", name: "build" }), "install");
    expect(sec.text).toContain("build");
    expect(sec.exit).toBeNull();

    const warn = translateUnboundEvent(
      ev({ event: "line", level: "warn", text: "the pinned commit moved" }),
      "install",
    );
    expect(warn.text).toContain("[warn]");

    const err = translateUnboundEvent(
      ev({
        event: "error",
        error: { code: "UNBOUND_CONSENT_REQUIRED", message: "m", hint: "h" },
      }),
      "install",
    );
    expect(err.exit).toBe(1);
    // The hint is frequently the only actionable half and must survive.
    expect(err.text).toContain("h");
  });

  it("ignores an unknown event rather than throwing", () => {
    // Standing forward-compat rule for this union: a future engine event must
    // never crash a running 90-minute rebuild's terminal.
    const t = translateUnboundEvent(ev({ event: "something-new", value: 1 }), "install");
    expect(t.text).toBe("");
    expect(t.exit).toBeNull();
  });

  it("a pct event shows nothing but does not end the run", () => {
    const t = translateUnboundEvent(ev({ event: "pct", value: 62 }), "install");
    expect(t.exit).toBeNull();
  });
});
