# The LAN button, pressed end to end on a remote Linux box — 2026-09-05/06

Everything below happened between **2026-09-05 22:57 UTC and 23:38 UTC** (2026-09-06
00:57–01:38 CEST) on `yulon-ubuntu`, the live 7.2 WotLK install at `/home/pk/wowserver`,
with a real 3.3.5a client on `vmhost` (the Hyper-V host, Windows 10). The box's own logs
and the database are in **UTC**; the box's `date` and the activity terminal print **CEST**
(UTC+2). Both stamps are given wherever it matters.

The code pressed was the committed tree at **`cfb4c04f`** — `git rev-parse HEAD` in
`/home/pk/p7/checkout` answered `cfb4c04f367536375abf6382694a1f800c468b8a` with
`git status --short` empty, at 01:06 CEST, immediately before the press. Nothing in
`pylauncher/` was modified for this run.

What this closes, in one sentence: **bug-checklist §39's last open claim — "the LAN button
as the owner wants it has still never been pressed on a real remote Linux box by the app" —
is no longer true.** It was pressed, twice, through the real widgets, on a box reached only
over ssh, and the ports it advertised were then used by a real client on another machine to
log in.

What it does **not** close is stated in "The bound this box puts on the press" below, and
is the reason §39 keeps one OPEN paragraph.

---

## 1. The way back in, proved BEFORE anything was pressed

The 7.1 lockout (`pyplan/gates/7.1-ubuntu-2026-09-04/ufw-lockout.txt`) was recovered through
the hypervisor's synthetic keyboard, and `Msvm_Keyboard.TypeText` was unusable on that guest.
So the recovery route was re-proved on this guest first, with the same tools, before the
firewall was touched:

* `C:\Users\PK\vmshot.ps1 -VMName yulon-ubuntu` at 00:59:53 CEST → **`console-before.png`**
  (63,214 bytes): the GNOME desktop, a terminal at a `Choice:` prompt belonging to another
  lane, untouched by this one.
* `C:\Users\PK\vmkeys.ps1 -VMName yulon-ubuntu -OpenTerminal` then `-Keys 'echo b39 console
  proof 0101'` → *"sent ctrl+alt+t"*, *"typed 27 key codes"*, *"sent Return"*, and
  **`console-keyboard-proof.png`** (36,826 bytes) shows a second terminal window with
  `pk@yulon-ubuntu:~$ echo b39 console proof 0101` and its output `b39 console proof 0101`.
  `-Keys` (virtual key codes) was used, not `-Text`/`TypeText`, for the reason the round-6
  record gives.
* The proof terminal was then closed with `-Keys 'exit'`; **`console-after.png`** (63,140 bytes) shows
  the same desktop as `console-before.png` (63,214 bytes), the other lane's `Choice:` prompt
  still waiting; the two differ in the clock in the top bar.

## 2. The failsafes, armed before the press and cancelled after it

`failsafe-armed.txt` (23:02:34 UTC):

```
sudo -n systemd-run --on-active=600 --unit=b39-failsafe /usr/sbin/ufw disable
sudo -n systemd-run --on-active=300 --unit=b39-failsafe2 /bin/sh -c "if /usr/sbin/ufw status | grep -q \"Status: active\"; then /usr/sbin/ufw allow 22/tcp; /usr/sbin/ufw disable; fi"
```

`systemctl list-timers 'b39-*'` then listed `b39-failsafe2.timer` at 01:07:34 CEST and
`b39-failsafe.timer` at 01:12:34 CEST (`failsafe-armed.txt:38-41`).

`b39-failsafe2` **did fire**, at 01:07:42 CEST, between the two presses: its condition read
ufw as inactive, so it ran nothing —
`Started b39-failsafe2.service` followed by `Deactivated successfully`
(`failsafe-cancelled.txt`), and `no dport 22 rule in user.rules` afterwards
(`guishape-and-newssh.txt:3`). `b39-failsafe.timer` was stopped at 23:10:18 UTC with
`2min 15s` left on it, and `systemctl list-units 'b39-*' --all` then printed
`0 loaded units listed.` (`failsafe-cancelled.txt`).

