# T18 live half — a chosen spec takes effect, on an install whose `playerbots.conf` the app deployed — yulon-ubuntu2, 2026-09-10

**Is the ticket's live half met? Yes.** The app deployed `env/dist/etc/modules/playerbots.conf`
through its own Modules-tab seam, the world was restarted through the app's own Stop and Start, the
world's own log says it read that file, `talents spec list` went from `Total 0 specs found` to
`Total 7 specs found` with the seven names the panel's picker now offers, and two chosen specs were
pressed on a bot and read back from the module with the tab counts matching its own list.
**This is the first spec ever seen taking effect on any tree in this project.**

**What it does NOT show is a person at a client.** No WoW client was driven: the box has one under
`~/clients/WoW-WotLK-3.3.5a-min` and no wine to run it, and the Hyper-V host T5 used is not this
lane's. Both "players" here are characters the playerbots module had online, and the panel frames
are offscreen widget grabs rather than photographs of a desktop. Section 8 says exactly what that
costs.

**Every sentence below names the file and line that shows it.** A claim with no `file:line` after it
is a claim this run did not make. Times are the box's own clock, read by `date +"%H:%M:%S %Z"`
inside the script that wrote the line (`lib.sh:31`) — no time in this folder was typed by hand
(memory note `stamps-come-from-the-clock`). Every `docker logs --since` window was opened with
`date -u +%Y-%m-%dT%H:%M:%SZ` (`lib.sh:32`) and queried BEFORE the action to prove it empty of the
pattern — the T3 trap of 2026-09-09 (`docker-logs-since-is-client-local`). Every box action was
announced in the box's own Claude activity terminal before it was taken (`lib.sh:34`,
`10-activity-terminal.txt`).

The box answers to `ssh yulon-ubuntu2`; its own `hostname` is `yulon-ubuntu`, which is what every
capture header prints (`01-ground.log:2`). One box, two names.

**The code this was pressed from** is `057d1957` — the tip of `hand-t18` while the run happened —
copied to the box as `~/t18-live` so every press is this ticket's tree and not the box clone's
older one (`02-harness.log:7`, `lua_root = /home/pk/t18-live/pylauncher/lua`). The branch was
fast-forwarded to `origin/yulon-phase8b` (`c0386f9f`) before this folder was committed; those nine
commits change no file under `pylauncher/`, so nothing measured here was measured against other
code.

## 0. What was pressed, in order

| # | at | what | capture |
|---|---|---|---|
| 01 | 21:56:41–21:56:53 | the box before anything was touched | `01-ground.log` |
| 02 | 22:00:29–22:02:42 | the six bridge scripts through `party.deploy`, this lane's harness, a restart through the app, the party staged, and **the BEFORE reading** | `02-harness.log` |
| 03 | 22:05:11–22:05:16 | **the activation** — the Modules tab's folder route (refused, quoted) and then the same seam with no copy | `03-activate.log` |
| 04 | 22:06:03–22:08:04 | **the restart through the app**, the world's own conf line, and **the AFTER reading** | `04-restart-and-list.log` |
| 05 | 22:09:21–22:09:38 | the shipped `PartyPanel`, offscreen, with the picker open | `05-picker.log`, four PNGs |
| 06 | 22:10:03–22:11:05 | **the spec press** and the talent rows around it | `06-spec-press.log`, `06-talents-{before,after}.txt` |
| 06b | 22:12:03–22:13:09 | **the readback**, the reply the module resets away, and a second chosen name | `06b-readback.log` |
| 07 | 22:13:48–22:15:41 | teardown: the bot's talents back to the module's chooser, the party disbanded, the harness off, a restart | `07-teardown.log` |
| 08 | 22:17:28–22:17:43 | the box read back against step 01's own numbers | `08-box-as-found.log` |
| 09 | 22:18:13–22:18:43 | this lane's directories off the box | `09-cleanup.log`, `09b-cleanup-out.txt` |

The scripts are in this folder and are what ran: `lib.sh`, `01-ground.sh`, `02-harness.sh`,
`03-activate.sh`, `04-restart-and-list.sh`, `05-picker.sh`, `06-spec-press.sh`, `06b-readback.sh`,
`07-teardown.sh`, `08-box-as-found.sh`, `09-cleanup.sh`, `09b-cleanup-out.sh`, the driver
`t18press.py`, and the staging harness `t18_stage.lua`.

