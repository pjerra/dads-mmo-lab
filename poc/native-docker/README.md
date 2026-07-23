# Native-Docker PoC — driving Docker Desktop without the `dml-arch` distro

**Branch:** `spike/docker-desktop-native`
**Goal:** prove the launcher can own a game's container lifecycle directly against
**Docker Desktop on Windows** — no hand-built `dml-arch` WSL distro, no bash `dml`
program in the middle. This is the "get rid of WSL / let Docker handle the Linux
VM itself" idea, turned into running code.

## TL;DR — it works

A minimal stand-in "game" ([docker-compose.yml](docker-compose.yml), one small
nginx container) was brought up, listed, reached, and torn down entirely through
the native Windows `docker.exe`, on Docker Desktop's own engine:

```
[1] docker compose -p dml-poc up -d      -> Container dml-poc-game-1  Started
[2] docker compose -p dml-poc ps         -> State: "running"   (context: desktop-linux)
[3] curl http://localhost:8899           -> HTTP 200
[4] docker compose -p dml-poc down       -> removed, zero leftovers
```

No `dml-arch`, no bash `dml`. The engine that ran it was Docker Desktop's
managed `desktop-linux` context — starting that engine even registered its own
`docker-desktop` WSL distro automatically, which is exactly the layer that
replaces our hand-built one.

## What's in the spike

| File | Role |
|---|---|
| `launcher/src-tauri/src/dml/native.rs` | The launcher-side backend: `docker.exe` discovery + `docker compose up/ps/down` builders + `ps --format json` parsing. 12 unit tests. Compiles into the app; **not yet wired** into the live command surface (WSL runner still owns every real feature). |
| `poc/native-docker/docker-compose.yml` | The stand-in "game" — swap the image for the real AzerothCore services when the port starts. |

Run the Rust tests: `cd launcher/src-tauri && cargo test native` (10–12 tests,
no engine needed). Reproduce the live run: start Docker Desktop, then
`docker compose -p dml-poc up -d` from this folder (see the credential-helper
note below if the pull fails).

## What the spike confirmed

1. **The launcher seam already exists.** `runner.rs` is already parameterized as
   `DmlRunner { program, prefix_args }`. Today it's `wsl.exe … dml`; a native
   backend is a sibling, not a rewrite. The launcher was built distro-agnostic
   without meaning to be.
