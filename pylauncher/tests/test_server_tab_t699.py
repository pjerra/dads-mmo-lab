# ruff: noqa: F811 - pytest fixtures imported from test_controller_view are parameters here
"""T699 / T696 / T698: the Server tab at a glance, Get playing, the mode in force, PLAY when ready.

Over the docker-free fakes of `test_controller_view`: `ps.names` is what `docker ps`
answers, so a poll (`refresh_status()`, inline) reads it the way the app does.
"""

from __future__ import annotations

from pathlib import Path
from types import SimpleNamespace
from typing import Any

import pytest

from tests.conftest import process_events
from tests.test_controller_view import (  # noqa: F401 - fixtures
    WORLD_UP,
    WOTLK,
    _answer,
    _built,
    _Deferred,
    _game_client,
    _play_view,
    _Ps,
    _RealmWait,
    _save_launcher,
    _this_host_owns,
    launched,
    ps,
)
from yulon import networking
from yulon.catalog.catalog import CatalogEntry, load_catalog
from yulon.controller import InstallStatus
from yulon.dashboard import Verdict
from yulon.ui import controller_view as cvm
from yulon.ui import server_status
from yulon.ui.controller_view import ControllerView

DB_AND_WORLD = "ac-database\nac-worldserver\n"


@pytest.fixture(autouse=True)
def _inline_jobs(monkeypatch: pytest.MonkeyPatch) -> None:
    """The view's jobs run inline, as `test_controller_view`'s and the launcher tests' do."""
    from yulon.ui.widgets.job import run_inline

    monkeypatch.setattr(cvm, "threaded_job_runner", lambda _parent: run_inline)


def _view(ps: _Ps, tmp_path: Path, *, play: bool = True, **kwargs: Any) -> ControllerView:
    original = _game_client(tmp_path / "clients" / "WoW")
    built = _built(original, tmp_path) if play else None
    view, _ = _play_view(ps, tmp_path, original=original, play=built, **kwargs)
    return view


def _poll(view: ControllerView, ps: _Ps, names: str) -> None:
    ps.names = names
    view.refresh_status()
    process_events()


# -- the pill and the rows: one reading, the poll's --------------------------------------


def test_the_pill_says_online_partly_up_and_offline_from_the_poll(
    qapp: object, ps: _Ps, tmp_path: Path
) -> None:
    view = _view(ps, tmp_path)
    _poll(view, ps, WORLD_UP)
    assert view.status_pill.look is server_status.ONLINE
    _poll(view, ps, DB_AND_WORLD)
    assert view.status_pill.look is server_status.PARTLY_UP
    assert "login server is not running" in view.glance_label.text()
    _poll(view, ps, "")
    assert view.status_pill.look is server_status.OFFLINE


def test_the_pill_says_starting_until_the_verdict_says_the_world_is_ready(
    qapp: object, ps: _Ps, tmp_path: Path
) -> None:
    view = _view(ps, tmp_path)
    view.services.dashboard = object()  # a game whose world prints a ready marker
    _poll(view, ps, WORLD_UP)
    assert view.status_pill.look is server_status.STARTING
    assert view.service_rows[2].line is not None
    assert view.service_rows[2].line.word == "Loading…"

    view._verdict_ready(Verdict(state="up", players=3, bots=500))

    assert view.status_pill.look is server_status.ONLINE
    assert view.service_rows[2].line.word == "Ready"
    assert view.verdict_label.text() == "up — 3 players, 500 bots"
    assert view.server_hero.isAncestorOf(view.verdict_label), "the world's line is not by the pill"
    assert view.glance_label.isHidden(), "a second line beside the world's own"


def test_docker_not_answering_is_unknown_on_the_pill_and_every_row(
    qapp: object, ps: _Ps, tmp_path: Path
) -> None:
    view = _view(ps, tmp_path)
    _poll(view, ps, WORLD_UP)
    view._status_failed(cvm.docker.DockerCommandError("no daemon"))
    assert view.status_pill.look is server_status.UNKNOWN
    assert [row.line.word for row in view.service_rows if row.line] == ["Unknown"] * 3


