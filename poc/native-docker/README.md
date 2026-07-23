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

## Recommended next increment

Point the real acore stack at Docker Desktop: commit a native compose template
for the WoW-playerbots server, add a `NativeDocker`-backed `games start/stop/
status` path behind a backend switch, and prove start/stop/status on the real
images. That's the first slice where the launcher drives an *actual* game
natively — everything above is the groundwork that makes it a bounded task.
