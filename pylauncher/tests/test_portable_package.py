"""T893: the character package, its writer, its reader and the refusals around them.

Pure code, no server: a `.ylchar` file is a zip, and everything here is the zip
and the sentences. Every refusal is asserted verbatim, because a sentence that
drifts is a sentence the player stops trusting.
"""

from __future__ import annotations

import errno
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
REAL_DUMP = Path(__file__).resolve().parent / "fixtures" / "portable" / "wotlk-t891.pdump"


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


def patch_member_headers(
    path: Path, member: str, *, method: int | None = None, flags: int = 0
) -> None:
    """Set one member's compression method and/or flag bits in its two headers.

    A zip names the method and the flags in BOTH the local header and the central
    directory, and a reader may believe either; a corruption that touched one of
    them would be half a file, and the half a reader ignores is not the one worth
    pinning.
    """
    blob = bytearray(path.read_bytes())
    wanted = member.encode()
    patched = 0
    for signature, name_at, size_at, flag_at, method_at in (
        (b"PK\x03\x04", 30, 26, 6, 8),
        (b"PK\x01\x02", 46, 28, 8, 10),
    ):
        pos = blob.find(signature)
        while pos != -1:
            size = int.from_bytes(blob[pos + size_at : pos + size_at + 2], "little")
            if blob[pos + name_at : pos + name_at + size] == wanted:
                current = int.from_bytes(blob[pos + flag_at : pos + flag_at + 2], "little")
                blob[pos + flag_at : pos + flag_at + 2] = (current | flags).to_bytes(2, "little")
                if method is not None:
                    blob[pos + method_at : pos + method_at + 2] = method.to_bytes(2, "little")
                patched += 1
            pos = blob.find(signature, pos + 4)
    assert patched == 2, patched
    path.write_bytes(bytes(blob))


def break_central_name(path: Path) -> None:
    """Set the UTF-8 flag on the first central-directory name and make that name 0xff."""
    blob = bytearray(path.read_bytes())
    entry = blob.find(b"PK\x01\x02")
    assert entry != -1
    flags = int.from_bytes(blob[entry + 8 : entry + 10], "little")
    blob[entry + 8 : entry + 10] = (flags | 0x800).to_bytes(2, "little")
    blob[entry + 46] = 0xFF
    path.write_bytes(bytes(blob))


def not_a_package_sentence(name: str) -> str:
    return f"{name} is not a Yu'lon character file, or it is damaged. Export the character again."


def rezip(source: Path, dest: Path, compression: int) -> Path:
    """A copy of `source` whose members are all written with another compression."""
    with zipfile.ZipFile(source) as src, zipfile.ZipFile(dest, "w", compression=compression) as out:
        for info in src.infolist():
            out.writestr(info.filename, src.read(info.filename))
    return dest


def flip_member_data(path: Path, member: str) -> None:
    """Flip one byte of a member's compressed data, in the local-header region.

    Ten bytes in, not the first: LZMA's first byte is the range coder's initial
    byte and liblzma ignores it, so a stream flipped there still decodes and a
    test built on it would prove nothing about a broken member.
    """
    blob = bytearray(path.read_bytes())
    wanted = member.encode()
    pos = blob.find(b"PK\x03\x04")
    while pos != -1:
        name_size = int.from_bytes(blob[pos + 26 : pos + 28], "little")
        extra_size = int.from_bytes(blob[pos + 28 : pos + 30], "little")
        if blob[pos + 30 : pos + 30 + name_size] == wanted:
            start = pos + 30 + name_size + extra_size
            blob[start + 10] ^= 0xFF
            path.write_bytes(bytes(blob))
            return
        pos = blob.find(b"PK\x03\x04", pos + 4)
    raise AssertionError(f"{member} is not in {path.name}")


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


def test_references_of_the_real_wotlk_dump_match_the_hand_count(tmp_path: Path) -> None:
    dump = REAL_DUMP.read_bytes()
    dumped = read_dump(dump.decode("utf-8"))
    assert dumped.name == "Raezia"
    assert (dumped.race, dumped.class_, dumped.gender, dumped.level) == (10, 3, 1, 80)
    assert dumped.money == 26789920
    assert dumped.played_seconds == 42
    assert dumped.item_count == 47
    assert dumped.references.creatures == (7430,)
    assert dumped.references.quests == (
        10277,
        10279,
        10282,
        10283,
        10284,
        10285,
        10296,
        10297,
        10298,
        10445,
        10888,
        10901,
        10985,
        11481,
        11482,
        11488,
        11490,
        11492,
    )
    assert dumped.references.items == (
        2901,
        5956,
        5976,
        12282,
        20899,
        20900,
        20901,
        20980,
        24490,
        27991,
        28395,
        30623,
        30633,
        30634,
        30635,
        30637,
        31084,
        33447,
        33448,
        36947,
        36979,
        37043,
        37139,
        37144,
        37262,
        37390,
        37624,
        37636,
        37657,
        37666,
        37723,
        37845,
        37848,
        41426,
        43131,
        43284,
        43406,
        45206,
        51809,
        52021,
    )
    dest = tmp_path / "raezia.ylchar"
    write_package(dest, facts(), dump)
    assert read_package(dest).dump_bytes() == dump


