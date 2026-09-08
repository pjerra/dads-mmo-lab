# 8.6 part 1 — the rebuild answers questions 1–3

The 2026-09-08 investigation (`pyplan/gates/8.6-wotlk-yulon-ubuntu-2026-09-08/README.md`) stopped at
the rebuild line, as owner answer 4 scoped it, and left four questions that only a rebuild could
answer. The owner authorised the rebuild for `yulon-ubuntu2` on 2026-09-09. Three of the four are now
answered, on a machine, and the fourth is not this file's.

The press, its ground, its screenshots and the defect it found are in
`pyplan/gates/rebuild-live-yulon-ubuntu2-2026-09-09/`. This file is the answers.

---

## 1. Does `ALE.ScriptPath` read non-zero in the binary — did the module actually compile in?

**Yes.** `party.read_engine_in_binary('ac-worldserver')`, the same call that answered `False` on the
same install two hours earlier and on `yulon-ubuntu` the day before:

```
before the rebuild:  engine=False   the Lua engine is not built into this server's executable.
after  the rebuild:  engine=True    the Lua engine is built into this server's executable.
```

`False` and not `None` on both sides, so the control string (`AiPlayerbot.Enabled`) was found each
time and the reading was working when it said no. The image id under `ac-worldserver` changed with it:
`703ea6521431` → `8d99cf823bfc` (press 1) → `cad2566406d2` (press 2). Log: `6-after-new-binary.log`.

## 2. Does `mod_ale.conf` arrive with `ALE.Enabled = 1` and the absolute script path?

**Yes, and from the corrected manifest.** The applier's own report and the file afterwards
(`4-install-ale.log`):

```
activate env/dist/etc/modules/mod_ale.conf from conf/mod_ale.conf.dist
set 2 key(s) in env/dist/etc/modules/mod_ale.conf
ALE.Enabled = 1
ALE.ScriptPath = "/azerothcore/env/dist/etc/modules/lua_scripts"
```

`ALE.Enabled` is the key 8.6 measured out of `conf/mod_ale.conf.dist:63` and
`src/LuaEngine/ALEConfig.cpp:20` after finding our manifest had named `ALE.EnableLuaEngine`, which
the module does not have. Written rather than surfaced, because the compiled default is `false`. The
clone came up at the manifest's pinned revision `319f43edd58ffa6ed72873ddd68820ab86e4a99b`, which is
what the pin is for — `azerothcore/mod-ale` moved twice in the two days before it was written.

The container sees the directory the path names, holding the deployed scripts:

```
/azerothcore/env/dist/etc/modules/lua_scripts:
  dml_addclass.lua  dml_bridge_ping.lua  dml_login.lua  dml_uninvite.lua  dml_whisper.lua
```

## 3. Does `dml_bridge_ping` answer with `DML-BRIDGE-READY`?

**Yes.** Over the app's own command channel, on the build the rebuild made
(`7-ping.log`, repeated on press 2's binary in `8-ping-after-press2.log`):

```
--- 'dml_bridge_ping' ---
outcome='yes'  denied=False  indeterminate=False
text: 'DML-BRIDGE-READY dml_bridge_ping\r\n'
```

The ground for that question was taken on the same install, through the same channel, before the
module existed on disk:

```
--- 'dml_bridge_ping' ---   outcome='no'   text: "Command 'dml_bridge_ping' does not exist\r\n"
--- 'server info'      ---   outcome='yes'  text: 'AzerothCore rev. 413bea61a85e+ …'
```

and the world server's own startup log names all five scripts without being asked:

```
[dml_addclass]    loaded -- addclass relay ready
[dml_bridge_ping] loaded -- DML-BRIDGE-READY on request
[dml_login]       loaded -- bring-online relay ready
[dml_uninvite]    loaded -- group-remove relay ready
[dml_whisper]     loaded
```

`.reload ale` answers `yes` with no text, and the ping answers the same before and after it, so the
bridge is not standing on a reload.

**So the answer 8.6 could not give is now given: the server-side route works on this tree.** The
2026-08-20 attempt and 8.6's careful repeat of it both failed for the reason 8.6 named — the engine
was not in the image — and a rebuild is what changed it. Nothing about the route was reconfigured to
make this work; the same five scripts 8.6 left on disk are the ones answering.

## 4. Does `dml_addclass <player> mage` put a bot in the party frame?

**Not asked here.** That one needs a real client logged in, and this lane's press was the server side.
It is the only one of the four still open, and it is now the only thing between My Party and a
verdict — every precondition under it has been read on a machine rather than assumed:

* the engine is in the binary (1);
* the conf names the directory the scripts are in, and the container sees them (2);
* the bridge answers over the channel My Party would use (3);
* and 8.6 already measured the pool the command draws on, on its own box:
  `50 AddClass accounts`, `500 characters collected for addclass command`.

`8.6`'s other measured fact still stands and still matters for whoever presses 4: a console or SOAP
caller cannot *see* `playerbots bot addclass` in the USAGE list, because AzerothCore filters the
command table by `IsInvokerVisible(handler)`. That is the argument for the bridge, not a fault in it,
and a USAGE list missing `bot` must not be read as "the subcommand is gone".

## What this does not say

The rebuild ran **no SQL**, by design, and this file claims none. AzerothCore's own updater applies a
module's SQL for the modules CMake baked into `AC_MODULES_LIST`, and nothing in this repository
measures that — `REBUILD_CLOSING_NOTE` points the user at the worldserver log rather than promising
it, and so does this.

`mod-ale` also brought its own example script into the directory (`LootPet.lua`, present after press 2
and not written by `party.deploy()`). Worth knowing before somebody reads the directory listing as
"six bridge scripts".
