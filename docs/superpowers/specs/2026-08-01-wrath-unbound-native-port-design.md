# Wrath Unbound — native Rust port design

Target: `crates/dml-wow/src/unbound.rs` (+ `unbound_uninstall.rs`), payload under `crates/dml-wow/data/unbound/`, CLI surface `dml-wow unbound install|uninstall|status` in `crates/dml-wow-cli`. Native backend only — the bash↔Rust asymmetry is documented in `docs/cli-contract.md` the same way `install-native`'s native-only status already is.

---

## 1. What we are building

A staged, resumable, NDJSON-streamed installer/uninstaller for the Wrath Unbound add-on (mod-unbound C++ module, 14 SQL migrations, 820-line Eluna Lua, a 6-file AzerothCore core patch, and two conf edits) that mutates an EXISTING native-backend AzerothCore server the user already plays on, mirroring the `install_native.rs` engine shape (`crates/dml-wow/src/install_native.rs`): state file written only after a stage truly finished, guards never recorded, one terminal event, `pct` from the ninja counter during the 30–90-minute rebuild. **This is a port rather than a script runner because the native backend has no bash to run the script in — and because the script's dominant failure class is the silent half-success (unchecked `git apply`, one-file idempotency grep, sed-matched-nothing, warning-then-banner), which only a rewrite can convert into refusals; the bash specifies the OUTCOME, not the mechanism.**

Non-negotiable inherited constraint to surface to the user (see Open Questions): the SQL migrations are MySQL writes through `docker exec -i ... mysql`, the same route `wow backup restore` uses. This adds a third sanctioned write class to the standing "MySQL is read-only" policy and needs explicit sign-off.

## 2. Stage machine (install)

Engine shape copied from `install_native.rs`: `Stage` enum + `STAGE_ORDER`, `run_stage` emits `section_start(name)` / `section_end(name, ok|error)`, marks state only on `Ok(())` and only when `records_completion()`, `Fail{code,message,hint}` propagates to the single `error_event`. State file is **`.dml-unbound.json` in the SERVER dir** (never `.dml-install.json` — `install_native` owns that name for the same title dir), bound by `composegen::install_id(server_dir)` (`crates/dml-wow/src/composegen.rs:158`) so a copied state file is discarded. State is a hint, not authority: every stage re-asks the disk/DB, exactly as `install_native`'s clones re-ask git.

```
preflight ──▶ locate ──▶ guard ──▶ backup ──▶ stage-files ──▶ clone-ale ──▶ sql-world
 (guard,      (guard,    (guard,   [rec]       [rec]           [rec]         [rec]
  not rec)     not rec)   not rec)                                             │
                                                                               ▼
   done ◀── ready ◀── up ◀── build ◀── verify ◀── conf ◀── core-patch ◀── sql-chars
            [rec]    [rec]  [rec,pct]  (guard,    [rec]      [rec]           [rec]
                                        not rec)
```

