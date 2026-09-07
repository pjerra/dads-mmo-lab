"""Did the password change, on a command that reported failure?

CMaNGOS's `HandleAccountSetPasswordCommand` sends its success message and then
`SetSentErrorMessage(true); return false;` -- deliberately, "to avoid normal
report for hide passwords" (`Level3.cpp:1178-1183`). SOAP turns a handler that
returned false into a fault, so a SUCCESSFUL change comes back as a failure.

This does not try to recompute SRP6. It reads the verifier, sends the command,
and reads it again: if the row moved, the change happened whatever the reply
said.

Usage:  python pwtruth.py
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, "/home/pk/gate83b/pylauncher")

from yulon import channel_setup, commands, soap  # noqa: E402
from yulon.catalog import composegen  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui import controller_view as view_module  # noqa: E402

ENTRY = load_catalog().get("wow-tbc")
SERVER_DIR = Path.home() / "tbc-7.4c"
TARGET = "GATE83B"


def read() -> tuple[str, str]:
    sql = view_module._sql_for(
        ENTRY, view_module._db_password(ENTRY, SERVER_DIR), wsl_distro=None
    )
    s, v = sql.query(
        "auth", f"SELECT s, v FROM realmd.account WHERE username = '{TARGET}';"
    ).strip().split()
    return s, v


def main() -> None:
    saved = channel_setup.load_credential(ENTRY.id, composegen.install_id(SERVER_DIR))
    assert saved is not None
    endpoint = saved.__class__(
        host=saved.host,
        port=saved.port,
        account=saved.account,
        password=saved.password,
        namespace=ENTRY.operations.namespace,
    )

    before = read()
    print(f"before : s={before[0][:16]}… v={before[1][:16]}…")

    line = commands.account_set_password(TARGET, "tru7h-p@ss99")
    reply = soap.execute(endpoint, line)
    print(f"the command reported: {reply.outcome!r}")
    print(f"  text: {reply.text.strip()[:120]!r}")

    after = read()
    print(f"after  : s={after[0][:16]}… v={after[1][:16]}…")
    print(f"the row CHANGED: {after != before}")

    if after != before and reply.outcome != "answered":
        print(
            "\\nFINDING: the password changed and the command said it did not.\\n"
            "On this core the outcome of `account set password` cannot be trusted;\\n"
            "only a read of the credential columns can say what happened."
        )


if __name__ == "__main__":
    main()
