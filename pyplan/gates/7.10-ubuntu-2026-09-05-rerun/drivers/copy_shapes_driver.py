"""The two 2026-09-05 cancel-copy findings, answered on the folder shape that produced them.

WHAT THIS IS NOT. It is not a cancelled install driven through a widget. That could not be
run on this box: `/home/pk/wowserver` holds the 7.2 WotLK install, and with its `ac-*`
containers stopped through the app -- ports free, every preflight check `[pass]` -- the engine
still refused the second `wow-wotlk` install on the container-name guard,
"A container called ac-database already exists and belongs to another install
(yulon-wow-wotlk-243c46e3)" (`widget-cancel-wotlk-refused.log`). The app's own remedy is to
remove that install's containers; this lane may not. And `wow-wotlk` is the only shipped game
whose source clones into the server dir itself (`dest: "."`), which is the whole input to the
reading these two findings are about -- so no other game reproduces the shape.

WHAT THIS IS. The folder state the 2026-09-05 cancel left is on record, field by field, in
`pyplan/gates/7.2-ubuntu-2026-09-05/widget-cancel-folder-after.txt`: no `.yulon-install.json`,
a `.git`, and a `docker-compose.yml` that `git status --porcelain` reported as tracked and
unmodified -- upstream's own. This driver BUILDS that folder from the same upstream repository
(a depth-1 clone, which is the only difference and it is not one the copy can read), asserts
each of those fields, and then renders `cancelled_install_message()` for it and for the same
`wow-tbc` folder shape.

That is enough to answer the findings only because the widget half is proved separately: the
`wow-tbc` run in `widget-cancel-tbc.log` compares the modal a real cancel showed with
`cancelled_install_message()` for its folder AS STRINGS. So what that function answers is what
a user reads; this driver asks it the 2026-09-05 question.

The 2026-09-05 modal text is embedded below verbatim from
`pyplan/gates/7.2-ubuntu-2026-09-05/widget-cancel.log:44`, so the two can be diffed here
rather than by memory.
"""

from __future__ import annotations

import shutil
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, "/home/pk/p7/checkout/pylauncher")

from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.catalog.installer import cancelled_install_message  # noqa: E402
from yulon.catalog.installer import compose_file, generated_compose_files  # noqa: E402

SHAPE = Path("/home/pk/p7-wotlk-cancel-shape")
UPSTREAM = "https://github.com/mod-playerbots/azerothcore-wotlk"

CHECKOUT = Path("/home/pk/p7/checkout")
LOG_0905 = CHECKOUT / "pyplan/gates/7.2-ubuntu-2026-09-05/widget-cancel.log"


def modal_of_2026_09_05() -> str:
    """The 2026-09-05 modal, READ OUT of the committed log rather than retyped here.

    A transcription is a second copy of a fact and the two can drift; this one is
    lifted from the file the checkout carries, so the comparison below is against
    what that run actually recorded. The line is
    `<stamp>        [modal] Stop was pressed, ...` -- one line, because the copy is
    one paragraph.
    """
    for line in LOG_0905.read_text(encoding="utf-8").splitlines():
        marker = "[modal] Stop was pressed,"
        if marker in line:
            return line[line.index(marker) + len("[modal] "):]
    raise SystemExit(f"no modal line in {LOG_0905}")


# Kept for the record: this is what the line above is expected to say. It is compared
# with the file rather than used in its place.
MODAL_TRANSCRIBED = (
    "Stop was pressed, so WoW WotLK has NOT been remembered as an install and the app will "
    "not show a tab for it. Stopping undoes nothing and tidies nothing away — look in "
    "/home/pk/gate72-cancel-install to see what the installer had got to (a download it was "
    "in the middle of may have removed its own leftovers; anything already finished stays). "
    "If the build had started, Docker keeps finishing the step it was on in the background — "
    "that is deliberate, and the finished pieces are what make a second attempt much faster, "
    "so do not clear Docker's build cache to tidy up. The source is there. If the build had "
    "already finished, the server may be built and even running: press \"Use existing…\", "
    "choose /home/pk/gate72-cancel-install, and the app will manage it from a tab — nothing "
    "is lost. If it had not, press Install again and choose /home/pk/gate72-cancel-install: "
    "the installer carries on from the last stage recorded in "
    "/home/pk/gate72-cancel-install/.yulon-install.json, and a stage is only skipped after "
    "what it left on disk has been checked."
)

MODAL_2026_09_05 = ""  # filled from the committed log at the top of main()

PASSES = 0
FAILS = 0


def say(text: str = "") -> None:
    print(text, flush=True)


def check(label: str, ok: bool, detail: str = "") -> None:
    global PASSES, FAILS
    if ok:
        PASSES += 1
        say(f"[OK]   {label}" + (f" -- {detail}" if detail else ""))
    else:
        FAILS += 1
        say(f"[FAIL] {label}" + (f" -- {detail}" if detail else ""))


