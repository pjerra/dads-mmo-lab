"""Three facts about the Tortoise core, each measured on a running server.

Every one of these was wrong in `catalog.json` on 2026-09-03, and every one was
invisible to the whole test suite because each is a fact about a THIRD-PARTY
BINARY -- what it prints, where it looks, which file it reads. A unit test
cannot discover any of them; only running the thing can. What a unit test CAN
do is stop a measured answer from being edited back to a guess, and that is all
this file is for.

The gate that found them (7.6, `yulon-ubuntu`) is written up in
`pyplan/checklist.md`. The short version of each is on its own test.
"""

from __future__ import annotations

import re
from pathlib import Path, PurePosixPath

import pytest

from yulon.catalog.catalog import load_catalog

TORTOISE = "wow-tortoise"


def _native(game: str = TORTOISE):
    entry = load_catalog().get(game)
    native = entry.install.native
    assert native is not None, f"{game} carries no install.native block"
    return native


def test_the_ready_marker_matches_what_this_core_actually_prints() -> None:
    """Tortoise announces itself with a line no other CMaNGOS entry uses.

    The marker was `World initialized|MaNGOS.*started up successfully|Ready to
    login`. Measured against a Tortoise worldserver that had finished booting
    and was serving its main loop: NONE of the three appears in its log, not
    once. What it prints is

        World server is up and running! Loading time: 0 minutes 32 seconds

    So a perfectly healthy Tortoise install waited out the full 1800-second
    timeout and then told the user "The server started but never reported
    ready." The install could not have passed on any machine.

    Its two siblings are not a guide here and that is the point: `wow-vanilla`
    and `wow-tbc` both use `Avg Diff:`, the world-tick line, and a `grep -c` for
    it over this core's whole log returns 0. Same family, same engine, different
    build, different words -- the same shape as MoveMapGen's exit code.
    """
    marker = _native().ready.world
    assert marker is not None
    banner = "World server is up and running! Loading time: 0 minutes 32 seconds"
    assert re.search(marker, banner), (
        f"the ready marker {marker!r} does not match the line this core prints when it is "
        f"ready ({banner!r}), so the install waits out its timeout on a healthy server"
    )
    assert not re.search(marker, "Avg Diff: 100ms"), (
        "matching the siblings' tick line would be a coincidence, not a reading: this core "
        "never prints it"
    )


def test_the_auto_updater_is_pointed_at_the_directory_that_holds_the_migrations() -> None:
    """One directory too high, and the updater says nothing when it finds nothing.

    The core applies its own SQL migrations from
    `Database.AutoUpdate.Path/<WorldUpdateName>` and records each in a
    `migrations` table. The catalog said `/opt/tortoise/sql/`, so it looked for
    `/opt/tortoise/sql/world`. The image has them at
    `/opt/tortoise/sql/database_updates/world` -- 125 files.

    `ProcessTargetUpdates` SKIPS a missing directory without an error, so the
    only symptom was the worldserver dying later, on its first query against a
    column a migration adds:

        SELECT DISTINCT(script_name) FROM spell_template
        [1054] Unknown column 'script_name' in 'SELECT'
        Your database structure is not up to date.

    With the path corrected the updater applied all 125 and the server reached
    its main loop.

    **The whole path, not its last component.** The first version asserted
    `endswith("database_updates")`, which a review pointed out accepts any
    parent at all: `/opt/tortoise/database_updates/` -- one directory too HIGH,
    the same class of mistake in the other direction -- passed it. The path
    below is where the 125 files were counted in the image that booted, so it
    is a measurement and it belongs here whole.
    """
    keys = _native().cmangos.conf.files["mangosd.conf"].keys  # type: ignore[union-attr]
    path = keys["Database.AutoUpdate.Path"].strip('"')
    assert path.rstrip("/") == "/opt/tortoise/sql/database_updates", (
        f"Database.AutoUpdate.Path is {path!r}; the migrations were counted at "
        "/opt/tortoise/sql/database_updates/world (125 files) in the image that booted, and "
        "the updater skips a missing directory in silence"
    )


