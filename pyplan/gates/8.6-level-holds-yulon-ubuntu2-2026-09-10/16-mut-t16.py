#!/usr/bin/env python3
"""T16's mutation table. One directory of its own, per `mutation-testing-pycache-trap`.

For each row: read the pristine bytes into MEMORY, assert the anchor matches
exactly once, write the mutant, purge `__pycache__` on both sides, run the named
test and record red/green, restore from the bytes held in memory, purge again,
re-run and record green. A row is only evidence if the mutant was in the file
and was executed, so the harness re-reads the mutated region back and prints it.
"""

from __future__ import annotations

import hashlib
import shutil
import subprocess
import sys
from pathlib import Path

REPO = Path("/home/perzi/dads-mmo-lab/.claude/worktrees/t16")
PARTY = REPO / "pylauncher" / "yulon" / "party.py"

MUTATIONS = [
    (
        "M1 the save is not sent before the row is read",
        PARTY,
        "    refusal = _save_refusal(send(SAVE_COMMAND))\n",
        '    refusal = ""\n',
        "test_the_level_readback_is_the_row_the_server_wrote_and_not_the_commands_own_yes",
    ),
    (
        "M2 the readback is the level that was asked for, not characters.level",
        PARTY,
        "    after = _row_level(members, guid)\n    if after == level or after is None:\n"
        "        return LevelStep(after)\n",
        "    after = level\n    if after == level or after is None:\n"
        "        return LevelStep(after)\n",
        "test_the_level_readback_is_the_row_the_server_wrote_and_not_the_commands_own_yes",
    ),
    (
        "M3 the level is resent whether or not the written row disagrees",
        PARTY,
        "    if after == level or after is None:\n        return LevelStep(after, saved=True)\n",
        "    if after is None:\n        return LevelStep(after, saved=True)\n",
        "test_a_level_the_written_row_agrees_with_is_never_sent_a_second_time",
    ),
    (
        "M4 a press that needed a second send does not say so",
        PARTY,
        "    return LevelStep(after, resent=True, saved=True)",
        "    return LevelStep(after, resent=False, saved=True)",
        "test_a_level_that_needed_a_second_send_says_so_in_the_panels_own_sentence",
    ),
    (
        "M5 the did-NOT-take sentence is replaced by the one that says it landed",
        PARTY,
        '            f"{level}, after the row was written and the level was sent a second time. '
        'Take "\n            "the level as not set."',
        '            f"{level}."',
        "test_a_level_the_written_row_still_disagrees_with_is_not_reported_as_set",
    ),
    (
        "M7 the saveall's answer is thrown away, as round 1 threw it away",
        PARTY,
        "    refusal = _save_refusal(send(SAVE_COMMAND))\n    if refusal:\n"
        "        return (_row_level(members, guid), refusal)\n",
        "    send(SAVE_COMMAND)\n",
        "test_a_saveall_the_server_refused_is_its_own_sentence_and_not_a_verdict_on_the_level",
    ),
    (
        "M8 an indeterminate save is reported as one the server refused",
        PARTY,
        "    if answer.indeterminate:\n",
        "    if False:\n",
        "test_a_saveall_that_never_came_back_is_not_reported_as_one_the_server_refused",
    ),
    (
        "M9 the already arm is tested before the readback, as round 1 tested it",
        PARTY,
        '    if after is None:\n        return (\n            " The level was sent and this bot '
        'is no longer in the group table, so no level "',
        '    if before == level:\n        return (\n            f" characters.level already read '
        '{level} before the press, so nothing about the "\n            "level changed and this '
        'says nothing about whether it would have."\n        )\n    if after is None:\n'
        '        return (\n            " The level was sent and this bot is no longer in the '
        'group table, so no level "',
        "test_a_bot_already_at_the_chosen_level_whose_written_row_disagrees_is_told_the_row",
    ),
    (
        "M10 a refused second send opens 'the level was not set', as round 1 opened it",
        PARTY,
        '            f"The server took the level and characters.level read {after}, not {level}, '
        'after the "\n            f"row was written. The second send was refused: {said}",\n'
        '            resent=True,',
        '            f"The level was not set: {said}",\n            resent=True,',
        "test_a_refused_second_send_says_the_first_one_was_taken",
    ),
    (
        "M6 a bot that left the group is treated as a level that disagreed",
        PARTY,
        "    after = _row_level(members, guid)\n    if after == level or after is None:\n"
        "        return LevelStep(after)\n",
        "    after = _row_level(members, guid)\n    if after == level:\n"
        "        return LevelStep(after)\n",
        "test_a_bot_that_left_the_group_before_the_row_was_read_is_not_sent_the_level_again",
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
        env={"YULON_REPO": str(REPO), "PATH": "/home/perzi/.local/bin:/usr/bin:/bin", "HOME": "/home/perzi"},
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
