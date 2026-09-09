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
* **No presets.** `party.py` has no preset store and the prior art's is a
  directory of files under `~/.dml` (`90-main.sh:4152-4230`), which is a feature
  rather than a control.

The spec, the level and "dismiss all" were the other three entries in that list
until T5 (2026-09-09), and each is now the seam's own method rather than a
second route: `specs()` reads this install's `playerbots.conf`, the level goes
through 8.4a's `set_level`, and `remove_all()` is `remove` per bot with each one
named.

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
    QSpinBox,
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

    def specs(self, klass: str) -> tuple[str, ...]: ...

    def max_level(self) -> int | None: ...

    def add(
        self,
        master: str,
        klass: str,
        *,
        gender: str = "",
        spec: str = "",
        level: int | None = None,
    ) -> party.Addition: ...

    def remove(self, master: str, bot: str) -> party.Dismissal: ...

    def remove_all(self, master: str) -> party.MassDismissal: ...


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

SPEC_AUTO = "let the server pick"
"""The first row of the spec picker, and the behaviour every add has had so far.

It carries `""` as its data, which is `party.add_bot`'s own word for "send
`talents autopick`". A picker whose only rows were real specs would be a control
with no way back to what the button did yesterday."""

LEVEL_AS_MADE = "leave it as the server made it"
"""The bottom of the level box, shown instead of the number 0.

A spin box has to hold some value and "no level was chosen" has to be one of
them, so the range starts below level 1 and wears these words there
(`QSpinBox.setSpecialValueText`). The alternative -- starting at 1 -- sends
level 1 for a box nobody touched, which resets every bot it is used on."""

NO_LEVEL_HERE = (
    f"No level can be chosen here: this server's own {party.MAX_LEVEL_KEY} could not be read out "
    f"of {party.WORLD_CONF}, so there is nothing to bound the box by. Setting that key and "
    "pressing Show this character's party again offers it."
)
"""The Characters tab's rule for a control this install cannot have (8.4d): not a
disabled box with no explanation, and not an empty space, but the sentence naming
what to fix. The key and the file are `party`'s own constants so this line cannot
name a path the reader does not read."""

DISMISS_ALL_IDLE = "Dismiss every bot…"
"""One button, two labels, and the armed one carries the count.

The tab's own gesture (`controller_view.REMOVE_IDLE`/`REMOVE_ARMED`) rather than
a second kind of confirmation: a user who has learned that pressing once only
arms is not surprised here. A modal would be the other option and it is the wrong
one twice over -- this panel's answers arrive from a worker thread, and the
prior art's own dismiss-all is a two-step in the page
(`Playerbots.svelte:204-220`)."""

NO_BOTS_TO_DISMISS = (
    "This character's party has no bots in it, so there is nothing to dismiss. Nothing was sent."
)
"""Refused here, and it never arms.

The same reason the empty character box is refused here: it costs a database read
and a `docker exec` to be told what the list on screen already says -- and arming
a button over an empty party is asking somebody to confirm nothing."""

DISMISS_ALL_CANCELLED = "Dismiss every bot was cancelled. Nothing was sent."
"""Said when a read stands an armed dismiss-all down.

It has to be SAID: a read of its own clears the report line, so without this an
armed press followed by Show this character's party would leave a disarmed button
and nothing anywhere saying the press had been dropped."""

