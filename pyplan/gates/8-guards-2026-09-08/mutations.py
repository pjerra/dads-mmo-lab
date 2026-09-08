"""Drives every mutation behind the two guards of 2026-09-08 and records what it saw.

Run from `pylauncher/`:  python ../pyplan/gates/8-guards-2026-09-08/mutations.py

Each mutation is a triple and the triple is the point: read the GROUND (the
suite green, the file's sha256 recorded), apply the break, purge `__pycache__`
on both sides, run, then restore and prove the restore by comparing the bytes
back to that hash. A step whose assertion was already true before its action
proves nothing, so the ground is recorded rather than assumed -- and
`mutation-testing pycache trap` is why the purge is on both sides: a same-length
edit reuses stale bytecode and the evidence is worthless.

The script refuses to start on a dirty `controller_view.py` or `catalog.json`,
because a mutation applied on top of somebody else's edit restores to the wrong
thing. That check is `git diff`, so it cannot speak for
`tests/catalog_provenance.py` while that file is still untracked -- which is
exactly why the RESTORE is a byte comparison and not a `git checkout`.
"""

from __future__ import annotations

import hashlib
import shutil
import subprocess
import sys
from datetime import UTC, datetime
from pathlib import Path

PYLAUNCHER = Path(__file__).resolve().parents[3] / "pylauncher"
CONTROLLER = PYLAUNCHER / "yulon" / "ui" / "controller_view.py"
CATALOG = PYLAUNCHER / "yulon" / "catalog" / "catalog.json"
TABLE = PYLAUNCHER / "tests" / "catalog_provenance.py"
JOIN = "            image_refs=composegen.built_image_refs(entry, server_dir),"

ROW_TO_DELETE = """    "wow-tortoise:accounts.level.max_level": Provenance(
        "measured-on",
        "gates/8.3d-tortoise-m910q-2026-09-07/README.md",
        note="asked: `... SHAPROBE 4` accepted, `5` answered `Incorrect values.`",
    ),
"""
"""One row of the table, deleted whole by mutation C.

The tortoise ceiling on purpose: it is the one debt-shaped value that ISN'T a
debt, because 8.3d actually asked (`SHAPROBE 4` accepted, `5` refused). Deleting
a row that was really earned is a better test of the guard than deleting one
nobody would miss.
"""

CITE_TO_BREAK = """    "wow-vanilla:play.revive_offline": Provenance(
        "measured-on",
        "gates/8.4c-vanilla-m910q-2026-09-07/README.md","""
"""Anchored on the whole three lines, not on the folder name.

Six vanilla rows cite that same README, so a bare string replace would either
hit all six or refuse as ambiguous -- and a mutation that changes six things at
once cannot say which one the red came from.
"""


def purge_pycache() -> int:
    gone = 0
    for cache in PYLAUNCHER.rglob("__pycache__"):
        shutil.rmtree(cache, ignore_errors=True)
        gone += 1
    return gone


def git(*args: str) -> str:
    return subprocess.run(
        ["git", *args], cwd=PYLAUNCHER, capture_output=True, text=True, check=True
    ).stdout


def pytest(*targets: str) -> tuple[int, str]:
    proc = subprocess.run(
        [sys.executable, "-m", "pytest", "-m", "not integration", "-q", *targets],
        cwd=PYLAUNCHER,
        capture_output=True,
        text=True,
    )
    return proc.returncode, proc.stdout + proc.stderr


def verdict(out: str) -> str:
    line = [ln for ln in out.splitlines() if " passed" in ln or " failed" in ln]
    return line[-1].strip() if line else "(no summary line)"


def reasons(out: str) -> list[str]:
    return [ln.strip()[:200] for ln in out.splitlines() if ln.startswith("E ") or "E  " in ln][:3]


def clean(path: Path) -> None:
    diff = git("diff", "--", str(path.relative_to(PYLAUNCHER)))
    if diff:
        raise SystemExit(f"REFUSING TO START: {path} is already modified")


