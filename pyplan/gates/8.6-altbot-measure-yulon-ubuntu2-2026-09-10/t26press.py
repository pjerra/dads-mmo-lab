"""T26 measure -- one SOAP command over the app's own seam, standard library only.

T13's `t13press.py` built the channel by importing the app (`channel_setup.
InstallChannel.live_channel`), which needed a 12 M copy of the worktree and a
676 M venv on the box. Both were deleted at T13's cleanup and this lane is a
READ-ONLY measurement, so the same wire is spoken here directly: the envelope,
the namespace, the Basic header and the fault-before-result classification are
`pylauncher/yulon/soap.py`'s, copied field for field, and the endpoint is read
from the app's OWN credential file --
`~/.local/share/yulon/credentials/wow-wotlk-<install_id>.json`, written by
`channel_setup.save_credential`, the same file `live_channel` reads.

The password is never printed, never echoed, never put in argv: it is read from
that file inside this process and goes only into the Authorization header.

    send <command...>      one command over the seam's channel
    who                    the account and endpoint (no password)
"""

from __future__ import annotations

import base64
import glob
import http.client
import json
import re
import sys

CRED = "/home/pk/.local/share/yulon/credentials/*.json"


def endpoint() -> dict:
    path = sorted(glob.glob(CRED))[0]
    return json.loads(open(path, encoding="utf-8").read())


ENV = (
    '<?xml version="1.0" encoding="utf-8"?>\n'
    '<SOAP-ENV:Envelope xmlns:SOAP-ENV="http://schemas.xmlsoap.org/soap/envelope/"'
    ' xmlns:ns1="{ns}">\n'
    "  <SOAP-ENV:Body>\n"
    "    <ns1:executeCommand><command>{command}</command></ns1:executeCommand>\n"
    "  </SOAP-ENV:Body>\n"
    "</SOAP-ENV:Envelope>\n"
)

_FAULT = re.compile(r"<faultstring>(.*?)</faultstring>", re.DOTALL)
_RESULT = re.compile(r"<result>(.*?)</result>", re.DOTALL)
_ENTITY = re.compile(r"&(#[0-9]+|#[xX][0-9a-fA-F]+|lt|gt|amp|quot|apos);")
_NAMED = {"lt": "<", "gt": ">", "amp": "&", "quot": '"', "apos": "'"}


def unescape(text: str) -> str:
    def one(m: re.Match) -> str:
        body = m.group(1)
        if body.startswith("#"):
            digits = body[1:]
            base = 16 if digits[:1] in ("x", "X") else 10
            return chr(int(digits.lstrip("xX") if base == 16 else digits, base))
        return _NAMED[body]

    return _ENTITY.sub(one, text)


def escape(text: str) -> str:
    return text.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def execute(ep: dict, command: str, timeout: float = 40.0):
    body = ENV.format(command=escape(command), ns=ep.get("namespace", "urn:AC"))
    auth = base64.b64encode(f"{ep['account']}:{ep['password']}".encode()).decode("ascii")
    conn = http.client.HTTPConnection(ep["host"], int(ep["port"]), timeout=timeout)
    try:
        conn.request(
            "POST",
            "/",
            body=body.encode("utf-8"),
            headers={
                "Content-Type": "text/xml; charset=utf-8",
                "Authorization": "Basic " + auth,
                "SOAPAction": '""',
            },
        )
        r = conn.getresponse()
        status, raw = r.status, r.read().decode("utf-8", errors="replace")
    except TimeoutError:
        return ("timeout", "", None)
    except http.client.RemoteDisconnected:
        return ("silent", "", None)
    except (http.client.HTTPException, OSError) as exc:
        return ("unreachable", f"{exc}", None)
    finally:
        conn.close()
    if status == 401:
        return ("unauthorised", "", status)
    if status == 403:
        return ("forbidden", "", status)
    f = _FAULT.search(raw)
    if f:
        return ("refused", unescape(f.group(1)), status)
    res = _RESULT.search(raw)
    if res:
        return ("answered", unescape(res.group(1)), status)
    return ("unreadable", raw.strip()[:600], status)


def main() -> int:
    args = sys.argv[1:]
    if not args:
        print(__doc__)
        return 2
    ep = endpoint()
    if args[0] == "who":
        print(f"account={ep['account']} host={ep['host']} port={ep['port']} ns={ep.get('namespace')}")
        return 0
    if args[0] == "send":
        command = " ".join(args[1:])
        print(f"COMMAND: {command}")
        outcome, text, status = execute(ep, command)
        print(f"outcome={outcome} http={status}")
        print(f"text={text!r}")
        return 0
    print(f"unknown: {args[0]}")
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
