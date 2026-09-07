# A redaction in this directory, 2026-09-07

`79-tbc-restart-evidence.log` and `79-vanilla-restart-loop.log` carried the two installs'
**generated MySQL root passwords**, in the worldserver's own connection-failure line:

    Cannot connect to world database tbc-db;3306;mangos;<password>;mangos

They were captured on 2026-09-04 by the 7.9 gate, merged upstream with #143, and reported by
GitGuardian on 2026-09-06. The values are replaced above with `<game>-REDACTED-SEE-BELOW`; nothing
else in those files changed, and the lines still show what they were evidence for — a worldserver
failing to reach its database.

**What they were.** Per-install passwords minted by `resolve_secrets()` as
`<game>-<token_hex(8)>` and kept in `<server_dir>/.env`. They are the MySQL **root** password for
one install's database container.

**What the exposure is.** Both installs publish 3306 as `127.0.0.1:3306:3306` — loopback only,
deliberately, and the compose files say why. So the password is usable only by someone who already
has a shell on that machine, and such a person can read `.env` directly. That bounds it; it does not
excuse it.

**What is still true.** The values remain in git history, here and upstream, because the commits
that carried them are merged. Redacting the working tree does not remove them from the objects.
Rotating the two installs' passwords is the only fix that makes the published values worthless, and
it is a one-command change per install (`ALTER USER`, rewrite `.env`, recreate the db container).

**What stops the next one.** `tests/test_no_secrets_in_evidence.py` walks every committed page and
capture for the `<game>-<16 hex>` shape and fails on one it does not recognise as an illustration.
A gate transcript is written by a machine and read by nobody in a hurry, which is exactly where a
secret hides.
