# Yu'lon reader — Phase 8 scoping

**Tree read:** `C:\Users\perzi\dml-phase8`, branch `yulon-phase8`, HEAD `1a332a8d`.
`git diff --stat 7bc5ebd3 HEAD` = `pyplan/phase8-kickoff.md | 402 +++` (one added file, nothing else).
**Every line number below is therefore a line number at 7bc5ebd3.** Paths are relative to
`C:\Users\perzi\dml-phase8\`; `pylauncher/yulon/` is abbreviated to `yulon/` where a path is long.

Nothing was written inside a worktree. No `docker`, no `pytest`, no server started. Only file reads,
`git`, `grep`, `sed`, and one `python -c` that read `catalog.json` and printed it.

Three verdicts are used throughout: **yes**, **no**, **could-not-ask**. Where a docstring and the
code disagree, both are reported.

---

## 1. The 20 features, checked against the code

The list itself: `pyplan/checklist.md:2457` (The Lab candidates) and `pyplan/checklist.md:2458`
(Hypeer Launcher shipped set). `pyplan/checklist.md:2456` is the ticked identification box.

| # | Feature | Verdict | Mechanism / citation |
|---|---|---|---|
| 1 | My Party (5-man bot group), Browse Bots (~2500) | **does not exist** | No occurrence. `grep -rniI 'my party\|browse bots\|bot.?count' pylauncher/ --include=*.py` → 0 hits in `yulon/`. Nothing in the app enumerates or groups bots. The bot count is install-time data only: `catalog.json:60` `"AC_AI_PLAYERBOT_MAX_RANDOM_BOTS": "500"` in `install.native.azerothcore.world_env`. |
| 2 | Item DB search + in-game item mail | **does not exist** | No occurrence (`item search`, `mail` → 0 hits in `yulon/ui/` and `yulon/`). The only reachable route to an item today is typing `lookup item …` by hand into the Console tab (`ui/controller_view.py:1280-1283`), which returns a text window, not a searchable database. |
| 3 | Teleport | **does not exist** | No occurrence (`teleport` → 0 hits in `yulon/`). |
| 4 | GM tools (revive/heal/level/gold/summon/rename) | **does not exist as tools** | No buttons, no command builders. `ui/controller_view.py:1417-1418` is a `QSpinBox` range 0-3 that sets an *account* GM level at creation time, not a GM command. Anything else is free-text into the Console tab. |
| 5 | Gear-set presets | **does not exist** | No occurrence. |
| 6 | Character sheet (gear, 3D paperdoll, talents, achievements) | **does not exist** | No occurrence (`paperdoll`, `character sheet`, `talent`, `achievement` → 0 hits). |
| 7 | Live dashboard (players/uptime/latency/bots) + streamed logs | **partly exists — logs only** | Streamed logs: **yes** — `ui/controller_view.py:1278-1279` "Follow worldserver log" → `follow_logs()` `:1312-1314` → `services.logs_source`, bound at `:316` to `docker.follow_logs(spec.world, wsl_distro=…)`; the argv is `[*prefix, "logs", "-f", "--tail", str(tail), container]` (`yulon/docker.py:2354`). Status: a 5 s `QTimer` (`ui/controller_view.py:729-732`, `status_poll_ms` default `:687`) calling `Controller.status` (`yulon/controller.py:127`) and rendering three up/down booleans (`ui/controller_view.py:893-898`). **No players, no uptime, no latency, no bot count anywhere** — the only "players"/"uptime" strings in `controller_view.py` are prose (`:1127` "player data", `:1909` "Players set realmlist to:"). `LogPanel`'s 1 s ticker (`ui/widgets/log_panel.py:161-163`, `_show_elapsed` `:393-398`) is job elapsed time, not server uptime. |
| 8 | GM console with history + autocomplete | **partly exists — console yes, history/autocomplete no** | Console: `ui/controller_view.py:1274-1310`; Send → `send_console_command()` `:1316` → `_send()` `:1323` → `services.send_console` `:1348`. `command_edit` is a bare `QLineEdit` (`:1280`) with a placeholder only (`:1281`). **No `QCompleter` is constructed anywhere in `yulon/ui/`; no history list, no up-arrow recall.** Transport detail in §2. |
| 9 | Module mgmt: install/remove, per-module update checks, tuning knobs, config editor | **partly exists — install/remove for ONE game** | Modules tab `ui/controller_view.py:1726-1747`; two buttons, `_module_action()` `:1777-1785` → `Applier.install` / `Applier.remove` (`yulon/apply.py:858`, `:942`). Enabled only when `services.store` and `services.applier` are both non-None (`:1745-1747`). `applier` is non-None only for `wow-wotlk` (`ui/controller_view.py:392-396`, `entry.has_manifests`; `catalog.json` sets `has_manifests: true` **only** on `wow-wotlk`). `yulon/controller_wow_wotlk/modules.py` is the only `modules.py` in any controller package. Per-module update check: `modules.refresh()` (`modules.py:65`) + `manifest_store.py` ETag machinery (`manifest_store.py:35`) exist, but **nothing in `yulon/ui/` calls `refresh`** — `grep -rn 'refresh(' yulon/ui/` finds only `refresh_status`/`refresh_backups`. **Tuning knobs: no UI.** The *manifest* can declare `conf` keys with defaults (`yulon/manifest.py:133-154`) and `prompts` (`:211-226`), and `Applier._conf()` (`yulon/apply.py:1388-1410`) writes them at install time — but there is no editor and `Applier.configure()` (`yulon/apply.py:915`) has no caller in `yulon/ui/`. **Config editor: does not exist.** |
| 10 | Settings with guardrails; account-wide sharing | **does not exist** | No settings surface. `catalog/installers/wow-wotlk/native/override.yml.tmpl:3-8` says the override file "BELONGS TO THE SETTINGS SURFACE" and that "whatever writes settings next will read and rewrite" it — that writer does not exist yet (nothing in `yulon/` writes `docker-compose.override.yml` after generation). No `QSettings`, no `restoreGeometry` (see bug box at `pyplan/bug-checklist.md:289`). "Account-wide sharing": no occurrence. |
| 11 | Accounts (create, list, set password, GM level) | **partly exists — create only** | Create: `ui/controller_view.py:1401-1445`, `create_account()` `:1447` → `services.create_account` `:1469-1473` → per-game `accounts.create_account`. GM level is only settable **at creation**, and only upward (`accounts.create_account` docstring `yulon/controller_wow_wotlk/accounts.py:339-343`: "its GM level is only ever raised"). **No list, no set-password, no standalone GM change** — matches the open bug box `pyplan/bug-checklist.md:239` ("The Accounts tab is create-only… A feature gap; no documented decision says it should be that way"). The tab is fully disabled where `entry.accounts.scheme is None` (`ui/controller_view.py:1431-1434`) — no entry currently sets it to None. |
| 12 | Backups with validate/restore | **exists** | Maintenance tab `ui/controller_view.py:1493-1547`. Back up now `:1514` → `:1609-1613` → `maintenance.backup` (`yulon/controller_wow_wotlk/maintenance.py:460`). Validate: `verify_dump()` (`maintenance.py:578`) is run on every dump written (`_dump_one` `:551`, "`.partial` that is only renamed if it verifies") and on any file chosen for restore (`plan_restore` `:906`). Restore is two-step: "Show restore plan" `:1526` → `plan_restore` (`maintenance.py:860`), then "Restore" `:1527`, disabled at build (`:1532`) and enabled only when `result.allowed` (`:1688`). Interrupted-restore marker machinery `maintenance.py:704-1298`; "Forget that record" `:1508`. |
| 13 | Self-update of core + playerbots | **partly exists — app only, not the server** | The *app's own* update check: `yulon/update.py:99 check_for_update()`, hitting `RELEASES_API` (`update.py:26`, the DadsMmoLab repo releases). Reached: `main.py:113`, `:343` on a `QThread`, banner at `main.py:356-357`. **There is no self-update of the AzerothCore core or of mod-playerbots** — nothing rebuilds or re-pulls the server. `git.py` exists but is only used by the module applier (`apply.py`'s `self.git`). |
| 14 | Auto-stop on client exit; keep-awake; single-instance guard; autostart | **mixed** | **Auto-stop: partly exists, Steam-Deck bash script only, not the app.** `pylauncher/catalog/installers/steam-deck/setup-gaming-mode.sh:185` `pgrep -fi -- "$CLIENT_PATTERN"` to detect the client and `:202` to wait for it to close, `CLIENT_PATTERN='Wow[.]exe\|wine.*[Ww]o[Ww]'` (`:86`). Its own header, `:65`, says "COPY THIS FILE OUT OF THE BUNDLE FIRST… Nothing copies this file out yet - see the bug checklist entry for it." **Nothing in the Python app knows about a client process** (see §7). **Keep-awake: exists but only during an install** — `platform.keep_awake()` (`yulon/platform.py:3760`), single caller `yulon/catalog/native.py:3097` `stack.enter_context(self._seams.keep_awake())`, bound at `native.py:1296`. Windows path refuses on the GUI thread (`platform.py:3830`) and the docstring at `:3771-3779` explicitly flags that the roadmap's lid-close wording overpromises. **Single-instance: partly exists, and is a port scan, not an instance guard** — `docker.port_conflicts()` (`yulon/docker.py:2233`), whose own docstring `:2236-2244` says it is "the single-instance guard from README §12" and then that it is a **global** port scan with "no concept of which install a container belongs to". There is no guard against two copies of `yulon.exe`. **Autostart: does not exist** (no occurrence of `autostart`, no registry Run key, no systemd unit, no launch agent). |
| 15 | LAN / internet play | **exists** | Networking tab `ui/controller_view.py:1801-1832`: three radios `:1804-1806` (LAN / Internet / `LOOPBACK_CHOICE` at `:607`), "Show plan" `:1816` → `networking.plan` (`yulon/networking.py:3134`), "Apply" `:1817` (disabled until `result.ready`, `:1861`) → `networking.apply` (`yulon/networking.py:3516`), bound at `ui/controller_view.py:323`. `apply()` runs firewall commands, then the realmlist UPDATE (`networking.py:3617-3632`), then records the intent file (`:3636`, `INTENT_FILE = ".yulon-network.json"` `:220`). |
| 16 | Doctor (diagnostics) and shell | **partly exists — a headless provision probe, no doctor, no shell** | `main.py:405 provision_headless()`, reached by `yulon --provision`; exit codes `main.py:400-402` (`PROVISION_READY=0`, `PROVISION_MANUAL=2`, `PROVISION_REBOOT=3`). `pyplan/checklist.md:3182` calls it "Also a support diagnostic." There is **no** "Doctor" surface in the UI (no occurrence of `doctor`/`diagnos` in `yulon/ui/`) and **no shell** anywhere. |
| 17 | Steam integration (add server + client to Steam library) | **does not exist as integration** | No `shortcuts.vdf` writer, no Steam library code (`grep -rniI 'shortcuts.vdf\|steam' yulon/` → only SteamOS *package-manager* handling: `platform.py:132 is_steamos()`, `:434-439`, `:1592-1593`; and `resources.py:10`). The Steam Deck script exists (`catalog/installers/steam-deck/setup-gaming-mode.sh`) but its own header `:56-58` tells the **user** to add it by hand via "Add a Non-Steam Game → Browse". |
| 18 | Uninstall / purge | **partly exists — containers only, deliberately not data** | Server tab "Remove" (`ui/controller_view.py:757`, two-press arm/confirm `:628-635`, `:1123-1150`) → `Controller.remove` (`yulon/controller.py:294`) → `docker.remove_staged` (`docker.py:814`), and `docker_ctl.py:53-55` records the deliberate rename ("Not `stop`… one keeps the containers and the other deletes them"). **Volumes are kept** (`ui/controller_view.py:1140-1145`). Nothing deletes the server directory, the images, the volumes, or the app's `config_dir()`. |
| 19 | `realmlist.wtf` / `Config.wtf` check-or-write in the user's client folder | **partly exists — the function exists and NOTHING calls it** | `networking.write_client_realmlist()` (`yulon/networking.py:3711-3737`) writes `set realmlist <address>` into `Data/<locale>/realmlist.wtf`, falling back to the repack top level and finally creating `Data/enUS/`. **Callers, tree-wide:** `tests/test_networking.py:350`, `:357`, `:363` and one allowlist entry in `tests/test_spine.py:2352`. **No caller in `yulon/` and none in `main.py`.** The Networking tab only *prints* the address as advice: `ui/controller_view.py:1909` "Players set realmlist to:". **`Config.wtf` is never mentioned anywhere in the tree.** Client-folder *validation* does exist and is reached at install time — `yulon/catalog/families/clientdir.py:67 validate()`, including a repack check keyed on `realmlist.wtf` at the root (`clientdir.py:246-278`). |
| 20 | Worldserver log snapshot before stop/restart | **does not exist** | `Controller.stop()` (`yulon/controller.py:272-292`) and `remove()` (`:294`) read no logs. The only buffered log read in the package is `docker._logs()` (`yulon/docker.py:1913`), whose callers are the *ready-wait* paths: `docker.py:2194`, `:2227` and `yulon/catalog/native.py:1012` (inside `_world_output()` `native.py:957`, used during an install's readiness wait). `docker.follow_logs()` is a live stream that never returns (`docker.py:2326`). Nothing captures a log to a file at any point. |

---

## 2. `controller_wow_wotlk/console.py` — `docker attach`, not SOAP

**It uses `docker attach`. It does not use SOAP, RA, or telnet.** File header `console.py:1-17`.

**The argv, verbatim.** Two shapes.

Local pty path — `console.py:205-225`, and the returned list is `console.py:225`:

```python
prefix = platform.docker_prefix(wsl_distro)
if prefix is None:
    raise ConsoleError(platform.DOCKER_CLI_MISSING_HELP)