def test_tortoise_materialises_the_playerbot_conf_its_siblings_do() -> None:
    """The bots this build is named for were disabled by a file that was never written.

    `8176a2ec` compiled playerbots into the Tortoise image on the owner's
    decision. The conf table then materialised `mangosd.conf` and `realmd.conf`
    and not `aiplayerbot.conf`, though the image ships `aiplayerbot.conf.dist`
    beside the other two, so the server said:

        AI Playerbot is Disabled. No configuration file at
        /opt/tortoise/etc/aiplayerbot.conf

    Compiled in, shipped, and off. Checked against the siblings rather than
    against a literal list, because "Vanilla and TBC write this file and
    Tortoise does not" is the whole finding, and a fifth CMaNGOS game should
    inherit the question rather than repeat the omission.
    """
    wanted = "aiplayerbot.conf"
    for game in ("wow-vanilla", "wow-tbc"):
        assert wanted in _native(game).cmangos.conf.files, (  # type: ignore[union-attr]
            f"{game} no longer writes {wanted}; this test compares against it"
        )
    assert wanted in _native().cmangos.conf.files, (  # type: ignore[union-attr]
        f"wow-tortoise does not write {wanted}, so the playerbots compiled into its image "
        "never load"
    )


@pytest.mark.parametrize("game", ["wow-tortoise", "wow-vanilla", "wow-tbc"])
def test_every_cmangos_game_asks_for_the_bot_population_the_owner_set(game: str) -> None:
    """500, in whichever file each entry writes it.

    Not a Tortoise fact, but the assertion that stops the file above being added
    empty: a conf table can name `aiplayerbot.conf` and set nothing, and the
    server would start the default population instead of the owner's.
    """
    conf = _native(game).cmangos.conf.files["aiplayerbot.conf"]  # type: ignore[union-attr]
    assert conf.keys.get("AiPlayerbot.MinRandomBots") == "500"
    assert conf.keys.get("AiPlayerbot.MaxRandomBots") == "500"


def test_tortoise_imports_the_playerbot_sql_its_own_bots_query() -> None:
    """Compiled in, configured, and then dead on a missing table.

    Enabling `aiplayerbot.conf` moved the crash rather than removing it: the
    bots initialise, load their area levels, and then die on

        select id, name, class from ai_playerbot_weightscales
        [1146] Table 'tw_world.ai_playerbot_weightscales' doesn't exist

    Nothing had ever imported the module's SQL. Vanilla's plan has carried a
    `playerbots characters` and a `playerbots world` phase all along -- the
    second listing both `sql/world/*.sql` and `sql/world/classic/*.sql`,
    because the classic subfolder is where the tables the bots read actually
    live -- and Tortoise's plan had neither.

    This is the third time on this entry that a thing was shipped and then not
    switched on: the bots were compiled into the image, then the conf that
    loads them was not written, then the SQL they read was not imported. Each
    step revealed the next only by running the server.
    """
    from yulon.catalog.catalog import load_catalog

    sql = _native().cmangos.sql  # type: ignore[union-attr]
    phases = {phase.name: phase for phase in sql.phases}
    assert "playerbots world" in phases, "the module's world SQL is never imported"
    assert "playerbots characters" in phases, "the module's character SQL is never imported"

    # WHICH DATABASE each phase loads into, which the phase names do not say.
    # A review moved `playerbots world` to `tw_char` and every assertion above
    # still held: the module's tables land in the characters schema, the
    # server's boot query still finds no `tw_world.ai_playerbot_weightscales`,
    # and the crash quoted in this docstring comes back unchanged.
    assert phases["playerbots world"].into == "tw_world", (
        "the bots' boot query names `tw_world.ai_playerbot_weightscales`; a phase that loads "
        f"into {phases['playerbots world'].into!r} imports the files and still leaves that "
        "table missing"
    )
    assert (
        phases["playerbots characters"].into == "tw_char"
    ), "the character-side module tables belong in the characters schema"

    # The exact glob. `"classic" in pattern` was also true of
    # `.../world/classical/*.sql` -- a directory that does not exist, matching
    # no file, importing nothing, and passing.
    patterns = phases["playerbots world"].files or ()
    assert any(pattern.endswith("/sql/world/classic/*.sql") for pattern in patterns), (
        "the `classic` subfolder holds `ai_playerbot_weightscales` and the rest of the "
        f"tables the bots query on boot; the phase lists only {list(patterns)}"
    )
    assert any(pattern.endswith("/sql/world/*.sql") for pattern in patterns), (
        "the module's top-level world SQL is imported alongside the classic subfolder, as "
        f"wow-vanilla does; the phase lists only {list(patterns)}"
    )

    vanilla = load_catalog().get("wow-vanilla").install.native
    assert vanilla is not None and vanilla.cmangos is not None
    sibling = {phase.name for phase in vanilla.cmangos.sql.phases}
    assert {
        "playerbots world",
        "playerbots characters",
    } <= sibling, "wow-vanilla no longer carries the phases this test compares against"


