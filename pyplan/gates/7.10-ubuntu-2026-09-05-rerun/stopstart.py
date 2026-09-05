"""Stop or start the live WotLK install THROUGH THE APP, so the cancel run can have the ports.

`python stopstart.py stop` / `python stopstart.py start`. Nothing here removes a container:
`Controller.stop()` and `Controller.start()` are the two the Server tab's own buttons call.
"""

from __future__ import annotations

import subprocess
import sys
from datetime import datetime
from pathlib import Path

sys.path.insert(0, "/home/pk/p7/checkout/pylauncher")

from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui.controller_view import ControllerServices  # noqa: E402

SERVER_DIR = Path("/home/pk/wowserver")


def ps() -> str:
    return subprocess.run(
        ["docker", "ps", "-a", "--format", "{{.Names}}\t{{.Status}}"],
        capture_output=True, text=True,
    ).stdout.strip()


def main() -> int:
    what = sys.argv[1]
    entry = load_catalog().get("wow-wotlk")
    services = ControllerServices.for_wotlk(entry, SERVER_DIR)
    print(f"{datetime.now().astimezone().isoformat(timespec='seconds')} before {what}:")
    print(ps())
    print("status before:", services.controller.status())
    if what == "stop":
        services.controller.stop()
    elif what == "start":
        services.controller.start()
    else:
        raise SystemExit(f"unknown: {what}")
    print(f"{datetime.now().astimezone().isoformat(timespec='seconds')} after {what}:")
    print(ps())
    print("status after:", services.controller.status())
    print("port_conflicts():", services.controller.port_conflicts())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
