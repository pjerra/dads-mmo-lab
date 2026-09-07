Thanks for the SOAP interface — I saw it land today and it is exactly the shape
that makes a launcher able to talk to a server without owning its console. I am
building **Yu'lon**, an open-source launcher for self-hosted WoW servers (I'm
from **Dads MMO Lab**), and I have it talking to a local Tortoise server.

While testing account tools against it I found a bug in `.account set password`
that I think is worth your time, and the new SOAP interface makes it reachable
remotely as well as from the console. **It locks a player out of their own
account, and the command reports success while doing it.**

**What happens**

`.account set password <name> <pass> <pass>` stores `sha_pass_hash` computed
with an **empty account name** — `SHA1(":" + UPPER(PASS))` instead of
`SHA1(UPPER(NAME) + ":" + UPPER(PASS))` — but only for an account that has
logged in at least once. That account can then never log in again with any
password, and the console still prints "The password was changed".

**Controlled repro** (local 1.18.1 server, playerbots branch, built 2026-09-03)

Four accounts created together, all four given the same password change twice
from the same script. Between the two rounds, only two of them logged in.

```
round 1 — all four, password "r0und-one11"
  CTRLONE   last_login 0000-00-00 00:00:00   hash correct
  CTRLTWO   last_login 0000-00-00 00:00:00   hash correct
  TREATONE  last_login 0000-00-00 00:00:00   hash correct
  TREATTWO  last_login 0000-00-00 00:00:00   hash correct

  (TREATONE and TREATTWO log in with a real 1.18.1 client, then log out)

round 2 — all four again, password "r0und-two22"
  CTRLONE   last_login 0000-00-00 00:00:00   hash correct
  CTRLTWO   last_login 0000-00-00 00:00:00   hash correct
  TREATONE  last_login 2026-09-07 15:32:25   EMPTY-NAME hash
  TREATTWO  last_login 2026-09-07 15:33:20   EMPTY-NAME hash
```

"EMPTY-NAME hash" means the stored value equals `SHA1(":" + UPPER(PASS))` and
not `SHA1(UPPER(NAME) + ":" + UPPER(PASS))`. An earlier pair of readings:

```
stored            5B70F10C79BABC4057A52E2A265D83AA1BF188E6
SHA1(NAME:PASS)   D1461243CDC1ECB656ACFDA0C132DFE0AD982F86
SHA1(:PASS)       5B70F10C79BABC4057A52E2A265D83AA1BF188E6   <- match
```

Corroborated three ways: a real 1.18.1 client refuses every password for the
affected account; realmd's own log shows it re-deriving `v`/`s` from the bad
hash and then counting a failed login; and an account that had *not* logged in
went straight to the realm list with its changed password.

**Where I'd look**

The handler is right for a fresh account and wrong for one with a login history,
so my guess is that the name it hashes with comes from a lookup that returns an
empty string in that state — a cache entry or a `GetName`-style call — rather
than from the name that was typed. Hashing with the name the command was given
would fix it wherever it comes from.

Happy to re-run anything on my box, send full logs, or test a patch — I have
this server and a client set up for automated testing, so a check takes minutes.