return [*prefix, "attach", "--sig-proxy=false", container]
```

`prefix` is `platform.docker_prefix()` (`yulon/platform.py:1051`), i.e. a resolved absolute
`docker` program, not the literal string `docker` (reason at `console.py:208-214`).

Windows-with-WSL path — `console.py:167-202`, returned at `:202`:

```python
prefix = platform.wsl_prefix(wsl_distro)
inner = " ".join(shlex.quote(part) for part in (
    "docker", "attach", "--sig-proxy=false", f"--detach-keys={DETACH_KEYS}", container))
return [*prefix, "script", "-qec", inner, "/dev/null"]
```

`DETACH_KEYS = "ctrl-p,ctrl-q"` (`console.py:127`); `DETACH_SEQUENCE` is the byte pair (`:136`).

**Transport shape: one command in, a fixed reply *window* out.** `send_command()`
(`console.py:228-336`):

* refuses more than one line — `console.py:259-260`, `ValueError`;
* refuses where it cannot send — `:262-263`, `raise ConsoleError(NO_TTY_HELP…)`;
* opens a pty (`:283`), spawns the attach client (`:285-293`), sleeps
  `_ATTACH_SETTLE_SECONDS = 0.6` (`:45`, `:316`), writes the one line (`:318`), sleeps
  `window` (default `_DEFAULT_WINDOW_SECONDS = 3.0`, `:76`) at `:322`, kills the client (`:328`),
  and parses (`:333`).
* **Nothing reads the stream while it arrives; the header says so in terms**
  (`console.py:10-12`: "It does NOT stop early at the prompt — nothing here reads the stream while
  it is arriving, so every command costs the full window"). That header also records that an
  earlier version of it claimed the opposite and was corrected.

**The reply is cut out of the window by the console's own prompt.** `_parse_reply()`
(`console.py:468-…`): everything after the FIRST prompt and before the SECOND, counted from the
app's own echo of the command (`:496-511`). `_PROMPT = "AC>"` (`:57`). A window with no prompt at
all comes back whole with `ConsoleReply.prompted = False` (`:513-520`, field at `:94-107`) — the
same shape for "no such container" and for "the worldserver is still loading maps", which the
transport cannot tell apart (`:98-102`).

**Platforms.** `can_send()` (`console.py:156-164`): `return wsl_distro is not None or
pty_supported()`. `pty_supported` is `runner.pty_supported` (`:114`). So: POSIX yes; Windows only
when the server lives in a named WSL distro that has `script(1)` (missing-`script` message at
`:147-153`).

**What the code says about Windows.** `NO_TTY_HELP` (`console.py:118-124`): "Docker refuses
`docker attach` when its input is not a TTY, and this platform has no pseudo-terminal, so the app
cannot send console commands here yet." The *guard that proves it arrives*, not the docstring:
`ui/controller_view.py:1292` `if not self._console_available():` → `:1299`
`self.send_button.setEnabled(False)` and `:1300` `self.command_edit.setEnabled(False)`, with the
sentence shown at `:1301-1304`. The re-arm after each send is also gated:
`ui/controller_view.py:1368` `self.send_button.setEnabled(self._console_available())`.
`_console_available()` is `ui/controller_view.py:1363` → `wotlk_console.can_send(wsl_distro=…)`.
The checklist agrees: `pyplan/checklist.md:3174-3176` — "**Not** the Console tab's `docker attach`
— `send_command()` refuses on `pty_supported()` first, and 6.5 already scopes the console to
Linux/macOS."

`console.attach()` (`console.py:572-580`) builds the interactive argv "for a terminal" — **it has
no caller in `yulon/` or `main.py`.** The UI shows the command as a formatted string from
`NO_TTY_HELP` (`ui/controller_view.py:1302`) rather than from `attach()`. So: **partly exists —
unreached.**

### What this implies for a feature that needs many small commands and typed replies

Stated as measured facts from this file, not as advice:

* **Cost per command is the whole window.** `console.py:10-12` and `:322`. At the default 3.0 s
  (`:76`) plus 0.6 s settle (`:45`) plus attach/detach, one command is ≥3.6 s. The detach grace is
  a further 5.0 s ceiling (`_DETACH_GRACE_SECONDS`, `:139`).
* **One attach client per command.** `send_command()` spawns and kills a client each call
  (`:285`, `:328`). There is no session, no connection reuse, no pipelining.
* **The delimiter is a single-writer property.** `console.py:65-73`: the twice-per-command prompt
  "is a single-writer property, not a property of the console"; a second attach client or a human
  typing puts foreign prompts and foreign echoes inside the app's window. Two overlapping sends are
  named as a reachable failure at `:507-511`.
* **Async output inside the answer stays inside the answer.** `console.py:522-527`: output landing
  *between* the two prompts is claimed as part of the reply and cannot be separated, "which would
  need the worldserver to mark its own log lines, which it does not". The same file records the
  live measurement: `server info`'s nine lines arrived "under six to forty-four lines of bot
  logins" (`:485-486`).
* **`prompted=False` is ambiguous by construction** (`:98-102`), so a caller cannot reliably tell a
  transport failure from a slow start.
* **Windows without a WSL-resident server cannot send at all** (`:156-164`, `ui/controller_view.py:1292-1300`).

Features 1, 2, 3, 4, 5, 6 and 8-with-typed-replies all require exactly the shape this transport
does not have. That is an observation about the transport, not a recommendation.

---

## 3. `controller_wow_wotlk/accounts.py` — the SRP6 row written directly

**Why direct.** `accounts.py:3-9`: the console "cannot do this everywhere" (no TTY on Windows), and
"SOAP cannot do it either, because SOAP authenticates against an account that must already exist,
so it can never create the first one."

**The seam it writes through.** `SqlSeam` Protocol, `accounts.py:148-158`:

```python
def run_statement(self, db: Db, statement: str) -> None: ...
def query(self, db: Db, statement: str) -> str: ...
```

Concretely `apply.DockerSql` — `accounts.py:627-642 sql_for()` returns
`DockerSql(container, db_root_password, client=docker_ctl.DB_CLIENT)`; `DockerSql` is
`yulon/apply.py:453-…`, and every statement is one `docker exec <db_container> mysql`
(`apply.py:530 _mysql()`, argv at `:595`, env at `:587`). Statements go over **stdin, never `-e`**
(`apply.py:499-501`: "argv is world-readable… and a statement can carry a password").
Reads use `--batch --skip-column-names` (`apply.py:504-528`).

**Databases touched — exactly one: `auth`.** `_ACCOUNTS_DB: Db = "auth"` (`accounts.py:111`).
Statements, all in the auth schema:

| Statement | Line | Table |
|---|---|---|
| `SELECT id FROM account WHERE username = …` | `accounts.py:543` | `account` |
| `INSERT INTO account (username, v, s, joindate)` (mangos_srp6) | `accounts.py:420` | `account` |
| `INSERT INTO account(username,sha_pass_hash,joindate)` (mangos_sha) | `accounts.py:430` | `account` |
| `INSERT INTO account (username, salt, verifier, expansion, reg_mail, email, joindate)` (azerothcore) | `accounts.py:437` | `account` |
| `INSERT INTO realmcharacters (realmid, acctid, numchars) SELECT realmlist.id, account.id, 0 FROM realmlist, account LEFT JOIN realmcharacters ON acctid=account.id WHERE acctid IS NULL` | `accounts.py:391-393` | `realmcharacters`, reading `realmlist` + `account` |
| `UPDATE account SET {gmlevel-column} = … WHERE id = …` (mangos schemes) | `accounts.py:501` | `account` |
| `INSERT INTO account_access (id, gmlevel, RealmID) … ON DUPLICATE KEY UPDATE gmlevel = …` (azerothcore) | `accounts.py:507-509` | `account_access` |
| `SELECT {column} FROM account WHERE id = …` | `accounts.py:555` | `account` |
| `SELECT gmlevel FROM account_access WHERE id = … AND RealmID = -1` | `accounts.py:562` | `account_access` |

**It never touches `characters`, `world`, `playerbots`.** Note the `realmcharacters` INSERT is
**server-wide, not scoped to this account** — the comment at `accounts.py:382-388` says so
explicitly ("the SELECT joins every row of `account` and seeds a counter for any account on the box
that lacks one").

Crypto constants read out of AzerothCore's own sources (`accounts.py:13-35`):
`GENERATOR = 7` (`:85`), `MODULUS` (`:86`), `SALT_LENGTH = 32` (`:88`), `VERIFIER_LENGTH = 32`
(`:89`), `MAX_USERNAME = 17` / `MAX_PASSWORD = 16` (`:93-94`), `NO_GM = 0` / `MAX_GM_LEVEL = 3`
(`:108-109`), `ALL_REALMS = -1` (`:103`).

Idempotence is explicit: every call drives to the same end state (`accounts.py:327-337`), and an
existing account keeps its credentials — salt and verifier are never rewritten (`:339-341`), which
is why **there is no set-password path here at all**.

---

## 4. Base controller and the four controller packages (per-fork, never merged)

### `yulon/controller.py` (base)

Concrete class, **no ABC and no `@abstractmethod`** — "must implement" is a convention, not a type.

| Member | Line |
|---|---|
| `PortConflictError` (+ `owner_summary()` `:61`) | `:35` |
| `InstallStatus` (+ `any_running` `:79`, `all_running` `:84`) | `:71` |
| `Controller.__init__(spec, server_dir, *, wsl_distro, import_probe, reset_unfinished)` | `:99` |
| `status()` | `:127` |
| `port_conflicts()` | `:166` |
| `start()` | `:194` |
| `_owners_of()` | `:215` |
| `stop_conflicting()` | `:225` |
| `stop_conflicting_and_start()` | `:266` |
| `stop()` → `docker.stop_staged` | `:272` |
| `remove()` → `docker.remove_staged` | `:294` |
| `import_state()` | `:307` |
| `repair_import(output=None)` | `:322` |
| `wait_db_healthy()` | `:355` |
| `wait_ready(realm_host, realm_port)` → `docker.azerothcore_ready()` | `:359` |

`wait_ready()` is the one AzerothCore-shaped fact in the base — which is why each CMaNGOS package
overrides it.

### Per-package facts

| | **wotlk (AzerothCore)** | **tbc (CMaNGOS)** | **vanilla (CMaNGOS)** | **tortoise (CMaNGOS fork)** |
|---|---|---|---|---|
| Modules | `__init__, accounts, console, controller, docker_ctl, maintenance, **modules**, repair` | `__init__, accounts, console, controller, docker_ctl, maintenance, repair` | same as tbc | `__init__, accounts, console, controller, docker_ctl, **game**, maintenance, repair` |
| Controller class | `WotlkController`, `controller.py:17-21`; **no `wait_ready` override** | `TbcController` `:31-62`; overrides `wait_ready` `:50-62` and **drops host/port** | `VanillaController` `:31-59`; overrides `wait_ready` `:50-59`, **forwards host, drops port** | `TortoiseController` `:32-64`; overrides `wait_ready` `:38-64`, **uses both**; factory `controller_for()` `:67-84`; ctor signature takes **no** `import_probe`/`reset_unfinished` (`:35-36`) |
| Console transport | the real one (`console.py:205-225`) | re-exports wotlk's `attach_argv`; `send_command()` `:63-87`; `attach()` `:90-92` | delegates to wotlk `transport.send_command()` `:76-106`; `attach()` `:109-115` | imports `attach_argv, send_command` from wotlk (`console.py:47`); public name is **`send()`**, not `send_command()` (`:56-82`); `attach()` `:85-91` |
| Prompt | `AC>`, `prompt_precedes_answer=True` (`console.py:57`; catalog defaults `catalog.py:978`, `:983`) | `mangos>`, `False` (`console.py:56`, `:59` ← `catalog.json` `wow-tbc.console`) | `mangos>`, `False` (`console.py:60`, `:63`) | `mangos>`, `False`, read live per call from `game.entry()` rather than cached |
| Account scheme | `azerothcore` (default, `catalog.py:947`) | `mangos_srp6` (`catalog.json` `wow-tbc.accounts`) | `mangos_srp6` | **`mangos_sha`** — the odd one out |
| Account seam | `DockerSql(SPEC.db=ac-database, …, client=DB_CLIENT)` `accounts.py:627-642` | `DockerSql(SPEC.db=tbc-db, …, schemas=ENTRY.schema_map(), client=DB_CLIENT)` `accounts.py:85-99` | `DockerSql(SPEC.db=vanilla-db, …)` `accounts.py:112-130` | `DockerSql(SPEC.db=tortoise-db, …, schemas=game.schemas(), client=game.db().client)` `accounts.py:97-113` |
| set-password / list-accounts | none / none | none / none | none / none | none / none |
| Maintenance | the shared engine (`maintenance.py`, 1332 lines): `DockerMysql` `:201`, `backup()` `:460`, `verify_dump()` `:578`, `plan_restore()` `:860`, `restore()` `:949` | thin binders `:120`, `:144`, `:166`; `CORE_DATABASES` `:94` | thin binders `:129`, `:160`, `:183`; `CORE_DATABASES` `:102` | thin binders `:101`, `:138`, `:160`; passes `client=game.db().client` |
| Does maintenance stop the server? | **No.** `plan_restore()` **refuses** while `spec.world`/`spec.auth` are up — `maintenance.py:934-946` builds the refusal; `backup()` **requires** `spec.db` up (`:503-507`) and records `server_was_running` (`:544`) without refusing | inherits the same (no stop anywhere in the file) | same | same |
| Repair | `import_state()` `:126` (AzerothCore `updates` bookkeeping), `reset_unfinished()` `:221-288` — real `DROP DATABASE`, refuses on `populated` `:259-262` and `unreadable` `:263-266` | `import_state()` `:149`, `import_gate()` `:113`; **`reset_unfinished()` `:179-210` always raises `NotImplementedError`** | `import_gate()` `:143-187` returns a live `(probe, reset)`; **nothing passes `reset` to the controller** | **no reset function is defined at all**; `import_probe()` `:115-125` exists and `controller_for()` does not pass it |
| `docker_ctl.SPEC` | `db=ac-database, auth=ac-authserver, world=ac-worldserver, ports=(3724, 8085), import_service="ac-db-import"` — hardcoded, `docker_ctl.py:35-44` | from `ENTRY.container_spec()`: `tbc-db / tbc-realmd / tbc-mangosd`, no import service | from `entry().container_spec()`: `vanilla-db / vanilla-realmd / vanilla-mangosd`, no import service | from `game.entry().container_spec()`: `tortoise-db / tortoise-realmd / tortoise-mangosd`, world port **8090**, no import service |
| `DB_CLIENT` | `mysql` (`docker_ctl.py:23-33`) | `mariadb` | `mariadb` | `game.db().client` |
| Re-exports of shared ops | full set + `repair_import` (`docker_ctl.py:48-70`) | full set, no `repair_import` | full set, no `repair_import` | **none** — deliberately, `docker_ctl.py:4-8` |
| SOAP / RA / Console.Enable | none (only prose at `accounts.py:5-6`) | none | none | none |

`controller_wow_tortoise/game.py` is the package's single catalog-facts accessor: `entry()` `:57`,
`db()` `:73`, `cmangos()` `:89`, `sql_plan()` `:112`, `schemas()` `:117`, `plan_schemas()` `:128`,
`core_databases()` `:151`, `ready_markers()` `:161`, `_native()` `:173`, `CatalogFactsError` `:48`.
It is not a gameplay module.

---

## 5. `apply.py`, `manifest.py`, `manifest_store.py`

### `apply.py` — the module applier

* `Applier.install()` `:858`, `configure()` `:915`, `remove()` `:942`. Only `install` and `remove`
  are reached from the UI (`ui/controller_view.py:1782`); **`configure()` has no caller in
  `yulon/ui/` or `main.py`** — partly exists, unreached.
* Order of primitives inside `install()`: `_deploy` → `_patches` → `_sql` → `_conf` → `_client` →
  `_dbc` (`apply.py:907-912`).
* **SQL:** `_sql()` `:1359-1372` and `_run_sql()` `:1373-1387`. Two bodies per step — an inline
  `statement` or a `path`/glob resolved against the clone. `applied_by="db-import"` is **skipped**
  and left to AzerothCore's own one-shot (`:1365-1367`); `applied_by="direct"` runs through
  `self.sql` — a `DockerSql`, i.e. `docker exec <db-container> mysql`.
* **Which database:** whatever `step.db` says. `Db` is the four-way key; `DB_NAMES` maps it to real
  schema names (`apply.py:60`), overridable per game via `DockerSql.schemas` (`apply.py:470`).
  So a manifest can address **auth, characters, world or playerbots**.
* **Does it write while the server runs? Yes — there is no guard.** `grep -n 'running\|is_running\|stopped' yulon/apply.py`
  returns only prose (`:146`, `:695`, `:745`, `:1170`). Nothing in `Applier` calls `docker.status`,
  `Controller.status`, or stops anything. The Modules tab's two buttons are enabled purely on
  `store is not None and applier is not None` (`ui/controller_view.py:1745-1747`) — never on server
  state.
* Ownership guard that *does* exist: `_require_own_clone()` `:965` and the clone claim file
  `CLAIM_FILE = ".yulon-clone.json"` (`:83`), `read_clone_claim()` `:106`, `write_clone_claim()` `:238`.

### `manifest.py` — what each primitive can do

| Primitive | Class:line | Reach |
|---|---|---|
| `sql` | `SqlStep` `:109-130` | A file/glob from the clone **or** an inline templated statement, into `auth/characters/world/playerbots`, `when` = install/remove, `applied_by` = `db-import` or `direct`. |
| `conf` | `ConfFile` `:143-154`, `ConfKey` `:133-140` | Path **relative to the server dir**; can copy a `.conf.dist` template in on activation and then key-write defaults. Applier: `apply.py:1388-1410`; only files ending `.conf` are key-written (`_CONF_KEY_WRITE_SUFFIXES` `apply.py:76`), Lua/DB "conf" is skipped (`apply.py:1390-1391`). |
| `client` | `ClientFile` `:184-193` | Copies from the clone **into the USER'S client folder**. Applier `apply.py:1411-1431`: `dest="addons"` → `<client>/Interface/AddOns/<name or basename>`; `"interface"` → `<client>/Interface`; anything else → `<client>/Data`. Directories go through `shutil.copytree(dirs_exist_ok=True)` (`:1427`). **Opt-in:** with `self.client_dir is None` every step is skipped with `"client {src}: no client dir configured"` (`apply.py:1413-1415`); `client_dir` reaches the applier only via `ui/controller_view.py:392-394` (`wotlk_modules.applier(server_dir, sql=sql, client_dir=client_dir)`), which itself is `None` unless the install recorded one. |
| `server_dbc` | `ServerDbc` `:196-200` | Copies DBC files into the server's `data/dbc/` volume through a `DbcCopier` (`apply.py:447-451`, `_dbc()` `:1432-1441`). Skipped when `self.dbc is None` — and **no call site in `yulon/` passes a `dbc` copier**: `wotlk_modules.applier()` (`modules.py:70-89`) has a `dbc` parameter with no non-test caller supplying it. Partly exists — unreached. |
| `npcs` | `Npc` `:202-208` | **Data only.** `entry`, `name`, `auto_spawned`, `note`. `grep -n 'npcs' yulon/apply.py` → no hits; the applier never reads it. It is display/record material, and nothing in `yulon/ui/` renders it either. |
| `deploy` | `Deploy` `:157-164` | Copy `src` (clone) → `dest` (server dir), with basename renames. `_deploy()` `apply.py:1282-1296`, `_undeploy()` `:1298-1326`, `_deploy_target()` `:1335-1341`. |
| `patches` | `Patch` `:167-181` | find/replace (optionally regex) against a file in the server dir or, with `in_clone`, in the clone; `when` install/remove. `_patches()` `apply.py:1342-1358`, `_apply_patch()` `:1485`. |
| `prompts` | `Prompt` `:211-226` | Values asked at configure time, referenced as `{key}` in sql statements, conf defaults and patch replacements. |
| `build` | `Build` `:103-106` | `rebuild: bool` — declares that install/remove needs the worldserver rebuilt. **Nothing acts on it:** `grep -n 'rebuild' yulon/apply.py` → no hits. |

### `manifest_store.py`

`ManifestStore` `:43` (`game_dir()` `:51`, `index_path()` `:55`, `item_path()` `:59`), ETag caching
(`_ETAG_SUFFIX` `:35`, `_TIMEOUT_SECONDS = 20.0` `:36`), `ManifestError` `:39`. Refresh is reached
from `modules.refresh()` (`controller_wow_wotlk/modules.py:65`) which, as noted in §1 row 9, has no
UI caller.

---

## 6. SOAP grep over the whole `pylauncher/` tree

Patterns run: `soap` (case-insensitive), `7878`, `Ra.Enable`, `RA.Enable`, `Console.Enable`,
`AC_SOAP`, and `enable` across `catalog/installers/`.

**Nothing anywhere sets `SOAP.Enabled=1`, `AC_SOAP_ENABLED`, `Ra.Enable`, `RA.Enable` or
`Console.Enable`. Nothing creates a SOAP-capable account. `grep -rniI 'enable' pylauncher/catalog/installers/`
returns zero lines.**

Every non-test hit, in full:

| Where | What it is |
|---|---|
| `yulon/catalog/catalog.py:670` | `soap_port: int = Field(default=7878, gt=0, lt=65536)` on `NativeInstall`. **No `catalog.json` entry sets it** — `json.dumps(entry).find('soap_port') == -1` for all four games. |
| `yulon/catalog/composegen.py:369` | `"SOAP_PORT": str(native.soap_port)` — the render token. |
| `yulon/catalog/composegen.py:748` | comment: "a later SOAP setup writes its own port binding". |
| `catalog/installers/wow-wotlk/native/base.yml.tmpl:264` | the **only** template that uses the token: `- "${DOCKER_SOAP_EXTERNAL_PORT:-127.0.0.1:{{SOAP_PORT}}}:7878"` |
| `base.yml.tmpl:32`, `:223`, `:258-261` | comments explaining the loopback pin and that "**Nothing writes** `IMAGE_TAG`" / a SOAP setup would write the whole binding. |
| `yulon/catalog/families/dockerfile.py:76, 130, 132, 153, 196-197, 269, 429` | `SOAP_PASSWORD` used purely as a **secret-name probe fixture** for the secret-leak refusal rule. Not a SOAP feature. |
| `yulon/controller_wow_wotlk/accounts.py:5-6` | prose: SOAP cannot create the first account. |
| `pylauncher/README.md:96` | the same prose. |
| `yulon/controller.py:238` | prose port example ("It published only 8085 and 7878"). |
| `pyplan/bug-checklist.md:795` | open box: the **bash** script installs publish 3306 and 7878 on every interface — about the scripts Phase 7 deleted, not about the Python engine. |
| tests | `tests/data/wotlk-rendered/docker-compose.yml:264`, `tests/data/wotlk-compose-config*.json`, `tests/support_compose.py:73-74, 256-261`, `tests/test_composegen.py:251-254`, `tests/test_compose_fixture.py:115-118, 312-317, 331-340, 412, 457, 503-506, 531`, `tests/test_dockerfile.py` (many, all `SOAP_PASSWORD` fixture), `tests/test_families_cmangos.py:1047`. |

**Net effect:** the WotLK compose file *publishes* container port 7878 on `127.0.0.1` — but the
worldserver's `SOAP.Enabled` is never set, so the port maps to a listener nothing turns on. The
three CMaNGOS entries publish no SOAP port at all (their templates never mention `SOAP_PORT`).

---

## 7. `catalog/catalog.json` — the four entries

`schema_version` + `games` (4). Per entry:

| | `wow-wotlk` | `wow-tbc` | `wow-vanilla` | `wow-tortoise` |
|---|---|---|---|---|
| `soap_port` | **absent** (model default 7878, `catalog.py:670`) | absent | absent | absent |
| `containers` | `db=ac-database, auth=ac-authserver, world=ac-worldserver, db_import=ac-db-import, client_data=ac-client-data-init` | `tbc-db / tbc-realmd / tbc-mangosd` | `vanilla-db / vanilla-realmd / vanilla-mangosd` | `tortoise-db / tortoise-realmd / tortoise-mangosd` |
| `ports` | `auth 3724, world 8085, db 3306` | `3724 / 8085 / 3306` | `3724 / 8085 / 3306` | `3724 / **8090** / 3306` |
| `databases` | `auth=acore_auth, characters=acore_characters, world=acore_world, extra=[acore_playerbots], playerbots=acore_playerbots, ale=acore_ale` | `realmd / characters / mangos`, extra `[logs]` | `realmd / characters / mangos`, extra `[logs]` | `tw_logon / tw_char / tw_world`, extra `[tw_logs]` |
| `console` | **absent** → defaults `AC>`, `prompt_precedes_answer=True` (`catalog.py:977-985`) | `{prompt: "mangos>", prompt_precedes_answer: false}` | same | same |
| `accounts` | **absent** → default `scheme: "azerothcore"` (`catalog.py:946-947`) | `{scheme: "mangos_srp6"}` | `{scheme: "mangos_srp6"}` | `{scheme: "mangos_sha"}` |
| `realmlist` | **absent** → defaults `table=realmlist, address_column=address, local_address_column="localAddress", realm_id=1` (`catalog.py:920-928`) | `{table: realmlist, address_column: address, local_address_column: **null**}` | same | same |
| `has_manifests` | **`true`** | absent (false) | absent (false) | absent (false) |
| Anything about SOAP / RA / console-enable | none | none | none | none |

`catalog.json:5-6` (`wow-wotlk`), `:97-98` (`wow-tbc`), `:459-460` (`wow-vanilla`), `:815-816`
(`wow-tortoise`); the `containers`/`ports`/`databases` blocks at `:65-86`, `:420-437`, `:776-793`,
`:1073-1090`.

Template lines asked for: `catalog/installers/wow-wotlk/native/base.yml.tmpl` `20-40` is the
comment block about the image tag and the SOAP-binding writer ("Nothing writes `IMAGE_TAG`");
`250-270` is the worldserver `environment:` (`AC_LOGIN/WORLD/CHARACTER/PLAYERBOTS_DATABASE_INFO`,
all `root` @ `ac-database:3306`) plus the two `ports:` entries, the SOAP one at `:264`. The override
template `catalog/installers/wow-wotlk/native/override.yml.tmpl` is 23 lines: the modules bind
(`:21`) and `{{ENVIRONMENT}}` (`:23`) — nothing about SOAP, and `:10-12` forbids a `ports:` entry.

---

## 8. Database writes performed TODAY while the worldserver may be running

| Seam | Where | Databases | Guard on the server being stopped? |
|---|---|---|---|
| `accounts.create_account()` | `yulon/controller_wow_wotlk/accounts.py:312`; reached from `ui/controller_view.py:1469-1473` | **auth only** (`_ACCOUNTS_DB` `:111`; tables `account`, `account_access`, `realmcharacters`) | **No.** No status check anywhere in `accounts.py`. The Create button is enabled whenever `entry.accounts.scheme is not None` (`ui/controller_view.py:1431`), never on server state. |
| `networking.apply()` realmlist UPDATE | `yulon/networking.py:3617-3632`, statement built by `realmlist_sql()` `:2888-2894`; reached from `ui/controller_view.py:323`, `:1874` | **auth** (`UPDATE {databases.auth}.{realmlist} SET address=… WHERE id=1`) | **No.** It runs whenever a plan is applied, and the report carries `restart_required=True` afterwards (`networking.py:3633`, `:3653`). A failure is *reported*, not raised (`:3623-3630`). |
| `apply.Applier` SQL mods | `yulon/apply.py:1359-1387` via `DockerSql`; reached from `ui/controller_view.py:1782-1785` | **any of auth / characters / world / playerbots**, chosen by the manifest's `step.db` (`manifest.py:119`, `apply.py:60`) | **No.** No running check in `apply.py` (§5). Buttons gated only on store/applier presence. |
| `apply.Applier` conf/deploy/patch writes | `apply.py:1282-1296`, `:1342-1358`, `:1388-1410` | not a DB; writes files under the server dir | **No.** |
| `maintenance.restore()` | `yulon/controller_wow_wotlk/maintenance.py:949` | every database named inside the dump (`_databases_named_in()` `:1255`) | **Yes — a refusal, not a stop.** `plan_restore()` `:934-946` adds a refusal when `spec.world` or `spec.auth` is in the container census; `RestorePlan.allowed` is False (`:762`) and the Restore button stays disabled (`ui/controller_view.py:1688`). It also refuses when `spec.db` is **not** running (`:947-948`). Nothing stops the server for the user. |
| `maintenance.backup()` | `maintenance.py:460` | reads all non-system schemas (`server_databases()` `:447`, `SYSTEM_SCHEMAS` `:120`) | **Requires the DB container up** (`:503-507`) and **does not require the world/auth to be down** — it records `server_was_running = spec.world in names or spec.auth in names` (`:544`) as a report field only. A hot backup is allowed. |
| `repair.reset_unfinished()` | `yulon/controller_wow_wotlk/repair.py:221-288` — `DROP DATABASE` | **auth / characters / world** (`CORE_DATABASES`, `_DB_KEYS`) | **Not on the server being stopped** — the guard is on *content*: refuses `populated` (`:257-262`) and `unreadable` (`:263-266`). Reached only through the Server tab's two-press Repair (`ui/controller_view.py:762`, `:1223-1227`) → `Controller.repair_import` (`controller.py:322`), which is offered only when `state.repairable` and the entry names an import service (`ui/controller_view.py:960-966`) — i.e. **`wow-wotlk` only**. TBC's `reset_unfinished()` raises `NotImplementedError`; Vanilla builds a reset nothing wires; Tortoise defines none. |

**Summary:** three write paths run against a possibly-live server with no stop guard at all —
account creation (auth), the realmlist UPDATE (auth), and module SQL (any of the four). Restore is
the only path that refuses.

---

## 9. MySQL READ paths

| Read | Seam | Line |
|---|---|---|
| `DockerSql.query()` — `--batch --skip-column-names` over `docker exec … mysql` | `apply.DockerSql` | `yulon/apply.py:504-528` |
| account existence / id / GM level | `DockerSql.query` | `accounts.py:543`, `:555`, `:562` |
| `DockerMysql.databases()` — the schema list | `docker exec … mysql`, `maintenance.DockerMysql` | `maintenance.py:252`; callers `maintenance.py:457` (`server_databases`), `repair.py:140`, `:268`, `:283` |
| `DockerMysql.dump_into()` / `load_from()` | `mysqldump` / `mysql` via `docker exec` | `maintenance.py:273`, `:278`, argv at `:288` |
| import-state table probe | `SqlQuery.query` | `repair.py:126 import_state()`, `_tables_in()` `:195`, `_player_data()` `:314` |
| realmlist current address | `docker.sql_query` | query built by `networking.realmlist_address_query()` `:3057`; **the only caller is `yulon/catalog/native.py:2920`** (install-time), never the Networking tab |
| CMaNGOS install-time marker gates | `docker.sql_query` (`yulon/docker.py:3421`) via `sqlplan.SqlQuery` | `yulon/catalog/families/cmangos.py:1173`, `:1188`, `:1255`, `:1284` |
| `mysql_client()` probe — which client binary the DB image ships | `docker exec` | `apply.py:329-408` |
| container status / health (not MySQL) | `docker ps` / `docker inspect` | `docker.status()` `:1788`, `_status_safe()` `:1804`, `health()` `:1828`, `container_state()` `:1883`, `port_conflicts()` `:2233`, `published_bindings()` `:2302` |

`STATUS_TIMEOUT_SECONDS = 30.0` (`docker.py:935`); `STOP_GRACE_SECONDS = 300` (`docker.py:938`,
with the 2026-08-23 measurement in its docstring: 90.7 s / 73.4 s / 58.3 s shutdowns at ~1980
characters online, and Docker's own default of 10 measured killing a live save with exit 137).

---

## 10. Does anything in Yu'lon know whether the WoW client is running?

**No — not in the Python app.** `grep -rniI 'psutil|wow\.exe|tasklist|pgrep|process_iter|client_running|auto.?stop'`
over `pylauncher/`:

* `yulon/platform.py:3330` — the word `psutil` appears once, in a docstring saying `docker info` is
  used **instead of** `psutil`. `psutil` is not imported anywhere.
* `tests/test_clientdir.py:94-101` — `Wow.exe` as a *path* the folder picker might return.
* `pylauncher/catalog/installers/steam-deck/setup-gaming-mode.sh` — the only place a client process
  is detected: `CLIENT_PATTERN='Wow[.]exe|wine.*[Ww]o[Ww]'` (`:86`), `pgrep -fi` at `:185` (wait
  for the client to appear, 300 s budget `:83`) and `:202` (wait for it to close, then stop the
  stack). Its `INTERACTIVE` guard (`:105-109`) exists because a Steam-launched process gets no tty.
  The script is **not** copied out of the bundle by anything — its own header says so (`:65`).
* `tests/test_steam_deck_script.py` drives that script with a stubbed `pgrep`.

So: auto-stop exists once, in bash, on one platform, reachable only if the user wires a Steam
shortcut by hand.

---

## 11. Does anything write into the client folder today?

Two writers exist in the tree; **one is reachable, one is not.**

* **`manifest` `client` primitive → `Applier._client()`** (`yulon/apply.py:1411-1431`): writes into
  `<client>/Interface/AddOns/<name>`, `<client>/Interface`, or `<client>/Data`. **Opt-in twice
  over:** it is skipped entirely when `self.client_dir is None` (`:1413-1415`), and `client_dir`
  reaches the applier only through `ui/controller_view.py:392-394`, which itself passes whatever
  the install recorded (may be `None`). It also only ever runs for `wow-wotlk` (the only entry with
  `has_manifests: true`). Its `ClientFile` docstring (`manifest.py:184-189`) pins the rule: "the app
  never ships game assets — these come from the item's own open-source repo and go into the
  user-supplied client directory."
* **`networking.write_client_realmlist()`** (`yulon/networking.py:3711-3737`): **unreached.** No
  caller in `yulon/` or `main.py`; only `tests/test_networking.py:350, 357, 363` and the spine
  allowlist at `tests/test_spine.py:2352`.

Read-only client access that *is* reached: `catalog/families/clientdir.py:67 validate()` at install
time (MPQ count, locale folders, repack detection keyed on a root `realmlist.wtf` `:246-278`, free
space `:280`).

---

## 12. `pyplan/checklist.md` — the lines asked for

**Phase 4 (all ticked), lines 52-56:**
`4.1 log_panel.py — streaming output widget` (`:53`); `4.2 catalog_view.py — browsable catalog`
(`:54`); `4.3 controller_view.py — per-install management (+ LAN/internet networking auto-setup
control)` (`:55`); `Phase 4 exit criteria met — human click-through against a live server on the
Ubuntu 24.04 VM, 2026-08-21` (`:56`).

**Phase 5 (all ticked), lines 62-66:** `5.1` silent Docker Desktop / WSL2 provisioning (`:63`);
`5.2` PyInstaller specs (`:64`); `5.3` GitHub Actions release matrix (`:65`); `5.4 Application
self-update check (README §10)` (`:66`); exit criteria (`:67`).

**7.9 —** `checklist.md:2120`, **ticked**:
> `- [x] 7.9 Controllers — controller_wow_tbc/, controller_wow_vanilla/, controller_wow_tortoise/ mirroring controller_wow_wotlk/; mysql → db.client in apply.py/maintenance.py; CMaNGOS-family account creation (was 7.1–7.3 before the scope change; still owed, now after install)`

with gate evidence under `pyplan/gates/7.9-cmangos/` (`:2124`, `:2169`, `:2198`, `:2230`, `:2234`)
and the closing note at `:2308` ("VANILLA CLIENT LOGIN DONE 2026-09-03 — 7.9 closed"). Other 7.9
references: `:1461`, `:2107`, `:2257`. `7.10` at `:2331`.

**Cross-cutting entries for the tabs — asked for, and this is the could-not-ask/no answer:**

* `grep -n 'Console tab' pyplan/checklist.md` → **one hit**, `:3174`, inside the Phase 6 Windows
  cross-cutting entry: "**Not** the Console tab's `docker attach` — `send_command()` refuses on
  `pty_supported()` first, and 6.5 already scopes the console to Linux/macOS. Two successive commit
  messages claimed more than this and were corrected; the claim is easy to make and worth checking
  each time." Full paragraph `:3168-3178`.
* `grep -n 'Accounts tab' pyplan/checklist.md` → **no hits.**
* `grep -n 'Modules tab' pyplan/checklist.md` → **no hits.**

So there is no Cross-cutting entry describing the Accounts tab or the Modules tab in
`checklist.md`. The Accounts tab *is* described in `pyplan/bug-checklist.md:239`, and the Modules
tab in `pyplan/bug-checklist.md:394`.

---

## 13. `pyplan/bug-checklist.md` — the 30 open boxes, mapped to a Phase 8 feature

`grep -c '^- \[ \]'` = **30**.

| Line | Defect (short) | Phase 8 feature that would sit on top of it |
|---|---|---|
| 158 | Python bind-mount probe omits the SELinux relabel (`docker.py:2428`, no `:z`/`:Z`) | none directly — install-time. Would break any new feature that adds a bind mount (config editor writing into a mounted `etc/`). |
| 163 | The whole `preflight` module is unreachable on Linux | 16 (doctor/diagnostics) — a doctor built on `preflight` would inherit the same unreachability. |
| 167 | `_cpu_check`'s remedy names Docker Desktop on native Linux | 16 (doctor). |
| 188 | `compose_file()` crashes on an unreachable WSL path (`installer.py:67`, WinError 64) | 7 (dashboard), 9 (modules) — any tab polling a WSL-resident install. |
| 193 | "Find in WSL…" appears on every tile and always fails (`catalog_view.py:263`) | none of the 20 directly; adjacent to 14 (single-instance/attach). |
| 227 | Two installs of one game cannot coexist (global `container_name:`s) | 7 (a dashboard per install), 10 (settings), 17 (Steam: one shortcut per install). |
| 234 | Tortoise on Windows: `platforms: ["linux"]`, no code path runs an install script inside WSL | 15 (LAN/internet on Windows), and every feature scoped "all four games". |
| **239** | **The Accounts tab is create-only — no list, no set-password, no standalone GM level** | **11 (accounts) — this box IS the feature gap.** |
| 242 | No floor on Docker's data root | 16 (doctor), 12 (backups need space). |
| 289 | `main.py:306 window.resize(1100, 750)` with no `QScreen` check; no `QSettings`/`restoreGeometry` anywhere | 10 (settings with guardrails) — there is no settings persistence to build on; 6/7 (large new views make it worse). |
| 365 | `wait_for_server()` can never report readiness on Fedora (bash, `pipefail` + SIGPIPE) | 7 (uptime/readiness), 16 (doctor). Bash-script defect — Phase 7 deleted the scripts; verify before carrying forward. |
| 375 | A fresh empty folder skip-compiled against another install's images | 13 (self-update of core: the same image-identity question). |
| **394** | **Modules tab opens with nothing selected and both buttons enabled; `_module_action()` returns silently (`controller_view.py:1307`)** | **9 (module management) — the tab this feature extends.** |
| 437 | Prompt table answers "yes" to the extraction failure that matters most (`installer.py:189`) | none of the 20 — install-time. |
| 453 | `"Build mmaps now?"` has no rule of its own | none of the 20 — install-time. |
| 489 | A finished build is wiped by a prompt the launcher answers "yes" to | 13 (self-update of core would re-enter this path). |
| 494 | `install-wow-tbc.sh` can never report readiness on every Linux distro | 7, 16. Bash. |
| 499 | A TBC mangosd stuck in a restart loop is reported as a fully successful install | **7 (live dashboard) — the dashboard would show "up" for a crash-looping server.** |
| 501 | Tortoise `create_default_account()` uses `printf | docker attach` against a tty container — refused | **11 (accounts) and 8 (console) — the exact transport limitation of §2, in bash.** |
| 510 | A dead Tortoise server is recorded as an installed one | 7 (dashboard), 14 (auto-stop against a server that never ran). |
| 514 | Compiled-image reuse check trusts one global fixed tag (Vanilla + TBC) | 13 (self-update of core). |
| 518 | `MoveMapGen` runs from the wrong working directory — marked "Refuted this round — do not report again" | 3 (teleport) touches mmaps indirectly; otherwise none. |
| 549 | The port refusal claims more than it can know ("Nothing has been changed" after an 842 MB clone) | 14 (single-instance guard — same guard). |
| 552 | First poll interval says "status: unknown" with Start enabled against a running server (`controller_view.py:325-335`, 5000 ms, no initial `refresh_status()`) | **7 (live dashboard) — the poll loop the dashboard would be built on.** |
| 616 | "Existing install found" fires on a completely empty directory | 18 (uninstall/purge), 10 (settings). |
| 631 | Readiness check greps for three strings the Tortoise core cannot print (`:1102`) | 7, 16. |
| 751 | `ac-client-data-init` runs on compose's implicit `default` network | none of the 20. |
| 764 | The captured SELinux label shape is not what the shipped Fedora script writes | none of the 20. |
| **795** | **Script installs publish MySQL and the SOAP admin console on every interface, not loopback** | **the SOAP question directly — this is the only SOAP-shaped box, and it is about the deleted bash scripts, not the Python engine (§6).** Also 15 (LAN/internet). |
| 1189 | `DbFacts.charset` has no `pattern=`, so a bad catalog entry fails mid-install | none of the 20 — catalog validation. |

---

## 14. Test files covering each feature that exists

Real names, from `ls pylauncher/tests/` and `ls pylauncher/tests/integration/`.

| Feature (that exists / partly exists) | Test files |
|---|---|
| Console (7 logs, 8 console) | `tests/test_console.py`, `tests/test_controller_view.py`, `tests/test_log_panel.py`, `tests/test_docker.py` |
| Accounts (11) | `tests/test_accounts.py`, `tests/test_controller_view.py`, `tests/integration/test_accounts_live.py` |
| Backups + validate/restore (12) | `tests/test_maintenance.py`, `tests/test_controller_view.py` |
| Module install/remove (9) | `tests/test_apply.py`, `tests/test_manifest.py`, `tests/test_manifest_store.py`, `tests/test_controller_view.py` |
| App self-update (13) | `tests/test_update.py`, `tests/test_main.py` |
| Keep-awake (14) | `tests/test_platform.py`, `tests/test_families_azerothcore.py` (`:216-234`, `:1550-1559`), `tests/test_install_wiring.py:307` |
| Single-instance / port conflicts (14) | `tests/test_docker.py`, `tests/test_docker_guard.py`, `tests/test_controller.py` |
| LAN / internet (15) | `tests/test_networking.py` |
| `write_client_realmlist` (19, unreached) | `tests/test_networking.py:345-363`, allowlisted in `tests/test_spine.py:2352` |
| Steam Deck auto-stop script (14, bash) | `tests/test_steam_deck_script.py` |
| Provision / headless doctor-ish (16) | `tests/test_provision.py`, `tests/test_main.py`, `tests/test_platform.py` |
| Remove containers (18) | `tests/test_controller.py`, `tests/test_docker.py` |
| Per-fork controllers | `tests/test_controller_wow_tbc.py`, `tests/test_controller_wow_vanilla.py`, `tests/test_controller_wow_tortoise.py`, `tests/test_controller_packages_agree.py`, `tests/test_tortoise_boot_facts.py` |
| Catalog facts (SOAP port, console, accounts, realmlist) | `tests/test_catalog.py`, `tests/test_catalog_invariants.py`, `tests/test_composegen.py`, `tests/test_compose_fixture.py` |
| Client folder validation (19, read side) | `tests/test_clientdir.py` |
| Live gates | `tests/integration/test_wotlk_live.py`, `test_docker_live.py`, `test_sqlplan_live.py`, `test_accounts_live.py` |

**No test file exists for** features 1-6, 10, 17, 20, module tuning knobs, config editor, console
history/autocomplete, account list/set-password, or an app-side client-process watcher — because
none of those exist to test.

---

## 15. Judgement calls I stopped on rather than deciding

* **Whether `soap_port` on `NativeInstall` (`catalog.py:670`) is intended as a future switch or as
  dead weight.** The field exists, the token renders, the compose port maps, nothing enables SOAP
  in the worldserver, and no `catalog.json` entry sets the field. That is a design decision, not a
  fact I can read.
* **Whether the deleted-bash-script bug boxes (365, 494, 499, 501, 510, 514, 518, 631, 795) still
  bind after Phase 7 removed `install-*.sh`.** Several name files that Phase 7's box says no longer
  exist. I did not re-check the file system for each because the boxes are still open and the
  mapping asked for is "which feature would sit on top of it", not "is it still live".
* **Whether `Applier.configure()`, `write_client_realmlist()`, `console.attach()`,
  `modules.refresh()` and the `dbc` copier are intended-but-unwired or abandoned.** All five are
  written, tested, and have no caller in `yulon/` or `main.py`. That is reported as
  "partly exists — unreached" with the grep evidence; which of the two it is, is the owner's call.
