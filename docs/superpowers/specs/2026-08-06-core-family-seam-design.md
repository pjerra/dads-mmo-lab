# The core-family seam — design

**Date:** 2026-08-06 · **Branch:** `feat/core-family` (off `rust-main` at `72f15be`)
**Decided by:** user, this session. Every ruling below is recorded with who made it.

---

## 1. What this is, and what it is not

The launcher fully operates **one** kind of server: an AzerothCore WotLK stack with
mod-playerbots. The user wants the Library to show **only WoW** — Vanilla, TBC, WotLK
— with all three **fully operable**, plus the ability to install **custom** WoW
servers.

Vanilla and TBC in this repo are **not AzerothCore**. They are CMaNGOS:

| Title | Core | Containers |
|---|---|---|
| `wow-server-playerbots` | `mod-playerbots/azerothcore-wotlk` + `mod-playerbots/mod-playerbots` | `ac-worldserver`, `ac-authserver`, `ac-database` |
| `wow-vanilla-server` | `cmangos/mangos-classic` + `cmangos/classic-db` + `cmangos/playerbots` | `vanilla-mangosd`, `vanilla-realmd`, `vanilla-db` |
| `wow-tbc-server` | `cmangos/mangos-tbc` + `cmangos/tbc-db` + `cmangos/playerbots` | `tbc-mangosd`, `tbc-realmd`, `tbc-db` |

(Sources: `guides/wow-*/install-wow-*.sh`, `cli/src/80-titles.sh:57-59`.)

`crates/dml-wow/` hardcodes `acore_*` across **18 of its 45 source files**, and the
config registry is 66 rows of `AC_*` env keys. So "fully operable vanilla" is not a
setting — it is a second emulator family reaching every operating surface.

**This spec covers sub-projects #0 and #1 only.** It is the seam, not the second
family.

---

## 2. The decomposition (user-approved)

| # | Sub-project | State |
|---|---|---|
| **0** | Trim Library to WoW only | **In this spec** — delivered by increment I0 |
| **1** | The core-family seam | **This spec** |
| 2 | CMaNGOS native install engine (vanilla + TBC) | Own spec, later |
| 3 | The CMaNGOS operating surface | Own spec, later. **Owes a live gate** |
| 4 | Custom WoW servers | Own spec, later |

**Ruling (user):** #0 is *hide, not delete*. The three non-WoW titles
(MapleStory, RuneScape, MU Online) stay on disk and stay as test fixtures; only the
Library stops showing them. The reason is coverage: `cli/tests/games-list.bats:15`
does `add_game runescape install`, and those titles are currently the **only
multi-title fixtures in the suite**. Deleting them now removes multi-title coverage
until #2 ships. They get deleted once vanilla exists to replace them as the fixture.

**Ruling (user):** "custom WoW server" means **tier 2** — a custom server within a
family the launcher already ships support for (AzerothCore *or* CMaNGOS), with the
user's choice of repo, branch and module list. It does **not** mean arbitrary
emulators (TrinityCore, VMaNGOS). That keeps the family a compile-time enum instead
of a plugin architecture.

**Ruling (user):** approach **B** — incremental extraction, one concern per change,
each shippable and green. Rejected: (A) one `Flavour` struct threaded through all 18
files at once — no reviewable intermediate state and nothing is mutation-testable
until the end; (C) a parallel `dml-cmangos` crate — directly violates the repo's
most-repeated rule, and the `logsnap` / boot-loop / Tailscale / stack-conflict /
bot-identity incidents were all one-surface fixes.

---

## 3. Research findings that this design rests on

Gathered 2026-08-06. Claims marked VERIFIED were read from upstream source or
confirmed locally; the rest are flagged where they matter.

**The transport generalises, and cheaply.** CMaNGOS ships the same gSOAP
`executeCommand` service AzerothCore does — same method name, same `command` /
`result` elements, same HTTP Basic auth, same 401/403 gates, same "queue at
SEC_CONSOLE" execution, same 7878 default. `src/mangosd/CMakeLists.txt` lists
`MaNGOSsoap.cpp` / `soapC.cpp` / `soapServer.cpp` in `EXECUTABLE_SRCS` with **no
`if()` guard** and `cmake/options.cmake` has no SOAP option, so **it is compiled in
unconditionally** — the images `install-wow-vanilla.sh` already builds contain it
today. It is merely `SOAP.Enabled = 0` in conf and unpublished in compose (that
installer publishes only 3724 and 8085).