class Log:
    def __init__(self) -> None:
        self.lines: list[str] = []

    def __call__(self, text: str = "") -> None:
        print(text)
        self.lines.append(text)


def mutate_line(path: Path, index0: int, expect: str, replace: str) -> None:
    lines = path.read_text(encoding="utf-8").split("\n")
    if lines[index0] != expect:
        raise SystemExit(f"anchor moved in {path.name}:{index0 + 1}: {lines[index0]!r}")
    lines[index0] = replace
    path.write_text("\n".join(lines), encoding="utf-8")


def drop_line(path: Path, index0: int, expect: str) -> None:
    lines = path.read_text(encoding="utf-8").split("\n")
    if lines[index0] != expect:
        raise SystemExit(f"anchor moved in {path.name}:{index0 + 1}: {lines[index0]!r}")
    del lines[index0]
    path.write_text("\n".join(lines), encoding="utf-8")


def sub(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    if text.count(old) != 1:
        raise SystemExit(f"anchor appears {text.count(old)} times in {path.name}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


def press(log: Log, name: str, what: str, apply, targets: tuple[str, ...], touched: Path) -> bool:
    """Ground, break, read, restore -- and prove the restore by the file's own bytes.

    Restored from a SNAPSHOT rather than by `git checkout`, which was the first
    version and could not work: `tests/catalog_provenance.py` is new in this
    commit, so it is untracked while the driver runs and `git checkout --` on it
    exits 1. The snapshot is also the stronger claim of the two -- it compares
    the bytes rather than asking git whether it minds them -- so it is what the
    other two files get as well, with the git answer printed beside it where
    there is one to print.
    """
    log(f"--- {name} ---")
    log(f"    breaks: {what}")
    before = touched.read_bytes()
    log(f"    before: sha256 {hashlib.sha256(before).hexdigest()[:16]}  {touched.name}")
    purge_pycache()
    apply()
    purge_pycache()
    code, out = pytest(*targets)
    log(f"    RED?    {code != 0}   {verdict(out)}")
    for line in reasons(out):
        log(f"      {line}")
    touched.write_bytes(before)
    purge_pycache()
    after = touched.read_bytes()
    restored = after == before
    log(f"    after:  sha256 {hashlib.sha256(after).hexdigest()[:16]}  identical={restored}")
    log()
    return code != 0 and restored


def main() -> int:
    log = Log()
    for path in (CONTROLLER, CATALOG, TABLE):
        clean(path)
    head = git("rev-parse", "HEAD").strip()
    log(f"# mutations for the two guards of 2026-09-08 -- HEAD {head[:8]}")
    log(f"# run {datetime.now(UTC).isoformat(timespec='seconds')}")
    log(f"# python {sys.version.split()[0]} on {sys.platform}")
    log()

    log("--- GROUND: the two files under test, and the suite, before anything is broken ---")
    purge_pycache()
    code, out = pytest("tests/test_purge.py", "tests/test_catalog_invariants.py")
    log(f"    green?  {code == 0}   {verdict(out)}")
    for path in (CONTROLLER, CATALOG, TABLE):
        log(
            f"    {path.name}: git diff empty = {git('diff', '--', str(path.relative_to(PYLAUNCHER))) == ''}"
        )
    log()
    if code != 0:
        log("GROUND IS NOT GREEN -- nothing below means anything.")
        (Path(__file__).with_name("mutations.txt")).write_text(
            "\n".join(log.lines) + "\n", encoding="utf-8"
        )
        return 1

    source = CONTROLLER.read_text(encoding="utf-8").split("\n")
    wotlk = source.index(JOIN)
    vanilla = source.index(JOIN, wotlk + 1)
    log(
        f"    the two joins live at controller_view.py:{wotlk + 1} (WotLK) "
        f"and :{vanilla + 1} (Vanilla)"
    )
    log()

    catalog_lines = CATALOG.read_text(encoding="utf-8").split("\n")
    tbc_level = next(
        i
        for i, ln in enumerate(catalog_lines)
        if ln.strip() == '"level": { "level_column": "gmlevel", "max_level": 3 },'
    )
    revive = next(
        i for i, ln in enumerate(catalog_lines) if ln.strip() == '"revive_offline": true,'
    )

    purge_targets = ("tests/test_purge.py",)
    catalog_targets = ("tests/test_catalog_invariants.py",)
    results = []

    results.append(
        press(
            log,
            "guard 4, mutation 1",
            "the WotLK join -> image_refs=() (the exact gap; at dcc64543 the whole suite "
            "stayed byte-identical under this)",
            lambda: mutate_line(CONTROLLER, wotlk, JOIN, "            image_refs=(),"),
            purge_targets,
            CONTROLLER,
        )
    )
    results.append(
        press(
            log,
            "guard 4, mutation 2",
            "the WotLK join also hands the PULLED database image",
            lambda: mutate_line(
                CONTROLLER,
                wotlk,
                JOIN,
                "            image_refs=composegen.built_image_refs(entry, server_dir)"
                ' + ("mysql:8.4",),',
            ),
            purge_targets,
            CONTROLLER,
        )
    )
    results.append(
        press(
            log,
            "guard 4, mutation 3",
            "the VANILLA join -> image_refs=() (proves the guard enumerates rather than "
            "naming one family)",
            lambda: mutate_line(CONTROLLER, vanilla, JOIN, "            image_refs=(),"),
            purge_targets,
            CONTROLLER,
        )
    )
    results.append(
        press(
            log,
            "guard 3, mutation A",
            "a new value in catalog.json with no provenance row",
            lambda: mutate_line(
                CATALOG,
                tbc_level,
                catalog_lines[tbc_level],
                '        "level": { "account_column": "id", "level_column": "gmlevel", '
                '"max_level": 3 },',
            ),
            catalog_targets,
            CATALOG,
        )
    )
    results.append(
        press(
            log,
            "guard 3, mutation B",
            "a value deleted from catalog.json while its row stays",
            lambda: drop_line(CATALOG, revive, catalog_lines[revive]),
            catalog_targets,
            CATALOG,
        )
    )
    results.append(
        press(
            log,
            "guard 3, mutation C",
            "a PROVENANCE row deleted (the tortoise ceiling, which 8.3d really did ask for)",
            lambda: sub(TABLE, ROW_TO_DELETE, ""),
            catalog_targets,
            TABLE,
        )
    )
    results.append(
        press(
            log,
            "guard 3, mutation D",
            "one measured-on citation pointed one day earlier, at a folder that does not exist",
            lambda: sub(TABLE, CITE_TO_BREAK, CITE_TO_BREAK.replace("09-07", "09-06")),
            catalog_targets,
            TABLE,
        )
    )
    results.append(
        press(
            log,
            "guard 3, mutation E",
            "a debt struck from OWED with nothing measured to discharge it",
            lambda: sub(
                TABLE,
                '    "wow-tbc:accounts.level.max_level": (',
                '    "wow-tbc-STRUCK:accounts.level.max_level": (',
            ),
            catalog_targets,
            TABLE,
        )
    )

    log("--- AFTER: everything restored, the ground read again ---")
    purge_pycache()
    code, out = pytest("tests/test_purge.py", "tests/test_catalog_invariants.py")
    log(f"    green?  {code == 0}   {verdict(out)}")
    log(
        f"    git diff over yulon/ and tests/catalog_provenance.py empty: "
        f"{git('diff', '--', 'yulon', 'tests/catalog_provenance.py') == ''}"
    )
    log()
    log(f"# {sum(results)} of {len(results)} mutations went red and restored clean.")

    out_path = Path(__file__).with_name("mutations.txt")
    out_path.write_text("\n".join(log.lines) + "\n", encoding="utf-8")
    print(f"\nwritten to {out_path}")
    return 0 if all(results) and code == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
