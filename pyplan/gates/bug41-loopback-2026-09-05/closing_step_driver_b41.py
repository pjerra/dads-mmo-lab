"""bug-checklist §41 gate, half two: the closing realm step, run against the live install.

Run on m910q against the finished CMaNGOS Vanilla install at /home/pk/vanilla-75b.

**What this is NOT.** It is not a second Install press. A second press on this box
was attempted first and REFUSED by the engine's own preflight, for disk and not
for anything to do with §41:

    [refuse] free space on Docker's disk and the server folder: 18 GB free, and
    the install needs 40 GB (the server folder and Docker's disk share one drive,
    so both needs add up)

(`press1.log` in this directory, m910q 2026-09-05 22:20:32 box-local). Freeing
22 GB on that box would have meant deleting other lanes' and the owner's
material, so it was not done.

**What this IS.** The real `StagedInstaller` for `wow-vanilla`, built by the same
`install_wiring.installer_for_app()` the CLI builds, handed the real
`InstallState` read off the real state file and the real `Secrets` resolved from
the real `.db_password`, with its closing realm step called against the live
`vanilla-db` container — the same method, the same seams and the same database
the press would reach one call further in. The rows are read back through
`docker exec ... mariadb`, a route the engine never touches.

Written 2026-09-05 for lane b41.
"""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

CHECKOUT = Path(__file__).resolve().parents[3] / "pylauncher"
sys.path.insert(0, str(CHECKOUT))

from yulon import networking, platform  # noqa: E402
from yulon.catalog import native  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.install_wiring import installer_for_app  # noqa: E402

SERVER_DIR = Path("/home/pk/vanilla-75b")
CLIENT_DIR = Path("/home/pk/clients/WoW-Client-1.12.1")
GAME = "wow-vanilla"

PASSES = 0
FAILS = 0


def say(text: str = "") -> None:
    print(text, flush=True)


def check(label: str, ok: bool, detail: str = "") -> None:
    global PASSES, FAILS
    if ok:
        PASSES += 1
        say(f"[OK]   {label}" + (f" -- {detail}" if detail else ""))
    else:
        FAILS += 1
        say(f"[FAIL] {label}" + (f" -- {detail}" if detail else ""))


def db_read(statement: str) -> str:
    password = (SERVER_DIR / ".db_password").read_text(encoding="utf-8").strip()
    done = subprocess.run(
        ["docker", "exec", "-e", f"MYSQL_PWD={password}", "vanilla-db",
         "mariadb", "-uroot", "-N", "-B", "-e", statement],
        capture_output=True, text=True,
    )
    return done.stdout.strip()


def run_closing_step(engine, entry) -> list[str]:
    """Call the engine's own `_advertise_realm()` with a real context."""
    state = native.read_state(SERVER_DIR, valid=engine.stage_names())
    ctx = native.StageContext(
        server_dir=SERVER_DIR,
        client_dir=CLIENT_DIR,
        state=state,
        cancel=None,
        secrets=engine.resolve_secrets(SERVER_DIR),
    )
    return list(engine._advertise_realm(ctx))


def main() -> int:
    entry = load_catalog().get(GAME)
    engine = installer_for_app(entry)
    say(f"engine class = {type(engine).__name__}   family = {engine.family}")
    say(f"stage names  = {engine.stage_names()}")
    say("_advertise_realm is defined on "
        f"{type(engine)._advertise_realm.__qualname__.split('.')[0]}, "
        "which is the shared spine both families inherit")
    say(f"platform.detect_lan_ip() = {platform.detect_lan_ip()!r}")
    say("")

    say("=" * 78)
    say("PART A -- the loopback the owner chose through the widgets is left alone")
    intent = networking.read_network_intent(SERVER_DIR)
    say(f"  {networking.INTENT_FILE} says {intent}")
    before = db_read("SELECT address FROM realmd.realmlist WHERE id=1;")
    say(f"  realm row before = {before!r}")
    check("the recorded intent is loopback",
          intent is not None and intent.mode == "loopback", str(intent))
    check("the row is on the loopback going in", before == "127.0.0.1", before)

    said = run_closing_step(engine, entry)
    say("  ---- what the closing step said ----")
    for line in said:
        say(f"  | {line}")
    say("  ------------------------------------")
    after = db_read("SELECT address FROM realmd.realmlist WHERE id=1;")
    say(f"  realm row after = {after!r}")
    check("the row is STILL the loopback", after == "127.0.0.1", after)
    check("exactly one line was said", len(said) == 1, str(len(said)))
    check("it says why it was left alone",
          bool(said) and "left exactly as it is" in said[0])
    check("it says nothing else can reach the server",
          bool(said) and "no other machine can reach this server" in said[0])
    check("it names the file the choice is in",
          bool(said) and networking.INTENT_FILE in said[0])
    check("it names the way back",
          bool(said) and "Networking tab" in said[0] and "Apply" in said[0])

    say("")
    say("=" * 78)
    say("PART B -- a server whose loopback was NEVER chosen is still rewritten")
    (SERVER_DIR / networking.INTENT_FILE).unlink()
    say(f"  removed {SERVER_DIR / networking.INTENT_FILE}")
    check("there is no recorded intent now",
          networking.read_network_intent(SERVER_DIR) is None)
    before2 = db_read("SELECT address FROM realmd.realmlist WHERE id=1;")
    say(f"  realm row before = {before2!r}")

    said2 = run_closing_step(engine, entry)
    say("  ---- what the closing step said ----")
    for line in said2:
        say(f"  | {line}")
    say("  ------------------------------------")
    after2 = db_read("SELECT address FROM realmd.realmlist WHERE id=1;")
    say(f"  realm row after = {after2!r}")
    check("the row is a reachable address again",
          networking.advertisable(after2) is not None, after2)
    check("it is this machine's LAN address", after2 == platform.detect_lan_ip(), after2)
    check("the line names that address",
          bool(said2) and after2 in said2[0], said2[0] if said2 else "")

    say("")
    say("=" * 78)
    say(f"{PASSES} passed, {FAILS} failed")
    say(f"FINAL realm row = {db_read('SELECT id,name,address,port FROM realmd.realmlist;')!r}")
    say(f"FINAL {networking.INTENT_FILE} present = "
        f"{(SERVER_DIR / networking.INTENT_FILE).exists()}")
    return 1 if FAILS else 0


if __name__ == "__main__":
    raise SystemExit(main())
