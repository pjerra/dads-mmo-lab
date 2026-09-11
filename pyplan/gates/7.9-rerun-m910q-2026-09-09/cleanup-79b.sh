#!/usr/bin/env bash
# Remove the dumps THIS run wrote, and make the removal itself a record.
#
# 7.9 section 5 takes a full backup of every database and section 11 restores every one of
# them, and `restore()` writes a `pre-restore_` safety dump of each first. On this box that is
# 26 files and ~1.0 GB across the three games, none of which existed before 05:47.
#
# It names every file with its size BEFORE deleting it, and re-counts afterwards against what
# `ground-start.txt` recorded, so "the box is as it was" is a comparison and not a claim. Only
# files matching this run's own `20260909_05*` stamps are touched -- TBC's thirteen from the
# 2026-09-04 lanes are not this run's and survive, which is the control that shows the glob is
# not deleting indiscriminately.
set -u
OUT=$HOME/lane79b/out
STAMP='20260909_05'
{
  echo "=== dump cleanup, 7.9 re-run 2026-09-09  $(date -Is)  (UTC $(date -u -Is)) ==="
  echo
  echo "ground-start.txt recorded these counts before anything ran:"
  sed -n '/backup dirs/,$p' "$OUT/ground-start.txt" | tail -n +2
  echo
  for d in "$HOME"/tbc-7.4c "$HOME"/vanilla-75b "$HOME"/tortoise-server; do
    b="$d/sql_scripts/backups"
    [ -d "$b" ] || { echo "$b: absent"; continue; }
    echo "--- $b ---"
    echo "before: $(ls -1 "$b" | wc -l) entries, $(du -sh "$b" | cut -f1)"
    echo "this run's files, named and sized before removal:"
    n=0
    for f in "$b/$STAMP"*; do
      [ -e "$f" ] || continue
      echo "   rm $(basename "$f")  $(stat -c %s "$f") B"
      rm -f "$f"
      n=$((n + 1))
    done
    echo "removed: $n"
    echo "after : $(ls -1 "$b" | wc -l) entries, $(du -sh "$b" | cut -f1)"
    echo "kept  : $(ls -1 "$b" | tr '\n' ' ')"
    echo
  done
  echo "--- disk ---"
  df -h /home/pk | tail -1
} >> "$OUT/dump-cleanup.txt" 2>&1
cat "$OUT/dump-cleanup.txt"
