import { describe, it, expect } from "vitest";
import {
  titleInstallGate,
  installUnavailableNotice,
  normalizeCatalog,
  UNSUPPORTED_HOST_HINT,
  MISSING_SCRIPTS_HINT,
  urlInstallGate,
} from "./title-install";
import type { TitleInfo } from "./api";

// The live blocker these pin: on the Windows-native backend the bash CLI runs
// under Git Bash, where the (Linux) installers dir does not exist, so every
// title came back script_available:false and Library told the user to re-run
// cli/dev-install.ps1 -- a step that was already fine and that could never
// make Linux-only installers runnable on Windows.

const row = (over: Partial<TitleInfo> = {}): TitleInfo => ({
  id: "runescape-server",
  name: "RuneScape",
  installed: false,
  running: null,
  script_available: true,
  ...over,
});

describe("titleInstallGate", () => {
  it("offers Install when the host supports installers and the script is there", () => {
    // Fails if the gate ever defaults closed -- that would kill installs on
    // the WSL backend, which is the one place they work.
    const g = titleInstallGate({ installSupported: true, scriptAvailable: true });
    expect(g).toEqual({ block: "none", canInstall: true, hint: "" });
  });

  it("blames the BACKEND, not dev-install, when the host can't run installers", () => {
    // Fails if Library/title-install goes back to one hint for both causes,
    // or if the copy stops naming the WSL backend as the route out.
    const g = titleInstallGate({ installSupported: false, scriptAvailable: false });
    expect(g.canInstall).toBe(false);
    expect(g.block).toBe("unsupported-host");
    expect(g.hint).toBe(UNSUPPORTED_HOST_HINT);
    expect(g.hint).toMatch(/WSL backend/);
    expect(g.hint).not.toMatch(/dev-install/);
  });

  it("host verdict outranks the file check (scripts present but unrunnable)", () => {
    // The ordering IS the fix: checking the file first is exactly how the
    // misdiagnosis happened. Fails if the two branches are ever swapped.
    const g = titleInstallGate({ installSupported: false, scriptAvailable: true });
    expect(g.block).toBe("unsupported-host");
    expect(g.canInstall).toBe(false);
  });

  it("still says 'run dev-install' when that IS the real fix", () => {
    // Fails if the fix over-corrects and drops the genuine dev-install case
    // (a WSL backend whose installer scripts were never shipped).
    const g = titleInstallGate({ installSupported: true, scriptAvailable: false });
    expect(g.block).toBe("missing-scripts");
    expect(g.canInstall).toBe(false);
    expect(g.hint).toBe(MISSING_SCRIPTS_HINT);
    expect(g.hint).toMatch(/dev-install\.ps1/);
  });

  it("gives the two blocked causes DIFFERENT copy", () => {
    // Fails the moment both reasons collapse back into one sentence.
    expect(UNSUPPORTED_HOST_HINT).not.toBe(MISSING_SCRIPTS_HINT);
  });
});

describe("installUnavailableNotice", () => {
  it("is silent while installs work", () => {
    expect(installUnavailableNotice({ installSupported: true, availableCount: 6 })).toBeNull();
  });

  it("is silent when there is nothing to install anyway", () => {
    // Fails if the notice is rendered unconditionally on an unsupported host:
    // an explanation over an empty list is noise, not honesty.
    expect(installUnavailableNotice({ installSupported: false, availableCount: 0 })).toBeNull();
  });

  it("explains the backend and names the way out", () => {
    // Fails if the page-level explanation is dropped (leaving a row of dead
    // buttons to be hovered one by one) or stops naming the Settings route.
    const n = installUnavailableNotice({ installSupported: false, availableCount: 3 });
    expect(n).not.toBeNull();
    expect(n!.title).toMatch(/WSL backend/);
    expect(n!.body).toMatch(/Settings/);
    expect(n!.body).toMatch(/dml-arch/);
    expect(n!.body).not.toMatch(/dev-install/);
  });
});

describe("normalizeCatalog", () => {
  it("carries install_supported:false through", () => {
    const c = normalizeCatalog({ titles: [row()], install_supported: false });
    expect(c.installSupported).toBe(false);
    expect(c.titles).toHaveLength(1);
  });

  it("carries install_supported:true through", () => {
    expect(normalizeCatalog({ titles: [], install_supported: true }).installSupported).toBe(true);
  });

  it("FAILS OPEN when the field is absent (older dml in dml-arch)", () => {
    // A `dml` predating the field omits it, and on that backend installs
    // genuinely work. Fails if absence is ever treated as "unsupported",
    // which would swap one wrong story for another and block real installs.
    expect(normalizeCatalog({ titles: [row()] }).installSupported).toBe(true);
    expect(normalizeCatalog({ titles: [row()], install_supported: undefined }).installSupported).toBe(
      true,
    );
  });

  it("tolerates a payload with no titles array", () => {
    expect(normalizeCatalog({}).titles).toEqual([]);
  });
});

describe("urlInstallGate", () => {
  it("blocks the URL install on a host that cannot run installers", () => {
    // The first cut of this fix explained honestly that installs need the WSL
    // backend, and then left the Install-from-URL card armed right below the
    // notice -- walking the user into the identical failure the notice had
    // just warned about.
    const g = urlInstallGate({ installSupported: false });
    expect(g.canInstall).toBe(false);
    expect(g.block).toBe("unsupported-host");
    expect(g.hint).toBe(UNSUPPORTED_HOST_HINT);
  });

  it("allows it on a supported host, without asking about shipped scripts", () => {
    // Nothing is shipped ahead of a URL install -- the installer comes from the
    // repo being cloned -- so scriptAvailable is not part of this decision.
    expect(urlInstallGate({ installSupported: true })).toEqual({
      block: "none",
      canInstall: true,
      hint: "",
    });
  });
});

describe("hint honesty", () => {
  it("does not order a Linux host to run a PowerShell script", () => {
    // MISSING_SCRIPTS_HINT is reachable on a Linux host too (a Linux dev
    // running against a local dml), where cli/dev-install.ps1 is not the
    // route. Naming it as THE instruction would repeat this bug in miniature.
    expect(MISSING_SCRIPTS_HINT).toMatch(/dev-install step/);
    expect(MISSING_SCRIPTS_HINT).not.toMatch(/^Installer scripts aren't on this backend yet — re-run cli\/dev-install\.ps1/);
  });
});