## 3. The press itself — real widgets, `QTest.mouseClick`, twice

`press-driver.py` builds the real `ControllerView` over
`ControllerServices.for_entry(load_catalog().get("wow-wotlk"), Path("/home/pk/wowserver"))`
— the object `main.py` builds for a real install — offscreen (`QT_QPA_PLATFORM=offscreen`),
with `status_poll_ms=0` so no timer can fill a label that a click did not. `Show plan` and
`Apply` are pressed with `QTest.mouseClick` on the real `QPushButton`s. Nothing calls
`networking.plan()` or `networking.apply()` directly.

**Press 1, `press.txt`, 23:06:56–23:06:57 UTC.** In order:

| what | where |
|---|---|
| `Apply` disabled before a plan exists | `press.txt:50` |
| `Show plan` clicked; the tab shows `working out the plan…` the same instant | `:56-59` |
| the plan the widget renders — `ufw allow 3724/tcp`, `ufw allow 8085/tcp`, the realmlist UPDATE, and the withheld-enable warning | `:60-70`; commands at `:65-66`, SQL at `:67` |
| `Apply` went live only once the plan arrived | `:71` |
| `Apply` clicked | `:77` |
| the module's own line: **`networking lan for wow-wotlk: 3 done, 1 skipped (1 refused), 0 manual`** | `:79` |
| the report the widget renders: `✓ ufw allow 3724/tcp`, `✓ ufw allow 8085/tcp`, `✓ realmlist → 172.30.55.119`, then `Could not do (run by hand):` with the withheld enable | `:82-92` |
| modal dialogs seen during the whole run: `[]` | `:94` |

So, in the terms §39 opened with: **`report.done` carries the two allows and the realmlist,
`report.skipped` carries one entry and it is the refusal, `manual_steps` is empty, and the
warning names SSH by name.** The four empty reports the bug was filed on are not what this
press produced.

Read back by routes that are not the widget:

* `sudo -n ufw status numbered` — `Status: inactive` before (`:11`) **and after** (`:99`).
  The enable was withheld, so ufw is exactly as the box had it.
* `/etc/ufw/user.rules` — `(no line mentions 3724 or 8085)` before (`:15`);
  `-A ufw-user-input -p tcp --dport 3724 -j ACCEPT` and the same for 8085 after
  (`:104`, `:106`). `ufw status` prints no rules at all while ufw is inactive, which is why
  the rule file is the readback that can answer this.
* `sudo -n ss -lntp | grep :22` — the same two `sshd`/`systemd` listener lines before and
  after (`:22-23`, `:113-114`).
* A **new** TCP connection to `172.30.55.119:22`, opened by `socket.create_connection` from
  the box, got `SSH-2.0-OpenSSH_9.6p1 Ubuntu-3ubuntu13.19` after the press (`:123`). That is
  the half the 7.1 lockout killed: the established session survived that outage too.
* A **new ssh login from the laptop** answered at 23:10:59 UTC:
  `NEW-SSH-LOGIN-OK from 172.30.48.1 64822 172.30.55.119 22`
  (`guishape-and-newssh.txt:2`).

`press.txt` ends `PASSES 18 FAILS 1`. **The one failure is the driver's assertion, not the
app**: `press-driver.py`'s enable check lowercased the plan text before splitting it on
`"Warnings:"`, so the split never happened and the word `enable` was found in the warning
prose. The driver is committed exactly as it ran. The claim it meant to make is asserted
correctly in press 2.

**Press 2, `press2.txt`, 23:09:37–23:09:38 UTC**, `PASSES 19 FAILS 0`
(`press-driver-2.py`, whose docstring says what it changed and why):

* `the plan's firewall block is exactly the two allows, with no enable in it --
  ['ufw allow 3724/tcp', 'ufw allow 8085/tcp']` (`press2.txt:75`).
