"""Tests for `PartyPanel` (8.6): My Party's own surface, offscreen, through a stub seam.

Every press here goes through the SAME `MyPartySeam` the gate script drove
(`pyplan/gates/8.6-wotlk-yulon-ubuntu2-2026-09-09/gate86b.py`), so what the panel
can do is exactly what was proved live on `yulon-ubuntu2`: read the state, add a
bot of a chosen class, see it in the party, dismiss it.

The stub answers with `party`'s own dataclasses rather than with look-alikes: the
four states of an `Addition` are the reason that class exists, and a fake with a
`joined` field of its own would let the panel treat "the server took the command"
as "a bot arrived", which is what "My Party works" used to mean (2026-08-20).
"""

from __future__ import annotations

from yulon import party
from yulon.ui.widgets.job import run_inline
from yulon.ui.widgets.party_panel import PartyPanel

READY_CHECKS = (
    party.Precondition("server_installed", True, ""),
    party.Precondition("bridge_answered", True, ""),
)
"""Two of the nine, which is all any assertion here needs. `party.preconditions()`
owns the list and `test_party.py` owns its contents."""


class _StubParty:
    """Stands in for `party.InstallParty` — one read and two presses, recorded.

    A stub rather than the real object for the reason every other seam in these
    tests is stubbed: the real one reads a database and sends over a channel, and
    a panel test that needed either would be a test of a server.
    """

    def __init__(
        self,
        state: party.PartyState | None = None,
        addition: party.Addition | None = None,
        dismissal: party.Dismissal | None = None,
    ) -> None:
        self.state_result = state or party.PartyState(True, "", READY_CHECKS)
        self.addition = addition or party.Addition(
            True, True, "Jilsur", True, True, "Jilsur joined the party, geared and specced."
        )
        self.dismissal = dismissal or party.Dismissal(True, True, "Jilsur left the party.")
        self.asked: list[str] = []
        self.added: list[tuple[str, str]] = []
        self.removed: list[tuple[str, str]] = []

    def state(self, master: str) -> party.PartyState:
        self.asked.append(master)
        return self.state_result

    def add(self, master: str, klass: str, *, gender: str = "") -> party.Addition:
        self.added.append((master, klass))
        return self.addition

    def remove(self, master: str, bot: str) -> party.Dismissal:
        self.removed.append((master, bot))
        return self.dismissal


def _member(name: str = "Jilsur", guid: int = 948) -> party.Member:
    return party.Member(name=name, guid=guid, klass=8, level=1)


def _panel(stub: _StubParty) -> PartyPanel:
    return PartyPanel(stub, jobs=run_inline)


def _rows(panel: PartyPanel) -> list[str]:
    return [panel.member_list.item(i).text() for i in range(panel.member_list.count())]


def _checks(panel: PartyPanel) -> list[str]:
    return [panel.check_list.item(i).text() for i in range(panel.check_list.count())]


def test_the_party_shows_the_bots_the_seam_reports(qapp: object) -> None:
    """The rows, named, with what the group table says about each."""
    stub = _StubParty(
        state=party.PartyState(True, "", READY_CHECKS, members=(_member(), _member("Anmi", 949)))
    )
    panel = _panel(stub)
    panel.character.setText("Pakka")

    panel.refresh_party()

    assert stub.asked == ["Pakka"]
    assert any("Jilsur" in row for row in _rows(panel))
    assert any("Anmi" in row for row in _rows(panel))


def test_a_blocker_is_the_whole_answer_and_no_row_is_drawn(qapp: object) -> None:
    """The precondition that stopped it, and never an empty list beside it.

    `PartyState`'s own docstring is the reason: the empty list a broken bridge
    produces is the same empty list a working party with no bots in it produces,
    and the 2026-08-20 failure is that pair being told apart wrongly.
    """
    stopped = (
        party.Precondition("engine_enabled", True, ""),
        party.Precondition("script_path", False, "the Lua engine is looking for scripts in x"),
    )
    stub = _StubParty(
        state=party.PartyState(False, "the Lua engine is looking for scripts in x", stopped)
    )
    panel = _panel(stub)
    panel.character.setText("Pakka")

    panel.refresh_party()

    assert _rows(panel) == []
    assert "the Lua engine is looking for scripts in x" in panel.summary.text()
    assert _checks(panel) == ["ok engine_enabled", "not yet: script_path"]
    assert panel.report.text() == "", "a refused read must not leave `Working…` standing"


