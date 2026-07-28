import { describe, expect, it } from "vitest";
import {
  DOCKER_RESTART_CONFIRM,
  dockerRestartCardVisible,
  dockerRestartConfirmed,
  dockerRestartButtonDisabled,
  dockerRestartNote,
  dockerRestartErrorText,
} from "./docker-restart";

// Pure decision + copy helpers for the Tools "Restart Docker in the distro"
// card. No Svelte, no Tauri -- vitest's default node environment.

describe("dockerRestartCardVisible", () => {
  it("shows the card on the WSL backend (there IS a distro to restart)", () => {
    expect(dockerRestartCardVisible({ native: false })).toBe(true);
  });

  it("hides the card in native mode -- native has no dml-arch distro at all", () => {
    expect(dockerRestartCardVisible({ native: true })).toBe(false);
  });

  it("hides the card until the backend probe answers", () => {
    // Destructive action: an unknown backend must not offer to restart a
    // daemon we can't confirm exists.
    expect(dockerRestartCardVisible(null)).toBe(false);
  });
});

describe("dockerRestartConfirmed", () => {
  it("accepts exactly the confirm phrase", () => {
    expect(dockerRestartConfirmed(DOCKER_RESTART_CONFIRM)).toBe(true);
    expect(DOCKER_RESTART_CONFIRM).toBe("restart-docker");
  });

  it("rejects empty, partial, wrong-case and near-miss input", () => {
    expect(dockerRestartConfirmed("")).toBe(false);
    expect(dockerRestartConfirmed("restart")).toBe(false);
    expect(dockerRestartConfirmed("Restart-Docker")).toBe(false);
    expect(dockerRestartConfirmed("restart-wsl")).toBe(false);
    expect(dockerRestartConfirmed("restart-docker ")).toBe(false);
  });
});

describe("dockerRestartButtonDisabled", () => {
  const armed = { busy: false, input: DOCKER_RESTART_CONFIRM, locked: false };

  it("enables only when unlocked, idle AND the phrase is typed", () => {
    expect(dockerRestartButtonDisabled(armed)).toBe(false);
  });

  it("stays disabled while the feature is locked, even fully armed", () => {
    expect(dockerRestartButtonDisabled({ ...armed, locked: true })).toBe(true);
  });

  it("stays disabled while a restart is already running", () => {
    expect(dockerRestartButtonDisabled({ ...armed, busy: true })).toBe(true);
  });

  it("stays disabled until the confirm phrase matches", () => {
    expect(dockerRestartButtonDisabled({ ...armed, input: "" })).toBe(true);
    expect(dockerRestartButtonDisabled({ ...armed, input: "restart-dock" })).toBe(true);
  });
});

