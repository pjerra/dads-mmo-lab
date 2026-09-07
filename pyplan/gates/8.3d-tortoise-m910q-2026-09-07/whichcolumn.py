"""Which column does THIS fork check, and what does it hold?

The account row here carries `sha_pass_hash`, `v`, `s`, `pass_verif` AND
`security` beside `rank`, so nothing about it can be assumed. An account is
created through the app with a known password and every candidate recipe is
computed against every candidate column.
"""

import hashlib
import sys
from pathlib import Path

sys.path.insert(0, "/home/pk/gate83b/pylauncher")
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui import controller_view as v_mod  # noqa: E402
from yulon.ui.controller_view import ControllerServices  # noqa: E402

USER, PASS = "SHAPROBE", "kn0wn-p@ss77"
E = load_catalog().get("wow-tortoise")
D = Path.home() / "tortoise-server"
svc = ControllerServices.for_entry(E, D)
made = svc.create_account(USER, PASS, 0)
print("account:", "created" if made.created else "already there")

sql = v_mod._sql_for(E, v_mod._db_password(E, D), wsl_distro=None)
AUTH = E.schema_map()["auth"]
cols = ["sha_pass_hash", "v", "s", "pass_verif", "rank", "security"]
row = sql.query(
    "auth",
    f"SELECT {', '.join(cols)} FROM {AUTH}.account WHERE username = '{USER}';",
).strip()
values = dict(zip(cols, row.split("\t")))
for c in cols:
    print(f"  {c:14} = {values.get(c)!r}")

candidates = {
    "SHA1(USER:PASS)": hashlib.sha1(f"{USER.upper()}:{PASS.upper()}".encode()).hexdigest().upper(),
    "SHA1(user:pass)": hashlib.sha1(f"{USER}:{PASS}".encode()).hexdigest().upper(),
    "SHA1(PASS)": hashlib.sha1(PASS.upper().encode()).hexdigest().upper(),
}
for name, want in candidates.items():
    for c, got in values.items():
        if got and got.strip().upper() == want:
            print(f"MATCH: {c} = {name}")
