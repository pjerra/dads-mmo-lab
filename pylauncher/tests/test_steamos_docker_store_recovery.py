"""T884 round 2: recovering a Deck when the store switch stops, and the gates around it.

The round-1 switch stopped Docker, wrote half a move and left it there: a
failed `install` or a Stop between the `systemctl stop` and the `systemctl
start` left the daemon down with one settings file moved. Everything here is
about what happens AFTER that stop: the files this run installed go back (the
`.yulon-bak`, or nothing), Docker is started anyway, and one sentence says
where it stopped and whether Docker runs.

The three gates are here too: only a `/var` that is its own small partition
is rewritten (a resized one, and one that is the root filesystem, are left
alone), the staging folder exists only after a yes and never after the run,
and the install's store plan is built from what `pacman` left on disk.

No sudo, no systemctl, no disk outside `tmp_path`: every command is scripted.
"""

from __future__ import annotations

import subprocess
import tempfile
import threading
from collections.abc import Callable, Sequence
from pathlib import Path

import pytest

from yulon import platform

GIB = 1024**3
TARGET_CONTAINERD = "/home/.yulon-docker/containerd"
GROUP_QUESTION_MARK = "add 'deck' to the docker group"


class _Deck:
    """A scripted Deck: every argv, an optional failure, and whether the daemon answers."""

    def __init__(
        self,
        *,
        fail: Callable[[list[str]], bool] | None = None,
        ready: bool = True,
        cancel: threading.Event | None = None,
    ) -> None:
        self.calls: list[list[str]] = []
        self.fail = fail
        self.ready = ready
        self.cancel = cancel

    def __call__(self, argv: list[str]) -> subprocess.CompletedProcess[str]:
        self.calls.append(argv)
        body = argv[2:] if argv[:2] == ["sudo", "-n"] else argv
        if argv[:2] == ["docker", "info"]:
            return subprocess.CompletedProcess(argv, 0 if self.ready else 1, "", "")
        if self.cancel is not None and body[:2] == ["systemctl", "stop"]:
            self.cancel.set()
        failed = self.fail is not None and self.fail(body)
        return subprocess.CompletedProcess(argv, 1 if failed else 0, "", "boom" if failed else "")

    def steps(self) -> list[str]:
        """Every command as it would be typed after `sudo`, docker probes left out."""
        return [
            " ".join(argv[2:]) if argv[:2] == ["sudo", "-n"] else " ".join(argv)
            for argv in self.calls
            if argv[:2] != ["docker", "info"]
        ]

    bodies = steps


def _deck(monkeypatch: pytest.MonkeyPatch) -> None:
    """A stock Deck: SteamOS, a 4 GiB /var, and the switch worth running."""
    monkeypatch.setattr(platform, "is_steamos", lambda: True)
    monkeypatch.setattr(platform, "_var_total_bytes", lambda: 4 * GIB)
    monkeypatch.setattr(platform, "on_var", lambda _path: True)
    monkeypatch.setattr(platform, "steamos_store_needs_home", lambda data_root=None: True)


