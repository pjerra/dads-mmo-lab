# 8.5d — Browse Bots, WoW Tortoise — **PLAN, NOT EVIDENCE**

> **NOTHING IN THIS FOLDER HAS BEEN RUN.** There is no transcript, there are no
> screenshots, and no clause of this box has been proved. Every number below was
> read off the install's **files on disk with the server DOWN** on 2026-09-08,
> or off the fork's own source; each one is labelled with how it was got.
> The gate exists so that it can be run the moment the Tortoise stack is up,
> without a design pass. When it is run, this file gets rewritten as a record —
> until then, an incomplete artefact that reads as fact is exactly the failure
> mode this project has already been bitten by
> (`incomplete-artifact-reads-as-fact`).

**The box.** `pyplan/checklist.md:2499` — *"Browse Bots, WoW Tortoise — as 8.5b
against tw_logon/tw_char. Definition of done: as 8.5b. Gate: m910q; evidence
`pyplan/gates/8.5d-tortoise-m910q-<date>/`. Visible effect: a listed online bot
is found by the in-game who-list."*

**The four inherited clauses (8.5b's, which are 8.5a's minus the type split):**

1. the total equals the same clause run by hand;
2. an unreadable marker refuses to answer rather than reporting zero;
3. a readable marker matching nothing while characters exist WARNS rather than
   reporting zero;
4. a listed online bot is found by the in-game who-list.

This is the **fourth** tree and the **third** with only the account-prefix
signal, so like TBC and Vanilla it must not draw a split by marker type and must
not name a playerbots registry table.

**Why it was written blind.** The Tortoise stack was down when this was written
and another lane held the one-server-at-a-time slot on m910q (the Vanilla stack
was up for two other boxes). Everything readable without a server was read;
everything that needs `mysqld` is a stage below rather than a claim here.

---

## Verification with the server down — every catalog claim, and how

The catalog's `wow-tortoise` observability block is a set of claims **about this
fork**, and this fork is not CMaNGOS. Each was checked against the real install
at `/home/pk/tortoise-server` and against the fork's own source. **All eight
hold. No code change is needed.**

| claim | verdict | what said so |
|---|---|---|
| `bots.prefix_conf_file = "etc/aiplayerbot.conf"` | CONFIRMED | the file exists on the install, and it is the path the server itself reads: `docker-compose.yml` binds `./etc → /opt/tortoise/etc`, the compiled-in `SYSCONFDIR` the module falls back to (`PlayerbotAIConfig.h:16`) |
| `bots.prefix_conf_key = "AiPlayerbot.RandomBotAccountPrefix"` | CONFIRMED | the fork's own key (`mod-playerbots/src/playerbot/PlayerbotAIConfig.cpp:545`), set **once**, at column 0, in `etc/aiplayerbot.conf:63` — `= RNDBOT`. One value, so `resolve_marker` returns `Marker('RNDBOT', 'conf')` and not the multiple-settings refusal |
| `bots.account_prefix = "rndbot"` (the fallback) | CONFIRMED | this fork's **own** compiled default: `GetStringDefault("AiPlayerbot.RandomBotAccountPrefix", "rndbot")`. Not a value inherited from a sibling. The live value is the uppercase `RNDBOT` from the conf; case is not a defect because `bot_clause` upper-cases both sides |
| `bots.registry = null` | CONFIRMED | the schemas on disk are `mysql, performance_schema, sys, tw_char, tw_logon, tw_logs, tw_world`. No `playerbots` schema, no registry table in the fork's SQL. `has_registry()` is False, `_registry_clause` is `1 = 0`, and `bot_clause` keeps its single arm |
| `characters.table = "characters"` in `tw_char` | CONFIRMED | `sql/create_databases.sql:963` inside `CREATE DATABASE tw_char` (`:23`); the table's files are in the volume. **ENGINE=MyISAM on this fork** (`.MYD`/`.MYI`, not `.ibd`) |
| `characters.account = "account"` | CONFIRMED | `account int(10) unsigned NOT NULL DEFAULT 0` — the second column of the shipped DDL, beside `guid`, `name varchar(12)` and `level` |
| `characters.online = "online"` | CONFIRMED | `online tinyint(3) unsigned NOT NULL DEFAULT 0` |
| the bot clause reaches `tw_logon.account (id, username)` | CONFIRMED | `entry.schema_map()` = `{auth: tw_logon, characters: tw_char, world: tw_world}` — no `playerbots` key; `tw_logon.account` has `id` and `username`, the two names `dbreads.bot_clause` hard-codes. Constructed verbatim: `account IN (SELECT id FROM tw_logon.account WHERE UPPER(username) LIKE 'RNDBOT%')`, and `_registry_clause` = `1 = 0` |

