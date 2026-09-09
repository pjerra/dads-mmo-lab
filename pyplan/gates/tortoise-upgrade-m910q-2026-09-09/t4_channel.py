"""T4 step 5 -- prove the upgraded world is the head, through the app's own command channel.

The wiring is `ui/controller_view.py::_for_tortoise` with the UI taken off: the same
`channel_setup.InstallChannel`, the same `controller_wow_tortoise.accounts.create_account`
(rank from the entry), the same `channel.SoapChannel`. Nothing here re-implements a seam.

Two subcommands, and the ORDER is the point (`pyplan/gates/tortoise-soap-yulon-arch-2026-09-09/`:
this fork reads `account.rank` at startup, so an account created while the world runs is refused
until it restarts):

`arm`     with the world DOWN: turn the channel on in the conf and the override, then press
          `prove()` once. The account is written by that press; the server is not there to
          answer, so the state parks at Pending. That is the intended half-step.
`ask`     with the world UP: `prove()` again -- which verifies rather than re-creates -- and then
          `server info` through the live channel.

The channel account's generated password is never printed and never written by this script: it
lives only in the app's own credential file (`channel_setup.save_credential`).
"""

from __future__ import annotations

import sys
import time
from pathlib import Path

from yulon import channel as channel_module
from yulon import channel_setup, docker, resources
from yulon.catalog import composegen
from yulon.catalog.catalog import load_catalog
from yulon.controller_wow_tortoise import accounts as tortoise_accounts

SERVER_DIR = Path.home() / "tortoise-server"
GAME = "wow-tortoise"


def _channel():
    entry = load_catalog().get(GAME)
    password = entry.install.db_password(SERVER_DIR)
    sql = tortoise_accounts.sql_for(password)
    spec = entry.container_spec()
    setup = channel_setup.InstallChannel(
        entry,
        SERVER_DIR,
        templates_root=resources.installers_dir(),
        install_id=composegen.install_id(SERVER_DIR),
        db_password=password,
        create=lambda name, pw, level: tortoise_accounts.create_account(
            sql, name, pw, gm_level=level
        ),
        reset=lambda name, pw: tortoise_accounts.reset_own_password(sql, name, pw),
        channel_for=lambda endpoint: channel_module.SoapChannel(
            endpoint=endpoint,
            state_of=lambda: docker.container_state(spec.world),
        ),
    )
    return entry, setup


def _say_state(where: str, state: object) -> None:
    account = getattr(state, "account", "")
    print(f"{where}: state={type(state).__name__} account={account or '(none)'}", flush=True)


def arm() -> int:
    entry, setup = _channel()
    ops = entry.operations
    print(
        f"entry operations: channel={ops.channel} port={ops.port} "
        f"namespace={ops.namespace} gm_level={ops.gm_level}",
        flush=True,
    )
    enabled = setup.enable(world_running=False)
    print(f"enable: wrote {enabled.path.name}, changed={enabled.changed}", flush=True)
    _say_state("before prove", setup.setup_state())
    _say_state("after prove (world down: the account is written, the ask cannot land)", setup.prove())
    return 0


def ask() -> int:
    _, setup = _channel()
    _say_state("from disk", setup.setup_state())
    state = setup.prove()
    _say_state("after prove (world up)", state)
    if type(state).__name__ == "Refused":
        # `arm` ran in a DIFFERENT process, and only a VERIFIED credential is
        # saved to disk -- so this process minted a new password for an account
        # that already exists, and `create_account` rightly refused to re-salt
        # it. That is exactly the state the Repair button is for, and it uses
        # this tree's own `reset_own_password` (a SQL write to the app's own
        # account, never the fork's `account set password`, which locks an
        # account out -- 8.3d). In the app both halves are one process and this
        # arm is not reached.
        _say_state("after repair", setup.repair())
    live = setup.live_channel()
    if live is None:
        print("no live channel: the credential is not verified", flush=True)
        return 1
    for command in ("server info", "server motd"):
        answer = live.send(command)
        print(f"--- {command} -> outcome={answer.outcome} ---", flush=True)
        print(answer.text.strip(), flush=True)
        if answer.reason:
            print(f"reason: {answer.reason}", flush=True)
    return 0


def full() -> int:
    """Both halves in ONE process, which is how the running app holds this state.

    `arm` then `ask` as two processes cannot work on this tree, and finding out why is a
    finding of its own (see the README): only a VERIFIED credential is written to disk, so
    the second process minted a second password, `create_account` correctly refused to
    re-salt a row it did not just make, and the Repair path that exists for exactly that
    raised `Unknown column 'salt'` -- `controller_wow_wotlk.accounts.reset_own_password`
    knows two schemes and this tree is the third.

    So: the account is created with the world DOWN (its rank is read at startup), the world
    is started, and the same object asks. The stack is left UP; the caller stops it.
    """
    import subprocess
    from datetime import datetime, timezone

    _, setup = _channel()
    _say_state("before prove (world down)", setup.setup_state())
    _say_state("after prove (world down)", setup.prove())
    # The container is restarted, not recreated, so its log still holds the
    # PREVIOUS run's ready banner. `--since` is what makes the wait below read
    # this start rather than the last one -- an artifact that reads exactly
    # like success is the failure mode this whole gate keeps meeting.
    since = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    print(f"--- starting the stack; the rank is read here (watching from {since}) ---", flush=True)
    subprocess.run(
        ["docker", "compose", "up", "-d"], cwd=SERVER_DIR, check=True, capture_output=True
    )
    for _ in range(90):
        logs = subprocess.run(
            ["docker", "logs", "--since", since, "tortoise-mangosd"],
            capture_output=True,
            text=True,
        )
        blob = logs.stdout + logs.stderr
        if "World server is up and running" in blob:
            print("world: up and running", flush=True)
            break
        if "AutoUpdater FAILED" in blob:
            print("world: AutoUpdater FAILED", flush=True)
            return 1
        time.sleep(10)
    else:
        print("world: did not report ready in 15 minutes", flush=True)
        return 1
    _say_state("after prove (world up)", setup.prove())
    live = setup.live_channel()
    if live is None:
        print("no live channel: the credential is not verified", flush=True)
        return 1
    for command in ("server info", "server motd"):
        answer = live.send(command)
        print(f"--- {command} -> outcome={answer.outcome} ---", flush=True)
        print(answer.text.strip(), flush=True)
        if answer.reason:
            print(f"reason: {answer.reason}", flush=True)
    return 0


if __name__ == "__main__":
    what = sys.argv[1] if len(sys.argv) > 1 else ""
    if what == "arm":
        sys.exit(arm())
    if what == "ask":
        sys.exit(ask())
    if what == "full":
        sys.exit(full())
    print(__doc__)
    sys.exit(2)
