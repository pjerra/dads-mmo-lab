"""T16 live half -- when a chosen level holds on a bot that has just joined.

Runs ON `yulon-ubuntu2` against `~/wowserver`, out of a copy of the T16
worktree at `~/t16-live`, so every press is THIS ticket's code and not the
box clone's older tree. `build()` is T18's `t18press.py:build()`, which is
T13's, which is 8.6's own `gate86b.build()` -- the objects
`controller_view._for_wotlk` assembles for the running app, from the same
inputs.

Nothing here writes a level command of its own on the `seam` route: `setlevel`
and the `seam` arm of `trial` call `play.InstallPlay.set_level`, which is the
Characters tab's own seam and the one `party.add_bot` is handed. The `hand`
arm sends `character level <bot> <n>` over the same channel with no app
function in the way, so the two can be compared.

    liveness                    container status, one line
    ground [name...]            facts, preconditions, group rows per name
    sql <statement>             one read against a schema of this install
    send <command...>           one command over the seam's own channel
    pinfo <name>                the LIVE level, out of the world's own `.pinfo`
    dblevel <name>              `characters.level` for one name
    setlevel <name> <level>     play.InstallPlay.set_level -- the app's seam
    addclass <master> <class>   party.add_command over that channel
    members <master>            InstallParty.members(master)
    addpress <master> <class> <level> [spec]
                                InstallParty.add(...) -- the panel's own Add
    dismiss <master> <bot>      InstallParty.remove -- the app's own uninvite
    trial <master> <bot> <delay> <level> <hand|seam> [tail]
                                THE MEASUREMENT: stage a join, wait for the
                                group row, send the level `delay` seconds after
                                it, and sample the LIVE level and
                                characters.level on a stamped clock either side
                                of it, through one `saveall` and out the far
                                end.

Every stamp printed here is read from the clock in this process at the moment
the line is written (memory note `stamps-come-from-the-clock`).
"""

from __future__ import annotations

import re
import sys
import time
from datetime import datetime
from pathlib import Path

TREE = Path(__file__).resolve().parents[3]  # .../t16-live
sys.path.insert(0, str(TREE / "pylauncher"))

from yulon import channel_setup, docker, party, play, resources, useraccounts  # noqa: E402
from yulon.catalog import composegen  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.controller_wow_wotlk import accounts as wotlk_accounts  # noqa: E402
from yulon.ui import controller_view  # noqa: E402

SERVER = Path.home() / "wowserver"
ENTRY = load_catalog().get("wow-wotlk")
CHARS = "acore_characters"

_LEVEL_IN_PINFO = re.compile(r"(?<![A-Za-z])Level:\s*(\d{1,3})")
"""`.pinfo` prints `GMLevel:` before it prints `Level:`, and a pattern that
matched the first of those would report every character as level 0 -- measured
on the box, 2026-09-10, on `Jurnaar` (guid 1, level 80, GMLevel 0)."""


def stamp() -> str:
    return datetime.now().strftime("%H:%M:%S.%f")[:-3]


def build() -> tuple[party.InstallParty, play.InstallPlay, object, object]:
    """The objects `_for_wotlk` assembles, from the same inputs (T18's `build`)."""
    spec = ENTRY.container_spec()
    password = controller_view._db_password(ENTRY, SERVER)  # noqa: SLF001
    sql = controller_view._sql_for(ENTRY, password, wsl_distro=None)  # noqa: SLF001
    install_id = composegen.install_id(SERVER)
    channel = channel_setup.InstallChannel(
        ENTRY,
        SERVER,
        templates_root=resources.installers_dir(),
        install_id=install_id,
        create=lambda name, pw, level: wotlk_accounts.create_account(
            sql, name, pw, gm_level=level, scheme=ENTRY.accounts.scheme or "azerothcore"
        ),
        reset=lambda name, pw: wotlk_accounts.reset_own_password(sql, name, pw),
        channel_for=lambda endpoint: __import__(
            "yulon.channel", fromlist=["SoapChannel"]
        ).SoapChannel(
            endpoint=endpoint,
            state_of=lambda: docker.container_state(spec.world),
        ),
    )
    seam = party.InstallParty(
        ENTRY,
        SERVER,
        sql=sql,
        channel_for_saved=channel.live_channel,
        container=spec.world,
        world_running=lambda: docker.container_state(spec.world).settled,
        link_writer=sql,
    )
    plays = play.InstallPlay(ENTRY, SERVER, sql=sql, channel_for_saved=channel.live_channel)
    return seam, plays, sql, channel.live_channel()


