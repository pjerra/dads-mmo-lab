# Modules-Page Round Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **Execution harness for THIS plan (user directive 2026-08-11):** one git
> worktree + one Workflow, **5 agents max, strictly sequential** (Task 1→5,
> one agent per task). The workflow script lives at
> `.claude/workflows/modules-page-round.js` and the worktree at
> `C:/Users/perzi/dml-desks/modules-round` (branch
> `feat/modules-page-round` off `rust-main`). The same worktree/workflow
> may be resumed from another session — but per the repo rule, **only ONE
> controller session may execute this plan at a time**; check
> `.superpowers/sdd/progress.md` and `git -C <worktree> log` before
> dispatching anything.

**Goal:** Ship the six-item Modules-page batch: auto-conf activation, installed-first layout, click-to-open config, catalog-driven setup notices, and the four update-honesty fixes with Rust pull-path tests.

**Architecture:** CLI behaviour changes (auto-conf, update honesty) land on both surfaces (bash `cli/` + Rust `crates/dml-wow`) with the shared helper extracted into `dml-wow::moduletail` so the launcher shell and the install arm use ONE resolver. Layout, click routing and setup notices are launcher-only (Svelte 5 runes + pure helpers pinned by vitest).

**Tech Stack:** Rust (cargo workspace), bash (bats in dml-arch), Svelte 5 + vitest, Tauri 2 commands in `launcher/src-tauri/src/lib.rs`.

## Global Constraints

- Mirror rule: any CLI-visible behaviour change lands in BOTH `cli/src/*.sh` and `crates/dml-wow`, tested on both. After editing `cli/src/*.sh`, run `bash cli/build.sh` and COMMIT the regenerated `cli/dml` (tracked build artifact).
- Never run bats and cargo tests concurrently (bats `setup()` rewrites `cli/dml` which cargo parity suites spawn). Run bats via: `wsl -d dml-arch -u dml --exec bash -lc 'cd <worktree-as-/mnt/...> && bats cli/tests/ > /tmp/b.out 2>&1; echo EXIT=$?'` then read counts from the file — never judge by piped tail.
- Bash tests: a mid-test `! cmd` asserts NOTHING — use `run cmd` + status checks or `[ "$(grep -c …)" = 0 ]`.
- Mutation proofs: after writing each named test, break the production code as specified, watch the exact test go RED, restore with the Edit tool (never `git checkout` — autocrlf; never rebuild Rust char literals through bash heredocs).
- Shell files are LF (`.gitattributes` enforces; keep new ones LF).
- Streamed UI outcomes derive from done/error EVENTS, never promise resolution.
- New/changed JSON fields are ADDITIVE ONLY.
- Baselines at branch point (remeasure before quoting): cargo 30 targets 1747/0; vitest 782/782; bats 865/0.

**Two recorded refinements vs the spec** (both honest deviations, flag at review):
1. Setup metadata lives in a LAUNCHER data module (`launcher/src/lib/setup-catalog.ts`), not `module-catalog.json` + both list arms. Reason: bash's registry is a hand-mirrored heredoc — piping rich nested metadata through two list arms duplicates data three times for a pure-UI feature. The launcher file is still "edit data, not code".
2. "Mark as done" persists in `localStorage` keyed by server dir (v1), not `launcher.json` — avoids a new Tauri command surface for a dismissal flag.

---

### Task 1: Auto-conf activation on install (both surfaces) — model: opus

**Files:**
- Modify: `crates/dml-wow/src/moduletail.rs` (add `conf_activate` core)
- Modify: `crates/dml-wow/src/modmgr.rs` (`install_cpp` calls it after clone success)
- Modify: `launcher/src-tauri/src/lib.rs:1033-1070` (`wow_module_conf_activate_native` delegates to the new core — delete its inline copy)
- Modify: `cli/src/70-modules.sh` (add `_module_conf_auto_activate`; install cpp path calls it)
- Modify: `cli/src/90-main.sh` (`conf-activate)` arm delegates to the same helper for the copy step)
- Rebuild + commit: `cli/dml`
- Test: `crates/dml-wow/src/moduletail.rs` unit tests, `cli/tests/wow-module-cpp.bats` additions

