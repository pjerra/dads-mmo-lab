# 8.2d — Command channel, WoW Vanilla — live gate

**Where and when.** `m910q`, 2026-09-07 10:55Z – 11:21Z, against the Vanilla install at
`~/vanilla-75b`, code at `b35492a5` in `~/gate82d`. Tortoise was stopped for it and is put back
after; every action was announced on that box's activity terminal.

**This box is the one that tests the seam rather than the tree.** 8.2c built a channel for a
family that reads no environment; this is the second member of that family, and the question is
whether it needed anything new. It did not: an operations block, the tab wiring, and one refactor a
second tree made obvious. `gate82d.py` is 8.2c's gate with four constants changed.

## The clauses

| Definition of done (as 8.2c) | Result |
|---|---|
| With the world stopped, one press writes the configuration | `SOAP.Enabled = 1`, `SOAP.IP = 0.0.0.0`, `SOAP.Port = 7878`, `Ra.Enable = 0`, with `mangosd.conf.before-channel` (66 440 bytes) beside it |
| A second press changes nothing | `changed=False` |
| The ordinary Start brings the world up | `restarts=0`, `Avg Diff: 51. Sessions online: 0.` in this run's own log |
| The tab reads verified with the time | `Command channel: verified as YULON_06CED116 at 2026-09-07 11:01 UTC` — on the **third** attempt, inside the three the setup allows |
| The account at this tree's own level 3 | `106  YULON_06CED116  3` on the account row; `SHOW TABLES LIKE 'account_access'` empty, and asserted |
| The reply text, **from the host** | `CMaNGOS/0.18 … Using World DB: Classic DB version 1.12.1 "Melting Pot v2". For Classic core z2815 … Online players: 0 (max: 0) Queued players: 0 (max: 0)` |
| Nothing on 8888 or 3443 inside the container | `[7878, 8085, 43059]` — both silent; **8085 alone before the press**, measured on this tree |
| A wrong credential reads refused, and the repair returns it without a second account | refused → repaired, one account throughout |
| An occupied 7878 rolls back and the server still starts | the tab's sentence, `SOAP.Enabled` back to `0`, `SOAP.IP` back to `127.0.0.1`, world running |

## Two facts this tree does not share with its sibling

**Its reply text has a field TBC's does not.** `Online players: 0 (max: 0) Queued players: 0 (max:
0)` — same family, same command, different sentence. Which is why the box asks for the reply to be
recorded rather than matched against a sibling's.

**Its world answers inside three attempts.** TBC's does not, and that is how 8.2c found the dead
end a fourth attempt used to fall into: the setup gives up, the next run generates a password the
existing account will never have, and every round trip after that is a 401 counted as silence.
Vanilla's world is quick enough that the same code never reaches it. The same difference, seen from
the other side — and the reason the fix is not "make the timeout longer".

**The namespace was measured here, not carried over.** `soapprobe.py` sent the same three envelope
shapes to this listener with right and wrong credentials, and the table is identical to TBC's:
`urn:AC` gets HTTP 500 whichever password it carries, `urn:MaNGOS` gets 200 or 401. Identical, and
read.

## The one piece of real code

`reset_own_password` took a `scheme` parameter and never read it. Harmless while AzerothCore was
the only tree with a channel, since its hard-coded `salt`/`verifier` were right — which is why 8.2c
gave TBC a copy of the whole function. Vanilla needing the identical thing settled it: read the
argument. **A parameter that is accepted and ignored is worse than no parameter, because it tells
the caller it has a choice it does not have.** Both trees bind the shared function now, and the
write ledger noticed from the other side: TBC's reset no longer *contains* a write.

## One thing this run did to itself

Assembling the transcript, a first attempt re-ran every stage to get a tidy file in one pass —
including `press`, which stops the world and does not start it again, and `break`, which writes a
deliberately wrong credential. It was stopped part way and left the world down with a broken
credential behind it; both were put back and the pass in `transcript.txt` is the read-only stages
only.

Recorded because it is the same mistake in miniature that these gates keep finding in the code: a
step that changes something, run as though it only reads.

## The files

* `gate82d.py` — 8.2c's gate, four constants changed.
* `soapprobe.py` — the namespace table, measured against this listener.
* `transcript.txt` — the presses quoted from the runs that made them, then a read-only pass.
* `1-before`, `2-after` the first verification, `3-refused`, `4-repaired`, `5-rolled-back`,
  `6-after`.

## How to re-run it

```
python gate82d.py  ports | before | press | prove | recover | rows | marker | break | repair | port | after
```

`press` stops the world and leaves it stopped — the user's own Start is the next step, which is the
shape of the feature. Tortoise holds the same ports on this box and goes back afterwards.
