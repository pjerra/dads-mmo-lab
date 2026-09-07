# 8.2c — Command channel, WoW TBC — live gate

**Where and when.** `m910q`, 2026-09-07 09:13Z – 10:13Z, against the TBC install at `~/tbc-7.4c`
that 7.4c built on this box, code at `5b835150` in `~/gate82c`. Tortoise was stopped first and is
restored afterwards, as the box's line requires; every action was announced on that box's activity
terminal.

**This box is the first one whose code is not 8.2a's.** The seam is the same and four things under
it are not, each measured here rather than inherited.

## The four per-tree differences

**The enable route is a conf file, not the container environment.** AzerothCore maps every ini key
`X` to `AC_<UPPER_SNAKE>` generically, which is why 8.2a's whole enable is four environment keys.
The CMaNGOS reader has no such rule anywhere in it — the keys come from the file and nowhere else
(`src/shared/Config/Config.cpp:73`, `:91`) — so `Operations.enable_conf` names `etc/mangosd.conf`
and the three keys, and the press patches it through `families/conf.patch()`, the same writer the
install stage already uses on that same file for six other keys.

**No CMaNGOS compose publishes 7878, so the override does.** The override template said in capitals
not to put ports in it: compose concatenates ports lists across files, so a second binding of a port
the base already has fails to bind after the build. That reason does not reach a port the base does
not bind at all, so the rule was narrowed rather than broken — the override may carry the channel's
port and no other, and the tests now name the number allowed instead of forbidding the key. (Owner's
call, asked and answered.)

**The GM level is a column on the account row.** There is no `account_access` table on this tree,
and nothing in this gate looks for one — `SHOW TABLES LIKE 'account_access'` comes back empty and
that is asserted, so a future change that started writing one would be caught here.

**The ready marker is `Avg Diff:`**, this core's own, not AzerothCore's `ready...`.

## The clauses

| Definition of done (as 8.2a) | Result |
|---|---|
| With the world stopped, one press writes the configuration | `SOAP.Enabled = 1`, `SOAP.IP = 0.0.0.0`, `SOAP.Port = 7878`, `Ra.Enable = 0` in `etc/mangosd.conf`, with `mangosd.conf.before-channel` (71 716 bytes) kept beside it; the override gained `- "${DOCKER_SOAP_EXTERNAL_PORT:-127.0.0.1:7878}:7878"` |
| It refuses while the world is running | it did, in the app's own sentence |
| A second press changes nothing | `changed=False` |
| The ordinary Start brings the world up | `restarts=0`, and `Avg Diff: 66. Sessions online: 0.` in this run's own log |
| The tab reads verified with the time | `Command channel: verified as YULON_37F13213 at 2026-09-07 10:05 UTC.` |
| The account exists at this tree's own level 3 | `106  YULON_37F13213  3`, on the account row; no access table exists or is looked for |
| The reply text, **from the host** | `CMaNGOS/0.18 … Using World DB: TBC-DB 1.11.0 'Vengeance One: A Cmangos Story' … Online players: 0 (max: 0)` |
| Nothing listening on 8888 or 3443 inside the container | `[7878, 8085, 35773]` — both silent. **Recorded rather than changed:** the playerbot command server is off by compiled default on this tree, measured before the press (`8085` alone) rather than assumed |
| A wrong credential reads refused, and the repair returns it without a second account | refused → repaired, **1 account before and after**, round trip `answered` |
| An occupied 7878 rolls back and the server still starts | the tab's sentence, `SOAP.Enabled` back to `0`, `SOAP.IP` back to `127.0.0.1`, port released, world running |

The rollback is the clause that differs most: on WotLK it undoes an override and gives a port back;
here it must also put `SOAP.Enabled` back to `0`, because the world reads that file on the way up
and a rolled-back channel that still said `1` would bind a port it had just been told to release.

## Four defects, and every one of them was hidden on WotLK

**1. A rejection during setup was counted as silence, and that is a permanent dead end.** TBC's
world takes minutes to load its bots, so the three round trips after a Start can all miss it and the
setup gives up with nothing saved — which is the designed behaviour. The run after that is the trap:
with no credential on disk it starts from `Idle`, generates a NEW password and calls `create`, which
by design keeps the password of an account that already exists. Every round trip from then on is a
401, and a 401 counted as "did not answer yet" gives up again, on every run, for ever, with no way
out but hand-written SQL. `Answer.denied` already told the two apart — 8.2a added it for the
check-on-open path — and the setup path was not reading it. Fixed at `b774fcf3`; both halves are
pinned, because collapsing them the other way would offer a password reset for a server nobody
managed to ask.

