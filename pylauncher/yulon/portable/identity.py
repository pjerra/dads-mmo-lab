"""This Yu'lon's own key pair, and the signatures that carry a character between friends (T894).

**What a signature says, and what it does not.** A package's `signature.json` names the key that
signed the member's `manifest.json` and that manifest's exact bytes, so a friend can tell WHO
vouches for the package and that nothing changed on the way. It says nothing about the contents
being earned: a package of hand-edited levels, a character copied from somebody else, or a realm
whose rules were rewritten and then signed is signed just as well as an honest one. The signature
is about provenance and integrity between two people who already paired, never about the contents
being fair.

**The two domains never stand in for each other.** A package signs `PACKAGE_CONTEXT + manifest`
and a request signs `REQUEST_CONTEXT + method, target, key id, time, nonce, body digest`, so a
signature collected for one can never be presented as the other. A held-back signed `/ack` cannot
be sent as `/leave` either: the method and the target (path and query) are inside the request's
message.

**One limit, named rather than hidden (decision 10).** The nonce book lives in memory, so a request
captured in the 300 seconds before a host restarts can be replayed once after it. A book on disk
would trade that for a file that has to be repaired; the window is the honest bound this keeps.
The book also holds a quota per key id (`NONCE_LIMIT_PER_KEY`), so one friend's flood refuses that
friend's requests only, never every other friend's.

**Nothing here logs the seed.** No log line, exception message, `repr` or file other than
`key.json` holds it, and `Identity` never hands it back out.
"""

from __future__ import annotations

import base64
import binascii
import hashlib
import json
import os
import re
import secrets
import socket
import stat
import threading
import time
import unicodedata
from collections.abc import Mapping
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path
from typing import Literal, Protocol

from yulon import platform, winacl
from yulon.log import get_logger

logger = get_logger(__name__)

FORMAT = 1
"""The only file and signature format this module writes or reads."""

ALGORITHM = "ed25519"
"""The only algorithm. Named in every file so a wrong reader refuses rather than guesses."""

DIR_NAME = "identity"
"""Under `platform.config_dir()`: the key and the paired list, beside `credentials/`."""

KEY_FILE = "key.json"
FRIENDS_FILE = "friends.json"

KEY_MODE = 0o600
"""Owner read/write for the key. Set at creation, and repaired on a read that finds it wider."""

DIR_MODE = 0o700
"""Owner only for `identity/`, on POSIX. On Windows the folder's owner-only DACL is what does it."""

FRESH_WINDOW_S = 300
"""How far a request's timestamp may be from now, in either direction: clock drift, not a queue."""

NONCE_LIMIT = 100_000
"""The most unexpired nonces the book holds. At the limit a request is refused, never forgotten."""

NONCE_LIMIT_PER_KEY = 5_000
"""The most unexpired nonces one key id may hold (round B).

The overall limit alone let one friend's flood fill the book and refuse every other friend;
a quota per key id refuses only the key that spends it."""

NAME_MAX = 40
"""The longest display name accepted, matching what a friend's list will show."""

PACKAGE_CONTEXT = b"yulon-package-v1\n"
REQUEST_CONTEXT = b"yulon-request-v1\n"
"""Domain separation: one signature never stands in for the other."""

HEADER_KEY = "X-Yulon-Key"
HEADER_TIME = "X-Yulon-Time"
HEADER_NONCE = "X-Yulon-Nonce"
HEADER_SIGNATURE = "X-Yulon-Signature"

Homecoming = Literal["ask", "always", "never"]
VisitorRule = Literal["allowed", "blocked"]
ReachedBy = Literal["tailscale", "address", ""]

_SELF_CHECK_SEED = bytes.fromhex("9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60")
_SELF_CHECK_PUBLIC = bytes.fromhex(
    "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a"
)
_SELF_CHECK_SIGNATURE = bytes.fromhex(
    "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e06522490155"
    "5fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b"
)
"""RFC 8032 section 7.1, TEST 1: a seed, its public key, and the empty message's signature."""

_TARGET = re.compile(r"^/[\x21-\x7e]*$")
_HEX_32 = re.compile(r"[0-9a-f]{32}")
_TIME = re.compile(r"[0-9]{1,12}")

