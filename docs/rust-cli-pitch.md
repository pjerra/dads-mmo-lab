# Per-game CLI binaries on a shared core — the `dml-wow` Rust workspace

**Branch:** `feat/rust-cli-workspace` (pjerra fork)
**Date:** 2026-07-27
**Companion doc:** `docs/cli-contract.md` — the machine contract (envelope shapes, event
vocabulary, error codes, per-command payloads). This document is the *why*; that one is the *what*.

## The direction

The design spec (`docs/superpowers/specs/2026-07-26-rust-cli-workspace-design.md`) records the
community direction this branch implements:

> The DML community (Baerthe, 2026-07-23) wants to move away from one universal
> `dml` CLI toward per-game, robust client software that attaches to Docker
> containers in isolation — portable to any platform, usable from any frontend
> (Tauri, Electron, plain scripts).

That is the spec's recorded paraphrase of the 2026-07-23 direction — no verbatim quotation from
Baerthe exists in this repo, so we quote the spec rather than invent one.

The reshaping was cheap because the work was already done once: the entire portable `dml` surface
had been ported to native Rust inside the Tauri launcher (2026-07-25/26, 818 lib tests at that
point, live-parity-verified against the bash CLI). But that code was only reachable as Tauri
commands. This branch moves it into a cargo workspace so the *same* functions are reachable three
ways: in-process (the launcher), as a standalone binary (any frontend or script), and as plain
Rust libraries (a future second game crate).

## What exists today

### Crate map

```
Cargo.toml                  # workspace root
crates/
  dml-core/                 # game-agnostic library, 10 modules:
                            #   backend, compose, conf, engine, envelope,
                            #   error, events, proc, runner, util
  dml-wow/                  # the WoW (AzerothCore + mod-playerbots) library:
                            #   30 modules + embedded registries
                            #   (config 66 rows, tuning 13, module catalog
                            #   19 cpp / 9 lua / 10 sql)
  dml-wow-cli/              # thin clap 4 binary, exe name "dml-wow":
                            #   74 subcommands, no logic of its own —
                            #   parse → call dml-wow → print envelope/stream
launcher/src-tauri/         # the Tauri 2 launcher, now a workspace member;
                            # its native mode calls dml-wow in-process
```

Build the CLI alone with `cargo build --release -p dml-wow-cli`. The only dependency added by the
whole exercise is `clap`; the library crates depend on neither tauri nor (directly) tokio.
(Honesty note: `reqwest`'s blocking feature pulls tokio in *transitively*; there are zero tauri
edges.)

The output contract is exactly the one the bash `dml --json` established: one JSON envelope per
command (`{"ok":true,"data":...}` / `{"ok":false,"error":{code,message,hint}}`), or an NDJSON
event stream for the 14 long-running arms (`line`, `section_start`, `section_end`, terminal
`done`/`error`). Exit codes: 0 on ok, 1 on an error envelope or a stream that ends in `error`
(or in nothing — a silent death is a failure to report), 2 on a clap usage error. `dml-wow
version` reports `"contract": "dml-json-v3"` so frontends can gate on it.

### Test state

- **1063 passed / 0 failed / 2 ignored** for `cargo test --workspace` on Windows, measured
  2026-07-27 with the server stack fully **down**, so that number does not depend on a live
  environment. Split: dml-core 107, dml-wow 648, the 18 parity suites 38 (self-skipping),
  dml-wow-cli 91 + 54, launcher 125. The Linux CI job runs 933 of these (it excludes the
  launcher package and three Windows-only tests).
- On top of the unit/integration tests, 18 `tests/*_parity.rs` suites live in
  `crates/dml-wow/tests/` and one CLI-layer suite (`cli_integration.rs`, arg parsing /
  envelope-and-exit-code shape / stream framing) in `crates/dml-wow-cli/tests/`.

### CI

`.github/workflows/rust.yml`, green on both jobs since commit `7bd9a47` (2026-07-27):

- **windows-latest**: `cargo build --workspace --locked` + `cargo test --workspace --locked`
  (includes the launcher crate).