**The bridge as found: five of the six.** `party.BRIDGE_SCRIPTS` names six and this box had five —
`dml_botadd.lua` was missing, and the panel's own precondition list said so:
*"some of the bridge scripts are missing from the server folder (dml_botadd.lua), so part of My
Party would answer and part would not"*, `ready: False` (`01-ground.log:96-98`, `:102-103`). Step 02
deployed all six through `party.deploy` and `ready` became `True` with all nine preconditions `ok`
(`02-harness.log:9`, `:78-79`).

## 1. Step 1 of the ticket — the activation route, and the module's spec keys

**The route.** `Applier._conf()` (`pylauncher/yulon/apply.py:2119-2145`) is the whole of it:

```python
target = self.server_dir / conf.file
if conf.template is not None and not target.exists():
    template = clone / conf.template
    ...
    shutil.copy2(template, target)
    log.done.append(f"activate {conf.file} from {conf.template}")
```

so the file it writes is `<server_dir>/<conf.file>` and the bytes come from `<clone>/<conf.template>`.
For a module this app ships no manifest for, both names are **discovered, not declared**:
`module_source._conf_steps()` (`pylauncher/yulon/module_source.py:253-278`) makes one `ConfFile` per
`conf/*.conf.dist` at the top level of the clone, with `file = env/dist/etc/modules/<name>.conf` and
`template = conf/<name>.conf.dist`, and `keys=()`.

Where the container reads it from is the bind mount, and the run proves that rather than reasoning
about it: the same md5 `bd8d55ae591adfed443f174ed1311c61` for
`~/wowserver/modules/mod-playerbots/conf/playerbots.conf.dist` and
`~/wowserver/env/dist/etc/modules/playerbots.conf` (`03-activate.log:50-51`) and — asked *inside* the
container — for `/azerothcore/env/dist/etc/modules/playerbots.conf` (`03-activate.log:64`).
`env/dist/etc` is volume-mounted to `/azerothcore/env/dist/etc`
(`manifests/wow-wotlk/modules/mod-ale.json`, notes), so no `docker cp` is involved and the file the
app wrote is the file `sConfigMgr` opens.

**The keys, quoted with file and line on the box:**

| | capture |
|---|---|
| `modules/mod-playerbots/conf/playerbots.conf.dist` defines **63** `AiPlayerbot.PremadeSpecName.*` keys | `01-ground.log:37` |
| warlock, T5's class: `2065:AiPlayerbot.PremadeSpecName.9.0 = affli pve` … `2087: … 9.5 = destro pvp` | `01-ground.log:39-44` |
| mage, this run's class, in the file the app later DEPLOYED: `2028: … 8.0 = arcane pve` … `2052: … 8.6 = frost pvp` | `03-activate.log:54-60` |
| the module's own words above them: `# AiPlayerbot.PremadeSpecName.<class>.<specno> = <name>  #Name of the talent specialisation`, `# 0 <= specno < 20, 1 <= level <= 80` | `01-ground.log:50`, `:54` |
| the module reads them at `src/PlayerbotAIConfig.cpp:493` — `os << "AiPlayerbot.PremadeSpecName." << cls << "." << spec;` | `01-ground.log:69` |
| and answers out of them at `src/Ai/Base/Actions/ChangeTalentsAction.cpp:130` — `out << "Total " << specFound << " specs found";` — and `:157` — `out << "Spec " << param << " not found";` | `01-ground.log:70-71` |
| the clone those line numbers are from: `HEAD b949b50bfcdd4fab937781bac2d7765e39330e4b`, nothing modified | `01-ground.log:30-31` |

Of the lines `party.py:854-892` cites, the lane re-read `PlayerbotAIConfig.cpp:493` and
`ChangeTalentsAction.cpp:130` and `:157` on the box (`01-ground.log:69-71`); the gap rule at
`:138-142` and the `==` compare at `:144` were re-read by the lead after the evidence review,
`13-changetalents-reread.txt`, and read as `party.py` says.

## 2. Step 2 — the activation, pressed through the app

