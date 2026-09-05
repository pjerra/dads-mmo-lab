#!/usr/bin/env bash
# Lane b43 round 2, second half of the seam-default mutation: the SAME mutation
# (`native.py` `Seams.keep_awake = platform.keep_awake` -> `= ExitStack`) run
# against the WHOLE narrow suite, so "nothing in the suite read the default" is
# a measurement of the suite and not of five files. Two checkouts of one fresh
# `git clone --shared`: 547b4c02 (assert absent) and aa6ab59e (assert present).
set -u
CLONE=/tmp/b43r2
PY=/home/pk/dads-mmo-lab/pylauncher/.venv/bin/python

purge() {
  find "$CLONE" -name __pycache__ -type d -prune -exec rm -rf {} + 2>/dev/null
  find /home/pk/dads-mmo-lab -name __pycache__ -type d -prune -exec rm -rf {} + 2>/dev/null
}

apply_mut() {
  "$PY" - <<'PYEOF'
import pathlib
p = pathlib.Path("/tmp/b43r2/pylauncher/yulon/catalog/native.py")
s = p.read_text(encoding="utf-8")
old = "    keep_awake: Callable[[], AbstractContextManager[None]] = platform.keep_awake"
new = "    keep_awake: Callable[[], AbstractContextManager[None]] = ExitStack"
assert s.count(old) == 1, s.count(old)
p.write_text(s.replace(old, new), encoding="utf-8")
print("mutation applied:", new.strip())
PYEOF
}

for SHA in 547b4c02 aa6ab59e30d95398e63c018fb641d3cb049739b4; do
  echo "################ SHA $SHA"
  git -C "$CLONE" checkout -q --detach "$SHA" || exit 9
  git -C "$CLONE" checkout -q -- .
  cd "$CLONE/pylauncher" || exit 9
  git -C "$CLONE" log --oneline -1
  grep -n "real.keep_awake is platform.keep_awake" tests/test_families_azerothcore.py || echo "(assert absent in this tree)"
  purge
  apply_mut
  grep -n "keep_awake: Callable" yulon/catalog/native.py
  purge
  echo "==== WHOLE NARROW SUITE, MUTATED ===="
  QT_QPA_PLATFORM=offscreen "$PY" -m pytest -q -p no:randomly -p no:cacheprovider \
    -m "not integration" -n 4 --dist loadfile 2>&1 | tail -8
  echo "==== mutated suite pytest exit=${PIPESTATUS[0]} ===="
  purge
  git -C "$CLONE" checkout -q -- .
  purge
  echo "==== TREE RESTORED ===="
  git -C "$CLONE" status --short
  cd /
done

echo "################ purge readback (clone, then project sources outside .venv)"
find "$CLONE" -name __pycache__ -type d | wc -l
find /home/pk/dads-mmo-lab -name __pycache__ -type d -not -path "*/.venv/*" | wc -l
hostname; date '+%Y-%m-%d %H:%M:%S %Z (box-local)'
