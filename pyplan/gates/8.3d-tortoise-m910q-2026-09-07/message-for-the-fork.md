Hi — I'm building **Yu'lon**, an open-source launcher for self-hosted WoW servers (I'm from **Dads MMO Lab**), and while testing the account tools against a local Tortoise server I found a reproducible bug in `.account set password` that I think is worth your time. It locks a player out of their own account, and the command reports success while doing it.

**What happens**

`.account set password <name> <pass> <pass>` stores `sha_pass_hash` computed with an **empty account name** — `SHA1(":" + UPPER(PASS))` instead of `SHA1(UPPER(NAME) + ":" + UPPER(PASS))` — but only for an account that has logged in at least once. The account can then never log in again with any password, and the console still prints "The password was changed".

**Minimal repro** (local 1.18.1 server, playerbots branch, built 2026-09-03)

1. Create two accounts. Log in with one of them, log out. Leave the other untouched.
2. From the console: `account set password <each account> hunter77 hunter77`
3. Compare `sha_pass_hash` with `SHA1(UPPER(name):UPPER(pass))`.

Measured here, same command, same password, one minute apart:

```
SHAPROBE   never logged in
  stored              C20C345AE839AF76C206C48BFAF73EB2C812145E
  SHA1(NAME:PASS)     C20C345AE839AF76C206C48BFAF73EB2C812145E   <- match

GATE83E    last_login 2026-09-07 15:08:23
  stored              5B70F10C79BABC4057A52E2A265D83AA1BF188E6
  SHA1(NAME:PASS)     D1461243CDC1ECB656ACFDA0C132DFE0AD982F86
  SHA1(:PASS)         5B70F10C79BABC4057A52E2A265D83AA1BF188E6   <- match
```

A real 1.18.1 client then refuses every password for that account, and realmd's own log shows it re-deriving `v`/`s` from the bad hash and counting a failed login. The account that had never logged in went straight to the realm list with its new password.

**Where I'd look**

The handler looks correct for a fresh account and wrong for one with a session/login history, so my guess is the account name it hashes with comes from a lookup that returns an empty string in that state — a cache entry or a `GetName`-style call — rather than from the name that was typed. Hashing with the name the command was given would fix it wherever it comes from.

Happy to re-run anything on my box, send the full logs, or test a patch — I have this server and client set up for automated testing, so a check is quick. Thanks for everything you've built; the fork is a joy to work against.
