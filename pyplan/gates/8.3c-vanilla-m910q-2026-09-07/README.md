# 8.3c — Accounts, WoW Vanilla — gated live on m910q, 2026-09-07

8.3b's own script with five constants changed — its own press, not its own code
— against the Vanilla install on `m910q` (`~/vanilla-75b`, CMaNGOS 0.18,
Classic DB 1.12.1 "Melting Pot v2"). The client is the real 1.12.1 client on the
Hyper-V host, pointed at `100.78.24.50` and driven in the interactive session.

## The four clauses

| clause | reading |
| --- | --- |
| 1 the list equals the same question by hand | 7 shown of 108; identical rows, ordered by name |
| 2 a password change the client can feel | the row's verifier became the one this password makes; the client is refused the old password and reaches the realm list with the new |
| 3 a GM level change in the row and in the server's words | `You change security level of account GATE83C to 2.`, the row reads 2, the tab reads 2, `account_access` asserted absent |
| 4 the app's own account | absent from the list, refused by both actions, still level 3 |

Screenshots `1-list.png` … `4-refused.png` are the tab; `5-client-old-refused.png`
and `6-client-new-realms.png` are the client. `transcript.txt` holds every stage,
the SRP6 measurement and both client logs.

## Measured on this tree, not inherited from TBC

The answers agreed with TBC's, which is exactly why they had to be asked here.

* `SHOW TABLES LIKE 'account_access'` — **empty**. The level is a column on the
  account row, so `accounts.level.table` is null and there is nothing to join.
* the server's own help: `Syntax: .account set gmlevel [#accountId|$accountName]
  #level`, `#level may range from 0 to 3` — **no realm argument**, because there
  is no table with realms in it to name.
* `account set password` reports a **failure when it works** here too
  (`HandleAccountSetPasswordCommand`, the same `SetSentErrorMessage(true);
  return false;` after the success message).

And a defect that was found by asking: `accounts.level` was absent from the
catalog for this game, and the services for it were assembled with **no accounts
object at all** — so the Accounts tab would have drawn its "this game cannot do
that yet" sentence for a game that can. Nothing raises when a fact and its
wiring disagree; the feature is simply missing. The test that now walks every
game requires the two to agree in both directions.

## The SRP6 recipe, and why it is in this folder

8.3b's adversarial review refused the row-changed inference: any other writer
changes that row too, and a refused command would then be reported as a success
while the account holds somebody else's password. So the check became
cryptographic — and the recipe was **measured here**, on this server, with
`srpprobe.py`: an account created through the app with a known password, its `s`
and `v` read back, and every candidate byte order computed until one reproduced
the stored verifier.

    s = E040A443299D8590D08C3353B3F9960548A9BB82B873659FC7533CA2031B485A
    v = 27BADE411219414667B39335D71F258E191312B78C065F630954135E8870111B
    MATCH: s=little h1=as-is x=little v=big

Exactly one combination matched; the wrong password matched none. That pair is
the test vector in `tests/test_srp6.py`, and the gate's own log line for the
password change now reads *"GATE83C's stored verifier is the one this password
makes"*.

## The client half

`client-login.ps1`, twice, against `100.78.24.50`:

* `old-p@ss12` → *"The information you have entered is not valid."*
* `n3w-p@ss34` → the realm-choosing screen

with the server's own record beside the photographs: `sessionkey` was NULL
before the successful login and held a key after it, with `locale` `enGB`, `os`
`Win`, `platform` `x86` — written by realmd only for a client it authenticated.

**Three things this client needed that the 2.4.3 one did not**, all now in the
driver and all found by looking at what came back:

1. It starts **fullscreen** and plays its intro, so the answer photograph at 22
   seconds was the cinematic and every keystroke went into a client with no
   login box yet. The configuration now asks for a 1024x768 window and turns the
   intro off.
2. **ESC does not skip it — it quits.** Three ESC presses left no client running
   at all and photographed a bare desktop. The log now records whether the
   client is still alive when the answer is taken, so that failure can never
   again look like an answer.
3. The window has to be raised **again immediately before typing**: something
   else on this host took the foreground in between, and the earlier TBC run
   typed a whole login into the task's own console window.
