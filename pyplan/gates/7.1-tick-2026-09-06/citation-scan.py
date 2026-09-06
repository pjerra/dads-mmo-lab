#!/usr/bin/env python3
"""Re-derive the citation counts in this folder, without pytest.

Written 2026-09-06 for the 7.1 tick. It is a copy of the rule
`pylauncher/tests/test_docs_pins.py` applies -- `GONE`, the 60-character window,
`_cited_as_live()` -- so a reader can ask "which names on this page does the
guard count as live, and which of those resolve?" of ANY git revision, including
the one before the pass, which pytest cannot be pointed at.

It is a copy and therefore a second place to remember: if the guard's rule ever
changes, this script is wrong and the numbers it produced stay true only of the
day they were produced. That is why every number it is quoted for is dated and
pinned to a SHA. The authoritative run is the pytest transcript beside it
(`before-docspins.txt`, `after-docspins.txt`); this script exists to say WHICH
names moved, which the assertion message alone does not.

    python citation-scan.py <repo> <ref-or-WORKTREE> <path-under-repo>
    python citation-scan.py <repo> --compare <ref> <path-under-repo>

The test names are always read out of the WORKING TREE's `pylauncher/tests/`,
because "does this citation resolve" is a question about the tree the reader has.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

GONE = re.compile(r"\bdeletes?\b|\bdeleted\b|never written", re.I)
WINDOW = 60


def cited_as_live(text: str) -> dict[str, list[int]]:
    out: dict[str, list[int]] = {}
    for lineno, line in enumerate(text.splitlines(), 1):
        for match in re.finditer(r"\btest_[a-z0-9_]+\b", line):
            near = line[max(0, match.start() - WINDOW) : match.end() + WINDOW]
            if not GONE.search(near):
                out.setdefault(match.group(), []).append(lineno)
    return out


def defined_names(repo: Path) -> set[str]:
    names: set[str] = set()
    for path in (repo / "pylauncher" / "tests").rglob("test_*.py"):
        names.add(path.stem)
        names |= set(
            re.findall(r"^def (test_[a-z0-9_]+)", path.read_text(encoding="utf-8"), re.M)
        )
    return names


def read(repo: Path, ref: str, rel: str) -> str:
    if ref == "WORKTREE":
        return (repo / rel).read_text(encoding="utf-8")
    out = subprocess.run(
        ["git", "-C", str(repo), "show", f"{ref}:{rel}"], capture_output=True, check=True
    )
    return out.stdout.decode("utf-8")


def main() -> int:
    repo = Path(sys.argv[1])
    known = defined_names(repo)
    if sys.argv[2] == "--compare":
        ref, rel = sys.argv[3], sys.argv[4]
        before = cited_as_live(read(repo, ref, rel))
        after = cited_as_live(read(repo, "WORKTREE", rel))
        print(f"page: {rel}")
        print(f"before ({ref}): live {len(before)}, unresolved {len([n for n in before if n not in known])}")
        print(f"after (worktree): live {len(after)}, unresolved {len([n for n in after if n not in known])}")
        print(f"\nno longer counted live ({len(set(before) - set(after))}):")
        for name in sorted(set(before) - set(after)):
            print(f"  {name:<84} resolves={name in known}  was at {before[name]}")
        print(f"\nnewly counted live ({len(set(after) - set(before))}):")
        for name in sorted(set(after) - set(before)):
            print(f"  {name:<84} resolves={name in known}  now at {after[name]}")
        return 0
    ref, rel = sys.argv[2], sys.argv[3]
    live = cited_as_live(read(repo, ref, rel))
    missing = {n: ls for n, ls in live.items() if n not in known}
    print(f"page: {rel}  at: {ref}")
    print(f"live names: {len(live)}   unresolved: {len(missing)}")
    for name in sorted(missing):
        print(f"  {name}: lines {missing[name]}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
