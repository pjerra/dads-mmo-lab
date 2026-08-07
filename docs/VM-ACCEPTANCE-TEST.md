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
4. Install **Docker Desktop and nothing else**. No Rust, no Node, no Git. Dev
   tools installed = the run is invalidated.
5. Snapshot the VM as `clean`.

`DML_BACKEND` note: leave it unset for the main run. **One deliberate check**
later (23) sets it to `auto` — that used to silently mean WSL and is now fixed;
the VM proves the fix on the hardware class it was written for.

---

## Phase 1 — Install (~10 min)

6. Build on the dev box (`npm run tauri build`), copy the NSIS installer over,
   run it. SmartScreen will warn — unsigned, "Run anyway" is expected.
7. Launch from the **Start menu**, never from a terminal (a terminal leaks the
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

31. Standalone check, PowerShell: run the bundled
    `dml-wow.exe server-detail` (or any DB verb) from an arbitrary directory
    with NO `DML_GAMES_DIR` set. **Pass:** a clean refusal whose hint names
    `DML_GAMES_DIR` — never a half-answer resolved against the current
    directory.

## Phase 8 — Lifecycle edges (~15 min)

32. Stop the server mid-boot: no hang, clear outcome.
33. A world-log snapshot lands in `%USERPROFILE%\.dml\logs\` on stop (the
    container's own log dies with `compose down`).
34. Quit the app with the server running: the exit prompt appears with the
    window on-screen and focused.

---

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
