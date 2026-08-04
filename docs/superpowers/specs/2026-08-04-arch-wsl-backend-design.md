# Backend::Arch — running the stack on dockerd inside an Arch WSL distro

**Status:** design approved by the user 2026-08-04. Branch `feat/arch-wsl-backend`
off `rust-main`.

**One sentence:** replace Docker Desktop with an Arch Linux WSL distro that runs
its own `dockerd`, and drive it with the Rust `dml-wow` binary running *inside*
that distro rather than on Windows.

---

## Why

The user asked for two things: run Docker on a Linux distro to save resources,
and compare that against Docker Desktop. Investigation on the user's machine on
2026-08-04 turned up a fact that reframes the work:

**`dml-arch` already runs its own dockerd.** Measured, not assumed:

```
$ wsl -d dml-arch -u dml -- sh -c "systemctl is-enabled docker; systemctl is-active docker"
enabled
active
$ pacman -Q docker docker-compose docker-buildx
docker 1:29.6.1-1
docker-compose 5.3.1-1
docker-buildx 0.35.0-1
$ docker info --format '{{.ServerVersion}} {{.Driver}} {{.MemTotal}}'
29.6.1 overlayfs 16771039232
```

Docker Desktop was *stopped* at the time. The WSL backend has always been "Arch
WSL hosting its own daemon"; what the bash CLI added on top was shell logic, not
an engine.

So this is not new infrastructure. It is **pointing the Rust code at the daemon
that is already there**, and retiring Docker Desktop as the default.

Two consequences fall out for free:

* **B4b is solved.** The `.sh`-in-a-distro runner that Vanilla and TBC wait on
  exists the moment the supported backend *is* an Arch box with systemd and
  NOPASSWD sudo. The six installers under `guides/*/install-*.sh` are pacman-first
  Arch scripts; they were written for exactly this host.
* **The Docker Desktop dependency goes away**, along with its Windows-side
  processes and its own `docker-desktop` utility distro.