def liveness() -> str:
    spec = ENTRY.container_spec()
    state = docker.container_state(spec.world)
    return (
        f"worldserver {state.status or 'unknown'} "
        f"started={state.started_at} restarts={state.restart_count}"
    )


def _rows(sql: object, statement: str) -> str:
    return sql.query("characters", statement).strip()  # type: ignore[attr-defined]


def db_level(sql: object, name: str) -> int | None:
    """`characters.level` for one name -- the column `Member.level` IS."""
    raw = _rows(sql, f"SELECT level FROM {CHARS}.characters WHERE name = '{name}' LIMIT 1;")
    return int(raw) if raw.strip().isdigit() else None


def live_level(channel: object, name: str) -> tuple[int | None, str]:
    """The level the WORLD is holding, out of its own `.pinfo`.

    The database row of an ONLINE character is whatever its last save wrote, so
    a reading of `characters.level` alone cannot tell "the module put the level
    back" apart from "the row has not been written yet". `.pinfo` answers off
    the live `Player` object, so the pair of readings tells those two apart.
    """
    answer = channel.send(f"pinfo {name}")  # type: ignore[attr-defined]
    text = (answer.text or "").replace("\n", " | ")
    match = _LEVEL_IN_PINFO.search(answer.text or "")
    return (int(match.group(1)) if match else None, text)


def _sample(sql: object, channel: object, name: str, t0: float, note: str = "") -> tuple[int | None, int | None]:
    live, _text = live_level(channel, name)
    row = db_level(sql, name)
    print(
        f"    {stamp()}  t0+{time.monotonic() - t0:6.1f}s  live={live!s:>4}  "
        f"characters.level={row!s:>4}  {note}"
    )
    # A sample costs two round trips (~0.7 s), and this keeps the cadence near
    # one second without pretending the samples are evenly spaced: every line
    # carries its own elapsed time, read from the clock.
    time.sleep(0.3)
    return live, row


def main() -> int:  # noqa: C901, PLR0911, PLR0912, PLR0915 - one verb per branch
    args = sys.argv[1:]
    if not args:
        print(__doc__)
        return 2
    verb = args[0]

    if verb == "liveness":
        print(liveness())
        return 0

    if verb in ("stop", "start"):
        # The Server tab's own two buttons: `docker.stop_staged` and
        # `docker.start_staged` are what `controller_view` calls for them.
        spec = ENTRY.container_spec()
        if verb == "stop":
            print(f"stop_staged -> {docker.stop_staged(spec, SERVER)}")
        else:
            print(f"start_staged -> {docker.start_staged(spec, SERVER)}")
        print(liveness())
        return 0

    if verb == "deploy":
        # "Enable My Party"'s own deploy seam.
        root = party.lua_root()
        dest = party.dest_dir(SERVER)
        print(f"lua_root  = {root}")
        print(f"dest_dir  = {dest}")
        result = party.deploy(root, dest)
        print(f"deploy -> changed={result.changed} names={result.names}")
        return 0

    seam, plays, sql, channel = build()
    print(liveness())

    if verb == "ground":
        facts = seam.facts()
        print(f"facts: {facts}")
        print(f"ready: {party.ready(facts)}  blocker: {party.blocker(facts)}")
        state = seam.state(args[1]) if len(args) > 1 else None
        if state is not None:
            for check in state.checks:
                print(f"    {'ok ' if check.met else 'NO '}{check.name}")
        for name in args[1:]:
            print(f"{name}: online guid = {seam.online_guid(name)}")
            print(f"{name}: seam.members -> {seam.members(name)}")
        return 0

    if verb == "sql":
        print(_rows(sql, args[1]))
        return 0

    if verb == "members":
        print(seam.members(args[1]))
        return 0

    if verb == "dismiss":
        # The app's own dismissal -- `dml_uninvite` then the logout whisper,
        # with the group table read back. T13's contract, not a hand-rolled one.
        result = seam.remove(args[1], args[2])
        print(
            f"{stamp()}  InstallParty.remove({args[1]!r}, {args[2]!r}) -> "
            f"removed={result.removed} logged_out={result.logged_out}"
        )
        print(f"    sentence: {result.sentence}")
        return 0

    if verb == "dblevel":
        print(f"characters.level {args[1]} = {db_level(sql, args[1])}")
        return 0

    if verb == "setlevel":
        outcome = plays.set_level(args[1], int(args[2]))
        print(f"play.InstallPlay.set_level({args[1]!r}, {args[2]}) -> {outcome}")
        return 0

    if verb == "addpress":
        level = int(args[3]) if len(args) > 3 and args[3] != "-" else None
        spec = args[4] if len(args) > 4 else ""
        print(f"{stamp()}  InstallParty.add({args[1]!r}, {args[2]!r}, level={level}, spec={spec!r})")
        result = seam.add(args[1], args[2], level=level, spec=spec)
        print(f"{stamp()}  added={result.added} joined={result.joined} bot={result.bot}")
        print(f"    geared={result.geared} specced={result.specced}")
        print(f"    level={result.level} before={result.level_before} after={result.level_after}")
        print(f"    sentence: {result.sentence}")
        return 0

    if channel is None:
        print("no channel")
        return 1

    if verb == "pinfo":
        level, text = live_level(channel, args[1])
        print(f"pinfo {args[1]}: level={level}")
        print(f"    raw: {text}")
        return 0

    if verb == "addclass":
        command = party.add_command(args[1], args[2])
        print(f"COMMAND: {command}")
        answer = channel.send(command)
        print(f"outcome={answer.outcome} reason={answer.reason!r} text={answer.text!r}")
        return 0

    if verb == "send":
        command = " ".join(args[1:])
        print(f"COMMAND: {command}")
        answer = channel.send(command)
        print(f"outcome={answer.outcome} reason={answer.reason!r}")
        print(f"text={answer.text!r}")
        return 0

    if verb == "levelpress":
        return levelpress(
            seam,
            plays,
            sql,
            channel,
            master=args[1],
            bot=args[2],
            level=int(args[3]),
        )

    if verb == "trial":
        return trial(
            seam,
            plays,
            sql,
            channel,
            master=args[1],
            bot=args[2],
            delay=float(args[3]),
            level=int(args[4]),
            how=args[5],
            tail=float(args[6]) if len(args) > 6 else 90.0,
        )

    print(f"unknown: {verb}")
    return 2