**The Bots tab is already drawn on this tree** — no new code:
`controller_view.py:992` is `bots=_BotBrowser(entry, server_dir, sql)` inside
`_for_tortoise` (which begins at `:911`), the fourth of four identical call
sites (WotLK `:612`, TBC `:775`, Vanilla `:888`). The tab is built by the shared
`_build_bots_tab` (`:2899`), which returns early only when `services.bots is
None`, and the split is suppressed for this tree at `:2999` via
`botlist.has_registry(self.entry)`.

**Rows that exist on disk, read read-only with the containers gone.** `docker ps
-a` lists no `tortoise-*` container at all — the stack was taken down with its
containers removed — but the data survives in the named volume
`yulon-wow-tortoise-58c6fd1c_db-data`. `tw_logon/account.ibd` holds
`RNDBOT0 … RNDBOT97` (~100, matching `AiPlayerbot.RandomBotAccountCount = 100`)
alongside the human accounts from 8.3d. `tw_char/characters.MYI`'s header reads
**901 records, 39 deleted** (the offset was validated against an empty table and
two small ones on the same install). **What could not be read: which account
each of those 901 rows belongs to** — that is a per-row integer inside MyISAM
record blocks. So whether any of the 901 is a *human* character is unknown until
`mysqld` is up, and it is the question clause (4) turns on.

---

## The stages, what each proves, and the ground each starts from

Run from `~/gate85d/pylauncher` — **its own copy of the tree, taken at the
moment of the run.** m910q is shared; 8.5c found `~/gate84c/pylauncher` a commit
behind and a gate cannot report that a difference was harmless without first
knowing there was one. Record the md5 of `botlist.py`, `dbreads.py` and
`controller_view.py` in the transcript.

