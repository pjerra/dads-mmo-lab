# 8.6 — My Party, WoW WotLK — the investigation up to the rebuild line

**Where and when.** `yulon-ubuntu`, 2026-09-08 09:38Z – 10:01Z, against the 7.2 WotLK install at
`~/wowserver` (image `yulon.local/ac-wotlk-worldserver:native-243c46e3`, built 2026-09-04, core
`413bea61a85e+`, mod-playerbots at `b949b50b`). Checkpoint `8.6-before-bridge-deploy-2026-09-08`
taken before anything was written. Every action was announced on that box's activity terminal
first. **The world was never restarted and never stopped**: `pid=205047 restarts=0
started=2026-09-08T09:07:39Z` at the start, at every screenshot, and at the end.

Owner answer 4 of 2026-09-08 scopes this run: *"investigate up to the rebuild line; the owner runs
the rebuild, if it is still worth running."*

---

## The box's first question, answered

> **Does the server-side route work at all?**
>
> **No — and not for the reason anybody had written down.** The Lua engine is not in the image, so
> the route cannot work today by any amount of configuration. A rebuild is the only thing that can
> change that answer, and after it the route is plausible on this tree for reasons measured below,
> not assumed.

The 2026-08-20 experiment was repeated properly and it answers the same way it did then, with the
cause named this time. Read `1-ground.txt`, then `2-deploy.txt`, then `3-after.txt` in order: the
deploy changed the disk and changed **nothing** about what the server says.

| | ground (09:54Z) | after the deploy (09:59Z) |
|---|---|---|
| `modules/mod-ale` cloned | False | False |
| ALE in the worldserver binary | **False** | **False** |
| `mod_ale.conf` exists | False | False |
| bridge scripts on disk | `[]` | **all five** |
| the server, asked `dml_bridge_ping` | `Command 'dml_bridge_ping' does not exist` | **the same, word for word** |

That table is the whole finding. A deploy that reports success is not evidence of a bridge, which
is what the box says and what this measured.

## What was measured, and how

**1. The Lua engine is not in the image — read from the binary, not from a conf file.**
`grep -c -a -F` inside the running container, against
`/azerothcore/env/dist/bin/worldserver` (528 MB, 2026-09-04):

```
ALE.Enabled 0   ALE.ScriptPath 0   LuaEngine 0   lua_scripts 0
RegisterPlayerEvent 0   ALEConfig 0   Eluna 0     luaJIT_setmode 0
```

**A grep that answers 0 for everything proves nothing**, so the same pass counted strings the
binary must have: `playerbots` **15**, `AiPlayerbot.Enabled` **1**, `1v1` **10**, `beastmaster`
**5**. The reading works and the engine is absent. Every case-insensitive `lua` hit was checked by
eye and every one of them was the word *evaluate* (`EvaluateBezier3`, `evaluate_percent`, …).

That check is now `party.read_engine_in_binary()`, and the control string is part of it: a zero for
the control answers **"could not be read"**, never "absent".

**2. Our manifest named a configuration key the module does not have.**
Read from `azerothcore/mod-ale` itself on 2026-09-08:

| | our `mod-ale.json` said | the module actually has |
|---|---|---|
| enable key | `ALE.EnableLuaEngine` | `ALE.Enabled` (`conf/mod_ale.conf.dist:63`, `src/LuaEngine/ALEConfig.cpp:20`) |
| its default | *(no default — surfaced only)* | compiled default **`"false"`**, while the shipped conf's own comment says `true` |
| revision | *(none — branch tip)* | pinned to `319f43ed` |

A key that exists nowhere is neither written nor read, so the engine would have taken the compiled
default — which is **off**. Both halves are fixed in this commit, and both are now asserted by
`tests/test_party.py` against the manifest file itself.

**3. `azerothcore/mod-ale` moved twice in two days**, which is the argument for the pin:
`c3de7942` was HEAD on 2026-09-06 (`phase8-delta.md:18`), `319f43ed` was HEAD on 2026-09-07. An
unpinned manifest hands a two-hour rebuild whatever that morning's HEAD happens to be.

**4. The worldserver's own log line for the bridge command** (`4-console.txt`, screenshot 4).
SOAP writes no log line — `ACSoap` logs the received command at `LOG_DEBUG("network.soap", …)`,
which this install does not have on. So the same command was typed at the **attach console**, which
is the same `ChatHandler` with `IsConsole()` true and whose output lands in `docker logs`:

```
dml_bridge_ping
AC> Command 'dml_bridge_ping' does not exist
```

Searching the whole log for the engine instead: **4182 lines read, 56 of them naming `playerbots`
(so the log was read), and not one line naming the bridge, the Lua engine or the script
directory.** The control is there because the first version of that filter matched a bare `ALE`
inside `botActiveAloneSmartSCALE` and returned twenty-five lines of nothing.

## What the machine refuted

**1. The prior art's explanation of the 2026-08-20 failure is not the explanation here.**
`dml_addclass.lua`'s own header says the entry point is `playerbots bot addclass`, *"NOT the
intuitive `playerbots addclass`, which the chat framework rejects with USAGE"*, and the brief
offered that as *"very likely the whole reason"* the attempt answered "Command does not exist". It
is not. Asked over SOAP, `playerbots bot addclass mage` came back:

```
### USAGE: .playerbots ...
Possible subcommands:  |- playerbots debug ...  |- playerbots gtask
                       |- playerbots pmon       |- playerbots rndbot
```

