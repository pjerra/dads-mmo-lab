"""Read the realm row the Networking tab just wrote, and the client`s own realmlist file.

Read-only. `realmlist_address_query()` is the app`s own SELECT -- the one that
says whether `realmlist_sql()` would change anything -- so the row is read
through the same words the plan is written in. Password via MYSQL_PWD.
"""
from __future__ import annotations

import os
import subprocess
import sys
from datetime import datetime
from pathlib import Path

from yulon import networking
from yulon.catalog.catalog import load_catalog

SERVER = Path("/home/pk/tortoise-vm")
CLIENT = Path("/home/pk/clients/TurtleWoW")
entry = load_catalog().get("wow-tortoise")
spec = entry.container_spec()
db = entry.install.native.db
os.environ["MYSQL_PWD"] = (
    (SERVER / (entry.install.password.file or ".db_password")).read_text().strip()
)
print(f"=== realmlist: {sys.argv[1]} ===  {datetime.now().astimezone():%Y-%m-%d %H:%M:%S %Z}")
q = networking.realmlist_address_query(entry)
print("the app`s own query:", q)
out = subprocess.run(
    ["docker", "exec", "-i", "-e", "MYSQL_PWD", spec.db, db.client, "-uroot", "-N", "-B"],
    input=q + "\n",
    capture_output=True,
    text=True,
)
print("row:", repr(out.stdout.strip()), out.stderr.strip())
full = subprocess.run(
    ["docker", "exec", "-i", "-e", "MYSQL_PWD", spec.db, db.client, "-uroot", "-t"],
    input="SELECT id, name, address, port FROM tw_logon.realmlist;\n",
    capture_output=True,
    text=True,
)
print(full.stdout)
rl = CLIENT / "realmlist.wtf"
print("client realmlist file:", rl)
print("  bytes:", repr(rl.read_bytes()))
