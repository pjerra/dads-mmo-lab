"""The enable press: writing the channel on, with the world stopped (8.2a).

The press is the risky half of this box, and its shape came out of the
adversarial review. The version before it probed that the port was free and then
wrote — check-then-act, with a window between the probe and the bind, and a bind
that can fail for reasons a probe cannot see. On the CMaNGOS trees that costs
`exit(-1)` with no character saves.

Requiring the world **stopped** removes the hazard instead of warning about it,
and it is smaller than the guard it replaced: the configuration is written while
the server is down, the user's ordinary Start brings it up through the staged
start Phase 7 already proved, and there is nothing running to interrupt.
"""

from __future__ import annotations

from pathlib import Path

import pytest

from yulon import channel_setup as setup
from yulon import resources
from yulon.catalog.catalog import load_catalog
from yulon.catalog.composegen import BASE_FILE, BUILD_FILE, OVERRIDE_FILE

WOTLK = load_catalog().get("wow-wotlk")


def _installed(tmp_path: Path) -> Path:
    """A server dir with the three compose files, as an install has them."""
    from yulon.catalog import composegen

    plan = composegen.render(WOTLK, tmp_path, templates_root=resources.installers_dir())
    composegen.write_plan(plan, tmp_path)
    return tmp_path


def test_the_press_refuses_while_the_world_is_running_and_writes_nothing(
    tmp_path: Path,
) -> None:
    """The whole shape of this step, in one assertion.

    A failed bind is not something a probe can prevent, so the press does not
    try: it declines to run at all while there is a world to lose.
    """
    server_dir = _installed(tmp_path)
    before = (server_dir / OVERRIDE_FILE).read_text(encoding="utf-8")

    with pytest.raises(setup.EnableRefused, match="stopped"):
        setup.enable(
            WOTLK,
            server_dir,
            templates_root=resources.installers_dir(),
            world_running=True,
        )

    assert (server_dir / OVERRIDE_FILE).read_text(encoding="utf-8") == before


def test_the_press_writes_the_keys_that_turn_the_channel_on(tmp_path: Path) -> None:
    server_dir = _installed(tmp_path)

    setup.enable(WOTLK, server_dir, templates_root=resources.installers_dir(), world_running=False)

    written = (server_dir / OVERRIDE_FILE).read_text(encoding="utf-8")
    assert 'AC_SOAP_ENABLED: "1"' in written
    assert 'AC_SOAP_IP: "0.0.0.0"' in written
    assert 'AC_AI_PLAYERBOT_COMMAND_SERVER_PORT: "0"' in written
    assert 'AC_RA_ENABLE: "0"' in written


def test_the_press_keeps_what_the_install_already_had(tmp_path: Path) -> None:
    """The bot population is in that same block, and losing it is a different server."""
    server_dir = _installed(tmp_path)
    before = (server_dir / OVERRIDE_FILE).read_text(encoding="utf-8")
    kept = [line for line in before.splitlines() if "AC_AI_PLAYERBOT" in line]
    assert kept, "the fixture has no existing env to preserve; this test proves nothing"

    setup.enable(WOTLK, server_dir, templates_root=resources.installers_dir(), world_running=False)

    written = (server_dir / OVERRIDE_FILE).read_text(encoding="utf-8")
    for line in kept:
        if "COMMAND_SERVER_PORT" in line:
            continue  # this one the press deliberately changes
        assert line in written


def test_a_second_press_changes_nothing(tmp_path: Path) -> None:
    """Idempotent, and it says so: `changed` is False the second time."""
    server_dir = _installed(tmp_path)

    first = setup.enable(
        WOTLK, server_dir, templates_root=resources.installers_dir(), world_running=False
    )
    after_first = (server_dir / OVERRIDE_FILE).read_text(encoding="utf-8")
    second = setup.enable(
        WOTLK, server_dir, templates_root=resources.installers_dir(), world_running=False
    )

    assert first.changed is True
    assert second.changed is False
    assert (server_dir / OVERRIDE_FILE).read_text(encoding="utf-8") == after_first


def test_the_press_touches_only_the_override(tmp_path: Path) -> None:
    """The reason the SOAP environment lives in this block and not in the install's.

    Phase 7.1's compose fixtures assert byte-identical rendered files. If this
    press rewrote the base or the build file, that assertion would start
    meaning something else — and nobody would notice until it mattered.
    """
    server_dir = _installed(tmp_path)
    untouched = {
        name: (server_dir / name).read_text(encoding="utf-8") for name in (BASE_FILE, BUILD_FILE)
    }

    setup.enable(WOTLK, server_dir, templates_root=resources.installers_dir(), world_running=False)

    for name, text in untouched.items():
        assert (server_dir / name).read_text(encoding="utf-8") == text, f"{name} was rewritten"


def test_an_entry_with_no_operations_block_is_refused_rather_than_guessed_at(
    tmp_path: Path,
) -> None:
    """8.2b, 8.2c and 8.2d each measure their own; nothing is inherited early."""
    tbc = load_catalog().get("wow-tbc")

    with pytest.raises(setup.EnableRefused, match="wow-tbc"):
        setup.enable(tbc, tmp_path, templates_root=resources.installers_dir(), world_running=False)


# -- the port claim, and rolling it back (8.2a) ------------------------------


