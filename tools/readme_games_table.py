#!/usr/bin/env python3
"""Print the front-page README's Supported-games table from the catalog.

The table cannot drift from ``pylauncher/yulon/catalog/catalog.json``: every
cell comes from one entry's ``name``, ``status``, ``install.platforms``,
``emulator.name`` and client version. Stdlib only.

Upstream repository URLs are never printed — DFO's lives under a profane
path, so the table names server software only, never links it.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
CATALOG = REPO / "pylauncher" / "yulon" / "catalog" / "catalog.json"

HEADER = "| Game | Status | Server software | Your client | Linux | Windows | macOS |"
DIVIDER = "|---|---|---|---|---|---|---|"

# Entries without a client block (DDON, DFO) name the client in words in
# their catalog description; the exact strings below are quoted from there.
CLIENT_FALLBACK = {
    "ddon": "03.04.003",
    "dfo": "US 2.38.3.25",
}

OTHER_GAMES = " (Other games)"


def client_version(entry: dict) -> str:
    client = entry.get("client") or {}
    version = client.get("version")
    if version:
        build = client.get("build") or client.get("required_build")
        return f"{version} ({build})" if build else str(version)
    return CLIENT_FALLBACK[entry["id"]]


def yes_no(platforms: list, name: str) -> str:
    return "yes" if name in platforms else "no"


def row(entry: dict, other: bool = False) -> str:
    name = entry["name"] + (OTHER_GAMES if other else "")
    platforms = entry["install"]["platforms"]
    return (
        f"| {name} | {entry['status']} | {entry['emulator']['name']} "
        f"| {client_version(entry)} | {yes_no(platforms, 'linux')} "
        f"| {yes_no(platforms, 'windows')} | {yes_no(platforms, 'macos')} |"
    )


def table(catalog: dict) -> str:
    lines = [HEADER, DIVIDER]
    for entry in catalog.get("games", []):
        lines.append(row(entry))
    for entry in catalog.get("single", []):
        lines.append(row(entry, other=True))
    return "\n".join(lines) + "\n"


def main() -> None:
    catalog = json.loads(CATALOG.read_text(encoding="utf-8"))
    sys.stdout.write(table(catalog))


if __name__ == "__main__":
    main()
