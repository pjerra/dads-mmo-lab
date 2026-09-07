"""`InstallChannel` — the channel state machine as the Server tab holds it (8.2a).

The module beneath this is pure and was tested that way. What is tested here is
the object the tab actually talks to: where it gets its state from when the app
opens, what it does when the credential it saved stops working, and the one
thing it must never do on that path — create a second account.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from yulon import channel_setup as setup
from yulon import resources
from yulon.catalog import composegen
from yulon.catalog.catalog import load_catalog

WOTLK = load_catalog().get("wow-wotlk")
INSTALL = "ab12cd34"


class _Answering:
    """A channel that answers the same way every time, and counts the asks."""

    def __init__(
        self,
        outcome: str,
        *,
        indeterminate: bool = False,
        text: str = "",
        denied: bool = False,
    ) -> None:
        self.outcome = outcome
        self.indeterminate = indeterminate
        self.text = text
        self.denied = denied
        self.asked = 0

    def send(self, _command: object) -> object:
        self.asked += 1
        return type(
            "Answer",
            (),
            {
                "outcome": self.outcome,
                "text": self.text,
                "indeterminate": self.indeterminate,
                "denied": self.denied,
            },
        )()


class _Scripted:
    """A channel whose answers are given in order, one per ask.

    A "no" here is the server REJECTING the credential -- `denied` -- because
    that is the only refusal `check()` acts on, and these scripts exist to
    drive it into the repair path.
    """

    def __init__(self, outcomes: list[str]) -> None:
        self.outcomes = list(outcomes)

    def send(self, _command: object) -> object:
        outcome = self.outcomes.pop(0) if self.outcomes else "no"
        return type(
            "Answer",
            (),
            {
                "outcome": outcome,
                "text": "",
                "indeterminate": False,
                "denied": outcome == "no",
            },
        )()


def _installed(tmp_path: Path) -> Path:
    server_dir = tmp_path / "server"
    server_dir.mkdir()
    plan = composegen.render(WOTLK, server_dir, templates_root=resources.installers_dir())
    composegen.write_plan(plan, server_dir)
    return server_dir


def _channel(
    tmp_path: Path,
    *,
    answering: _Answering,
    create: object = None,
    reset: object = None,
) -> setup.InstallChannel:
    def refuse_create(*_args: object) -> object:
        raise AssertionError("an account was created")

    return setup.InstallChannel(
        WOTLK,
        _installed(tmp_path),
        templates_root=resources.installers_dir(),
        install_id=INSTALL,
        create=create or refuse_create,  # type: ignore[arg-type]
        reset=reset or (lambda *_a: None),
        channel_for=lambda _endpoint: answering,
        config_dir=tmp_path / "config",
    )


def _save(
    tmp_path: Path, *, password: str = "saved-password", at: str | None = "2026-09-07 01:23 UTC"
) -> None:
    setup.save_credential(
        setup.Verified(setup.account_name(INSTALL), password, at=at),
        game=WOTLK.id,
        install_id=INSTALL,
        host="127.0.0.1",
        port=7878,
        config_dir=tmp_path / "config",
    )


def test_with_no_credential_on_disk_it_starts_idle(tmp_path: Path) -> None:
    channel = _channel(tmp_path, answering=_Answering("yes"))

    assert isinstance(channel.setup_state(), setup.Idle)


def test_a_saved_credential_reads_as_verified_with_the_time_it_was_proved(
    tmp_path: Path,
) -> None:
    """Because a credential is written only after a round trip answered.

    The file IS the record of that answer, so the time in it is not decoration:
    it is the difference between "this works" and "this worked once".
    """
    _save(tmp_path)

    channel = _channel(tmp_path, answering=_Answering("yes"))

    state = channel.setup_state()
    assert isinstance(state, setup.Verified)
    assert state.at == "2026-09-07 01:23 UTC"


def test_a_credential_written_before_times_existed_reads_as_verified_without_one(
    tmp_path: Path,
) -> None:
    """It must not be repaired, and it must not claim it was proved just now."""
    _save(tmp_path)
    path = setup.credential_path(WOTLK.id, INSTALL, config_dir=tmp_path / "config")
    raw = json.loads(path.read_text(encoding="utf-8"))
    del raw["verified_at"]
    path.write_text(json.dumps(raw), encoding="utf-8")

    state = _channel(tmp_path, answering=_Answering("yes")).setup_state()

    assert isinstance(state, setup.Verified)
    assert state.at is None


def test_checking_a_credential_the_server_rejects_downgrades_it_to_refused(
    tmp_path: Path,
) -> None:
    _save(tmp_path)
    channel = _channel(tmp_path, answering=_Answering("no", text="401", denied=True))

    state = channel.check()

    assert isinstance(state, setup.Refused)
    assert channel.setup_state() is state


def test_a_server_that_cannot_be_reached_leaves_the_credential_alone(
    tmp_path: Path,
) -> None:
    """ "I could not ask" is not "your password is wrong".

    A stopped world would otherwise offer the user a repair for a problem that
    is not theirs, and the repair would reset a working password.
    """
    _save(tmp_path)
    channel = _channel(tmp_path, answering=_Answering("no", indeterminate=True))

    state = channel.check()

    assert isinstance(state, setup.Verified)


def test_a_server_that_is_simply_not_there_does_not_offer_to_reset_anything(
    tmp_path: Path,
) -> None:
    """An adversarial review's first finding, 2026-09-07.

    `check()` used to downgrade on anything that was not a yes and not a
    timeout. Connection refused, a socket error and an unreadable reply all
    arrive as `unknown` with `indeterminate` false, so opening the tab against
    a stopped server read as "your password is wrong" and offered to rotate a
    GM account's password to fix a container that was not running.

    Only the server saying it does not accept this credential may downgrade,
    which is what `denied` names.
    """
    _save(tmp_path)
    channel = _channel(tmp_path, answering=_Answering("unknown", denied=False))

    assert isinstance(channel.check(), setup.Verified)


def test_a_repair_interrupted_after_the_reset_is_recoverable_by_repairing_again(
    tmp_path: Path,
) -> None:
    """The window the review named, and what actually closes it.

    Repair writes the database first and the credential file last, so a crash
    in between leaves the server accepting a password nothing on disk knows.
    That is not a dead end: the stale file is refused, which is exactly the
    state `repair()` exists for, and the account is this app's own -- nobody
    plays it and nothing else uses it. Rotating again is the recovery, and it
    is the same button.

    A journal was considered and not written: it would put a second copy of a
    live credential on disk to protect an account whose only recovery cost is
    one more reset.
    """
    _save(tmp_path, password="stale")
    resets: list[str] = []
    # The state is already Refused -- that is what the interrupted attempt
    # left behind -- so this repair asks once, with the password it has just
    # written, and the server accepts it.
    answering = _Scripted(["yes"])
    channel = _channel(
        tmp_path,
        answering=answering,
        reset=lambda name, _pw: resets.append(name),
    )
    channel._state = setup.Refused(
        setup.account_name(INSTALL), "stale", reason="left behind by an interrupted repair"
    )

    state = channel.repair()

    assert isinstance(state, setup.Verified)
    assert resets == [setup.account_name(INSTALL)]
    saved = setup.load_credential(WOTLK.id, INSTALL, config_dir=tmp_path / "config")
    assert saved is not None and saved.password != "stale"


def test_checking_an_install_with_no_credential_asks_the_server_nothing(
    tmp_path: Path,
) -> None:
    answering = _Answering("yes")
    channel = _channel(tmp_path, answering=answering)

    channel.check()

    assert answering.asked == 0
    assert isinstance(channel.setup_state(), setup.Idle)


def test_repair_resets_the_existing_account_and_never_creates_another(
    tmp_path: Path,
) -> None:
    """The `create` seam this fixture installs raises if it is ever called."""
    _save(tmp_path, password="stale")
    resets: list[tuple[str, str]] = []
    # No to the stale credential, yes to the one the reset writes -- which is
    # the only sequence that exercises a repair rather than a no-op.
    answering = _Scripted(["no", "yes"])
    channel = _channel(
        tmp_path,
        answering=answering,
        reset=lambda name, password: resets.append((name, password)),
    )
    channel.check()

    state = channel.repair()

    assert isinstance(state, setup.Verified)
    assert state.at is not None
    assert [name for name, _ in resets] == [setup.account_name(INSTALL)]
    saved = setup.load_credential(WOTLK.id, INSTALL, config_dir=tmp_path / "config")
    assert saved is not None
    assert saved.password == resets[0][1]
    assert saved.password != "stale"


def test_repair_from_a_state_that_is_not_refused_does_nothing(tmp_path: Path) -> None:
    """Repair is a button, and a button can be pressed at the wrong moment.

    Resetting the password of an account that is working would take a channel
    that answers and break it for as long as the reset takes to prove.
    """
    _save(tmp_path)
    resets: list[object] = []
    channel = _channel(tmp_path, answering=_Answering("yes"), reset=lambda *a: resets.append(a))

    state = channel.repair()

    assert isinstance(state, setup.Verified)
    assert resets == []


def test_rolling_back_from_the_install_undoes_its_own_press(tmp_path: Path) -> None:
    channel = _channel(tmp_path, answering=_Answering("yes"))
    override = channel.server_dir / composegen.OVERRIDE_FILE
    before = override.read_text(encoding="utf-8")
    channel.enable(world_running=False)

    assert channel.roll_back() is True
    assert override.read_text(encoding="utf-8") == before


def test_the_press_still_refuses_while_the_world_is_running(tmp_path: Path) -> None:
    channel = _channel(tmp_path, answering=_Answering("yes"))

    with pytest.raises(setup.EnableRefused, match="stopped"):
        channel.enable(world_running=True)


def test_settle_creates_and_proves_a_channel_that_has_never_been_set_up(
    tmp_path: Path,
) -> None:
    """What the tab calls after a start, and the only thing that ever creates.

    Without it the press writes a configuration nobody ever proves: the
    account is never made, so the channel the user turned on never works.
    """
    made: list[tuple[str, str, int]] = []
    channel = _channel(
        tmp_path,
        answering=_Answering("yes"),
        create=lambda name, pw, level: made.append((name, pw, level)),
    )

    state = channel.settle()

    assert isinstance(state, setup.Verified)
    assert [name for name, _, _ in made] == [setup.account_name(INSTALL)]
    assert made[0][2] == 3, "the channel account must be a full administrator"


def test_settle_on_a_working_channel_checks_it_rather_than_creating_again(
    tmp_path: Path,
) -> None:
    """The latch, at the seam the tab presses on every start."""
    _save(tmp_path)
    answering = _Answering("yes")
    channel = _channel(tmp_path, answering=answering)

    state = channel.settle()

    assert isinstance(state, setup.Verified)
    assert answering.asked == 1


def test_settle_leaves_a_channel_that_gave_up_alone(tmp_path: Path) -> None:
    channel = _channel(tmp_path, answering=_Answering("yes"))
    channel._state = setup.GaveUp(account="YULON_AB12CD34", reason="three tries")

    assert isinstance(channel.settle(), setup.GaveUp)


class _Captures:
    """Keeps the endpoint it was handed, and refuses to send anything."""

    def __init__(self) -> None:
        self.endpoints: list[object] = []

    def __call__(self, endpoint: object) -> object:
        self.endpoints.append(endpoint)
        return self

    def send(self, _command: object) -> object:
        raise AssertionError("this test does not send anything")


def test_the_endpoint_carries_this_install_s_own_namespace(tmp_path: Path) -> None:
    """`Endpoint.namespace` has a default, and this is what stops anything using it.

    The default exists so a credential file written before the field did stays
    readable. Nothing inside the app may lean on it: TBC's listener answers
    `urn:MaNGOS`, and an `urn:AC` envelope comes back HTTP 500 `method name or
    namespace not recognized` -- which from the caller's side is
    indistinguishable from a world that has not finished loading. Measured on
    m910q, 2026-09-07, against a live CMaNGOS worldserver.
    """
    tbc = load_catalog().get("wow-tbc")
    captures = _Captures()
    channel = setup.InstallChannel(
        tbc,
        tmp_path,
        templates_root=resources.installers_dir(),
        install_id=INSTALL,
        create=lambda *_args: None,
        channel_for=captures,
        config_dir=tmp_path / "config",
    )
    setup.save_credential(
        setup.Verified(account="YULON_AB12CD34", password="pw", at="2026-09-07 09:00 UTC"),
        game=tbc.id,
        install_id=INSTALL,
        host="127.0.0.1",
        port=7878,
        config_dir=tmp_path / "config",
    )

    assert channel.live_channel() is not None

    assert captures.endpoints, "no endpoint was built"
    assert captures.endpoints[0].namespace == "urn:MaNGOS", captures.endpoints[0]
    assert tbc.operations is not None and tbc.operations.namespace == "urn:MaNGOS"
