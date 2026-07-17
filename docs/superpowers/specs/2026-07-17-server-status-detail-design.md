# Server Status & Health Panel — Design Spec

**Date:** 2026-07-17
**Branch:** `feat/dml-launcher-windows`
**Status:** Approved design (user: "looks good"), Round A of the modules/console/titles phase.

## Problem

Home (and Dashboard) decide "World is up / World is down" from `dml wow server-info` alone,
which probes SOAP. SOAP is unreachable during the ~2-minute boot window (worldserver spawns
~1900 bots) and whenever dml-arch's Docker port-forwarding breaks (recurring iptables issue,
hit twice on 2026-07-16). In both cases the card says **"World is down"** while the container
is demonstrably running — the status card and the Start/Stop button contradict each other.

The user also wants to click the server and see real information about it.

## Goals

1. Status derived from the **container** first, SOAP second — four honest states.
2. A clickable server card on Home that expands into a **full health panel**.
3. One new CLI command carrying all of it, testable in bats like everything else.

Out of scope (later rounds / not wanted): per-container start/stop buttons, server console
(Round B), auto-polling (manual Refresh stays, consistent with every page), module/title
management (Rounds C/D).

## The four states

| Verdict | Condition | Home card |
|---|---|---|
| `stopped` | ac-worldserver container not running | grey dot, "Server is stopped", "Start the server below." |
| `starting` | container running, SOAP not answering, no boot-complete marker yet | amber dot, "Starting up…", "The world is still loading — this takes a couple of minutes while bots spawn." |
| `online` | SOAP answers (including HTTP 401 — an auth failure still proves the world is up) | green dot, "World is up" + players / uptime / update-time stats |
| `soap_unreachable` | container running, boot-complete marker present, SOAP still not answering | red dot, "World is running, but the launcher can't reach it", hint: "If this persists for more than a minute, Docker's networking in the distro is likely stuck — restarting Docker inside dml-arch usually fixes it." |

The boot-complete marker is AzerothCore's `World Initialized In <N> Minutes <N> Seconds`
log line (verified live on this build: `WORLD: World Initialized In 0 Minutes 14 Seconds`;
emitted from `World.cpp` `METRIC_EVENT("events", "World initialized", ...)`). Bot logins
continue after the marker and SOAP begins listening shortly after it, so there is a brief
legitimate marker-but-no-SOAP window — the `soap_unreachable` copy says "if this persists"
rather than claiming certainty.

**Stale-marker guard:** `games stop` is `compose down` (fresh logs on every start), but
`wow backup restore` and `wow restart` use `compose stop`/`start`, which preserve container
logs — a marker from the *previous* run would make a rebooting world read as
`soap_unreachable` instead of `starting`. The marker grep therefore only looks at log lines
since the container's current start: `docker logs --since "$(docker inspect -f
'{{.State.StartedAt}}' ac-worldserver)"`.

## CLI: `dml wow server-detail --json`

One request-response envelope (`json_ok`), no NDJSON, read-only (docker inspection + one
SOAP `server info` — no MySQL, no writes). Lives in `cli/src/40-config.sh` helpers +
a `server-detail` arm in `90-main.sh` next to `server-info`.

```json
{
  "ok": true,
  "data": {
    "verdict": "starting",
    "containers": [
      { "name": "ac-worldserver", "role": "world",    "state": "running", "status": "Up 33 seconds" },
      { "name": "ac-authserver",  "role": "auth",     "state": "running", "status": "Up 33 seconds" },
      { "name": "ac-database",    "role": "database", "state": "running", "status": "Up 41 seconds (healthy)" }
    ],
    "world_ready": false,
    "soap": { "reachable": false, "auth_ok": null, "version": null, "players": null,
              "uptime": null, "mean_ms": null, "median_ms": null },
    "ports": { "world": "8085", "auth": "3724", "soap": "7878", "db": "3306" }
  }
}
```

- **containers**: exactly the three long-running services, in that fixed order. `state` is
  `running` | `exited` | `absent` (absent = container doesn't exist, e.g. after
  `compose down`); `status` is Docker's human status text (`Up 2 hours (healthy)`), `""`
  when absent. Source: one `docker ps -a --format '{{.Names}}|{{.State}}|{{.Status}}'`
  call, matched by name. The one-shot containers (ac-db-import, ac-client-data-init) are
  deliberately excluded — "Exited (0)" is their healthy state and would only alarm.