`mod-playerbots` has **no shipped manifest** (`pylauncher/manifests/wow-wotlk/modules/` holds 21
files and none of them is it); it arrives with the catalog entry's own emulator sources
(`catalog.json:19-24`, `dest: modules/mod-playerbots`). So the Modules tab's shipped list has no row
for it, and the only route it offers is *"add a module from a folder"*. Both presses below go through
`wotlk_modules.install_custom(applier)` — the tab's ONE custom-install seam
(`controller_wow_wotlk/modules.py:153-174`) — over the applier `controller_view._for_wotlk` builds
(`controller_view.py:1094-1108`), with the T7 guard's `world_running` and `start_database` seams
attached (`t18press.py`, `modules()`).

**Press 1 — the tab's folder route, refused, and the refusal is the finding**
(`03-activate.log:20-24`):

```
derive_folder(/home/pk/wowserver/modules/mod-playerbots) -> id='mod-playerbots' type='module'
    source=None
    origin=Origin(kind='folder', path='/home/pk/wowserver/modules/mod-playerbots', added='2026-09-10')
REFUSED ApplyError: modules/mod-playerbots is already a git checkout and there is no record here
of one this app made. Continuing would run `git fetch` and `git reset --hard` over it, which throws
away anything you have changed there, so nothing was touched. Move that folder aside — a module
clone holds nothing but the module, so re-cloning it costs only the download — and then install
mod-playerbots again.
```

That is `Applier._require_own_clone()`'s case 5 (`apply.py:1621-1641`): the clone has a `.git` and
no `.yulon-clone.json` (`01-ground.log:31-33`), and a **folder-derived manifest has no `source`**, so
there is no repository for it to be a checkout OF and `_adoption_refusal` is never asked —
`NO_RECORD`, by `apply.py:1633-1637`'s own comment. The server directory IS claimed
(`.yulon-install.json`, `01-ground.log:34`), so adoption fact 2 was never the problem; fact 1 has
nothing to compare against. **A second refusal was waiting behind it and never ran:**
`module_source.copy_folder()` (`module_source.py:514-519`) refuses a source inside the destination's
own `modules/` directory, which this one is — the applier's ownership guard simply gets there first.

**Press 2 — the same seam, with no folder to copy** (`03-activate.log:26-39`):

```
install_custom(manifest, None)  ->  applier.install(manifest, None, folder=None, complete=complete)

INFO [yulon.apply] install mod-playerbots: 1 step(s), 0 skipped, 1 left unapplied, rebuild=True
report.action  = 'install'
report.item    = 'mod-playerbots'
    done    : activate env/dist/etc/modules/playerbots.conf from conf/playerbots.conf.dist
    pending : PendingSql(db='playerbots', path='data/sql/playerbots/**/*.sql', files=(65 files))
report.rebuild_required     = True
report.restart_recommended  = False
```

`install_custom`'s own signature is `(manifest, folder|None)` and `None` means "there is no folder to
copy in" (`controller_wow_wotlk/modules.py:170-172`); with a sourceless manifest the applier clones
nothing either (`apply.py:1174-1185`), so the pass is *complete → deploy → patches → sql → conf*, and
only the conf step had anything to do. **This is the app's own function, not a reimplementation of
it** — the same `Applier` object, the same completer, the same `_conf()`.

**The T7 guard did not refuse, and that is its own answer.** The world was UP throughout
(`03-activate.log:6`, `worldserver running started=2026-09-10T20:00:57`), and
`Applier._refuse_direct_sql_into_a_running_world()` (`apply.py:1915`, the `at_risk` filter at `:1982-1986`) refuses only steps with
`applied_by == "direct"` into a world-held database. A folder-derived manifest's SQL is
`applied_by="db-import"` (`module_source.py:282-290`), reported and not run — the `pending` line
above. So nothing needed the world stopped for the activation, and nothing was stopped for it.

**What the conf deployed:**

| | capture |
|---|---|
| path `/home/pk/wowserver/env/dist/etc/modules/playerbots.conf`, 119 723 bytes, where there had been none | `01-ground.log:122` (`exists = False`) vs `03-activate.log:47`, `:69` |
| byte-identical to the module's own template, and to what the container sees | `03-activate.log:50-51`, `:64` — md5 `bd8d55ae591adfed443f174ed1311c61` on all three |
| 63 `AiPlayerbot.PremadeSpecName` keys are in the deployed file; mage's seven at lines 2028–2052 | `03-activate.log:53-60` |
| no key was written into it: the derived manifest has `keys=()`, so `_conf()` copied and stopped | `03-activate.log:35` — one `done` line, and no `set N key(s) in …` line anywhere |

