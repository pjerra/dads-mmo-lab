#!/usr/bin/env bash
# Builds the single-file dml CLI from cli/src/*.sh (glob order = numeric prefixes).
#
# WHY THIS WRITES A TEMP FILE AND RENAMES IT, rather than the obvious
# `cat src/*.sh > dml`:
#
# 1. ATOMICITY. `> dml` truncates the target instantly and then fills it over
#    many milliseconds, so ANY concurrent reader sees a half-file. This project
#    has already paid for that: overlapping a bats run (whose every `setup()`
#    calls this script) with the cargo parity suites (which spawn `bash cli/dml`
#    as their oracle) produced `cli/dml: line NNNN: syntax error: unexpected end
#    of file` and ~450 red tests that looked like a real regression and were not.
#    `mv` within one directory is an atomic rename: a reader gets the whole old
#    file or the whole new one, never a torn one. The standing "never run bats
#    and cargo at the same time" rule stays good advice, but a rule is not a
#    mechanism, and this is the mechanism.
#
# 2. A BROKEN BUILD IS NEVER PUBLISHED. The parse check used to run on `dml`
#    AFTER it was already in place, so a syntax error in src/ shipped the broken
#    file and merely reported it afterwards -- every later reader, including the
#    next test, got the bad artifact. Checking the temp file first means a failed
#    build leaves the previous working CLI untouched.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"

# Same directory as the target on purpose: `mv` is only atomic within one
# filesystem, and /tmp is frequently a different one.
tmp="$(mktemp dml.build.XXXXXX)"
trap 'rm -f "$tmp"' EXIT

cat src/*.sh > "$tmp"
chmod +x "$tmp"
bash -n "$tmp"   # parse check BEFORE it becomes cli/dml
mv -f "$tmp" dml
trap - EXIT

echo "built cli/dml ($(wc -l < dml) lines)"
