"""T896: the lease one move (or one copy) holds on a character's folder.

Two Yu'lon windows, or two copies of the app, must never move the same character at once: the
second would write a package over the first one's history, or load a copy the first is already
erasing. The lease is an OS lock, not a file holding a pid -- a process that is killed, or a
computer losing power, releases the kernel's lock on the way out, so the next window takes over
and resumes the journal instead of waiting for an answer that will never come. Linux and macOS
lock with `flock`, Windows with `msvcrt.locking` on one byte of `lease`.

`lease-holder.json` says whose lease it is, for the sentence the second window shows. It is
never believed on its own: it is written only while the lock is held, and read only after the
lock was refused, so a stale file from a dead process cannot block a live one.
"""

from __future__ import annotations

import json
import os
import sys
import threading
from collections.abc import Iterator
from contextlib import contextmanager
from pathlib import Path

from yulon import platform
from yulon.log import get_logger

logger = get_logger(__name__)

LEASE_FILE = "lease"
"""The file the kernel lock is taken on. It holds no byte of its own."""

HOLDER_FILE = "lease-holder.json"
"""Whose lease it is: `{"pid": int, "purpose": str}`. Overwritten by every holder."""

MODE = 0o600
"""Owner read/write: only this account ever opens the lease."""

UNKNOWN_HOLDER = "Another Yu'lon window"
"""What the refusal says when the holder file cannot be read or trusted."""

_LOCK = threading.Lock()
"""Guards `_HELD` itself, never held across an OS call."""

_HELD: dict[str, tuple[int, int, str]] = {}
"""Folder key -> (thread ident, depth, purpose), for this process only.

`flock` and `msvcrt` refuse another *thread* of this process as they refuse another process,
which is right: two threads moving one character is the race the lease exists for. The depth is
what lets one thread nest -- `move` holds the folder, and `store` and `prune` inside it take the
same lease again without a second OS call or a refusal against itself.
"""


class LeaseTaken(Exception):
    """Another holder has this character's folder, so nothing was started."""

    def __init__(self, purpose: str) -> None:
        self.purpose = purpose
        super().__init__(
            f"{purpose} is still running for this character, so Yu'lon did not start another."
        )


@contextmanager
def hold(folder: Path, purpose: str) -> Iterator[None]:
    """Hold `folder`'s lease for this block, or raise `LeaseTaken` naming the other holder.

    `purpose` begins the refusal sentence: "A move", "A stop copy".
    """
    key = str(folder.resolve())
    ident = threading.get_ident()
    with _LOCK:
        held = _HELD.get(key)
        if held is not None:
            if held[0] != ident:
                raise LeaseTaken(held[2])
            _HELD[key] = (ident, held[1] + 1, held[2])
    if held is not None:
        try:
            yield
        finally:
            with _LOCK:
                _released = _HELD.get(key)
                if _released is not None:
                    if _released[1] <= 1:
                        del _HELD[key]
                    else:
                        _HELD[key] = (_released[0], _released[1] - 1, _released[2])
        return

    folder.mkdir(mode=0o700, parents=True, exist_ok=True)
    fd = os.open(
        folder / LEASE_FILE,
        os.O_RDWR | os.O_CREAT | getattr(os, "O_BINARY", 0),
        MODE,
    )
    try:
        taken = _try_lock(fd)
    except BaseException:
        os.close(fd)
        raise
    if not taken:
        try:
            purpose_of_holder = _read_holder(folder)
        finally:
            os.close(fd)
        raise LeaseTaken(purpose_of_holder)
    try:
        platform.write_private_atomically(
            folder / HOLDER_FILE,
            json.dumps({"pid": os.getpid(), "purpose": purpose}).encode("utf-8"),
        )
        with _LOCK:
            _HELD[key] = (ident, 1, purpose)
        yield
    finally:
        with _LOCK:
            _HELD.pop(key, None)
        try:
            _unlock(fd)
        finally:
            os.close(fd)


def _try_lock(fd: int) -> bool:
    """Take the exclusive lock on `fd` without waiting, or answer False.

    Written per platform rather than through a shim: mypy reads the `sys.platform` branch for
    each platform it is asked about, so `msvcrt` and `fcntl` are each the one that exists there.
    """
    if sys.platform == "win32":
        import msvcrt

        os.lseek(fd, 0, 0)
        try:
            msvcrt.locking(fd, msvcrt.LK_NBLCK, 1)
        except OSError:
            return False
        return True
    import fcntl

    try:
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        return False
    return True


def _unlock(fd: int) -> None:
    """Release what `_try_lock` took. A lock already gone is not an error."""
    if sys.platform == "win32":
        import msvcrt

        os.lseek(fd, 0, 0)
        msvcrt.locking(fd, msvcrt.LK_UNLCK, 1)
    else:
        import fcntl

        fcntl.flock(fd, fcntl.LOCK_UN)


def _read_holder(folder: Path) -> str:
    """The other holder's purpose, or the sentence for a file that cannot be believed."""
    try:
        written = json.loads((folder / HOLDER_FILE).read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return UNKNOWN_HOLDER
    if not isinstance(written, dict):
        return UNKNOWN_HOLDER
    purpose = written.get("purpose")
    if not isinstance(written.get("pid"), int) or not isinstance(purpose, str) or not purpose:
        return UNKNOWN_HOLDER
    logger.info(f"{folder}: the lease is held by pid {written['pid']} ({purpose})")
    return purpose