It also reverses a recorded decision: `docs/ROADMAP-TO-BETA.md` says the user
wants `dml-arch` retired and fixes v0.1.0 on the Desktop-native backend. This
design makes the distro the foundation instead. That reversal is deliberate and
was taken by the user with the cost stated (see [Cost and schedule](#cost-and-schedule)).

---

## Decisions taken (user, 2026-08-04)

| # | Question | Decision |
|---|---|---|
| 1 | Scope | **Full `Backend::Arch`, shipped.** Detection, launcher picker, provisioning, lifecycle. Not a spike. |
| 2 | The bash `dml` CLI | **Retired as a runtime path.** The Rust binary is the only thing the launcher spawns. Bash survives only as the oracle the 18 parity suites diff against. |
| 3 | Server directory | **Inside the distro, `~/games/<title>`** — Linux ext4, not `/mnt/c`. |
| 4 | Fresh-PC distro | **`wsl --install archlinux`** from the official catalog, then a Rust provisioning chain. No hosted rootfs tarball. |
| 5 | Architecture | **The Rust binary runs inside the distro.** Launcher spawns `wsl.exe -d dml-arch -u dml --exec dml-wow <cmd> --json`. |
| 6 | Existing servers | **Fresh install into `~/games`.** The bash-built `~/games/wow-server-playerbots` is left alone as a fallback and as the other half of the comparison. |
| 7 | `migrate.rs` and `C:\Users\perzi\dml-native` | **Left alone.** Both keep serving `Backend::Native`. No Desktop→Arch import path in this round. |
| 8 | Branch | `feat/arch-wsl-backend` |

---

## Architecture

```
Tauri launcher (Windows)
  │  UI, tray, status poll, IPC
  └─ wsl.exe -d dml-arch -u dml --exec dml-wow <cmd> --json
       │  one spawn per command; NDJSON on stdout
       └─ dml-wow-cli  (Linux binary at /usr/local/bin/dml-wow)
            │  all filesystem, git, docker work — Linux paths only
            └─ dockerd  (systemd unit inside the distro)
                 └─ AzerothCore stack, server dir ~/games/<title>
```

Three enum arms after this work:

* **`Backend::Arch`** — new, becomes what detection picks.
* **`Backend::Native`** — Docker Desktop. Kept working as a fallback, never extended.
* **`Backend::Wsl`** — retired as a runtime path. The variant stays in the enum
  for one job only: an existing `launcher.json` or a `DML_BACKEND=wsl` env that
  still says `wsl` **resolves to `Arch`**, since it names the same distro and the
  same daemon. Nothing routes to the bash CLI. Refusing instead would strand
  every current user on a backend that no longer exists, and silently mapping it
  to `Native` would point them at a server directory that is not theirs.

### Why the binary runs inside the distro, not on Windows

The alternative considered was keeping the Rust process on Windows and prefixing
only the docker invocation (`engine.rs::docker_program()` → `wsl --exec docker`).
Rejected for three reasons:

1. **Decision 3 puts the server directory on Linux ext4.** That makes every
   filesystem path in `dml-wow` a Linux path — `conf.rs`, `composegen`,
   `install_native`, `backup`, `logsnap`. Running the Rust on Windows turns each
   of those into a `\\wsl$\dml-arch\home\dml\...` UNC question. This repo has
   recorded the "which spelling of this path is it" bug class three separate
   times (`canon_path`'s four spellings, the stack-conflict working-dir
   comparison, `wslpath` vs string surgery). Do not manufacture more of it.
2. **Spawn cost.** `wsl.exe` costs roughly 200–400 ms per invocation. Prefixing
   every docker call pays that on status polls, container listings and readiness
   probes; spawning once per command pays it once.
3. **The open bounded-call bug.** `dml_core::proc::run_bounded_outcome`'s
   deadline is *still not fully enforced* when the child spawns grandchildren
   (documented in CLAUDE.md, pinning test `#[ignore]`d, measured 605 s on a
   600 ms bound). `wsl.exe` is named in those notes as a program that spawns
   helpers. Under the Windows-side design that hazard sits on every docker call;
   under this one it sits on one spawn per command.

A TCP `DOCKER_HOST=tcp://127.0.0.1:2375` route was also rejected: it needs a
non-Desktop docker CLI shipped for Windows, `docker compose` would still resolve
relative bind paths against the Windows cwd (breaking the compose file), and it
exposes an unauthenticated daemon to every process on the box.

---

## Components

### 1. `CommandTarget` — the one new seam (`dml-core`)

```rust
pub enum CommandTarget {
    /// Spawn directly on this host. What Backend::Native does today.
    Host,
    /// Spawn inside a WSL distro: wsl.exe -d <name> -u <user> --exec <argv...>
    Distro { name: String, user: String },
}
```

`DmlRunner` carries a `CommandTarget`. `startup.rs` builds it from the resolved
backend, at the point where it already builds the runner once per process.

**`--exec`, never `--`.** Verified 2026-07-28 and recorded in CLAUDE.md: `wsl -- `
runs a shell, which splits on `;`, expands `$HOME`, and globs `*` against the
cwd. `--exec` passes real argv. Every argument crossing this boundary — title
ids, paths, user text — must go through it.

This enum is the *entire* routing change. Everything below it is unmodified Rust
running on Linux paths.

### 2. Provisioning chain (`dml-core::setup` + `launcher/src-tauri/src/provision.rs`)

Extends the existing `backend_status` probe chain, keeping its contract: answer
"what is the **first** thing missing", stream NDJSON `TermEvent`s over the
existing `Channel<Value>` seam, be idempotent, and re-consume the probe chain
both before and after rather than asking its own questions.

Ordered steps:

| # | Step | Command |
|---|---|---|
| 1 | WSL2 present and current | `wsl.exe --version` (WSL 2.7.10 on the dev box) |
| 2 | Distro registered | `wsl --install archlinux --name dml-arch --no-launch` |
| 3 | systemd enabled | write `/etc/wsl.conf` `[boot] systemd=true`, then `wsl --terminate dml-arch` to apply |
| 4 | `dml` user | `useradd`, NOPASSWD sudoers drop-in, `wsl --manage dml-arch --set-default-user dml` |
| 5 | Packages | `pacman -Syu --noconfirm docker docker-compose docker-buildx git` |
| 6 | Daemon | `usermod -aG docker dml`, `systemctl enable --now docker` |
| 7 | Binary | deploy `/usr/local/bin/dml-wow` 0755, version handshake |
| 8 | Ready | `dml-wow version --json` round-trips and `docker info` answers |

Flags verified against WSL 2.7.10 on 2026-08-04: `--name`, `--no-launch`,
`--location`, `--vhd-size`, `--web-download`, and `--manage <Distro>
--set-default-user` all exist.

**`docker-buildx` is not optional.** `install_native.rs`'s `pct` progress parser
reads BuildKit vertex headers out of the streamed build output, and the resume
story rests on BuildKit's cache. Without buildx the build falls back to the
legacy builder, the progress bar goes silent and resume degrades — a failure
that looks like a hang rather than a missing package.

**Failures are reported, never swallowed.** Each step gets its own error code
(`ARCH_WSL_MISSING`, `ARCH_INSTALL_FAILED`, `ARCH_SYSTEMD_FAILED`,
`ARCH_PACMAN_FAILED`, `ARCH_DOCKER_FAILED`, `ARCH_BINARY_DEPLOY_FAILED`) so a
broken pacman mirror does not surface as the same blank message as a missing
WSL. This is the Tailscale lesson: a sudo refusal, a missing unit, a dying
daemon and a merely-slow one must not produce one indistinguishable string.

### 3. Getting the Linux binary onto the box

* **Build.** CI's `linux` job in `.github/workflows/rust.yml` already builds
  `-p dml-core -p dml-wow -p dml-wow-cli` on `ubuntu-latest`. It gains an
  artifact upload. An ubuntu-built glibc binary runs on Arch (older glibc build,
  newer host).
* **Ship.** The binary rides in `bundle.resources` in `tauri.conf.json`.
  `payload.rs` owns that manifest and already fails the test run when the
  manifest and the layout drift apart; the new entry joins it.
* **Deploy.** `provision.rs` copies it to `/usr/local/bin/dml-wow` at 0755, with
  the resource dir translated once via `wsl --exec wslpath -a -u` — the exact
  mechanism it already uses. Never string surgery on `C:\` → `/mnt/c`.
* **Version handshake.** On launcher start, run `dml-wow version --json`; on
  mismatch with the launcher's own version, redeploy. Without this the project
  reacquires the "old CLI in the distro" bug class that `normalizeCatalog`
  currently has to fail open on.
* **Dev loop.** `pacman -S rust` inside the distro; build with
  `CARGO_TARGET_DIR=~/target` so cargo is not writing through drvfs.

### 4. Engine control (`dml-core::engine`)

Docker Desktop discovery (`candidate_docker_paths`, `docker desktop start|stop`,
`ENGINE_START_ASK_TIMEOUT`) has no meaning inside the distro. Introduce:

```rust
pub trait EngineControl {
    fn is_up(&self) -> Tri;
    fn start(&self) -> Result<()>;
    fn stop(&self) -> Result<()>;
}
```

* **`DesktopEngine`** — today's implementation, unchanged, used by `Backend::Native`.
* **`SystemdEngine`** — `systemctl is-active docker` / `systemctl start docker`.
  `is_up` stays a `Tri`: systemd failing to answer is evidence of nothing.

**Stop-engine under Arch is `wsl --terminate dml-arch`**, issued from the Windows
side (the distro cannot terminate itself and have anything left to report). That
returns the VM's RAM, which is the symmetric equivalent of what `docker desktop
stop` does today, and it is what makes the auto-shutdown feature meaningful on
this backend.

### 5. `dml-wow` changes

Deliberately small. The point of decision 5 is that most modules do not change.

* **`install_native.rs`** — logic unchanged. Default games dir flips from
  `%USERPROFILE%\dml-native` to `~/games`. `DML_GAMES_DIR` remains the override
  seam.
* **`composegen`** — Linux paths throughout. `canon_path`'s four-spellings
  folding stays: the stack-conflict guard still reads
  `com.docker.compose.project.working_dir` labels, and the four spellings are
  what make "ours" detectable.
* **Stack-conflict and port guards** — unchanged in logic, but note the ports are
  now published by the distro's dockerd. WSL2 localhost forwarding still puts
  3724 / 8085 / 7878 / 3306 on the Windows `127.0.0.1`, so the launcher's SOAP
  and MySQL clients keep working from Windows without a tunnel.
* **`migrate.rs`** — untouched this round (decision 7).
* **`logsnap`, `backup`, `conf`** — untouched; they now write Linux paths under
  `~/.dml`, which is what they were written for.

### 6. Launcher changes

* `startup.rs` resolution gains `Arch` and prefers it when the distro is usable.
  Precedence is unchanged and load-bearing: `DML_* env` → `~/.dml/launcher.json`
  → auto-detect. The parity, bats and CLI-integration suites all inject those
  env vars as override seams.
* `backend.rs::detect` gains an Arch arm. Tri-state discipline holds: a probe
  that could not answer never flips a working user off their backend.
* Settings backend picker gains Arch; Desktop is labelled a fallback.
* The backend-setup page drives the new chain through existing
  `backend-setup.svelte.ts`.
* **One new path helper** for user-facing surfaces: `\\wsl$\dml-arch\home\dml\...`
  for "open folder" buttons, `wslpath` for anything crossing into a command.
  Everything else stays Linux-side.

---

## Data flow, worked example: start the server

1. Home's start button → `games_lifecycle` IPC.
2. AppState's runner has `CommandTarget::Distro { name: "dml-arch", user: "dml" }`.
3. Spawn `wsl.exe -d dml-arch -u dml --exec dml-wow games start --id wow-… --json`.
4. Inside the distro the engine check runs `systemctl is-active docker`, starting
   it if needed.
5. The guard sequence runs unchanged: stack-conflict, port refusal, boot-loop watch.
6. `docker compose up -d` runs against the local socket; NDJSON `section_start`,
   `line`, `pct`, `done` events stream back over `wsl.exe`'s stdout.
7. The launcher's existing `TermEvent` parser consumes them. **No frontend change**
   — the contract is identical, which is the payoff of keeping the CLI seam.

---

## Error handling

* **Every probe is tri-state.** "Could not answer" is never "no". Applies to
  `systemctl is-active`, `wsl --list`, `docker info`.
* **Distinct codes per provisioning step** (listed above) so the user is told the
  actual blocker.
* **First-run downloads.** `wsl --install archlinux` and `pacman -Syu` pull a few
  hundred MB. A dead mirror or an offline box is a real first-run failure and is
  reported as one, with the failing command echoed. No silent retry loop.
* **Never generate over a compose file DML did not write** —
  `INSTALL_COMPOSE_EXISTS` applies unchanged in `~/games`, where a bash-built
  server already lives.
* **Bounded calls.** Every `wsl.exe` invocation from Windows must carry a
  wall-clock bound. Given the open grandchild bug, the plan's first task is to
  either land the `run_bounded_outcome` fix or prove the bound holds for
  `wsl.exe --exec` specifically. A backend whose every call goes through an
  unbounded spawn is not shippable.

---

## Testing

* **Pure units** for: `CommandTarget` argv construction (including that `--exec`
  is used and arguments survive metacharacters), provisioning step ordering and
  the "first thing missing" answer, `EngineControl` selection per backend,
  `detect` with the new arm.
* **Ordering is asserted against the real call site**, never against a restated
  pure list. The `lifecycle_steps_for_mode` lesson: a list production never reads
  will stay green while the real sequence rots.
* **Parity suites unaffected** — Rust and the bash oracle are both Linux-side now,
  which if anything makes them more honest. Keep the per-platform `find_bash` /
  `yq_path` gates and the `--nocapture` discipline: a suite that skips for a
  broken reason must not be indistinguishable from one that ran.
* **CI.** The `linux` job stops being advisory and becomes the job that builds
  the shipped artifact. It must fail the build, not warn.
* **Live gate (human).** Provision a throwaway distro name (`dml-arch-test`) from
  nothing on this box: `wsl --install` → provision → install a title → world
  ready. Then `wsl --unregister dml-arch-test`.
* **Anti-vacuity rule.** After writing any test that overrides a shared fixture,
  mutate the production code and watch it go red. Two vacuous-pass generators
  were found in one day on 2026-07-29 in harnesses that looked obviously correct.

---

## The comparison

Deliverable: `docs/backend-comparison-2026-08.md`. Numbers, not adjectives, with
the method pinned so it can be re-run. Same title, same client data, same modules,
measured on both backends.

| Metric | How | Why it is on the list |
|---|---|---|
| Idle RAM | Windows working set of Docker Desktop processes + `vmmem`/`vmmemWSL` | the claim being tested |
| RAM, server up + 500 bots | same, under load | idle savings can evaporate under load |
| Launcher open → world ready | wall clock | daily-use cost |
| Full install, clone → ready | wall clock | baseline is the 21m18s native run of 2026-07-31 |
| Disk | image store + build cache size | |
| RAM returned after stop | sample after stop + engine stop | Desktop is known to hold memory |

The two servers that make this measurable already exist: the bash-built
`~/games/wow-server-playerbots` in the distro, and the migrated
`C:\Users\perzi\dml-native` under Desktop. Decision 6 adds a third, freshly built
under the new engine, which is the one the Arch column is measured on.

---

## Non-goals

* No hosted rootfs tarball. Catalog install only (decision 4).
* No multi-distro support. `dml-arch` is the one supported name; the field stays
  configurable but nothing else is tested.
* No Windows-side docker CLI, no TCP daemon.
* No Desktop→Arch migration path (decision 7).
* Docker Desktop keeps working and is not extended.
* `Backend::Wsl`'s bash runtime path is not maintained, only its parity-oracle
  role.

---

## Cost and schedule

**2–3 weeks.** It moves v0.1.0's foundation after the roadmap fixed that
foundation as Desktop-native, and the live gates already listed in
`ROADMAP-TO-BETA.md` Phase C would need re-running on the new backend.

Against that it *removes* B4b from the backlog — the `.sh`-in-a-distro runner
Vanilla and TBC are blocked on is a solved problem once the supported backend is
an Arch box with systemd and passwordless sudo.

`ROADMAP-TO-BETA.md` and `CLAUDE.md` must both be updated when this lands: the
former's scope section still names Desktop-native as the v0.1.0 backend and says
`dml-arch` is being retired, and the latter's `crates/` section describes the
native path as the only Rust route.

---

## Risks

1. **First-run network dependency.** `wsl --install archlinux` plus a full
   `pacman -Syu` is the riskiest minute of a stranger's first run. Mitigation:
   distinct error codes, the failing command echoed, and the hosted-rootfs option
   kept on the shelf if testing shows real failure rates.
2. **Arch is a rolling release.** A `pacman -Syu` months from now installs a
   docker the project has never tested. Mitigation for this round: record the
   pinned-known-good versions (docker 29.6.1, compose 5.3.1, buildx 0.35.0) in
   the provisioning code and report the installed versions in `doctor`.
3. **The unbounded-call bug.** Named in error handling above; it is task one.
4. **Two servers in one distro.** `~/games` will hold the bash-built server and
   the new one at the same time. The `ac-*` container names are global to the
   docker *engine*, so only one can run — the existing stack-conflict refusal is
   what keeps that honest, and it must be verified to fire correctly against a
   bash-built neighbour rather than refusing the user's own server (the exact
   false refusal recorded on 2026-08-02).
