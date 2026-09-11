"""8.4b's owed re-press: `character rename` on an ONLINE character, on TBC.

The one clause 8.4b's entry claimed and never pressed. `live-actions.py` sent
three actions and rename was not one of them; the only rename in that folder is
on `Bimokk`, offline.

The whole point of this script is the GROUND. A rename press whose `at_login`
bit is already set proves nothing -- it is a step whose assertion is true before
its action runs -- so this reads the row three times before it sends anything:

  1. before the client is started at all (the character must be offline and the
     bit clear -- if it is set, this refuses and says to clear it first),
  2. the moment the world says the character is logged in,
  3. immediately before the press, off the same row the app itself canonicalises
     against.

Then it reads the row at intervals WHILE the character is still online, waits
for the logout the client performs, and reads it once more. 8.4a found the row
moves at logout rather than at the press; whether that holds here is what this
run answers, and either answer is a true one.

The clock in every line is UTC.
"""
import argparse
import logging
import sys
import time
from pathlib import Path

sys.path.insert(0, "/home/pk/gate84b-repress/pylauncher")
logging.basicConfig(level=logging.INFO, format="%(levelname)s %(name)s %(message)s")

from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui import controller_view as v  # noqa: E402
from yulon.ui.controller_view import ControllerServices  # noqa: E402

E = load_catalog().get("wow-tbc")
D = Path.home() / "tbc-7.4c"
NAME = "Ddsgate"
Q = chr(39)
RENAME_BIT = 0x1

sql = v._sql_for(E, v._db_password(E, D), wsl_distro=None)
tab = ControllerServices.for_entry(E, D).play


def stamp(text: str) -> None:
    print(f"[{time.strftime('%H:%M:%SZ', time.gmtime())}] {text}", flush=True)


def row() -> dict:
    raw = sql.query(
        "characters",
        f"SELECT name, level, online, at_login, map FROM characters.characters "
        f"WHERE name = {Q}{NAME}{Q};",
    ).strip()
    if not raw:
        raise SystemExit(f"no character called {NAME} on this server")
    name, level, online, at_login, map_id = raw.split("\t")
    return {
        "name": name,
        "level": int(level),
        "online": int(online),
        "at_login": int(at_login),
        "map": int(map_id),
    }


def show(label: str, r: dict) -> None:
    flag = "SET" if r["at_login"] & RENAME_BIT else "clear"
    stamp(
        f"{label}: name={r['name']} level={r['level']} online={r['online']} "
        f"at_login={r['at_login']} (rename bit {flag}) map={r['map']}"
    )


def wait_for(online: int, seconds: int) -> dict | None:
    deadline = time.time() + seconds
    while time.time() < deadline:
        r = row()
        if r["online"] == online:
            return r
        time.sleep(5)
    return None


ap = argparse.ArgumentParser()
ap.add_argument("--login-wait", type=int, default=420)
ap.add_argument("--logout-wait", type=int, default=600)
# The client photographs on a timer and cannot see this press. Holding here for
# a moment after the login means several of its frames are taken BEFORE the
# command is sent, which is the only way a "the character was in the world when
# it was pressed" claim can be checked afterwards -- by two UTC clocks, not by
# a frame somebody named "before".
ap.add_argument("--press-delay", type=int, default=40)
args = ap.parse_args()

stamp("##### stage 1 -- ground, before the client is started")
ground = row()
show("ground (offline)", ground)
if ground["online"]:
    raise SystemExit(f"{NAME} is already online; this stage wants it logged out")
if ground["at_login"] & RENAME_BIT:
    raise SystemExit(
        f"{NAME}'s rename bit is ALREADY set, so a press could not move it. "
        "Clear it (log in and answer the prompt) and run this again -- a step "
        "whose assertion is true before its action proves nothing."
    )
stamp("ground accepted: offline, rename bit clear")

stamp(f"##### stage 2 -- waiting up to {args.login_wait}s for the client to reach the world")
online = wait_for(1, args.login_wait)
if online is None:
    raise SystemExit("the character never came online; nothing was pressed")
show("the world says logged in", online)

stamp(f"holding {args.press_delay}s so the client's timer takes frames before the press")
time.sleep(args.press_delay)

stamp("##### stage 3 -- the ground the press is about, read while ONLINE")
before = row()
show("before the press (online)", before)
if before["at_login"] & RENAME_BIT:
    raise SystemExit("the rename bit went up before the press; refusing to claim it")
if not before["online"]:
    raise SystemExit("the character logged out between the two reads; refusing to press")

stamp("##### stage 4 -- the press, through the app's own Characters tab")
out = tab.rename(NAME)
stamp(f"rename(): done={out.done} {(out.text or out.problem).strip()!r}")

stamp("##### stage 5 -- the row, while the character is STILL online")
elapsed = 0
for wait in (0, 5, 10, 30, 60):
    if wait:
        time.sleep(wait)
    elapsed += wait
    r = row()
    show(f"+{elapsed:>3}s after the press", r)
    if not r["online"]:
        stamp("   (the character is no longer online at this read)")

stamp(f"##### stage 6 -- waiting up to {args.logout_wait}s for the logout")
after = wait_for(0, args.logout_wait)
if after is None:
    stamp("the character was still online when the wait ran out")
    after = row()
show("after the logout", after)

stamp("##### verdict")
moved_at_press = bool(before["at_login"] ^ r["at_login"]) if r["online"] else None
stamp(
    f"at_login: ground {ground['at_login']} -> before-press {before['at_login']} -> "
    f"last-online-read {r['at_login']} (online={r['online']}) -> after-logout {after['at_login']}"
)
if r["online"] and (r["at_login"] & RENAME_BIT):
    stamp("the row moved AT THE PRESS, while the character was still in the world")
elif after["at_login"] & RENAME_BIT:
    stamp("the row moved AT LOGOUT, not at the press -- 8.4a's finding holding here")
else:
    stamp("the rename bit is NOT set even after the logout: the press did not take")
