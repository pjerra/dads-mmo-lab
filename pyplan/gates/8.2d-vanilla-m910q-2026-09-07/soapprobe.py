"""What does THIS tree's SOAP listener answer, byte for byte?

The CMaNGOS source study stopped at exactly this question and said so: the
gSOAP-generated envelope lives in `soapC.cpp`, which is generated code it did
not read, and "a caller-side decision about how to distinguish command failed
from transport failed needs either that file read or a live probe". This is the
live probe.

Three envelopes are sent so the answer is attributable: AzerothCore's namespace
(`urn:AC`), CMaNGOS's own (`urn:MaNGOS`), and one with no namespace at all.
Each is sent with the right credentials and with wrong ones, because the point
is to learn which failures are distinguishable from the outside.

Usage:  python soapprobe.py <account> <password>
"""

from __future__ import annotations

import base64
import http.client
import sys

HOST, PORT = "127.0.0.1", 7878
COMMAND = "server info"

ENVELOPE = (
    '<?xml version="1.0" encoding="UTF-8"?>\n'
    '<SOAP-ENV:Envelope xmlns:SOAP-ENV="http://schemas.xmlsoap.org/soap/envelope/"'
    "{ns}>\n"
    "<SOAP-ENV:Body>\n"
    "<{prefix}executeCommand>\n"
    "<command>{command}</command>\n"
    "</{prefix}executeCommand>\n"
    "</SOAP-ENV:Body>\n"
    "</SOAP-ENV:Envelope>\n"
)

SHAPES = {
    "urn:AC (what the app sends today)": (' xmlns:ns1="urn:AC"', "ns1:"),
    "urn:MaNGOS": (' xmlns:ns1="urn:MaNGOS"', "ns1:"),
    "no namespace": ("", ""),
}


def ask(account: str, password: str, ns: str, prefix: str) -> tuple[int, str]:
    body = ENVELOPE.format(ns=ns, prefix=prefix, command=COMMAND)
    token = base64.b64encode(f"{account}:{password}".encode()).decode()
    conn = http.client.HTTPConnection(HOST, PORT, timeout=20)
    try:
        conn.request(
            "POST",
            "/",
            body=body.encode(),
            headers={
                "Content-Type": "text/xml; charset=utf-8",
                "SOAPAction": '""',
                "Authorization": f"Basic {token}",
            },
        )
        answer = conn.getresponse()
        return answer.status, answer.read().decode("utf-8", "replace")
    finally:
        conn.close()


def main() -> None:
    account, password = sys.argv[1], sys.argv[2]
    for label, (ns, prefix) in SHAPES.items():
        for who, pw in (("the real password", password), ("a wrong password", "definitely-wrong")):
            print(f"\n=== {label} / {who} ===")
            try:
                status, text = ask(account, pw, ns, prefix)
            except Exception as exc:  # noqa: BLE001 - every failure is a reading here
                print(f"    {type(exc).__name__}: {exc}")
                continue
            print(f"    HTTP {status}")
            for line in text.splitlines()[:12]:
                print(f"    {line}")


if __name__ == "__main__":
    main()
