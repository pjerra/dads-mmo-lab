"""Start this install's database ALONE, through the app's own primitive.

`docker.start_database()` is the function T7 wired for the applier's stopped-world
path and the one `stage_start_db` calls; using it here rather than a raw
`docker start` keeps the before-readings on the same code path the press uses.
The world server is not touched.
"""

from __future__ import annotations

from pathlib import Path

from yulon import docker
from yulon.catalog.catalog import load_catalog

SERVER = Path("/home/pk/tortoise-server")
entry = load_catalog().get("wow-tortoise")
spec = entry.container_spec()
print("world_running before:", docker.world_running(spec.world))
started = docker.start_database(spec, SERVER, because="the before-readings need it")
print("start_database had to start it:", started)
print("world_running after:", docker.world_running(spec.world))