def test_the_import_is_verified_by_the_tables_the_bots_need() -> None:
    """A table COUNT is not a schema check, and that is how this got through.

    The only world-side verification was `COUNT(*) FROM information_schema.tables
    >= 150`. The database that crashed the server on boot had 285 tables, so it
    passed comfortably while missing every table the bots read and 125
    migrations besides. A count answers "did something get imported", never
    "did the right things".

    The `ai_playerbot%` count is the same check `wow-vanilla` has carried since
    its own import was fixed, and it fails on exactly the database this gate
    produced.
    """
    checks = _native().cmangos.sql.verify  # type: ignore[union-attr]
    bots = [c for c in checks if "ai_playerbot" in c.query]
    assert bots, "nothing verifies that the playerbot tables arrived"
    rule = bots[0]
    assert rule.min >= 10, f"a threshold of {rule.min} would pass on an empty import"

    # WHICH database it counts in. A review pointed the rule at `tw_char`: the
    # query still mentions `ai_playerbot`, the threshold is still 10, and the
    # check now passes against the characters schema while the world import it
    # exists to verify goes unexamined -- the exact failure this docstring is
    # about, restored by a one-word edit the assertion above cannot see.
    assert rule.db == "tw_world", f"the rule runs against {rule.db!r}, not the world database"
    assert "table_schema='tw_world'" in rule.query, (
        f"the rule counts tables in whichever schema {rule.query!r} names; the bots read "
        "theirs out of tw_world"
    )


def test_the_ready_budget_covers_a_measured_first_boot_not_a_round_number() -> None:
    """1718 seconds of loading, and a budget that missed it by about two minutes.

    Tortoise's first boot builds `ai_playerbot_equip_cache` -- one row per
    class/spec/level/slot/quality/item -- and it settled at **1,334,079 rows**
    at roughly a thousand inserts a second. The worldserver then printed

        World server is up and running! Loading time: 28 minutes 38 seconds

    with `RestartCount=0`: nothing was wrong, it was simply slow. The install
    had already given up. The budget was 1800 s and the load took 1718 s, which
    sounds like it fits and does not: the `ready` stage's clock starts when the
    stage does, and the world container started 213 s later -- compose recreate,
    then the database health wait -- so the stage needed about 1931 s.

    The floor asserted here is the MEASUREMENT plus that gap, not the number
    that happens to be shipped. `1b88d49d` set the same precedent for TBC after
    a 793 s boot timed out at 600 s: a test that pins the shipped value only
    records what someone typed, and goes green on a value chosen carelessly.

    Its siblings stay at 1800 s and that is not an oversight -- Vanilla reached
    ready in about nine minutes with the same 500 bots, because its Bots module
    builds no such cache. This is a per-fork cost, like the exit code and the
    banner.
    """
    ready = _native().ready
    measured_load = 28 * 60 + 38
    container_start_gap = 213
    assert ready.timeout_s >= measured_load + container_start_gap, (
        f"the ready budget is {ready.timeout_s}s; a first boot measured {measured_load}s of "
        f"loading and the stage's clock starts ~{container_start_gap}s before the container's"
    )
    for game in ("wow-vanilla", "wow-tbc"):
        assert (
            _native(game).ready.timeout_s < ready.timeout_s
        ), f"{game} does not pay Tortoise's equip-cache cost and should not carry its budget"


