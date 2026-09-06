#!/usr/bin/env bash
# Lane b41 round 3: the two mutations round 2's record claimed were held and were not.
# Throwaway shared clone at the round-3 tip, removed at the end.
set -uo pipefail
R=/home/pk/p7-b41-r3
SHA=30671d6e974a219617219320acd7aa87b4d1e674
N=$R/pylauncher/yulon/catalog/native.py
W=$R/pylauncher/yulon/networking.py
FILES="tests/test_spine.py tests/test_networking.py tests/test_controller_view.py"
purge() { find $R -name __pycache__ -type d -prune -exec rm -rf {} + 2>/dev/null; }
restore() { cd $R && git checkout -q -- pylauncher/yulon && purge; }
run() { cd $R/pylauncher && .venv/bin/python -m pytest $FILES -q -p no:cacheprovider --tb=line 2>&1 | tail -14; }

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
echo "########## M5: detect the address FIRST, then read the recorded intent"
restore
python3 - "$N" <<'PY'
import sys
p=sys.argv[1]; s=open(p,encoding='utf-8').read()
old="""        chosen = networking.read_network_intent(ctx.server_dir)
        if chosen is not None and chosen.mode == "loopback":
            yield loopback_chosen_on_purpose(chosen)
            return
        address = self._detected_lan_ip()
        if address is None:"""
new="""        address = self._detected_lan_ip()
        chosen = networking.read_network_intent(ctx.server_dir)
        if chosen is not None and chosen.mode == "loopback":
            yield loopback_chosen_on_purpose(chosen)
            return
        if address is None:"""
assert s.count(old)==1
open(p,'w',encoding='utf-8').write(s.replace(old,new))
print("M5 applied")
PY
purge
cd $R && git diff --stat
run

echo
echo "########## M6: read the recorded intent AFTER the 'no address' early return"
restore
python3 - "$N" <<'PY'
import sys
p=sys.argv[1]; s=open(p,encoding='utf-8').read()
old="""        chosen = networking.read_network_intent(ctx.server_dir)
        if chosen is not None and chosen.mode == "loopback":
            yield loopback_chosen_on_purpose(chosen)
            return
        address = self._detected_lan_ip()
        if address is None:
            yield REALM_ADDRESS_UNKNOWN
            return"""
new="""        address = self._detected_lan_ip()
        if address is None:
            yield REALM_ADDRESS_UNKNOWN
            return
        chosen = networking.read_network_intent(ctx.server_dir)
        if chosen is not None and chosen.mode == "loopback":
            yield loopback_chosen_on_purpose(chosen)
            return"""
assert s.count(old)==1
open(p,'w',encoding='utf-8').write(s.replace(old,new))
print("M6 applied")
PY
purge
cd $R && git diff --stat
run

echo
echo "########## M7: build the firewall commands before the mode is looked at (the round-2 behaviour)"
restore
python3 - "$W" <<'PY'
import sys
p=sys.argv[1]; s=open(p,encoding='utf-8').read()
old="""    wants_firewall = mode != "loopback\""""
new="""    wants_firewall = True"""
assert s.count(old)==1
open(p,'w',encoding='utf-8').write(s.replace(old,new))
print("M7 applied")
PY
purge
cd $R && git diff --stat
run

echo
echo "########## RESTORED: baseline again"
restore
run
cd $R && git status --porcelain
echo "########## removing the clone"
cd /home/pk && rm -rf $R && ls -d $R 2>&1