- **world_ready**: the `--since`-guarded marker grep (case-insensitive, `grep -m1` on
  `World Initialized In`). Only computed when ac-worldserver is running; `false` otherwise.
- **soap**: reuses `soap_exec 'server info'` + `_parse_server_info`. rc 0 →
  `reachable:true, auth_ok:true` + parsed stats; rc 3 (401) → `reachable:true,
  auth_ok:false`, stats null; rc 2/4 → `reachable:false, auth_ok:null`, stats null.
- **ports**: host ports from `docker port <name> <internal>` (world 8085, auth 3724,
  soap 7878 on ac-worldserver; db 3306 on ac-database); each `null` when unavailable
  (container absent/stopped or unpublished).
- **verdict** (derived in the CLI so the logic is bats-tested once, not duplicated per page):
  world container not `running` → `stopped`; else SOAP `reachable` → `online`; else
  `world_ready` → `soap_unreachable`; else `starting`.
- The existing `server-info` verb is untouched (public CLI surface, tested).
- Errors: only genuine environment failures error (`NO_WOW` when the title dir is missing,
  matching existing wow verbs). Docker down → all containers `absent`, verdict `stopped`
  (down is an answer, not an error — same philosophy as `server-info`).

## Launcher UI

**Home** switches from `wowServerInfo()` to the new `wowServerDetail()` and renders the
four-state card from `verdict` (new `.dot.mid` amber + reuse of existing on/off colors;
`soap_unreachable` uses the error-red border treatment on the card, not a blocking error).
Stats row (players / uptime / update time) renders only when `online` and stats are non-null.

The **WoW server card** (the one with Start/Stop) becomes expandable: clicking the card
title area (a proper `<button>` for a11y, chevron indicator) toggles an inline **health
panel** below it:

- three container rows — dot (green running / grey exited / grey absent) + role label
  ("World server" / "Auth server" / "Database") + Docker's status text;
- when online: version, uptime, players online, world update time (mean/median ms);
- ports row: game 8085 · auth 3724 · SOAP 7878 · DB 3306 (from `ports`, "?" when null);
- SOAP row: "reachable" / "unreachable" (+ "authentication failing — check ~/.dml/soap.env"
  when `auth_ok === false`).

The panel populates from the same `wowServerDetail()` result already fetched by
`refresh()` — expanding does not fire a new request; Refresh refreshes it. Start/Stop
behavior, streaming terminal, and `gamesStatus`-driven button logic are unchanged.

**Dashboard** switches its world card to the same `wowServerDetail()` verdict + copy
(fixing the identical lie there); its character viewer is untouched.

**Plumbing:** `wow_server_detail` Rust command (request-response via `run_json_cmd`,
mirroring `wow_server_info`) + `ServerDetail` types and `wowServerDetail()` wrapper in
`api.ts`. `wowServerInfo` stays (its CLI verb remains public; Dashboard/Home simply stop
calling it).

## Testing

- **bats** (new `wow-server-detail.bats`): verdict matrix — world absent → `stopped`;
  world running + SOAP ok → `online` with stats; world running + curl dead + no marker →
  `starting`; marker present + curl dead → `soap_unreachable`; 401 → `online` with
  `auth_ok:false`; stale-marker case — logs contain the marker but only before
  `StartedAt` cutoff → `starting` (pins the `--since` guard); ports null when container
  absent; docker completely down → `stopped`, exit 0. Docker stub grows `ps`/`logs`/
  `inspect`/`port` arms (env-var seams like the existing compose/exec arms).
- **Gates:** full bats suite, vitest, `cargo test`, `svelte-check` — all existing
  baselines stay green (bats 234 + new, vitest 19, cargo 17, svelte-check 0/0).
- **Live gate (batched with the roadmap gate):** watch Home go stopped → starting →
  online across a real boot; expand the health panel and check the three containers,
  ports, and stats; next time Docker's forwarding breaks, confirm the `soap_unreachable`
  diagnostic appears.
