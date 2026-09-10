"""T13 live half -- the `dml_uninvite` contract, pressed against a live world.

Runs ON `yulon-ubuntu2` against `~/wowserver`, out of a copy of the T13
worktree at `~/t13-live` (so the Lua and the seam under test are THIS
ticket's, not the box clone's older tree).

`build()` is `8.6-wotlk-yulon-ubuntu2-2026-09-09/gate86b.py`'s own `build()`,
copied verbatim except for the one line that says where the tree is: that file
hard-codes `~/dads-mmo-lab/pylauncher`, and this ticket's code is not in the
box's clone. Everything it assembles -- `DockerSql`,
`channel_setup.InstallChannel.live_channel`, `party.InstallParty` -- is what
`controller_view._for_wotlk` assembles for the running app. Nothing here
writes a command string of its own: `deploy` calls `party.deploy`, `uninvite`
calls `party.uninvite_command`, and `send` hands whatever it is given to the
same `SoapChannel` the panel's seam uses.

    ground <master>...        liveness, facts, preconditions, and the group
                              table rows for each name given
    deploy                    party.deploy(party.lua_root(), dest_dir) -- the
                              app's own deploy seam, onto this install
    sql <statement>           one read against acore_characters
    send <command...>         one command over the seam's own channel
    uninvite <player> <bot>   party.uninvite_command(...) over that channel
    group <master>            seam.members(master), the seam's own group read
    remove <master> <bot>     the whole seam: InstallParty.remove -> dismiss()
    grouprows                 every group_member row on the server, raw
"""

from __future__ import annotations

import sys
from pathlib import Path

TREE = Path(__file__).resolve().parents[3]  # .../t13-live
sys.path.insert(0, str(TREE / "pylauncher"))

from yulon import channel_setup, dbreads, docker, party, resources, useraccounts  # noqa: E402
from yulon.catalog import composegen  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.controller_wow_wotlk import accounts as wotlk_accounts  # noqa: E402
from yulon.ui import controller_view  # noqa: E402

SERVER = Path.home() / "wowserver"
ENTRY = load_catalog().get("wow-wotlk")


def build() -> tuple[party.InstallParty, useraccounts.InstallAccounts, object]:
    """The same three objects `_for_wotlk` assembles, from the same inputs."""
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
    )
    accounts = useraccounts.InstallAccounts(
        ENTRY,
        SERVER,
        sql=sql,
        channel_for_saved=channel.live_channel,
        app_account=channel_setup.account_name(install_id),
    )
    return seam, accounts, sql


def liveness() -> str:
    spec = ENTRY.container_spec()
    state = docker.container_state(spec.world)
    return (
        f"worldserver {state.status or 'unknown'} "
        f"started={state.started_at} restarts={state.restart_count}"
    )


CHARS = "acore_characters"


def _rows(sql: object, statement: str) -> str:
    return sql.query("characters", statement).strip()  # type: ignore[attr-defined]


def main() -> int:
    args = sys.argv[1:]
    if not args:
        print(__doc__)
        return 2
    verb = args[0]

    if verb == "deploy":
        # The app's own deploy seam, on the app's own source root.
        root = party.lua_root()
        dest = party.dest_dir(SERVER)
        print(f"lua_root  = {root}")
        print(f"dest_dir  = {dest}")
        result = party.deploy(root, dest)
        print(f"deploy -> changed={result.changed} names={result.names}")
        return 0

    seam, _accounts, sql = build()
    print(liveness())

    if verb == "ground":
        facts = seam.facts()
        print(f"facts: {facts}")
        print(f"ready: {party.ready(facts)}  blocker: {party.blocker(facts)}")
        marker = dbreads.resolve_marker(ENTRY, SERVER)
        print(f"marker: {marker.marker} {marker.problem}")
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

    if verb == "grouprows":
        print(
            _rows(
                sql,
                "SELECT gm.guid, gm.memberGuid, c.name, gm.memberFlags, gm.subgroup, gm.roles "
                f"FROM {CHARS}.group_member gm JOIN {CHARS}.characters c "
                "ON c.guid = gm.memberGuid ORDER BY gm.guid, gm.memberGuid;",
            )
        )
        return 0

    if verb == "remove":
        # The whole Python half of the seam, not just the wire: dismiss() sends
        # the uninvite, reads the reply, and (only on a success) whispers the
        # logout and polls the group table back.
        master, bot = args[1], args[2]
        print(f"BEFORE: {seam.members(master)}")
        result = seam.remove(master, bot)
        print(f"RESULT: {result}")
        print(f"AFTER : {seam.members(master)}")
        return 0

    if verb == "group":
        print(f"seam.members({args[1]!r}) -> {seam.members(args[1])}")
        return 0

    if verb in ("send", "uninvite"):
        channel = seam._channel_for_saved()  # noqa: SLF001 - inside the seam's own house
        if channel is None:
            print("no channel")
            return 1
        if verb == "uninvite":
            command = party.uninvite_command(args[1], args[2])
        else:
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
