"""T700: Install never runs apt, systemctl or any other root command without a yes.

Every test goes through the real `platform.ensure_docker()` with a fake runner, so
the argv a box would have received is what is asserted. The machine under test is
the one the audit measured: passwordless sudo (every `sudo -n` step succeeds, so no
password dialog ever stands in for a question), `docker info` failing, and either
docker-ce installed but stopped or no engine at all.
"""

from __future__ import annotations

import subprocess
from collections.abc import Callable
from pathlib import Path

import pytest

from yulon import platform


class _Box:
    """Passwordless sudo: records argv, answers `docker info` with 1, everything else with 0."""

    def __init__(self, docker_rc: int = 1) -> None:
        self.calls: list[list[str]] = []
        self.docker_rc = docker_rc

    def __call__(self, argv: list[str]) -> subprocess.CompletedProcess[str]:
        self.calls.append(argv)
        if argv[:2] == ["docker", "info"]:
            return subprocess.CompletedProcess(argv, self.docker_rc, "", "")
        if argv[0] == "id":
            return subprocess.CompletedProcess(argv, 0, "pk docker", "")
        return subprocess.CompletedProcess(argv, 0, "", "")

    def privileged(self) -> list[list[str]]:
        """Everything that ran as root or changed the package set or a service."""
        return [c for c in self.calls if c[:1] == ["sudo"]]


def _machine(
    monkeypatch: pytest.MonkeyPatch,
    *,
    unit: bool = False,
    desktop: bool = False,
    binaries: tuple[str, ...] = (),
    pm: str = "apt-get",
) -> Callable[[str], str | None]:
    monkeypatch.setattr(platform.sys, "platform", "linux")
    monkeypatch.setattr(platform, "is_steamos", lambda: False)
    monkeypatch.setattr(platform, "_docker_service_installed", lambda: unit)
    monkeypatch.setattr(platform, "_docker_desktop_installed", lambda find: desktop)
    present = {pm, *binaries}
    return lambda name: f"/usr/bin/{name}" if name in present else None


def _asker(reply: str | None) -> tuple[list[str], Callable[[str], str | None]]:
    asked: list[str] = []

    def ask(question: str) -> str | None:
        asked.append(question)
        return reply

    return asked, ask


def _ensure(box: _Box, which: Callable[[str], str | None], ask=None, **kw):  # type: ignore[no-untyped-def]
    return platform.ensure_docker(run=box, which=which, user="pk", wait_seconds=0.0, ask=ask, **kw)


APT_VERBS = ("apt-get", "dpkg", "snap", "pacman", "dnf", "zypper")


def _package_manager_ran(box: _Box) -> list[list[str]]:
    return [c for c in box.calls if any(v in c for v in APT_VERBS)]


# ---- installed but stopped: ask to start it, never touch the package manager