def test_the_ready_budget_also_covers_the_windows_first_boot_measured_over_9p() -> None:
    """3702 seconds from `up` to `finished` on native Windows, against a budget of 3600.

    The Linux floor above is not the largest one this entry has been measured to
    need. On `yulon-win11-gate` (2026-09-05, `pyplan/gates/7.7-win11-tortoise/`)
    the same first boot -- same 500 bots, same equip cache -- read its world over
    Docker Desktop's 9p share instead of a native disk, and the worldserver printed

        World server is up and running! Loading time: 59 minutes 18 seconds

    3558 s of loading, 3559 s after the container's `StartedAt`, `RestartCount=0`.
    The `ready` stage's own wall clock is what the budget is spent against, and
    the transcript brackets it exactly: `start_staged()` logged at 23:41:37 and
    `install of wow-tortoise finished` at 00:43:19 box-local, **3702 s** -- the
    banner plus the health wait before it and the poll that noticed it after.

    The repo's budget at the time was 3600 s. That run finished only because the
    copy on the box had been given 10800 s beforehand, on the TBC measurement
    that 9p roughly doubles a boot (7.7's note); under 3600 the stage would have
    expired 102 s before it observed the banner and reported a complete, correct
    install as `never reported ready` -- the TBC verdict of the night before,
    repeated. So the number asserted here is the measured stage wall, not the
    loading time and not the value that happens to be shipped; a smaller budget
    that fits Linux fails this test in front of whoever shrinks it.

    One confound, recorded rather than hidden: a 9.98 GB client re-download
    (`tortoise-dl.log`, second entry, 23:59-00:23 box-local at 6.6 MB/s) ran on
    the same box during 24 of those 59 minutes. The boot may be faster without
    it; the budget still has to cover the boot that was measured.
    """
    ready = _native().ready
    windows_stage_wall = 3702  # 23:41:37 -> 00:43:19, from the transcript's own stamps
    assert ready.timeout_s >= windows_stage_wall, (
        f"the ready budget is {ready.timeout_s}s; the Windows first boot's ready stage was "
        f"measured at {windows_stage_wall}s wall over 9p (7.7, 2026-09-05)"
    )


def test_the_fatal_pattern_does_not_fire_on_a_line_that_says_it_is_harmless() -> None:
    r"""`Could not open` matched 4,854 lines that end "Logging to it is off for this run."

    `ready.fatal` exists to end the wait early instead of burning the whole
    timeout on a server that will never be ready. Tortoise is the only entry
    that declares one, and as written it read

        Correct \*.map files not found|Could not open|Database .* not found

    A booted, healthy Tortoise worldserver prints, thousands of times:

        Could not open bot log file ../logs/bot_events.csv (No such file or
        directory). Logging to it is off for this run.

    The line explains in its own second sentence that nothing is wrong. The
    install read the first three words, declared the server dead, and gave up
    about a hundred seconds in -- on a server that went on to report itself up.
    Every "Could not open" in that log, all 4,854 of them, was this one form,
    which is why the exclusion can be this specific rather than a guess.

    Both directions are asserted. A pattern narrowed until it matches nothing
    would pass the first half and quietly retire the fast-fail this field is
    for.
    """
    fatal = _native().ready.fatal
    assert fatal is not None
    benign = (
        "Could not open bot log file ../logs/bot_events.csv (No such file or directory). "
        "Logging to it is off for this run."
    )
    # The exclusion is anchored on the sentence that says the line is harmless,
    # not on the filename in it. The first version keyed on "bot log file",
    # which is the phrase this one boot happened to print; a review pointed out
    # that the playerbots module writes a family of optional logs and any
    # sibling message would have gone straight back to declaring a healthy
    # server dead. This one is the same shape with a different subject, and it
    # must be excluded too.
    sibling = (
        "Could not open combat log ../logs/other.csv (No such file or directory). "
        "Logging to it is off for this run."
    )
    assert not re.search(
        fatal, sibling
    ), f"{fatal!r} fires on an optional log this particular boot did not happen to print"
    assert not re.search(fatal, benign), (
        f"the fatal pattern {fatal!r} fires on a line whose own text says logging was simply "
        "turned off; the install then reports a healthy server as dead"
    )
    for real in (
        "Could not open the configuration file",
        "Correct *.map files not found",
        "Database tw_world not found",
    ):
        assert re.search(fatal, real), f"the pattern no longer catches a real failure: {real!r}"


