import { describe, expect, it } from "vitest";
import {
  hasTuningTarget,
  moduleNav,
  requestConfFile,
  requestTuning,
  takeConfFile,
  takeTuning,
} from "./module-nav.svelte";

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

// Round 2, Task 3: a row's "tune" action (and the name-click that mirrors it)
// is only offered when a real Tuning-tab target exists -- a curated card
// (guided knobs) or the plain "Open config file" fallback that card's header
// now carries. Without this gate an installed module with NEITHER (the
// recorded round-1 gap: an uncurated Lua script has no card at all) sends the
// user to the Tuning tab for nothing to expand -- a dead-end click. Pure OR
// so module-row.ts's builder and ModuleManager.svelte's dispatch read the
// same answer.
describe("hasTuningTarget", () => {
  it("true when a curated card exists, even without a conf-file fallback", () => {
    expect(hasTuningTarget(true, false)).toBe(true);
  });

  it("resolves to the open-config fallback -- true when there's no curated card but a conf file exists", () => {
    expect(hasTuningTarget(false, true)).toBe(true);
  });

  it("true when both exist", () => {
    expect(hasTuningTarget(true, true)).toBe(true);
  });

  it("suppressed -- false when neither a curated card nor a conf-file fallback exists", () => {
    expect(hasTuningTarget(false, false)).toBe(false);
  });
});