- **ubuntu-latest**: build + test `-p dml-core -p dml-wow -p dml-wow-cli` only — the launcher is
  Tauri/Windows-focused and needs webkit system libs; the community CLI story on Linux is the
  three crates.

Getting Linux green took three passes, and the middle one is the interesting part. First, 10
unit tests (1 dml-core + 9 dml-wow) had to be made platform-portable — drive-letter `Path`
literals, `;`-joined PATH strings, `cmd.exe` children. Then the *green itself* turned out to be
partly hollow: every one of the 18 parity suites carried a `find_bash()` that probed only
`C:\Program Files\Git\...`, so on Linux they all announced "no bash" and passed while bash sat
at `/usr/bin/bash`. Six of them — cache-status, client-path, config-write, tuning-write,
item-info, lan-public-ip — are offline or network-only by construction and genuinely can run
there; they now do. The Linux job also runs with `--nocapture` so the skip lines are visible in
the log, because a suite skipping for a broken reason is otherwise indistinguishable from one
that ran.

## Parity methodology: the bash CLI is the oracle

The bash CLI (`cli/src/*.sh`, concatenated into `cli/dml`) is the reference implementation the
Rust port was written against. "Parity" here is not "looks similar" — the 18 parity suites in
`crates/dml-wow/tests/` run the *actual bash CLI* next to the Rust code and deep-equal (in
several places byte-equal) the outputs: config/tuning reads and writes, module state, status,
backups, stats, DB-backed pages, SOAP flows, item info, client-path detection, and more. The
`lan` arm's exit-code semantics were proven by running both binaries across 14 action/state
combinations (14/14 agreement).

Two properties of these suites matter for evaluating the branch:

1. **They need the real thing.** A suite runs only when its prerequisites exist — the native
   title files, bash, yq, a reachable MySQL, or the full live server, depending on the suite
   (measured tiering of the first 17: 7 need nothing, 6 need the DB only, 4 need the full
   stack; the 18th, `tuning_write_parity`, landed after that measurement and needs bash plus
   its fixtures).
   Anywhere else they print why they skipped and pass.
2. **Therefore CI green does not include most of them.** Six suites run on Linux CI; the full
   18 run only against an installed, running server. That full run has been done: on
   2026-07-27, against a live AzerothCore + playerbots snapshot, all 18 suites executed with
   **zero skips** — 686 tests passed, 0 failed. This is by design (CI has no AzerothCore), but
   it means the parity guarantee is only as portable as someone re-running the suites on their
   own box —
   which anyone can do; the prereq paths are overridable via `DML_GAMES_DIR` / `DML_BASH` /
   `DML_SCRIPT` / `DML_YQ_BIN`.

## How a third party attaches

Spawn the binary, read stdout. That is the whole integration surface:

- **Veil Lab / an Electron app**: spawn `dml-wow status`, parse one JSON line; spawn
  `dml-wow start` and read NDJSON events as they arrive, deriving success strictly from the
  terminal `done`/`error` event (never from process exit alone, and never from byte order —
  JSON key order is unspecified between the Rust and bash emitters).
- **A plain script**: `dml-wow backup create --name nightly` and check the exit code; `dml-wow
  bots --online --limit 10` piped into `jq`.
- **The Tauri launcher**: doesn't spawn at all — it links `dml-wow` and calls the same functions
  in-process. Same code, three transports.

Configuration is environment variables only, deliberately — no config file, no global flags:
`DML_GAMES_DIR` (where the titles live), `DML_SOAP_URL`/`DML_SOAP_USER`/`DML_SOAP_PASS` or
`~/.dml/soap.env` (SOAP credentials), plus optional overrides like `DML_DOCKER` and
`DML_BACKUP_KEEP`. Everything has a working default; nothing panics on unset.

Do not code against this document — code against `docs/cli-contract.md`, which enumerates the
envelope/event shapes, the error-code vocabulary, the exit-code table, every subcommand, and the
per-command caveats. The contract identity is the `"contract": "dml-json-v3"` string in the
`version` envelope.

