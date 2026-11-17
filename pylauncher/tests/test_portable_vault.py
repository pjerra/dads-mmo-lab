"""T896: the vault of character copies, their History, and the journal of a move.

Pure code, no server: every copy is a real `.ylchar` file made by T893's writer, the clock is a
parameter, and the steps of a move are fakes that count what a world was asked to do. Every
sentence is asserted verbatim, because a sentence that drifts is a sentence the player stops
trusting.
"""

from __future__ import annotations

import itertools
import json
import logging
import stat
import sys
from collections.abc import Callable
from datetime import UTC, datetime, timedelta
from pathlib import Path

import pytest

from tests.test_portable_package import facts as export_facts
from tests.test_portable_package import flip_member_data, small_dump
from tests.test_portable_package import game as a_game
from yulon.portable import vault as vault_module
from yulon.portable.identity import PackageRefused
from yulon.portable.package import (
    SUFFIX,
    CharacterPackageError,
    Package,
    read_package,
    write_package,
)
from yulon.portable.vault import (
    STEPS,
    Confirmed,
    Holder,
    MoveResult,
    MoveSteps,
    StepContext,
    StepRefused,
    StoredCopy,
    Vault,
    VaultError,
)

AT = datetime(2026, 11, 17, 21, 40, tzinfo=UTC)
UID = "0" * 32
OTHER_UID = "1" * 32
NAME = "Priestest"
FRIEND_WORLD = "f" * 32
OTHER_WORLD = "d" * 32
VISIT = "e" * 32
HOME = Holder("home")
FRIEND = Holder("friend", world=FRIEND_WORLD, visit_id=VISIT)


class Clock:
    """A clock a test moves by hand; the vault never reads the real one."""

    def __init__(self, at: datetime = AT) -> None:
        self.at = at

    def __call__(self) -> datetime:
        return self.at

    def advance(self, *, days: int = 0) -> None:
        self.at = self.at + timedelta(days=days)


_MADE = itertools.count(1)


def a_package(
    folder: Path,
    *,
    uid: str = UID,
    sequence: int = 1,
    parents: tuple[str, ...] | None = None,
    game_id: str = "wow-wotlk",
    signed: bool = False,
) -> Path:
    """A real character file: distinct manifests, so distinct copies."""
    lineage = parents if parents is not None else ("a" * 64,) * min(sequence - 1, 20)
    dest = folder / f"character-{next(_MADE)}{SUFFIX}"
    write_package(
        dest,
        export_facts(uid=uid, sequence=sequence, parents=lineage, game=a_game(id=game_id)),
        small_dump(),
        signer=(lambda manifest: b"a signature") if signed else None,
    )
    return dest


def digest_of(path: Path) -> str:
    return read_package(path).digest


def a_vault(base: Path, clock: Clock | None = None, **over: object) -> Vault:
    options: dict[str, object] = {"now": clock or Clock()}
    options.update(over)
    return Vault(base / "cfg", **options)


def state_json(vault: Vault, uid: str = UID) -> dict:
    return json.loads((vault.character_dir(uid) / "state.json").read_text())


def journal_path(vault: Vault, uid: str = UID) -> Path:
    return vault.character_dir(uid) / "journal.json"


def copies_of(vault: Vault, uid: str = UID) -> list[StoredCopy]:
    return list(vault.copies(uid))


# -- the worlds, as fakes that count what a move asked of them ------------------


class Crash(Exception):
    """A raised exception standing in for a Yu'lon that died inside a step."""


class FakeWorlds:
    """The six step callables of a move, with the effects a real world would have."""

    def __init__(self, base: Path, vault: Vault, *, uid: str = UID) -> None:
        self.base = base
        self.made_dir = base / "made"
        self.vault = vault
        self.uid = uid
        self.dumps = 0
        self.dump_sequence = 2
        self.no_dump = False
        self.dump_uid = uid
        self.banned: set[str] = set()
        self.erased: set[str] = set()
        self.loads: dict[str, int] = {}
        self.present: set[tuple[str, str]] = set()
        self.started: set[tuple[str, str]] = set()
        self.unlocked: list[str] = []
        self.events: list[str] = []
        self.checked: list[str] = []
        self.answers: list[str] = []
        self.confirms = 0
        self.crash: tuple[str, str] | None = None
        self.refuse: str | None = None
        self.refuse_start = False
        self.dump_damaged = False
        self.dump_signed = False
        self.journaled = True

    def steps(self) -> MoveSteps:
        return MoveSteps(
            dump=self.dump,
            lock=self.lock,
            load=self.load,
            start=self.start,
            unlock=self.unlock,
            confirm=self.confirm,
        )

    def snapshot(self) -> dict[str, object]:
        """The durable state of every world, for comparing one run with another."""
        return {
            "banned": sorted(self.banned),
            "erased": sorted(self.erased),
            "loads": dict(sorted(self.loads.items())),
            "present": sorted(self.present),
            "started": sorted(self.started),
            "unlocked": list(self.unlocked),
        }

    def _recorded(self, step: str) -> None:
        if not self.journaled:
            return
        written = json.loads(journal_path(self.vault, self.uid).read_text())
        assert written["steps"][step] == "started", (step, written["steps"])
        self.checked.append(step)

    def _crash(self, step: str, when: str = "in_step") -> None:
        if self.crash == (step, when):
            raise Crash(step)

    def dump(self, ctx: StepContext) -> Path | None:
        self._recorded("dump")
        self.dumps += 1
        self.events.append("dump")
        if self.no_dump:
            self._crash("dump")
            return None
        made = a_package(
            self.made_dir,
            uid=self.dump_uid,
            sequence=self.dump_sequence,
            signed=self.dump_signed,
        )
        if self.dump_damaged:
            flip_member_data(made, "character.pdump")
        self._crash("dump")
        return made

    def lock(self, ctx: StepContext) -> None:
        self._recorded("lock")
        self.events.append(f"lock:{ctx.source.kind}")
        if self.refuse == "lock":
            raise StepRefused("Home is already busy, so nothing was moved.")
        if ctx.source.kind == "home":
            self.banned.add(ctx.source.key)
        elif ctx.source.kind == "friend":
            self.erased.add(ctx.source.visit_id or "")
        self._crash("lock")

    def load(self, ctx: StepContext) -> None:
        self._recorded("load")
        if self.crash == ("load", "before_effect"):
            self.events.append("load:lost")
            raise Crash("load")
        key = ctx.target.key
        self.loads[key] = self.loads.get(key, 0) + 1
        self.present.add((key, ctx.name))
        self.events.append(f"load:{key}")
        self._crash("load")

    def confirm(self, ctx: StepContext) -> Confirmed:
        self._recorded("confirm")
        self.confirms += 1
        if self.refuse == "confirm":
            raise StepRefused("The world did not answer, so nothing was moved.")
        if self.answers:
            answer = self.answers.pop(0)
            if answer == "loaded":
                self.present.add((ctx.target.key, ctx.name))
            self.events.append(f"confirm:{answer}")
            self._crash("confirm")
            return Confirmed(answer, ctx.name if answer == "loaded" else None)
        if (ctx.target.key, ctx.name) in self.present:
            self.events.append("confirm:loaded")
            self._crash("confirm")
            return Confirmed("loaded", ctx.name)
        self.events.append("confirm:absent")
        self._crash("confirm")
        return Confirmed("absent")

    def start(self, ctx: StepContext) -> None:
        self._recorded("start")
        self.events.append("start")
        if self.refuse_start:
            raise StepRefused("The world is still busy, so Yu'lon did not start it.")
        self.started.add((ctx.target.key, ctx.name))
        self._crash("start")

    def unlock(self, ctx: StepContext) -> None:
        self.events.append("unlock")
        self.unlocked.append(ctx.source.key)
        self.banned.discard(ctx.source.key)


