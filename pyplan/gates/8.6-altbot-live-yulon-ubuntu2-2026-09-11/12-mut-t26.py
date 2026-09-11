#!/usr/bin/env python3
"""T26 live half -- the mutation table for the one code change this lane made.

T16's `16-mut-t26.py`, unchanged in shape and for the same reason
(`mutation-testing-pycache-trap`): read the pristine bytes into MEMORY, assert
the anchor matches exactly once, write the mutant, purge `__pycache__` on both
sides, run the named test, restore from the bytes held in memory, purge again,
re-run and record green. A row is only evidence if the mutant was in the file
and was executed, so the harness asserts the mutant is in the file before it
runs anything.

Round 1's change is `party_rows_sql` + `InstallParty.party_members`, and
`add_named` being handed the second of those instead of `members` (M1-M4: the
clause it drops, the clause it keeps, the seam it is wired to, and the anchor it
reads from).

Round 2's change is the union in `group_rows_sql`, `AltbotMemory` behind it, and
the ground read `InstallParty.remove` now takes (M5-M12: the union arm, the
master's exclusion from it, the name check on a value that comes off disk,
remembering on the join, pruning against the party, NOT pruning on a failed
read, and the dismissal ground read).
"""

from __future__ import annotations

import hashlib
import shutil
import subprocess
import sys
from pathlib import Path

REPO = Path("/home/perzi/dads-mmo-lab/.claude/worktrees/t26")
PARTY = REPO / "pylauncher" / "yulon" / "party.py"

MUTATIONS = [
    (
        "M1 the master's own row is not dropped from the named route's party read",
        PARTY,
        "        f\"AND gm.memberGuid <> {int(master_guid)} \"\n"
        "        \"ORDER BY c.name\"\n",
        "        \"ORDER BY c.name\"\n",
        "test_the_named_routes_party_read_does_not_filter_by_the_bot_marker",
    ),
    (
        "M2 add_named is handed members() again, as the unit half handed it",
        PARTY,
        "            members=lambda: self.party_members(master),\n",
        "            members=lambda: self.members(master),\n",
        "test_the_seam_polls_the_named_add_with_the_unfiltered_party_read",
    ),
    (
        "M3 members() loses the bot marker too -- the fix made too wide",
        PARTY,
        "                group_rows_sql(\n"
        "                    self.entry,\n"
        "                    answer.marker,\n"
        "                    master_guid=guid,\n"
        "                    also=self._altbots.names(master),\n"
        "                )\n",
        "                party_rows_sql(self.entry, master_guid=guid)\n",
        "test_the_bot_routes_party_read_still_filters_by_the_bot_marker",
    ),
    (
        "M5 the party read drops the names this app remembers adding",
        PARTY,
        "        clause = f\"({clause}) OR c.name IN ({names})\"\n",
        "        clause = f\"({clause})\"\n",
        "test_the_group_read_counts_a_name_this_app_remembers_adding",
    ),
    (
        "M6 the party read no longer drops the master by guid, so the union can return him",
        PARTY,
        "        f\"AND gm.memberGuid <> {int(master_guid)} \"\n"
        "        f\"AND ({clause}) \"\n",
        "        f\"AND ({clause}) \"\n",
        "test_the_group_read_never_counts_the_master_himself",
    ),
    (
        "M7 a remembered name goes into the query without being checked",
        PARTY,
        "    remembered = tuple(name for name in also if valid_name(name))\n",
        "    remembered = tuple(also)\n",
        "test_a_remembered_name_that_is_not_a_character_name_is_refused_not_quoted",
    ),
    (
        "M8 add_named does not remember the character it saw join",
        PARTY,
        "            self._altbots.remember(master, result.name)\n",
        "            pass\n",
        "test_the_seam_remembers_a_character_it_added_and_then_counts_it",
    ),
    (
        "M9 the record is never pruned against the group table",
        PARTY,
        "            self._altbots.keep_only(master, [row.name for row in rows])\n",
        "            pass\n",
        "test_a_remembered_character_that_left_the_party_is_forgotten",
    ),
    (
        "M10 the record is pruned on a read that FAILED too",
        PARTY,
        "        rows = read_members(raw)\n        if not isinstance(rows, str):\n",
        "        rows = read_members(raw)\n        if True:\n",
        "test_a_party_row_that_did_not_parse_forgets_nothing_either",
    ),
    (
        "M11 the seam's dismissal ground read is thrown away -- round 1's vacuous success",
        PARTY,
        "        if all(row.name != bot for row in standing):\n",
        "        if False:\n",
        "test_the_seam_refuses_to_dismiss_a_character_that_is_not_in_the_party",
    ),
    (
        "M4 the named route's read is anchored on the master's GUID, not his group",
        PARTY,
        "        f\"WHERE gm.guid = (SELECT guid FROM {chars}.group_member \"\n"
        "        f\"WHERE memberGuid = {int(master_guid)} LIMIT 1) \"\n"
        "        f\"AND gm.memberGuid <> {int(master_guid)} \"\n"
        "        \"ORDER BY c.name\"\n",
        "        f\"WHERE gm.guid = {int(master_guid)} \"\n"
        "        f\"AND gm.memberGuid <> {int(master_guid)} \"\n"
        "        \"ORDER BY c.name\"\n",
        "test_the_named_routes_party_read_is_anchored_on_the_masters_own_group",
    ),
]


