#!/usr/bin/env python3
"""Round 2, must-fix 2: tie the bytes step 04 PRESSED to the bytes that shipped.

`04-second-press.log:11` prints the md5 of the `party.py` the second press ran
against: `fca7d004a700164ff6374b187ce93d16`. The tree that held it was
`~/t16-live` and it is gone. So the tie is proved the other way round: take the
file as it was COMMITTED, undo the two edits made after the press, and show the
md5 comes back to `fca7d004…`.

Both undone edits are inside docstrings and the second is followed by `black`'s
own reflow of the paragraph it touched. Neither changes a statement, and this
script prints the diff between the reconstruction and the committed file so a
reader can see that for themselves rather than take the word for it.

Run from the worktree root:

    python3 pyplan/gates/8.6-level-holds-…/14-pressed-bytes.py <git-ref>

`<git-ref>` defaults to the commit that carried the code half.
"""

from __future__ import annotations

import difflib
import hashlib
import subprocess
import sys
from datetime import datetime

PATH = "pylauncher/yulon/party.py"
PRESSED_MD5 = "fca7d004a700164ff6374b187ce93d16"

# The two edits, newest first, each as (what is in the committed file, what the
# pressed file had there). Both are docstring text.
UNDO = [
    (
        """apart is that with room for a world under more load — and it is a CEILING, not a
wait: the poll returns the moment the row agrees, and on the second press the
whole step, send and save and read, took 3.2 seconds and then 4.4.""",
        """apart is that with room for a world under more load — and it is a CEILING, not a
wait: the poll returns the moment the row agrees, which on the second press took
2.1 seconds.""",
    ),
    (
        """    world's own `.pinfo` read the chosen level within a second of the send and
    never stopped reading it — ninety seconds later in the three rows whose tail
    ran that long, fifteen to forty-six in the other four. There is no window.
    What T5 photographed is
    `characters.level` not having been written: `.character level` on an ONLINE""",
        """    world's own `.pinfo` read the chosen level within a second of the send and
    still read it a minute later, through the same 30-second tail in which
    nothing in the module moved it. There is no window. What T5 photographed is
    `characters.level` not having been written: `.character level` on an ONLINE""",
    ),
]


def main() -> int:
    ref = sys.argv[1] if len(sys.argv) > 1 else "e241fc95"
    print(f"T16 round 2, must-fix 2 -- {datetime.now():%H:%M:%S} local")
    committed = subprocess.run(
        ["git", "show", f"{ref}:{PATH}"], capture_output=True, text=True, check=True
    ).stdout
    print(f"committed {ref}:{PATH}")
    print(f"    md5    = {hashlib.md5(committed.encode()).hexdigest()}")
    print(f"    sha256 = {hashlib.sha256(committed.encode()).hexdigest()}")

    rebuilt = committed
    for after, before in UNDO:
        count = rebuilt.count(after)
        print(f"undo: anchor matched {count} time(s)")
        if count != 1:
            print("ABORTED: an anchor that does not match exactly once proves nothing")
            return 2
        rebuilt = rebuilt.replace(after, before)

    md5 = hashlib.md5(rebuilt.encode()).hexdigest()
    print(f"reconstruction md5 = {md5}")
    print(f"pressed md5        = {PRESSED_MD5}   (04-second-press.log:11)")
    print("MATCH" if md5 == PRESSED_MD5 else "NO MATCH")

    print()
    print("the whole difference between the pressed bytes and the committed file:")
    diff = difflib.unified_diff(
        rebuilt.splitlines(keepends=True),
        committed.splitlines(keepends=True),
        fromfile="pressed (reconstructed)",
        tofile=f"committed {ref}",
        n=1,
    )
    for line in diff:
        print(line.rstrip("\n"))
    return 0 if md5 == PRESSED_MD5 else 1


if __name__ == "__main__":
    raise SystemExit(main())