def test_each_service_row_is_its_own_container(qapp: object, ps: _Ps, tmp_path: Path) -> None:
    view = _view(ps, tmp_path)
    _poll(view, ps, DB_AND_WORLD)
    words = [(row.line.name, row.line.word) for row in view.service_rows if row.line]
    assert words == [
        ("Database", "Running"),
        ("Login server", "Stopped"),
        ("World server", "Running"),  # up in docker ps; "Ready" is the realm's word
    ]
    assert view.services_summary_label.text() == "2 of 3 running"
    assert view.status_label.isHidden(), "the rows already say what the poll's line says"


def test_the_status_line_still_says_what_is_not_the_polls_words(
    qapp: object, ps: _Ps, tmp_path: Path
) -> None:
    view = _view(ps, tmp_path)
    view.status_label.setText("Starting…")
    assert not view.status_label.isHidden()


# -- Get playing (T696) ------------------------------------------------------------------


def test_get_playing_without_a_ready_to_play_client_offers_make(
    qapp: object, ps: _Ps, tmp_path: Path
) -> None:
    view = _view(ps, tmp_path, play=False)
    steps = {row.step.key: row for row in view.step_rows if row.step}
    assert steps["client"].step.done is False
    assert not view.step_make_button.isHidden()
    assert view.step_make_button.text() == cvm.STEP_MAKE_LABEL
    assert view.step_account_button.isHidden(), "accounts not read yet: nothing to offer"


def test_get_playing_with_no_client_folder_offers_to_set_it(
    qapp: object, ps: _Ps, tmp_path: Path
) -> None:
    view, _ = _play_view(ps, tmp_path, original=None, play=None)
    assert view.step_make_button.text() == cvm.SET_CLIENT_DIR_LABEL


def test_get_playing_counts_the_accounts_read_once_the_server_is_up(
    qapp: object, ps: _Ps, tmp_path: Path
) -> None:
    view = _view(ps, tmp_path)
    reads: list[int] = []

    def read() -> object:
        reads.append(1)
        return SimpleNamespace(accounts=(), announced="192.168.0.60")

    view.realm_reads = read
    _poll(view, ps, "")
    assert reads == [], "read while the server was down"
    assert not view.step_start_button.isHidden()

    _poll(view, ps, WORLD_UP)
    _poll(view, ps, WORLD_UP)

    assert reads == [1], "read on every poll, or never"
    steps = {row.step.key: row.step for row in view.step_rows if row.step}
    assert steps["server"].done and steps["client"].done
    assert steps["account"].done is False
    assert view.step_start_button.isHidden()
    assert not view.step_account_button.isHidden()
    assert view.steps_label.text() == "2 of 4 done"
    assert view.realm_address_value.text() == "192.168.0.60"

    _poll(view, ps, "")
    _poll(view, ps, WORLD_UP)
    assert reads == [1, 1], "a server that came back up is read again"