def trial(  # noqa: C901, PLR0913, PLR0915
    seam: party.InstallParty,
    plays: play.InstallPlay,
    sql: object,
    channel: object,
    *,
    master: str,
    bot: str,
    delay: float,
    level: int,
    how: str,
    tail: float,
) -> int:
    """One row of the timing table.

    A FRESH join each time, and a different bot each time: the question is what
    the module does to a character in the seconds after it joins a master's
    party, so a bot that is already in the party would measure nothing.

    The join is a real core group INVITE through this lane's harness, not the
    panel's own Add: `dml_addclass` cannot run on this box at all, because the
    master would have to be a non-bot session and there is none (see
    `t16_stage.lua`'s header for the module's own guard and the two presses
    that met it).

    Two numbers are read on the same clock at every sample: the LIVE level out
    of the world's own `.pinfo`, and `characters.level`. `.character level` on
    an ONLINE character calls `GiveLevel` and writes no row
    (AzerothCore `src/server/scripts/Commands/cs_character.cpp:252-281` on this
    box), so a reading of the row alone cannot tell "the module put the level
    back" apart from "the row has not been written yet".
    """
    candidates = [name for name in bot.split(",") if name]
    print(
        f"== TRIAL  master={master} candidates={candidates} delay=+{delay:g}s "
        f"level={level} how={how}"
    )
    before_rows = seam.members(master)
    print(f"{stamp()}  party before: {before_rows}")

    # The invite is RE-SENT, and a candidate that will not come is dropped for
    # the next one. Two reasons, both the module's:
    #
    #   * `PlayerbotSecurity::LevelFor` returns PLAYERBOT_SECURITY_TALK -- below
    #     PLAYERBOT_SECURITY_INVITE -- when `bot->GetLevel() - from->GetLevel()
    #     > 5` and the two are not guild-mates (`GroupInvitationPermission = 1`
    #     on this install), and `AcceptInvitationAction::Execute` then DECLINES
    #     in silence. That is what refused every invite while this lane's own
    #     defect had left the master at level 42.
    #   * even an allowed invite is answered only on the bot's own next AI check,
    #     and a core invite expires.
    #
    # Both are staging, not measurement: t0 is the moment the group ROW appears,
    # whichever invite to whichever candidate made it.
    joined = None
    bot = ""
    for name in candidates:
        print(
            f"{stamp()}  candidate {name}: live={live_level(channel, name)[0]} "
            f"characters.level={db_level(sql, name)}"
        )
        command = f"dml_t16_stage invite {master} {name}"
        for attempt in range(3):
            print(f"{stamp()}  COMMAND ({attempt + 1}): {command}")
            answer = channel.send(command)  # type: ignore[attr-defined]
            print(f"{stamp()}  outcome={answer.outcome} text={answer.text!r}")
            deadline = time.monotonic() + 12
            while time.monotonic() < deadline:
                rows = seam.members(master)
                if not isinstance(rows, str):
                    # BY NAME, and not "the row that was not there before". The
                    # master on this box is itself a playerbot, so `members()`
                    # returns HIM too -- `group_rows_sql`'s clause is a BOT
                    # clause, and in the app the master is a real player it
                    # filters out. The first version of this driver took the
                    # first new row and levelled the MASTER in one trial; that
                    # trial was thrown away and this is the fix.
                    joined = next((m for m in rows if m.name == name), None)
                    if joined is not None:
                        break
                time.sleep(0.2)
            if joined is not None:
                break
        if joined is not None:
            bot = name
            break
    if joined is None:
        print(f"{stamp()}  NO GROUP ROW from any of {candidates} -- nothing measured")
        return 1

    t0 = time.monotonic()
    print(
        f"{stamp()}  GROUP ROW APPEARED: {joined.name} guid={joined.guid} "
        f"characters.level={joined.level}  <- t0"
    )

    # From t0 to the send, so what the module does on its own is on the same
    # clock as what the press does.
    while time.monotonic() - t0 < delay - 0.9:
        _sample(sql, channel, joined.name, t0, "(before the send)")
    while time.monotonic() - t0 < delay:
        time.sleep(0.02)

    live_before, row_before = _sample(sql, channel, joined.name, t0, "<- the ground, at the send")
    sent = stamp()
    if how == "seam":
        outcome = plays.set_level(joined.name, level)
        print(f"{sent}  t0+{time.monotonic() - t0:6.1f}s  SEND via play.set_level -> {outcome}")
    else:
        line = f"character level {joined.name} {level}"
        reply = channel.send(line)  # type: ignore[attr-defined]
        print(
            f"{sent}  t0+{time.monotonic() - t0:6.1f}s  SEND by hand {line!r} -> "
            f"outcome={reply.outcome} text={reply.text!r}"
        )
    t_send = time.monotonic()

    live10 = row10 = None
    saved_at: float | None = None
    agrees_at: float | None = None
    live_s = row_s = None
    end = t_send + tail
    while time.monotonic() < end:
        note = ""
        elapsed = time.monotonic() - t_send
        if live10 is None and elapsed >= 10:
            note = "<- THE READING, 10 s after the send"
        live, row = _sample(sql, channel, joined.name, t0, note)
        if note:
            live10, row10 = live, row
            # The one thing measured to write an online character's row: the
            # server's own `saveall` (`cs_misc.cpp:1402` -> ObjectAccessor.cpp:286,
            # which walks every Player in the world, bots included).
            save = channel.send("saveall")  # type: ignore[attr-defined]
            saved_at = time.monotonic()
            print(f"    {stamp()}  saveall -> outcome={save.outcome} text={save.text!r}")
        if saved_at is not None and live_s is None and time.monotonic() - saved_at >= 3:
            live_s, row_s = _sample(
                sql, channel, joined.name, t0, "<- THE READING, 3 s after .saveall"
            )
        if agrees_at is None and row == level:
            agrees_at = time.monotonic()
            print(
                f"    {stamp()}  characters.level AGREES at t_send+"
                f"{agrees_at - t_send:.1f}s"
                + (f" (saveall+{agrees_at - saved_at:.1f}s)" if saved_at else "")
            )
            if live10 is not None and live_s is not None:
                break

    live_e, row_e = _sample(sql, channel, joined.name, t0, "<- the end of the trial")
    print(
        f"RESULT delay=+{delay:g}s how={how} bot={joined.name} guid={joined.guid} "
        f"level_at_join={joined.level} asked={level} "
        f"live_at_send={live_before} row_at_send={row_before} "
        f"live@send+10={live10} row@send+10={row10} "
        f"live@save+3={live_s} row@save+3={row_s} "
        f"row_agrees_after={None if agrees_at is None else round(agrees_at - t_send, 1)} "
        f"live_at_end={live_e} row_at_end={row_e}"
    )
    return 0


