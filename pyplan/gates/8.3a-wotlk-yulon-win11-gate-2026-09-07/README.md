# 8.3a — Accounts, WoW WotLK — the WINDOWS half

**Where and when.** `yulon-win11-gate`, 2026-09-07 08:07Z – 08:26Z, against `D:\gate\wotlk-server77`
through the command channel 8.2b verified on the same box an hour earlier. The client half ran on
the Hyper-V host (`DESKTOP-FP27AUV`) with its own 3.3.5a client at
`C:\clients\WoW-WotLK-3.3.5a-min`, against this VM at `172.30.56.117`. Code at `3af8edb4` in
`C:\gate\src82b`.

The box's line says the Windows half is **its own press, not its own code**, and `gate83a_win.py`
is 8.3a's own gate script with four constants changed — the path, the source root, the shots
directory and the target account name. Nothing else differs.

## The clauses

| Definition of done | Result |
|---|---|
| The list matches the account table read by hand, minus the bots and the app's own account | **1 shown out of 102** — `[(102, 'GATE83W', 2)]` from the tab, the identical row from the hand query. Before the target existed it was **0 out of 101**, which is the same agreement with nothing in it |
| After a password change the game client logs in with the new one… | **it did** — the real 3.3.5a client reached the character screen on realm `AzerothCore`, `6-client-new-password-in.png`, corroborated by `last_login 2026-09-07 08:24:06` and `last_ip 172.24.0.1` on a row that had been `NULL` |
| …and is refused the old | **it was** — "The information you have entered is not valid", `5-client-old-password-refused.png` |
| After a level change the row reads the new value **and the server's own account query reports it** | `account_access` `102 2 -1`; `.account info GATE83W` → `GMLevel: 2`; the tab → `Account(id=102, username='GATE83W', gm_level=2)` |
| The app's own account cannot be selected for either action | absent from the list; `set_password` and `set_gm_level` both refused; and the server still reports `YULON_F7357748 … GMLevel: 3`, which is what those refusals protect |

The password change is real at the cryptographic level, not just at the row level: the stored
verifier was recomputed by hand from `salt` and `n3w-p@ss34` and matched byte for byte
(`SHA1(salt ‖ SHA1("GATE83W:N3W-P@SS34"))`, x little-endian, `v = g^x mod N` stored little-endian).
That check was written to answer a failure and stayed because it is the strongest form of the
clause.

## Three things this box refuted, and none of them were the app

The client half failed twice before it passed, and every cause was in the way a Windows host is
wired rather than in the feature.

**1. Docker Desktop can ship BLOCK rules for its own backend.** The published ports were on
`0.0.0.0` and unreachable from the host. Two enabled inbound rules named `Docker Desktop Backend`
with `Action = Block` on Private and Public — what Windows writes when the first network prompt is
dismissed, and a block beats any allow. Until they were disabled the server was reachable only from
the box itself. The app's Networking plan tells a Windows user to set the profile to Private and
says nothing about this; that is worth a sentence in the plan and is noted rather than changed here.

**2. This client reads `Data\<locale>\realmlist.wtf` FIRST, not `WTF\Config.wtf`.** The driver
wrote Config.wtf with the right address, the client connected somewhere else entirely, and the
proof it never arrived is on the server: `failed_logins 0`, `last_login NULL`, nothing in the
authserver's log. `Data\enUS\realmlist.wtf` still said `172.30.55.119` from an earlier gate. The
first "refusal" screenshot of the run was therefore a refusal by a **different server** and proved
nothing — it was thrown away and the attempt re-run. `client-login.ps1` now writes every
`Data\<locale>\realmlist.wtf` as well, and says so in the log.

**3. `$_` in a PowerShell `-replace` means the entire input string.** Patching the driver with
`-replace` spliced the whole script into the middle of itself and the next run died after one line.
Fixed by using `String.Replace`, which is literal. Recorded because the corruption was silent: the
file still parsed.

## The realm address

Set to `172.30.56.117` **through the app's own Networking plan/apply**, not by writing the row —
the row had held `172.30.52.119` since the 7.7 gate, because this VM's DHCP lease changed across a
shutdown. `gate83a_win_realm.py` is that call and nothing else.

## The files

* `gate83a_win.py` — 8.3a's own script, four constants changed.
* `gate83a_win_realm.py` — the realm address, through `networking.plan`/`apply`.
* `client-login.ps1` — the client driver as it now stands, with the `Data\<locale>` fix.
* `transcript.txt` — every stage's output, and both client runs' logs.
* `1-` … `4-` the Accounts tab on Windows; `5-` and `6-` the real client, refused and in.

## How to re-run it

```
python gate83a_win.py  list | password | level | own | shots
python gate83a_win_realm.py  show | <lan_ip>
powershell -File C:\Users\PK\drive-login.ps1 -ClientDir C:\clients\WoW-WotLK-3.3.5a-min `
    -Realmlist <vm ip> -Account GATE83W -Password "<password>" -Label <label>
```
