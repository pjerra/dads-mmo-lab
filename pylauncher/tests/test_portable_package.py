"""T893: the character package, its writer, its reader and the refusals around them.

Pure code, no server: a `.ylchar` file is a zip, and everything here is the zip
and the sentences. Every refusal is asserted verbatim, because a sentence that
drifts is a sentence the player stops trusting.
"""

from __future__ import annotations

import json
import os
import stat
import sys
import zipfile
from datetime import datetime
from pathlib import Path

import pytest

import yulon
from yulon.platform import PRIVATE_MODE
from yulon.portable import package
from yulon.portable.package import (
    CharacterPackageError,
    ExportFacts,
    GameRef,
    read_dump,
    read_package,
    write_package,
)

AT = datetime(2026, 11, 17, 21, 40)
DIGEST_A = "a" * 64
UID = "0" * 32

FIXTURE = Path(__file__).resolve().parent / "fixtures" / "portable" / "azerothcore-small.pdump"


def small_dump() -> bytes:
    return FIXTURE.read_bytes()


def game(**over: object) -> GameRef:
    base: dict[str, object] = {
        "id": "wow-wotlk",
        "family": "azerothcore",
        "client_version": "3.3.5a",
        "build": 12340,
    }
    base.update(over)
    return GameRef(**base)  # type: ignore[arg-type]


def facts(**over: object) -> ExportFacts:
    base: dict[str, object] = {
        "game": game(),
        "core": {
            "repository": "https://github.com/azerothcore/azerothcore-wotlk.git",
            "commit": "e" * 40,
        },
        "schema": {"kind": "updates_digest", "value": "abc"},
        "realm_type": None,
        "modules": (),
        "uid": UID,
        "forked_from": None,
        "guild": None,
        "home": {"key_fingerprint": None, "realm_name": "My Realm"},
        "sequence": 1,
        "parents": (),
        "holder": {"world_fingerprint": None, "visit_id": None},
        "played_on": (),
        "made": AT,
    }
    base.update(over)
    return ExportFacts(**base)  # type: ignore[arg-type]


_PACK_COUNT = 0


def pack(tmp_path: Path, **over: object) -> Path:
    global _PACK_COUNT
    _PACK_COUNT += 1
    dest = tmp_path / f"priestest-{_PACK_COUNT}.ylchar"
    write_package(dest, facts(**over), small_dump())
    return dest


def rewrite(
    source: Path,
    dest: Path,
    *,
    replace: dict[str, bytes] | None = None,
    drop: tuple[str, ...] = (),
    add: dict[str, bytes] | None = None,
    manifest: dict | None = None,
) -> Path:
    """A copy of `source` with members replaced, dropped or added: the corruptions."""
    replace = replace or {}
    with zipfile.ZipFile(source) as src, zipfile.ZipFile(dest, "w") as out:
        for info in src.infolist():
            if info.filename in drop:
                continue
            data = replace.get(info.filename, src.read(info.filename))
            if info.filename == package.MANIFEST_NAME and manifest is not None:
                data = json.dumps(manifest).encode()
            out.writestr(info.filename, data)
        for name, data in (add or {}).items():
            out.writestr(name, data)
    return dest


def manifest_of(path: Path) -> dict:
    with zipfile.ZipFile(path) as z:
        return json.loads(z.read(package.MANIFEST_NAME))


def raw_manifest_of(path: Path) -> bytes:
    with zipfile.ZipFile(path) as z:
        return z.read(package.MANIFEST_NAME)


# ------------------------------------------------------------------ round trip


def test_write_then_read_gives_the_same_manifest_and_the_same_dump_bytes(
    tmp_path: Path,
) -> None:
    path = pack(tmp_path)
    found = read_package(path)
    assert found.manifest_bytes == raw_manifest_of(path)
    assert found.dump_bytes() == small_dump()
    assert found.manifest.character.name == "Priestest"
    assert found.digest == __import__("hashlib").sha256(found.manifest_bytes).hexdigest()


