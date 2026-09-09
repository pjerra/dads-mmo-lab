#!/usr/bin/env bash
# Does `updater-transcript.log` actually show 173 DISTINCT world migrations applied IN ORDER?
#
# The README's claim used to rest on two aggregates -- 173 rows in the table afterwards, 0 attempts
# on the next start -- and a reviewer was right that neither is a transcript: both are consistent
# with a file applied twice and another skipped. This re-derives the claim from the updater's own
# lines instead, and from the file list inside the image the updater read:
#
#   1. the first start's world section holds exactly 173 `Attempting to execute update` lines;
#   2. no name appears twice;
#   3. the names, in the order the updater printed them, are exactly the image's
#      `sql/database_updates/world/*.sql` in byte-sorted name order (`LC_ALL=C sort`), which is the
#      order `AutoUpdater` walks a directory in;
#   4. that section contains no `failed to apply`.
#
# Read-only: it reads a committed file and starts one throwaway `docker run --rm` on m910q to list
# the image's own files. It never touches the install and starts no server.
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
TRANSCRIPT="${1:-$HERE/updater-transcript.log}"
IMAGE="${2:-yulon.local/cmangos-tortoise-server:native-58c6fd1c}"
BOX="${YULON_BOX:-m910q}"
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT

# The FIRST start's world section only: from its `Found … for world` to that start's ready banner.
awk '/Found [0-9]+ possible migrations for world/ {seen++} seen==1 {print} seen==1 && /World server is up and running/ {exit}' \
    "$TRANSCRIPT" > "$tmp/section.txt"
grep -o 'Attempting to execute update [^,]*' "$tmp/section.txt" \
    | sed 's/Attempting to execute update //' > "$tmp/applied.txt"

ssh "$BOX" "docker run --rm --entrypoint sh $IMAGE -c 'ls /opt/tortoise/sql/database_updates/world'" \
    | tr -d '\r' | sed 's/\.sql$//' | LC_ALL=C sort > "$tmp/files.txt"

applied=$(wc -l < "$tmp/applied.txt")
unique=$(LC_ALL=C sort -u "$tmp/applied.txt" | wc -l)
files=$(wc -l < "$tmp/files.txt")
found=$(grep -oE 'Found [0-9]+ possible migrations for world' "$tmp/section.txt" | head -1)
failed=$(grep -c 'failed to apply' "$tmp/section.txt")

echo "transcript:            $TRANSCRIPT"
echo "image:                 $IMAGE"
echo "the updater said:      $found"
echo "attempt lines:         $applied"
echo "distinct names:        $unique"
echo "*.sql in the image:    $files"
echo "failed to apply:       $failed"

rc=0
[ "$applied" = 173 ] || { echo "FAIL: expected 173 attempt lines"; rc=1; }
[ "$unique" = "$applied" ] || { echo "FAIL: a migration was attempted more than once"; rc=1; }
[ "$files" = "$applied" ] || { echo "FAIL: the image ships $files files and $applied were attempted"; rc=1; }
[ "$failed" = 0 ] || { echo "FAIL: $failed migrations failed to apply"; rc=1; }
if diff -u "$tmp/files.txt" "$tmp/applied.txt" > "$tmp/order.diff"; then
    echo "order:                 attempted in exactly the image's sorted file order"
else
    echo "FAIL: the order attempted is not the image's sorted file order:"; head -20 "$tmp/order.diff"; rc=1
fi
echo "first attempted:       $(head -1 "$tmp/applied.txt")"
echo "last attempted:        $(tail -1 "$tmp/applied.txt")"
[ "$rc" = 0 ] && echo "VERDICT: 173 distinct migrations, in the image's own order, none failed"
exit "$rc"
