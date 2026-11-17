"""Play Together's identity: key pairs, package signatures and signed requests (T894).

The failing case comes first in each group: a package one byte different, a request whose body,
method or target changed, a key file that would otherwise be replaced. Every test drives the real
module; the one seam stood in for is the Ed25519 backend, and even that only where the test is
about the seam itself.
"""

from __future__ import annotations

import base64
import hashlib
import json
import logging
import os
import secrets
import subprocess
import sys
from pathlib import Path

import pytest

from yulon.portable import identity

ROOT = Path(__file__).resolve().parents[1]
NOW = 1_700_000_000.0
"""A fixed instant: every request test's freshness is then a fact about the code, not the clock."""

SEED = bytes.fromhex("9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60")


def _seed(fill: int) -> bytes:
    return bytes([fill]) * 32


def _me(fill: int = 7, name: str = "Me") -> identity.Identity:
    return identity.Identity(_seed(fill), name)


def _friend(
    person: identity.Identity,
    *,
    visitor: identity.VisitorRule = "allowed",
    homecoming: identity.Homecoming = "ask",
    worlds: tuple[tuple[str, identity.Homecoming], ...] = (),
) -> identity.Friend:
    return identity.Friend(
        key_id=person.key_id,
        public_key=person.public_key,
        name=person.name,
        visitor=visitor,
        homecoming=homecoming,
        homecoming_worlds=worlds,
    )


def _paired(*people: identity.Identity) -> identity.FriendList:
    friends = identity.FriendList()
    for person in people:
        friends = friends.with_friend(_friend(person))
    return friends


def _headers(
    person: identity.Identity,
    method: str = "GET",
    target: str = "/state",
    body: bytes = b"",
    *,
    nonce: str | None = None,
    now: float = NOW,
) -> dict[str, str]:
    """One correctly signed request's headers, with a chosen nonce or a fresh one."""
    chosen = nonce if nonce is not None else secrets.token_hex(16)
    message = identity._request_message(method, target, person.key_id, int(now), chosen, body)
    return identity.SignedRequest(
        key_id=person.key_id,
        timestamp=int(now),
        nonce=chosen,
        signature=base64.b64encode(person.sign(message)).decode("ascii"),
    ).headers()


def _json(payload: bytes) -> dict[str, object]:
    parsed = json.loads(payload)
    assert isinstance(parsed, dict)
    return parsed


def _with_signature_byte(signature_json: bytes, index: int = 0) -> bytes:
    parsed = _json(signature_json)
    raw = bytearray(base64.b64decode(str(parsed["signature"])))
    raw[index] ^= 1
    parsed["signature"] = base64.b64encode(bytes(raw)).decode("ascii")
    return json.dumps(parsed).encode()


def _seed_spellings(seed: bytes) -> set[str]:
    """Every way a seed could leak as text: hex, both base64s, raw bytes, its repr."""
    return {
        seed.hex(),
        base64.b64encode(seed).decode("ascii"),
        base64.urlsafe_b64encode(seed).decode("ascii"),
        seed.decode("latin-1"),
        repr(seed),
    }


# ---------------------------------------------------------------- packages


def test_a_friends_package_checks_and_names_the_friend() -> None:
    friend = identity.Identity(_seed(1), "Mira")
    me = _me()
    signature = identity.sign_package(friend, b"a manifest")
    signer = identity.check_package(b"a manifest", signature, friends=_paired(friend), me=me)
    assert signer == identity.Signer(friend.key_id, "Mira", False)


def test_one_changed_manifest_byte_is_refused() -> None:
    friend = identity.Identity(_seed(1), "Mira")
    signature = identity.sign_package(friend, b"a manifest")
    changed = bytes([b"a manifest"[0] ^ 1]) + b"a manifest"[1:]
    with pytest.raises(identity.PackageRefused) as refused:
        identity.check_package(changed, signature, friends=_paired(friend), me=_me())
    assert refused.value.reason == "bad-signature"