def test_references_of_the_small_dump_match_the_hand_count() -> None:
    dumped = read_dump(small_dump().decode("utf-8"))
    assert dumped.references.items == (2504, 5043, 6948)
    assert dumped.references.quests == (7, 33, 11335)
    assert dumped.references.creatures == (299,)


def test_the_character_facts_are_read_from_the_dump() -> None:
    dumped = read_dump(small_dump().decode("utf-8"))
    assert dumped.name == "Priestest"
    assert (dumped.race, dumped.class_, dumped.gender) == (1, 5, 1)
    assert dumped.level == 12
    assert dumped.money == 1234
    assert dumped.played_seconds == 3600
    assert dumped.item_count == 3


def test_a_built_package_holds_no_account_name_password_salt_or_verifier(
    tmp_path: Path,
) -> None:
    path = pack(tmp_path)
    with zipfile.ZipFile(path) as z:
        for info in z.infolist():
            assert info.filename.lower() == info.filename or True
            blob = z.read(info.filename).lower()
            for word in (b"password", b"verifier", b"salt", b"sha_pass_hash", b"session_key"):
                assert word not in blob, (info.filename, word)

    def _keys(node: object) -> list[str]:
        if isinstance(node, dict):
            return [str(k) for k in node] + [k for v in node.values() for k in _keys(v)]
        if isinstance(node, list):
            return [k for v in node for k in _keys(v)]
        return []

    for key in _keys(manifest_of(path)):
        assert key.lower() not in {"account", "username", "password", "salt", "verifier"}


# ------------------------------------------------------------------ corruptions


def test_a_newer_format_is_refused_and_names_the_yulon_that_made_it(tmp_path: Path) -> None:
    good = pack(tmp_path)
    manifest = manifest_of(good)
    manifest["format"] = package.FORMAT + 1
    bad = rewrite(good, tmp_path / "newer.ylchar", manifest=manifest)
    with pytest.raises(CharacterPackageError) as raised:
        read_package(bad)
    assert raised.value.reason == "newer_format"
    assert str(raised.value) == (
        f"{bad.name} was made by a newer Yu'lon ({yulon.__version__}). Update Yu'lon "
        "on this computer, then try again."
    )


def test_a_wrong_kind_is_refused(tmp_path: Path) -> None:
    good = pack(tmp_path)
    manifest = manifest_of(good)
    manifest["kind"] = "server"
    bad = rewrite(good, tmp_path / "kind.ylchar", manifest=manifest)
    with pytest.raises(CharacterPackageError) as raised:
        read_package(bad)
    assert raised.value.reason == "wrong_kind"
    assert str(raised.value) == (
        f"{bad.name} is a Yu'lon file, but it does not hold a character, so Yu'lon "
        "will not open it."
    )


def test_a_listed_member_that_is_missing_is_refused(tmp_path: Path) -> None:
    good = pack(tmp_path)
    bad = rewrite(good, tmp_path / "missing.ylchar", drop=(package.DUMP_NAME,))
    with pytest.raises(CharacterPackageError) as raised:
        read_package(bad)
    assert raised.value.reason == "missing_member"
    assert str(raised.value) == (
        f"{bad.name} is missing {package.DUMP_NAME}, which its list names, so Yu'lon "
        "will not open it."
    )


def test_a_member_the_list_does_not_name_is_refused(tmp_path: Path) -> None:
    good = pack(tmp_path)
    bad = rewrite(good, tmp_path / "stray.ylchar", add={"extras/foo.json": b"{}"})
    with pytest.raises(CharacterPackageError) as raised:
        read_package(bad)
    assert raised.value.reason == "extra_member"
    assert str(raised.value) == (
        f"{bad.name} holds a file its list does not name (extras/foo.json), so Yu'lon "
        "will not open it."
    )


