#!/usr/bin/env python3
"""One-time generator: Achievement.dbc + Achievement_Category.dbc (3.3.5) ->
achievements JSON for the launcher.

Achievement.dbc 3.3.5 fields (62 x uint32):
  0 ID, 1 Faction(-1 both/0 horde/1 alliance), 2 Map, 3 Previous,
  4..19 Title_lang[16], 20 Title_flags, 21..36 Description_lang[16],
  37 Desc_flags, 38 Category, 39 Points, 40 UIOrder, 41 Flags, 42 IconID,
  43..58 Reward_lang[16], 59 Reward_flags, 60 MinimumCriteria, 61 SharesCriteria
Achievement_Category.dbc 3.3.5 fields (20 x uint32):
  0 ID, 1 Parent(-1 root), 2..17 Name_lang[16], 18 Name_flags, 19 UIOrder

The Statistics subtree (root category named "Statistics") and counter-flagged
achievements (Flags & 1) are excluded -- they're counters, not achievements,
and the in-game pane hides them too.
"""
import json
import struct
import sys


def read_dbc(path):
    with open(path, "rb") as f:
        data = f.read()
    magic, records, fields, rec_size, str_size = struct.unpack_from("<4sIIII", data, 0)
    assert magic == b"WDBC", f"{path}: not WDBC"
    assert rec_size == fields * 4, f"{path}: rec_size {rec_size} != fields {fields} * 4"
    base = 20
    rows = [struct.unpack_from(f"<{fields}I", data, base + i * rec_size) for i in range(records)]
    return rows, data[base + records * rec_size:], fields


def cstr(strings, off):
    end = strings.index(b"\0", off)
    return strings[off:end].decode("utf-8", "replace")


def signed(u):
    return u - 0x100000000 if u >= 0x80000000 else u


def main(ach_path, cat_path, out_path):
    cats_raw, cat_strings, cat_fields = read_dbc(cat_path)
    ach_raw, ach_strings, ach_fields = read_dbc(ach_path)
    print(f"Achievement fields: {ach_fields}, Category fields: {cat_fields}", file=sys.stderr)
    assert cat_fields == 20, f"unexpected Achievement_Category layout ({cat_fields} fields)"
    assert ach_fields == 62, f"unexpected Achievement layout ({ach_fields} fields)"

    cats = {}
    for r in cats_raw:
        cats[r[0]] = {
            "id": r[0],
            "parent": signed(r[1]),
            "name": cstr(cat_strings, r[2]),
            "order": signed(r[19]),
        }

    # Exclude the Statistics subtree (root category named Statistics + descendants).
    stat_roots = {c["id"] for c in cats.values() if c["parent"] == -1 and c["name"] == "Statistics"}
    assert stat_roots, "Statistics root category not found -- layout drift?"
    excluded = set(stat_roots)
    changed = True
    while changed:
        changed = False
        for c in cats.values():
            if c["id"] not in excluded and c["parent"] in excluded:
                excluded.add(c["id"])
                changed = True

    out_cats = [
        {"id": c["id"], "parent": (None if c["parent"] == -1 else c["parent"]), "name": c["name"], "order": c["order"]}
        for c in cats.values()
        if c["id"] not in excluded
    ]
    out_cats.sort(key=lambda c: (c["parent"] is not None, c["parent"] or 0, c["order"], c["id"]))

    out_ach = []
    counter_flagged = 0
    for r in ach_raw:
        aid, faction, cat, points, order, flags = r[0], signed(r[1]), r[38], r[39], r[40], r[41]
        if cat in excluded:
            continue
        if flags & 1:  # ACHIEVEMENT_FLAG_COUNTER
            counter_flagged += 1
            continue
        out_ach.append(
            {
                "id": aid,
                "cat": cat,
                "name": cstr(ach_strings, r[4]),
                "desc": cstr(ach_strings, r[21]),
                "points": points,
                "order": order,
                "faction": faction,
            }
        )
    out_ach.sort(key=lambda a: (a["cat"], a["order"], a["id"]))

    print(f"categories kept: {len(out_cats)} (excluded {len(excluded)} statistics)", file=sys.stderr)
    print(f"achievements kept: {len(out_ach)} (dropped {counter_flagged} counters)", file=sys.stderr)
    assert len(out_cats) > 20 and len(out_ach) > 800, "suspiciously small output"

    with open(out_path, "w", encoding="utf-8") as f:
        json.dump({"categories": out_cats, "achievements": out_ach}, f, separators=(",", ":"), sort_keys=True)
    print(f"wrote {out_path}", file=sys.stderr)


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2], sys.argv[3])
