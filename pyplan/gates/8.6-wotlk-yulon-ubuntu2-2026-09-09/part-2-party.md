# 8.6 part 2 — My Party, pressed with a real client in the world

**Where and when.** `yulon-ubuntu2`, 2026-09-09 (the VM's clock is UTC; the Hyper-V host's is PST and
its timestamps run seven hours behind in the client logs quoted below). The server is the WotLK
install at `~/wowserver` on the image the rebuild lane built,
`sha256:cad2566406d2…` under `ac-worldserver`. The client is the real 3.3.5a client on the Hyper-V
host at `C:\clients\WoW-WotLK-3.3.5a-min`, driven through `schtasks /it`. Checkpoint
`8.6-before-my-party-2026-09-09` was taken on the host before anything was written. Every action was
announced on the box's activity terminal first.

Part 1 (`part-1-rebuild.md`) answered questions 1–3 and left one open:

> **4. Does `dml_addclass <player> mage` put a bot in the party frame?** … it is now the only thing
> between My Party and a verdict.

**It does.** This file is that press, and three others beside it.

---

## The ground, read before the press

`gate86b.py ground Pakka`, through the same `InstallParty` object
`ControllerServices.my_party` holds:

```
worldserver running started=2026-09-08T21:15:35Z restarts=0
facts: server_installed=True world_running=True engine_cloned=True engine_in_binary=True
       conf_present=True engine_enabled=True
       script_path='/azerothcore/env/dist/etc/modules/lua_scripts'
       deployed=('LootPet.lua','dml_addclass.lua','dml_bridge_ping.lua','dml_login.lua',
                 'dml_uninvite.lua','dml_whisper.lua')
       bridge_answered=True
ready: True  blocker: None
marker: Marker(prefix='rndbot', source='default')
Pakka online guid: 1001
party rows now: ()
group_member rows on the whole server: 0
```

**`group_member` held zero rows on the whole server.** That is the reading that makes the press mean
anything: a row appearing after it cannot be a row that was already there, and 500 of the 1001
characters on this install are bots. The same read was taken again with the client in the world,
immediately before the add, and answered the same.

## The press, and what the client showed

| # | File | What it shows | `Wow.exe` at that moment |
|---|---|---|---|
| 0 | `0-character-select.png` | the realm `Yulon ubuntu2` and `Pakka, Level 4 Warrior, Durotar` — a real client, really logged in | `alive pid=19760` |
| 1 | `1-in-world-no-party.png` | **the ground**: Pakka in Burning Blade Coven, one unit frame, no party | `alive pid=19760` |
| 2 | `2-party-frame-with-bot.png` | **the answer**: `Jilsur` in the party frame under Pakka, standing beside him in the world | `alive pid=19760` |
| 3 | `3-party-frame-after-dismiss.png` | the party frame gone, the bot gone from the world | `alive pid=19760` |

Frame 1 is named for what it is rather than "before", and it was *looked at* before it was accepted:
the first two attempts at it photographed a login screen and were deleted rather than filed under
that name (see "What went wrong" below). Liveness is read before every capture and written beside it,
because a client that has died photographs exactly like a client refusing to show a bot.

**The app's own report of the add** (`gate86b.py add Pakka mage`):

```
BEFORE: ()
RESULT: Addition(added=True, joined=True, bot='Jilsur', geared=True, specced=True,
                 sentence='Jilsur joined the party, geared and specced.', blocker='')
AFTER : (Member(name='Jilsur', guid=948, klass=8, level=1),)
```

`klass=8` is MAGE, which is the class that was asked for. One row, on a bot the marker recognises —
the group read is `dbreads.bot_clause`, the same two-armed clause the Bots tab counts with, so the
master's own `group_member` row cannot appear in it.

**And the client said the same thing in its own words**, in the chat log visible in frame 2:

```
Jilsur joins the party.
Looting changed to Group Loot.
Loot threshold set to Uncommon.
To [Jilsur]: autogear
Accepting Whisper: ON
To [Jilsur]: talents autopick
[Jilsur] whispers: Auto pick talents
Jilsur greets everyone with a hearty hello!
```

That is "geared and specced" arriving as two whispers the server acted on, not as a claim: `addclass`
alone does neither, which is why the prior art whispers afterwards
(`rust-main:crates/dml-wow/src/party.rs:246,249`).

**The dismiss**, and the client's words for it (frame 3):

```
BEFORE: (Member(name='Jilsur', guid=948, klass=8, level=1),)
RESULT: Dismissal(removed=True, logged_out=True, sentence='Jilsur left the party.')
AFTER : ()

    Your group has been disbanded.
    [Jilsur] whispers: AI was reset to defaults
    [Jilsur] whispers: I'm logging out!
    [Jilsur] whispers: Bye bye!
```

`removed` is the group table read back afterwards, never the uninvite's own `yes`.

## The bridge absent: which precondition failed, and never ready

Two arms were pressed, each with its ground read first.

**Arm 1 — point at a missing script.** `ALE.ScriptPath` was pointed at
`…/lua_scripts_absent` and the engine reloaded. The group:

```
ready=False
blocker="the Lua engine is looking for scripts in '…/lua_scripts_absent', which is not where the
         bridge was deployed. It must be /azerothcore/env/dist/etc/modules/lua_scripts."
    ok server_installed … ok conf_present  ok engine_enabled
    NO script_path
    ok deployed  ok bridge_answered
```

and a press with it in that state:

```
RESULT: Addition(added=False, joined=False, bot=None, …, blocker="the Lua engine is looking for
        scripts in '…/lua_scripts_absent' …")
AFTER : ()
```