def test_a_built_package_holds_no_account_name_password_salt_or_verifier(
    tmp_path: Path,
) -> None:
    path = pack(tmp_path)
    with zipfile.ZipFile(path) as z:
        for info in z.infolist():
            blob = z.read(info.filename).lower()
            for word in (
                b"username",
                b"password",
                b"verifier",
                b"salt",
                b"sha_pass_hash",
                b"session_key",
            ):
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


@pytest.mark.parametrize("version", ["2.0\u202e", "2.0\u0085"])
def test_a_newer_format_byline_with_an_invisible_character_is_not_shown(
    tmp_path: Path, version: str
) -> None:
    good = pack(tmp_path)
    manifest = manifest_of(good)
    manifest["format"] = package.FORMAT + 1
    manifest["made_by"]["yulon"] = version
    bad = rewrite(good, tmp_path / "byline.ylchar", manifest=manifest)
    with pytest.raises(CharacterPackageError) as raised:
        read_package(bad)
    assert raised.value.reason == "newer_format"
    assert str(raised.value) == (
        f"{bad.name} was made by a newer Yu'lon. Update Yu'lon on this computer, then try again."
    )
    assert version not in str(raised.value)


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
    with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED) as out:
        with out.open(package.DUMP_NAME, "w", force_zip64=True) as sink:
            for _ in range(65):
                sink.write(b"\0" * (1024 * 1024))
    assert path.stat().st_size < package.MAX_PACKAGE_BYTES
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


# ------------------------------------- what the zip and the model refuse (round B)


def test_a_deeply_nested_manifest_is_refused(tmp_path: Path) -> None:
    good = pack(tmp_path)
    bad = rewrite(good, tmp_path / "deep.ylchar", replace={package.MANIFEST_NAME: b"[" * 200000})
    with pytest.raises(CharacterPackageError) as raised:
        read_package(bad)
    assert raised.value.reason == "not_a_package"
    assert str(raised.value) == not_a_package_sentence(bad.name)


def test_an_unsupported_compression_method_is_refused(tmp_path: Path) -> None:
    bad = tmp_path / "method.ylchar"
    bad.write_bytes(pack(tmp_path).read_bytes())
    patch_member_headers(bad, package.MANIFEST_NAME, method=99)
    with pytest.raises(CharacterPackageError) as raised:
        read_package(bad)
    assert raised.value.reason == "not_a_package"
    assert str(raised.value) == not_a_package_sentence(bad.name)


def test_an_encrypted_member_is_refused(tmp_path: Path) -> None:
    bad = tmp_path / "encrypted.ylchar"
    bad.write_bytes(pack(tmp_path).read_bytes())
    patch_member_headers(bad, package.MANIFEST_NAME, flags=1)
    with pytest.raises(CharacterPackageError) as raised:
        read_package(bad)
    assert raised.value.reason == "not_a_package"
    assert str(raised.value) == not_a_package_sentence(bad.name)


def test_an_encrypted_unlisted_member_is_not_a_package(tmp_path: Path) -> None:
    good = pack(tmp_path)
    bad = rewrite(good, tmp_path / "encrypted-extra.ylchar", add={"extras/secret.json": b"{}"})
    patch_member_headers(bad, "extras/secret.json", flags=1)
    with pytest.raises(CharacterPackageError) as raised:
        read_package(bad)
    assert raised.value.reason == "not_a_package"
    assert str(raised.value) == not_a_package_sentence(bad.name)


def test_an_lzma_member_with_a_broken_stream_is_not_a_package(tmp_path: Path) -> None:
    good = pack(tmp_path)
    bad = rezip(good, tmp_path / "lzma.ylchar", zipfile.ZIP_LZMA)
    flip_member_data(bad, package.DUMP_NAME)
    with pytest.raises(CharacterPackageError) as raised:
        read_package(bad)
    assert raised.value.reason == "not_a_package"
    assert str(raised.value) == not_a_package_sentence(bad.name)


