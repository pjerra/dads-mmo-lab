# 8.3a — Accounts, WoW WotLK — live gate, Linux half

**Where and when.** `yulon-ubuntu`, 2026-09-07 00:57Z – 01:02Z, against the finished 7.2 WotLK
install at `~/wowserver`, code at `ab059592`. The client half was driven on the Hyper-V host
(`DESKTOP-FP27AUV`) with `C:\clients\WoW-WotLK-3.3.5a-min`, in the interactive session.

**This box IS ticked** -- its Windows half was pressed on the gate box the same day (`pyplan/gates/8.3a-wotlk-yulon-win11-gate-2026-09-07/`), which is what its own line required. This README opened with "NOT ticked" for a day after that, and a reader trusting the folder over the checklist would have been misled (audit, 2026-09-08). Its own line says: *"Windows follows on the gate box once 8.2b has
run there, as its own press, and this box does not tick on Linux alone if the Windows half is
claimed — it is claimed, so both are gated."* 8.2b has not run. Everything below is the Linux half,
finished.

## What was proved

| Definition of done | Result |
|---|---|
| The list matches the account table read by hand minus the bots and the app's account | **equal** — `[(102, GATELOGIN, 3), (101, YULON, 3), (105, YULONGATE, 0)]` from the tab and from the same question asked by hand, out of **104** accounts in the table |
| After a password change the game client logs in with the new one | **it did** — the real 3.3.5a client reached the realm's character screen, `6-client-new-password-in.png` |
| …and is refused the old | **it was** — "The information you have entered is not valid", `5-client-old-password-refused.png` |
| After a level change the row reads the new value | `account_access` → `107  2  -1` |
| …**and the server's own account query reports it** | `.account info GATE83A` → `GMLevel: 2` |
| The app's own account cannot be selected for either action | it is absent from the list, and both actions refuse it by name; `.account info YULON_243C46E3` still reads `GMLevel: 3` |

The server's own record of the two client attempts, read afterwards through the channel:

```
| Account: GATE83A (ID: 107),
 GMLevel: 1
| Created: 2026-09-07 00:57:58
| Last Login: 2026-09-07 01:02:04 (Failed Logins: 0)
| OS: Win - Latency: 0 ms
| Last IP: 172.30.48.1 (Locked: No)
```

`Last Login` and `OS: Win` are the server's own account of the successful attempt — the same event
the screenshot shows from the client's side. AzerothCore clears `Failed Logins` on a success, which
is why it reads 0 after the pair rather than 1.

## The screenshots

| File | What it shows |
|---|---|
| `1-list.png` | the Accounts tab: four accounts out of 104, with their GM levels |
| `2-chosen.png` | one account chosen, and both changes now offered |
| `3-level-set.png` | a level change, in the server's own words, with the list already re-read |
| `4-refused.png` | what the tab says if the app's own account is put to either action |
| `5-client-old-password-refused.png` | the real client, old password |
| `6-client-new-password-in.png` | the real client, new password, on the realm |

Cropped to the client window: the host's desktop behind it carried a console with the gate's own
arguments in it, and evidence should be the thing being measured.

## How the client half was driven

`client-login.ps1` and `drive-login.ps1` (in this folder) run the client in the **interactive
session** via `schtasks /it`. A process started from an ssh session lands in session 0, where there
is no desktop: the client draws nothing and `SendKeys` reaches nothing. The command lives in a
`.cmd` file because `schtasks /tr` is capped at 261 characters. Both facts are already in
`windows-gate-box-recipes`; this is the first gate to use them for a client rather than a build.

The script writes `WTF\Config.wtf`'s realmlist, launches `Wow.exe` from its own directory, waits for
a window handle, types account / TAB / password / ENTER, and photographs the screen before typing,
after typing and after the answer. Nothing about the result is inferred from the script: the answer
is read off the screenshot and cross-checked against the server's own `account info`.

## Two per-tree facts, measured rather than assumed

`.account info $account` is the server's own account query. Its existence came from asking this
server `help account` and then `help account info` — the command table on this core has no
per-account lookup under any name a person would guess, and `account onlinelist` only answers for
accounts that are online.

The logical schema name `playerbots` is `acore_playerbots` on this install, and `auth` is
`acore_auth`. The by-hand query resolves both through `entry.schema_map()`, because a gate that
hard-codes a schema name is measuring its own assumption.

## How to re-run it

```
python gate83a.py  list | password | level | own | shots
```

`password` creates `GATE83A` if it is not there and leaves it with the new password; `level` sets
it to 2; `shots` sets it to 1 while photographing the tab.
