"""Read-only pre-check for a headless server box: `yulon --box-check`.

Asks the machine it runs on everything the PC needs before an install: the OS,
the CPU architecture, glibc, whether the `docker` CLI is present and whether
its daemon answers, whether the ssh user may reach that daemon, whether `git`
can run, the free space of the home folder, the firewall tool and its state,
and whether this process is root — plus the build's own version. One JSON
object on one line on stdout, every key always present (`null` when unknown),
no writes, no starts, no sudo. `main()` answers it before Qt loads, beside
`--version` and `--server`: the box this runs on has no display.
"""

from __future__ import annotations

import getpass
import importlib
import json
import os
import platform as _stdlib_platform
import shutil
import subprocess
from collections.abc import Callable
from pathlib import Path
from typing import Literal, TypedDict

from yulon import __version__, git, networking, platform, runner
from yulon.catalog import preflight
from yulon.log import get_logger, use_utf8_streams

logger = get_logger(__name__)

_BOUNDS_SECONDS = 10.0
"""How long one external answer may take; a slower one reads as unknown.

Ten seconds is the brief's own bound for the `docker info` question, and the
same bound every other probe gets: generous for answers that arrive in
milliseconds, short against a wedged daemon. `runner.run()` reports the expiry
as a non-zero return rather than raising, so a slow answer reads as unknown
through the same code as a refused one.
"""

RunCmd = Callable[[list[str]], subprocess.CompletedProcess[str]]
"""A command in, its completed process out. The seam the tests drive."""

OsRelease = dict[str, str]
"""`/etc/os-release` as parsed: every `KEY=value` line, quotes stripped."""

FirewallTool = Literal["ufw", "firewalld"] | None
"""Which firewall tool holds the rules: the brief's two, or `None`."""


class FirewallState(TypedDict):
    """The firewall reading: which tool holds the rules, and whether they are up."""

    tool: FirewallTool
    active: bool | None


class BoxReport(TypedDict):
    """The `--box-check` report: every key always present, `null` when unknown."""

    os_id: str | None
    os_version: str | None
    arch: str
    glibc: str | None
    docker_installed: bool
    docker_usable: bool
    docker_group_member: bool
    git_installed: bool
    free_disk_gib: float | None
    firewall: FirewallState
    is_root: bool | None
    yulon_version: str


def _live(argv: list[str]) -> subprocess.CompletedProcess[str]:
    """The real runner with the module bound: every external command gets one."""
    return runner.run(argv, timeout=_BOUNDS_SECONDS)


def read_os_release(path: Path | None = None) -> OsRelease:
    """Parse `/etc/os-release` into `KEY=value` pairs; anything unread is `{}`.

    Quotes come off the value (`VERSION_ID="24.04"` reads as `24.04`); a line
    with no `=` is skipped. `{}` is the same unknown the report prints as
    `null` upstream — a missing file (a container without one) is not an error.
    """
    release = path if path is not None else Path("/etc/os-release")
    try:
        text = release.read_text(encoding="utf-8")
    except OSError as exc:
        logger.info(f"could not read {release}: {exc}")
        return {}
    fields: OsRelease = {}
    for line in text.splitlines():
        key, sep, value = line.partition("=")
        if not sep or not key.strip():
            continue
        value = value.strip()
        if len(value) >= 2 and value.startswith('"') and value.endswith('"'):
            value = value[1:-1]
        fields[key.strip()] = value
    return fields


def detect_firewall(
    which: Callable[[str], str | None] | None = None,
    run: RunCmd | None = None,
    system: str | None = None,
) -> FirewallState:
    """Which Linux firewall tool is here, and whether it is up.

    Only `ufw` and `firewalld` count — anything else (Windows, macOS, or a
    Linux box with neither) answers `(None, None)` rather than naming a tool
    this report has no use for. The tool name is `platform.detect_firewall()`'s
    answer, narrowed to the brief's two; the state is read from the tool's own
    state answer and never guessed: `ufw status` says `Status: active`,
    firewalld's is `networking.detect_firewalld_daemon()`'s reading (rc 0 is
    running, rc 252/36 is positively stopped, rc 253 at uid 1000 is unknown).
    A missing tool, a refusal, a timeout — all read as `None`.
    """
    find = which if which is not None else shutil.which
    do: RunCmd = run if run is not None else _live
    here = system if system is not None else platform.detect()
    if here != "linux":
        return FirewallState(tool=None, active=None)
    if system is not None:
        # Seamed OS: pick ufw vs firewalld from `find` here. The shared
        # `platform.detect_firewall()` reads the REAL host through
        # `platform.detect()` ("netsh" on Windows, "alf" on macOS), so on a
        # faked `system` it would answer the wrong machine.
        if find("ufw") is not None:
            tool = "ufw"
        elif find("firewall-cmd") is not None:
            tool = "firewalld"
        else:
            tool = "none"
    else:
        tool = platform.detect_firewall(which=find)
    if tool == "ufw":
        try:
            proc = do(["ufw", "status"])
        except (OSError, subprocess.SubprocessError):
            return FirewallState(tool="ufw", active=None)
        if proc.returncode != 0:
            return FirewallState(tool="ufw", active=None)
        for line in proc.stdout.splitlines():
            if line.strip().lower().startswith("status:"):
                return FirewallState(
                    tool="ufw", active=line.split(":", 1)[1].strip().lower() == "active"
                )
        return FirewallState(tool="ufw", active=None)
    if tool == "firewalld":
        daemon = networking.detect_firewalld_daemon(run=do)
        if daemon == "running":
            return FirewallState(tool="firewalld", active=True)
        if daemon == "stopped":
            return FirewallState(tool="firewalld", active=False)
        return FirewallState(tool="firewalld", active=None)
    return FirewallState(tool=None, active=None)