**And two things the app itself read differently at once, with no restart:**
`party.spec_names()` went from 0 class ids (`01-ground.log:124`) to 10 (`03-activate.log:71-81`), and
`dbreads.resolve_marker()` — `dbreads.py:147-151`, the branch whose whole docstring is about a
`.dist` with no `.conf` — went from `Marker(prefix='rndbot', source='default')`
(`01-ground.log:125`) to `Marker(prefix='rndbot', source='conf')` (`03-activate.log:82`). The app
stopped falling back on the module's compiled default and started reading the file.

## 3. Step 3 — the restart, and `talents spec list` before and after

**How a reply is heard at all, with nobody at a client.** `ChangeTalentsAction` answers through
`PlayerbotAI::TellMasterNoFacing` (`src/Bot/PlayerbotAI.cpp:3010-3040`), which builds a chat packet
and calls `master->SendDirectMessage` — a socket-less bot session drops it. Its other branch, for a
bot master with the `debug` non-combat strategy, calls `bot->Say(text, …)` instead; `Player::Say`
calls `sScriptMgr->OnPlayerCanUseChat` (AzerothCore
`src/server/game/Entities/Player/Player.cpp:9570`), ALE forwards that to `PLAYER_EVENT_ON_CHAT`
(`mod-ale/src/ALE_SC.cpp:654`, `mod-ale/src/LuaEngine/Hooks.h:183`), and `t18_stage.lua` prints it
into the worldserver log. **The replies quoted below are therefore the world's own log lines.** The
harness's header carries the whole argument, including why the master had to be made with a real
`Player:GroupInvite` rather than T13's `GroupCreate` (`AcceptInvitationAction.cpp:51` is the only
line in the module that sets a master without a real player, and `FindNewMaster` will not).

**The restart** (`04-restart-and-list.log:11-25`) was the app's own Stop and Start —
`docker.stop_staged` then `docker.start_staged` (`t18press.py`, verbs `stop`/`start`), which are what
the Server tab's two buttons call. `stop_staged -> True`, `start_staged -> True`, and the bridge
answered `DML-BRIDGE-READY` on the third try.

**The world's own log**, for the window opened at `2026-09-10T20:06:03Z` and proved to hold **0**
`Loaded playerbots config` lines before the restart (`04-restart-and-list.log:6-9`):

| when | what the world said | capture |
|---|---|---|
| before this ticket | `> Config::LoadFile: Failed open file '/azerothcore/env/dist/etc/modules/playerbots.conf'` — four times, once per start in the log | `01-ground.log:75-85` |
| after the deploy of step 02 and its restart, still | the same line | `02-harness.log:72` |
| after the activation and this restart | `> playerbots.conf` / `Load Playerbots Config...` / `Loading TalentSpecs...` / `>> Loaded playerbots config in 13699 ms` | `04-restart-and-list.log:27-30` |
| and the old line is gone from that window: **0** | | `04-restart-and-list.log:32` |

**`talents spec list`, before and after, in the module's own words:**

BEFORE — `02-harness.log:176-186`, window `2026-09-10T20:02:22Z`, proved to hold 0 `specs found`
lines first (`:179`):

```
[t18_chat] WHISP Jurnaar -> Lacosh: talents spec list
[t18_chat] SAY   Lacosh: Total 0 specs found
```

(`02-harness.log:184-185`.) That is T5's sentence exactly
(`8.6-spec-level-dismiss-…/client-4-spec-list-from-the-module.png`), this time out of the world's own
log instead of a photograph of a game window.

AFTER — `04-restart-and-list.log:111-129`, window `2026-09-10T20:07:51Z`, likewise proved empty
first (`:114`):

```
[t18_chat] WHISP Jurnaar -> Lacosh: talents spec list
[t18_chat] SAY   Lacosh: 1. arcane pve (56-3-12)
[t18_chat] SAY   Lacosh: 2. fire pve (18-53-0)
[t18_chat] SAY   Lacosh: 3. frost pve (18-0-53)
[t18_chat] SAY   Lacosh: 4. frostfire pve (0-57-14)
[t18_chat] SAY   Lacosh: 5. arcane pvp (52-7-12)
[t18_chat] SAY   Lacosh: 6. fire pvp (19-52-0)
[t18_chat] SAY   Lacosh: 7. frost pvp (20-0-51)
[t18_chat] SAY   Lacosh: Total 7 specs found
```

