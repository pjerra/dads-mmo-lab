"""Is it the LOGIN that corrupts the hash, or was it something about that account?

8.3d's first reading compared two different accounts, which the adversarial
review would not accept -- and it was right: identity, row state and command
order were all confounded with it.

This is the controlled version. Four accounts created identically in the same
second. Every one gets the SAME password change TWICE, from the same script, in
the same order. Between the two rounds, only the treatment group logs in.

    round 1   all four        -> expect: every hash correct
    (the client logs TREAT_A and TREAT_B in, and nothing else changes)
    round 2   all four        -> expect: the two that logged in are corrupted
                                 and the two that did not are still correct

Usage:  python tcontrolled.py make      create them and do round 1
        python tcontrolled.py round2    do round 2 and report
"""

import hashlib
import sys
from pathlib import Path

sys.path.insert(0, "/home/pk/gate83b/pylauncher")
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui import controller_view as v  # noqa: E402
from yulon.ui.controller_view import ControllerServices  # noqa: E402

E = load_catalog().get("wow-tortoise")
D = Path.home() / "tortoise-server"
sql = v._sql_for(E, v._db_password(E, D), wsl_distro=None)
svc = ControllerServices.for_entry(E, D)
Q = chr(39)

CONTROL = ["CTRLONE", "CTRLTWO"]
TREATED = ["TREATONE", "TREATTWO"]
ALL = CONTROL + TREATED
FIRST = "r0und-one11"
SECOND = "r0und-two22"


def hash_of(name: str, password: str) -> str:
    return hashlib.sha1(f"{name}:{password.upper()}".encode()).hexdigest().upper()


def stored(name: str) -> str:
    return sql.query(
        "auth", f"SELECT sha_pass_hash FROM tw_logon.account WHERE username = {Q}{name}{Q};"
    ).strip()


def report(name: str, password: str) -> None:
    got = stored(name)
    with_name = hash_of(name, password)
    without = hash_of("", password)
    verdict = "CORRECT" if got == with_name else ("EMPTY-NAME" if got == without else "unknown")
    login = sql.query(
        "auth", f"SELECT last_login FROM tw_logon.account WHERE username = {Q}{name}{Q};"
    ).strip()
    print(f"   {name:9} last_login={login:20} {verdict}")


def main() -> None:
    if sys.argv[1] == "make":
        for name in ALL:
            made = svc.create_account(name, "st@rt-p@ss00", 0)
            print(f"{name}: {'created' if made.created else 'already there'}")
        print("round 1 -- the same change, all four:")
        for name in ALL:
            svc.accounts.set_password(name, FIRST)
            report(name, FIRST)
        print(f"\nNow log in as {TREATED[0]} and {TREATED[1]} with {FIRST!r}, then run round2.")
    else:
        print("round 2 -- the same change again, all four:")
        for name in ALL:
            svc.accounts.set_password(name, SECOND)
            report(name, SECOND)


if __name__ == "__main__":
    main()
