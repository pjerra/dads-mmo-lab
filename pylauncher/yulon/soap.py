"""One `executeCommand` envelope over the standard library (Phase 8.2a).

The wire and nothing else. This module does not build command text, does not
hold a lock, does not know a game, and never lets a password reach a log, a
`repr` or an exception.

## Where the envelope comes from

Not derived: ported from prior art that runs. The Rust launcher's client
(`rust-main:crates/dml-wow/src/soap.rs`, itself a port of `cli/src/20-soap.sh`)
posts exactly this shape to `http://127.0.0.1:7878/`, and the worldserver's own
symbol is `int ns1__executeCommand(soap*, char* command, char** result)`
(`pyplan/phase8-reads/cmangos.md:122` names the same symbol on the CMaNGOS
family). The namespace is `urn:AC`.

**Fault before result.** The prior art checks `<faultstring>` first and
`<result>` second, and the order is load-bearing rather than incidental: the
server sends the same print buffer in both directions (`ACSoap.cpp:133-140`), so
a body carrying a fault also carries text a result-first reader would report as
success.

## Two places this deliberately differs from the prior art, and why

* **A 403 is its own outcome.** The Rust client knows only 401, so a
  gmlevel-too-low answer falls through to "the command failed" — and the two
  need different repairs: one rewrites a password, the other raises a level.
  AzerothCore returns 403 when the account is below `SEC_ADMINISTRATOR`
  (`ACSoap.cpp:103-107`), so that is answered as `forbidden`.
* **A body that is neither fault nor result is `unreadable`, not a failure.**
  The prior art reports the whole body as a fault, which for an HTML error page
  from something in the way reads as "your command failed: <html>". This phase
  types every answer as yes, no, or could-not-ask, and a reply nobody can parse
  is the third of those.

What is kept from the prior art without change: the escape order (`&` before
`<` and `>`, so the entities' own ampersand is not re-escaped) and extracting
the reply text **raw**, without entity decoding — the text is evidence, and a
decoder is another thing that can be wrong between the server and the capture.
"""

from __future__ import annotations

import base64
import re
import socket
import urllib.error
import urllib.request
from dataclasses import dataclass, field
from typing import Literal

from yulon.log import get_logger

logger = get_logger(__name__)

Outcome = Literal[
    "answered", "refused", "unauthorised", "forbidden", "unreadable", "unreachable", "timeout"
]

DEFAULT_TIMEOUT = 20.0
"""Seconds for one round trip, and it is a bound rather than a guess.

The command runs on the world thread and this call blocks until it finishes
(`ACSoap.cpp:120-130`), so a busy world can be slow — but the failure this
protects against is not slowness. A listener bound to the container's own
loopback leaves Docker accepting the connection and relaying nothing
(`pyplan/gates/8-spikes/published-port-vs-container-loopback/`), so a client
without a timeout waits forever against a server that is running perfectly.
"""


class SoapError(RuntimeError):
    """Something went wrong on the wire. Never carries a password."""


@dataclass(frozen=True)
class Endpoint:
    """Where the listener is and who we are to it."""

    host: str
    port: int
    account: str
    password: str = field(repr=False)
    """Kept out of the repr, like `apply.DockerSql.root_password`.

    A frozen dataclass reprs every field by default, and this object is handed
    to worker threads and closed over by the tab's seams: a pytest assertion
    diff, a logged object or a traceback frame dump would each print it.
    """

    @property
    def url(self) -> str:
        return f"http://{self.host}:{self.port}/"


@dataclass(frozen=True)
class Reply:
    """What came back, typed. `text` is raw — see the module docstring."""

    outcome: Outcome
    text: str = ""
    http_status: int | None = None

    @property
    def answered(self) -> bool:
        """True only for a command the server ran and reported as succeeding."""
        return self.outcome == "answered"


_ENVELOPE = (
    '<?xml version="1.0" encoding="utf-8"?>\n'
    '<SOAP-ENV:Envelope xmlns:SOAP-ENV="http://schemas.xmlsoap.org/soap/envelope/"'
    ' xmlns:ns1="urn:AC">\n'
    "  <SOAP-ENV:Body>\n"
    "    <ns1:executeCommand><command>{command}</command></ns1:executeCommand>\n"
    "  </SOAP-ENV:Body>\n"
    "</SOAP-ENV:Envelope>\n"
)


def envelope(command: str) -> str:
    """The request body for `command`, escaped."""
    return _ENVELOPE.format(command=escape(command))


def escape(text: str) -> str:
    """XML-escape for the outbound `<command>`; `&` first, then `<` and `>`."""
    return text.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def execute(endpoint: Endpoint, command: str, *, timeout: float = DEFAULT_TIMEOUT) -> Reply:
    """POST one command and classify the answer. Never raises.

    Every failure is an outcome rather than an exception, because the caller is
    a seam that has to say something to a person either way, and because the
    interesting failures here — a socket that accepts and stays silent, an
    account whose level is too low — are answers about the server rather than
    faults in this code.
    """
    request = urllib.request.Request(  # noqa: S310 - the scheme is this module's own
        endpoint.url,
        data=envelope(command).encode("utf-8"),
        headers={
            "Content-Type": "text/xml; charset=utf-8",
            "Authorization": _basic(endpoint),
            "SOAPAction": '""',
        },
        method="POST",
    )
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:  # noqa: S310
            return _classify(response.status, response.read().decode("utf-8", errors="replace"))
    except urllib.error.HTTPError as exc:
        # 401, 403 and the fault-bearing 500 all arrive here: `urlopen` raises
        # for any status it does not consider success, and the body is the part
        # that matters.
        body = exc.read().decode("utf-8", errors="replace") if exc.fp is not None else ""
        return _classify(exc.code, body)
    except TimeoutError:
        return Reply("timeout", f"no answer from {endpoint.url} within {timeout:g}s")
    except urllib.error.URLError as exc:
        # `URLError.reason` is the socket error; a timeout can arrive here too,
        # wrapped, depending on where in the exchange it happened.
        if isinstance(exc.reason, TimeoutError | socket.timeout):
            return Reply("timeout", f"no answer from {endpoint.url} within {timeout:g}s")
        return Reply("unreachable", f"could not reach {endpoint.url}: {exc.reason}")
    except OSError as exc:
        return Reply("unreachable", f"could not reach {endpoint.url}: {exc}")


def _basic(endpoint: Endpoint) -> str:
    """The Basic header. Built here so no caller has to hold the pair as text."""
    pair = f"{endpoint.account}:{endpoint.password}".encode()
    return "Basic " + base64.b64encode(pair).decode("ascii")


_FAULT = re.compile(r"<faultstring>(.*?)</faultstring>", re.DOTALL)
_RESULT = re.compile(r"<result>(.*?)</result>", re.DOTALL)


def _classify(status: int, body: str) -> Reply:
    """Turn one HTTP answer into one outcome. Fault is checked before result."""
    if status == 401:
        return Reply("unauthorised", "the server did not accept this account and password", status)
    if status == 403:
        return Reply(
            "forbidden",
            "the account exists but its GM level is below administrator, which SOAP requires",
            status,
        )
    fault = _FAULT.search(body)
    if fault:
        return Reply("refused", fault.group(1), status)
    result = _RESULT.search(body)
    if result:
        return Reply("answered", result.group(1), status)
    logger.warning(f"a SOAP reply carried neither a result nor a fault (HTTP {status})")
    return Reply("unreadable", body.strip()[:400], status)