**2. The SOAP namespace was a constant, and it is a per-tree fact.** `urn:AC` on AzerothCore,
`urn:MaNGOS` here. The source study stopped at exactly this question and marked it "needs a
judgement / live test", so it was measured: `soapprobe.py` sends three envelope shapes with right
and wrong credentials. The nasty part is in that table — **the namespace is checked before the
password**, so the wrong one answers HTTP 500 `method name or namespace not recognized` even for a
bad credential, which from the caller's side is indistinguishable from a world still loading. That
is exactly how it presented. `Operations.namespace` has no default, because a default here is one
tree's answer inherited by the rest and this particular wrong answer is invisible.

**3. `prove()` built its own endpoint and kept the field default.** The namespace fix went into
`live_channel()` and the endpoint seam, and `prove()` — the path the gate actually takes — still
sent `urn:AC`. The test asserted the site that had been fixed. Filed already as
`reviews-check-functions-not-call-sites`; the test now asks the object for **every** endpoint it
hands out rather than naming today's call sites.

**4. The credential file recorded the wrong namespace.** `live_channel()` overrides it from the
entry, so the app was safe; anything reading the file was not, and this gate read it, sent `urn:AC`
and got HTTP 500 from a channel that had verified seconds earlier. The file carries it now, read
back with `.get` so a credential written before the field is still a credential.

## The adversarial review, and what it changed

Four findings, all four real, all four fixed at `5cdb22e3`.

**The rollback restored the whole conf to undo four keys.** The backup is written by the first
press and lives until a rollback consumes it, so it can be arbitrarily old — the argument already
written down for the compose override, which is why `roll_back(expected=...)` exists there. The
conf had no equivalent. It is an inverse **patch** now, and `rollbackprobe.py` proves it against
this install's real 71 kB file rather than a fixture:

```
before       SOAP.Enabled 1   SOAP.IP 0.0.0.0    Console.Enable 1   71714 bytes, 1927 lines
edited       Console.Enable 0 and one new line                      71751 bytes
rolled back  SOAP.Enabled 0   SOAP.IP 127.0.0.1  Console.Enable 0   71753 bytes, 1928 lines
```

The channel's own keys went back and nothing else moved. Under the previous code the file would
have returned to 71 716 bytes and 1927 lines, taking both edits with it.

**The port could be published twice.** `publish` says what the entry wants, not what the install
has, and two things make it stale on a real box: somebody editing the base compose, and a later
template revision adding the binding while the flag stays. The answer now comes from the base file
that will actually be loaded.

**Both backups outlive a rollback that could not finish.** The conf's copy was deleted inside the
restore, so a failure on the override or the `.env` left nothing to retry from — the ordering
argument the override's own comment already makes.

**A proven namespace beats the catalog.** The file records what a real round trip answered through;
the catalog is a claim about the tree. The file wins where it has an answer, the catalog bootstraps
one written before 8.2c, and a disagreement is logged rather than resolved in silence. That took
`save_credential`'s default with it: a default there was the same inheritance in miniature.

## The files

* `gate82c.py` — the gate. Stages: `ports before press prove recover rows marker break repair port after`.
* `soapprobe.py` — three envelope shapes against the live listener, right and wrong credentials.
  This is the probe the source study asked for.
* `rollbackprobe.py` — the inverse-patch rollback, against this install's real conf.
* `answerprobe.py` — what `soap.execute()` and `channel.SoapChannel` make of those same answers.
* `transcript.txt` — the presses quoted from the runs that made them, then a fresh read-only pass.
* `1-` before, `2-verified` the recovery, `3-refused`, `4-repaired`, `5-rolled-back`, `6-after`.

## How to re-run it

```
python gate82c.py  ports | before | press | prove | recover | rows | marker | break | repair | port | after
```

Tortoise holds the same ports on this box, so it must be stopped first — and put back afterwards,
which is what the box's own line asks for.