| stage | does | guard / refusal | recorded | resume behaviour |
|---|---|---|---|---|
| **preflight** | resolve docker+git (`install_native::ProcIo::from_env`, install_native.rs:465), start a stopped Docker Desktop (`ProcIo::ensure_engine`, :480), free-disk check on the server-dir drive | `UNBOUND_GIT_MISSING`, engine codes from `native::ensure_engine_up_stream`, `UNBOUND_DISK_LOW` (<10 GiB) | no — "a guard a resume skips is not a guard" | always re-runs |
| **locate** | `maint::resolve_server_dir(title_dir)` (`crates/dml-wow/src/maint.rs:71`); no prompting ever — the launcher/CLI passes `--id` / the title dir | `UNBOUND_NO_SERVER` | no | re-runs |
| **guard** | (a) checkout is AzerothCore: `.git` is a dir + remote probe, copy `Engine::core_checkout_is_ours` (install_native.rs:1180); (b) stack liveness: `compose ps -q ac-database` **with cwd = server dir**, capture the CONTAINER ID — every later `docker exec` uses that ID, never the engine-global name (kills the bash's wrong-stack failure mode); (c) Playerbots probe via `db::query` (`crates/dml-wow/src/db.rs:288`, `DbConfig::from_env` + title `.env`): ≥9 distinct trainer IDs in 200002–200018 with ≥100 rows; (d) **6-file patch coherence**: probe ALL six patched files for their symbol, refuse on a mixed answer — this is the bash's worst bug, fixed; (e) add-on state: same version complete → refuse unless `--repair`; different version → refuse; (f) consent: migrations 03/05/06 delete third-party rows (incl. `skillraceclassinfo_dbc ID>=10000`, which the bash consent text omitted) — require `opts.accept_data_changes` | `UNBOUND_NOT_AZEROTHCORE`, `UNBOUND_SERVER_NOT_RUNNING`, `UNBOUND_NOT_PLAYERBOTS`, `UNBOUND_PATCH_INCOHERENT`, `UNBOUND_ALREADY_INSTALLED`, `UNBOUND_VERSION_MISMATCH`, `UNBOUND_CONSENT_REQUIRED` | no | re-runs in full every attempt |
| **backup** | `modmgr::module_backup_now` (`crates/dml-wow/src/modmgr.rs:858`) — world-inclusive, gzip-streamed, atomic, 30-min timeout, stderr tail on Err (the bash discarded stderr and never validated the dump) | `UNBOUND_BACKUP_FAILED` — hard stop, copy the `BACKUP_FAILED` arm shape at modmgr.rs:2145 | **yes** | skipped if recorded (a resumed run within one install keeps its backup; a `--repair` fresh run re-backs-up because state was reset) |
| **stage-files** | write the 19-file payload manifest under `server_dir` via `conf::atomic_write` (`crates/dml-core/src/conf.rs:133`), each path checked; overwrite is fine — these files are ours and versioned | any write error → `UNBOUND_WRITE_FAILED` naming the path (the bash checked nothing) | yes | re-runs cheaply (writes are idempotent) |
| **clone-ale** | skip if `modules/mod-ale/CMakeLists.txt` exists; half-clone (dir w/o CMakeLists) → remove + re-clone; clone + checkout pinned `1cb86c9600260c3731c96dc3c98d25b4fc3f2153`. **Pin failure is a refusal**, not the bash's warn-and-build-head | `UNBOUND_ALE_CLONE_FAILED`, `UNBOUND_ALE_PIN_MISMATCH` | yes | disk-evidence skip (CMakeLists exists) |
| **sql-world** | 13 db-world files in `01..08,10..14` order + `npc_setup.sql`, each through the new `apply_sql_file(program, password, "acore_world", path)` (§5) piped to `docker exec -i <container-id> mysql`; per-file `line_event`; first failure aborts with the FILE NAME and stderr tail (bash's npc_setup grep-oracle is gone — everything is exit-code + stderr) | `UNBOUND_SQL_FAILED` | yes | re-runs; migrations are individually idempotent (verified in the RE report) — but the DELETE-then-INSERT ones re-fire, which consent already covered |
| **sql-chars** | `01_unbound_characters.sql` → `acore_characters` via the same helper | `UNBOUND_SQL_FAILED` | yes | re-runs |
| **core-patch** | order is load-bearing: (1) 6-file symbol probe again (state may be stale), all-present → skip; (2) `git apply --check`; (3) `git apply`, **exit status checked** (the bash never checked the real apply); (4) post-apply probe confirms all 6 symbols | `UNBOUND_PATCH_CHECK_FAILED` (hint: local edits to the 6 files / AC revision drift; name the 6 files) | yes | symbol-probe skip |
| **conf** | record PRIOR values into state first (for uninstall), `conf::bak_sibling` both files, then: `conf_ensure` + `conf_write(worldserver.conf, "ValidateSkillLearnedBySpells", "0")` and `conf_write(mod_ale.conf, ...)` for the 9 ALE keys (`crates/dml-core/src/conf.rs:171,495`; route via `config::conf_path_in`, `crates/dml-wow/src/config.rs:157`). `conf_write` rewrites duplicates and appends canonically — the bash's duplicate-key trap and sed-matched-nothing false success are structurally impossible. NB `config::config_set_direct` refuses worldserver.conf; call `conf_write` directly like the curated route does | `UNBOUND_CONF_MISSING` when `conf_ensure` cannot materialise worldserver.conf — a hard stop, not the bash's warn-and-banner | yes | `conf_write` returns `Ok(false)` when equal → true no-op |
| **verify** | read-back with `parse_conf` (`conf.rs:41`, same unquoting rules as the writer so check and write cannot disagree): lua file exists, `ALE.Enabled=1`, `ALE.ScriptPath` exact, `ValidateSkillLearnedBySpells=0`. Failure REFUSES (the bash asked "Continue anyway?" into a 90-minute build) | `UNBOUND_VERIFY_FAILED` | no — it must re-run on every resume | re-runs |
| **build** | new `compose_build_service_argv("ac-worldserver")` (no `-f` — an existing server auto-loads its own files, §5), cwd = server dir, streamed via the `run_echo_with` pattern with a **locally-created** `BuildProgress` (install_native.rs:980) emitting `pct_event`; tee to `~/.dml/logs/unbound-build-<ts>.log` via `proc::run_streamed_unbounded` (`crates/dml-core/src/proc.rs:285`) — deliberately unbounded | `UNBOUND_BUILD_FAILED` naming the log path | yes | re-runs; BuildKit cache makes it cheap, pct starts over by construction |
| **up** | `compose up -d --force-recreate ac-worldserver`, capture the NEW container id + `StartedAt` | `UNBOUND_UP_FAILED` | yes | re-runs |
| **ready** | assemble `Engine::do_ready`'s ingredients (install_native.rs:1605 is the model): resolve container via `compose ps -a -q ac-worldserver` EVERY poll, `docker inspect -f '{{.State.StartedAt}}|{{.State.RestartCount}}'`, `docker logs --since <StartedAt>` (never `--tail`, never the whole log — the bash's stale-marker false positive is gone), grep for `[UNBOUND] Prereq map built.`, `BootLoopWatch` (`crates/dml-wow/src/lifecycle.rs:553`) for restart-count climb, 10s poll, 1800s cap. **Timeout is a FAILURE** — the bash's strongest success signal could not fail the run; here it is the only thing that lets `done` fire | `UNBOUND_READY_TIMEOUT` (hint includes the boot-loop note and log path) | yes | re-runs (cheap) |

`done_event` data: `{addon_version, server_dir, backup_path, migrations_applied: [...], manual_step: ".npc add 900001"}`. The Mentor spawn stays manual (see §9/§10); `done` must NOT claim it happened — the bash banner lied about this.

On terminal failure: `persist_failure` writes `last_error`, and the error hint always names `backup_path` — the failure path leaves a server that still boots because nothing before `build` touches the running binary, and a failed build leaves the old container running (compose `build` does not stop it; `up --force-recreate` only runs after a successful build).

## 3. The payload

The 1898 heredoc lines become **19 committed files + zero templates**. DONE —
extracted and committed in `d7d6d8b`; the layout below is what is actually on
disk (the patch sits at the module root, mirroring the bash's
`$MODULE_DIR/unbound-core-access.patch`, not in a `patch/` subdir):

```
crates/dml-wow/data/unbound/           19 files, 84 976 bytes
  module/src/UnboundSystem.cpp            module/src/UnboundSystem_loader.cpp
  module/npc_setup.sql
  module/sql/db-world/01_unbound_world.sql … 08, 10 … 14   (13 files — NO 09, do not "fix" the gap)
  module/sql/db-characters/01_unbound_characters.sql
  module/unbound-core-access.patch
  lua/unbound_mentor.lua                  (820 lines / 33 KB — the largest payload)
```

- **Literal vs interpolated:** all 19 quoted heredocs are `$`-free (verified in the inventory) and extract byte-for-byte. The single interpolated heredoc (mod_ale.conf, one variable, one compile-time constant value) is **not extracted at all** — the conf stage writes its 9 keys through `conf_write`, which unifies the bash's write-fresh and repair-in-place branches into one idempotent code path. No templating engine exists in this feature; if one is ever needed, `composegen`'s `{{TOKEN}}`-with-unresolved-is-error pattern (composegen.rs:66-68) is the precedent.
- **Extraction** was a one-off state-machine script (opener→body→terminator; trust start_line order, not `WU_PAYLOAD_EOF_n` numbering; lines 658/672/673/2390 contain literal `<<`; line 406 is a here-string; the EOF_18/EOF_12 pair at 1513/1514 has zero gap). It worked entirely in **bytes** — never decoding, stripping or re-encoding — because 19 of 20 bodies carry UTF-8 box characters and the patch carries space-only lines; any text-mode round trip loses one or the other.
  **Correctness was proven, not reviewed:** the script rebuilt the original installer from the extracted files and diffed it against the source — byte-identical, an outcome no mangled, mis-ranged or skipped body can produce. Post-staging checks confirmed all 19 git blobs byte-match the working tree and no payload file carries a CR.
- **The patch is an opaque blob.** Six body lines are a single space (source lines 2406, 2416, 2424, 2428, 2441, 2456 → patch lines 51, 61, 69, 73, 86, 101) — any trailing-whitespace strip kills `git apply` silently, against a server the user already plays on, mid-rebuild. Defenses: (1) `.gitattributes` gained `crates/dml-wow/data/unbound/** text eol=lf` — DONE, and RECURSIVE on purpose, since the existing `data/*.json` and `data/*.tmpl` globs do not reach a subdirectory; (2) a pin test asserts the embedded patch contains exactly 6 space-only lines and zero CR bytes; (3) never run a formatter over `data/unbound/`.
- **Embedding:** `include_str!` per file, collected in a `const MANIFEST: &[(rel_path, &str)]` following `registry.rs:29-31`; the stage-files stage is a loop over MANIFEST → `atomic_write(server_dir.join(rel), body)`. A pin test asserts file count = 19 and per-file byte lengths, so drift fails loudly (payload.rs spirit, `launcher/src-tauri/src/payload.rs`).
- **CMakeLists.txt: RESOLVED — none is needed, and inventing one would be wrong.** Checked against the user's own AzerothCore tree (`C:\Users\perzi\dml-native\wow-server-playerbots`) rather than reasoned about. `modules/CMakeLists.txt` builds its module list with `GetModuleSourceList()` and then pulls sources with `CollectSourceFiles(${MODULE_SOURCE_PATH} …)` — it **globs**, it does not read a per-module manifest. Four of the five installed modules (mod-playerbots, mod-transmog, mod-learn-spells, mod-ah-bot-plus) ship no `CMakeLists.txt` at all; only mod-ale does, and only because it bundles a Lua library. The optional per-module hook is `modules/<mod>/<mod>.cmake`, which nothing here uses.
  The loader symbol lines up too: `ConfigureScriptLoader` generates `Add${dir}Scripts()` with `-`→`_`, so a directory named `mod-unbound` needs exactly `Addmod_unboundScripts` — which the extracted `UnboundSystem_loader.cpp` already defines, forwarding to `AddUnboundScripts()`.
  What this does NOT prove is that the module *compiles*; that still needs the one live rebuild named in §10.

## 4. Reuse map

| need | existing API | file |
|---|---|---|
| find server dir under title dir | `resolve_server_dir` / `require_server_dir` | `crates/dml-wow/src/maint.rs:71,92` |
| games/title dir without cwd fallback | `games_dir_for_install`, `valid_title_id` | `crates/dml-wow/src/install_native.rs:672,691` |
| "is this checkout ours" probe | `checkout_probe_argv`, `core_checkout_is_ours` pattern, `maint::is_git_checkout` | `install_native.rs:728,1180; maint.rs:361` |
| safety backup, refuse-on-fail shape | `module_backup_now`; `BACKUP_FAILED` arm | `crates/dml-wow/src/modmgr.rs:858,2145` |
| dump/restore plumbing | `backup::dump_to`, `restore::stream_restore` | `crates/dml-wow/src/backup.rs:724`, `restore.rs:133` |
| MySQL creds (never `-ppassword`) | `DbConfig::from_env` / `resolve_db_config`; pass `db_cfg` as a parameter (the Backups-round Critical) | `crates/dml-wow/src/db.rs:133,218` |
| read-only DB probes (trainer/canary/version) | `db::query`, `query_with_params`, `count_result` | `db.rs:288,310,403` |
| embed payload | `include_str!` + LazyLock/const + pin test | `registry.rs:29-31`, `composegen.rs:66-68` |
| atomic file writes | `conf::atomic_write` | `crates/dml-core/src/conf.rs:133` |
| single-statement SQL | `mysql_run_stmt` (pub, any db) | `modmgr.rs:487` |
| conf edits (both confs, both directions) | `conf_write`, `conf_ensure`, `conf_path_in`, `bak_sibling` | `conf.rs:171,495`; `config.rs:157` |
| conf read-back | `parse_conf`, `kv_rows`, `strip_conf_quotes` | `conf.rs:41,297,28` |
| streamed 90-min build + tee | `run_streamed_unbounded` / `run_streamed_lines`; call-site model `module_rebuild_stream` | `crates/dml-core/src/proc.rs:285,316`; `modmgr.rs:2112` |
| docker/git resolution, engine autostart | `ProcIo::from_env`, `ProcIo::ensure_engine`, `native::ensure_engine_up_stream` | `install_native.rs:465,480`; `native.rs:201` |
| injectable subprocess seam | `InstallIo`, `Call`, `RunOutcome`, `PROBE_TIMEOUT` discipline | `install_native.rs:421,369,398,393` |
| build percentage | `BuildProgress::observe`, `parse_build_step` (created locally per build) | `install_native.rs:980,943` |
| readiness ingredients + boot-loop | `READY_INSPECT_FORMAT`, `parse_started_and_restarts`, `logsnap::parse_container_id`, `BootLoopWatch`, `boot_loop_note` | `install_native.rs:877,887`; `logsnap.rs:277,285`; `lifecycle.rs:553,518` |
| rebuild-pending marker | `rebuild_pending_add/clear` | `modmgr.rs:278,294` |
| NDJSON + exit code | `dml_core::events::*`; `out::stream_sink`, `TerminalSeen` + `stream_exit` (sticky-on-failure; copy the InstallNative arm at `run.rs:790`) | `crates/dml-core/src/events.rs`; `crates/dml-wow-cli/src/out.rs`, `run.rs` |
| refusal shape | `Fail{code,message,hint}` + `CODE_*` convention; `CmdError` | `install_native.rs:1033,117-129`; `crates/dml-core/src/error.rs` |
| state file shape + install_id binding | `InstallState`/`load_state`/`save_state`/`persist` semantics (copy the shape, new file name) | `install_native.rs:93,605` + composegen.rs:158 |

## 5. New machinery (each with why reuse fails)

1. **`apply_sql_file(program, password, db, path) -> Result<(), String>`** in modmgr or unbound.rs. `mysql_run_file` (modmgr.rs:553) already takes a `db` param but is **private** and returns bool with no stderr; `sql_install_clone_sql` (modmgr.rs:1282) hardcodes `acore_world` and we need `acore_characters` plus per-file error text. Cheapest: make `mysql_run_file` + `run_with_stdin_bounded_draining` pub and add an error-returning wrapper whose `Err` carries the stderr tail (same shape as `dump_to`'s Err). It must also accept a **container id** instead of the literal name `ac-database` (wrong-stack fix) — a small signature change threaded through.
2. **Git patch apply/revert wrappers**: argv builders for `git -C <dir> apply [--check] [-R] <patchfile>` + `probe_patch_symbols(server_dir) -> PatchPresence {All, None, Mixed(Vec<file>)}` reading the 6 files for their symbols. Nothing in the workspace applies a patch (the only patch code, `wow_pull_repo` at modmgr.rs:1405, creates one). The patch file itself is written to a temp path from the embedded blob so uninstall never depends on a file it deleted (the bash uninstaller's own lesson).
3. **Per-service compose argv builders**: `compose_service_build_argv("ac-worldserver")`, `compose_service_up_recreate_argv("ac-worldserver")`, `compose_service_ps_q_argv(service)` — `dml_core::compose` only has whole-stack up/down, and `install_native::build_argv` names the three generated `-f` files which a pre-existing server does not have. **No `-f` flags**: compose must auto-load whatever the server actually uses.
4. **`UnboundState` / `.dml-unbound.json`**: copy `InstallState`'s shape and `load_state` discipline (version + install_id binding, stage-name strings, best-effort persist) but with extra fields: `addon_version: "1.2.2"`, `prior_conf: {validate_skill_learned: Option<String>}`, `migrations_applied: Vec<String>`. Cannot reuse `.dml-install.json` — install_native owns it, and the two must coexist in one title dir. This also fixes the uninstaller's "restore 1, not the prior value" residue.
5. **Payload manifest + staging loop** (§3). No multi-file staging helper exists (`copy_dir_contents` is private and copies trees, not embedded bytes).
6. **Callable readiness wait with a parameterised marker**: `Engine::do_ready` is private; reassemble its pub ingredients with `[UNBOUND] Prereq map built.` (install) and the inverse assertion (uninstall).
7. **`UnboundIo` trait**: `InstallIo` reused if possible (the engine shells only git + docker), plus one extra method for the mysql-crate probes (`fn db_query(...)`) so guard tests never need a live DB. FakeIo copies `install_native.rs:1908`'s replace-in-place `set()` semantics verbatim.
8. **Uninstall engine** (§7) — no rollback path exists in Rust at all.

## 6. Refusals

All emitted as the terminal `error_event{code,message,hint}`; every message names a fact the user cannot guess + a concrete next action; all documented in `docs/cli-contract.md`.

| code | condition | phase |
|---|---|---|
| `UNBOUND_NO_SERVER` | `resolve_server_dir` → None | locate |
| `UNBOUND_NOT_AZEROTHCORE` | no `.git` dir, remote probe mismatch, or `src/server/game/Entities/Player/Player.h` absent | guard |
| `UNBOUND_SERVER_NOT_RUNNING` | `compose ps -q ac-database` (cwd = server dir) empty; hint: `dml-wow start`, and if a bare `docker ps` shows an ac-database from ANOTHER stack, say so | guard |
| `UNBOUND_NOT_PLAYERBOTS` | trainer probe <9 distinct IDs or <100 rows | guard |
| `UNBOUND_ALREADY_INSTALLED` | state records this addon_version complete; hint: `--repair` re-runs everything, `unbound uninstall` removes | guard |
| `UNBOUND_VERSION_MISMATCH` | state records a DIFFERENT addon_version; hint: uninstall first (no in-place upgrade in v1) | guard |
| `UNBOUND_PATCH_INCOHERENT` | some but not all 6 files carry their symbol — the bash's silently-missing-feature bug; hint names exactly which files, suggests `git checkout` of the six | guard |
| `UNBOUND_CONSENT_REQUIRED` | `accept_data_changes` not set; message enumerates 03/05/06's third-party-row deletes including `skillraceclassinfo_dbc ID>=10000` | guard |
| `UNBOUND_DISK_LOW` | <10 GiB free on the server-dir volume (the build needs it) | preflight |
| `UNBOUND_GIT_MISSING` | no git resolvable | preflight |
| `UNBOUND_BACKUP_FAILED` | dump Err (stderr tail included — never `2>/dev/null`) | backup |
| `UNBOUND_WRITE_FAILED` | any staged-file write error, naming the path (root-owned env/dist is the known case) | stage-files |
| `UNBOUND_ALE_CLONE_FAILED` / `UNBOUND_ALE_PIN_MISMATCH` | clone fails / pinned checkout fails (bash warned; we refuse — an untested mod-ale head is not buildable-with-confidence) | clone-ale |
| `UNBOUND_SQL_FAILED` | any migration non-zero, message = file name + stderr tail + backup path | sql-* |
| `UNBOUND_PATCH_CHECK_FAILED` | `git apply --check` fails on a tree the symbol probe called clean | core-patch |
| `UNBOUND_CONF_MISSING` | worldserver.conf cannot be materialised (`conf_ensure` fails) — hard stop, not the bash's warn-then-banner | conf |
| `UNBOUND_VERIFY_FAILED` | read-back mismatch | verify |
| `UNBOUND_BUILD_FAILED` / `UNBOUND_UP_FAILED` | `PIPESTATUS[0]`-equivalent non-zero; names the tee log | build/up |
| `UNBOUND_READY_TIMEOUT` | marker absent after 1800s; includes `boot_loop_note` when `RestartCount` climbed | ready |

## 7. Uninstall

Same engine shape, own stage order, own state tokens in `.dml-unbound.json` (an `uninstall_completed` list). Honest-inverse policy: what cannot be reverted is **named in `done_event.data.residue`**, never silently banner-ed.

```
preflight ─▶ locate ─▶ guard ─▶ backup ─▶ sql-revert ─▶ remove-files ─▶ patch-revert
                                [rec]      [rec]          [rec]           [rec]
       done(residue[]) ◀── ready ◀── up ◀── build ◀── conf-revert ◀──────────┘
                           [rec]    [rec]   [rec,pct]    [rec]
```

- **guard**: detection asks BOTH the filesystem AND the database (`SELECT 1 FROM unbound_milestones` via `db::query` — the canary the bash uninstaller forgot), refuses `UNBOUND_NOT_INSTALLED` unless `--force`; consent flag repeats the installer's third-party-row warning (the bash uninstaller dropped it).
- **backup**: `module_backup_now`, hard fail — the one thing the bash got right, kept.
- **sql-revert**: the 11 statements via `mysql_run_stmt` against the compose-resolved container id, `creature_addon` before `creature` (kept). **Narrowed predicates when state permits**: delete the exact `skillraceclassinfo_dbc` IDs and exact `playercreateinfo_spell_custom` (classmask, Spell) pairs the embedded payload inserts (derivable at build time from the payload, pinned by a test) instead of `ID>=10000` / whole-classmask wipes. `--legacy-wide-delete` falls back to the bash's range predicates for servers installed by the old script. Each failure is a `warn` line AND appended to residue; a failure here does not abort (matching the bash's deliberate `|| exit_code=$?`), but unlike bash it is never forgotten.
- **remove-files**: delete `modules/mod-unbound/`, the lua, the legacy lua path; **never** mod-ale or mod_ale.conf (shared engine — deliberate residue, named).
- **patch-revert**: symbol probe → skip when absent; embedded patch blob → temp file → `git apply -R --check` → `-R` apply, status checked. `--check` failure = warn + residue entry "core patch left applied (inert without mod-unbound)" — continue.
- **conf-revert**: restore `prior_conf.validate_skill_learned` from state when recorded, else `1`, via `conf_write` (fixes the hardcoded-restore residue where state exists; residue-named where it does not).
- **build/up/ready**: same per-service builders + pct. Ready's assertion is **inverted and doubled**: `status::world_ready_from_logs` (`crates/dml-wow/src/status.rs:290`) must be TRUE and the `[UNBOUND]` marker must be ABSENT in `docker logs --since <StartedAt>` — a crash-looping worldserver can no longer look like a clean uninstall. Timeout = `UNBOUND_READY_TIMEOUT`.
- **done**: residue array always includes the permanent classes: mod-ale + mod_ale.conf left enabled (and a note when the INSTALL created them on a server that had neither); `lua_scripts/` dir left; cross-class spells in `character_spell` until each character's next login; Mentor Stones (900100) in inventories — the bulk-delete SQL is included as TEXT in done data, never executed; `unbound_character_unlocks` was dropped (it is progression data — say so); plus any per-run failures collected above. Declining/aborting between sql-revert and build leaves the recorded half-uninstalled state, and `unbound status` reports it (`next_stage` from the state file) instead of pretending it is a normal outcome.

## 8. Testing

All engine tests run against `FakeIo` implementing `UnboundIo` — no docker, no git, no DB, no 90-minute build. Live proof follows the `provision::tests::live_*` pattern (`launcher/src-tauri/src/provision.rs`): `#[ignore]`d tests that touch the real snapshot server, run explicitly with `-- --ignored`.

- **Per-stage**: each stage function takes `&dyn UnboundIo` + emit; tests script replies and read back `FakeIo`'s recorded `Call` list (program, argv, cwd, timeout). Probes must carry `Some(PROBE_TIMEOUT)`, long runners `None` — asserted from the recorded Calls.
- **Payload pins**: manifest count = 19; per-file byte length + FNV hash; the patch contains exactly 6 space-only lines and 0 CR bytes; no `09_*` filename; sql-world apply order is `01..08,10..14`.
- **State discipline**: after a full fake run, state contains NO guard/verify/locate/preflight tokens; a state file copied to a different dir loads as None (install_id mismatch); `mark` uses stage-name strings.
- **Resume**: interrupt after `sql-world`, reload, assert the next run's first mutating Call is the sql-chars pipe and that guard Calls (docker/git probes) STILL ran.
- **Vacuous-pass traps, each mapped to a recorded incident:**
  - *Override silently ignored* (FakeIo first-match bug, install_native.rs:1908): `FakeIo::set` replaces same-key entries; plus one canary test that registers an override after the shared `happy_io()` builder, mutates production to depend on it, and is documented as the red-proof.
  - *Source scan reads comments* (feature-keys/Test-InstallerNative, 2026-08-01): any "the engine never execs by bare container name" or "never runs mysql with -ppassword" invariant is asserted on **recorded argv from FakeIo**, never a grep over `unbound.rs`.
  - *Ordering anchored on the wrong thing* (Get-FileHash/Move-Item): the backup-before-mutate test does NOT assert "backup Call precedes sql Call"; it scripts the backup to FAIL and asserts **zero mutating Calls were recorded and the terminal event is `UNBOUND_BACKUP_FAILED`** — the refusal is the anchor. Same for consent: no `accept_data_changes` → zero Calls past guard.
  - *Skips-green suite* (parity find_bash trap): unit tests have no environmental gate at all (pure fakes); the live tests are `#[ignore]`, which is visible in the summary, never a silent early-return. Any assertion satisfiable by an early error return gets a companion (e.g. ready-timeout test asserts elapsed poll count, which a failed spawn cannot satisfy).
  - *Pure-list restatement* (`lifecycle_steps_for_mode` lesson): no test pins a stage-order constant; order tests drive the real `go()` through FakeIo and read the Calls back.
- **pct**: reuse `BuildProgress`'s existing tests; add one asserting a fresh `BuildProgress` per build Call (feed a resumed-build transcript, assert pct restarts).
- **conf round-trip**: `conf_write` + `parse_conf` against fixtures containing the bash's pathological inputs (duplicate `ALE.Enabled`, commented key, `ALE.Enabled = 0`, CRLF) — the check and the write share one parser so these are cheap.
- **Never run bats and cargo parity suites concurrently** (standing rule); this feature adds no bats.

## 9. Explicitly OUT of scope

- Launcher UI wiring (same status as install-native: reachable only via the binary; `install-progress.svelte.ts` + statusLabel precedence is the future consumer).
- A bash mirror. The bash `dml unbound` download-and-run route stays as-is for WSL; the asymmetry is documented, not resolved.
- Modules-page catalog surfacing (embedded-payload add-ons fit none of the three registry families — needs its own round).
- Automated Mentor spawn via SOAP (`soap_bootstrap.rs` proves a fresh server has no GM3 account; cannot be assumed).
- In-place version upgrade (v1: uninstall then install).
- Registering migrations in AzerothCore's `updates` table; backup pruning changes; multi-stack per-install container names.
- Porting the wizard's interactive prompts in any form.

## 10. Open questions for the user

1. ~~**CMakeLists.txt**~~ — **ANSWERED 2026-08-01, no user input needed.** AzerothCore globs module sources; no per-module `CMakeLists.txt` exists or is wanted. See §3. The narrower claim "mod-unbound *compiles*" still wants one live rebuild on the snapshot server (`C:\Users\perzi\dml-native`), which is a verification task, not a decision.
2. **Third sanctioned MySQL write**: the migrations are writes via `docker exec mysql`, joining restore and the realmlist toggle. Sanction this class explicitly (and the uninstall's DELETE/DROP set), or the feature cannot ship under the standing read-only policy.
3. **Uninstall on bash-installed servers**: no state file exists there. Default to refusing the wide deletes and requiring `--legacy-wide-delete`, or default to the bash-compatible wide behaviour? I designed for the former.
4. **Mentor spawn**: leave fully manual (`.npc add 900001` in done data), or add an optional best-effort SOAP spawn when `~/.dml/soap.env` exists, with the manual instruction as fallback?
5. **Payload ownership**: 1.2.2 is pinned by extraction. When the upstream wizard bumps, does this repo's copy track it (who re-extracts?), or is the Rust port now the canonical home of the payload?