def begun(base: Path, clock: Clock | None = None) -> tuple[Vault, FakeWorlds]:
    """A character with one copy at home, and the fakes that move it."""
    base.mkdir(parents=True, exist_ok=True)
    worlds = FakeWorlds(base, a_vault(base, clock))
    worlds.made_dir.mkdir(parents=True, exist_ok=True)
    worlds.vault.store(a_package(worlds.made_dir, uid=UID, sequence=1), "stop")
    return worlds.vault, worlds


# -- 1. retention ---------------------------------------------------------------


def test_a_sixth_stop_copy_removes_the_oldest(tmp_path: Path) -> None:
    clock = Clock()
    vault = a_vault(tmp_path, clock)
    made = tmp_path / "made"
    made.mkdir()
    for sequence in range(1, 7):
        vault.store(a_package(made, sequence=sequence), "stop")
    assert [copy.number for copy in copies_of(vault)] == [2, 3, 4, 5, 6]
    assert vault.state(UID).last_number == 6


def test_a_before_visit_copy_goes_after_30_days(tmp_path: Path) -> None:
    clock = Clock()
    vault = a_vault(tmp_path, clock)
    made = tmp_path / "made"
    made.mkdir()
    visit = vault.store(a_package(made, sequence=1), "before-visit")
    vault.store(a_package(made, sequence=2), "stop")
    clock.advance(days=29)
    assert vault.prune(UID) == ()
    assert visit.file in [copy.file for copy in copies_of(vault)]
    clock.advance(days=1)
    assert vault.prune(UID) == (visit.file,)
    assert visit.file not in [copy.file for copy in copies_of(vault)]


def test_a_sixth_visit_copy_of_one_world_removes_its_oldest_only(tmp_path: Path) -> None:
    clock = Clock()
    vault = a_vault(tmp_path, clock)
    made = tmp_path / "made"
    made.mkdir()
    for sequence in range(1, 7):
        vault.store(a_package(made, sequence=sequence), "visit", world=FRIEND_WORLD)
    other = vault.store(a_package(made, sequence=7), "visit", world=OTHER_WORLD)
    kept = [copy for copy in copies_of(vault) if copy.world == FRIEND_WORLD]
    assert [copy.number for copy in kept] == [2, 3, 4, 5, 6]
    assert other in copies_of(vault)


def test_the_visit_count_setting_is_honoured(tmp_path: Path) -> None:
    clock = Clock()
    vault = a_vault(tmp_path, clock, visit_copies_per_world=3)
    made = tmp_path / "made"
    made.mkdir()
    for sequence in range(1, 5):
        vault.store(a_package(made, sequence=sequence), "visit", world=FRIEND_WORLD)
    assert [copy.number for copy in copies_of(vault)] == [2, 3, 4]
    for count in (0, 51):
        with pytest.raises(ValueError):
            a_vault(tmp_path / "other", clock, visit_copies_per_world=count)


def test_a_before_merge_copy_goes_after_30_days(tmp_path: Path) -> None:
    clock = Clock()
    vault = a_vault(tmp_path, clock)
    made = tmp_path / "made"
    made.mkdir()
    merge = vault.store(a_package(made, sequence=1), "before-merge")
    vault.store(a_package(made, sequence=2), "stop")
    clock.advance(days=30)
    assert vault.prune(UID) == (merge.file,)
    assert [copy.number for copy in copies_of(vault)] == [2]


def test_the_newest_copy_is_never_pruned(tmp_path: Path) -> None:
    clock = Clock()
    vault = a_vault(tmp_path, clock)
    made = tmp_path / "made"
    made.mkdir()
    newest = vault.store(a_package(made, sequence=1), "before-visit")
    clock.advance(days=45)
    assert vault.prune(UID) == ()
    assert copies_of(vault) == [newest]