**The entire envelope-level difference is the namespace URN**: `urn:MaNGOS` vs
`urn:AC`. Today that is a hardcoded string literal at
`crates/dml-wow/src/soap.rs:129` (VERIFIED locally).

**RA is not the answer.** CMaNGOS also ships Remote Administration (`Ra.Enable`,
port 3443), but it is a stateful, unframed telnet session whose output boundary is
the reappearance of a `mangos>` prompt, and with the shipped `Ra.Restricted = 1` it
runs at the account's own level — so `account create`, `account set gmlevel`,
`account set password` and `server exit` are **blocked over RA** and work over SOAP.
Documented as a fallback; not designed for.

**Four things do not generalise, and each shapes the seam:**

1. **Argument arity differs, silently.** `account set gmlevel <u> <lvl> -1` loses
   AzerothCore's realmid; `server set motd 1 enUS <text>` loses the index and
   locale; `teleport name` becomes `tele name` (no `teleport` alias). A vocabulary
   of string templates is not sufficient — it must carry arity.
2. **Some commands are console-blocked.** `ChatCommand::AllowConsole = false`
   refuses a command to SOAP/RA regardless of security — confirmed `false` on
   `.additem`, `.additemset`, `.tele group`, `.tele add`. Separately,
   `AccountTypes::CONSOLE = 4` is unreachable by any account, so level-4 commands
   (`account create`, `account set gmlevel`, `account onlinelist`) are **SOAP-only**.
3. **Bot identity gets worse, not merely different.** CMaNGOS has **no**
   `playerbots_account_type` registry; `PlayerbotAIConfig.cpp` identifies humans with
   `SELECT username, id FROM account where username not like '<prefix>%%'`. The
   two-signal detector from the 2026-08-01 incident becomes **one-signal**, which
   makes the empty-prefix refusal and the `%` / `_` / `\` / `'` escaping the only
   thing standing between this and that failure recurring. Default prefix is
   `rndbot` — the same as AzerothCore's, since both descend from ike3.
4. **The GM Tools are Eluna, and upstream CMaNGOS has no Eluna option at all.**
   `dml_gm_money` / `dml_gm_health` / `dml_gm_revive` / `dml_summon_npc` are this
   repo's own Lua bridge (`cli/lua/gm/dml_gm.lua:13-15`, VERIFIED locally).
   `cmangos/mangos-classic`'s `cmake/options.cmake` has no `BUILD_ELUNA`. On CMaNGOS
   those four either degrade to core `.modify` / `.revive` / `.npc add` or the title
   builds against a third-party Eluna fork. **That is a #3 decision, not a #1 one.**

**Database shape (VERIFIED).** CMaNGOS is four databases; upstream defaults are
`classicrealmd` / `classicmangos` / `classiccharacters` / `classiclogs`, but *this
repo's installers override all four to unprefixed `realmd` / `mangos` /
`characters` / `logs` (`install-wow-vanilla.sh:1269-1272`,
`install-wow-tbc.sh:1116-1119`). **Vanilla and TBC therefore use identical database
names**, so title isolation is compose-level only. `gmlevel` is a column on
`realmd.account`; there is **no `account_access` table**. `characters.online` is the
same column with the same semantics as AzerothCore's.

**Two open items carried forward, not resolved here:**

- `.revive`'s argument form on CMaNGOS is unconfirmed — the shipped help says
  "Revive the selected player" with no argument while `AllowConsole=true`. Verify
  live in #3; do not assume `.revive <name>` works.
- `install-wow-vanilla.sh:602-605` clones playerbots into `src/modules/Bots` citing
  the playerbots README, while upstream master expects `src/modules/PlayerBots` and
  `FetchContent`s it itself. Possibly dead weight, possibly a conflict. Independent
  of this spec; worth its own look.

---

## 4. What the seam carries

**The spine, in one sentence: the family says which questions to ask; the install
says what the answers are.**

That distinction is what stops this becoming the repo's recorded "TWO RESOLVERS FOR
ONE VALUE" bug. `CoreFamily` never holds a value the server itself already knows.

```rust
enum CoreFamily { AzerothCore }   // CMaNGOS is added by #2 — see §6
```

An enum, not a string, so a third family is a **compile error at every match**
rather than a silent fallthrough. The same reasoning that made
`backend::from_override`'s `_ => Backend::Wsl` catch-all a live bug (found on
`rust-main` this session: `DML_BACKEND=auto` resolved Native and then ran as Wsl).

