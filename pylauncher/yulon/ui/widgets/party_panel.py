"""My Party's own surface (8.6): the seam the gate script drove, with buttons on it.

`PartyPanel` is the whole of My Party in the app. It does what
`pyplan/gates/8.6-wotlk-yulon-ubuntu2-2026-09-09/gate86b.py` did on the live
server -- read the state, add a bot of a chosen class, see it in the party,
dismiss it -- through the same `MyPartySeam` object, because that gate's own
"What was NOT proved here" is *"The Qt widget for My Party does not exist"* and
the exit review names the consequence: a capability reachable only from a script
(`pyplan/phase8-exit-review-2026-09-09.md`, clause 3).

Call down / signal up (style-guide SS5): the panel calls `state()`, `add()` and
`remove()` on the seam it was handed, off the GUI thread through the runner it was
handed, and says `party_changed` when the group may have moved. It knows nothing
about databases, channels, containers or which view is showing it.

## What it does not do, and why

The prior art's own party page is bigger (`rust-main:launcher/src/lib/pages/Playerbots.svelte`),
and every part of it this panel leaves out is a part the seam has no method for.
Left out deliberately, each with the seam gap that decides it:

* **No character picker.** The seam reads one name's guid (`party.InstallParty.online_guid`)
  and has no "who is online" listing; the prior art's picker is fed by a separate
  CLI call (`Playerbots.svelte:75` `wowPartyOnline()`, offered as a `<select>` at
  `:390-398`). So the character is typed, and a name that is not in the world
  comes back as the seam's own sentence.
* **No "Enable My Party" / bridge deploy button.** `party.deploy()` exists and
  `InstallParty` does not expose it, so there is nothing to press through this
  seam. The panel shows WHICH precondition is unmet instead, which is the half
  that tells a person what to do -- and the prior art's own note is that a deploy
  button is how this feature last reported success for a no-op
  (`rust-main:crates/dml-wow/src/bridge.rs:56-70`).
* **No spec, no level, no "dismiss all", no presets.** `add()` takes a class and
  a gender; there is no spec whisper in `party.py` at all
  (the prior art's is `party.rs:253` `spec_whisper_cmd`), the level is 8.4a's
  Characters tab and a different seam, and dismiss-all is a loop the seam does
  not have (`Playerbots.svelte:204-220`, behind a two-step confirm there).

One thing the prior art does that this panel copies exactly: a bot that has not
arrived within the poll window is reported as NOT joined, with its cause
(`Playerbots.svelte:167`, the `SPAWNING_NOTE` branch of `add()`). Here that
sentence is `party.Addition`'s own and the panel only shows it.

## Two rules from `job.py` that this panel is bound by

The work runs through an injected `JobRunner`, and the callbacks are this class's
own bound `@Slot`s. Both halves matter: a plain function or lambda connected to a
worker's signal is delivered ON THE WORKER THREAD in PySide6, and a worker held
only by a local is collected before its thread runs it -- the GUI segfault
`job.py::InFlight` exists for.
"""

from __future__ import annotations

from collections.abc import Callable
from typing import Protocol, cast

from PySide6.QtCore import Qt, Signal, Slot
from PySide6.QtWidgets import (
    QComboBox,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QListWidget,
    QPushButton,
    QVBoxLayout,
    QWidget,
)

from yulon import party
from yulon.log import get_logger
from yulon.ui.widgets.job import JobRunner

logger = get_logger(__name__)


