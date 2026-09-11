"""T26 live half -- the app's own "Add this character" route, pressed for real.

Runs ON `yulon-ubuntu2` against `~/wowserver`, out of a copy of the T26
worktree at `~/t26-live`, so every press is THIS ticket's code and not the box
clone's older tree. `build()` is `t16press.py:build()`, which is T18's, which is
T13's, which is 8.6's own `gate86b.build()` -- the objects
`controller_view._for_wotlk` assembles for the running app, from the same
inputs, with `link_writer` wired the way `controller_view.py:1053` wires it.

Nothing here spells a command of its own for the four claims: `candidates`,
`addnamed`, `linkplan` and `linkaccount` call `party.InstallParty`'s own
methods and print what they answer. `send` and `sql` exist for the ground and
the read-backs around them.

    liveness                     container status, one line
    ground [name...]             facts, preconditions, group rows per name
    sql <statement>              one read against acore_characters
    sqldb <db> <statement>       one read against a named schema of this install
    send <command...>            one command over the seam's own channel
    deploy                       "Enable My Party"'s own deploy seam
    stop | start                 the Server tab's own two buttons
    members <master>             InstallParty.members(master) AND party_members(master)
    candidates <master>          InstallParty.candidates(master) -- THE PICKER
    addnamed <master> <name>     InstallParty.add_named(...) -- THE PRESS
    linkplan <master> <account>  InstallParty.link_plan(...) -- the first press
    linkaccount <master> <acct>  InstallParty.link_account(...) -- the write
    dismiss <master> <bot>       InstallParty.remove -- the app's own uninvite
    mkaccount <name> [gmlevel]   the Accounts tab's own create_account; the
                                 password comes from $T26_PW and is never in
                                 argv, never printed and never logged
    accountid <name>             one account's id, for the ledger's sake

Every stamp printed here is read from the clock in this process at the moment
the line is written (memory note `stamps-come-from-the-clock`).
"""

from __future__ import annotations

import os
import sys
from datetime import datetime
from pathlib import Path

TREE = Path(__file__).resolve().parents[3]  # .../t26-live
sys.path.insert(0, str(TREE / "pylauncher"))

from yulon import channel_setup, docker, party, resources  # noqa: E402
from yulon.catalog import composegen  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.controller_wow_wotlk import accounts as wotlk_accounts  # noqa: E402
from yulon.ui import controller_view  # noqa: E402

SERVER = Path.home() / "wowserver"
ENTRY = load_catalog().get("wow-wotlk")
CHARS = "acore_characters"


def stamp() -> str:
    return datetime.now().strftime("%H:%M:%S.%f")[:-3]


def build() -> tuple[party.InstallParty, object, object]:
    """The objects `_for_wotlk` assembles, from the same inputs (T16's `build`)."""
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
    return seam, sql, channel.live_channel()


def liveness() -> str:
    spec = ENTRY.container_spec()
    state = docker.container_state(spec.world)
    return (
        f"worldserver {state.status or 'unknown'} "
        f"started={state.started_at} restarts={state.restart_count}"
    )


def _rows(sql: object, statement: str, db: str = "characters") -> str:
    return sql.query(db, statement).strip()  # type: ignore[attr-defined]


