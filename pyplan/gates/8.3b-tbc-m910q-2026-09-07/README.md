# 8.3b — Accounts, WoW TBC — gated live on m910q, 2026-09-07

The same four clauses as 8.3a, against a different lineage, and the tree refuted
three things this box would otherwise have inherited from its sibling.

Server: the 7.4c TBC install on `m910q` (`~/tbc-7.4c`, CMaNGOS 0.18, built
2026-09-03). Client: the real 2.4.3 client on the Hyper-V host, pointed at
`100.78.24.50` and driven in the interactive session through `schtasks /it`.
Code: `f7367a5a` for the server half, `896c0dc9` for the channel half.

## The four clauses

| clause | reading |
| --- | --- |
| 1 the list equals the same question by hand | 6 shown of 107 in the table; the by-hand rows are identical, ordered by name |
| 2 a password change the client can feel | the row's `v` and `s` both moved; the old password is REFUSED by a real client and the new one reaches the realm list |
| 3 a GM level change in the row and in the server's words | `You change security level of account GATE83B to 2.`, the row reads 2, the tab reads 2 |
| 4 the app's own account | absent from the list, refused by both actions, still level 3 |

Screenshots: `1-list.png`, `2-chosen.png`, `3-level-set.png`, `4-refused.png`
(the tab), `5-client-old-refused.png` and `6-client-new-realms.png` (the client).
`transcript.txt` holds every stage, the wire capture and both client logs.

## What this tree refuted

**1. `account set password` reports a failure when it works.** Not a fault of
this app and not occasional. The handler prints its success line and then

    // OK, but avoid normal report for hide passwords, but log use command
    LogCommand(msg);
    SetSentErrorMessage(true);
    return false;

(`Level3.cpp:1178-1183`), and SOAP turns a handler that returned false into a
fault. `pwtruth.py` measured it on the live server:

    before : s=EC5E34BDB045F950… v=1243716ED25AFF31…
    the command reported: 'unreachable'
    after  : s=F810FF9782BB8F03… v=56A312814A238144…
    the row CHANGED: True

A person told their password change failed retypes the old one, for an account
that no longer has it. So **the reply is a hint and the row is the answer**.

**What "the row says" means changed after this box was first written, and this
paragraph is the correction rather than a tidy-up.** The first version compared
the credential columns before and after and took any difference as proof. This
box's own adversarial review refused that, and was right: anything else that
writes that row — a second window, an administrator at a console, another
install sharing this auth database — would have turned a refused command into a
reported success, and the person would be locked out BY the reassurance.

The shipped check recomputes the verifier instead. SRP6 stores
`g^H(s, H(USER:PASS)) mod N`, so the row's own salt and the password that was
asked for say whether THAT password is the one the account now has — whoever
wrote the row, and whatever the server said. There is no before-read left, so
there is no window to race, and a false positive is unreachable rather than
merely unlikely. The recipe was measured on the live Vanilla server the same
afternoon (`8.3c-vanilla-m910q-2026-09-07/`), and it reproduces this tree's rows
too; `transcript.txt` ends with clause 2 re-run against the shipped code, where
the log line reads *"GATE83B's stored verifier is the one this password makes"*.

**2. A refused command arrives as nothing at all.** Asked with a raw socket, so
nothing in this app could be blamed for the reading:

    'server info'                 -> 951 bytes, HTTP 200
    'account set'                 -> 0 bytes
    'blargh'                      -> 0 bytes
    'account characters GATE83B'  -> 0 bytes

The core hands a failed command to `soap_sender_fault` (`MaNGOSsoap.cpp:133`)
and the client receives no HTTP at all. This app was answering *"the server is
running but nothing is listening on the command channel — it may not be turned
on for this install yet"* about a channel that had answered a moment earlier.
The connect separates them: a channel nobody is on refuses the connection, and
this one accepts it, reads the whole request and hangs up. That is now the
`silent` outcome, and it is `indeterminate` rather than a `no` — clause 1 above
is exactly a command that hung up and had run.

**3. `account info` does not exist here.** `help account` lists characters,
create, delete, onlinelist, lock, set and password. The 8.3a script asked for
`account info` because AzerothCore has one; this gate asks the questions this
tree can answer, and its docstring no longer claims a confirmation that was
taken on the other tree.

Two further per-tree facts, measured before the code was written:
`account set gmlevel` takes **no realm argument** (`Level3.cpp:1080`), because
the level is a column on the account row and there is no table with realms in
it; and `account_access` is asserted **absent**, so nothing writes one.

## The client half

`client-login-tbc.ps1`, twice, against `100.78.24.50`:

* `old-p@ss12` → *"The information you have entered is not valid."*
  (`5-client-old-refused.png`)
* `n3w-p@ss34` → the realm-choosing screen (`6-client-new-realms.png`)

And the server's own record of it, which is what makes this a reading rather
than a photograph: the account row's `sessionkey` was `NULL` before the
successful login and held a session key after it, with `locale` `enGB`, `os`
`Win` and `platform` `x86` — written by realmd for a client it authenticated.

Two things the script had to learn on this client. It publishes its realmlist in
three places (`realmlist.wtf` beside `Wow.exe`, `Data\enGB\realmlist.wtf`,
`WTF\Config.wtf`) and all three are written, because 8.1d's first capture was a
refusal by a completely different server. And the window: one run published no
`MainWindowHandle` at all and the next published one while `GxWindowClass` found
nothing, so both routes are tried and the log records which answered — the first
attempt typed the whole login into the task's own console window and
photographed an empty password box.