def test_a_bzip2_member_is_not_a_package(tmp_path: Path) -> None:
    good = pack(tmp_path)
    bad = rezip(good, tmp_path / "bzip2.ylchar", zipfile.ZIP_BZIP2)
    with pytest.raises(CharacterPackageError) as raised:
        read_package(bad)
    assert raised.value.reason == "not_a_package"
    assert str(raised.value) == not_a_package_sentence(bad.name)


def test_a_central_name_that_is_not_utf8_is_not_a_package(tmp_path: Path) -> None:
    bad = tmp_path / "utf8.ylchar"
    bad.write_bytes(pack(tmp_path).read_bytes())
    break_central_name(bad)
    with pytest.raises(CharacterPackageError) as raised:
        read_package(bad)
    assert raised.value.reason == "not_a_package"
    assert str(raised.value) == not_a_package_sentence(bad.name)


def test_a_member_that_turns_unreadable_after_the_package_was_read_is_refused(
    tmp_path: Path,
) -> None:
    path = pack(tmp_path)
    found = read_package(path)
    patch_member_headers(path, package.DUMP_NAME, method=99)
    with pytest.raises(CharacterPackageError) as raised:
        found.dump_bytes()
    assert raised.value.reason == "not_a_package"
    assert str(raised.value) == not_a_package_sentence(path.name)


def test_a_dump_member_with_an_unknown_method_is_refused(tmp_path: Path) -> None:
    bad = tmp_path / "dumpmethod.ylchar"
    bad.write_bytes(pack(tmp_path).read_bytes())
    patch_member_headers(bad, package.DUMP_NAME, method=99)
    with pytest.raises(CharacterPackageError) as raised:
        read_package(bad)
    assert raised.value.reason == "not_a_package"
    assert str(raised.value) == not_a_package_sentence(bad.name)


def test_a_dump_that_turns_into_lzma_after_the_package_was_read_is_refused(
    tmp_path: Path,
) -> None:
    path = pack(tmp_path)
    found = read_package(path)
    lzma_copy = rezip(path, tmp_path / "lzma.ylchar", zipfile.ZIP_LZMA)
    flip_member_data(lzma_copy, package.DUMP_NAME)
    path.write_bytes(lzma_copy.read_bytes())
    with pytest.raises(CharacterPackageError) as raised:
        found.dump_bytes()
    assert raised.value.reason == "not_a_package"
    assert str(raised.value) == not_a_package_sentence(path.name)


def test_a_name_that_turns_unreadable_after_the_package_was_read_is_refused(
    tmp_path: Path,
) -> None:
    path = pack(tmp_path)
    found = read_package(path)
    bad = tmp_path / "utf8.ylchar"
    bad.write_bytes(path.read_bytes())
    break_central_name(bad)
    path.write_bytes(bad.read_bytes())
    with pytest.raises(CharacterPackageError) as raised:
        found.dump_bytes()
    assert raised.value.reason == "not_a_package"
    assert str(raised.value) == not_a_package_sentence(path.name)


def test_a_dump_that_is_gone_after_the_package_was_read_is_refused(tmp_path: Path) -> None:
    path = pack(tmp_path)
    found = read_package(path)
    rewrite(path, tmp_path / "gone.ylchar", drop=(package.DUMP_NAME,))
    path.write_bytes((tmp_path / "gone.ylchar").read_bytes())
    with pytest.raises(CharacterPackageError) as raised:
        found.dump_bytes()
    assert raised.value.reason == "missing_member"
    assert str(raised.value) == (
        f"{path.name} is missing {package.DUMP_NAME}, which its list names, so Yu'lon "
        "will not open it."
    )


def test_a_signature_member_with_an_unknown_method_is_refused(tmp_path: Path) -> None:
    path = tmp_path / "signed.ylchar"
    write_package(path, facts(), small_dump(), signer=lambda manifest_bytes: b"sig")
    patch_member_headers(path, package.SIGNATURE_NAME, method=99)
    with pytest.raises(CharacterPackageError) as raised:
        read_package(path)
    assert raised.value.reason == "not_a_package"
    assert str(raised.value) == not_a_package_sentence(path.name)


def _with_played_on(manifest: dict, **fields: object) -> dict:
    played: dict[str, object] = {"name": "My Realm", "first": "2026-11-01", "last": "2026-11-17"}
    played.update(fields)
    manifest["played_on"] = [played]
    return manifest


