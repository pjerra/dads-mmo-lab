"""The text a command becomes, and the arguments it refuses (Phase 8.2a).

Pure: this module sends nothing, holds no lock and knows no container. It turns
a template and some arguments into one line, and refuses arguments the server
would refuse — so nothing reaches the wire that cannot succeed.

## The leading dot lives here, once

The console and SOAP parser strips a leading `.` or `!` before it looks anything
up (`azerothcore.md:161-176`, `if (str[0] == '.' || str[0] == '!')`), while the
in-game handler *requires* one (`Chat.cpp:255-256`). So on this route the dot is
a matter of taste — and the prior art shipped both tastes: its `gm_level_cmd`
emits `.character level {player} {level}` with a dot and its `gm_at_login_cmd`
emits `character {flag} {player}` without, both pinned by their own tests, with
no comment explaining the asymmetry (`hypeer.md:1369-1374`).

Yu'lon's templates therefore carry no dot, and this module is the one place that
decides. A template that grows one anyway still produces the same line, because
`line()` strips it.

## The validators are the server's, not this app's

The charsets and lengths come from the bash oracle by way of the Rust port
(`soap_cmds.rs:24-40`, from `cli/src/90-main.sh:116-138`). They are allow-lists,
which is what stops an argument smuggling a second command into a line: a space
or a semicolon is simply not in the set.
"""

from __future__ import annotations

import re

_ACCOUNT_NAME = re.compile(r"^[A-Za-z0-9_]{3,20}$")
_ACCOUNT_PASSWORD = re.compile(r"^[A-Za-z0-9_@#%+=!-]{4,16}$")
_CHARACTER_NAME = re.compile(r"^[A-Za-z0-9_]{1,12}$")


class CommandError(ValueError):
    """An argument the server would refuse, refused here instead."""


def line(text: str) -> str:
    """One command, normalised: no leading `.`/`!`, no surrounding space.

    Inner spacing is left exactly as given — `server set motd` takes a message,
    and collapsing its spaces would change what players read.
    """
    stripped = text.strip()
    if stripped[:1] in (".", "!"):
        stripped = stripped[1:].strip()
    if not stripped:
        raise CommandError("an empty command was asked for; nothing was sent")
    if "\n" in stripped or "\r" in stripped:
        raise CommandError("a command is one line, and this one carries a line break")
    return stripped


def valid_account_name(name: str) -> bool:
    """`^[A-Za-z0-9_]{3,20}$` — what account creation accepts."""
    return bool(_ACCOUNT_NAME.match(name))


def valid_account_password(password: str) -> bool:
    """`^[A-Za-z0-9_@#%+=!-]{4,16}$` — and the 16 is a ceiling, not a preference."""
    return bool(_ACCOUNT_PASSWORD.match(password))


def valid_character_name(name: str) -> bool:
    """`^[A-Za-z0-9_]{1,12}$` — a character name as this core stores it."""
    return bool(_CHARACTER_NAME.match(name))


def account_create(account: str, password: str) -> str:
    """`account create <user> <pass>`, with both parts checked first."""
    _require(valid_account_name(account), f"{account!r} is not a name this server would accept")
    _require(valid_account_password(password), "that password is not one this server would accept")
    return line(f"account create {account} {password}")


def account_set_gm_level(account: str, level: int) -> str:
    """`account set gmlevel <user> <n> -1`.

    The `-1` is "every realm". SOAP reads the level from `account_access` with
    no realm filter, so a row pinned to one realm id would not be seen on this
    path; the prior art passes `-1` for the same reason (`soap_cmds.rs:95`).
    """
    _require(valid_account_name(account), f"{account!r} is not a name this server would accept")
    _require(0 <= level <= 3, f"{level} is not a GM level this server has")
    return line(f"account set gmlevel {account} {level} -1")


def account_set_password(account: str, password: str) -> str:
    """`account set password <user> <pass> <pass>` — the server wants it twice."""
    _require(valid_account_name(account), f"{account!r} is not a name this server would accept")
    _require(valid_account_password(password), "that password is not one this server would accept")
    return line(f"account set password {account} {password} {password}")


SERVER_INFO = "server info"
"""The round trip that proves a channel without changing anything.

It prints the core revision, `Players online: N. Max online: M.` and uptime
(`cmangos.md` records the same shape on that family), so a reply is both proof
the channel works and something a person can read in a capture.
"""


def _require(ok: bool, message: str) -> None:
    if not ok:
        raise CommandError(message)