def test_tortoise_is_no_longer_marked_as_work_in_progress() -> None:
    """`wip` outlived its evidence, and the peer level is the honest one.

    The entry sat at `wip` because its installer had never been driven with a
    real client. On 2026-09-03 it was, on `yulon-ubuntu`: 2133 mmap files, a
    clean import into empty schemas with all three verify rules green, 125
    migrations applied by the core, `World server is up and running!`, ports
    3724 and 8090 listening, and an account created through the app's own
    `sql_for_install()` + `create_account()`.

    `beta` and not `stable`, deliberately: what is proven is that the app can
    install and start this server, and its two CMaNGOS siblings are at `beta`
    on exactly that evidence. What is NOT yet proven for any of the three is a
    human logging in with a game client. Promoting past `beta` is the owner's
    call and wants that first.
    """
    from yulon.catalog.catalog import load_catalog

    catalog = load_catalog()
    assert (
        catalog.get("wow-tortoise").status != "wip"
    ), "the Tortoise installer has been driven end to end; `wip` now understates it"
    peers = {game: catalog.get(game).status for game in ("wow-tbc", "wow-vanilla", "wow-tortoise")}
    assert len(set(peers.values())) == 1, (
        f"the three CMaNGOS entries carry the same class of evidence and should carry the "
        f"same status until one of them earns more: {peers}"
    )


def test_every_tortoise_source_is_pinned_to_a_commit_not_a_moving_branch() -> None:
    """A branch is not a pin, and this entry's whole calibration depends on one.

    The core was cloned by BRANCH (`playerbots-integration-gh`) while Eluna
    beside it carried a `rev`. Six defects were fixed on 2026-09-03 by reading
    what that tree does -- which exit status MoveMapGen returns, where the
    migrations live, what the ready banner says, which conf file the bots want,
    which SQL they read, which log line is harmless. Every one of those answers
    is a property of a particular commit. With the source on a branch tip, the
    next install could clone a tree where any of them has changed, and the
    catalog would still claim the old answers.

    `Source.rev`'s own description already says this ("a pinned server must
    rebuild the same bytes"); the entry simply had not used it. Pinned to
    7c0fb278, the commit that was built, extracted, migrated, booted and logged
    into on `yulon-ubuntu`.

    Asserted over ALL sources rather than the one that was wrong, so a third
    source added later cannot arrive unpinned.
    """
    from yulon.catalog.catalog import load_catalog

    sources = load_catalog().get("wow-tortoise").emulator.sources
    unpinned = [s.repo for s in sources if not s.rev]
    assert not unpinned, (
        f"{unpinned} are cloned from a moving ref; this entry's catalog encodes measurements "
        "taken from one commit, and a branch tip is free to invalidate all of them"
    )
    # Truthiness is a declaration, and a review noted that any wrong hash
    # satisfied it. The core's rev is asserted BY VALUE because every other
    # fact in this file was measured against that tree: moving the pin means
    # taking those measurements again, and this line is where that decision has
    # to be written down rather than discovered on a four-hour install. The
    # rest are held to the SHAPE of a full commit id, because an abbreviation
    # is a prefix and a prefix can stop being unique.
    core = next(s for s in sources if s.repo.endswith("tortoise-wow"))
    assert core.rev == "3a8472e68e4aca5d2675feb9241e924f1c31899c", (
        f"the core is pinned to {core.rev!r}. Until 2026-09-08 it was 7c0fb278, where every "
        "measurement in this file was taken; the pin moved to 3a8472e -- the SOAP interface "
        "(3f9a062) and the account-lockout fix (3a8472e), which the fork's own README now says "
        "are the last of its own work before it archives -- and the measurements were taken "
        "again: the ready banner, the migrations path, the SQL globs and the harmless bot-log "
        "line on a copy of m910q's install (pyplan/gates/tortoise-reimport-rehearsal-m910q-"
        "2026-09-08/), the exit status and the bot conf on the fresh install on yulon-arch "
        "(pyplan/gates/tortoise-fresh-yulon-arch-2026-09-08/). Moving it again means taking "
        "them again"
    )
    for source in sources:
        assert re.fullmatch(
            r"[0-9a-f]{40}", source.rev or ""
        ), f"{source.repo} is pinned to {source.rev!r}, which is not a full commit id"


