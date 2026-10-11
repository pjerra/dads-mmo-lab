"""T699: the Server tab's state at a glance, as words and shapes, without a widget."""

from __future__ import annotations

import pytest

from yulon.controller import InstallStatus
from yulon.ui import server_status as ss

PORTS = (None, 3724, 8085)


@pytest.mark.parametrize(
    ("status", "word", "shape"),
    [
        ("running", "ONLINE", "dot"),
        ("starting", "STARTING", "half"),
        ("partial", "PARTLY UP", "triangle"),
        ("stopped", "OFFLINE", "ring"),
        ("unknown", "STATUS UNKNOWN", "question"),
        ("stopping", "STOPPING", "half"),
        ("restarting", "RESTARTING", "half"),
        ("loop", "CRASH LOOP", "diamond"),
        ("failed", "UPDATE FAILED", "cross"),
        ("REALM ONLINE-ish nonsense", "OFFLINE", "ring"),
    ],
)
def test_every_badge_word_has_its_pill(status: str, word: str, shape: str) -> None:
    look = ss.pill_for(status)
    assert (look.word, look.shape) == (word, shape)


def test_the_four_main_states_differ_in_colour_shape_and_word() -> None:
    """Colour-blind safe: no two of the mockup's four share a tone, a shape or a word."""
    looks = [ss.pill_for(s) for s in ("running", "starting", "partial", "stopped")]
    for part in ("word", "shape", "tone"):
        values = [getattr(look, part) for look in looks]
        assert len(set(values)) == 4, (part, values)


def test_unknown_is_never_drawn_as_offline() -> None:
    assert ss.pill_for("unknown").tone != ss.pill_for("stopped").tone
    assert ss.pill_for("unknown").shape != ss.pill_for("stopped").shape


def test_all_up_and_ready_reads_running_running_ready() -> None:
    lines = ss.service_lines(InstallStatus(True, True, True), "running", (3306, 3724, 8085))
    assert [(line.name, line.word) for line in lines] == [
        ("Database", "Running"),
        ("Login server", "Running"),
        ("World server", "Ready"),
    ]
    assert [line.shape for line in lines] == ["dot", "dot", "dot"]
    assert lines[0].detail == "port 3306"
    assert lines[1].detail == "port 3724"
    assert lines[2].detail.startswith("port 8085")


def test_a_world_up_but_not_ready_is_loading() -> None:
    lines = ss.service_lines(InstallStatus(True, True, True), "starting", PORTS)
    assert (lines[2].word, lines[2].shape) == ("Loading…", "half")
    assert (lines[0].word, lines[1].word) == ("Running", "Running")


def test_partly_up_marks_the_stopped_service_as_the_one_to_look_at() -> None:
    lines = ss.service_lines(InstallStatus(True, False, True), "partial", PORTS)
    assert (lines[1].word, lines[1].shape, lines[1].tone) == ("Stopped", "triangle", "partly")
    assert (lines[0].shape, lines[2].shape) == ("dot", "dot")


def test_all_stopped_is_three_rings() -> None:
    lines = ss.service_lines(InstallStatus(False, False, False), "stopped", PORTS)
    assert [(line.word, line.shape, line.tone) for line in lines] == [
        ("Stopped", "ring", "down")
    ] * 3


def test_an_unread_poll_says_unknown_on_every_row_never_stopped() -> None:
    lines = ss.service_lines(None, "unknown", PORTS)
    assert [line.word for line in lines] == ["Unknown"] * 3
    assert {line.shape for line in lines} == {"question"}


def test_a_database_with_no_published_port_says_no_port() -> None:
    lines = ss.service_lines(InstallStatus(True, True, True), "running", PORTS)
    assert lines[0].detail == ""


@pytest.mark.parametrize(
    ("world", "word", "shape"),
    [("loop", "Crash loop", "diamond"), ("failed", "Update failed", "cross")],
)
def test_the_world_row_carries_the_verdicts_loop_and_failure(
    world: str, word: str, shape: str
) -> None:
    lines = ss.service_lines(InstallStatus(True, True, True), world, PORTS)
    assert (lines[2].word, lines[2].shape) == (word, shape)


def test_a_world_without_a_verdict_is_running_not_ready() -> None:
    """A held badge ("stopping") over a world in docker ps claims neither ready nor loading."""
    lines = ss.service_lines(InstallStatus(True, True, True), "stopping", PORTS)
    assert (lines[2].word, lines[2].shape) == ("Running", "dot")


def test_the_services_summary_counts_what_runs() -> None:
    assert ss.services_summary(InstallStatus(True, True, True)) == "3 of 3 running"
    assert ss.services_summary(InstallStatus(True, False, False)) == "1 of 3 running"
    assert ss.services_summary(None) == "not known"


def test_partly_up_names_what_is_down_and_the_press() -> None:
    said = ss.partly_up_sentence(InstallStatus(True, False, True))
    assert said == "The login server is not running. Press Start to bring it up."
    two = ss.partly_up_sentence(InstallStatus(True, False, False))
    assert "login server and the world server are not running" in two
    assert ss.partly_up_sentence(InstallStatus(True, True, True)) == ""
    assert ss.partly_up_sentence(InstallStatus(False, False, False)) == ""
    assert ss.partly_up_sentence(None) == ""


def test_get_playing_has_four_steps_in_order() -> None:
    steps = ss.get_playing(True, True, 0)
    assert [s.key for s in steps] == ["server", "client", "account", "play"]
    assert [s.done for s in steps] == [True, True, False, False]
    assert ss.steps_done(steps) == "2 of 4 done"


def test_an_unread_account_count_is_neither_done_nor_missing() -> None:
    steps = ss.get_playing(False, False, None)
    assert steps[2].done is None
    assert ss.steps_done(steps) == "0 of 4 done"


def test_with_everything_set_up_only_play_is_left() -> None:
    steps = ss.get_playing(True, True, 2)
    assert ss.steps_done(steps) == "Ready to play"
    assert ss.steps_done(ss.get_playing(True, True, None)) == "2 of 4 done"


@pytest.mark.parametrize(
    ("chosen", "announced", "said"),
    [
        ("lan", None, "LAN"),
        ("internet", "203.0.113.9", "Internet"),
        ("loopback", "127.0.0.1", "This computer only"),
        (None, "192.168.0.60", "LAN (set at install)"),
        (None, None, "LAN (set at install)"),
        (None, "127.0.0.1", "This computer only (set at install)"),
        ("lan", "127.0.0.1", "LAN"),
    ],
)
def test_the_mode_now_is_the_last_applied_one(
    chosen: str | None, announced: str | None, said: str
) -> None:
    assert ss.mode_now(chosen, announced) == said


@pytest.mark.parametrize(
    ("status", "players", "said"),
    [
        ("running", 3, "Online · 3 players"),
        ("running", 1, "Online · 1 player"),
        ("running", None, "Online"),
        ("starting", 3, "Starting"),
        ("partial", None, "Partly up"),
        ("stopped", None, "Offline"),
        ("unknown", None, "Status unknown"),
    ],
)
def test_the_sidebar_line(status: str, players: int | None, said: str) -> None:
    assert ss.sidebar_line(status, players) == said
