"""Every door that tears something down reaches the one predicate (T690).

The RELATIONSHIP half of `test_teardown_guard.py`, split out because it reads
source and needs no Qt. It finds every function that tears a tab, the window or
the app down (a `drop_controller()` call, a quit, a forced exit, a rebuild
signal) and fails for any that does not reach `ControllerView.teardown_work()`,
so a door added later without the guard fails here and not in a player's restore.

A door counts as guarded only when it CALLS a guard before it tears down and OBEYS
the answer (an `if` on it that returns or raises). Mutations (each must turn a test
here RED): `refusal = None` in `request_removal()`; `add_controller()` asking
`busy_reason()`; `_client_dir_busy()` not asking the predicate; the exec in
`offer_a_docker_group_restart()` made before its refusal.
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
    "finish_removal",  # ...as a removal is carried out
    "_hard_exit",  # os._exit
    "_leave_with_jobs_still_running",
    "quit_app",  # the event loop ends
    "close_window",  # the self-update's close seam
    "yulon_quit",  # the tray's real quit
    "quit_for_good",
    "execv",  # the process is replaced
    "restart_under_docker_group",  # ...by the docker-group restart
}
TEARDOWN_EMITS = {"client_dir_changed", "play_client_dir_changed", "uninstalled"}
THE_PREDICATE = "teardown_work"
WINDOW_CLOSE = "window.close"
CLOSE_DOORS = {WINDOW_CLOSE, "close_window", "yulon_quit", "self.quit", "quit_for_good"}
"""A close of the window is answered by main's close filter, which `_close_filter_obeys` checks."""
GUARD_LOOKUPS = {"teardown_refusal", "yulon_close_refusal"}
"""Guards looked up by name, as `getattr(view, "teardown_refusal")` does."""

Functions = dict[str, list[tuple[Path, ast.AST]]]


def _functions() -> Functions:
    found: Functions = {}
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


def _callee_name(call: ast.Call) -> str:
    callee = call.func
    return callee.id if isinstance(callee, ast.Name) else getattr(callee, "attr", "")


def _called(function: ast.AST) -> set[str]:
    """The names a function calls, plus the guards it looks up with `getattr(x, "name")`."""
    names: set[str] = set()
    for node in _own_nodes(function):
        if isinstance(node, ast.Call):
            names.add(_callee_name(node))
        elif isinstance(node, ast.Constant) and isinstance(node.value, str):
            if node.value in GUARD_LOOKUPS:
                names.add(node.value)
    return names


def _sites(function: ast.AST) -> list[tuple[int, str]]:
    """(line, what) for every place the function tears something down."""
    sites: list[tuple[int, str]] = []
    for node in _own_nodes(function):
        if not isinstance(node, ast.Call):
            continue
        callee = node.func
        name = _callee_name(node)
        if name in TEARDOWN_CALLS:
            sites.append((node.lineno, name))
        if name == "quit" and ast.unparse(callee) == "self.quit":
            sites.append((node.lineno, "self.quit"))
        if name == "emit" and isinstance(callee, ast.Attribute):
            owner = getattr(callee.value, "attr", "")
            if owner in TEARDOWN_EMITS:
                sites.append((node.lineno, f"{owner}.emit"))
        if name == "close" and isinstance(callee, ast.Attribute):
            if ast.unparse(callee.value) in {"self.window", "window"}:
                sites.append((node.lineno, WINDOW_CLOSE))
        if name == "exit" and ast.unparse(callee) == "QApplication.exit":
            sites.append((node.lineno, "QApplication.exit"))
    return sites


def _tears_down(function: ast.AST) -> set[str]:
    return {what for _line, what in _sites(function)}


def _chain(functions: Functions, seed: set[str]) -> set[str]:
    """Functions that reach a seed by calls, through names defined ONCE only.

    `busy_reason`, `run` or `forget` are defined in several places, and a chain
    through one of them proves nothing.
    """
    guarded = set(seed)
    changed = True
    while changed:
        changed = False
        for name, defs in functions.items():
            if name in guarded or len(defs) != 1:
                continue
            if _called(defs[0][1]) & guarded:
                guarded.add(name)
                changed = True
    return guarded


