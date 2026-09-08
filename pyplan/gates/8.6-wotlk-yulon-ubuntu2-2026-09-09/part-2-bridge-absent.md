# 8.6 part 2, second half — the bridge genuinely absent

The box's last clause: *"with the bridge absent the group says which precondition failed and never
reads as ready."* Three ways of making it absent are offered — stop the engine's conf key, restart,
or point at a missing script. All three were pressed on `yulon-ubuntu2` on 2026-09-09, in that order,
and the third one is the only one that made the SERVER forget the bridge.

## Why a restart was needed at all, measured rather than assumed

Two conf edits and two `reload ale` calls, each with the reading before and after:

| what was changed | `reload ale` | `dml_bridge_ping` afterwards |
|---|---|---|
| `ALE.ScriptPath` → `…/lua_scripts_absent` (a directory that does not exist) | `yes` | `DML-BRIDGE-READY dml_bridge_ping` |
| `ALE.Enabled = 0` | `yes` | `DML-BRIDGE-READY dml_bridge_ping` |

**`.reload ale` reloads scripts; it does not re-read either of these keys.** So on this tree the conf
can say the engine is off, or point at nothing at all, while the bridge is still answering — and the
world process has to be restarted before the conf and the wire agree. That is a per-tree fact,
measured here, and it is the reason this file exists rather than a paragraph in `part-2-party.md`.

It also says something about the box's own design from the other side: `read_probe` asks the SERVER
because the conf is not evidence. This is the same statement with the signs flipped — the conf is not
evidence that the bridge is *gone* either.

## The press

Ground, read first: `ALE.Enabled = 0` in `mod_ale.conf`, the world still up on the run started
`2026-09-08T21:15:35Z`, and `dml_bridge_ping` still answering `DML-BRIDGE-READY`. The group at that
moment already read `ready=False`, blocking on `engine_enabled` — from the conf — while
`bridge_answered` still read `ok`.

Then `docker restart ac-worldserver`. The new run started `2026-09-08T22:24:14Z`, and:

```
--- the server, asked ---
probe -> Answer(outcome='no', text="Command 'dml_bridge_ping' does not exist\r\n")

--- the group ---
ready=False
blocker='the Lua engine is switched off: ALE.Enabled is not set to 1 in mod_ale.conf. Its compiled
         default is off, whatever the comment beside it in the shipped file says.'
members=()
    ok server_installed   ok world_running   ok engine_cloned   ok engine_in_binary
    ok conf_present
    NO engine_enabled
        the Lua engine is switched off: ALE.Enabled is not set to 1 in mod_ale.conf. …
    ok script_path        ok deployed
    NO bridge_answered
        the bridge does not answer: this server does not know the bridge's own commands. If
        everything above this line is met, the world server has not read the scripts yet — restart
        it, and if it still does not answer, the Lua engine is not reading that directory.
```

`Command 'dml_bridge_ping' does not exist` is the same sentence, word for word, that this box read on
`yulon-ubuntu` on 2026-09-08 before the rebuild — and this time it was produced deliberately, from a
server that had been answering ten minutes earlier, so the classifier is shown recognising a real
absence rather than a permanent one.

**Two preconditions are unmet and both are shown; the blocker is the first.** That is the ordering
rule doing its job: the engine being switched off is what a person can act on, and "the bridge does
not answer" below it is its consequence rather than a second thing to fix.

## The worldserver's own log line for the bridge command

The box asks for it, and the run before the restart wrote all of it without being asked
(`docker logs ac-worldserver`):

```
[dml_bridge_ping] DML-BRIDGE-READY dml_bridge_ping
[dml_addclass]    Pakka ran: .playerbots bot addclass mage
[dml_whisper]     Pakka -> Jilsur: autogear
[dml_whisper]     Pakka -> Jilsur: talents autopick
[dml_bridge_ping] DML-BRIDGE-READY dml_bridge_ping
[dml_uninvite]    removed Jilsur from group
[dml_whisper]     Pakka -> Jilsur: logout
```

Every command the app sent, named by the script that ran it, in the world server's own file. Note
`[dml_addclass] Pakka ran: .playerbots bot addclass mage` — the playerbot command a console or SOAP
caller cannot even see, run inside Pakka's own session, which is the entire argument for the bridge.

## Put back

`ALE.Enabled = 1` and another `docker restart ac-worldserver`; the five bridge scripts announce
themselves in the startup log again and the box is left as the rebuild lane left it. Nothing else in
`mod_ale.conf` was touched — the file was copied to `/tmp/mod_ale.conf.bak` before the first edit and
the `ALE.ScriptPath` arm was restored from that copy.
