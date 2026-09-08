"""Is there an OFFLINE character with a corpse yet? Bounded, no background loop."""
import sys, time
from pathlib import Path
sys.path.insert(0,"/home/pk/gate84c/pylauncher")
from yulon.catalog.catalog import load_catalog
from yulon.ui import controller_view as v
E=load_catalog().get("wow-vanilla");D=Path.home()/"vanilla-75b"
sql=v._sql_for(E,v._db_password(E,D),wsl_distro=None)
q=lambda st: sql.query("characters",st).strip()
deadline = time.monotonic() + float(sys.argv[1] if len(sys.argv)>1 else 120)
while time.monotonic() < deadline:
    total = q("SELECT COUNT(*) FROM characters.corpse;")
    off = q("SELECT c.name,c.guid,co.corpse_type FROM characters.corpse co JOIN characters.characters c ON c.guid=co.player WHERE c.online=0;")
    print(time.strftime("%H:%M:%SZ", time.gmtime()), "corpses:", total, "| offline bearers:", off.replace("\n"," ; ") or "none", flush=True)
    if off:
        break
    time.sleep(15)
