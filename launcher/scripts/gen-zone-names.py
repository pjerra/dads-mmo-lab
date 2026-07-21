#!/usr/bin/env python3
"""One-time generator: AreaTable.dbc (3.3.5) -> zone-names JSON.

Same WDBC layout as gen-talent-trees.py: 20-byte header (magic, recordCount,
fieldCount, recordSize, stringBlockSize), then records of fieldCount uint32s,
then the string block.

AreaTable.dbc 3.3.5 fields (36 uint32s per record):
  0 ID, 1 ContinentID(map), 2 ParentAreaID, 3 AreaBit, 4 Flags,
  5 SoundProviderPref, 6 SoundProviderPrefUnderwater, 7 AmbienceID,
  8 ZoneMusic, 9 IntroSound, 10 ExplorationLevel,
  11..26 AreaName_lang[16] (11 = enUS string offset), 27 AreaName_flags,
  28 FactionGroupMask, 29..32 LiquidTypeID[4], 33 MinElevation(float),
  34 AmbientMultiplier(float), 35 LightID

Output: {"<area id>": "<enUS name>"} for EVERY row with a non-empty name --
characters.zone stores AreaTable ids, so the Statistics page can label the
bot-watch zone list without a DB round trip. Get the input via read-only
docker cp from the running worldserver:

  docker cp ac-worldserver:/azerothcore/env/dist/data/dbc/AreaTable.dbc /tmp/AreaTable.dbc
  python3 gen-zone-names.py /tmp/AreaTable.dbc ../src/lib/zone-names-wotlk.json
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
    return rows, strings, fields


def cstr(strings, off):
    end = strings.index(b"\0", off)
    return strings[off:end].decode("utf-8", "replace")


def main(area_path, out_path):
    rows, strings, fields = read_dbc(area_path)
    assert fields == 36, f"AreaTable.dbc: expected 36 fields (3.3.5), got {fields}"

    out = {}
    for r in rows:
        area_id, name_off = r[0], r[11]  # enUS locale slot
        name = cstr(strings, name_off) if name_off else ""
        if name:
            out[str(area_id)] = name

    # Offset sanity against three well-known zones -- if the name field ever
    # moves, fail loudly instead of committing garbage.
    checks = {"1637": "Orgrimmar", "17": "The Barrens", "4395": "Dalaran"}
    for aid, expect in checks.items():
        got = out.get(aid)
        assert got == expect, f"sanity check failed: area {aid} -> {got!r}, expected {expect!r}"

    print(f"areas: {len(out)}", file=sys.stderr)
    with open(out_path, "w", encoding="utf-8") as f:
        json.dump(out, f, separators=(",", ":"), sort_keys=True, ensure_ascii=False)
    print(f"wrote {out_path}", file=sys.stderr)


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