2. **"Docker handles the Linux VM" is literally true.** Docker Desktop runs its
   own `docker-desktop` WSL2 VM. Dropping `dml-arch` removes *our* distro and the
   bash middleman — one fewer thing we maintain — though a Linux VM still exists
   under Docker Desktop (worth stating plainly: it's not "no VM", it's "not our
   VM").
3. **`docker.exe` discovery is needed.** A default Docker Desktop install here is
   **per-user** (`%LOCALAPPDATA%\Programs\DockerDesktop\resources\bin\docker.exe`)
   and is **not on the machine PATH**. `native.rs` resolves it: `DML_DOCKER`
   override → bare `docker` on PATH → known per-user/system locations.
4. **Credential-helper PATH gotcha (found live).** `docker.exe` shells out to
   `docker-credential-desktop.exe` even for anonymous Hub pulls. It sits next to
   `docker.exe`, so the backend must prepend that dir to the child's PATH or the
   first `up` dies before pulling a layer. `native.rs` does this.
5. **`ps --format json` shape verified.** Real output is NDJSON with
   `Name`/`Service`/`State`/`Health`; the parser handles both NDJSON and the
   array form.

## The honest part — what a full switch still needs

The launcher side is the easy ~10%. The hard ~90% is **not in the launcher and
not even in git**: the ~7000-line bash `dml` program generates each game's
compose file *inside* the distro at install time and drives the whole lifecycle
(install, DB import, config generation, realmlist, client-data volumes, LAN,
backups, module builds). "No WSL" means re-hosting that orchestration to run
natively on Windows against Docker Desktop. Concretely, a port must replace:

- **Compose generation** — today written into `/home/dml/games/<title>/…` by
  bash; would become committed templates + a native renderer.
- **The install scripts** (`install.sh`, `dml run <git-url>`) — these are Linux
  shell; they either move into a build container or get reimplemented.
- **Host-path assumptions** — `/home/dml/...`, `/mnt/c/...` translation
  (`realmlist.rs` already does the C:\ ↔ /mnt/c/ mapping; the rest doesn't).
- **Distro-only launcher features** — the `.wslconfig` RAM editor, ext4.vhdx
  shrink, "restart WSL", "open shell" all lose meaning; Docker Desktop owns that
  now. They get dropped, not ported.

None of that is hard *per se* — but it's the shared DML orchestration layer, so
it's a team decision (and a good fit for the "small per-game client apps
attaching to containers" direction), not a launcher-only change.

## Increment 2 — the REAL acore stack, natively

The toy above proved the plumbing; this increment points the **real** server at
Docker Desktop.

- **`wow-playerbots/docker-compose.yml`** — the actual acore stack
  (ac-database / ac-db-import / ac-authserver / ac-worldserver / ac-client-data),
  re-expressed for Docker Desktop: no `build:` (we pull published images), no
  host bind mounts (named volumes, self-contained), dev/tools services dropped,
  DB wiring via the `AC_*_DATABASE_INFO` env vars. `docker compose config`
  validates it natively and lists all five services.
- **`dml/backend.rs`** — the single switch point. `Backend::selected()` reads
  `DML_BACKEND`; default `Wsl` (nothing changes), `DML_BACKEND=native` routes the
  game lifecycle to `NativeDocker`. Ships dormant and tested, so the port is a
  routing change at the `games_*` call sites, not a rewrite.
- **`dml/native.rs`** gained `start()` / `stop()` / `status()` mirroring the WSL
  `games start/stop/status`, with a pure `game_state()` (running/stopped) so both
  backends report the same shape.

### Live result (real acore images, on Docker Desktop) ✅

Brought up the auth path (`docker compose -p dml-wow-native up -d ac-authserver`
→ db → db-import → authserver) on Docker Desktop, engine context `desktop-linux`:

```
ac-database   running (healthy)     mysql:8.4
ac-db-import  exited (0)            acore/ac-wotlk-db-import:master   # base SQL import completed
ac-authserver running               acore/ac-wotlk-authserver:master

authserver log:  Connected to MySQL database at ac-database
                 DatabasePool 'acore_auth' opened successfully.
                 Added realm "AzerothCore" at 127.0.0.1:8085.

acore_auth tables = 22        # db-import really populated the real schema
realmlist rows   = 1
port 3724 -> 0.0.0.0:3724     # Test-NetConnection 127.0.0.1:3724 = True (reachable from Windows)
```

The real acore authserver + db-import + MySQL, the real dependency graph (health
gate + `service_completed_successfully`), a genuinely populated schema, and the
realm socket listening on the Windows host — all driven by the exact
`docker compose` commands `native.rs` builds, with **no `dml-arch` distro and no
bash `dml`**. Then torn down clean (`down -v`); the images stay cached so the
next native `up` needs no re-pull.

> One bug found and fixed live: a YAML anchor (`*ac-db`) does not expand inside a
> quoted scalar, so the first run's DB connection string was malformed and
> db-import failed. The connection strings are now inlined literally (matching
> the distro's own compose). The dependency ordering was correct from the first
> run — only the string was wrong.

### What this instance is — and isn't

It is a **clean, real acore server**: the genuine acore images, the real
db-import populating a real MySQL, the real dependency graph (health gate +
`service_completed_successfully`) — all driven natively, no `dml-arch`, no bash
`dml`. It is **not** the user's world: Docker Desktop has its own volume
namespace, so the DB/characters/2500 bots living in `dml-arch`'s `ac-database`
volume are not here. Bringing the configured server's data + `env/dist/etc`
config tree across engines is a separate migration (MySQL dump/restore + a
config copy) — the last mile after orchestration, not part of proving it.

## Recommended next increment (3)

Wire `Backend::selected()` into the launcher's `games_start` / `games_stop` /
`games_status` commands so flipping `DML_BACKEND=native` makes the running app
drive the native stack, and add the data-migration path (dump the distro DB →
restore into the native volume, copy `env/dist/etc`) so the native server is the
user's actual world, modules and bots included.
