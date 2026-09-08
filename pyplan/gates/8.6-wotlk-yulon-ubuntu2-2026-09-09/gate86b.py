"""8.6 part 2 -- My Party pressed through the tab's seam, on a live WotLK server.

Runs ON `yulon-ubuntu2`, against `~/wowserver`. Every press goes through
`yulon.party.InstallParty`, which is the object `ControllerServices.my_party`
holds -- built here exactly as `controller_view._for_wotlk` builds it, from the
same `DockerSql`, the same `channel_setup.InstallChannel.live_channel` and the
same `docker.container_state`. Nothing here reimplements a command string.

    ground <master>      read everything BEFORE any press and print it
    state <master>       the seam's own PartyState
    add <master> <class> add one bot, through the seam
    dismiss <m> <bot>    remove it, through the seam
    password <acct> <pw> set an account's password through 8.3a's seam

Each subcommand prints the worldserver's liveness first, so a reading taken
against a server that had died is not mistaken for a reading about My Party.
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path.home() / "dads-mmo-lab" / "pylauncher"))

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
    password = controller_view._db_password(ENTRY, SERVER)
    sql = controller_view._sql_for(ENTRY, password, wsl_distro=None)
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


def show_checks(state: party.PartyState) -> None:
    for check in state.checks:
        mark = "ok " if check.met else "NO "
        print(f"    {mark}{check.name}")
        if not check.met:
            print(f"        {check.sentence}")


def main() -> int:
    args = sys.argv[1:]
    if not args:
        print(__doc__)
        return 2
    seam, accounts, sql = build()
    print(liveness())
    verb = args[0]

    if verb == "ground":
        master = args[1]
        facts = seam.facts()
        print(f"facts: {facts}")
        print(f"ready: {party.ready(facts)}  blocker: {party.blocker(facts)}")
        marker = dbreads.resolve_marker(ENTRY, SERVER)
        print(f"marker: {marker.marker} {marker.problem}")
        print(f"{master} online guid: {seam.online_guid(master)}")
        print(f"party rows now: {seam.members(master)}")
        chars = ENTRY.schema_map()["characters"]
        print(
            "group_member rows on the whole server: "
            + sql.query("characters", f"SELECT COUNT(*) FROM {chars}.group_member;").strip()
        )
        return 0

    if verb == "state":
        state = seam.state(args[1])
        print(f"ready={state.ready}")
        print(f"blocker={state.blocker!r}")
        print(f"problem={state.problem!r}")
        print(f"members={state.members}")
        show_checks(state)
        return 0

    if verb == "add":
        master, klass = args[1], args[2]
        before = seam.members(master)
        print(f"BEFORE: {before}")
        result = seam.add(master, klass)
        print(f"RESULT: {result}")
        print(f"AFTER : {seam.members(master)}")
        return 0

    if verb == "dismiss":
        master, bot = args[1], args[2]
        print(f"BEFORE: {seam.members(master)}")
        result = seam.remove(master, bot)
        print(f"RESULT: {result}")
        print(f"AFTER : {seam.members(master)}")
        return 0

    if verb == "password":
        # 8.3a's own seam, so the client's account is changed the way every
        # other account on this install is. The value is NOT printed.
        result = accounts.set_password(args[1], args[2])
        print(f"set_password ok={getattr(result, 'ok', result)}")
        return 0

    print(f"unknown: {verb}")
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
