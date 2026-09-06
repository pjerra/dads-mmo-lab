#!/usr/bin/env bash
# Lane b41 round 4: the two thirds of the loopback firewall branch that round 3
# asserted nothing about, plus the netsh manual step and a control.
# Throwaway shared clone at the round-4 tip, removed at the end.
set -uo pipefail
R=/home/pk/p7-b41-r4
SHA=2586b9139c2a0b5c553666be6547c0804f690c2a
W=$R/pylauncher/yulon/networking.py
FILES="tests/test_spine.py tests/test_networking.py tests/test_controller_view.py"
purge() { find $R -name __pycache__ -type d -prune -exec rm -rf {} + 2>/dev/null; }
restore() { cd $R && git checkout -q -- pylauncher/yulon && purge; }
run() { cd $R/pylauncher && .venv/bin/python -m pytest $FILES -q -p no:cacheprovider --tb=line 2>&1 | tail -14; }

# P1/P2: what the plan actually does, with seams that record every call. This is
# the reading the docstring of plan() quotes.
probe() {
  cd $R/pylauncher && .venv/bin/python - "$1" <<'PY'
import sys
sys.path.insert(0, ".")
from yulon import networking, platform
from yulon.catalog.catalog import load_catalog

label = sys.argv[1]
WOTLK = load_catalog().get("wow-wotlk")


def seams(calls):
    def firewalld():
        calls.append("detect_firewalld")
        return "stopped"

    def zones(_d):
        calls.append("detect_zones")
        return None

    return {"detect_firewalld": firewalld, "detect_zones": zones}


for mode in ("loopback", "lan"):
    calls = []
    p = networking.plan(
        WOTLK, mode, firewall="firewalld", steamos=False, wsl=False,
        detect_lan=lambda: "192.168.10.134", **seams(calls),
    )
    print(f"{label} firewalld {mode}: seams={calls} "
          f"fw={[' '.join(c) for c in p.firewall_commands]} "
          f"manual={list(p.manual_steps)} warnings={len(p.warnings)}")
    for w in p.warnings:
        print(f"    warn: {w[:110]}")

for mode in ("loopback", "lan"):
    calls = []

    def alf():
        calls.append("detect_alf")
        return platform.AlfState(enabled=True, block_all=False)

    p = networking.plan(
        WOTLK, mode, firewall="alf", steamos=False, wsl=False,
        detect_lan=lambda: "192.168.10.134", detect_alf=alf,
    )
    print(f"{label} alf {mode}: detect_alf called={bool(calls)} "
          f"firewall_state={p.firewall_state} manual={[m[:70] for m in p.manual_steps]}")

for mode in ("loopback", "lan"):
    p = networking.plan(
        WOTLK, mode, firewall="netsh", steamos=False, wsl=False,
        detect_lan=lambda: "192.168.10.134",
    )
    print(f"{label} netsh {mode}: manual={list(p.manual_steps)}")
PY
}

rm -rf $R
cd /home/pk && git clone -q --shared dads-mmo-lab $R
cd $R && git fetch -q https://github.com/pjerra/dads-mmo-lab.git lane/b41 && git checkout -q --detach $SHA
rm -rf pylauncher/.venv && ln -sfn ../../dads-mmo-lab/pylauncher/.venv pylauncher/.venv
echo "########## clone: $R   commit: $(git rev-parse HEAD)   date: $(date -u '+%Y-%m-%dT%H:%M:%SZ')"
purge

echo
echo "########## BASELINE at $SHA"
run

echo
echo "########## P1/P2: what the three backends do for loopback and for lan, with recording seams"
purge
probe UNMUTATED
purge

echo
echo "########## MR1: drop the wants_firewall guard from the firewalld branch"
restore
python3 - "$W" <<'PY'
import sys
p = sys.argv[1]; s = open(p, encoding='utf-8').read()
old = '    if wants_firewall and backend == "firewalld":'
new = '    if backend == "firewalld":'
assert s.count(old) == 1
open(p, 'w', encoding='utf-8').write(s.replace(old, new))
print("MR1 applied")
PY
purge
cd $R && git diff --stat
run
purge
probe MUTATED-MR1
purge

echo
echo "########## MR3: drop the wants_firewall guard from the alf branch"
restore
python3 - "$W" <<'PY'
import sys
p = sys.argv[1]; s = open(p, encoding='utf-8').read()
old = '    if wants_firewall and backend == "alf":'
new = '    if backend == "alf":'
assert s.count(old) == 1
open(p, 'w', encoding='utf-8').write(s.replace(old, new))
print("MR3 applied")
PY
purge
cd $R && git diff --stat
run
purge
probe MUTATED-MR3
purge

echo
echo "########## MR4: drop the wants_firewall guard from the netsh manual step"
restore
python3 - "$W" <<'PY'
import sys
p = sys.argv[1]; s = open(p, encoding='utf-8').read()
old = '    if wants_firewall and backend == "netsh":'
new = '    if backend == "netsh":'
assert s.count(old) == 1
open(p, 'w', encoding='utf-8').write(s.replace(old, new))
print("MR4 applied")
PY
purge
cd $R && git diff --stat
run

echo
echo "########## MR2 (control, held since round 3): drop it from the 'none' branch"
restore
python3 - "$W" <<'PY'
import sys
p = sys.argv[1]; s = open(p, encoding='utf-8').read()
old = '    elif wants_firewall and backend == "none":'
new = '    elif backend == "none":'
assert s.count(old) == 1
open(p, 'w', encoding='utf-8').write(s.replace(old, new))
print("MR2 applied")
PY
purge
cd $R && git diff --stat
run

echo
echo "########## RESTORED: baseline again"
restore
run

# The finding itself, measured rather than quoted: at the ROUND-3 tip the same
# two mutations answer green, which is why round 4 exists.
OLD=ccfe7f97140b7674632774688b27a3cc4e725252
echo
echo "########## ROUND-3 TIP $OLD: were MR1 and MR3 silent there?"
cd $R && git checkout -q --detach $OLD && purge
echo "-- baseline at the round-3 tip"
run
for M in MR1:firewalld MR3:alf; do
  name=${M%%:*}; be=${M##*:}
  echo "-- $name at the round-3 tip: if wants_firewall and backend == \"$be\": -> if backend == \"$be\":"
  restore
  python3 - "$W" "$be" <<'PY'
import sys
p, be = sys.argv[1], sys.argv[2]
s = open(p, encoding='utf-8').read()
old = f'    if wants_firewall and backend == "{be}":'
new = f'    if backend == "{be}":'
assert s.count(old) == 1
open(p, 'w', encoding='utf-8').write(s.replace(old, new))
print("applied")
PY
  purge
  run
done
restore
cd $R && git checkout -q --detach $SHA && purge
echo "-- back at $SHA"
run
cd $R && git status --porcelain
echo "########## removing the clone"
cd /home/pk && rm -rf $R && ls -d $R 2>&1
