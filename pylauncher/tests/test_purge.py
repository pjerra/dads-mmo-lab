"""Uninstall and purge, the code half (8.9a).

Every seam `yulon.purge` reaches the world through is injected here, so nothing
in this file needs Docker, a folder full of AzerothCore, or a GUI. What is
asserted is the part that cannot be recovered from if it is wrong: the
refusals, the ORDER, the scope, and which volume survives a ticked box.

The refusals come first in this file for the same reason they come first in the
module: this is the one action whose bug deletes somebody's server.
"""

from __future__ import annotations

import os
from collections.abc import Callable
from pathlib import Path

import pytest

from yulon import docker, logsnap, purge
from yulon.catalog import composegen
from yulon.controller_wow_wotlk import docker_ctl
from yulon.ownership import Ownership

SPEC = docker_ctl.SPEC
GAME = "wow-wotlk"

UNASKED = object()
"""What a seam answers when Docker could not be asked at all.

A distinct sentinel and not `None`: `None` is the ANSWER those two seams give
for exactly that case, so a default of `None` in this harness would make
"Docker would not say" unexpressible — which is the case two of the tests below
are entirely about.
"""


def _images(server_dir: Path) -> tuple[str, ...]:
    """The four per-install refs `composegen.built_image_refs()` computes, spelled out."""
    tag = composegen.image_tag(server_dir)
    return tuple(
        f"yulon.local/ac-wotlk-{name}:{tag}"
        for name in ("worldserver", "authserver", "db-import", "client-data")
    )


class Recorder:
    """Every seam of one Uninstaller, recording what was called in what order.

    A single object rather than a bag of lambdas because the ORDER assertions
    are the point of most of this file, and an order is a property of the whole
    set of calls rather than of any one of them.
    """

    def __init__(
        self,
        server_dir: Path,
        *,
        project: str | None = "yulon-wow-wotlk-deadbeef",
        claim: Ownership = Ownership.OWNED,
        running: docker.Running | None = None,
        containers: object = UNASKED,
        volumes: object = UNASKED,
        snapshot: logsnap.Snapshot | None = None,
        remove_folder: Callable[[Path], None] | None = None,
        image_warning: str = "",
    ) -> None:
        self.server_dir = server_dir
        self.order: list[str] = []
        self.removed_volumes: list[str] = []
        self.removed_images: list[str] = []
        self.forgotten = 0
        self._project = project
        self._claim = claim
        self._running = running if running is not None else docker.Running()
        self._containers = [SPEC.world, SPEC.db] if containers is UNASKED else containers
        self._volumes = (
            [f"{project}_db-data", f"{project}_client-data"] if volumes is UNASKED else volumes
        )
        self._snapshot = snapshot or logsnap.Snapshot(path=Path("/logs/snap.log"))
        self._remove_folder = remove_folder
        self._image_warning = image_warning

    # -- read seams -------------------------------------------------------
    def claim(self, path: Path) -> Ownership:
        self.order.append("claim")
        return self._claim

    def project_of(self) -> str | None:
        self.order.append("project")
        return self._project

    def census(self, project: str) -> docker.Running:
        self.order.append(f"census:{project}")
        return self._running

    def containers_of(self, project: str) -> list[str] | None:
        self.order.append(f"containers:{project}")
        return None if self._containers is None else list(self._containers)

    def volumes_of(self, project: str) -> list[str] | None:
        self.order.append(f"volumes:{project}")
        return None if self._volumes is None else list(self._volumes)

    def folder_size(self, path: Path) -> int:
        self.order.append("size")
        return 2_300_000_000

    # -- write seams ------------------------------------------------------
    def snapshot(self) -> logsnap.Snapshot:
        self.order.append("snapshot")
        return self._snapshot

    def remove_containers(self) -> bool:
        self.order.append("remove_containers")
        return True

    def remove_volume(self, name: str) -> None:
        self.order.append(f"remove_volume:{name}")
        self.removed_volumes.append(name)

    def remove_image(self, ref: str) -> str:
        self.order.append(f"remove_image:{ref}")
        self.removed_images.append(ref)
        return self._image_warning

    def remove_folder(self, path: Path) -> None:
        self.order.append(f"remove_folder:{path}")
        if self._remove_folder is not None:
            self._remove_folder(path)

    def forget(self) -> None:
        self.order.append("forget")
        self.forgotten += 1

    # -- the object under test --------------------------------------------
    def uninstaller(self, **overrides: object) -> purge.Uninstaller:
        seams = dict(
            game=GAME,
            server_dir=self.server_dir,
            spec=SPEC,
            image_refs=_images(self.server_dir),
            claim=self.claim,
            project_of=self.project_of,
            census=self.census,
            containers_of=self.containers_of,
            volumes_of=self.volumes_of,
            folder_size=self.folder_size,
            snapshot=self.snapshot,
            remove_containers=self.remove_containers,
            remove_volume=self.remove_volume,
            remove_image=self.remove_image,
            remove_folder=self.remove_folder,
            forget=self.forget,
        )
        seams.update(overrides)
        return purge.Uninstaller(**seams)  # type: ignore[arg-type]