(`04-restart-and-list.log:120-128`.) `Lacosh` is a mage, and the app's own reading for a mage is
`('arcane pve', 'fire pve', 'frost pve', 'frostfire pve', 'arcane pvp', 'fire pvp', 'frost pvp')`
(`04-restart-and-list.log:70`). **Seven names, the same names, in the same order.** The app and the
server agree, which is the thing T5 could not test because there was nothing to agree about.

## 4. The picker

| | capture |
|---|---|
| mage: the shipped `QComboBox` offers **8** rows — `let the server pick` and the module's seven | `05-picker.log:44`, and the frame `05-picker-frame-mage-list.png` |
| warlock — T5's class: **7** rows — `let the server pick`, `affli pve`, `demo pve`, `destro pve`, `affli pvp`, `demo pvp`, `destro pvp` | `05-picker.log:59`, `05-picker-frame-warlock-list.png` |
| the whole panel around them, character `Jurnaar`, class `mage`/`warlock`, level cap 80 | `05-picker-frame-mage.png`, `05-picker-frame-warlock.png`, read-outs at `05-picker.log:41-48` and `:56-63` |

Compare `8.6-spec-level-dismiss-…/panel-7-corrected-picker-agrees-with-server.png`, which shows the
same control offering **one** row on the same box the day before.

**These are offscreen grabs** (`QT_QPA_PLATFORM=offscreen`), not photographs of a desktop, and the
driver says so at the call site (`t18press.py`, `_frame`). The widget, its seam and its job runner
are the shipped ones — `PartyPanel(seam, jobs=threaded_job_runner(window))`, which is
`_build_my_party_group`'s own line — so what the frames show is what the tab shows; only the surface
they are painted on differs. One thing the offscreen platform cannot do is paint a popup WINDOW (it
produced a 100-byte PNG, measured on the first attempt), which is why the list frame is a grab of the
popup's view rather than of its window, and the driver prints the view's size beside it
(`05-picker.log:48`).

**The panel reads its specs on a thread**, and the first attempt at these frames caught that: the
grab came back with an empty picker and the panel said why in its own log —
`dropped a stale spec reading for warrior: 6 names`. The driver now spins the real event loop for
four seconds after the class changes (`t18press.py`, `settle`), which is what a person does. The
kept frames are from the second attempt; the empty ones were not kept, and this paragraph is the
record that they existed.

## 5. The press, and the spec read back from the bot

The command string was built by `party.spec_command(master, bot, name)` — the app's own builder, the
one the panel's Add sends — and went over the seam's own channel (`t18press.py`, verb `spec`).
Nothing here typed a whisper by hand except the deliberate control in row H.

| # | what | what came back | capture |
|---|---|---|---|
| A | before anything, `talents` | `My current talent spec is: arcane (63/8/0) mage` | `06-spec-press.log:19` |
| B | `character_talent` for guid 188, after the server's own `.saveall` | **26** rows, `11080,12042,…,54646,54734` | `06-spec-press.log:28`, `:31`, `06-talents-before.txt` |
| C | **the press**: `COMMAND: dml_whisper Jurnaar Lacosh talents spec frost pve` | `outcome=yes`, and the module said nothing — section 6 | `06-spec-press.log:39-43` |
| D | `character_talent` after, `.saveall` again | **28** rows, an entirely different set; `DIFFERENT — the talent rows changed` | `06-spec-press.log:59`, `:62`, `:65-67`, `06-talents-after.txt` |
| E | the readback, `talents`, with the debug strategy re-armed | **`My current talent spec is: frost (18/0/53) mage`** | `06b-readback.log:49` |
| F | the module's own list gives `3. frost pve (18-0-53)` | the readback's three numbers ARE the chosen spec's | `04-restart-and-list.log:123` |
| G | a second chosen name, `fire pve`, through the same builder | `DIFFERENT — 'fire pve' moved the build again`, and `My current talent spec is: fire (18/53/0) mage` against the list's `2. fire pve (18-53-0)` | `06b-readback.log:56`, `:63`, `:78`, `04-restart-and-list.log:122` |
| H | the control: `talents spec warglaive pvp`, sent by hand | `IDENTICAL to 06-talents-after.txt — 'warglaive pvp' moved nothing`, same md5 `841788871ab0…` | `06-spec-press.log:69-77`, `06b-readback.log:30-32` |
| I | and what the APP would have done with that name, from the app's own function | *"'warglaive pvp' is not one of the premade specs this server has for mage: arcane pve, fire pve, frost pve, frostfire pve, arcane pvp, fire pvp, frost pvp. Nothing was sent."* — and `''` for `frost pve` | `06-spec-press.log:80-84` |

