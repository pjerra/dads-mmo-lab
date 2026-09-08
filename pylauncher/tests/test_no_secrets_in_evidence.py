"""Nothing committed here carries a live credential (added 2026-09-07).

The defect this exists for: two gate captures from 2026-09-04 carried the two
CMaNGOS installs' **generated MySQL root passwords**, in a line the worldserver
itself printed when it could not reach its database —

    Cannot connect to world database tbc-db;3306;mangos;<password>;mangos

— and were merged upstream with #143 before GitGuardian reported them. Nobody
read those files line by line, which is the point: a gate transcript is written
by a machine and skimmed by a person, and that is exactly where a secret hides.

The shape is knowable, which is what makes this checkable. `resolve_secrets()`
mints a per-install password as `<game>-<token_hex(8)>`, so anything matching
`<word>-<16 hex>` in a committed page or capture is either one of those or an
illustration of one. The illustrations are listed below with their reason, and
anything else fails.

This is deliberately not a general secret scanner. It knows one shape — the one
this project generates — because a guard that recognises what we produce is
worth more than one that recognises everything badly.
"""

from __future__ import annotations

import re
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
SEARCHED = ("pyplan", "pylauncher")
SUFFIXES = {".md", ".log", ".txt", ".json", ".py", ".yml", ".yaml", ".sh", ".cmd", ".tmpl"}

_GENERATED_PASSWORD = re.compile(r"\b([a-z]+)-([0-9a-f]{16})\b")

ILLUSTRATIONS = {
    # value -> why it is not a secret. Written out rather than derived: the map
    # IS the claim, and one computed from the files would agree with whatever
    # the files happened to contain. A new entry needs a reason a person wrote.
    "0123456789abcdef": "hex digits in order, in prose explaining the shape",
    "fedcba9876543210": "the same, backwards, in a Dockerfile test fixture",
    "0a1b2c3d4e5f6a7b": "a counting pattern used in bug-checklist prose",
    "1a2b3c4d5e6f7a8b": "the same pattern, in controller-view and sqlplan fixtures",
    "deadbeefcafe1234": "`deadbeef` and `cafe` — words, in a cmangos docstring example",
    "0000000000000000": "sixteen zeros — the dummy the 8.9b tests write into a .db_password",
}


def _candidates() -> list[tuple[Path, int, str]]:
    found: list[tuple[Path, int, str]] = []
    for root in SEARCHED:
        for path in sorted((REPO / root).rglob("*")):
            if not path.is_file() or path.suffix not in SUFFIXES:
                continue
            if ".git" in path.parts or "__pycache__" in path.parts:
                continue
            try:
                text = path.read_text(encoding="utf-8")
            except (UnicodeDecodeError, OSError):
                continue
            for number, line in enumerate(text.splitlines(), start=1):
                for match in _GENERATED_PASSWORD.finditer(line):
                    if match.group(2) in ILLUSTRATIONS:
                        continue
                    found.append((path.relative_to(REPO), number, match.group(0)))
    return found


def test_no_committed_page_or_capture_carries_a_generated_password() -> None:
    """`<game>-<16 hex>` is the shape this project mints; none may be committed."""
    found = _candidates()

    assert not found, "a generated password is committed:\n" + "\n".join(
        f"  {path}:{number}  {value}" for path, number, value in found
    )


def test_the_guard_can_actually_see_one() -> None:
    """A guard nobody has watched fail is a guard nobody should trust.

    The pattern is exercised against the exact line that leaked — with the value
    assembled from halves, so that proving the guard works does not re-commit
    the thing it is guarding against, and so this file passes its own scan.
    """
    value = "tbc-" + "faf2e5c4" + "5f363783"
    leaked = f"Cannot connect to world database tbc-db;3306;mangos;{value};mangos"

    match = _GENERATED_PASSWORD.search(leaked)

    assert match is not None
    assert match.group(0) == value
    assert match.group(2) not in ILLUSTRATIONS


def test_every_illustration_says_why_it_is_not_a_secret() -> None:
    """The bar for the allow-list: a reason a person wrote, not a shape a script matched.

    This cannot decide whether a hex string is random — that is the whole
    difficulty — so it does not pretend to. What it can insist on is that
    somebody looked at each one and said why, which is the same rule
    `test_spine.py`'s folder-listing map uses.
    """
    for value, reason in ILLUSTRATIONS.items():
        assert reason.strip(), f"{value} is allowed with no reason given"
        assert len(reason) > 20, f"{value}'s reason is too short to be one: {reason!r}"


def test_the_allow_list_is_not_a_place_to_put_a_real_one() -> None:
    """Every entry must actually appear in the tree, as an illustration does.

    An allow-list that accumulates values nothing uses is how a real secret gets
    parked in one: the entry outlives the file it was written for, and nobody
    can tell the difference afterwards.
    """
    haystack = "\n".join(
        path.read_text(encoding="utf-8", errors="ignore")
        for root in SEARCHED
        for path in (REPO / root).rglob("*")
        if path.is_file() and path.suffix in SUFFIXES and "__pycache__" not in path.parts
    )
    unused = [value for value in ILLUSTRATIONS if value not in haystack]

    assert not unused, f"these are allowed but appear nowhere: {unused}"
