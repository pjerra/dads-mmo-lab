# SHIP-LIST — the ordered path from "613 commits, nothing released" to "v0.1.0 in someone else's hands"

Written 2026-07-28 out of a grilling session. This is **not** a feature roadmap.
Every item here exists to get the existing work in front of real users. Nothing
on this list adds a capability.

**The one rule that makes the rest work: no new features until Phase 4 is done.**
No new spec, no new "round", no new page. If an idea arrives, it goes in
`docs/superpowers/specs/` and waits.

---

## The native install is PROVEN (2026-07-31)

First end-to-end native install on real hardware: 8/8 stages, exit 0, **21m18s**.

It found one bug no unit test could have. The build overlay named no
`dockerfile:`, and AzerothCore keeps its Dockerfile at `apps/docker/Dockerfile`,
so Compose looked for `<checkout>/Dockerfile` and died after 600+ MB of clone and
five green stages. Every test in this repo drives a FAKE docker that never opens
the file, so the well-formed YAML and correct stage names sailed through. Fixed,
with a regression test that reads the rendered overlay.

Compose-project isolation held in reality, not only in a unit test:
`dml-native-test-0205b295_db-data` sits beside the untouched
`dml-wow-native_db-data`.

The tree that produced a working server -- what the SHA pin should record:

| repo | SHA |
|---|---|
| core (`mod-playerbots/azerothcore-wotlk`, branch `Playerbot`) | `190184a04539937a617bf033e39378196c0c63f5` |
| module (`mod-playerbots`) | `ba46fcdecde3d0c6c2f244fcb3ea862430b6ae5b` |

**The `native-test` title dir is KEPT ON PURPOSE** -- stopped, not deleted. It is
the only proven native install, and the launcher-wiring work needs one to wire
against; rebuilding costs 21+ minutes. AMENDED 2026-08-01 (roadmap): it stays
KEPT even after the wiring — the warm Docker cache is worth the disk.

THREE THINGS THIS RUN DID NOT PROVE, so none of them may be assumed:

1. **A COLD build.** The AzerothCore core compiled at 4.7 objects/sec off a warm
   layer/ccache from the existing server on this machine; only playerbots' AI
   compiled cold, at 1.2/sec. A stranger with no cache still pays the full hours.
2. **RESUME AFTER A GENERATOR FIX.** `generate-compose` was recorded done, so a
   plain re-run reuses the OLD generated file and silently skips the fix. The
   state file had to be deleted by hand. Real design gap.
3. **THE MEMORY HEADROOM.** Preflight warned: "8 CPUs but only 15.6 GB -- room
   for 7 jobs, not 8. Nothing caps build parallelism for you here." The
   worldserver link survived, but got lucky rather than proven; the compose
   templates still pass no `-j` limit.

## Known flake (found 2026-07-30, NOT investigated)

`cli/tests/soap.bats` test 6, "wow soap-exec returns result envelope on success",
failed once in a full-suite run and passed on the next full run and in isolation
(10/10). Nothing in that code path was touched that day. NB the original
"cross-test contamination of `~/.dml/soap.env`" hypothesis was DISPROVED
(roadmap §A3, 2026-08-03: `setup()` exports `HOME="$FIXTURE"` from a fresh
`mktemp -d` per test, so tests cannot see each other's soap.env; two real
hazards were found and closed instead). If it flakes again, start from §A3's
findings — not from the contamination theory.

Worth fixing before the beta, for one reason: an intermittently red suite is
indistinguishable from a real regression, and this repo has already lost time to
exactly that (a build race between bats and the cargo parity suites produced ~450
fake failures). A gate you have to re-run to believe is not a gate.

## Decisions taken (user, 2026-07-30)

Recorded here because a decision that lives only in a conversation is a decision
that gets lost. All four were put to the user with recommendations and all four
were taken as recommended.

1. ~~**v0.1.0 beta scope: WSL-only.**~~ **REVERSED by the user, 2026-08-01:
   THE BETA WAITS FOR NATIVE.** The original reasoning was that native could not
   install a title and had no readiness wait. Both premises are now false:
   * `install-native` ran END TO END on real hardware for the first time on
     2026-07-31 -- 8/8 stages, exit 0, 21m18s, with `ac-worldserver` and
     `ac-database` up and healthy. It is not a paper path.
   * The "no readiness wait" claim was only ever true of `games start`;
     `install_native.rs` has a real `Stage::Ready` (1800s timeout, 10s poll,
     project-scoped log matching, boot-loop watch armed).
   What remains is INTEGRATION, not architecture: the launcher is still not
   wired to the engine, backend auto-detect can never select native on a fresh
   machine (it requires a server dir that only the install creates), and native
   first-run is suppressed. Roughly 6-8 focused days.
   The user also wants `dml-arch` RETIRED. Docker Desktop's own distro cannot
   host the installers (Alpine, no sudo, no systemd, rebuilt on upgrade), so
   native IS the retirement path.
2. **Pin AzerothCore + mod-playerbots to known-good SHAs.** A stranger's install
   must be reproducible; today a bad upstream commit breaks installs for reasons
   we cannot reproduce locally, and each failure costs that user a full source
   build. We own the bumps. (Implementation is Phase 1 Task 1 — still to do.)