def test_a_copy_an_open_journal_names_is_never_pruned(tmp_path: Path) -> None:
    clock = Clock()
    vault, worlds = begun(tmp_path, clock)
    vault.store(a_package(worlds.made_dir, sequence=3), "stop")
    worlds.dump_sequence = 2
    worlds.crash = ("load", "in_step")
    with pytest.raises(Crash):
        vault.move(UID, FRIEND, worlds.steps())
    journal = json.loads(journal_path(vault).read_text())
    named = journal["package"]
    assert named is not None
    clock.advance(days=45)
    assert vault.prune(UID) == ()
    assert named in [copy.file for copy in copies_of(vault)]


def test_a_clock_moved_back_deletes_nothing(
    tmp_path: Path, caplog: pytest.LogCaptureFixture
) -> None:
    clock = Clock()
    vault = a_vault(tmp_path, clock)
    made = tmp_path / "made"
    made.mkdir()
    visit = vault.store(a_package(made, sequence=1), "before-visit")
    vault.store(a_package(made, sequence=2), "stop")
    clock.at = AT - timedelta(days=40)
    with caplog.at_level(logging.WARNING, logger="yulon.portable.vault"):
        assert vault.prune(UID) == ()
    assert visit.file in [copy.file for copy in copies_of(vault)]
    warnings = [
        record
        for record in caplog.records
        if record.levelno == logging.WARNING and record.name == "yulon.portable.vault"
    ]
    assert len(warnings) == 1


def test_a_cut_short_store_leaves_no_partial_after_prune(tmp_path: Path) -> None:
    clock = Clock()
    vault = a_vault(tmp_path, clock)
    made = tmp_path / "made"
    made.mkdir()
    copy = vault.store(a_package(made, sequence=1), "stop")
    packages = vault.character_dir(UID) / "packages"
    partial = packages / f"{copy.file}.0123456789abcdef.partial"
    partial.write_bytes(b"half a copy")
    removed = vault.prune(UID)
    assert partial.name in removed
    assert not partial.exists()
    assert copy.file in [kept.file for kept in copies_of(vault)]


# -- 2. store -------------------------------------------------------------------


def test_a_damaged_package_is_not_stored(tmp_path: Path) -> None:
    vault = a_vault(tmp_path)
    made = tmp_path / "made"
    made.mkdir()
    damaged = a_package(made, sequence=1)
    flip_member_data(damaged, "character.pdump")
    with pytest.raises(CharacterPackageError):
        vault.store(damaged, "stop")
    assert vault.copies(UID) == ()
    assert not vault.character_dir(UID).exists()


def test_the_same_package_stored_twice_is_one_copy(tmp_path: Path) -> None:
    vault = a_vault(tmp_path)
    made = tmp_path / "made"
    made.mkdir()
    one = a_package(made, sequence=1)
    first = vault.store(one, "stop")
    again = vault.store(one, "stop")
    assert again == first
    assert vault.copies(UID) == (first,)


def test_a_lower_sequence_goes_to_history_only(
    tmp_path: Path, caplog: pytest.LogCaptureFixture
) -> None:
    vault = a_vault(tmp_path)
    made = tmp_path / "made"
    made.mkdir()
    newest = vault.store(a_package(made, sequence=3), "stop")
    newest_digest = vault.state(UID).newest_digest
    lower = vault.store(a_package(made, sequence=2, parents=("c" * 64,)), "before-visit")
    assert lower.number == 2
    state = vault.state(UID)
    assert state.newest == newest.file
    assert state.newest_digest == newest_digest
    assert state.sequence == 3
    with caplog.at_level(logging.WARNING, logger="yulon.portable.vault"):
        vault.store(a_package(made, sequence=3, parents=("d" * 64, "e" * 64)), "before-merge")
    assert any(record.levelno == logging.WARNING for record in caplog.records)
    assert vault.state(UID).newest_digest == newest_digest
    assert len(copies_of(vault)) == 3


def test_the_state_carries_the_next_lineage(tmp_path: Path) -> None:
    vault = a_vault(tmp_path)
    made = tmp_path / "made"
    made.mkdir()
    first = a_package(made, sequence=1)
    vault.store(first, "stop")
    second = a_package(made, sequence=2, parents=(digest_of(first),))
    vault.store(second, "stop")
    state = vault.state(UID)
    assert state.sequence == 2
    assert state.parents == (digest_of(second), digest_of(first))
    deep = a_package(made, sequence=22, parents=("c" * 64,) * 20)
    vault.store(deep, "stop")
    state = vault.state(UID)
    assert len(state.parents) == 20
    assert state.parents[0] == digest_of(deep)


@pytest.mark.skipif(sys.platform == "win32", reason="a mode is a POSIX guarantee")
def test_copies_are_owner_only(tmp_path: Path) -> None:
    vault = a_vault(tmp_path)
    made = tmp_path / "made"
    made.mkdir()
    copy = vault.store(a_package(made, sequence=1), "stop")
    folder = vault.character_dir(UID)
    assert stat.S_IMODE(vault.root.stat().st_mode) == 0o700
    assert stat.S_IMODE(folder.stat().st_mode) == 0o700
    assert stat.S_IMODE((folder / "packages").stat().st_mode) == 0o700
    assert stat.S_IMODE((folder / "packages" / copy.file).stat().st_mode) == 0o600
    assert stat.S_IMODE((folder / "state.json").stat().st_mode) == 0o600
    assert stat.S_IMODE((folder / "lease").stat().st_mode) == 0o600
    assert stat.S_IMODE((folder / "lease-holder.json").stat().st_mode) == 0o600


