#!/usr/bin/env bash
# Lane b43 round 2. The mutation the round-1 review asked for: the installer's
# DEFAULT keep_awake seam (`native.py` `Seams.keep_awake = platform.keep_awake`)
# is replaced by `ExitStack`, i.e. an install that holds nothing. Run twice:
# at aa6ab59e (the new identity assert present -> must go RED) and at its parent
# 547b4c02 (assert absent -> the mutation SURVIVES, which is why the assert was
# added). `__pycache__` purged on both sides before and after every pytest.
set -u
CLONE=/tmp/b43r2
PY=/home/pk/dads-mmo-lab/pylauncher/.venv/bin/python
T=tests/test_families_azerothcore.py::test_every_seam_defaults_to_the_real_function_it_stands_in_for

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

rm -rf "$CLONE"
git clone -q --shared /home/pk/dads-mmo-lab "$CLONE" || exit 9
git -C "$CLONE" fetch -q https://github.com/pjerra/dads-mmo-lab.git lane/b43 || exit 9

for SHA in aa6ab59e30d95398e63c018fb641d3cb049739b4 547b4c02; do
  echo "################ SHA $SHA"
  git -C "$CLONE" checkout -q --detach "$SHA" || exit 9
  git -C "$CLONE" checkout -q -- .
  cd "$CLONE/pylauncher" || exit 9
  git -C "$CLONE" log --oneline -1
  echo "---- does this tree carry the new assert?"
  grep -n "real.keep_awake is platform.keep_awake" tests/test_families_azerothcore.py || echo "(absent)"
  echo "---- grep of the seam default, unmutated"
  grep -n "keep_awake: Callable" yulon/catalog/native.py
  purge
  echo "==== BASELINE (unmutated) ===="
  "$PY" -m pytest -p no:randomly -q "$T" 2>&1 | tail -6
  echo "==== baseline pytest exit=${PIPESTATUS[0]} ===="
  purge
  apply_mut
  grep -n "keep_awake: Callable" yulon/catalog/native.py
  purge
  echo "==== MUTATED RUN ===="
  "$PY" -m pytest -p no:randomly -q "$T" 2>&1 | tail -20
  echo "==== mutated pytest exit=${PIPESTATUS[0]} ===="
  purge
  git -C "$CLONE" checkout -q -- .
  purge
  echo "==== TREE RESTORED ===="
  git -C "$CLONE" status --short
  cd /
done

echo "################ purge readback (both sides, must be 0 and 0)"
find "$CLONE" -name __pycache__ -type d | wc -l
find /home/pk/dads-mmo-lab -name __pycache__ -type d | wc -l
echo "################ host and stamp"
hostname; date '+%Y-%m-%d %H:%M:%S %Z (box-local)'
"$PY" -V
