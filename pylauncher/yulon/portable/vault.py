"""T896: the vault of character copies, their History, and the journal of a move.

Every character is a `uid` (T893's random id, the same on every server), and the vault keeps one
folder per `uid` under `<config_dir>/play-together/`. A folder holds the newest copy, the copies
History keeps, `state.json` (who holds the character, its sequence, its lineage, the name used on
each world) and, while a move is open, `journal.json`.

**The copies are plain files, named by the vault's own counter.** `number-kind-holder-stamp`, one
`packages/` folder for every kind: no separate index a crash could split from the copies it
describes, and the order is the counter's, never the clock's -- a copy dated after `now` is not
deleted early because a clock moved.

**The journal is what makes a move repeatable.** Each of the five steps is written before it is
done and again after, and every step's callable is safe to call twice, so a crash at any point is
finished by the next start: a load is never sent twice without asking the world first, a ban sent
twice is one ban, and a visit id makes a second ack a no-op. That is the callers' contract; this
module only guarantees the order and that the record is on disk before the call goes out.

The one thing here that is not a file: the worlds. Dumping a character is a callback, locking its
copy is a callback, and so are the load, the read-back and the start. Nothing in this module
talks to a server.
"""

from __future__ import annotations

import json
import os
import re
import secrets
from collections.abc import Callable
from dataclasses import dataclass, replace
from datetime import UTC, datetime, timedelta
from pathlib import Path
from typing import Literal, cast

from pydantic import BaseModel, ConfigDict, Field, ValidationError

from yulon import platform
from yulon.log import get_logger
from yulon.portable import identity, lease
from yulon.portable.package import (
    MAX_PARENTS,
    SUFFIX,
    CharacterPackageError,
    Package,
    read_package,
)
from yulon.said import SaidByYulon

logger = get_logger(__name__)

DIR_NAME = "play-together"
"""`<config_dir>/play-together`: this app's own folder, nothing a server reads."""

PACKAGES_DIR = "packages"
"""Every kind of copy in one folder: kind, world and time are in the name."""

STATE_FILE = "state.json"
JOURNAL_FILE = "journal.json"
PARTIAL = ".partial"
"""A store cut short: `<copy name>.<16 hex>.partial`, removed by `prune`."""

FORMAT = 1
"""The record format this build writes, and the newest it can read."""

DIR_MODE = 0o700
FILE_MODE = 0o600
KEEP_STOP_COPIES = 5
KEEP_DAYS = 30
VISIT_COPIES_DEFAULT = 5
HOME_KEY = "home"
"""The vault's word for the home install, in a copy's name and in `names`."""

CopyKind = Literal["stop", "before-visit", "visit", "before-merge"]
HolderKind = Literal["home", "friend", "vault"]
StepName = Literal["dump", "lock", "load", "confirm", "start"]
StepState = Literal["pending", "started", "done", "skipped"]
Answer = Literal["loaded", "failed", "absent", "unknown"]
Outcome = Literal["done", "rolled_back", "waiting", "old_holder_unreachable"]

STEPS: tuple[StepName, ...] = ("dump", "lock", "load", "confirm", "start")
"""The five journaled steps of a move, in order. The read-back is a step of its own (9a row 1)."""

Checker = Callable[[Package], None]
"""Proves who signed a package, or raises `identity.Refused`.

Built over `identity.check_package` by the caller, so the vault never trusts a signature itself.
"""

_UID = re.compile(r"[0-9a-f]{32}")
_HEX32 = re.compile(r"[0-9a-f]{32}")
_NAME = re.compile(
    r"(\d{6,})-(stop|before-visit|visit|before-merge)-(home|[0-9a-f]{32})-(\d{8}T\d{6}Z)\.ylchar"
)
_STAMP = "%Y%m%dT%H%M%SZ"

_NEWER_FORMAT = "This character's record was written by a newer Yu'lon. Update Yu'lon to use it."


# --------------------------------------------------------------- what it says


class VaultError(Exception):
    """Something about the vault could not be done, with a sentence for the player."""

    def __init__(
        self,
        reason: Literal["record_damaged", "newer_format", "move_open", "no_copy"],
        sentence: str,
    ) -> None:
        self.reason = reason
        super().__init__(sentence)


class StepRefused(SaidByYulon):
    """A step that CERTAINLY did nothing, with the sentence that says why."""


