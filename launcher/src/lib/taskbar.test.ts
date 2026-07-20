import { describe, it, expect, vi, beforeEach } from "vitest";

// Mock the Tauri invoke bridge so the helper is testable without a running
// shell. vi.hoisted keeps the spy reachable from the hoisted vi.mock factory.
const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

describe("taskbar progress cue", () => {
  beforeEach(() => {
    invoke.mockReset();
    invoke.mockResolvedValue(undefined);
    // Fresh module state (the depth counter is module-level).
    vi.resetModules();
  });

  it("turns the cue on for the first op and off when it ends", async () => {
    const { taskbarBusy, taskbarIdle } = await import("./taskbar");
    taskbarBusy();
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenLastCalledWith("set_taskbar_progress", { active: true });
    taskbarIdle();
    expect(invoke).toHaveBeenCalledTimes(2);
    expect(invoke).toHaveBeenLastCalledWith("set_taskbar_progress", { active: false });
  });

  it("stays on until the last overlapping op finishes", async () => {
    const { taskbarBusy, taskbarIdle } = await import("./taskbar");
    taskbarBusy();
    taskbarBusy();
    expect(invoke).toHaveBeenCalledTimes(1); // only the first flips it on
    taskbarIdle();
    expect(invoke).toHaveBeenCalledTimes(1); // one op still outstanding
    taskbarIdle();
    expect(invoke).toHaveBeenCalledTimes(2);
    expect(invoke).toHaveBeenLastCalledWith("set_taskbar_progress", { active: false });
  });

  it("ignores an unbalanced idle", async () => {
    const { taskbarIdle } = await import("./taskbar");
    taskbarIdle();
    expect(invoke).not.toHaveBeenCalled();
  });

  it("swallows an invoke rejection without throwing", async () => {
    invoke.mockRejectedValue(new Error("no tauri here"));
    const { taskbarBusy } = await import("./taskbar");
    expect(() => taskbarBusy()).not.toThrow();
  });
});
