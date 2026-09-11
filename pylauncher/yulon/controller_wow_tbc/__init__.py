"""TBC controller — the CMaNGOS-shaped siblings of `controller_wow_wotlk/` (roadmap 7.9).

Each server gets its own controller package so game-specific behavior stays
isolated. This one manages the `wow-tbc` catalog entry: CMaNGOS mangos-tbc with
the cmangos/playerbots fork, three containers named `tbc-db` / `tbc-realmd` /
`tbc-mangosd`, four MariaDB schemas (`realmd`, `characters`, `mangos`, `logs`)
and a worldserver console that is not AzerothCore's.

**Almost nothing here is an implementation.** The WotLK package's account
crypto, console transport and backup/restore engine are already game-agnostic —
they take the scheme, the prompt, the container spec and the schema names as
arguments — so this package imports them and supplies THIS game's arguments
from `catalog.json`. What is written out here is only what AzerothCore's
version cannot answer:

* `docker_ctl.ready_spec()` — CMaNGOS prints `Avg Diff:`, never `ready...`, and
  the entry names no auth marker at all, so `docker.azerothcore_ready()` is
  wrong in both halves.
* `controller.TbcController.wait_ready()` — the base class calls
  `azerothcore_ready()` directly, so inheriting it unchanged would poll a
  mangosd log for a line it never prints until the 480s default ran out.
* `repair.py` — AzerothCore's probe reads `updates`/`updates_include`, the two
  tables ITS updater keeps. CMaNGOS's core has no such pair, so the evidence
  here is the `yulon_install` marker row this app's own installer writes
  (`catalog/families/sqlplan.py`), and the one question that evidence cannot
  answer is stated rather than guessed at — see that module.

`modules.py` binds `manifests/wow-tbc/` (roadmap 8.7b), and what it binds is
the interesting part. An AzerothCore module is a git repository compiled into
the worldserver; CMaNGOS has no such mechanism, so there is nothing here to
clone or link in. What this core DOES have is a conf file it reads at startup
and a world database it loads at startup — so on this game a "module" is a
**configuration activation** (`conf[].keys` into `etc/mangosd.conf`) or a **SQL
mod** (`sql[].statement` into `mangos`), both of which the manifest schema
already expressed for AzerothCore's own `mods` family. The `modules`, `ale` and
`kegs` indexes exist and are empty, because "there are none" is a fact and a
missing file is an error message in the user's Modules tab.

One schema field was added for it: `Build.restart`. `ApplyReport.
restart_recommended` was derived from NPCs, direct SQL and server DBCs, and a
conf-only item has none of the three — so it reported "nothing further needed"
over a value mangosd would not read until the next start.

Public entry points, for the view that dispatches on `entry.id`:

* `controller.TbcController(server_dir, *, wsl_distro=...)`
* `docker_ctl.SPEC`, `.ready_spec()`, `.wait_server_ready()`,
  `.wait_db_healthy_ready()`, `.port_conflicts_here()`, and the shared
  `start`/`start_staged`/`stop_staged`/`remove`/`status`/`health` re-exports
* `accounts.create_account()`, `.sql_for()`, `.SCHEME`
* `console.send_command()`, `.attach()`, `.PROMPT`,
  `.PROMPT_PRECEDES_ANSWER`, plus `ConsoleReply`/`ConsoleError`/`can_send`
* `maintenance.mysql_for()`, `.backup()`, `.plan_restore()`, `.restore()`,
  `.CORE_DATABASES`, plus the shared report types
* `repair.import_state()`, `.import_gate()`, `.db_password()`,
  `.reset_unfinished()` (which raises — read its docstring before wiring it)
* `modules.store()`, `.applier(server_dir, sql=...)`, `.apply_module()`,
  `.refresh()`, `.GAME` — note that `applier()` REQUIRES its SQL runner,
  unlike WotLK's, because this game's database password is generated per
  install rather than fixed in the catalog
"""