def test_the_unmet_check_carries_its_own_sentence(qapp: object) -> None:
    """All nine are listed, and the one that failed says why where a person can read it.

    `PartyState.checks` carries every check rather than only the blocking one so
    somebody fixing this can see how far down the list they have got.
    """
    stopped = (
        party.Precondition("engine_in_binary", False, "the Lua engine is not in the binary"),
    )
    stub = _StubParty(state=party.PartyState(False, "the Lua engine is not in the binary", stopped))
    panel = _panel(stub)
    panel.character.setText("Pakka")

    panel.refresh_party()

    assert panel.check_list.item(0).toolTip() == "the Lua engine is not in the binary"


def test_a_party_that_could_not_be_read_says_the_problem_and_shows_no_rows(qapp: object) -> None:
    """`ready` with a `problem` is the third answer: the checks passed, the READ did not."""
    stub = _StubParty(
        state=party.PartyState(True, "", READY_CHECKS, problem="Pakka is not logged in.")
    )
    panel = _panel(stub)
    panel.character.setText("Pakka")

    panel.refresh_party()

    assert _rows(panel) == []
    assert "not logged in" in panel.summary.text()


def test_a_refusal_does_not_leave_the_last_readings_rows_on_screen(qapp: object) -> None:
    """The ground is read first: the rows ARE there before the refusal."""
    stub = _StubParty(state=party.PartyState(True, "", READY_CHECKS, members=(_member(),)))
    panel = _panel(stub)
    panel.character.setText("Pakka")
    panel.refresh_party()
    assert _rows(panel) != [], "the ground: a party was on screen before the refusal"

    stub.state_result = party.PartyState(False, "the world server is not running", READY_CHECKS)
    panel.refresh_party()

    assert _rows(panel) == []
    assert "the world server is not running" in panel.summary.text()


def test_no_character_named_sends_nothing_at_all(qapp: object) -> None:
    """Neither the read nor either press reaches the seam with an empty box."""
    stub = _StubParty()
    panel = _panel(stub)

    panel.refresh_party()
    panel.add_bot()
    panel.dismiss_bot()

    assert (stub.asked, stub.added, stub.removed) == ([], [], [])
    assert "Type the name of the character" in panel.report.text()


def test_the_classes_offered_are_the_ones_this_trees_addclass_takes(qapp: object) -> None:
    """Read off `party.BOT_CLASSES`, so a class this tree refuses cannot be offered.

    `dk` is in that tuple because THIS tree's `addclass` takes it, which the bash
    launcher's own list deliberately did not -- a per-tree fact, measured per
    tree. The panel offering its own list is how the two drift.
    """
    panel = _panel(_StubParty())

    offered = [panel.klass.itemText(i) for i in range(panel.klass.count())]

    assert tuple(offered) == party.BOT_CLASSES


def test_the_press_sends_the_chosen_class_and_the_servers_own_sentence_survives_the_re_read(
    qapp: object,
) -> None:
    """The add's own report text is still on screen after the group is re-read.

    Two things happen on one press: the seam says what it did, and the party is
    read again -- because the panel must never draw a row the group table has not
    got. The second must not silently replace the first. A panel that ends a
    press reading "1 bot in this party" has thrown away the only sentence that
    said whether the bot was GEARED, or whether the server took the command and
    no bot ever arrived.
    """
    stub = _StubParty(state=party.PartyState(True, "", READY_CHECKS, members=(_member(),)))
    panel = _panel(stub)
    panel.character.setText("Pakka")
    panel.klass.setCurrentText("mage")

    panel.add_bot()

    assert stub.added == [("Pakka", "mage")]
    assert panel.report.text() == "Jilsur joined the party, geared and specced."
    assert any("Jilsur" in row for row in _rows(panel)), "the group was re-read, not assumed"


def test_a_bot_that_never_joined_is_not_drawn_as_a_party_member(qapp: object) -> None:
    """The third outcome, and the panel adds no row of its own for it.

    `Addition(added=True, joined=False)` is the server taking the command and no
    bot arriving within the poll window. Reporting that as success is what
    "My Party works" used to mean, and a panel that drew its own row after a
    `yes` would be saying it in a different place.
    """
    stub = _StubParty(
        addition=party.Addition(
            True,
            False,
            None,
            False,
            False,
            "the server accepted the command and no bot joined the party within 6 seconds.",
        )
    )
    panel = _panel(stub)
    panel.character.setText("Pakka")

    panel.add_bot()

    assert _rows(panel) == []
    assert "no bot joined the party" in panel.report.text()


