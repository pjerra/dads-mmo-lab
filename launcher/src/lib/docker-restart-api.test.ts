import { describe, it, expect, vi, beforeEach } from "vitest";

// Mock the Tauri invoke bridge so the api wrapper is testable without a running
// shell (same pattern as native-setup.test.ts / taskbar.test.ts). vi.hoisted
// keeps the spy reachable from the hoisted vi.mock factory.
const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke, Channel: class {} }));

import { wowDockerRestart } from "./api";

describe("wowDockerRestart api wrapper", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("invokes the WSL-only docker-restart command with no arguments", async () => {
    invoke.mockResolvedValue({ restarted: true, waited_seconds: 4 });
    const got = await wowDockerRestart();
    expect(invoke).toHaveBeenCalledTimes(1);
    // The command name is half of the fixed cross-lane contract -- a rename on
    // either side is a silent "command not found" at runtime.
    expect(invoke).toHaveBeenCalledWith("wow_docker_restart");
    expect(got.restarted).toBe(true);
  });

  it("propagates the CLI's error codes so the card can explain each one", async () => {
    // The four contract codes reach the caller UNCHANGED -- the copy layer
    // (docker-restart.ts) switches on `code`, so a wrapper that swallowed or
    // re-wrapped them would collapse every failure into the generic branch.
    for (const code of ["NOT_SUPPORTED", "NO_SUDO", "RESTART_FAILED", "DOCKER_STILL_DOWN"]) {
      invoke.mockRejectedValueOnce({ code, message: "nope", hint: "" });
      await expect(wowDockerRestart()).rejects.toMatchObject({ code });
    }
  });
});