def _recorder(tmp_path: Path, **kwargs: object) -> Recorder:
    server_dir = tmp_path / "wowserver"
    server_dir.mkdir(exist_ok=True)
    return Recorder(server_dir, **kwargs)  # type: ignore[arg-type]


# -- 1. the ownership refusal ---------------------------------------------


def test_an_install_whose_record_cannot_be_read_is_refused_and_not_removed(
    tmp_path: Path,
) -> None:
    """`UNKNOWN` is the case the app knows LEAST about; it must act least freely."""
    rec = _recorder(tmp_path, claim=Ownership.UNKNOWN)
    with pytest.raises(purge.PurgeError) as caught:
        rec.uninstaller().run(keep_characters=False)
    assert "cannot read" in str(caught.value)
    assert "Nothing was removed" in str(caught.value)
    assert rec.order == ["claim"], rec.order


def test_a_folder_nothing_claims_is_refused_too_and_says_something_different(
    tmp_path: Path,
) -> None:
    """UNCLAIMED is not permission.

    `state.json` records whatever folder "Use existing…" was pointed at, so a
    user who adopted their own hand-built AzerothCore tree has a record naming
    a folder Yu'lon never created. Written as `if claim is UNKNOWN: refuse`,
    that user loses their server. Only OWNED authorises a delete — and the two
    refusals must not read the same, because the advice differs: one says a
    record here is unreadable, the other says there is no record at all.
    """
    rec = _recorder(tmp_path, claim=Ownership.UNCLAIMED)
    with pytest.raises(purge.PurgeError) as caught:
        rec.uninstaller().run(keep_characters=False)
    message = str(caught.value)
    assert "Nothing here says Yu'lon installed it" in message
    assert "cannot read" not in message
    assert rec.order == ["claim"], rec.order


def test_the_two_refusals_do_not_share_their_wording(tmp_path: Path) -> None:
    """Three values, never two: the messages are how a user tells them apart."""
    unknown = purge.refusal_for(Ownership.UNKNOWN, tmp_path)
    unclaimed = purge.refusal_for(Ownership.UNCLAIMED, tmp_path)
    assert unknown and unclaimed and unknown != unclaimed
    assert purge.refusal_for(Ownership.OWNED, tmp_path) == ""


def test_a_plan_of_an_unprovable_install_carries_the_refusal_and_no_targets(
    tmp_path: Path,
) -> None:
    """The dialog must be able to show the refusal without the run being reached."""
    rec = _recorder(tmp_path, claim=Ownership.UNKNOWN)
    plan = rec.uninstaller().plan()
    assert plan.refusal
    assert plan.containers == () and plan.volumes == () and plan.images == ()


def test_an_install_whose_compose_project_cannot_be_resolved_is_refused(
    tmp_path: Path,
) -> None:
    """A `None` project is the refusal path, never a fallback: every command is scoped to it."""
    rec = _recorder(tmp_path, project=None)
    with pytest.raises(purge.PurgeError) as caught:
        rec.uninstaller().run(keep_characters=False)
    assert "which Docker project" in str(caught.value)
    assert "snapshot" not in rec.order