def test_the_press_claims_the_host_port_in_dotenv_so_the_claim_can_be_released(
    tmp_path: Path,
) -> None:
    """The claim is written even though it equals the compose default.

    The base compose publishes SOAP at `127.0.0.1:7878` by its DEFAULT VALUE,
    so before this the port was claimed by every install whether or not it had
    a channel — and nothing could give it back. Writing the claim down is what
    makes releasing it possible: `roll_back()` has a key to change.
    """
    server_dir = _installed(tmp_path)

    setup.enable(WOTLK, server_dir, templates_root=resources.installers_dir(), world_running=False)

    env = (server_dir / ".env").read_text(encoding="utf-8")
    assert f"{setup.HOST_PORT_VAR}=127.0.0.1:7878" in env


def test_rolling_back_restores_the_override_the_press_replaced(tmp_path: Path) -> None:
    server_dir = _installed(tmp_path)
    before = (server_dir / OVERRIDE_FILE).read_text(encoding="utf-8")
    setup.enable(WOTLK, server_dir, templates_root=resources.installers_dir(), world_running=False)
    assert (server_dir / OVERRIDE_FILE).read_text(encoding="utf-8") != before

    rolled = setup.roll_back(WOTLK, server_dir)

    assert rolled is True
    assert (server_dir / OVERRIDE_FILE).read_text(encoding="utf-8") == before


def test_rolling_back_releases_the_host_port_so_the_server_starts_again(
    tmp_path: Path,
) -> None:
    """The clause this exists for.

    An occupied 7878 stops the CONTAINER from being created at all — Docker
    refuses to publish a port something else holds — so restoring the
    environment alone would leave the install exactly as unstartable as it was.
    The claim goes back to `127.0.0.1:0`, which asks the daemon for any free
    port: the server starts, and the channel is not reachable at a port it does
    not own, which is what rolled back means.
    """
    server_dir = _installed(tmp_path)
    setup.enable(WOTLK, server_dir, templates_root=resources.installers_dir(), world_running=False)

    setup.roll_back(WOTLK, server_dir)

    env = (server_dir / ".env").read_text(encoding="utf-8")
    assert f"{setup.HOST_PORT_VAR}=127.0.0.1:0" in env
    assert f"{setup.HOST_PORT_VAR}=127.0.0.1:7878" not in env


def test_rolling_back_with_nothing_to_undo_says_so_and_writes_nothing(
    tmp_path: Path,
) -> None:
    """A rollback that never had a press behind it must not invent one.

    Restoring "the state before" from a backup that does not exist would write
    an empty override over a good one.
    """
    server_dir = _installed(tmp_path)
    before = (server_dir / OVERRIDE_FILE).read_text(encoding="utf-8")

    assert setup.roll_back(WOTLK, server_dir) is False
    assert (server_dir / OVERRIDE_FILE).read_text(encoding="utf-8") == before


def test_a_second_press_does_not_overwrite_the_backup_of_the_original(
    tmp_path: Path,
) -> None:
    """Otherwise the second press backs up the channel's own configuration.

    The user presses twice — the first press already having written the
    channel on — and the rollback would then restore a file with the channel
    still in it, which is not a rollback at all.
    """
    server_dir = _installed(tmp_path)
    original = (server_dir / OVERRIDE_FILE).read_text(encoding="utf-8")
    setup.enable(WOTLK, server_dir, templates_root=resources.installers_dir(), world_running=False)
    setup.enable(WOTLK, server_dir, templates_root=resources.installers_dir(), world_running=False)

    setup.roll_back(WOTLK, server_dir)

    assert (server_dir / OVERRIDE_FILE).read_text(encoding="utf-8") == original


def test_a_start_failure_that_names_our_port_is_told_apart_from_one_that_does_not() -> None:
    """The predicate that decides whether a rollback is even relevant.

    Docker's message for a taken published port names the port; a message about
    anything else must not cause the channel to be silently switched off.
    """
    # Two wordings, and the SECOND one is why this test exists in this shape.
    # The first is what Docker's older message looks like and what this
    # predicate was written against. The second is what the daemon on
    # `yulon-ubuntu` actually said on 2026-09-07 when the gate held the port,
    # and it contains neither "bind for" nor "already allocated" -- so the
    # first version of this predicate answered no, no rollback happened, and
    # the gate failed on the clause it was written to prove.
    older = (
        "Error response from daemon: driver failed programming external "
        "connectivity on endpoint ac-worldserver: Bind for 127.0.0.1:7878 failed: "
        "port is already allocated"
    )
    measured = (
        "Error response from daemon: failed to set up container networking: driver "
        "failed programming external connectivity on endpoint ac-worldserver "
        "(34fb00de9718): failed to bind host port 127.0.0.1:7878/tcp: address "
        "already in use"
    )
    for taken in (older, measured):
        assert setup.blames_the_host_port(taken, 7878) is True
        assert setup.blames_the_host_port(taken, 8085) is False
    assert setup.blames_the_host_port("ac-database exited with code 1", 7878) is False
    assert setup.blames_the_host_port("could not bind: out of memory", 7878) is False
    # Compose says this on every command in an install that has not claimed the
    # port yet. It names 7878 and is not a failure at all, so matching on the
    # number alone would turn an ordinary start into a silent rollback.
    assert (
        setup.blames_the_host_port(
            'The "DOCKER_SOAP_EXTERNAL_PORT" variable is not set. ' "Defaulting to 127.0.0.1:7878.",
            7878,
        )
        is False
    )
    assert setup.blames_the_host_port("Bind for 127.0.0.1:7878 failed", 78) is False
    assert setup.blames_the_host_port("failed to bind host port 127.0.0.1:78780/tcp", 7878) is False