`bot` is absent from that list — and it is **in** the command table:
`{"bot", HandlePlayerbotCommand, SEC_PLAYER, Console::No}`
(`modules/mod-playerbots/src/Script/PlayerbotCommandScript.cpp:36`). AzerothCore filters the table
by `IsInvokerVisible(handler)` before walking it, so a console or SOAP caller cannot *see* the
subcommand. **That corroborates the need for the bridge rather than explaining its failure**, and
it means a USAGE list missing `bot` must never be read as "the subcommand is gone". Recorded in the
script's header on the box that measured it.

**2. Deathknight is a class this tree's `addclass` accepts.** `PlayerbotMgr.cpp:1089` lists
`warrior/paladin/hunter/rogue/priest/shaman/mage/warlock/druid/**dk**`, gated on the master's level
reaching the heroic start level (`:1156`). The bash launcher's `_valid_bot_class`
(`rust-main:cli/src/50-party.sh`) deliberately excluded deathknight and said so twice. That was its
tree's fact; inheriting it here would have refused a class this server supports.

**3. Two of the app's own sentences were false in the state they were printed in** — found by
running the code against the real install rather than by reading it. All nine preconditions are
shown, not only the blocking one, and two of them asserted somebody else's fact:

* `engine_in_binary`: *"the Lua engine **is installed** but not built into the server"* — it was
  not installed; the precondition immediately above had just said so.
* `bridge_answered`: *"**The scripts are in place**, so the world server has not read them"* — they
  were not in place.

Neither is ever the blocker, so neither would have reached a user through `blocker()`. That is
exactly why it survived. Fixed, with a test that walks every sentence in the all-broken state.

## What the rebuild would settle, and the command that settles it

**The owner runs this. This lane did not, and may not.** The manifest's `build.rebuild` flag is not
acted on by the applier today, so there is nothing automatic to wait for either.

```
Modules page -> install "AzerothCore Lua Engine (ALE)" for wow-wotlk
then, on yulon-ubuntu:
    cd ~/wowserver && docker compose -f docker-compose.yml -f docker-compose.override.yml build worldserver
    docker compose -f docker-compose.yml -f docker-compose.override.yml up -d worldserver
```

After it, four things are answerable and none of them are today:

1. Does `ALE.ScriptPath` in the binary read non-zero — i.e. did the module actually compile in?
   `party.read_engine_in_binary()` answers this in one call.
2. Does `mod_ale.conf` arrive with `ALE.Enabled = 1` and the absolute script path, from the
   corrected manifest?
3. **Does `dml_bridge_ping` answer with `DML-BRIDGE-READY`?** That is the arrival proof the box
   asks for — the script answering, over the same wire the question went out on.
4. Only then: does `dml_addclass <player> mage` put a bot in the party frame?

If (1) or (3) fails, My Party goes back to the owner as the box allows.

## What is already in place, and is not a reason to doubt the route

Measured on this install, from the worldserver's own startup log:

```
Account type assignment complete: 50 RNDbot accounts, 50 AddClass accounts, 0 unassigned
>> 500 characters collected for addclass command from 50 AddClass accounts.
```

`AiPlayerbot.AddClassCommand = 1` and `AiPlayerbot.AddClassAccountPoolSize = 50`
(`PlayerbotAIConfig.cpp:645-646`, compiled defaults; the module conf is a `.dist` this install never
activates, so the defaults are what it runs on). So the server-side pool My Party draws from is
built and populated **on this box today**. The engine is the only missing piece.

## The screenshots

Every capture is the VM's own desktop, photographed from the Hyper-V host with `vmshot.ps1` —
GNOME refuses an in-guest screenshot driven from ssh. `show86.sh` puts one reading on that desktop;
the picture is of real bytes on a real machine, not a render of them. The worldserver's liveness was
read at each capture and is quoted under each row.

| File | What it shows | worldserver at that moment |
|---|---|---|
| `1-ground-no-engine-no-bridge.png` | the reading before anything was written: no engine in the binary, no conf, no scripts, and the server refusing the bridge command | `running pid=205047 restarts=0` |
| `2-deploy-five-scripts-changed-then-not.png` | `party.deploy()` copying all five scripts (`changed: True`), then a second press changing nothing (`changed: False`) | `running pid=205047 restarts=0` |
| `3-after-deploy-changed-nothing.png` | the same four questions after the deploy: `deployed` flipped to ok, **the server's answer did not change** | `running pid=205047 restarts=0` |
| `4-worldserver-own-log-line.png` | the bridge command typed at the console and its answer in `docker logs` — the worldserver's own words, in the worldserver's own file | `running pid=205047 restarts=0` |

## Where the plan was silent

`phase8-decisions.md` and `phase8-parity-decisions.md` say My Party is the mod-ale bridge over SOAP
and that its first question is whether it works. Neither says **who writes `mod_ale.conf`**. The
Rust launcher's `bridge_setup_stream` repaired that file itself (`ensure_ale_conf`, two enforced
keys); here the same file is owned by `mod-ale.json`. This lane put it in the manifest and left
`party.py` unable to write a conf at all — one writer, one idea of a working conf — but that is a
choice made in the silence and it is named here rather than left implicit.

## Left on the box

The five bridge scripts, deployed at `~/wowserver/env/dist/etc/modules/lua_scripts/`. They are inert
without the engine and they are what the rebuild will need, so they were left rather than removed.
Nothing else on that install was touched: no conf written, no module installed, no container
restarted, no compose file changed. A sibling lane (8.7a) was working on the same box; the
checkpoint this lane took would roll their work back too, so it should not be restored casually.
