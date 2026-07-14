#!/usr/bin/env bash
# Builds the single-file dml CLI from cli/src/*.sh (glob order = numeric prefixes).
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
cat src/*.sh > dml
chmod +x dml
bash -n dml   # parse check
echo "built cli/dml ($(wc -l < dml) lines)"
