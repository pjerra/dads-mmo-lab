#!/usr/bin/env bash
# Lane b43 mutation harness. Runs from a fresh `git clone --shared` at the lane SHA.
set -u
CLONE=/tmp/b43mut
PY=/home/pk/dads-mmo-lab/pylauncher/.venv/bin/python
cd "$CLONE/pylauncher" || exit 9

purge() {
  find "$CLONE" -name __pycache__ -type d -prune -exec rm -rf {} + 2>/dev/null
  find /home/pk/dads-mmo-lab -name __pycache__ -type d -prune -exec rm -rf {} + 2>/dev/null
}

run_mut() {
  local name="$1" test="$2"
  echo "######## MUTATION $name"
  purge
  "$PY" -m pytest -p no:randomly -q "$test" 2>&1 | tail -25
  echo "######## exit=$? (pytest above)"
  purge
  cd "$CLONE" && git checkout -q -- . && cd "$CLONE/pylauncher"
  purge
}

echo "==== BASELINE (unmutated) ===="
purge
"$PY" -m pytest -p no:randomly -q \
  tests/test_platform.py::test_keep_awake_on_windows_refuses_the_declared_gui_thread \
  tests/test_platform.py::test_keep_awake_on_windows_holds_the_main_thread_of_a_process_with_no_window \
  tests/test_platform.py::test_keep_awake_on_windows_says_in_the_log_that_the_assertion_was_taken \
  tests/test_install_wiring.py::test_the_harness_holds_a_windows_machine_awake_on_its_own_main_thread \
  tests/test_main.py::test_the_launcher_declares_which_thread_is_its_gui_thread 2>&1 | tail -6
purge

# M1: the §43 bug itself, restored.
"$PY" - <<'PYEOF'
import pathlib
p = pathlib.Path("/tmp/b43mut/pylauncher/yulon/platform.py")
s = p.read_text(encoding="utf-8")
old = "        if gui_thread() is threading.current_thread():"
new = "        if threading.main_thread() is threading.current_thread():"
assert s.count(old) == 1, s.count(old)
p.write_text(s.replace(old, new), encoding="utf-8")
print("M1 applied")
PYEOF
run_mut "M1 refuse-by-main-thread (the original bug)" \
  "tests/test_platform.py::test_keep_awake_on_windows_holds_the_main_thread_of_a_process_with_no_window tests/test_install_wiring.py::test_the_harness_holds_a_windows_machine_awake_on_its_own_main_thread"

# M2: main.py stops declaring its GUI thread.
"$PY" - <<'PYEOF'
import pathlib
p = pathlib.Path("/tmp/b43mut/pylauncher/main.py")
s = p.read_text(encoding="utf-8")
old = "    platform.declare_gui_thread()\n"
assert s.count(old) == 1, s.count(old)
p.write_text(s.replace(old, ""), encoding="utf-8")
print("M2 applied")
PYEOF
run_mut "M2 main.py never declares its GUI thread" \
  "tests/test_main.py::test_the_launcher_declares_which_thread_is_its_gui_thread"

# M3: the Windows branch never refuses anyone.
"$PY" - <<'PYEOF'
import pathlib
p = pathlib.Path("/tmp/b43mut/pylauncher/yulon/platform.py")
s = p.read_text(encoding="utf-8")
old = "        if gui_thread() is threading.current_thread():"
new = "        if False and gui_thread() is threading.current_thread():"
assert s.count(old) == 1
p.write_text(s.replace(old, new), encoding="utf-8")
print("M3 applied")
PYEOF
run_mut "M3 the GUI thread is no longer refused" \
  "tests/test_platform.py::test_keep_awake_on_windows_refuses_the_declared_gui_thread"

# M4: the taken assertion stops saying so in the log.
"$PY" - <<'PYEOF'
import pathlib
p = pathlib.Path("/tmp/b43mut/pylauncher/yulon/platform.py")
s = p.read_text(encoding="utf-8")
old = ('        logger.info(\n'
       '            "holding this machine awake for the build: "\n'
       '            "SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED)"\n'
       '        )\n')
assert s.count(old) == 1, s.count(old)
p.write_text(s.replace(old, "        pass\n"), encoding="utf-8")
print("M4 applied")
PYEOF
run_mut "M4 a taken assertion is no longer logged" \
  "tests/test_platform.py::test_keep_awake_on_windows_says_in_the_log_that_the_assertion_was_taken"

echo "==== TREE RESTORED ===="
cd "$CLONE" && git status --short && git log --oneline -1