# ------------------------------------------------------------------- the data


@dataclass(frozen=True)
class Holder:
    """Which world holds the character, and the visit id while it is a friend's."""

    kind: HolderKind
    world: str | None = None
    visit_id: str | None = None

    def __post_init__(self) -> None:
        if self.kind == "friend":
            if _HEX32.fullmatch(self.world or "") is None:
                raise ValueError("a friend holder names a world as 32 hex digits")
            if _HEX32.fullmatch(self.visit_id or "") is None:
                raise ValueError("a friend holder names a visit id as 32 hex digits")
        elif self.world is not None or self.visit_id is not None:
            raise ValueError("only a friend holder names a world and a visit")

    @property
    def key(self) -> str:
        """`names`' key: `home` at home, the world's fingerprint on a friend's world."""
        if self.kind == "home":
            return HOME_KEY
        if self.kind == "friend":
            assert self.world is not None
            return self.world
        raise ValueError("the vault holds no world, so it has no name there")


@dataclass(frozen=True)
class StoredCopy:
    """One `.ylchar` file in `packages/`, read from its name."""

    file: str
    number: int
    kind: CopyKind
    world: str | None
    stored_at: datetime
    path: Path


@dataclass(frozen=True)
class CharacterState:
    """`state.json`: everything the vault decides from, and nothing a package repeats."""

    uid: str
    name: str
    holder: Holder
    sequence: int
    parents: tuple[str, ...]
    newest: str | None
    newest_digest: str | None
    last_number: int
    names: tuple[tuple[str, str], ...]
    revoked: tuple[Holder, ...]


@dataclass(frozen=True)
class Fallback:
    """The newest copy that reads and signs clean, and why the others were passed over."""

    copy: StoredCopy | None
    package: Package | None
    refused: tuple[tuple[str, str], ...]
    sentence: str | None


@dataclass(frozen=True)
class Confirmed:
    """What the world answered when asked whether the character arrived."""

    answer: Answer
    name: str | None = None
    """The name it was read back under, when it arrived."""


@dataclass(frozen=True)
class StepContext:
    """What one journaled step is told; `rerun` means an earlier run started it and was cut off."""

    move_id: str
    uid: str
    name: str
    source: Holder
    target: Holder
    package: Path | None
    rerun: bool


@dataclass(frozen=True)
class MoveSteps:
    """The worlds, as callables. Each is safe to call twice; `load` only after a `confirm`."""

    dump: Callable[[StepContext], Path | None]
    lock: Callable[[StepContext], None]
    load: Callable[[StepContext], None]
    start: Callable[[StepContext], None]
    unlock: Callable[[StepContext], None]
    confirm: Callable[[StepContext], Confirmed]


@dataclass(frozen=True)
class MoveResult:
    outcome: Outcome
    holder: Holder
    sentence: str | None = None
    load_failed: bool = False
    newest_at: datetime | None = None


# ---------------------------------------------------------------- the records


class _HolderRecord(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True)

    kind: HolderKind
    world: str | None = None
    visit_id: str | None = None


class _StateRecord(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True)

    format: int
    uid: str
    name: str
    holder: _HolderRecord
    sequence: int
    parents: tuple[str, ...] = Field(max_length=MAX_PARENTS)
    newest: str | None = None
    newest_digest: str | None = None
    last_number: int
    names: dict[str, str] = Field(default_factory=dict)
    revoked: tuple[_HolderRecord, ...] = ()


class _JournalRecord(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True)

    format: int
    move_id: str
    uid: str
    name: str
    source: _HolderRecord
    target: _HolderRecord
    from_vault: bool
    last_number_before: int
    package: str | None = None
    rolling_back: bool
    steps: dict[StepName, StepState]


@dataclass(frozen=True)
class _Journal:
    move_id: str
    uid: str
    name: str
    source: Holder
    target: Holder
    from_vault: bool
    last_number_before: int
    package: str | None
    rolling_back: bool
    steps: dict[StepName, StepState]


def _json_bytes(value: object) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True, ensure_ascii=False) + "\n").encode("utf-8")


def _holder_obj(holder: Holder) -> dict[str, object]:
    return {"kind": holder.kind, "world": holder.world, "visit_id": holder.visit_id}


def _holder(record: _HolderRecord) -> Holder:
    return Holder(kind=record.kind, world=record.world, visit_id=record.visit_id)


