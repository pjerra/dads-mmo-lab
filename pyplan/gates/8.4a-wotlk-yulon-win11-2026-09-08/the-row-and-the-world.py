"""What `saveall` does and does not write, asked of this box rather than assumed.

The online arm's `set level` answered *"You changed level of Amvezuan to 60."*,
the client showed *"[Amvezuan] has earned the achievement [Level 60]!"* and the
row read 55 after twenty-four `saveall`s. 8.4a's Linux box wrote down the
general shape -- *for an ONLINE character the world holds the truth until it is
asked to save* -- and its gate then leaned on `saveall` to close the gap.

That is the sentence this probe is here to test, because a nudge that does not
nudge is a gate step whose failure would be read as the app's.

Usage:  python the-row-and-the-world.py <character>
"""

from __future__ import annotations

import sys
import time
from datetime import UTC, datetime
from pathlib import Path

sys.path.insert(0, r"C:\gate\src84a\pylauncher")

from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui import controller_view as view_module  # noqa: E402
from yulon.ui.controller_view import ControllerServices  # noqa: E402

SERVER_DIR = Path(r"D:\wow-server")
ENTRY = load_catalog().get("wow-wotlk")


def say(text: str) -> None:
    print(f"[{datetime.now(UTC).strftime('%H:%M:%SZ')}] {text}", flush=True)


def reader():
    return view_module._sql_for(ENTRY, view_module._db_password(ENTRY, SERVER_DIR), wsl_distro=None)


def row(name: str) -> str:
    return reader().query(
        "characters",
        f"SELECT level, health, ROUND(position_x, 2), online FROM acore_characters.characters "
        f"WHERE name = '{name}';",
    ).strip()


def channel():
    svc = ControllerServices.for_entry(ENTRY, SERVER_DIR)
    assert svc.channel_setup is not None
    live = svc.channel_setup.live_channel()
    assert live is not None
    return live


def main() -> None:
    name = sys.argv[1]
    chan = channel()
    say(f"===== GROUND: {name}'s row is  level/health/x/online = {row(name)} =====")
    answer = chan.send(f"character level {name} 61")
    say(f"the world was told to set level 61: {answer.outcome} {(answer.text or '').strip()[:70]!r}")
    time.sleep(2)
    say(f"the row, two seconds later          : {row(name)}")
    answer = chan.send("saveall")
    say(f"saveall answered                    : {answer.outcome} {(answer.text or '').strip()[:40]!r}")
    for wait in (2, 5, 10, 20):
        time.sleep(wait)
        say(f"the row, {wait:>2}s after that saveall : {row(name)}")
    say("what the world itself says about the character, through the same channel:")
    answer = chan.send(f"pinfo {name}")
    for line in (answer.text or "").splitlines():
        if line.strip():
            say(f"    {line.strip()}")


if __name__ == "__main__":
    main()
