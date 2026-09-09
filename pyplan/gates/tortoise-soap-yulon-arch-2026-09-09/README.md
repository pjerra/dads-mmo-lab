# Tortoise's command channel is SOAP — pressed on a fresh install, yulon-arch, 2026-09-08/09

The evidence behind `c7e577c7` ("Tortoise joins the SOAP trees"), written after the fact because the
press ran interleaved with the owner's own evening rather than as a lane.

The owner asked for it in those words on 2026-09-08: *"can you make a tortoise server on a vm with
the fixed soap and password? that way we dont need to rebuild the m910q yet."* So: a fresh install
of the Shyalya fork at the pin the catalog now names, on `yulon-arch`, and the channel driven
through the app's own seams.

## What the install proved, and what it cost

| | |
| --- | --- |
| box | `yulon-arch` (Hyper-V, Y: drive), docker without sudo, Python 3.13 at `~/y8v313` |
| tree | `~/tortoise-vm`, `wow-tortoise` at pin `3a8472e68e4aca5d2675feb9241e924f1c31899c` |
| first boot | ~21 minutes, world up with 500 playerbots |
| SOAP | `SOAP.Enabled = 1`, `SOAP.IP = 0.0.0.0`, `SOAP.Port = 7878`, namespace `urn:MaNGOS` |

**A fresh install of this fork does not come up without hand-applied character SQL.** The world
crash-looped on the char migration `20260903211500` until `src/tortoise-wow/sql/character_updates/*.sql`
were applied into `tw_char` by hand — `20260708055500` first, for the index the later one needs. The
catalog has no `sql.phases` entry for that directory, so **every** fresh Tortoise install hits it.
That is owed work, named here rather than quietly fixed under a different heading; the
`INSERT IGNORE` rewrite in the image template (`3a1ed6ee`) handles the *world* half of the same
family of problems and is not this.

## The channel: what the fork actually requires

`gm_level` was first written `3`, copied from Vanilla. The fork answered the first `server info`
with *"the account exists but its GM level is below administrator, which SOAP requires"*. This tree's
`account.rank` scale runs to 4 (measured in 8.3d) and its `SOAPThread::MinLevel` is
`SEC_ADMINISTRATOR` = 4 — **not** MaNGOS's 3. The catalog now says 4, the model's `le=` was widened
from 3 to 9, and a new `CatalogEntry` validator refuses any entry whose `operations.gm_level`
exceeds its own `accounts.level.max_level`, so a rank no row on that tree can hold is a catalog
error instead of a 401 on the first press.

## The fact this press exists to record: **the rank is read at startup**

`rank-cache-test3.log`. An account created with rank 4 **while the world runs** is refused, with the
row correct the entire time:

```
[20:25:11Z] made YULONCACHE1 with rank 4 while the world runs: 108  YULONCACHE1  4
[20:25:11Z] BEFORE any restart: HTTP 403 :: <faultstring>Error 403: HTTP 403 Forbidden</faultstring>
[20:25:11Z] restarting the world, nothing else changed
[20:26:55Z] AFTER the restart:  HTTP 200 :: <result>Core revision: ...
[20:26:55Z] GROUND: the row never changed: 4   1
```

The password was written here in the scheme `AccountMgr::CheckPassword` uses, so a refusal could not
be the password; the only thing that changed between the two questions was one restart. So on this
fork a channel account made after first start **cannot speak to the server until the world
restarts**, and `channel_setup`'s repair path needs to know that. Not yet fixed in the app.

### Two earlier attempts that proved nothing, kept on purpose

`rank-cache-test2.log` read HTTP 200 on both sides of its restart and concluded the refusals had been
the password. Its "before" was not before anything: the attempt before it had already restarted the
world four minutes after the account was made, so the restart it was trying to measure had already
happened. The first attempt was worse — it read the database password from a key name this install's
`.env` does not carry, so the password it thought it had written was never written, and it called the
world ready off a banner the fork prints while still loading. Both are here because a gate that only
keeps its successful run cannot show what a wrong reading looked like: this one was one confident
sentence away from filing "the lockout is fixed" against a measurement of nothing.
See [[a-confident-reason-with-nothing-behind-it]] in the working notes.

## What is NOT proved here

* **The password-change verdict.** The fork's account-lockout bug (bug list: an empty-name hash for
  any account that has logged in) was the original reason for this VM. Runs 3, 4 and 5 all reported
  `set_password -> done=False` with the server refusing the *channel* account, so the writer never
  reached the row and the question stands exactly where 8.3d left it. `tortoise-check5.log` is the
  clean statement of that refusal, not of the bug.
* **The published loopback port.** `operations.publish` is `true` and the override was not
  regenerated from the new catalog during this press, so `127.0.0.1:7878` was answered by the
  container's own address only.
* **Nothing was pressed through the UI.** The tab wiring (`_for_tortoise` → `channel_setup.InstallChannel`
  + `SoapChannel`, mirroring `_for_vanilla`) is covered by `test_controller_view.py`'s soap-trees
  test and was not clicked on a machine.

## Files

| file | what it is |
| --- | --- |
| `tortoise-check5.log` | the app's own seams end to end: enable, account create at rank 4, `server info`, the bot password attempt |
| `rank-cache-test3.log` | the startup-cache proof above |
| `rank-cache-test2.log` | the contaminated attempt, kept for the lesson |
| `rank-cache-test.sh` | the script of the third attempt, with its own account and a password it wrote itself |

Hashes in the logs are redacted to `<sha1-redacted>`; the generated channel password never appears.