def _state_obj(state: CharacterState) -> dict[str, object]:
    return {
        "format": FORMAT,
        "uid": state.uid,
        "name": state.name,
        "holder": _holder_obj(state.holder),
        "sequence": state.sequence,
        "parents": list(state.parents),
        "newest": state.newest,
        "newest_digest": state.newest_digest,
        "last_number": state.last_number,
        "names": dict(state.names),
        "revoked": [_holder_obj(holder) for holder in state.revoked],
    }


def _journal_obj(journal: _Journal) -> dict[str, object]:
    return {
        "format": FORMAT,
        "move_id": journal.move_id,
        "uid": journal.uid,
        "name": journal.name,
        "source": _holder_obj(journal.source),
        "target": _holder_obj(journal.target),
        "from_vault": journal.from_vault,
        "last_number_before": journal.last_number_before,
        "package": journal.package,
        "rolling_back": journal.rolling_back,
        "steps": dict(journal.steps),
    }


def _mark(path: Path, journal: _Journal, step: StepName, state: StepState) -> _Journal:
    """Write the whole journal with `step` at `state`, and return it.

    The one write of a journal, so every change to a move's record goes through a function a
    test can patch -- a crash at any mark is the crash the player would have had.
    """
    marked = replace(journal, steps={**journal.steps, step: state})
    platform.write_private_atomically(path, _json_bytes(_journal_obj(marked)))
    return marked


def _record_damaged(path: Path, folder: Path) -> VaultError:
    return VaultError(
        "record_damaged",
        f"Yu'lon cannot read its record of this character ({path}), so it changed nothing. "
        f"The copies are still in {folder}.",
    )


def _name(state: CharacterState | None) -> str:
    return state.name if state is not None else "this character"


def _none_good(name: str) -> str:
    return f"Every copy of {name} in Yu'lon's vault is damaged, so there is none to offer."


def _stored_at(stamp: str) -> datetime:
    return datetime.strptime(stamp, _STAMP).replace(tzinfo=UTC)


def _in_flight(journal: _Journal) -> tuple[StepName, StepState]:
    """The step a rollback interrupts: the last one started and not done, else the last one."""
    for step in reversed(STEPS):
        if journal.steps[step] == "started":
            return step, "started"
    last = STEPS[-1]
    return last, journal.steps[last]


# -------------------------------------------------------------------- the vault


