import { describe, expect, it } from "vitest";
import {
  KEEP_AWAKE_FAILURE_LIMIT,
  azerothReadyTransition,
  serverWentDownTransition,
  chipStartVisible,
  containersExist,
  lanRefreshApplied,
  shouldReleaseKeepAwakeOnFailure,
  statusLabel,
} from "./server-status.svelte";
import type { ServerDetail } from "./api";
import { emptyProgress } from "./install-progress.svelte";

function detailWith(containers: ServerDetail["containers"]): ServerDetail {
  return {
    verdict: "stopped",
    exit_code: null,
    containers,
    world_ready: false,
    soap: {
      reachable: false,
      auth_ok: null,
      version: null,
      players: null,
      uptime: null,
      mean_ms: null,
      median_ms: null,
    },
    ports: { world: null, auth: null, soap: null, db: null },
    bots: { online: null, max: null },
  };
}

// Truth table -- restarting wins regardless of verdict (the explicit
// restart-state flag overrides the polled verdict so the bar doesn't flap
// stopped/starting mid-restart).
describe("statusLabel", () => {
  it("shows Restarting (amber) whenever restarting is true, for every verdict", () => {
    expect(statusLabel("online", true, null)).toEqual({ label: "Restarting…", dot: "mid" });
    expect(statusLabel("starting", true, null)).toEqual({ label: "Restarting…", dot: "mid" });
    expect(statusLabel("stopped", true, null)).toEqual({ label: "Restarting…", dot: "mid" });
    expect(statusLabel("soap_unreachable", true, null)).toEqual({ label: "Restarting…", dot: "mid" });
    expect(statusLabel(null, true, null)).toEqual({ label: "Restarting…", dot: "mid" });
  });

  it("maps online to World is up / green, not restarting", () => {
    expect(statusLabel("online", false, null)).toEqual({ label: "World is up", dot: "on" });
  });

  it("maps starting to Starting… / amber, not restarting", () => {
    expect(statusLabel("starting", false, null)).toEqual({ label: "Starting…", dot: "mid" });
  });

  it("maps soap_unreachable to Unreachable / orange-bad, not restarting", () => {
    expect(statusLabel("soap_unreachable", false, null)).toEqual({ label: "Unreachable", dot: "bad" });
  });

  it("maps stopped to Stopped / red-off, not restarting", () => {
    expect(statusLabel("stopped", false, null)).toEqual({ label: "Stopped", dot: "off" });
  });

  /**
   * A stop in flight, reported live on 2026-08-03.
   *
   * Containers on their way down make the poll flap to `soap_unreachable`, and
   * Home renders that as "World is running, but the launcher can't reach it" --
   * an alarm, about a server the user just asked to stop. The same reasoning
   * that already gave `restarting` an override applies here; it was simply
   * never wired for the stop path.
   */
  it("overrides every verdict while a stop is in flight", () => {
    for (const v of ["online", "starting", "soap_unreachable", "crashed", "stopped", null] as const) {
      expect(statusLabel(v, false, null, true)).toEqual({ label: "Stopping…", dot: "mid" });
    }
  });

  it("never renders a FAULT dot while stopping", () => {
    // Home keys its alarming warn/crash card styling on `s.dot`, not on the raw
    // verdict, so this is the property that keeps a stopping server from
    // flashing red. Stated separately from the label because a future edit
    // could keep the wording and lose the styling guarantee.
    for (const v of ["soap_unreachable", "crashed"] as const) {
      expect(statusLabel(v, false, null, true).dot).not.toBe("bad");
      expect(statusLabel(v, false, null, true).dot).not.toBe("crash");
    }
  });

  it("ranks install over restart over stop", () => {
    // A restart passes THROUGH a stop, so if both flags are ever set at once
    // the restart is the truthful one -- "Stopping…" on a server that is about
    // to come back up would send the user to press Start.
    expect(statusLabel("online", true, null, true).label).toBe("Restarting…");
    const inst = { ...emptyProgress(), active: true, stage: "build", pct: 40 };
    expect(statusLabel("online", true, inst, true).label).not.toBe("Restarting…");
  });

  it("defaults `stopping` to false so no call site is silently opted in", () => {
    // The opposite of `install`, which is REQUIRED. A stop is the rare state
    // and defaulting it true would mislabel every ordinary poll.
    expect(statusLabel("online", false, null).label).toBe("World is up");
  });

  it("maps crashed to Server crashed with its own distinct dot kind", () => {
    expect(statusLabel("crashed", false, null)).toEqual({ label: "Server crashed", dot: "crash" });
    // Restarting still wins -- recovering FROM a crash shows the restart.
    expect(statusLabel("crashed", true, null)).toEqual({ label: "Restarting…", dot: "mid" });
  });

  it("falls back to Stopped for an unpolled/null verdict, not restarting", () => {
    expect(statusLabel(null, false, null)).toEqual({ label: "Stopped", dot: "off" });
  });

  // Precedence: installing > restarting > polled verdict.
  it("lets a running install outrank every verdict AND the restart flag", () => {
    // The case this exists for: during a FIRST install there is no stack, so
    // the poll reports "stopped" for the hours the machine spends compiling.
    // Reporting the busiest the PC ever gets as an idle server is the bug.
    const building = { ...emptyProgress(), active: true, stage: "build", pct: 62 };
    for (const verdict of ["stopped", "crashed", "online", "starting", null] as const) {
      expect(statusLabel(verdict, false, building)).toEqual({
        label: "Building… 62%",
        dot: "mid",
      });
      expect(statusLabel(verdict, true, building)).toEqual({
        label: "Building… 62%",
        dot: "mid",
      });
    }
  });

  it("shows the stage without a number before the compile starts counting", () => {
    const cloning = { ...emptyProgress(), active: true, stage: "clone-core" };
    expect(statusLabel("stopped", false, cloning)).toEqual({
      label: "Downloading AzerothCore…",
      dot: "mid",
    });
  });

  it("shows the final wait as elapsed time rather than a percentage", () => {
    const waiting = {
      ...emptyProgress(),
      active: true,
      stage: "ready",
      limitSecs: 1800,
      stageStartedAt: 1_700_000_000_000,
      nowMs: 1_700_000_271_000,
    };
    expect(statusLabel("stopped", false, waiting)).toEqual({
      label: "Waiting for the world… 4:31",
      dot: "mid",
    });
  });

  it("hands the display back the moment the install goes inactive", () => {
    // A store left active would pin the chip to a stage that is not running.
    // An INACTIVE store carrying leftover stage/pct must not linger either.
    const stale = { ...emptyProgress(), active: false, stage: "build", pct: 99 };
    expect(statusLabel("online", false, stale)).toEqual({ label: "World is up", dot: "on" });
  });
});

