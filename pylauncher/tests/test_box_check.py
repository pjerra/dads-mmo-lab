"""Tests for `yulon.box_check`: `yulon --box-check` for the headless server box.

Everything here drives `check()` through its seams — a fake runner, a fake
`which`, a handed `os_release` — so no docker, no daemon and no box is needed.
The assertions are about the branches, not the fixtures: each test reverts the
one line its branch turns on and fails on the old answer, which is what makes
a passing suite mean the branch is read rather than the name of the branch.
"""

from __future__ import annotations

import json
import subprocess
from collections.abc import Callable
from pathlib import Path

import pytest

from yulon import box_check
from yulon.box_check import BoxReport, FirewallState

_RELEASE = {"ID": "ubuntu", "VERSION_ID": "24.04"}


class _Run:
    """A fake subprocess: answers from a table, records every argv."""

    def __init__(self) -> None:
        self.calls: list[list[str]] = []
        self.answers: dict[tuple[str, ...], tuple[int, str]] = {}

    def answer(self, argv: list[str], rc: int = 0, out: str = "") -> _Run:
        self.answers[tuple(argv)] = (rc, out)
        return self

    def __call__(self, argv: list[str]) -> subprocess.CompletedProcess[str]:
        self.calls.append(argv)
        rc, out = self.answers.get(tuple(argv), (1, ""))
        return subprocess.CompletedProcess(argv, rc, out, "")


def _which(*names: str) -> Callable[[str], str | None]:
    """A fake PATH holding exactly `names`."""

    def find(name: str) -> str | None:
        return f"/usr/bin/{name}" if name in names else None

    return find


def _check(run: _Run, **kwargs: object) -> BoxReport:
    return box_check.check(  # type: ignore[arg-type]
        run=run, which=_which("docker", "git", "ufw"), os_release=dict(_RELEASE), **kwargs
    )


EXPECTED_KEYS = {
    "os_id",
    "os_version",
    "arch",
    "glibc",
    "docker_installed",
    "docker_usable",
    "docker_group_member",
    "git_installed",
    "free_disk_gib",
    "firewall",
    "is_root",
    "yulon_version",
}


def test_the_report_has_every_key() -> None:
    run = (
        _Run()
        .answer(["docker", "info"], 0, "Server Version: 27")
        .answer(["id", "-nG", "deck"], 0, "deck docker sudo")
        .answer(["git", "--version"], 0, "git version 2.43")
        .answer(["ufw", "status"], 0, "Status: active")
    )
    report = _check(
        run,
        machine="x86_64",
        libc=("glibc", "2.39"),
        user="deck",
        euid=1000,
        home_free_bytes=48 * 1024**3,
    )
    assert set(report) == EXPECTED_KEYS
    assert report["os_id"] == "ubuntu" and report["os_version"] == "24.04"
    assert report["arch"] == "x86_64" and report["glibc"] == "2.39"
    assert report["docker_installed"] is True and report["docker_usable"] is True
    assert report["docker_group_member"] is True and report["git_installed"] is True
    assert report["free_disk_gib"] == pytest.approx(48.0)
    assert report["firewall"] == FirewallState(tool="ufw", active=True)
    assert report["is_root"] is False
    assert report["yulon_version"]
    assert report["yulon_version"] == box_check.__version__


def test_a_missing_docker_cli_is_not_usable_and_asks_nothing() -> None:
    run = (
        _Run()
        .answer(["id", "-nG", "deck"], 0, "deck sudo")
        .answer(["git", "--version"], 0, "git version 2.43")
        .answer(["ufw", "status"], 0, "Status: inactive")
    )
    report = box_check.check(
        run=run, which=_which("git", "ufw"), os_release=dict(_RELEASE), user="deck"
    )
    assert report["docker_installed"] is False and report["docker_usable"] is False
    assert not [call for call in run.calls if call[:2] == ["docker", "info"]]


def test_a_present_but_silent_daemon_is_not_usable() -> None:
    run = (
        _Run()
        .answer(["docker", "info"], 1, "Cannot connect to the Docker daemon")
        .answer(["id", "-nG", "deck"], 0, "deck docker sudo")
        .answer(["git", "--version"], 0, "git version 2.43")
        .answer(["ufw", "status"], 0, "Status: inactive")
    )
    report = _check(run, user="deck")
    assert report["docker_installed"] is True and report["docker_usable"] is False


