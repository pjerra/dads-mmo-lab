# VM acceptance test — what a clean machine has to prove

Written 2026-08-06, before merging `feat/core-family` into `rust-main`.

**Why a VM and not this box.** The dev machine hides at least seven things a
user's machine does not have, and two of them have already produced false
green results *this week*:

| Hidden here | A fresh VM has |
|---|---|
| `DML_GAMES_DIR`, `DML_BACKEND`, `DML_SCRIPT`, `DML_YQ_BIN` as **User env vars** | none — and a whole branch passed `cargo test --workspace` here that would have failed CI |
| A working migrated server at `C:\Users\perzi\dml-native`, full of real data | nothing to read |
| Docker Desktop installed, WSL2 configured, engine auto-starting | nothing |
| The `dml-arch` WSL distro | nothing |
| Rust 1.97 + MSVC Build Tools + Node 22 + WebView2 | WebView2 only (ships with Win11) |
| `yq.exe` at a known path | nothing |
| A warm BuildKit cache and pulled images | a 30–90 minute build from zero |

Automated suites cannot see any of this. That is the entire point of the run.

---

## Phase 0 — VM preconditions

1. Windows 11 Pro, fresh install, fully updated.
2. **Nested virtualisation ON in the hypervisor.** Without it WSL2 and Docker
   Desktop will not start at all, and the failure looks like a DML bug.
3. 16 GB RAM minimum, 8 vCPU recommended (the AzerothCore build is the load).
4. **150 GB free disk.** Source tree + build output + ~15 GB client data.
5. Install **Docker Desktop** and nothing else. No Rust, no Node, no Git, no
   WSL distro. If you install dev tools you have rebuilt this box and thrown
   away the run.
6. Take a VM snapshot here and name it `clean`. Several phases below are worth
   re-running from it.

**`DML_BACKEND=auto` is now safe** (fixed in `0c61127`, after this document was
first written — it used to resolve to `Wsl` and drive a `dml-arch` distro that
does not exist on a fresh VM). Leaving it unset is still the realistic user
path and the one to test. **Setting it to `auto` explicitly is worth one
deliberate check**, since that is the obvious thing to type on a new box: the
app must come up on the native backend, and the Settings dropdown must stay
editable rather than reporting itself locked by an environment variable.

---

## Phase 1 — Install the launcher

Build on the dev box (`npm run tauri build`), copy the NSIS installer over.

1. Run the installer. **SmartScreen will warn — it is unsigned.** "More info →
   Run anyway". *Pass:* installs without admin-rights surprises.
2. Launch from the Start menu, not from a terminal. *This matters:* a terminal
   would leak the shell's environment, which is the thing being tested.

**Pass:** the window opens on Home with no env vars set anywhere.

---

## Phase 2 — First run, zero configuration

The single riskiest phase, because everything downstream assumes it worked.

3. *Pass:* Home renders. Backend resolves to **native** on its own.
4. Check `%USERPROFILE%\.dml\launcher.json` gets written.
5. Close the window. *Pass:* it hides to tray, does not exit.
6. Re-open from tray; open a second copy from the Start menu. *Pass:* the
   single-instance guard focuses the existing window instead of opening two.

---

## Phase 3 — First-run backend setup

7. The setup screen should say what is **first** missing, not a generic error.
8. *Pass:* the bundled payload (the `dml` script, the Lua bridges, the six
   installers) is found. A "payload missing" here means `tauri.conf.json`'s
   `bundle.resources` did not ship — a packaging bug the dev box cannot show
   you, because the files are simply present in the repo.

---

## Phase 4 — Native install of WoW WotLK Playerbots

The long one. Budget 30–90 minutes depending on vCPU.

9. Library → WoW WotLK (Playerbots) → Install.
10. Watch the progress bar. *Pass:* it moves during clone, build and up, and
    **never goes backwards**. During "ready" it shows elapsed time, not a
    percentage — that is deliberate.