**Nothing was sent.** That is the whole difference from 2026-08-20, when a control pressed happily
into a bridge that was not there and reported success. The conf was restored and the group read
`ready=True` again, so the change was the cause and it is reversible.

**Arm 2 — stop the engine's conf key.** `ALE.Enabled = 0`, and the group named that precondition
instead:

```
ready=False
blocker='the Lua engine is switched off: ALE.Enabled is not set to 1 in mod_ale.conf. Its compiled
         default is off, whatever the comment beside it in the shipped file says.'
    NO engine_enabled
```

## What this box refuted

**1. `LootPet.lua` made a complete bridge report as a partial one.** The `deployed` precondition
asked for set EQUALITY with `BRIDGE_SCRIPTS`, and `mod-ale` installs its own example script into the
very directory the bridge is deployed to — the ground read above shows six files where the app
expected five. On the only install where the Lua engine has ever run, My Party would have refused
every press with *"some of the bridge scripts are missing … ()"*, naming nothing, while all five were
there. The precondition now asks that ours are all present. Test:
`test_a_script_the_module_brought_is_not_a_missing_bridge_script`.

**2. `.reload ale` does not act on `ALE.Enabled` or `ALE.ScriptPath`.** Measured here, twice, on this
tree: with `ALE.ScriptPath` pointing at a directory that does not exist and after a `reload ale` that
answered `yes`, `dml_bridge_ping` still answered `DML-BRIDGE-READY`; with `ALE.Enabled = 0` and
another `yes` from `reload ale`, it answered again. **So the conf can say the engine is off while the
bridge is still running, and it can point at nothing while the bridge still answers.** Two
consequences, and both belong to this module rather than to a sibling's:

* the conf is not evidence about the bridge in either direction, which is the argument for asking the
  server (`read_probe`) that this box was built on — restated, now from the *other* side;
* a person switching the engine off and pressing again gets the app's refusal, correctly, from the
  conf — and the server they are talking to has not changed at all until it restarts.

**3. `Data\<locale>\realmlist.wtf` beats `Config.wtf`'s `SET realmlist`.** 8.5a's `client-who.ps1`
writes only `WTF\Config.wtf`. This client's `Data\enUS\realmlist.wtf` held `172.30.55.155`, a VM that
no longer exists, so three login attempts went to an address nothing answers. The discriminator that
found it was the auth server's own row rather than the screen: `account.failed_logins` stayed **0**
and `last_login` stayed at a timestamp from before the run — a wrong password increments one and
moves the other, and *nothing arriving* moves neither. `client-party.ps1` now writes all three files
and says why.

**4. The host's `netsh portproxy` for 3724/8085 accepts and does not forward.**
`Test-NetConnection 100.99.204.5 -Port 3724` answered `True` while a `tcpdump` on the VM saw no
packet, because the proxy's own listener completes the handshake. The realm was pointed at the VM's
own address (`172.30.48.189`) for the run; **the row was `100.99.204.5` before and is left changed**
(see "Left on the box"). `Test-NetConnection … -InformationLevel Quiet` is not evidence that a
forwarded port forwards.

## What was NOT proved here

**The `bridge_answered=False` arm was not pressed on this box until the restart below.** Finding 2
is why: while the world process is up, no conf edit can make the bridge stop answering, so the only
route to a genuinely absent bridge is a worldserver restart with the engine off. That restart was
run, and its outcome is recorded in `part-2-bridge-absent.md` beside this file. The pre-rebuild
refusal (`Command 'dml_bridge_ping' does not exist`) is already photographed on `yulon-ubuntu` in
`pyplan/gates/8.6-wotlk-yulon-ubuntu-2026-09-08/`.

**The Qt widget for My Party does not exist.** What was built is the seam:
`party.InstallParty`, the `MyPartySeam` protocol, and `ControllerServices.my_party` wired in
`_for_wotlk` — the same shape as 8.5a's `bots` and 8.4a's `play`. Every press above went through that
object. Drawing the tab is not in this lane and is not claimed.

**Nothing was measured about a second bot, a full party, or presets.** One bot, added and dismissed.

## The scripts

* `gate86b.py` — the driver. Builds `InstallParty` from the same `DockerSql`,
  `channel_setup.InstallChannel.live_channel` and `docker.container_state` that
  `controller_view._for_wotlk` builds it from, and prints the worldserver's liveness before every
  subcommand.
* `client-party.ps1` — starts the client, logs in, waits in the world for a flag per frame.
* `client-login-fix.ps1` — clicks the login fields at measured points. It exists because blind
  `SendKeys` typed into the wrong box; its header records the frame that was deleted for it.
* `client-shoot.ps1` — one click and one photograph, with `Wow.exe`'s liveness in the log.

## Left on the box

* **`acore_auth.realmlist.address` and `.localAddress` are `172.30.48.189`**, changed from
  `100.99.204.5` for the reason in finding 4. A client on the Hyper-V host reaches the server at that
  address; a client anywhere else needs the host proxy fixed or the row put back.
* **The client's three realmlist files on the host now say `172.30.48.189`** (they said
  `172.30.55.119` and `172.30.55.155`, both dead).
* **The `PERZI` account's password was changed** through 8.3a's own Set-password seam. It is not
  written down here.
* The bridge scripts, `mod_ale.conf` and the image are as the rebuild lane left them.
* The scheduled tasks `gate86client`, `gate86fix`, `gate86shot` and `gate86shoot2` on `vmhost`, and
  `C:\gate86\`, are deleted at the end of this run — a stale one re-fires at 23:59.
