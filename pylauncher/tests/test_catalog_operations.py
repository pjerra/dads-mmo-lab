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


def test_every_tree_now_states_its_channel_and_they_are_not_the_same() -> None:
    """8.2e was the last one, and the four answers are three different shapes.

    This test replaces the one that named the trees which had no block yet: as
    of 8.2e there are none, and asserting an absence that can no longer happen
    is a test that can only ever pass.
    """
    channels = {
        game: load_catalog().get(game).operations
        for game in ("wow-wotlk", "wow-tbc", "wow-vanilla", "wow-tortoise")
    }
    assert all(block is not None for block in channels.values()), channels

    assert channels["wow-wotlk"].enable_env and not channels["wow-wotlk"].enable_conf
    for game in ("wow-tbc", "wow-vanilla"):
        assert channels[game].enable_conf and not channels[game].enable_env
    # And the one that has nothing to switch on at all.
    assert channels["wow-tortoise"].channel == "attach"
    assert channels["wow-tortoise"].port is None


def test_an_attach_channel_declares_nothing_it_does_not_have() -> None:
    """Tortoise links neither gsoap nor RASocket, so most of this block is meaningless.

    There is no listener to enable, so no `enable_env` and no `enable_conf`;
    no port to publish; no envelope, so no namespace; and no account, because
    the console runs its commands at `SEC_CONSOLE` with account id 0. Declaring
    any of them would be describing a channel this core does not have.
    """
    ops = Operations(channel="attach")

    assert ops.enable_env == {} and ops.enable_conf is None
    assert ops.port is None
    assert ops.namespace is None
    assert ops.gm_level is None
    assert ops.publish is False


def test_an_attach_channel_that_claims_a_port_or_a_namespace_is_refused() -> None:
    """A field that cannot apply is worse than a missing one: it reads as measured."""
    for extra in ({"port": 7878}, {"namespace": "urn:MaNGOS"}, {"gm_level": 3}):
        with pytest.raises(ValidationError, match="attach"):
            Operations(channel="attach", **extra)


def test_a_soap_channel_still_has_to_say_all_three() -> None:
    """The other side of the same validator: nothing became optional for SOAP."""
    for missing in ("port", "namespace", "gm_level"):
        fields = {
            "channel": "soap",
            "namespace": "urn:AC",
            "port": 7878,
            "gm_level": 3,
            "enable_env": {"AC_SOAP_ENABLED": "1"},
        }
        del fields[missing]
        with pytest.raises(ValidationError):
            Operations(**fields)


def test_each_tree_states_its_own_soap_namespace() -> None:
    """Measured live on 2026-09-07, and it is not the same on both families.

    The CMaNGOS source study stopped exactly here -- "the gSOAP-generated
    envelope is in `soapC.cpp`, which is generated code I did not read... needs
    a judgement / live test" -- so it was measured against the running TBC
    server on m910q. `urn:AC` gets HTTP 500 `Method 'ns1:executeCommand' not
    implemented: method name or namespace not recognized`; `urn:MaNGOS` gets
    HTTP 200 and the server's own `CMaNGOS/0.18 ... Online players: 0`.

    There is no default, and that is the point: a default would be one tree's
    answer inherited by every other, and the failure it produces looks exactly
    like a world that has not finished loading -- the namespace is checked
    BEFORE the password, so a wrong one answers 500 even for a bad credential.
    """
    wotlk = load_catalog().get("wow-wotlk")
    tbc = load_catalog().get("wow-tbc")

    assert wotlk.operations is not None and wotlk.operations.namespace == "urn:AC"
    assert tbc.operations is not None and tbc.operations.namespace == "urn:MaNGOS"


def test_a_channel_with_no_namespace_is_refused() -> None:
    """A namespace nobody measured is a guess, and this one fails invisibly."""
    with pytest.raises(ValidationError):
        Operations(
            channel="soap",
            port=7878,
            gm_level=3,
            enable_env={"AC_SOAP_ENABLED": "1"},
        )


def test_a_soap_channel_that_never_says_how_to_switch_soap_on_is_refused() -> None:
    """The relationship has no other owner, so the model owns it."""
    with pytest.raises(ValidationError, match="turns SOAP on"):
        Operations(
            channel="soap",
            namespace="urn:AC",
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
        namespace="urn:MaNGOS",
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
            namespace="urn:MaNGOS",
            port=7878,
            gm_level=3,
            enable_conf=ConfEnable(file="etc/mangosd.conf", keys={"Ra.Enable": "0"}),
        )


def test_declaring_both_routes_is_refused() -> None:
    """One install is switched on one way. Two routes is a question, not a plan."""
    with pytest.raises(ValidationError, match="switched on ONE way"):
        Operations(
            channel="soap",
            namespace="urn:AC",
            port=7878,
            gm_level=3,
            enable_env={"AC_SOAP_ENABLED": "1"},
            enable_conf=ConfEnable(file="etc/mangosd.conf", keys={"SOAP.Enabled": "1"}),
        )


def test_a_block_with_no_enable_keys_at_all_is_refused() -> None:
    with pytest.raises(ValidationError):
        Operations(channel="soap", namespace="urn:AC", port=7878, gm_level=3, enable_env={})
