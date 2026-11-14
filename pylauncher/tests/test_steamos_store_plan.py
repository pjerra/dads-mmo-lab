"""T884 round 3: ONE plan applies the store change, on all three paths.

The switch, the T160 repair and a fresh install each ran Docker's two settings
files through their own loop, and each one failed differently: the switch put
back what it had installed, the repair put back and carried on, and the install
did neither. Round 3 makes the settings change a single helper, and this file
pins the contract it has to keep whoever calls it:

* the backups are THIS run's — `<file>.yulon-bak-<stamp>`, written every time,
  so an older `.yulon-bak` left by an earlier run is never the file a restore
  reads and is never deleted;
* the steps run in order and stop at the first failure, whether that is a
  non-zero result or an exception of ANY kind out of the runner;
* what this run already installed goes back in reverse order (its own backup,
  or `rm -f` when the file was not there before), and a restore that fails is
  reported without stopping the rest;
* the outcome says where it stopped, which files went back and which could not.

The three paths are here too, because "one helper" only means something if each
of them stops calling it in its own way: the switch still starts Docker after a
failure and tries the restored settings ONCE when the first start fails, and
neither the repair nor the install installs the second settings file after the
first one failed.

No sudo, no systemctl, no disk outside `tmp_path`: every command is scripted.
"""

from __future__ import annotations

import re
import subprocess
import tempfile
from collections.abc import Callable, Iterator, Sequence
from pathlib import Path

import pytest

from tests.test_steamos_docker_repair import _answers, _repair
from tests.test_steamos_docker_repair import _Deck as _RepairDeck
from yulon import platform

GIB = 1024**3
GROUP_QUESTION_MARK = "add 'deck' to the docker group"

STAMPED = re.compile(r"\.yulon-bak-\d{8}T\d{6}Z$")
"""This run's own backup name: the stamp is fixed per run and in UTC (T884 r3)."""


# --------------------------------------------------------------- the fixtures


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


