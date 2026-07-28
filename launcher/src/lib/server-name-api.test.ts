import { describe, it, expect, vi, beforeEach } from "vitest";

// Mock the Tauri invoke bridge so the api wrappers are testable without a
// running shell (same pattern as docker-restart-api.test.ts).
const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke, Channel: class {} }));

import { gamesName, activeGameGet, activeGameSet, traySetServers, gamesList } from "./api";

describe("server rename / active server api wrappers", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  // The command names and argument KEYS are half of a fixed cross-lane
  // contract -- a rename or a mis-cased key is a silent runtime failure, not a
  // compile error.
  it("gamesName sends the name to set", async () => {
    invoke.mockResolvedValue({ id: "wow-server-playerbots", name: "Dad's Server" });
    const got = await gamesName("wow-server-playerbots", "Dad's Server");
    expect(invoke).toHaveBeenCalledWith("games_name", {
      id: "wow-server-playerbots",
      name: "Dad's Server",
    });
    expect(got.name).toBe("Dad's Server");
  });

  it("gamesName sends null to clear the name", async () => {
    // null must reach Rust as null (the --clear arm), NOT as an omitted key or
    // an empty string -- an empty string is a validation error on the CLI side.
    invoke.mockResolvedValue({ id: "wow-server-playerbots", name: null });
    const got = await gamesName("wow-server-playerbots", null);
    expect(invoke).toHaveBeenCalledWith("games_name", { id: "wow-server-playerbots", name: null });
    expect(got.name).toBeNull();
  });

  it("activeGameGet takes no arguments and passes null straight through", async () => {
    invoke.mockResolvedValue(null);
    expect(await activeGameGet()).toBeNull();
    expect(invoke).toHaveBeenCalledWith("active_game_get");
    invoke.mockResolvedValue("maplestory-server");
    expect(await activeGameGet()).toBe("maplestory-server");
  });

  it("activeGameSet sends the id and propagates a refusal", async () => {
    invoke.mockResolvedValue(undefined);
    await activeGameSet("maplestory-server");
    expect(invoke).toHaveBeenCalledWith("active_game_set", { id: "maplestory-server" });
    // Rust refuses an id that is not installed rather than persisting it; the
    // caller must be able to see WHY.
    invoke.mockRejectedValueOnce({ code: "NOT_FOUND", message: "nope", hint: "" });
    await expect(activeGameSet("gone-server")).rejects.toMatchObject({ code: "NOT_FOUND" });
  });

  it("traySetServers sends the rows under the `servers` key, snake_case fields intact", async () => {
    invoke.mockResolvedValue(undefined);
    const rows = [{ id: "wow-server-playerbots", display_name: "Dad's Server", running: true }];
    await traySetServers(rows);
    // Rust deserializes TrayServer with plain serde (no rename_all), so the
    // field name is display_name on BOTH sides -- camelCasing it here would
    // fail deserialization at runtime with nothing to see in the UI.
    expect(invoke).toHaveBeenCalledWith("tray_set_servers", { servers: rows });
  });

  it("gamesList never yields a blank label, even from a dml that predates display_name", async () => {
    // Fail-open, same doctrine as normalizeCatalog's install_supported: an
    // older `dml` still installed in dml-arch omits the key entirely, and a
    // server rendering as an empty string is worse than one showing its id.
    invoke.mockResolvedValue({
      games: [
        { id: "wow-server-playerbots", path: "/games/wow", running: false },
        { id: "maplestory-server", path: "/games/maple", running: false, display_name: "" },
        { id: "runescape-server", path: "/games/rs", running: true, display_name: "Kids' RS" },
      ],
    });
    const got = await gamesList();
    expect(got.map((g) => g.display_name)).toEqual([
      "wow-server-playerbots",
      "maplestory-server",
      "Kids' RS",
    ]);
  });
});
