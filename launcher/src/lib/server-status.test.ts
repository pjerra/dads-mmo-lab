import { describe, expect, it } from "vitest";
import { containersExist, statusLabel } from "./server-status.svelte";
import type { ServerDetail } from "./api";

function detailWith(containers: ServerDetail["containers"]): ServerDetail {
  return {
    verdict: "stopped",
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
    expect(statusLabel("online", true)).toEqual({ label: "Restarting…", dot: "mid" });
    expect(statusLabel("starting", true)).toEqual({ label: "Restarting…", dot: "mid" });
    expect(statusLabel("stopped", true)).toEqual({ label: "Restarting…", dot: "mid" });
    expect(statusLabel("soap_unreachable", true)).toEqual({ label: "Restarting…", dot: "mid" });
    expect(statusLabel(null, true)).toEqual({ label: "Restarting…", dot: "mid" });
  });

  it("maps online to World is up / green, not restarting", () => {
    expect(statusLabel("online", false)).toEqual({ label: "World is up", dot: "on" });
  });

  it("maps starting to Starting… / amber, not restarting", () => {
    expect(statusLabel("starting", false)).toEqual({ label: "Starting…", dot: "mid" });
  });

  it("maps soap_unreachable to Unreachable / orange-bad, not restarting", () => {
    expect(statusLabel("soap_unreachable", false)).toEqual({ label: "Unreachable", dot: "bad" });
  });

  it("maps stopped to Stopped / red-off, not restarting", () => {
    expect(statusLabel("stopped", false)).toEqual({ label: "Stopped", dot: "off" });
  });

  it("falls back to Stopped for an unpolled/null verdict, not restarting", () => {
    expect(statusLabel(null, false)).toEqual({ label: "Stopped", dot: "off" });
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