3. **WebView2: the prerequisites script handles it.** That script already has to
   enable WSL2 and install Docker Desktop, both of which need admin and a reboot,
   so the check costs nothing extra there and keeps the download small. Stated
   honestly, "download one file, double-click, play" is really "run one script
   once, then double-click forever" — see 4.0h.
4. **`docs/superpowers/plans/2026-07-25-common-writes-to-rust.md`: committed, and
   deferred to post-beta.** Tracked so it stops being invisible to git, the
   roadmap and any audit — the exact failure mode that already lost a
   user-approved perf-advisor spec and a 13-item feature batch — but explicitly
   NOT before the beta, because it competes with the five live gates that
   actually block the release.

---

## Phase 0 — stop the bleeding (30 minutes)

- [x] **0.1 — Clean the worktree.** DONE (as of 2026-08-10 the tree is clean
      apart from two untracked measurement dirs — `results/backend-comparison/`
      and `launcher/src-tauri/backend/`). The standing rule holds: never start
      a smoke pass on a dirty tree.
- [ ] **0.2 — Stop trying to merge to `main`. Change the default branch instead.**
      On GitHub (`pjerra/dads-mmo-lab` → Settings → Branches) set the default
      branch to **`rust-main`**.
      Leave `main` exactly where it is: an untouched mirror of `upstream/main`.
      That keeps `git pull upstream main` clean forever, and it means "what is
      the current work" has a one-word answer for anyone who clones the repo.
      Releases get tagged on `rust-main`. There is no merge ceremony to perform.
- [x] **0.3 — Delete the dead branches.** DONE 2026-07-30 (user decision).
      Measured first: SIX of the seven branches had **zero** commits not already
      in `rust-main`, so nothing needed merging and nothing was lost. `main` is
      byte-identical to `upstream/main` (0 commits each way) — exactly the
      untouched pull-mirror 0.2 wants, so it STAYS and is never merged into.
      `feat/multi-server-tray` STAYS: it has one unmerged commit, and it is
      blocked on per-install container names anyway (`ac-*` `container_name`s are
      global to the docker ENGINE, so one stack per PC until that lands).

      Deleted, local and on origin where they existed. Tip SHAs recorded so
      "deleted" never means "unrecoverable" — `git branch <name> <sha>` restores
      any of them, and every one of these commits is already an ancestor of
      `rust-main`:

      | branch | tip | last commit |
      |---|---|---|
      | `feat/backup-names-autobackup` | `ef035f0` | 2026-07-26 |
      | `feat/dml-launcher-windows` (+origin) | `a589cd5` | 2026-07-23 |
      | `feat/round2-launcher-batch` | `268b771` | 2026-07-28 |
      | `fix/games-folder-mnt-hints-updater` (+origin) | `38b3a9c` | 2026-07-14 |
      | `spike/docker-desktop-native` (+origin) | `568ea4c` | 2026-07-26 |
      | `origin/feat/rust-cli-workspace` (old name of `rust-main`) | `1c503d4` | — |

---

## Phase 1 — prove it works (the boring part; ~2-3 hours before beta)

`.superpowers/sdd/NATIVE-TAIL-SMOKE.md` currently has **zero of its boxes
checked**. `docs/SMOKE-TESTS.md` is at **51 green / 99 unchecked (measured
2026-08-10; the file has grown well past the original 44 rows)**. Those two files are the
definition of "ready". Until they are green, "it's not ready yet" is not a
judgement — it's just an untested build.

**You do not have to green the whole checklist before a beta.** You have to green
the parts where a bug costs a stranger their data or their evening. The rest is
what beta testers are *for* — and you currently have zero of those, so more
solo clicking has sharply diminishing returns.

Test against the disposable snapshot at `C:\Users\perzi\dml-native`, never the
real server.

**Required before the beta goes out:**

- [ ] **1.1 — NATIVE-TAIL-SMOKE section A (quick reads).** Pure eyeballing.
      ~30 min. If a read is broken, everything downstream lies to the user.
- [ ] **1.2 — Section B (low-risk actions).** ~1 hour. Backup create/list/delete
      is in here — that's the safety net every other risky action leans on.
- [ ] **1.3 — Section E (lifecycle + self-update).** ~1 hour. Start/Stop/Restart
      is the button 100% of users press, and self-update is the one that can
      break a working install remotely.
- [ ] **1.4 — The 3 sharp edges in Phase 3 below.** Especially the mid-flush
      kill test.

**Can ship as "untested, feedback welcome" and get greened by real testers:**

- [ ] **1.5 — Section C (party, needs a char online).**
- [ ] **1.6 — Section D (module family).**
- [ ] **1.7 — The 9 remaining rows of `docs/SMOKE-TESTS.md`** (SS3 titles,
      SS10 modules+rebuild, SS11 backups).
- [ ] **1.8 — The tuning-tab click-through** (`[guided-config]` rows). Headline
      test: `AllowMixedWeaponTypes` live-applies a sword→axe transmog.

Keep the "Enable untested features" toggle exactly as it is — it's what makes
this split honest. Anything not smoked stays behind it, and the release notes
say so.

