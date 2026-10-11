"""`build/check_tag_order.py`: a `-Public` tag must sort above every other public tag.

T669. The updater offers whatever sorts highest, so a tag cut below the newest
one (a typo, a re-cut, a stray `v0.9.2-Public` for `v0.9.20`) either hides the
real release from everyone or, worse, outranks it. The release workflow refuses
such a tag before building anything.
"""

from __future__ import annotations

import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "build"))

import check_tag_order as cto  # noqa: E402

HISTORY = [
    "v0.8.0-Public",
    "v0.8.65-Public",
    "v0.8.66-fixtest",
    "v0.8.7-Public",
    "v0.8.90-Public",
    "v0.9.13-Public",
    "v0.9.14-Public",
    "v0.9.15-Public",
    "v0.9.16-Public",
]


@pytest.mark.parametrize(
    "tag",
    ["v0.9.17-Public", "v0.9.100-Public", "v0.10.0-Public", "v1.0.0-Public"],
)
def test_a_tag_above_the_newest_public_one_passes(tag: str) -> None:
    assert cto.tag_order_problem([*HISTORY, tag], tag) is None


@pytest.mark.parametrize(
    "tag",
    [
        "v0.9.2-Public",  # the typo for v0.9.20: .2 is below .16 from 0.9 on
        "v0.9.016-Public",  # the newest spelled another way: equal is not above
        "v0.8.91-Public",  # a backport: a lower line must not take the top
        "v0.9.9-Public",
    ],
)
def test_a_tag_not_above_the_newest_public_one_is_refused(tag: str) -> None:
    problem = cto.tag_order_problem([*HISTORY, tag], tag)
    assert problem is not None
    assert "v0.9.16-Public" in problem, "the message names the tag it lost to"


def test_the_tag_is_not_compared_with_itself() -> None:
    """The tag being built is in `git tag --list` already."""
    assert cto.tag_order_problem([*HISTORY, "v0.9.17-Public"], "v0.9.17-Public") is None


def test_the_first_public_tag_passes() -> None:
    assert cto.tag_order_problem(["v0.8.0-Public"], "v0.8.0-Public") is None
    assert cto.tag_order_problem([], "v0.8.0-Public") is None


def test_patch_ten_passes_after_patch_nine_and_the_old_rule_would_have_refused_it() -> None:
    """The hybrid rule in the guard: v0.10.10 is above v0.10.9 (it was .1 < .9)."""
    tags = [f"v0.10.{n}-Public" for n in range(11)]
    assert cto.tag_order_problem(tags, "v0.10.10-Public") is None
    assert cto.tag_order_problem(tags, "v0.10.9-Public") is not None


def test_test_tags_are_not_checked_and_do_not_count() -> None:
    """A `-fixtest` tag is never offered to players, so order says nothing about it."""
    assert cto.tag_order_problem([*HISTORY, "v0.1.0-fixtest"], "v0.1.0-fixtest") is None
    # ... and a fixtest tag far above does not block the next public release.
    assert (
        cto.tag_order_problem([*HISTORY, "v0.99.0-fixtest", "v0.9.17-Public"], "v0.9.17-Public")
        is None
    )


def test_a_public_tag_the_app_cannot_read_is_refused() -> None:
    """Ten digits: `version_key` refuses it, so the update check would drop the release."""
    tag = "v0.9.1234567890-Public"
    problem = cto.tag_order_problem([*HISTORY, tag], tag)
    assert problem is not None and "cannot" in problem


def test_main_exit_codes(tmp_path: Path, capsys: pytest.CaptureFixture[str]) -> None:
    def git_with(*extra: str):  # type: ignore[no-untyped-def]
        def run(argv: list[str]) -> str:
            assert argv[:2] == ["tag", "--list"]
            return "\n".join([*HISTORY, *extra]) + "\n"

        return run

    assert cto.main(["--tag", "v0.9.17-Public"], run_git=git_with("v0.9.17-Public")) == 0
    assert cto.main(["--tag", "v0.9.2-Public"], run_git=git_with("v0.9.2-Public")) == 1
    assert "v0.9.2-Public" in capsys.readouterr().err

    # a branch name, a dispatch run: nothing to check, and git is not even asked
    def never(argv: list[str]) -> str:
        raise AssertionError("git was asked about a ref that is not a public tag")

    assert cto.main(["--tag", "Yulon"], run_git=never) == 0
    assert cto.main(["--tag", "v0.8.66-fixtest"], run_git=never) == 0


def test_main_refuses_when_the_tags_cannot_be_listed(capsys: pytest.CaptureFixture[str]) -> None:
    """Failing open would let the tag through unchecked, which is the thing refused."""

    def broken(argv: list[str]) -> str:
        raise OSError("not a git repository")

    assert cto.main(["--tag", "v0.9.17-Public"], run_git=broken) == 1
    assert "not a git repository" in capsys.readouterr().err


WORKFLOW = (
    Path(__file__).resolve().parents[2] / ".github" / "workflows" / "release.yml"
).read_text(encoding="utf-8")


def test_the_release_workflow_runs_the_guard_before_it_builds() -> None:
    """The guard is only a guard if the build waits for it, and it needs every tag."""
    head, _, rest = WORKFLOW.partition("\n  tag-order:")
    assert rest, "release.yml has no tag-order job"
    job, _, build = rest.partition("\n  build:")
    assert build, "the build job no longer follows the guard"
    assert 'python build/check_tag_order.py --tag "${GITHUB_REF_NAME}"' in job
    assert "fetch-depth: 0" in job, "a shallow clone has no other tags to compare with"
    assert "needs: tag-order" in build.split("\n  notes-and-checksums:")[0]