def test_a_damaged_state_file_is_refused_not_rewritten(tmp_path: Path) -> None:
    vault = a_vault(tmp_path)
    made = tmp_path / "made"
    made.mkdir()
    vault.store(a_package(made, sequence=1), "stop")
    path = vault.character_dir(UID) / "state.json"
    path.write_text("{not a record")
    folder = vault.character_dir(UID)
    with pytest.raises(VaultError) as excinfo:
        vault.state(UID)
    assert excinfo.value.reason == "record_damaged"
    assert str(excinfo.value) == (
        f"Yu'lon cannot read its record of this character ({path}), so it changed nothing. "
        f"The copies are still in {folder}."
    )
    with pytest.raises(VaultError):
        vault.store(a_package(made, sequence=2, parents=("b" * 64,)), "stop")
    assert path.read_text() == "{not a record"


def test_a_newer_state_format_is_refused(tmp_path: Path) -> None:
    vault = a_vault(tmp_path)
    made = tmp_path / "made"
    made.mkdir()
    vault.store(a_package(made, sequence=1), "stop")
    path = vault.character_dir(UID) / "state.json"
    record = json.loads(path.read_text())
    record["format"] = 2
    path.write_text(json.dumps(record))
    with pytest.raises(VaultError) as excinfo:
        vault.state(UID)
    assert excinfo.value.reason == "newer_format"
    assert str(excinfo.value) == (
        "This character's record was written by a newer Yu'lon. Update Yu'lon to use it."
    )


# -- 3. folders -----------------------------------------------------------------


def test_two_characters_of_one_name_in_two_games_do_not_share_a_folder(tmp_path: Path) -> None:
    vault = a_vault(tmp_path)
    made = tmp_path / "made"
    made.mkdir()
    vault.store(a_package(made, uid=UID, sequence=1, game_id="wow-wotlk"), "stop")
    vault.store(a_package(made, uid=OTHER_UID, sequence=1, game_id="wow-tbc"), "stop")
    assert vault.characters() == tuple(sorted((UID, OTHER_UID)))
    assert vault.character_dir(UID) != vault.character_dir(OTHER_UID)
    assert vault.state(UID).name == vault.state(OTHER_UID).name == NAME
    one = [copy.file for copy in copies_of(vault, UID)]
    two = [copy.file for copy in copies_of(vault, OTHER_UID)]
    assert one == two
    assert vault.character_dir(UID) / "packages" != vault.character_dir(OTHER_UID) / "packages"


def test_a_bad_uid_is_refused(tmp_path: Path) -> None:
    vault = a_vault(tmp_path)
    for bad in ("../escape", "0" * 31, "A" * 32, "", "0" * 33, "../../etc"):
        with pytest.raises(ValueError):
            vault.character_dir(bad)
        with pytest.raises(ValueError):
            vault.state(bad)
        with pytest.raises(ValueError):
            vault.copies(bad)
    assert not vault.root.exists()
    assert not (tmp_path / "escape").exists()


# -- 4. damage ------------------------------------------------------------------


def test_a_damaged_newest_copy_falls_back_to_the_one_before(tmp_path: Path) -> None:
    vault = a_vault(tmp_path)
    made = tmp_path / "made"
    made.mkdir()
    older = vault.store(a_package(made, sequence=1), "stop")
    newer = vault.store(a_package(made, sequence=2, parents=("a" * 64,)), "stop")
    flip_member_data(newer.path, "character.pdump")
    fallback = vault.newest_good(UID)
    assert fallback.copy == older
    assert fallback.package is not None
    assert fallback.package.digest == digest_of(older.path)
    with pytest.raises(CharacterPackageError) as excinfo:
        read_package(newer.path)
    assert fallback.refused == ((newer.file, str(excinfo.value)),)
    assert fallback.sentence == (
        f"The newest copy of {NAME} is damaged, so Yu'lon will not use it. The copy from "
        f"{older.stored_at:%Y-%m-%d %H:%M} UTC is offered instead."
    )


def test_a_bad_signature_falls_back(tmp_path: Path) -> None:
    bad: list[str] = []

    def checker(package: Package) -> None:
        if package.digest in bad:
            raise PackageRefused("bad-signature")

    vault = a_vault(tmp_path, checker=checker)
    made = tmp_path / "made"
    made.mkdir()
    older = vault.store(a_package(made, sequence=1, signed=True), "stop")
    newer = vault.store(a_package(made, sequence=2, parents=("a" * 64,), signed=True), "stop")
    bad.append(read_package(newer.path).digest)
    fallback = vault.newest_good(UID)
    assert fallback.copy == older
    assert fallback.refused == ((newer.file, "its signature does not hold (bad-signature)"),)
    assert fallback.sentence == (
        f"The newest copy of {NAME} is damaged, so Yu'lon will not use it. The copy from "
        f"{older.stored_at:%Y-%m-%d %H:%M} UTC is offered instead."
    )


def test_an_unsigned_copy_is_refused_when_a_checker_is_set(tmp_path: Path) -> None:
    seen: list[str] = []

    def checker(package: Package) -> None:
        seen.append(package.digest)

    vault = a_vault(tmp_path, checker=checker)
    made = tmp_path / "made"
    made.mkdir()
    with pytest.raises(PackageRefused) as excinfo:
        vault.store(a_package(made, sequence=1), "stop")
    assert excinfo.value.reason == "format"
    assert vault.copies(UID) == ()
    assert seen == []
    signed = a_package(made, sequence=1, signed=True)
    stored = vault.store(signed, "stop")
    assert stored.number == 1
    # `store` proves the copy once; `prune`, which runs after it, asks again for the copy it
    # would offer, so the digest is seen from the first check on.
    assert seen[0] == digest_of(signed)
    assert set(seen) == {digest_of(signed)}


def test_every_copy_damaged_offers_none(tmp_path: Path) -> None:
    vault = a_vault(tmp_path)
    made = tmp_path / "made"
    made.mkdir()
    older = vault.store(a_package(made, sequence=1), "stop")
    newer = vault.store(a_package(made, sequence=2, parents=("a" * 64,)), "stop")
    flip_member_data(older.path, "character.pdump")
    flip_member_data(newer.path, "character.pdump")
    fallback = vault.newest_good(UID)
    assert fallback.copy is None
    assert fallback.package is None
    assert [file for file, _why in fallback.refused] == [newer.file, older.file]
    assert fallback.sentence == (
        f"Every copy of {NAME} in Yu'lon's vault is damaged, so there is none to offer."
    )