describe("dockerRestartNote", () => {
  it("explains that containers come back by themselves on success", () => {
    const s = dockerRestartNote(true);
    expect(s).toMatch(/container/i);
    expect(s.length).toBeGreaterThan(20);
  });

  it("says so distinctly when the daemon never confirmed it came back", () => {
    const ok = dockerRestartNote(true);
    const not = dockerRestartNote(false);
    expect(not).not.toBe(ok);
    expect(not).toMatch(/didn't|not/i);
  });
});

describe("dockerRestartErrorText", () => {
  const codes = ["NOT_SUPPORTED", "NO_SUDO", "RESTART_FAILED", "RESTART_TIMEOUT", "DOCKER_STILL_DOWN"];

  it("never leaks a raw error code to the user", () => {
    for (const code of codes) {
      const text = dockerRestartErrorText({ code, message: "boom", hint: "" });
      expect(text).not.toContain(code);
      expect(text).not.toContain("_");
    }
  });

  it("gives each failure its own explanation, not one generic string", () => {
    const texts = codes.map((code) => dockerRestartErrorText({ code, message: "boom", hint: "" }));
    expect(new Set(texts).size).toBe(codes.length);
  });

  it("names systemd when the distro has none", () => {
    expect(dockerRestartErrorText({ code: "NOT_SUPPORTED", message: "" })).toMatch(/systemd/i);
  });

  it("names sudo when passwordless sudo is unavailable", () => {
    expect(dockerRestartErrorText({ code: "NO_SUDO", message: "" })).toMatch(/sudo/i);
  });

  it("shows systemctl's own stderr, which the CLI puts in the HINT, for a failed restart", () => {
    // The exact envelope cli/src/90-main.sh:1724 emits: `message` is the
    // generic sentence with systemctl's exit code, `hint` is the stderr tail
    // that says WHY (cli/tests/wow-docker-restart.bats:85 asserts the hint
    // carries "Failed to restart docker.service"). Reading `message` here
    // renders a tautology and drops the only actionable line.
    const text = dockerRestartErrorText({
      code: "RESTART_FAILED",
      message: "Could not restart the Docker daemon (systemctl exit 1)",
      hint: "Failed to restart docker.service: Unit docker.service is masked.",
    });
    expect(text).toContain("Unit docker.service is masked.");
    // ...and does not put the tautology on screen in its place.
    expect(text).not.toContain("systemctl exit 1");
  });

  it("falls back to the message when a failed restart carries no hint", () => {
    // Nothing is lost if the hint is ever empty (or a future backend puts the
    // cause in `message`): the detail slot still gets filled.
    const text = dockerRestartErrorText({
      code: "RESTART_FAILED",
      message: "systemctl was killed before it answered",
      hint: "",
    });
    expect(text).toContain("systemctl was killed before it answered");
  });

  it("still gives the three static codes a route, so none of them needs its hint quoted", () => {
    // The audit behind the hint-first change above: RESTART_FAILED is the only
    // code whose envelope carries a per-run cause. The other three ship static
    // CLI hints ("Open the DML shell (Tools -> DML shell) and run ...", "use
    // Tools -> Restart WSL") that these hand-written sentences already
    // restate, which is why they keep fixed copy. Drop that route from any of
    // them and the hint stops being redundant -- this fires when that happens.
    for (const code of ["NOT_SUPPORTED", "NO_SUDO", "RESTART_TIMEOUT", "DOCKER_STILL_DOWN"]) {
      expect(dockerRestartErrorText({ code, message: "", hint: "" })).toMatch(/Restart WSL|DML shell/);
    }
  });

  it("says the restart ran out of time rather than that it failed", () => {
    // dml-arch's docker.service is Type=notify with TimeoutStartSec=0, so a
    // dockerd wedged during startup makes `systemctl restart docker` wait
    // forever; the CLI kills it at its cap and reports RESTART_TIMEOUT. That
    // is NOT the same story as RESTART_FAILED -- the restart may still be in
    // progress -- so it must not borrow RESTART_FAILED's "failed" copy.
    // Asserted with an EMPTY message/hint on purpose: the fallback branch
    // just echoes those two, so only a real mapped sentence survives here.
    const text = dockerRestartErrorText({ code: "RESTART_TIMEOUT", message: "", hint: "" });
    expect(text).toMatch(/still running|didn't finish|timed out/i);
    expect(text).not.toMatch(/failed/i);
    expect(text.length).toBeGreaterThan(40);
  });

  it("quotes the hint for RESTART_TIMEOUT, because the CLI raises it for two different stalls", () => {
    // The CLI emits RESTART_TIMEOUT both when the systemd PROBE times out
    // (systemd itself is wedged -- waiting cannot help, the fix is
    // wsl --shutdown) and when the restart COMMAND times out (Docker may
    // genuinely still arrive). Only the hint tells them apart, so a static
    // sentence is guaranteed wrong for one of them: it used to tell a user
    // with a wedged systemd to "give it a minute and re-check".
    const wedgedSystemd = dockerRestartErrorText({
      code: "RESTART_TIMEOUT",
      message: "systemd did not answer within 10s",
      hint: "The distro's systemd is not responding. From Windows run: wsl --shutdown, then reopen -- or use Tools -> Restart WSL.",
    });
    expect(wedgedSystemd).toMatch(/wsl --shutdown/);
    expect(wedgedSystemd).not.toMatch(/give it a minute/i);

    const slowDaemon = dockerRestartErrorText({
      code: "RESTART_TIMEOUT",
      message: "Restarting the Docker daemon timed out after 90s",
      hint: "systemd is still waiting for Docker to come up. Give it a minute and re-check from Home; if it stays down, use Tools -> Restart WSL.",
    });
    expect(slowDaemon).toMatch(/still waiting for Docker/);
    // The two stalls must not render as the same sentence.
    expect(slowDaemon).not.toBe(wedgedSystemd);
  });

  it("says the daemon is still down without repeating the code", () => {
    const text = dockerRestartErrorText({ code: "DOCKER_STILL_DOWN", message: "" });
    expect(text).toMatch(/still/i);
  });

  it("falls back to message + hint for an unmapped error", () => {
    expect(dockerRestartErrorText({ code: "IPC", message: "bridge gone", hint: "relaunch" })).toBe(
      "bridge gone — relaunch",
    );
    expect(dockerRestartErrorText({ message: "bridge gone" })).toBe("bridge gone");
  });

  it("survives a non-object rejection", () => {
    expect(dockerRestartErrorText("kaboom")).toBe("kaboom");
  });
});
