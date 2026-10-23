"""Headless entry points for the ssh box (Phase 1 task 1): `--help`, `--server status`.

The box has no display. `--help` must answer before Qt loads (spike S1: with no
verb the app opened the GUI, which aborts headless with no platform plugin);
`--server status` with nothing installed must say the same "no install"
sentence with the same exit code for every game; and no headless verb may run
the self-update probe, which writes into the install folder (`detect._probe`).
"""

from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any

import pytest

import main
from yulon import server_cli


def _qt_module_names() -> set[str]:
    """The Qt modules loaded right now (`PySide6*`, `shiboken*`), if any."""
    return {
        name
        for name in sys.modules
        if name == "PySide6" or name.startswith(("PySide6.", "shiboken"))
    }


def _empty_config_dir(tmp_path: Path) -> Path:
    """A config dir whose state knows no installs."""
    config = tmp_path / "config"
    config.mkdir()
    (config / "state.json").write_text(
        json.dumps({"schema_version": 1, "installs": []}), encoding="utf-8"
    )
    return config


@pytest.mark.parametrize("flag", ["--help", "-h"])
def test_help_prints_the_verbs_and_leaves_before_qt(
    monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str], flag: str
) -> None:
    """`--help` answers with the verbs and exit 0, without loading Qt."""
    before = _qt_module_names()
    monkeypatch.setattr(sys, "argv", ["yulon", flag])
    assert main.main() == 0
    out = capsys.readouterr().out
    assert "--version" in out
    assert "--server" in out and "status" in out
    assert "--provision" in out
    assert _qt_module_names() == before
    widgets = sys.modules.get("PySide6.QtWidgets")
    assert widgets is None or widgets.QApplication.instance() is None


def test_server_status_with_no_install_says_the_same_for_two_games(tmp_path: Path) -> None:
    """The "no install" sentence and exit code do not vary by game."""
    config = _empty_config_dir(tmp_path)
    before = _qt_module_names()

    def _never_built(*args: Any, **kwargs: Any) -> Any:
        raise AssertionError("with no install, no services are ever built")

    answers: dict[str, tuple[int, list[str]]] = {}
    for game in ("wow-wotlk", "wow-tbc"):
        said: list[str] = []
        code = server_cli.run(
            ["status", game],
            say=said.append,
            config_dir=config,
            services_for=_never_built,
            tick=0.01,
        )
        answers[game] = (code, said)
    wotlk_code, wotlk_said = answers["wow-wotlk"]
    tbc_code, tbc_said = answers["wow-tbc"]
    assert wotlk_code == tbc_code == server_cli.EXIT_REFUSED
    assert len(wotlk_said) == len(tbc_said) == 1
    assert "has no wow-wotlk install" in wotlk_said[0]
    assert "has no wow-tbc install" in tbc_said[0]
    assert wotlk_said[0].replace("wow-wotlk", "GAME") == tbc_said[0].replace("wow-tbc", "GAME")
    assert _qt_module_names() == before


def test_server_status_does_not_run_the_self_update_probe(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    """Headless `--server status` never probes for self-updates."""
    _empty_config_dir(tmp_path)
    argv = ["yulon", "--server", "status", "wow-wotlk"]
    # The guard is the fix: this argv must count as headless, or the probe
    # skip below promises nothing. Reverting it fails this line first.
    assert main._is_headless_verb(argv[1:]) is True
    monkeypatch.setattr(sys, "argv", argv)
    monkeypatch.setattr(main.platform, "restart_under_docker_group", lambda: False)
    probed: list[str] = []

    import yulon.selfupdate.detect as detect_module
    import yulon.update as update_module

    def _spy_detect(*args: Any, **kwargs: Any) -> Any:
        probed.append("detect_install")
        raise AssertionError("headless --server must not probe for self-updates")

    def _spy_check(*args: Any, **kwargs: Any) -> Any:
        probed.append("check_with_cache")
        raise AssertionError("headless --server must not check for updates")

    monkeypatch.setattr(detect_module, "detect_install", _spy_detect)
    monkeypatch.setattr(update_module, "check_with_cache", _spy_check)
    before = _qt_module_names()
    assert main.main() == server_cli.EXIT_REFUSED
    out = capsys.readouterr().out
    assert "has no wow-wotlk install" in out
    assert probed == []
    assert _qt_module_names() == before