class PartySeam(Protocol):
    """What this panel needs: one read and two presses.

    The widget-side half of `controller_view.MyPartySeam`, and the same three
    methods. Two declarations rather than one import because a widget may not
    import a view (style-guide SS3's layer rule, and `ui/widgets/*.py`'s own row
    in that table: a widget must not know which view is using it) -- a
    `TYPE_CHECKING` import would be the same knowledge with a flag on it. What
    keeps them honest is that the view hands its own object down: if the two
    ever disagree, mypy fails at that call, which is `_build_my_party_group`.

    Typed rather than `object`, which is what this was first written as. Three
    `# type: ignore[attr-defined]` comments were the price, and the thing they
    bought silence about is a method name typed wrongly here -- a panel that
    calls `self._seam.state()` on an object that spells it `party_state` fails
    where nothing checks the name, which is at the user's press. Measured:
    renaming `state` here to `party_state` makes mypy report both halves --
    `"PartySeam" has no attribute "state"` in this file, and
    `Argument 1 to "PartyPanel" has incompatible type "MyPartySeam"` at
    `controller_view.py`'s call.

    The RETURNS are `party`'s own dataclasses and not `object` either. That is
    not extra coupling -- this module already imports `party` for `BOT_CLASSES`
    and `MAX_NAME` -- and it is what lets the drawing slots cast once and then
    read `state.blocker` instead of `getattr(state, "blocker", "")`. A defensive
    `getattr` chain over a shape this file KNOWS is a chain that answers "" for
    a field somebody renamed, which is a refusal that quietly stops being shown.
    """

    def state(self, master: str) -> party.PartyState: ...

    def add(self, master: str, klass: str, *, gender: str = "") -> party.Addition: ...

    def remove(self, master: str, bot: str) -> party.Dismissal: ...


DISMISS_NOTHING = "Dismiss"
"""What the dismiss button says with no bot chosen, and it is disabled saying it.

The Characters tab's rule, for its reason (`controller_view._CHARACTER_ACTIONS`):
a button called "Dismiss" is one somebody presses believing it acts on the row
they are looking at, so with a row chosen it says that row's name."""

NO_CHARACTER = (
    "Type the name of the character you are playing first. A bot is added to a live "
    "session -- the server resolves the master by name in the world -- so nothing was sent."
)
"""The refusal for an empty name field, and nothing is sent under it.

The seam would answer this too (`party.InstallParty.online_guid` returns None for
a name that is not a name, and `add()` turns that into its own sentence), but a
press with an empty box is a press a person can take back, and it costs a
database read and a `docker exec` to be told so."""

NO_BOT_CHOSEN = "Choose a bot in the party list first. Nothing was sent."

WORKING = "Working -- the server is being asked. This can take a few seconds."
"""Shown while a press is in flight, and the buttons are disabled under it.

`party.add_bot` polls for up to six seconds (12 x 500 ms) before it will say a
bot did not arrive, so a person watching a panel that said nothing would press
again. A second press is refused rather than queued: the seam re-reads the
preconditions on every press and two adds in flight would poll each other's
bots."""