11. **Kill the launcher mid-build, deliberately, once.** Reopen and resume.
    *Pass:* it picks up from the last completed stage rather than restarting.
    This is the whole reason the install engine is a state machine.
12. *Pass:* stack comes up; Home reports the server online.

If it fails: capture the terminal NDJSON, `%USERPROFILE%\.dml\logs\`, and
`docker compose ps` output before touching anything.

---

## Phase 5 — SOAP self-setup

13. *Pass:* the launcher creates the `dmlsoap` GM3 account by itself and raises
    a banner naming it. A fresh AzerothCore has **no** accounts, so every SOAP
    feature depends on this working unattended.
14. Reveal the password from Home's health panel. *Pass:* it appears on demand.
15. Reload the webview (or restart the app). *Pass:* the manual fallback card
    still renders if setup had failed — a reloaded UI must not be told only
    "already concluded".

---

## Phase 6 — What `feat/core-family` actually changes

16. Open **Library**. *Pass:* exactly three titles — WoW Vanilla, WoW TBC, WoW
    WotLK (Playerbots). **No MapleStory, no RuneScape, no MU Online.** They are
    hidden, not deleted; this is the user-visible deliverable of the branch.
17. *Pass:* Vanilla and TBC appear but are not installable on native (they are
    CMaNGOS, deferred to v0.2) — the page should say so rather than failing.

---

## Phase 7 — The schema-name fix (the branch's real bug fix)

This is the one thing on the branch that no automated test can prove on a real
server, because on a stock install the compose and the conf happen to agree.

18. Stop the server. Edit the title's `docker-compose.yml`: change
    `AC_WORLD_DATABASE_INFO`'s last field from `acore_world` to
    `acore_world_nope`. Save. Restart the launcher (not just the server).
19. Open **Item Database**. *Pass:* it fails, and fails **because of the new
    name** — proving the app now reads the compose environment. Before this
    branch it would have queried its own hardcoded `acore_world` and worked,
    silently talking to a database the server was not using.
20. Revert the edit. *Pass:* Item DB works again.

**Known and expected to still be broken (Task 6, recorded in the plan):**
Dashboard, Bots, the Accounts tab and paperdoll would *not* follow a real
rename, because 29 SQL strings still name their schema inline. Do not file
these as new bugs — but do confirm they behave the same before and after the
edit, which tells us the branch changed nothing it did not intend to.

---

## Phase 8 — Feature sweep (each is pass/fail on its own)

21. **Home** — start, stop, restart. Players online reads 0 with bots running
    (not 1000 — that was a real incident).
22. **Dashboard** — renders real numbers.
23. **Item Database** — search returns items.
24. **Characters** — list, then paperdoll on one character.
25. **Teleport** — list loads.
26. **GM Tools** — revive/heal/gold on an online character, level on an offline
    one. Summon an NPC (it should despawn on a timer, deliberately).
27. **Playerbots / My Party** — enable, restart once, add a class, bot joins.
28. **Settings / Modules** — edit a module conf and save; confirm `.env` and
    `docker-compose.override.yml` are **read-only** in the raw editor.
29. **Backups** — create, list, then restore. Restore is the only sanctioned
    write path into character data; confirm it takes a safety dump first.

---

## Phase 9 — Lifecycle edge cases

30. Stop the server while it is mid-boot. *Pass:* no hang, clear outcome.
31. Confirm a world log snapshot lands in `%USERPROFILE%\.dml\logs\` on stop —
    the container's own log is destroyed by `compose down`.
32. Quit the app with the server running. *Pass:* the exit prompt appears and
    the window is on-screen and focused when it does.

---

## What to record

For every failure: the phase number, the exact error **code** (not just the
message), the terminal NDJSON, and whether it reproduced from the `clean`
snapshot. A failure that does not reproduce from `clean` is a state bug, which
is more interesting than one that does.

## Done before the run

The `DML_BACKEND=auto` bug is **fixed** (`0c61127`) — it was precisely the wrong
thing to discover at hour three of a VM session. Phase 0 now tests it instead of
avoiding it.
