"""T893: the character package, its writer, its reader and the refusals around them.

Pure code, no server: a `.ylchar` file is a zip holding the core's pdump text
(`character.pdump`), a `manifest.json` listing it with its size and SHA-256, and
optionally a `signature.json` (T894's seam) that signs the manifest bytes.
Every refusal is asserted verbatim, because a sentence that drifts is a sentence
the player stops trusting.
"""

from __future__ import annotations

import hashlib
import io
import json
import os
import re
import secrets
import zipfile
from collections.abc import Callable
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import IO, Literal

from pydantic import (
    BaseModel,
    ConfigDict,
    Field,
    ValidationError,
    field_validator,
    model_validator,
)

import yulon
from yulon.log import get_logger
from yulon.platform import PRIVATE_MODE

logger = get_logger(__name__)

FORMAT = 1
"""The package format this build writes and the newest it can read."""

KIND: Literal["character"] = "character"
SUFFIX = ".ylchar"
MANIFEST_NAME = "manifest.json"
DUMP_NAME = "character.pdump"
SIGNATURE_NAME = "signature.json"
MAX_PACKAGE_BYTES = 8 * 1024**2
MAX_UNPACKED_BYTES = 64 * 1024**2
MAX_MANIFEST_BYTES = 1024**2
MAX_SIGNATURE_BYTES = 64 * 1024
MAX_PARENTS = 20

_PRIVATE_MODE = 0o600
_EXTRA = re.compile(r"extras/[a-z0-9_]{1,64}\.json")

HEX64 = r"^[0-9a-f]{64}$"
HEX32 = r"^[0-9a-f]{32}$"
SHA1 = r"^[0-9a-f]{40}$"

Signer = Callable[[bytes], bytes]
"""Signs the manifest bytes; T894 checks the answer, this ticket only stores it."""

DUMP_TABLES: frozenset[str] = frozenset(
    {
        "characters",
        "character_account_data",
        "character_achievement",
        "character_achievement_progress",
        "character_action",
        "character_aura",
        "character_declinedname",
        "character_equipmentsets",
        "character_glyphs",
        "character_homebind",
        "character_inventory",
        "character_pet",
        "character_pet_declinedname",
        "character_queststatus",
        "character_queststatus_daily",
        "character_queststatus_weekly",
        "character_queststatus_monthly",
        "character_queststatus_seasonal",
        "character_queststatus_rewarded",
        "character_reputation",
        "character_skills",
        "character_spell",
        "character_spell_cooldown",
        "character_talent",
        "mail",
        "mail_items",
        "pet_aura",
        "pet_spell",
        "pet_spell_cooldown",
        "item_instance",
        "character_gifts",
    }
)
"""AzerothCore's `DumpTables[]`: the tables a v1 dump may hold (31)."""

REFERENCE_COLUMNS: dict[str, tuple[str, str]] = {
    "item_instance": ("itemEntry", "item"),
    "character_gifts": ("entry", "item"),
    "character_pet": ("entry", "creature"),
    "character_queststatus": ("quest", "quest"),
    "character_queststatus_daily": ("quest", "quest"),
    "character_queststatus_weekly": ("quest", "quest"),
    "character_queststatus_monthly": ("quest", "quest"),
    "character_queststatus_seasonal": ("quest", "quest"),
    "character_queststatus_rewarded": ("quest", "quest"),
}
"""Table -> (column, kind): where `references` reads its ids from."""

Reason = Literal[
    "not_a_package",
    "too_big",
    "unpacks_too_big",
    "newer_format",
    "wrong_kind",
    "unsafe_name",
    "duplicate_name",
    "missing_member",
    "extra_member",
    "wrong_size",
    "wrong_hash",
    "exists",
    "family_unknown",
    "dump_unreadable",
    "dump_table_unknown",
    "dump_not_one_character",
    "too_big_to_write",
    "write_failed",
]


class CharacterPackageError(Exception):
    """A character package that cannot be read or written, in the player's own words."""

    def __init__(self, reason: Reason, sentence: str) -> None:
        super().__init__(sentence)
        self.reason = reason


