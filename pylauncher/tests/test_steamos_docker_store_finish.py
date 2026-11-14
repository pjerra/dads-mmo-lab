"""T884 round 5: one finish for every Deck store change.

Round 4 finished the store change in the switch only, and even there it read a
failed `docker info` as Docker refusing the settings — and "started again" with
`systemctl start` on a unit that was already active, a no-op that leaves Docker
running on /var while `/etc` points at /home. The T160 repair kept the new
settings when Docker then failed, because it never finished the change at all.

One helper, `_finish_store_change()`, now runs for the switch, the repair and a
fresh install once the store plan was applied: a `restart` (never a `start`),
a readiness poll, and an undo only when the unit is INACTIVE or the restart
itself failed. A unit that is active while this process cannot reach the socket
keeps the settings and gets the session sentence.

No sudo, no systemctl, no disk outside `tmp_path`: every command is scripted.
"""

from __future__ import annotations

import subprocess
import tempfile
from collections.abc import Sequence
from pathlib import Path

import pytest

from tests.test_steamos_docker_repair import _answers, _repair
from tests.test_steamos_docker_repair import _Deck as _RepairDeck
from yulon import platform

GIB = 1024**3
GROUP_QUESTION_MARK = "add 'deck' to the docker group"


class _SwitchDeck:
    """A scripted Deck for the switch: restarts, readiness, and the unit's own state.

    `ready_after_restarts` is how many `systemctl restart` calls have to have
    run before `docker info` answers; None means it never does. `unit_active`
    is what `systemctl is-active docker` says while `docker info` does not
    answer, which is the difference between "Docker refused the settings" and
    "this process cannot reach the socket".
    """

    def __init__(
        self,
        *,
        ready_after_restarts: int | None = None,
        unit_active: bool = False,
        restart_failures: int = 0,
    ) -> None:
        self.calls: list[list[str]] = []
        self.restarts = 0
        self.ready_after_restarts = ready_after_restarts
        self.unit_active = unit_active
        self.restart_failures = restart_failures

    def _ready(self) -> bool:
        return self.ready_after_restarts is not None and self.restarts >= self.ready_after_restarts

    def __call__(self, argv: list[str]) -> subprocess.CompletedProcess[str]:
        self.calls.append(argv)
        body = argv[2:] if argv[:2] == ["sudo", "-n"] else argv
        if argv[:2] == ["docker", "info"]:
            return subprocess.CompletedProcess(argv, 0 if self._ready() else 1, "", "")
        if argv == ["systemctl", "is-active", "docker"]:
            active = self.unit_active or self._ready()
            state = "active" if active else "inactive"
            return subprocess.CompletedProcess(argv, 0 if active else 3, state + "\n", "")
        if body[:2] == ["systemctl", "restart"]:
            self.restarts += 1
            if self.restarts <= self.restart_failures:
                return subprocess.CompletedProcess(argv, 1, "", "restart job failed")
            return subprocess.CompletedProcess(argv, 0, "", "")
        return subprocess.CompletedProcess(argv, 0, "", "")

    def steps(self) -> list[str]:
        """Every command as it would be typed after `sudo`, docker probes left out."""
        return [
            " ".join(argv[2:]) if argv[:2] == ["sudo", "-n"] else " ".join(argv)
            for argv in self.calls
            if argv[:2] != ["docker", "info"] and argv != ["systemctl", "is-active", "docker"]
        ]


class _InstallDeck(_SwitchDeck):
    """The same scripted Deck for `ensure_docker()`: probes and package steps answer 0."""

    def __call__(self, argv: list[str]) -> subprocess.CompletedProcess[str]:
        if argv[:2] == ["id", "-nG"]:
            self.calls.append(argv)
            return subprocess.CompletedProcess(argv, 0, "deck wheel docker\n", "")
        return super().__call__(argv)