def test_one_changed_signature_byte_is_refused() -> None:
    friend = identity.Identity(_seed(1), "Mira")
    signature = identity.sign_package(friend, b"a manifest")
    with pytest.raises(identity.PackageRefused) as refused:
        identity.check_package(
            b"a manifest", _with_signature_byte(signature), friends=_paired(friend), me=_me()
        )
    assert refused.value.reason == "bad-signature"


def test_a_package_from_an_unpaired_key_is_refused() -> None:
    stranger = identity.Identity(_seed(2), "Stranger")
    signature = identity.sign_package(stranger, b"a manifest")
    with pytest.raises(identity.PackageRefused) as refused:
        identity.check_package(b"a manifest", signature, friends=_paired(), me=_me())
    assert refused.value.reason == "unknown-signer"


def test_a_known_key_id_with_another_key_is_refused() -> None:
    stranger = identity.Identity(_seed(2), "Stranger")
    impostor = identity.Identity(_seed(3), "Impostor")
    signature = identity.sign_package(stranger, b"a manifest")
    friends = identity.FriendList().with_friend(
        identity.Friend(key_id=stranger.key_id, public_key=impostor.public_key, name="Stranger")
    )
    with pytest.raises(identity.PackageRefused) as refused:
        identity.check_package(b"a manifest", signature, friends=friends, me=_me())
    assert refused.value.reason == "unknown-signer"


def test_a_key_id_that_is_not_the_fingerprint_is_refused() -> None:
    friend = identity.Identity(_seed(1), "Mira")
    parsed = _json(identity.sign_package(friend, b"a manifest"))
    parsed["key_id"] = "0" * 32
    with pytest.raises(identity.PackageRefused) as refused:
        identity.check_package(
            b"a manifest", json.dumps(parsed).encode(), friends=_paired(friend), me=_me()
        )
    assert refused.value.reason == "key-id-mismatch"


def test_a_package_signed_here_is_its_own() -> None:
    me = _me()
    signature = identity.sign_package(me, b"a manifest")
    signer = identity.check_package(b"a manifest", signature, friends=_paired(), me=me)
    assert signer == identity.Signer(me.key_id, me.name, True)


def test_the_name_in_the_signature_file_is_not_believed() -> None:
    friend = identity.Identity(_seed(1), "Mira")
    parsed = _json(identity.sign_package(friend, b"a manifest"))
    parsed["name"] = "Somebody Else"
    signer = identity.check_package(
        b"a manifest",
        json.dumps(parsed).encode(),
        friends=_paired(friend),
        me=_me(),
    )
    assert signer.name == "Mira"


def test_a_newer_signature_format_is_refused() -> None:
    friend = identity.Identity(_seed(1), "Mira")
    parsed = _json(identity.sign_package(friend, b"a manifest"))
    parsed["format"] = 2
    with pytest.raises(identity.PackageRefused) as refused:
        identity.check_package(
            b"a manifest", json.dumps(parsed).encode(), friends=_paired(friend), me=_me()
        )
    assert refused.value.reason == "format"


# ---------------------------------------------------------------- requests


def test_a_paired_friends_request_is_accepted() -> None:
    friend = identity.Identity(_seed(1), "Mira")
    friend_entry = _friend(friend)
    headers = {name.lower(): value for name, value in _headers(friend).items()}
    sender = identity.check_request(
        "GET",
        "/state",
        b"",
        headers,
        friends=identity.FriendList().with_friend(friend_entry),
        nonces=identity.NonceBook(),
        now=NOW,
    )
    assert sender == friend_entry


def test_an_unknown_key_is_refused() -> None:
    stranger = identity.Identity(_seed(2), "Stranger")
    with pytest.raises(identity.RequestRefused) as refused:
        identity.check_request(
            "GET",
            "/state",
            b"",
            _headers(stranger),
            friends=_paired(),
            nonces=identity.NonceBook(),
            now=NOW,
        )
    assert refused.value.reason == "unknown-key"