**A chosen spec takes effect.** Two of them, each landing on the tab distribution the module's own
list publishes for that name, and a name the picker does not offer landing on nothing at all.

The database rows were flushed with the server's own `.saveall` before each reading
(`06-spec-press.log:24-26`, `:55-57`), because T5 measured that an online character's row lags until
a save (`8.6-spec-level-dismiss-…/README.md`, section 2).

## 6. Step 4 of the ticket — the module still refuses one thing, and it is not a name

The deployed conf did NOT make the picker offer a name the module refuses: every name in the picker
is a name the module listed, and both that were pressed took. What the run found instead is
narrower, and it is a fact about this module worth carrying:

**The module's own confirmation of a chosen spec cannot be heard, even by a listener that hears its
other replies.** `ChangeTalentsAction::Execute` calls `botAI->ResetStrategies()` BETWEEN
`SpecPick(param)` and `botAI->TellMaster(out)` — `ChangeTalentsAction.cpp:70` (`out << SpecPick(param)`),
`:71` (`botAI->ResetStrategies()`), `:93` (`botAI->TellMaster(out)`), re-read on the box after the
evidence review in `13-changetalents-reread.txt` (the lane's own capture `06b-readback.log:6-21`
elided the `spec` branch at its line 17) — and `ResetStrategies` puts the non-combat strategies back to the conf's own
list, which is empty — `AiPlayerbot.RandomBotNonCombatStrategies = ""`, line 1371 of the deployed
file (`06b-readback.log:22`). So the one branch of `TellMasterNoFacing` a bot master can hear is
switched off by the action's own reply path, and `Picking frost pve` is dropped exactly as
`Spec <x> not found` was dropped for T5 — this time for a second, independent reason. `spec list` is
the one talents sub-command that does NOT reset (`06b-readback.log:9-12`), which is why the list came
through and the press did not.

This corroborates rather than contradicts `party.py`, whose `_spec_refusal` docstring already says
*"the server does not refuse it anywhere this app can hear"* (`party.py:1202-1210`). **No change to
`party.py` is owed by this run** — section 7.

## 7. What the press asked of the code: nothing

The ticket allows `pylauncher/yulon/party.py` to change *only* if the picker's reading of the
deployed conf needs a fix the press finds. It did not:

* `spec_names()` read the deployed file and only the deployed file, and produced ten class ids
  (`03-activate.log:71-81`) whose mage entry matches the module's own `SpecList()` name for name and
  in order (`04-restart-and-list.log:70` against `:121-127`).
* `read_spec_names`'s "a gap ends the class" rule was not exercised into a wrong answer: mage runs
  0–6 with no gap and the module reported all seven.
* `_spec_refusal` refused the unlisted name with the list in its sentence and let the listed one
  through (`06-spec-press.log:80-84`).
* `valid_spec`'s shape admitted every name in the conf: 7 of 7 offered for mage, 6 of 6 for warlock
  (`05-picker.log:44`, `:59`), so nothing was silently filtered out.

**So this commit carries no code change and no test change**, and the ticket's second file
(`pylauncher/tests/test_party.py`) is untouched. T16 keeps `add_bot`; this run never reached it.

## 8. What this does NOT show

* **A person at a client.** No `Wow.exe` was launched, no realm dialog crossed, no party frame
  photographed. Both "players" are random bots the module had online, and the master had to be made
  by a harness because the module will not give a bot master to a bot-led group (`t18_stage.lua`
  header, with the source lines).
* **The press through the panel's own button.** `PartyPanel`'s Add was never clicked: it builds a bot
  through `InstallParty.add`, which is T16's file this week. What was pressed is the command that
  press sends, through the app's own builder and the app's own channel.
* **`Picking <spec>` in the module's own words.** Section 6; the effect was measured instead.
* **The module's `Spec <x> not found`.** Same mechanism; the control's *absence* of an effect was
  measured instead (row H).
