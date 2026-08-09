# Module rebuild fix — make it compile, refuse honestly, standing button

**Date:** 2026-08-09
**Status:** approved (user picked option 1 of 3)
**Branch:** feat/core-family

## Problem

`wow module rebuild` on a native install reports success without compiling
anything. Found live on the user's VM (the real server, installed via the
one-click native installer): `rebuild.log` contains only container lines, no
compile lines. C++ modules installed from the launcher's Modules page
therefore never enter the worldserver binary, while the UI says "Rebuild
complete."

### Root cause

Native installs keep ALL build config in `docker-compose.build.yml`, which
compose **never auto-loads** — the install engine passes it explicitly
(`compose -f docker-compose.yml -f docker-compose.build.yml build`). The
module rebuild arm (Rust `modmgr::module_rebuild_stream`, bash `90-main.sh`
`rebuild)`) runs plain `docker compose up -d --build`, which never sees the
overlay: it recreates containers, exits 0, and the arm clears the pending
marker and reports `rebuilt: true`. This is the recorded "boots a silently
wrong server" class. `unbound.rs` already names and avoids this exact trap
(`resolve_build_files` + the `compose config` build-section guard, review
CRITICAL 2026-08-02); the module arm never got the same fix.

Two adjacent traps, same family:

- **C++ module install on an image-only server** (migrated installs have no
  build config anywhere and no source checkout): the install clones the
  module, marks rebuild pending, and the user discovers only later — or
  never — that the module can never be compiled in.
- **Unbound's mod-ale pin vs Modules-page clones** (hit live 2026-08-09, VM):
  Modules-page installs are `git clone --depth 1` at HEAD, so the local
  history lacks Unbound's pinned commit; the re-pin `git checkout` fails and
  the install refuses. Right refusal, missing remedy: it should fetch the pin
  before giving up.

## Scope

In: the three fixes below, both CLI surfaces where applicable, the launcher
button, contract docs, tests.

Out (filed in the post-smoke roadmap as a follow-up): a rebuild engine for
migrated image-only servers (build fresh images from a source checkout,
retag `dml.local/...`, swap). Until then those servers get an honest refusal
instead of a lying success.

## Design

### 1. Shared build-capability helper (Rust)

Extract from `unbound.rs` into one shared home (e.g. `buildcap.rs` in
`dml-wow`), used by unbound AND modmgr so they cannot drift:

- `resolve_build_files(sdir) -> Vec<String>`: the `-f` set. If
  `composegen::BUILD_FILE` exists → `-f` base, override, build file (each
  only if present). Else → empty (bash-era/WSL servers keep `build:` in the
  base compose and need no flags).
- `worldserver_build_config(docker, sdir, files) -> Tri`: `compose <files>
  config --format json`, answer = `services.ac-worldserver.build` present.
  Tri-state: a compose that cannot answer is evidence of nothing.

### 2. Rebuild actually builds (Rust native arm)

`module_rebuild_stream` gains, in order:

1. **Guard before backup** (mirrors unbound's ordering: refuse before the
   user waits on a dump): if `worldserver_build_config` answers **No** →
   refuse with new code `MODULE_NO_BUILD_CONFIG`, message naming prebuilt
   images, hint naming the two server shapes that can build (native install,
   WSL install) and that a migrated server cannot take C++ modules yet.
   `CouldNotTell` → warn and proceed (never refuse on silence).
2. Backup (unchanged), stop worldserver (unchanged).
3. **Build step**: `compose <resolved -f set> build ac-worldserver`,
   streamed unbounded to `rebuild.log` (unchanged path), parsing ninja
   counters into `pct` events via the same `BuildProgress` the installer and
   unbound use. Then `compose up -d` (no `--build`).
4. Clear pending, `rebuilt: true` (unchanged).

On a WSL-shaped dir (no BUILD_FILE) the `-f` set is empty and the build uses
the base compose's own `build:` sections — same compile the WSL arm does
today, now via an explicit `build` step instead of `up --build`.

### 3. C++ install/update refuse on no-build servers (both surfaces)

`install_cpp` (Rust) and the bash cpp install/update arms run the same guard
BEFORE cloning; refusal reuses `MODULE_NO_BUILD_CONFIG` with an
install-flavoured message. Lua/SQL modules are unaffected (no compile).

### 4. bash mirror (WSL rebuild arm)