# -- 5. the journal -------------------------------------------------------------


def test_the_record_is_written_before_each_step(tmp_path: Path) -> None:
    vault, worlds = begun(tmp_path)
    result = vault.move(UID, FRIEND, worlds.steps())
    assert result.outcome == "done"
    assert worlds.checked == list(STEPS)
    assert not journal_path(vault).exists()


def test_a_rerun_after_load_confirms_and_never_loads_again(tmp_path: Path) -> None:
    vault, worlds = begun(tmp_path)
    worlds.crash = ("load", "in_step")
    with pytest.raises(Crash):
        vault.move(UID, FRIEND, worlds.steps())
    worlds.crash = None
    result = vault.resume(UID, worlds.steps())
    assert result is not None
    assert result.outcome == "done"
    assert worlds.loads == {FRIEND_WORLD: 1}
    assert worlds.confirms == 1
    assert worlds.events[-2:] == ["confirm:loaded", "start"]


def test_absent_on_a_rerun_sends_the_load_once_more(tmp_path: Path) -> None:
    vault, worlds = begun(tmp_path)
    worlds.crash = ("load", "in_step")
    with pytest.raises(Crash):
        vault.move(UID, FRIEND, worlds.steps())
    worlds.crash = None
    worlds.answers = ["absent"]
    result = vault.resume(UID, worlds.steps())
    assert result is not None
    assert result.outcome == "done"
    assert worlds.loads == {FRIEND_WORLD: 2}
    asked = worlds.events.index("confirm:absent")
    resent = worlds.events.index(f"load:{FRIEND_WORLD}", asked)
    confirmed = worlds.events.index("confirm:loaded", resent)
    assert asked < resent < confirmed


def test_a_failed_confirm_unlocks_home(tmp_path: Path) -> None:
    vault, worlds = begun(tmp_path)
    before = vault.state(UID)
    worlds.answers = ["failed"]
    result = vault.move(UID, FRIEND, worlds.steps())
    assert result.outcome == "rolled_back"
    assert result.load_failed is True
    assert result.holder == HOME
    assert result.sentence == (
        f"{NAME} could not be loaded into that world, so the move was undone."
    )
    assert worlds.unlocked == ["home"]
    assert worlds.banned == set()
    after = vault.state(UID)
    assert after.holder == before.holder
    assert after.revoked == ()
    assert not journal_path(vault).exists()


def test_a_failed_confirm_from_a_friend_leaves_the_vault_holding(tmp_path: Path) -> None:
    vault, worlds = begun(tmp_path)
    first = vault.move(UID, FRIEND, worlds.steps())
    assert first.outcome == "done"
    assert vault.state(UID).holder == FRIEND
    worlds.answers = ["failed"]
    result = vault.move(UID, HOME, worlds.steps())
    assert result.outcome == "rolled_back"
    assert result.load_failed is True
    assert result.holder == Holder("vault")
    state = vault.state(UID)
    assert state.holder == Holder("vault")
    assert FRIEND in state.revoked
    assert worlds.erased == {VISIT}


def test_unknown_waits_and_keeps_the_journal(tmp_path: Path) -> None:
    vault, worlds = begun(tmp_path)
    before = vault.state(UID)
    worlds.answers = ["unknown"]
    result = vault.move(UID, FRIEND, worlds.steps())
    assert result.outcome == "waiting"
    assert result.holder == HOME
    assert result.sentence == (
        f"Yu'lon could not tell whether {NAME} arrived. It asks again at the next start."
    )
    assert journal_path(vault).exists()
    assert vault.state(UID).holder == before.holder
    assert worlds.loads == {FRIEND_WORLD: 1}


def test_an_unreachable_old_holder_changes_nothing(tmp_path: Path) -> None:
    vault, worlds = begun(tmp_path)
    before = vault.state(UID)
    copies = copies_of(vault)
    worlds.no_dump = True
    result = vault.move(UID, FRIEND, worlds.steps())
    assert result.outcome == "old_holder_unreachable"
    assert result.holder == HOME
    assert result.newest_at == copies[0].stored_at
    assert vault.state(UID) == before
    assert copies_of(vault) == copies
    assert worlds.banned == set()
    assert worlds.loads == {}
    assert worlds.started == set()
    assert not journal_path(vault).exists()


def test_continue_from_the_vault_revokes_the_friends_copy(tmp_path: Path) -> None:
    vault, worlds = begun(tmp_path)
    first = vault.move(UID, FRIEND, worlds.steps())
    assert first.outcome == "done"
    dumps = worlds.dumps
    worlds.no_dump = True
    result = vault.move(UID, HOME, worlds.steps(), from_vault=True)
    assert result.outcome == "done"
    assert result.holder == HOME
    assert worlds.dumps == dumps
    state = vault.state(UID)
    assert state.holder == HOME
    assert FRIEND in state.revoked
    assert worlds.erased == set()


def test_play_on_the_holder_only_starts(tmp_path: Path) -> None:
    vault, worlds = begun(tmp_path)
    worlds.journaled = False
    result = vault.move(UID, HOME, worlds.steps())
    assert result.outcome == "done"
    assert result.holder == HOME
    assert worlds.started == {("home", NAME)}
    assert worlds.dumps == 0
    assert worlds.loads == {}
    assert worlds.banned == set()
    assert not journal_path(vault).exists()


