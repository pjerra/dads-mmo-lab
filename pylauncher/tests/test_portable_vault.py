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
from datetime import UTC, datetime, timedelta
from pathlib import Path

import pytest

from tests.test_portable_package import facts as export_facts
from tests.test_portable_package import flip_member_data, small_dump
from tests.test_portable_package import game as a_game
from yulon.portable import identity
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
        made = a_package(self.made_dir, uid=self.dump_uid, sequence=self.dump_sequence)
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
    with pytest.raises(identity.Refused) as excinfo:
        vault.store(a_package(made, sequence=1), "stop")
    assert excinfo.value.reason == "format"
    assert vault.copies(UID) == ()
    assert seen == []
    signed = a_package(made, sequence=1, signed=True)
    stored = vault.store(signed, "stop")
    assert stored.number == 1
    assert seen == [digest_of(signed)]


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


CRASHES: list[tuple[str, str]] = [
    (step, when) for step in STEPS for when in ("in_step", "after_mark")
] + [("load", "before_effect")]


@pytest.mark.parametrize("step,when", CRASHES, ids=[f"{s}-{w}" for s, w in CRASHES])
def test_a_crash_at_every_point_then_a_rerun_ends_the_same(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, step: str, when: str
) -> None:
    plain_vault, plain_worlds = begun(tmp_path / "plain")
    finished = plain_vault.move(UID, FRIEND, plain_worlds.steps())
    assert finished.outcome == "done"

    crash_vault, crash_worlds = begun(tmp_path / "crash")
    if when == "after_mark":
        real = vault_module._mark

        def flaky(path, journal, marked, state):
            marked_journal = real(path, journal, marked, state)
            if marked == step and state == "done":
                raise Crash(marked)
            return marked_journal

        monkeypatch.setattr(vault_module, "_mark", flaky)
    crash_worlds.crash = (step, when)
    with pytest.raises(Crash):
        crash_vault.move(UID, FRIEND, crash_worlds.steps())
    crash_worlds.crash = None

    result = crash_vault.resume(UID, crash_worlds.steps())
    assert result is not None
    assert result.outcome == "done"
    assert crash_worlds.loads == plain_worlds.loads == {FRIEND_WORLD: 1}
    assert crash_worlds.snapshot() == plain_worlds.snapshot()
    assert state_json(crash_vault) == state_json(plain_vault)
    assert [(copy.kind, copy.file) for copy in copies_of(crash_vault)] == [
        (copy.kind, copy.file) for copy in copies_of(plain_vault)
    ]