// Chip quick-start visibility (Batch 2 F8): only startable states, never
// mid-restart, never without data.
describe("chipStartVisible", () => {
  it("shows for stopped and crashed", () => {
    expect(chipStartVisible("stopped", false)).toBe(true);
    expect(chipStartVisible("crashed", false)).toBe(true);
  });

  it("hides for running-ish verdicts", () => {
    expect(chipStartVisible("online", false)).toBe(false);
    expect(chipStartVisible("starting", false)).toBe(false);
    expect(chipStartVisible("soap_unreachable", false)).toBe(false);
  });

  it("hides while restarting, whatever the polled verdict says", () => {
    expect(chipStartVisible("stopped", true)).toBe(false);
    expect(chipStartVisible("crashed", true)).toBe(false);
  });

  it("hides when there is no verdict at all", () => {
    expect(chipStartVisible(null, false)).toBe(false);
  });
});

// "Azeroth is ready" notification trigger (Batch 3 F10): fires only when a
// poll OBSERVES the world becoming online -- never on the first poll after
// app launch, never on a SOAP blip recovering.
describe("azerothReadyTransition", () => {
  it("fires on starting→online (normal boot tail; restarts pass through this too)", () => {
    expect(azerothReadyTransition("starting", "online")).toBe(true);
  });

  it("fires on stopped→online and crashed→online (start completed between polls)", () => {
    expect(azerothReadyTransition("stopped", "online")).toBe(true);
    expect(azerothReadyTransition("crashed", "online")).toBe(true);
  });

  it("does NOT fire on the first poll after app launch (prev null)", () => {
    expect(azerothReadyTransition(null, "online")).toBe(false);
  });

  it("does NOT fire on soap_unreachable→online (blip recovery, not a boot)", () => {
    expect(azerothReadyTransition("soap_unreachable", "online")).toBe(false);
  });

  it("does NOT fire while staying online, or for any non-online next", () => {
    expect(azerothReadyTransition("online", "online")).toBe(false);
    expect(azerothReadyTransition("starting", "starting")).toBe(false);
    expect(azerothReadyTransition("online", "stopped")).toBe(false);
    expect(azerothReadyTransition("starting", "crashed")).toBe(false);
    expect(azerothReadyTransition("stopped", null)).toBe(false);
  });
});

