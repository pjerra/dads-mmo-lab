# VM acceptance test — Round 2 (2026-08-07)

Supersedes the 2026-08-06 draft, which was never run. Written after Task 12
(one games-dir resolver), the auto-backend fix, and Task 6 (schema names
resolved end-to-end, both surfaces). Run this against a launcher built from
`feat/core-family` AFTER the B1 (bash mirror) chunk has landed — the plan
assumes both halves of Task 6 are in the build.

**Why a VM and not the dev box.** The dev machine hides at least seven things
a user's machine does not have, and two of them produced false green results
this week:

| Hidden on the dev box | A fresh VM has |
|---|---|
| `DML_GAMES_DIR`, `DML_BACKEND`, `DML_SCRIPT`, `DML_YQ_BIN` as User env vars | none — a whole branch passed `cargo test` here that would have failed CI |
| A working migrated server at `C:\Users\perzi\dml-native`, full of real data | nothing to read |
| Docker Desktop installed, WSL2 configured, engine auto-starting | nothing |
| The `dml-arch` WSL distro | nothing |
| Rust + MSVC + Node toolchains, `yq.exe` at a known path | WebView2 only |
| A warm BuildKit cache and pulled images | a 30–90 min build from zero |
| A compose env whose schema names happen to equal the old hardcoded ones | the same — which is why Phase 7 *edits* them |

---

## Phase 0 — VM preconditions (~30 min)

1. Windows 11 Pro, fresh, updated.
2. **Nested virtualisation ON** in the hypervisor — without it WSL2/Docker
   Desktop won't start and the failure masquerades as a DML bug.
3. 16 GB RAM minimum, 8 vCPU recommended; **150 GB free disk**.
4. **Install NOTHING else by hand.** No Docker, no Git, no Rust, no Node. The
   prerequisites are Phase 1's job, and installing them yourself is what made
   the 2026-08-09 round mis-test the product (see below).
5. Snapshot the VM as `bare` — clean Windows, nested virt on, nothing else.
   This is the only snapshot worth keeping: every round restores to it, so
   every round re-tests the prerequisite path too.

**WHY PHASE 1 CHANGED (2026-08-09).** The earlier revision of this plan said
"install Docker Desktop and nothing else, then copy the NSIS installer over and
run it". That is **side-loading**, and it skips the entire consumer path:
`guides/DML-Windows/Install-DML-Native.ps1` is the script a stranger actually
runs, and it is what installs Git for Windows, the pinned `yq.exe`, the
Defender exclusions and `~/.dml/launcher.json` — none of which the NSIS bundle
carries. Skipping it produced a `execvpe(/bin/bash) failed` WSL relay error on
the Library page of a launcher that was correctly on the native backend
(finding F2). `ROADMAP-TO-BETA.md` had already written the rule this round
broke: side-loading "tests the launcher but NOT the path a stranger takes. A
gate that skips the distribution step proves less than it appears to."

`DML_BACKEND` note: leave it unset for the main run. **One deliberate check**
later (23) sets it to `auto` — that used to silently mean WSL and is now fixed;
the VM proves the fix on the hardware class it was written for.

---

## Phase 1 — Install, the way a stranger does (~25 min + a reboot)

6. Copy `Install-DML-Native.ps1` and the freshly built NSIS installer to the VM
   (`scripts/vm-test/serve-vm-files.ps1` on the dev box stages and serves both;
   `fetch-vm-files.ps1` pulls them and verifies every SHA256).