_KEY_FIELDS = ("format", "algorithm", "seed", "name", "created_at")
_FRIEND_FIELDS = (
    "key_id",
    "public_key",
    "name",
    "reached_by",
    "reached_at",
    "homecoming",
    "homecoming_worlds",
    "visitor",
    "paired_at",
)
_HOMECOMINGS: frozenset[str] = frozenset(("ask", "always", "never"))
_VISITORS: frozenset[str] = frozenset(("allowed", "blocked"))
_REACHED_BY: frozenset[str] = frozenset(("tailscale", "address", ""))


# ------------------------------------------------------------------ refusals


class IdentityError(Exception):
    """Something about this Yu'lon's identity could not be done."""


class BackendMissing(IdentityError):
    """No Ed25519 implementation is importable, so signing cannot happen here."""


class KeyFileDamaged(IdentityError):
    """`key.json` exists and is not a key file this module wrote. Never replaced by a load."""


class KeyNotProtected(IdentityError):
    """The key's folder could not be made private, so no key was written into it (round B)."""


class FriendListDamaged(IdentityError):
    """`friends.json` exists and is not a paired list this module wrote. Never read as empty."""


class Refused(IdentityError):
    """A package or a request that did not pass. `reason` is the short machine-readable code."""

    def __init__(self, reason: str, detail: str = "") -> None:
        self.reason = reason
        super().__init__(f"{reason}: {detail}")


class PackageRefused(Refused):
    """A `signature.json` that does not prove the package it arrived with."""


class RequestRefused(Refused):
    """A request whose headers, signature, time, nonce or sender did not pass."""


# ------------------------------------------------------------------- backend


class Ed25519Backend(Protocol):
    """The one seam Ed25519 math is reached through.

    A vendored pure-Python fallback would be a second implementation of the same math, so it is
    deliberately not built here: without `cryptography` this app can read its files and say
    `BackendMissing`, and cannot sign.
    """

    @property
    def name(self) -> str: ...

    def public_key(self, seed: bytes) -> bytes: ...

    def sign(self, seed: bytes, message: bytes) -> bytes: ...

    def verify(self, public_key: bytes, message: bytes, signature: bytes) -> bool: ...


class CryptographyBackend:
    """Ed25519 through `cryptography`, with every import inside the method that needs it."""

    @property
    def name(self) -> str:
        try:
            import cryptography
        except ImportError as exc:  # pragma: no cover - the seam's own error, driven by a test
            raise BackendMissing("cryptography is not installed") from exc
        return f"cryptography {getattr(cryptography, '__version__', '?')}"

    def public_key(self, seed: bytes) -> bytes:
        try:
            from cryptography.hazmat.primitives.asymmetric import ed25519
        except ImportError as exc:
            raise BackendMissing("cryptography is not installed") from exc
        return ed25519.Ed25519PrivateKey.from_private_bytes(seed).public_key().public_bytes_raw()

    def sign(self, seed: bytes, message: bytes) -> bytes:
        try:
            from cryptography.hazmat.primitives.asymmetric import ed25519
        except ImportError as exc:
            raise BackendMissing("cryptography is not installed") from exc
        return ed25519.Ed25519PrivateKey.from_private_bytes(seed).sign(message)

    def verify(self, public_key: bytes, message: bytes, signature: bytes) -> bool:
        if len(public_key) != 32 or len(signature) != 64:
            return False
        try:
            from cryptography.exceptions import InvalidSignature
            from cryptography.hazmat.primitives.asymmetric import ed25519
        except ImportError as exc:
            raise BackendMissing("cryptography is not installed") from exc
        try:
            ed25519.Ed25519PublicKey.from_public_bytes(public_key).verify(signature, message)
        except (InvalidSignature, ValueError):
            return False
        return True


_backend: Ed25519Backend | None = None
"""The backend this process made once. A test stands in for it by assigning here."""


def get_backend() -> Ed25519Backend:
    """The Ed25519 backend, made on first use."""
    global _backend
    if _backend is None:
        _backend = CryptographyBackend()
    return _backend


# ------------------------------------------------------------------ identity


def key_id_of(public_key: bytes) -> str:
    """A public key's id: the first 32 hex characters of its SHA-256.

    No prefix and no hyphen, so a key id never takes the `<word>-<16 hex>` shape this project's
    generated database passwords have (`test_no_secrets_in_evidence`).
    """
    return hashlib.sha256(public_key).hexdigest()[:32]


