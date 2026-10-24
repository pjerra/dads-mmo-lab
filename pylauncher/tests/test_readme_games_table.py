"""The front page's games table is generated from the catalog (readme-w1).

`tools/readme_games_table.py` prints the Supported-games table from
`pylauncher/yulon/catalog/catalog.json`, so the README cannot drift from the
catalog. These pins hold the script to that contract: every entry gets a
row in its own words, and no upstream URL ever leaks in (DFO's lives under
a profane path, so the table names software only, never links it).
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
SCRIPT = REPO / "tools" / "readme_games_table.py"
CATALOG = REPO / "pylauncher" / "yulon" / "catalog" / "catalog.json"


def _output() -> str:
    done = subprocess.run(
        [sys.executable, str(SCRIPT)],
        capture_output=True,
        text=True,
        check=True,
    )
    assert done.stderr == "", done.stderr
    return done.stdout


def _entries() -> list[dict]:
    catalog = json.loads(CATALOG.read_text(encoding="utf-8"))
    return list(catalog["games"]) + list(catalog["single"])


def _row_for(output: str, name: str) -> str:
    (line,) = [ln for ln in output.splitlines() if name in ln and ln.startswith("|")]
    return line


def test_every_catalog_entry_has_a_row_in_its_own_words() -> None:
    output = _output()
    entries = _entries()
    for entry in entries:
        line = _row_for(output, entry["name"])
        assert f"| {entry['status']} |" in line
        assert entry["emulator"]["name"] in line
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        platforms = entry["install"]["platforms"]
        assert cells[4] == ("yes" if "linux" in platforms else "no")
        assert cells[5] == ("yes" if "windows" in platforms else "no")
        assert cells[6] == ("yes" if "macos" in platforms else "no")


def test_client_versions_match_the_catalog() -> None:
    output = _output()
    catalog = json.loads(CATALOG.read_text(encoding="utf-8"))
    by_id = {e["id"]: e for e in catalog["games"] + catalog["single"]}
    for entry in _entries():
        line = _row_for(output, entry["name"])
        client = entry.get("client") or {}
        if client.get("version"):
            assert client["version"] in line
            build = client.get("build") or client.get("required_build")
            assert str(build) in line
        else:
            # No client block (DDON, DFO): the fallback is quoted from the
            # entry's own description, so it stays checkable there.
            fallback = {"ddon": "03.04.003", "dfo": "US 2.38.3.25"}[entry["id"]]
            assert fallback in line
            assert fallback in by_id[entry["id"]]["description"]


def test_single_entries_are_marked_other_games() -> None:
    output = _output()
    catalog = json.loads(CATALOG.read_text(encoding="utf-8"))
    for entry in catalog["single"]:
        assert "(Other games)" in _row_for(output, entry["name"])
    for entry in catalog["games"]:
        assert "(Other games)" not in _row_for(output, entry["name"])


def test_no_upstream_url_can_leak_through_the_table() -> None:
    output = _output()
    assert "http" not in output
    catalog = CATALOG.read_text(encoding="utf-8")
    assert "gitgud.io" in catalog  # the ground: DFO's URL really is in the catalog
    assert "gitgud.io" not in output