def _staging_under(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> None:
    """`tempfile.mkdtemp` that keeps the staging folders under `tmp_path`."""
    real = tempfile.mkdtemp

    def spy(*args: object, **kwargs: object) -> str:
        return real(*args, dir=str(tmp_path), **kwargs)  # type: ignore[arg-type]

    monkeypatch.setattr(tempfile, "mkdtemp", spy)


def _plan(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> tuple[platform.StorePlan, Path, Path]:
    """The real plan for a Deck whose two settings live under `tmp_path`."""
    monkeypatch.setattr(platform, "on_var", lambda _path: True)
    daemon, config = _settings(monkeypatch, tmp_path, present=True)
    return platform._stage_steamos_store(), daemon, config


def _bodies(calls: Sequence[list[str]]) -> Iterator[list[str]]:
    """Each argv without its `sudo -n` prefix, which is how the steps are named."""
    for argv in calls:
        yield argv[2:] if argv[:2] == ["sudo", "-n"] else argv


class _Steps:
    """A step runner for the helper alone: records each argv, fails or raises on cue."""

    def __init__(
        self,
        *,
        fail: Callable[[list[str]], bool] | None = None,
        raises: Callable[[list[str]], bool] | None = None,
    ) -> None:
        self.calls: list[list[str]] = []
        self._fail = fail
        self._raises = raises

    def __call__(self, commands: list[list[str]]) -> tuple[list[str], list[str]]:
        done: list[str] = []
        failed: list[str] = []
        for argv in commands:
            self.calls.append(list(argv))
            if self._raises is not None and self._raises(argv):
                raise RuntimeError("the step runner fell over")
            if self._fail is not None and self._fail(argv):
                failed.append(f"{' '.join(argv)}: exit 1 boom")
            else:
                done.append(" ".join(argv))
        return done, failed

    def shown(self) -> list[str]:
        return [" ".join(argv) for argv in self.calls]


class _Deck:
    """A scripted Deck for the whole switch or the whole install.

    `fail`/`raises` see the argv without its `sudo -n` prefix. `starts` counts
    `systemctl start` calls and `docker info` only answers once more starts have
    run than were scripted to fail, so a report that says Docker is ready has to
    have issued the retry.
    """

    def __init__(
        self,
        *,
        fail: Callable[[list[str]], bool] | None = None,
        raises: Callable[[list[str]], bool] | None = None,
        start_failures: int = 0,
    ) -> None:
        self.calls: list[list[str]] = []
        self._fail = fail
        self._raises = raises
        self.start_failures = start_failures
        self.starts = 0

    def __call__(self, argv: list[str]) -> subprocess.CompletedProcess[str]:
        self.calls.append(argv)
        body = argv[2:] if argv[:2] == ["sudo", "-n"] else argv
        if argv[:2] == ["docker", "info"]:
            answered = self.starts > self.start_failures
            return subprocess.CompletedProcess(argv, 0 if answered else 1, "", "")
        if body[:2] == ["systemctl", "start"]:
            self.starts += 1
            if self.starts <= self.start_failures:
                return subprocess.CompletedProcess(argv, 1, "", "start job failed")
            return subprocess.CompletedProcess(argv, 0, "", "")
        if self._raises is not None and self._raises(body):
            raise RuntimeError("the step runner fell over")
        failed = self._fail is not None and self._fail(body)
        return subprocess.CompletedProcess(argv, 1 if failed else 0, "", "boom" if failed else "")

    def steps(self) -> list[str]:
        """Every command as it would be typed after `sudo`, docker probes left out."""
        return [
            " ".join(argv[2:]) if argv[:2] == ["sudo", "-n"] else " ".join(argv)
            for argv in self.calls
            if argv[:2] != ["docker", "info"]
        ]


def _backup_of(run: object, target: Path) -> str:
    """The backup this run made of `target`, from its own `cp -a` step."""
    made = {body[2]: body[3] for body in _bodies(run.calls) if body[:1] == ["cp"]}
    assert str(target) in made, f"this run made no backup of {target}: {made}"
    path = made[str(target)]
    assert STAMPED.search(path), f"the backup is not this run's stamped copy: {path}"
    return path


def _switch_deck(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> tuple[Path, Path]:
    """A stock Deck whose switch is worth running, with its own settings under `tmp_path`."""
    monkeypatch.setattr(platform, "is_steamos", lambda: True)
    monkeypatch.setattr(platform, "_var_total_bytes", lambda: 4 * GIB)
    monkeypatch.setattr(platform, "on_var", lambda _path: True)
    monkeypatch.setattr(platform, "steamos_store_needs_home", lambda data_root=None: True)
    _staging_under(monkeypatch, tmp_path)
    return _settings(monkeypatch, tmp_path)


# ------------------------------------------------------------- the helper alone


def test_a_failed_backup_stops_before_its_file_is_installed(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """The backup comes first, and a backup that fails means its file is not touched.

    The mutation this kills is the round-2 order: a failing `cp` there did not
    stop the `install` that followed, so a settings file was replaced with the
    only copy of what it held unknown.
    """
    plan, daemon, config = _plan(monkeypatch, tmp_path)
    run = _Steps(fail=lambda argv: argv[0] == "cp" and argv[2] == str(config))

    outcome = platform._apply_store_plan(plan, run)

    assert outcome.step is not None and outcome.step.startswith(f"cp -a {config}")
    assert not [shown for shown in run.shown() if shown.startswith("install -D")], run.shown()
    assert outcome.installed == ()
    assert outcome.restored == () and outcome.not_restored == ()
    assert not [argv for argv in run.calls if argv[0] in ("install", "rm") and "-m" in argv]


def test_the_first_install_failure_never_installs_the_second_file(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """Stopping at the first failure is what keeps the second file's state knowable."""
    plan, daemon, config = _plan(monkeypatch, tmp_path)
    run = _Steps(fail=lambda argv: argv[:2] == ["install", "-D"] and argv[-1] == str(daemon))

    outcome = platform._apply_store_plan(plan, run)

    assert outcome.step is not None and outcome.step.startswith("install -D -m 0644")
    assert str(config) not in [argv[-1] for argv in run.calls], "config.toml was installed"
    assert outcome.installed == (), "an install that failed is not an install"


def test_the_restore_uses_this_runs_stamped_backup_not_an_old_one(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """An earlier run's `.yulon-bak` is not this run's undo, and is not touched.

    The old file holds different content on purpose: if the restore named it,
    the assertion that it is byte-identical would fail, and a Deck whose first
    switch was followed by a second would be put back to the FIRST run's copy.
    """
    plan, daemon, config = _plan(monkeypatch, tmp_path)
    older = daemon.with_name(daemon.name + ".yulon-bak")
    older.write_text('{"older": true}\n', encoding="utf-8")
    run = _Steps(fail=lambda argv: argv[:2] == ["install", "-D"] and argv[-1] == str(config))

    outcome = platform._apply_store_plan(plan, run)

    backup = _backup_of(run, daemon)
    assert ["install", "-m", "0644", backup, str(daemon)] in run.calls
    assert ["install", "-m", "0644", str(older), str(daemon)] not in run.calls
    assert outcome.restored == (daemon,)
    assert older.read_text(encoding="utf-8") == '{"older": true}\n', "an old backup was read"


def test_a_runner_that_raises_stops_the_plan_and_still_puts_back_what_it_installed(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """ANY exception out of the runner is a failed step, not only `OSError` (T884 r3).

    Catching `OSError` alone left a `RuntimeError` from a caller's own runner to
    unwind past the restore: the plan stopped half way and Docker's settings
    stayed that way.
    """
    plan, daemon, config = _plan(monkeypatch, tmp_path)
    run = _Steps(raises=lambda argv: argv[:2] == ["install", "-D"] and argv[-1] == str(config))

    outcome = platform._apply_store_plan(plan, run)

    assert outcome.step is not None and "fell over" in outcome.step
    assert ["install", "-m", "0644", _backup_of(run, daemon), str(daemon)] in run.calls
    assert outcome.restored == (daemon,)
    assert outcome.not_restored == ()


# --------------------------------------------------------------- the switch


def test_a_failed_start_puts_the_settings_back_and_starts_once_more(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """A first start that fails on the new settings gets one retry on the restored ones.

    The retry is the point: the settings change is what the Deck might not start
    on, and the plan's own undo is the only thing that can make the old settings
    current again. Two start calls, the restore between them, and a report that
    says Docker is running again on the files that went back.
    """
    daemon, config = _switch_deck(monkeypatch, tmp_path)
    run = _Deck(start_failures=1)

    report = platform.switch_steamos_store(ask=lambda _q: "yes", run=run, wait_seconds=0.0)

    steps = run.steps()
    starts = [
        index for index, step in enumerate(steps) if step == "systemctl start containerd docker"
    ]
    assert len(starts) == 2, steps
    put_back_daemon = f"install -m 0644 {_backup_of(run, daemon)} {daemon}"
    put_back_config = f"install -m 0644 {_backup_of(run, config)} {config}"
    assert put_back_config in steps and put_back_daemon in steps, steps
    assert steps.index(put_back_config) > starts[0]
    assert steps.index(put_back_daemon) > starts[0]
    assert steps.index(put_back_config) < starts[1] and steps.index(put_back_daemon) < starts[1]
    said = report.manual_steps[0]
    assert said.startswith('The switch failed at "systemctl start containerd docker"'), said
    assert platform.STEAMOS_STORE_FILES_PUT_BACK in said
    assert platform.STEAMOS_STORE_RUNNING_AGAIN in said
    assert report.docker_ready


def test_a_runner_that_raises_during_the_switch_still_restores_and_starts(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """The cleanup does not depend on the plan failing politely (T884 r3)."""
    daemon, config = _switch_deck(monkeypatch, tmp_path)
    run = _Deck(raises=lambda body: body[:2] == ["install", "-D"] and body[-1] == str(config))

    report = platform.switch_steamos_store(ask=lambda _q: "yes", run=run, wait_seconds=0.0)

    steps = run.steps()
    assert "systemctl start containerd docker" in steps, steps
    assert f"install -m 0644 {_backup_of(run, daemon)} {daemon}" in steps, steps
    assert report.manual_steps[0].startswith(
        'The switch failed at "install -D -m 0644'
    ), report.manual_steps[0]
    assert platform.STEAMOS_STORE_FILES_PUT_BACK in report.manual_steps[0]
    assert report.docker_ready


# --------------------------------------------------------------- the install


def _install_deck(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> tuple[Path, Path]:
    """A fresh Deck whose packages and store plan are worth running."""
    monkeypatch.setattr(platform.sys, "platform", "linux")
    monkeypatch.setattr(platform, "is_steamos", lambda: True)
    monkeypatch.setattr(platform, "linux_docker_on_board", lambda _which=None: "none")
    monkeypatch.setattr(platform, "is_image_based", lambda: False)
    monkeypatch.setattr(platform, "steamos_store_needs_home", lambda data_root=None: True)
    _staging_under(monkeypatch, tmp_path)
    return _settings(monkeypatch, tmp_path)


def _consent(question: str) -> str:
    """Yes to the install, no to the docker group: the group is not what these tests are about."""
    return "no" if GROUP_QUESTION_MARK in question else "yes"


def _never_feeds(_argv: list[str], _text: str) -> subprocess.CompletedProcess[str]:
    raise AssertionError("no step needed a sudo password in this test")


def _install(run: _Deck) -> platform.ProvisionReport:
    return platform.ensure_docker(
        run=run,
        which=lambda name: "/usr/bin/pacman" if name == "pacman" else None,
        user="deck",
        ask=_consent,
        run_input=_never_feeds,
        wait_seconds=0.0,
    )


def _store_steps(steps: Sequence[str]) -> list[str]:
    """The store plan's own commands: a backup, a directory, or an install of a settings file."""
    return [
        step
        for step in steps
        if step.startswith(("cp -a ", "install -d "))
        or (step.startswith("install -D ") and step.endswith(("daemon.json", "config.toml")))
    ]


def test_the_install_stages_nothing_after_a_failed_package_step(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """A pacman that failed is not a Deck whose settings can be read and merged.

    Staging after it (round 2's fix) is what made this reachable: the plan was
    built from whatever half-installed state the package left, and its merge
    could then write a `root` for a containerd the package never installed.
    """
    _install_deck(monkeypatch, tmp_path)
    run = _Deck(fail=lambda body: body[:1] == ["pacman"])

    report = _install(run)

    steps = run.steps()
    assert not _store_steps(steps), steps
    assert platform.STEAMOS_STORE_MANUAL_STEP in report.manual_steps
    assert "systemctl enable --now docker" in steps, "the daemon is still enabled"


def test_the_install_never_installs_the_second_settings_after_the_first_failed(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """The helper stops the plan, so the install's own loop cannot carry on past it."""
    daemon, config = _install_deck(monkeypatch, tmp_path)
    run = _Deck(fail=lambda body: body[:2] == ["install", "-D"] and body[-1] == str(daemon))

    report = _install(run)

    steps = run.steps()
    assert not [step for step in steps if step.endswith(str(config))], steps
    assert platform.STEAMOS_STORE_MANUAL_STEP in report.manual_steps
    assert "systemctl enable --now docker" in steps, "the daemon is still enabled"


# ----------------------------------------------------------------- the repair


def _repair_plan(tmp_path: Path, daemon: Path, config: Path) -> platform.StorePlan:
    staged = tmp_path / "staged"
    staged.mkdir()
    (staged / "daemon.json").write_text('{"data-root": "/home/.yulon-docker/docker"}\n')
    (staged / "config.toml").write_text('version = 2\nroot = "/home/.yulon-docker/containerd"\n')
    return platform.StorePlan(
        commands=[
            ["install", "-d", "-m", "0711", "/home/.yulon-docker/containerd"],
            ["install", "-d", "-m", "0710", "/home/.yulon-docker/docker"],
            ["install", "-D", "-m", "0644", str(staged / "daemon.json"), str(daemon)],
            ["install", "-D", "-m", "0644", str(staged / "config.toml"), str(config)],
        ],
        folder=staged,
    )


def test_the_repair_never_installs_the_second_settings_after_the_first_failed(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """The repair carries past a store failure, not past the REST of the store plan."""
    monkeypatch.setattr(platform, "is_steamos", lambda: True)
    monkeypatch.setattr(platform, "steamos_store_needs_home", lambda data_root=None: True)
    daemon, config = _settings(monkeypatch, tmp_path)
    plan = _repair_plan(tmp_path, daemon, config)
    monkeypatch.setattr(platform, "_stage_steamos_store", lambda: plan)
    daemon_install = f"install -D -m 0644 {tmp_path / 'staged' / 'daemon.json'} {daemon}"
    deck = _RepairDeck(fail={daemon_install: (1, "Read-only file system")})

    report = _repair(deck, _answers("y")[1])

    assert daemon_install in deck.steps
    assert not [step for step in deck.steps if step.endswith(str(config))], deck.steps
    assert deck.steps.index(daemon_install) > deck.steps.index(
        "pacman -Sy --noconfirm docker docker-compose docker-buildx"
    ), "the plan is built after the packages"
    assert deck.steps.index(daemon_install) < deck.steps.index("systemctl daemon-reload")
    assert "systemctl daemon-reload" in deck.steps, "the repair still finishes"
    assert "systemctl enable --now docker" in deck.steps
    assert platform.STEAMOS_STORE_MANUAL_STEP in report.manual_steps
    assert report.docker_ready
