"""Tests for the Characters tab (8.4a) — what it draws and what it promises.

The tab's own rules, from this box's line: every action is drawn only where the
tree has the command, every button names the selected character in its label,
and every press reports all three outcomes.
"""

from __future__ import annotations

from pathlib import Path

import pytest

from yulon.catalog.catalog import load_catalog
from yulon.play import Character
from yulon.ui import controller_view as controller_view_module
from yulon.ui.controller_view import ControllerServices, ControllerView
from yulon.ui.widgets.job import run_inline

WOTLK = load_catalog().get("wow-wotlk")
VANILLA = load_catalog().get("wow-vanilla")


@pytest.fixture(autouse=True)
def _inline_jobs(qapp: object, monkeypatch: pytest.MonkeyPatch) -> None:
    """The session's one offscreen QApplication, and jobs that run inline.

    `qapp` is depended on rather than requested per test: a view built with no
    QApplication takes the interpreter down, which reads as a test file that
    printed nothing at all.
    """
    monkeypatch.setattr(controller_view_module, "threaded_job_runner", lambda _parent: run_inline)


class _Play:
    """A stand-in for `InstallPlay`, recording what the tab asked it to do."""

    def __init__(self, *, characters: tuple[Character, ...] = (), pieces: int = 19) -> None:
        self.characters = characters
        self.pieces = pieces
        self.calls: list[tuple[str, tuple[object, ...]]] = []
        self.mail_item_cap = 12
        self.answer = _Outcome(True, text="done")

    def listing(self) -> tuple[Character, ...]:
        self.calls.append(("listing", ()))
        return self.characters

    def gear_set_size(self, character: str) -> tuple[int, int]:
        self.calls.append(("gear_set_size", (character,)))
        return (self.pieces, (self.pieces + 11) // 12)

    def teleport(self, character: str, location: str) -> object:
        self.calls.append(("teleport", (character, location)))
        return self.answer

    def set_level(self, character: str, level: int) -> object:
        self.calls.append(("set_level", (character, level)))
        return self.answer

    def rename(self, character: str) -> object:
        self.calls.append(("rename", (character,)))
        return self.answer

    def revive(self, character: str) -> object:
        self.calls.append(("revive", (character,)))
        return self.answer

    def mail_gold(self, character: str, *, gold: int, subject: str, body: str) -> object:
        self.calls.append(("mail_gold", (character, gold)))
        return self.answer

    def mail_items(self, character: str, *, items, subject: str, body: str) -> object:
        self.calls.append(("mail_items", (character, items)))
        return self.answer

    def send_gear_set(self, character: str, *, to: str, subject: str, body: str) -> object:
        self.calls.append(("send_gear_set", (character, to)))
        return self.answer

    def find_items(self, text: str) -> tuple[object, ...]:
        self.calls.append(("find_items", (text,)))
        return ()


class _Outcome:
    def __init__(self, done: bool, text: str = "", problem: str = "") -> None:
        self.done = done
        self.text = text
        self.problem = problem
        self.indeterminate = False


def _view(tmp_path: Path, entry=WOTLK, play=None) -> ControllerView:
    services = ControllerServices.for_entry(entry, tmp_path / entry.id)
    services.play = play
    return ControllerView(entry, services, status_poll_ms=0)


def _people() -> tuple[Character, ...]:
    return (
        Character(1, "Guglu", 78, True, "ADMIN"),
        Character(2, "Ganaar", 7, False, "PLAYER"),
    )


# -- what is drawn ----------------------------------------------------------


def test_a_tree_that_has_not_measured_its_play_block_gets_a_sentence(tmp_path: Path) -> None:
    """Not a disabled button: a button that cannot work is a promise the tab
    cannot keep, and 8.4c and 8.4d are the boxes that make it work there."""
    view = _view(tmp_path, VANILLA, play=None)

    assert view.character_list.isVisibleTo(view) is False
    assert VANILLA.name in view.character_report.text()


def test_the_actions_wait_for_a_character_to_be_chosen(tmp_path: Path) -> None:
    view = _view(tmp_path, play=_Play(characters=_people()))

    for button in view.character_buttons():
        assert button.isEnabled() is False, button.text()


def test_every_button_names_the_character_it_would_act_on(tmp_path: Path) -> None:
    """This box's own rule. "Revive" is a button somebody presses believing it
    acts on the row they are looking at; "Revive Guglu" is one they can check."""
    view = _view(tmp_path, play=_Play(characters=_people()))
    view.refresh_characters()
    view.character_list.setCurrentRow(0)

    for button in view.character_buttons():
        assert "Guglu" in button.text(), button.text()


def test_choosing_a_different_character_renames_every_button(tmp_path: Path) -> None:
    view = _view(tmp_path, play=_Play(characters=_people()))
    view.refresh_characters()
    view.character_list.setCurrentRow(0)
    view.character_list.setCurrentRow(1)

    for button in view.character_buttons():
        assert "Ganaar" in button.text(), button.text()


def test_the_gear_button_says_how_many_mails_before_it_is_pressed(tmp_path: Path) -> None:
    """The definition of done: "a nineteen-piece gear set sends the number of
    mails the button promised". The promise has to exist to be kept."""
    view = _view(tmp_path, play=_Play(characters=_people(), pieces=19))
    view.refresh_characters()
    view.character_list.setCurrentRow(0)

    assert "19" in view.send_gear_button.text()
    assert "2" in view.send_gear_button.text(), view.send_gear_button.text()


def test_a_character_wearing_nothing_leaves_the_gear_button_alone(tmp_path: Path) -> None:
    view = _view(tmp_path, play=_Play(characters=_people(), pieces=0))
    view.refresh_characters()
    view.character_list.setCurrentRow(0)

    assert view.send_gear_button.isEnabled() is False


# -- what is pressed --------------------------------------------------------


def test_each_action_reaches_the_seam_with_the_chosen_character(tmp_path: Path) -> None:
    play = _Play(characters=_people())
    view = _view(tmp_path, play=play)
    view.refresh_characters()
    view.character_list.setCurrentRow(0)

    view.teleport_where.setText("Stormwind")
    view.teleport_character()
    view.new_level.setValue(60)
    view.set_character_level()
    view.rename_character()
    view.revive_character()
    view.gold_amount.setValue(5)
    view.mail_gold()

    # The list and the gear size are re-read after every success on purpose --
    # a level change moves the row somebody is looking at -- so what this
    # asserts is the ACTIONS, in order, with the reads filtered out.
    reads = {"listing", "gear_set_size"}
    assert [name for name, _ in play.calls if name not in reads] == [
        "teleport",
        "set_level",
        "rename",
        "revive",
        "mail_gold",
    ]
    assert ("teleport", ("Guglu", "Stormwind")) in play.calls
    assert ("set_level", ("Guglu", 60)) in play.calls
    assert ("mail_gold", ("Guglu", 5)) in play.calls


def test_a_refusal_is_shown_and_does_not_look_like_a_success(tmp_path: Path) -> None:
    play = _Play(characters=_people())
    play.answer = _Outcome(False, problem="Character not found")
    view = _view(tmp_path, play=play)
    view.refresh_characters()
    view.character_list.setCurrentRow(0)

    view.revive_character()

    assert "Character not found" in view.character_report.text()


def test_a_success_says_what_the_server_said(tmp_path: Path) -> None:
    play = _Play(characters=_people())
    play.answer = _Outcome(True, text="You revive Guglu.")
    view = _view(tmp_path, play=play)
    view.refresh_characters()
    view.character_list.setCurrentRow(0)

    view.revive_character()

    assert "revive" in view.character_report.text().lower()
