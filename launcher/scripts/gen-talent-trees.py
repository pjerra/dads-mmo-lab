#!/usr/bin/env python3
"""One-time generator: Talent.dbc + TalentTab.dbc (3.3.5) -> talent-trees JSON.

WDBC layout: 20-byte header (magic, recordCount, fieldCount, recordSize,
stringBlockSize), then records of fieldCount uint32s, then the string block.

Talent.dbc 3.3.5 fields (all uint32):
  0 ID, 1 TabID, 2 Row(tier), 3 Col, 4..12 RankSpellId[9],
  13..15 PrereqTalent[3], 16..18 PrereqRank[3], 19 Flags,
  20 RequiredSpellID, 21..22 CategoryMask[2]
TalentTab.dbc 3.3.5 fields:
  0 ID, 1..16 Name_lang[16], 17 Name_flags, 18 SpellIconID,
  19 RaceMask, 20 ClassMask, 21 PetTalentMask, 22 OrderIndex, 23 BackgroundFile(strref)
"""
import json
import struct
import sys


def read_dbc(path):
    with open(path, "rb") as f:
        data = f.read()
    magic, records, fields, rec_size, str_size = struct.unpack_from("<4sIIII", data, 0)
    assert magic == b"WDBC", f"{path}: not WDBC"
    assert rec_size == fields * 4, f"{path}: unexpected record size {rec_size} for {fields} fields"
    base = 20
    rows = []
    for i in range(records):
        off = base + i * rec_size
        rows.append(struct.unpack_from(f"<{fields}I", data, off))
    strings = data[base + records * rec_size:]
    return rows, strings


def cstr(strings, off):
    end = strings.index(b"\0", off)
    return strings[off:end].decode("utf-8", "replace")


def main(talent_path, tab_path, out_path):
    tabs_raw, tab_strings = read_dbc(tab_path)
    talents_raw, _ = read_dbc(talent_path)

    tabs = {}
    for r in tabs_raw:
        tab_id, class_mask, pet_mask, order = r[0], r[20], r[21], r[22]
        if pet_mask != 0 or class_mask == 0:
            continue  # pet trees / non-class tabs
        tabs[tab_id] = {
            "id": tab_id,
            "name": cstr(tab_strings, r[1]),  # enUS locale slot
            "class_mask": class_mask,
            "order": order,
            "talents": [],
        }

    for r in talents_raw:
        tid, tab_id, row, col = r[0], r[1], r[2], r[3]
        if tab_id not in tabs:
            continue
        ranks = [s for s in r[4:13] if s != 0]
        prereq = [
            {"id": r[13 + i], "rank": r[16 + i] + 1}
            for i in range(3)
            if r[13 + i] != 0
        ]
        tabs[tab_id]["talents"].append(
            {"id": tid, "row": row, "col": col, "ranks": ranks, **({"prereq": prereq} if prereq else {})}
        )

    # classId (1..11) -> ordered trees
    out = {}
    for tab in tabs.values():
        mask = tab["class_mask"]
        if mask & (mask - 1) != 0:
            continue  # multi-class tab: none expected in 3.3.5, skip defensively
        class_id = mask.bit_length()
        tab["talents"].sort(key=lambda t: (t["row"], t["col"]))
        entry = {"id": tab["id"], "name": tab["name"], "talents": tab["talents"]}
        out.setdefault(str(class_id), []).append((tab["order"], entry))
    for k in out:
        out[k] = [e for _, e in sorted(out[k], key=lambda x: x[0])]

    # sanity
    classes = sorted(int(k) for k in out)
    counts = {k: sum(len(t["talents"]) for t in v) for k, v in out.items()}
    print(f"classes: {classes}", file=sys.stderr)
    print(f"trees per class: { {k: len(v) for k, v in out.items()} }", file=sys.stderr)
    print(f"talents per class: {counts}", file=sys.stderr)
    assert classes == [1, 2, 3, 4, 5, 6, 7, 8, 9, 11], f"unexpected classes {classes}"
    assert all(len(v) == 3 for v in out.values()), "every class must have exactly 3 trees"

    with open(out_path, "w", encoding="utf-8") as f:
        json.dump(out, f, separators=(",", ":"), sort_keys=True)
    print(f"wrote {out_path}", file=sys.stderr)


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2], sys.argv[3])