def test_an_old_timestamp_is_refused() -> None:
    friend = identity.Identity(_seed(1), "Mira")
    friends = _paired(friend)
    with pytest.raises(identity.RequestRefused) as refused:
        identity.check_request(
            "GET",
            "/state",
            b"",
            _headers(friend, now=NOW - 301),
            friends=friends,
            nonces=identity.NonceBook(),
            now=NOW,
        )
    assert refused.value.reason == "stale"
    # Exactly the window is still fresh.
    identity.check_request(
        "GET",
        "/state",
        b"",
        _headers(friend, now=NOW - 300),
        friends=friends,
        nonces=identity.NonceBook(),
        now=NOW,
    )


def test_a_future_timestamp_is_refused() -> None:
    friend = identity.Identity(_seed(1), "Mira")
    with pytest.raises(identity.RequestRefused) as refused:
        identity.check_request(
            "GET",
            "/state",
            b"",
            _headers(friend, now=NOW + 301),
            friends=_paired(friend),
            nonces=identity.NonceBook(),
            now=NOW,
        )
    assert refused.value.reason == "stale"


def test_a_reused_nonce_is_refused() -> None:
    friend = identity.Identity(_seed(1), "Mira")
    headers = _headers(friend, nonce="ab" * 16)
    book = identity.NonceBook()
    identity.check_request(
        "GET", "/state", b"", headers, friends=_paired(friend), nonces=book, now=NOW
    )
    with pytest.raises(identity.RequestRefused) as refused:
        identity.check_request(
            "GET", "/state", b"", headers, friends=_paired(friend), nonces=book, now=NOW
        )
    assert refused.value.reason == "reused-nonce"


@pytest.mark.parametrize("change", ["body", "method", "target"])
def test_a_changed_body_method_or_target_is_refused(change: str) -> None:
    friend = identity.Identity(_seed(1), "Mira")
    headers = _headers(friend)
    body, method, target = b"", "GET", "/state"
    if change == "body":
        body = b"x"
    elif change == "method":
        method = "POST"
    else:
        target = "/state?after=1"
    with pytest.raises(identity.RequestRefused) as refused:
        identity.check_request(
            method,
            target,
            body,
            headers,
            friends=_paired(friend),
            nonces=identity.NonceBook(),
            now=NOW,
        )
    assert refused.value.reason == "bad-signature"


def test_a_blocked_visitor_is_refused() -> None:
    friend = identity.Identity(_seed(1), "Mira")
    friends = identity.FriendList().with_friend(_friend(friend, visitor="blocked"))
    with pytest.raises(identity.RequestRefused) as refused:
        identity.check_request(
            "GET",
            "/state",
            b"",
            _headers(friend),
            friends=friends,
            nonces=identity.NonceBook(),
            now=NOW,
        )
    assert refused.value.reason == "blocked"


def test_a_bad_signature_does_not_spend_the_nonce() -> None:
    friend = identity.Identity(_seed(1), "Mira")
    nonce = "cd" * 16
    good = _headers(friend, nonce=nonce)
    bad = dict(good)
    raw = bytearray(base64.b64decode(bad[identity.HEADER_SIGNATURE]))
    raw[0] ^= 1
    bad[identity.HEADER_SIGNATURE] = base64.b64encode(bytes(raw)).decode("ascii")
    book = identity.NonceBook()
    with pytest.raises(identity.RequestRefused) as refused:
        identity.check_request(
            "GET", "/state", b"", bad, friends=_paired(friend), nonces=book, now=NOW
        )
    assert refused.value.reason == "bad-signature"
    assert len(book) == 0, "a forger burned a friend's nonce"
    identity.check_request(
        "GET", "/state", b"", good, friends=_paired(friend), nonces=book, now=NOW
    )
    assert len(book) == 1


