"""Tests for `yulon.soap` — one command envelope over the standard library (8.2a).

Driven against a real `http.server` on 127.0.0.1 rather than a mock: this module
exists to speak HTTP to something, and the failures worth catching are wire
failures — a 401, a fault body, a socket that accepts and says nothing. A mock
would confirm the code calls the functions it calls.

The envelope shape is `ns1__executeCommand(soap*, char* command, char** result)`
(`cmangos.md:122` names the same symbol on that family) in namespace `urn:AC`,
which is what the Hypeer launcher's working client posts to
`http://127.0.0.1:7878/` (`hypeer.md:48`, `soap.rs:127-132`). That is prior art
that runs, not a derivation, and 8.2a's gate is where this client's own
round-trip confirms it against a real worldserver.
"""

from __future__ import annotations

import http.server
import logging
import threading
from collections.abc import Iterator
from contextlib import contextmanager

import pytest

from yulon import soap

ACCOUNT = "YULON_ab12"
PASSWORD = "a-password-that-must-never-be-logged"

_OK = """<?xml version="1.0" encoding="UTF-8"?>
<SOAP-ENV:Envelope xmlns:SOAP-ENV="http://schemas.xmlsoap.org/soap/envelope/"
 xmlns:ns1="urn:AC">
 <SOAP-ENV:Body>
  <ns1:executeCommandResponse><result>{body}</result></ns1:executeCommandResponse>
 </SOAP-ENV:Body>
</SOAP-ENV:Envelope>
"""

_FAULT = """<?xml version="1.0" encoding="UTF-8"?>
<SOAP-ENV:Envelope xmlns:SOAP-ENV="http://schemas.xmlsoap.org/soap/envelope/">
 <SOAP-ENV:Body>
  <SOAP-ENV:Fault>
   <faultcode>SOAP-ENV:Client</faultcode>
   <faultstring>{body}</faultstring>
   <detail>{body}</detail>
  </SOAP-ENV:Fault>
 </SOAP-ENV:Body>
</SOAP-ENV:Envelope>
"""


@contextmanager
def _server(handler_for: object) -> Iterator[soap.Endpoint]:
    """A one-request-at-a-time HTTP server on a port the OS picks."""

    class _Handler(http.server.BaseHTTPRequestHandler):
        def do_POST(self) -> None:  # noqa: N802 - the stdlib spells it this way
            length = int(self.headers.get("Content-Length", "0"))
            body = self.rfile.read(length).decode("utf-8")
            handler_for(self, body)  # type: ignore[operator]

        def log_message(self, *_args: object) -> None:
            """Silence the stdlib's stderr logging during tests."""

    httpd = http.server.HTTPServer(("127.0.0.1", 0), _Handler)
    thread = threading.Thread(target=httpd.serve_forever, daemon=True)
    thread.start()
    try:
        yield soap.Endpoint(
            host="127.0.0.1", port=httpd.server_address[1], account=ACCOUNT, password=PASSWORD
        )
    finally:
        httpd.shutdown()
        httpd.server_close()


def _respond(handler: object, status: int, body: str) -> None:
    handler.send_response(status)  # type: ignore[attr-defined]
    handler.send_header("Content-Type", "text/xml; charset=utf-8")  # type: ignore[attr-defined]
    handler.send_header("Content-Length", str(len(body.encode())))  # type: ignore[attr-defined]
    handler.end_headers()  # type: ignore[attr-defined]
    handler.wfile.write(body.encode())  # type: ignore[attr-defined]


# -- what the server says ---------------------------------------------------


def test_a_command_that_succeeds_comes_back_as_its_print_buffer() -> None:
    """The accumulated console output IS the result string (`ACSoap.cpp:133-137`)."""
    seen: list[str] = []

    def handle(handler: object, body: str) -> None:
        seen.append(body)
        _respond(handler, 200, _OK.format(body="Account created."))

    with _server(handle) as endpoint:
        reply = soap.execute(endpoint, "account create bob hunter2")

    assert reply.outcome == "answered"
    assert reply.text == "Account created."
    assert "executeCommand" in seen[0]
    assert "urn:AC" in seen[0]
    assert "account create bob hunter2" in seen[0]


