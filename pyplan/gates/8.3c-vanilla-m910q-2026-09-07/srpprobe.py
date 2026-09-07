"""Which SRP6 recipe does THIS tree store? Asked of a row whose password is known.

Not derived from reading a core and hoping. An account is created with a known
password through the app, its `s` and `v` are read back, and every candidate
recipe is computed until one reproduces `v` exactly. The one that does is this
tree measured fact.
"""
import hashlib, sys
from pathlib import Path
sys.path.insert(0, "/home/pk/gate83b/pylauncher")
from yulon.catalog.catalog import load_catalog
from yulon.ui import controller_view as v_mod
from yulon.ui.controller_view import ControllerServices

GAME, DIR = sys.argv[1], sys.argv[2]
USER, PASS = sys.argv[3], sys.argv[4]

E = load_catalog().get(GAME); D = Path.home()/DIR
svc = ControllerServices.for_entry(E, D)
made = svc.create_account(USER, PASS, 0)
print("account:", "created" if made.created else "already there")

sql = v_mod._sql_for(E, v_mod._db_password(E, D), wsl_distro=None)
AUTH = E.schema_map()["auth"]; Q = chr(39); row = sql.query("auth", f"SELECT s, v FROM {AUTH}.account WHERE username = {Q}{USER}{Q};").strip()
s_hex, v_hex = row.split()
print(f"s = {s_hex}")
print(f"v = {v_hex}")

N = int("894B645E89E1535BBDAD5B8B290650530801B18EBFBF5E8FAB3C82872A3E9BB7", 16)
g = 7
h1 = hashlib.sha1(f"{USER.upper()}:{PASS.upper()}".encode()).digest()

for s_order in ("big", "little"):
    s_bytes = bytes.fromhex(s_hex)
    if s_order == "little":
        s_bytes = s_bytes[::-1]
    for h1_order in ("as-is", "reversed"):
        h = h1[::-1] if h1_order == "reversed" else h1
        for x_order in ("little", "big"):
            x = int.from_bytes(hashlib.sha1(s_bytes + h).digest(), x_order)
            val = pow(g, x, N)
            for v_order in ("big", "little"):
                got = val.to_bytes(32, v_order).hex().upper().lstrip("0") or "0"
                want = v_hex.upper().lstrip("0")
                if got == want:
                    print(f"MATCH: s={s_order} h1={h1_order} x={x_order} v={v_order}")