def _window_seam_is_the_close_refusal(functions: Functions, guarded: set[str]) -> bool:
    """main.py hands the window `yulon_close_refusal` = a call of the guarded close_refusal."""
    tree = ast.parse((ROOT / "main.py").read_text(encoding="utf-8"))
    for node in ast.walk(tree):
        if isinstance(node, ast.Assign) and any(
            isinstance(t, ast.Attribute) and t.attr == "yulon_close_refusal" for t in node.targets
        ):
            calls = {_callee_name(c) for c in ast.walk(node.value) if isinstance(c, ast.Call)}
            return "close_refusal" in calls and "close_refusal" in guarded
    return False


def _guarded(functions: Functions) -> set[str]:
    guarded = _chain(functions, {THE_PREDICATE})
    if _window_seam_is_the_close_refusal(functions, guarded):
        guarded = _chain(functions, guarded | {"yulon_close_refusal"})
    return guarded


def _obeys(function: ast.AST, guards: set[str], before: int) -> bool:
    """The function calls a guard before `before` and RETURNS or RAISES on its answer."""
    bound: set[str] = set()
    for node in _own_nodes(function):
        value = None
        targets: list[ast.expr] = []
        if isinstance(node, ast.Assign):
            value, targets = node.value, list(node.targets)
        elif isinstance(node, ast.NamedExpr):
            value, targets = node.value, [node.target]
        if value is not None and any(
            isinstance(c, ast.Call) and _callee_name(c) in guards for c in ast.walk(value)
        ):
            bound.update(t.id for t in targets if isinstance(t, ast.Name))
    for node in _own_nodes(function):
        if not isinstance(node, ast.If) or node.lineno >= before:
            continue
        tested = list(ast.walk(node.test))
        uses = any(isinstance(n, ast.Call) and _callee_name(n) in guards for n in tested) or any(
            isinstance(n, ast.Name) and n.id in bound for n in tested
        )
        leaves = any(
            isinstance(n, (ast.Return, ast.Raise)) for stmt in node.body for n in ast.walk(stmt)
        )
        if uses and leaves:
            return True
    return False


def _close_filter_obeys(functions: Functions, guarded: set[str]) -> bool:
    """main's close filter asks `close_refusal()` and lets the close through only on None."""
    return any(
        path.name == "main.py"
        and "close_refusal" in _called(node)
        and "close_refusal" in guarded
        and _obeys(node, {"close_refusal"}, 10**9)
        for path, node in functions.get("eventFilter", [])
    )


def _callers(functions: Functions, name: str) -> set[str]:
    return {
        caller
        for caller, defs in functions.items()
        for _path, node in defs
        if name in {_callee_name(c) for c in _own_nodes(node) if isinstance(c, ast.Call)}
    }


def _door_sites(functions: Functions) -> dict[str, list[tuple[int, str]]]:
    doors: dict[str, list[tuple[int, str]]] = {}
    for name, defs in functions.items():
        for _path, node in defs:
            if (sites := _sites(node)) and name != "drop_controller":
                doors.setdefault(name, []).extend(sites)
    return doors


# Doors that do not ask a guard themselves, and why that is true. Each kind is checked:
#   after_close: only ever called from the declared callers, which run after a close
#       was accepted (or forced); a new caller fails.
#   slot: the rebuild signal's emit site; the handler connected to that signal asks a guard
#       before it drops the tab.
#   callers: only the declared callers call it, and each of them asks a guard and obeys it.
#   standalone: the reason stands alone (nothing else could be running).
EXEMPT: dict[str, tuple[str, set[str] | str, str]] = {
    "_end": (
        "after_close",
        {"quit", "_quit_if_closed"},
        "ends the loop after `window.close()` was accepted by the close filter",
    ),
    "quit_app": ("after_close", {"_end"}, "the tray's seam for ending the event loop"),
    "_leave_with_jobs_still_running": (
        "after_close",
        {"main"},
        "the process end after the window's close was accepted (or forced) and jobs were joined",
    ),
    "main": ("after_close", set(), "the process's own end, after `app.exec()` returned"),
    "_finish_make": ("slot", "play_client_dir_changed:on_play_client_dir_changed", ""),
    "_play_client_deleted": ("slot", "play_client_dir_changed:on_play_client_dir_changed", ""),
    "finish_removal": (
        "callers",
        {"request_removal", "on_stopped_for_removal"},
        "carries out a removal both callers asked `forget_refusal()` about",
    ),
    "on_uninstalled": (
        "callers",
        {"finish_removal"},
        "an uninstall that finished (`_uninstall_done`) or a removal asks for the drop",
    ),
    "_uninstall_done": (
        "standalone",
        "",
        "the uninstall's own end; it held the tab's lock (`_busy`, `_uninstall_running`) "
        "for its whole run, so no other job began on the tab",
    ),
    "_regain_docker_group": (
        "standalone",
        "",
        "start-up, before any window, tab or job exists",
    ),
}


