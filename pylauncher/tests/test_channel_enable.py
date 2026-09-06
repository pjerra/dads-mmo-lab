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
