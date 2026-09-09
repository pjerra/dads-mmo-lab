"""Direct-driving feature sweep for the wow-server-playerbots install.

Reuses the exact production wiring (ControllerServices.for_wotlk) that the
GUI's controller_view.py buttons call, so this is not a reimplementation --
it is the same seam, called non-interactively because the native
xdg-desktop-portal folder picker has no AT-SPI content over this SSH-driven
session (documented separately).
"""
import sys
import traceback
from pathlib import Path

sys.path.insert(0, str(Path.home() / "lane710b" / "checkout" / "pylauncher"))

SERVER_DIR = Path.home() / "wowserver"

results = []


def section(name):
    print(f"\n{'=' * 20} {name} {'=' * 20}")


def ok(msg):
    print(f"[OK] {msg}")
    results.append(("OK", msg))


def fail(msg, exc=None):
    print(f"[FAIL] {msg}")
    if exc:
        traceback.print_exc()
    results.append(("FAIL", msg))


section("Loading catalog entry + building ControllerServices.for_wotlk (real GUI wiring)")
from yulon.catalog.catalog import load_catalog
from yulon.ui.controller_view import ControllerServices

import yulon  # noqa: E402  -- provenance, printed rather than assumed

print("yulon package under test:", Path(yulon.__file__).resolve())

catalog = load_catalog()
entry = catalog.get("wow-wotlk")
services = ControllerServices.for_wotlk(entry, SERVER_DIR)
print("entry:", entry.id, entry.name if hasattr(entry, "name") else "")
print("services built OK:", services)

section("Controller status()")
try:
    status = services.controller.status()
    print("status:", status)
    ok(f"status() -> db={status.db} auth={status.auth} world={status.world}")
except Exception as e:
    fail("status()", e)

section("Controller import_state()")
# 2026-09-08 re-run, the second and last assertion these drivers gained. The 2026-09-05
# folder's README names this exact line as a false pass:
# `aborted-nodockergroup/sweep1.log:44` reads
# `[OK] import_state() -> ImportState(state='unreadable', detail='could not list the
# databases: permission denied …')` -- green, on a driver that could not reach docker at
# all. `unreadable` is the one answer that means the question was not asked, so it fails
# here. Every other state is reported as it comes.
try:
    istate = services.controller.import_state()
    print("import_state:", istate)
    if getattr(istate, "state", None) == "unreadable":
        fail(
            f"import_state() came back 'unreadable', which means the databases could not be "
            f"listed at all -- this is the 2026-09-05 aborted run's false pass: {istate}"
        )
    else:
        ok(f"import_state() -> {istate}")
except Exception as e:
    fail("import_state()", e)

section("Controller port_conflicts()")
try:
    conflicts = services.controller.port_conflicts()
    print("port_conflicts:", conflicts)
    ok(f"port_conflicts() -> {conflicts}")
except Exception as e:
    fail("port_conflicts()", e)

print("\n\n=== SUMMARY SO FAR ===")
for status_, msg in results:
    print(status_, msg)