class Vault:
    """One player's copies of every character, and the journal of the move in flight."""

    def __init__(
        self,
        config_dir: Path | None = None,
        *,
        checker: Checker | None = None,
        visit_copies_per_world: int = VISIT_COPIES_DEFAULT,
        now: Callable[[], datetime] = lambda: datetime.now(UTC),
    ) -> None:
        if not 1 <= visit_copies_per_world <= 50:
            raise ValueError("the visit copy count is 1 to 50")
        self.root = (config_dir or platform.config_dir()) / DIR_NAME
        self._checker = checker
        self._visit_copies = visit_copies_per_world
        self._now = now

    # -- what is there ------------------------------------------------------

    def character_dir(self, uid: str) -> Path:
        """The one folder of `uid`. Anything else than 32 hex digits is a ValueError, not a path."""
        if _UID.fullmatch(uid) is None:
            raise ValueError(f"{uid!r} is not a character id")
        return self.root / uid

    def characters(self) -> tuple[str, ...]:
        """The `uid`s with a folder here, sorted. A missing root answers none."""
        found = [
            entry.name
            for entry in self._entries(self.root)
            if _UID.fullmatch(entry.name) is not None and _real_dir(entry)
        ]
        return tuple(sorted(found))

    def state(self, uid: str) -> CharacterState | None:
        """`state.json`, or None when this character has never been stored."""
        folder = self.character_dir(uid)
        path = folder / STATE_FILE
        try:
            raw = path.read_bytes()
        except FileNotFoundError:
            return None
        except OSError as exc:
            raise _record_damaged(path, folder) from exc
        try:
            obj = json.loads(raw)
        except ValueError as exc:
            raise _record_damaged(path, folder) from exc
        if not isinstance(obj, dict):
            raise _record_damaged(path, folder)
        version = obj.get("format")
        if isinstance(version, int) and not isinstance(version, bool) and version > FORMAT:
            raise VaultError("newer_format", _NEWER_FORMAT)
        try:
            record = _StateRecord.model_validate(obj)
            return CharacterState(
                uid=record.uid,
                name=record.name,
                holder=_holder(record.holder),
                sequence=record.sequence,
                parents=tuple(record.parents),
                newest=record.newest,
                newest_digest=record.newest_digest,
                last_number=record.last_number,
                names=tuple(sorted(record.names.items())),
                revoked=tuple(_holder(one) for one in record.revoked),
            )
        except (ValidationError, ValueError) as exc:
            raise _record_damaged(path, folder) from exc

    def copies(self, uid: str) -> tuple[StoredCopy, ...]:
        """Every copy in `packages/`, by the vault's own counter."""
        folder = self.character_dir(uid) / PACKAGES_DIR
        found: list[StoredCopy] = []
        for entry in self._entries(folder):
            match = _NAME.fullmatch(entry.name)
            if match is None or not _real_file(entry):
                continue
            number, kind, holder, stamp = match.groups()
            try:
                stored_at = _stored_at(stamp)
            except ValueError:
                logger.warning(f"{entry.name}: a copy whose name carries no time Yu'lon can read")
                continue
            found.append(
                StoredCopy(
                    file=entry.name,
                    number=int(number),
                    kind=cast(CopyKind, kind),
                    world=None if holder == HOME_KEY else holder,
                    stored_at=stored_at,
                    path=Path(entry.path),
                )
            )
        return tuple(sorted(found, key=lambda copy: copy.number))

    @staticmethod
    def _entries(folder: Path) -> list[os.DirEntry[str]]:
        """The one folder listing: a folder that cannot be listed answers no entries."""
        try:
            with os.scandir(folder) as scan:
                return list(scan)
        except OSError:
            return []

    # -- storing ------------------------------------------------------------

    def store(self, source: Path, kind: CopyKind, *, world: str | None = None) -> StoredCopy:
        """Keep `source` as a copy of this character, or answer the one already kept.

        A package that reads or signs badly is refused before anything is written, and the same
        package stored twice is one copy: a rerun of a dump must not grow the History.
        """
        if kind == "visit":
            if world is None or _HEX32.fullmatch(world) is None:
                raise ValueError("a visit copy names the world it was made on, as 32 hex digits")
        elif world is not None:
            raise ValueError("only a visit copy names a world")
        package = read_package(source)
        self._check(package)
        uid = package.manifest.character.uid
        folder = self.character_dir(uid)
        self.root.mkdir(mode=DIR_MODE, parents=True, exist_ok=True)
        with lease.hold(folder, "A copy"):
            state = self.state(uid)
            if state is None:
                state = CharacterState(
                    uid=uid,
                    name=package.manifest.character.name,
                    holder=Holder("home"),
                    sequence=0,
                    parents=(),
                    newest=None,
                    newest_digest=None,
                    last_number=0,
                    names=(),
                    revoked=(),
                )
            if state.newest_digest == package.digest:
                kept = next((copy for copy in self.copies(uid) if copy.file == state.newest), None)
                if kept is not None:
                    return kept
            number = state.last_number + 1
            if kind == "visit":
                assert world is not None
                held = world
            else:
                held = HOME_KEY
            stamp = f"{self._now().astimezone(UTC):{_STAMP}}"
            name = f"{number:06d}-{kind}-{held}-{stamp}{SUFFIX}"
            packages = folder / PACKAGES_DIR
            packages.mkdir(mode=DIR_MODE, parents=True, exist_ok=True)
            final = packages / name
            self._store_copy(source, final)
            self._write_state(folder, self._with_best(state, package, name, number))
            self.prune(uid)
            return StoredCopy(
                file=name,
                number=number,
                kind=kind,
                world=None if held == HOME_KEY else held,
                stored_at=_stored_at(stamp),
                path=final,
            )

    def _store_copy(self, source: Path, final: Path) -> None:
        """Write the copy beside its final name, prove it reads, and only then name it."""
        partial = final.with_name(f"{final.name}.{secrets.token_hex(8)}{PARTIAL}")
        fd = os.open(
            partial,
            os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_BINARY", 0),
            FILE_MODE,
        )
        try:
            with os.fdopen(fd, "wb") as handle:
                handle.write(source.read_bytes())
                handle.flush()
                os.fsync(handle.fileno())
            read_package(partial)
            os.replace(partial, final)
            platform.fsync_folder(final.parent)
        except BaseException:
            partial.unlink(missing_ok=True)
            raise

    def _write_state(self, folder: Path, state: CharacterState) -> None:
        platform.write_private_atomically(folder / STATE_FILE, _json_bytes(_state_obj(state)))

    def _with_best(
        self, state: CharacterState, package: Package, name: str, number: int
    ) -> CharacterState:
        """The state after storing `package`: the newest is the highest sequence, not the clock."""
        sequence = package.manifest.sequence
        digest = package.digest
        if sequence > state.sequence:
            return replace(
                state,
                name=package.manifest.character.name,
                sequence=sequence,
                parents=(digest, *package.manifest.parents)[:MAX_PARENTS],
                newest=name,
                newest_digest=digest,
                last_number=number,
            )
        if sequence == state.sequence and digest != state.newest_digest:
            logger.warning(
                f"{package.manifest.character.name}: two copies claim sequence {sequence}; "
                f"{name} is kept in History only"
            )
        return replace(state, last_number=number)

    # -- pruning ------------------------------------------------------------

    def prune(self, uid: str) -> tuple[str, ...]:
        """Remove the copies their keep rule has passed, and the leftovers of cut-short stores."""
        folder = self.character_dir(uid)
        self.root.mkdir(mode=DIR_MODE, parents=True, exist_ok=True)
        with lease.hold(folder, "A copy"):
            now = self._now().astimezone(UTC)
            packages = folder / PACKAGES_DIR
            removed: list[str] = []
            for entry in self._entries(packages):
                if entry.name.endswith(PARTIAL) and _real_file(entry):
                    path = Path(entry.path)
                    path.unlink(missing_ok=True)
                    removed.append(path.name)
            try:
                state = self.state(uid)
                journal = self._journal(folder)
            except VaultError:
                logger.warning(
                    f"{uid}: this character's record could not be read, so no copy it might "
                    "name was removed"
                )
                return tuple(removed)
            protected: set[str] = set()
            if state is not None and state.newest is not None:
                protected.add(state.newest)
            if journal is not None and journal.package is not None:
                protected.add(journal.package)
            copies = list(self.copies(uid))
            ahead = [copy for copy in copies if copy.stored_at - now > timedelta(days=1)]
            if ahead:
                logger.warning(
                    f"{uid}: {len(ahead)} cop{'y is' if len(ahead) == 1 else 'ies are'} dated "
                    "after this computer's clock; the vault's own counter keeps the order"
                )
            doomed: set[str] = set()
            stops = sorted(
                (copy for copy in copies if copy.kind == "stop"),
                key=lambda copy: copy.number,
                reverse=True,
            )
            doomed.update(copy.file for copy in stops[KEEP_STOP_COPIES:])
            for world in sorted({copy.world or "" for copy in copies if copy.kind == "visit"}):
                visits = sorted(
                    (
                        copy
                        for copy in copies
                        if copy.kind == "visit" and (copy.world or "") == world
                    ),
                    key=lambda copy: copy.number,
                    reverse=True,
                )
                doomed.update(copy.file for copy in visits[self._visit_copies :])
            for copy in copies:
                if copy.kind in ("before-visit", "before-merge"):
                    if max(now - copy.stored_at, timedelta(0)) >= timedelta(days=KEEP_DAYS):
                        doomed.add(copy.file)
            for file in sorted(doomed - protected):
                path = packages / file
                path.unlink(missing_ok=True)
                removed.append(path.name)
            return tuple(removed)

    # -- reading a copy back ------------------------------------------------

    def newest_good(self, uid: str) -> Fallback:
        """The newest copy that reads and signs clean; the copies before it are the fallback."""
        state = self.state(uid)
        name = _name(state)
        copies = self.copies(uid)
        if not copies:
            return Fallback(copy=None, package=None, refused=(), sentence=_none_good(name))
        by_file = {copy.file: copy for copy in copies}
        order: list[StoredCopy] = []
        if state is not None and state.newest in by_file:
            order.append(by_file[state.newest])
        order.extend(
            sorted(
                (copy for copy in copies if copy not in order),
                key=lambda copy: copy.number,
                reverse=True,
            )
        )
        refused: list[tuple[str, str]] = []
        for copy in order:
            package: Package | None = None
            why: str | None = None
            try:
                package = read_package(copy.path)
            except CharacterPackageError as exc:
                why = str(exc)
            else:
                if package.manifest.character.uid != uid:
                    why = "it belongs to another character"
                elif self._checker is not None and not package.signed:
                    why = "it is not signed"
                elif self._checker is not None:
                    try:
                        self._checker(package)
                    except identity.Refused as exc:
                        why = f"its signature does not hold ({exc.reason})"
            if why is None:
                assert package is not None
                sentence = None
                if refused:
                    sentence = (
                        f"The newest copy of {name} is damaged, so Yu'lon will not use it. "
                        f"The copy from {copy.stored_at:%Y-%m-%d %H:%M} UTC is offered instead."
                    )
                return Fallback(
                    copy=copy, package=package, refused=tuple(refused), sentence=sentence
                )
            refused.append((copy.file, why))
        return Fallback(copy=None, package=None, refused=tuple(refused), sentence=_none_good(name))

    def _check(self, package: Package) -> None:
        if self._checker is None:
            return
        if not package.signed:
            raise identity.Refused("format", "unsigned")
        self._checker(package)

    # -- the move -----------------------------------------------------------

    def move(
        self, uid: str, target: Holder, steps: MoveSteps, *, from_vault: bool = False
    ) -> MoveResult:
        """Move the character to `target`, finishing any move an earlier run left open."""
        folder = self.character_dir(uid)
        self.root.mkdir(mode=DIR_MODE, parents=True, exist_ok=True)
        with lease.hold(folder, "A move"):
            state = self.state(uid)
            if target.kind == "vault":
                raise ValueError("a move never moves a character into the vault")
            if from_vault and (state is None or state.holder.kind != "friend"):
                raise ValueError("continuing from the vault revokes a friend's copy only")
            if state is None:
                raise VaultError("no_copy", _none_good("this character"))
            name = _name(state)
            journal = self._journal(folder)
            if journal is not None:
                if journal.target != target:
                    raise VaultError(
                        "move_open",
                        f"A move of {name} did not finish. Yu'lon finishes that one first.",
                    )
                return self._run(folder, journal, steps)
            if target == state.holder:
                context = StepContext(
                    move_id=secrets.token_hex(16),
                    uid=uid,
                    name=name,
                    source=state.holder,
                    target=target,
                    package=self._newest_copy(folder, state),
                    rerun=False,
                )
                steps.start(context)
                return MoveResult("done", state.holder, newest_at=self._newest_at(uid))
            journal = _Journal(
                move_id=secrets.token_hex(16),
                uid=uid,
                name=name,
                source=state.holder,
                target=target,
                from_vault=from_vault,
                last_number_before=state.last_number,
                package=None,
                rolling_back=False,
                steps={step: "pending" for step in STEPS},
            )
            journal = _mark(folder / JOURNAL_FILE, journal, STEPS[0], "pending")
            return self._run(folder, journal, steps)

    def resume(self, uid: str, steps: MoveSteps) -> MoveResult | None:
        """Finish the move an earlier run left open, or None when there is none."""
        folder = self.character_dir(uid)
        self.root.mkdir(mode=DIR_MODE, parents=True, exist_ok=True)
        with lease.hold(folder, "A move"):
            journal = self._journal(folder)
            if journal is None:
                return None
            return self._run(folder, journal, steps)

    def _run(self, folder: Path, journal: _Journal, steps: MoveSteps) -> MoveResult:
        path = folder / JOURNAL_FILE
        if journal.rolling_back:
            return self._rollback(folder, journal, steps, None)

        if journal.steps["dump"] in ("pending", "started"):
            rerun = journal.steps["dump"] == "started"
            journal = _mark(path, journal, "dump", "started")
            if journal.from_vault or journal.source.kind == "vault":
                fallback = self.newest_good(journal.uid)
                if fallback.copy is None:
                    raise VaultError("no_copy", fallback.sentence or _none_good(journal.name))
                if journal.from_vault and journal.source.kind == "friend":
                    self._revoke(folder, journal)
                journal = _mark(
                    path, replace(journal, package=fallback.copy.file), "dump", "skipped"
                )
            else:
                adopted = self._adopted(journal) if rerun else None
                if adopted is not None:
                    journal = _mark(path, replace(journal, package=adopted), "dump", "done")
                else:
                    context = self._context(folder, journal, rerun)
                    try:
                        got = steps.dump(context)
                    except StepRefused as exc:
                        return self._rollback(folder, journal, steps, str(exc))
                    if got is None:
                        kept = self.newest_good(journal.uid).copy
                        self._end_journal(path)
                        return MoveResult(
                            "old_holder_unreachable",
                            journal.source,
                            newest_at=None if kept is None else kept.stored_at,
                        )
                    handed = read_package(got)
                    if handed.manifest.character.uid != journal.uid:
                        return self._rollback(
                            folder,
                            journal,
                            steps,
                            "The world handed back another character, so nothing was moved.",
                        )
                    kind: CopyKind = "before-visit" if journal.source.kind == "home" else "visit"
                    world = journal.source.world if journal.source.kind == "friend" else None
                    stored = self.store(got, kind, world=world)
                    journal = _mark(path, replace(journal, package=stored.file), "dump", "done")

        if journal.steps["dump"] == "skipped" and journal.steps["lock"] == "pending":
            journal = _mark(path, journal, "lock", "skipped")
        if journal.steps["lock"] in ("pending", "started"):
            rerun = journal.steps["lock"] == "started"
            journal = _mark(path, journal, "lock", "started")
            try:
                steps.lock(self._context(folder, journal, rerun))
            except StepRefused as exc:
                return self._rollback(folder, journal, steps, str(exc), locked=False)
            journal = _mark(path, journal, "lock", "done")

        own_load = False
        if journal.steps["load"] == "pending":
            journal = _mark(path, journal, "load", "started")
            try:
                steps.load(self._context(folder, journal, False))
            except StepRefused as exc:
                return self._rollback(folder, journal, steps, str(exc))
            journal = _mark(path, journal, "load", "done")
            own_load = True

        while journal.steps["confirm"] != "done":
            rerun = journal.steps["confirm"] == "started"
            journal = _mark(path, journal, "confirm", "started")
            try:
                confirmed = steps.confirm(self._context(folder, journal, rerun))
            except StepRefused as exc:
                return self._rollback(folder, journal, steps, str(exc))
            if confirmed.answer == "loaded":
                self._accept(folder, journal, confirmed)
                journal = _mark(path, journal, "confirm", "done")
                break
            if confirmed.answer == "failed":
                return self._rollback(folder, journal, steps, None, load_failed=True)
            if confirmed.answer == "unknown":
                current = self.state(journal.uid)
                return MoveResult(
                    "waiting",
                    journal.source if current is None else current.holder,
                    sentence=(
                        f"Yu'lon could not tell whether {journal.name} arrived. "
                        "It asks again at the next start."
                    ),
                )
            if own_load:
                return self._rollback(folder, journal, steps, None, load_failed=True)
            journal = _mark(path, journal, "load", "started")
            try:
                steps.load(self._context(folder, journal, True))
            except StepRefused as exc:
                return self._rollback(folder, journal, steps, str(exc))
            journal = _mark(path, journal, "load", "done")
            own_load = True

        if journal.steps["start"] != "done":
            rerun = journal.steps["start"] == "started"
            journal = _mark(path, journal, "start", "started")
            try:
                steps.start(self._context(folder, journal, rerun))
            except StepRefused as exc:
                return self._rollback(folder, journal, steps, str(exc))
            journal = _mark(path, journal, "start", "done")
        self._end_journal(path)
        self.prune(journal.uid)
        return MoveResult("done", journal.target, newest_at=self._newest_at(journal.uid))

    def _rollback(
        self,
        folder: Path,
        journal: _Journal,
        steps: MoveSteps,
        sentence: str | None,
        *,
        load_failed: bool = False,
        locked: bool | None = None,
    ) -> MoveResult:
        """Undo what the move did: unlock home, or leave the vault holding a friend's character."""
        path = folder / JOURNAL_FILE
        step, state = _in_flight(journal)
        journal = _mark(path, replace(journal, rolling_back=True), step, state)
        applied = journal.steps["lock"] in ("started", "done") if locked is None else locked
        seeded = journal.from_vault and journal.source.kind == "friend"
        current = self.state(journal.uid)
        holder = journal.source if current is None else current.holder
        revoked = () if current is None else current.revoked
        if applied and journal.source.kind == "home":
            steps.unlock(self._context(folder, journal, True))
            holder = Holder("home")
        elif (applied and journal.source.kind == "friend") or seeded:
            holder = Holder("vault")
            if journal.source not in revoked:
                revoked = (*revoked, journal.source)
        if current is not None and (current.holder != holder or current.revoked != revoked):
            self._write_state(folder, replace(current, holder=holder, revoked=revoked))
        self._end_journal(path)
        if load_failed:
            sentence = (
                f"{journal.name} could not be loaded into that world, so the move was undone."
            )
        return MoveResult("rolled_back", holder, sentence=sentence, load_failed=load_failed)

    def _context(self, folder: Path, journal: _Journal, rerun: bool) -> StepContext:
        package = None if journal.package is None else folder / PACKAGES_DIR / journal.package
        return StepContext(
            move_id=journal.move_id,
            uid=journal.uid,
            name=journal.name,
            source=journal.source,
            target=journal.target,
            package=package,
            rerun=rerun,
        )

    def _adopted(self, journal: _Journal) -> str | None:
        """A copy a cut-off dump already stored: a rerun keeps it instead of dumping again."""
        for copy in self.copies(journal.uid):
            if copy.number <= journal.last_number_before:
                continue
            try:
                package = read_package(copy.path)
            except CharacterPackageError:
                continue
            if package.manifest.character.uid != journal.uid:
                continue
            try:
                self._check(package)
            except identity.Refused:
                continue
            return copy.file
        return None

    def _accept(self, folder: Path, journal: _Journal, confirmed: Confirmed) -> None:
        state = self.state(journal.uid)
        if state is None:
            return
        names = dict(state.names)
        names[journal.target.key] = confirmed.name or journal.name
        self._write_state(
            folder,
            replace(
                state,
                holder=journal.target,
                names=tuple(sorted(names.items())),
            ),
        )

    def _revoke(self, folder: Path, journal: _Journal) -> None:
        state = self.state(journal.uid)
        if state is None or journal.source in state.revoked:
            return
        self._write_state(folder, replace(state, revoked=(*state.revoked, journal.source)))

    def _end_journal(self, path: Path) -> None:
        path.unlink(missing_ok=True)

    def _journal(self, folder: Path) -> _Journal | None:
        path = folder / JOURNAL_FILE
        try:
            raw = path.read_bytes()
        except FileNotFoundError:
            return None
        except OSError as exc:
            raise _record_damaged(path, folder) from exc
        try:
            obj = json.loads(raw)
        except ValueError as exc:
            raise _record_damaged(path, folder) from exc
        if not isinstance(obj, dict):
            raise _record_damaged(path, folder)
        version = obj.get("format")
        if isinstance(version, int) and not isinstance(version, bool) and version > FORMAT:
            raise VaultError("newer_format", _NEWER_FORMAT)
        try:
            record = _JournalRecord.model_validate(obj)
            return _Journal(
                move_id=record.move_id,
                uid=record.uid,
                name=record.name,
                source=_holder(record.source),
                target=_holder(record.target),
                from_vault=record.from_vault,
                last_number_before=record.last_number_before,
                package=record.package,
                rolling_back=record.rolling_back,
                steps=dict(record.steps),
            )
        except (ValidationError, ValueError) as exc:
            raise _record_damaged(path, folder) from exc

    def _newest_copy(self, folder: Path, state: CharacterState) -> Path:
        if state.newest is not None:
            named = folder / PACKAGES_DIR / state.newest
            if named.is_file():
                return named
        found = self.copies(state.uid)
        if not found:
            raise VaultError("no_copy", _none_good(_name(state)))
        return found[-1].path

    def _newest_at(self, uid: str) -> datetime | None:
        state = self.state(uid)
        newest: StoredCopy | None = None
        if state is not None and state.newest is not None:
            newest = next((copy for copy in self.copies(uid) if copy.file == state.newest), None)
        if newest is None:
            found = self.copies(uid)
            newest = found[-1] if found else None
        return None if newest is None else newest.stored_at


def _real_dir(entry: os.DirEntry[str]) -> bool:
    try:
        return entry.is_dir(follow_symlinks=False)
    except OSError:
        return False


def _real_file(entry: os.DirEntry[str]) -> bool:
    try:
        return entry.is_file(follow_symlinks=False)
    except OSError:
        return False
