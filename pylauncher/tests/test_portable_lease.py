"""T896: the lease one move holds on a character's folder, against a second Yu'lon.

The kernel holds the lock, not a file with a pid in it: a process that is killed, or the
computer losing power, leaves no lock behind, so the next start takes over and resumes the
journal instead of waiting for a pid that will never answer.
"""

from __future__ import annotations

import json
import subprocess
import sys
import threading
from pathlib import Path

import pytest

from yulon.portable import lease
from yulon.portable.lease import LeaseTaken, hold

ROOT = Path(__file__).resolve().parent.parent

_CHILD = """
import sys
from pathlib import Path
from yulon.portable.lease import hold

with hold(Path(sys.argv[1]), "A stop copy"):
    print("held", flush=True)
    sys.stdin.readline()
"""


def a_child(folder: Path) -> subprocess.Popen[str]:
    """A second process holding the lease on `folder`, waiting on its stdin."""
    child = subprocess.Popen(
        [sys.executable, "-c", _CHILD, str(folder)],
        cwd=ROOT,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        text=True,
    )
    assert child.stdout is not None
    assert child.stdout.readline().strip() == "held"
    return child


def test_a_second_process_is_refused_while_the_first_holds(tmp_path: Path) -> None:
    folder = tmp_path / "character"
    child = a_child(folder)
    try:
        with pytest.raises(LeaseTaken) as excinfo:
            with hold(folder, "A move"):
                pytest.fail("the second holder got in")
        assert str(excinfo.value) == (
            "A stop copy is still running for this character, so Yu'lon did not start another."
        )
        assert excinfo.value.purpose == "A stop copy"
    finally:
        child.communicate("\n", timeout=60)
        assert child.returncode == 0


def test_a_killed_holder_leaves_a_lease_the_next_takes(tmp_path: Path) -> None:
    folder = tmp_path / "character"
    child = a_child(folder)
    assert child.stdout is not None
    child.kill()
    child.wait(timeout=60)
    with hold(folder, "A move"):
        held = json.loads((folder / lease.HOLDER_FILE).read_text())
        assert held["purpose"] == "A move"


def test_the_same_thread_may_hold_twice(tmp_path: Path) -> None:
    folder = tmp_path / "character"
    with hold(folder, "A move"):
        with hold(folder, "A copy"):
            pass
        written = json.loads((folder / lease.HOLDER_FILE).read_text())
        assert written["purpose"] == "A move"


def test_another_thread_is_refused(tmp_path: Path) -> None:
    folder = tmp_path / "character"
    outcome: list[BaseException] = []

    def other() -> None:
        try:
            with hold(folder, "A copy"):
                outcome.append(RuntimeError("the other thread got in"))
        except LeaseTaken as exc:
            outcome.append(exc)

    with hold(folder, "A move"):
        thread = threading.Thread(target=other)
        thread.start()
        thread.join(timeout=60)
        assert not thread.is_alive()
    assert len(outcome) == 1
    assert isinstance(outcome[0], LeaseTaken)
    assert str(outcome[0]) == (
        "A move is still running for this character, so Yu'lon did not start another."
    )


def test_the_lease_is_free_after_an_exception(tmp_path: Path) -> None:
    folder = tmp_path / "character"
    with pytest.raises(RuntimeError):
        with hold(folder, "A move"):
            raise RuntimeError("a step died")
    assert lease._HELD == {}
    with hold(folder, "A move"):
        assert (folder / lease.LEASE_FILE).exists()