def _connected(signal: str, handler: str) -> bool:
    for path in SOURCES:
        for node in ast.walk(ast.parse(path.read_text(encoding="utf-8"))):
            if (
                isinstance(node, ast.Call)
                and _callee_name(node) == "connect"
                and isinstance(node.func, ast.Attribute)
                and getattr(node.func.value, "attr", "") == signal
                and any(isinstance(a, ast.Name) and a.id == handler for a in node.args)
            ):
                return True
    return False


def test_every_door_asks_a_guard_before_it_tears_down_and_obeys_the_answer() -> None:
    functions = _functions()
    guarded = _guarded(functions)
    close_ok = _close_filter_obeys(functions, guarded)
    failures: list[str] = []
    for name, sites in sorted(_door_sites(functions).items()):
        if name in EXEMPT:
            continue
        whats = {what for _line, what in sites}
        if whats <= CLOSE_DOORS:
            if not close_ok:
                failures.append(f"{name}: closes the window, and main's close filter does not obey")
            continue
        first = min(line for line, what in sites if what not in CLOSE_DOORS)
        node = functions[name][0][1]
        if not _obeys(node, guarded - {name} - {"drop_controller"}, first):
            failures.append(
                f"{name}: tears down at line {first} ({sorted(whats)}) without first asking a "
                "guard that reaches teardown_work() and returning on its answer"
            )
    assert not failures, "\n".join(failures)


def test_the_close_filter_obeys_a_refusal_that_reaches_the_predicate() -> None:
    functions = _functions()
    guarded = _guarded(functions)
    assert "close_refusal" in guarded, "close_refusal() no longer reaches teardown_work()"
    assert "_busy_reasons" in guarded, "_busy_reasons() no longer asks each tab's teardown_refusal"
    assert _close_filter_obeys(
        functions, guarded
    ), "main's close filter does not obey close_refusal"
    assert "yulon_close_refusal" in _guarded(functions), "the window's seam is not close_refusal"


def test_the_exemptions_are_current_and_each_reason_holds() -> None:
    functions = _functions()
    guarded = _guarded(functions)
    doors = _door_sites(functions)
    stale = sorted(set(EXEMPT) - set(doors))
    assert not stale, f"EXEMPT names functions that no longer tear anything down: {stale}"
    for name, (kind, detail, why) in EXEMPT.items():
        if kind in ("after_close", "callers"):
            assert isinstance(detail, set)
            actual = _callers(functions, name)
            assert actual == detail, f"{name}: callers are {sorted(actual)}, EXEMPT says {detail}"
        if kind == "callers":
            for caller in sorted(detail):  # type: ignore[arg-type]
                if caller in EXEMPT:
                    continue
                sites = [line for line, w in doors[caller] if w == name]
                assert sites, f"{caller} does not call {name}"
                node = functions[caller][0][1]
                assert _obeys(
                    node, guarded - {caller}, min(sites)
                ), f"{caller} calls {name} without asking a guard first and obeying it"
        if kind == "slot":
            signal, handler = str(detail).split(":")
            assert _connected(signal, handler), f"{signal} is not connected to {handler}"
            sites = [line for line, w in doors[handler] if w == "drop_controller"]
            node = functions[handler][0][1]
            assert _obeys(
                node, guarded - {handler, "drop_controller"}, min(sites)
            ), f"{handler} drops the tab without asking a guard first and obeying it"
        if kind == "standalone":
            assert why, f"{name} is exempt without a reason"


def test_the_scan_sees_the_doors_this_ticket_names() -> None:
    """An empty scan would pass the tests above, so the doors it must find are named."""
    doors = _door_sites(_functions())
    for expected in (
        "request_removal",
        "on_stopped_for_removal",
        "on_client_dir_changed",
        "on_play_client_dir_changed",
        "add_controller",
        "change_client_dir",
        "forget_client_dir",
        "offer_a_docker_group_restart",
        "_try_to_restart",
        "quit",
        "ask_to_quit",
    ):
        assert expected in doors, f"the scan no longer sees {expected}"