def valid_name(name: str) -> str:
    """`name` stripped, if it is 1 to `NAME_MAX` characters with no Unicode category C."""
    stripped = name.strip()
    if not 1 <= len(stripped) <= NAME_MAX:
        raise ValueError(f"a name is 1 to {NAME_MAX} characters")
    if any(unicodedata.category(character)[0] == "C" for character in stripped):
        raise ValueError("a name carries no control or unassigned characters")
    return stripped


def _default_name() -> str:
    """This computer's name, or `"Yu'lon"` when it is empty or not a name this app accepts."""
    try:
        name = socket.gethostname()[:NAME_MAX]
    except OSError:
        return "Yu'lon"
    try:
        return valid_name(name)
    except ValueError:
        return "Yu'lon"


def identity_dir(config_dir: Path | None = None) -> Path:
    """`<config_dir>/identity`, or under `platform.config_dir()` when none is given."""
    root = config_dir if config_dir is not None else platform.config_dir()
    return root / DIR_NAME


class Identity:
    """This Yu'lon's key pair and the name it shows to friends. Never hands the seed back out."""

    __slots__ = ("key_id", "public_key", "name", "_seed")

    def __init__(self, seed: bytes, name: str) -> None:
        if len(seed) != 32:
            raise ValueError("an Ed25519 seed is 32 bytes")
        self._seed = seed
        self.public_key = get_backend().public_key(seed)
        self.key_id = key_id_of(self.public_key)
        self.name = valid_name(name)

    def sign(self, message: bytes) -> bytes:
        """This Yu'lon's 64-byte signature over `message`, exactly as given."""
        return get_backend().sign(self._seed, message)

    def __repr__(self) -> str:
        return f"Identity(key_id={self.key_id!r}, name={self.name!r})"


def _is_format(value: object) -> bool:
    """`True` only for the integer 1: `True == 1` and is not a format."""
    return isinstance(value, int) and not isinstance(value, bool) and value == FORMAT


def _b64(data: bytes) -> str:
    return base64.b64encode(data).decode("ascii")


def _unb64(value: str) -> bytes:
    """Strict standard base64, or `ValueError`."""
    try:
        return base64.b64decode(value, validate=True)
    except (binascii.Error, ValueError) as exc:
        raise ValueError("not standard base64") from exc


def _json_bytes(value: object) -> bytes:
    """The JSON settings every file this module writes uses."""
    return (json.dumps(value, indent=2, sort_keys=True, ensure_ascii=False) + "\n").encode("utf-8")


def _created_at() -> str:
    return datetime.now(UTC).strftime("%Y-%m-%dT%H:%M:%SZ")


def _key_payload(identity: Identity, created_at: str) -> bytes:
    return _json_bytes(
        {
            "format": FORMAT,
            "algorithm": ALGORITHM,
            "seed": _b64(identity._seed),
            "name": identity.name,
            "created_at": created_at,
        }
    )


def _loose_mode(path: Path) -> int | None:
    """On POSIX, the mode `path` has when someone other than its owner may read or write it."""
    if os.name == "nt":
        return None
    try:
        mode = stat.S_IMODE(path.stat().st_mode)
    except OSError as exc:
        logger.warning(f"could not read {path}'s mode: {exc}")
        return None
    return mode if mode & 0o077 else None


def _key_object(path: Path) -> dict[str, object]:
    """`key.json` as the object it must be, or `KeyFileDamaged` naming only the path."""
    damaged = KeyFileDamaged(f"{path} is not a key file this Yu'lon wrote")
    try:
        raw = path.read_bytes()
    except OSError as exc:
        raise KeyFileDamaged(f"{path} could not be read ({type(exc).__name__})") from exc
    try:
        parsed = json.loads(raw)
    except ValueError as exc:
        raise damaged from exc
    if not isinstance(parsed, dict) or set(parsed) != set(_KEY_FIELDS):
        raise damaged
    if not _is_format(parsed["format"]) or parsed["algorithm"] != ALGORITHM:
        raise damaged
    if not isinstance(parsed["seed"], str) or not isinstance(parsed["name"], str):
        raise damaged
    if not isinstance(parsed["created_at"], str):
        raise damaged
    return parsed


