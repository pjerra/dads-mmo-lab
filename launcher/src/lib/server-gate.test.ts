import { describe, it, expect } from "vitest";
import { requiresServer, serverGate } from "./server-gate";

// Pages that need the DB/SOAP up show a friendly "start the server" greeting
// instead of erroring or rendering empty. DECIDED 2026-07-21: a greeting with
// a Start button, NOT a greyed-out sidebar.

describe("requiresServer", () => {
  // The decided gate list. Anything added here changes user-visible behaviour
  // on a page, so it should be a deliberate edit, not a drive-by.
  it.each(["console", "dashboard", "teleport", "gmtools", "items", "party", "browse", "accounts"])(
    "gates %s, which is useless while the server is stopped",
    (page) => {
      expect(requiresServer(page)).toBe(true);
    },
  );

  it.each([
    "home", // carries the Start button itself
    "library",
    "tools", // disk tools work offline
    "modmanager", // installs apply on next start
    "settings", // config edits apply on next start
    "botworld",
    "ahbot",
    "accountwide",
    "backups", // Verify works offline
    "commands", // static reference
    "help",
  ])("leaves %s usable while stopped", (page) => {
    expect(requiresServer(page)).toBe(false);
  });

  it("does not gate an unknown page id", () => {
    expect(requiresServer("nonesuch")).toBe(false);
  });
});

describe("serverGate", () => {
  it("does not gate when the world is up", () => {
    expect(serverGate("online", false)).toBeNull();
  });

  it("does not gate before the first poll has landed", () => {
    // A null verdict means "we don't know yet". Gating on it would flash
    // "start the server" on every cold app start.
    expect(serverGate(null, false)).toBeNull();
  });

  it("offers to start a stopped server", () => {
    const g = serverGate("stopped", false);
    expect(g?.kind).toBe("stopped");
    expect(g?.canStart).toBe(true);
  });

  it("offers to start a crashed server, and says it crashed", () => {
    const g = serverGate("crashed", false);
    expect(g?.kind).toBe("crashed");
    expect(g?.canStart).toBe(true);
  });

  it("does not offer Start while the server is already starting", () => {
    const g = serverGate("starting", false);
    expect(g?.kind).toBe("starting");
    expect(g?.canStart).toBe(false);
  });

  it("does not offer Start while a restart is in flight", () => {
    // restarting wins over the polled verdict, which lags a restart.
    const g = serverGate("stopped", true);
    expect(g?.kind).toBe("starting");
    expect(g?.canStart).toBe(false);
  });

  it("reports a running-but-unresponsive server without offering Start", () => {
    // soap_unreachable means the containers are up but SOAP isn't answering;
    // starting it again is not the fix.
    const g = serverGate("soap_unreachable", false);
    expect(g?.kind).toBe("unreachable");
    expect(g?.canStart).toBe(false);
  });

  it("keeps the world up even when a restart flag is set", () => {
    // An online verdict with restarting=true is the tail of a restart; the
    // page is usable, so don't blank it out.
    expect(serverGate("online", true)).toBeNull();
  });

  it("gives every gate a title and a body to render", () => {
    for (const v of ["stopped", "crashed", "starting", "soap_unreachable"] as const) {
      const g = serverGate(v, false);
      expect(g?.title).toBeTruthy();
      expect(g?.body).toBeTruthy();
    }
  });
});
