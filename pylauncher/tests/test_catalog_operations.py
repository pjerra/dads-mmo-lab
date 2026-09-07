"""The per-entry operations block: how a tree's command channel is switched on (8.2a).

Every value here is a per-tree fact and is asserted against the entry rather
than against a literal, so a tree that changes its mind fails rather than
inherits its neighbour's answer.
"""

from __future__ import annotations

import pytest
from pydantic import ValidationError

from yulon.catalog.catalog import ConfEnable, Operations, load_catalog

WOTLK = load_catalog().get("wow-wotlk")


def test_wotlks_channel_is_soap_on_the_port_its_compose_file_already_publishes() -> None:
    """7878 is not a new number: the base compose has published it all along.

    `${DOCKER_SOAP_EXTERNAL_PORT:-127.0.0.1:7878}:7878` publishes a container
    port that nothing is listening on until this step turns SOAP on
    (`azerothcore.md:104-106`).
    """
    assert WOTLK.operations is not None
    assert WOTLK.operations.channel == "soap"
    assert WOTLK.operations.port == 7878


def test_the_listener_binds_every_interface_inside_the_container() -> None:
    """The load-bearing value in the whole block, and the one that was measured.

    `SOAP.IP` defaults to `127.0.0.1` — the container's OWN loopback — and the
    spike measured that such a listener does not serve a published port and does
    not refuse either: Docker accepts and relays nothing, so a connect probe
    passes on the broken configuration. The host side stays pinned to loopback
    by the compose publication, so `0.0.0.0` here is not an exposure.
    """
    assert WOTLK.operations is not None
    assert WOTLK.operations.enable_env["AC_SOAP_IP"] == "0.0.0.0"
    assert WOTLK.operations.enable_env["AC_SOAP_ENABLED"] == "1"


def test_the_same_press_closes_the_playerbot_command_server() -> None:
    """It defaults to ON, and this install reads no module conf at all.

    `AiPlayerbot.CommandServerPort` is 8888 in the dist AND in the compiled
    fallback (`PlayerbotAIConfig.cpp:468`), it accepts `<command>,<bot guid>`
    lines on its own detached thread, and 8.1a's gate measured this core
    logging "Not found modules config files" — so the compiled default is what
    is in force. Opening one remote channel while leaving another at a
    listening default is the finding this avoids.
    """
    assert WOTLK.operations is not None
    assert WOTLK.operations.enable_env["AC_AI_PLAYERBOT_COMMAND_SERVER_PORT"] == "0"
    assert 8888 in WOTLK.operations.must_not_listen


def test_the_remote_console_is_left_off_and_asserted_off() -> None:
    """`Ra.Enable` is 0 by default; the press says so anyway and the gate checks."""
    assert WOTLK.operations is not None
    assert WOTLK.operations.enable_env["AC_RA_ENABLE"] == "0"
    assert 3443 in WOTLK.operations.must_not_listen


def test_the_environment_keys_are_spelled_the_way_this_core_derives_them() -> None:
    """`"AC_" + upper_snake(key)`, the rule the catalog already relies on.

    Asserted against a key that predates this block: the install's own
    `world_env` carries `AC_AI_PLAYERBOT_MIN_RANDOM_BOTS` for
    `AiPlayerbot.MinRandomBots`, which is the proof the transform is generic
    rather than a per-key coincidence.
    """
    assert WOTLK.operations is not None
    native = WOTLK.install.native
    assert native is not None and native.azerothcore is not None
    existing = set(native.azerothcore.world_env)
    assert any(
        key.startswith("AC_AI_PLAYERBOT_") for key in existing
    ), "the proof key is gone; the spelling rule below rests on it"
    for key in WOTLK.operations.enable_env:
        assert key.startswith("AC_"), key
        assert key.upper() == key, key


def test_the_gm_level_is_the_one_soap_itself_requires() -> None:
    """`SEC_ADMINISTRATOR` (3), below which the listener answers 403."""
    assert WOTLK.operations is not None
    assert WOTLK.operations.gm_level == 3


@pytest.mark.parametrize("game", ["wow-vanilla", "wow-tortoise"])
def test_the_other_trees_have_no_block_until_their_own_box(game: str) -> None:
    """8.2b, 8.2c and 8.2d measure their own; Tortoise has no SOAP at all."""
    assert load_catalog().get(game).operations is None


def test_a_soap_channel_that_never_says_how_to_switch_soap_on_is_refused() -> None:
    """The relationship has no other owner, so the model owns it."""
    with pytest.raises(ValidationError, match="turns SOAP on"):
        Operations(
            channel="soap",
            port=7878,
            gm_level=3,
            enable_env={"AC_RA_ENABLE": "0"},
        )


def test_a_tree_with_no_environment_route_says_which_conf_file_turns_it_on() -> None:
    """CMaNGOS reads no environment at all, so the env route cannot serve it.

    AzerothCore maps every ini key `X` to `AC_<UPPER_SNAKE>` generically
    (`Config.cpp:435-438`), which is why 8.2a's whole enable is four environment
    keys. The CMaNGOS lineage has no such rule anywhere in its reader — the
    keys come from the conf file and nowhere else (`src/shared/Config/
    Config.cpp:73`, `:91`) — so its channel is switched on by patching
    `mangosd.conf`, which is the mechanism the install already uses for six
    other keys.
    """
    ops = Operations(
        channel="soap",
        port=7878,
        gm_level=3,
        enable_conf=ConfEnable(
            file="etc/mangosd.conf",
            keys={"SOAP.Enabled": "1", "SOAP.IP": "0.0.0.0", "SOAP.Port": "7878"},
        ),
    )

    assert ops.enable_env == {}
    assert ops.enable_conf is not None
    assert ops.enable_conf.file == "etc/mangosd.conf"
    assert ops.enable_conf.keys["SOAP.Enabled"] == "1"


def test_a_conf_route_that_never_mentions_soap_is_refused_like_the_env_one() -> None:
    """The same clause as the environment route: a channel nobody can enable is none."""
    with pytest.raises(ValidationError, match="turns SOAP on"):
        Operations(
            channel="soap",
            port=7878,
            gm_level=3,
            enable_conf=ConfEnable(file="etc/mangosd.conf", keys={"Ra.Enable": "0"}),
        )


def test_declaring_both_routes_is_refused() -> None:
    """One install is switched on one way. Two routes is a question, not a plan."""
    with pytest.raises(ValidationError, match="switched on ONE way"):
        Operations(
            channel="soap",
            port=7878,
            gm_level=3,
            enable_env={"AC_SOAP_ENABLED": "1"},
            enable_conf=ConfEnable(file="etc/mangosd.conf", keys={"SOAP.Enabled": "1"}),
        )


def test_a_block_with_no_enable_keys_at_all_is_refused() -> None:
    with pytest.raises(ValidationError):
        Operations(channel="soap", port=7878, gm_level=3, enable_env={})
