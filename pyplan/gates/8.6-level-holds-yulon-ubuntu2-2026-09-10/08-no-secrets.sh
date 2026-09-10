#!/usr/bin/env bash
# T16 live half, step 08 -- this folder grepped for anything password-shaped,
# BEFORE it is committed (memory note `gate-logs-carry-generated-passwords`: the
# CI guard runs after the push unless you run it first).
#
# Run from the laptop against the worktree, not on the box: the files are here by
# the time this matters.
set -u
GATE="$(cd "$(dirname "$0")" && pwd)"
echo "=============================================================="
echo "T16 step 08 -- secrets -- $(date +'%H:%M:%S %Z') -- $(hostname)"
echo "=============================================================="
echo "folder: $GATE"
echo
echo "---- what is in it ----"
ls -l "$GATE"
echo
echo "---- the project's own shape: <word>-<16 hex> ----"
grep -rInE '[A-Za-z0-9_]+-[0-9a-f]{16}' "$GATE" || echo "(no match)"
echo
echo "---- any Database.Info-shaped line, which carries the password ----"
grep -rIn 'Database.Info\|DATABASE_INFO\|MYSQL_PASSWORD\|MYSQL_ROOT_PASSWORD\|db_password' "$GATE" || echo "(no match)"
echo
echo "---- the SOAP account's password, which the driver never prints ----"
grep -rIn 'SoapChannel(endpoint' "$GATE" || echo "(no match)"
grep -rIniE 'password|passwd|secret' "$GATE" || echo "(no match)"
echo
echo "---- a bare 16-hex or 32-hex run anywhere (md5 sums are named as such) ----"
grep -rInoE '\b[0-9a-f]{32}\b' "$GATE" | head -20 || true
echo
echo "done $(date +'%H:%M:%S %Z')"
