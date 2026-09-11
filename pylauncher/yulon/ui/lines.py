"""What kind of line arrived, and what it says once its prefix is off (T35).

The install engine, the git seam and every relayed subprocess all yield plain
strings into one `LogPanel`, and until T35 they arrived indistinguishable: the
97 minutes of `[Map 230] Building tile [32,32] (08 / 12)` the T30 install wrote
had the same weight as "Cloning …". This module is the one place that answers
"what is this line" — a pure classifier, no Qt, so the engine side can build a
line the panel side will understand without either importing the other's world.

Two prefixes carry what a line's own text cannot say: that a line is a
subprocess's output rather than the app's own sentence (`TOOL`), and that a line
is a progress reading for the header strip rather than something to append
(`PROGRESS`). Everything else is recognised BY SHAPE, because the engine's own
`Step N of M (P%): <name>` and `--- <name>` lines are what gate scripts, log
captures and the interrupted-import watchers grep, and a prefix on those would
have moved a format the whole project matches on.

`\\x1e` (RECORD SEPARATOR) begins both prefixes: it is not printable, so nothing
git, BuildKit, cmake or a shell script emits contains one, and a line that
somehow did would be classified rather than lost — see `parse()`.
"""

from __future__ import annotations

import re
from dataclasses import dataclass
from typing import Literal

PROGRESS = "\x1eprogress "
"""Marks a progress reading: `PROGRESS + "<stage> <percent|-> <text>"`.

Not appended by the panel. `-` where the source gave no number, because a busy
bar and a bar at 0% say different things and a progress line with no percent is
the first kind.
"""

TOOL = "\x1etool "
"""Marks one line of a subprocess's output, relayed as it arrived."""

Kind = Literal["stage", "marker", "sentence", "tool", "warning", "failure", "progress"]


@dataclass(frozen=True)
class Parsed:
    """One line, classified. `text` is what a reader should see — never the prefix."""

    kind: Kind
    text: str
    stage: str = ""
    percent: int | None = None


@dataclass(frozen=True)
class Step:
    """The three fields of a `Step N of M (P%): <name>` line, for the header strip."""

    number: int
    total: int
    name: str


_STEP = re.compile(r"^Step (\d+) of (\d+)(?: \(\d+%\))?: (.*)$")
_STAGE = re.compile(r"^Step \d+ of \d+")
_MARKER = re.compile(r"^--- ")
_FAILURE = re.compile(r"\bFAILED\b|^INSTALL FAILED|^error:|\berror\b.*:|\[fail\]|refused")
_WARNING = re.compile(r"^warning:|:\s+warning:|\[warn\]|\bWARNING\b")
"""The amber shapes, and the second alternative is not one of T35's three.

`^warning:` alone cannot see a compiler, and the compiler is the reason this
tone exists: gcc and clang both write `mmaps.cpp:88:5: warning: …`, so an
anchored pattern left every real warning in a two-hour build dimmed as ordinary
tool chatter. `_FAILURE` needed no such addition — `\berror\b.*:` is unanchored
and already matches `collect2: error: …` where `^error:` would not.
"""


def _tone(text: str) -> Kind | None:
    """`failure`, `warning`, or neither — the two tones a line's own words can claim.

    Failure is asked first so one line gets one tone: "WARNING: the import
    FAILED" is a failure, and painting it amber would say the softer of the two
    things it says.
    """
    if _FAILURE.search(text):
        return "failure"
    if _WARNING.search(text):
        return "warning"
    return None


def _progress(payload: str) -> Parsed | None:
    """`<stage> <percent|-> <text>` as a `Parsed`, or None when it is not that shape."""
    parts = payload.split(" ", 2)
    if len(parts) < 2:
        return None
    stage, percent = parts[0], parts[1]
    text = parts[2] if len(parts) == 3 else ""
    if percent == "-":
        return Parsed("progress", text, stage=stage, percent=None)
    if not percent.isdigit():
        return None
    return Parsed("progress", text, stage=stage, percent=int(percent))


def parse(line: str) -> Parsed:
    """Classify one line and hand back what should be shown for it.

    Order is the whole specification, so it is spelled out here rather than left
    to be read off the branches:

    1. `PROGRESS` — a progress reading. A payload that will not parse becomes a
       `sentence` WITH THE PREFIX STRIPPED, never a dropped line: the panel does
       not append progress lines, so a malformed one classified as progress
       would be a line the engine yielded and nobody ever saw.
    2. `TOOL` — relayed subprocess output, unless the payload's own words claim
       a tone. A compiler's `warning:` is still amber, because the reason tool
       output is dimmed is that most of it is chatter, and that line is not.
    3. The engine's own shapes: `Step N of M`, then `--- `.
    4. The two tones, failure before warning.
    5. Anything else is one of the app's own sentences.
    """
    if line.startswith(PROGRESS):
        payload = line[len(PROGRESS) :]
        return _progress(payload) or Parsed("sentence", payload)
    if line.startswith(TOOL):
        payload = line[len(TOOL) :]
        return Parsed(_tone(payload) or "tool", payload)
    if _STAGE.match(line):
        return Parsed("stage", line)
    if _MARKER.match(line):
        return Parsed("marker", line)
    return Parsed(_tone(line) or "sentence", line)


def parse_step(line: str) -> Step | None:
    """The stage line's own three fields, or None for any other line.

    Read off the engine's format rather than counted here: the spine is the only
    thing that knows how many stages this run has, and a resumed install or a
    rebuild has a different total from a first install's.
    """
    found = _STEP.match(line)
    if found is None:
        return None
    return Step(int(found.group(1)), int(found.group(2)), found.group(3))