def purge() -> None:
    for cache in REPO.rglob("__pycache__"):
        if ".venv" not in str(cache):
            shutil.rmtree(cache, ignore_errors=True)


def run(test: str) -> tuple[bool, str]:
    proc = subprocess.run(
        ["yt", f"tests/test_party.py::{test}", "-q", "--no-header", "-p", "no:cacheprovider"],
        cwd=REPO,
        env={
            "YULON_REPO": str(REPO),
            "PATH": "/home/perzi/.local/bin:/usr/bin:/bin",
            "HOME": "/home/perzi",
        },
        capture_output=True,
        text=True,
    )
    tail = [line for line in proc.stdout.splitlines() if "passed" in line or "failed" in line]
    return proc.returncode == 0, (tail[-1] if tail else proc.stdout.strip()[-200:])


def main() -> int:
    print(f"party.py sha256 before: {hashlib.sha256(PARTY.read_bytes()).hexdigest()[:16]}")
    rows = []
    for name, path, anchor, mutant, test in MUTATIONS:
        pristine = path.read_bytes()
        text = pristine.decode()
        count = text.count(anchor)
        if count != 1:
            print(f"{name}: ANCHOR MATCHED {count} TIMES -- row abandoned, nothing written")
            rows.append((name, test, f"anchor matched {count}", "-"))
            continue
        path.write_text(text.replace(anchor, mutant))
        assert mutant in path.read_text(), "the mutant is not in the file"
        purge()
        ok_red, red_line = run(test)
        path.write_bytes(pristine)
        assert hashlib.sha256(path.read_bytes()).hexdigest() == hashlib.sha256(pristine).hexdigest()
        purge()
        ok_green, green_line = run(test)
        rows.append(
            (
                name,
                test,
                ("SURVIVED " if ok_red else "died: ") + red_line,
                ("green: " if ok_green else "STILL RED ") + green_line,
            )
        )
        print(f"{name}\n    mutated -> {rows[-1][2]}\n    restored -> {rows[-1][3]}")
    print(f"party.py sha256 after:  {hashlib.sha256(PARTY.read_bytes()).hexdigest()[:16]}")
    print()
    print("| # | mutation | test | mutated | restored |")
    print("|---|---|---|---|---|")
    for name, test, red, green in rows:
        num, _, rest = name.partition(" ")
        print(f"| {num} | {rest} | `{test}` | {red} | {green} |")
    return 0


if __name__ == "__main__":
    sys.exit(main())