## What a second game crate would look like

The split rule, from the spec: *"if a second game would need it unchanged, it goes in
`dml-core`; if it mentions AzerothCore, WoW schemas, SOAP commands, or playerbots, it goes in
`dml-wow`."*

A hypothetical `dml-<game>` crate starts with these `dml-core` surfaces
(`crates/dml-core/src/lib.rs`):

- **`envelope`** — the ok/error envelope constructors and parser (`ok_envelope`,
  `error_envelope`, `parse_envelope`, `envelope_to_result`). Emitting these *is* speaking the
  contract.
- **`events`** — the NDJSON stream event constructors (`line_event`, `section_start`,
  `section_end`, `done_event`, `error_event`). Section-name constants stay with the game crate.
- **`error`** — `CmdError {code, message, hint}` plus the public helpers `bad_arg`,
  `not_found_err`, `io_internal_err` (generic wording; game-specific hints belong in the game
  crate).
- **`proc`** — generic subprocess execution in the two shapes long-running tooling needs:
  `run_captured` (bounded, captured-then-split) and `run_streamed_unbounded` (drained as it
  arrives — the 30-90-minute-build shape).
- **`runner`** — `DmlRunner`: spawning a child CLI and consuming its envelope/stream, including
  the consumer-side `CLI_CRASH` synthesis when a child dies without a terminal event.
- **`engine`** — docker binary and Docker Desktop discovery (`DML_DOCKER` /
  `DML_DOCKER_DESKTOP` overrides, known install paths, PATH fallback) and engine up/down.
- **`compose`** — generic `docker compose` lifecycle primitives: games-dir resolution
  (`DML_GAMES_DIR`), per-title compose dirs, the up/stop argv sequences, running-count, the
  port-conflict bind probe.
- **`conf`** — the generic `Key = value` conf-file engine with byte-parity writes. (Not the
  compose-override YAML reader/writers: those hardcode AzerothCore's `ac-worldserver` service
  key, so Task 7 moved them into `dml_wow::config` and dropped `serde_yaml_ng` from `dml-core`
  entirely. A second game crate brings its own override handling.)
- **`backend`** — the Wsl/Native orchestration selector (`DML_BACKEND`).
- **`util`** — `home_dir`/`dml_home_dir` (`~/.dml` resolution, `USERPROFILE` then `HOME`).

What the game crate itself supplies is everything on the other side of the rule — for WoW that
turned out to be 30 modules: the title registry and status probes, the server-protocol client
(SOAP, for AzerothCore), DB readers (bound parameters only), config/tuning/module registries and
their writers, backup/restore streaming, party/GM/game-specific operations. A second game
replaces those with its own title id, its own protocol, its own schemas — and gets the envelope
contract, the docker/compose machinery, the subprocess discipline, and the conf-file engine for
free.

