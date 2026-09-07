# 8.3d — Accounts, WoW Tortoise — gated live on m910q, 2026-09-07

The same four clauses as 8.3b against the Shyalya/tortoise fork, and one clause
that only exists here, because this box found a defect in the fork itself that
locks people out of their own accounts while reporting success.

Server: `~/tortoise-server` on `m910q` (Tortoise 1.18.1, `tw_logon`/`tw_char`).
Client: the real 1.18.1 client (build 7272, "Mysteries of Azeroth") on the
Hyper-V host. Code: `52b1ce59`.

## The four clauses

| clause | reading |
| --- | --- |
| 1 the list equals the same question by hand | 4 shown of 104; identical rows |
| 2 a password change the client can feel | the client is refused the old password and reaches the realm list with the new — **on an account that has not logged in before**; see below |
| 3 a GM level change in the row and the server's words | `You change security level of account GATE83D to 4.`, `rank` reads 4, the tab reads 4 |
| 4 the app's own account | absent from the list, refused by both actions, and **never created here at all** |

Clause 4 reads differently on this tree and that is the right answer rather than
a missing one: the console IS the channel here (8.2e), so this app never makes
itself a GM account and there is no credential on disk. The refusals still
stand — the name is derived from the install id — and the sentence no longer
claims this app made an account it did not.

## Measured on this fork, all of it by asking

* the level is `account.rank`, and **not** `account.security`: `security`
  stayed NULL through every change while `rank` moved, and both columns exist
* `account set gmlevel <account> <level>` — no realm argument
* the ceiling is **4**, not 3: `... SHAPROBE 4` answered *"You change security
  level of account SHAPROBE to 4."* and `5` answered *"Incorrect values."*, so
  the fork's check grants at the caller's own level rather than strictly below
  it. This app drew 0-to-3 for every game and refused 4 **in its own voice**
* the credential is one unsalted `sha_pass_hash` = `SHA1(UPPER(user):UPPER(pass))`,
  measured twice: what this app writes, and what the SERVER writes when it
  changes a password itself. `v` and `s` exist on this row too and are not it.

## The defect this box found

`account set password` answers **"The password was changed"** and then stores,
for an account that has logged in before, a hash **with an empty account name in
it**. The account can never log in again, and the server has just reported
success.

**Isolated properly, after the adversarial review refused the first attempt.**
That version compared two DIFFERENT accounts and attributed the difference to
`last_login`, which confounds identity, row state and command order with it. The
controlled version (`controlled-experiment.py`) creates four accounts together
and gives all four the same password change twice, from the same script; between
the rounds, only two of them log in.

    round 1   all four        every hash correct
    round 2   CTRLONE, CTRLTWO      still correct
              TREATONE, TREATTWO    EMPTY-NAME hash

Prior login is the only thing that differs, and it is now a within-account
before-and-after with two controls that took the same number of changes.

The first pair of readings, which the `lockout` stage still asserts:

    SHAPROBE  last_login 0000-00-00 00:00:00
       stored           C20C345AE839AF76C206C48BFAF73EB2C812145E
       SHA1(NAME:PASS)  C20C345AE839AF76C206C48BFAF73EB2C812145E   MATCH
       SHA1(:PASS)      5B70F10C79BABC4057A52E2A265D83AA1BF188E6

    GATE83E   last_login 2026-09-07 15:08:23
       stored           5B70F10C79BABC4057A52E2A265D83AA1BF188E6
       SHA1(NAME:PASS)  D1461243CDC1ECB656ACFDA0C132DFE0AD982F86
       SHA1(:PASS)      5B70F10C79BABC4057A52E2A265D83AA1BF188E6   MATCH

Corroborated from three sides: a real client refused every password for
`GATE83E`; realmd's own log shows it re-deriving `v`/`s` from that hash and then
counting a failed login; and the account that had **not** logged in went
straight to the realm list with its changed password (`6-client-new-realms.png`,
`7-fresh-account-in.png`).

**What this app does about it.** Nothing can repair the row from here — that is
the server's own write. What changed is that a `yes` this app can check is no
longer believed on the reply alone: the account's own row is read after every
password change on a tree whose scheme has been measured, and where the row does
not hold the password that was asked for, the person is told so:

> The server said GATE83E's password was changed, but the account's stored
> credential is not that password, so it will not log in with it. Nothing here
> can put it right: the change has to be made on the server itself, and the old
> password may or may not still work. Try logging in before relying on either.

The `lockout` stage asserts **both** halves — the fresh account confirmed and
the used one refused — because a check that answered no to everything would look
identical on the second.

## Three things the client needed, and one that is a rule

The 1.18.1 client took three attempts to drive, and each failure is in the log
rather than in somebody's memory:

1. `TurtleWoW.exe` in that folder is the **installer** and `turtle-wow.exe` is
   the **launcher**; the game is `WoW.exe`. The first two typed a login into a
   setup wizard and into an update dialog.
2. Neither `{TAB}` nor `{ENTER}` moves between the account and password boxes
   here. realmd logged `WHERE username = 'OLD-P@SS12'` on one attempt and a name
   left over from an earlier session on the next — the keystrokes never reached
   the account box. The driver now clicks both boxes.
3. The driver records whether the client is still alive when the answer is
   photographed, because on the 1.12.1 client a dead client had already been
   photographed once as though it were an answer.

And the rule, which is this box's own: **the client half has to be run before
the account has ever logged in.** On this fork that is not a convenience, it is
the difference between a password change that works and one that cannot.
