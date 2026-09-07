import socket, base64, sys
CRED = None
import json, os, glob
# find the saved credential
sys.path.insert(0, "/home/pk/gate83b/pylauncher")
from pathlib import Path
from yulon import channel_setup
from yulon.catalog import composegen
from yulon.catalog.catalog import load_catalog
E = load_catalog().get("wow-tbc")
c = channel_setup.load_credential(E.id, composegen.install_id(Path.home()/"tbc-7.4c"))

def ask(cmd):
    body = ("<?xml version=\"1.0\" encoding=\"utf-8\"?>"
            "<SOAP-ENV:Envelope xmlns:SOAP-ENV=\"http://schemas.xmlsoap.org/soap/envelope/\" "
            "xmlns:ns1=\"urn:MaNGOS\">"
            f"<SOAP-ENV:Body><ns1:executeCommand><command>{cmd}</command></ns1:executeCommand>"
            "</SOAP-ENV:Body></SOAP-ENV:Envelope>").encode()
    auth = base64.b64encode(f"{c.account}:{c.password}".encode()).decode()
    head = (f"POST / HTTP/1.1\r\nHost: 127.0.0.1:{c.port}\r\nAuthorization: Basic {auth}\r\n"
            f"Content-Type: text/xml; charset=utf-8\r\nContent-Length: {len(body)}\r\n"
            "Connection: close\r\n\r\n").encode()
    s = socket.create_connection((c.host, c.port), 10)
    s.sendall(head + body)
    out = b""
    while True:
        chunk = s.recv(4096)
        if not chunk: break
        out += chunk
    s.close()
    print(f"=== {cmd!r}: {len(out)} bytes")
    print(out.decode("utf-8", "replace")[:400].replace("\r\n", "\n"))
    print()

for cmd in ("server info", "account set", "blargh", "account characters GATE83B"):
    ask(cmd)
