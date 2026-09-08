"""Set TBCGATE's password through the app's own button, so the client can log in.

8.4b set it the same way and did not write the value down; this run does.
"""
import logging
import sys
from pathlib import Path

sys.path.insert(0, "/home/pk/gate84b-repress/pylauncher")
logging.basicConfig(level=logging.INFO, format="%(levelname)s %(name)s %(message)s")

from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui.controller_view import ControllerServices  # noqa: E402

E = load_catalog().get("wow-tbc")
D = Path.home() / "tbc-7.4c"
ACCOUNT = "TBCGATE"
PASSWORD = sys.argv[1]

admin = ControllerServices.for_entry(E, D).accounts
out = admin.set_password(ACCOUNT, PASSWORD)
print(f"set_password: done={out.done} {(out.text or out.problem).strip()!r}")
