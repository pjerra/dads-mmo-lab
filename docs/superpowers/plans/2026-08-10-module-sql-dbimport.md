# Round 5.9 — Module SQL via db-import Rebuild Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The native rebuild builds `ac-db-import` alongside `ac-worldserver` so newly installed modules' SQL auto-applies on the next start, plus a post-rebuild advisory naming modules whose SQL is still unledgered.

**Architecture:** One argv change in `modmgr::module_rebuild_stream_with`'s build step; a pure disk-vs-ledger classification with best-effort tri-state wiring after the up step. Native-only — WSL composes carry `build:` for all services, so bash already rebuilds db-import (recorded one-sided exception).

**Tech Stack:** Rust (dml-wow). No bash, no launcher changes.

**Spec:** `docs/superpowers/specs/2026-08-10-module-sql-dbimport-design.md`

## Global Constraints

- Branch: `feat/core-family`. NO merge to `main`.
- Native-only round: `cli/src`, `cli/dml`, `launcher/src` must NOT change. No bats run needed (state this in reports); never run bats and cargo together regardless.
- The advisory NEVER changes the rebuild outcome: DB unreadable → one warn `could not read the update ledger -- skipping the module-SQL check.` and skip; per-module copy exactly `<key>: <N> SQL file(s) not yet applied by the updater -- they land on the next rebuild + restart.`
- MySQL stays read-only: the advisory issues SELECTs only (`SELECT name FROM updates`), through the existing `crate::db` reader with resolved schema names — never a guessed literal, never a write.
- cargo at `%USERPROFILE%\.cargo\bin\cargo.exe`; targeted runs only until the final battery (`--no-fail-fast` + clean-env comparison there).
- Commits end with the standard trailer:
```
Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_014ZA3WP4yd4R9BFBRrfSBY5
```

---

### Task 1: Build step gains the `ac-db-import` target

**Files:**
- Modify: `crates/dml-wow/src/modmgr.rs:2403-2405` (build argv) and its test module (assertions ~3059-3092)