* **Eight of the ten classes' presses.** Two spec names on one mage. The other 61 names were read,
  offered, and never sent.
* **A rebuilt image.** `report.rebuild_required = True` (`03-activate.log:37`) was reported and
  deliberately not acted on: the module is already compiled into this image and a conf file needs no
  rebuild. The report says it because a folder-derived module manifest says it for every module.
* **The db-import SQL.** 65 files were reported as pending and none was run (`03-activate.log:36`).
* **A restart the app did not recommend.** The activation reported `restart_recommended = False`
  (`03-activate.log:38`), and this folder shows the world did not read the conf until the restart of
  step 04 (`03-activate.log:66`, "still the RUNNING world's old config"; `04-restart-and-list.log:27-32`).
  `apply.py:2207-2212` derives that flag from `build.restart`, `npcs`, direct SQL and `server_dbc`
  alone, so no conf activation on any route ever recommends a restart: a user pressing the Modules
  tab's Install is told nothing further is needed, and the spec still cannot take effect. An app
  defect this run measured and did not fix; filed as **T27** by the lead.
* **A second install of any kind.** Nothing was installed, cloned, or copied; one file was copied
  into place by `_conf()` and that is the whole of what the activation wrote.

## 9. What this lane left on the box, and why

| left | why | capture |
|---|---|---|
| `env/dist/etc/modules/playerbots.conf` | the app's own artefact — what the Modules tab's activation puts there, and the thing 8.6 needs. Removing it would put the box back to "a chosen spec cannot take effect" | `08-box-as-found.log:57`, `:69-70` (still md5 `bd8d55ae…`), `09b-cleanup-out.txt` |
| `env/dist/etc/modules/lua_scripts/dml_botadd.lua` | **T26's** script (merged in T26's unit half, `b86e23b0`), not this ticket's; T26's live half is not gated by this folder. The sixth bridge script, put there by `party.deploy` — the app's own seam — because without it the panel's own preconditions read `ready: False` | `01-ground.log:98`, `02-harness.log:9`, `08-box-as-found.log:59-67` |
| `~/.local/share/yulon/manifests/user/wow-wotlk/modules/mod-playerbots.json` | the app's own record of the activation, written by `wotlk_modules.complete()` (`modules.py:126-140`) — the same call the tab makes, in the same pass. **Named as a judgement call for the lead:** it puts a `mod-playerbots` row in the Modules tab's list, whose Install would repeat this activation harmlessly (the conf exists, so `_conf()` copies nothing) and whose Remove is refused by the same guard press 1 hit. `wotlk_modules.forget()` drops it in one line (`t18press.py`, verb `forget`) if the lead would rather it were not there; deleting it by hand would leave the conf on disk with no record of what put it there, which is why it was kept | `03-activate.log:99` |

