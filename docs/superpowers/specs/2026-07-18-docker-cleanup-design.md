# Docker Cleanup — Design Spec + Plan (Round K)

**Date:** 2026-07-18 · **Branch:** `feat/dml-launcher-windows` · Design review waived. Port of the manager's `cleanup_docker` (wow-manage.sh:6688-6769) — reclaims the tens of GB worldserver rebuilds accumulate. New features land LOCKED (K0 rule): flag `docker-clean` + SMOKE-TESTS rows in the same round.

## CLI

**`wow docker-usage --json`** — read-only: `{lines: ["…"]}` = raw `docker system df` output lines (json_ok; DOCKER_DOWN when docker absent).

**`wow docker-clean --level 1|2|3 --json`** — NDJSON streamed. Level closed set (BAD_ARG otherwise); labels: 1 = build cache only (safe), 2 = + build volume (CMake artifacts), 3 = + unused images (maximum recovery).
Sequence (each step streamed as info lines; manager-faithful):
1. `NOT_FOUND` if no server dir; `DOCKER_DOWN` if docker down.
2. Protect the DB volume: if ac-database isn't running, `docker compose up -d ac-database` (tolerated failure, warn).
3. `docker compose stop ac-worldserver` (tolerated).
4. `docker builder prune -af` — stream its summary (the `Total reclaimed` line at minimum).
5. Level ≥2: project = lowercased `basename <server dir>` filtered to `[a-z0-9-]`; build volume = first `docker volume ls --format '{{.Name}}'` match of `^<project>.*(ac.build|build)`; `docker volume rm` it — in-use/missing → warn line, NOT fatal.
6. Level ≥3: `docker image prune -af` — stream summary.
7. `ndjson_done {"level":N,"cleaned":true}` + info line `Next rebuild will be a full recompile (30-90 min).`
All failures after the docker-up check degrade to warn lines (cleanup is best-effort); the verb only hard-fails on validation/docker-down/no-server.

## Plumbing

`wow_docker_usage()` (run_json_cmd) + `wow_docker_clean(level: u8, on_event)` (stream_args, level validated 1-3 CLI-side). api.ts: `wowDockerUsage(): Promise<{lines: string[]}>`, `wowDockerClean(level, onEvent)` (TermEvent stream).

## UI (ModuleManager, bottom card `Disk cleanup`)

- On open/refresh: `wowDockerUsage` → `<pre>` of the lines (muted; error → inline note).
- Level select with the three labels above (default 1) + `Clean` button → two-step confirm copy EXACTLY `Stops the worldserver. The next rebuild after cleaning will be a full 30-90 minute recompile. Continue?` → streams into the page's Terminal (sawDone/streamErr contract); refresh usage after.
- Gated `featureLocked("docker-clean")`; flag registered `"untested"` in features.svelte.ts; SMOKE-TESTS.md §10 gains rows (usage shows; level-1 clean streams + reclaims; rebuild afterwards works).

## Tasks & tests

1. **CLI + bats** (`wow-docker-clean.bats`, ~7): usage happy (stub `system df` lines) + docker-down; clean level validation; level-1 sequence order via call log (db-protect → stop world → builder prune, NO volume/image calls); level-2 volume rm with project-derived grep (fixture volume name) + in-use warn path (stub exit 1 → warn, done still ok); level-3 image prune present; done payload. Extend the docker stub with `builder`, `volume`, `image`, `system` arms (log argv; canned outputs via `DML_STUB_DOCKER_OUT`; exit via `DML_STUB_DOCKER_FAIL_ARM` matching arm name). Expect bats 384+7=391. Commit `feat(cli): docker-usage + docker-clean (3-level, DB-volume protected)`.
2. **Rust + api** (gates cargo 25/vitest 41/check 0-0). Commit `feat(launcher): docker cleanup commands`.
3. **UI + flag + smoke rows** (gates vitest 41/check 0-0). Commit `feat(launcher): disk cleanup card (locked until smoke-tested)`.
