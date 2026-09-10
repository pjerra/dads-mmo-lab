"""Tests for `yulon.steam` (roadmap 8.8, the two Steam library entries).

The captured entries these round-trip against are not hand-typed: they are the
two `shortcuts.vdf` entries `dmlpack` packed off a real Steam Deck, committed to
this repository at `pyplan/gates/8.8-steam-read-yulon-arch-2026-09-10/`. A codec
tested against a hand-written fixture tests the fixture.
"""

from __future__ import annotations

import json
from collections.abc import Callable
from datetime import datetime
from pathlib import Path

import pytest

from yulon import steam

REPO = Path(__file__).resolve().parents[2]
CAPTURE = (
    REPO
    / "pyplan"
    / "gates"
    / "8.8-steam-read-yulon-arch-2026-09-10"
    / "live-shortcut-entries.json"
)

FIXED = datetime(2026, 9, 10, 20, 5, 0)


def _captured_entries() -> list[dict[str, object]]:
    """The two verbatim entries from the Deck capture, in file order."""
    return [sc["vdf"] for sc in json.loads(CAPTURE.read_text(encoding="utf-8"))["shortcuts"]]


def _profile(home: Path) -> Path:
    """A single Steam profile under `home`, laid out the way the box's is."""
    config = home / ".local/share/Steam/userdata/18347166/config"
    config.mkdir(parents=True)
    (home / ".local/share/Steam/config").mkdir(parents=True)
    (home / ".local/share/Steam/config/config.vdf").write_text(_CONFIG_VDF, encoding="utf-8")
    return config


def _proton(home: Path, name: str = "GE-Proton11-6-x86_64") -> None:
    """A compatibility tool with the manifest shape the box's Proton actually has."""
    tool = home / ".local/share/Steam/compatibilitytools.d" / name
    tool.mkdir(parents=True)
    (tool / "compatibilitytool.vdf").write_text(
        '"compatibilitytools"\n'
        "{\n"
        '  "compat_tools"\n'
        "  {\n"
        f'    "{name}" // Internal name of this tool\n'
        "    {\n"
        '      "install_path" "."\n'
        f'      "display_name" "{name}"\n'
        '      "from_oslist"  "windows"\n'
        '      "to_oslist"    "linux"\n'
        "    }\n"
        "  }\n"
        "}\n",
        encoding="utf-8",
    )


_CONFIG_VDF = (
    '"InstallConfigStore"\n'
    "{\n"
    '\t"Software"\n'
    "\t{\n"
    '\t\t"Valve"\n'
    "\t\t{\n"
    '\t\t\t"Steam"\n'
    "\t\t\t{\n"
    '\t\t\t\t"AutoUpdateWindowEnabled"\t\t"0"\n'
    '\t\t\t\t"ipv6check_http_state"\t\t"bad"\n'
    "\t\t\t}\n"
    "\t\t}\n"
    "\t}\n"
    "}\n"
)
"""The shape read off `yulon-arch` on 2026-09-10: tabs, no `CompatToolMapping`."""


def _client(home: Path) -> Path:
    """A client folder with the three executables the box's TurtleWoW folder has."""
    client = home / "clients/TurtleWoW"
    client.mkdir(parents=True)
    for name in ("WoW.exe", "TurtleWoW.exe", "TWPatcher.exe"):
        (client / name).write_bytes(b"MZ")
    return client


def _shortcuts(
    home: Path,
    *,
    running: bool = False,
    platform_id: str = "linux",
    client_dir: Path | None = None,
    now: Callable[[], datetime] = lambda: FIXED,
) -> steam.SteamShortcuts:
    return steam.SteamShortcuts(
        game="Turtle WoW",
        server_dir=home / "tortoise-vm",
        client_dir=client_dir,
        home=home,
        is_running=lambda: running,
        now=now,
        platform_id=lambda: platform_id,
        launcher=lambda: ("/usr/bin/yulon", ""),
    )


# --------------------------------------------------------------------------
# the codec
# --------------------------------------------------------------------------


