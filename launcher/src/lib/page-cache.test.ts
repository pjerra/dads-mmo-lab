import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  createCachedStore,
  formatLoadError,
  pickConfigReader,
} from "./page-cache.svelte";

// Pure error formatting -- the message + optional " — hint" every page shows.
describe("formatLoadError", () => {
  it("uses message alone when there is no hint", () => {
    expect(formatLoadError({ message: "boom" })).toBe("boom");
  });
  it("appends the hint after an em-dash", () => {
    expect(formatLoadError({ message: "boom", hint: "try again" })).toBe("boom — try again");
  });
  it("stringifies a non-DmlErr throw", () => {
    expect(formatLoadError("plain string")).toBe("plain string");
    expect(formatLoadError(new Error("oops"))).toBe("oops");
  });
  it("ignores an empty hint", () => {
    expect(formatLoadError({ message: "boom", hint: "" })).toBe("boom");
  });
});

// Native-vs-wsl reader routing (shared by config, module list, module tuning).
describe("pickConfigReader", () => {
  it("routes native to the fast Rust read", () => {
    expect(pickConfigReader("native")).toBe("read");
  });
  it("routes wsl to the CLI list", () => {
    expect(pickConfigReader("wsl")).toBe("list");
  });
});

// End-to-end routing of the module list + module tuning caches: native mode
// must source from the in-process Rust readers (wowModuleRead / wowTuningRead)
// and WSL mode from the CLI commands (wowModuleList / wowConfigTuningList).
// The api layer is mocked; each mode gets a fresh module registry so the
// resolveBackendMode memo can't leak the first mode into the second.
describe("cache native-vs-wsl routing", () => {
  const spies = {
    backendMode: vi.fn(),
    wowConfigList: vi.fn(async () => []),
    wowConfigRead: vi.fn(async () => []),
    wowConfigFiles: vi.fn(async () => []),
    wowModuleList: vi.fn(async () => "module-list-wsl"),
    wowModuleRead: vi.fn(async () => "module-list-native"),
    wowConfigTuningList: vi.fn(async () => "tuning-wsl"),
    wowTuningRead: vi.fn(async () => "tuning-native"),
  };

  beforeEach(() => {
    vi.resetModules();
    for (const s of Object.values(spies)) s.mockClear();
    vi.doMock("./api", () => spies);
  });

  afterEach(() => {
    vi.doUnmock("./api");
  });

  async function loadFresh(mode: "native" | "wsl") {
    spies.backendMode.mockResolvedValue(mode);
    // Fresh import -> fresh module-scope memo + the mocked api above.
    const pc = await import("./page-cache.svelte");
    await pc.moduleListCache.refresh();
    await pc.moduleTuningCache.refresh();
    return pc;
  }

  it("native sources both caches from the Rust readers", async () => {
    const pc = await loadFresh("native");
    expect(spies.wowModuleRead).toHaveBeenCalledTimes(1);
    expect(spies.wowModuleList).not.toHaveBeenCalled();
    expect(spies.wowTuningRead).toHaveBeenCalledTimes(1);
    expect(spies.wowConfigTuningList).not.toHaveBeenCalled();
    expect(pc.moduleListCache.store.data).toBe("module-list-native");
    expect(pc.moduleTuningCache.store.data).toBe("tuning-native");
  });

  it("wsl sources both caches from the CLI commands", async () => {
    const pc = await loadFresh("wsl");
    expect(spies.wowModuleList).toHaveBeenCalledTimes(1);
    expect(spies.wowModuleRead).not.toHaveBeenCalled();
    expect(spies.wowConfigTuningList).toHaveBeenCalledTimes(1);
    expect(spies.wowTuningRead).not.toHaveBeenCalled();
    expect(pc.moduleListCache.store.data).toBe("module-list-wsl");
    expect(pc.moduleTuningCache.store.data).toBe("tuning-wsl");
  });
});

// Deferred promise helper for driving single-flight timing.
function deferred<T>() {
  let resolve!: (v: T) => void;
  let reject!: (e: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

describe("createCachedStore", () => {
  it("starts empty (no data, not loaded, no error)", () => {
    const { store } = createCachedStore(async () => 1);
    expect(store.data).toBeNull();
    expect(store.loaded).toBe(false);
    expect(store.loading).toBe(false);
    expect(store.error).toBeNull();
  });

  it("fills data and flips loaded on a successful refresh", async () => {
    const { store, refresh } = createCachedStore(async () => "value");
    await refresh();
    expect(store.data).toBe("value");
    expect(store.loaded).toBe(true);
    expect(store.loading).toBe(false);
    expect(store.error).toBeNull();
  });

  it("is single-flight: a second refresh while one is in flight is a no-op", async () => {
    let calls = 0;
    const d = deferred<string>();
    const { store, refresh } = createCachedStore(() => {
      calls++;
      return calls === 1 ? d.promise : Promise.resolve("second");
    });
    const r1 = refresh();
    // store.loading is now true -> this call collapses into the first.
    const r2 = refresh();
    d.resolve("first");
    await Promise.all([r1, r2]);
    expect(calls).toBe(1);
    expect(store.data).toBe("first");
  });

  it("keeps the last-known data when a later refresh fails, surfacing the error", async () => {
    let mode: "ok" | "fail" = "ok";
    const { store, refresh } = createCachedStore(async () => {
      if (mode === "fail") throw { message: "down", hint: "retry" };
      return "good";
    });
    await refresh();
    expect(store.data).toBe("good");

    mode = "fail";
    await refresh();
    // Stale-but-good data survives; only the error is new.
    expect(store.data).toBe("good");
    expect(store.loaded).toBe(true);
    expect(store.error).toBe("down — retry");
  });

  it("clears the error again on the next successful refresh", async () => {
    let mode: "ok" | "fail" = "fail";
    const { store, refresh } = createCachedStore(async () => {
      if (mode === "fail") throw new Error("x");
      return "y";
    });
    await refresh();
    expect(store.error).toBe("x");
    mode = "ok";
    await refresh();
    expect(store.error).toBeNull();
    expect(store.data).toBe("y");
  });

  it("invalidate() drops the cache back to empty", async () => {
    const { store, refresh, invalidate } = createCachedStore(async () => 42);
    await refresh();
    expect(store.data).toBe(42);
    expect(store.loaded).toBe(true);
    invalidate();
    expect(store.data).toBeNull();
    expect(store.loaded).toBe(false);
    expect(store.error).toBeNull();
  });
});