def _stage_join(
    seam: party.InstallParty, sql: object, channel: object, master: str, candidates: list[str]
) -> party.Member | None:
    """A real core group invite, retried, until one candidate's ROW appears."""
    for name in candidates:
        print(
            f"{stamp()}  candidate {name}: live={live_level(channel, name)[0]} "
            f"characters.level={db_level(sql, name)}"
        )
        for attempt in range(3):
            command = f"dml_t16_stage invite {master} {name}"
            print(f"{stamp()}  COMMAND ({attempt + 1}): {command}")
            answer = channel.send(command)  # type: ignore[attr-defined]
            print(f"{stamp()}  outcome={answer.outcome} text={answer.text!r}")
            deadline = time.monotonic() + 12
            while time.monotonic() < deadline:
                rows = seam.members(master)
                if not isinstance(rows, str):
                    joined = next((m for m in rows if m.name == name), None)
                    if joined is not None:
                        return joined
                time.sleep(0.2)
    return None


def levelpress(
    seam: party.InstallParty,
    plays: play.InstallPlay,
    sql: object,
    channel: object,
    *,
    master: str,
    bot: str,
    level: int,
) -> int:
    """THE SECOND PRESS: `party._level_step`, the code half, against this server.

    Not a re-implementation of it and not `add_bot` around it: the same function
    the panel's Add now calls, handed the app's own `play.InstallPlay.set_level`,
    the app's own channel and the app's own group read. `add_bot` itself cannot
    be pressed on this box -- its first act is `dml_addclass`, which needs a
    non-bot master (step 02) -- so the press is the step under it, exactly as
    T18 pressed `party.spec_command` where the panel's Add was out of reach.
    """
    candidates = [name for name in bot.split(",") if name]
    print(f"== SECOND PRESS  master={master} candidates={candidates} level={level}")
    joined = _stage_join(seam, sql, channel, master, candidates)
    if joined is None:
        print(f"{stamp()}  NO GROUP ROW from any of {candidates} -- nothing pressed")
        return 1
    print(
        f"{stamp()}  GROUP ROW APPEARED: {joined.name} guid={joined.guid} "
        f"characters.level={joined.level}"
    )
    before = joined.level
    live0, _ = live_level(channel, joined.name)
    print(f"{stamp()}  before the press: live={live0} characters.level={db_level(sql, joined.name)}")

    started = time.monotonic()
    step = party._level_step(  # noqa: SLF001 - the point of the press
        bot=joined.name,
        guid=joined.guid,
        level=level,
        set_level=plays.set_level,
        send=channel.send,  # type: ignore[attr-defined]
        members=lambda: party._rows_only(seam.members(master)),  # noqa: SLF001
        tries=party.LEVEL_TRIES,
        pause=party.LEVEL_SLEEP,
        sleep=time.sleep,
    )
    took = time.monotonic() - started
    print(f"{stamp()}  party._level_step -> {step}   (took {took:.1f}s)")
    print(f"{stamp()}  the panel's own sentence: {party._level_note(level, before, step.after, step.problem, resent=step.resent)!r}")  # noqa: SLF001,E501

    live1, _ = live_level(channel, joined.name)
    print(f"{stamp()}  10 s later:")
    time.sleep(10)
    live2, _ = live_level(channel, joined.name)
    row2 = db_level(sql, joined.name)
    print(f"{stamp()}  live={live2} characters.level={row2}")
    save = channel.send("saveall")  # type: ignore[attr-defined]
    print(f"{stamp()}  saveall -> {save.outcome} {save.text!r}")
    time.sleep(10)
    live3, _ = live_level(channel, joined.name)
    row3 = db_level(sql, joined.name)
    print(f"{stamp()}  after .saveall: live={live3} characters.level={row3}")
    print(
        f"PRESS bot={joined.name} guid={joined.guid} before={before} asked={level} "
        f"step.after={step.after} step.resent={step.resent} step.saved={step.saved} "
        f"took={took:.1f}s live_right_after={live1} live@10s={live2} row@10s={row2} "
        f"live_after_saveall={live3} row_after_saveall={row3}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
