# dads-mmo-lab — Claude Code notes

Dad's MMO Lab: self-hosted MMO private-server tooling (WoW WotLK via AzerothCore + mod-playerbots, and other titles) targeting WSL2 on Windows. Fork of the upstream dads-mmo-lab repo (remotes: `origin` = pjerra fork, `upstream`). License AGPL-3.0.

## Current work: DML Launcher (branch `feat/dml-launcher-windows`)

Open-source cross-platform GUI (Tauri 2, Windows-first) replacing the closed-source "The Lab". Spec: `docs/superpowers/specs/2026-07-14-dml-launcher-windows-design.md`. Plans in `docs/superpowers/plans/`:
- Plan 1 (dml CLI JSON foundation) — **complete** on this branch
- Plan 2 (launcher shell, Tauri 2 + Svelte 5) / Plan 3 (WoW SOAP+MySQL features) — written, pending execution
- Only ONE controller session may execute a plan on this checkout at a time (a Task-6 double-dispatch already happened once; check `.superpowers/sdd/progress.md` and `git log` before dispatching anything).

## cli/ — the dml CLI (Plan 1 output)

- `cli/dml` is a **committed build artifact**: `bash cli/build.sh` concatenates `cli/src/*.sh` in glob order (00-head, 10-json, 90-main). NEVER edit `cli/dml` directly; edit `cli/src/*.sh` and rebuild.
- Runs inside WSL2 distro `dml-arch` as user `dml`; games in `$HOME/games` (override for tests: `DML_GAMES_DIR`). Repo inside WSL: `/mnt/c/Users/perzi/dads-mmo-lab`.
- `--json` contract (envelopes + NDJSON streams, error codes) is documented in `cli/README.md` — the Tauri GUI and Plan 2+ build against it; changing it is a breaking change.
- Tests: `wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bats tests/"` (bats + jq installed in the distro; jq is test-only, never a runtime dep). Windows call-path smoke: `powershell -File cli\tests\windows-smoke.ps1` (also dev-installs the built CLI into the distro via `cli/dev-install.ps1`).
- Test harness `cli/tests/helpers/env.bash` stubs `docker` (env: `DML_STUB_RUNNING`, `DML_STUB_DOCKER_DOWN`, `DML_STUB_COMPOSE_EXIT`) and the port-scan tool so tests never touch the real host.
- `guides/DML-Windows/Install-DML.ps1` still embeds the OLD CLI v2.6.0 as bootstrap (here-string, lines ~836–1633) — do NOT edit it casually; installer↔cli sync is a dedicated later plan (must bump `$ExpectedCliVersion` at ~line 813 together with the embedded script).
- `set -euo pipefail` is load-bearing in the built CLI (`${1:?}` usage, `_stream_cmd` pipefail exit-code propagation).

## Gotchas

- **Line endings:** bash inside WSL chokes on CRLF. `.gitattributes` forces LF for `*.sh`, `*.bats`, `*.bash`, and `cli/dml`; keep new shell files LF.
- **PowerShell 5.1 + UTF-8:** BOM-less `.ps1` files containing non-ASCII (em dashes) mis-parse under the ANSI codepage. `cli/tests/windows-smoke.ps1` carries a UTF-8 BOM on purpose; keep BOMs on any non-ASCII `.ps1`.
- **Rust toolchain PATH:** Rust 1.97 MSVC is installed per-user but `%USERPROFILE%\.cargo\bin` may be missing from a fresh Claude shell's PATH — use the full path or prepend it per call. MSVC Build Tools 2022 (C++ workload) and Node 22 are installed; WebView2 present.
- The `games` namespace is JSON-first: `games list`/`status` always emit JSON; only `games start|stop|restart` have a text mode. Legacy top-level commands (`list`, `status`, `start`, …) keep their exact text output — the old C# tray parses it.
- `dml-start.sh` hook: `games start|restart` run `<compose_dir>/dml-start.sh <mode>` when present+executable (avoids re-running ac-db-import on restarts).

## SDD bookkeeping

`.superpowers/sdd/` (untracked) holds the progress ledger, task briefs/reports, and review packages for plan execution. Ledger: `.superpowers/sdd/progress.md`.