def test_both_boot_patterns_are_read_as_regular_expressions_not_literal_text() -> None:
    """One flag holds up both of the measurements above, and nothing here watched it.

    `ready.regex` is what decides whether the two patterns are compiled or
    `re.escape`d. Every other test in this file reads the pattern STRINGS, so a
    review's flip of `regex` to `false` left all of them green while retiring
    both facts at once:

    * `ready.world` is four alternatives -- three of them from before the banner
      was measured -- so as literal text it matches a line containing a `|`
      character, which nothing prints. A healthy server would wait out the whole
      ready budget and be reported as never ready, which is the original defect.
    * `ready.fatal` is an alternation carrying a negative lookahead. Escaped, it
      stops matching `Correct *.map files not found` at all, so an install with
      no maps runs its whole timeout instead of failing in seconds -- and the
      lookahead that keeps a healthy server's 4,854 harmless bot-log lines from
      reading as fatal becomes literal text too.

    The suite does catch the flip elsewhere (`test_controller_wow_tortoise.py`),
    which is exactly why it belongs here as well: this file is the one a person
    edits when they change one of these patterns, and the flag they must not
    touch while doing it should fail in front of them.
    """
    ready = _native().ready
    assert ready.regex is True, (
        "both boot patterns are alternations and one carries a negative lookahead; read as "
        "literal text neither can match anything this server prints"
    )
    assert ready.world is not None and ready.fatal is not None
    assert re.escape(ready.world) != ready.world, (
        "the ready marker has no regex syntax left in it -- if that is deliberate, this test "
        "and the `regex` flag both need revisiting"
    )
    assert not re.search(re.escape(ready.world), "World server is up and running!"), (
        "read literally the marker does not match the banner this core prints, which is what "
        "`regex: false` would do to it"
    )
    assert re.search(
        ready.fatal, "Correct *.map files not found"
    ), "the no-maps line is the one fatal this entry can hit in its first seconds"
    assert not re.search(re.escape(ready.fatal), "Correct *.map files not found"), (
        "read literally the fatal pattern misses the failure it exists for, and the install "
        "spends its whole ready budget before saying so"
    )


# --- the fork's own `sql/character_updates/`, which nothing on this tree applies ------


CHARACTER_UPDATES = "src/tortoise-wow/sql/character_updates/*.sql"
"""The directory, spelled as the phase globs it.

Listed read-only on `m910q` 2026-09-09 at
`~/tortoise-server/src/tortoise-wow/sql/character_updates/`; it ships three files.
"""

THE_THREE_FILES = (
    "20260708055500_ai_playerbot_random_bots_index.sql",
    "20260731160000_guild_bank_money_unsigned.sql",
    "20260812142512_character_inventory_copy.sql",
)
"""Every file in that directory on 2026-09-09, in the order the fork's own timestamps put
them in. Named rather than globbed because the ORDER is what is being asserted."""


def _sql_plan():
    native = _native()
    assert native.cmangos is not None
    return native.cmangos.sql


def _phase_names() -> list[str]:
    return [phase.name for phase in _sql_plan().phases]