| Carried | AzerothCore | CMaNGOS (for #2/#3) | Why the family owns it |
|---|---|---|---|
| Conf directory | `env/dist/etc/` | decided in #2 | Container-layout convention, fixed per family |
| Conf file names | `worldserver.conf`, `authserver.conf` | `mangosd.conf`, `realmd.conf` | Fixed by the emulator |
| DB-name **keys** | `LoginDatabaseInfo`, `WorldDatabaseInfo`, `CharacterDatabaseInfo` | same keys, different conf | Which conf to read them from differs |
| SOAP URN | `urn:AC` | `urn:MaNGOS` | Compiled into the binary |
| Command vocabulary | per-intent: string + **arity** + console-safe bit | as above | Differs in arity, not just spelling |
| Bot-identity strategy | registry **or** prefix | prefix **only** | The *number* of available signals differs |
| Config registry | 66 `AC_*` rows | `mangosd.conf` keys | Different key namespace |

**Deliberately NOT carried, each exclusion load-bearing:**

- **Database names.** Values come from the running server's own conf.
  `crates/dml-wow/src/db.rs:61-63` hardcodes `acore_world` / `acore_characters` /
  `acore_auth`, and **nothing in the repo reads `*DatabaseInfo` at all** (VERIFIED).
  That is already a live bug for any AzerothCore user who renamed their databases.
- **Container names and ports.** The compose file is already ground truth and the
  stack-conflict guard already resolves it via
  `com.docker.compose.project.working_dir`. Duplicating it into the family creates
  the second resolver.
- **Repo URLs, branches, module lists.** Per-*install*, not per-family — which is
  exactly what makes tier-2 "custom" expressible without inventing a family.

---

## 5. Where it lives, and how a title declares its family

**Code location.** `crates/dml-wow/src/family.rs`. `CoreFamily` is a WoW concept, so
it belongs in `dml-wow`, not game-agnostic `dml-core` (which owns `Backend`).
Per-family vocabulary and config registries become embedded data files alongside the
three that already exist — `data/vocab-azerothcore.json` etc., `include_str!`-loaded,
matching the `config-registry.json` pattern rather than inventing a new one.

**Resolution — three steps, and the third is a refusal:**

1. **The install record, authoritative.** `.dml-install.json` / `.dml-migrate.json`
   gain a `family` field, written at install time. Ground truth for anything DML
   built.
2. **Compose inference, for servers that predate the record.** Parse
   `container_name:` keys through the existing `install_native::parse_stack_owners`
   helper — never a bare grep, because the repo already ate a false refusal from a
   compose that merely *mentioned* an AC image. `ac-worldserver` / `ac-authserver`
   → AzerothCore; `*-mangosd` / `*-realmd` → CMaNGOS.
3. **`TITLE_FAMILY_UNKNOWN` — refuse.** No default, ever.

**Step 2 is the main path today, not a fallback.** The user's live server has **no
install record** — verified, there are no dotfiles in
`C:\Users\perzi\dml-native\wow-server-playerbots` at all — and its compose file
identifies it unambiguously (`ac-worldserver`, `dml.local/ac-wotlk-*`). Inference
must be built first-class and tested against that real file.

**Why refuse rather than default to AzerothCore:** a wrong guess sends `urn:AC` SOAP
at a MaNGOS server and reads `acore_characters` from a database called `characters`.
Both fail in the **silently-wrong** direction — `ok:true` with numbers that are not
the server's — which is the exact class the games-dir incident recorded, and worse
than an error because nothing looks broken.

**Mirror obligation.** The catalog exists on both surfaces —
`cli/src/80-titles.sh:57-62` and `crates/dml-wow/src/destructive.rs:108` — so the
`family` column lands on both, in the same change. That same field is what #0's
Library filter reads.

---

## 6. Increments

Six. Each ships on its own, leaves AzerothCore behaviour **byte-identical**, and is
proven against the live server before the next starts.

| | Increment | Delivers | Parallel |
|---|---|---|---|
| **I0** | `CoreFamily` + `family` catalog column (both surfaces) + resolution chain + Library filter | **Sub-project #0** — the WoW-only Library | — |
| **I1** | `Database::name()` → conf-resolved | A standalone AzerothCore bug fix | after I0 |
| **I2** | SOAP URN → family-owned (one literal, `soap.rs:129`) | No behaviour change | ✓ |
| **I3** | Command vocabulary → family-owned data with arity + console-safe bit | No behaviour change | ✓ |
| **I4** | Bot identity → family-owned strategy (AC keeps both signals) | No behaviour change | ✓ |
| **I5** | Config registry → family-selected | No behaviour change | ✓ |

**`CoreFamily` ships with only the `AzerothCore` variant.** Adding `CMaNGOS` in #2
is then a **compile error at every site that must learn about it** — the enum
earning its keep exactly when the work arrives, instead of six `todo!()` landmines
planted now.

**Parallelism:** I0 and I1 are strictly sequential (everything hangs off the family
existing; I1 is its first real consumer). I2–I5 are mutually independent once I0
lands and go out in parallel git worktrees, one agent each, integrated and verified
centrally. Worktrees are created with `git worktree add` directly — the harness's
isolation flag is recorded as seeding from an unrelated commit (`a624a38`), with
three of four needing manual reset.

---

## 7. Error handling

**One principle: every uncertainty about which family a server is resolves as a
refusal, never as a default.**

| Failure | Answer | Why not the alternative |
|---|---|---|
| Family unresolvable | `TITLE_FAMILY_UNKNOWN` | Defaulting sends `urn:AC` at a MaNGOS server |
| Conf unreadable / `*DatabaseInfo` absent | `DB_NAMES_UNRESOLVED` | Falling back to the old constants restores the bug I1 exists to fix, invisibly |
| Intent has no row for this family | **Build failure** — intent × family coverage test | A runtime gap surfaces as a confusing SOAP fault at click time |
| Command is `AllowConsole = false` | Refuse **before sending**, naming the command | The server's fault text does not say "console-blocked" |
| Bot prefix empty | Refuse (already the rule) | On CMaNGOS the prefix is the *only* signal |

**Capabilities a family lacks are absent or disabled with a stated reason — never
present and broken.** Precedent: `launcher/src/lib/title-install.ts` gives one hint
per reason plus a page notice. A GM Tools button that throws when clicked on a
CMaNGOS server teaches the user the launcher is unreliable; a disabled button naming
the reason teaches them something true.

---

## 8. Testing

**The oracle already exists.** The real invariant of I1–I5 is "AzerothCore behaviour
is byte-identical", and the 18 `crates/dml-wow/tests/*_parity.rs` suites are exactly
that — bash as reference, Rust as subject, live. Every increment is checked against
the running stack.

Note this is deliberate here and is the same mechanism `/ship-check` warns about:
with `DML_GAMES_DIR` pointing at a running server the workspace run's parity suites
go live instead of skipping. For this branch that is the point.

**Four layers, each closing a hole the others cannot see:**

1. **Mutation proof per increment.** Make the code wrong in the specific way the
   test exists to catch, watch it go RED, restore with an edit — never
   `git checkout`, which rewrites line endings here (`core.autocrlf=true`, no
   `*.rs`/`*.ts`/`*.svelte` rule).
2. **Source scans for the wiring.** Family resolution is wiring: a pure
   `resolve_family()` with perfect unit tests stays green under a revert that simply
   stops calling it — the failure this branch paid for twice on 2026-08-05. Call
   sites are scan-pinned with `launcher/src/lib/source-scan.ts` and the Rust
   `production_half` / `strip_comments` / `strip_cfg_test` helpers landed in
   `c95e2c7`.
3. **Intent × family coverage**, `vocab_coverage_tests`-shaped: a missing mapping
   fails the build, not the user's click.
4. **A structural guard against a second resolver.** Modelled on
   `startup.rs`'s `games_dir_reader_scan_tests`: a runtime directory walk pinning an
   **exact map** of the places that resolve a title's family. A fixed file list
   cannot see a second resolver arriving in a new file, and that is precisely how
   the games-dir incident happened — the 18 parity suites structurally could not see
   it, because they pass explicit paths to avoid the process env.

**The honest limit:** until #2 builds a CMaNGOS server, every CMaNGOS path is
unproven. Its tests prove *shape* — that data is well-formed and arity matches
research — not that vanilla works. **A live gate is owed at #3** and must not be
skipped because the tests pass.

---

## 9. Out of scope, explicitly

- Building, installing or running any CMaNGOS server (#2).
- Any CMaNGOS operating surface — SOAP enablement, config registry, DB mapping,
  bot identity implementation (#3).
- Custom repo/branch/module selection in the UI (#4).
- Deleting the three non-WoW titles (deferred until vanilla can replace them as the
  multi-title test fixture).
- The Eluna question for CMaNGOS GM Tools — a #3 product decision.
