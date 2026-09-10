"""Ask the adopt press itself what it says about `~/tortoise-vm`.

The Modules tab greys "Adopt as imported..." here, because the tab enables it
only on a `populated` reading and these databases read `imported`
(`controller_view.py`: "On a server Yu`lon installed it never lights up at all,
because that server already carries the row"). So the button cannot be pressed
on this box. This drives the SAME engine method the button is wired to
(`adopt_as_imported`, the generator `ControllerView.adopt_as_imported` runs)
and records the sentence it raises, so the refusal is in the press`s own words
and not only in a greyed pixel.

Nothing is written: the refusal is raised inside `stage_adopt` before
`write_import_marker()`, and the database was already up so the press`s
conditional stop does not fire.
"""

from __future__ import annotations

from datetime import datetime
from pathlib import Path

from yulon.catalog.catalog import load_catalog
from yulon.catalog.installer import InstallerError, InstallOptions
from yulon.install_wiring import installer_for_app

SERVER = Path("/home/pk/tortoise-vm")
entry = load_catalog().get("wow-tortoise")
engine = installer_for_app(entry)
opts = InstallOptions(server_dir=SERVER)

print(f"=== adopt press ===  {datetime.now().astimezone():%Y-%m-%d %H:%M:%S %Z}")
print("adopt stages:", [(s.name, s.recorded) for s in engine.adopt_stages()])
print()
print("--- the confirmation this button would show ---")
print(engine.adopt_confirmation(opts))
print()
print("--- the press, line by line ---")
try:
    for line in engine.adopt_as_imported(opts):
        print("  >", line)
except InstallerError as exc:
    print()
    print("REFUSED (InstallerError), the sentence a user reads:")
    print("  !", exc)
else:
    print()
    print("the press ran to the end (no refusal)")
print(f"=== done ===  {datetime.now().astimezone():%Y-%m-%d %H:%M:%S %Z}")