def test_the_codec_round_trips_the_decks_own_entries_byte_for_byte() -> None:
    """Serialise the captured entries, parse them back, serialise again: identical.

    Byte for byte and not merely equal-as-dicts, because the two failures this
    codec can have are both invisible to a dict comparison: an int written as a
    string still reads back as the same *value*, and a document that loses its
    second terminator byte still parses -- and makes Steam drop every non-Steam
    shortcut in the library.
    """
    root = {"shortcuts": {str(i): e for i, e in enumerate(_captured_entries())}}

    payload = steam.vdf_dump(root)
    parsed, consumed = steam.vdf_parse(payload)

    assert parsed == root
    assert consumed == len(payload)
    assert steam.vdf_dump(parsed) == payload
    assert payload.endswith(b"\x08\x08")


def test_the_empty_document_is_the_exact_bytes_steam_writes() -> None:
    """Spelled out by hand from the format, because `endswith` cannot see this.

    An empty `shortcuts` map is `00 "shortcuts" 00 | 08 | 08`: the type byte, the
    key, the map's own END, then the ROOT's END. A `vdf_dump` that forgot the
    second one still ends `08 08` on any real file — the last entry's `tags` map
    contributes two ENDs of its own — so a round-trip test and an `endswith`
    both sail past the one mistake that makes Steam drop every non-Steam
    shortcut in the library. Eleven literal bytes catch it.
    """
    assert steam.vdf_dump({"shortcuts": {}}) == b"\x00shortcuts\x00\x08\x08"


def test_a_document_that_loses_its_last_byte_is_not_silently_accepted() -> None:
    """The `08 08` terminator is checked, because Steam's answer to a bad one is silent."""
    payload = steam.vdf_dump({"shortcuts": {}})

    assert steam.terminated(payload)
    assert not steam.terminated(payload[:-1])


def test_the_appid_is_gen_appid_of_the_quoted_exe_and_the_name() -> None:
    """Steam's derivation, checked against the one captured entry that still matches it.

    `Turtle WoW V2 Server` was never renamed, so its stored appid is still
    `crc32(Exe + AppName) | 0x80000000`. Its sibling `Turtle WoW` is NOT -- see
    `test_an_entry_whose_stored_appid_was_never_derived_keeps_it`.
    """
    server = _captured_entries()[1]

    derived = steam.gen_appid(str(server["Exe"]), str(server["AppName"]))

    assert steam.to_signed(derived) == server["appid"] == -966953680
    assert derived == 3328013616


# --------------------------------------------------------------------------
# the profile
# --------------------------------------------------------------------------


def test_the_two_steam_roots_that_are_one_directory_count_once(tmp_path: Path) -> None:
    """`~/.steam/steam` is a symlink to `~/.local/share/Steam` on an ordinary box.

    Measured on `yulon-arch`, 2026-09-10. A finder that counted candidates
    instead of resolved directories would find the same profile twice and refuse
    a single-account machine with "two profiles".
    """
    config = _profile(tmp_path)
    (tmp_path / ".steam").mkdir()
    (tmp_path / ".steam/steam").symlink_to(tmp_path / ".local/share/Steam")

    assert steam.find_profile(tmp_path) == config


def test_no_steam_profile_is_refused_by_name(tmp_path: Path) -> None:
    """Nothing to write into is a refusal that says what to do, not a crash."""
    with pytest.raises(steam.SteamRefusal) as caught:
        steam.find_profile(tmp_path)

    assert "No Steam profile" in str(caught.value)
    assert "Sign in to Steam once" in str(caught.value)


def test_two_steam_profiles_are_refused_and_both_ids_are_named(tmp_path: Path) -> None:
    """Two accounts is a question this app cannot answer, so it asks it by id.

    By ID and never by account name: the name is the owner's, and every record
    this feature writes is read by somebody else later.
    """
    for account in ("18347166", "28006437"):
        (tmp_path / ".local/share/Steam/userdata" / account / "config").mkdir(parents=True)

    with pytest.raises(steam.SteamRefusal) as caught:
        steam.find_profile(tmp_path)

    assert "18347166" in str(caught.value)
    assert "28006437" in str(caught.value)