class PartyPanel(QWidget):
    """My Party for one install: a character, a class, the group, and a dismissal.

    `party_changed` is emitted after every press that REACHED the seam, so
    whatever else is showing the bots can re-read them (the users-surface
    design's own cross-link, `pyplan/phase8-designs/b-users-surface.md:111`).
    Including the presses that came back refused, because a refusal is the
    seam's answer and not the panel's guess -- a `blocker` means nothing was
    sent, but a `joined=False` means the command WAS sent and the bot may still
    be walking in. A press this panel refused locally emits nothing: it never
    reached a server, so there is nothing for anybody to re-read.
    """

    party_changed = Signal()

    def __init__(
        self,
        seam: PartySeam,
        *,
        jobs: JobRunner,
        parent: QWidget | None = None,
    ) -> None:
        super().__init__(parent)
        self._seam = seam
        self._jobs = jobs
        self._busy = False
        self._clear_report = False

        self.character = QLineEdit(self)
        self.character.setPlaceholderText("the character you are playing, logged in")
        self.character.setMaxLength(party.MAX_NAME)
        self.character.returnPressed.connect(self.refresh_party)
        self.refresh_button = QPushButton("Show this character's party", self)
        self.refresh_button.clicked.connect(self.refresh_party)

        self.klass = QComboBox(self)
        # The server's own ten words, unmapped. A display table ("Death Knight"
        # for `dk`) would be a second list of classes to keep in step with
        # `party.BOT_CLASSES`, and the word that goes to the world server is
        # then not the word that was read on screen. `BOT_CLASSES` is this
        # tree's own measurement -- `dk` is in it because THIS tree's addclass
        # takes it, which the bash launcher's list deliberately did not.
        self.klass.addItems(party.BOT_CLASSES)
        self.add_button = QPushButton("Add a bot", self)
        self.add_button.clicked.connect(self.add_bot)

        self.member_list = QListWidget(self)
        self.member_list.currentRowChanged.connect(self._member_chosen)
        self.dismiss_button = QPushButton(DISMISS_NOTHING, self)
        self.dismiss_button.clicked.connect(self.dismiss_bot)

        self.check_list = QListWidget(self)
        self.summary = QLabel("", self)
        self.summary.setWordWrap(True)
        self.summary.setTextInteractionFlags(Qt.TextInteractionFlag.TextSelectableByMouse)
        self.report = QLabel("", self)
        self.report.setWordWrap(True)
        self.report.setTextInteractionFlags(Qt.TextInteractionFlag.TextSelectableByMouse)

        who = QHBoxLayout()
        who.addWidget(QLabel("Character", self))
        who.addWidget(self.character)
        who.addWidget(self.refresh_button)
        pick = QHBoxLayout()
        pick.addWidget(QLabel("Add a", self))
        pick.addWidget(self.klass)
        pick.addWidget(self.add_button)
        box = QVBoxLayout(self)
        box.addLayout(who)
        box.addLayout(pick)
        box.addWidget(QLabel("In the party now", self))
        box.addWidget(self.summary)
        box.addWidget(self.member_list)
        box.addWidget(self.dismiss_button)
        box.addWidget(QLabel("What My Party needs", self))
        box.addWidget(self.check_list)
        box.addWidget(self.report)
        self._member_chosen(-1)

    # -- reads ---------------------------------------------------------------

    @Slot()
    def refresh_party(self) -> None:
        """Read this character's party, off the GUI thread. A press of its own."""
        self._read(note=WORKING)

    def _read(self, *, note: str | None) -> None:
        """Read the group. `note` is `None` for the re-read that FOLLOWS a press.

        A press says two things -- what the server did, and what the group table
        holds afterwards -- and the second must not overwrite the first. Before
        this split the panel ended every add reading "1 bot in this party", which
        had thrown away the only sentence saying whether the bot was geared, or
        whether the server took the command and no bot ever arrived.
        """
        master = self._master()
        if master is None:
            return
        # A read that is its own press owns the report line, so its outcome has
        # to CLEAR it: the whole answer to "show me this party" is the summary
        # and the checks, and a `Working…` left standing over them is a panel
        # that looks like it is still asking.
        self._clear_report = note is not None
        self._start(note)
        self._run(lambda: self._seam.state(master), self._state_read)

    @Slot(object)
    def _state_read(self, result: object) -> None:
        """Draw the group, or the precondition that stopped it.

        The rows are cleared FIRST and drawn only from what this reading holds:
        a refusal beside the last successful reading's rows is a panel saying
        "there is no bridge" over a party it is still showing.

        The parameter is `object` because that is what a `JobRunner` carries --
        one runner delivers every job's result -- and it is cast once here, on
        the line below, rather than read field by field through `getattr`. Every
        view slot in this app is written the same way.
        """
        state = cast(party.PartyState, result)
        self._done()
        if self._clear_report:
            self.report.setText("")
        self.member_list.clear()
        self._member_chosen(-1)
        self.check_list.clear()
        for check in state.checks:
            self.check_list.addItem("ok " + check.name if check.met else f"not yet: {check.name}")
            if not check.met:
                self.check_list.item(self.check_list.count() - 1).setToolTip(check.sentence)
        if state.blocker or state.problem:
            self.summary.setText(state.blocker or state.problem)
            return
        members = state.members
        for member in members:
            self.member_list.addItem(self._row(member))
        self.summary.setText(
            "This character's party has no bots in it yet."
            if not members
            else f"{len(members)} {'bot' if len(members) == 1 else 'bots'} in this party."
        )

    @staticmethod
    def _row(member: party.Member) -> str:
        """One group row. The class is the NUMBER the table holds.

        `characters.class` is an id and this app has no measured id-to-name table
        for this tree; the prior art has one in the browser
        (`rust-main:launcher/src/lib/wow.ts` `className`). A wrong class name
        beside a bot is worse than the number that was actually read, so the
        number is what is shown until a box measures the mapping.
        """
        return f"{member.name} — level {member.level} — class {member.klass}"

    # -- presses -------------------------------------------------------------

    @Slot()
    def add_bot(self) -> None:
        """Add one bot of the chosen class to this character's party."""
        master = self._master()
        if master is None:
            return
        klass = self.klass.currentText()
        self._start(WORKING)
        self._run(
            lambda: self._seam.add(master, klass),
            self._added,
        )

    @Slot(object)
    def _added(self, result: object) -> None:
        """Say what the press did, then re-read the group rather than assume it.

        The panel never adds the row itself. `Addition` has four states and two
        of them -- the command accepted with no bot arriving, and a precondition
        that stopped it -- are exactly the ones a panel that drew its own row
        would report as a party member who is not there.
        """
        addition = cast(party.Addition, result)
        self._done()
        self.report.setText(addition.sentence)
        self._read(note=None)
        self.party_changed.emit()

    @Slot()
    def dismiss_bot(self) -> None:
        """Send the chosen bot away."""
        master = self._master()
        if master is None:
            return
        bot = self._chosen_bot()
        if bot is None:
            self.report.setText(NO_BOT_CHOSEN)
            return
        self._start(WORKING)
        self._run(
            lambda: self._seam.remove(master, bot),
            self._dismissed,
        )

    @Slot(object)
    def _dismissed(self, result: object) -> None:
        self._done()
        self.report.setText(cast(party.Dismissal, result).sentence)
        self._read(note=None)
        self.party_changed.emit()

    @Slot(object)
    def _failed(self, exc: object) -> None:
        """A seam that raised. One sentence, and the panel stays usable."""
        self._done()
        logger.warning(f"My Party could not ask the server: {type(exc).__name__}: {exc}")
        self.report.setText(f"My Party could not ask the server: {exc}")
        self.party_changed.emit()

    # -- the small print -----------------------------------------------------

    def _master(self) -> str | None:
        """The name in the box, or `None` with the refusal already on screen."""
        if self._busy:
            return None
        master = self.character.text().strip()
        if not master:
            self.report.setText(NO_CHARACTER)
            return None
        return master

    def _chosen_bot(self) -> str | None:
        item = self.member_list.currentItem()
        if item is None:
            return None
        # The row's own text is "<name> — level …"; the name is what the seam
        # takes, and it is read back off the row rather than kept in a parallel
        # list that could drift out of step with what is on screen.
        return item.text().split(" — ")[0]

    @Slot(int)
    def _member_chosen(self, row: int) -> None:
        """Name the bot on the button, and disable it where there is none."""
        bot = self._chosen_bot() if row >= 0 else None
        self.dismiss_button.setText(DISMISS_NOTHING if bot is None else f"Dismiss {bot}")
        self.dismiss_button.setEnabled(bot is not None and not self._busy)

    def _run(self, work: Callable[[], object], on_done: Callable[[object], None]) -> None:
        self._jobs(work, on_done, self._failed)

    def _start(self, note: str | None) -> None:
        self._busy = True
        if note is not None:
            self.report.setText(note)
        self._arm(False)

    def _done(self) -> None:
        self._busy = False
        self._arm(True)

    def _arm(self, on: bool) -> None:
        self.refresh_button.setEnabled(on)
        self.add_button.setEnabled(on)
        self.dismiss_button.setEnabled(on and self.member_list.currentItem() is not None)