* Same module line: `3 done, 1 skipped (1 refused), 0 manual` (`:80`).
* `this SECOND press changed nothing in the rule list (the allows are idempotent) --
  4 game-port lines before, 4 after` (`:122`). Pressing `Apply` twice is what a user does;
  it neither doubled a rule nor failed.

**The GUI shape, measured in the same sitting.** Both presses ran over ssh, so
`SSH_CONNECTION` was set (`press.txt:5`) and `detect_ssh_route()` read
`connected=True, ports=(22,)`. A launcher on the box's own desktop has no `SSH_CONNECTION`.
That shape was measured too, read-only: with the three `SSH_*` variables stripped,
`detect_ssh_route()` read `connected=False, ports=()` and `firewall_commands` was the
**same two allows**, one refusal, no manual steps, and `_format_plan()` produced the same
text (`guishape-and-newssh.txt:6-20`). The default plan withholds the enable in both shapes,
so the press is not an artefact of having been run over ssh.

## 4. Reachable from another machine

`vmhost-ports.txt`, 23:12:12 UTC, from the Hyper-V host:

```
port 3724 TcpTestSucceeded=True SourceAddress=172.30.48.1 RemoteAddress=172.30.55.119
port 8085 TcpTestSucceeded=True SourceAddress=172.30.48.1 RemoteAddress=172.30.55.119
port 22   TcpTestSucceeded=True SourceAddress=172.30.48.1 RemoteAddress=172.30.55.119
```

`172.30.48.1` is the host's own address on `vEthernet (Default Switch)`, `/20`
(`vmhost-ports.txt:10`) — the same subnet as the VM's `172.30.55.119/20`.

### The bound this box puts on the press

**Those ports were reachable before the press as well, and this run can prove why rather
than merely say so.** ufw was `inactive` with an empty `### RULES ###` section before the
press (`press.txt:11,15`), and after it `sudo -n iptables -S | grep -c ufw` answered **0**
(`iptables-bound.txt:1`): none of ufw's chains is in the live ruleset while ufw is off, so
the two rules the press wrote have no effect on a packet today. The only live rules naming
the game ports are Docker's own DNAT lines (`iptables-bound.txt:4-5`).

So on this box the press proves that the app **writes the right rules and refuses the
dangerous one**; it does not prove that opening a port changed reachability, because nothing
was closed. Proving that half needs a box where ufw is already enabled — and the app
deliberately never enables it (§39 rounds 6–10), so that box has to arrive that way.

## 5. The account, made by the Accounts tile

`account-driver.py` typed into the real `QLineEdit`s with `QTest.keyClicks` and clicked
`Create` with `QTest.mouseClick`. `account.txt`:

* before: `COUNT(*)` for `LANGATE` = `0` (`:4`)
* `account_report -> 'LANGATE: created (id 104).'` (`:9`)
* read back by `docker exec ac-database mysql`:
  `104 LANGATE 32 32 -99 NULL 127.0.0.1 0 2026-09-05 23:12:00` (`:13`) — a real SRP6 row
  (salt 32, verifier 32), no `account_access` row, which is what GM 0 leaves.

## 6. The client login, from the other machine, with no tunnel

`vmhost:C:\clients\WoW-WotLK-3.3.5a-min` (3.3.5a build 12340, native D3D9, no DXVK).
`Data\enUS\realmlist.wtf` was written `set realmlist 172.30.55.119` and `WTF\Config.wtf`'s
`SET realmList` set to the same. `wowdrive-b39.ps1` ran as a scheduled task with `/ru PK /it`
so it landed in **console session 1** (`client-run3.log:1` records `session id: 1`), and
drove the login screen with `keybd_event` scancodes, the shape
`pyplan/gates/7.1-client-login/wowdrive.ps1` established.

It took three runs, and the two failures are recorded because each is a fact about the box:

