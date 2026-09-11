"""Stop or Start this install through the app's own controller, never a raw docker stop.

`sys.argv[1]` is `stop`, `start` or `status`. The controller is the one the
Server tab drives (`ControllerServices.for_entry(...).controller`), so what
happens here is what that button does.
"""

from __future__ import annotations

import sys
from datetime import datetime
from pathlib import Path

from yulon import docker
from yulon.catalog.catalog import load_catalog
from yulon.ui.controller_view import ControllerServices

SERVER = Path("/home/pk/tortoise-server")
entry = load_catalog().get("wow-tortoise")
spec = entry.container_spec()
what = sys.argv[1]

print(f"=== control {what} ===  {datetime.now().astimezone():%Y-%m-%d %H:%M:%S %Z}")
controller = ControllerServices.for_entry(entry, SERVER).controller
print("status before:", controller.status())
print("world_running before:", docker.world_running(spec.world))
if what == "stop":
    print("controller.stop() ->", controller.stop())
elif what == "start":
    controller.start()
    print("controller.start() returned")
print("status after:", controller.status())
print("world_running after:", docker.world_running(spec.world))
print(f"=== done ===  {datetime.now().astimezone():%Y-%m-%d %H:%M:%S %Z}")
