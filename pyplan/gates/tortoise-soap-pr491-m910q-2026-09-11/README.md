# PR 491 — SOAP behind `ENABLE_SOAP` on the Penqle core, built and pressed on `m910q`, 2026-09-11

The owner's ask (2026-09-11, ~16:30 CEST): "make a PR to penqle repo with the soap commit update", then
"use m910q or vms to verify" and "review the PR with the context from the project". This folder is the
verification and the review; the PR is https://github.com/tortoise-wow/tortoise-wow/pull/491.

Why it matters here: T30 moved the Tortoise entry to `tortoise-wow/tortoise-wow` `bot-helpers`, where SOAP
does not exist, so `operations` fell back to `attach`; the ticket names the flip back (four `operations`
keys, three `SOAP.*` conf keys) for the day SOAP lands upstream — T31. PR 491 is that landing.

## The PR

Target `tortoise-wow/tortoise-wow` `main` (Penqle commits straight to that repo: PR 490's head is
`tortoise-wow:main`, and 17 of the last 30 commits on `main` are Penqle's). Head `pjerra:soap-optional`.

Two commits, cherry-picked with `-x`, authorship kept with Shyalya:

| | fork commit | what |
|---|---|---|
| 1 | `3f9a062` | re-add the SOAP remote-command interface (`ns1__executeCommand`, gsoap 2.8.135 vendored as `dep/lib/gsoap/libgsoap++.a` + header) |
| 2 | `0c1f90a` | make it optional via `ENABLE_SOAP` (default ON on Linux, OFF on Windows) — the commit the owner linked |

Both were needed: upstream `main` has never carried SOAP (`git log -S MaNGOSsoap` on `main` is empty), so
the `ENABLE_SOAP` commit alone had nothing to gate. Conflicts in `Master.cpp`, `mangosd/CMakeLists.txt` and
`mangosd.conf.dist.in` were all fork-only neighbours (ExecutionWatch/StallBreadcrumb, the Windows Boost
lib-dir search, the Leech and module conf blocks); only the SOAP hunks were taken.

The same two commits also cherry-pick **clean, no conflicts** onto `bot-helpers` `9980181` (T30's pin);
that variant is pushed as `pjerra:soap-on-bot-helpers` and is build 3 below.

## Builds — all three on `m910q` (4 cores, 15 GB), Docker 29.7.2, `ubuntu:24.04` builder and runtime

The Dockerfile is Yu'lon's own `wow-tortoise/native/Dockerfile.tmpl` rendered by hand (`CORE_DIR=/opt/tortoise`,
`MAKE_JOBS=4`); the box's own docker, not the laptop's. Logs: `~/pr491/build-on.log`, `~/pr491-bh/build-bh.log`.

| # | tree | cmake | `make` wall | result |
|---|---|---|---|---|
| 1 | `main` `ec746bb` + PR (`e04f495`) | the template's six flags, `-DMODULES=disabled`, `-DENABLE_SOAP=ON` (playerbots flags dropped: `main` has no `modules/mod-playerbots`) | 592 s at `-j4` | image `yulon.local/cmangos-tortoise-server:pr491-soap-on`; `mangosd` 24.7 MB carries `ns1__executeCommand`, `urn:MaNGOS`, `SOAP.Enabled`, the "remote command interface bound" line; `etc/mangosd.conf.dist` has the three `SOAP.*` keys at `:2131-2133` |
| 2 | same, `-DENABLE_SOAP=OFF` | reconfigured inside build 1's builder stage, incremental | only `Master.cpp` recompiled, `mangosd` relinked | no `ns1__executeCommand`, no `soap_serve` in the binary |
| 3 | `bot-helpers` `9980181` + PR (`dc41bdb`) + `Sagiroth/TortoiseBots` `fd7ec9e` at `modules/TortoiseBots` | T30's template as merged on `hand-t30` (`-DMODULES=static -DMODULE_TORTOISEBOTS=static`), `ENABLE_SOAP` at its Linux default (ON) | 1320 s at `-j4` | image `…:pr491-bh-soap-on`; `TortoiseBots: static` at configure; the binary carries both the SOAP symbols and the module |

So the PR compiles and links on `main`, compiles without SOAP, and coexists with the module on the branch
Yu'lon pins. Build 3 is the tree T31 would install.

## The finding that should shape the PR: the vendored archive needs glibc >= 2.38

Measured, not inherited:

```
$ nm dep/lib/gsoap/libgsoap++.a | grep -E ' U (strlcpy|__isoc23_strto)'
  U __isoc23_strtol   U __isoc23_strtoll   U __isoc23_strtoul   U __isoc23_strtoull   U strlcpy
```

`strlcpy` and the `__isoc23_*` symbols are glibc 2.38. Upstream's `README.md:37` recommends **Ubuntu 22.04**,
which is glibc 2.35; the project measured exactly this failure on 2026-09-08 (the Dockerfile's own comment:
hundreds of `undefined reference to 'strlcpy'` after 39 minutes on `ubuntu:22.04`), and building gsoap from
the source beside it fails too, because `dep/src/gsoap/stdsoap2.cpp` is 2.7.15 against a 2.8.135 header.
With `ENABLE_SOAP` defaulting **ON on Linux**, every builder on the recommended platform hits that link error
the day this merges. Yu'lon's image is on `ubuntu:24.04` since the 8th for this exact reason and is not
affected.

Also measured: build 2's SOAP-off binary floors at `GLIBC_2.38` too — on a 24.04 builder the floor comes from
the toolchain, so the archive only *adds* a demand on older builders, which is where it hurts.

What the PR now says (body updated): default `ENABLE_SOAP` to OFF everywhere, or vendor gsoap 2.8.135's
`stdsoap2.cpp` beside the header and build it through the existing `dep/src/gsoap` target instead of shipping
a prebuilt `.a` — that would also make Windows a non-special case.

## Review of the code (against `main` as it is today)

1. **A SOAP request that reaches the queue during shutdown hangs the process.** `ns1__executeCommand`
   blocks in a 50 ms sleep loop until the world thread calls `m_commandFinished`; `World::~World`
   (`World.cpp:230-232`) drains `cliCmdQueue` with `delete command`, never calling it. The SOAP worker then
   never returns, and `~SOAPThread` joins it forever at the end of `Master::Run`. Narrow window, real hang.
   A fix is either a bounded wait with a fault on timeout, or the drain calling `m_commandFinished(arg, false)`.
2. **`soapThread` outlives the databases.** It is a local in `Master::Run` and is destroyed at scope exit,
   *after* `LoginDatabase.StopServer()` (`Master.cpp:326-329`); the accept loop polls `World::IsStopped()`
   every 3 s, so a request in that window runs `sAccountMgr.GetId()` against a stopped database. Resetting
   `soapThread` right after `world_thread.join()` (`Master.cpp:305`) closes it.
3. **Rank is read at startup, not per request** — `AccountMgr::GetSecurity` is a pure map lookup
   (`AccountMgr.cpp:250-256`), no database fallback, so an account inserted by SQL after the world is up
   answers 403 until a restart. Not this PR's code, but it is the behaviour the gate
   `tortoise-soap-yulon-arch-2026-09-09/` measured on the fork, and it is what Yu'lon's `channel_setup`
   repair path has to know on this tree too. The press below creates its accounts before start for that reason.
4. Database access from the SOAP thread is fine on this core: `Database::ThreadStart()` is a no-op and
   queries go through the locked round-robin connection pool (`Database.cpp:255-293`).
5. `urn:MaNGOS`, HTTP 401/403 before the body, fault-before-result, one command per connection: all what
   `yulon/soap.py` already classifies (`unauthorised`, `forbidden`, `refused`, `answered`).
6. Not in the PR and still true on `main`: the fork's account-lockout fix (`3a8472e`) is absent
   (`Username.empty()` never appears in `main`'s `AccountMgr.cpp`), so the 8.3d warning on
   `account set password` stays whichever way SOAP goes.

## The press — one mangosd from build 1 on a throwaway install, `m910q`, 15:50-15:52 UTC

`evidence/press.sh` is the whole run; `evidence/press-trimmed.log` its log (the raw one was 10.9 MB because
`grep -i soap` over the world log also matched one quest-text migration line; the bracketed lines and the
client lines are what is kept). Fresh databases from the image's own `sql/` (the `INSERT IGNORE` rewrite
included), the owner's extracted data at `~/tortoise-server/data` bound **read-only**, conf from the
image's `mangosd.conf.dist` with the catalog's keys, `Console.Enable = 0`, `SOAP.Enabled = 1`,
`SOAP.IP = 0.0.0.0`, port published on `127.0.0.1:17878` only. Nothing of the owner's install written.

| step | measured |
|---|---|
| `create_databases.sql` | 418 tables, ok |
| `sql/base/*.sql` into `tw_world` | 191 files in 33 s |
| accounts | `PR491ADMIN` rank 4 and `PR491LOW` rank 1 inserted **before** start (finding 3 above) |
| world | `World server is up and running! Loading time: 0 minutes 19 seconds` — ready 25 s after `docker run`; `SOAP: remote command interface bound to http://0.0.0.0:7878` at line 27377 of the world log |
| `curl` admin, `server info` | HTTP 200, `<result>Core revision: e04f4959c0f6d47fe4a5 / 2026-09-11 15:04:43 +0200 / Linux_x64 …` (the PR head's sha) |
| `curl` wrong password | HTTP 401 |
| `curl` rank-1 account | HTTP 403 |
| `curl` unknown account | HTTP 401 |
| `yulon/soap.py` (the app's own client, `namespace="urn:MaNGOS"`, `yulon.log` shimmed to `logging`) | `server info` → `answered` 200; `account create PR491VIASOAP …` → `answered`, row `6 PR491VIASOAP 0` read back from `tw_logon.account`; `thiscommanddoesnotexist` → `refused` 500 "There is no such command"; wrong password → `unauthorised` 401; rank 1 → `forbidden` 403 |
| `docker stop -t 120` | 3 s, exit code 0, `Halting process...` — the normal shutdown path does not hang (finding 1 needs a request in flight at the moment of shutdown, which this did not stage) |
| cleanup | `pr491-*` containers 0 left; volume and network removed |

So the five outcomes `soap.py` types are all produced by this build, in the shapes the app already
classifies, and the T31 flip (`channel: "soap"`, port 7878, `urn:MaNGOS`, `gm_level` 4, the three
`SOAP.*` keys) has nothing to change against this tree.

## What is NOT proved here

* **Windows.** `ENABLE_SOAP=OFF` was built on Linux only; the fork's author built Windows on the fork.
* **A request in flight during shutdown** (finding 1). Not staged; the claim rests on reading the code.
* **The app's own engine.** The press was hand-driven from the image; no `state.json`, no compose files,
  no Adopt. T31 is where the engine installs this tree.
* **Upstream's opinion.** The PR is open, unreviewed by Penqle at the time of writing.

## The box, as left (owner's ask, 17:55 CEST: "clean up the m910q to get more space")

Mine, removed: the two `pr491-*` images, both source clones, the throwaway containers/volume/network.
`docker builder prune -af` reclaimed **19.0 GB** (that cache included the owner's earlier image builds; the
next Yu'lon rebuild there is a full 46-minute one rather than cached). `docker image prune` found nothing
dangling. `/`: **13 GB free before, 23 GB free after** (80 % used). Kept: `~/pr491/press/` (evidence, 100 KB)
and the two build logs under `~/pr491/` and `~/pr491-bh/`.

**Not touched, the owner's to decide** (sizes measured): `~/yulon-runs` 12 GB, `~/TurtleWoW` 9.3 GB (the
client — not a candidate), `~/vanilla-75b` 2.8 GB, `~/yulon-run` 1.1 GB, `~/yulon-74c` 672 MB, the three
`~/tortoise-backup-2026-09-0*` folders 220 MB each; images with no container: the four
`dml.local/ac-wotlk-*:native-5272511d` (3.1 GB), `…tortoise-server:rollback-2026-09-08` (965 MB — the
8th's rollback), `dml/tortoise-wow:local` (748 MB), `fw41ssh`/`fw41img` (1.2 GB), `cmangos-vanilla-server:native-0baff6f3`
(385 MB), `fedora:41` (242 MB); `/var/log/journal` 513 MB, `/var/cache/apt` 145 MB. `r6` (the owner's) untouched.
