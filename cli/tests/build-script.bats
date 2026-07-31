#!/usr/bin/env bats
#
# cli/build.sh is the only thing that produces cli/dml, and cli/dml is a
# COMMITTED artifact that every bats file's setup() regenerates and that the
# cargo parity suites spawn as their oracle. A build script that can publish a
# broken or half-written CLI therefore poisons two independent test surfaces at
# once -- which has happened, and cost ~450 red tests that were not regressions.
#
# NB these tests never run the real build against the real cli/dml: they copy
# the script into a scratch tree with its own src/. Running the real one would
# race the very setup() every other file performs.

setup() {
  SCRATCH="$(mktemp -d)"
  BUILD_SH="$BATS_TEST_DIRNAME/../build.sh"
  mkdir -p "$SCRATCH/src"
  cp "$BUILD_SH" "$SCRATCH/build.sh"
}

teardown() { [[ -n "${SCRATCH:-}" ]] && rm -rf "$SCRATCH"; }

@test "build.sh concatenates src/*.sh in glob order" {
  printf '#!/usr/bin/env bash\nfirst=1\n' > "$SCRATCH/src/00-a.sh"
  printf 'second=2\n'                    > "$SCRATCH/src/10-b.sh"
  run bash "$SCRATCH/build.sh"
  [ "$status" -eq 0 ]
  [ -x "$SCRATCH/dml" ]
  run cat "$SCRATCH/dml"
  [[ "$output" == *"first=1"*"second=2"* ]]
}

# THE REGRESSION THIS FILE EXISTS FOR. The parse check used to run on `dml`
# AFTER the redirect had already replaced it, so a syntax error in src/ was
# published first and reported second -- every later reader, including the next
# test in the run, got the broken artifact.
@test "a syntax error in src/ never replaces a working cli/dml" {
  printf '#!/usr/bin/env bash\ngood=1\n' > "$SCRATCH/src/00-a.sh"
  run bash "$SCRATCH/build.sh"
  [ "$status" -eq 0 ]

  # Now break the source and rebuild.
  printf 'if [ -z "$x"\n' > "$SCRATCH/src/10-broken.sh"
  run bash "$SCRATCH/build.sh"
  [ "$status" -ne 0 ]

  # The PREVIOUS good build must still be there and still be parseable.
  run cat "$SCRATCH/dml"
  [[ "$output" == *"good=1"* ]]
  [[ "$output" != *"10-broken"* ]]
  run bash -n "$SCRATCH/dml"
  [ "$status" -eq 0 ]
}

@test "a failed build leaves no temp file behind" {
  printf 'if [ -z "$x"\n' > "$SCRATCH/src/00-broken.sh"
  run bash "$SCRATCH/build.sh"
  [ "$status" -ne 0 ]
  run bash -c 'ls "'"$SCRATCH"'"/dml.build.* 2>/dev/null | wc -l'
  [ "$output" -eq 0 ]
}

# `mv` is only atomic WITHIN one filesystem, so the temp file has to be created
# beside the target rather than in /tmp. Asserted on the script's text because
# the property itself (no reader ever sees a torn file) cannot be observed from
# a single-threaded test.
@test "the temp file is created beside the target, not in /tmp" {
  run grep -E 'mktemp[[:space:]]+dml\.build\.' "$SCRATCH/build.sh"
  [ "$status" -eq 0 ]
  run grep -E 'mktemp .*(/tmp|-t |--tmpdir)' "$SCRATCH/build.sh"
  [ "$status" -ne 0 ]
}

# The check must happen on the temp file. If someone "simplifies" this back to
# checking the published name, the guarantee above is gone even though the
# syntax check is still visibly present.
@test "the parse check runs on the temp file, before the rename" {
  run bash -c '
    b="'"$SCRATCH"'/build.sh"
    check_line=$(grep -n "bash -n" "$b" | head -1 | cut -d: -f1)
    mv_line=$(grep -n "^mv " "$b" | head -1 | cut -d: -f1)
    [ -n "$check_line" ] && [ -n "$mv_line" ] && [ "$check_line" -lt "$mv_line" ]
  '
  [ "$status" -eq 0 ]
}