**Interfaces:**
- Produces (Rust): `pub enum ConfActivateOutcome { Activated(&'static str), AlreadyActive, NoDistYet, NoConf }` and `pub fn conf_activate(sdir: &Path, key: &str, force: bool) -> std::io::Result<ConfActivateOutcome>` in `moduletail.rs`. Copies `module_conf_dist_path()` → `env/dist/etc/modules/<conf_name>`, creating parents. `force=false` + existing active file → `AlreadyActive` (NOT an error). Task 4's UI catch-up relies on the launcher command's existing JSON shape being unchanged.
- Produces (bash): `_module_conf_auto_activate <sdir> <mkey>` → echoes the conf name on activation, empty otherwise; exit 0 always (advisory).

- [ ] **Step 1: Failing Rust tests** — in `moduletail.rs` tests module, tempdir fixtures (follow `module_conf_state_ready_when_dist_present_but_not_active` at `moduletail.rs:687` for the fixture shape):

```rust
#[test]
fn a_conf_activate_copies_dist_when_ready() {
    let t = tempfile::tempdir().unwrap();
    let sdir = t.path();
    let dist = sdir.join("modules/mod-ahbot/conf");
    std::fs::create_dir_all(&dist).unwrap();
    std::fs::write(dist.join("mod_ahbot.conf.dist"), "Key = 1\n").unwrap();
    let out = conf_activate(sdir, "mod-ahbot", false).unwrap();
    assert!(matches!(out, ConfActivateOutcome::Activated("mod_ahbot.conf")));
    let active = sdir.join("env/dist/etc/modules/mod_ahbot.conf");
    assert_eq!(std::fs::read_to_string(active).unwrap(), "Key = 1\n");
}

#[test]
fn a_conf_activate_never_overwrites_an_existing_conf() {
    let t = tempfile::tempdir().unwrap();
    let sdir = t.path();
    std::fs::create_dir_all(sdir.join("modules/mod-ahbot/conf")).unwrap();
    std::fs::write(sdir.join("modules/mod-ahbot/conf/mod_ahbot.conf.dist"), "DEFAULT\n").unwrap();
    std::fs::create_dir_all(sdir.join("env/dist/etc/modules")).unwrap();
    std::fs::write(sdir.join("env/dist/etc/modules/mod_ahbot.conf"), "USER EDIT\n").unwrap();
    let out = conf_activate(sdir, "mod-ahbot", false).unwrap();
    assert!(matches!(out, ConfActivateOutcome::AlreadyActive));
    assert_eq!(
        std::fs::read_to_string(sdir.join("env/dist/etc/modules/mod_ahbot.conf")).unwrap(),
        "USER EDIT\n"
    );
}

#[test]
fn a_conf_activate_no_dist_is_a_quiet_outcome_not_an_error() {
    let t = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(t.path().join("modules/mod-ahbot")).unwrap();
    let out = conf_activate(t.path(), "mod-ahbot", false).unwrap();
    assert!(matches!(out, ConfActivateOutcome::NoDistYet));
}
```

