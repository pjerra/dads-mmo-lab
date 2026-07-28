# Spec — Server Performance Advisor (brainstormed 2026-07-27)

**Status:** APPROVED in brainstorm 2026-07-27; NOT built. Committed here 2026-07-28 — it had been sitting in the untracked `.superpowers/sdd/`, which is gitignored, so it appeared in no plan, no roadmap and no ledger, and was invisible to every later audit. That is why nothing happened to it for a day: the parking spot was the bug.

**Freshness warning:** the code references below were written against the tree of 2026-07-27, BEFORE the round-2 launcher merge, the incident follow-ups and the Rust workspace changes that followed. Re-verify each claim (notably "the native parser currently drops the percentiles") before building on it.

**Goal:** continuously measure world-tick latency (p50/p95/p99) + resource telemetry, classify *why* the server is struggling, and recommend the exact setting to change — naming the file, key, value, and the ORDER across layers. **Advisory-only in v1 (read-only ⇒ ships unlocked, no smoke gate).** Auto-apply is a later phase bolted onto the same engine.

## Decisions taken (user)
- Reach: **advisory only for now**, maybe write later.
- Sampling: **background + saved history**.
- Placement: **Statistics → "Performance" section + a Home badge only when degraded/critical**.
- Include **disk I/O** and **per-map** tracking (see the per-map tiering below).

## 1. Sampler
Every **30s** while the world is up (piggybacks the existing status poll cadence; skip when stopped).

| Source | Fields |
|---|---|
| SOAP `server info` | `diff_ms`, `mean_ms`, `median_ms`, **`p95_ms`, `p99_ms`, `max_ms`** (already in the text; the native parser currently drops the percentiles — capture them), connected players |
| `docker stats --no-stream` | per container (`ac-worldserver`, `ac-database`, `ac-authserver`): `cpu_pct`, `mem_used`, `mem_limit`, **`blk_read`/`blk_write`**, `net_io`, `pids` |
| Characters DB | total online, bots online, real players; **per-map population** `SELECT map, COUNT(*) FROM characters WHERE online=1 GROUP BY map` |
| Config snapshot (cached, re-read on change) | `MapUpdate.Threads`, `Network.Threads`, `MaxRandomBots`, WSL `processors`/`memory`, docker compose limits, host core count |
| Filesystem | free disk space on the games/backups volume |

**NB:** docker's `cpu_pct` is *per-core* percent (100% = one core). Saturation = `100 × allocated_cores`. The engine MUST use allocated cores, not 100, as the ceiling — this is what distinguishes CPU-bound from single-thread-bound.

## 2. Storage
`~/.dml/metrics/YYYY-MM-DD.jsonl` — append-only, one file per day, **30-day retention** (prune = delete whole files on startup). ~1–3 MB/month. Charts downsample on read (1-min buckets ≤24h, 1-hour buckets for 7/30d).

## 3. Verdict engine (pure, unit-tested; no I/O)
Input: a sample window + config snapshot. Output: `{ state, confidence, evidence[], recommendations[] }`.

**Bands (p99 diff):** `<100ms` healthy · `100–200` watch · `200–500` degraded · `>500` critical.

**Classification:**
| Signal pattern | Diagnosis | Recommendation |
|---|---|---|
| CPU ≈ 100×allocated_cores | CPU-bound | Raise the capping layer first, then threads |
| mem/limit >85%, or swapping | RAM-bound | WSL memory ↑ / MySQL buffer pool ↑ |
| worldserver CPU low, `ac-database` CPU/IO high | DB-bound | MySQL tuning — more cores won't help |
| CPU pinned ≈100% (ONE core) while others idle **and** `MapUpdate.Threads=1` | **Single-thread-bound** | Threads ↑ (cores alone do nothing) |
| high `blk_*` growth + high diff, CPU fine | I/O-bound | Disk/host issue, Defender exclusion, move volume |
| mean ≫ median (ratio >4), resources fine | Periodic stalls (saveall / bot randomize) | Interval tuning, NOT hardware |
| p99 healthy + large headroom | Over-provisioned | "You could run ~N more bots" |

