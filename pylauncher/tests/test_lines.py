"""Tests for `yulon.ui.lines` (T35): what shape a log line has, and what it says once stripped."""

from __future__ import annotations

import pytest

from yulon.ui import lines


def test_a_progress_line_carries_its_stage_its_percent_and_its_text() -> None:
    """Mutation: drop the `PROGRESS` branch and this reads `sentence` with the payload intact."""
    parsed = lines.parse(lines.PROGRESS + "clone-core 42 Receiving objects:  42% (420/1000)")
    assert parsed.kind == "progress"
    assert parsed.stage == "clone-core"
    assert parsed.percent == 42
    assert parsed.text == "Receiving objects:  42% (420/1000)"


def test_a_progress_line_with_no_number_is_busy_rather_than_zero() -> None:
    """`-` and `0` are different sentences: one is "working", the other is "nothing done"."""
    parsed = lines.parse(lines.PROGRESS + "clone-core - Enumerating objects")
    assert parsed.kind == "progress"
    assert parsed.percent is None
    assert parsed.text == "Enumerating objects"


@pytest.mark.parametrize(
    "payload",
    ["", "clone-core", "clone-core nine tiles left", "clone-core 42% Receiving objects"],
)
def test_a_malformed_progress_line_becomes_a_sentence_and_loses_nothing(payload: str) -> None:
    """The control character never reaches a reader, and neither does a dropped line.

    Mutation: return `Parsed("progress", ...)` for an unparsable payload and the
    percent is `None` for a line that had one, so the strip goes busy mid-clone.
    """
    parsed = lines.parse(lines.PROGRESS + payload)
    assert parsed.kind == "sentence"
    assert parsed.text == payload
    assert lines.PROGRESS not in parsed.text


def test_a_tool_line_is_tool_and_keeps_only_its_payload() -> None:
    """Mutation: drop the `TOOL` branch and this reads `sentence` with `\\x1etool ` still on it."""
    parsed = lines.parse(lines.TOOL + "#12 [5/8] RUN cmake --build .")
    assert parsed.kind == "tool"
    assert parsed.text == "#12 [5/8] RUN cmake --build ."


@pytest.mark.parametrize(
    ("payload", "kind"),
    [
        ("mmaps.cpp:88: warning: unused variable 'tile'", "warning"),
        ("collect2: error: ld returned 1 exit status", "failure"),
    ],
)
def test_a_tools_own_warning_or_failure_wins_over_the_tool_tone(payload: str, kind: str) -> None:
    """A compiler's `warning:` is still amber, and its `error:` still red.

    Mutation: return `tool` for every `TOOL`-prefixed line and both of these read
    `tool`, which is the dimmed tone the torrent gets.
    """
    parsed = lines.parse(lines.TOOL + payload)
    assert parsed.kind == kind
    assert parsed.text == payload


def test_a_step_line_is_a_stage() -> None:
    """Mutation: drop `_STEP` and this falls to `sentence`."""
    parsed = lines.parse("Step 3 of 9 (33%): clone-core")
    assert parsed.kind == "stage"
    assert parsed.text == "Step 3 of 9 (33%): clone-core"


def test_a_marker_line_is_a_marker() -> None:
    """Mutation: drop `_MARKER` and this falls to `sentence`."""
    assert lines.parse("--- clone-core").kind == "marker"


@pytest.mark.parametrize(
    "line",
    [
        "the build FAILED after 97 minutes",
        "INSTALL FAILED",
        "error: could not read the manifest",
        "an error happened here: exit 2",
        "[fail] acore_world",
        "/home/pk is your home folder itself, so the install was refused",
    ],
)
def test_every_failure_shape_reads_as_a_failure(line: str) -> None:
    """Mutation: drop any one alternative from `_FAILURE` and that line falls to `sentence`."""
    assert lines.parse(line).kind == "failure"


@pytest.mark.parametrize(
    "line",
    ["warning: this machine may go to sleep", "[warn] no mmaps yet", "WARNING: 3 GB of RAM"],
)
def test_every_warning_shape_reads_as_a_warning(line: str) -> None:
    """Mutation: drop any one alternative from `_WARNING` and that line falls to `sentence`."""
    assert lines.parse(line).kind == "warning"


def test_a_failure_wins_over_a_warning_in_the_same_line() -> None:
    """One line, one tone, and the worse of the two is the one worth painting."""
    assert lines.parse("WARNING: the import FAILED").kind == "failure"


@pytest.mark.parametrize(
    "line",
    ["Cloning azerothcore-wotlk into .", "Sources are in place.", "", "  "],
)
def test_anything_else_is_a_sentence_and_is_shown_as_written(line: str) -> None:
    assert lines.parse(line) == lines.Parsed("sentence", line)


def test_a_step_line_gives_up_its_number_its_total_and_its_name() -> None:
    """What the strip's label is built from.

    Mutation: read the total off group 1 (`number`) and the label says
    `Step 3 of 3`, which the panel's own test refuses.
    """
    step = lines.parse_step("Step 3 of 9 (33%): clone-core")
    assert step == lines.Step(3, 9, "clone-core")


@pytest.mark.parametrize(
    "line", ["--- clone-core", "Step 3 of nine (33%): clone-core", "Sources are in place."]
)
def test_a_line_that_is_not_a_step_line_yields_no_step(line: str) -> None:
    assert lines.parse_step(line) is None


def test_the_two_prefixes_share_one_control_character_no_engine_line_can_hold() -> None:
    """`\\x1e` is RECORD SEPARATOR: nothing git, BuildKit or the spine prints uses it.

    The prefixes are asserted here rather than only used, because they are the
    wire format between the engine and the panel and a changed spelling would
    strand the other half silently.
    """
    assert lines.PROGRESS == "\x1eprogress "
    assert lines.TOOL == "\x1etool "
    assert lines.PROGRESS.startswith("\x1e") and lines.TOOL.startswith("\x1e")