def test_docker_refusing_to_list_this_projects_containers_is_not_nothing_is_there(
    tmp_path: Path,
) -> None:
    """`project_containers()` answers None when Docker could not be asked."""
    rec = _recorder(tmp_path, containers=None)
    with pytest.raises(purge.PurgeError) as caught:
        rec.uninstaller().run(keep_characters=False)
    assert "could not ask Docker" in str(caught.value)
    assert "snapshot" not in rec.order


def test_containers_docker_will_not_name_an_owner_for_refuse_the_whole_purge(
    tmp_path: Path,
) -> None:
    """`unreadable` is not proof of anything, and this action cannot proceed on none."""
    rec = _recorder(tmp_path, running=docker.Running(unreadable=(SPEC.world,)))
    with pytest.raises(purge.PurgeError) as caught:
        rec.uninstaller().run(keep_characters=False)
    assert "would not say which project owns" in str(caught.value)


def test_a_stranger_wearing_our_container_names_refuses(tmp_path: Path) -> None:
    """Two installs of one game share container names; the label is the only proof."""
    rec = _recorder(tmp_path, running=docker.Running(strangers=((SPEC.world, "someone-else"),)))
    with pytest.raises(purge.PurgeError) as caught:
        rec.uninstaller().run(keep_characters=False)
    assert "someone-else" in str(caught.value)


# -- 2. the running refusal ------------------------------------------------


def test_a_purge_of_a_running_server_refuses_and_says_to_stop_it_first(
    tmp_path: Path,
) -> None:
    """Asked of the census that knows, not of a flag a caller passed in.

    `_running().ours` is what compose actually stamped, taken before any
    command is issued. Deleting the database volume out from under a live
    mysqld loses exactly the characters a ticked box promised to keep.
    """
    rec = _recorder(tmp_path, running=docker.Running(ours=(SPEC.world, SPEC.db)))
    with pytest.raises(purge.PurgeError) as caught:
        rec.uninstaller().run(keep_characters=True)
    message = str(caught.value)
    assert SPEC.world in message
    assert "Stop the server first" in message
    assert "snapshot" not in rec.order, "a refusal must not have written anything"


def test_the_running_refusal_is_asked_before_any_removal_command(tmp_path: Path) -> None:
    rec = _recorder(tmp_path, running=docker.Running(ours=(SPEC.world,)))
    with pytest.raises(purge.PurgeError):
        rec.uninstaller().run(keep_characters=False)
    assert not any(step.startswith("remove_") for step in rec.order), rec.order


def test_the_plan_of_a_running_install_says_so_rather_than_offering_a_purge(
    tmp_path: Path,
) -> None:
    rec = _recorder(tmp_path, running=docker.Running(ours=(SPEC.world,)))
    plan = rec.uninstaller().plan()
    assert "Stop the server first" in plan.refusal


# -- 3. the order ----------------------------------------------------------


def test_the_snapshot_is_first_and_the_record_is_forgotten_last(tmp_path: Path) -> None:
    """The whole order, asserted as a sequence rather than as a set of facts.

    A test that merely checks both happened passes with the two swapped. This
    one names the sequence, so a snapshot taken after `compose down` (the
    container's log goes with the container) or a record forgotten before the
    folder is gone (an install nobody owns, which is the one state the ownership
    refusal cannot recover from) fails here.
    """
    rec = _recorder(tmp_path)
    project = "yulon-wow-wotlk-deadbeef"
    rec.uninstaller().run(keep_characters=False)
    destructive = [step for step in rec.order if step.startswith(("snapshot", "remove_", "forget"))]
    assert destructive == [
        "snapshot",
        "remove_containers",
        f"remove_volume:{project}_db-data",
        f"remove_volume:{project}_client-data",
        *[f"remove_image:{ref}" for ref in _images(rec.server_dir)],
        f"remove_folder:{rec.server_dir}",
        "forget",
    ], rec.order


