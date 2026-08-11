import { describe, expect, it } from "vitest";
import { moduleNav, requestConfFile, requestTuning, takeConfFile, takeTuning } from "./module-nav.svelte";

// One-shot cross-tab navigation targets (Modules-page round, Task 4): a click
// on an installed module's name/conf on the Modules tab sets a pending
// target, the Tuning/Files tab consumes it exactly once via take*() and then
// it reads back null -- mirrors char-store.svelte.ts's charView pattern.
describe("module-nav", () => {
  it("request -> take returns the value once, then null", () => {
    expect(takeTuning()).toBeNull();
    requestTuning("mod-ahbot");
    expect(moduleNav.tuningKey).toBe("mod-ahbot");
    expect(takeTuning()).toBe("mod-ahbot");
    expect(takeTuning()).toBeNull();
  });

  it("confFile request -> take returns the value once, then null", () => {
    expect(takeConfFile()).toBeNull();
    requestConfFile("mod_ahbot.conf");
    expect(moduleNav.confFile).toBe("mod_ahbot.conf");
    expect(takeConfFile()).toBe("mod_ahbot.conf");
    expect(takeConfFile()).toBeNull();
  });

  it("requestTuning does not disturb a pending confFile target", () => {
    requestConfFile("playerbots.conf");
    requestTuning("mod-transmog");
    expect(moduleNav.confFile).toBe("playerbots.conf");
    expect(takeTuning()).toBe("mod-transmog");
    expect(takeConfFile()).toBe("playerbots.conf");
  });

  it("requestConfFile does not disturb a pending tuning target", () => {
    requestTuning("mod-npc-beastmaster");
    requestConfFile("mod_ahbot.conf");
    expect(moduleNav.tuningKey).toBe("mod-npc-beastmaster");
    expect(takeConfFile()).toBe("mod_ahbot.conf");
    expect(takeTuning()).toBe("mod-npc-beastmaster");
  });
});