def test_a_level_given_as_text_is_not_a_package(tmp_path: Path) -> None:
    good = pack(tmp_path)
    manifest = manifest_of(good)
    manifest["character"]["level"] = "80"
    bad = rewrite(good, tmp_path / "text.ylchar", manifest=manifest)
    with pytest.raises(CharacterPackageError) as raised:
        read_package(bad)
    assert raised.value.reason == "not_a_package"
    assert str(raised.value) == not_a_package_sentence(bad.name)


@pytest.mark.parametrize("name", ["\n\x1b[31m", "My\x85Realm"])
def test_a_played_on_name_with_control_characters_is_not_a_package(
    tmp_path: Path, name: str
) -> None:
    good = pack(tmp_path)
    manifest = _with_played_on(manifest_of(good), name=name)
    bad = rewrite(good, tmp_path / "played.ylchar", manifest=manifest)
    with pytest.raises(CharacterPackageError) as raised:
        read_package(bad)
    assert raised.value.reason == "not_a_package"
    assert str(raised.value) == not_a_package_sentence(bad.name)


def test_a_rate_of_infinity_is_not_a_package(tmp_path: Path) -> None:
    good = pack(tmp_path)
    manifest = _with_played_on(manifest_of(good), xp_rate=float("inf"))
    bad = rewrite(good, tmp_path / "rate.ylchar", manifest=manifest)
    with pytest.raises(CharacterPackageError) as raised:
        read_package(bad)
    assert raised.value.reason == "not_a_package"
    assert str(raised.value) == not_a_package_sentence(bad.name)


def test_a_character_name_with_a_bidi_override_is_not_a_package(tmp_path: Path) -> None:
    good = pack(tmp_path)
    manifest = manifest_of(good)
    manifest["character"]["name"] = "Rae\u202ezia"
    bad = rewrite(good, tmp_path / "bidi.ylchar", manifest=manifest)
    with pytest.raises(CharacterPackageError) as raised:
        read_package(bad)
    assert raised.value.reason == "not_a_package"
    assert str(raised.value) == not_a_package_sentence(bad.name)


@pytest.mark.parametrize(("sequence", "parents"), [(2, []), (1, [DIGEST_A])])
def test_a_sequence_that_does_not_match_its_parents_is_not_a_package(
    tmp_path: Path, sequence: int, parents: list[str]
) -> None:
    good = pack(tmp_path)
    manifest = manifest_of(good)
    manifest["sequence"] = sequence
    manifest["parents"] = parents
    bad = rewrite(good, tmp_path / "lineage.ylchar", manifest=manifest)
    with pytest.raises(CharacterPackageError) as raised:
        read_package(bad)
    assert raised.value.reason == "not_a_package"
    assert str(raised.value) == not_a_package_sentence(bad.name)


@pytest.mark.parametrize(("sequence", "parents"), [(2, ()), (1, (DIGEST_A,))])
def test_a_sequence_that_does_not_match_its_parents_is_refused_when_writing(
    tmp_path: Path, sequence: int, parents: tuple[str, ...]
) -> None:
    from pydantic import ValidationError

    dest = tmp_path / "lineage.ylchar"
    with pytest.raises(ValidationError):
        write_package(dest, facts(sequence=sequence, parents=parents), small_dump())
    assert not dest.exists()


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