def _load_key(path: Path) -> Identity:
    """The key `path` holds, after repairing its POSIX mode if it was found wider than 0o600."""
    loose = _loose_mode(path)
    if loose is not None:
        try:
            os.chmod(path, KEY_MODE)
        except OSError as exc:
            logger.warning(f"could not make {path} owner-only: {exc}")
        else:
            logger.warning(
                f"{path} was readable or writable by others ({loose:03o}); set it back to 0o600"
            )
    parsed = _key_object(path)
    seed_text = parsed["seed"]
    name_text = parsed["name"]
    assert isinstance(seed_text, str) and isinstance(name_text, str)  # checked by `_key_object`
    try:
        seed = _unb64(seed_text)
        name = valid_name(name_text)
    except ValueError as exc:
        raise KeyFileDamaged(f"{path} is not a key file this Yu'lon wrote") from exc
    if len(seed) != 32:
        raise KeyFileDamaged(f"{path} is not a key file this Yu'lon wrote")
    return Identity(seed, name)


def _empty_key_file(path: Path) -> bool:
    """`True` when `key.json` holds zero bytes: a power cut before the first write, not a key."""
    try:
        return path.stat().st_size == 0
    except OSError:
        return False


def load(config_dir: Path | None = None) -> Identity | None:
    """This Yu'lon's key pair, or `None` when `key.json` is not there."""
    folder = identity_dir(config_dir)
    path = folder / KEY_FILE
    if not path.exists():
        return None
    if _empty_key_file(path):
        # Zero bytes is a power cut between `os.open` and the first write, not damage:
        # `load_or_create` removes it and makes the key. Any other content is damaged.
        return None
    winacl.secure_folder(folder, reading=True)
    return _load_key(path)


def _create_key(path: Path, payload: bytes) -> None:
    """Create `key.json` exclusively and owner-only; a failure after the open removes it."""
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_BINARY", 0)
    handle = os.open(path, flags, KEY_MODE)
    try:
        stream = os.fdopen(handle, "wb")
    except BaseException:
        # The stream never took the handle over, so this is the one and only close.
        try:
            os.close(handle)
        except OSError:
            pass
        path.unlink(missing_ok=True)
        raise
    try:
        with stream:
            stream.write(payload)
            stream.flush()
            os.fsync(stream.fileno())
    except BaseException:
        path.unlink(missing_ok=True)
        raise


def load_or_create(config_dir: Path | None = None) -> Identity:
    """This Yu'lon's key pair, made once if absent. An existing key is never replaced."""
    found = load(config_dir)
    if found is not None:
        return found
    folder = identity_dir(config_dir)
    folder.mkdir(mode=DIR_MODE, parents=True, exist_ok=True)
    path = folder / KEY_FILE
    if not winacl.secure_folder_checked(folder):
        # A key anyone in the profile can read is not a private key (round B). Nothing is
        # written: the caller sees why, and a later run on a folder that can be protected works.
        raise KeyNotProtected(
            "Yu'lon could not make its key folder private on this computer, "
            "so it did not make a key."
        )
    if _empty_key_file(path):
        # The name a power cut left behind: empty, so no key ever landed. Remove it, or the
        # exclusive create below would find it and refuse.
        path.unlink(missing_ok=True)
    identity = Identity(secrets.token_bytes(32), _default_name())
    try:
        _create_key(path, _key_payload(identity, _created_at()))
    except FileExistsError:
        other = load(config_dir)
        if other is None:
            raise KeyFileDamaged(f"{path} is there and could not be read") from None
        return other
    platform.fsync_folder(folder)
    logger.info(f"made this Yu'lon's key pair; key id {identity.key_id}")
    return identity


def rename(identity: Identity, name: str, *, config_dir: Path | None = None) -> Identity:
    """Keep the same key pair under a new display name, written atomically over `key.json`."""
    folder = identity_dir(config_dir)
    path = folder / KEY_FILE
    winacl.secure_folder(folder)
    loaded = _load_key(path)
    if identity.public_key != loaded.public_key:
        # A rename is a name change on the key in the file, never a way to replace it.
        raise IdentityError("this is not the key of this Yu'lon")
    parsed = _key_object(path)
    created = parsed["created_at"]
    assert isinstance(created, str)  # checked by `_key_object`
    renamed = Identity(loaded._seed, name)
    platform.write_private_atomically(path, _key_payload(renamed, created))
    logger.info(f"renamed this Yu'lon to {renamed.name!r}; key id {renamed.key_id}")
    return renamed


# ------------------------------------------------------------------ packages


