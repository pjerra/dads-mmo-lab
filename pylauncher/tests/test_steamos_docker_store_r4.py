"""T884 round 4: an unreadable setting, a Stop during the undo, and a silent start.

Three holes left by round 3, each one a case where the report said something
the Deck did not do:

* A settings file that EXISTS but cannot be read was read as absent, so a fresh
  `data-root` was staged and installed over a file this run never saw. An
  unreadable file now refuses the whole plan and the report names it.
* The undo used the same runner as the plan, so a Stop pressed after the first
  settings file was installed made the restore refuse to run as well: Docker
  was started on half-moved settings. The undo and the cleanup start never
  check the cancel.
* `systemctl start` returning 0 was read as "Docker is up". When it was not,
  the move was reported as done and the Deck was left on settings the daemon
  had refused. A start that never gets ready is undone and retried once on the
  restored files, and the report says which settings Docker runs on.

No sudo, no systemctl, no disk outside `tmp_path`: every command is scripted.
"""

from __future__ import annotations

import subprocess
import tempfile
import threading
from collections.abc import Callable
from pathlib import Path

import pytest

from yulon import platform

GIB = 1024**3


class _Deck:
    """A scripted Deck: every argv, how many starts Docker needs, and when a Stop lands.

    `ready_after_starts` is the number of `systemctl start` calls that have to
    have run before `docker info` answers, so readiness can lag a successful
    start. `cancel_after` runs after the named command returned, which is how a
    player's Stop arrives between two steps of one plan.
    """

    def __init__(
        self,
        *,
        ready_after_starts: int = 1,
        cancel_after: Callable[[list[str]], bool] | None = None,
        cancel: threading.Event | None = None,
    ) -> None:
        self.calls: list[list[str]] = []
        self.starts = 0
        self.ready_after_starts = ready_after_starts
        self.cancel_after = cancel_after
        self.cancel = cancel

    def __call__(self, argv: list[str]) -> subprocess.CompletedProcess[str]:
        self.calls.append(argv)
        body = argv[2:] if argv[:2] == ["sudo", "-n"] else argv
        if argv[:2] == ["docker", "info"]:
            up = self.starts >= self.ready_after_starts
            return subprocess.CompletedProcess(argv, 0 if up else 1, "", "")
        if body[:2] == ["systemctl", "start"]:
            self.starts += 1
        if self.cancel is not None and self.cancel_after is not None and self.cancel_after(body):
            self.cancel.set()
        return subprocess.CompletedProcess(argv, 0, "", "")

    def steps(self) -> list[str]:
        """Every command as it would be typed after `sudo`, docker probes left out."""
        return [
            " ".join(argv[2:]) if argv[:2] == ["sudo", "-n"] else " ".join(argv)
            for argv in self.calls
            if argv[:2] != ["docker", "info"]
        ]


def _deck(monkeypatch: pytest.MonkeyPatch) -> None:
    """A stock Deck: SteamOS, a 4 GiB /var, and the switch worth running."""
    monkeypatch.setattr(platform, "is_steamos", lambda: True)
    monkeypatch.setattr(platform, "_var_total_bytes", lambda: 4 * GIB)
    monkeypatch.setattr(platform, "on_var", lambda _path: True)
    monkeypatch.setattr(platform, "steamos_store_needs_home", lambda data_root=None: True)


