# Teaching the launcher `dml-wow`'s vocabulary — implementation report

**Branch:** `worktree-agent-ad40bc788868182d0` · **Base:** `d91be22` · **Date:** 2026-08-05
**Status:** COMPLETE. Design steps 1–7 built, committed and mutation-verified. Step 8 (the
default flip) deliberately NOT done — that is the controller's call.

---

## 0. Base correction, first

The worktree was created from the wrong commit — `a624a38 Update Install-DML.ps1 …`, exactly
the one the brief warned about. `git reset --hard d91be22`, then both confirmation checks:
`crates/dml-core/src/vocab.rs` absent, `ARCH_BINARY` present in `runner.rs`. They agreed, so
work proceeded.

## 1. Commits

| SHA | What |
|---|---|
| `7fa90f4` | `dml_core::vocab` — the translation table (pure, no I/O) |
| `c96dbe0` | T1 — every translated argv parses under the real clap tree |
| `dd01974` | Route the Arch runner through the table; `--json` becomes conditional |
| `6c049fa` | T2 — every launcher call site is classified |
| `e6df6af` | `games-list` / `games-status`, the last two unanswered verbs |
| `d0f43e7` | T3 — the streaming flag agrees with `run.rs` |
| `170c932` | **R1**: start the engine with systemd inside the distro, not Docker Desktop |

## 2. Coverage

**107 call sites extracted from the launcher's own source. 74 translate to `dml-wow`; 33 fall
back to the bash `dml` in the same distro.** Nothing is gated off — every verb has a working
destination on day one.

`TABLE` holds **105 rows: 70 `DmlWow`, 35 `Bash`.** (Rows and call sites differ because some
verbs appear at more than one site, e.g. `games stop` at three.)

The 33 bash-routed verbs are the ones with no `dml-wow` arm: `doctor`, `lan`, `games catalog`,
`games install`, `run <url>`, `unbound`/`unbound-remove`, the `party` arms not ported
(`online`/`specs`/`list`/`dismiss-all`/`preset-show`/`preset-import`), `world-restart`,
`bridge-setup`/`party-setup`, `ahbot repair`, `module client-patch`/`conf-activate`/`tracking`/
`fixit`/`place-npc`/`update-check`, `config pb-keys`/`conf-keys`/`raw-reset`, `entity-info`,
`teleport-coords`, `gm return-home`, `docker-usage`, `update-check`, `tailscale`, `port-check`,
`docker-restart`, `lan public-ip`. Each migrates later, one at a time.

Two of them are worth flagging as *improvements* on Arch rather than compromises:
`games catalog` and `games install` route to bash INSIDE the distro, where
`_installers_supported` is true — so the Library page's Install button arms and the six `.sh`
installers actually run. That is the v0.2 "`.sh`-in-a-distro runner" story landing early, for
free. **It ships as a side effect and should be confirmed as wanted in v0.1.0 scope.**

## 3. The safety property, and how it is structural

`--json` is appended **iff the target is bash**, decided in one new private method
`DmlRunner::resolve()`. The `Vocabulary::Bash` arm clones `prefix_args` and passes `args`
through untouched — there is no code path from `Backend::Wsl` or `Backend::Native` into the
table at all, and only `arch()` ever sets `Vocabulary::Arch`.

Pinned by argv read off real `Command`s, not by prose:
`the_wsl_backend_argv_is_unchanged_by_the_translation_seam`,
`the_native_backend_argv_is_unchanged_by_the_translation_seam`,
`no_constructor_but_arch_opts_into_the_translation_table`.

Two safety additions ride the translation rather than a call site, so they hold no matter which
site sent the verb (including the `run_captured` path):

* `games stop` always gains **`--no-stop-engine`** (R2).
* `backup restore` and `docker-clean` gain the **`--yes`** the bash CLI never needed.

## 4. `start` on the Arch path — R1

**It would still have failed after translation, and now it does not.**