DISMISS_ALL_MOVED = (
    "Dismiss every bot was not sent: the character or the party is not the one that was "
    "confirmed. Nothing was sent -- press it again to confirm what is on screen now."
)
"""The subject moved between the two presses, so the confirmation is void.

Round 1's finding, from both reviewers, and it is the whole reason an arm carries
a subject rather than a flag: armed on Pakka's two bots, a person who then types
another name into the character box and presses again used to send
`remove_all("Anmi")` -- a party that had never been shown, counted, or named in
the sentence they agreed to. The second press now requires the SAME normalised
master and the SAME bots the first press named, and anything else stands the arm
down with this line instead of firing."""

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
        self._after_read = ""
        # What an armed "dismiss every bot" is ABOUT: the normalised master and
        # the bots the arming sentence named, snapshotted at the first press.
        # `None` is disarmed. A bare flag was round 1's defect -- it armed on one
        # party and fired on whatever the box said later.
        self._confirmed: tuple[str, tuple[str, ...]] | None = None
        # Why the last arm was stood down, said on the NEXT press and then
        # forgotten. Empty for the ways a person cancels ON PURPOSE (the read,
        # or moving on to another button), which need no explanation and no
        # third press; set only where the subject changed underneath them, so
        # that press explains itself and leaves the button idle rather than
        # quietly starting a new confirmation about a different party.
        self._stood_down = ""
        # Which spec request the picker is currently showing. Bumped per
        # request, checked at completion: an answer older than the newest
        # request is about a class the box no longer shows.
        self._spec_generation = 0

        self.character = QLineEdit(self)
        self.character.setPlaceholderText("the character you are playing, logged in")
        self.character.setMaxLength(party.MAX_NAME)
        self.character.returnPressed.connect(self.refresh_party)
        # Typing is not pressing, but it changes what a confirmed dismissal
        # would be about, so it stands one down. Every edit, not just Enter.
        self.character.textChanged.connect(self._character_edited)
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
        self.spec = QComboBox(self)
        self.level = QSpinBox(self)
        self.level_absent = QLabel("", self)
        self.level_absent.setWordWrap(True)
        # Connected AFTER `spec` exists and after the class list was filled:
        # `addItems` moves the current index, and a handler wired before either
        # would read a spec box that has not been built yet.
        self.klass.currentTextChanged.connect(self._class_chosen)
        self.add_button = QPushButton("Add a bot", self)
        self.add_button.clicked.connect(self.add_bot)

        self.member_list = QListWidget(self)
        self.member_list.currentRowChanged.connect(self._member_chosen)
        self.dismiss_button = QPushButton(DISMISS_NOTHING, self)
        self.dismiss_button.clicked.connect(self.dismiss_bot)
        self.dismiss_all_button = QPushButton(DISMISS_ALL_IDLE, self)
        self.dismiss_all_button.clicked.connect(self.dismiss_all)

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
        pick.addWidget(QLabel("specced", self))
        pick.addWidget(self.spec)
        pick.addWidget(QLabel("at level", self))
        pick.addWidget(self.level)
        pick.addWidget(self.add_button)
        box = QVBoxLayout(self)
        box.addLayout(who)
        box.addLayout(pick)
        box.addWidget(self.level_absent)
        box.addWidget(QLabel("In the party now", self))
        box.addWidget(self.summary)
        box.addWidget(self.member_list)
        box.addWidget(self.dismiss_button)
        box.addWidget(self.dismiss_all_button)
        box.addWidget(QLabel("What My Party needs", self))
        box.addWidget(self.check_list)
        box.addWidget(self.report)
        self._member_chosen(-1)
        # Both are per-install readings and both are read again on every
        # "Show this character's party": a game installed, a conf edited or a
        # module deployed while this tab is open changes both answers, and this
        # module's own rule is that facts are re-read per press rather than
        # cached at start-up.
        self._load_level_bound()
        self._load_specs()

    # -- reads ---------------------------------------------------------------

    @Slot()
    def refresh_party(self) -> None:
        """Read this character's party, off the GUI thread. A press of its own.

        It is also the way out of an armed "dismiss every bot", which is what
        the tab's own armed paragraph tells people ("Press Refresh to cancel").

        The busy check is here as well as in `_master()` because this press
        starts three jobs and only one of them goes through that: a refresh
        while an add is polling would otherwise re-read the spec list and the
        level bound under it, which is work nobody asked for and a spec picker
        rebuilt mid-press.
        """
        if self._busy:
            return
        self._after_read = DISMISS_ALL_CANCELLED if self._stand_down("") else ""
        self._load_level_bound()
        self._load_specs()
        self._read(note=WORKING)

    def _load_specs(self) -> None:
        """Re-ask the seam which specs this install has for the chosen class.

        The answer carries the request that asked for it. Round 1's finding
        (Codex): each class change starts an independent read, the completions
        arrive in whatever order the runner finishes them, and a slower mage read
        landing after a druid read filled the DRUID picker with mage specs — so
        the next Add would send a spec `_spec_refusal` refuses, or worse, one
        this tree's module answers "not found" to in the game window where
        nothing can hear it.
        """
        self._spec_generation += 1
        asked = self._spec_generation
        klass = self.klass.currentText()
        self._read_alongside(lambda: (asked, klass, self._seam.specs(klass)), self._specs_read)

    @Slot(object)
    def _specs_read(self, result: object) -> None:
        """Fill the picker, keeping the chosen spec if the new class has it too.

        A completion older than the newest request is DROPPED rather than drawn:
        the generation is the panel's own counter, so "older" means "another
        request has been made since", which is exactly when this answer is about
        a class the box no longer shows.

        `_done()` is deliberately NOT called here: this is a read that rides
        along beside a press rather than one, and re-arming the buttons under an
        add that is still polling would let a second one start.
        """
        generation, klass, offered = cast("tuple[int, str, tuple[str, ...]]", result)
        if generation != self._spec_generation:
            logger.info(f"dropped a stale spec reading for {klass}: {len(offered)} names")
            return
        wanted = self.spec.currentData()
        self.spec.clear()
        self.spec.addItem(SPEC_AUTO, "")
        for name in offered:
            self.spec.addItem(name, name)
        if wanted:
            found = self.spec.findData(wanted)
            self.spec.setCurrentIndex(max(found, 0))

    def _load_level_bound(self) -> None:
        self._read_alongside(self._seam.max_level, self._level_bound_read)

    @Slot(object)
    def _level_bound_read(self, result: object) -> None:
        """Bound the box by this server's own top level, or offer no box at all.

        80 is nowhere in this panel. A cap that could not be read is not a cap of
        80 — the sentence names the key to set, and the box stays out of reach
        until it is, because a level sent against a bound nobody knows is a
        number this app made up.
        """
        top = cast("int | None", result)
        self.level.setSpecialValueText(LEVEL_AS_MADE)
        if top is None:
            self.level.setRange(0, 0)
            self.level.setEnabled(False)
            self.level_absent.setText(NO_LEVEL_HERE)
            self.level_absent.setVisible(True)
            return
        self.level.setRange(0, top)
        self.level.setEnabled(True)
        self.level_absent.setText("")
        self.level_absent.setVisible(False)

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

    @Slot(str)
    def _class_chosen(self, _klass: str) -> None:
        """A different class has a different spec list, so the picker is re-read."""
        self._load_specs()

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
            # Empty for an ordinary read, and the cancellation notice for the
            # one that stood an armed dismiss-all down: that press left nothing
            # else anywhere to say it had been dropped.
            self.report.setText(self._after_read)
            self._after_read = ""
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
        """Add one bot of the chosen class, spec and level to this character's party.

        The spec and the level are both optional and both say so in their own
        control: `SPEC_AUTO` carries `""`, which is the `talents autopick`
        whisper this button has always sent, and the level box's bottom step
        carries `LEVEL_AS_MADE`, which sends no level at all.
        """
        master = self._master()
        if master is None:
            return
        self._stand_down("")
        klass = self.klass.currentText()
        spec = cast("str | None", self.spec.currentData()) or ""
        level = self.level.value() or None
        self._start(WORKING)
        self._run(
            lambda: self._seam.add(master, klass, spec=spec, level=level),
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
    def dismiss_all(self) -> None:
        """Arm on the first press, send every bot away on the second — the SAME one.

        The arm carries its subject: the normalised master and the bots this
        panel named in the sentence a person agreed to. The second press
        recomputes both and fires only where they still match; anything else
        stands the arm down with `DISMISS_ALL_MOVED` and sends nothing. Round
        1's finding is the reason, and it is not theoretical -- armed on Pakka's
        two bots, typing another name and pressing again used to dismiss the
        party of a character who had never been on screen.

        The names are the list the seam last read, and the seam reads the group
        table again for itself when it fires: it is that reading, not this one,
        that decides who is actually sent away. This one decides what was
        CONFIRMED, which is a different question and the only one a person can
        answer.

        No wall-clock expiry, deliberately. The hazard an expiry addresses is an
        arm that goes stale while nobody is looking, and the two identities are
        what "stale" means here -- a clock would be a seam this widget has no
        other use for, and an arm that outlives nothing is still an arm on
        exactly the party named on the button and drawn in the list beneath it.
        """
        master = self._master()
        if master is None:
            return
        if self._stood_down:
            # The subject moved since the confirmation. Say so and stay idle:
            # re-arming here would start a NEW confirmation about a different
            # party off a press that was meant for the old one.
            said, self._stood_down = self._stood_down, ""
            self.report.setText(said)
            return
        count = self.member_list.count()
        if count == 0:
            self._stand_down("")
            self.report.setText(NO_BOTS_TO_DISMISS)
            return
        subject = (master, self._shown_bots())
        if self._confirmed is None:
            self._confirmed = subject
            self.dismiss_all_button.setText(f"Press again to dismiss {party.bots_word(count)}")
            self.report.setText(
                f"This uninvites {party.bots_word(count)} from {master}'s party -- "
                f"{', '.join(subject[1])} -- and whispers each one to log out. Press Show this "
                "character's party to cancel."
            )
            return
        if self._confirmed != subject:
            self._stand_down("")
            self.report.setText(DISMISS_ALL_MOVED)
            return
        self._stand_down("")
        self._start(WORKING)
        self._run(lambda: self._seam.remove_all(master), self._dismissed_all)

    def _shown_bots(self) -> tuple[str, ...]:
        """The bots this panel is showing, by name, in the order they are drawn.

        Read off the rows rather than kept in a list of its own, for
        `_chosen_bot`'s reason: a parallel list is a list that can drift out of
        step with what somebody is looking at, and what is on screen is exactly
        what the confirmation is about.
        """
        return tuple(
            self.member_list.item(row).text().split(" — ")[0]
            for row in range(self.member_list.count())
        )

    @Slot(str)
    def _character_edited(self, _text: str) -> None:
        """A different name in the box is a different party, so any arm goes."""
        if self._stand_down(DISMISS_ALL_MOVED):
            self.report.setText(DISMISS_ALL_MOVED)

    @Slot(object)
    def _dismissed_all(self, result: object) -> None:
        """One sentence naming every bot, then the group table read back."""
        self._done()
        self.report.setText(cast(party.MassDismissal, result).sentence)
        self._read(note=None)
        self.party_changed.emit()

    def _stand_down(self, reason: str) -> bool:
        """Drop any armed dismiss-all, and say whether there was one.

        `reason` is what the NEXT press will say before it refuses to fire —
        empty where the person cancelled on purpose (a read, or another button),
        because that needs no explanation and no third press.
        """
        was_armed = self._confirmed is not None
        self._confirmed = None
        self._stood_down = reason if was_armed else ""
        self.dismiss_all_button.setText(DISMISS_ALL_IDLE)
        return was_armed

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
        # Silently, unlike the read's cancellation: this press has an answer of
        # its own and a cancellation notice over it would throw away the only
        # sentence saying what happened to the bot.
        self._stand_down("")
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
        """A PRESS whose seam raised. One sentence, and the panel stays usable."""
        self._done()
        logger.warning(f"My Party could not ask the server: {type(exc).__name__}: {exc}")
        self.report.setText(f"My Party could not ask the server: {exc}")
        self.party_changed.emit()

    @Slot(object)
    def _side_failed(self, exc: object) -> None:
        """A side READING whose seam raised — the spec list, or the level bound.

        It must NOT call `_done()`. Those two reads ride along beside a press
        rather than being one, and `_done()` re-arms every button: a `specs()`
        that raised while an add was still polling would unlock the buttons
        under it and let a second add start, which is the thing `WORKING` and
        `_busy` exist to prevent. Round 1 flagged it (not blocking) and it is a
        slot rather than a comment because that is what makes it impossible.

        The report line is left alone while a press owns it, for the same
        reason: the press's own sentence is the one a person is waiting for.
        """
        logger.warning(f"My Party could not read this install: {type(exc).__name__}: {exc}")
        if not self._busy:
            self.report.setText(f"My Party could not read this install: {exc}")

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

    def _read_alongside(
        self, work: Callable[[], object], on_done: Callable[[object], None]
    ) -> None:
        """A reading that is not a press: it fails without unlocking the panel."""
        self._jobs(work, on_done, self._side_failed)

    def _start(self, note: str | None) -> None:
        self._busy = True
        if note is not None:
            self.report.setText(note)
        self._arm(False)

    def _done(self) -> None:
        self._busy = False
        self._arm(True)

    def _arm(self, on: bool) -> None:
        """Every control a press would change the meaning of, locked while it runs.

        The three pickers as well as the buttons (round 1, not blocking): the
        class box changing mid-press re-reads the spec list under an add that
        has already sent its class, and a level or spec chosen while the server
        is being asked is one the answer on screen will not be about. The level
        box also obeys its own bound -- re-enabling it here on an install whose
        `MaxPlayerLevel` could not be read would offer a control
        `_level_bound_read` had just withheld.
        """
        self.refresh_button.setEnabled(on)
        self.add_button.setEnabled(on)
        self.dismiss_button.setEnabled(on and self.member_list.currentItem() is not None)
        self.dismiss_all_button.setEnabled(on)
        self.klass.setEnabled(on)
        self.spec.setEnabled(on)
        self.level.setEnabled(on and self.level.maximum() > 0)
