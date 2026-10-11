"""Every door that tears something down reaches the one predicate (T690).

The RELATIONSHIP half of `test_teardown_guard.py`, split out because it reads
source and needs no Qt. It finds every function that tears a tab, the window or
the app down (a `drop_controller()` call, a quit, a forced exit, a rebuild
signal) and fails for any that does not reach `ControllerView.teardown_work()`,
so a door added later without the guard fails here and not in a player's restore.

Mutation: drop the `teardown_refusal()` call from `_client_dir_busy()`, from
`close_refusal()`'s sweep, or from `forget_refusal()` -- the doors that lean on
them are listed as unguarded.
"""

from __future__ import annotations

import ast
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

SOURCES = (
    ROOT / "main.py",
    ROOT / "yulon" / "ui" / "tray.py",
    ROOT / "yulon" / "ui" / "controller_view.py",
    ROOT / "yulon" / "ui" / "catalog_view.py",
)

TEARDOWN_CALLS = {
    "drop_controller",  # the tab leaves the window
    "on_uninstalled",  # ...as a removal or an uninstall ends
    "_hard_exit",  # os._exit
    "_leave_with_jobs_still_running",
    "quit_app",  # the event loop ends
    "quit_for_good",
    "close_window",  # the self-update's close seam
    "yulon_quit",  # the tray's real quit
}
TEARDOWN_EMITS = {"client_dir_changed", "play_client_dir_changed", "uninstalled"}
THE_PREDICATE = "teardown_work"


def _functions() -> dict[str, list[tuple[Path, ast.AST]]]:
    found: dict[str, list[tuple[Path, ast.AST]]] = {}
    for path in SOURCES:
        for node in ast.walk(ast.parse(path.read_text(encoding="utf-8"))):
            if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
                found.setdefault(node.name, []).append((path, node))
    return found


def _own_nodes(function: ast.AST) -> list[ast.AST]:
    """The nodes of one function, not of the functions nested inside it."""
    out: list[ast.AST] = []
    todo = list(ast.iter_child_nodes(function))
    while todo:
        node = todo.pop()
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.Lambda)):
            continue
        out.append(node)
        todo.extend(ast.iter_child_nodes(node))
    return out


def _called(function: ast.AST) -> set[str]:
    """The names a function calls, plus the guards it looks up with `getattr(x, "name")`."""
    names: set[str] = set()
    for node in _own_nodes(function):
        if isinstance(node, ast.Call):
            callee = node.func
            names.add(callee.id if isinstance(callee, ast.Name) else getattr(callee, "attr", ""))
        elif isinstance(node, ast.Constant) and node.value in GUARD_LOOKUPS:
            names.add(str(node.value))
    return names


def _tears_down(function: ast.AST) -> set[str]:
    hit: set[str] = set()
    for node in _own_nodes(function):
        if not isinstance(node, ast.Call):
            continue
        callee = node.func
        name = callee.id if isinstance(callee, ast.Name) else getattr(callee, "attr", "")
        if name in TEARDOWN_CALLS:
            hit.add(name)
        if name == "quit" and ast.unparse(callee) == "self.quit":
            hit.add("self.quit")
        if name == "emit" and isinstance(callee, ast.Attribute):
            owner = getattr(callee.value, "attr", "")
            if owner in TEARDOWN_EMITS:
                hit.add(f"{owner}.emit")
        if name == "close" and isinstance(callee, ast.Attribute):
            if ast.unparse(callee.value) in {"self.window", "window"}:
                hit.add(WINDOW_CLOSE)
        if name == "exit" and ast.unparse(callee) == "QApplication.exit":
            hit.add("QApplication.exit")
    return hit


WINDOW_CLOSE = "window.close"
GUARD_LOOKUPS = {"teardown_refusal"}
"""`main._busy_reasons()` asks each tab by `getattr(view, "teardown_refusal")`."""
CLOSE_DOORS = {WINDOW_CLOSE, "close_window", "yulon_quit", "self.quit"}
"""A close of the window is answered by main's close filter, which `_close_filter_asks` checks."""


def _close_filter_asks(functions: dict[str, list[tuple[Path, ast.AST]]], guarded: set[str]) -> bool:
    """True when the window's close filter in main.py asks a guarded refusal."""
    return any(
        path.name == "main.py" and _called(node) & guarded & {"close_refusal"}
        for path, node in functions.get("eventFilter", [])
    )


def _guarded(functions: dict[str, list[tuple[Path, ast.AST]]]) -> set[str]:
    """Functions that reach `teardown_work()`, by name, through any chain of calls.

    Only a name defined ONCE counts as a link: `busy_reason`, `run` or `forget` are
    defined in several places, and a chain through one of them proves nothing.
    A function whose only teardown is a close of the window is as guarded as main's
    close filter is, which `_close_filter_asks()` checks once the filter's own chain
    is known.
    """
    guarded = _chain(functions, {THE_PREDICATE}, closes=False)
    if _close_filter_asks(functions, guarded):
        guarded = _chain(functions, guarded, closes=True)
    return guarded


def _chain(
    functions: dict[str, list[tuple[Path, ast.AST]]], seed: set[str], *, closes: bool
) -> set[str]:
    guarded = set(seed)
    changed = True
    while changed:
        changed = False
        for name, defs in functions.items():
            if name in guarded or len(defs) != 1:
                continue
            _path, node = defs[0]
            links = bool(_called(node) & guarded)
            if closes:
                links = links or bool(_tears_down(node) & CLOSE_DOORS)
            if links:
                guarded.add(name)
                changed = True
    return guarded