`Cmd::Start` calls `ensure_engine_up_stream` unconditionally. That went straight to
`docker_desktop_program()`, which finds nothing on Linux, so `ensure_decision` returned
`NoDesktop` and the arm emitted terminal `DOCKER_DESKTOP_MISSING` and **aborted before
compose** — with the distro's dockerd down, which is the only situation the prerequisite exists
for. (With dockerd up it printed "Docker Desktop engine already running", a cosmetic lie that
hid how close it sat to failing.)

`EngineKind::for_backend` and `start_engine_systemd()` — both built on this branch and
referenced nowhere but their own tests — are now wired, through a new
`EngineKind::for_host(is_windows)`. The two questions are different and only one is answerable
in-process: `for_backend` is the launcher's ("which backend am I about to drive");
`for_host` is `dml-wow`'s own ("where am I"). `DML_BACKEND` **cannot** answer the second — there
is no `WSLENV` anywhere in this repo, so a `wsl.exe --exec` child inherits none of the
launcher's environment and would read the default whatever the user chose. The platform is the
answer, routed through `for_backend` so the two stay one mapping.

**How it is proved.** `ensure_engine_up_systemd_with` takes the docker program, the start
command and the poll budget, so the tests drive the whole path without shelling
`sudo systemctl start docker` on anyone's machine and without waiting 180 seconds — the same
seam, for the same reason, as `stop_engine_stream_with`'s own recorded incident in that file.

* `the_systemd_path_never_blames_docker_desktop` — drives the path with a dead engine and
  asserts no `DOCKER_DESKTOP_MISSING` anywhere in the event stream, while still failing
  honestly with `DOCKER_ENGINE_TIMEOUT` (so it is not passing by doing nothing).
* `a_sudo_refusal_is_reported_and_carried_into_the_timeout` — the Tailscale lesson: the cause is
  named when it happens *and* in the error the user reads three minutes later.
* `a_timeout_after_a_clean_start_points_at_the_daemon_not_at_sudo` — the inverse; no invented
  sudo failure.
* `a_linux_host_controls_the_engine_with_systemd_not_docker_desktop` — asserts both platforms
  from either, because the failure is on the platform CI mostly is not.

The tri-state is respected, not collapsed: `Yes` proceeds, `No` says the engine is down,
`Unknown` says it could not tell and asks systemd anyway (start on an active unit is a no-op).
Same safe action for the latter two, **different operator-visible line** — refusing on a slow
probe would block a working server, and claiming "up" would compose against a dead engine.

R2's other half is closed at the root too: a server stop on a systemd host deliberately leaves
the daemon up and says so. Inside the distro that dockerd is a shared system service, not a
per-server VM whose RAM is being reclaimed.

**Honest limit:** this is proved by driven unit tests on Windows, not by a live run inside
`dml-arch`. The live in-distro `start` remains a user gate.

## 5. The three tests, and their mutations

Every one was committed first, then mutated, confirmed red for the right reason, reverted, and
`git status --short` confirmed clean.

| Test | Mutation | Result |
|---|---|---|
| **T1** `every_dml_wow_translation_parses_under_the_real_clap_tree` | rename `ItemsSearch`'s command to `item-search` | **RED** — "translated to `["items-search", …]` which dml-wow REFUSES: unrecognized subcommand 'items-search'" |
| **T2** `every_launcher_call_site_is_classified` | add `run_json_cmd(state, vec!["wow".into(), "brand-new-thing".into()])` | **RED** — "these launcher call sites have no dml_core::vocab::TABLE row: `["wow brand-new-thing"]`" |
| **T2** `comments_are_stripped_but_string_literals_are_not` | delete the `//` stripping arm | **RED** — extracted `"wow invented-by-a-comment"` from a comment |
| **T3** `the_streaming_flag_matches_run_rs` | `Cmd::Restart` arm → `emit_ok(...)` | **RED** — "row `["games","restart"]` says streams=true but Cmd::Restart's arm in run.rs does not call stream_dispatch" |
| **Byte-identity** `the_wsl_backend_argv_is_unchanged_by_the_translation_seam` | make `Vocabulary::Bash` consult the table | **RED** — WSL argv became `dml status --json` instead of `dml wow server-detail --json`, the exact historical regression |
| **R1** `a_linux_host_controls_the_engine_with_systemd_not_docker_desktop` | `for_host` → always `Desktop` | **RED** — "left: Desktop, right: Systemd" |

