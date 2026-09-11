"""Step 6 of the 8.7c gate: the running server's only word about a SQL mod.

`all-stackables` changes no conf key, so `.server motd`'s trick does not apply —
the check that the server *reports* something about it is a query, and that is
`stage_sqlmod`. What the RUNNING server can still say is that it loads
`item_template` with the new values and reaches its ready marker, which is worth
having because a bad value in that table is exactly the sort of thing a world
load refuses on. Written as its own file rather than a heredoc because the
counting query needs double quotes inside an f-string.

Run with `~/gate81b-venv/bin/python`, PYTHONPATH at the pylauncher checkout, from
the folder holding `gate87c.py`.
"""

from __future__ import annotations

import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from gate87c import SERVER, alive, manifest, say, services, sh, wait_ready  # noqa: E402

PASSWORD = (SERVER / ".db_password").read_text(encoding="utf-8").strip()
AT_200 = "stackable = 200"


def count(where: str) -> int:
    out = sh(
        "docker", "exec", "vanilla-db", "mariadb", "-uroot", f"-p{PASSWORD}",
        "-N", "-B", "mangos", "-e", f"SELECT COUNT(*) FROM item_template WHERE {where}",
    )
    return int(out.splitlines()[-1])


def main() -> None:
    alive("ground")
    ground = count(AT_200)
    say(f"ground: {ground} rows at 200 before this step")

    made = services()
    made.applier.install(manifest("all-stackables"), {"stack_size": "200"})
    applied = count(AT_200)
    say(f"installed again: {applied} rows at 200")
    assert applied > ground, "nothing changed; the restart below would prove nothing"

    say("stopping and starting through the app's own controller")
    made.controller.stop()
    mark = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(time.time() - 2))
    made.controller.start()
    wait_ready(mark)
    alive("after the restart with the SQL mod applied")
    say("the world loaded item_template with the new values and reached `Avg Diff:`")

    services().applier.remove(manifest("all-stackables"))
    back = count(AT_200)
    say(f"removed: {back} rows at 200")
    assert back == ground, f"{back} rows still at 200, expected {ground}"
    say("SQL MOD RESTART PASSED: the running server took the changed table and came back up")


if __name__ == "__main__":
    main()