1. **`client-run1.log`** — the client started and died without a window.
   `client-run1-missing-speech-mpq.jpg` shows why: *"Missing or corrupted data — Failed to
   open archive \*\*\*\*\speech-\*\*\*\*.MPQ."* The minimal copy
   (`vmhost:C:\clients\wotlk-pull.log`, finished 23:02:02 UTC) had left out
   `speech-enus.mpq`, `expansion-speech-enus.mpq` and `lichking-speech-enus.mpq`.
   **Not a graphics failure** — worth saying, because the brief expected D3D9 to be the risk.
   The three files (1.03 GB) were relayed laptop → `m910q` → host in 49 s + 105 s
   (`speech-mpq-relay.log`), and their sizes on the host matched the laptop's byte for byte.
2. **`client-run2.log`** — the login screen came up (`window handle 4196040, title 'World of
   Warcraft'`, `focus: True` at 01:28:25 CEST) and then **my own Escape keystroke quit the
   client**: the driver sent one "in case a dialog is up", and one second later every
   screenshot is of the desktop. The account field is focused the moment that screen draws
   (`client-login-screen.jpg`), so the Escape was removed. The committed
   `wowdrive-b39.ps1` is the run-3 version and says so in a comment; runs 1 and 2 differ from
   it in exactly two ways their logs record (no `FindWindow` fallback in run 1, an Escape in
   runs 1 and 2).
3. **`client-run3.log`, 01:30:55–01:32:04 CEST — the login.** `LANGATE` and its password were
   typed, Return sent at 01:31:23, and the client's own
   `client-connection-20260906-0131.log` records the whole exchange:

   ```
   :2   9/6 01:31:23.751  GRUNT: state: RESPONSE_CONNECTED result: LOGIN_OK 172.30.55.119:3724
   :6   9/6 01:31:23.951  GRUNT: state: LOGIN_STATE_AUTHENTICATED result: LOGIN_OK
   :10  9/6 01:31:24.195  ClientConnection Completed: COP_AUTHENTICATE code=AUTH_OK result=TRUE
   :12  9/6 01:31:24.770  ClientConnection Completed: COP_GET_CHARACTERS code=44 result=TRUE
   ```

   `client-character-select.jpg` is the character-selection screen with the realm name
   `AzerothCore` and `Create New Character` — an empty character list, because none was made.

**The server's own record, `serverside.txt` at 23:32:38 UTC**, read by `docker exec` +
`SELECT`, a route the client had no part in:

```
:5  104  LANGATE  2026-09-05 23:31:24  172.30.48.1  0  1  2026-09-05 23:12:00
:8  id=104 last_login=2026-09-05 23:31:24 last_ip=172.30.48.1 last_attempt_ip=172.30.48.1 failed_logins=0 online=1
```

`last_login 23:31:24` UTC is the client's `01:31:24` CEST — the same event, to the second.
`failed_logins 0`: first try. **`last_ip 172.30.48.1` is the Hyper-V host's own address on
the Default Switch** (`vmhost-ports.txt:10`), which is the thing the 2026-09-05 07:31 login
could not show: that one went through `ssh -L` and the server recorded
`last_ip 172.18.0.1`, Docker's bridge gateway
(`pyplan/gates/7.1-client-login/LOGIN-2026-09-05.md`). This login was neither tunnelled nor
loopback: the client was told `172.30.55.119`, it connected there, and the server logged the
address it came from.

What this login does **not** show, the same two limits the 09-04 and 09-05 records state:
`Enter World` was never clicked and no character was created, so nothing here speaks to the
world server beyond a character-list reply.

## 7. The box, put back

`ufw-restored.txt` and `state-after-cleanup.txt`, 23:35:35–23:36:36 UTC:

* `sudo -n ufw delete allow 3724/tcp` and `8085/tcp` — `Rules updated` / `Rules updated (v6)`
  each; `grep -E "3724|8085"` over `user.rules` and `user6.rules` then found nothing.