- [ ] **Step 2: Run, verify FAIL** — `cargo test -p dml-wow a_conf_activate` → compile error (function undefined).
- [ ] **Step 3: Implement `conf_activate` in `moduletail.rs`** using `module_conf_name` / `module_conf_dist_path` (both already there). Match the launcher's current copy semantics exactly (`lib.rs:1043-1066`): same active path, `create_dir_all` parent, `fs::copy`.
- [ ] **Step 4: Run, verify PASS.**
- [ ] **Step 5: Delegate the launcher command** — `wow_module_conf_activate_native` keeps its error contract (`NO_CONF` / `EXISTS` when `!force` / `NEEDS_REBUILD`) but maps them FROM `ConfActivateOutcome` (`NoConf`→NO_CONF, `AlreadyActive`→EXISTS, `NoDistYet`→NEEDS_REBUILD). Delete the inline copy logic. `cargo test -p dml-wow` + `cargo build -p launcher` must stay green.
- [ ] **Step 6: Hook `install_cpp`** — in `modmgr.rs::install_cpp`, immediately after the clone/marker success path (before the final done emit), call `conf_activate(sdir, &mkey, false)`; on `Activated(name)` emit `note_event(format!("Activated {name} with defaults — tune it on the Modules page."))`; every other outcome (incl. `Err`) emits nothing (install must not fail on a conf nicety). Add test asserting the note by driving `install_cpp` with the existing fake-git test setup in `modmgr.rs` tests (search `install_cpp` in the tests module for the fixture; plant a `conf/<name>.conf.dist` in the fake clone source so the clone carries it).
- [ ] **Step 7: Mutation proof** — invert the `AlreadyActive` guard (make it overwrite): `a_conf_activate_never_overwrites_an_existing_conf` must go RED. Restore via Edit.
- [ ] **Step 8: Bash mirror** — in `70-modules.sh` add:

```bash
# Auto-activate a cpp module's conf after install: copy the clone's
# .conf.dist to the active dir IFF no active conf exists. Advisory --
# never fails the install; echoes the conf name when it activated.
_module_conf_auto_activate() {
  local sdir="$1" mkey="$2" cname dist active
  cname="$(_module_conf_name "$mkey")"
  [[ -z "$cname" ]] && return 0
  active="$sdir/env/dist/etc/modules/$cname"
  [[ -f "$active" ]] && return 0
  dist="$(_module_conf_dist "$sdir" "$mkey")"
  [[ -z "$dist" ]] && return 0
  mkdir -p "$(dirname "$active")" && cp "$dist" "$active" && printf '%s' "$cname"
}
```

Call it in the cpp install path after the clone succeeds; when it echoes a name, emit the same note line as Rust (`json_note "Activated <name> with defaults — tune it on the Modules page."` — match the file's existing note emitter, grep `json_note\|_note_event` in `10-json.sh` and use that exact function). The `conf-activate)` arm in `90-main.sh` keeps its refusals but uses `cp` via the same helper semantics (leave the arm's copy in place if extraction would disturb its EXISTS/--force contract — the arm predates this and its behaviour must not change).
- [ ] **Step 9: bats** — in `cli/tests/wow-module-cpp.bats` (existing install fixtures) add: (a) install of a module whose stub clone carries `conf/<name>.conf.dist` → active conf file exists afterwards + note line present in the NDJSON; (b) pre-existing active conf with user content → byte-identical after install. Follow the file's existing stub-git pattern.
- [ ] **Step 10: Rebuild artifact + full check** — `bash cli/build.sh`, run bats (file-redirect form), `cargo test -p dml-wow`. Then commit: `git add -A && git commit -m "feat(modules): install activates the module conf itself (both surfaces)"`.

### Task 2: Update honesty — 4 audit defects + Rust pull-path tests (both surfaces) — model: opus

**Files:**
- Modify: `crates/dml-wow/src/modmgr.rs` — `module_update_stream_with` (~line 2207), `wow_pull_repo` (~1448), `git_diff_capture` (~376), the advisory emits (~1620 install, ~1992-2004 update), `rebuild_pending_add` call site (~1999)
- Modify: `cli/src/70-modules.sh` (`_wow_pull_repo` ~948: patch capture), `cli/src/90-main.sh` (update arm ~6369-6462: advisory + pending honesty)
- Rebuild + commit: `cli/dml`
- Create: `crates/dml-wow/tests/module_update_pull.rs`
- Test: additions to `cli/tests/wow-module-update.bats`

**Interfaces:**
- Consumes: nothing from Task 1 (different functions in the same files — this is why Tasks 1→2 are SEQUENTIAL, same-file edits).
- Produces: the update done payload keeps its shape `{key, changed, before, after, pending_rebuild}`; `pending_rebuild` now reflects the marker write's real result. UI copy in `module-tabs.ts::updateDoneNote` needs no change (it already branches on `pending_rebuild`).