def test_the_snapshot_runs_while_the_containers_are_still_there(tmp_path: Path) -> None:
    """Not "both happened" — the snapshot seam refuses if the containers are gone.

    `logsnap.capture()` resolves the container through `compose ps` in the
    server dir, so it can only answer while both the containers and the compose
    files exist. Swap the two and this raises out of the snapshot.
    """
    rec = _recorder(tmp_path)

    def snapshot() -> logsnap.Snapshot:
        assert "remove_containers" not in rec.order, "the log was taken after the container went"
        assert rec.server_dir.exists(), "the log was taken after the folder went"
        rec.order.append("snapshot")
        return logsnap.Snapshot(path=Path("/logs/snap.log"))

    rec.uninstaller(snapshot=snapshot).run(keep_characters=False)
    assert rec.order.index("snapshot") < rec.order.index("remove_containers")


def test_a_folder_that_will_not_delete_leaves_the_record_pointing_at_it(
    tmp_path: Path,
) -> None:
    """The recoverable failure. Forget the record first and no surface can reach it again."""

    def explode(path: Path) -> None:
        raise purge.PurgeError(f"{path} could not be deleted")

    rec = _recorder(tmp_path, remove_folder=explode)
    with pytest.raises(purge.PurgeError):
        rec.uninstaller().run(keep_characters=False)
    assert rec.forgotten == 0, "the record was forgotten for a server that is still there"


def test_a_snapshot_that_fails_does_not_prevent_the_uninstall(tmp_path: Path) -> None:
    """`logsnap` never raises and never blocks the action it runs in front of."""
    rec = _recorder(tmp_path, snapshot=logsnap.Snapshot(problem="no such container"))
    report = rec.uninstaller().run(keep_characters=False)
    assert report.snapshot.problem == "no such container"
    assert rec.forgotten == 1


def test_the_record_is_still_forgotten_when_the_state_file_cannot_be_written(
    tmp_path: Path,
) -> None:
    """Everything else is gone, so this is a warning on a finished job, not a failure."""

    def forget() -> None:
        rec.order.append("forget")
        raise OSError("read-only file system")

    rec = _recorder(tmp_path)
    report = rec.uninstaller(forget=forget).run(keep_characters=False)
    assert report.record_forgotten is False
    assert any("state.json" in warning for warning in report.warnings), report.warnings


# -- 4. the scope ----------------------------------------------------------


def test_nothing_outside_this_projects_own_objects_is_ever_named(tmp_path: Path) -> None:
    """The neighbour is a second WotLK install: same container names, another project.

    Its volumes carry ITS project prefix and its images ITS install id. Nothing
    this purge issues may name any of them, and the container half goes through
    the project label rather than through the globally-pinned names.
    """
    neighbour = "yulon-wow-wotlk-cafebabe"
    rec = _recorder(tmp_path)
    rec.uninstaller().run(keep_characters=False)
    said = " ".join(rec.order)
    assert neighbour not in said
    assert f"{neighbour}_db-data" not in rec.removed_volumes
    for step in rec.order:
        if step.startswith(("census:", "containers:", "volumes:")):
            assert step.endswith("yulon-wow-wotlk-deadbeef"), step


def test_every_volume_removed_was_one_docker_listed_for_this_project(tmp_path: Path) -> None:
    """Discovered by label, never computed from a folder basename.

    The bash prior art hardcoded `wow-server-playerbots_ac-database` out of an
    assumed folder name, which either misses or hits somebody else's data.
    """
    rec = _recorder(tmp_path, volumes=["yulon-wow-wotlk-deadbeef_db-data"])
    rec.uninstaller().run(keep_characters=False)
    assert rec.removed_volumes == ["yulon-wow-wotlk-deadbeef_db-data"]


def test_only_this_installs_own_four_image_refs_are_removed(tmp_path: Path) -> None:
    """`--rmi all` would take `mysql:8.4` with it; `--rmi local` would take nothing."""
    rec = _recorder(tmp_path)
    rec.uninstaller().run(keep_characters=False)
    assert rec.removed_images == list(_images(rec.server_dir))
    assert not any("mysql" in ref for ref in rec.removed_images)
    assert not any("alpine" in ref for ref in rec.removed_images)