# --------------------------------------------------------------------------
# the write
# --------------------------------------------------------------------------


def test_it_writes_two_entries_the_artwork_and_the_compat_tool(tmp_path: Path) -> None:
    """One press: the file, the two entries, eight grid files, one compat mapping."""
    config = _profile(tmp_path)
    _proton(tmp_path)
    client = _client(tmp_path)

    report = _shortcuts(tmp_path, client_dir=client).add()

    root, _ = steam.vdf_parse((config / "shortcuts.vdf").read_bytes())
    names = [e["AppName"] for e in root["shortcuts"].values()]
    assert names == ["Turtle WoW", "Turtle WoW Server"]
    assert report.entries == ("Turtle WoW", "Turtle WoW Server")
    assert len(report.artwork) == 6
    assert all(path.exists() for path in report.artwork)
    assert report.compat_tool == "GE-Proton11-6-x86_64"
    assert "CompatToolMapping" in (tmp_path / ".local/share/Steam/config/config.vdf").read_text(
        encoding="utf-8"
    )


def test_the_logo_slot_is_left_empty_and_the_two_tiles_are_not_the_same_picture(
    tmp_path: Path,
) -> None:
    """Two findings from the live run on `yulon-arch`, both visible in its frames.

    A `_logo.png` is drawn INSTEAD of the entry's name, and these two entries
    differ by one word, so writing that slot loses the only thing telling them
    apart on the game page. And in the library grid no name is drawn at all, so
    the two tiles have to differ by colour or they are indistinguishable.
    """
    config = _profile(tmp_path)
    _proton(tmp_path)
    client = _client(tmp_path)

    report = _shortcuts(tmp_path, client_dir=client).add()

    assert not list((config / "grid").glob("*_logo.png"))
    assert {path.name.split(".")[0].rstrip("p_hero") for path in report.artwork}
    client_art, server_art = report.artwork[0], report.artwork[3]
    assert client_art.read_bytes() != server_art.read_bytes()
    assert client_art.read_bytes() == steam.icon_png(steam.CLIENT_ACCENT)
    assert server_art.read_bytes() == steam.icon_png(steam.SERVER_ACCENT)


def test_the_client_entry_points_at_the_clients_own_executable(tmp_path: Path) -> None:
    """`WoW.exe` and not `TWPatcher.exe`, of the three the box's folder holds."""
    config = _profile(tmp_path)
    _proton(tmp_path)
    client = _client(tmp_path)

    _shortcuts(tmp_path, client_dir=client).add()

    root, _ = steam.vdf_parse((config / "shortcuts.vdf").read_bytes())
    entry = next(e for e in root["shortcuts"].values() if e["AppName"] == "Turtle WoW")
    assert entry["Exe"] == f'"{client / "WoW.exe"}"'
    assert entry["appid"] == steam.to_signed(steam.gen_appid(str(entry["Exe"]), "Turtle WoW"))


def test_a_second_press_replaces_the_two_entries_rather_than_appending(tmp_path: Path) -> None:
    """The box's own requirement, and the one thing the prior art does NOT do.

    `dmlpack.register_shortcuts` is add-only: it sees the name, says "already
    present -- left alone", and a user who moved their client folder is stuck
    with an entry pointing at the old one. Here the second press rewrites both
    entries in place and the count does not grow.
    """
    config = _profile(tmp_path)
    _proton(tmp_path)
    client = _client(tmp_path)
    shortcuts = _shortcuts(tmp_path, client_dir=client)

    first = shortcuts.add()
    second = shortcuts.add()

    root, _ = steam.vdf_parse((config / "shortcuts.vdf").read_bytes())
    assert len(root["shortcuts"]) == 2
    assert not first.replaced
    assert second.replaced