@dataclass(frozen=True)
class Signer:
    """Who a checked package turned out to be, and whether it is this Yu'lon."""

    key_id: str
    name: str
    is_self: bool


def sign_package(identity: Identity, manifest: bytes) -> bytes:
    """`signature.json` for a package whose manifest is `manifest`, byte for byte."""
    return _json_bytes(
        {
            "format": FORMAT,
            "algorithm": ALGORITHM,
            "key_id": identity.key_id,
            "public_key": _b64(identity.public_key),
            "name": identity.name,
            "signature": _b64(identity.sign(PACKAGE_CONTEXT + manifest)),
        }
    )


def _package_signature(signature_json: bytes) -> tuple[str, str, bytes, bytes]:
    """(key id, name, public key, signature) from a `signature.json`, else `format` refused."""
    bad = "not a signature this format wrote"
    try:
        parsed = json.loads(signature_json)
    except ValueError as exc:
        raise PackageRefused("format", bad) from exc
    if not isinstance(parsed, dict):
        raise PackageRefused("format", bad)
    if any(field not in parsed for field in ("key_id", "public_key", "name", "signature")):
        raise PackageRefused("format", bad)
    if not _is_format(parsed.get("format")) or parsed.get("algorithm") != ALGORITHM:
        raise PackageRefused("format", bad)
    key_id = parsed["key_id"]
    public_text = parsed["public_key"]
    name = parsed["name"]
    signature_text = parsed["signature"]
    if not (
        isinstance(key_id, str)
        and isinstance(public_text, str)
        and isinstance(name, str)
        and isinstance(signature_text, str)
    ):
        raise PackageRefused("format", bad)
    try:
        public_key = _unb64(public_text)
        signature = _unb64(signature_text)
    except ValueError as exc:
        raise PackageRefused("format", bad) from exc
    if len(public_key) != 32 or len(signature) != 64:
        raise PackageRefused("format", bad)
    return key_id, name, public_key, signature


def check_package(
    manifest: bytes, signature_json: bytes, *, friends: FriendList, me: Identity
) -> Signer:
    """Who signed `manifest`, or `PackageRefused` naming why it is not trusted here."""
    key_id, _claimed_name, public_key, signature = _package_signature(signature_json)
    if key_id != key_id_of(public_key):
        raise PackageRefused("key-id-mismatch", "the key id is not this public key's fingerprint")
    if not get_backend().verify(public_key, PACKAGE_CONTEXT + manifest, signature):
        raise PackageRefused("bad-signature", "the manifest does not match this signature")
    if key_id == me.key_id and public_key == me.public_key:
        return Signer(me.key_id, me.name, True)
    friend = friends.find(key_id)
    if friend is not None and friend.public_key == public_key:
        # Decision 12: the paired list's name, never the one signature.json carries.
        return Signer(friend.key_id, friend.name, False)
    raise PackageRefused("unknown-signer", f"no paired friend has key id {key_id}")


# ------------------------------------------------------------------ requests


def _request_message(
    method: str, target: str, key_id: str, timestamp: int, nonce: str, body: bytes
) -> bytes:
    """The bytes a request's signature covers; `ValueError` for a method or target it refuses."""
    if method not in ("GET", "POST"):
        raise ValueError(f"{method!r} is not a method this app signs")
    if not _TARGET.fullmatch(target):
        raise ValueError(f"{target!r} is not a target this app signs")
    lines = [method, target, key_id, str(timestamp), nonce, hashlib.sha256(body).hexdigest()]
    return REQUEST_CONTEXT + "\n".join(lines).encode("ascii")