One thing is *not* yet shared: the CLI-side plumbing (`out.rs`'s panic-free stdout writers and
sticky `TerminalSeen` exit-code tracker, plus `run.rs`'s `stream_dispatch`) lives in
`dml-wow-cli`, not `dml-core`. A second game CLI today would copy that pattern (~small); hoisting it into
`dml-core` is an obvious follow-up if a second crate materializes.

## Honest status

**Windows** is the exercised platform: the workspace was built and tested there throughout, the
18 parity suites genuinely execute there against a live native server, and the launcher consumes
`dml-wow` in-process in the same builds. Remaining owed work even on Windows: the plan's final
full-stack live gate (Task 18) has not run yet, and until it does the *mutating* happy paths
(account create/delete, gm commands, mail-item, teleport, party operations) rest on code
reading, parity of the read paths, and sealed-endpoint probes rather than a recorded live run.

**Linux** is built and unit-tested in CI (both jobs green since `7bd9a47`, 2026-07-27) and
nothing more. The parity suites self-skip there; no live Linux end-to-end run has ever happened.
The spec named this risk explicitly: this machine is Windows, so **a live Linux smoke needs a
community tester** — that is the concrete ask of this pitch. `docs/FOR-TESTERS.md` exists for
exactly this.

Known gaps, named:

- **No cross-process SOAP serialization.** Each `dml-wow` invocation takes a fresh in-process
  lock only; two concurrent invocations (or one next to the GUI or the bash CLI) can interleave
  SOAP calls. The bash CLI serializes via `flock` — but only where `flock(1)` exists (Linux);
  Git Bash on Windows skips it, so on Windows this is parity, on Linux it is a regression
  relative to bash.
- **`server.motd` read gap.** The native config reader never reads the DB for the MOTD, so
  `config list`/`config get server.motd` show the registry default even when the DB is reachable
  and holds a custom value. Setting it (`dml-wow motd`) works live over SOAP but is never
  reflected back by this CLI's own read arms.
- **The library assumes its callers validated first.** Validation lives in the CLI/launcher
  wrappers, not at the library boundary, and it fails two different ways. Some functions
  *panic* on unvalidated input (`lan::lan_action`'s `.expect`/`unreachable!`, `moduletail`'s
  `.expect("validated above")`). Worse, `backup::delete_backup` does not panic and does not
  validate: it `remove_file`s whatever `dir.join(file)` resolves to, so a caller that forwards
  an unchecked name gets arbitrary-file deletion (`backup delete ../../anything`). The CLI and
  launcher both run the name check, so neither can reach it — but anyone consuming the
  *library* directly must validate at their own boundary. Hardening to typed errors is the
  correct long-term fix and was explicitly deferred.
- **Embedded registries can go silently stale.** Config/tuning/module registries are baked in at
  compile time; a user who pulls a newer `cli/` but keeps an older binary gets stale registry
  data with no error. Guarded by exact-count asserts and parity suites — which skip on boxes
  without the prereqs.
- **`install` is the one non-JSON command** (interactive stdio passthrough — the bash installers
  prompt, and NDJSON-wrapping them would deadlock the prompts). It passes the child installer's
  exit code through verbatim, so an installer exiting 2 is indistinguishable from the CLI's own
  "2 = usage error"; its bash-resolution preflight approximates `CreateProcess` search (no
  `PATHEXT`) but fails closed.
- **BrokenPipe exits 0.** A consumer that closes the pipe early can observe exit 0 for a command
  that was mid-way through reporting an error (standard SIGPIPE-race semantics, deliberate).
- **Deliberately absent arms.** `tailscale`, the realmlist family, and `lan public-ip` are
  launcher-only; `party dismiss-all`/`preset-show`/`preset-import` and `gm return-home` were not
  ported. The bash spellings `config raw-read`/`raw-write`/`tuning-list` became `config read`/
  `write`/`tuning list`.
- **Two hints still speak bash-CLI syntax** (the empty `console` command hint and the
  `accountwide` validator messages cite `dml wow ... --json` forms that do not exist in this
  binary) — kept for byte-parity with the launcher path; wrong advice in a standalone CLI.
- **`games-remove` lost the bash oracle's pre-deletion targets list** (the brief forbade any
  disk probe before confirmation); a `--dry-run` is the deferred fix.
- **CRLF handling differs from Git-Bash bash on Windows**: the Rust writers preserve CRLF files
  byte-identically; bash under Git Bash flattens them to LF on any write. Rust matches the
  Linux/production semantics — the Git Bash behavior is the anomaly.
- **`lan` success paths are covered compositionally** (27 unit tests + the emit path), not
  end-to-end — the Rust side speaks the MySQL wire protocol directly, so the test stub that
  faked bash's SQL layer cannot fake it. One live exit-0 `lan` assertion is owed to the final
  live gate.

None of these is hidden in the code — each is recorded in the plan ledger or module docs, and
the per-command details live in `docs/cli-contract.md`. The pitch, in one sentence: the
portable, contract-stable, per-game CLI the 2026-07-23 direction asked for already exists for
WoW, it is verified against the bash CLI it replaces, and the shared core it stands on is the
concrete starting point for game number two.
