---
name: ship-check
description: Use when about to declare work done, before committing a multi-surface change, or when the user types /ship-check — runs the repo's full verification battery in the one safe order.
---

# Ship-check — the full battery, in the one safe order

Run the steps SEQUENTIALLY and report each result as it lands. Authority: the repo's `CLAUDE.md` files — if this skill and `CLAUDE.md` ever disagree, `CLAUDE.md` wins and this file must be updated.

## Steps (skip a step only if its surface is untouched — and say so when skipping)

1. **`cli/src` changed → rebuild the artifact**: `bash cli/build.sh`. `cli/dml` is a committed build artifact — stage it with the src change. If the config/tuning registry arms in `cli/src/40-config.sh` or the module catalog arm (`catalog)` in `cli/src/90-main.sh`) changed, regenerate `crates/dml-wow/data/{config-registry,tuning-registry,module-catalog}.json` from the bash oracle — the exact three commands are in the `crates/dml-wow/src/registry.rs` header. Never hand-edit them.

2. **bats, in the distro, exit code to a file** (never judge by a piped tail — it reports tail's exit code). Two calls, and the inner script must be SINGLE-quoted or the outer shell eats `$?`:
   ```
   wsl -d dml-arch -u dml -- bash -lc 'cd /mnt/c/Users/perzi/dads-mmo-lab && bats cli/tests/ > /tmp/bats.out 2>&1; echo EXIT=$?'
   wsl -d dml-arch -u dml -- bash -lc 'head -1 /tmp/bats.out; grep -c "^not ok" /tmp/bats.out'
   ```
   Pass = `EXIT=0` AND not-ok count 0 AND the plan line still reads about `1..840` — a whole file failing to load also looks green, and only the plan line catches it. `bats` and `jq` are NOT on the Git Bash PATH on this machine; they exist only inside the distro.

3. **Rust workspace, from the REPO ROOT, only after bats has fully finished**: `cargo test --workspace` (prepend `$HOME/.cargo/bin` to PATH if cargo is missing). Pass = exit 0, every summary line `0 failed`.
   Two traps:
   - **Cargo ABORTS the remaining test binaries after the first failing one.** A red run reports FEWER binaries than a green one (observed: 20 of 29). Never compare a red run's totals to the baseline and conclude tests vanished.
   - **If `DML_GAMES_DIR` points at a games dir whose server is UP, the parity suites in this run go LIVE instead of skipping**, and step 6 has effectively already run — several suites at once against one SOAP endpoint. A green workspace run with the server DOWN does NOT prove bash↔Rust parity; that is step 6's job.

4. **Frontend, from `launcher/`**: `npm test` (vitest), then `npm run check` (svelte-check; pass = 0 errors / 0 warnings).

5. **If `cli/` changed**: `powershell -NoProfile -File cli\tests\windows-smoke.ps1` (pass = a `SMOKE OK - N game(s)` line). **If the installer or guides changed**: `powershell -NoProfile -File guides\DML-Windows\tests\Test-InstallerDefender.ps1` and `Test-InstallerNative.ps1`.

6. **If `cli/src` or the mirrored Rust surface changed — the live parity gate** (needs the snapshot server at `C:\Users\perzi\dml-native`; start the stack first with `cargo run -p dml-wow-cli -- start`, `status` verdict `online` is enough):
   ```
   $env:DML_GAMES_DIR="C:\Users\perzi\dml-native"; $env:DML_YQ_BIN="C:\Users\perzi\dml-native\tools\yq.exe"
   cargo test -p dml-wow --tests -- --nocapture
   ```
   Pass = all 18 `crates/dml-wow/tests/*_parity.rs` suites RUN with ZERO lines containing `SKIP`, 0 failed. `--nocapture` is mandatory — cargo swallows the skip announcements of passing tests, and a parity suite that skips still reports `ok`.

7. **Close-out**: `--json` surface changed → `docs/cli-contract.md` + `cli/README.md` updated, and the `TermEvent` union in `launcher/src/lib/api.ts` still matches `cli/src/10-json.sh`'s emitters. Then `git status` — artifact staged, no new `??` plan/doc file left untracked.

## Hard rules (each one is a recorded incident)

- NEVER overlap step 2 with step 3 or step 6: every bats `setup()` runs `bash cli/build.sh`, which REWRITES `cli/dml` in place, while the parity suites spawn `bash cli/dml` as their oracle.
- NEVER run `tauri dev` during step 3 (same `target/` lock, double the RAM peak).
- The `#[ignore]`d tests are LIVE gates, not part of the battery: `account_write.rs` and `srp6.rs` write/read a live `acore_auth`, `install_native.rs` needs a running container, `part5a_parity.rs` has two, and `engine.rs`'s one is flaky by construction (its deterministic replacement is `abandon_never_blocks_the_caller_on_the_reap` in `proc.rs`). Never fold `--ignored` into the workspace run.
- Counts drift; the EXIT CODE is the authority. **Measured baseline on `rust-main`, 2026-08-06, server DOWN:**

  | suite | command | result |
  |---|---|---|
  | cargo workspace | `cargo test --workspace` | exit 0 — 29 binaries, **1667 passed, 0 failed, 7 ignored**, ≈92s wall |
  | dml-wow tests only | `cargo test -p dml-wow --tests` | exit 0 — 21 binaries, **1087 passed, 0 failed, 5 ignored** |
  | bats | see step 2 | exit 0 — plan `1..840`, **840 ok, 0 not ok, 0 skipped** |
  | vitest | `npm test` | exit 0 — **63 files, 772 tests** |
  | svelte-check | `npm run check` | **333 files, 0 errors, 0 warnings** |
  | installer (native) | `Test-InstallerNative.ps1` | **83 checks, 0 failures** |
  | installer (defender) | `Test-InstallerDefender.ps1` | **145 checks passed** |

  A workspace wall clock climbing back toward 260s is itself a regression signal. The `launcher_lib` binary's count moves whenever `launcher/src-tauri/src/lib.rs` is edited — treat a small delta there as expected, not as a missing suite.