Add the same pre-backup guard (`docker compose config` probed for an
ac-worldserver build section; awk/grep anchored on the compose config JSON,
not a raw scan of the YAML — the `_stack_is_ac` lesson). Keep
`up -d --build` as the build command there: WSL base composes carry
`build:`, proven by the Jul 20 rebuild.log. Tri-state degrade mirrors Rust:
cannot-answer → warn + proceed.

### 5. `module list` gains `can_build` (contract addition)

- Rust native: `BUILD_FILE` exists at sdir → `true`; composegen installs
  always have it, migrated installs never do. Disk evidence only — no
  docker call on the list path.
- bash: emit `true` (WSL installs are source checkouts); the authoritative
  refusal lives in the arms. Field documented in `docs/cli-contract.md` and
  `cli/README.md`.
  **Implemented deviation:** bash does not emit a hardcoded `true` -- it
  reuses the same `_module_can_build` tri-state probe the install/update/
  rebuild guards use, collapsed to a bool (`true` for `yes` *or* an
  unreadable answer -- fail-open, same rule as the guard; `false` only for a
  definite `no`), so a migrated-shaped WSL server (were one ever to exist)
  reports honestly instead of always `true`.
- Frontend `normalizeCatalog` FAILS OPEN on a missing field (older CLI keeps
  working) — same pattern as `install_supported`.

### 6. Launcher: standing Rebuild button (Modules page)

- New **"Rebuild server"** button at the top of the Modules page, always
  visible (not only when `rebuild_pending` is non-empty). Same confirm flow
  as the banner button ("30–90 minutes, stops the world while building"),
  same backup checkbox, same streamed terminal via `wowModuleRebuild`.
- Disabled with a hover hint when `can_build` is `false` ("This server runs
  prebuilt images — C++ modules can't be compiled into it") or when a
  feature lock / another operation is active.
- The pending banner stays exactly as-is (it carries the "required for: X"
  context the standing button lacks).

### 7. Unbound clone-ale: fetch the pin before refusing

In the pin-mismatch path (`do_clone_ale`), before the `ALE_PIN_MISMATCH`
refusal: run a bounded `git -C modules/mod-ale fetch origin <MOD_ALE_COMMIT>`
(then retry the checkout). Covers the Modules-page `--depth 1` clone. Fetch
fails or checkout still fails → the existing refusal, hint updated to name
the shallow-clone cause. Mirrors nothing (unbound is Rust-only by design).

## Error handling summary

- `MODULE_NO_BUILD_CONFIG` (new, both surfaces): rebuild / cpp install on a
  server with no ac-worldserver build config. Refusal happens before backup
  (rebuild) / before clone (install).
- Compose config unreadable → warn + proceed on every path (tri-state; a
  probe that cannot answer refuses nothing).
- Build failure keeps the existing `BUILD_FAILED` shape + `rebuild.log`
  pointer.

## Testing

- **Argv needle**: fake-docker assertions pinned on the
  `docker-compose.build.yml build` needle — NB the rendered argv never
  contains the substring "compose build" (recorded lesson in
  `install_native.rs` tests); assert the overlay filename.
- **Guard mutation**: deleting the guard call goes red — a test drives a
  no-build compose config through the rebuild and asserts the refusal code,
  and a separate test asserts the build argv is absent after refusal.
- **Ordering**: refusal happens BEFORE the backup call (assert via fake-io
  call order, not a pure list — the `lifecycle_steps_for_mode` lesson).
- **Tri-state**: compose config error → rebuild proceeds with a warn line.
- **bash**: bats for the WSL guard (stub compose config), refusal envelope,
  and unchanged happy path. Run bats and cargo suites sequentially, never
  overlapped (recorded rule).
- **Frontend**: vitest for `can_build` fail-open normalization + button
  disabled state; svelte flow reuses the existing runStream contract test
  shape (UI outcome from done/error events, not promise rejection).
- **Unbound**: pin-mismatch test gains the fetch-then-retry arm (fetch
  scripted to succeed → install proceeds; fetch fails → existing refusal).

## Live gates (user)

1. VM: copy mod-city-bots into the server's `modules/`, click the standing
   Rebuild button, watch real compile lines + `pct` progress, then the
   `mod-city-bots: stage cast loaded` log line.
2. VM: delete `modules/mod-ale`, Resume the Unbound install (unblocks today,
   independent of this fix).
3. This machine (migrated install): standing button renders disabled with
   the hint; `dml-wow module rebuild` refuses with `MODULE_NO_BUILD_CONFIG`.