def test_a_full_nonce_book_refuses() -> None:
    friend = identity.Identity(_seed(1), "Mira")
    friends = _paired(friend)
    book = identity.NonceBook(limit=1)
    identity.check_request(
        "GET", "/state", b"", _headers(friend), friends=friends, nonces=book, now=NOW
    )
    with pytest.raises(identity.RequestRefused) as refused:
        identity.check_request(
            "GET", "/state", b"", _headers(friend), friends=friends, nonces=book, now=NOW
        )
    assert refused.value.reason == "nonce-book-full"


def test_old_nonces_are_forgotten() -> None:
    book = identity.NonceBook()
    book.remember("a" * 32, "b" * 32, int(NOW), now=int(NOW))
    assert len(book) == 1
    book.remember(
        "c" * 32,
        "d" * 32,
        int(NOW + identity.FRESH_WINDOW_S + 1),
        now=int(NOW + identity.FRESH_WINDOW_S + 1),
    )
    assert len(book) == 1, "the nonce from outside the window was kept"


_HEADER_CASES: dict[str, tuple[str, str | None]] = {
    "missing-key": (identity.HEADER_KEY, None),
    "missing-time": (identity.HEADER_TIME, None),
    "missing-nonce": (identity.HEADER_NONCE, None),
    "missing-signature": (identity.HEADER_SIGNATURE, None),
    "time-not-a-number": (identity.HEADER_TIME, "soon"),
    "time-too-long": (identity.HEADER_TIME, "1" * 13),
    "key-not-hex": (identity.HEADER_KEY, "zz" * 16),
    "nonce-not-hex": (identity.HEADER_NONCE, "AB" * 16),
    "signature-not-base64": (identity.HEADER_SIGNATURE, "!!!!"),
    "signature-too-short": (
        identity.HEADER_SIGNATURE,
        base64.b64encode(b"x" * 32).decode(),
    ),
}


@pytest.mark.parametrize("case", list(_HEADER_CASES))
def test_a_missing_or_malformed_header_is_refused(case: str) -> None:
    friend = identity.Identity(_seed(1), "Mira")
    headers = _headers(friend)
    name, value = _HEADER_CASES[case]
    if value is None:
        del headers[name]
        expected = "missing-header"
    else:
        headers[name] = value
        expected = "malformed"
    with pytest.raises(identity.RequestRefused) as refused:
        identity.check_request(
            "GET",
            "/state",
            b"",
            headers,
            friends=_paired(friend),
            nonces=identity.NonceBook(),
            now=NOW,
        )
    assert refused.value.reason == expected


def test_a_package_signature_is_not_a_request_signature() -> None:
    """The domain prefixes are the only difference: without them this one WOULD be a request.

    The manifest is deliberately the request's own signed payload, byte for byte, so a
    package signature and this request's signature cover the same text once the two
    domains are removed. That is what makes this test about the separation itself.
    """
    friend = identity.Identity(_seed(1), "Mira")
    nonce = "ef" * 16
    plain = "\n".join(
        ["GET", "/state", friend.key_id, str(int(NOW)), nonce, hashlib.sha256(b"").hexdigest()]
    )
    package = _json(identity.sign_package(friend, plain.encode("ascii")))
    headers = {
        identity.HEADER_KEY: friend.key_id,
        identity.HEADER_TIME: str(int(NOW)),
        identity.HEADER_NONCE: nonce,
        identity.HEADER_SIGNATURE: str(package["signature"]),
    }
    with pytest.raises(identity.RequestRefused) as refused:
        identity.check_request(
            "GET",
            "/state",
            b"",
            headers,
            friends=_paired(friend),
            nonces=identity.NonceBook(),
            now=NOW,
        )
    assert refused.value.reason == "bad-signature"

    # The other direction: a request signature presented as a package signature, with the
    # manifest equal to the request's signed bytes. Only `PACKAGE_CONTEXT` stands in the way.
    signed = identity.sign_request(friend, "GET", "/state", b"", now=NOW)
    request_bytes = identity.REQUEST_CONTEXT + "\n".join(
        [
            "GET",
            "/state",
            friend.key_id,
            str(int(NOW)),
            signed.nonce,
            hashlib.sha256(b"").hexdigest(),
        ]
    ).encode("ascii")
    signature_file = json.dumps(
        {
            "format": identity.FORMAT,
            "algorithm": identity.ALGORITHM,
            "key_id": friend.key_id,
            "public_key": base64.b64encode(friend.public_key).decode("ascii"),
            "name": friend.name,
            "signature": signed.signature,
        }
    ).encode()
    with pytest.raises(identity.PackageRefused) as refused:
        identity.check_package(request_bytes, signature_file, friends=_paired(friend), me=_me())
    assert refused.value.reason == "bad-signature"


