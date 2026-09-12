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


# ---------------------------------------------------------------------------
# T35 point 4: a relayed line is marked as a tool's, and a number in it moves
# the strip instead of scrolling past.


def test_a_relayed_line_with_no_number_in_it_is_tool_output() -> None:
    """The default, and most of a build is this.

    Mutation: return the line unmarked and the panel cannot tell a compiler's
    chatter from the engine's own sentences, which is the state T35 was filed
    about.
    """
    marked = lines.relayed(">> Applying update 2026_01_01_00.sql", stage="import")
    assert marked == lines.TOOL + ">> Applying update 2026_01_01_00.sql"
    assert lines.parse(marked).kind == "tool"


def test_buildkits_step_count_becomes_a_percent() -> None:
    """`#12 [5/8]` is five steps of eight, which is the only progress a build reports.

    BuildKit prints no percentage of its own. What it prints is a step number
    out of a total, per stage of the Dockerfile, and that is what moves.

    The sample is a real line, taken from the 7.7 WotLK gate's build transcript
    (`pyplan/gates/`): every step line there carries its stage's NAME before the
    count -- `#10 [builder 3/6]`, `#10 [ac-authserver skeleton 3/4]` -- because
    every Dockerfile this app generates is multi-stage.

    Mutation: drop the BuildKit rule and this line comes back as `tool`, so the
    bar never moves during the longest stage of the install.
    """
    parsed = lines.parse(lines.relayed("#10 [builder 5/8] RUN cmake --build .", stage="build"))
    assert parsed.kind == "progress"
    assert parsed.percent == 62  # 5 * 100 // 8
    assert parsed.stage == "build"
    assert parsed.text == "#10 [builder 5/8] RUN cmake --build ."


def test_the_compiler_reports_from_inside_a_buildkit_step() -> None:
    """The finer of the two numbers, on the line BuildKit really prints it on.

    Also a real shape from the same transcript: `#13 0.242 [  0%] Building C
    object dep/src/bzip2/...`. BuildKit prefixes a step's OWN OUTPUT with the
    step number and an elapsed time, so such a line matches the compiler's
    pattern and not the step-count one — which is why a build shows step
    progress between steps and compiler progress during them.

    Mutation: drop the compiler rule and the bar stands still for the whole of
    `RUN cmake --build`, which is hours.
    """
    parsed = lines.parse(
        lines.relayed("#13 0.242 [ 43%] Building C object dep/src/x.c.o", stage="build")
    )
    assert parsed.kind == "progress"
    assert parsed.percent == 43


def test_the_compilers_percent_wins_where_a_line_carries_both() -> None:
    """Contrived, and pinned anyway, because the precedence is a choice.

    No line in the gate transcripts carries both numbers. If one ever does, the
    compiler's figure is the one that means something: a BuildKit step covers a
    whole `RUN cmake --build` and moves once an hour.

    Mutation: read the compiler's percent BEFORE BuildKit's step count and the
    step's figure wins instead.
    """
    parsed = lines.parse(
        lines.relayed("#10 [builder 5/8] RUN [ 43%] Building CXX object", stage="build")
    )
    assert parsed.percent == 43


def test_the_mmap_generators_tile_count_becomes_a_percent_and_a_short_sentence() -> None:
    """The 97 minutes of tile lines the T30 install wrote, as one moving field.

    Measured from `pyplan/gates/7.7-win11-tortoise/tortoise77.log`, which holds
    8005 of these: `[Map 000] Building tile [22,52] (01 / 741)`. The percentage
    is within the map, because that is what the numbers in the line are, and the
    text is rewritten short — the tile's own coordinates are noise at one line
    per second.

    Mutation: drop the mmap rule and the strip shows nothing for the longest
    stage a CMaNGOS install has, with the tile lines back in the panel.
    """
    parsed = lines.parse(lines.relayed("[Map 230] Building tile [32,32] (08 / 12)", stage="mmaps"))
    assert parsed.kind == "progress"
    assert parsed.percent == 66  # 8 * 100 // 12
    assert parsed.text == "Map 230 · tile 8 of 12"


def test_a_tile_line_with_no_map_in_it_still_moves_the_bar() -> None:
    """The count is the reading; the map is the label. A line with only one is not lost."""
    parsed = lines.parse(lines.relayed("Building tile [1,2] (03 / 10)", stage="mmaps"))
    assert parsed.kind == "progress"
    assert parsed.percent == 30
    assert parsed.text == "tile 3 of 10"


def test_a_relayed_line_that_divides_by_nothing_is_tool_output() -> None:
    """A total of zero is not a percentage, and must not be a crash either.

    Defensive rather than observed: no `0/0` appears in any gate transcript in
    `pyplan/gates/`. It is guarded because the cost of being wrong is not a
    wrong reading — it is a `ZeroDivisionError` on the thread carrying a running
    install's output.

    Mutation: compute `k * 100 // m` without the guard and this raises instead
    of answering.
    """
    assert lines.parse(lines.relayed("#1 [internal 0/0] load", stage="build")).kind == "tool"