def test_create_account_asks_and_runs_the_accounts_tabs_own_create(
    qapp: object, ps: _Ps, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    told: list[str] = []
    monkeypatch.setattr(cvm, "show_information", lambda _p, _t, text: told.append(text))
    view = _view(ps, tmp_path)
    accounts: list[tuple[str, ...]] = [(), ("pkpk",)]
    view.realm_reads = lambda: SimpleNamespace(accounts=accounts.pop(0), announced=None)
    _poll(view, ps, WORLD_UP)
    assert not view.step_account_button.isHidden()
    changed: list[int] = []
    view.accounts_changed.connect(lambda: changed.append(1))
    view._account_asker = lambda _parent: ("pkpk", "secret")

    view.step_account_button.click()

    made = view.services.create_account.__self__.created  # type: ignore[attr-defined]
    assert made == [("pkpk", "secret", 0)]
    assert view.account_password.text() == "", "the password stayed in a field"
    assert told and told[0].startswith("pkpk: created")
    assert changed == [1]
    assert view.step_account_button.isHidden(), "the step did not tick after the account"
    assert view.steps_label.text() == "Ready to play"


def test_create_account_cancelled_creates_nothing(qapp: object, ps: _Ps, tmp_path: Path) -> None:
    view = _view(ps, tmp_path)
    view._account_asker = lambda _parent: None
    view.create_account_from_a_dialog()
    assert view.services.create_account.__self__.created == []  # type: ignore[attr-defined]


def test_create_account_with_an_empty_field_says_so_in_a_box(
    qapp: object, ps: _Ps, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    warned: list[str] = []
    monkeypatch.setattr(cvm, "show_warning", lambda _p, _t, text: warned.append(text))
    view = _view(ps, tmp_path)
    view._account_asker = lambda _parent: ("pk", "")
    view.create_account_from_a_dialog()
    assert warned == [cvm.ACCOUNT_FIELDS_NEEDED]
    assert view.services.create_account.__self__.created == []  # type: ignore[attr-defined]


# -- the mode in force (T698) ------------------------------------------------------------


@pytest.mark.parametrize(
    ("mode", "radio", "said"),
    [
        ("internet", "internet_radio", "Internet"),
        ("loopback", "loopback_radio", "This computer only"),
        (None, "lan_radio", "LAN (set at install)"),
    ],
)
def test_networking_and_the_realm_card_show_the_mode_last_applied(
    qapp: object, ps: _Ps, tmp_path: Path, mode: str | None, radio: str, said: str
) -> None:
    if mode is not None:
        assert networking.record_network_intent(tmp_path, mode) == ""  # type: ignore[arg-type]
    view = _view(ps, tmp_path)
    assert getattr(view, radio).isChecked()
    assert view.network_mode_value.text() == said
    assert view.network_now_label.text().startswith(f"Now: {said}.")
    assert "Launcher" in view.network_now_label.text()


def test_after_apply_the_mode_now_is_the_one_applied(qapp: object, ps: _Ps, tmp_path: Path) -> None:
    view = _view(ps, tmp_path)
    assert view.network_mode_value.text() == "LAN (set at install)"
    networking.record_network_intent(tmp_path, "internet")
    view._apply_done(None)
    assert view.network_mode_value.text() == "Internet"


# -- PLAY when ready (T699) --------------------------------------------------------------


def test_play_pressed_while_its_start_runs_waits_for_it_and_the_realm(
    qapp: object, ps: _Ps, tmp_path: Path, launched: list[object], monkeypatch: pytest.MonkeyPatch
) -> None:
    _this_host_owns(monkeypatch)
    jobs = _Deferred()
    view = _view(ps, tmp_path, job_runner=jobs)
    wait = _RealmWait(launched)
    view.services.ready_after_start = wait

    view.start_server()  # the Start is on its worker
    assert view._busy and view.play_button is not None
    assert view.play_button.isEnabled(), "PLAY greyed while the Start runs"
    assert view.play_button.text() == cvm.PLAY_WHEN_READY_LABEL

    view.play_button.click()
    assert view.play_label.text() == cvm.PLAY_WAITS_FOR_THE_START
    assert launched == [] and wait.asked == []

    ps.names = WORLD_UP
    while jobs.queue:
        jobs.run()
    assert wait.asked == [0], "the realm was not waited for after the Start"
    assert len(launched) == 1


def test_play_while_a_start_fails_ends_with_its_words(
    qapp: object, ps: _Ps, tmp_path: Path, launched: list[object]
) -> None:
    jobs = _Deferred()
    view = _view(ps, tmp_path, job_runner=jobs)
    asked_before = len(jobs.queue)
    view.start_server()
    start = len(jobs.queue) - 1
    assert start >= asked_before, "Start queued nothing"
    view.play_button.click()  # type: ignore[union-attr]
    work, _done, failed = jobs.queue.pop(start)
    failed(RuntimeError("compose said no"))
    assert view.play_label.text() == cvm.PLAY_START_FAILED
    assert not view._play_pending and launched == []


def test_play_on_a_world_still_loading_waits_for_the_realm_first(
    qapp: object, ps: _Ps, tmp_path: Path, launched: list[object], monkeypatch: pytest.MonkeyPatch
) -> None:
    _this_host_owns(monkeypatch)
    view = _view(ps, tmp_path)
    view.services.dashboard = object()
    wait = _RealmWait(launched)
    view.services.ready_after_start = wait
    _poll(view, ps, WORLD_UP)
    assert view.status_pill.look is server_status.STARTING

    view.play()

    assert wait.asked == [0], "launched into a world that was still loading"
    assert len(launched) == 1


def test_play_on_a_ready_world_does_not_wait(
    qapp: object, ps: _Ps, tmp_path: Path, launched: list[object], monkeypatch: pytest.MonkeyPatch
) -> None:
    _this_host_owns(monkeypatch)
    view = _view(ps, tmp_path)
    wait = _RealmWait(launched)
    view.services.ready_after_start = wait
    _poll(view, ps, WORLD_UP)
    assert view.play_button.text() == cvm.PLAY_LABEL  # type: ignore[union-attr]
    view.play()
    assert wait.asked == [] and len(launched) == 1


def test_play_says_it_starts_a_stopped_server(qapp: object, ps: _Ps, tmp_path: Path) -> None:
    view = _view(ps, tmp_path)
    _poll(view, ps, "")
    assert view.play_button.text() == cvm.PLAY_STARTS_IT_LABEL  # type: ignore[union-attr]


def test_a_stop_running_keeps_play_greyed(qapp: object, ps: _Ps, tmp_path: Path) -> None:
    """Only a Start is waited for: a Stop is not bringing the realm up."""
    jobs = _Deferred()
    view = _view(ps, tmp_path, job_runner=jobs)
    ps.names = WORLD_UP
    view.stop_server()
    assert not view.play_button.isEnabled()  # type: ignore[union-attr]


# -- the Danger zone, folded -------------------------------------------------------------


def test_the_danger_zone_is_folded_and_opens(qapp: object, ps: _Ps, tmp_path: Path) -> None:
    view = _view(ps, tmp_path)
    assert view.danger_body.isHidden()
    view.danger_toggle.click()
    assert not view.danger_body.isHidden()
    assert view.danger_toggle.text() == cvm.DANGER_HIDE


def test_a_repair_offer_opens_the_danger_zone_by_itself(
    qapp: object, ps: _Ps, tmp_path: Path
) -> None:
    view = _view(ps, tmp_path)
    view.repair_button.setVisible(True)
    assert not view.danger_body.isHidden()


def test_every_removal_press_is_still_in_the_danger_zone(
    qapp: object, ps: _Ps, tmp_path: Path
) -> None:
    view = _view(ps, tmp_path)
    for press in (view.remove_button, view.forget_install_button, view.repair_button):
        assert press is not None and view.danger_body.isAncestorOf(press)


# -- every game shares the tab -----------------------------------------------------------


@pytest.mark.parametrize(
    "game", ["wow-wotlk", "wow-tbc", "wow-vanilla", "wow-tortoise", "wow-centurion", "wow-unbound"]
)
def test_every_game_builds_the_new_tab_with_three_service_rows(
    qapp: object, ps: _Ps, tmp_path: Path, game: str
) -> None:
    entry: CatalogEntry = load_catalog().get(game)
    view = _view(ps, tmp_path, entry=entry)
    _poll(view, ps, "")
    assert [row.line.name for row in view.service_rows if row.line] == list(
        server_status.SERVICE_NAMES
    )
    assert view.service_rows[1].line.detail == f"port {entry.ports.auth}"
    assert view.service_rows[2].line.detail == f"port {entry.ports.world}"
    titles = [view._tabs.tabText(i) for i in range(view._tabs.count())]
    assert titles[0] == "Server" and titles[-1] == "Console"
    assert view._tabs.currentIndex() == 0


def test_the_sub_tabs_are_in_the_mockups_order(qapp: object, ps: _Ps, tmp_path: Path) -> None:
    view = _view(ps, tmp_path)
    titles = [view._tabs.tabText(i) for i in range(view._tabs.count())]
    wanted = [t for t in cvm.SUB_TAB_ORDER if t in titles]
    assert titles == wanted
    assert titles.index("Accounts") < titles.index("Networking") < titles.index("Console")


def test_the_server_tab_has_no_install_status_twice(qapp: object, ps: _Ps, tmp_path: Path) -> None:
    """The poll's InstallStatus and the rows are one reading: a fresh poll moves both."""
    view = _view(ps, tmp_path)
    _poll(view, ps, WORLD_UP)
    assert view._last_polled == InstallStatus(True, True, True)
    _poll(view, ps, "ac-database\n")
    assert [row.line.word for row in view.service_rows if row.line][1:] == ["Stopped", "Stopped"]
    _ = WOTLK


def test_play_on_a_partly_up_server_asks_to_start_the_rest_not_a_stopped_one(
    qapp: object, ps: _Ps, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """PARTLY UP's PLAY reads "PLAY (starts it)", and its question must not call it stopped."""
    asked: list[tuple[str, str]] = []
    monkeypatch.setattr(
        cvm, "_ask_with", lambda _parent, title, text, _yes, *_a: asked.append((title, text))
    )
    view = _view(ps, tmp_path)
    _poll(view, ps, DB_AND_WORLD)
    assert view.play_button.text() == cvm.PLAY_STARTS_IT_LABEL  # type: ignore[union-attr]

    view.play()

    assert len(asked) == 1
    title, text = asked[0]
    assert "stopped" not in text.casefold(), text
    assert "login server" in text.casefold(), "the question does not say what is down"
    assert not view._play_pending, "a cancelled question left Play waiting"


def test_play_on_a_stopped_server_still_says_stopped(
    qapp: object, ps: _Ps, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    asked: list[str] = []
    monkeypatch.setattr(cvm, "_ask_with", lambda _p, _t, text, _y, *_a: asked.append(text))
    view = _view(ps, tmp_path)
    _poll(view, ps, "")
    view.play()
    assert asked == ["The server is stopped. Start it first?"]


def test_create_account_with_a_name_the_rules_refuse_says_why_in_a_box(
    qapp: object, ps: _Ps, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """T673's rule refuses before anything is written; from the dialog that must be said, not
    left on the Accounts tab's line where the player who pressed Create account… cannot see it.
    """
    warned: list[str] = []
    monkeypatch.setattr(cvm, "show_warning", lambda _p, _t, text: warned.append(text))
    view = _view(ps, tmp_path)
    view._account_asker = lambda _parent: ("pk", "secret")  # two letters: too short a name
    view.create_account_from_a_dialog()
    assert view.services.create_account.__self__.created == []  # type: ignore[attr-defined]
    assert len(warned) == 1 and "account name" in warned[0], warned
    assert not view._account_from_a_dialog, "the next run's end would be said as this one's"


def test_play_while_another_job_of_the_tab_runs_is_refused_not_queued(
    qapp: object, ps: _Ps, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Only the tab's own Start brings the realm up, so only it is waited for ("PLAY when
    ready"). A PLAY pressed while a Stop runs is refused as a busy tab, as before T699: it must
    not sit queued to start the game when that Stop ends.
    """
    told: list[str] = []
    monkeypatch.setattr(cvm, "show_information", lambda _p, _t, text: told.append(text))
    jobs = _Deferred()
    view = _view(ps, tmp_path, job_runner=jobs)
    ps.names = WORLD_UP
    view.stop_server()
    assert view._busy and view._busy_job == "Stop"

    view.play()

    assert not view._play_pending, "PLAY queued behind a Stop as if it were the Start"
    assert not view._play_after_start
    assert view.play_label.text() != cvm.PLAY_WAITS_FOR_THE_START
    assert told and "busy" in told[0], told


# -- Round 4 (cold review) ---------------------------------------------------------------


def test_the_mode_in_force_is_read_once_not_on_every_poll(
    qapp: object, ps: _Ps, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """The record is a file in the server's folder -- a 9p read for a WSL server -- so it is
    read at build and after Apply, and every poll only formats what was read."""
    reads: list[Path] = []
    real = networking.read_network_intent

    def counted(server_dir: Path) -> object:
        reads.append(server_dir)
        return real(server_dir)

    monkeypatch.setattr(cvm.networking, "read_network_intent", counted)
    view = _view(ps, tmp_path)
    built = len(reads)
    assert built >= 1
    for names in (WORLD_UP, "", WORLD_UP):
        _poll(view, ps, names)
    view._verdict_ready(Verdict(state="up", players=1, bots=2))
    view._clear_the_verdict()
    assert len(reads) == built, f"read {len(reads) - built} more times on polls"
    networking.record_network_intent(tmp_path, "internet")
    view._apply_done(None)
    assert len(reads) == built + 1
    assert view.network_mode_value.text() == "Internet"


def test_before_the_first_reading_the_pill_says_unknown_and_play_claims_nothing(
    qapp: object, ps: _Ps, tmp_path: Path
) -> None:
    view = _view(ps, tmp_path)
    assert view._last_polled is None
    assert view.status_pill.look is server_status.UNKNOWN
    assert view.play_button.text() == cvm.PLAY_LABEL  # type: ignore[union-attr]
    assert view.glance_label.isHidden(), "says nothing is running before anything was read"
    _poll(view, ps, "")
    assert view.status_pill.look is server_status.OFFLINE
    assert view.play_button.text() == cvm.PLAY_STARTS_IT_LABEL  # type: ignore[union-attr]


def _remote_view(
    ps: _Ps, tmp_path: Path, monkeypatch: pytest.MonkeyPatch, jobs: Any = None
) -> ControllerView:
    _this_host_owns(monkeypatch)  # 192.168.0.60 is another computer's
    original = _game_client(tmp_path / "clients" / "WoW")
    play = _built(original, tmp_path)
    _save_launcher(play, {"realm_address": "192.168.0.60"})
    view, _ = _play_view(ps, tmp_path, original=original, play=play, job_runner=jobs)
    if jobs is not None:
        while jobs.queue:
            jobs.run()
    return view


def test_with_a_remote_realm_play_says_play_whatever_the_local_server_does(
    qapp: object, ps: _Ps, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """T667: another computer's realm does not need this one; PLAY never says it starts it."""
    view = _remote_view(ps, tmp_path, monkeypatch)
    _poll(view, ps, "")
    assert view.play_button.text() == cvm.PLAY_LABEL  # type: ignore[union-attr]
    _poll(view, ps, DB_AND_WORLD)
    assert view.play_button.text() == cvm.PLAY_LABEL  # type: ignore[union-attr]


def test_with_a_remote_realm_play_does_not_wait_for_a_local_start(
    qapp: object, ps: _Ps, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setattr(cvm, "show_information", lambda *_a: None)
    jobs = _Deferred()
    view = _remote_view(ps, tmp_path, monkeypatch, jobs)
    view.start_server()
    assert view._busy_job == "Start"
    assert view.play_button.text() != cvm.PLAY_WHEN_READY_LABEL  # type: ignore[union-attr]
    view.play()
    assert not view._play_after_start, "PLAY at another computer waits for the local Start"
    assert view.play_label.text() != cvm.PLAY_WAITS_FOR_THE_START


def test_the_danger_zone_opens_when_remove_from_yulon_is_the_way_out(
    qapp: object, ps: _Ps, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """T54/T95: a server whose folder is gone has "Remove from Yu'lon…" lit as the answer; a
    folded zone would hide the one press that is the way out."""
    view = _view(ps, tmp_path)
    assert view.danger_body.isHidden()
    monkeypatch.setattr(cvm.platform, "folder_is_gone", lambda _path: True)
    view._update_forget_visibility()
    assert view.forget_install_button is not None
    assert view.forget_install_button.property("primary") is True
    assert not view.danger_body.isHidden(), "the way out is lit inside a folded zone"


def test_a_failed_account_read_is_not_asked_again_on_every_poll(
    qapp: object, ps: _Ps, tmp_path: Path
) -> None:
    """Retried only once the server has been seen down and up again, not every five seconds."""
    view = _view(ps, tmp_path)
    asked: list[int] = []

    def broken() -> object:
        asked.append(1)
        raise RuntimeError("the database said no")

    view.realm_reads = broken
    for _ in range(3):
        _poll(view, ps, WORLD_UP)
    assert asked == [1], f"asked {len(asked)} times while the server stayed up"
    _poll(view, ps, "")
    _poll(view, ps, WORLD_UP)
    assert asked == [1, 1], "a server that came back up is not asked again"


def test_a_launcher_save_to_another_computers_address_is_read_again(
    qapp: object, ps: _Ps, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    _this_host_owns(monkeypatch)
    original = _game_client(tmp_path / "clients" / "WoW")
    play = _built(original, tmp_path)
    view, _ = _play_view(ps, tmp_path, original=original, play=play)
    _poll(view, ps, "")
    assert view.play_button.text() == cvm.PLAY_STARTS_IT_LABEL  # type: ignore[union-attr]
    _save_launcher(play, {"realm_address": "192.168.0.60"})
    view.launcher_saved.emit("")
    assert view.play_button.text() == cvm.PLAY_LABEL  # type: ignore[union-attr]