def test_a_member_of_another_size_is_refused(tmp_path: Path) -> None:
    good = pack(tmp_path)
    with zipfile.ZipFile(good) as z:
        dump = z.read(package.DUMP_NAME)
    bad = rewrite(good, tmp_path / "size.ylchar", replace={package.DUMP_NAME: dump + b" "})
    with pytest.raises(CharacterPackageError) as raised:
        read_package(bad)
    assert raised.value.reason == "wrong_size"
    assert str(raised.value) == (
        f"{bad.name} does not match the list inside it ({package.DUMP_NAME} has "
        "another size), so Yu'lon will not open it. It is damaged, or it changed "
        "while it was being read: copy it again."
    )


def test_a_member_with_other_contents_is_refused(tmp_path: Path) -> None:
    good = pack(tmp_path)
    with zipfile.ZipFile(good) as z:
        dump = bytearray(z.read(package.DUMP_NAME))
    dump[0] = (dump[0] + 1) % 256
    bad = rewrite(good, tmp_path / "hash.ylchar", replace={package.DUMP_NAME: bytes(dump)})
    with pytest.raises(CharacterPackageError) as raised:
        read_package(bad)
    assert raised.value.reason == "wrong_hash"
    assert str(raised.value) == (
        f"{bad.name} does not match the list inside it ({package.DUMP_NAME} has "
        "other contents), so Yu'lon will not open it. It is damaged, or it changed "
        "while it was being read: copy it again."
    )


def test_two_members_of_one_name_are_refused(tmp_path: Path) -> None:
    good = pack(tmp_path)
    bad = tmp_path / "double.ylchar"
    with zipfile.ZipFile(good, "r") as src, zipfile.ZipFile(bad, "w") as out:
        for info in src.infolist():
            out.writestr(info, src.read(info.filename))
        with zipfile.ZipFile(good) as src2:
            out.writestr(package.DUMP_NAME, src2.read(package.DUMP_NAME))
    with pytest.raises(CharacterPackageError) as raised:
        read_package(bad)
    assert raised.value.reason == "duplicate_name"
    assert str(raised.value) == (
        f"{bad.name} holds two files of the same name, so it cannot be trusted."
    )


def test_a_large_signature_is_not_a_package(tmp_path: Path) -> None:
    good = pack(tmp_path)
    bad = rewrite(
        good,
        tmp_path / "sig.ylchar",
        add={package.SIGNATURE_NAME: b"s" * (package.MAX_SIGNATURE_BYTES + 1)},
    )
    with pytest.raises(CharacterPackageError) as raised:
        read_package(bad)
    assert raised.value.reason == "not_a_package"
    assert str(raised.value) == (
        f"{bad.name} is not a Yu'lon character file, or it is damaged. Export the "
        "character again."
    )


@pytest.mark.parametrize(
    "name", ["../x", "extras/../../x.json", "/etc/passwd", "C:/x", "\\x", "a\\b"]
)
def test_a_name_that_could_write_outside_the_folder_is_refused(tmp_path: Path, name: str) -> None:
    good = pack(tmp_path)
    bad = rewrite(good, tmp_path / "unsafe.ylchar", add={name: b"x"})
    with pytest.raises(CharacterPackageError) as raised:
        read_package(bad)
    assert raised.value.reason == "unsafe_name"
    assert str(raised.value) == (
        f"{bad.name} holds a file whose name could write outside the folder it is "
        "unpacked into, so Yu'lon will not open it."
    )


def test_a_file_over_the_cap_is_refused_before_it_is_opened(tmp_path: Path) -> None:
    path = tmp_path / "big.ylchar"
    with path.open("wb") as fh:
        fh.truncate(package.MAX_PACKAGE_BYTES + 1)
    with pytest.raises(CharacterPackageError) as raised:
        read_package(path)
    assert raised.value.reason == "too_big"
    assert str(raised.value) == (
        f"{path.name} is larger than a character file can be (8 MB), so Yu'lon " "will not open it."
    )