def test_an_image_still_in_use_is_a_warning_and_not_a_failed_uninstall(
    tmp_path: Path,
) -> None:
    """A shared base layer another title holds is not this uninstall's problem."""
    rec = _recorder(tmp_path, image_warning="in use by another container")
    report = rec.uninstaller().run(keep_characters=False)
    assert report.warnings
    assert rec.forgotten == 1


def test_the_purge_never_prunes_anything(tmp_path: Path) -> None:
    """`image prune`, `builder prune` and `system prune` reach the whole daemon."""
    source = (Path(__file__).resolve().parents[1] / "yulon" / "purge.py").read_text(
        encoding="utf-8"
    )
    for word in ("prune", "--rmi", "system"):
        assert f'"{word}"' not in source, word


# -- 5. keep my characters -------------------------------------------------


def test_ticked_keeps_exactly_the_database_volume_and_removes_the_other(
    tmp_path: Path,
) -> None:
    """One object survives: `<project>_db-data`, which IS the three acore schemas."""
    rec = _recorder(tmp_path)
    report = rec.uninstaller().run(keep_characters=True)
    assert report.kept_volumes == ("yulon-wow-wotlk-deadbeef_db-data",)
    assert rec.removed_volumes == ["yulon-wow-wotlk-deadbeef_client-data"]


def test_ticked_still_removes_the_folder_and_the_record(tmp_path: Path) -> None:
    """ "Keep my characters" is about one volume, not about keeping the install.

    The definition of done is that a reinstall TO THE SAME FOLDER finds the
    characters, which needs the folder gone so the installer has somewhere to
    install.
    """
    rec = _recorder(tmp_path)
    rec.uninstaller().run(keep_characters=True)
    assert f"remove_folder:{rec.server_dir}" in rec.order
    assert rec.forgotten == 1


def test_the_kept_volume_is_the_name_a_reinstall_to_this_folder_recomputes(
    tmp_path: Path,
) -> None:
    """The whole chain the checkbox rests on, asserted without Docker.

    The volume is `<project>_db-data`; the project is
    `yulon-<game>-<install_id>`; `install_id` is a hash of the folder's
    absolute path. So a reinstall to the SAME folder computes the same name and
    compose binds the pre-existing volume rather than creating an empty one.
    The decisions doc marks this "unverified — owed a test"; this is its code
    half, and the live gate is the other.
    """
    server_dir = tmp_path / "wowserver"
    server_dir.mkdir(exist_ok=True)
    project = composegen.project_name(GAME, server_dir)
    rec = Recorder(server_dir, project=project)
    report = rec.uninstaller().run(keep_characters=True)
    reinstalled = composegen.project_name(GAME, server_dir)
    assert report.kept_volumes == (f"{reinstalled}_db-data",)


def test_unticked_leaves_no_container_volume_image_folder_or_record(
    tmp_path: Path,
) -> None:
    """The other direction of the checkbox, asserted as a whole."""
    rec = _recorder(tmp_path)
    report = rec.uninstaller().run(keep_characters=False)
    assert report.removed_containers is True
    assert report.kept_volumes == ()
    assert sorted(report.removed_volumes) == [
        "yulon-wow-wotlk-deadbeef_client-data",
        "yulon-wow-wotlk-deadbeef_db-data",
    ]
    assert report.removed_images == _images(rec.server_dir)
    assert report.folder_removed is True
    assert report.record_forgotten is True


def test_a_ticked_box_refuses_when_the_database_volume_cannot_be_identified(
    tmp_path: Path,
) -> None:
    """Fail closed: promising to keep characters and finding no volume to keep is a lie."""
    rec = _recorder(tmp_path, volumes=["yulon-wow-wotlk-deadbeef_client-data"])
    with pytest.raises(purge.PurgeError) as caught:
        rec.uninstaller().run(keep_characters=True)
    assert "database volume" in str(caught.value)
    assert rec.removed_volumes == []


