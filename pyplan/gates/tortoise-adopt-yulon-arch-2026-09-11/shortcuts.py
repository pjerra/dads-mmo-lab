"""Read shortcuts.vdf with Yu`lon`s own codec and print ONLY the entry shape.

`yulon.steam.vdf_parse` is the app`s own reader (T17). Printed per entry: the
app name, the exe, and the two time fields Steam itself writes when an entry is
launched -- which is what moves the file`s checksum without any entry changing.
Nothing here is written. The Steam account is named only by its userdata id.
"""
from __future__ import annotations

import sys
from datetime import datetime, timezone
from pathlib import Path

from yulon import steam

VDF = Path(sys.argv[2])
print(f"=== shortcuts.vdf: {sys.argv[1]} ===  {datetime.now().astimezone():%Y-%m-%d %H:%M:%S %Z}")
print("path:", VDF)
data, consumed = steam.vdf_parse(VDF.read_bytes())
print("bytes consumed:", consumed, "of", VDF.stat().st_size)
shortcuts = data.get("shortcuts", data)
for key in sorted(shortcuts, key=lambda k: int(k) if str(k).isdigit() else 0):
    e = shortcuts[key]
    print(f"  [{key}]")
    for field in ("AppName", "appname", "Exe", "exe", "StartDir", "startdir", "LaunchOptions", "launchoptions"):
        if field in e:
            print(f"      {field}: {e[field]!r}")
    for field in ("LastPlayTime", "lastplaytime"):
        if field in e:
            v = e[field]
            when = datetime.fromtimestamp(v, timezone.utc).isoformat() if isinstance(v, int) and v else "0 (never)"
            print(f"      {field}: {v} -> {when}")
