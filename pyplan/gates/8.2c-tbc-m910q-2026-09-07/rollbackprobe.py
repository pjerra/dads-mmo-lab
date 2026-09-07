"""The rollback fix, against the real 71 kB mangosd.conf rather than a fixture.

The review's first finding was that a rollback restored the whole file from a
backup of unknown age, to undo four keys. This edits something unrelated the way
a person would, rolls back, and asks the file what survived.

Nothing here starts or stops a server: the world is already down.

Usage:  python rollbackprobe.py
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, "/home/pk/gate82c/pylauncher")

from yulon import channel_setup  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402

ENTRY = load_catalog().get("wow-tbc")
SERVER_DIR = Path.home() / "tbc-7.4c"
CONF = SERVER_DIR / ENTRY.operations.enable_conf.file
MARK = 'Motd = "the rollback probe was here"'


def keys_of(text: str) -> dict[str, str]:
    wanted = ("SOAP.Enabled", "SOAP.IP", "SOAP.Port", "Ra.Enable", "Console.Enable")
    found = {}
    for line in text.splitlines():
        for key in wanted:
            if line.startswith(key) and line[len(key) :].lstrip().startswith("="):
                found[key] = line.split("=", 1)[1].strip()
    return found


def main() -> None:
    before = CONF.read_text(encoding="utf-8")
    print(f"the file: {len(before)} bytes, {len(before.splitlines())} lines")
    print(f"channel keys now: {keys_of(before)}")

    # An unrelated edit, after the press, exactly as a person would make one.
    edited = before.replace("Console.Enable = 1", f"Console.Enable = 0\n{MARK}")
    CONF.write_text(edited, encoding="utf-8")
    print(f"edited: Console.Enable -> 0, and one new line ({len(edited)} bytes)")

    rolled = channel_setup.roll_back(ENTRY, SERVER_DIR)
    after = CONF.read_text(encoding="utf-8")
    print(f"roll_back() -> {rolled}")
    print(f"the file now: {len(after)} bytes, {len(after.splitlines())} lines")
    print(f"channel keys now: {keys_of(after)}")
    print(f"the unrelated edit survived: {MARK in after}")
    print(f"Console.Enable survived as 0: {'Console.Enable = 0' in after}")
    print(f"the backup is gone: {not CONF.with_name(CONF.name + '.before-channel').exists()}")

    assert MARK in after, "the rollback threw away a line added after the press"
    assert "Console.Enable = 0" in after, "the rollback threw away an unrelated edit"
    assert keys_of(after)["SOAP.Enabled"] == "0", "the channel was not turned off"
    assert keys_of(after)["SOAP.IP"] == "127.0.0.1", "SOAP.IP was not put back"
    print("PROVED: the channel's own keys went back and nothing else moved")

    # Put the box back the way the gate left it: the edit undone, channel on.
    CONF.write_text(
        after.replace(f"Console.Enable = 0\n{MARK}", "Console.Enable = 1"), encoding="utf-8"
    )
    channel_setup.enable(
        ENTRY,
        SERVER_DIR,
        templates_root=__import__("yulon.resources", fromlist=["x"]).installers_dir(),
        world_running=False,
        db_password=__import__(
            "yulon.ui.controller_view", fromlist=["x"]
        )._db_password(ENTRY, SERVER_DIR),
    )
    final = CONF.read_text(encoding="utf-8")
    print(f"restored: {len(final)} bytes, channel keys {keys_of(final)}")


if __name__ == "__main__":
    main()