// The other direction (tray work): a server going down while you are not
// looking is exactly what a tray user wants to be told about. Deliberately
// narrow -- only FROM online, only TO a settled down-state.
describe("serverWentDownTransition", () => {
  it("fires when a running server stops or crashes", () => {
    expect(serverWentDownTransition("online", "stopped")).toBe(true);
    expect(serverWentDownTransition("online", "crashed")).toBe(true);
  });

  it("does NOT fire on the first poll after app launch (prev null)", () => {
    // A server that was already down is not news, and notifying on startup
    // would be noise on every launch.
    expect(serverWentDownTransition(null, "stopped")).toBe(false);
    expect(serverWentDownTransition(null, "crashed")).toBe(false);
  });

  it("does NOT fire for a transient SOAP blip or a deliberate restart", () => {
    expect(serverWentDownTransition("online", "soap_unreachable")).toBe(false);
    expect(serverWentDownTransition("online", "starting")).toBe(false);
    expect(serverWentDownTransition("soap_unreachable", "stopped")).toBe(false);
  });

  it("does NOT fire when nothing changed", () => {
    expect(serverWentDownTransition("stopped", "stopped")).toBe(false);
  });
});

// LAN auto-refresh toast gating (Batch 2 F6 review): the toast must follow
// the CLI's own success line, not a "did the IP change" guess -- wow_lan
// surfaces refusals and failures as plain text, never an IPC error.
describe("lanRefreshApplied", () => {
  it("is true only on the CLI's success line", () => {
    expect(lanRefreshApplied("[ok] LAN address refreshed: 192.168.1.5 -> 192.168.1.9\n")).toBe(true);
  });
  it("is false when the CLI refused a public/internet realm address", () => {
    expect(
      lanRefreshApplied("[dml] Realm address 203.0.113.7 is not a LAN address -- leaving it alone.\n"),
    ).toBe(false);
  });
  it("is false on a DB-write failure", () => {
    expect(lanRefreshApplied("[dml] ERROR: Could not update the realm address.\n")).toBe(false);
  });
  it("is false on empty output", () => {
    expect(lanRefreshApplied("")).toBe(false);
  });
});

// Keep-awake release on a stuck poll loop (improvements Batch 2): a failed
// poll skips the online→stopped transition that normally releases the sleep
// block, so it must be released once failures pile up -- but only while the
// block is actually engaged.
describe("shouldReleaseKeepAwakeOnFailure", () => {
  it("does not release below the limit", () => {
    for (let n = 0; n < KEEP_AWAKE_FAILURE_LIMIT; n++) {
      expect(shouldReleaseKeepAwakeOnFailure(n, true)).toBe(false);
    }
  });

  it("releases at and beyond the limit while engaged", () => {
    expect(shouldReleaseKeepAwakeOnFailure(KEEP_AWAKE_FAILURE_LIMIT, true)).toBe(true);
    expect(shouldReleaseKeepAwakeOnFailure(KEEP_AWAKE_FAILURE_LIMIT + 5, true)).toBe(true);
  });

  it("never releases when the block was not engaged (nothing to release)", () => {
    expect(shouldReleaseKeepAwakeOnFailure(KEEP_AWAKE_FAILURE_LIMIT, false)).toBe(false);
    expect(shouldReleaseKeepAwakeOnFailure(KEEP_AWAKE_FAILURE_LIMIT + 99, false)).toBe(false);
  });
});

describe("containersExist", () => {
  it("is false with no detail at all (never polled successfully)", () => {
    expect(containersExist(null)).toBe(false);
  });

  it("is false when every container row is absent (never installed / removed)", () => {
    expect(
      containersExist(
        detailWith([
          { name: "world", role: "world", state: "absent", status: "" },
          { name: "auth", role: "auth", state: "absent", status: "" },
          { name: "db", role: "database", state: "absent", status: "" },
        ]),
      ),
    ).toBe(false);
  });

  it("is true when at least one container row exists but is exited (installed, currently stopped)", () => {
    expect(
      containersExist(
        detailWith([
          { name: "world", role: "world", state: "exited", status: "Exited (0)" },
          { name: "auth", role: "auth", state: "absent", status: "" },
          { name: "db", role: "database", state: "absent", status: "" },
        ]),
      ),
    ).toBe(true);
  });

  it("is true when containers are running", () => {
    expect(
      containersExist(
        detailWith([{ name: "world", role: "world", state: "running", status: "Up 5 minutes" }]),
      ),
    ).toBe(true);
  });
});
