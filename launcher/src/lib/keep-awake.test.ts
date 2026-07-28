import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ServerDetail } from "./api";

// Wiring-level cover for keep-awake engagement. The pure truth table in
// transitions.test.ts is not enough here: BOTH keep-awake bugs shipped so far
// (the 2min watchdog starvation, and the release paths below) were wiring
// bugs that a green pure-helper suite could not see. These tests drive the
// real refreshServerStatus loop with the Tauri bridge mocked, exactly like
// taskbar.test.ts does.
const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

const ONLINE: ServerDetail = {
  verdict: "online",
  exit_code: null,
  containers: [{ name: "world", role: "world", state: "running", status: "Up 5 minutes" }],
  world_ready: true,
  soap: {
    reachable: true,
    auth_ok: true,
    version: "AzerothCore",
    players: 1,
    uptime: "1h",
    mean_ms: 3,
    median_ms: 3,
  },
  ports: { world: null, auth: null, soap: null, db: null },
  bots: { online: 0, max: 40 },
};

// What the next server-detail poll does. Swapped per phase of a test.
let nextPoll: () => Promise<ServerDetail> = () => Promise.resolve(ONLINE);

// The `on` argument of every set_keep_awake call, in order.
function keepAwakeCalls(): boolean[] {
  return invoke.mock.calls
    .filter(([cmd]) => cmd === "set_keep_awake")
    .map(([, args]) => (args as { on: boolean }).on);
}

// The store's side effects are fire-and-forget `.then()` chains; let them
// settle before asserting.
async function settle(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
}

describe("keep-awake engagement across the poll loop", () => {
  let refreshServerStatus: () => Promise<void>;
  let serverStatus: { keepAwakeActive: boolean };

  beforeEach(async () => {
    // Module-level state (the poll store, the backend-mode memo, the testing
    // -mode rune) must be fresh per test.
    vi.resetModules();
    invoke.mockReset();
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "wow_server_detail" || cmd === "wow_server_detail_read") return nextPoll();
      return Promise.resolve(undefined);
    });
    nextPoll = () => Promise.resolve(ONLINE);
    const store = await import("./server-status.svelte");
    refreshServerStatus = store.refreshServerStatus;
    serverStatus = store.serverStatus;
    // keep-awake is an untested-flagged feature: unlock it, or every
    // engagement below is correctly suppressed and the tests prove nothing.
    const { setTestingMode } = await import("./features.svelte");
    setTestingMode(true);
  });

  it("re-engages after the failure-limit release when polls recover with the server still online", async () => {
    // The residual from the tray round: the 3-failed-poll release drops the
    // sleep block, but recovery is online→online -- no transition -- so a
    // transition-only engage never fires again and the PC sleeps mid-session.
    await refreshServerStatus();
    await settle();
    expect(keepAwakeCalls()).toEqual([true]);

    nextPoll = () => Promise.reject(new Error("backend down"));
    for (let i = 0; i < 3; i++) await refreshServerStatus();
    await settle();
    expect(keepAwakeCalls()).toEqual([true, false]);
    expect(serverStatus.keepAwakeActive).toBe(false);

    nextPoll = () => Promise.resolve(ONLINE);
    await refreshServerStatus();
    await settle();
    expect(keepAwakeCalls()).toEqual([true, false, true]);
    expect(serverStatus.keepAwakeActive).toBe(true);
  });

  it("re-asserts the block on every online poll, so the Rust watchdog's release cannot stick", async () => {
    // The watchdog (lib.rs) releases the block after 120s with no status
    // push and tells the frontend NOTHING -- keepAwakeActive stays true. So
    // engagement cannot be guarded on our own belief about it: it has to be
    // asserted every poll, the same doctrine as the tray status heartbeat.
    await refreshServerStatus();
    await settle();
    await refreshServerStatus();
    await settle();
    expect(keepAwakeCalls()).toEqual([true, true]);
  });

  it("never engages while the feature is locked or the toggle is off", async () => {
    const { setTestingMode } = await import("./features.svelte");
    setTestingMode(false); // keep-awake is flagged untested -> locked
    await refreshServerStatus();
    await settle();
    await refreshServerStatus();
    await settle();
    expect(keepAwakeCalls()).toEqual([]);
    expect(serverStatus.keepAwakeActive).toBe(false);
  });

  it("does not fight the user turning the Tools toggle off mid-session", async () => {
    // The per-poll assert reads the same pref the toggle writes, so an
    // explicit "off" stays off instead of being re-engaged 7s later.
    await refreshServerStatus();
    await settle();
    const { setKeepAwakePref } = await import("./tool-prefs.svelte");
    setKeepAwakePref(false);
    await refreshServerStatus();
    await settle();
    expect(keepAwakeCalls()).toEqual([true]);
  });

  it("still releases on a real online→stopped transition", async () => {
    await refreshServerStatus();
    await settle();
    nextPoll = () => Promise.resolve({ ...ONLINE, verdict: "stopped", world_ready: false });
    await refreshServerStatus();
    await settle();
    expect(keepAwakeCalls()).toEqual([true, false]);
    expect(serverStatus.keepAwakeActive).toBe(false);
  });
});