7. Run the prerequisite script in an **elevated** PowerShell:

   ```
   powershell -ExecutionPolicy Bypass -File Install-DML-Native.ps1 `
       -InstallDocker -InstallGit -NoLauncher
   ```

   **Watch what it does — this is a tested surface, not just setup.** It should
   name a missing BIOS virtualization bit before any 600 MB download, enable WSL
   with `wsl --install --no-distribution`, queue itself in RunOnce and ask for a
   restart, then on resume install Docker Desktop and Git via winget, drop a
   hash-verified `yq.exe` into `<GamesDir>\tools\`, apply the Defender exclusion
   BEFORE any build, and write `~/.dml/launcher.json` with `backend=native`.
   Ending in "Ready." is the pass; ending in "Not ready yet:" with a named
   problem is a legitimate outcome to record, not a crash.

   **`-NoLauncher` is deliberate and must not be dropped for this round.**
   Without it the script installs the launcher from the newest GitHub *release*
   — `v0.1.0-rc1`, and this plan exists to test a branch build that is not
   released. Its own launcher-install path is therefore NOT covered here; test
   that separately at release time, or it will never be tested at all.
8. Now install the build under test: run the NSIS installer by hand.
   SmartScreen will warn — unsigned, "Run anyway" is expected.
9. Launch from the **Start menu**, never from a terminal (a terminal leaks the
   shell's env — the zero-config resolution is the thing under test).

**Pass:** window opens on Home.

## Phase 2 — Zero-config first run (~10 min)

8. Backend resolves to **native** with no env vars anywhere.
9. `%USERPROFILE%\.dml\launcher.json` gets written.
10. Close → hides to tray; second launch from Start menu → focuses the
    existing window (single-instance).

## Phase 3 — First-run backend setup (~5 min)

11. The setup screen names what is **first** missing, not a generic error.
12. The bundled payload (dml script, Lua bridges, six installers) is found —
    "payload missing" here is a packaging bug the dev box structurally cannot
    show (the files are simply present in the repo).

## Phase 4 — Native install of WoW WotLK Playerbots (30–90 min)

13. Library → **exactly three titles** (Vanilla, TBC, WotLK Playerbots — no
    MapleStory/RuneScape/MU). Vanilla/TBC visible but not installable on
    native (CMaNGOS, v0.2) with a stated reason, not a failure.
14. Install WotLK Playerbots. Progress bar moves during clone/build/up and
    **never goes backwards**; the "ready" stage shows elapsed time, not a
    percentage (deliberate).
15. **Kill the launcher mid-build once, deliberately.** Reopen, resume — it
    continues from the last completed stage, it does not restart.
16. Stack comes up; Home reports the server online.

On failure: capture the terminal NDJSON, `%USERPROFILE%\.dml\logs\`, and
`docker compose ps` before touching anything.

## Phase 5 — SOAP self-setup (~10 min)

17. The launcher creates the `dmlsoap` GM3 account by itself and raises a
    banner naming it (a fresh AzerothCore has NO accounts).
18. Home's health panel reveals the password on demand.
19. Restart the app: if setup had failed, the manual fallback card must still
    render (a reloaded UI must not be told only "already concluded").

## Phase 6 — Feature sweep (~60 min, each pass/fail on its own)

20. **Home** start/stop/restart; "Players online" reads 0 with bots running
    (not 1000 — that was a real incident).
21. **Dashboard, Item DB, Characters (+paperdoll), Teleport, GM Tools**
    (revive/heal/gold online, level offline, summon → NPC despawns on a
    timer, deliberately), **My Party** (enable → restart once → add class →
    bot joins), **Settings/Modules** (edit a module conf; `.env` and
    `docker-compose.override.yml` are read-only in the raw editor),
    **Backups** (create → list → restore; restore takes a safety dump first).
22. After the backup create: open the archive (`7z`/`zcat`) and confirm it
    contains **all four** schemas (`acore_characters`, `acore_playerbots`,
    `acore_auth`, and with include-world also `acore_world`).

## Phase 7 — What this branch actually changed (~30 min, the centerpiece)

### 7a. `DML_BACKEND=auto` (the fixed trap)

23. Quit the launcher. Set a User env var `DML_BACKEND=auto`. Relaunch.
    **Pass:** the app runs on the **native** backend (before the fix: it
    silently drove a nonexistent WSL distro), AND Settings' backend dropdown
    is **editable** — not "locked by an environment variable". Remove the var
    after.

### 7b. Schema names are read, not assumed

The stock install's compose env and conf agree on `acore_*`, so resolution is
invisible until you make the sources disagree. That's the test.

24. Stop the server. Edit the title's `docker-compose.yml`: change the last
    field of `AC_WORLD_DATABASE_INFO` from `acore_world` to
    `acore_world_nope`. Restart the launcher (not just the server).
25. **Pass: the failure is CONSISTENT.** Item DB, Dashboard, Bot Browser,
    Characters/paperdoll ALL fail — because they all read the same resolved
    name. Before Task 6, this exact edit produced the incoherent state: Item
    DB failed while Dashboard/Bots/paperdoll kept "working" against the
    hardcoded old name — silently reading a database the server wasn't using.
26. **Backup create on this state must REFUSE or carry the new name** — never
    quietly dump the old `acore_world`. (The recorded worst class: a backup
    that reports success and contains nothing.)
27. Revert the edit. Everything works again.

### 7c. The refusal, and the page that used to lie about it

28. Break `AC_WORLD_DATABASE_INFO` differently: truncate it to four fields
    (`ac-database;3306;root;password` — no dbname). Restart the launcher.
    **Pass:** DB pages refuse with the **schema-names story** ("could not read
    the schema names…"), NOT "Is ac-database running?" — and specifically the
    **Statistics page** says the same (its error mapper used to collapse this
    into the engine-down message; fixed this branch).
29. A value whose dbname carries hostile characters
    (`…;root;pw;acore_world; DROP TABLE x` style) must also REFUSE — the
    identifier gate. No page may pass it into a query.
30. Revert. Green again.

### 7d. Games-dir resolution (Task 12)

31. Standalone check, PowerShell. **NB (finding F4): `dml-wow.exe` is NOT in
    `bundle.resources`, so there is no bundled binary to run** — this step as
    originally written cannot execute. Run it against the bundled bash CLI
    instead: `& "C:\Program Files\Git\bin\bash.exe" "<install dir>\cli\dml" wow
    server-detail --json` from an arbitrary directory with NO `DML_GAMES_DIR`
    set. **Pass:** a clean refusal whose hint names `DML_GAMES_DIR` — never a
    half-answer resolved against the current directory.

## Phase 8 — Lifecycle edges (~15 min)

32. Stop the server mid-boot: no hang, clear outcome.
33. A world-log snapshot lands in `%USERPROFILE%\.dml\logs\` on stop (the
    container's own log dies with `compose down`).
34. Quit the app with the server running: the exit prompt appears with the
    window on-screen and focused.

---

## Findings from the 2026-08-09 run (recorded, not yet fixed)

- **F1** — a PC with **neither** Docker **nor** WSL shows the WSL first-run
  screen ("set up WSL2… Install-DML.ps1"), not the Docker card. Correct per
  `backend::detect` (never pick native without Docker → fall back to the Wsl
  default → WSL advice), and wrong for a native-only v0.1: the `docker=no,
  wsl=no` row should send the user to Docker Desktop. Not hit by the sanctioned
  Phase 0 order, which installs Docker first.
- **F2 (real, narrower than first written)** — Git for Windows is a RUNTIME
  dependency of native mode: the `dml` brain is a bash script hosted by Git Bash
  (`runner.rs::native`), and `install_native` shells `git` for its two clones.
  With no Git, `find_bash()` falls through to a bare `bash`, which on a
  WSL-enabled box is `C:\Windows\System32\bash.exe` — the shim — whose default
  distro is Docker Desktop's own `docker-desktop`, an image with no `/bin/bash`.
  Result: `execvpe(/bin/bash) failed` on the Library page, an error naming WSL
  while the app is correctly on the native backend.

  **`Install-DML-Native.ps1` installs Git (and yq, and `launcher.json`), so the
  consumer path is covered** — the first write-up of this finding did not know
  that and blamed the product for a plan error. What remains genuinely broken:
  the NSIS installer is a published release asset a stranger can download
  DIRECTLY, next to the script, and then nothing installs Git, nothing installs
  yq, and the launcher neither detects nor explains the gap. **The launcher must
  answer for its own prerequisites regardless of how it was installed** — a
  `no_git` arm in `dml_core::setup`'s native chain plus a `first-run.ts` card
  mirroring `no_docker`. Release notes should also point at the script.
- **F5 (needs one confirmation)** — the script's DEFAULT path rests on "Docker
  Desktop enables WSL during its own setup" (`Install-DML-Native.ps1:636-640`),
  and that did not hold on this VM: Docker Desktop was installed interactively
  and still reported virtualization unsupported until `wsl --install` was run by
  hand. **Caveat, stated because it changes the verdict:** nested virt was being
  fixed over the same period, so Docker may have run its setup while the
  platform was genuinely absent and then failed to recover. Re-check from the
  `bare` snapshot with nested virt already on. If it reproduces, the default
  path needs the same `wsl --install --no-distribution` the `-InstallDocker`
  path already does.
- **F4** — Phase 7d referenced a bundled `dml-wow.exe` that the installer does
  not carry. Step rewritten; decide separately whether the binary SHOULD ship.

## Expected still-broken (do NOT file as new)

- **The module subsystem on a renamed-schema server** (module install/repair/
  tuning-tail/fixit, Wrath Unbound): recorded exception — the modules' own SQL
  payloads hardcode standard names internally, so DML's tooling keeps them
  too. On a renamed server these fail; that is the documented ruling, not a
  regression.
- **Anything on the WSL backend**: v0.2 scope.

## What to record

Per failure: phase number, the exact error **code**, the terminal NDJSON, and
whether it reproduces from the `clean` snapshot. A failure that does NOT
reproduce from `clean` is a state bug — more interesting, not less.