Taken away again: `t18_stage.lua` (removed, and the world restarted so it is not loaded — the count
of `t18_stage` lines in the teardown restart's window is **0**, `07-teardown.log:78-87`), the staged
party (disbanded; `group_member` and `groups` both back to 0, `08-box-as-found.log:47`, `:49`), the
`debug` strategy (`nc -debug`, `07-teardown.log:28-30`, and a relog resets it anyway), `~/t18-live`
and `~/t18-out` (`09-cleanup.log`, `09b-cleanup-out.txt`).

**The bot's talents.** `Lacosh` (guid 188, a random bot) was left with a build chosen by
`talents autopick` — `PlayerbotFactory::InitTalentsTree(true)`, the module's own randomiser, the same
call that gave it the `arcane (63/8/0)` build step 06 read. **It is not the same build**, and nothing
could restore that one: a random bot's talents are the module's to choose and it re-rolls them
itself. What the teardown did is hand the choice back to the module, and this sentence is the whole
of the claim (`07-teardown.log:5-24`; the rows after autopick are in `07-talents-after-autopick.txt`
and are the same 25 the `fire pve` press left, which is the module's answer and not this lane's).

**No account was created and none was touched.** This lane used the app's own SOAP account for the
channel and two characters the module already had online. Account count 103 and the three
`account_access` rows are unchanged, with **0** orphaned rows (`08-box-as-found.log:23-32`).
`PERZI` was never used.

## 10. The owner's things

Read in step 01 and read back in step 08, and identical in both:

| thing | value | step 01 | step 08 |
|---|---|---|---|
| `PERZI.last_login` | `2026-09-08 23:24:16` | `01-ground.log:130` | `08-box-as-found.log:14` |
| `Pakka` | level 6, offline | `01-ground.log:133` | `08-box-as-found.log:17` |
| the realm row, all three columns | `100.99.204.5`, `100.99.204.5`, `255.255.255.0` | `01-ground.log:135` | `08-box-as-found.log:19` |
| `LootPet2.lua` | 37 294 bytes, 2026-09-09 02:06 | `01-ground.log:136` | `08-box-as-found.log:20`, `:66` |
| `Logger.ALE` | `706:Logger.ALE=4,Console Server` | `01-ground.log:137` | `08-box-as-found.log:21` |
| characters on the server | 1001, of which 500 online | `01-ground.log:148` (500 online) | `08-box-as-found.log:36`, `:38` |
| the module clone | `HEAD b949b50bfcdd4fab937781bac2d7765e39330e4b`, nothing modified | `01-ground.log:31-32` | `08-box-as-found.log:73-75` |

The realm row was never edited, so no authserver restart was owed or made. The world was restarted
three times, all three through the app's own Stop and Start, and it is up at the end with all three
containers healthy (`08-box-as-found.log:6-8`, `09b-cleanup-out.txt`).

## 11. Secrets

Nothing in this folder cats a conf file or dumps a container environment: steps 01 and 03 print
`AiPlayerbot.*` key lines and file listings only, and the worldserver container's environment — which
carries the database password in its `AC_*_DATABASE_INFO` values — is never printed by any script
here (memory note `gate-logs-carry-generated-passwords`). `tests/test_no_secrets_in_evidence.py` was
run against this worktree before the commit, and a grep over every file in this folder for the
password-shaped patterns found nothing: `11-no-secrets.txt`.

`--checks` (pytest + mypy ×3 + ruff + black) was run on m910q from this worktree's `pylauncher/`
before the commit and came back ALL GREEN (`12-checks-m910q.txt`, whose tail carries the mypy ×3, ruff and black lines and the verdict; the pytest count line was not captured, so no count is claimed here)
(`12-checks-m910q.txt`). No file under `pylauncher/` was changed by
this ticket, so it is a check that the branch is still green rather than a check of anything this
run wrote.

## 12. Deviations from the ticket, named

1. **The folder is dated 2026-09-10, not 09-09.** The ticket names
   `8.6-spec-takes-effect-yulon-ubuntu2-2026-09-09`; the run happened on the 10th and the folder
   carries the day it was measured, like every other folder in `pyplan/gates/`.
2. **No client seat was used.** The ticket's Hand line contemplates one. There is no wine on this
   box and the client under `~/clients` cannot run; the Hyper-V host T5 drove is another lane's.
   Everything the ticket asked for was captured without one, at the cost named in section 8.
3. **The Modules tab's route was pressed through its seams, not through its buttons.** The view's
   `module_from_folder` and `module_install_custom` were called with the arguments the view passes
   them, from a driver, because the tab's own window is not reachable on a headless box and the
   panel-in-a-window pattern (T5's `panel86t5.py`) drives one widget, not a whole tab. Named rather
   than smoothed over: what was pressed is the seam under the button, not the button.
4. **A staging harness and a chat-capture hook were loaded onto the server** and removed again
   (`t18_stage.lua`, section 3 and `07-teardown.log`). Neither is part of the app, neither answers
   in words `party.py` parses, and the world was restarted with the file gone.
5. **`nc +debug` was whispered to one bot** to make the module speak where a log could hear it, and
   `nc -debug` at the end. It changes one bot's strategy list, does not survive a relog, and is the
   only reason the replies in sections 3 and 5 exist at all.
6. **`.saveall` was sent three times**, to flush `character_talent` before each database reading.
7. **No account or character of the lane's own was made.** The definition of done says "own account
   and character only, erased after"; the lane used two of the module's existing characters and
   created nothing (`08-box-as-found.log:23-32`: 103 accounts, 0 orphaned `account_access` rows,
   PERZI unused), so there was nothing to erase. Section 9 says the same; it belongs here too.
   It is the server's own console command and saves every online character, which is what the
   world does periodically anyway.