| stage | clause it proves | the ground it starts from — which its own action does not produce |
|---|---|---|
| `total` | **(1)** | the live install and its live marker; three readings (the app, the app's own clause by hand, a statement typed with nothing taken from the module under test), each with the clock beside it because the pool moves at runtime |
| `refuse` | **(2)** | the live install answering a non-zero total *at the same moment*, through the same seam — the refusal must come from the substituted conf, not from a sick server or a wrong password. Both refusals on the path are exercised and labelled: the resolver's (`dbreads.py:174-181`) and `botlist.page`'s own (`botlist.py:147-153`) |
| `warn` | **(3)** | a marker written into a conf copy and **read back** through `resolve_marker`, the live marker answering non-zero at the same moment, and a hand count of the substituted marker |
| `filterzero` | — (guards (3)) | the **live** marker, answering a full count: that is what makes the point that the filter, not the marker, found nothing. Without it, clause (3)'s sentence has two causes and the screenshot cannot say which one it caught |
| `nosplit` | the inherited 8.5b reading | the schema list asked of the running server, plus the one catalog reading both the SQL and the view take. Regression, not new work |
| `page` | usability of the list | page one's cursor, chosen by the server; the filter string comes from page **two**'s rows |
| `races` | the faction map | `data/dbc/ChrRaces.dbc`, written by the extractor at install time |
| `wholaw` | the who-list rules | `etc/mangosd.conf` as the server read it, and the fork's own handler |
| `realmrow` | the client's route in | `tw_logon.realmlist`, read **before** the client is started |
| `who` | **(4)**, the choosing half | the app's own Bots page and the characters table; the subject is picked by the asker's own race, so same team by construction |

### The refusal stage uses the recipe that works, and this is why

`fake_conf` copies the install **directory** and builds the target path from the
catalog:

```python
target = root / entry.observability.bots.prefix_conf_file
target.parent.mkdir(parents=True)
```

which is `gate81c.py:93-107`'s shape. A lane earlier tonight copied `etc/` to a
temp directory instead and put the conf where `dbreads.resolve_marker` does not
look: the resolver opens `<server_dir>/<prefix_conf_file>`, a file that is not
there is **not a refusal** (`dbreads.py:144-158` returns the catalog default on
`FileNotFoundError`), and the stage would have passed while proving the
opposite.

**On this tree that mistake is invisible rather than loud.** The catalog default
is `rndbot`, `bot_clause` upper-cases both sides, and this install's accounts
really are `RNDBOT0…` — so a mislocated conf resolves to a **working** marker
and the blank-marker stage answers with a full, plausible bot count while
photographing itself as proof of a refusal. `fake_conf` therefore asserts the
copy is where the resolver will look, and `warn` asserts the prefix it reads
back is the substituted one and not the default.

---

## The client half — clause (4) — and the blocker it has to be planned around

### The blocker, stated plainly

On the Tortoise build **currently on m910q**, changing an account's password
**locks that account out**: for an account that has logged in before, the server
answers *"The password was changed"* and stores a hash with an **empty account
name** in it (8.3d's controlled experiment: four accounts, the same change
twice, only two logged in between the rounds — those two, and only those two,
came back `EMPTY-NAME`). It is a fork bug, we reported it, and it is fixed
upstream in **`3a8472e6`** — but that fix is **not in the binary on this box**,
and picking it up needs **the owner's rebuild**.

So **8.3a's password button cannot be used to get the client in.** The route has
to be an account whose password is already known and has never been changed
since it last worked.

### The route that exists, from 8.3d's own records

| account | password | state | use |
|---|---|---|---|
| **`GATE83D`** | `n3w-p@ss34` | hash correct; rank **4** (GM); **already driven to the realm list with this exact client and server on 2026-09-07** (`7-fresh-account-in.png`, realmd's own session-key write) | **first choice** |
| `CTRLONE`, `CTRLTWO` | `r0und-two22` | hash correct, `last_login` still `0000-00-00` | spares |
| `SHAPROBE` | `l0ck-p@ss77` | hash correct, never logged in, rank 3 | spare |
| `GATE83E`, `TREATONE`, `TREATTWO` | — | **BRICKED** — empty-name hash, can never log in | do not use |
| `TORTGATE` | unknown | password not recorded anywhere in `pyplan/gates/` | unknown |

**The rule that follows: log in with the password these accounts already have,
and never press the password button on any of them.** One change on `GATE83D`
would brick it now (it has logged in); `CTRLONE`/`CTRLTWO`/`SHAPROBE` would
survive exactly one more change and then be one login away from the same fate.

A clean alternative that also sidesteps the bug: **`account create` through the
console** (8.2e's channel — this core has neither SOAP nor a telnet listener, so
the worldserver console *is* the channel) with a fresh name and a known
password. Creation is not the buggy path, and the app reads the row back after
the fact, so the hash can be proved before the client is started.

`GATE83D`'s rank 4 is a bonus rather than a problem: `MiscHandler.cpp:253-255`
only stamps `m_lastWhoRequest` for a `SEC_PLAYER` asker, so a GM asker bypasses
the 30-second `/who` cooldown. It does not weaken the clause — the filters that
matter to the *subject* (level range, `IsInWorld`, name match) apply either way,
and this install's faction filter is off regardless.

### The part that is genuinely unproven, and it is the risk

**None of those accounts is known to have a character**, and clause (4) needs one
standing in the world. 8.3d only ever reached the realm list. So:

1. `gate85d.py who` is the reading that settles it — it asks for every character
   whose account is not `RNDBOT%`. If it comes back empty, that is the finding,
   not a failure to look.
2. If no character exists, one must be **created through the 1.18.1 creation
   screen**, which no run has driven. `client-who-turtle.ps1 -CreateCharacter`
   exists for that, with the coordinates as **parameters** and a photograph
   after every single action, precisely because they are guesses.
3. **A bot's account is not a spare seat** — the playerbots module owns those
   sessions. It must not be borrowed.
4. If character creation cannot be driven, **clause (4) has no route on this
   tree tonight**, and that is the finding to report rather than something to
   work around.

### Measured client facts the driver carries

* the game is `WoW.exe`; `TurtleWoW.exe` is the **installer** and
  `turtle-wow.exe` the **launcher** — the driver refuses to start either;
* **TAB does not move** from the account box to the password box, and neither
  does ENTER: realmd logged `WHERE username = 'OLD-P@SS12'` on one attempt, the
  password in the name field. **Both boxes are clicked**, at 8.3d's own
  fractions (`0.518, 0.514` and `0.518, 0.624`), which produced a successful
  login;
* it starts **fullscreen**; `SET gxWindow "1"` is what makes it driveable;
* `SET movie "0"`, because **ESC quits this client** rather than skipping. The
  driver sends ESC nowhere, ever;
* it publishes a real `MainWindowHandle` (8.3d saw two); the class and title
  lookups are kept as fallbacks because a click with no handle lands at 0,0 —
  i.e. types the login at the desktop;
* **every capture logs whether the process is alive**, because a client that has
  died photographs exactly like one that refused.

### Measured server facts that change how the client must be driven

* **a 30-second `/who` cooldown** (`MiscHandler.cpp:253-255`). A second `/who`
  inside it returns *nothing at all* — no "0 players", no line — and that
  photographs exactly like a miss. 8.5b asked two names about ten seconds apart
  on TBC, which has no such rule; here the driver waits **35 s** between names
  and asks about one name at a time.
* `etc/mangosd.conf:919` **`AllowTwoSide.WhoList = 1`** — the faction filter is
  **off** on this install, the *opposite* of the Vanilla one 8.5c gated. So an
  opposite-faction subject is expected to be **found** here; an empty answer for
  it is a finding about the config or the client, not a filter working as
  designed.
* `:1644` `GM.InWhoList.Level = 3` — a subject above it is hidden. The bots are
  not GMs, so this only matters if a GM character is ever the subject.
* `MiscHandler.cpp:140` drops any subject outside the **level range the client
  itself sends**. What a bare `/who Name` sends on 1.18.1 is unmeasured, which
  is why the gate picks a subject near the asker's level first and a far one
  second: a hit on the near one and a miss on the far one is a level filter.
* the race → faction map is **this install's own**: `data/dbc/ChrRaces.dbc` has
  **ten** races, with **Goblin (9) HORDE** and **High Elf (10) ALLIANCE** —
  while the Vanilla install's own DBC put Goblin on the *Alliance* side. Field 8
  is `baseLanguage` and field 17 the enUS name per this fork's
  `DBCStructure.h:147,156`, and `Player::TeamForRace` (`Player.cpp:7943`)
  switches on that same field. Inheriting a sibling's map would be wrong here.
* the realm row must be reachable **from the Hyper-V host**, not just from
  m910q. 8.5c found `192.168.10.134` in Vanilla's row, unreachable from the
  client box, while `100.78.24.50` (Tailscale) is reachable from both. `realmrow`
  reads it before anything is started; if it is wrong the fix is Yu'lon's own
  Networking feature (`networking.plan(entry, "lan", lan_ip="100.78.24.50")`
  then `services.network_apply(plan)`), and the Tailscale address has to be
  *said*, because `network_plan("lan")` detects the LAN one.
* `etc/realmd.conf:88-96` — `WrongPass.MaxCount = 10`, `BanTime = 300`. Ten
  failed logins from the client box costs a five-minute ban; do not loop.

---

## How to run it, when the server is up

The Tortoise stack is **stopped** and another lane holds the slot. Nothing here
starts it — that is the owner's call and the one-server-at-a-time rule is his.
Once it is up:

```
# on m910q
ssh m910q '/home/pk/bin/claude-say "8.5d: running the Bots gate against Tortoise"'
rm -rf ~/gate85d && mkdir -p ~/gate85d && cp -r ~/dads-mmo-lab/pylauncher ~/gate85d/
md5sum ~/gate85d/pylauncher/yulon/{botlist.py,dbreads.py,ui/controller_view.py}
~/gate81b-venv/bin/python gate85d.py total
~/gate81b-venv/bin/python gate85d.py refuse
~/gate81b-venv/bin/python gate85d.py warn
~/gate81b-venv/bin/python gate85d.py filterzero
~/gate81b-venv/bin/python gate85d.py nosplit
~/gate81b-venv/bin/python gate85d.py page
~/gate81b-venv/bin/python gate85d.py races
~/gate81b-venv/bin/python gate85d.py wholaw
~/gate81b-venv/bin/python gate85d.py realmrow
~/gate81b-venv/bin/python gate85d.py who
```

and the independent reading, typed at the shell, with the password in the
environment and never in argv:

```
MYSQL_PWD=$(cat ~/tortoise-server/.db_password) \
  docker exec -e MYSQL_PWD -i tortoise-db mariadb -uroot -N -B -e \
  "SELECT COUNT(*) FROM tw_char.characters WHERE account IN
     (SELECT id FROM tw_logon.account WHERE UPPER(username) LIKE 'RNDBOT%');"
```

On the Hyper-V host (`ssh vmhost`), after copying `client-who-turtle.ps1` and
`drive-who-turtle.ps1` into `C:\Users\PK` — **learn the realm flow first**, it
costs one run and it is the step nobody has seen on this client:

```
powershell -File C:\Users\PK\drive-who-turtle.ps1 -Realmlist 100.78.24.50 `
  -Account GATE83D -Password 'n3w-p@ss34' -Label tw85d-realm -DryRealm
```

then, with the flow known and a character in hand:

```
powershell -File C:\Users\PK\drive-who-turtle.ps1 -Realmlist 100.78.24.50 `
  -Account GATE83D -Password 'n3w-p@ss34' -Label tw85d-who -EnterWorld `
  -WorldSeconds 60 -Who '<the same-race subject>'
```

Re-run `gate85d.py still-online <name>` immediately before the client types, and
ask about **one name per run** unless the asker is a GM.

---

## The `--checks` gate, run once because the checkout is shared

`YULON_TEST_BOX=yulon-fedora bash ~/run-tests-vm.sh --checks` was run twice from
this lane on 2026-09-08 (00:25Z and 00:33Z). **Both runs were RED, and no
failure is this box's.** This box added no package code at all — only this
folder — and every failure names another lane's in-flight, still-untracked
`yulon/purge.py`:

| run | failures | black |
|---|---|---|
| 00:25Z, 3362 passed | `test_spine::…folder_listing…` (`purge.py::folder_bytes`, `purge.py::_clear_read_only`), `test_families_cmangos::…live_volume_refusal…`, `test_write_ledger::…` (`purge.py::_clear_read_only::os.chmod`, `purge.py::remove_tree::shutil.rmtree`) | `tests/test_purge.py` |
| 00:33Z, 3381 passed | the same three plus `test_controller_packages_agree::…` | `test_purge.py`, `controller_view.py`, `test_controller_view.py`, `test_docker.py` |

Nineteen more tests existed in the second run than in the first, eight minutes
later: **the tree moved under the gate**, which is what a shared checkout does
(`shared-checkout-branch-drift`). mypy (three platforms) and ruff were clean in
both. Nothing in either run touches `botlist.py`, `dbreads.py`, the Bots tab or
`pyplan/gates/`. **Re-run `--checks` before this box is ticked**, when the purge
lane has landed.

## What this plan does **not** claim

* No clause is proved. No stage has run. The screenshots the stages name
  (`1-bots.png` … `6-filter-found-nothing.png`) do not exist yet.
* The 901 characters and ~100 `RNDBOT` accounts are **file readings**, not
  query results; `total` is what turns them into an answer, and it asserts the
  bot count is neither zero nor the whole table, because if every character is a
  bot the equality proves nothing.
* Every line number here and in `gate85d.py` is a claim like any other. 8.5c
  re-checked its own citations after its run and found **four of them wrong**,
  all plausible. Re-check these against the tree before this file is quoted.
* The catalog's `PLAY` block for `wow-tortoise` belongs to the 8.4d lane and was
  not read or touched here. This box's business is the **observability** block,
  and the right answer for it turned out to be: verify it and change nothing.
