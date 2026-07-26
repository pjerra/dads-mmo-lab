# DML Rust CLI Workspace — Design

**Date:** 2026-07-26
**Branch:** `feat/rust-cli-workspace` (off `spike/docker-desktop-native`)
**Status:** Approved design, pre-implementation

## Motivation

The DML community (Baerthe, 2026-07-23) wants to move away from one universal
`dml` CLI toward per-game, robust client software that attaches to Docker
containers in isolation — portable to any platform, usable from any frontend
(Tauri, Electron, plain scripts).

We are already most of the way there: the entire portable `dml` surface was
ported to native Rust (2026-07-25/26, `098dbac..568ea4c`, 818 lib tests,
live-parity-verified against the bash oracle). But that code lives inside the
launcher's Tauri core (`launcher/src-tauri/src/dml/`, 33 modules) and is only
reachable as Tauri commands. Nothing outside the launcher can use it.

This project reshapes that code into a cargo workspace: a shared core crate, a
WoW game crate, and a thin standalone CLI binary — the concrete proof of the
per-game direction, offered to the DML maintainers as a branch on this fork.

## Decisions (made during brainstorming)

| Question | Decision |
|---|---|
| Audience | Community deliverable — a pitch Baerthe/James can build and evaluate |
| Platforms | Windows (Docker Desktop) + Linux (native docker) from day one |
| Delivery | Cargo workspace on this fork branch; upstream/own-repo is their call later |
| Install | Manage-only; `install` wraps the existing bash installer scripts |
| Shape | Shared core crate + per-game library crate + per-game CLI binary |

## Architecture

```
dads-mmo-lab/
  Cargo.toml            # NEW: workspace root
  crates/
    dml-core/           # shared, game-agnostic library
    dml-wow/            # WoW library (all existing ported logic)
    dml-wow-cli/        # thin binary crate; exe name: dml-wow
  launcher/src-tauri/   # Tauri app, becomes a workspace member,
                        # depends on dml-wow (and dml-core transitively)
```

The launcher keeps calling the same functions in-process (no subprocess, no
speed regression). The CLI exposes the same functions to everyone else.

### dml-core (game-agnostic)

Moves from `launcher/src-tauri/src/dml/`:

- Subprocess runner: bounded, pipe-draining docker/git execution
  (`runner.rs` / `output_bounded_draining`).
- Compose/engine lifecycle: docker binary discovery, compose up/stop,
  Docker Desktop start/stop on Windows (`lifecycle.rs`, `backend.rs`,
  the engine parts of `native.rs`).
- Config-file engine: registry-driven conf read + byte-parity `conf_write`,
  override-YAML parsing (the file-format halves of `config.rs`/`tuning.rs`).
- Output contract: JSON envelope + NDJSON stream events
  (`envelope.rs`; events `section_start/line/section_end/done/error`).
- Platform layer (new, small): games-dir resolution (`DML_GAMES_DIR` env →
  platform default), docker discovery (Docker Desktop per-user path on
  Windows vs `PATH` on Linux), engine startup (on Linux: expect a running
  dockerd; if absent, fail with a clear actionable message — we do not
  manage systemd).

### dml-wow (game library)

Everything WoW-specific, i.e. the rest of the 33 `dml/` modules: game
registry/status, SOAP client + commands, MySQL readers/writers (bound params
only), config/tuning/module registries and writes, module manager + tail,
accounts, backup/restore (streaming, constant memory), party ops + presets,
paperdoll, teleports, stats, destructive ops (flush guard trio: `FlushGuard`
Drop + `.dml-bot-flush-armed` breadcrumb + games-start heal), AH bot, LAN,
cache status, item info, client path.

Split rule of thumb: if a second game would need it unchanged, it goes in
`dml-core`; if it mentions AzerothCore, WoW schemas, SOAP commands, or
playerbots, it goes in `dml-wow`.

### dml-wow-cli (binary)

- Crate `dml-wow-cli`, binary name `dml-wow`.
- clap subcommands mirroring the bash `dml wow <cmd>` surface without the
  `wow` prefix: `dml-wow config list`, `dml-wow start`, `dml-wow backup`, …
- No logic: parse args → call `dml-wow` function → print JSON envelope to
  stdout (streaming commands print NDJSON events as they happen).
- Exit code 0 on `ok`, non-zero with an error envelope otherwise — same
  contract the bash `dml --json` established.
- `dml-wow install` shells out to the existing bash installer script and
  streams its output as NDJSON `line` events. On Windows this requires Git
  Bash; if missing, fail with a message saying exactly that. The installers
  stay bash on purpose (standing verdict: the install scripts ARE the
  product; porting them is explicitly out of scope).

### Launcher integration

- Tauri commands in `launcher/src-tauri` become one-line wrappers over
  `dml-wow` calls. Frontend (`api.ts` routing, Svelte pages) unchanged.
- WSL mode untouched: it still shells to the bash `dml` byte-identically.
- Native mode behavior must stay byte-identical — proven by re-running the
  existing live parity suites (17 `tests/*_parity.rs`) after the move.

## The contract is the pitch

`docs/cli-contract.md` (new) documents, for frontend authors:

- The JSON envelope shape and the NDJSON stream event vocabulary.
- The command surface (name, args, envelope payload per command).
- How to attach: spawn `dml-wow <cmd> ...`, read stdout, that's it.

This document is what makes "attach your own frontend / Electron app" real
for the community rather than a slogan. The envelope/event formats already
exist and are tested; this writes them down.

## Testing & CI

- The 818 lib tests move with their modules into the two library crates.
- The 17 live parity suites stay integration tests (they need a live server
  and the bash oracle); they keep their current skip-gracefully-if-no-server
  behavior and are re-run locally as the byte-identical gate.
- New tests only for the CLI layer: arg parsing, envelope/exit-code shape,
  streaming event framing.
- New GitHub Actions workflow: build + `cargo test --workspace` on
  `windows-latest` and `ubuntu-latest` (parity tests self-skip there).
  Existing vitest/svelte-check baselines unchanged (vitest 385, check 0/0).

## Deliverable to the community

1. The branch itself — builds with `cargo build --release -p dml-wow-cli`.
2. `docs/cli-contract.md` — the attach-a-frontend contract.
3. A short pitch doc (`docs/rust-cli-pitch.md`): what this is, why per-game
   binaries on a shared core, how Veil Lab / an Electron app would attach,
   what a second game crate would look like (shape only).
4. A live demo on the native server: `dml-wow status`, `start`, `backup`.

## Error handling

Unchanged from the port: errors travel inside the envelope/stream (never
panics across the boundary), DB errors collapse to `DB_UNREACHABLE`, SQL uses
bound parameters only, subprocesses are time-bounded and pipe-drained. The
CLI adds only: unknown-args errors from clap (stderr, non-zero exit) and the
missing-Git-Bash / missing-docker preflight messages.

## Out of scope

- Porting installer scripts to Rust.
- macOS support (nothing should preclude it; nothing tests it).
- A second game crate (the shape supports it; we don't build one).
- Any WSL-mode change, any launcher UI change beyond import paths.
- Merging to main (standing policy: no merge until the user asks).

## Risks

- **Workspace-izing the Tauri app**: making `launcher/src-tauri` a workspace
  member moves the target dir to the repo root and can surprise the Tauri
  bundler; verified by building the release exe as part of the work.
- **Move-refactor regressions**: mitigated by the parity suites (native) and
  byte-identical WSL behavior (untouched code paths).
- **Linux is claimed but this machine is Windows**: CI covers build + unit
  tests; a live Linux end-to-end run needs a community tester (called out
  honestly in the pitch doc).