def test_a_refused_lock_undoes_nothing(tmp_path: Path) -> None:
    vault, worlds = begun(tmp_path)
    before = vault.state(UID)
    worlds.refuse = "lock"
    result = vault.move(UID, FRIEND, worlds.steps())
    assert result.outcome == "rolled_back"
    assert result.holder == HOME
    assert result.sentence == "Home is already busy, so nothing was moved."
    assert worlds.unlocked == []
    assert worlds.banned == set()
    assert worlds.loads == {}
    assert vault.state(UID).holder == before.holder
    assert not journal_path(vault).exists()


def test_another_target_while_a_move_is_open_is_refused(tmp_path: Path) -> None:
    vault, worlds = begun(tmp_path)
    worlds.crash = ("load", "in_step")
    with pytest.raises(Crash):
        vault.move(UID, FRIEND, worlds.steps())
    worlds.crash = None
    other = Holder("friend", world="a" * 32, visit_id="b" * 32)
    with pytest.raises(VaultError) as excinfo:
        vault.move(UID, other, worlds.steps())
    assert excinfo.value.reason == "move_open"
    assert str(excinfo.value) == (
        f"A move of {NAME} did not finish. Yu'lon finishes that one first."
    )
    result = vault.move(UID, FRIEND, worlds.steps())
    assert result.outcome == "done"


# -- the load is the point of no return -----------------------------------------

DIRECTIONS = ("home->friend", "friend->home", "from-vault")
TARGETS = {"home->friend": FRIEND, "friend->home": HOME, "from-vault": HOME}


def a_move(base: Path, direction: str) -> tuple[Vault, FakeWorlds, Callable[[], MoveResult]]:
    """A vault standing where `direction` starts, and the callable that makes the move.

    `friend->home` and `from-vault` first finish a move to the friend: the character has to be
    there before it can be moved home. The fake's dump grows by one sequence for the second
    move, so its copy is not the digest of one already stored.
    """
    vault, worlds = begun(base)
    if direction == "home->friend":
        return vault, worlds, lambda: vault.move(UID, FRIEND, worlds.steps())
    first = vault.move(UID, FRIEND, worlds.steps())
    assert first.outcome == "done"
    worlds.dump_sequence += 1
    if direction == "friend->home":
        return vault, worlds, lambda: vault.move(UID, HOME, worlds.steps())
    return vault, worlds, lambda: vault.move(UID, HOME, worlds.steps(), from_vault=True)


@pytest.mark.parametrize("direction", DIRECTIONS)
def test_a_refused_start_after_a_loaded_confirm_keeps_the_target_as_holder(
    tmp_path: Path, direction: str
) -> None:
    """The load arrived: the target holds the character now, whatever `start` answers."""
    vault, worlds, run = a_move(tmp_path, direction)
    target = TARGETS[direction]
    worlds.refuse_start = True
    result = run()
    assert result.outcome == "waiting"
    assert result.holder == target
    assert result.sentence == "The world is still busy, so Yu'lon did not start it."
    assert journal_path(vault).exists()
    assert worlds.unlocked == []
    assert vault.state(UID).holder == target
    sent = len(worlds.events)
    worlds.refuse_start = False
    resumed = vault.resume(UID, worlds.steps())
    assert resumed is not None
    assert resumed.outcome == "done"
    assert resumed.holder == target
    assert worlds.events[sent:] == ["start"]


@pytest.mark.parametrize("direction", DIRECTIONS)
def test_a_refused_confirm_on_a_rerun_waits_and_never_rolls_back(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, direction: str
) -> None:
    """After the load started, a refusal proves nothing about the target: so nothing is undone."""
    vault, worlds, run = a_move(tmp_path, direction)
    arm_mark_crash(monkeypatch, "load", "done")
    with pytest.raises(Crash):
        run()
    before = vault.state(UID)
    worlds.refuse = "confirm"
    result = vault.resume(UID, worlds.steps())
    assert result is not None
    assert result.outcome == "waiting"
    assert result.holder == before.holder
    assert result.sentence == "The world did not answer, so nothing was moved."
    assert journal_path(vault).exists()
    assert worlds.unlocked == []
    assert vault.state(UID) == before
    worlds.refuse = None
    resumed = vault.resume(UID, worlds.steps())
    assert resumed is not None
    assert resumed.outcome == "done"


# -- what a rollback must write down before it acts ------------------------------


def arm_mark_crash(monkeypatch: pytest.MonkeyPatch, step: str, state: str) -> None:
    """Die once, just after the journal says `step` is at `state`."""
    real = vault_module._mark
    armed = True

    def flaky(path, journal, marked, marked_state):
        nonlocal armed
        written = real(path, journal, marked, marked_state)
        if armed and marked == step and marked_state == state:
            armed = False
            raise Crash(marked)
        return written

    monkeypatch.setattr(vault_module, "_mark", flaky)


def arm_rollback_crash(monkeypatch: pytest.MonkeyPatch) -> None:
    """Die once, just after the journal says a rollback has begun."""
    real = vault_module._mark
    armed = True

    def flaky(path, journal, marked, marked_state):
        nonlocal armed
        written = real(path, journal, marked, marked_state)
        if armed and written.rolling_back:
            armed = False
            raise Crash("rollback")
        return written

    monkeypatch.setattr(vault_module, "_mark", flaky)


