"""T884: Docker's store on a Steam Deck's small /var partition.

A stock Deck keeps /var on its own partition of a few hundred MB, and Docker's
image store is NOT the folder the player picks: `data-root` defaults to
/var/lib/docker and Docker 29's containerd store keeps its layers in
/var/lib/containerd. So an install with 100 GB free where it asked died with
`no space left on device` in /var (Discord report, 2026-11-14).

Both settings move to /home/.yulon-docker, never the SD card, and only on a
Deck whose /var is still the stock size and whose store is on it. Every test
here runs through the seams: no sudo, no systemctl, and no disk on this
machine is touched.
"""

from __future__ import annotations

import json
import subprocess
import tomllib
from pathlib import Path

import pytest

from yulon import platform

GIB = 1024**3
TARGET_DOCKER = "/home/.yulon-docker/docker"
TARGET_CONTAINERD = "/home/.yulon-docker/containerd"

STORE = [
    ["install", "-d", "-m", "0710", TARGET_DOCKER],
    ["install", "-D", "-m", "0644", "/tmp/staged/daemon.json", "/etc/docker/daemon.json"],
]


class _Run:
    """Every argv the switch asked for, and an answer that succeeds."""

    def __init__(self, docker_rc: int = 0) -> None:
        self.calls: list[list[str]] = []
        self.docker_rc = docker_rc

    def __call__(self, argv: list[str]) -> subprocess.CompletedProcess[str]:
        self.calls.append(argv)
        if argv[:2] == ["docker", "info"]:
            return subprocess.CompletedProcess(argv, self.docker_rc, "", "")
        return subprocess.CompletedProcess(argv, 0, "", "")


def _deck(monkeypatch: pytest.MonkeyPatch, *, total_gib: int = 4, on_var: bool = True) -> None:
    """SteamOS, a /var of `total_gib`, and everything on it (or nothing)."""
    monkeypatch.setattr(platform, "is_steamos", lambda: True)
    monkeypatch.setattr(platform, "_var_total_bytes", lambda: total_gib * GIB)
    monkeypatch.setattr(platform, "on_var", lambda _path: on_var)


# ---------------------------------------------------------------- the commands


def test_engine_commands_write_store_before_enable() -> None:
    """T884: the settings land after the read-only lock and before Docker starts.

    Before the enable, because the FIRST start reads them; after the relock,
    because they are root writes and the lock is off for exactly the pacman run.
    Omitted (the base) or moved after the enable, and Docker's first start
    writes its store to /var again.
    """
    plan = platform.docker_engine_commands("pacman", steamos=True, store=STORE)

    assert plan == [
        ["steamos-readonly", "disable"],
        ["pacman", "-Sy", "--noconfirm", "docker", "docker-compose", "docker-buildx"],
        ["steamos-readonly", "enable"],
        *STORE,
        ["systemctl", "enable", "--now", "docker"],
    ]


def test_non_steamos_commands_unchanged() -> None:
    """`store` is SteamOS's: an apt/dnf plan that was handed one must not grow it."""
    plain = platform.docker_engine_commands("apt", steamos=False)
    with_store = platform.docker_engine_commands("apt", steamos=False, store=STORE)
    assert with_store == plain
    arch = platform.docker_engine_commands("pacman", steamos=False, store=STORE)
    assert ["install", "-d", "-m", "0710", TARGET_DOCKER] not in arch


def test_repair_store_before_daemon_reload() -> None:
    """T160's repair also puts the store right, and before the reload that reads it."""
    commands = platform.steamos_docker_repair_commands(devmode=False, store=STORE)
    shown = [" ".join(argv) for argv, _tolerated in commands]

    assert shown.index("install -d -m 0710 " + TARGET_DOCKER) < shown.index(
        "systemctl daemon-reload"
    )
    assert shown.index("pacman -Sy --noconfirm docker docker-compose docker-buildx") < shown.index(
        "install -d -m 0710 " + TARGET_DOCKER
    )
    tolerated = {(" ".join(argv)): tolerated for argv, tolerated in commands}
    assert all(tolerated[" ".join(argv)] for argv in STORE), "a store failure is carried past"