### Two real extractor flaws T2 found while being written

Both of the class that yields a silently wrong answer rather than an error, so both are worth
recording:

1. Resolving `let args = vec![…]` backwards **walked out of the enclosing function**, so
   `stream_args`' own body resolved to an unrelated caller's argv and reported a verb that site
   never sends. Resolution is now scoped to the enclosing item-level `fn`.
2. `run_json_cmd` / `stream_args` / `stream_action` *wrap* a runner method; their bodies are not
   call sites, and `stream_action`'s `&["games", action, &id]` was being read as a bare `games`
   verb. They are skipped by name and **counted**, with the count asserted at 3.

T2's non-vacuity is three independent guards: a floor of 100 extracted sites (actual: 107), four
known-verb probes so an extractor that quietly stopped matching *one* shape is caught while the
floor still holds, and an exact-set assertion on the one site whose verb is a runtime value
(`tool_install`'s — whose `TOOL_NAMES` are read out of the source and checked individually, so
adding a third tool goes red).

**T2 runs on the Windows CI job only** — the ubuntu job builds the three crates, not the
launcher. Do not later read a green ubuntu run as coverage for it.

## 6. Verification

| Suite | Result |
|---|---|
| `cargo test -p dml-core` | 311 passed, 0 failed, 1 ignored |
| `cargo test -p dml-wow` | 997 lib + 97 across 21 parity/integration suites, 0 failed |
| `cargo test -p dml-wow-cli` | 65 + 92 + 56 + 6, 0 failed |
| `cargo test -p launcher` | 206 passed, 0 failed, 1 ignored |
| `cargo build --workspace` | clean |
| `npm test` (launcher) | 62 files, 752 tests passed |
| `npm run check` | 331 files, **0 errors 0 warnings** |

`cargo test --workspace` was NOT run, per the brief. Note `dml-wow-cli`'s
`install_native_refuses_an_unreachable_docker_before_it_creates_anything` still takes **184 s**
against its 30 s bound — the known R9 issue a sibling agent is fixing. It passes; it is just slow.

Also smoke-tested against the real binary with a fixture games dir:

```
games-list          → {"data":{"games":[{"id":"alpha","path":"…/alpha","running":false},
                                        {"id":"beta","path":"","running":false},
                                        {"id":"gamma","path":"…/gamma/inner","running":false}]},"ok":true}
games-status alpha  → {"data":{"id":"alpha","state":"stopped"},"ok":true}          exit 0
games-status nope   → NOT_FOUND                                                     exit 1
games-status ../evil→ BAD_ID                                                        exit 1
```

## 7. Should the default flip?

**Not yet — but the blocker this task existed to remove is gone.** The vocabulary mismatch and
the `start` refusal are both fixed and pinned. What remains is a short list, and none of it is
this work:

**Genuinely blocking:**

* **A live gate.** Nothing here has been run inside a real `dml-arch`. The R1 fix in particular
  is proved by driven unit tests on Windows; the in-distro `start` needs one real run.
* **R6 — SOAP `soap.env` split-brain.** `wow_soap_autosetup` (lib.rs:6739) resolves
  `dml_core::util::home_dir()` and writes the **Windows** `~/.dml/soap.env`, while the in-distro
  CLI reads `/home/dml/.dml/soap.env`. The launcher can prove a SOAP round-trip and every
  CLI-routed SOAP verb (`gm`, `party`, `console`) still gets `SOAP_AUTH`.
  **I did not fix it, deliberately.** It is not small — it needs a new write path *into* the
  distro plus a decision about which side owns the truth — and it is not introduced by this
  work: the same split already exists on `Backend::Wsl` today, where SOAP verbs also route
  through the in-distro CLI. It also sits on a feature with its own outstanding user gate.
  `native_soap_copy` (lib.rs:5929) does exactly the opposite direction and is the model for the
  fix. Note the *account* is shared (it is a real `acore_auth` row); only the credentials FILE
  is split, so the remedy is narrow.