**Interfaces:**
- Consumes: the existing fake-docker harness (`write_fake_rebuild_docker` — its matchers key on the substring `build ac-worldserver`, which the new argv still contains, so the fake scripts need NO change; verify this, don't assume).
- Produces: the build call argv `compose <(-f set)> build ac-worldserver ac-db-import`.

- [ ] **Step 1: Update the tests to demand the new argv (failing first).** In `rebuild_builds_through_the_overlay_then_ups_without_it` (and any sibling asserting the build line), change the terminal assertion from `ends_with("build ac-worldserver")` to `ends_with("build ac-worldserver ac-db-import")`. Add to the same test: the build line contains BOTH target names and the `up -d` line contains NEITHER (`!up_line.contains("ac-db-import")` guards against the targets leaking into up).
- [ ] **Step 2: Run to verify failure** — `cargo test -p dml-wow modmgr::tests::rebuild -- --nocapture` → the changed test FAILS on the old argv.
- [ ] **Step 3: Implement** — `modmgr.rs:2405`:

```rust
    build_args.extend(["build", "ac-worldserver", "ac-db-import"]);
```

Update the line comment above it: db-import must rebuild WITH the worldserver, or the updater keeps serving install-time module SQL (spec 2026-08-10; the VM ledger evidence).

- [ ] **Step 4: Run to verify pass** — same command; all rebuild tests green (the up-failure and refusal tests are argv-agnostic but confirm).
- [ ] **Step 5: Mutation check** — remove `"ac-db-import"` from the argv, the Step-1 test MUST go red alone; restore. Record in the commit message.
- [ ] **Step 6: Commit**

```bash
git add crates/dml-wow/src/modmgr.rs
git commit -m "feat(modmgr): rebuild builds ac-db-import so module SQL auto-applies

Mutation-verified: dropping the target reds the argv test."
```

---

### Task 2: Post-rebuild unledgered-SQL advisory

**Files:**
- Modify: `crates/dml-wow/src/modmgr.rs` (two pure functions + wiring after the up step) and its test module

**Interfaces:**
- Consumes: `crate::db::{query, Database, DbConfig}` (`query(cfg, db, sql) -> Result<QueryResult, DbError>` at `db.rs:461` — check how existing callers iterate rows and how the `Database` enum spells its variants, including the optional playerbots one, before writing code). The rebuild stream already holds `db_cfg`.
- Produces:
  - `pub fn module_sql_files(sdir: &Path) -> Vec<(Database, String, String)>` — (target db, module key, sql filename) for every `modules/<key>/data/sql/<dbdir>/updates/*.sql`, where `<dbdir>` maps: `db-world`|`db_world`→World, `db-characters`|`db_characters`→Characters, `db-auth`|`db_auth`→Auth, `playerbots`→Playerbots. Non-`.sql` files and other dirs ignored; a module with no sql dir contributes nothing.
  - `pub fn unledgered_modules(files: &[(Database, String, String)], ledgered: &dyn Fn(Database, &str) -> Option<bool>) -> Option<Vec<(String, usize)>>` — per-module missing counts, sorted by key; the callback answers "is this filename in that db's ledger" with `None` = could not tell. ANY `None` makes the whole answer `None` (tri-state: a half-read ledger must not accuse modules). Design the exact callback shape to fit how `query` results read — a prefetched `HashMap<Database, HashSet<String>>` wrapped in a closure is fine.
- Wiring in `module_rebuild_stream_with`, AFTER the successful up and `rebuild_pending_clear`, BEFORE the `done` event: prefetch `SELECT name FROM updates` for each Database the scanned files reference (skip playerbots when the schema is unresolved — the existing resolved-names seam); any query error → emit warn `could not read the update ledger -- skipping the module-SQL check.` and proceed to `done`. Otherwise emit one warn per module: `<key>: <N> SQL file(s) not yet applied by the updater -- they land on the next rebuild + restart.` Zero missing → no lines.

- [ ] **Step 1: Write the failing pure-function tests:**

```rust
    #[test]
    fn module_sql_files_scans_both_dir_spellings_and_ignores_noise() {
        // tempdir server: modules/mod-a/data/sql/db-world/updates/{one.sql, notes.txt}
        // modules/mod-b/data/sql/db_characters/updates/two.sql
        // modules/mod-c/ (no sql dir), modules/mod-d/data/sql/playerbots/updates/three.sql
        // Expect exactly three (Database, key, file) rows; notes.txt absent.
    }

    #[test]
    fn unledgered_modules_counts_only_missing_files_per_module() {
        // files: mod-a/one.sql (World), mod-a/two.sql (World), mod-b/three.sql (Characters)
        // ledger closure: one.sql known, others unknown-but-answerable (Some(false))
        // Expect [("mod-a", 1), ("mod-b", 1)].
    }

    #[test]
    fn a_ledger_that_cannot_answer_yields_none_not_accusations() {
        // closure returns None for one lookup → whole result None.
    }
```

- [ ] **Step 2: Run to verify failure** — `cargo test -p dml-wow modmgr::tests::module_sql -- --nocapture` → compile errors.
- [ ] **Step 3: Implement the pure functions**, then the wiring per Interfaces. Keep the SELECT literal `SELECT name FROM updates` executed via `db::query` per Database — the schema comes from the reader's resolved names (verify how `Database` maps to schema internally; do NOT splice schema strings into the SQL yourself).
- [ ] **Step 4: Extend the existing happy-path rebuild test**: the fake harness has no live DB, so the queries error and the run must emit the skip-warn (assert the warn line) while the `done {"rebuilt":true}` event is UNCHANGED — this pins tri-state degradation on the default path for free.
- [ ] **Step 5: Run to verify pass** — `cargo test -p dml-wow modmgr -- --nocapture` all green.
- [ ] **Step 6: Mutation check** — make the wiring treat a query error as "no modules missing" (skip the warn): the Step-4 assertion MUST go red; restore. Record in the commit.
- [ ] **Step 7: Commit**

```bash
git add crates/dml-wow/src/modmgr.rs
git commit -m "feat(modmgr): post-rebuild advisory names modules with unledgered SQL"
```

---

### Task 3: Docs + verification battery

**Files:**
- Modify: `docs/cli-contract.md`, `cli/README.md` (rebuild description: second build target + advisory vocabulary — scope to the NATIVE arm, bash unchanged), `crates/CLAUDE.md` (one-liner: why db-import rebuilds, VM ledger evidence), `docs/superpowers/plans/2026-07-20-post-smoke-roadmap.md` (Round 5.9 → built, live gate pending)

- [ ] **Step 1: Make the doc edits** (byte-copy the advisory strings from Global Constraints).
- [ ] **Step 2: Battery** — `env -u DML_GAMES_DIR -u DML_BACKEND -u DML_SCRIPT -u DML_YQ_BIN cargo test --workspace --no-fail-fast` (sum every `test result:`; zero failures), then the same WITH ambient env — totals must match. No bats (no bash change), no vitest (no launcher change) — say so in the report.
- [ ] **Step 3: Commit**

```bash
git add docs/cli-contract.md cli/README.md crates/CLAUDE.md docs/superpowers/plans/2026-07-20-post-smoke-roadmap.md
git commit -m "docs: round 5.9 -- db-import rebuild + unledgered-SQL advisory"
```

---

## User live gate (after all tasks — recorded rollout step from the spec)

Supervised first rebuild on the VM: update the launcher build there, click Rebuild server, watch db-import's run on the next start apply the nine modules' SQL backlog and ledger it. Any non-re-runnable hand-applied file stops db-import with the filename — resolve on the spot (expected zero to two). Then the advisory should report nothing.