* **ufw rewrote its rule file when the press wrote the first rule**, and deleting the rules
  did not undo that: the file went 307 bytes → 1479 during the press and stood at **1269**
  bytes after the deletes, because ufw replaced the box's truncated file with its full
  canonical template (LOGGING and RATE LIMITING sections). The pre-press bytes existed —
  lane 710 had copied them at 22:06 as `/home/pk/p7/out710/ufw-user.rules.before`, 307 bytes,
  `sha256 320f53e1ee90a7fd92f17b67f50b06b51cb20998cd52f01dbcb52e24160618bf`, and its content
  is character-for-character what this lane's own pre-press read printed
  (`state-before.txt`) — so both files were restored from that copy and re-hashed:
  `320f53e1…` / 307 bytes and 107 bytes for `user6.rules`. `ufw status verbose` →
  `Status: inactive`; `iptables -S | grep -c ufw` → `0`; mode `-rw-r----- root root` on both.
* `LANGATE`'s rows deleted from `acore_auth.realmcharacters`, `account_access` and `account`
  by three single statements; readback `0 / 0 / 0` and `MAX(id)` = **102**
  (`state-after-cleanup.txt`), the same maximum this box had before lane 710 and this lane
  ran (`state-before.txt` lists 102 `GATELOGIN` as the last row).
* The realm row was **not** changed by anything here: it read
  `1 AzerothCore 172.30.55.119 172.30.55.119 255.255.255.0 8085` before the press
  (`press.txt:19`), after it (`:110`) and after the cleanup
  (`state-after-cleanup.txt`) — the app's UPDATE wrote the value the row already held, which
  is also why `report.done` can carry `realmlist → 172.30.55.119` with nothing having moved.
* No `b39-*` units; the three `ac-*` containers `Up 3 hours`; the checkout and venv at
  `/home/pk/p7/` left for whoever needs them next.
* On `m910q`: the relay directory `~/clients/b39-speech` and its `:8766` server are gone;
  `~/clients` holds the 1.12.1 and 2.4.3 clients it held before.
* On `vmhost`: the client was closed (`Get-Process wow` → 0), the `b39-wow` scheduled task
  deleted, `C:` at 123.9 GB free. The three speech MPQs were **left in place**: they make the
  copied client work, and the client itself is the owner's, staged for this gate.

## Files

| file | what it is |
|---|---|
| `press-driver.py` / `press.txt` | press 1, exactly as run (one wrong assertion, see §3) |
| `press-driver-2.py` / `press2.txt` | press 2, the corrected assertion and the idempotence check |
| `account-driver.py` / `account.txt` | `LANGATE` made through the Accounts tile |
| `wowdrive-b39.ps1` | the client driver, run-3 version |
| `client-run1.log` / `client-run2.log` / `client-run3.log` | the three client runs |
| `client-connection-20260906-0131.log` | the client's own 12-line trace of the login |
| `serverside.txt` | the `acore_auth.account` row, read by `docker exec` + `SELECT` |
| `state-before.txt` | the box before anything: ufw, rules, realm row, accounts, listeners, units |
| `failsafe-armed.txt` / `failsafe-cancelled.txt` | the two timers, armed and cancelled |
| `guishape-and-newssh.txt` | a new ssh login, and the plan with `SSH_CONNECTION` stripped |
| `vmhost-ports.txt` | `Test-NetConnection` from the other machine, and its own address |
| `iptables-bound.txt` | why the reachability above is not evidence the press caused it |
| `ufw-restored.txt` / `state-after-cleanup.txt` | the box put back, with hashes |
| `speech-mpq-relay.log` | the 1.03 GB the minimal client copy was missing |
| `console-before.png` / `console-keyboard-proof.png` / `console-after.png` | the out-of-band way back in, proved first, and the desktop left as found |
| `client-run1-missing-speech-mpq.jpg` | the dialog run 1 died on |
| `client-login-screen.jpg` / `client-account-typed.jpg` / `client-after-auth.jpg` / `client-character-select.jpg` | the login, half-scale JPEG |