def test_a_shortcut_somebody_else_added_is_left_alone(tmp_path: Path) -> None:
    """Upsert keyed by `AppName` touches two entries and only two."""
    config = _profile(tmp_path)
    _proton(tmp_path)
    client = _client(tmp_path)
    theirs = dict(_captured_entries()[0]) | {"AppName": "Dekaron Server"}
    (config / "shortcuts.vdf").write_bytes(steam.vdf_dump({"shortcuts": {"0": theirs}}))

    _shortcuts(tmp_path, client_dir=client).add()

    root, _ = steam.vdf_parse((config / "shortcuts.vdf").read_bytes())
    assert len(root["shortcuts"]) == 3
    assert root["shortcuts"]["0"] == theirs


def test_an_entry_whose_stored_appid_was_never_derived_keeps_it(tmp_path: Path) -> None:
    """Replace preserves the stored appid, because the artwork on disk is named after it.

    Measured on the Deck capture: `Turtle WoW`'s stored appid is `-105858796`
    and `gen_appid(Exe, AppName)` is `-1169327133`; seven other derivations were
    tried and none reproduces it. Steam keeps the appid it first issued when a
    shortcut is renamed or re-pointed, so recomputing on replace would orphan
    four grid files and leave the tile blank.
    """
    config = _profile(tmp_path)
    _proton(tmp_path)
    client = _client(tmp_path)
    theirs = dict(_captured_entries()[0]) | {"AppName": "Turtle WoW"}
    (config / "shortcuts.vdf").write_bytes(steam.vdf_dump({"shortcuts": {"0": theirs}}))

    report = _shortcuts(tmp_path, client_dir=client).add()

    root, _ = steam.vdf_parse((config / "shortcuts.vdf").read_bytes())
    entry = next(e for e in root["shortcuts"].values() if e["AppName"] == "Turtle WoW")
    assert entry["appid"] == -105858796
    assert entry["Exe"] == f'"{client / "WoW.exe"}"'
    assert any(str(steam.to_unsigned(-105858796)) in path.name for path in report.artwork)


def test_a_replace_keeps_steams_own_record_of_the_entry(tmp_path: Path) -> None:
    """`LastPlayTime`, `tags`, and any field this module has never heard of.

    The numbers are the ones Steam actually wrote on `yulon-arch` on 2026-09-10
    after each entry had been launched once from Big Picture. The first version
    of `upsert` put `0` back over both, which is a button that erases "last
    played" every time it is pressed.
    """
    config = _profile(tmp_path)
    _proton(tmp_path)
    client = _client(tmp_path)
    lived_in = dict(_captured_entries()[0]) | {
        "AppName": "Turtle WoW",
        "LastPlayTime": 1789071688,
        "tags": {"0": "Dad's MMO Lab"},
        "SomeFieldValveAddsNextYear": "keep me",
    }
    (config / "shortcuts.vdf").write_bytes(steam.vdf_dump({"shortcuts": {"0": lived_in}}))

    _shortcuts(tmp_path, client_dir=client).add()

    root, _ = steam.vdf_parse((config / "shortcuts.vdf").read_bytes())
    entry = next(e for e in root["shortcuts"].values() if e["AppName"] == "Turtle WoW")
    assert entry["LastPlayTime"] == 1789071688
    assert entry["tags"] == {"0": "Dad's MMO Lab"}
    assert entry["SomeFieldValveAddsNextYear"] == "keep me"
    assert entry["Exe"] == f'"{client / "WoW.exe"}"'


def test_a_second_press_over_an_unchanged_install_writes_the_same_bytes(
    tmp_path: Path,
) -> None:
    """ "A second press changes nothing" is 8.8's definition of done, byte for byte."""
    config = _profile(tmp_path)
    _proton(tmp_path)
    client = _client(tmp_path)
    shortcuts = _shortcuts(tmp_path, client_dir=client)

    shortcuts.add()
    first = (config / "shortcuts.vdf").read_bytes()
    shortcuts.add()

    assert (config / "shortcuts.vdf").read_bytes() == first