def show_picker(picker: party.Picker, *, limit: int = 20, watch: str = "") -> None:
    print(f"    problem : {picker.problem!r}")
    print(f"    added   : {picker.added}   max_added: {picker.max_added}")
    print(f"    note    : {picker.note}")
    print(f"    rows    : {len(picker.rows)}  (offered {sum(1 for r in picker.rows if r.allowed)})")
    shown = 0
    for row in picker.rows:
        if row.allowed:
            print(f"      OFFER  {row.name:<14} level {row.level:<3} {row.account_or_guild:<30} {row.allowed_by}")
            shown += 1
        if shown >= limit:
            print(f"      ... and more (only the first {limit} offered rows are printed)")
            break
    refused = [row for row in picker.rows if not row.allowed]
    print(f"    refused : {len(refused)}")
    for row in refused[:limit]:
        print(f"      GREY   {row.name:<14} level {row.level:<3} {row.account_or_guild:<30} {row.refused_because}")
    if watch:
        print(f"    the rows this lane is about ({watch}*), every one of them:")
        seen = False
        for row in picker.rows:
            if row.name.startswith(watch):
                seen = True
                mark = "OFFER " if row.allowed else "GREY  "
                why = row.allowed_by or row.refused_because
                print(f"      {mark} {row.name:<14} level {row.level:<3} {row.account_or_guild:<30} {why}")
        if not seen:
            print(f"      (no row whose name begins with {watch})")


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
        spec = ENTRY.container_spec()
        if verb == "stop":
            print(f"stop_staged -> {docker.stop_staged(spec, SERVER)}")
        else:
            print(f"start_staged -> {docker.start_staged(spec, SERVER)}")
        print(liveness())
        return 0

    if verb == "deploy":
        root = party.lua_root()
        dest = party.dest_dir(SERVER)
        print(f"lua_root  = {root}")
        print(f"dest_dir  = {dest}")
        result = party.deploy(root, dest)
        print(f"deploy -> changed={result.changed} names={result.names}")
        return 0

    seam, sql, channel = build()
    print(liveness())

    if verb == "mkaccount":
        password = os.environ.get("T26_PW", "")
        if not password:
            print("no $T26_PW in the environment; nothing was written")
            return 2
        level = int(args[2]) if len(args) > 2 else 0
        result = wotlk_accounts.create_account(
            sql,
            args[1],
            password,
            gm_level=level,
            scheme=ENTRY.accounts.scheme or "azerothcore",
        )
        print(
            f"{stamp()}  create_account({args[1]!r}) -> created={result.created} "
            f"gm_level={result.gm_level}"
        )
        return 0

    if verb == "accountid":
        print(_rows(sql, f"SELECT id, username FROM acore_auth.account WHERE username = '{args[1]}';", "auth"))
        return 0

    if verb == "sql":
        print(_rows(sql, args[1]))
        return 0

    if verb == "sqldb":
        print(_rows(sql, args[2], args[1]))
        return 0

    if verb == "members":
        # BOTH reads, because the difference between them is the defect this
        # lane found: `members()` keeps only rows the bot marker recognises and
        # `party_members()` keeps everyone but the master.
        print(f"InstallParty.members({args[1]!r})       -> {seam.members(args[1])}")
        print(f"InstallParty.party_members({args[1]!r}) -> {seam.party_members(args[1])}")
        return 0

    if verb == "ground":
        facts = seam.facts()
        print(f"facts: {facts}")
        print(f"ready: {party.ready(facts)}  blocker: {party.blocker(facts)}")
        for name in args[1:]:
            state = seam.state(name)
            for check in state.checks:
                print(f"    {'ok ' if check.met else 'NO '}{check.name}")
            print(f"{name}: online guid = {seam.online_guid(name)}")
            print(f"{name}: seam.members -> {seam.members(name)}")
        return 0

    if verb == "candidates":
        print(f"{stamp()}  InstallParty.candidates({args[1]!r})")
        show_picker(seam.candidates(args[1]), watch=args[2] if len(args) > 2 else "")
        return 0

    if verb == "addnamed":
        print(f"{stamp()}  InstallParty.add_named({args[1]!r}, {args[2]!r})")
        result = seam.add_named(args[1], args[2])
        print(
            f"{stamp()}  added={result.added} joined={result.joined} "
            f"name={result.name!r} unreadable={result.unreadable}"
        )
        print(f"    blocker : {result.blocker!r}")
        print(f"    sentence: {result.sentence}")
        return 0

    if verb in ("linkplan", "linkaccount"):
        method = seam.link_plan if verb == "linkplan" else seam.link_account
        print(f"{stamp()}  InstallParty.{verb.replace('link', 'link_')}({args[1]!r}, {args[2]!r})")
        result = method(args[1], args[2])
        print(
            f"{stamp()}  linked={result.linked} master_account={result.master_account!r} "
            f"other_account={result.other_account!r}"
        )
        print(f"    blocker : {result.blocker!r}")
        print(f"    sentence: {result.sentence}")
        return 0

    if verb == "dismiss":
        result = seam.remove(args[1], args[2])
        print(
            f"{stamp()}  InstallParty.remove({args[1]!r}, {args[2]!r}) -> "
            f"removed={result.removed} logged_out={result.logged_out}"
        )
        print(f"    sentence: {result.sentence}")
        return 0

    if channel is None:
        print("no channel")
        return 1

    if verb == "send":
        command = " ".join(args[1:])
        print(f"COMMAND: {command}")
        answer = channel.send(command)
        print(f"outcome={answer.outcome} reason={answer.reason!r}")
        print(f"text={answer.text!r}")
        return 0

    print(f"unknown: {verb}")
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
