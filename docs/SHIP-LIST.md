# SHIP-LIST — the ordered path from "613 commits, nothing released" to "v0.1.0 in someone else's hands"

Written 2026-07-28 out of a grilling session. This is **not** a feature roadmap.
Every item here exists to get the existing work in front of real users. Nothing
on this list adds a capability.

**The one rule that makes the rest work: no new features until Phase 4 is done.**
No new spec, no new "round", no new page. If an idea arrives, it goes in
`docs/superpowers/specs/` and waits.

---

## Phase 0 — stop the bleeding (30 minutes)

- [ ] **0.1 — Clean the worktree.** 9 modified files + 3 untracked are sitting
      uncommitted on `rust-main`. Commit them or stash them. Never start a smoke
      pass on a dirty tree — you won't know whether a bug is in the build or in
      the half-finished edit.
- [ ] **0.2 — Stop trying to merge to `main`. Change the default branch instead.**
      On GitHub (`pjerra/dads-mmo-lab` → Settings → Branches) set the default
      branch to **`rust-main`**.
      Leave `main` exactly where it is: an untouched mirror of `upstream/main`.
      That keeps `git pull upstream main` clean forever, and it means "what is
      the current work" has a one-word answer for anyone who clones the repo.
      Releases get tagged on `rust-main`. There is no merge ceremony to perform.
- [ ] **0.3 — Delete the dead branches** or mark them historical in the README:
      `spike/docker-desktop-native` and `feat/round2-launcher-batch` are fully
      contained in `rust-main`. `feat/backup-names-autobackup` and
      `feat/dml-launcher-windows` need a decision: merged, or gone.

---

## Phase 1 — prove it works (the boring part; ~2-3 hours before beta)

`.superpowers/sdd/NATIVE-TAIL-SMOKE.md` currently has **zero of its boxes
checked**. `docs/SMOKE-TESTS.md` is at **35 of 44 rows**. Those two files are the
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

- [ ] **2.1 — Time bash vs Rust on the same box, same operations.** At minimum:
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

- [ ] **3.1 — Audit `unwrap()`/`expect()` in exactly three files:**
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

- [ ] **4.1 — Bundle the backend payload into the installer.** Add
      `bundle.resources` to `tauri.conf.json`: `cli/dml`, `cli/lua/**`,
      `guides/*/install-*.sh`. The exe must carry everything the distro needs.
- [ ] **4.2 — Replace `dev-install.ps1` with a real command.** A Tauri
      command that provisions the distro from the **bundled** resources
      (resolved via Tauri's resource dir, never a hardcoded path). Same
      `install -m` steps, no repo required. Keep `dev-install.ps1` for your own
      dev loop if you like, but it stops being the user's route.
- [ ] **4.3 — Fix the hint in `crates/dml-core/src/error.rs:25`.** It should
      point at the in-app setup button, not at a dev script.
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

- [ ] **4.0b — Two launchers, and a stranger cannot tell them apart.** FOUND ON
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

- [ ] **4.5 — Decide what v0.1.0 covers.** Cheapest honest beta: **WSL mode
      works end to end; native mode is the faster path for people who already
      have a server.** That ships in days. Native title install then becomes
      v0.2.0 rather than a release blocker. (Note: 4.1/4.2 build exactly the
      plumbing Phase 6.3 needs to ship the Linux `dml-wow` binary into the
      distro later — this work is not throwaway.)
- [ ] **4.6 — Test on a machine that has never seen DML.** A fresh Windows VM,
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
