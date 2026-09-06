# The LAN button, pressed end to end on a remote Linux box — 2026-09-05/06

The **press and its readbacks** — every file the press produced here; the one exception is
`activity-log-excerpts.txt`, written in rounds 4 and 5, which dates itself — were taken between
**2026-09-05 22:59 UTC and 23:36:36 UTC** (2026-09-06 00:59–01:36 CEST) on four machines:
`yulon-ubuntu` (the live 7.2 WotLK install), `vmhost` (the Hyper-V host, Windows 10, which
ran the 3.3.5a client), and — for the speech-MPQ relay — the laptop and `m910q`. Two things
this record cites were done by someone else and fall outside it: the orchestrator's client
copy, finished `23:02:02` CEST = **21:02:02 UTC** (:263–266), and lane 710's copy of the
pre-press ufw rule file at **22:06** CEST = 20:06 UTC (:363). That window does **not** bound
this lane's own box actions, which went on after it; `activity-log-excerpts.txt`, committed
here, carries the two Linux boxes' own stamped lines. `vmhost`'s activity log is UTF-16 on
the host and is not committed, so the three `vmhost` stamps below — `02:16:52`, `02:26:23`
and `03:22:26` — are re-derivable only there. Three of the Linux lines fall in the small
hours of 2026-09-06 CEST, still 2026-09-05 in UTC: `m910q` **01:37:49 CEST = 23:37:49
UTC**, the relay directory `~/clients/b39-speech` and its `:8766` server removed; `m910q`
**01:48:57–01:56:57 CEST = 23:48:57–23:56:57 UTC**, the round-1 `--checks` run; and
`yulon-ubuntu` **01:51:18 CEST = 23:51:18 UTC**, the throwaway driver folder
`/home/pk/p7/b39` removed, its parent `/home/pk/p7` reading back `2026-09-06
01:51:18.345299699 +0200` as its own mtime, and `01:56:56` CEST the lane's closing line.
The rest is record-keeping later on 2026-09-06, when CEST and UTC share the date: round 2's
read-only correction pass, which the boxes stamp `02:16:18` CEST (`m910q`) and `02:16:52`–
`02:26:23` CEST (`vmhost`); the `stat -c` and `sha256sum` reads on `m910q` of rounds 2 and 3
(:286–292, :388–391); the `--checks` runs of every later fix and review stage, from the
round-1 review's at `02:02:20`–`02:06:00` CEST on; the `rm` of the throwaway relay
log on `m910q` at **02:56:35 CEST** (Cleanup); round 4's own reads, announced from
`03:22:00` CEST (`m910q`), `03:22:04` (`yulon-ubuntu`) and `03:22:26` (`vmhost`) onward —
among them the `ls -la --time-style=full-iso /home/pk/p7` at `03:29:22` CEST that re-derived
the mtime quoted above (the box's line at that stamp carries the stamp and nothing else:
`claude-say` ran with no message. Its `03:29:31` line records what the read was and blames
a wrong `claude-say` path; `activity-log-excerpts.txt` carries both lines and the check
that refutes that explanation); and round 5's own reads, announced at `03:58:57` CEST
(`yulon-ubuntu`), `03:58:58` (`m910q`) and `04:06:03` (`yulon-ubuntu` again, the
`claude-say` check), which quoted the two log ranges round 4 had skipped.

Of those, round 2's correction pass and rounds 4 and 5's reads changed nothing on any box
beyond the activity-terminal lines the rules require; the two `rm`s changed state on purpose,
and each `--checks` run synced a scratch checkout onto `m910q`. The correction pass's calls, as its
round's record attributes them: on `vmhost`, `Get-Date`, `tailscale ip -4`, `Get-ChildItem`,
`Get-Item` + `Get-Content -Tail` + `VersionInfo`, and two `Add-Content` lines to
`C:\Users\PK\claude-activity.log`; on `m910q`, `cat /tmp/b39-http.log`, `ls ~/clients`,
`pgrep -af`, one `scp` off the box and the `claude-say` lines that announce them; on the
laptop, one `stat`. The other rounds' calls are attributed where they appear rather than
listed here.