def test_docker_refusing_to_list_volumes_refuses_the_purge(tmp_path: Path) -> None:
    """A volume list that could not be read is not an empty one."""
    rec = _recorder(tmp_path, volumes=None)
    with pytest.raises(purge.PurgeError) as caught:
        rec.uninstaller().run(keep_characters=False)
    assert "could not ask Docker" in str(caught.value)


# -- the plan --------------------------------------------------------------


def test_the_plan_names_the_folder_its_size_and_what_would_go(tmp_path: Path) -> None:
    """The dialog's numbers come from here, and `plan()` writes nothing."""
    rec = _recorder(tmp_path)
    plan = rec.uninstaller().plan()
    assert plan.refusal == ""
    assert plan.server_dir == rec.server_dir
    assert plan.folder_bytes == 2_300_000_000
    assert plan.containers == (SPEC.world, SPEC.db)
    assert plan.character_volume == "yulon-wow-wotlk-deadbeef_db-data"
    assert plan.images == _images(rec.server_dir)
    assert not any(step.startswith(("snapshot", "remove_", "forget")) for step in rec.order)


def test_the_plan_says_what_it_leaves_behind(tmp_path: Path) -> None:
    """Owner answer 1 is a list of exclusions, and a blast radius has to be readable.

    Backups especially: an uninstall that deleted them would make "Keep my
    characters" pointless in the one case it matters.
    """
    rec = _recorder(tmp_path)
    plan = rec.uninstaller().plan()
    left = " ".join(plan.left_behind).lower()
    assert "backup" in left
    assert "client" in left


# -- 7. the Windows read-only bit -----------------------------------------


def test_a_tree_whose_files_are_read_only_is_still_deleted(tmp_path: Path) -> None:
    """Git writes packs and loose objects read-only; a bare rmtree stops on them.

    Simulated here with a directory whose write bit is cleared, which is the
    POSIX shape of the same stop: `unlink` inside it is refused. The retry
    clears the bit and finishes. The Windows spelling of this
    (FILE_ATTRIBUTE_READONLY on the file itself) is unproven on this box — see
    the gate plan.
    """
    tree = tmp_path / "checkout"
    (tree / ".git" / "objects").mkdir(parents=True)
    victim = tree / ".git" / "objects" / "pack"
    victim.write_text("x", encoding="utf-8")
    victim.chmod(0o444)
    (tree / ".git" / "objects").chmod(0o555)

    purge.remove_tree(tree)
    assert not tree.exists()


@pytest.mark.skipif(
    os.name == "nt" or os.geteuid() == 0,  # type: ignore[attr-defined]
    reason=(
        "needs a directory whose write bit actually refuses an unlink: Windows ignores "
        "chmod on a directory, and root ignores the bit. The Windows spelling of this "
        "failure is its own press on the gate box."
    ),
)
def test_a_delete_that_cannot_finish_says_so_instead_of_reporting_success(
    tmp_path: Path,
) -> None:
    """The Rust prior art is `let _ = remove_dir_all(...)` and reports success having
    deleted nothing. This must fail loudly and name the path."""
    parent = tmp_path / "locked"
    tree = parent / "checkout"
    tree.mkdir(parents=True)
    (tree / "file").write_text("x", encoding="utf-8")
    parent.chmod(0o555)
    try:
        with pytest.raises(purge.PurgeError) as caught:
            purge.remove_tree(tree)
        assert str(tree) in str(caught.value)
        assert "read-only" in str(caught.value).lower()
    finally:
        parent.chmod(0o755)


def test_a_wsl_resident_install_is_refused_rather_than_half_deleted(
    tmp_path: Path,
) -> None:
    """The folder is inside the distro and cannot be `rmtree`d from Windows.

    Refused cleanly rather than half-done: nothing here runs a command inside a
    distro yet, and a purge that removed the Docker objects and left the folder
    would leave an install the ownership refusal can no longer authorise.
    """
    rec = _recorder(tmp_path)
    with pytest.raises(purge.PurgeError) as caught:
        rec.uninstaller(wsl_distro="Ubuntu").run(keep_characters=False)
    assert "WSL" in str(caught.value)
    assert "snapshot" not in rec.order