class _FinishRepairDeck(_RepairDeck):
    """The repair's scripted Deck where readiness follows the RESTARTS (T884 r5).

    `systemctl enable --now docker` succeeding is what the old report trusted;
    here only a `restart` can make `docker info` answer, so a repair that never
    finishes the change cannot look ready by accident.
    """

    def __init__(self, *, ready_after_restarts: int | None = None, **kwargs: object) -> None:
        super().__init__(**kwargs)  # type: ignore[arg-type]
        self.ready_after_restarts = ready_after_restarts
        self.restarts = 0

    def _ready(self) -> bool:
        return self.ready_after_restarts is not None and self.restarts >= self.ready_after_restarts

    def __call__(self, argv: list[str]) -> subprocess.CompletedProcess[str]:
        if argv[:2] == ["docker", "info"]:
            self.calls.append(argv)
            return subprocess.CompletedProcess(argv, 0 if self._ready() else 1, "", "")
        if argv == ["systemctl", "is-active", "docker"]:
            self.calls.append(argv)
            state = "active" if self._ready() else "inactive"
            return subprocess.CompletedProcess(argv, 0 if self._ready() else 3, state + "\n", "")
        body = argv[2:] if argv[:2] == ["sudo", "-n"] else argv
        if body[:2] == ["systemctl", "restart"]:
            self.calls.append(argv)
            self.steps.append(" ".join(body))
            self.restarts += 1
            return subprocess.CompletedProcess(argv, 0, "", "")
        return super().__call__(argv)


def _deck(monkeypatch: pytest.MonkeyPatch) -> None:
    """A stock Deck: SteamOS, a 4 GiB /var, and the switch worth running."""
    monkeypatch.setattr(platform, "is_steamos", lambda: True)
    monkeypatch.setattr(platform, "_var_total_bytes", lambda: 4 * GIB)
    monkeypatch.setattr(platform, "on_var", lambda _path: True)
    monkeypatch.setattr(platform, "steamos_store_needs_home", lambda data_root=None: True)


def _settings(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path, *, present: bool = True
) -> tuple[Path, Path]:
    """The two settings files under `tmp_path`, there or not, as this Deck's own."""
    daemon = tmp_path / "etc" / "docker" / "daemon.json"
    config = tmp_path / "etc" / "containerd" / "config.toml"
    daemon.parent.mkdir(parents=True, exist_ok=True)
    config.parent.mkdir(parents=True, exist_ok=True)
    if present:
        daemon.write_text('{"log-driver": "local"}\n', encoding="utf-8")
        config.write_text('version = 2\nroot = "/var/lib/containerd"\n', encoding="utf-8")
    monkeypatch.setattr(platform, "_DOCKER_DAEMON_JSON", daemon)
    monkeypatch.setattr(platform, "_CONTAINERD_CONFIG", config)
    return daemon, config