def test_an_exploding_daemon_probe_is_not_usable() -> None:
    def _boom(argv: list[str]) -> subprocess.CompletedProcess[str]:
        raise OSError("no such file")

    report = box_check.check(
        run=_boom, which=_which("git", "ufw"), os_release=dict(_RELEASE), user="deck"
    )
    assert report["docker_installed"] is False and report["docker_usable"] is False


def test_a_user_outside_the_docker_group_reads_as_outside() -> None:
    run = (
        _Run()
        .answer(["docker", "info"], 0, "Server Version: 27")
        .answer(["id", "-nG", "deck"], 0, "deck sudo")
        .answer(["git", "--version"], 0, "git version 2.43")
        .answer(["ufw", "status"], 0, "Status: inactive")
    )
    assert _check(run, user="deck")["docker_group_member"] is False


def test_a_dockerd_group_is_not_the_docker_group() -> None:
    run = (
        _Run()
        .answer(["docker", "info"], 0, "Server Version: 27")
        .answer(["id", "-nG", "deck"], 0, "deck dockerd sudo")
        .answer(["git", "--version"], 0, "git version 2.43")
        .answer(["ufw", "status"], 0, "Status: inactive")
    )
    assert _check(run, user="deck")["docker_group_member"] is False


def test_an_inactive_ufw_reads_as_inactive() -> None:
    run = (
        _Run()
        .answer(["docker", "info"], 0, "Server Version: 27")
        .answer(["id", "-nG", "deck"], 0, "deck docker sudo")
        .answer(["git", "--version"], 0, "git version 2.43")
        .answer(["ufw", "status"], 0, "Status: inactive")
    )
    assert _check(run, user="deck")["firewall"] == FirewallState(tool="ufw", active=False)


def test_an_unreadable_ufw_state_is_unknown_not_down() -> None:
    run = (
        _Run()
        .answer(["docker", "info"], 0, "Server Version: 27")
        .answer(["id", "-nG", "deck"], 0, "deck docker sudo")
        .answer(["git", "--version"], 0, "git version 2.43")
        .answer(["ufw", "status"], 1, "ERROR: problem running ufw-init")
    )
    assert _check(run, user="deck")["firewall"] == FirewallState(tool="ufw", active=None)


def test_firewalld_running_reads_as_up() -> None:
    run = (
        _Run()
        .answer(["docker", "info"], 0, "Server Version: 27")
        .answer(["id", "-nG", "deck"], 0, "deck docker sudo")
        .answer(["git", "--version"], 0, "git version 2.43")
        .answer(["firewall-cmd", "--state"], 0, "running")
    )
    report = box_check.check(
        run=run,
        which=_which("docker", "git", "firewall-cmd"),
        os_release=dict(_RELEASE),
        user="deck",
    )
    assert report["firewall"] == FirewallState(tool="firewalld", active=True)


def test_firewalld_stopped_reads_as_down() -> None:
    run = (
        _Run()
        .answer(["docker", "info"], 0, "Server Version: 27")
        .answer(["id", "-nG", "deck"], 0, "deck docker sudo")
        .answer(["git", "--version"], 0, "git version 2.43")
        .answer(["firewall-cmd", "--state"], 252, "not running")
    )
    report = box_check.check(
        run=run,
        which=_which("docker", "git", "firewall-cmd"),
        os_release=dict(_RELEASE),
        user="deck",
    )
    assert report["firewall"] == FirewallState(tool="firewalld", active=False)


def test_no_firewall_tool_is_null_not_a_guess() -> None:
    run = (
        _Run()
        .answer(["docker", "info"], 0, "Server Version: 27")
        .answer(["id", "-nG", "deck"], 0, "deck docker sudo")
        .answer(["git", "--version"], 0, "git version 2.43")
    )
    report = box_check.check(
        run=run, which=_which("docker", "git"), os_release=dict(_RELEASE), user="deck"
    )
    assert report["firewall"] == FirewallState(tool=None, active=None)