def _settings(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> tuple[Path, Path]:
    """The two settings files under `tmp_path`, there, as this Deck's own."""
    daemon = tmp_path / "etc" / "docker" / "daemon.json"
    config = tmp_path / "etc" / "containerd" / "config.toml"
    daemon.parent.mkdir(parents=True, exist_ok=True)
    config.parent.mkdir(parents=True, exist_ok=True)
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


def _backup_of(steps: list[str], target: Path) -> str:
    """The backup THIS run made of `target`, from its own `cp -a` step."""
    for step in steps:
        parts = step.split()
        if parts[:2] == ["cp", "-a"] and parts[2] == str(target):
            return parts[3]
    raise AssertionError(f"this run made no backup of {target}: {steps}")


def _unreadable(monkeypatch: pytest.MonkeyPatch, path: Path) -> None:
    """Make exactly `path` fail its read with `PermissionError`, as a locked file does."""
    real = Path.read_text

    def read_text(self: Path, *args: object, **kwargs: object) -> str:
        if self == path:
            raise PermissionError(13, "Permission denied", str(self))
        return real(self, *args, **kwargs)  # type: ignore[arg-type]

    monkeypatch.setattr(Path, "read_text", read_text)


# --------------------------------------------------- an existing, unreadable file


@pytest.mark.parametrize("name", ["daemon.json", "config.toml"])
def test_an_unreadable_settings_file_refuses_the_whole_plan(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path, name: str
) -> None:
    """T884 r4: unreadable is not missing — neither file is staged or installed.

    Round 3's `_read_text_or_none()` answered None for a `PermissionError`, which
    is the same answer a missing file gets, so a fresh `data-root` was merged
    and installed over settings this run had never read. One unreadable file
    refuses BOTH files: after half a move the Deck is on settings nobody chose.
    """
    _deck(monkeypatch)
    daemon, config = _settings(monkeypatch, tmp_path)
    wanted = daemon if name == "daemon.json" else config
    _unreadable(monkeypatch, wanted)
    made = _spy_mkdtemp(monkeypatch, tmp_path)
    run = _Deck()

    report = platform.switch_steamos_store(ask=lambda _q: "yes", run=run, wait_seconds=0.0)

    assert run.steps() == [], f"a refused plan still ran commands: {run.steps()}"
    assert made == [], "a refused plan still staged a settings folder"
    said = " ".join(report.manual_steps)
    assert f"could not read {wanted}" in said, said
    assert not report.docker_ready


# ------------------------------------------------------- a Stop during the undo


def test_a_stop_after_the_first_settings_file_still_puts_it_back(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """T884 r4: the undo is not cancellable — the file written before the Stop goes back.

    The Stop lands after `install -D ... daemon.json`, so the next step is
    refused and the plan stops with one file moved. The restore used the same
    cancelling runner in round 3, so it refused too and Docker was started on
    the half-moved settings; the cleanup has to run whether Stop is set or not.
    """
    _deck(monkeypatch)
    daemon, config = _settings(monkeypatch, tmp_path)
    cancel = threading.Event()
    run = _Deck(
        cancel=cancel,
        cancel_after=lambda body: body[:2] == ["install", "-D"] and body[-1] == str(daemon),
    )

    report = platform.switch_steamos_store(
        ask=lambda _q: "yes", run=run, cancel=cancel, wait_seconds=0.0
    )

    steps = run.steps()
    put_back_daemon = f"install -m 0644 {_backup_of(steps, daemon)} {daemon}"
    assert put_back_daemon in steps, steps
    assert not [
        step for step in steps if step.startswith("install -D ") and step.endswith(str(config))
    ], steps
    assert "systemctl start containerd docker" in steps, steps
    said = report.manual_steps[0]
    assert platform.STEAMOS_STORE_FILES_PUT_BACK in said, said
    assert platform.STEAMOS_STORE_RUNNING_AGAIN in said, said
    assert report.docker_ready


# ------------------------------------------------ a start that never gets ready


def test_a_start_that_never_gets_ready_puts_the_settings_back_and_starts_again(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """T884 r4: `systemctl start` returning 0 is not a daemon that came up.

    Docker answers only after the second start, so the first start succeeded and
    the daemon still refused the new settings. The settings go back, the start
    is issued once more on the restored ones, and the report says the files were
    put back and Docker is running — instead of claiming the move was made.
    """
    _deck(monkeypatch)
    daemon, config = _settings(monkeypatch, tmp_path)
    run = _Deck(ready_after_starts=2)

    report = platform.switch_steamos_store(ask=lambda _q: "yes", run=run, wait_seconds=0.0)

    steps = run.steps()
    starts = [
        index for index, step in enumerate(steps) if step == "systemctl start containerd docker"
    ]
    assert len(starts) == 2, steps
    for target in (daemon, config):
        put_back = f"install -m 0644 {_backup_of(steps, target)} {target}"
        assert put_back in steps, steps
        assert starts[0] < steps.index(put_back) < starts[1], steps
    said = report.manual_steps[0]
    assert said.startswith('The switch failed at "systemctl start containerd docker"'), said
    assert platform.STEAMOS_STORE_FILES_PUT_BACK in said, said
    assert platform.STEAMOS_STORE_RUNNING_AGAIN in said, said
    assert report.docker_ready
