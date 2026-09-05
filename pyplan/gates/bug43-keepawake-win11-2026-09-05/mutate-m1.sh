#!/usr/bin/env bash
# Lane b43 mutation M1: restore the §43 bug (refuse by main-thread identity).
set -u
CLONE=/tmp/b43mut
PY=/home/pk/dads-mmo-lab/pylauncher/.venv/bin/python
purge() {
  find "$CLONE" -name __pycache__ -type d -prune -exec rm -rf {} + 2>/dev/null
  find /home/pk/dads-mmo-lab -name __pycache__ -type d -prune -exec rm -rf {} + 2>/dev/null
}
cd "$CLONE" && git checkout -q -- . && purge && cd "$CLONE/pylauncher" || exit 9

T1=tests/test_platform.py::test_keep_awake_on_windows_holds_the_main_thread_of_a_process_with_no_window
T2=tests/test_install_wiring.py::test_the_harness_holds_a_windows_machine_awake_on_its_own_main_thread

echo "==== BASELINE for M1 ===="
"$PY" -m pytest -p no:randomly -q "$T1" "$T2" 2>&1 | tail -4
echo "==== baseline pytest exit=${PIPESTATUS[0]} ===="
purge

"$PY" - <<'PYEOF'
import pathlib
p = pathlib.Path("/tmp/b43mut/pylauncher/yulon/platform.py")
s = p.read_text(encoding="utf-8")
old = "        if gui_thread() is threading.current_thread():"
new = "        if threading.main_thread() is threading.current_thread():"
assert s.count(old) == 1, s.count(old)
p.write_text(s.replace(old, new), encoding="utf-8")
print("M1 applied:", new.strip())
PYEOF
purge
echo "==== M1 MUTATED RUN ===="
"$PY" -m pytest -p no:randomly -q "$T1" "$T2" 2>&1 | tail -45
echo "==== M1 pytest exit=${PIPESTATUS[0]} ===="
purge
cd "$CLONE" && git checkout -q -- . && purge
echo "==== TREE RESTORED ===="
git -C "$CLONE" status --short
git -C "$CLONE" log --oneline -1