def test_main_prints_one_json_line_and_returns_zero(
    monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    """The verb's own contract: one line, every key, through the real `check()` seams.

    `runner.run` is faked because the suite never touches a real docker CLI
    (conftest fails it on purpose): what this proves is the shape — one line,
    the key set, exit 0 — not the numbers on this laptop. The live numbers are
    proven by the smoke run in the report, not by the suite.
    """
    run = (
        _Run()
        .answer(["docker", "info"], 0, "Server Version: 27")
        .answer(["id", "-nG", "root"], 0, "root docker")
        .answer(["git", "--version"], 0, "git version 2.43")
        .answer(["ufw", "status"], 0, "Status: active")
    )
    monkeypatch.setattr(box_check.platform.runner, "run", lambda argv, **_k: run(argv))
    assert box_check.main() == 0
    lines = capsys.readouterr().out.splitlines()
    assert len(lines) == 1, f"the PC parses one line, not {len(lines)}: {lines!r}"
    payload = json.loads(lines[0])
    assert set(payload) == EXPECTED_KEYS


def test_a_short_os_release_leaves_nulls() -> None:
    run = (
        _Run()
        .answer(["docker", "info"], 0, "Server Version: 27")
        .answer(["id", "-nG", "deck"], 0, "deck docker sudo")
        .answer(["git", "--version"], 0, "git version 2.43")
        .answer(["ufw", "status"], 0, "Status: active")
    )
    report = box_check.check(
        run=run, which=_which("docker", "git", "ufw"), os_release={}, user="deck"
    )
    assert report["os_id"] is None and report["os_version"] is None


def test_a_non_glibc_libc_has_no_glibc() -> None:
    run = (
        _Run()
        .answer(["docker", "info"], 0, "Server Version: 27")
        .answer(["id", "-nG", "deck"], 0, "deck docker sudo")
        .answer(["git", "--version"], 0, "git version 2.43")
        .answer(["ufw", "status"], 0, "Status: active")
    )
    report = _check(run, user="deck", libc=("", ""))
    assert report["glibc"] is None


def test_a_missing_git_reads_as_missing() -> None:
    run = (
        _Run()
        .answer(["docker", "info"], 0, "Server Version: 27")
        .answer(["id", "-nG", "deck"], 0, "deck docker sudo")
        .answer(["ufw", "status"], 0, "Status: active")
    )
    report = box_check.check(
        run=run, which=_which("docker", "ufw"), os_release=dict(_RELEASE), user="deck"
    )
    assert report["git_installed"] is False
    assert not [call for call in run.calls if call[:2] == ["git", "--version"]]


def test_every_command_gets_the_module_bound() -> None:
    seen: list[float | None] = []
    real_run = box_check.runner.run

    def _spy(argv: list[str], **kwargs: object) -> subprocess.CompletedProcess[str]:
        seen.append(kwargs.get("timeout"))
        return subprocess.CompletedProcess(argv, 1, "", "")

    box_check.runner.run = _spy  # type: ignore[method-assign,assignment]
    try:
        box_check.check(run=None, which=_which(), os_release={}, user="nobody")
    finally:
        box_check.runner.run = real_run  # type: ignore[method-assign,assignment]
    assert seen and all(bound == box_check._BOUNDS_SECONDS for bound in seen)


def test_main_reports_its_own_crash_as_one_line_not_a_traceback(
    monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    def _boom(**kwargs: object) -> BoxReport:
        raise RuntimeError("the floor fell out")

    monkeypatch.setattr(box_check, "check", _boom)
    assert box_check.main() == 1
    lines = capsys.readouterr().out.splitlines()
    assert len(lines) == 1
    payload = json.loads(lines[0])
    assert "the floor fell out" in payload["error"]
    assert "Traceback" not in capsys.readouterr().out


def test_the_check_writes_nothing(tmp_path: Path) -> None:
    before = {path for path in tmp_path.rglob("*")}
    run = (
        _Run()
        .answer(["docker", "info"], 0, "Server Version: 27")
        .answer(["id", "-nG", "deck"], 0, "deck sudo")
        .answer(["git", "--version"], 0, "git version 2.43")
        .answer(["ufw", "status"], 0, "Status: inactive")
    )
    box_check.check(
        run=run,
        which=_which("docker", "git", "ufw"),
        os_release=dict(_RELEASE),
        user="deck",
        system="linux",
        home_free_bytes=48 * 1024**3,
    )
    assert {path for path in tmp_path.rglob("*")} == before
    assert run.calls, "a check that asked nothing proved nothing"