def test_a_zip_that_unpacks_past_the_limit_is_refused(tmp_path: Path) -> None:
    path = tmp_path / "fat.ylchar"
    member = package.DUMP_NAME
    manifest = {
        "format": 1,
        "kind": "character",
        "made_by": {"yulon": "t", "made": AT.isoformat()},
        "game": {
            "id": "wow-wotlk",
            "family": "azerothcore",
            "client_version": "3.3.5a",
            "build": 12340,
        },
        "core": {
            "repository": "https://github.com/azerothcore/azerothcore-wotlk.git",
            "commit": "e" * 40,
            "uid": UID,
            "forked_from": None,
            "name": "Priestest",
            "race": 1,
            "class": 5,
            "gender": 1,
            "level": 12,
            "money": 1234,
            "played_seconds": 3600,
            "item_count": 0,
            "guild": None,
        },
        "home": {"key_fingerprint": None, "realm_name": "My Realm"},
        "sequence": 1,
        "parents": [],
        "holder": {"world_fingerprint": None, "visit_id": None},
        "played_on": [],
        "references": {"items": [], "quests": [], "creatures": []},
        "members": [
            {
                "name": member,
                "bytes": package.MAX_UNPACKED_BYTES + 1,
                "sha256": "0" * 64,
            }
        ],
    }
    with zipfile.ZipFile(path, "w") as out:
        out.writestr(package.MANIFEST_NAME, json.dumps(manifest).encode())
        with out.open(member, "w", force_zip64=True) as sink:
            sink.write(b"\0")
    blob = bytearray(path.read_bytes())
    entry = blob.find(b"PK\x01\x02", blob.find(member.encode()))
    assert entry != -1
    blob[entry + 24 : entry + 28] = (package.MAX_UNPACKED_BYTES + 1).to_bytes(4, "little")
    path.write_bytes(bytes(blob))
    with pytest.raises(CharacterPackageError) as raised:
        read_package(path)
    assert raised.value.reason == "unpacks_too_big"
    assert str(raised.value) == (
        f"{path.name} unpacks to more than a character file can hold, so Yu'lon "
        "will not open it."
    )


def test_a_file_that_is_not_a_zip_is_refused(tmp_path: Path) -> None:
    path = tmp_path / "junk.ylchar"
    path.write_bytes(b"not a zip at all")
    with pytest.raises(CharacterPackageError) as raised:
        read_package(path)
    assert raised.value.reason == "not_a_package"
    assert str(raised.value) == (
        f"{path.name} is not a Yu'lon character file, or it is damaged. Export the "
        "character again."
    )


def test_a_zip_without_a_manifest_is_refused(tmp_path: Path) -> None:
    path = tmp_path / "nomanifest.ylchar"
    with zipfile.ZipFile(path, "w") as out:
        out.writestr(package.DUMP_NAME, b"x")
    with pytest.raises(CharacterPackageError) as raised:
        read_package(path)
    assert raised.value.reason == "not_a_package"
    assert str(raised.value) == (
        f"{path.name} is not a Yu'lon character file, or it is damaged. Export the "
        "character again."
    )


def test_a_manifest_that_breaks_the_model_is_not_a_package(tmp_path: Path) -> None:
    good = pack(tmp_path)
    manifest = manifest_of(good)
    manifest["character"]["level"] = 999
    bad = rewrite(good, tmp_path / "model.ylchar", manifest=manifest)
    with pytest.raises(CharacterPackageError) as raised:
        read_package(bad)
    assert raised.value.reason == "not_a_package"
    assert str(raised.value) == (
        f"{bad.name} is not a Yu'lon character file, or it is damaged. Export the "
        "character again."
    )


# ------------------------------------------------------------------ the dump


