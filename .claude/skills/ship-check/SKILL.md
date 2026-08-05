---
name: ship-check
description: Use when about to declare work done, before committing a multi-surface change, or when the user types /ship-check — runs the repo's full verification battery in the one safe order.
---

# Ship-check — the full battery, in the one safe order

Run the steps SEQUENTIALLY and report each result as it lands. Authority: the CLAUDE.md files — if this skill and CLAUDE.md ever disagree, CLAUDE.md wins and this file must be updated.

## Steps (skip a step only if its surface is untouched — and say so when skipping)

1. **cli/src changed → rebuild the artifact**: `bash cli/build.sh`. `cli/dml` is a committed build artifact — stage it with the src change. If the config/tuning registry arms in `40-config.sh` or the module catalog in `70-modules.sh` changed, regenerate `crates/dml-wow/data/*.json` from the bash oracle (procedure in the `crates/dml-wow/src/registry.rs` header) — never hand-edit them.
2. **bats, in the distro, exit code to a file** (never judge by a piped tail — it reports tail's exit code):
   `wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bats tests/ > /tmp/bats-out.txt 2>&1; echo EXIT:$?; grep -c '^not ok' /tmp/bats-out.txt"`
   Pass = `EXIT:0` AND not-ok count 0 AND the plan-line total has not dropped sharply (whole files failing to load also looks green).
3. **Rust workspace, from the REPO ROOT, only after bats has fully finished**: `cargo test --workspace` (prepend `%USERPROFILE%\.cargo\bin` if cargo is missing from PATH). Pass = exit 0, every summary line `0 failed`. NB the parity suites SKIP offline — a green run does NOT prove bash↔Rust parity; that is step 6's job.
4. **Frontend, from `launcher/`**: `npm test` (vitest), then `npm run check` (svelte-check; pass = 0 errors / 0 warnings).
5. **If cli/ changed**: `powershell -File cli\tests\windows-smoke.ps1`. **If the installer or guides changed**: `powershell -File guides\DML-Windows\tests\Test-InstallerDefender.ps1` and `Test-InstallerNative.ps1`.
6. **If cli/src or the mirrored Rust surface changed — the live parity gate** (needs the snapshot server at `C:\Users\perzi\dml-native`; start the stack first with `cargo run -p dml-wow-cli -- start`, `status` verdict `online` is enough):
   `$env:DML_GAMES_DIR="C:\Users\perzi\dml-native"; $env:DML_YQ_BIN="C:\Users\perzi\dml-native\tools\yq.exe"; cargo test -p dml-wow --tests -- --nocapture`
   Pass = all 18 parity suites RUN with ZERO lines containing SKIP, 0 failed. `--nocapture` is mandatory — cargo swallows the skip announcements of passing tests.
7. **Close-out**: `--json` surface changed → `docs/cli-contract.md` + `cli/README.md` updated, and the TermEvent union in `launcher/src/lib/api.ts` still matches `cli/src/10-json.sh` emitters. Then `git status` — artifact staged, no new `??` plan/doc file left untracked.

## Hard rules (each one is a recorded incident)

- NEVER overlap step 2 with step 3 or step 6: every bats `setup()` rewrites `cli/dml` in place while the parity suites spawn it as their oracle.
- NEVER run tauri dev during step 3 (same `target/` lock, double the RAM peak).
- The `#[ignore]`d bounded-call pin runs STANDALONE only: `cargo test -p dml-core --lib -- --ignored` — never folded into the workspace run.
- Counts drift; the EXIT CODE is the authority. Healthy snapshot (2026-08-05): cargo 1427, bats 813, vitest 603, svelte-check 0/0, installer 128, parity 693/2-ignored, workspace wall clock ≈91s — a climb back toward 260s is itself a regression signal.