**Worth an audit before flipping, not necessarily fixes:**

* **R4 — `DML_*` env vars do not cross `wsl.exe`.** No `WSLENV` anywhere in the repo. The
  parity/bats/CLI-integration suites all inject these as override seams and those seams do not
  reach an Arch child. `for_this_host()` is written to not depend on any of them, and
  `compose::games_dir_from` already falls back to `$HOME/games` on purpose — but
  `DML_SOAP_*`, `DML_YQ_BIN`, `DML_BOT_ACCOUNT_PREFIX`, `DML_DOCKER`, `DML_LOG_SNAPSHOT_*` are
  now silently host-only on Arch.
* **R5 — `~/.dml` is the distro's.** `backups/`, `party-presets/`, `logs/`, `wowhead-cache`.
  The interval-backup watcher is gated on `Backend::Native`; the backup *page* commands are not
  obviously so.
* **R3 — `--exec` changes argument splitting** relative to `Wsl`'s bare `--`. This is a FIX
  (MOTD text, mail bodies, names with spaces now arrive intact), but it is an observable
  behaviour change and anything pinned to the old word-splitting will move.
* **R7 — `version` reports `"backend":"native"`** unconditionally (run.rs:295). False on Arch.
  Cosmetic, but misleading to anyone reading the envelope.
* **R8 — the bundled `dml-wow` may be a stub.** Verify `provision.rs`'s ELF re-check fires on a
  real mis-built installer *before* the flip makes it a default-path failure.
* **R9 — CI cannot validate this yet.** The 184 s unbounded-call test wedges
  `cargo test --workspace`; T1/T3 live in that workspace. Land that fix first or a green run
  proves less than it looks.
* **Scope question:** `games catalog` / `games install` routing to bash *inside the distro*
  arms the Library Install button for real. Confirm that is wanted in v0.1.0.

**Not blocking:** `backend_mode()` still returns `"wsl"` for Arch (lib.rs:1466). Nothing in this
design needs it to change — every `mode === "native" ? A : B` ternary picks the WSL command,
which is now translated. Only widen `BackendMode` if a later round wants Arch-specific copy;
the two comparisons that would care are `Backups.svelte:158-159` and
`page-cache.svelte.ts:66`'s `pickConfigReader`.

## 8. Judgement calls made autonomously

1. **`games list` / `games status` started as `Bash` rows** and flipped to `DmlWow` in step 5,
   so every commit is green on its own rather than step 2 landing red.
2. **The lib target exposes only `cli`.** Routing the binary through it would mean widening
   `out.rs`'s deliberately `pub(crate)` write helpers to satisfy a test. Price, stated: `cli.rs`
   compiles twice.
3. **`EngineKind::for_host(is_windows)` decides by platform, not by `backend::selected()`.**
   The latter would read `Wsl` inside the distro whatever the user chose (R4), and would send a
   hand-run `dml-wow.exe start` on Windows down the systemd path.
4. **`expect` rather than a silent push** when an Arch prefix is empty: a malformed runner would
   otherwise spawn `wsl.exe -d … --exec status`, a wrong command that looks like a right one.
5. **`strip_comments` is duplicated** between the launcher test module and
   `tests/vocab_surface.rs`. They are different crates; a dev-dependency crate for sixty lines
   of test utility is more machinery than the problem deserves.
6. **`install_native`'s `ensure_engine` no longer short-circuits** on "no Docker Desktop exe"
   when the host's starter is systemd — it was reasoning from a Desktop-only fact after the
   semantics of `ensure_engine_up_stream` changed underneath it.
7. **R6 left unfixed**, with reasoning in §7.
8. **Contract doc corrected**, not just extended: the `wow docker-restart` rationale was built
   on "`dml-wow` is the native Windows binary … never runs inside `dml-arch`", which this
   backend makes false. The conclusion survives on different footing (redundant, not
   impossible); the native-backend half of the argument is untouched.