The 22:59–23:36:36 UTC bounds above are the earliest and latest stamps **of the press** a
reader can re-derive from the files committed here, `activity-log-excerpts.txt` aside (its
stamps run from `00:59:20` to `04:06:03` CEST and date the lane's later actions, not the
press): the first is `console-before.png`'s own top-bar clock, `Sep 6 00:59` CEST, and the
last is `ufw-restored.txt:59`, `01:36:36` CEST. (The earliest stamp inside a press text file
is `state-before.txt:1`, `2026-09-05T23:02:16Z`; the excerpt's first quoted line, `00:59:20`
CEST = `22:59:20` UTC, is the lane's opening announcement and precedes it by 176 s.) An
earlier draft of this line said "22:57 UTC and 23:38 UTC"; neither of those figures had an
artifact behind it. Drafts after that one said the window bounded everything this lane did
on a box; the two boxes' activity terminals refute that, which is what the enumeration
above replaced it with.

The box's own logs and the database are in **UTC**; the box's `date` and the activity
terminal print **CEST** (UTC+2). `vmhost`'s clock is CEST too — `Get-Date -Format o` there
printed `2026-09-06T02:16:54.5296260+02:00` when the readbacks in this file were taken —
so every stamp copied off that host is CEST unless it says `Z`. Both stamps are given
wherever it matters.

The code pressed was the committed tree at **`cfb4c04f`** — `git rev-parse HEAD` in
`/home/pk/p7/checkout` answered `cfb4c04f367536375abf6382694a1f800c468b8a` with
`git status --short` empty, at 01:06 CEST, immediately before the press. Nothing in
`pylauncher/` was modified for this run.

What this closes, in one sentence: **bug-checklist §39's last open claim — "the LAN button
as the owner wants it has still never been pressed on a real remote Linux box by the app" —
is no longer true.** It was pressed, twice, through the real widgets, on a box reached only
over ssh, and the ports it advertised were then used by a real client on another machine to
log in.

What it does **not** close is stated in "The bound this box puts on the press" below.
§39 is CLOSED on this press (`pyplan/bug-checklist.md:2643` reads
*"CLOSED 2026-09-05 on the press itself"*); the bound is recorded inside that closure as a
design fact and a limit of this box, not as a remaining repair.

---

## 1. The way back in, proved BEFORE anything was pressed

The 7.1 lockout (`pyplan/gates/7.1-ubuntu-2026-09-04/ufw-lockout.txt`) was recovered through
the hypervisor's synthetic keyboard, and `Msvm_Keyboard.TypeText` was unusable on that guest.
So the recovery route was re-proved on this guest first, with the same tools, before the
firewall was touched:

**How to read the stamps in this section.** The two scripts' stdout was watched at the
terminal and **not kept as a file**, so nothing in this folder reproduces it (the
`01:00:46` line in `activity-log-excerpts.txt` announces what was about to be done, not
what the scripts printed); an earlier draft of this section quoted it (*"sent
ctrl+alt+t"*, *"typed 27 key codes"*, *"sent Return"*) and gave a `00:59:53 CEST` stamp
that likewise exists nowhere. Both are dropped.
What IS committed is the three screenshots, and each carries the guest's own top-bar clock,
which is the stamp cited below.

* `C:\Users\PK\vmshot.ps1 -VMName yulon-ubuntu -Out …` → **`console-before.png`**
  (63,214 bytes), top bar `Sep 6 00:59` (CEST; 22:59 UTC): the GNOME desktop, a terminal at
  a `Choice:` prompt belonging to another lane, untouched by this one.
* `C:\Users\PK\vmkeys.ps1 -VMName yulon-ubuntu -OpenTerminal` then `-Keys 'echo b39 console
  proof 0101'` → **`console-keyboard-proof.png`** (36,826 bytes), top bar `Sep 6 01:01`
  (23:01 UTC), showing a second terminal window with
  `pk@yulon-ubuntu:~$ echo b39 console proof 0101` and its output `b39 console proof 0101`.
  So the hypervisor's synthetic keyboard reached a shell on this guest: that is the claim,
  and the image is the whole of the evidence for it.
  `-Keys` (virtual key codes) was used, not `-Text`/`TypeText`, for the reason the round-6
  record gives.
* The proof terminal was then closed with `-Keys 'exit'`; **`console-after.png`** (63,140 bytes),
  top bar `Sep 6 01:01`, shows the same desktop as `console-before.png` (63,214 bytes), the
  other lane's `Choice:` prompt still waiting.

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

**Those ports were almost certainly reachable before the press as well — but that is an
inference here, not a measurement, and the difference matters.** What was measured: ufw was
`inactive` with an empty `### RULES ###` section before the press (`press.txt:11,15`), and
after it `sudo -n iptables -S | grep -c ufw` answered **0** (`iptables-bound.txt:1`) — none
of ufw's chains is in the live ruleset while ufw is off, so the two rules the press wrote
have no effect on a packet today, and nothing else in the ruleset filters 3724 or 8085 (the
only live rules naming the game ports are Docker's own DNAT lines, `iptables-bound.txt:4-5`).
From that it follows that nothing was filtering those ports before the press either. What
was **not** measured by this lane is a `Test-NetConnection` from the host BEFORE the press:
the only probe in this folder is after it (`vmhost-ports.txt:2`, `2026-09-05T23:12:12Z`).
The lane brief reports one at 21:47 CEST, before any LAN step, answering `True` for 3724 —
that is a figure the brief hands over, not one this run took, and it is cited that way.

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

`vmhost:C:\clients\WoW-WotLK-3.3.5a-min` (native D3D9, no DXVK). The build number is the
executable's own: `(Get-Item …\wow.exe).VersionInfo` read on `vmhost` at
`2026-09-06T02:16:54+02:00` printed `FileVersion : 3, 3, 5, 12340`,
`ProductVersion : Version 3.3` — so **3.3.5a build 12340**.
`Data\enUS\realmlist.wtf` was written `set realmlist 172.30.55.119` and `WTF\Config.wtf`'s
`SET realmList` set to the same. `wowdrive-b39.ps1` ran as a scheduled task with `/ru PK /it`
so it landed in **console session 1** (`client-run3.log:1` records `session id: 1`), and
drove the login screen with `keybd_event` scancodes, the shape
`pyplan/gates/7.1-client-login/wowdrive.ps1` established.

It took three runs, and the two failures are recorded because each is a fact about the box:

1. **`client-run1.log`** — the client started and died without a window.
   `client-run1-missing-speech-mpq.jpg` shows why: *"Missing or corrupted data — Failed to
   open archive \*\*\*\*\speech-\*\*\*\*.MPQ."* The minimal copy
   (`vmhost:C:\clients\wotlk-pull.log`, whose last line is `done 2026-09-05T23:02:02` and
   whose `wotlk-pull.done` has mtime `9/5/2026 11:02:02 PM` on that CEST host — so
   **23:02:02 CEST = 21:02:02 UTC**; an earlier draft labelled it 23:02:02 UTC, which would
   have put the copy finishing four minutes before the press) had left out
   `speech-enus.mpq`, `expansion-speech-enus.mpq` and `lichking-speech-enus.mpq`.
   **Not a graphics failure** — worth saying, because the brief expected D3D9 to be the risk.

   **The relay of those three files, told against its artifacts, including the leg that
   failed.** Leg 1, laptop → `m910q` by `scp`, took **49 s**: `01:21:09` to `01:21:58` CEST
   (`speech-mpq-relay.log:1-5`), and `:6-11` is m910q's listing of the three at
   `438856302`, `241298910` and `354400446` bytes. Leg 2 as that log records it **failed**:
   three `curl -f` pulls from `http://100.78.24.50:8766/` exited `rc=7` in 2.3–2.6 s each
   (`:16-18`, `01:21:59`–`01:22:08`), and the script's closing `Get-ChildItem` on the host
   printed nothing at all because no file had been written. The transfer that actually
   landed the files was a **retry three minutes later whose client-side transcript was not
   kept** — that is a gap in this record and is stated as one. Its server side survives:
   `speech-relay-httpd.log` is m910q's `python3 -m http.server 8766` access log, copied from
   `m910q:/tmp/b39-http.log` at 02:16 CEST on 2026-09-06, and shows three `200` responses to
   `100.99.204.5` — `vmhost`'s Tailscale address (`tailscale ip -4` on that host printed
   `100.99.204.5`) — beginning `01:25:21`, `01:26:05` and `01:26:30` CEST, after one
   `01:25:11` self-test from m910q's own `100.78.24.50`. The host's files, read back at
   `2026-09-06T02:16:54+02:00`, have mtimes `1:26:05 AM`, `1:26:30 AM` and `1:27:05 AM`, each
   the completion of a request whose log line was already written when that request opened.
   That property is not assumed, it is read off the log file itself: `stat -c '%n %s %y'
   /tmp/b39-http.log` on `m910q` printed `331 2026-09-06 01:26:30.922109455 +0200` three
   times on 2026-09-06 — the round-2 reviewer, between `02:35:13` and `02:42:33` CEST (the
   bounds its own two lines in m910q's activity terminal give, quoted in
   `activity-log-excerpts.txt`; that reading printed no date of its own), the round-2 meta at
   `02:48:19` CEST, and round 3 at `02:56:01` CEST, all identical
   — and that last write of the log is **34.54 s BEFORE** `lichking-speech-enus.mpq`'s
   completion mtime, which `Get-ChildItem … LastWriteTime.ToString('o')` on `vmhost` printed in
   full precision as `2026-09-06T01:27:05.4629084+02:00` when it was read again at `03:22:26`
   CEST on 2026-09-06 (a folder listing shows the same mtime to the second, `1:27:05 AM`; an
   earlier draft of this line subtracted that rounded display instead and came out 0.46 s
   short). That is only possible if `python3 -m http.server` stamps its line when a request
   opens, not when it finishes. So leg 2 succeeded in **104 s** (`01:25:21` → `01:27:05`) for
   1,034,555,658 bytes, an arithmetic on stamps rather than an assertion. The access log
   stamps whole seconds only, so 104 s is the figure at the resolution it offers; against the
   host's full-precision mtime and a `:21.000` start it is 104.46 s. An earlier draft reported
   105 s while citing the failed log for it.

   Sizes: the host's `438856302` / `241298910` / `354400446` bytes equal m910q's listing at
   `speech-mpq-relay.log:9-11` and equal the laptop's originals, `stat -c '%n %s'
   /c/wow335ahd/Data/enUS/{speech,expansion-speech,lichking-speech}-enus.MPQ` at 02:16:45
   CEST on 2026-09-06 printing the same three numbers. That is a size comparison on all
   three hops, taken in this sitting; **no checksum was taken on any hop**, so the earlier
   draft's "byte for byte" overstated it and is dropped.
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
* On `m910q`: the relay directory `~/clients/b39-speech` and its `:8766` server were removed
  at **01:37:49 CEST = 23:37:49 UTC** on 2026-09-06 (`activity-log-excerpts.txt`, the m910q
  excerpt's `01:37:49` line); `~/clients` then held the 1.12.1 and 2.4.3 clients it held
  before (`ls ~/clients` at 02:56:35 CEST on 2026-09-06 printed `WoW-Client-1.12.1` and
  `WoW-Client-2.4.3`, nothing else).
  `/tmp/b39-http.log`, the 331-byte access log, was **removed at 02:56:35 CEST on
  2026-09-06** (`rm` then `ls -la` → `No such file or directory`); an earlier draft of this
  file said it had been kept on purpose so the relay could be re-derived from the box, which
  was not a reason — immediately before the removal, `sha256sum /tmp/b39-http.log` on the box
  printed `ffcd04b376cd9e56606e17611003d7786cda7cfbd6fb7b56537a29da8449f1c9` at 02:56:01
  CEST, and that is the hash of the committed copy `speech-relay-httpd.log`, so the box copy
  carried nothing this folder does not.
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
| `speech-mpq-relay.log` | the relay of the 1.03 GB the minimal client copy was missing — leg 1 succeeded, leg 2 as logged failed (`rc=7`) |
| `speech-relay-httpd.log` | m910q's `http.server` access log, the only surviving transcript of the leg-2 retry that landed the files |
| `activity-log-excerpts.txt` | the two Linux boxes' own activity-terminal lines for this lane — where every CEST announcement stamp the header gives for `m910q` or `yulon-ubuntu` up to round 4's comes from (the UTC halves beside them are those minus two hours); round 5's own three, `03:58:57`, `03:58:58` and `04:06:03`, are named here only in prose and stand on the boxes as `yulon-ubuntu` lines 151-152 and `m910q` line 405 (read 2026-09-06 04:33 CEST); `vmhost`'s activity log is UTF-16 on the host and is not committed, so its `02:16:52`, `02:26:23` and `03:22:26` are re-derivable only there, and the two stamps from other agents' work (:263–266, :363) come from their own records |
| `console-before.png` / `console-keyboard-proof.png` / `console-after.png` | the out-of-band way back in, proved first, and the desktop left as found |
| `client-run1-missing-speech-mpq.jpg` | the dialog run 1 died on |
| `client-login-screen.jpg` / `client-account-typed.jpg` / `client-after-auth.jpg` / `client-character-select.jpg` | the login, half-scale JPEG |