def test_a_dump_row_without_its_reference_column_is_refused(tmp_path: Path) -> None:
    dump = small_dump() + _row("character_pet", "`id`, `owner`", "'3', '5'")
    dest = tmp_path / "x.ylchar"
    with pytest.raises(CharacterPackageError) as raised:
        write_package(dest, facts(), dump)
    assert raised.value.reason == "dump_unreadable"
    assert str(raised.value) == (
        "The server's dump of the character is not in the shape Yu'lon reads "
        "(line 16), so nothing was written."
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


def test_a_destination_made_while_writing_is_not_overwritten(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    other = b"another writer was here first"
    real_link = os.link

    def link(src: str, dst: str, *args: object, **kwargs: object) -> None:
        Path(dst).write_bytes(other)
        return real_link(src, dst, *args, **kwargs)

    monkeypatch.setattr(package.os, "link", link)
    dest = tmp_path / "x.ylchar"
    with pytest.raises(CharacterPackageError) as raised:
        write_package(dest, facts(), small_dump())
    assert raised.value.reason == "exists"
    assert str(raised.value) == (f"{dest.name} already exists, so Yu'lon did not overwrite it.")
    assert dest.read_bytes() == other
    assert not list(tmp_path.glob("*.partial"))


def test_two_writers_do_not_share_a_partial(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    dest = tmp_path / "x.ylchar"
    seen: list[str] = []
    real_read = package.read_package

    def read(path: Path):
        seen.append(path.name)
        if len(seen) == 1:
            # While this writer is between writing and publishing, another writer
            # starts on the same destination. It finishes first; this one must
            # then find the destination taken rather than replace it.
            write_package(dest, facts(uid="1" * 32), small_dump())
        return real_read(path)

    monkeypatch.setattr(package, "read_package", read)
    with pytest.raises(CharacterPackageError) as raised:
        write_package(dest, facts(), small_dump())
    assert raised.value.reason == "exists"
    assert len(seen) == 2
    assert seen[0] != seen[1]
    assert not list(tmp_path.glob("*.partial"))


def test_a_filesystem_without_hard_links_still_writes_once(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    def no_links(src: str, dst: str, *args: object, **kwargs: object) -> None:
        raise OSError(errno.EPERM, "hard links are not permitted here")

    monkeypatch.setattr(package.os, "link", no_links)
    dest = tmp_path / "x.ylchar"
    write_package(dest, facts(), small_dump())
    assert read_package(dest).manifest.character.name == "Priestest"
    with pytest.raises(CharacterPackageError) as raised:
        write_package(dest, facts(uid="2" * 32), small_dump())
    assert raised.value.reason == "exists"
    assert not list(tmp_path.glob("*.partial"))


def test_a_destination_made_just_before_the_exclusive_create_is_not_overwritten(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    other = b"another writer was here first"
    dest = tmp_path / "x.ylchar"
    real_open = os.open

    def no_links(src: str, dst: str, *args: object, **kwargs: object) -> None:
        raise OSError(errno.EPERM, "hard links are not permitted here")

    def open_with_race(path: str, flags: int, mode: int = 0o777) -> int:
        if Path(path) == dest and flags & os.O_EXCL:
            Path(dest).write_bytes(other)
        return real_open(path, flags, mode)

    monkeypatch.setattr(package.os, "link", no_links)
    monkeypatch.setattr(package.os, "open", open_with_race)
    with pytest.raises(CharacterPackageError) as raised:
        write_package(dest, facts(), small_dump())
    assert raised.value.reason == "exists"
    assert dest.read_bytes() == other
    assert not list(tmp_path.glob("*.partial"))


def test_a_link_failure_that_is_not_about_missing_hard_links_fails_the_write(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    def refused(src: str, dst: str, *args: object, **kwargs: object) -> None:
        raise OSError(errno.EACCES, "permission denied")

    monkeypatch.setattr(package.os, "link", refused)
    dest = tmp_path / "x.ylchar"
    with pytest.raises(CharacterPackageError) as raised:
        write_package(dest, facts(), small_dump())
    assert raised.value.reason == "write_failed"
    assert not dest.exists()
    assert not list(tmp_path.glob("*.partial"))


def test_a_link_that_reports_einval_is_a_filesystem_without_hard_links(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    def no_links(src: str, dst: str, *args: object, **kwargs: object) -> None:
        raise OSError(errno.EINVAL, "invalid argument")

    monkeypatch.setattr(package.os, "link", no_links)
    dest = tmp_path / "x.ylchar"
    write_package(dest, facts(), small_dump())
    assert read_package(dest).dump_bytes() == small_dump()
    assert not list(tmp_path.glob("*.partial"))


def test_a_failed_copy_does_not_delete_another_writers_file(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    other = b"another writer was here first"
    dest = tmp_path / "x.ylchar"

    def no_links(src: str, dst: str, *args: object, **kwargs: object) -> None:
        raise OSError(errno.EPERM, "hard links are not permitted here")

    def failing_fsync(fd: int) -> None:
        # Another writer takes the name over after this one created its own file
        # there, then the copy fails: the name no longer holds this writer's
        # file, so the cleanup must leave it alone.
        dest.unlink()
        dest.write_bytes(other)
        raise OSError(errno.EIO, "the disk went away")

    monkeypatch.setattr(package.os, "link", no_links)
    monkeypatch.setattr(package.os, "fsync", failing_fsync)
    with pytest.raises(CharacterPackageError) as raised:
        write_package(dest, facts(), small_dump())
    assert raised.value.reason == "write_failed"
    assert str(raised.value) == (
        f"Yu'lon could not write {dest.name}: [Errno 5] the disk went away. Nothing was written."
    )
    assert dest.read_bytes() == other
    assert not list(tmp_path.glob("*.partial"))


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
    prior: tuple[str, ...] = package.next_lineage(read_package(tmp_path / "second.ylchar"))[1]
    assert len(prior) == 2
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
