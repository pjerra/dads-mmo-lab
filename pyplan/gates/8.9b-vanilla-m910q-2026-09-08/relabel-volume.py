"""Fix a defect in this gate's own SUBJECT, found by step 5 (`plan`).

`setup89b.py` created the clone's database volume with `docker volume create`,
which produces a volume carrying NO labels. The original's, created by compose,
carries four -- among them `com.docker.compose.project`. `purge.Uninstaller`
enumerates volumes by that label (`docker.project_volumes()`), never by name, and
that is the RIGHT behaviour: `docker volume rm` exits non-zero on a name that
does not exist, and the volume sets differ by family
(`pyplan/8.9b-gate-plan.md` defect 4).

So the plan printed **"volumes removed: none"** while `docker volume ls` showed
the volume plainly. Every later step would have "passed" without the purge ever
having seen it -- a subject built wrong, in exactly the dimension the box
measures. Recorded here rather than quietly corrected, because it is a worked
example of the rule the plan opens with: an assertion read off the wrong listing
answers the same before and after.

The fix is to let compose create the volume, so its labels are compose's own,
and copy the data in afterwards:

    down (keep the volume)  ->  volume rm  ->  compose create (labelled, empty)
    ->  copy the original's bytes in  ->  up

Usage:  ~/gate81b-venv/bin/python relabel-volume.py
"""

from __future__ import annotations

import subprocess
import sys
import time
from pathlib import Path

GAME = "wow-vanilla"
CLONE = Path.home() / "gate89b" / "server"
ORIGINAL = Path.home() / "vanilla-75b"


def say(message: str) -> None:
    print(f"[{time.strftime('%H:%M:%SZ', time.gmtime())}] {message}", flush=True)


def sh(*args: str, check: bool = True) -> str:
    done = subprocess.run(args, capture_output=True, text=True, check=False)
    if check and done.returncode != 0:
        raise SystemExit(f"{' '.join(args)} exited {done.returncode}: {done.stderr.strip()}")
    return done.stdout.strip()


def compose(*args: str, check: bool = True) -> str:
    return sh(
        "docker", "compose",
        "-f", str(CLONE / "docker-compose.yml"),
        "-f", str(CLONE / "docker-compose.override.yml"),
        *args, check=check,
    )


def main() -> None:
    from yulon.catalog import composegen

    clone_project = composegen.project_name(GAME, CLONE)
    clone_volume = f"{clone_project}_db-data"
    source_volume = f"{composegen.project_name(GAME, ORIGINAL)}_db-data"

    before = sh("docker", "volume", "inspect", clone_volume, "--format", "{{json .Labels}}")
    say(f"ground: {clone_volume} labels = {before}")
    assert before in ("null", "{}"), f"already labelled ({before}); this step would do nothing"

    say("down (containers and network only)")
    compose("down", check=False)
    say(f"removing the unlabelled {clone_volume}")
    sh("docker", "volume", "rm", clone_volume)
    say("compose create -- this is what puts the labels on")
    compose("create")
    after = sh("docker", "volume", "inspect", clone_volume, "--format", "{{json .Labels}}")
    say(f"now: {clone_volume} labels = {after}")
    assert clone_project in after, after

    say(f"copying {source_volume} into it")
    sh("docker", "run", "--rm",
       "-v", f"{source_volume}:/from:ro", "-v", f"{clone_volume}:/to",
       "alpine:3.20", "sh", "-c", "rm -rf /to/* /to/.[!.]* 2>/dev/null; cp -a /from/. /to/")

    say("up")
    compose("up", "-d")
    mark = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(time.time() - 5))
    for _ in range(360):
        if "Avg Diff:" in sh("docker", "logs", "--since", mark, "vanilla-mangosd", check=False):
            say("the subject reached `Avg Diff:` again")
            break
        time.sleep(5)
    else:
        raise SystemExit("the subject did not come back up")
    say(sh("docker", "ps", "--format", "{{.Names}} {{.Status}}"))
    say("SUBJECT FIXED -- rerun `gate89b.py baseline` before going on")


if __name__ == "__main__":
    sys.exit(main())
