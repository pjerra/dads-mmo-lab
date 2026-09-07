"""What does the app's OWN seam make of this server's answers?

The probe before this one read the wire. This one reads what `soap.execute()`
and `channel.SoapChannel` turn that wire into, because the setup's decision is
made on the Answer and not on the bytes.

Usage:  python answerprobe.py <account> <good-password>
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, "/home/pk/gate82c/pylauncher")

from yulon import channel as channel_module  # noqa: E402
from yulon import commands, docker, soap  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402

ENTRY = load_catalog().get("wow-tbc")
SPEC = ENTRY.container_spec()
OPS = ENTRY.operations


def main() -> None:
    account, good = sys.argv[1], sys.argv[2]
    for label, password in (("the right password", good), ("a wrong password", "definitely-wrong")):
        endpoint = soap.Endpoint(
            host="127.0.0.1",
            port=OPS.port,
            account=account,
            password=password,
            namespace=OPS.namespace,
        )
        reply = soap.execute(endpoint, commands.SERVER_INFO)
        print(f"\n=== {label} ===")
        print(f"  soap.Reply.outcome  : {reply.outcome!r}")
        print(f"  soap.Reply.text     : {reply.text.splitlines()[:2]}")
        live = channel_module.SoapChannel(
            endpoint=endpoint,
            state_of=lambda: docker.container_state(SPEC.world),
        )
        answer = live.send(commands.SERVER_INFO)
        print(f"  Answer.outcome      : {answer.outcome!r}")
        print(f"  Answer.denied       : {answer.denied}")
        print(f"  Answer.indeterminate: {answer.indeterminate}")
        print(f"  Answer.text         : {answer.text.splitlines()[:2]}")


if __name__ == "__main__":
    main()
