"""Finds every place `yulon/` writes, and reads the ledger that must list them.

Test support, not shipped code — it lives beside `support_compose.py` and
`support_native.py` for the same reason: it is machinery a test needs and
nothing in the app should be able to import.

**What counts as a write, and why the list is shaped like this.** Three of the
call spellings here were chosen against a specific way of being wrong:

* `.open(...)` is checked by MODE, because `Path.open("wb")` is how the log
  snapshot writes and a walk that only knew the builtin `open()` would miss it.
* `os.replace` is matched only when it is qualified by `os`, because bare
  `.replace(` in this package is `str.replace` four times out of five and a
  ledger full of string operations is a ledger nobody reads.
* `run_statement`/`run_file` are the SQL seam's write half. `query()` is
  deliberately absent: it is the read half, and `dbreads` is allowed to call it.

`os.open` counts without its flags being inspected. It is how a file gets a
private mode at creation time, and over-inclusive is the safe direction for a
ledger: a read that gets a row costs a line, while a write that gets none costs
the whole guarantee. It was missing until 2026-09-07, and what found it was this
test's OTHER direction — a hand-written row that resolved to no site.

`mkdir` is deliberately NOT a write here. Creating an empty directory puts no
content anywhere, there are twenty-odd of them, and content still needs one of
the calls above — so nothing can hide behind one. Named rather than omitted.
"""

from __future__ import annotations

import ast
from dataclasses import dataclass
from pathlib import Path

_PATH_METHODS = {
    "write_text",
    "write_bytes",
    "unlink",
    "touch",
    "rmdir",
    "symlink_to",
    "chmod",
}
_SQL_WRITE_METHODS = {"run_statement", "run_file"}
_QUALIFIED = {
    ("os", "open"),
    ("os", "replace"),
    ("os", "rename"),
    ("os", "remove"),
    ("os", "unlink"),
    ("os", "rmdir"),
    ("os", "chmod"),
    ("os", "truncate"),
    ("shutil", "rmtree"),
    ("shutil", "copy"),
    ("shutil", "copy2"),
    ("shutil", "copyfile"),
    ("shutil", "copytree"),
    ("shutil", "move"),
    ("shutil", "unpack_archive"),
}
_WRITING_MODE = set("wax+")


@dataclass(frozen=True)
class WriteSite:
    """One call that can change something outside this process."""

    module: str
    function: str
    callee: str
    line: int

    @property
    def key(self) -> str:
        """`module::function::callee` — the ledger's key.

        Not the line number, on purpose: a ledger keyed to lines is rewritten by
        every edit above it, and a table that churns is a table people stop
        reading. Two writes of the same kind in one function are one row.
        """
        return f"{self.module}::{self.function}::{self.callee}"


@dataclass(frozen=True)
class LedgerRow:
    site: str
    writes: str
    running: str


def _mode_of(node: ast.Call) -> str:
    """The mode string an `open()` call was given, empty if it was not a constant."""
    for keyword in node.keywords:
        if keyword.arg == "mode" and isinstance(keyword.value, ast.Constant):
            return str(keyword.value.value)
    # `open(path, "w")` carries the mode second; `path.open("w")` carries it
    # first, because the path is the receiver rather than an argument.
    position = 0 if isinstance(node.func, ast.Attribute) else 1
    if len(node.args) > position and isinstance(node.args[position], ast.Constant):
        return str(node.args[position].value)
    return ""


def _callee(node: ast.Call) -> str | None:
    """What this call writes with, or `None` if it writes nothing."""
    func = node.func
    if isinstance(func, ast.Attribute):
        owner = func.value.id if isinstance(func.value, ast.Name) else None
        if (owner, func.attr) in _QUALIFIED:
            return f"{owner}.{func.attr}"
        if func.attr in _PATH_METHODS or func.attr in _SQL_WRITE_METHODS:
            return func.attr
        if func.attr == "open":
            mode = _mode_of(node)
            return f"open({mode})" if set(mode) & _WRITING_MODE else None
        if func.attr == "replace" and len(node.args) == 1 and not node.keywords:
            # `tmp.replace(target)` is `Path.replace` — an atomic rename over
            # whatever was there, and how `state.py` and `manifest_store.py`
            # save. Arity is what separates it from the two calls it is spelt
            # like: `str.replace` needs two arguments and
            # `datetime.replace(tzinfo=…)` passes none positionally. The first
            # version of this walker excluded every bare `.replace(` to keep
            # string operations out of the ledger, and silently lost seven real
            # writes with them.
            return "replace"
        return None
    if isinstance(func, ast.Name) and func.id == "open":
        mode = _mode_of(node)
        return f"open({mode})" if set(mode) & _WRITING_MODE else None
    return None


def walk_tree(tree: ast.AST, *, module: str) -> list[WriteSite]:
    """Every write in one parsed module, attributed to the function it sits in."""
    holder: dict[int, str] = {}
    for parent in ast.walk(tree):
        if isinstance(parent, ast.FunctionDef | ast.AsyncFunctionDef):
            for line in range(parent.lineno, (parent.end_lineno or parent.lineno) + 1):
                # The innermost function wins: a walk that took the outermost
                # would collapse two writes in one module onto one row.
                holder[line] = parent.name
    found: list[WriteSite] = []
    for node in ast.walk(tree):
        if not isinstance(node, ast.Call):
            continue
        callee = _callee(node)
        if callee is None:
            continue
        found.append(WriteSite(module, holder.get(node.lineno, "<module>"), callee, node.lineno))
    return found


def find_write_sites(package: Path) -> list[WriteSite]:
    """Every write in every module under `package`, sorted by key."""
    found: list[WriteSite] = []
    for path in sorted(package.rglob("*.py")):
        module = path.relative_to(package).as_posix()
        found += walk_tree(ast.parse(path.read_text(encoding="utf-8")), module=module)
    return sorted(found, key=lambda site: (site.key, site.line))


def parse_ledger(path: Path) -> list[LedgerRow]:
    """Read the pipe table out of `write-ledger.md`.

    Only rows whose first cell is a backticked `module::function::callee` are
    taken, so the page can carry as much prose and as many other tables as it
    needs without either of them becoming load-bearing by accident.
    """
    rows: list[LedgerRow] = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line.startswith("| `"):
            continue
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        if len(cells) < 3:
            continue
        site = cells[0].strip("`")
        if "::" not in site:
            continue
        rows.append(LedgerRow(site=site, writes=cells[1], running=cells[2]))
    return rows
