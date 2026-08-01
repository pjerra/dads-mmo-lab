import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ServerDetail } from "./api";

// WIRING cover for automatic SOAP setup, and the reason it is a separate file
// from soap-autosetup-trigger.test.ts is the whole point of it.
//
// Those 7 tests exercise `shouldTryAutosetup`, a pure predicate. Its only
// reader is `maybeAutosetup`, whose only caller is one line inside
// `refreshServerStatus` -- and refreshServerStatus was exercised by no test at
// all. Delete that single line and every suite in the repo stays green while
// the entire feature is dead: no account is ever created, and because Library's
// SOAP surface is gone and `soapSetupState.needed` is now raised ONLY by the
// gave_up verdict, the manual fallback card is unreachable too. What the user
// gets is a fresh install where GM Tools, My Party, the console and
// announcements all fail on a bare SOAP_AUTH with nothing on screen explaining
// why.
//
// So this drives the REAL poll with the Tauri bridge mocked -- the same shape
// keep-awake.test.ts uses, and for the same reason it exists. No suite in this
// repo mocks `./api`; they all mock the bridge underneath it, which is stronger
// here: it pins the `wow_soap_autosetup` command name too, and a rename on
// either side of that string is a silent "command not found" at runtime.
const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke, Channel: class {} }));

/** SOAP answered and refused us: the one state autosetup exists for. */
function detail(soap: Partial<ServerDetail["soap"]> = {}): ServerDetail {
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

// What the next poll answers, and what autosetup answers. Swapped per test.
let nextPoll: () => Promise<ServerDetail> = () => Promise.resolve(detail());
let nextAutosetup: () => Promise<unknown> = () =>
  Promise.resolve({ status: "created", user: "dmlsoap", reason: null });

function autosetupCalls(): number {
  return invoke.mock.calls.filter(([cmd]) => cmd === "wow_soap_autosetup").length;
}

// maybeAutosetup is fire-and-forget (`void`ed off the poll so a failed setup
// can never break the status loop); let its chain settle before asserting.
async function settle(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
}

describe("automatic SOAP setup is wired into the status poll", () => {
  let refreshServerStatus: () => Promise<void>;
  let soapSetupState: { needed: boolean; autoResult: { user: string } | null; gaveUpReason: string | null };

  beforeEach(async () => {
    // Both stores keep module-level state (the settled/in-flight latches, the
    // banner) -- without a reset, test 2 inherits test 1's concluded run.
    vi.resetModules();
    invoke.mockReset();
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "wow_server_detail" || cmd === "wow_server_detail_read") return nextPoll();
      if (cmd === "wow_soap_autosetup") return nextAutosetup();
      return Promise.resolve(undefined);
    });
    nextPoll = () => Promise.resolve(detail());
    nextAutosetup = () => Promise.resolve({ status: "created", user: "dmlsoap", reason: null });
    refreshServerStatus = (await import("./server-status.svelte")).refreshServerStatus;
    soapSetupState = (await import("./soap-setup-state.svelte")).soapSetupState;
  });

  it("asks the backend to create the account when the poll finds SOAP refusing us", async () => {
    await refreshServerStatus();
    await settle();
    expect(autosetupCalls()).toBe(1);
  });

  it("carries the created account all the way into the banner state", async () => {
    // End to end through the real chain: poll -> shouldTryAutosetup ->
    // wowSoapAutosetup -> applyAutosetupOutcome -> the store the shell renders.
    await refreshServerStatus();
    await settle();
    expect(soapSetupState.autoResult).toEqual({ user: "dmlsoap" });
    expect(soapSetupState.needed).toBe(false);
  });

  it("raises the manual fallback card when the backend gives up", async () => {
    // gave_up is now the ONLY path that raises `needed`, so if this stopped
    // reaching the store the fallback would be unreachable from anywhere.
    nextAutosetup = () =>
      Promise.resolve({ status: "gave_up", user: null, reason: "SOAP rejected the new account" });
    await refreshServerStatus();
    await settle();
    expect(soapSetupState.needed).toBe(true);
    expect(soapSetupState.gaveUpReason).toBe("SOAP rejected the new account");
  });

  it("does not ask on a server whose SOAP already works", async () => {
    // The guard is wired to the real poll, not just unit-tested in isolation:
    // a call site that ignored it would create an account on every launch.
    nextPoll = () => Promise.resolve(detail({ auth_ok: true }));
    await refreshServerStatus();
    await settle();
    expect(autosetupCalls()).toBe(0);
  });

  it("asks once, not once per poll tick", async () => {
    // The poll runs every few seconds for the life of the app; a settled run
    // must stop the IPC, not just stop the account creation.
    for (let i = 0; i < 3; i++) {
      await refreshServerStatus();
      await settle();
    }
    expect(autosetupCalls()).toBe(1);
  });

  it("keeps the poll healthy when autosetup itself fails", async () => {
    // Best-effort by design. A rejected setup must leave the status detail and
    // lastError exactly as a clean poll would -- the sidebar chip and Home must
    // not report a broken server because an account could not be created.
    nextAutosetup = () => Promise.reject(new Error("IPC exploded"));
    const store = await import("./server-status.svelte");
    await refreshServerStatus();
    await settle();
    expect(store.serverStatus.detail?.verdict).toBe("online");
    expect(store.serverStatus.lastError).toBeNull();
    // Unsettled, so the next poll tries again.
    await refreshServerStatus();
    await settle();
    expect(autosetupCalls()).toBe(2);
  });
});