# --------------------------------------------------------------------------
# the refusals
# --------------------------------------------------------------------------


def test_it_refuses_to_write_while_steam_is_running_and_writes_nothing(tmp_path: Path) -> None:
    """Steam rewrites this file when it exits and discards anything it did not make.

    The refusal names the process, and the assertion that matters is the second
    one: a refusal that had already written the file would be worse than no
    refusal at all.
    """
    config = _profile(tmp_path)
    _proton(tmp_path)
    client = _client(tmp_path)

    with pytest.raises(steam.SteamRefusal) as caught:
        _shortcuts(tmp_path, running=True, client_dir=client).add()

    assert "Steam is running" in str(caught.value)
    assert not (config / "shortcuts.vdf").exists()
    assert not (config / "grid").exists()


@pytest.mark.parametrize("platform_id", ["windows", "macos"])
def test_nothing_is_written_on_windows_or_macos(tmp_path: Path, platform_id: str) -> None:
    """The platform seam, and the file system is the assertion.

    8.8 is Linux and Steam Deck only. On the other two the button is never built
    -- `_steam_seam()` answers `None` -- and this is the belt below that brace:
    the module itself refuses rather than trusting the caller.
    """
    config = _profile(tmp_path)
    _proton(tmp_path)
    client = _client(tmp_path)

    with pytest.raises(steam.SteamRefusal) as caught:
        _shortcuts(tmp_path, platform_id=platform_id, client_dir=client).add()

    assert "Linux" in str(caught.value)
    assert not (config / "shortcuts.vdf").exists()
    assert list(config.iterdir()) == []


def test_no_proton_is_refused_with_the_name_of_what_to_install(tmp_path: Path) -> None:
    """A mapping naming a Proton the box has not got is a mapping that fails at launch."""
    _profile(tmp_path)
    client = _client(tmp_path)

    with pytest.raises(steam.SteamRefusal) as caught:
        _shortcuts(tmp_path, client_dir=client).add()

    assert "No Proton" in str(caught.value)
    assert "compatibilitytools.d" in str(caught.value)


def test_an_install_with_no_client_folder_is_refused_rather_than_invented(
    tmp_path: Path,
) -> None:
    """Half of this feature is an entry pointing at the client; a guess is not one."""
    _profile(tmp_path)
    _proton(tmp_path)

    with pytest.raises(steam.SteamRefusal) as caught:
        _shortcuts(tmp_path, client_dir=None).add()

    assert "no client folder" in str(caught.value)


# --------------------------------------------------------------------------
# the backup
# --------------------------------------------------------------------------


