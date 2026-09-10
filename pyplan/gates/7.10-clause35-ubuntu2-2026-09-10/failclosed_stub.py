"""A driver that dies where the real one is most dangerous — to exercise the trap.

Round 2's must-fix is that `run-t3.sh` threw away every driver's exit status and
walked on through its cleanup printing "run finished". Fixing that is one thing;
showing the fix works is another, and it cannot be shown by a run in which
nothing fails.

So this stands in for `clause35_falsify.py` at exactly its worst moment. The real
driver reads the realm row, sets it to `192.168.77.77` so plan and row disagree,
presses Apply, and puts the row back at B6. This one does the first half and then
exits 1 — the shape of a driver killed, or raising, between B1 and B6. Nothing
else about the invocation changes: the same `run-t3.sh`, the same trap, the same
box.

What the run then has to show, in `restore.log`, is that the runner exited
non-zero AND the row went back to 100.99.204.5 on both address columns with mask
255.255.255.0, with `ac-authserver` restarted and its own `Added realm` line read
back — none of which the round-1 script would have done, because it had already
decided the run was over.

It is deliberately small and it asserts nothing. A stub that could itself fail
for a second reason would make the reading ambiguous.
"""

from __future__ import annotations

import os
import subprocess
import sys

DISAGREEING = "192.168.77.77"


def mysql(statement: str) -> str:
    done = subprocess.run(
        ["docker", "exec", "-e", "MYSQL_PWD", "ac-database",
         "mysql", "-uroot", "-N", "-B", "-e", statement],
        capture_output=True, text=True, env={**os.environ, "MYSQL_PWD": "password"},
    )
    return done.stdout.strip()


def main() -> int:
    row = mysql("SELECT address, localAddress, localSubnetMask "
                "FROM acore_auth.realmlist WHERE id=1;")
    print(f"the row as found: {row!r}", flush=True)
    print(f"setting it to {DISAGREEING}, as the real driver's B1 does", flush=True)
    mysql(f"UPDATE acore_auth.realmlist SET address='{DISAGREEING}', "
          f"localAddress='{DISAGREEING}' WHERE id=1;")
    row = mysql("SELECT address, localAddress, localSubnetMask "
                "FROM acore_auth.realmlist WHERE id=1;")
    print(f"read back: {row!r}", flush=True)
    print("now dying where B6 would have put it back -- exit 1", flush=True)
    return 1


if __name__ == "__main__":
    sys.exit(main())
