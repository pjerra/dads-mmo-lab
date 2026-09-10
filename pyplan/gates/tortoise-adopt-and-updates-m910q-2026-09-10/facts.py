"""Read the install's own facts through the app, so nothing here re-derives them."""

from __future__ import annotations

from datetime import datetime
from pathlib import Path

from yulon.catalog.catalog import load_catalog
from yulon.catalog.installer import InstallOptions

SERVER = Path("/home/pk/tortoise-server")
entry = load_catalog().get("wow-tortoise")
spec = entry.container_spec()
print(f"=== facts ===  {datetime.now().astimezone():%Y-%m-%d %H:%M:%S %Z}")
print("entry:", entry.id, entry.name)
print("containers: db=%s world=%s auth=%s" % (spec.db, spec.world, spec.auth))
print("schemas:", entry.databases.auth, entry.databases.characters, entry.databases.world)
print("password file:", entry.install.password.file, "mode:", entry.install.password.mode)

from yulon.catalog import native  # noqa: E402

phases = native.update_phases(entry)
print("rerunnable phases:", [p.name for p in phases])
print("phase globs:", [list(p.files) for p in phases])
print("ADOPT_BUTTON_LABEL:", native.ADOPT_BUTTON_LABEL)
print("UPDATES_BUTTON_LABEL:", native.UPDATES_BUTTON_LABEL)

from yulon.install_wiring import installer_for_app  # noqa: E402

engine = installer_for_app(entry)
options = InstallOptions(server_dir=SERVER)
print("adopt stages:", [(s.name, s.recorded) for s in engine.adopt_stages()])
print("update stages:", [(s.name, s.recorded) for s in engine.update_stages()])
print("marker row:", engine.marker_row())
print("--- adopt confirmation ---")
print(engine.adopt_confirmation(options))
print("--- end adopt confirmation ---")
print("--- updates confirmation ---")
print(engine.update_confirmation(options))
print("--- end updates confirmation ---")
