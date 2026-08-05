import { describe, it, expect } from "vitest";
import { keepaliveWarning } from "./keepalive-state.svelte";
import type { KeepaliveReport } from "./api";

function report(over: Partial<KeepaliveReport> = {}): KeepaliveReport {
  return {
    applies: true,
    wanted: true,
    holding: true,
    gave_up: false,
    attempts: 0,
    last_error: null,
    last_verdict: null,
    ...over,
  };
}

describe("keepaliveWarning", () => {
  it("says nothing while the holder is doing its job", () => {
    expect(keepaliveWarning(report())).toBeNull();
    expect(keepaliveWarning(report({ wanted: false, holding: false }))).toBeNull();
  });

  it("says nothing on a backend that has no keep-alive", () => {
    // Native/Wsl report applies:false. A warning there would name a mechanism
    // that is not running, and reassurance would be worse -- so both are null,
    // and the caller must not read this as "everything is fine".
    expect(keepaliveWarning(report({ applies: false, gave_up: true, last_error: "x" }))).toBeNull();
  });

  it("names the failure AND its consequence when the holder gave up", () => {
    const msg = keepaliveWarning(
      report({ holding: false, gave_up: true, attempts: 5, last_error: "wsl.exe not found" }),
    );
    // The reason the user can act on...
    expect(msg).toContain("wsl.exe not found");
    // ...and the part that makes it urgent rather than trivia. A message that
    // says only "the keep-alive failed" is the silence this feature removes.
    expect(msg).toContain("15 seconds");
  });

  it("still warns when there is no reason to quote", () => {
    // A give-up whose spawns all SUCCEEDED has no OS error attached. It must
    // not degrade into an empty banner or no banner at all.
    const msg = keepaliveWarning(report({ holding: false, gave_up: true, last_error: null }));
    expect(msg).not.toBeNull();
    expect(msg).toContain("15 seconds");
    expect(msg).not.toContain("undefined");
    expect(msg).not.toContain("null");
  });

  it("names the way out, not just the damage", () => {
    // A give-up latches until the user asks again -- `games_start` calls
    // `server_should_run()`, which is the ONE thing that reopens the budget in
    // `wsl_keepalive.rs`. A banner that reports a dead keep-alive without
    // naming the action that revives it leaves the user with no visible move,
    // which is how the recovery path stayed broken unnoticed.
    for (const r of [
      report({ holding: false, gave_up: true, last_error: "wsl.exe not found" }),
      report({ holding: false, gave_up: true, last_error: null }),
    ]) {
      expect(keepaliveWarning(r)).toContain("Start");
    }
  });

  it("survives a missing report", () => {
    expect(keepaliveWarning(null)).toBeNull();
  });
});