def test_a_refused_lock_that_dies_in_its_rollback_never_invents_a_lock(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """The refusal is written down before the undo: a rerun must not read it as a lock taken."""
    vault, worlds = begun(tmp_path)
    assert vault.move(UID, FRIEND, worlds.steps()).outcome == "done"
    worlds.refuse = "lock"
    arm_rollback_crash(monkeypatch)
    with pytest.raises(Crash):
        vault.move(UID, HOME, worlds.steps())
    result = vault.resume(UID, worlds.steps())
    assert result is not None
    assert result.outcome == "rolled_back"
    assert result.sentence == "Home is already busy, so nothing was moved."
    state = vault.state(UID)
    assert state.holder == FRIEND
    assert FRIEND not in state.revoked
    assert worlds.unlocked == []
    assert not journal_path(vault).exists()


@pytest.mark.parametrize("direction", DIRECTIONS)
def test_a_crash_inside_the_rollback_is_rerun_to_the_same_sentence(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, direction: str
) -> None:
    """The undo keeps its sentence and `load_failed` in the journal, so a rerun says the same."""
    plain_vault, plain_worlds, plain_run = a_move(tmp_path / "plain", direction)
    plain_worlds.answers = ["failed"]
    plain = plain_run()
    assert plain.outcome == "rolled_back"

    vault, worlds, run = a_move(tmp_path / "crash", direction)
    worlds.answers = ["failed"]
    arm_rollback_crash(monkeypatch)
    with pytest.raises(Crash):
        run()
    result = vault.resume(UID, worlds.steps())
    assert result is not None
    assert result.outcome == "rolled_back"
    assert result.load_failed is True
    assert result.sentence == (
        f"{NAME} could not be loaded into that world, so the move was undone."
    )
    assert result.sentence == plain.sentence
    assert result.holder == plain.holder
    assert state_json(vault) == state_json(plain_vault)
    assert worlds.snapshot() == plain_worlds.snapshot()


# -- the dump's copy: the number is reserved, never guessed ---------------------


def arm_store_copy_crash(monkeypatch: pytest.MonkeyPatch) -> None:
    """Die once before the copy is renamed into place, as a lost computer would."""
    real = Vault._store_copy
    armed = True

    def flaky(self, source, final):
        nonlocal armed
        if armed:
            armed = False
            raise Crash("copy")
        return real(self, source, final)

    monkeypatch.setattr(Vault, "_store_copy", flaky)


def arm_state_write_crash(monkeypatch: pytest.MonkeyPatch) -> None:
    """Die once after the copy is in place and before the state that names it."""
    real = Vault._write_state
    armed = True

    def flaky(self, folder, state):
        nonlocal armed
        if armed:
            armed = False
            raise Crash("state")
        return real(self, folder, state)

    monkeypatch.setattr(Vault, "_write_state", flaky)


def test_a_crash_between_the_copy_and_its_state_adopts_it_on_the_rerun(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """The dump's file name is in the journal: the rerun adopts that copy, sequence and all."""
    vault, worlds = begun(tmp_path)
    assert vault.move(UID, FRIEND, worlds.steps()).outcome == "done"
    worlds.dump_sequence = 5
    arm_state_write_crash(monkeypatch)
    with pytest.raises(Crash):
        vault.move(UID, HOME, worlds.steps())
    reserved = json.loads(journal_path(vault).read_text())["reserved"]
    assert reserved is not None
    assert (vault.character_dir(UID) / "packages" / reserved).is_file()
    dumps = worlds.dumps
    result = vault.resume(UID, worlds.steps())
    assert result is not None
    assert result.outcome == "done"
    assert worlds.dumps == dumps
    state = vault.state(UID)
    assert state.sequence == 5
    assert state.newest == reserved
    assert state.last_number == 3
    assert [copy.file for copy in copies_of(vault)].count(reserved) == 1


def test_a_copy_stored_while_the_move_is_open_never_takes_its_number(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Two stores never share a number, and only the reserved name is ever adopted."""
    vault, worlds = begun(tmp_path)
    assert vault.move(UID, FRIEND, worlds.steps()).outcome == "done"
    worlds.dump_sequence = 5
    arm_store_copy_crash(monkeypatch)
    with pytest.raises(Crash):
        vault.move(UID, HOME, worlds.steps())
    reserved = json.loads(journal_path(vault).read_text())["reserved"]
    assert reserved is not None
    assert not (vault.character_dir(UID) / "packages" / reserved).exists()
    first = vault.store(a_package(worlds.made_dir, sequence=3), "stop")
    second = vault.store(a_package(worlds.made_dir, sequence=4), "stop")
    assert reserved not in (first.file, second.file)
    dumps = worlds.dumps
    result = vault.resume(UID, worlds.steps())
    assert result is not None
    assert result.outcome == "done"
    assert worlds.dumps == dumps + 1
    assert vault.state(UID).newest == reserved
    numbers = [copy.number for copy in copies_of(vault)]
    assert len(numbers) == len(set(numbers))


# -- a dump the vault cannot trust ----------------------------------------------


def test_a_dump_that_does_not_read_undoes_the_move(tmp_path: Path) -> None:
    """A file the vault cannot read is not a move that stays open for ever."""
    vault, worlds = begun(tmp_path)
    before = vault.state(UID)
    worlds.dump_damaged = True
    result = vault.move(UID, FRIEND, worlds.steps())
    assert result.outcome == "rolled_back"
    assert result.holder == HOME
    assert result.sentence == (
        "The world handed back a character file Yu'lon cannot trust, so nothing was moved."
    )
    assert vault.state(UID) == before
    assert not journal_path(vault).exists()
    worlds.dump_damaged = False
    other = Holder("friend", world=OTHER_WORLD, visit_id=VISIT)
    assert vault.move(UID, other, worlds.steps()).outcome == "done"


def test_a_dump_the_signer_refuses_undoes_the_move(tmp_path: Path) -> None:
    """`store`'s refusal is a refusal of the dump, not a crash that blocks the character."""
    vault = a_vault(tmp_path, checker=lambda package: None)
    worlds = FakeWorlds(tmp_path, vault)
    worlds.made_dir.mkdir(parents=True)
    vault.store(a_package(worlds.made_dir, sequence=1, signed=True), "stop")
    before = vault.state(UID)
    result = vault.move(UID, FRIEND, worlds.steps())
    assert result.outcome == "rolled_back"
    assert result.sentence == (
        "The world handed back a character file Yu'lon cannot trust, so nothing was moved."
    )
    assert vault.state(UID) == before
    assert not journal_path(vault).exists()
    worlds.dump_signed = True
    other = Holder("friend", world=OTHER_WORLD, visit_id=VISIT)
    assert vault.move(UID, other, worlds.steps()).outcome == "done"


# -- what prune may not remove ---------------------------------------------------


def test_the_copy_offered_when_the_newest_is_damaged_is_never_pruned(tmp_path: Path) -> None:
    """The fallback is what the player is offered, so the keep rule does not take it."""
    clock = Clock()
    vault = a_vault(tmp_path, clock)
    made = tmp_path / "made"
    made.mkdir()
    fallback = vault.store(a_package(made, sequence=1), "before-visit")
    newest = vault.store(a_package(made, sequence=2, parents=("a" * 64,)), "stop")
    flip_member_data(newest.path, "character.pdump")
    clock.advance(days=31)
    assert vault.prune(UID) == ()
    assert fallback.file in [copy.file for copy in copies_of(vault)]


# -- a journal that no longer parses --------------------------------------------


def test_a_damaged_journal_can_be_abandoned_and_kept_aside(tmp_path: Path) -> None:
    """The player decides: the file is kept under a new name, the state is untouched."""
    vault, worlds = begun(tmp_path)
    worlds.crash = ("load", "in_step")
    with pytest.raises(Crash):
        vault.move(UID, FRIEND, worlds.steps())
    path = journal_path(vault)
    path.write_text("{not a journal")
    old = vault.character_dir(UID) / "journal.damaged-1.json"
    old.write_text("{an earlier one")
    for call in (
        lambda: vault.move(UID, FRIEND, worlds.steps()),
        lambda: vault.resume(UID, worlds.steps()),
    ):
        with pytest.raises(VaultError) as excinfo:
            call()
        assert excinfo.value.reason == "record_damaged"
    before = vault.state(UID)
    result = vault.abandon_damaged_journal(UID)
    assert result.outcome == "waiting"
    assert result.holder == before.holder
    assert result.sentence == (
        f"Yu'lon lost track of {NAME}'s last move. "
        f"Check which world {NAME} is in before you play."
    )
    assert not path.exists()
    assert (vault.character_dir(UID) / "journal.damaged-2.json").read_text() == "{not a journal"
    assert old.read_text() == "{an earlier one"
    assert vault.state(UID) == before
    with pytest.raises(ValueError):
        vault.abandon_damaged_journal(UID)
    worlds.crash = None
    assert vault.move(UID, FRIEND, worlds.steps()).outcome == "done"


def test_an_intact_journal_is_never_moved_aside(tmp_path: Path) -> None:
    vault, worlds = begun(tmp_path)
    worlds.crash = ("load", "in_step")
    with pytest.raises(Crash):
        vault.move(UID, FRIEND, worlds.steps())
    before = journal_path(vault).read_bytes()
    with pytest.raises(ValueError):
        vault.abandon_damaged_journal(UID)
    assert journal_path(vault).read_bytes() == before


# -- absent right after this run's own load ---------------------------------------


def test_absent_right_after_this_run_s_own_load_undoes_the_move(tmp_path: Path) -> None:
    """Decision 10: an `absent` the load we just sent cannot explain is a failed load."""
    vault, worlds = begun(tmp_path)
    worlds.answers = ["absent"]
    result = vault.move(UID, FRIEND, worlds.steps())
    assert result.outcome == "rolled_back"
    assert result.load_failed is True
    assert result.holder == HOME
    assert result.sentence == (
        f"{NAME} could not be loaded into that world, so the move was undone."
    )
    assert worlds.unlocked == ["home"]
    assert worlds.loads == {FRIEND_WORLD: 1}
    assert not journal_path(vault).exists()


# -- a crash at every point -------------------------------------------------------


POINTS: list[tuple[str, str]] = [
    (step, when) for step in STEPS for when in ("in_step", "after_mark")
] + [
    ("load", "before_effect"),
    ("store", "before_copy"),
    ("store", "before_state"),
]

NOT_A_POINT: dict[str, set[tuple[str, str]]] = {
    "from-vault": {
        ("dump", "in_step"),
        ("dump", "after_mark"),
        ("lock", "in_step"),
        ("lock", "after_mark"),
        ("store", "before_copy"),
        ("store", "before_state"),
    },
}

CRASHES: list[tuple[str, str, str]] = [
    (direction, step, when)
    for direction in DIRECTIONS
    for step, when in POINTS
    if (step, when) not in NOT_A_POINT.get(direction, set())
]


@pytest.mark.parametrize(
    "direction,step,when", CRASHES, ids=[f"{d}-{s}-{w}" for d, s, w in CRASHES]
)
def test_a_crash_at_every_point_then_a_rerun_ends_the_same(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    direction: str,
    step: str,
    when: str,
) -> None:
    plain_vault, plain_worlds, plain_run = a_move(tmp_path / "plain", direction)
    finished = plain_run()
    assert finished.outcome == "done"

    crash_vault, crash_worlds, crash_run = a_move(tmp_path / "crash", direction)
    if when == "after_mark":
        arm_mark_crash(monkeypatch, step, "done")
    elif (step, when) == ("store", "before_copy"):
        arm_store_copy_crash(monkeypatch)
    elif (step, when) == ("store", "before_state"):
        arm_state_write_crash(monkeypatch)
    crash_worlds.crash = (step, when)
    with pytest.raises(Crash):
        crash_run()
    crash_worlds.crash = None

    result = crash_vault.resume(UID, crash_worlds.steps())
    assert result is not None
    assert result.outcome == "done"
    assert crash_worlds.loads == plain_worlds.loads
    assert crash_worlds.snapshot() == plain_worlds.snapshot()
    assert state_json(crash_vault) == state_json(plain_vault)
    assert [(copy.kind, copy.file) for copy in copies_of(crash_vault)] == [
        (copy.kind, copy.file) for copy in copies_of(plain_vault)
    ]