- [ ] **Step 1: Fix (c) staged-edits patch, Rust** — `git_diff_capture` switches its invocation to `git diff --binary HEAD` (captures staged + unstaged vs HEAD). Failing test first, in the new `crates/dml-wow/tests/module_update_pull.rs` — real git, no stubs (shape mirrors `cli/tests/wow-module-update.bats:38`'s bare-origin fixture):

```rust
// Helper used by every test in this file.
fn fixture() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let t = tempfile::tempdir().unwrap();
    let origin = t.path().join("origin.git");
    let sdir = t.path().join("server");
    let clone = sdir.join("modules/mod-fixture");
    std::fs::create_dir_all(&sdir).unwrap();
    let run = |dir: &std::path::Path, args: &[&str]| {
        let ok = std::process::Command::new("git").args(args).current_dir(dir)
            .env("GIT_AUTHOR_NAME", "t").env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t").env("GIT_COMMITTER_EMAIL", "t@t")
            .status().unwrap().success();
        assert!(ok, "git {args:?} failed");
    };
    run(t.path(), &["init", "--bare", "origin.git"]);
    run(t.path(), &["clone", origin.to_str().unwrap(), clone.to_str().unwrap()]);
    std::fs::write(clone.join("f.txt"), "v1\n").unwrap();
    run(&clone, &["add", "."]);
    run(&clone, &["commit", "-m", "v1"]);
    run(&clone, &["push", "origin", "HEAD"]);
    (t, sdir, clone)
}

#[test]
fn b_staged_edits_land_in_the_backup_patch() {
    let (_t, _sdir, clone) = fixture();
    std::fs::write(clone.join("f.txt"), "STAGED EDIT\n").unwrap();
    std::process::Command::new("git").args(["add", "."]).current_dir(&clone).status().unwrap();
    let patch = dml_wow::modmgr::git_diff_capture(std::ffi::OsStr::new("git"), &clone).unwrap();
    assert!(patch.contains("STAGED EDIT"), "staged edit missing from patch: {patch}");
}
```

(If `git_diff_capture` is private, make it `pub(crate)` visible to an integration test via a `#[doc(hidden)] pub` re-export, or move the test into the unit-test module — implementer's choice, note it in the report.)
- [ ] **Step 2: Run → FAIL (patch is empty). Implement (`--binary HEAD`). Run → PASS.** Mirror in bash: `70-modules.sh:955` `(cd … && git diff)` → `(cd … && git diff --binary HEAD)`. bats addition: dirty fixture stages the edit (`git add`), runs `module update`, asserts the patch file contains the edit (the existing dirty-worktree test at wow-module-update.bats covers unstaged; copy its shape).
- [ ] **Step 3: Fix (b) pending honesty, Rust** — at `modmgr.rs:1999` capture `let marked = rebuild_pending_add(sdir, &key).is_ok();` (currently `let _ =`, and NOT for mod-arac — keep the arac exemption, `marked=false` there). Done payload emits `"pending_rebuild": marked && changed` (today: unconditional true when changed && !arac). On `changed && !arac && !marked` emit a warn event naming the marker path (`.dml-rebuild-pending`) and saying the rebuild banner will not light — reuse the file's existing warn emitter. Test in `module_update_pull.rs`: make the marker unwritable (create `.dml-rebuild-pending` as a DIRECTORY in sdir — `rebuild_pending_add`'s file write then fails cross-platform), run the update flow, assert `pending_rebuild:false` + the warn present.
- [ ] **Step 4: Fix (a) advisory truthfulness, Rust** — the line "module SQL (if any) is applied automatically by the server's db-import on next start" (~2004 update, ~1620 install) is emitted ONLY when `marked` (update) / on install keep it only if the install path also marks pending rebuild (it does for cpp — verify by reading; if install never marks, the install line moves behind the same condition as its marker). mod-arac update instead emits: `"mod-arac is data-only: new SQL is NOT auto-applied — re-run Apply client patch / apply its SQL manually (Repair panel)."` Tests: fixture update with marker-ok → advisory present; arac-shaped (skip marker) → honest line present, old line ABSENT (assert both).
- [ ] **Step 5: Core pull-path coverage (d)** — same file, real-git tests mirroring bats: `b_changed_pull_marks_rebuild_pending` (origin gains a commit → update → `changed:true`, marker file exists, `pending_rebuild:true`); `b_up_to_date_pull_writes_no_marker`; `b_dirty_worktree_edit_survives_and_patch_written`. Drive through `module_update_stream_with` with a collected-events closure and the fake-docker guard forced to `Some(true)` (see the existing guard tests at `modmgr.rs:3655+` for how the fake is injected).
- [ ] **Step 6: Bash mirror of (a)+(b)** — update arm `90-main.sh:6369-6462`: capture `_rebuild_pending_add`'s exit, emit `pending_rebuild` accordingly + the warn on failure; advisory behind the same condition; arac honest line. bats: advisory-only-when-marked (make sdir's marker path a directory via the fixture), arac honest-line test. Remember: `run` + explicit status/greps, never mid-test `!`.
- [ ] **Step 7: Mutation proofs** — (i) revert `--binary HEAD` → `b_staged_edits_land_in_the_backup_patch` RED; (ii) hardcode `pending_rebuild: true` → the marker-unwritable test RED; (iii) unconditional advisory emit → the arac absence-assert RED. Restore each via Edit.
- [ ] **Step 8: Rebuild `cli/dml`, run cargo then bats (never concurrent), commit** — `git add -A && git commit -m "fix(modules): update honesty -- advisory, pending_rebuild, staged-edit patch + Rust pull tests"`.