def _spy_mkdtemp(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> list[Path]:
    """`tempfile.mkdtemp` that keeps its folders under `tmp_path` and records them."""
    made: list[Path] = []
    real = tempfile.mkdtemp

    def spy(*args: object, **kwargs: object) -> str:
        folder = real(*args, dir=str(tmp_path), **kwargs)  # type: ignore[arg-type]
        made.append(Path(folder))
        return folder

    monkeypatch.setattr(tempfile, "mkdtemp", spy)
    return made


def _restores(steps: Sequence[str]) -> list[str]:
    """The commands that put a settings file back: our `install -m 0644` and `rm -f`."""
    return [
        step for step in steps if step.startswith("install -m 0644 ") or step.startswith("rm -f ")
    ]


def _unreadable(monkeypatch: pytest.MonkeyPatch, path: Path) -> None:
    """Make exactly `path` fail its read with `PermissionError`, as a locked file does."""
    real = Path.read_text

    def read_text(self: Path, *args: object, **kwargs: object) -> str:
        if self == path:
            raise PermissionError(13, "Permission denied", str(self))
        return real(self, *args, **kwargs)  # type: ignore[arg-type]

    monkeypatch.setattr(Path, "read_text", read_text)


def _repair_deck(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> tuple[Path, Path]:
    """A Deck whose repair runs the real T884 store plan over `tmp_path`'s settings."""
    _deck(monkeypatch)
    return _settings(monkeypatch, tmp_path)


def _consent(question: str) -> str:
    """Yes to the install, no to the docker group: the group is not what these tests are about."""
    return "no" if GROUP_QUESTION_MARK in question else "yes"


def _never_feeds(_argv: list[str], _text: str) -> subprocess.CompletedProcess[str]:
    raise AssertionError("no step needed a sudo password in this test")


def _install_deck(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> tuple[Path, Path]:
    """A fresh Deck whose packages, store plan, enable and finish all run."""
    monkeypatch.setattr(platform.sys, "platform", "linux")
    _deck(monkeypatch)
    monkeypatch.setattr(platform, "linux_docker_on_board", lambda _which=None: "none")
    monkeypatch.setattr(platform, "is_image_based", lambda: False)
    _spy_mkdtemp(monkeypatch, tmp_path)
    return _settings(monkeypatch, tmp_path)


def _install(run: _InstallDeck) -> platform.ProvisionReport:
    return platform.ensure_docker(
        run=run,
        which=lambda name: "/usr/bin/pacman" if name == "pacman" else None,
        user="deck",
        ask=_consent,
        run_input=_never_feeds,
        wait_seconds=0.0,
    )


def _switch(deck: _SwitchDeck) -> platform.ProvisionReport:
    return platform.switch_steamos_store(ask=lambda _q: "yes", run=deck, wait_seconds=0.0)


# ------------------------------------------------- the switch that cannot answer


def test_a_switch_that_cannot_answer_on_an_active_unit_keeps_the_settings(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """T884 r5: `docker info` failing is not Docker refusing settings when the unit is up.

    The restart succeeded, the unit is `active`, and only this process cannot
    reach the socket — the old branch undid a perfectly good move and then
    restarted Docker on the files it had just put back. Nothing goes back here,
    and the report says the session is what to fix.
    """
    _deck(monkeypatch)
    _settings(monkeypatch, tmp_path, present=True)
    deck = _SwitchDeck(ready_after_restarts=None, unit_active=True)

    report = _switch(deck)

    steps = deck.steps()
    assert deck.restarts == 1, steps
    assert _restores(steps) == [], f"an active unit still undid the settings: {steps}"
    said = " ".join(report.manual_steps)
    assert platform.STEAMOS_DOCKER_SESSION_STEP in said, said
    assert platform.STEAMOS_STORE_ON_NEW_SETTINGS in said, said
    assert not report.docker_ready


def test_after_an_undo_the_switch_issues_a_restart_not_a_start(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """T884 r5: `systemctl start` on an active unit is a no-op; the retry must restart.

    The unit is inactive here, so the settings do go back — and what follows
    them is a `restart`, which re-reads `/etc` whatever state the units were in.
    The old `start` left Docker on the new settings while the files said old.
    """
    _deck(monkeypatch)
    daemon, config = _settings(monkeypatch, tmp_path, present=True)
    deck = _SwitchDeck(ready_after_restarts=None, unit_active=False)

    report = _switch(deck)

    steps = deck.steps()
    restores = _restores(steps)
    assert len(restores) == 2, steps
    assert deck.restarts == 2, steps
    assert "systemctl start containerd docker" not in steps, steps
    last_restart = max(
        i for i, step in enumerate(steps) if step == "systemctl restart containerd docker"
    )
    assert all(steps.index(restore) < last_restart for restore in restores), steps
    said = report.manual_steps[0]
    assert platform.STEAMOS_STORE_FILES_PUT_BACK in said, said
    assert platform.STEAMOS_STORE_NOT_RUNNING_AGAIN in said, said


# ---------------------------------------------------------------- the install


def test_the_install_undoes_the_new_settings_and_restarts_when_the_unit_stays_down(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """T884 r5: an install whose daemon never answers on the new settings undoes them.

    `enable --now` ran first and started the unit on the new store; the restart
    then failed to produce a daemon and the unit is inactive, so this is Docker
    refusing the settings. They go back, a `restart` re-reads the old ones, and
    the report says both.
    """
    daemon, config = _install_deck(monkeypatch, tmp_path)
    deck = _InstallDeck(ready_after_restarts=2, unit_active=False)

    report = _install(deck)

    steps = deck.steps()
    restores = _restores(steps)
    assert len(restores) == 2, steps
    assert deck.restarts == 2, steps
    assert "systemctl start containerd docker" not in steps, steps
    assert steps.index("systemctl enable --now docker") < steps.index(
        "systemctl restart containerd docker"
    ), "enable --now still comes first"
    said = " ".join(report.manual_steps)
    assert platform.STEAMOS_STORE_FILES_PUT_BACK in said, said
    assert platform.STEAMOS_STORE_RUNNING_AGAIN in said, said
    assert report.docker_ready


# ----------------------------------------------------------------- the repair


def test_the_repair_undoes_the_new_settings_when_its_enable_fails(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """T884 r5: a repair that cannot enable Docker on the new settings must put them back.

    The enable is the repair's own finish step, and round 4 stopped there with
    the move installed: Docker never read the settings and the Deck was left
    pointing at /home. The helper takes the failed step, undoes the files and
    restarts on them.
    """
    daemon, config = _repair_deck(monkeypatch, tmp_path)
    deck = _FinishRepairDeck(
        ready_after_restarts=1,
        fail={"systemctl enable --now docker": (1, "enable failed")},
    )

    report = _repair(deck, _answers("y")[1])

    steps = deck.steps
    restores = _restores(steps)
    assert len(restores) == 2, steps
    assert "systemctl restart containerd docker" in steps, steps
    assert "systemctl start containerd docker" not in steps, steps
    said = report.manual_steps[0]
    assert platform.STEAMOS_STORE_FILES_PUT_BACK in said, said
    assert platform.STEAMOS_STORE_RUNNING_AGAIN in said, said
    assert report.docker_ready


def test_the_repair_undoes_the_new_settings_when_the_unit_stays_inactive(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """T884 r5: the Codex finding — the repair must not keep settings Docker refused.

    The restart returned 0, `docker info` did not answer, and the unit is
    inactive: the settings go back, a second `restart` runs on them, and the
    report says the files were put back and Docker runs on them again.
    """
    daemon, config = _repair_deck(monkeypatch, tmp_path)
    deck = _FinishRepairDeck(ready_after_restarts=2)

    report = _repair(deck, _answers("y")[1])

    steps = deck.steps
    restores = _restores(steps)
    assert len(restores) == 2, steps
    restarts = [i for i, step in enumerate(steps) if step == "systemctl restart containerd docker"]
    assert len(restarts) == 2, steps
    assert all(steps.index(restore) < restarts[1] for restore in restores), steps
    said = report.manual_steps[0]
    assert platform.STEAMOS_STORE_FILES_PUT_BACK in said, said
    assert platform.STEAMOS_STORE_RUNNING_AGAIN in said, said
    assert report.docker_ready


# ----------------------------------------------- unreadable on the other paths


def test_the_install_refuses_to_stage_an_unreadable_config_toml(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """T884 r5: the install path reports the unreadable file, as the switch does.

    Round 4 fixed the reader on every path; this pins the install's half of it:
    the whole store plan is refused, so neither settings file is staged or
    written, and the manual step names the file.
    """
    daemon, config = _install_deck(monkeypatch, tmp_path)
    _unreadable(monkeypatch, config)
    deck = _InstallDeck(ready_after_restarts=1)

    report = _install(deck)

    steps = deck.steps()
    assert not [step for step in steps if step.startswith(("cp -a ", "install -d "))], steps
    assert not [step for step in steps if step.startswith("install -D ")], steps
    said = " ".join(report.manual_steps)
    assert f"could not read {config}" in said, said
    assert "systemctl enable --now docker" in steps, "the daemon is still enabled"


def test_the_repair_refuses_to_stage_an_unreadable_daemon_json(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """T884 r5: the repair path reports the unreadable file, as the switch does."""
    daemon, config = _repair_deck(monkeypatch, tmp_path)
    _unreadable(monkeypatch, daemon)
    deck = _FinishRepairDeck(ready_after_restarts=1)

    report = _repair(deck, _answers("y")[1])

    steps = deck.steps
    assert not [step for step in steps if step.startswith(("cp -a ", "install -d "))], steps
    assert not [step for step in steps if step.startswith("install -D ")], steps
    said = " ".join(report.manual_steps)
    assert f"could not read {daemon}" in said, said
    assert "systemctl enable --now docker" in steps, "the repair still finishes"