def test_the_backup_exists_before_the_file_is_written(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Proved by stopping the write, not by reading the order off the source.

    The write is made to fail; if the backup is there afterwards it was taken
    first, and the file Steam reads is still the one it wrote.
    """
    config = _profile(tmp_path)
    _proton(tmp_path)
    client = _client(tmp_path)
    original = steam.vdf_dump({"shortcuts": {"0": dict(_captured_entries()[0])}})
    (config / "shortcuts.vdf").write_bytes(original)

    def _explode(path: Path, payload: bytes) -> None:
        raise OSError("the disk went away")

    monkeypatch.setattr(steam, "_write_shortcuts", _explode)
    with pytest.raises(OSError):
        _shortcuts(tmp_path, client_dir=client).add()

    backup = config / "shortcuts.vdf.yulon-bak-20260910-200500"
    assert backup.read_bytes() == original
    assert (config / "shortcuts.vdf").read_bytes() == original


# --------------------------------------------------------------------------
# the compatibility tool
# --------------------------------------------------------------------------


def test_the_compat_edit_preserves_every_other_byte() -> None:
    """Take the inserted block back out and the file is what it was, byte for byte.

    `config.vdf` is 17,977 bytes of Steam's own state on the box -- CM websocket
    tables, shader-cache buckets, the depot list. A writer that reformatted it
    would be a writer that lost some of it.
    """
    edited, block = steam.compat_mapping(_CONFIG_VDF, appid=3328013616, tool="GE-Proton11-6")

    assert block in edited
    assert edited.replace(block, "", 1) == _CONFIG_VDF
    assert '"3328013616"' in block
    assert '"name"\t\t"GE-Proton11-6"' in block


def test_the_compat_edit_replaces_the_tool_when_the_appid_is_already_mapped() -> None:
    """A second press with a different Proton changes the name and nothing else."""
    once, _ = steam.compat_mapping(_CONFIG_VDF, appid=3328013616, tool="proton_9")

    twice, _ = steam.compat_mapping(once, appid=3328013616, tool="GE-Proton11-6")

    assert twice.count("CompatToolMapping") == 1
    assert twice.count('"3328013616"') == 1
    assert "proton_9" not in twice
    assert '"name"\t\t"GE-Proton11-6"' in twice


def test_a_second_appid_joins_the_section_rather_than_starting_another() -> None:
    """Two shortcuts, one `CompatToolMapping`."""
    once, _ = steam.compat_mapping(_CONFIG_VDF, appid=3328013616, tool="proton_9")

    twice, _ = steam.compat_mapping(once, appid=3125640163, tool="proton_9")

    assert twice.count("CompatToolMapping") == 1
    assert '"3328013616"' in twice and '"3125640163"' in twice


def test_the_compat_tool_name_is_read_from_the_tools_own_manifest(tmp_path: Path) -> None:
    """`compatibilitytools.d` is preferred because its internal name is ON DISK.

    A Proton under `steamapps/common` has no file naming what Steam calls it in
    `CompatToolMapping`, so that name is derived from Steam's convention and is
    the second choice, not the first.
    """
    _proton(tmp_path, name="GE-Proton11-6-x86_64")
    common = tmp_path / ".local/share/Steam/steamapps/common/Proton 9.0"
    common.mkdir(parents=True)

    assert steam.find_compat_tool(tmp_path / ".local/share/Steam") == "GE-Proton11-6-x86_64"


def test_an_official_proton_folder_is_named_the_way_steam_names_it(tmp_path: Path) -> None:
    """`Proton 9.0` is `proton_9` in `CompatToolMapping`, and beats Experimental.

    A numbered Proton wins because it is the one Valve ships as stable; the
    Experimental build is picked only when it is the only Proton there is.
    """
    root = tmp_path / ".local/share/Steam"
    (root / "steamapps/common/Proton 8.0").mkdir(parents=True)
    (root / "steamapps/common/Proton 9.0").mkdir()
    (root / "steamapps/common/Proton - Experimental").mkdir()

    assert steam.find_compat_tool(root) == "proton_9"


def test_experimental_is_used_when_it_is_the_only_proton(tmp_path: Path) -> None:
    """The named-build table, for the folders whose name carries no version."""
    root = tmp_path / ".local/share/Steam"
    (root / "steamapps/common/Proton - Experimental").mkdir(parents=True)

    assert steam.find_compat_tool(root) == "proton_experimental"


# --------------------------------------------------------------------------
# the sentence on the button
# --------------------------------------------------------------------------


def test_the_confirmation_names_the_entries_the_file_the_backup_and_the_tool(
    tmp_path: Path,
) -> None:
    """Everything the press changed, in one sentence a person can check by hand."""
    config = _profile(tmp_path)
    _proton(tmp_path)
    client = _client(tmp_path)
    (config / "shortcuts.vdf").write_bytes(steam.vdf_dump({"shortcuts": {}}))

    said = steam.confirmation(_shortcuts(tmp_path, client_dir=client).add())

    assert "Turtle WoW" in said and "Turtle WoW Server" in said
    assert str(config / "shortcuts.vdf") in said
    assert "shortcuts.vdf.yulon-bak-20260910-200500" in said
    assert "GE-Proton11-6-x86_64" in said
    assert "6 artwork files" in said
    assert "Restart Steam" in said