def _settings(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path, *, present: bool
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


def _boundary(name: str, daemon: Path, config: Path) -> Callable[[list[str]], bool]:
    """A predicate that fails the named command of the store plan, and nothing else."""
    if name == "backup":
        return lambda body: body[0] == "cp"
    if name == "mkdir":
        return lambda body: body[:2] == ["install", "-d"]
    if name == "install daemon.json":
        return lambda body: body[:2] == ["install", "-D"] and body[-1] == str(daemon)
    if name == "install config.toml":
        return lambda body: body[:2] == ["install", "-D"] and body[-1] == str(config)
    if name == "start":
        return lambda body: body[:2] == ["systemctl", "start"]
    raise AssertionError(f"unknown boundary {name}")


# ------------------------------------------------- what happens after the stop


@pytest.mark.parametrize(
    "name",
    ["backup", "mkdir", "install daemon.json", "install config.toml", "start"],
)
def test_a_failure_after_the_stop_puts_back_what_it_installed(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path, name: str
) -> None:
    """T884 r2: each boundary after `stop` undoes exactly the files this run installed.

    Both settings files existed before, so a file that was installed is put back
    from its `.yulon-bak` and a file that never was is not mentioned at all. The
    start is issued after every failure (Docker was stopped) and the report says
    where it stopped, what was put back and whether Docker runs.
    """
    _deck(monkeypatch)
    daemon, config = _settings(monkeypatch, tmp_path, present=True)
    ready = name != "start"
    run = _Deck(fail=_boundary(name, daemon, config), ready=ready)

    report = platform.switch_steamos_store(ask=lambda _q: "yes", run=run, wait_seconds=0.0)

    steps = run.steps()
    put_back_daemon = f"install -m 0644 {daemon}.yulon-bak {daemon}"
    put_back_config = f"install -m 0644 {config}.yulon-bak {config}"
    expected = {
        "backup": [],
        "mkdir": [],
        "install daemon.json": [],
        "install config.toml": [put_back_daemon],
        "start": [put_back_daemon, put_back_config],
    }[name]
    assert _restores(steps) == expected
    assert "systemctl start containerd docker" in steps, steps
    assert steps.index("systemctl stop docker.socket docker containerd") < steps.index(
        "systemctl start containerd docker"
    )
    if name != "start":
        assert steps[-1] == "systemctl start containerd docker", steps
    said = report.manual_steps[0]
    assert said.startswith('The switch failed at "'), said
    if expected:
        assert platform.STEAMOS_STORE_FILES_PUT_BACK in said
    else:
        assert platform.STEAMOS_STORE_NOTHING_INSTALLED in said
    if ready:
        assert platform.STEAMOS_STORE_RUNNING_AGAIN in said
    else:
        assert platform.STEAMOS_STORE_NOT_RUNNING_AGAIN in said


def test_files_that_were_not_there_before_are_removed_again(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """T884 r2: a Deck with no settings files gets them removed, not a backup copied.

    `install -m 0644 <f>.yulon-bak <f>` on a file that never existed would fail
    and leave the half-migrated file behind; the undo for that case is `rm -f`.
    """
    _deck(monkeypatch)
    daemon, config = _settings(monkeypatch, tmp_path, present=False)
    run = _Deck(fail=_boundary("start", daemon, config), ready=False)

    report = platform.switch_steamos_store(ask=lambda _q: "yes", run=run, wait_seconds=0.0)

    assert _restores(run.steps()) == [f"rm -f {daemon}", f"rm -f {config}"]
    assert platform.STEAMOS_STORE_NOT_RUNNING_AGAIN in report.manual_steps[0]
    assert not report.docker_ready


def test_a_cancel_after_the_stop_still_starts_docker(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """T884 r2: the cleanup is not cancellable — a Stop after `stop` starts Docker again.

    Nothing was installed when the cancel lands, so there is no file to put back;
    what must not happen is a Deck left with Docker stopped because the player
    pressed Stop in a dialog.
    """
    _deck(monkeypatch)
    _settings(monkeypatch, tmp_path, present=True)
    cancel = threading.Event()
    run = _Deck(cancel=cancel)

    report = platform.switch_steamos_store(
        ask=lambda _q: "yes", run=run, cancel=cancel, wait_seconds=0.0
    )

    steps = run.steps()
    assert steps[0] == "systemctl stop docker.socket docker containerd"
    assert steps[1] == "systemctl start containerd docker", steps
    assert not [step for step in steps if step.startswith(("cp ", "install "))]
    said = report.manual_steps[0]
    assert platform.STEAMOS_STORE_NOTHING_INSTALLED in said
    assert platform.STEAMOS_STORE_RUNNING_AGAIN in said
    assert report.docker_ready


# --------------------------------------------------------------- the 20 GiB gate


def test_a_resized_var_gets_no_question_and_no_change(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """T884 r2: a /var resized on purpose is not a small /var, so nothing is offered.

    The row that offers the switch is drawn from the same predicate, so a 64 GiB
    /var produces neither the row nor the question: the switch answers "nothing
    to do" before it asks anything.
    """
    monkeypatch.setattr(platform, "is_steamos", lambda: True)
    monkeypatch.setattr(platform, "_var_total_bytes", lambda: 64 * GIB)
    monkeypatch.setattr(platform, "on_var", lambda _path: True)
    _settings(monkeypatch, tmp_path, present=True)
    run = _Deck()
    asked: list[str] = []

    report = platform.switch_steamos_store(
        ask=lambda question: asked.append(question) or "yes", run=run
    )

    assert asked == [], "a resized /var must not be asked about"
    assert run.calls == []
    assert report.manual_steps == (platform.STEAMOS_STORE_NOTHING_TO_DO_STEP,)


def test_var_on_the_root_filesystem_answers_false_and_keeps_a_moved_store(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """T884 r2: /var on `/` has no small partition to escape, so Yu'lon rewrites nothing.

    With /var and / on one device the answer is False however small /var
    reports, so neither the switch nor the install's plan is ever built for it:
    the player's `data-root = /opt/docker` is read by nothing that writes. A
    store that genuinely sits off /var is kept by the merge itself, which is
    the other half of the same promise.
    """
    daemon, config = _settings(monkeypatch, tmp_path, present=False)
    daemon.write_text('{"data-root": "/opt/docker"}\n', encoding="utf-8")
    config.write_text('version = 2\nroot = "/opt/docker/containerd"\n', encoding="utf-8")
    monkeypatch.setattr(platform, "is_steamos", lambda: True)
    monkeypatch.setattr(platform, "_var_total_bytes", lambda: 4 * GIB)
    monkeypatch.setattr(platform, "on_var", lambda _path: True)
    monkeypatch.setattr(platform, "_device_of", lambda _path: 2049)

    assert platform.var_is_its_own_mount() is False
    assert platform.steamos_store_needs_home(None) is False, "a /var on / is not moved"

    monkeypatch.setattr(platform, "on_var", lambda path: str(path).startswith("/var"))
    merged = platform.merged_daemon_json(daemon.read_text(encoding="utf-8"))
    assert merged is not None and '"/opt/docker"' in merged, "the player's data-root is kept"


# ------------------------------------------------------------------- staging


def test_a_declined_install_creates_no_staging_folder(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """T884 r2: the install stages only after the player said yes.

    Round 1 merged and staged before the consent question, so a "no" still left
    a `yulon-docker-store-*` folder holding this machine's settings in /tmp.
    """
    monkeypatch.setattr(platform.sys, "platform", "linux")
    monkeypatch.setattr(platform, "is_steamos", lambda: True)
    monkeypatch.setattr(platform, "linux_docker_on_board", lambda _which=None: "none")
    monkeypatch.setattr(platform, "is_image_based", lambda: False)
    monkeypatch.setattr(platform, "steamos_store_needs_home", lambda data_root=None: True)
    _settings(monkeypatch, tmp_path, present=True)
    made = _spy_mkdtemp(monkeypatch, tmp_path)

    report = platform.ensure_docker(
        run=_Deck(ready=False),
        which=lambda name: "/usr/bin/pacman" if name == "pacman" else None,
        user="deck",
        ask=lambda _question: "no",
        run_input=_never_feeds,
        wait_seconds=0.0,
    )

    assert made == [], "a declined install staged a settings folder anyway"
    assert report.manual_steps and report.manual_steps[0].startswith(
        "You said no, so nothing was run."
    )


def test_a_switch_run_removes_its_staging_folder(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """T884 r2: the staged files are gone once the run ends, success or not."""
    _deck(monkeypatch)
    _settings(monkeypatch, tmp_path, present=True)
    made = _spy_mkdtemp(monkeypatch, tmp_path)

    platform.switch_steamos_store(ask=lambda _q: "yes", run=_Deck(), wait_seconds=0.0)

    assert len(made) == 1, made
    assert not made[0].exists(), "the staging folder outlived the run"


# ------------------------------------------------------- the install path's plan


def _never_feeds(_argv: list[str], _text: str) -> subprocess.CompletedProcess[str]:
    raise AssertionError("no step needed a sudo password in this test")


def test_the_store_plan_sees_a_config_toml_pacman_installed(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """T884 r2: the plan is built after the packages, so a shipped config is merged.

    The package's `/etc/containerd/config.toml` appears only when `pacman` runs.
    A plan built before that saw no file, merged the two-line default and then
    overwrote the package's config with it — the exact ordering this pins:
    pacman, then the backup and the merge, then the enable. The `.yulon-bak`
    line is the second half of the same proof: a file that was there is copied,
    not clobbered.
    """
    monkeypatch.setattr(platform.sys, "platform", "linux")
    monkeypatch.setattr(platform, "is_steamos", lambda: True)
    monkeypatch.setattr(platform, "linux_docker_on_board", lambda _which=None: "none")
    monkeypatch.setattr(platform, "is_image_based", lambda: False)
    monkeypatch.setattr(platform, "steamos_store_needs_home", lambda data_root=None: True)
    daemon, config = _settings(monkeypatch, tmp_path, present=False)
    shipped = 'version = 2\nroot = "/var/lib/containerd"\n# shipped by the package\n'
    made = _spy_mkdtemp(monkeypatch, tmp_path)
    staged_text: dict[str, str] = {}

    class _Run(_Deck):
        def __call__(self, argv: list[str]) -> subprocess.CompletedProcess[str]:
            body = argv[2:] if argv[:2] == ["sudo", "-n"] else argv
            if body[:1] == ["pacman"]:
                config.write_text(shipped, encoding="utf-8")
            if body[:1] == ["install"] and "-D" in body and Path(body[-1]) == config:
                staged_text["config"] = Path(body[-2]).read_text(encoding="utf-8")
            return super().__call__(argv)

    run = _Run(ready=False)
    report = platform.ensure_docker(
        run=run,
        which=lambda name: "/usr/bin/pacman" if name == "pacman" else None,
        user="deck",
        ask=_consent,
        run_input=_never_feeds,
        wait_seconds=0.0,
    )

    assert report.platform == "linux"
    steps = run.bodies()
    backup = f"cp -a {config} {config}.yulon-bak"
    install = next(step for step in steps if step.endswith(f" {config}") and " -D " in step)
    pacman = next(step for step in steps if step.startswith("pacman "))
    enable = steps.index("systemctl enable --now docker")
    assert steps.index(backup) < steps.index(install) < enable
    assert steps.index(pacman) < steps.index(backup), "the plan was built before pacman ran"
    assert "# shipped by the package" in staged_text["config"], staged_text
    assert f'root = "{TARGET_CONTAINERD}"' in staged_text["config"], staged_text
    assert made and not made[0].exists(), "the staging folder outlived the install"


def _consent(question: str) -> str:
    """Yes to the install, no to the docker group: the group is not what this test is about."""
    return "no" if GROUP_QUESTION_MARK in question else "yes"