def _effective_user(uid: int | None) -> str:
    """The account THIS process runs as: the passwd name for the effective uid.

    `platform._linux_user(None)` answers the INVOKER under sudo (`SUDO_USER`),
    which is the right account to invite into the group but the wrong account
    for a read-only check: `is_root` already answers about this process, and
    the group probe must agree with it. The passwd database is the same source
    `_linux_user` trusts; the environment fallback is its last line too.
    """
    if uid is not None:
        try:
            pwd = importlib.import_module("pwd")
            return str(pwd.getpwuid(uid).pw_name)
        except (ImportError, KeyError):
            pass
    try:
        return getpass.getuser()
    except Exception:  # noqa: BLE001 - no user database; the probe degrades to a name
        return os.environ.get("USER") or os.environ.get("USERNAME") or "deck"


def check(
    *,
    run: RunCmd | None = None,
    which: Callable[[str], str | None] | None = None,
    os_release: OsRelease | None = None,
    machine: str | None = None,
    libc: tuple[str, str] | None = None,
    user: str | None = None,
    euid: int | None = None,
    home_free_bytes: int | float | None = None,
    system: str | None = None,
) -> BoxReport:
    """Collect the pre-check report; every external probe bounded, every failure a `null`.

    Everything is a seam so the tests can drive each branch without the box:
    `run` for the subprocesses, `which` for PATH, `os_release` for the file,
    `machine`/`libc` for what the stdlib would say, `user`/`euid` for who runs
    this, `system` for which OS to behave as. A `home_free_bytes` of `None`
    (the default) reads the live home folder through `preflight.free_bytes()`;
    anything else is taken as given, so a test can be a full disk without
    filling one. `run` defaults to the live runner with the module bound on
    every call; a probe that raises or times out answers unknown, never raises.

    Where a helper already asks the question, it is asked: `docker_ready()`
    for the daemon (the same probe every install asks), `git_available()` for
    git (which knows the macOS stub), `platform._docker_group_member()` for
    the exact-token group read (`dockerd` must not read as a member),
    `networking.detect_firewalld_daemon()` for firewalld's state, and
    `preflight.free_bytes()` for the disk. The one exception is the hand the
    helper reads: both pass `run` and the check's own `find`, so the fake PATH
    and the fake runner decide what is installed. Without them the `which` half
    would read the live PATH no test can fake, and the macOS-stub half would
    read the real `sys.platform` on a Mac suite run instead of the OS the test
    handed in.
    """
    do: RunCmd = run if run is not None else _live
    find = which if which is not None else shutil.which
    release = os_release if os_release is not None else read_os_release()
    docker_installed = find("docker") is not None
    docker_usable = docker_installed and platform.docker_ready(run=do, timeout=_BOUNDS_SECONDS)
    here = system if system is not None else platform.detect()
    uid = euid
    if uid is None:
        geteuid = getattr(os, "geteuid", None)
        uid = geteuid() if geteuid is not None else None
    if user is not None:
        who = user
    elif uid == 0:
        # Under sudo `platform._linux_user(None)` answers the INVOKER
        # (`SUDO_USER`), while `is_root` above answers about THIS process --
        # so the group probe would ask about one account and the root flag
        # about another. The check asks whether this process may reach the
        # daemon, so it asks about the process: the passwd name for the
        # effective uid, falling back to the environment the same way
        # `_linux_user` does.
        who = _effective_user(uid)
    else:
        who = platform._linux_user(None)
    sys_name = {"linux": "linux", "windows": "win32", "macos": "darwin"}[here]
    git_installed = git.git_available(run=do, which=find, sys_platform=sys_name)
    free = home_free_bytes
    if free is None:
        free = preflight.free_bytes(Path.home())
    libc_ver = libc if libc is not None else _stdlib_platform.libc_ver()
    report: BoxReport = {
        "os_id": release.get("ID") or None,
        "os_version": release.get("VERSION_ID") or None,
        "arch": machine if machine is not None else _stdlib_platform.machine(),
        "glibc": libc_ver[1] if libc_ver[0].lower() == "glibc" else None,
        "docker_installed": docker_installed,
        "docker_usable": docker_usable,
        # `id -nG <user>` reads the group database, not this login session, so
        # `docker_group_member` can be true while `docker_usable` is still false
        # until the user logs in again.
        "docker_group_member": platform._docker_group_member(do, who),
        "git_installed": git_installed,
        "free_disk_gib": free / 1024**3 if free is not None else None,
        "firewall": detect_firewall(which=find, run=do, system=system),
        "is_root": uid == 0 if uid is not None else None,
        "yulon_version": __version__,
    }
    return report


def main() -> int:
    """Print the report as one JSON object on one line; 1 only if the check itself crashed."""
    use_utf8_streams()
    try:
        report: BoxReport = check()
    except Exception as exc:  # noqa: BLE001 - the crash is the report; print it, do not trace it
        logger.warning(f"--box-check could not run: {type(exc).__name__}: {exc}")
        print(json.dumps({"error": f"{type(exc).__name__}: {exc}"}))
        return 1
    # `ensure_ascii` stays defaulted: a redirected Windows stdout is cp1252, and
    # the `YULON_PROVISION_JSON` line died there once with UnicodeEncodeError
    # (main.py). An ASCII-safe line is a lossless one — json.loads reads it back.
    print(json.dumps(report))
    return 0