def test_a_press_stopped_by_a_precondition_says_which_one_and_sends_nothing(qapp: object) -> None:
    """`blocker` set is the seam saying it never touched the channel."""
    stopped = "the Lua engine is switched off: ALE.Enabled is not set to 1 in mod_ale.conf."
    stub = _StubParty(
        addition=party.Addition(False, False, None, False, False, stopped, blocker=stopped),
        state=party.PartyState(False, stopped, READY_CHECKS),
    )
    panel = _panel(stub)
    panel.character.setText("Pakka")

    panel.add_bot()

    assert "ALE.Enabled" in panel.report.text()
    assert _rows(panel) == []


def test_the_dismiss_button_names_the_bot_it_would_send_away(qapp: object) -> None:
    """The Characters tab's rule: a button somebody presses says what it acts on."""
    stub = _StubParty(state=party.PartyState(True, "", READY_CHECKS, members=(_member(),)))
    panel = _panel(stub)
    panel.character.setText("Pakka")
    panel.refresh_party()
    assert panel.dismiss_button.text() == "Dismiss", "the ground: nothing is chosen yet"
    assert panel.dismiss_button.isEnabled() is False

    panel.member_list.setCurrentRow(0)

    assert panel.dismiss_button.text() == "Dismiss Jilsur"
    assert panel.dismiss_button.isEnabled() is True


def test_dismiss_with_no_bot_chosen_sends_nothing(qapp: object) -> None:
    stub = _StubParty(state=party.PartyState(True, "", READY_CHECKS, members=(_member(),)))
    panel = _panel(stub)
    panel.character.setText("Pakka")
    panel.refresh_party()

    panel.dismiss_bot()

    assert stub.removed == []
    assert panel.report.text() == "Choose a bot in the party list first. Nothing was sent."


def test_dismissing_reads_the_group_back_rather_than_dropping_the_row_itself(
    qapp: object,
) -> None:
    """`Dismissal.removed` is the group table read afterwards; so is the list."""
    stub = _StubParty(state=party.PartyState(True, "", READY_CHECKS, members=(_member(),)))
    panel = _panel(stub)
    panel.character.setText("Pakka")
    panel.refresh_party()
    panel.member_list.setCurrentRow(0)
    assert _rows(panel) != [], "the ground: the bot is in the list before the press"
    stub.state_result = party.PartyState(True, "", READY_CHECKS)

    panel.dismiss_bot()

    assert stub.removed == [("Pakka", "Jilsur")]
    assert panel.report.text() == "Jilsur left the party."
    assert _rows(panel) == []


def test_a_second_press_is_refused_while_one_is_in_flight(qapp: object) -> None:
    """`add_bot` polls for up to six seconds; a person will press again.

    Two adds in flight would poll each other's bots -- `party.add_bot` decides a
    bot arrived by finding a guid that was not in ITS own before-reading.
    """
    held: list[object] = []

    def never_finishes(work: object, on_done: object, on_error: object) -> None:
        held.append(work)

    stub = _StubParty()
    panel = PartyPanel(stub, jobs=never_finishes)  # type: ignore[arg-type]
    panel.character.setText("Pakka")

    panel.add_bot()
    assert panel.add_button.isEnabled() is False, "the ground: the button is locked while it runs"
    panel.add_bot()
    panel.refresh_party()

    assert len(held) == 1


def test_a_seam_that_raises_becomes_a_sentence_and_the_panel_stays_usable(qapp: object) -> None:
    """Every seam failure has one answer, and the panel is pressable afterwards."""

    class _Broken(_StubParty):
        def state(self, master: str) -> party.PartyState:
            raise RuntimeError("no such container")

    panel = _panel(_Broken())
    panel.character.setText("Pakka")

    panel.refresh_party()

    assert "no such container" in panel.report.text()
    assert panel.add_button.isEnabled() is True


def test_a_press_that_reached_the_server_says_so_and_a_local_refusal_does_not(
    qapp: object,
) -> None:
    """`party_changed` is for whoever else is counting bots (design :111)."""
    stub = _StubParty()
    panel = _panel(stub)
    heard: list[int] = []
    panel.party_changed.connect(lambda: heard.append(1))

    panel.add_bot()  # no character named: refused here, nothing sent
    assert heard == [], "the ground: a local refusal reached no server"

    panel.character.setText("Pakka")
    panel.add_bot()

    assert heard == [1]