def sh(argv: list[str]) -> str:
    done = subprocess.run(argv, capture_output=True, text=True)
    return (done.stdout + done.stderr).strip()


def main() -> int:
    entry = load_catalog().get("wow-wotlk")
    tbc = load_catalog().get("wow-tbc")
    global MODAL_2026_09_05
    MODAL_2026_09_05 = modal_of_2026_09_05()
    check("the 2026-09-05 modal read out of the committed log matches the transcription "
          "in this driver", MODAL_2026_09_05 == MODAL_TRANSCRIBED,
          f"{len(MODAL_2026_09_05)} chars from {LOG_0905}")

    say("=" * 78)
    say("build the folder a cancelled wow-wotlk clone leaves, then ask the copy about it")
    say("=" * 78)
    if SHAPE.exists():
        shutil.rmtree(SHAPE)
    say(f"       git clone --depth 1 {UPSTREAM} {SHAPE}")
    say(sh(["git", "clone", "--depth", "1", "--quiet", UPSTREAM, str(SHAPE)]) or "       (quiet)")

    say("")
    say("-- the folder, field by field, against 7.2-ubuntu-2026-09-05/widget-cancel-folder-after.txt")
    state = SHAPE / ".yulon-install.json"
    say(f"       .yulon-install.json    : {state.is_file()}")
    say(f"       .git                   : {(SHAPE / '.git').is_dir()}")
    say(f"       docker-compose.yml     : {(SHAPE / 'docker-compose.yml').is_file()}")
    say(f"       git ls-files it        : {sh(['git', '-C', str(SHAPE), 'ls-files', 'docker-compose.yml'])!r}")
    say(f"       git status --porcelain : {sh(['git', '-C', str(SHAPE), 'status', '--porcelain', 'docker-compose.yml'])!r}")
    say(f"       generated_compose_files: {generated_compose_files(SHAPE)}")
    say(f"       compose_file()         : {compose_file(SHAPE)}")
    check("no .yulon-install.json -- the record clone-core removes and never puts back",
          not state.is_file())
    check("a .git at the server dir root", (SHAPE / ".git").is_dir())
    check("upstream's docker-compose.yml is there, git-tracked and unmodified",
          sh(["git", "-C", str(SHAPE), "ls-files", "docker-compose.yml"]) == "docker-compose.yml"
          and sh(["git", "-C", str(SHAPE), "status", "--porcelain", "docker-compose.yml"]) == "")
    check("this app wrote no compose file here", generated_compose_files(SHAPE) == ())
    check("and `compose_file()` -- what `attach_existing()` gates on -- still answers",
          compose_file(SHAPE) is not None, f"{compose_file(SHAPE)}")

    say("")
    say("-- what the copy said on 2026-09-05 (widget-cancel.log:44), for the same shape")
    say(f"         {MODAL_2026_09_05}")
    say("")
    say("-- what the copy on cfb4c04f says for this folder")
    now = cancelled_install_message(entry, SHAPE)
    say(f"         {now}")

    say("")
    say("-- finding 1: the compose-file reading fired on upstream's own file")
    check("2026-09-05 said 'nothing is lost' about that folder", "nothing is lost" in MODAL_2026_09_05)
    check("2026-09-05 offered adoption ('Use existing…')", "Use existing" in MODAL_2026_09_05)
    check("cfb4c04f does NOT say 'nothing is lost' about it", "nothing is lost" not in now)
    check("cfb4c04f does not claim the file came down with the source as if it knew",
          "came down with the server's source: " not in now)
    check("cfb4c04f puts BOTH readings of that file to the user",
          "If it was already there before this attempt" in now
          and "If this attempt was downloading into an empty folder" in now)

    say("")
    say("-- finding 2: the resume promised over a folder with no record")
    check("2026-09-05 promised the resume", "the installer carries on from the last stage" in MODAL_2026_09_05)
    check("cfb4c04f does not promise it", "the installer carries on from the last stage" not in now)
    check("cfb4c04f warns the next press will be REFUSED, which is what the engine did "
          "(7.2-ubuntu-2026-09-05/cycle2-pressA2-refused-existing-checkout.log:31)",
          "Do not press Install again on this folder" in now and "the app will refuse it" in now)
    check("and it names the refusal the engine actually raises on a checkout, not the "
          "not-empty one", "it holds a git checkout" in now)

    say("")
    say("-- the same function on the wow-tbc folder the widget run cancelled, for contrast")
    tbc_dir = Path("/home/pk/p7-cancel-install-tbc")
    if tbc_dir.is_dir():
        say(f"         {cancelled_install_message(tbc, tbc_dir)}")
    else:
        say(f"       ({tbc_dir} is gone -- the widget run removed it; see widget-cancel-tbc.log)")

    shutil.rmtree(SHAPE)
    check("the throwaway shape folder was removed", not SHAPE.exists())
    say("")
    say("=" * 78)
    say(f"RESULT: {PASSES} OK, {FAILS} FAIL")
    say("=" * 78)
    return 0 if FAILS == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