@dataclass(frozen=True)
class SignedRequest:
    """The four headers that carry a signed request."""

    key_id: str
    timestamp: int
    nonce: str
    signature: str

    def headers(self) -> dict[str, str]:
        return {
            HEADER_KEY: self.key_id,
            HEADER_TIME: str(self.timestamp),
            HEADER_NONCE: self.nonce,
            HEADER_SIGNATURE: self.signature,
        }

    @classmethod
    def from_headers(cls, headers: Mapping[str, str]) -> SignedRequest:
        """The headers as a `SignedRequest`, or `RequestRefused` for a missing or malformed one."""
        lowered: dict[str, str] = {}
        for name, value in headers.items():
            folded = str(name).lower()
            if folded in lowered:
                # Two spellings of one name: a verifier that picks one lets the other be
                # read by whoever forwards this. There is no honest sender that sends both.
                raise RequestRefused("malformed", f"two headers are named {folded!r}")
            lowered[folded] = value
        values: dict[str, str] = {}
        for name in (HEADER_KEY, HEADER_TIME, HEADER_NONCE, HEADER_SIGNATURE):
            found = lowered.get(name.lower())
            if found is None:
                raise RequestRefused("missing-header", name)
            if not isinstance(found, str):
                raise RequestRefused("malformed", name)
            values[name] = found
        if not _TIME.fullmatch(values[HEADER_TIME]):
            raise RequestRefused("malformed", HEADER_TIME)
        for name in (HEADER_KEY, HEADER_NONCE):
            if not _HEX_32.fullmatch(values[name]):
                raise RequestRefused("malformed", name)
        try:
            signature = _unb64(values[HEADER_SIGNATURE])
        except ValueError as exc:
            raise RequestRefused("malformed", HEADER_SIGNATURE) from exc
        if len(signature) != 64:
            raise RequestRefused("malformed", HEADER_SIGNATURE)
        return cls(
            key_id=values[HEADER_KEY],
            timestamp=int(values[HEADER_TIME]),
            nonce=values[HEADER_NONCE],
            signature=values[HEADER_SIGNATURE],
        )


def sign_request(
    identity: Identity,
    method: str,
    target: str,
    body: bytes,
    *,
    now: float | None = None,
) -> SignedRequest:
    """Sign one request from this Yu'lon, with a fresh nonce every call."""
    timestamp = int(time.time() if now is None else now)
    nonce = secrets.token_hex(16)
    message = _request_message(method, target, identity.key_id, timestamp, nonce, body)
    return SignedRequest(
        key_id=identity.key_id,
        timestamp=timestamp,
        nonce=nonce,
        signature=_b64(identity.sign(message)),
    )


class NonceBook:
    """The nonces this launch has already spent, per (key id, nonce).

    In memory, so a request captured in the window before this process started can be replayed
    once after a restart. `remember` is called only after a signature has checked, so a forger
    cannot burn a friend's nonces.
    """

    def __init__(self, *, limit: int = NONCE_LIMIT) -> None:
        self._limit = limit
        self._lock = threading.Lock()
        self._seen: dict[tuple[str, str], int] = {}
        self._per_key: dict[str, int] = {}
        self._oldest: int | None = None

    def remember(self, key_id: str, nonce: str, timestamp: int, *, now: int) -> None:
        """Record a checked nonce, or refuse it as reused or as one too many."""
        with self._lock:
            cutoff = now - FRESH_WINDOW_S
            if self._oldest is not None and self._oldest < cutoff:
                for spent, at in list(self._seen.items()):
                    if at < cutoff:
                        del self._seen[spent]
                        left = self._per_key[spent[0]] - 1
                        if left:
                            self._per_key[spent[0]] = left
                        else:
                            del self._per_key[spent[0]]
                self._oldest = min(self._seen.values(), default=None)
            key = (key_id, nonce)
            if key in self._seen:
                raise RequestRefused("reused-nonce", "this nonce was already spent")
            if len(self._seen) >= self._limit:
                raise RequestRefused("nonce-book-full", "too many unexpired nonces")
            if self._per_key.get(key_id, 0) >= NONCE_LIMIT_PER_KEY:
                # A quota per key id: a flood refuses its own key, never the book.
                raise RequestRefused(
                    "nonce-book-full", f"key id {key_id} has too many unexpired nonces"
                )
            self._seen[key] = timestamp
            self._per_key[key_id] = self._per_key.get(key_id, 0) + 1
            self._oldest = timestamp if self._oldest is None else min(self._oldest, timestamp)

    def __len__(self) -> int:
        with self._lock:
            return len(self._seen)


