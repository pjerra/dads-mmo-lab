"""8.6 ground reading: what the WotLK worldserver says to a bridge command TODAY.

Run BEFORE anything is deployed. Its whole job is to record the starting state,
because a gate step whose assertion is already true before its action proves
nothing -- and the step this gate is built around (deploy the bridge scripts,
then ask again) is exactly the shape that trap takes.

Standard library only, and deliberately NOT the app's own `yulon.soap`: the
ground must be readable before the app's code is on the box.
"""

from __future__ import annotations

import base64
import datetime
import http.client
import json
import pathlib
import re
import sys

CRED = pathlib.Path.home() / ".local/share/yulon/credentials/wow-wotlk-243c46e3.json"
HOST = "127.0.0.1"
PORT = 7878


def esc(text: str) -> str:
    return text.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def ask(user: str, pw: str, command: str, timeout: float = 20.0):
    body = (
        '<?xml version="1.0" encoding="utf-8"?>'
        '<SOAP-ENV:Envelope xmlns:SOAP-ENV="http://schemas.xmlsoap.org/soap/envelope/"'
        ' xmlns:ns1="urn:AC">'
        "<SOAP-ENV:Body><ns1:executeCommand><command>"
        + esc(command)
        + "</command></ns1:executeCommand></SOAP-ENV:Body></SOAP-ENV:Envelope>"
    )
    conn = http.client.HTTPConnection(HOST, PORT, timeout=timeout)
    auth = base64.b64encode(f"{user}:{pw}".encode()).decode()
    conn.request(
        "POST",
        "/",
        body=body.encode(),
        headers={"Content-Type": "application/xml", "Authorization": "Basic " + auth},
    )
    resp = conn.getresponse()
    raw = resp.read().decode("utf-8", "replace")
    conn.close()
    fault = re.search(r"<faultstring>(.*?)</faultstring>", raw, re.S)
    result = re.search(r"<result>(.*?)</result>", raw, re.S)
    return resp.status, (fault.group(1) if fault else None), (result.group(1) if result else None), raw


def main() -> int:
    cred = json.loads(CRED.read_text())
    print("credential keys:", sorted(cred))
    user = cred.get("username") or cred.get("user") or cred.get("account") or ""
    pw = cred.get("password") or cred.get("secret") or ""
    print("account:", user)
    print("UTC:", datetime.datetime.now(datetime.timezone.utc).isoformat())
    for command in sys.argv[1:]:
        status, fault, result, raw = ask(user, pw, command)
        print("=" * 72)
        print("COMMAND :", command)
        print("STATUS  :", status)
        print("FAULT   :", repr(fault))
        print("RESULT  :", repr(result))
        if fault is None and result is None:
            print("RAW     :", raw[:600])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