def _row(table: str, columns: str, values: str) -> bytes:
    return f"INSERT INTO `{table}` ({columns}) VALUES ({values});\n".encode()


def test_a_dump_with_a_table_the_core_does_not_dump_is_refused(tmp_path: Path) -> None:
    dump = small_dump() + _row("account", "`id`, `username`, `salt`", "'1', 'GM', 'abc'")
    dest = tmp_path / "x.ylchar"
    with pytest.raises(CharacterPackageError) as raised:
        write_package(dest, facts(), dump)
    assert raised.value.reason == "dump_table_unknown"
    assert str(raised.value) == (
        "The server's dump of the character holds the table account, which is "
        "not a character table, so nothing was written."
    )


def test_a_dump_line_without_column_names_is_refused(tmp_path: Path) -> None:
    dump = small_dump().replace(
        b"INSERT INTO `mail`",
        b"INSERT INTO `mail` VALUES ('x');\nINSERT INTO `mail`",
        1,
    )
    dest = tmp_path / "x.ylchar"
    with pytest.raises(CharacterPackageError) as raised:
        write_package(dest, facts(), dump)
    assert raised.value.reason == "dump_unreadable"
    assert "(line 11)" in str(raised.value)
    assert str(raised.value) == (
        "The server's dump of the character is not in the shape Yu'lon reads "
        "(line 11), so nothing was written."
    )


def test_a_dump_without_exactly_one_character_is_refused(tmp_path: Path) -> None:
    dest = tmp_path / "x.ylchar"
    with pytest.raises(CharacterPackageError) as raised:
        write_package(dest, facts(), small_dump() + small_dump())
    assert raised.value.reason == "dump_not_one_character"
    assert str(raised.value) == (
        "The server's dump does not hold exactly one character, so nothing was written."
    )


def test_another_family_is_not_exported_yet(tmp_path: Path) -> None:
    dest = tmp_path / "x.ylchar"
    with pytest.raises(CharacterPackageError) as raised:
        write_package(dest, facts(game=game(id="wow-tbc", family="cmangos")), small_dump())
    assert raised.value.reason == "family_unknown"
    assert str(raised.value) == (
        "Yu'lon cannot export a character of wow-tbc yet, so nothing was written."
    )


def test_writing_refuses_to_overwrite_and_leaves_no_partial(tmp_path: Path) -> None:
    path = pack(tmp_path)
    with pytest.raises(CharacterPackageError) as raised:
        write_package(path, facts(uid="2" * 32), small_dump())
    assert raised.value.reason == "exists"
    assert str(raised.value) == (f"{path.name} already exists, so Yu'lon did not overwrite it.")
    assert not list(tmp_path.glob("*.partial"))


def test_a_package_past_the_cap_is_not_written(tmp_path: Path) -> None:
    import secrets as _secrets

    row = _row(
        "item_instance",
        "`guid`, `itemEntry`, `owner_guid`, `count`, `text`",
        "'999', '6948', '5', '1', '" + _secrets.token_hex(9 * 1024 * 1024) + "'",
    )
    dest = tmp_path / "x.ylchar"
    with pytest.raises(CharacterPackageError) as raised:
        write_package(dest, facts(), small_dump() + row)
    assert raised.value.reason == "too_big_to_write"
    assert "more than the 8 MB a character file can be" in str(raised.value)
    assert not dest.exists()
    assert not list(tmp_path.glob("*.partial"))


# ------------------------------------------------------------------ owner only


def _mode(path: Path) -> int:
    return stat.S_IMODE(path.stat().st_mode)


@pytest.mark.skipif(sys.platform == "win32", reason="POSIX modes; Windows has the folder's ACL")
def test_the_package_is_owner_only(tmp_path: Path) -> None:
    old = os.umask(0o002)  # Ubuntu's default user umask
    try:
        dest = tmp_path / "priestest-wow-wotlk-20261117-2140.ylchar"
        write_package(dest, facts(), small_dump())
    finally:
        os.umask(old)
    assert _mode(dest) == 0o600, oct(_mode(dest))