def test_the_forks_own_character_updates_are_applied_into_the_characters_database() -> None:
    """Nothing on this tree applies `sql/character_updates/`, and a world dies without it.

    Two measurements, a week apart, of one missing phase:

    * **A fresh install crash-loops immediately.** `yulon-arch`, 2026-09-08: the
      world crash-looped on character migration `20260903211500` until these
      files were applied into `tw_char` by hand
      (`pyplan/gates/tortoise-soap-yulon-arch-2026-09-09/README.md`).
    * **An established install crash-loops on a date.** `m910q`, 2026-09-09
      01:09Z: 903 characters, serving gates for weeks, dead on
      `TRUNCATE character_inventory_copy` the first morning honor maintenance
      fell due -- `saved_variables.nextHonorMaintenanceDay` 20705 was that day
      (`pyplan/gates/7.9-rerun-m910q-2026-09-09/README.md`, finding 1). That
      table is created by `20260812142512_character_inventory_copy.sql` and by
      nothing else under `sql/`. The fresh-install reading alone understates
      this: every install that has never had these applied is one
      honor-maintenance day from being unstartable, and the day is weekly.

    **Two separate things on this tree apply SQL and neither reads this
    directory.** `Database.AutoUpdate.Path` points the core's own updater at
    `sql/database_updates/`, whose only children are `character` and `world`
    (read on `m910q`, 2026-09-09) -- a different directory. The fork's own
    `sql/setup_databases.sh` imports `create_databases.sql` and then
    `database_updates/*.sql`, and that top level holds no `.sql` at all. So the
    pinned-path test above is not a substitute for this one: both paths can be
    right at the same time and this directory still be applied by nobody.

    `into` is asserted because the phase name does not say it, and all three
    files edit `tw_char` tables. The world half of the same family of problems
    is the image template's `INSERT IGNORE` rewrite (`3a1ed6ee`), not this.
    """
    phases = {phase.name: phase for phase in _sql_plan().phases}
    assert "character updates" in phases, (
        "wow-tortoise has no phase for `sql/character_updates/`; a fresh install of this "
        "fork does not come up, and an established one dies on its first honor-maintenance "
        f"day. The plan's phases are {_phase_names()}"
    )
    phase = phases["character updates"]
    assert phase.into == "tw_char", (
        "all three files edit characters-database tables (`ai_playerbot_random_bots`, "
        "`guild_bank_money`, `character_inventory`); a phase loading into "
        f"{phase.into!r} applies them and leaves `tw_char` exactly as broken"
    )
    assert CHARACTER_UPDATES in (phase.files or ()), (
        "the fork's directory is `sql/character_updates/`; `sql/database_updates/character/` "
        "is the OTHER one, the one its own updater already reads. The phase globs "
        f"{list(phase.files or ())}"
    )
    keys = _native().cmangos.conf.files["mangosd.conf"].keys  # type: ignore[union-attr]
    updater = keys["Database.AutoUpdate.Path"].strip('"').rstrip("/")
    assert not updater.endswith("character_updates"), (
        "were the updater pointed here it would apply these itself; it is pointed at "
        f"{updater!r}, which is the whole reason this phase exists"
    )


def test_the_character_updates_phase_runs_after_the_phases_that_make_its_tables() -> None:
    """Order, read off the files' contents and not off their timestamps.

    Each of the three ALTERs or copies a table something EARLIER creates, and
    the two creators are two different phases (both read on `m910q`, 2026-09-09):

    * `20260708055500_ai_playerbot_random_bots_index.sql` indexes
      `ai_playerbot_random_bots`. `create_databases.sql` does not contain that
      table; `modules/mod-playerbots/sql/characters/ai_playerbot_random_bots.sql`
      does -- the `playerbots characters` phase.
    * `20260731160000_guild_bank_money_unsigned.sql` and
      `20260812142512_character_inventory_copy.sql` need `guild_bank_money` and
      `character_inventory`, both `CREATE TABLE`s inside `create_databases.sql`
      (its lines 1455 and 573) -- the `schemas` phase.

    So it belongs after BOTH, and after the one whose file is named in the
    evidence is not enough: placed after `schemas` alone, the index file fails
    on a table that does not exist yet and `on_error: fail` stops the install.
    """
    order = _phase_names()
    assert "character updates" in order, f"there is no such phase to order; the plan is {order}"
    for earlier in ("schemas", "playerbots characters"):
        assert earlier in order, f"the phase this order is measured against is gone: {earlier}"
        assert order.index("character updates") > order.index(earlier), (
            f"`character updates` runs before `{earlier}`, which creates a table it edits; "
            f"the order is {order}"
        )