def check_request(
    method: str,
    target: str,
    body: bytes,
    headers: Mapping[str, str],
    *,
    friends: FriendList,
    nonces: NonceBook,
    now: float | None = None,
) -> Friend:
    """A checked request's sender, or `RequestRefused` naming the check that refused it."""
    signed = SignedRequest.from_headers(headers)
    if method not in ("GET", "POST") or not _TARGET.fullmatch(target):
        raise RequestRefused("malformed", "the method or target is not one this app accepts")
    friend = friends.find(signed.key_id)
    if friend is None:
        raise RequestRefused("unknown-key", f"no paired friend has key id {signed.key_id}")
    if friend.visitor == "blocked":
        # The reason goes back over the wire: name the key, never the name that friend chose.
        raise RequestRefused(
            "blocked", f"the friend with key id {signed.key_id} is not allowed to visit"
        )
    current = int(time.time() if now is None else now)
    if abs(current - signed.timestamp) > FRESH_WINDOW_S:
        raise RequestRefused("stale", "the timestamp is outside the fresh window")
    message = _request_message(method, target, signed.key_id, signed.timestamp, signed.nonce, body)
    try:
        signature = _unb64(signed.signature)
    except ValueError as exc:  # from_headers already checked it; kept for the type's honesty
        raise RequestRefused("malformed", HEADER_SIGNATURE) from exc
    if not get_backend().verify(friend.public_key, message, signature):
        raise RequestRefused("bad-signature", "the request does not match this signature")
    nonces.remember(signed.key_id, signed.nonce, signed.timestamp, now=current)
    return friend


# --------------------------------------------------------------- paired list


@dataclass(frozen=True)
class Friend:
    """One paired Yu'lon: its key, the name it chose, and the visitor rules this host keeps."""

    key_id: str
    public_key: bytes
    name: str
    reached_by: ReachedBy = ""
    reached_at: str = ""
    homecoming: Homecoming = "ask"
    homecoming_worlds: tuple[tuple[str, Homecoming], ...] = ()
    visitor: VisitorRule = "allowed"
    paired_at: str = ""

    def __post_init__(self) -> None:
        """A friend's key id is its key's fingerprint, and a public key is 32 bytes.

        The list's one invariant, kept where a `Friend` is made (round B): `friends.json`
        is checked on load as well, so a damaged file is `FriendListDamaged` and never a
        `Friend` that quietly holds somebody else's key under a known id.
        """
        if len(self.public_key) != 32:
            raise ValueError("a public key is 32 bytes")
        if self.key_id != key_id_of(self.public_key):
            raise ValueError(f"key id {self.key_id} is not this public key's fingerprint")

    def homecoming_for(self, world_id: str) -> Homecoming:
        """The setting for `world_id`: its override if it has one, else this friend's default."""
        for world, setting in self.homecoming_worlds:
            if world == world_id:
                return setting
        return self.homecoming


def _friend_object(entry: object, path: Path) -> Friend:
    """One paired-list entry as a `Friend`, or `FriendListDamaged`."""
    damaged = FriendListDamaged(f"{path} is not a paired list this Yu'lon wrote")
    if not isinstance(entry, dict):
        raise damaged
    if any(field not in entry for field in _FRIEND_FIELDS):
        raise damaged
    key_id = entry["key_id"]
    public_text = entry["public_key"]
    name = entry["name"]
    reached_by = entry["reached_by"]
    reached_at = entry["reached_at"]
    homecoming = entry["homecoming"]
    worlds = entry["homecoming_worlds"]
    visitor = entry["visitor"]
    paired_at = entry["paired_at"]
    if not (
        isinstance(key_id, str)
        and isinstance(public_text, str)
        and isinstance(name, str)
        and isinstance(reached_at, str)
        and isinstance(paired_at, str)
        and isinstance(reached_by, str)
        and isinstance(homecoming, str)
        and isinstance(visitor, str)
        and isinstance(worlds, dict)
    ):
        raise damaged
    if reached_by not in _REACHED_BY or homecoming not in _HOMECOMINGS:
        raise damaged
    if visitor not in _VISITORS:
        raise damaged
    try:
        public_key = _unb64(public_text)
        clean_name = valid_name(name)
    except ValueError as exc:
        raise damaged from exc
    if len(public_key) != 32 or key_id != key_id_of(public_key):
        raise damaged
    overrides: list[tuple[str, Homecoming]] = []
    for world, setting in worlds.items():
        if not isinstance(world, str) or not isinstance(setting, str):
            raise damaged
        if setting not in _HOMECOMINGS:
            raise damaged
        overrides.append((world, setting))  # type: ignore[arg-type]
    return Friend(
        key_id=key_id,
        public_key=public_key,
        name=clean_name,
        reached_by=reached_by,  # type: ignore[arg-type]
        reached_at=reached_at,
        homecoming=homecoming,  # type: ignore[arg-type]
        homecoming_worlds=tuple(sorted(overrides)),
        visitor=visitor,  # type: ignore[arg-type]
        paired_at=paired_at,
    )