# ---------------------------------------------------------------- key file


def test_the_key_file_is_created_exclusive_and_owner_only(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    real_open = os.open
    calls: list[tuple[Path, int, int]] = []

    def watched(path: object, flags: int, mode: int = 0o777, *, dir_fd: int | None = None) -> int:
        calls.append((Path(str(path)), flags, mode))
        return real_open(path, flags, mode, dir_fd=dir_fd)  # type: ignore[arg-type]

    monkeypatch.setattr(identity.os, "open", watched)
    identity.load_or_create(tmp_path)
    key_calls = [call for call in calls if call[0].name == identity.KEY_FILE]
    assert key_calls, "key.json was never created through os.open"
    for path, flags, mode in key_calls:
        assert flags & os.O_EXCL, f"{path} was created without O_EXCL"
        assert flags & os.O_CREAT
        assert mode == 0o600


@pytest.mark.skipif(sys.platform == "win32", reason="a POSIX mode is what this asserts")
def test_the_key_file_and_folder_are_owner_only_on_posix(tmp_path: Path) -> None:
    identity.load_or_create(tmp_path)
    folder = tmp_path / identity.DIR_NAME
    assert (folder / identity.KEY_FILE).stat().st_mode & 0o777 == 0o600
    assert folder.stat().st_mode & 0o777 == 0o700


@pytest.mark.skipif(sys.platform == "win32", reason="a POSIX mode is what this asserts")
def test_a_loosened_key_file_is_made_owner_only_again(tmp_path: Path) -> None:
    made = identity.load_or_create(tmp_path)
    key = tmp_path / identity.DIR_NAME / identity.KEY_FILE
    os.chmod(key, 0o644)
    assert identity.load(tmp_path) is not None
    assert key.stat().st_mode & 0o777 == identity.KEY_MODE
    assert identity.load(tmp_path).key_id == made.key_id  # type: ignore[union-attr]


def test_a_second_load_returns_the_same_key(tmp_path: Path) -> None:
    made = identity.load_or_create(tmp_path)
    again = identity.load(tmp_path)
    assert again is not None
    assert again.key_id == made.key_id
    assert again.public_key == made.public_key


def test_an_existing_key_is_never_replaced(tmp_path: Path) -> None:
    other = identity.Identity(_seed(9), "Other")
    folder = tmp_path / identity.DIR_NAME
    folder.mkdir()
    key = folder / identity.KEY_FILE
    key.write_bytes(identity._key_payload(other, "2020-01-01T00:00:00Z"))
    before = key.read_bytes()
    found = identity.load_or_create(tmp_path)
    assert found.key_id == other.key_id
    assert key.read_bytes() == before


def test_a_damaged_key_file_is_refused_not_replaced(tmp_path: Path) -> None:
    folder = tmp_path / identity.DIR_NAME
    folder.mkdir()
    key = folder / identity.KEY_FILE
    damaged = b"{not a key file"
    key.write_bytes(damaged)
    with pytest.raises(identity.KeyFileDamaged):
        identity.load(tmp_path)
    with pytest.raises(identity.KeyFileDamaged):
        identity.load_or_create(tmp_path)
    assert key.read_bytes() == damaged


def test_a_failed_write_leaves_no_key_file(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    def refuse(fd: int) -> None:
        raise OSError("no room on the disk")

    monkeypatch.setattr(identity.os, "fsync", refuse)
    with pytest.raises(OSError):
        identity.load_or_create(tmp_path)
    assert not (tmp_path / identity.DIR_NAME / identity.KEY_FILE).exists()
    assert identity.load(tmp_path) is None


def test_rename_keeps_the_key(tmp_path: Path) -> None:
    made = identity.load_or_create(tmp_path)
    key = tmp_path / identity.DIR_NAME / identity.KEY_FILE
    created = _json(key.read_bytes())["created_at"]
    renamed = identity.rename(made, "A New Name", config_dir=tmp_path)
    assert renamed.key_id == made.key_id
    assert renamed.name == "A New Name"
    reloaded = identity.load(tmp_path)
    assert reloaded is not None
    assert reloaded.key_id == made.key_id
    assert reloaded.name == "A New Name"
    assert _json(key.read_bytes())["created_at"] == created


# ----------------------------------------------------------------- friends


def test_the_paired_list_survives_a_save_and_load(tmp_path: Path) -> None:
    mira = identity.Identity(_seed(1), "Mira")
    jon = identity.Identity(_seed(2), "Jon")
    friends = identity.FriendList(
        (
            identity.Friend(
                key_id=mira.key_id,
                public_key=mira.public_key,
                name="Mira",
                reached_by="tailscale",
                reached_at="mira-laptop",
                homecoming="always",
                homecoming_worlds=(("wow-tbc", "never"), ("wow-vanilla", "always")),
                visitor="blocked",
                paired_at="2026-01-02T03:04:05Z",
            ),
            _friend(jon),
        )
    )
    path = friends.save(tmp_path)
    assert path == tmp_path / identity.DIR_NAME / identity.FRIENDS_FILE
    assert identity.FriendList.load(tmp_path) == friends


def test_a_damaged_paired_list_is_refused_not_emptied(tmp_path: Path) -> None:
    folder = tmp_path / identity.DIR_NAME
    folder.mkdir()
    path = folder / identity.FRIENDS_FILE
    damaged = b"{not a paired list"
    path.write_bytes(damaged)
    with pytest.raises(identity.FriendListDamaged):
        identity.FriendList.load(tmp_path)
    assert path.read_bytes() == damaged


def test_a_world_override_wins_over_the_friends_homecoming() -> None:
    mira = identity.Identity(_seed(1), "Mira")
    friend = _friend(mira, homecoming="ask", worlds=(("wow-tbc", "always"),))
    assert friend.homecoming_for("wow-tbc") == "always"
    assert friend.homecoming_for("wow-vanilla") == "ask"


def test_a_new_key_under_a_known_key_id_is_refused() -> None:
    mira = identity.Identity(_seed(1), "Mira")
    impostor = identity.Identity(_seed(2), "Impostor")
    friends = identity.FriendList().with_friend(_friend(mira))
    with pytest.raises(ValueError):
        friends.with_friend(
            identity.Friend(key_id=mira.key_id, public_key=impostor.public_key, name="Impostor")
        )


# ----------------------------------------------------------------- secrets


def _exercise_identity(tmp_path: Path) -> identity.Identity:
    """Every operation that touches the seed, for the log and file searches."""
    me = identity.load_or_create(tmp_path)
    identity.load(tmp_path)
    identity.rename(me, "Renamed", config_dir=tmp_path)
    friend = identity.Identity(_seed(2), "Mira")
    signature = identity.sign_package(me, b"a manifest")
    identity.check_package(
        b"a manifest", signature, friends=identity.FriendList().with_friend(_friend(me)), me=friend
    )
    request = identity.sign_request(me, "POST", "/visit", b"body", now=NOW)
    identity.check_request(
        "POST",
        "/visit",
        b"body",
        request.headers(),
        friends=identity.FriendList().with_friend(_friend(me)),
        nonces=identity.NonceBook(),
        now=NOW,
    )
    return me


def test_the_seed_is_never_logged(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, caplog: pytest.LogCaptureFixture
) -> None:
    real_bytes = secrets.token_bytes
    monkeypatch.setattr(
        identity.secrets, "token_bytes", lambda length: SEED if length == 32 else real_bytes(length)
    )
    caplog.set_level(logging.DEBUG, logger="yulon")
    _exercise_identity(tmp_path)
    damaged_dir = tmp_path / "damaged"
    (damaged_dir / identity.DIR_NAME).mkdir(parents=True)
    (damaged_dir / identity.DIR_NAME / identity.KEY_FILE).write_bytes(b"{}")
    with pytest.raises(identity.KeyFileDamaged):
        identity.load(damaged_dir)
    said = "\n".join(record.getMessage() for record in caplog.records)
    said += "\n" + caplog.text
    for spelling in _seed_spellings(SEED):
        assert spelling not in said, "the private seed reached the log"


def test_the_seed_is_not_in_repr_signature_file_or_headers(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    real_bytes = secrets.token_bytes
    monkeypatch.setattr(
        identity.secrets, "token_bytes", lambda length: SEED if length == 32 else real_bytes(length)
    )
    me = identity.load_or_create(tmp_path)
    friend = _friend(identity.Identity(_seed(2), "Mira"))
    spellings = _seed_spellings(SEED)
    shown = repr(me) + repr(friend)
    signature_file = identity.sign_package(me, b"a manifest")
    headers = identity.sign_request(me, "GET", "/state", b"", now=NOW).headers()
    visible = shown.encode() + signature_file + json.dumps(headers).encode()
    for spelling in spellings:
        assert spelling.encode("latin-1", errors="ignore") not in visible
    # Control: the file that must hold it does, so the search above is not vacuous.
    key = tmp_path / identity.DIR_NAME / identity.KEY_FILE
    assert base64.b64encode(SEED).decode("ascii") in key.read_text(encoding="utf-8")


# ------------------------------------------------------- the seam and smoke


class _Trusting:
    """A backend that verifies everything: the one mistake the self-check exists to catch."""

    name = "trusting"

    def __init__(self, real: identity.CryptographyBackend) -> None:
        self.real = real

    def public_key(self, seed: bytes) -> bytes:
        return self.real.public_key(seed)

    def sign(self, seed: bytes, message: bytes) -> bytes:
        return self.real.sign(seed, message)

    def verify(self, public_key: bytes, message: bytes, signature: bytes) -> bool:
        return True


class _Missing:
    name = "missing"

    def public_key(self, seed: bytes) -> bytes:
        raise identity.BackendMissing("cryptography is not installed")

    def sign(self, seed: bytes, message: bytes) -> bytes:
        raise identity.BackendMissing("cryptography is not installed")

    def verify(self, public_key: bytes, message: bytes, signature: bytes) -> bool:
        raise identity.BackendMissing("cryptography is not installed")


def test_a_backend_that_accepts_everything_fails_the_self_check(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(identity, "_backend", _Trusting(identity.CryptographyBackend()))
    with pytest.raises(identity.IdentityError) as refused:
        identity.self_check()
    assert "changed manifest" in str(refused.value)


def test_a_missing_backend_is_named(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(identity, "_backend", _Missing())
    with pytest.raises(identity.BackendMissing):
        identity.self_check()


def test_self_check_signing_writes_no_config(tmp_path: Path) -> None:
    env = {
        **os.environ,
        "QT_QPA_PLATFORM": "offscreen",
        "XDG_DATA_HOME": str(tmp_path / "data"),
        "APPDATA": str(tmp_path / "appdata"),
        "HOME": str(tmp_path / "home"),
        "USERPROFILE": str(tmp_path / "home"),
    }
    done = subprocess.run(
        [sys.executable, "main.py", "--self-check-signing"],
        cwd=ROOT,
        env=env,
        capture_output=True,
        text=True,
        timeout=120,
    )
    assert done.returncode == 0, done.stderr[-1500:]
    assert done.stdout.startswith("signing self-check: ok"), done.stdout[-1500:]
    assert list(tmp_path.rglob("*")) == [], "the self-check left something behind"