def test_a_command_the_server_refused_comes_back_as_a_fault_not_as_success() -> None:
    """Failure is a SOAP sender fault carrying the same buffer (`ACSoap.cpp:139-140`)."""

    def handle(handler: object, body: str) -> None:
        _respond(handler, 500, _FAULT.format(body="Account cannot be created."))

    with _server(handle) as endpoint:
        reply = soap.execute(endpoint, "account create bob hunter2")

    assert reply.outcome == "refused"
    assert reply.text == "Account cannot be created."


def test_wrong_credentials_are_told_apart_from_a_command_that_failed() -> None:
    """401 is about US, not about the command — and the repair path keys off it."""

    def handle(handler: object, body: str) -> None:
        _respond(handler, 401, "")

    with _server(handle) as endpoint:
        reply = soap.execute(endpoint, "server info")

    assert reply.outcome == "unauthorised"


def test_an_account_below_administrator_is_its_own_outcome() -> None:
    """403 means the account exists and its level is too low (`ACSoap.cpp:103-107`).

    Distinct from 401 because the repair is different: one rewrites a password,
    the other raises a GM level.
    """

    def handle(handler: object, body: str) -> None:
        _respond(handler, 403, "")

    with _server(handle) as endpoint:
        reply = soap.execute(endpoint, "server info")

    assert reply.outcome == "forbidden"


def test_a_reply_that_is_not_xml_is_reported_rather_than_parsed_optimistically() -> None:
    def handle(handler: object, body: str) -> None:
        _respond(handler, 200, "<html>a proxy got in the way</html>")

    with _server(handle) as endpoint:
        reply = soap.execute(endpoint, "server info")

    assert reply.outcome == "unreadable"


# -- what the wire does -----------------------------------------------------


def test_nothing_listening_is_could_not_ask_rather_than_a_failed_command() -> None:
    """The three outcomes the spike measured: refused, silent, answering."""
    with _server(lambda *_: None) as endpoint:
        dead = soap.Endpoint(
            host=endpoint.host, port=endpoint.port, account=ACCOUNT, password=PASSWORD
        )
    # the server is closed now, so this port refuses

    reply = soap.execute(dead, "server info")

    assert reply.outcome == "unreachable"
    assert reply.text


def test_a_socket_that_accepts_and_says_nothing_times_out_instead_of_hanging() -> None:
    """The published-port misconfiguration answers exactly like this.

    `gates/8-spikes/published-port-vs-container-loopback/` measured it: a
    listener on the container's own loopback leaves Docker accepting the
    connection and relaying nothing. A client without a timeout waits forever on
    a server that is running perfectly.
    """
    import socket

    listener = socket.socket()
    listener.bind(("127.0.0.1", 0))
    listener.listen(1)
    endpoint = soap.Endpoint(
        host="127.0.0.1", port=listener.getsockname()[1], account=ACCOUNT, password=PASSWORD
    )

    try:
        reply = soap.execute(endpoint, "server info", timeout=0.5)
    finally:
        listener.close()

    assert reply.outcome == "timeout"


# -- the password ------------------------------------------------------------


def test_the_password_never_reaches_a_repr_a_log_or_an_exception(
    caplog: pytest.LogCaptureFixture,
) -> None:
    """One test for all three, because one leak is enough.

    The endpoint is handed to worker threads and closed over by the tab's seams;
    a pytest assertion diff, a logged object or a traceback frame dump would each
    print it. `DockerSql.root_password` closed this same channel on 2026-08-23
    and its sibling was missed, which is why it is asserted rather than reviewed.
    """

    def handle(handler: object, body: str) -> None:
        _respond(handler, 500, _FAULT.format(body="no"))

    with caplog.at_level(logging.DEBUG), _server(handle) as endpoint:
        assert PASSWORD not in repr(endpoint)
        reply = soap.execute(endpoint, "server info")
        assert PASSWORD not in repr(reply)

    assert PASSWORD not in caplog.text
    try:
        raise soap.SoapError(f"something went wrong talking to {endpoint}")
    except soap.SoapError as exc:
        assert PASSWORD not in str(exc)


def test_the_command_is_escaped_so_a_bracket_cannot_change_the_envelope() -> None:
    """Command text arrives from a template and a user; XML is not a place to trust it."""
    seen: list[str] = []

    def handle(handler: object, body: str) -> None:
        seen.append(body)
        _respond(handler, 200, _OK.format(body="ok"))

    with _server(handle) as endpoint:
        soap.execute(endpoint, "account create <bob> & 'friends'")

    assert "<bob>" not in seen[0]
    assert "&lt;bob&gt;" in seen[0]
    assert seen[0].count("<ns1:executeCommand>") == 1
