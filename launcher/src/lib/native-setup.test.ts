import { describe, it, expect, vi, beforeEach } from "vitest";

// Mock the Tauri invoke bridge so the api wrappers are testable without a
// running shell (same pattern as taskbar.test.ts). vi.hoisted keeps the spy
// reachable from the hoisted vi.mock factory.
const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke, Channel: class {} }));

import {
  nativeSetupStatus,
  startDockerDesktop,
  nativeYqInstall,
  nativeSoapCopy,
  nativeDefenderScript,
} from "./api";

describe("native-setup api wrappers", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("nativeSetupStatus invokes the read-only status command and returns it", async () => {
    const payload = {
      native: true,
      docker: { running: false, path: "C:/docker.exe" },
      yq: { present: false, path: "C:/dml-native/tools/yq.exe" },
      soap: { present: true, path: "C:/Users/x/.dml/soap.env", distro_available: true },
    };
    invoke.mockResolvedValue(payload);
    const got = await nativeSetupStatus();
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith("native_setup_status");
    expect(got).toEqual(payload);
  });

  it("startDockerDesktop invokes the launch command", async () => {
    invoke.mockResolvedValue({ launched: true, path: "C:/Docker Desktop.exe" });
    const got = await startDockerDesktop();
    expect(invoke).toHaveBeenCalledWith("start_docker_desktop");
    expect(got.launched).toBe(true);
  });

  it("nativeYqInstall invokes the yq-install command", async () => {
    invoke.mockResolvedValue({ installed: true, path: "C:/yq.exe", bytes: 12345678 });
    const got = await nativeYqInstall();
    expect(invoke).toHaveBeenCalledWith("native_yq_install");
    expect(got.bytes).toBe(12345678);
  });

  it("nativeSoapCopy invokes the soap-copy command", async () => {
    invoke.mockResolvedValue({ copied: true, path: "C:/Users/x/.dml/soap.env" });
    const got = await nativeSoapCopy();
    expect(invoke).toHaveBeenCalledWith("native_soap_copy");
    expect(got.copied).toBe(true);
  });

  it("nativeDefenderScript invokes the script-generator command and returns the path", async () => {
    invoke.mockResolvedValue("C:/Users/x/Downloads/dml-native-defender-exclusions.ps1");
    const got = await nativeDefenderScript();
    expect(invoke).toHaveBeenCalledWith("native_defender_script");
    expect(got).toContain("dml-native-defender-exclusions.ps1");
  });

  it("propagates a rejected invoke (command error surfaces to the caller)", async () => {
    invoke.mockRejectedValue({ code: "BAD_ARG", message: "nope", hint: "" });
    await expect(nativeYqInstall()).rejects.toMatchObject({ code: "BAD_ARG" });
  });
});