def test_a_stopped_engine_is_asked_about_even_with_passwordless_sudo(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    which = _machine(monkeypatch, unit=True, binaries=("docker", "dockerd"))
    box = _Box()
    asked, ask = _asker("y")
    _ensure(box, which, ask)
    start_questions = [q for q in asked if "installed" in q and "not answering" in q]
    assert len(start_questions) == 1, asked
    # The question comes before the first root command, and names it.
    assert "systemctl start docker" in start_questions[0]
    assert "root" in start_questions[0]
    assert "Docker Desktop" not in start_questions[0]
    assert box.privileged() == [["sudo", "-n", "systemctl", "start", "docker"]]


def test_a_stopped_engine_never_reaches_the_package_manager(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    which = _machine(monkeypatch, unit=True, binaries=("docker", "dockerd"))
    box = _Box()
    _asked, ask = _asker("y")
    report = _ensure(box, which, ask)
    assert not _package_manager_ran(box), box.calls
    assert not any("enable" in c for c in box.calls), box.calls
    assert "docker.io" not in " ".join(map(" ".join, box.calls))
    assert report.platform == "linux"


@pytest.mark.parametrize("reply", [None, "", "n", "no", "maybe"])
def test_a_no_to_starting_runs_nothing(monkeypatch: pytest.MonkeyPatch, reply: str | None) -> None:
    which = _machine(monkeypatch, unit=True, binaries=("docker",))
    box = _Box()
    asked, ask = _asker(reply)
    report = _ensure(box, which, ask)
    assert asked, "the question was never put"
    assert box.privileged() == [], box.calls
    assert not _package_manager_ran(box)
    assert any("sudo systemctl start docker" in m for m in report.manual_steps), report


def test_nobody_to_ask_means_nothing_runs_as_root(monkeypatch: pytest.MonkeyPatch) -> None:
    which = _machine(monkeypatch, unit=True, binaries=("docker",))
    box = _Box()
    report = _ensure(box, which, None)
    assert box.privileged() == [], box.calls
    assert report.docker_ready is False
    assert any("systemctl start docker" in m for m in report.manual_steps)


def test_an_engine_with_no_service_unit_is_left_alone_and_not_overwritten(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """A snap, static or CLI-only docker: another engine must never be installed over it."""
    which = _machine(monkeypatch, unit=False, binaries=("docker",))
    box = _Box()
    asked, ask = _asker("y")
    report = _ensure(box, which, ask)
    assert box.privileged() == [], box.calls
    assert not _package_manager_ran(box)
    assert not any("Install Docker Engine" in q for q in asked), asked
    assert any("not answering" in m and "another copy" in m for m in report.manual_steps), report


# ---- Docker Desktop for Linux: its own user service, no root, no packages


def test_a_stopped_docker_desktop_is_started_as_the_user_after_a_yes(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    which = _machine(monkeypatch, desktop=True, binaries=("docker",))
    box = _Box()
    asked, ask = _asker("y")
    _ensure(box, which, ask)
    assert asked and "Docker Desktop" in asked[0]
    assert ["systemctl", "--user", "start", "docker-desktop"] in box.calls
    assert box.privileged() == [], box.calls


def test_a_no_to_starting_docker_desktop_runs_nothing(monkeypatch: pytest.MonkeyPatch) -> None:
    which = _machine(monkeypatch, desktop=True, binaries=("docker",))
    box = _Box()
    _asked, ask = _asker("n")
    _ensure(box, which, ask)
    assert not [c for c in box.calls if "start" in c], box.calls


# ---- no engine at all: ask before installing, naming what will run


def test_installing_asks_first_even_with_passwordless_sudo(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    which = _machine(monkeypatch)
    box = _Box()
    asked, ask = _asker("y")
    _ensure(box, which, ask)
    install = [q for q in asked if "not installed" in q]
    assert len(install) == 1, asked
    q = install[0]
    # Exactly what will run: the packages, the root level, and "Docker Engine" on Linux.
    assert "docker.io" in q and "docker-compose-v2" in q and "docker-buildx" in q
    assert "root" in q
    assert "Docker Desktop" not in q
    assert q.rstrip().endswith("(y/n):")
    first_root = next(i for i, c in enumerate(box.calls) if c[:1] == ["sudo"])
    assert first_root > 0
    assert [
        "sudo",
        "-n",
        "apt-get",
        "install",
        "-y",
        "docker.io",
        "docker-compose-v2",
        "docker-buildx",
    ] in box.calls


@pytest.mark.parametrize("reply", [None, "", "n", "no", "nope"])
def test_a_no_to_installing_runs_nothing_privileged(
    monkeypatch: pytest.MonkeyPatch, reply: str | None
) -> None:
    which = _machine(monkeypatch)
    box = _Box()
    asked, ask = _asker(reply)
    report = _ensure(box, which, ask)
    assert asked
    assert box.privileged() == [], box.calls
    assert not _package_manager_ran(box)
    assert report.done == ()
    assert any("apt-get install -y docker.io" in m for m in report.manual_steps), report


def test_nobody_to_ask_installs_nothing(monkeypatch: pytest.MonkeyPatch) -> None:
    which = _machine(monkeypatch)
    box = _Box()
    report = _ensure(box, which, None)
    assert box.privileged() == [], box.calls
    assert any("apt-get install -y docker.io" in m for m in report.manual_steps)


def test_a_dry_run_still_shows_the_plan_and_asks_nothing(monkeypatch: pytest.MonkeyPatch) -> None:
    which = _machine(monkeypatch)
    box = _Box()
    asked, ask = _asker("y")
    report = _ensure(box, which, ask, dry_run=True)
    assert asked == []
    assert box.privileged() == []
    assert report.skipped[0] == "(dry run) apt-get update"


def test_a_cancelled_run_asks_nothing(monkeypatch: pytest.MonkeyPatch) -> None:
    import threading

    which = _machine(monkeypatch)
    box = _Box()
    asked, ask = _asker("y")
    stop = threading.Event()
    stop.set()
    _ensure(box, which, ask, cancel=stop)
    assert asked == []
    assert box.privileged() == []


def test_the_engine_probe_is_a_filesystem_check_that_needs_no_command(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    unit_dir = tmp_path / "system"
    unit_dir.mkdir()
    monkeypatch.setattr(platform, "_SYSTEMD_UNIT_DIRS", (str(unit_dir),))
    assert platform._docker_service_installed() is False
    (unit_dir / "docker.socket").write_text("[Socket]\n", encoding="utf-8")
    assert platform._docker_service_installed() is True