### Task 3: Installed/Available layout split (launcher) — model: sonnet

**Files:**
- Create: `launcher/src/lib/module-split.ts`
- Create: `launcher/src/lib/module-split.test.ts`
- Modify: `launcher/src/lib/pages/ModuleManager.svelte` (the three family cards, lines ~627-963)

**Interfaces:**
- Produces:

```ts
export type LuaStatus = "installed" | "cloned" | "absent";
export function luaStatus(m: { cloned: boolean; deployed: boolean }): LuaStatus;
export function luaStatusLabel(s: LuaStatus): string; // "Installed" | "Cloned, not deployed" | "Not installed"
export function splitInstalled<T>(rows: T[], isInstalled: (m: T) => boolean): { installed: T[]; available: T[] };
export function defaultExpanded(installedCount: number): boolean; // true iff installedCount === 0
```

- Task 4 modifies the same component AFTER this task (sequential, same file).
- cpp `isInstalled` = `m.installed`; lua = `m.cloned || m.deployed`; sql = `m.installed`.

- [ ] **Step 1: Failing vitest** — `module-split.test.ts`: splitInstalled preserves order within halves and loses no rows; luaStatus truth table (deployed⇒installed regardless of cloned, cloned-only⇒cloned, neither⇒absent); defaultExpanded(0)=true / (1)=false.
- [ ] **Step 2: Run → FAIL. Implement `module-split.ts` (pure, no Svelte imports). Run → PASS.**
- [ ] **Step 3: Rework the three cards** in ModuleManager.svelte. Per family: `{@const parts = splitInstalled(list.families.cpp, (m) => m.installed)}`; render `<h4>Installed ({parts.installed.length})</h4>` + existing row markup for installed rows; then a toggle row (`<button class="avail-toggle">▸ Available ({parts.available.length})</button>`, per-family `$state` booleans initialised from `defaultExpanded`) guarding the available rows. Lua rows: replace the two badges with ONE `<span class="badge {…}">{luaStatusLabel(luaStatus(m))}</span>` (`installed`→on, `cloned`→warn, `absent`→off). Keep ALL existing buttons/panels (repair, place-NPC, fixit, ARAC, backup checkboxes) attached to their rows unchanged; ALE-note renders above the lua Available section. Do not touch the rebuild/update/client/cleanup cards.
- [ ] **Step 4: Init expansion from data** — expansion state must initialise AFTER the first list load (`$derived` of list presence with a one-shot `$effect` that runs when `list` first becomes non-null; do not re-collapse on refresh).
- [ ] **Step 5: `npm test -- --run` + `npm run check` green (vitest count grows; svelte-check 0 errors).** Manual sanity: `npm run tauri dev` is a USER gate, not this task's.
- [ ] **Step 6: Commit** — `git add -A && git commit -m "feat(launcher): modules page splits installed-first with collapsible catalogs"`.