# Doors that are not guarded themselves, with why that is true. `via` names the
# guarded function that does the asking for them (and must be guarded); None means
# the reason stands alone. An entry whose function no longer tears anything down is
# stale and fails.
EXEMPT: dict[str, tuple[str, str | None]] = {
    "_end": (
        "runs only after `window.close()` was accepted by the close filter",
        "close_refusal",
    ),
    "quit_app": ("the tray's seam for ending the event loop; `_end` calls it", "close_refusal"),
    "_leave_with_jobs_still_running": (
        "the process end after the window's close was accepted and the jobs were joined",
        "close_refusal",
    ),
    "_finish_make": (
        "emits the rebuild signal; `on_play_client_dir_changed` refuses it while work runs",
        "rebuild_refused",
    ),
    "_play_client_deleted": (
        "emits the rebuild signal; `on_play_client_dir_changed` refuses it while work runs",
        "rebuild_refused",
    ),
    "finish_removal": (
        "called only by `request_removal` and `on_stopped_for_removal`, which ask "
        "`forget_refusal()` first",
        "forget_refusal",
    ),
    "on_uninstalled": (
        "an uninstall that has finished (or `finish_removal`) asks for the drop; "
        "the uninstall held the tab's lock for its whole run",
        None,
    ),
    "_uninstall_done": (
        "the uninstall's own end; the tab held its lock (`_busy`, `_uninstall_running`) "
        "for the whole run, so no other job began on it",
        None,
    ),
}


def _doors(functions: dict[str, list[tuple[Path, ast.AST]]]) -> dict[str, set[str]]:
    doors: dict[str, set[str]] = {}
    for name, defs in functions.items():
        for _path, node in defs:
            if (hit := _tears_down(node)) and name != "drop_controller":
                doors.setdefault(name, set()).update(hit)
    return doors


def test_every_function_that_tears_something_down_reaches_the_one_predicate() -> None:
    functions = _functions()
    guarded = _guarded(functions)
    close_ok = _close_filter_asks(functions, guarded)
    unguarded: dict[str, list[str]] = {}
    for name, hit in _doors(functions).items():
        if name in guarded or name in EXEMPT:
            continue
        if hit <= CLOSE_DOORS and close_ok:
            continue  # only closes the window; the close filter answers it
        unguarded[name] = sorted(hit)
    assert not unguarded, (
        "these functions tear a tab, the window or the app down without reaching "
        f"ControllerView.teardown_work(); guard them or list them in EXEMPT with why: {unguarded}"
    )


def _first_line(function: ast.AST, names: set[str]) -> int | None:
    lines = [
        node.lineno
        for node in _own_nodes(function)
        if isinstance(node, ast.Call)
        and (node.func.id if isinstance(node.func, ast.Name) else getattr(node.func, "attr", ""))
        in names
    ]
    return min(lines) if lines else None


def test_a_door_asks_before_it_tears_down_not_after() -> None:
    """Reaching the predicate somewhere is not enough: the ask must come BEFORE the drop.

    `on_client_dir_changed()` calls `add_controller()`, which is guarded, so a door that
    dropped the tab first and asked afterwards would still read as guarded by name.
    """
    functions = _functions()
    guarded = _guarded(functions) - {"drop_controller"}
    checked = 0
    for name, defs in functions.items():
        if name in EXEMPT or name not in guarded:
            continue
        for _path, node in defs:
            hits = _tears_down(node)
            if not hits & {"drop_controller"} and not any(h.endswith(".emit") for h in hits):
                continue
            tears = _first_line(node, {"drop_controller", "emit"})
            asks = _first_line(node, guarded - {name})
            assert (
                asks is not None and tears is not None and asks < tears
            ), f"{name} tears down (line {tears}) before it asks a guard (line {asks})"
            checked += 1
    assert checked >= 4, "the scan found too few doors that drop a tab or emit a rebuild"


def test_the_close_filter_asks_a_refusal_that_reaches_the_predicate() -> None:
    functions = _functions()
    guarded = _guarded(functions)
    assert "close_refusal" in guarded, "close_refusal() no longer reaches teardown_work()"
    assert "_busy_reasons" in guarded, "_busy_reasons() no longer asks each tab's teardown_refusal"
    assert _close_filter_asks(
        functions, guarded
    ), "main's close filter does not ask close_refusal()"


def test_the_exemptions_are_current_and_their_guards_are_real() -> None:
    functions = _functions()
    guarded = _guarded(functions)
    doors = _doors(functions)
    stale = sorted(set(EXEMPT) - set(doors))
    assert not stale, f"EXEMPT names functions that no longer tear anything down: {stale}"
    for name, (why, via) in EXEMPT.items():
        assert why, f"{name} is exempt without a reason"
        assert via is None or via in guarded, f"{name} says {via} guards it, and {via} is not"


def test_the_scan_sees_the_doors_this_ticket_names() -> None:
    """An empty scan would pass the tests above, so the doors it must find are named."""
    doors = _doors(_functions())
    for expected in (
        "on_client_dir_changed",
        "on_play_client_dir_changed",
        "add_controller",
        "change_client_dir",
        "forget_client_dir",
        "finish_removal",
        "_try_to_restart",
        "quit",
        "ask_to_quit",
    ):
        assert expected in doors, f"the scan no longer sees {expected}"