def _friend_json(friend: Friend) -> dict[str, object]:
    return {
        "key_id": friend.key_id,
        "public_key": _b64(friend.public_key),
        "name": friend.name,
        "reached_by": friend.reached_by,
        "reached_at": friend.reached_at,
        "homecoming": friend.homecoming,
        "homecoming_worlds": dict(friend.homecoming_worlds),
        "visitor": friend.visitor,
        "paired_at": friend.paired_at,
    }


@dataclass(frozen=True)
class FriendList:
    """The Yu'lons paired with this one. Not secret: names and public keys all."""

    friends: tuple[Friend, ...] = ()

    @classmethod
    def load(cls, config_dir: Path | None = None) -> FriendList:
        """The list on disk; empty when there is none, `FriendListDamaged` if it will not parse."""
        folder = identity_dir(config_dir)
        path = folder / FRIENDS_FILE
        if not path.is_file():
            return cls()
        winacl.secure_folder(folder, reading=True)
        damaged = FriendListDamaged(f"{path} is not a paired list this Yu'lon wrote")
        try:
            raw = path.read_bytes()
        except OSError as exc:
            raise FriendListDamaged(f"{path} could not be read ({type(exc).__name__})") from exc
        try:
            parsed = json.loads(raw)
        except ValueError as exc:
            raise damaged from exc
        if not isinstance(parsed, dict) or not _is_format(parsed.get("format")):
            raise damaged
        entries = parsed.get("friends")
        if not isinstance(entries, list):
            raise damaged
        return cls(tuple(_friend_object(entry, path) for entry in entries))

    def find(self, key_id: str) -> Friend | None:
        """The friend with `key_id`, or `None`."""
        return next((friend for friend in self.friends if friend.key_id == key_id), None)

    def with_friend(self, friend: Friend) -> FriendList:
        """This list with `friend` replacing its own key id, never another public key under it."""
        existing = self.find(friend.key_id)
        if existing is None:
            return FriendList((*self.friends, friend))
        if existing.public_key != friend.public_key:
            raise ValueError(f"key id {friend.key_id} already belongs to another key")
        return FriendList(
            tuple(friend if other.key_id == friend.key_id else other for other in self.friends)
        )

    def without(self, key_id: str) -> FriendList:
        """This list without the entry whose key id is `key_id`."""
        return FriendList(tuple(friend for friend in self.friends if friend.key_id != key_id))

    def save(self, config_dir: Path | None = None) -> Path:
        """Write the list atomically, owner-only, and answer where it went."""
        folder = identity_dir(config_dir)
        folder.mkdir(mode=DIR_MODE, parents=True, exist_ok=True)
        winacl.secure_folder(folder)
        path = folder / FRIENDS_FILE
        payload = _json_bytes(
            {"format": FORMAT, "friends": [_friend_json(friend) for friend in self.friends]}
        )
        platform.write_private_atomically(path, payload)
        return path


# ---------------------------------------------------------------- self check


def self_check() -> str:
    """Prove signing and checking work in memory, or raise `IdentityError`. Writes nothing."""
    backend = get_backend()
    if backend.public_key(_SELF_CHECK_SEED) != _SELF_CHECK_PUBLIC:
        raise IdentityError("the backend's public key does not match RFC 8032's test vector")
    if backend.sign(_SELF_CHECK_SEED, b"") != _SELF_CHECK_SIGNATURE:
        raise IdentityError("the backend's signature does not match RFC 8032's test vector")
    identity = Identity(secrets.token_bytes(32), "self-check")
    manifest = b"a manifest for the signing self-check"
    signature = sign_package(identity, manifest)
    signer = check_package(manifest, signature, friends=FriendList(), me=identity)
    if not signer.is_self:
        raise IdentityError("a package signed with this key was not recognised as this Yu'lon's")
    changed = bytes([manifest[0] ^ 1]) + manifest[1:]
    try:
        check_package(changed, signature, friends=FriendList(), me=identity)
    except Refused as exc:
        if exc.reason != "bad-signature":
            raise IdentityError(
                f"a changed manifest was refused as {exc.reason!r}, not 'bad-signature'"
            ) from exc
    else:
        raise IdentityError("a changed manifest passed the package check")
    return f"signing self-check: ok (ed25519 through {backend.name})"