# ------------------------------------------------------------------ lineage


def test_new_uids_are_random_hex_and_not_the_guid() -> None:
    seen = {package.new_uid() for _ in range(10)}
    assert len(seen) == 10
    assert all(len(uid) == 32 and all(c in "0123456789abcdef" for c in uid) for uid in seen)
    assert "5" * 32 not in seen


def test_a_rename_keeps_the_uid(tmp_path: Path) -> None:
    altered = small_dump().replace(b"Priestest", b"Priestess", 1)
    dest = tmp_path / "x.ylchar"
    manifest = write_package(dest, facts(), altered)
    assert manifest.character.uid == UID
    assert manifest.character.name == "Priestess"
    assert read_package(dest).manifest.character.name == "Priestess"


def test_a_fork_gets_a_new_uid_and_names_the_old_one(tmp_path: Path) -> None:
    from pydantic import ValidationError

    child, parent = package.fork(UID)
    assert child != UID and parent == UID
    with pytest.raises(ValidationError):
        write_package(tmp_path / "self.ylchar", facts(uid=UID, forked_from=UID), small_dump())


def test_the_next_package_counts_up_and_keeps_twenty_parents_newest_first(
    tmp_path: Path,
) -> None:
    first = read_package(pack(tmp_path))
    assert first.manifest.sequence == 1
    assert first.manifest.parents == ()
    sequence, parents = package.next_lineage(first)
    assert sequence == 2
    assert parents == (first.digest,)
    second = write_package(
        tmp_path / "second.ylchar",
        facts(sequence=2, parents=parents),
        small_dump(),
    )
    assert second.sequence == 2
    prior: tuple[str, ...] = parents
    for generation in range(3, 25):
        head = read_package(pack(tmp_path, sequence=generation, parents=prior))
        assert head.manifest.sequence == generation
        prior = package.next_lineage(head)[1]
        assert prior[0] == head.digest
        assert len(prior) <= package.MAX_PARENTS
    assert len(prior) == 20


def test_the_file_name_is_name_game_and_minute() -> None:
    assert (
        package.package_filename("Priestest", "wow-wotlk", AT)
        == "Priestest-wow-wotlk-20261117-2140.ylchar"
    )


def test_a_name_with_path_characters_keeps_only_letters() -> None:
    assert package.package_filename("../Pr/st", "wow-wotlk", AT).startswith("Prst-")
    assert package.package_filename("///", "wow-wotlk", AT).startswith("Character-")


# ------------------------------------------------------------------ signatures


def test_an_unsigned_package_reads_and_says_so(tmp_path: Path) -> None:
    found = read_package(pack(tmp_path))
    assert found.signature is None
    assert not found.signed


def test_a_signer_gets_the_exact_manifest_bytes_and_its_answer_is_stored(
    tmp_path: Path,
) -> None:
    seen: list[bytes] = []

    def signer(manifest_bytes: bytes) -> bytes:
        seen.append(manifest_bytes)
        return b"sig-by-t894"

    dest = tmp_path / "signed.ylchar"
    write_package(dest, facts(), small_dump(), signer=signer)
    assert len(seen) == 1
    assert seen[0] == raw_manifest_of(dest)
    found = read_package(dest)
    assert found.signed
    assert found.signature == b"sig-by-t894"


def test_the_signature_is_not_in_the_member_list(tmp_path: Path) -> None:
    def signer(manifest_bytes: bytes) -> bytes:
        return b"sig"

    dest = tmp_path / "signed.ylchar"
    manifest = write_package(dest, facts(), small_dump(), signer=signer)
    assert [m.name for m in manifest.members] == [package.DUMP_NAME]


def test_the_private_mode_is_the_platforms() -> None:
    assert package._PRIVATE_MODE == PRIVATE_MODE