**Layer-chain guard (load-bearing):** validate every recommendation against `host ≥ WSL ≥ docker limit ≥ MapUpdate.Threads`. Never emit a change a lower layer would nullify; instead emit the ordered chain (raise WSL first, *then* threads).

## 4. Per-map — tiering (timing is NOT exposed by this build; verified 2026-07-27)
Checked all paths: SOAP has no per-map command; `Logger.diff=3` (already enabled) logs only whole-server diffs with no map id; the only built-in per-map timing is `Metric.Enable` → **InfluxDB**.
- **Tier 1 (build now):** per-map **population** time-series (free DB query above).
- **Tier 3 (build now):** **correlation** — flag "p99 rose as map 1 crossed 300 online".
- **Tier 2 (later, opt-in):** true per-map timing via `Metric.Enable=1` + an InfluxDB sidecar. Requires an extra container + config writes ⇒ conflicts with advisory-only; ship as an explicit "advanced diagnostics" toggle.

## 5. UI
**Statistics → "Performance":** verdict banner · p50/p95/p99 chart (1h/24h/7d/30d) · players+bots overlay (**= feature #4, same data**) · CPU/mem/disk per container · per-map population · ordered recommendation cards (file, key, value, cost, expected effect, `[Copy]` + `[Open file]`) · **"Current configuration" layer-chain table**.
**Home:** small badge only when degraded/critical.

## 6. Honesty rules
No verdict under ~10 min of samples ("collecting…") · flag a server restart mid-window · always show sample count + window · never claim a cause the evidence doesn't support (report "inconclusive").

## 7. GOLDEN ACCEPTANCE TEST (real data captured 2026-07-27, this box)
The engine MUST produce `single_thread_bound` for this input:
```
worldserver: cpu=96.48%  mem=5.236GiB/15.62GiB (33.5%)  blkio=17.3MB/0B
database:    cpu=2.81%   mem=819.8MiB (5.1%)            blkio=1.94GB/29.5GB
diff log:    101–118ms sustained, 0 players online, 500 bots
config:      MapUpdate.Threads=1, WSL processors=4, host cores=32, no docker limits
per-map:     {1:161, 530:134, 0:128, 571:77}
```
Expected: state=degraded/watch; diagnosis=**single-thread-bound** (96% ≈ one core of 4 allocated); RAM/disk/DB explicitly ruled out; rec #1 = `MapUpdate.Threads 1→3` in `env/dist/etc/worldserver.conf` (restart), rec #2 = `.wslconfig processors 4→8+` (needs `wsl --shutdown`) to unlock beyond that; supporting evidence = 4 populated maps ⇒ map-parallelism will actually help.

## 7b. VERIFIED OUTCOME (A/B run 2026-07-27) — the diagnosis was correct
Applied rec #1 (`MapUpdate.Threads 1→3`) on the live snapshot, world-restart, 500 bots back online, 180s settle, same 4-sample method:

| metric | before (threads=1) | after (threads=3) | change |
|---|---|---|---|
| mean | 25ms | 9ms | **−63%** |
| median | 5ms | 2.5ms | −50% |
| **p95** | ~89ms | **~27ms** | **−70%** |
| **p99** | ~98ms | **~36ms** | **−63%** |
| max | ~106ms | ~41ms | −61% |
| worldserver CPU | ~98% (1 core) | **~175%** (1.75 cores) | parallelism engaged |

p99 moved from the *watch/degraded* band into *healthy* (<100ms). CPU rising past 100% is the proof the map threads actually engaged.
**Follow-on insight for the engine:** 3 threads only reached ~175%, not ~300% — map load is uneven (map 1 = 161 online vs map 571 = 77), so parallelism is capped by the busiest map. The engine should therefore NOT naively recommend threads == cores; it should weigh the per-map population distribution (Tier-1 data) when sizing the recommendation, and recognise diminishing returns.
**New state:** healthy with headroom (CPU 175/400%, RAM 33%) ⇒ the engine's *over-provisioned* branch should now fire: room to raise `MaxRandomBots` above 500.

## 8. Shared infrastructure
The sampler + history store also powers **feature-batch item #4 (characters-online statistics)** — build once, both features consume it.