def test_the_three_files_are_applied_in_name_order_after_the_bots_own_table(
    tmp_path: Path,
) -> None:
    """The order the import will really use, not the order the catalog was written in.

    `sqlplan.expand()` is the seam that turns this plan into an ordered list of
    runs, so the plan is expanded over a tree carrying the three real filenames
    (dropped in reverse, so a listing that did not sort would be caught) and the
    runs are read back. Two things are asserted about them and neither is
    visible in the JSON: that the three arrive in their timestamp order, and
    that the whole group arrives after the module file that creates
    `ai_playerbot_random_bots`.

    Every other directory the plan globs is created from the plan's own patterns
    rather than from a list written here, so a phase added later cannot make
    this test lie by failing its `fail` glob. The one file in this directory
    that is not `.sql` is there because a phase that streamed a README at the
    database would also pass an assertion about the three.
    """
    from yulon.catalog.families import sqlplan

    entry = load_catalog().get(TORTOISE)
    plan = _sql_plan()
    schemas = {
        name: name
        for name in (
            entry.databases.auth,
            entry.databases.characters,
            entry.databases.world,
            *entry.databases.extra,
        )
    }
    tokens = {
        "DB_USER": "mangos",
        "DB_PASSWORD": "not-a-real-password",
        "WORLD_DB": entry.databases.world,
        "CHAR_DB": entry.databases.characters,
        "AUTH_DB": entry.databases.auth,
        "LOGS_DB": entry.databases.extra[0],
        "REALM_HOST": "127.0.0.1",
        "WORLD_PORT": "8085",
        "CLIENT_BUILD": str(entry.client.build),
    }
    for phase in plan.phases:
        for pattern in phase.files or ():
            if pattern == CHARACTER_UPDATES:
                continue
            directory = tmp_path / PurePosixPath(pattern).parent
            directory.mkdir(parents=True, exist_ok=True)
            (directory / PurePosixPath(pattern).name.replace("*", "aaa")).write_text("--\n")
    bots = tmp_path / "src/tortoise-wow/modules/mod-playerbots/sql/characters"
    bots.mkdir(parents=True, exist_ok=True)
    (bots / "ai_playerbot_random_bots.sql").write_text("--\n")
    updates = tmp_path / PurePosixPath(CHARACTER_UPDATES).parent
    updates.mkdir(parents=True, exist_ok=True)
    for name in reversed(THE_THREE_FILES):
        (updates / name).write_text("--\n")
    (updates / "README.md").write_text("not SQL\n")

    runs = sqlplan.expand(plan, tmp_path, schemas, tokens)
    applied = [run.rel for run in runs]
    mine = [
        run for run in runs if run.path is not None and run.path.parent.name == "character_updates"
    ]
    assert [run.path.name for run in mine] == list(THE_THREE_FILES), (  # type: ignore[union-attr]
        "the fork names these by timestamp, so name order IS the order it wrote them in; "
        f"the import would apply {[run.rel for run in mine]}"
    )
    assert {run.schema for run in mine} == {
        entry.databases.characters
    }, f"the files are streamed into {[run.schema for run in mine]}, not the characters db"
    creator = "modules/mod-playerbots/sql/characters/ai_playerbot_random_bots.sql"
    made = [index for index, rel in enumerate(applied) if rel.endswith(creator)]
    assert made, f"the module file that creates the bots' table was not applied at all: {applied}"
    assert applied.index(mine[0].rel) > made[0], (
        "the first character update indexes `ai_playerbot_random_bots`, and the import "
        f"reaches it before the file that creates that table; the order is {applied}"
    )


def test_the_phase_records_why_this_directory_is_the_apps_job() -> None:
    """The per-tree fact travels with the phase, because the next reader will ask.

    A phase that applies a directory the tree's own updater ignores looks
    redundant next to `Database.AutoUpdate.Path`, and a reader who deletes it
    gets a world that boots for a week. Both dates are here because they are two
    different claims -- a fresh install and an established one -- and the
    weekly one is the one that is easy to lose.
    """
    phases = {phase.name: phase for phase in _sql_plan().phases}
    assert "character updates" in phases, f"there is no such phase; the plan is {_phase_names()}"
    notes = " ".join(phases["character updates"].notes)
    assert notes, "the phase carries no note saying why the app applies what the updater will not"
    for fact in ("character_updates", "2026-09-08", "2026-09-09"):
        assert fact in notes, f"the phase's notes do not mention {fact!r}: {notes!r}"