**Rule for this whole phase:** a bug found during smoke gets **fixed
immediately**, before the next box. Do not collect bugs into a list to fix
later — that turns a 4-hour smoke pass into another two-week build round, which
is exactly how this project got to 613 unreleased commits.

---

## Phase 2 — get the number (1 hour)

Right now the case for Rust is "it feels faster". That loses an argument. A
measurement wins one.

- [x] **2.1 — DONE 2026-07-29** (median of 3, table in docs/rust-cli-pitch.md):
      `version` 30ms vs 174ms (5.7x), `games list` 31ms vs 314ms (10.2x).
      `status` was measured and DELIBERATELY NOT claimed: with the stack down it
      compares two docker failure-timeouts, not two implementations -- `docker
      info` alone costs 1017ms on this box when Docker Desktop is off. Left in
      the table as a non-result with the reason. `start`/`restart` and the
      config write are unmeasured: the first two are dominated by Docker, and
      the write mutates a live server so it belongs with the supervised smokes.
      SIDE FINDING worth its own line: ~1s of every native `status` is a dead
      `docker info` when the engine is down, and Home polls status every 7s.
- [ ] ~~**2.1 — Time bash vs Rust on the same box, same operations.**~~ At minimum:
      `status`, `games list`, `games start`, `games restart`, one config write.
      Three runs each, take the median, record milliseconds.
      - bash: `wsl.exe -d dml-arch -u dml -- dml <cmd> --json`
      - Rust: `target\release\dml-wow.exe <cmd>`
      - PowerShell: `Measure-Command { ... }`
- [ ] **2.2 — Put the table in `docs/rust-cli-pitch.md`.** Be honest in it: if
      most of `start`'s wall-clock is Docker pulling containers, say so. A pitch
      that admits its own limits is the one people believe.

This table is the single most useful artifact for the conversation in Phase 5.1.

---

## Phase 3 — the three sharp edges (do these while smoking, they're small)

- [x] **3.1 — DONE, and the job was far smaller than this item assumed.**
      Audited 2026-07-29. Of ~76 `unwrap()`/`expect()` calls across the three
      files, only **five** are in production code; the rest are inside
      `#[cfg(test)]`, where panicking is the correct behaviour. All five are the
      same idiom -- `child.stdout.take().expect("stdout was piped")` -- and all
      five are provably unreachable: the `Stdio::piped()` call that guarantees
      the handle sits TWO LINES ABOVE each one (restore.rs:148, backup.rs:728),
      and they run before any data flows, so none can fire mid-write.
      `backup.rs` correctly takes only the two handles it pipes (stdin is
      `Stdio::null()`).
      The wider check was also clean: no `panic!`, `assert!`, `unreachable!` or
      `todo!` anywhere in those files' production paths, and every
      `unwrap_or_else` supplies a DEFAULT VALUE rather than panicking.
      So there is nothing to convert. Converting them would add error paths that
      cannot be taken, for a risk that does not exist. The item's instinct was
      right -- those three files ARE the only place a panic costs characters --
      but the panics it feared were never there.
      (Original item kept below for the reasoning, which is still correct.)

- [ ] ~~**3.1 — Audit `unwrap()`/`expect()` in exactly three files:**~~
      `crates/dml-wow/src/restore.rs`, `destructive.rs`, `backup.rs`.
      There are ~874 across the workspace; **do not audit all of them.** Almost
      everywhere a panic is just a crash and the user restarts the app. In those
      three files a panic happens mid-write to a live character database, and
      the cost is lost characters. That is the only place the difference
      matters. Convert those to real error returns.
- [ ] **3.2 — Test the FlushGuard hole for real.** `FlushGuard` is a `Drop`
      guard, and `Drop` does **not** run on `taskkill`, a power cut, or a
      Windows update reboot. The `.dml-bot-flush-armed` breadcrumb plus the
      games-start heal is what actually saves you — the `Drop` is only the happy
      path. So test the unhappy one: arm a flush, kill the launcher from Task
      Manager mid-flush, start the server, confirm the heal fires. (This row is
      already on the smoke checklist — don't skip it, it's the one that proves
      the design.)
- [ ] **3.3 — Decide the code-signing question now, not at release.** Two valid
      answers: (a) ship unsigned, and write the SmartScreen click-through into
      the README with a screenshot, or (b) buy a cert (~$200/yr). Pick one
      today. Discovering this on release day is what turns a launch into a
      week of "is this a virus?" messages.

---

## Phase 4 — first-run must work (the actual release gate)

**The blocker is not the title install. It is that the launcher cannot set
itself up on anybody else's computer.** Evidence, all verified 2026-07-28:

- `cli/dev-install.ps1` — the only thing that installs the `dml` CLI, the lua
  bridges and the six installer scripts into the distro — has
  `/mnt/c/Users/perzi/dads-mmo-lab` **hardcoded on line 3**. It cannot run on
  anyone else's machine.
- `launcher/src-tauri/tauri.conf.json` has **no `bundle.resources`**. The MSI
  and NSIS therefore ship the GUI and nothing else — no `cli/dml`, no
  `cli/lua/**`, no `guides/*/install-*.sh`.
- The error a stranger will actually hit is hardcoded in
  `crates/dml-core/src/error.rs:25`:
  *"Is the dml CLI v3.0.0 installed? Run: `powershell -File cli\dev-install.ps1`"*
  — a file they do not have, which would fail if they did.