def _said(manifest_bytes: bytes) -> str:
    """` (<made_by.yulon>)` when it is short printable text, else nothing."""
    try:
        raw = json.loads(manifest_bytes.decode("utf-8"))
    except (ValueError, UnicodeDecodeError):
        return ""
    made_by = raw.get("made_by") if isinstance(raw, dict) else None
    version = made_by.get("yulon") if isinstance(made_by, dict) else None
    if not isinstance(version, str) or not version or len(version) > 40:
        return ""
    if any(ord(c) < 32 or ord(c) == 127 for c in version):
        return ""
    return f" ({version})"


def _megabytes(size: int) -> int:
    """Whole megabytes, rounded up, for a sentence about space."""
    return max(1, -(-size // (1024 * 1024)))


def _text(name: str, value: str, limit: int) -> str:
    if not value or len(value) > limit or any(ord(c) < 32 or ord(c) == 127 for c in value):
        raise ValueError(f"a {name} is printable text of up to {limit} characters")
    return value


class _Strict(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True)


class MadeBy(_Strict):
    yulon: str
    made: str
    key_fingerprint: str | None = Field(default=None, pattern=HEX32)
    display_name: str | None = None

    @field_validator("yulon")
    @classmethod
    def _yulon(cls, value: str) -> str:
        return _text("yulon version", value, 40)

    @field_validator("made")
    @classmethod
    def _made(cls, value: str) -> str:
        return _text("made time", value, 40)

    @field_validator("display_name")
    @classmethod
    def _display_name(cls, value: str | None) -> str | None:
        return None if value is None else _text("display name", value, 100)


class GameRef(_Strict):
    id: str = Field(pattern=r"^[a-z0-9]+(?:-[a-z0-9]+)*$")
    family: str = Field(pattern=r"^[a-z0-9]{1,40}$")
    client_version: str
    build: int = Field(gt=0)

    @field_validator("client_version")
    @classmethod
    def _client_version(cls, value: str) -> str:
        return _text("client version", value, 20)


class CoreRef(_Strict):
    repository: str
    commit: str = Field(pattern=SHA1)

    @field_validator("repository")
    @classmethod
    def _repository(cls, value: str) -> str:
        return _text("repository", value, 200)


class SchemaRef(_Strict):
    kind: Literal["updates_digest", "db_version_column"]
    value: str

    @field_validator("value")
    @classmethod
    def _value(cls, value: str) -> str:
        return _text("schema reference", value, 200)


class ModuleRef(_Strict):
    id: str
    commit: str | None = Field(default=None, pattern=SHA1)

    @field_validator("id")
    @classmethod
    def _id(cls, value: str) -> str:
        return _text("module id", value, 100)


class CharacterFacts(_Strict):
    model_config = ConfigDict(extra="forbid", frozen=True, populate_by_name=True)

    uid: str = Field(pattern=HEX32)
    forked_from: str | None = Field(default=None, pattern=HEX32)
    name: str
    race: int = Field(ge=0)
    class_: int = Field(alias="class", ge=0)
    gender: int = Field(ge=0)
    level: int = Field(ge=1, le=255)
    money: int = Field(ge=0)
    played_seconds: int = Field(ge=0)
    item_count: int = Field(ge=0)
    guild: str | None = None

    @field_validator("name")
    @classmethod
    def _name(cls, value: str) -> str:
        return _text("character name", value, 24)

    @field_validator("guild")
    @classmethod
    def _guild(cls, value: str | None) -> str | None:
        return None if value is None else _text("guild name", value, 24)

    @model_validator(mode="after")
    def _a_fork_names_another_character(self) -> CharacterFacts:
        if self.forked_from is not None and self.forked_from == self.uid:
            raise ValueError("a fork names the character it was forked from, not itself")
        return self


class Home(_Strict):
    key_fingerprint: str | None = Field(default=None, pattern=HEX32)
    realm_name: str

    @field_validator("realm_name")
    @classmethod
    def _realm_name(cls, value: str) -> str:
        return _text("realm name", value, 100)


class Holder(_Strict):
    world_fingerprint: str | None = Field(default=None, pattern=HEX32)
    visit_id: str | None = Field(default=None, pattern=HEX32)


class PlayedOn(_Strict):
    key_fingerprint: str | None = Field(default=None, pattern=HEX32)
    name: str = Field(min_length=1, max_length=100)
    first: str = Field(pattern=r"^\d{4}-\d{2}-\d{2}$")
    last: str = Field(pattern=r"^\d{4}-\d{2}-\d{2}$")
    xp_rate: float | None = Field(default=None, ge=0)
    drop_rate: float | None = Field(default=None, ge=0)


class References(_Strict):
    items: tuple[int, ...] = ()
    quests: tuple[int, ...] = ()
    creatures: tuple[int, ...] = ()

    @field_validator("items", "quests", "creatures")
    @classmethod
    def _ids(cls, value: tuple[int, ...]) -> tuple[int, ...]:
        if any(not isinstance(v, int) or isinstance(v, bool) or v <= 0 for v in value):
            raise ValueError("a reference is an id above zero")
        if tuple(sorted(value)) != value or len(set(value)) != len(value):
            raise ValueError("references are distinct and sorted")
        return value


class Member(_Strict):
    name: str
    bytes: int = Field(ge=0)
    sha256: str = Field(pattern=HEX64)


class Manifest(_Strict):
    model_config = ConfigDict(extra="forbid", frozen=True, populate_by_name=True)

    format: int
    kind: Literal["character"] = "character"
    made_by: MadeBy
    game: GameRef
    core: CoreRef
    schema_: SchemaRef = Field(alias="schema")
    realm_type: str | None = None
    modules: tuple[ModuleRef, ...] = ()
    character: CharacterFacts
    home: Home
    sequence: int = Field(ge=1)
    parents: tuple[str, ...] = Field(max_length=20)
    holder: Holder
    played_on: tuple[PlayedOn, ...] = ()
    references: References = References()
    members: tuple[Member, ...] = Field(min_length=1)

    @field_validator("realm_type")
    @classmethod
    def _realm_type(cls, value: str | None) -> str | None:
        return None if value is None else _text("realm type", value, 40)

    @field_validator("parents")
    @classmethod
    def _parents(cls, value: tuple[str, ...]) -> tuple[str, ...]:
        for parent in value:
            if not re.fullmatch(r"[0-9a-f]{64}", parent):
                raise ValueError("a parent is a manifest SHA-256")
        return value

    @model_validator(mode="after")
    def _one_dump_and_only_listed_extras(self) -> Manifest:
        names = [member.name for member in self.members]
        if len(set(names)) != len(names):
            raise ValueError("member names are distinct")
        if names.count(DUMP_NAME) != 1:
            raise ValueError("the members hold exactly one character dump")
        for name in names:
            if name != DUMP_NAME and not _EXTRA.fullmatch(name):
                raise ValueError("every other member is an extras file")
        return self


@dataclass(frozen=True)
class ExportFacts:
    """What `write_package` needs that the dump does not say."""

    game: GameRef
    core: CoreRef
    schema: SchemaRef
    realm_type: str | None
    modules: tuple[ModuleRef, ...]
    uid: str
    forked_from: str | None
    guild: str | None
    home: Home
    sequence: int
    parents: tuple[str, ...]
    holder: Holder
    played_on: tuple[PlayedOn, ...]
    made: datetime
    made_by_key: str | None = None
    made_by_name: str | None = None


@dataclass(frozen=True)
class DumpFacts:
    """What `read_dump` reads out of the dump text."""

    name: str
    race: int
    class_: int
    gender: int
    level: int
    money: int
    played_seconds: int
    item_count: int
    references: References


@dataclass(frozen=True)
class Package:
    """A package that has been read and whose every member matched the manifest."""

    path: Path
    manifest: Manifest
    manifest_bytes: bytes
    signature: bytes | None

    @property
    def digest(self) -> str:
        """SHA-256 hex of the manifest bytes: what a child names in `parents`."""
        return hashlib.sha256(self.manifest_bytes).hexdigest()

    @property
    def signed(self) -> bool:
        """Whether T894's seam is filled: `signature.json` travelled with it."""
        return self.signature is not None

    def dump_bytes(self) -> bytes:
        """The dump member, checked again as it is read."""
        (member,) = [m for m in self.manifest.members if m.name == DUMP_NAME]
        try:
            with zipfile.ZipFile(self.path) as archive, archive.open(member.name, mode="r") as fh:
                data = fh.read(member.bytes + 1)
        except (OSError, KeyError, zipfile.BadZipFile) as exc:
            raise CharacterPackageError(
                "wrong_size",
                f"{self.path.name} does not match the list inside it ({member.name} has "
                "another size), so Yu'lon will not open it. It is damaged, or it changed "
                "while it was being read: copy it again.",
            ) from exc
        if len(data) != member.bytes:
            raise CharacterPackageError(
                "wrong_size",
                f"{self.path.name} does not match the list inside it ({member.name} has "
                "another size), so Yu'lon will not open it. It is damaged, or it changed "
                "while it was being read: copy it again.",
            )
        if hashlib.sha256(data).hexdigest() != member.sha256:
            raise CharacterPackageError(
                "wrong_hash",
                f"{self.path.name} does not match the list inside it ({member.name} has "
                "other contents), so Yu'lon will not open it. It is damaged, or it changed "
                "while it was being read: copy it again.",
            )
        return data


def new_uid() -> str:
    """A fresh character id: 16 random bytes, hex."""
    return secrets.token_hex(16)


def fork(uid: str) -> tuple[str, str]:
    """`(new uid, old uid)`: "Import as a new character" keeps the old one named."""
    return new_uid(), uid


def next_lineage(parent: Package) -> tuple[int, tuple[str, ...]]:
    """`(sequence, parents)`: one more than the parent, newest first, at most 20."""
    return (
        parent.manifest.sequence + 1,
        (parent.digest, *parent.manifest.parents)[:MAX_PARENTS],
    )


def package_filename(character_name: str, game_id: str, made: datetime) -> str:
    """`<Name>-<game>-<yyyymmdd-hhmm>.ylchar`, letters only for the name."""
    kept = "".join(c for c in character_name if c.isalpha()) or "Character"
    return f"{kept}-{game_id}-{made:%Y%m%d-%H%M}{SUFFIX}"


_INSERT = re.compile(
    r"^INSERT INTO `(?P<table>[A-Za-z0-9_]+)` "
    r"\((?P<columns>`[A-Za-z0-9_]+`(?:, `[A-Za-z0-9_]+`)*)\) VALUES "
    r"(?P<values>\(.*\));$"
)
_NAME = re.compile(r"[A-Za-z0-9_]+")
_UNESCAPES = {"0": "\0", "n": "\n", "r": "\r", "Z": "\x1a", "\\": "\\", "'": "'", '"': '"'}


def _split_values(body: str) -> list[str] | None:
    """The values of one row: quoted strings with backslash escapes, ", " between them."""
    values: list[str] = []
    pos = 0
    end = len(body)
    if pos >= end or body[pos] != "(":
        return None
    pos += 1
    first = True
    while True:
        if not first:
            if pos >= end or body[pos : pos + 2] != ", ":
                return None
            pos += 2
        first = False
        if pos >= end or body[pos] != "'":
            return None
        pos += 1
        out: list[str] = []
        closed = False
        while pos < end:
            char = body[pos]
            if char == "\\":
                if pos + 1 >= end:
                    return None
                out.append(_UNESCAPES.get(body[pos + 1], body[pos + 1]))
                pos += 2
            elif char == "'":
                closed = True
                pos += 1
                break
            else:
                out.append(char)
                pos += 1
        if not closed:
            return None
        values.append("".join(out))
        if pos >= end:
            return None
        if body[pos] == ")":
            pos += 1
            break
    if pos != end:
        # More tuples on the line: the core writes one row per line, so refuse.
        return None
    return values


def _parse_insert(line: str) -> tuple[str, dict[str, str]] | None:
    """`(table, {column: value})` for one dump line, or None when it is another shape."""
    found = _INSERT.fullmatch(line)
    if found is None:
        return None
    columns = found.group("columns").split(", ")
    if any(not _NAME.fullmatch(c[1:-1]) for c in columns):
        return None
    values = _split_values(found.group("values"))
    if values is None or len(values) != len(columns):
        return None
    return found.group("table"), {
        column[1:-1]: value for column, value in zip(columns, values, strict=True)
    }


def _read_int(row: dict[str, str], column: str, line: int, table: str) -> int:
    try:
        return int(row[column])
    except (KeyError, ValueError) as exc:
        raise CharacterPackageError(
            "dump_unreadable",
            "The server's dump of the character is not in the shape Yu'lon reads "
            f"(line {line}), so nothing was written.",
        ) from exc


def read_dump(text: str) -> DumpFacts:
    """Read the character out of the core's pdump text.

    Raises `CharacterPackageError` (`dump_table_unknown`, `dump_not_one_character`
    or `dump_unreadable` with the line number) when the text is another shape.
    """
    character_rows: list[tuple[int, dict[str, str]]] = []
    counts: dict[str, int] = {}
    items: set[int] = set()
    quests: set[int] = set()
    creatures: set[int] = set()
    line_number = 0
    for raw in text.split("\n"):
        line_number += 1
        line = raw[:-1] if raw.endswith("\r") else raw
        if not line.strip() or line.lstrip().startswith("IMPORTANT NOTE:"):
            continue
        parsed = _parse_insert(line)
        if parsed is None:
            raise CharacterPackageError(
                "dump_unreadable",
                "The server's dump of the character is not in the shape Yu'lon reads "
                f"(line {line_number}), so nothing was written.",
            )
        table, row = parsed
        if table not in DUMP_TABLES:
            raise CharacterPackageError(
                "dump_table_unknown",
                f"The server's dump of the character holds the table {table}, which is "
                "not a character table, so nothing was written.",
            )
        if table == "characters":
            character_rows.append((line_number, row))
        if table == "item_instance":
            counts["item_instance"] = counts.get("item_instance", 0) + 1
        spec = REFERENCE_COLUMNS.get(table)
        if spec is not None:
            column, _kind = spec
            raw_value = row.get(column)
            if raw_value is None or raw_value == "0":
                continue
            try:
                value = int(raw_value)
            except ValueError as exc:
                raise CharacterPackageError(
                    "dump_unreadable",
                    "The server's dump of the character is not in the shape Yu'lon reads "
                    f"(line {line_number}), so nothing was written.",
                ) from exc
            if value == 0:
                continue
            _column, kind = spec
            if kind == "item":
                items.add(value)
            elif kind == "quest":
                quests.add(value)
            else:
                creatures.add(value)
    if len(character_rows) != 1:
        raise CharacterPackageError(
            "dump_not_one_character",
            "The server's dump does not hold exactly one character, so nothing was written.",
        )
    at, row = character_rows[0]
    try:
        name = row["name"]
        if not name or len(name) > 24 or any(ord(c) < 32 or ord(c) == 127 for c in name):
            raise ValueError("bad name")
        facts = DumpFacts(
            name=name,
            race=_read_int(row, "race", at, "characters"),
            class_=_read_int(row, "class", at, "characters"),
            gender=_read_int(row, "gender", at, "characters"),
            level=_read_int(row, "level", at, "characters"),
            money=_read_int(row, "money", at, "characters"),
            played_seconds=_read_int(row, "totaltime", at, "characters"),
            item_count=counts.get("item_instance", 0),
            references=References(
                items=tuple(sorted(items)),
                quests=tuple(sorted(quests)),
                creatures=tuple(sorted(creatures)),
            ),
        )
    except (KeyError, ValueError) as exc:
        raise CharacterPackageError(
            "dump_unreadable",
            "The server's dump of the character is not in the shape Yu'lon reads "
            f"(line {at}), so nothing was written.",
        ) from exc
    if not 1 <= facts.level <= 255:
        raise CharacterPackageError(
            "dump_unreadable",
            "The server's dump of the character is not in the shape Yu'lon reads "
            f"(line {at}), so nothing was written.",
        )
    if facts.money < 0 or facts.played_seconds < 0:
        raise CharacterPackageError(
            "dump_unreadable",
            "The server's dump of the character is not in the shape Yu'lon reads "
            f"(line {at}), so nothing was written.",
        )
    return facts


def _private_opener(path: str, flags: int) -> int:
    """An `open(opener=)` that creates the file 0600 and never reuses one (`O_EXCL`)."""
    return os.open(path, flags | os.O_EXCL, _PRIVATE_MODE)


def build_manifest(facts: ExportFacts, dump_facts: DumpFacts, dump: bytes) -> Manifest:
    """The manifest for `facts` and the facts read out of the dump."""
    return Manifest.model_validate(
        {
            "format": FORMAT,
            "kind": KIND,
            "made_by": {
                "yulon": yulon.__version__,
                "made": facts.made.isoformat(),
                "key_fingerprint": facts.made_by_key,
                "display_name": facts.made_by_name,
            },
            "game": facts.game,
            "core": facts.core,
            "schema": facts.schema,
            "realm_type": facts.realm_type,
            "modules": tuple(facts.modules),
            "character": {
                "uid": facts.uid,
                "forked_from": facts.forked_from,
                "name": dump_facts.name,
                "race": dump_facts.race,
                "class": dump_facts.class_,
                "gender": dump_facts.gender,
                "level": dump_facts.level,
                "money": dump_facts.money,
                "played_seconds": dump_facts.played_seconds,
                "item_count": dump_facts.item_count,
                "guild": facts.guild,
            },
            "home": facts.home,
            "sequence": facts.sequence,
            "parents": tuple(facts.parents),
            "holder": facts.holder,
            "played_on": tuple(facts.played_on),
            "references": dump_facts.references,
            "members": (
                {
                    "name": DUMP_NAME,
                    "bytes": len(dump),
                    "sha256": hashlib.sha256(dump).hexdigest(),
                },
            ),
        }
    )


def write_package(
    dest: Path,
    facts: ExportFacts,
    dump: bytes,
    *,
    signer: Signer | None = None,
) -> Manifest:
    """Write the character file to `dest`, read it back, and only then keep its name."""
    name = dest.name
    if dest.exists():
        raise CharacterPackageError(
            "exists", f"{name} already exists, so Yu'lon did not overwrite it."
        )
    if facts.game.family != "azerothcore":
        raise CharacterPackageError(
            "family_unknown",
            f"Yu'lon cannot export a character of {facts.game.id} yet, so nothing was written.",
        )
    try:
        dump_facts = read_dump(dump.decode("utf-8"))
    except UnicodeDecodeError as exc:
        raise CharacterPackageError(
            "dump_unreadable",
            "The server's dump of the character is not in the shape Yu'lon reads "
            "(line 0), so nothing was written.",
        ) from exc
    manifest = build_manifest(facts, dump_facts, dump)
    manifest_bytes = manifest.model_dump_json(by_alias=True, indent=2).encode("utf-8")
    signature = signer(manifest_bytes) if signer is not None else None

    data = io.BytesIO()
    with zipfile.ZipFile(data, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=6) as archive:
        archive.writestr(MANIFEST_NAME, manifest_bytes)
        archive.writestr(DUMP_NAME, dump)
        if signature is not None:
            archive.writestr(SIGNATURE_NAME, signature)
    packed = data.getvalue()
    if len(packed) > MAX_PACKAGE_BYTES:
        raise CharacterPackageError(
            "too_big_to_write",
            f"The character file would be {_megabytes(len(packed))} MB, more than the 8 MB "
            "a character file can be, so nothing was written.",
        )
    partial = dest.with_name(dest.name + ".partial")
    try:
        partial.unlink(missing_ok=True)
        with open(partial, "wb", opener=_private_opener) as fh:
            fh.write(packed)
        read_package(partial)
        partial.replace(dest)
    except OSError as exc:
        partial.unlink(missing_ok=True)
        raise CharacterPackageError(
            "write_failed", f"Yu'lon could not write {name}: {exc}. Nothing was written."
        ) from exc
    except BaseException:
        partial.unlink(missing_ok=True)
        raise
    assert _PRIVATE_MODE == PRIVATE_MODE
    return manifest


def _unsafe_name(name: str) -> bool:
    """A member name a zip can use to write somewhere it was not asked to."""
    if not name or name.startswith(("/", "\\")) or "\\" in name or ":" in name:
        return True
    for part in name.split("/"):
        if part in ("", ".", ".."):
            return True
        if any(ord(c) < 32 or ord(c) == 127 for c in part):
            return True
    return False


def read_package(path: Path) -> Package:
    """Open a character file, check it end to end, and return it.

    Order matters. The size cap is checked before the zip is opened, the names
    before anything is read, the format before the rest of the manifest, and every
    member's length and SHA-256 before the caller is given anything.
    """
    name = path.name
    try:
        if path.stat().st_size > MAX_PACKAGE_BYTES:
            raise CharacterPackageError(
                "too_big",
                f"{name} is larger than a character file can be (8 MB), so Yu'lon "
                "will not open it.",
            )
    except OSError as exc:
        raise CharacterPackageError(
            "not_a_package",
            f"{name} is not a Yu'lon character file, or it is damaged. Export the "
            "character again.",
        ) from exc
    try:
        archive = zipfile.ZipFile(path)
    except (OSError, zipfile.BadZipFile) as exc:
        raise CharacterPackageError(
            "not_a_package",
            f"{name} is not a Yu'lon character file, or it is damaged. Export the "
            "character again.",
        ) from exc
    with archive:
        names = archive.namelist()
        if any(_unsafe_name(n) for n in names):
            raise CharacterPackageError(
                "unsafe_name",
                f"{name} holds a file whose name could write outside the folder it is "
                "unpacked into, so Yu'lon will not open it.",
            )
        if len(set(names)) != len(names):
            raise CharacterPackageError(
                "duplicate_name",
                f"{name} holds two files of the same name, so it cannot be trusted.",
            )
        if sum(info.file_size for info in archive.infolist()) > MAX_UNPACKED_BYTES:
            raise CharacterPackageError(
                "unpacks_too_big",
                f"{name} unpacks to more than a character file can hold, so Yu'lon "
                "will not open it.",
            )
        if MANIFEST_NAME not in names:
            raise CharacterPackageError(
                "not_a_package",
                f"{name} is not a Yu'lon character file, or it is damaged. Export the "
                "character again.",
            )
        try:
            if archive.getinfo(MANIFEST_NAME).file_size > MAX_MANIFEST_BYTES:
                raise ValueError("manifest too large")
            manifest_bytes = archive.open(MANIFEST_NAME, mode="r").read()
            raw = __import__("json").loads(manifest_bytes.decode("utf-8"))
        except (ValueError, UnicodeDecodeError, OSError, zipfile.BadZipFile) as exc:
            raise CharacterPackageError(
                "not_a_package",
                f"{name} is not a Yu'lon character file, or it is damaged. Export the "
                "character again.",
            ) from exc
        if not isinstance(raw, dict):
            raise CharacterPackageError(
                "not_a_package",
                f"{name} is not a Yu'lon character file, or it is damaged. Export the "
                "character again.",
            )
        raw_format = raw.get("format")
        if isinstance(raw_format, bool) or not isinstance(raw_format, int) or raw_format < 1:
            raise CharacterPackageError(
                "not_a_package",
                f"{name} is not a Yu'lon character file, or it is damaged. Export the "
                "character again.",
            )
        if raw_format > FORMAT:
            raise CharacterPackageError(
                "newer_format",
                f"{name} was made by a newer Yu'lon{_said(manifest_bytes)}. Update Yu'lon "
                "on this computer, then try again.",
            )
        if raw.get("kind") != KIND:
            raise CharacterPackageError(
                "wrong_kind",
                f"{name} is a Yu'lon file, but it does not hold a character, so Yu'lon "
                "will not open it.",
            )
        try:
            manifest = Manifest.model_validate(raw)
        except ValidationError as exc:
            logger.info(f"{name}: manifest rejected: {exc}")
            raise CharacterPackageError(
                "not_a_package",
                f"{name} is not a Yu'lon character file, or it is damaged. Export the "
                "character again.",
            ) from exc
        listed = {member.name for member in manifest.members}
        stray = sorted(set(names) - listed - {MANIFEST_NAME, SIGNATURE_NAME})
        if stray:
            raise CharacterPackageError(
                "extra_member",
                f"{name} holds a file its list does not name ({stray[0]}), so Yu'lon "
                "will not open it.",
            )
        if SIGNATURE_NAME in listed:
            raise CharacterPackageError(
                "not_a_package",
                f"{name} is not a Yu'lon character file, or it is damaged. Export the "
                "character again.",
            )
        for member in manifest.members:
            if member.name not in names:
                raise CharacterPackageError(
                    "missing_member",
                    f"{name} is missing {member.name}, which its list names, so Yu'lon "
                    "will not open it.",
                )
            try:
                with archive.open(member.name, mode="r") as fh:
                    digest, size = _hash_stream(fh, limit=member.bytes)
            except (OSError, zipfile.BadZipFile) as exc:
                raise CharacterPackageError(
                    "wrong_size",
                    f"{name} does not match the list inside it ({member.name} has "
                    "another size), so Yu'lon will not open it. It is damaged, or it changed "
                    "while it was being read: copy it again.",
                ) from exc
            if size != member.bytes:
                raise CharacterPackageError(
                    "wrong_size",
                    f"{name} does not match the list inside it ({member.name} has "
                    "another size), so Yu'lon will not open it. It is damaged, or it changed "
                    "while it was being read: copy it again.",
                )
            if digest != member.sha256:
                raise CharacterPackageError(
                    "wrong_hash",
                    f"{name} does not match the list inside it ({member.name} has "
                    "other contents), so Yu'lon will not open it. It is damaged, or it changed "
                    "while it was being read: copy it again.",
                )
        signature: bytes | None = None
        if SIGNATURE_NAME in names:
            try:
                if archive.getinfo(SIGNATURE_NAME).file_size > MAX_SIGNATURE_BYTES:
                    raise ValueError("signature too large")
                with archive.open(SIGNATURE_NAME, mode="r") as fh:
                    signature = fh.read(MAX_SIGNATURE_BYTES + 1)
            except (OSError, zipfile.BadZipFile, ValueError) as exc:
                raise CharacterPackageError(
                    "not_a_package",
                    f"{name} is not a Yu'lon character file, or it is damaged. Export the "
                    "character again.",
                ) from exc
            if len(signature) > MAX_SIGNATURE_BYTES:
                raise CharacterPackageError(
                    "not_a_package",
                    f"{name} is not a Yu'lon character file, or it is damaged. Export the "
                    "character again.",
                )
    return Package(path=path, manifest=manifest, manifest_bytes=manifest_bytes, signature=signature)


def _hash_stream(fh: IO[bytes], limit: int | None = None) -> tuple[str, int]:
    """SHA-256 and length of a stream; with `limit`, it stops one byte past it."""
    digest = hashlib.sha256()
    size = 0
    read = fh.read
    while True:
        chunk = read(1 << 20)
        if not chunk:
            break
        digest.update(chunk)
        size += len(chunk)
        if limit is not None and size > limit:
            break
    return digest.hexdigest(), size