def test_store_commands_back_up_an_existing_file_once(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """An existing settings file is copied to `.yulon-bak` once, and never over a backup."""
    daemon = tmp_path / "daemon.json"
    daemon.write_text("{}\n", encoding="utf-8")
    config = tmp_path / "config.toml"
    config.write_text("version = 2\n", encoding="utf-8")
    monkeypatch.setattr(platform, "_DOCKER_DAEMON_JSON", daemon)
    monkeypatch.setattr(platform, "_CONTAINERD_CONFIG", config)
    staged = tmp_path / "staged"

    first = platform.steamos_store_commands(staged, True, True)
    assert ["cp", "-a", str(daemon), str(daemon) + ".yulon-bak"] in first
    assert ["cp", "-a", str(config), str(config) + ".yulon-bak"] in first

    (daemon.parent / (daemon.name + ".yulon-bak")).write_text("old\n", encoding="utf-8")
    (config.parent / (config.name + ".yulon-bak")).write_text("old\n", encoding="utf-8")
    second = platform.steamos_store_commands(staged, True, True)
    assert not [argv for argv in second if argv[0] == "cp"], "the only copy was overwritten"


# --------------------------------------------------------------- daemon.json


def test_daemon_json_merge_keeps_unknown_keys(monkeypatch: pytest.MonkeyPatch) -> None:
    """Only `data-root` changes; every other key is kept, on a fresh file and an old one."""
    monkeypatch.setattr(platform, "on_var", lambda _path: True)
    existing = json.dumps(
        {"log-driver": "local", "data-root": "/var/lib/docker", "registry-mirrors": ["https://m"]}
    )

    merged = json.loads(platform.merged_daemon_json(existing) or "{}")

    assert merged["data-root"] == TARGET_DOCKER
    assert merged["log-driver"] == "local"
    assert merged["registry-mirrors"] == ["https://m"]
    fresh = platform.merged_daemon_json(None)
    assert fresh == json.dumps({"data-root": TARGET_DOCKER}, indent=2) + "\n"


def test_daemon_json_unparsable_refused() -> None:
    """A file this app did not write is never rewritten on a guess."""
    assert platform.merged_daemon_json("{ this is not json") is None
    assert platform.merged_daemon_json("[]") is None


def test_user_data_root_off_var_kept(monkeypatch: pytest.MonkeyPatch) -> None:
    """A data-root the user moved off /var is their choice, and is kept."""
    monkeypatch.setattr(platform, "on_var", lambda path: str(path) != "/mnt/coa/docker")

    merged = json.loads(
        platform.merged_daemon_json(json.dumps({"data-root": "/mnt/coa/docker"})) or "{}"
    )

    assert merged["data-root"] == "/mnt/coa/docker"


# ---------------------------------------------------------- containerd config


def test_config_toml_root_replaced_and_inserted(monkeypatch: pytest.MonkeyPatch) -> None:
    """A top-level `root` on /var is replaced; one that is missing is inserted."""
    monkeypatch.setattr(platform, "on_var", lambda _path: True)

    replaced = platform.merged_containerd_config('version = 2\nroot = "/var/lib/containerd"\n')
    assert replaced is not None
    assert tomllib.loads(replaced)["root"] == TARGET_CONTAINERD
    assert tomllib.loads(replaced)["version"] == 2

    inserted = platform.merged_containerd_config("version = 2\n")
    assert inserted is not None
    assert tomllib.loads(inserted)["root"] == TARGET_CONTAINERD
    assert inserted.splitlines()[0] == "version = 2"

    fresh = platform.merged_containerd_config(None)
    assert fresh == f'version = 2\nroot = "{TARGET_CONTAINERD}"\n'


def test_config_toml_tables_untouched(monkeypatch: pytest.MonkeyPatch) -> None:
    """A `root` inside a table belongs to that table and is left alone."""
    monkeypatch.setattr(platform, "on_var", lambda _path: True)
    existing = (
        'version = 2\n[plugins."io.containerd.grpc.v1.cri"]\n'
        '  root = "/var/lib/containerd"\n  sandbox_image = "pause:3.9"\n'
    )

    merged = platform.merged_containerd_config(existing)

    assert merged is not None
    parsed = tomllib.loads(merged)
    assert parsed["root"] == TARGET_CONTAINERD
    table = parsed["plugins"]["io.containerd.grpc.v1.cri"]
    assert table["root"] == "/var/lib/containerd"
    assert table["sandbox_image"] == "pause:3.9"


def test_config_toml_unparsable_refused() -> None:
    assert platform.merged_containerd_config("root = [unclosed") is None


def test_containerd_root_read_from_config(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> None:
    """The root is the one containerd's config names, not a constant /var/lib/containerd.

    Reading a constant is the mutation: after the store moved to /home, preflight
    would keep measuring the partition it left, which is exactly the T884 report
    ("the folder does not matter") one layer along.
    """
    config = tmp_path / "config.toml"
    config.write_text(f'version = 2\nroot = "{TARGET_CONTAINERD}"\n', encoding="utf-8")
    monkeypatch.setattr(platform, "_CONTAINERD_CONFIG", config)
    assert platform.containerd_configured_root() == Path(TARGET_CONTAINERD)

    monkeypatch.setattr(platform, "_CONTAINERD_CONFIG", tmp_path / "missing.toml")
    assert platform.containerd_configured_root() == Path("/var/lib/containerd")


# ------------------------------------------------------------- when to move it


def test_resized_var_left_alone(monkeypatch: pytest.MonkeyPatch) -> None:
    """A /var resized on purpose is the player's layout, and is not undone (T884).

    Only the stock-small /var is moved; at 64 GB the predicate is False however
    the store is placed, and dropping that total check (the mutation) makes it
    True.
    """
    _deck(monkeypatch, total_gib=64)
    assert platform.steamos_store_needs_home(None) is False

    _deck(monkeypatch, total_gib=4)
    assert platform.steamos_store_needs_home(None) is True

    _deck(monkeypatch, total_gib=4, on_var=False)
    assert platform.steamos_store_needs_home(None) is False, "a store off /var is kept"


# ----------------------------------------------------------------- the switch


def test_switch_declined_runs_nothing(monkeypatch: pytest.MonkeyPatch) -> None:
    """Only a deliberate yes changes anything, and the question says what stops."""
    run = _Run()
    asked: list[str] = []

    def ask(question: str) -> str:
        asked.append(question)
        return "no"

    report = platform.switch_steamos_store(ask=ask, run=run)

    assert asked == [platform.STEAMOS_STORE_SWITCH_QUESTION]
    assert "images" in platform.STEAMOS_STORE_SWITCH_QUESTION
    assert "stays in /var" in platform.STEAMOS_STORE_SWITCH_QUESTION
    assert run.calls == []
    assert report.manual_steps == (platform.STEAMOS_STORE_DECLINED_STEP,)
    assert report.docker_ready is False


def test_switch_yes_stops_installs_and_starts_in_order(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """The consented switch's order: stop, back up and install, start, and no data move."""
    daemon = tmp_path / "daemon.json"
    config = tmp_path / "config.toml"
    monkeypatch.setattr(platform, "_DOCKER_DAEMON_JSON", daemon)
    monkeypatch.setattr(platform, "_CONTAINERD_CONFIG", config)
    staged = tmp_path / "staged"

    def staged_commands() -> tuple[list[list[str]], tuple[()]]:
        return platform.steamos_store_commands(staged, True, True), ()

    monkeypatch.setattr(platform, "_stage_steamos_store", staged_commands)
    run = _Run()

    report = platform.switch_steamos_store(ask=lambda _q: "yes", run=run, wait_seconds=0.0)

    shown = [" ".join(argv) for argv in run.calls if argv[0] != "docker"]
    first = shown[0]
    last = shown[-1]
    assert first == "sudo -n systemctl stop docker.socket docker containerd"
    assert last == "sudo -n systemctl start containerd docker"
    assert any("daemon.json" in line for line in shown)
    assert any("config.toml" in line for line in shown)
    assert report.docker_ready and platform.STEAMOS_STORE_MOVED_STEP in report.manual_steps
    assert not daemon.exists() and not config.exists(), "the switch deletes no data"
