"""Point the client at 127.0.0.1 with Yu`lon`s own writer.

`networking.write_client_realmlist()` is the app`s own function for the file
the Networking tab only NAMES ("Players set realmlist to: 127.0.0.1"): the tab
writes the realm row in `tw_logon.realmlist` and prints the address, and no
button in this build writes the client`s `realmlist.wtf` (the function has no
UI caller; `tests/test_spine.py` lists it as a writer with a glob). Calling it
here is Yu`lon`s own code doing it, rather than a hand edit.
"""
from __future__ import annotations

from datetime import datetime
from pathlib import Path

from yulon import networking

CLIENT = Path("/home/pk/clients/TurtleWoW")
rl = CLIENT / "realmlist.wtf"
print(f"=== client realmlist ===  {datetime.now().astimezone():%Y-%m-%d %H:%M:%S %Z}")
print("before:", repr(rl.read_bytes()))
out = networking.write_client_realmlist(CLIENT, "127.0.0.1")
print("write_client_realmlist wrote:", out)
print("after: ", repr(out.read_bytes()))