### Task 4: Click-to-open + auto-conf catch-up (launcher) — model: sonnet

**Files:**
- Create: `launcher/src/lib/module-nav.svelte.ts`
- Create: `launcher/src/lib/module-nav.test.ts`
- Modify: `launcher/src/lib/pages/ModuleManager.svelte` (row click zones, catch-up pass, tab switch)
- Modify: `launcher/src/lib/ModuleTuning.svelte` (consume pending target: expand + scroll)
- Modify: `launcher/src/lib/ModuleFiles.svelte` (consume pending file preselect)

**Interfaces:**
- Consumes: Task 3's row markup; the existing `wowModuleConfActivate` API wrapper and the `conf` field (`"ready"`/`"active"`) on `CppModule`; the existing `tab` state in ModuleManager.
- Produces:

```ts
// module-nav.svelte.ts — one-shot cross-tab navigation targets.
export const moduleNav: { tuningKey: string | null; confFile: string | null };
export function requestTuning(key: string): void;
export function requestConfFile(name: string): void;
export function takeTuning(): string | null;   // returns and CLEARS
export function takeConfFile(): string | null; // returns and CLEARS
```

- [ ] **Step 1: Failing vitest** — `module-nav.test.ts`: request→take returns the value once, second take returns null; requestTuning does not disturb confFile and vice versa.
- [ ] **Step 2: Run → FAIL. Implement as a runes store (`$state` object, module scope — the `restart-state.svelte.ts` pattern). Run → PASS.**
- [ ] **Step 3: Click zones (installed rows only)** — module name becomes `<button class="mname-link" onclick={() => { requestTuning(m.key); tab = "tuning"; }}>` (cpp + lua installed rows); add a conf filename element on installed cpp rows with a known `conf_name`: `<button class="conf-link" onclick={() => { requestConfFile(m.conf_name); tab = "files"; }}>{m.conf_name}</button>`. Hover: underline. Uninstalled rows keep plain text.
- [ ] **Step 4: Consumers** — ModuleTuning: in its `active`-driven load path, `const k = takeTuning(); if (k) { expand that module's card; queueMicrotask(() => document.getElementById(`tune-${k}`)?.scrollIntoView({ block: "start" })); }` — give each server-module card `id="tune-{key}"` (read ModuleTuning's card markup first and reuse its existing expand state variable). ModuleFiles: on becoming active, `const f = takeConfFile(); if (f) select/load that file` via its existing file-selection function (read the component; call the same function its file list buttons call).
- [ ] **Step 5: Auto-conf catch-up** — in ModuleManager, after `refresh()` completes on mount (one-shot flag `catchupDone`), collect installed cpp rows with `m.conf === "ready"`; for each sequentially `await wowModuleConfActivate(m.key)` in a try/catch (an `EXISTS` race or `NEEDS_REBUILD` is swallowed — the catch-up is advisory); if any activated, set `note = "Activated default configs: <names>"` and refresh once more. Skip entirely while `busy`.
- [ ] **Step 6: `npm test -- --run` + `npm run check` green. Commit** — `git add -A && git commit -m "feat(launcher): click-to-open module tuner/conf + auto-conf catch-up"`.

### Task 5: Needs-setup notices (launcher) — model: sonnet

**Files:**
- Create: `launcher/src/lib/setup-catalog.ts`
- Create: `launcher/src/lib/setup-catalog.test.ts`
- Modify: `launcher/src/lib/pages/ModuleManager.svelte` (chip + Setup panel on installed rows)

**Interfaces:**
- Consumes: Task 4's `requestTuning`/`requestConfFile` + `tab` for setup actions; existing `wowModulePlaceNpc`, `wowModuleFixit`, `PLACE_NPC_KEYS` machinery.
- Produces:

```ts
export type SetupAction =
  | { type: "open-tuner"; key: string; label: string }
  | { type: "open-files"; file: string; label: string }
  | { type: "place-npc"; key: string; label: string }
  | { type: "fixit"; fix: "battlepass-npc"; label: string }
  | { type: "copy-command"; command: string; label: string };
export interface ModuleSetup { summary: string; steps: string[]; actions: SetupAction[] }
export const SETUP_CATALOG: Record<string, ModuleSetup>; // keyed by module key
export function setupFor(key: string): ModuleSetup | null;
export function setupDoneKey(serverDir: string, moduleKey: string): string; // localStorage key
```

- [ ] **Step 1: Failing vitest** — catalog shape sanity (every action's referenced machinery type is one of the five; every entry has ≥1 step; `setupFor` unknown key → null; `setupDoneKey` incorporates both args so two servers don't share dismissals).
- [ ] **Step 2: Implement the catalog** with these entries (content per spec §5):
  - `mod-ahbot`: summary "AHBot only auctions once it has an auction-house character."; steps: create a dedicated account+character, set `AuctionHouseBot.Account`/`AuctionHouseBot.GUID` in mod_ahbot.conf, restart the world server; actions: open-tuner(mod-ahbot), open-files(mod_ahbot.conf), copy-command(`account create ahbot <password>`).
  - `battlepass`: steps for the missing vendor NPC; actions: fixit(battlepass-npc).
  - `bmah`, `mod-1v1-arena`, `mod-npc-beastmaster`, `mod-transmog`: NPC placement steps; actions: place-npc(key).
  - `mod-arac`: client-patch step + restart; actions: open-files reference is not right here — use copy-free steps only plus a note pointing at the row's own "Apply client patch" button.
- [ ] **Step 3: Run vitest → PASS.**
- [ ] **Step 4: UI** — installed rows where `setupFor(key)` is non-null AND `!localStorage.getItem(setupDoneKey(serverDir, key))`: amber chip `Needs setup` + a `Setup…` button toggling an inline panel (the repair-panel visual pattern, `ModuleManager.svelte:718-794`): summary, `<ol>` steps, action buttons (each action type dispatches to the consumed machinery; copy-command uses `navigator.clipboard.writeText` + a transient "copied" note), and `Mark as done` (sets the localStorage key, hides chip+panel; a small "Setup" ghost link remains so the panel stays reachable after dismissal). Server dir: reuse however the page already learns the server identity — if nothing on this page exposes it, key by the literal `"native"` backend string and record that in a code comment as v1.
- [ ] **Step 5: `npm test -- --run` + `npm run check` green. Commit** — `git add -A && git commit -m "feat(launcher): catalog-driven needs-setup notices with guided actions"`.

---

## Final verification (controller, after Task 5)

1. `cargo test --workspace --no-fail-fast` (expect ≥ baseline, 0 failed) — then, never concurrently, bats via the documented `wsl -d dml-arch -u dml --exec` form (expect ≥865 ok / 0 not ok), then `npm test -- --run` + `npm run check` in `launcher/`.
2. `dml-mirror-reviewer` agent over the branch diff (Tasks 1–2 touch mirrored surfaces).
3. Merge decision + live gates are the USER's: real module update (then unlock `module-update`), fresh install showing auto-conf, layout + AHBot setup click-through.

## Workflow + worktree (execution harness)

- Worktree: `git worktree add C:/Users/perzi/dml-desks/modules-round -b feat/modules-page-round rust-main` (once; reuse thereafter).
- Workflow: `.claude/workflows/modules-page-round.js` — 5 agents, strictly sequential, one per task, models per task header (T1/T2 opus, T3–T5 sonnet). Each agent: reads THIS plan's task N in the worktree, executes its steps exactly (TDD order, mutation proofs), commits in the worktree, and returns a structured report (files, test counts, mutations RED/restored, deviations).
- Resume from another session: invoke the workflow by name/scriptPath with `resumeFromRunId`, or just re-run — completed tasks are detected by their commits in the worktree; the script skips a task whose commit subject already exists in `git log`.