- There is **no first-run/onboarding screen** anywhere in
  `launcher/src/lib/pages/`. A new user lands on Home, sees a status card for a
  server that does not exist, and has nothing to click.

So today the install path is: *clone the repo, be called perzi, live at
C:\Users\perzi.* Everything else on this list is downstream of fixing that.

- [x] **4.1 — DONE 2026-07-28.** `bundle.resources` in `tauri.conf.json` carries
      `cli/dml`, `cli/lua/party`, `cli/lua/gm` and the six title installers;
      the layout is owned by `launcher/src-tauri/src/payload.rs`, which FAILS
      THE TEST RUN if the manifest drifts from what the code expects.
- [x] **4.2 — DONE 2026-07-28.** `backend_setup`
      (`launcher/src-tauri/src/provision.rs`) provisions the distro from the
      bundled resources, streamed over the usual `Channel<Value>` TermEvent
      seam, idempotent, and it CONSUMES the `backend_status` probe chain both
      before and after rather than asking its own questions. `dev-install.ps1`
      is now the dev loop only, and `provision.rs`'s
      `dev_install_ps1_installs_the_same_destinations_at_the_same_modes` reads
      the `.ps1` and fails when the two drift.
- [x] **4.3 — DONE.** `crates/dml-core/src/error.rs`'s hint now names the
      backend/distro state ("is WSL + the dml-arch distro present?" / native
      mode's Git Bash + Docker Desktop), not a dev script.
- [ ] **4.4 — Add a first-run screen.** Detect, in order: no `dml-arch` distro
      → no `dml` CLI in it → no titles installed. Each state gets one sentence
      and one button. A stranger must never see an empty status card with no
      next step.
- [x] **4.0 — WebView2 is not guaranteed to be present.** FOUND ON THE VM,
      2026-07-28: the launcher would not start -- "missing WebView2". The
      config had no `bundle.windows` block at all, so the install mode was
      whatever Tauri defaulted to. Now declared explicitly as
      `embedBootstrapper` (ships the bootstrapper inside the installer instead
      of fetching it mid-install). ESCALATION PATH if debloated/modified
      Windows images keep failing: `fixedRuntime` embeds a WebView2 build in
      the app and needs no system-wide install at all (~180MB). Decide which
      once we know whether the failure was at install time or at launch.

- [x] **4.0b — Two launchers, and a stranger cannot tell them apart.** FOUND ON
      THE VM, 2026-07-28: after `Install-DML.ps1` + the new installer, the
      tester could not find the app they had just installed -- the Start menu
      offers the OLD C# tray (`C:\DML\DML-Launcher.exe`, installed by
      Install-DML.ps1) alongside the new one (`DML Launcher\launcher.exe`),
      differing only by a hyphen. Both were running simultaneously on the dev
      box too. Cheapest honest fix for v0.1.0: stop installing the C# tray from
      Install-DML.ps1, or rename ours so the two are visibly different. Until
      then it MUST be in the release notes -- "which of these two icons is the
      app?" is a first-30-seconds question, and this is item 7 of the recovered
      backlog (retire the C# tray) arriving as a real user-facing bug.

      MEASURED ON THE VM, shortcut targets resolved:
          Startup\DML Launcher.lnk  -> C:\DML\DML-Launcher.exe          (OLD)
          Desktop\DML Launcher.lnk  -> C:\DML\DML-Launcher.exe          (OLD)
          Programs\DML Launcher.lnk -> %LOCALAPPDATA%\DML Launcher\launcher.exe (NEW)
      All three carry the SAME display name. The desktop icon and the login
      auto-start are both the OLD app; the new one is reachable only from the
      Start-menu list. The tester -- who built the thing -- could not find it.
      A stranger has no chance, and any old-tray bug gets reported against the
      new app. Fix before any release: stop Install-DML.ps1 creating those two
      shortcuts (and installing the C# tray at all).

      **FIXED 2026-07-30.** The C# tray is retired: `Install-DML.ps1` no longer
      builds it or creates its two shortcuts. It is gated behind an opt-in
      `-LegacyTray` switch rather than deleted, because the embedded C# is ~900
      lines inside a file that also carries the bootstrap CLI here-string, and a
      reversible gate is the smaller, safer diff.

      Removing the CREATION was NOT enough on its own, and that is the part worth
      remembering: every box that already ran an older installer still had both
      shortcuts, still pointing at the old exe, so the confusion would have
      survived the upgrade untouched. `Remove-LegacyTrayShortcuts` now cleans them
      up on every run — but ONLY when the shortcut's resolved `TargetPath` really
      is the retired exe. Anyone who hit this confusion may well have repointed
      "DML Launcher.lnk" at the new app themselves, and deleting that would undo
      their own fix. That safety property is the one the mutation test proves:
      making the cleanup unconditional turns "a shortcut repointed at the NEW
      launcher is left alone" red.
      The installer's closing summary also claimed "DML Launcher is on your
      Desktop and starts with Windows" — true only of the retired app — and now
      points at the real next step instead.
      Harness: `guides/DML-Windows/tests/Test-InstallerDefender.ps1`, 128 -> 138
      checks. The three static checks assert ordering, not mere presence: both
      shortcut lines still exist in the file (inside the opt-in branch), so
      grepping for them would prove nothing.

      ONE OBSERVABLE CHECK LEFT FOR THE USER, 10 seconds during the next
      installer run: after installing the launcher, **is there a Desktop icon for
      DML Launcher, and does the Start-menu entry open the new app?**
      `tauri.conf.json` has `nsis: {}` — no explicit shortcut configuration — and
      whether Tauri's NSIS template creates a Desktop shortcut by default could
      not be settled here: the bundler template is not in the cargo registry (it
      ships prebuilt inside the npm CLI) and the built installer's script data is
      LZMA-compressed, so grepping the exe is inconclusive. Guessing at an `nsis`
      config key could break the installer build, which is worse than the current
      state. If the answer is "no Desktop icon", that is a one-line config fix —
      but it must be made against an observation, not an assumption.

- [ ] **4.0c — The title installer talks to Steam Deck owners.** Seen on the VM
      run, 2026-07-28: the WoW installer says "This will take 2-4 hours on your
      Steam Deck", "Keep it plugged in and on a hard flat surface", "The fan
      will be loud", and "Keep your Steam Deck plugged in!". A Windows user
      installing from the launcher is not on a Steam Deck, and the timing
      estimate is wrong for their hardware too (a 4-vCPU VM is slower). These
      are shared scripts, so the fix is conditional copy rather than a rewrite:
      detect the host and say something true. Cheap, and it is the first long
      wait a new user ever sits through -- the moment they most need to trust
      what they are reading.

- [ ] **4.0d — Known, recorded, NOT fixed (Phase 4 review residue, 2026-07-29).**
      Found by verification, judged not worth another corrective wave at 03:00
      after three waves on the same code. None blocks a release; all are real.
      - `classify_wsl_list`: exit 0 + empty stdout + ANY stderr byte is a shrug,
        where the same input with a silent stderr settles as NoDistro. A routine
        wsl.exe proxy advisory therefore costs a user the "no distro" screen.
      - A probe TIMEOUT (`CouldNotTell`) carries no detail text, so the one
        screen with no repair on it shows a blank diagnostics line — and the new
        120s cold budget makes timeouts the likeliest could-not-tell.
      - The CLI_BAD_OUTPUT hint's "names a real button" assertion moved from
        dml-core to the launcher crate, which the Linux CI job does not build.
        The pin is now Windows-only.
      - `the_bundled_resource_dirs_match_the_repo` asserts on the state of
        `target/<profile>/` rather than on code; its stale-file half reds on a
        condition build.rs deliberately declines to fix, so it can fail for a
        developer who did nothing wrong.
      - Three frontend copy tests match prose with regexes. They are
        change-detectors: ordinary rewording reds them.
      - Three `classify_wsl_list` tests pass verbatim against the PRE-Phase-4
        classifier. They are happy-path guard rails, not evidence for the
        change, and two read as if they pin the reordering. Worth relabelling
        so nobody mistakes them for coverage.

- [ ] **4.0e — A FRESH INSTALL PRODUCES A SERVER THE LAUNCHER CANNOT TALK TO.**
      Found on the clean VM, 2026-07-29.

      **CORRECTION, 2026-07-29 (diagnosed over SSH against the live VM): item 1
      below was WRONG and is struck out.** SOAP was reachable the whole time.
      `docker port` showed `7878/tcp -> 127.0.0.1:7878`, the worldserver log
      showed `Found config value 'SOAP.IP' from environment variable
      'AC_SOAP_IP'`, and `dml wow server-info` returned **`SOAP_AUTH`** — an
      HTTP 401, which is proof the world ANSWERED and rejected the login. The
      `SOAP.IP = "127.0.0.1"` the user pasted was the conf-file default, already
      overridden by the env var. Anyone acting on the struck-out theory would
      re-plumb networking that was never broken.

      **The actual cause: a fresh install has NO SOAP ACCOUNT.** `~/.dml/soap.env`
      does not exist and the AC docker image's `admin/admin` is not usable for
      SOAP, so every SOAP feature fails auth. Creating a GM3 account is a manual
      worldserver-console step no button performs.

      1. ~~The title installer leaves `SOAP.IP = "127.0.0.1"`, i.e. SOAP bound to
         the CONTAINER's own loopback.~~ **DISPROVEN — see the correction above.**
      2. The remedy, `dml wow soap-setup`, hard-requires `yq` — which NOTHING
         installed. The installer's pacman line had `jq`, not `go-yq`. So the bug
         blocked its own fix, and the error told the user to run pacman by hand in
         a product whose premise is that they never open a terminal.
      FIXED SO FAR: `go-yq` added to Install-DML.ps1's phase3 (fresh installs).
      STILL TO DO: (a) make the title install enable SOAP itself so no
      post-install step is needed at all; (b) have the launcher's backend
      provisioning ensure `yq`, since that is what reaches EXISTING installs —
      the installer only helps new ones (same lesson as the dml-start.sh
      boot-loop finding); (c) stop the soap_unreachable card blaming Docker when
      the real cause is that SOAP was never reachable in the first place.
      The new native compose generator already prevents this class by shipping
      SOAP enabled and bound to 0.0.0.0 by default.

- [x] **4.0f — `AC_*` env vars SILENTLY DISCARD the launcher's own config saves.**
      Found live on the VM, 2026-07-29, and it is a release-grade honesty bug: the
      user changed the world bot population in Bot World, saved, restarted — and
      the old value came back, five times. Cause: the title installer's
      `docker-compose.override.yml` sets
      `AC_AI_PLAYERBOT_MIN/MAX_RANDOM_BOTS: 1600/2000`, the AzerothCore image
      applies `AC_*` env vars ON TOP of `playerbots.conf`, and the launcher's
      editor writes the conf. So the save landed, was reported as saved, and was
      then overridden at boot. `server-detail` even reported `bots.max: 100` from
      the conf while the server ran 2000 — the UI and the server disagreed and
      neither was lying about what it read.
      IMMEDIATE FIX (given to the user): delete the two `_RANDOM_BOTS` lines from
      the override, then `docker compose up -d` (NOT restart — restart reuses the
      old container's environment).
      **ROOT CAUSE FOUND 2026-07-30, and it is NOT the installer's env keys.**
      The save path is already correct: `config set bots.population` writes the
      conf AND removes both legacy `AC_*` env keys from
      `docker-compose.override.yml` (pinned by `wow-config-pb.bats` "removes both
      legacy envs", and the conf-row route does the same — "legacy env override is
      removed and forces restart even with SOAP up"). The edit landed.

      What fails is **APPLYING** it. The launcher then says "restart to apply",
      and its restart button calls `wow_world_restart` →
      `lifecycle::world_restart_stream` → **`docker restart -t 300
      ac-worldserver` and nothing else** (`lifecycle.rs`, verified: no `compose
      up` anywhere on that path). `docker restart` restarts the SAME container
      object, and a container's environment is fixed when it is CREATED. So the
      env keys were removed from the file, the container kept them anyway, and the
      old value came back — exactly five times, as reported.

      Two distinct kinds of apply are being conflated:
        * a conf-FILE change needs only the worldserver process restarted →
          `docker restart` is sufficient;
        * an override/env change (including REMOVING a shadowing key) needs the
          container RECREATED → `docker compose up -d`, which is what `games
          restart` does (it goes down → up) but `world-restart` does not.
      `config set bots.population` does BOTH kinds at once, so it needs a
      recreate, and the UI offers the one action that cannot deliver it.

      **CONFIRMED AGAINST THE REAL BOX, 2026-07-30.** There are TWO servers on
      this machine and they differ exactly where it matters. The WSL install
      (`~/games/wow-server-playerbots`) has `AC_AI_PLAYERBOT_MIN/MAX_RANDOM_BOTS:
      1600/2000` in its override AND a `playerbots.conf` that does not set those
      keys at all -- so the running bot count comes ENTIRELY from env, and the
      first population save both writes the conf and removes the env, which only
      a recreate can apply. The native install (`C:\Users\perzi\dml-native`) has
      no env shadow and a plain 500/500 conf. With no `~/.dml/launcher.json` the
      backend auto-detects, which on this box resolves to the WSL one -- the
      shadowed one. The reproduction is on disk, not inferred.

      (a) DONE -- `apply_needed: recreate | world-restart | none` now rides along
      with `restart_required` on BOTH surfaces (bash `cli/src/90-main.sh` and
      native `crates/dml-wow`), for `config set` (both routes, built in one place
      by `config::cfgset_outcome` so they cannot drift), `config tuning-set`, and
      `bridge-setup`. Mutation-verified on both surfaces.
      (b) DONE -- `launcher/src/lib/apply-needed.ts` (pure, 15 vitest cases) owns
      the ranking, the escalation (a multi-row save keeps the STRONGEST answer,
      never the last one) and the copy. Three things changed in the UI: the
      banner now NAMES the button instead of saying "restart the server"; the
      banner is shown on **Home**, where the restart buttons actually are (before
      this the user had to navigate away from the advice to reach the button);
      and "Restart world only" is DISABLED while a recreate is pending, with the
      reason in its tooltip -- a tooltip alone was already there and was not
      enough. A full Restart or a cold Start clears the pending state, and only
      on success.
      An absent `apply_needed` resolves to `recreate`, the STRONGER apply -- an
      older `dml` in the distro then does one needless slow restart instead of
      silently failing to apply. Guessing the weaker answer is the bug itself.
      (c) DONE -- user decision the same day, taken with the measured
      consequence in front of them: `AC_AI_PLAYERBOT_MIN/MAX_RANDOM_BOTS` are gone
      from ALL FOUR installers (Arch, Ubuntu, Fedora at 1600/2000, and
      `Install-WoW-WotLK.ps1` which set 200/250 in TWO places -- the installers had
      already drifted from each other). A fresh install now takes the module's own
      `.dist` default of **500/500** (measured, not assumed): so the Linux routes
      drop from 2000 to 500 and the PS1 route RISES from 250 to 500. Existing
      installs are untouched and keep their env keys until a save migrates them.
      This was not a new policy -- `composegen.rs`'s "the shadowing rule" already
      said a generated override must not carry env keys that shadow curated rows,
      with a tripwire enforcing it on the native side. The installers were simply
      out of step.
      Tripwire: `installers_carry_no_bot_count_env_keys`. Its first version was
      itself broken and adversarial review caught it -- the forbidden set came only
      from registry rows, and `AiPlayerbot.MinRandomBots` is NOT a registry row (it
      is a hard-coded companion write), so a test named "no bot count env keys"
      would have passed while an installer re-pinned the bot FLOOR at 1600. It now
      unions in the companion keys, DISCOVERS the installer files instead of
      hard-coding them, and accepts `KEY=value` as well as `KEY: value`. All three
      gaps are mutation-proven.
      NB this also explains why the workaround that DID work was `docker compose
      up -d` rather than a restart — that was the right instruction for the wrong
      stated reason.
      NB a config save that silently does nothing is worse than a refusal: it
      teaches the user the product does not work.

- [ ] **4.0g — Tailscale "Play Together" could never complete a login.** FOUND
      ON THE VM, 2026-07-29 — **FIXED the same day**, both surfaces,
      mutation-verified. `tailscale up --timeout=8s` gave up before the control
      plane answered (measured at **30s** in tailscaled's own journal), so the
      user got "timeout waiting for Tailscale service to enter a Running state"
      and never received the auth URL that is the entire point of the flow. Fix:
      45s default via `DML_TS_UP_TIMEOUT`, an outer bound guaranteed to outlive
      the inner timeout, the pending URL recovered from `tailscale status --json`
      when `up` printed none, and the daemon-start failure REPORTED rather than
      discarded. Full detail in the post-smoke roadmap. Not a release blocker
      (Play Together is optional) but it was 100% broken for everyone.

- [ ] **4.0h — "DOWNLOAD ONE FILE, DOUBLE-CLICK, PLAY" (user requirement,
      2026-07-30).** The user does not want an installer: one file, fast, the
      consumer just uses it. Related standing decision: **the launcher IS the
      product on Windows** — CLIs and scripts are bootstrap plumbing, never the
      user surface.
      He asked whether a prerequisites `.ps1` could do the checking. RECOMMENDED
      ANSWER: the logic yes, a user-facing `.ps1` no. A DOWNLOADED script is a
      worse front door than an installer — Windows blocks downloaded scripts by
      default, so the user must Unblock the file or type an `-ExecutionPolicy
      Bypass` incantation, and it becomes a second artifact to keep in sync with
      the launcher. Instead: ONE button in the launcher ("Set up my PC") that
      probes and provisions, invoking PowerShell/DISM INTERNALLY as an
      implementation detail. No script on disk means no execution-policy prompt.
      Most of this already exists — `backend_status` (`dml_core::setup`) answers
      "what is the first thing missing" and `backend_setup` (`provision.rs`)
      provisions from bundled resources.
      THE THREE REAL OBSTACLES, so nobody plans around a fantasy:
      1. **The payload ships as `bundle.resources`, i.e. files NEXT TO the exe —
         an installer concept.** A single portable exe must embed it
         (`include_bytes!`) or fetch it on first run. This is the biggest delta.
      2. **WebView2.** Win11 ships it; Win10 may not, and `embedBootstrapper`
         (item 4.0) is an INSTALLER feature. A portable exe on a machine without
         WebView2 fails exactly as the VM did. Either detect-and-offer, or accept
         `fixedRuntime`'s ~180 MB.
      3. **Enabling WSL/Hyper-V needs elevation AND a reboot.** A portable exe can
         request UAC; it cannot avoid the reboot.
      HONEST SCOPE: true one-file-double-click works today for Win11 machines that
      already have WSL or Docker. Everyone else needs a fallback, and pretending
      otherwise is how 4.0 and 4.0e happened. Decide the fallback before shipping
      the portable exe, not after.

- [ ] **4.5 — Decide what v0.1.0 covers.** Cheapest honest beta: **WSL mode
      works end to end; native mode is the faster path for people who already
      have a server.** That ships in days. Native title install then becomes
      v0.2.0 rather than a release blocker. (Note: 4.1/4.2 build exactly the
      plumbing Phase 6.3 needs to ship the Linux `dml-wow` binary into the
      distro later — this work is not throwaway.)
- [ ] **4.6 — Test on a machine that has never seen DML. IN PROGRESS since
      2026-07-28** on a clean Windows 11 Pro VM (Hyper-V, 2 vCPU / 4 GB, nested
      virtualisation enabled after `ExposeVirtualizationExtensions` was turned
      on). It has already paid for itself: EVERY blocker in 4.0-4.0g was found
      there and NONE of them reproduced on the dev machine. Reached so far:
      installer → distro → WoW WotLK installed → server reaches ready (2m19s) →
      status green. Still unverified BY A HUMAN on that box: the five Lab-parity
      rounds (GM Tools, My Party, Summon NPCs, party presets, Backups) and the
      module config-files page. That untested surface — not the remaining
      checkboxes — is the real distance to a beta.
      A fresh Windows VM,
      or a friend's PC. No Docker, no WSL, no Rust, no Node, no repo. Download
      → install → running server, following only the README.
      **This is the release gate.** Everything else is polish. If a stranger
      can't complete that path, the release is not ready — and no amount of
      additional features changes that.

---

## Phase 5 — release

**Social context, so future sessions don't re-litigate it:** a project mod
(Baerthe) has explicitly said, twice, to set this up as a GitHub release and
post it in the server. Permission to release is **not** an open question.
Three things are still open, and they're what 5.1-5.3 are about:

- The project leader (James) has been DM'd and has not replied — he is
  reportedly swamped with work and volunteering, and the mod has offered to
  raise it with him. Silence here is almost certainly bandwidth, not rejection.
- **Nobody has ever run this software except its author.** The mod has no
  Windows machine and said so.
- There are **three concurrent rewrites** in flight: James's revised DML, the
  mod's planned MAUI C# app, and this. Nobody has shown anyone else a roadmap.

- [ ] **5.1 — Get ONE other human to run it before the public post.** Any
      Windows user in the community. This is now the single highest-value
      action available — higher than any remaining feature, higher than more
      solo smoke testing. One stranger's first 10 minutes will find more than
      another week of your own clicking, because you can no longer see the
      parts that are only obvious to you.
- [ ] **5.2 — Send James one short message that asks nothing.** Not a pitch,
      not a proposal, no decision requested: "Built a Windows/Rust launcher on
      top of DML. It's AGPL and public, take anything useful from it, no reply
      needed — link + 2-min video + speed numbers." A message with no ask can't
      be a demand on a busy person, and it can't be used by anyone else as
      leverage against him. Attach the Phase 2 table.
- [ ] **5.3 — Do not let this be framed as "cornering James."** If the
      launcher is introduced as evidence that the leader's universal-DML design
      is wrong, its reception stops depending on the code. Frame it as an
      addition — "here is a Windows front end, it speaks the JSON contract" —
      and let the architecture argument be a separate conversation that other
      people can have without your name on the ammunition.
- [ ] **5.4 — Give it its own name.** James has a revised DML in progress. Two
      things called "the DML launcher" is how a contribution turns into a
      competitor by accident. A distinct name makes coexistence the default
      reading.
- [ ] **5.5 — Tag `v0.1.0` on `rust-main`.** `npm run tauri build` →
      NSIS + MSI land under the **root** `target/release/bundle/`.
- [ ] **5.6 — Attach the installers to a GitHub Release** with honest notes:
      what's smoked and what's still behind "Enable untested features", that
      it's unsigned and what SmartScreen will say, that title installs need WSL
      mode (until Phase 4 lands), and that AGPL-3.0 means the source goes with
      it.
- [ ] **5.7 — Call it beta and mean it.** Ask for bug reports, not stars.
- [ ] **5.8 — Ask the other two what they're building.** One message each to
      James and the mod: "what's the shape of your version, so I don't build the
      same thing twice?" Three parallel rewrites with no shared roadmap is how
      all three die. This costs one message and can save a month.

---

## Deliberately NOT on this list

- **Rewriting `guides/*/install-*.sh` in Rust.** They run `pacman`, `apt`,
  `systemctl`, `usermod`. They are Linux system scripts and they should stay
  bash forever. This was already decided correctly.
- **`dml doctor` in Rust.** Same reason — distro diagnostics belong in the
  distro.
- **Any new feature.** Including the Server Performance Advisor spec. It waits.

---

## Phase 6 — kill the CLI bash (ONLY after Phases 1-5; this is next month's job)

This removes the "a fix on one surface only half-ships" tax permanently. It is
the right end state, and it is also the single most dangerous refactor left, so
it goes **after** a release, not before one.

The order below is not optional. Bash is currently your **test oracle** — all 18
`crates/dml-wow/tests/*_parity.rs` suites work by running `cli/dml` and
comparing Rust's answer to it. Delete bash first and you delete the only proof
that the Rust is correct.

- [ ] **6.1 — Freeze the oracle.** Run every parity suite against live bash and
      write the bash outputs to golden files
      (`crates/dml-wow/tests/golden/<suite>/…`). Commit them.
- [ ] **6.2 — Rewrite the 18 suites** to diff Rust against those golden files
      instead of spawning bash. Confirm all 18 still pass with **zero SKIP
      lines**. This also fixes a real problem: parity currently can't run in CI
      because it needs a live server, so the number that matters only ever runs
      on your desk.
- [ ] **6.3 — Build `dml-wow` for Linux** (the Ubuntu CI job already proves it
      compiles) and install that binary into the `dml-arch` distro as part of
      `cli/dev-install.ps1`.
- [ ] **6.4 — Flip the WSL-mode spawn** from
      `wsl.exe -d dml-arch -u dml -- dml <cmd> --json` to
      `wsl.exe -d dml-arch -u dml -- dml-wow <cmd>`. One line, one surface.
- [ ] **6.5 — Delete** `cli/src/*.sh`, `cli/dml`, `cli/build.sh`,
      `cli/tests/*.bats` (~10,400 lines). **Keep** `guides/*/install-*.sh`.
- [ ] **6.6 — Delete the mirroring rules from `CLAUDE.md`** — the bash↔Rust
      mirror requirement, the "never run bats and cargo parity at the same time"
      gotcha, and the "regenerate the embedded data tables from the bash oracle"
      note. When those three paragraphs are gone, the port is genuinely done.

Also gone at 6.6: the `include_str!`-embedded registries in
`crates/dml-wow/data/` stop being copies of a bash source of truth and simply
become the source of truth.
