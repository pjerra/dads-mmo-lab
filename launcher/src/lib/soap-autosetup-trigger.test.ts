import { describe, it, expect } from "vitest";
import { shouldTryAutosetup } from "./server-status.svelte";
import type { ServerDetail } from "./api";

function detail(soap: Partial<ServerDetail["soap"]>): ServerDetail {
  return {
    verdict: "online",
    exit_code: null,
    containers: [],
    world_ready: true,
    soap: {
      reachable: true,
      auth_ok: false,
      version: null,
      players: null,
      uptime: null,
      mean_ms: null,
      median_ms: null,
      ...soap,
    },
    ports: { world: null, auth: null, soap: null, db: null },
    bots: { online: null, max: null },
  };
}

describe("shouldTryAutosetup", () => {
  it("fires for a server that answers and refuses us", () => {
    expect(shouldTryAutosetup(detail({}), false, false)).toBe(true);
  });

  it("never fires when SOAP already works", () => {
    expect(shouldTryAutosetup(detail({ auth_ok: true }), false, false)).toBe(false);
  });

  it("never fires for an unreachable SOAP", () => {
    // A world server still booting is not a broken account. Rust would answer
    // not_needed anyway; not asking saves a pointless round-trip on every tick
    // of a stopped server.
    expect(shouldTryAutosetup(detail({ reachable: false }), false, false)).toBe(false);
  });

  it("never fires on an unknown auth state", () => {
    // auth_ok is `boolean | null` and null means "not determined". Treating it
    // as false would create an account on evidence we do not have.
    expect(shouldTryAutosetup(detail({ auth_ok: null }), false, false)).toBe(false);
  });

  it("never fires without a detail at all", () => {
    expect(shouldTryAutosetup(null, false, false)).toBe(false);
  });

  it("stops once the run is settled", () => {
    // Rust latches too, but this stops a pointless IPC call every poll tick
    // for the whole life of the app.
    expect(shouldTryAutosetup(detail({}), true, false)).toBe(false);
  });

  it("does not stack up while one call is in flight", () => {
    expect(shouldTryAutosetup(detail({}), false, true)).toBe(false);
  });
});
