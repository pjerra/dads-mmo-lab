# CMaNGOS-family source reader — Phase 8 scoping

Read-only. No decisions, no recommendations. Every fact below was read at the SHA named in
its section. Per-fork facts are never carried between sections.

Trees fetched into
`C:\Users\perzi\AppData\Local\Temp\claude\C--Users-perzi-dads-mmo-lab\e6db1900-f1cd-4099-995e-e1ef6bec1004\scratchpad\cmangos-trees\`
(shallow, `git fetch --depth 1 origin <sha>`; all four fetches exited 0, no substitutions).

| tree | dir | fetched SHA |
|---|---|---|
| `cmangos/mangos-tbc` | `mangos-tbc/` | `f82e7d679c283b66bc2adc1b751aa1275e655673` |
| `cmangos/mangos-classic` | `mangos-classic/` | `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f` |
| `cmangos/playerbots` | `playerbots/` | `993f18091e67565986cf55c4d9b8e6eae11223f9` |
| `Shyalya/tortoise-wow` | `tortoise-wow/` | `7c0fb278f3f8966422f219e6f5035cb09b76ada7` |

DB repos (`cmangos/tbc-db`, `cmangos/classic-db`) skipped per brief. `ElunaLuaEngine/Eluna`
skipped per brief; its role in Tortoise is reported from the Tortoise tree's own files (§ Tortoise F).

---

## Step 1 — catalog pins and patched conf keys

Read verbatim from `/c/Users/perzi/dml-phase8/pylauncher/yulon/catalog/catalog.json`.

### wow-tbc

`emulator.name`: `"CMaNGOS mangos-tbc + cmangos/playerbots + tbc-db"`

`emulator.sources[]`:

| repo | dest | rev | branch/ref |
|---|---|---|---|
| `cmangos/mangos-tbc` | `src/mangos-tbc` | `f82e7d679c283b66bc2adc1b751aa1275e655673` | (none) |
| `cmangos/playerbots` | `src/mangos-tbc/src/modules/Bots` | `993f18091e67565986cf55c4d9b8e6eae11223f9` | (none) |
| `cmangos/tbc-db` | `src/tbc-db` | `5078439a44d208732a903bca2d7df51941fb373a` | (none) |

`ports`: `{"auth": 3724, "world": 8085, "db": 3306}`
`databases`: `{"auth": "realmd", "characters": "characters", "world": "mangos", "extra": ["logs"]}`
`containers`: `{"db": "tbc-db", "auth": "tbc-realmd", "world": "tbc-mangosd"}`
`console`: `{"prompt": "mangos>", "prompt_precedes_answer": false}`
No `soap` key, no `ra` key anywhere in the entry (or in the whole file — grep for
`soap|Ra\.Enable|RASocket|telnet` over `catalog.json` returns nothing).

`install.native.cmangos.conf` — `source_dir: "/opt/mangos/etc"`, files/keys patched at install:

- `mangosd.conf`: `LoginDatabaseInfo`, `WorldDatabaseInfo`, `CharacterDatabaseInfo`,
  `LogsDatabaseInfo`, `DataDir = "/opt/mangos/data"`, `WorldServerPort = {{WORLD_PORT}}`
- `realmd.conf`: `LoginDatabaseInfo`
- `aiplayerbot.conf`: `AiPlayerbot.MinRandomBots = 500`, `AiPlayerbot.MaxRandomBots = 500`,
  `AiPlayerbot.RandomBotAccountCount = 100`
- `ahbot.conf`: 11 auction-house tuning keys

**`SOAP.Enabled` and `Ra.Enable` are NOT among the patched keys.** They keep the tree's
`.conf.dist` defaults (both off — see § TBC A).

`install.native.cmangos.patches`: one entry,
`shared/cmangos/patches/vmap-extractor-doodad-name-case.patch` against `src/mangos-tbc`.

### wow-vanilla

`emulator.name`: `"CMaNGOS mangos-classic + cmangos/playerbots + classic-db"`

| repo | dest | rev | branch/ref |
|---|---|---|---|
| `cmangos/mangos-classic` | `src/mangos-classic` | `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f` | (none) |
| `cmangos/playerbots` | `src/mangos-classic/src/modules/Bots` | `993f18091e67565986cf55c4d9b8e6eae11223f9` | (none) |
| `cmangos/classic-db` | `src/classic-db` | `22b51464f1625f6ef6275771de1f5466c6f5d19e` | (none) |

`ports`, `databases`, `containers` (`vanilla-db` / `vanilla-realmd` / `vanilla-mangosd`),
`console` — identical shape to TBC; `world` port 8085.

`install.native.cmangos.conf` — same `mangosd.conf`/`realmd.conf`/`ahbot.conf` key sets as TBC,
plus `aiplayerbot.conf` with `"match_commented": true` and keys
`AiPlayerbot.MinRandomBots = 500`, `MaxRandomBots = 500`, `RandomBotAccountCount = 100`,
`SyncLevelWithPlayers = 1`, `SyncLevelMaxAbove = 5`, `SyncLevelNoPlayer = 1`.
**No `SOAP.*`, no `Ra.*`.**

### wow-tortoise

`emulator.name`: `"Shyalya/tortoise-wow (Turtle-WoW V2 + cmangos playerbots)"`

| repo | dest | rev | branch/ref |
|---|---|---|---|
| `Shyalya/tortoise-wow` | `src/tortoise-wow` | `7c0fb278f3f8966422f219e6f5035cb09b76ada7` | `branch: "playerbots-integration-gh"` |
| `ElunaLuaEngine/Eluna` | `src/tortoise-wow/src/modules/Eluna` | `1b06f28ff3a00054d915d824c725fb4283fee74d` | (none) |

`ports`: `{"auth": 3724, "world": 8090, "db": 3306}`
`databases`: `{"auth": "tw_logon", "characters": "tw_char", "world": "tw_world", "extra": ["tw_logs"]}`
`containers`: `{"db": "tortoise-db", "auth": "tortoise-realmd", "world": "tortoise-mangosd"}`
`console`: `{"prompt": "mangos>", "prompt_precedes_answer": false}`

`install.native.cmangos.conf` — `source_dir: "/opt/tortoise/etc"`:

- `aiplayerbot.conf`: `AiPlayerbot.MinRandomBots = 500`, `MaxRandomBots = 500`,
  `RandomBotAccountCount = 100`
- `mangosd.conf`: `LoginDatabase.Info`, `WorldDatabase.Info`, `CharacterDatabase.Info`,
  `LogsDatabase.Info` (note the dotted `.Info` spelling, unlike CMaNGOS),
  `Database.AutoUpdate.Path = "/opt/tortoise/sql/database_updates/"`,
  `DataDir = "/opt/tortoise/data"`, `WorldServerPort = {{WORLD_PORT}}`, `GameType = 0`,
  `GM.LoginState = 0`, `GM.StartLevel = 1`, `AutoHonorRestart = 0`,
  `AutoRestart.MaxServerUptime = 0`
- `realmd.conf`: `LoginDatabaseInfo`, `ForcePinAccountRank = 10`

**No `SOAP.*`, no `Ra.*`, no `Console.Enable`.** `patches` is `null`.

Cross-cutting: `-DBUILD_PLAYERBOTS` is passed for all three
(`pylauncher/catalog/installers/wow-tbc/native/Dockerfile.tmpl:84` `-DBUILD_PLAYERBOTS=1`,
`wow-vanilla/native/Dockerfile.tmpl:81` `-DBUILD_PLAYERBOTS=1`,
`wow-tortoise/native/Dockerfile.tmpl:106` `-DBUILD_PLAYERBOTS=ON`), so the playerbots command
tables below are compiled in on all three.

---

# TBC — `cmangos/mangos-tbc` @ `f82e7d679c283b66bc2adc1b751aa1275e655673`
(playerbots module from `cmangos/playerbots` @ `993f18091e67565986cf55c4d9b8e6eae11223f9`)

## A. Remote command channels

| question | answer | citation | verbatim |
|---|---|---|---|
| SOAP present in mangosd? | yes | `src/mangosd/MaNGOSsoap.cpp:75` | `int ns1__executeCommand(soap* soap, char* command, char** result)` |
| SOAP compiled unconditionally? | yes (no guard) | `src/mangosd/CMakeLists.txt:24`, `:58` | `    MaNGOSsoap.cpp` … `  gsoap` |
| gsoap dependency built? | yes | `dep/src/CMakeLists.txt:54` | `  add_subdirectory(gsoap)` |
| SOAP started by which key | `SOAP.Enabled` | `src/mangosd/Master.cpp:248` | `        if (sConfig.GetBoolDefault("SOAP.Enabled", false))` |
| SOAP bind/port defaults in code | `127.0.0.1` / `7878` | `src/mangosd/Master.cpp:249` | `            soapThread.reset(new SOAPThread(sConfig.GetStringDefault("SOAP.IP", "127.0.0.1"), sConfig.GetIntDefault("SOAP.Port", 7878)));` |
| SOAP `.conf.dist` defaults | off, 127.0.0.1, 7878 | `src/mangosd/mangosd.conf.dist.in:1858-1860` | `SOAP.Enabled = 0` / `SOAP.IP = 127.0.0.1` / `SOAP.Port = 7878` |
| SOAP auth: what is checked | HTTP basic user+pass against `account`, then security level | `src/mangosd/MaNGOSsoap.cpp:78`, `:84`, `:91`, `:97` | `    if (!soap->userid \|\| !soap->passwd)` / `    auto const accountId = sAccountMgr.GetId(soap->userid);` / `    if (!sAccountMgr.CheckPassword(accountId, soap->passwd))` / `    if (sAccountMgr.GetSecurity(accountId) < SOAPThread::MinLevel)` |
| SOAP minimum account security | `SEC_ADMINISTRATOR` (= 3) | `src/mangosd/MaNGOSsoap.h:49` | `        static const AccountTypes MinLevel = AccountTypes::SEC_ADMINISTRATOR;` |
| which table/column holds it | `realmd`.`account`.`gmlevel` | `src/game/Accounts/AccountMgr.cpp:229` | `    auto queryResult = LoginDatabase.PQuery("SELECT gmlevel FROM account WHERE id = '%u'", acc_id);` |
| access level the SOAP command runs at | `SEC_CONSOLE` (= 4), regardless of the caller's own gmlevel | `src/mangosd/MaNGOSsoap.cpp:113` | `    sWorld.QueueCliCommand(new CliCommandHolder(accountId, SEC_CONSOLE, command,` |
| executed on the world thread? | yes — queued, executed inside `World::Update` | `src/mangosd/MaNGOSsoap.cpp:112`, `src/game/World/World.cpp:1748`, `:2229`, `:2240` | `    // commands are executed in the world thread. We have to wait for them to be completed` / `    ProcessCliCommands();` / `void World::ProcessCliCommands()` / `        CliHandler handler(command->m_cliAccountId, command->m_cliAccessLevel, command->m_print);` |
| SOAP call blocks until done | yes, 50 ms poll on the SOAP thread | `src/mangosd/MaNGOSsoap.cpp:127-128` | `    while (!commandExecuted)` / `        std::this_thread::sleep_for(std::chrono::milliseconds(50));` |
| SOAP returns the command's output text? | yes; on failure the same text comes back as a SOAP fault | `src/mangosd/MaNGOSsoap.cpp:114-120`, `:135`, `:140` | `                           [&buffer](const char* output)` … `            buffer.push_back(*p);` / `        auto ret = soap_sender_fault(soap, printBuffer, printBuffer);` / `    *result = printBuffer;` |
| SOAP concurrency | one request at a time: accept → serve → destroy in a single loop | `src/mangosd/MaNGOSsoap.cpp:51-63` | `    while (!World::IsStopped())` … `        soap_serve(copy);` |
| RA / telnet present? | yes | `src/mangosd/RASocket.cpp:111` | `bool RASocket::HandleInput()` |
| RA started by which key | `Ra.Enable` | `src/mangosd/Master.cpp:240` | `        bool raEnable = sConfig.GetBoolDefault("Ra.Enable", false);` |
| RA `.conf.dist` defaults | off / `0.0.0.0` / 3443 / MinLevel 3 / Secure 1 / Restricted 1 | `src/mangosd/mangosd.conf.dist.in:1851-1856` | `Ra.Enable = 0` / `Ra.IP = 0.0.0.0` / `Ra.Port = 3443` / `Ra.MinLevel = 3` / `Ra.Secure = 1` / `Ra.Restricted = 1` |
| RA reads its keys as `RA.*`, conf writes `Ra.*` — does it match? | yes; the config reader lowercases both sides | `src/shared/Config/Config.cpp:73`, `:91` | `        auto const entry = boost::algorithm::to_lower_copy(trimmedEntry);` / `    auto const nameLower = boost::algorithm::to_lower_copy(name);` |
| RA auth | username → gmlevel ≥ minLevel → password | `src/mangosd/RASocket.cpp:113`, `:136`, `:139`, `:164` | `    auto const minLevel = static_cast<AccountTypes>(sConfig.GetIntDefault("RA.MinLevel", AccountTypes::SEC_ADMINISTRATOR));` / `            m_accountLevel = sAccountMgr.GetSecurity(m_accountId);` / `            if (m_accountLevel < minLevel)` / `            if (sAccountMgr.CheckPassword(m_accountId, m_input))` |
| RA gets console-only commands? | only when `RA.Restricted` is 0 | `src/mangosd/RASocket.cpp:153-154`, `:50` | `            if (m_accountLevel >= SEC_ADMINISTRATOR && !m_restricted)` / `                m_accountLevel = SEC_CONSOLE;` / `        m_restricted = sConfig.GetBoolDefault("RA.Restricted", true);` |
| RA returns output? | yes, streamed to the socket | `src/mangosd/RASocket.cpp:196-198` | `                sWorld.QueueCliCommand(new CliCommandHolder(m_accountId, m_accountLevel, m_input.c_str(),` / `                [this](const char* buffer) { this->Send(buffer); },` / `                [this](bool) { this->Send("mangos>"); }));` |

Consequence worth stating plainly (fact, not judgement): with `Ra.Restricted = 1` (the dist
default and the value Yu'lon leaves in place), an RA session caps at the account's own gmlevel,
so `SEC_CONSOLE` commands (`account create`, `account set gmlevel`, `character erase`,
`server exit`, `server log`) are unreachable over RA but reachable over SOAP, which hardcodes
`SEC_CONSOLE`.

## B. Console

| question | answer | citation | verbatim |
|---|---|---|---|
| stdin console exists | yes, `CliRunnable` | `src/mangosd/Master.cpp:164` | `        cliThread = new MaNGOS::Thread(new CliRunnable);` |
| config key + default | `Console.Enable`, default true; also `= 1` in the dist | `src/mangosd/Master.cpp:160`, `mangosd.conf.dist.in:1850` | `    if (sConfig.GetBoolDefault("Console.Enable", true))` / `Console.Enable = 1` |
| disabled in Windows service mode | yes | `src/mangosd/Master.cpp:158` | `    if (sConfig.GetBoolDefault("Console.Enable", true) && (m_ServiceStatus == -1)/* need disable console in service mode*/)` |
| console access level and account id | `SEC_CONSOLE`, account id `0` | `src/mangosd/CliRunnable.cpp:147` | `            sWorld.QueueCliCommand(new CliCommandHolder(0, SEC_CONSOLE, command.c_str(), &utf8print, &commandFinished));` |
| prompt | `mangos>` | `src/mangosd/CliRunnable.cpp:62` | `    printf("mangos>");` |
| input | line-based `fgets` on stdin, converted from console encoding to utf8 | `src/mangosd/CliRunnable.cpp:123`, `:141` | `        char* command_str = fgets(commandbuf, sizeof(commandbuf), stdin);` / `            if (!consoleToUtf8(command_str, command))       // convert from console encoding to utf8` |

## C. Commands Phase 8 needs

Gate for any non-session caller: `src/game/Chat/Chat.cpp:3550-3557`
```
bool CliHandler::isAvailable(ChatCommand const& cmd) const
{
    // skip non-console commands in console case
    if (!cmd.AllowConsole)
        return false;

    // normal case
    return GetAccessLevel() >= (AccountTypes)cmd.SecurityLevel;
}
```
Command struct field order is `Name, SecurityLevel, AllowConsole, Handler, Help, ChildCommands`
(`src/game/Chat/Chat.h:50-59`). Security levels are overridable at runtime from the **world DB**
`command` table: `src/game/Chat/Chat.cpp:1071` `        auto queryResult = WorldDatabase.Query("SELECT name,security,help FROM command");`.
`AccountTypes` (`src/shared/Common.h:124-128`): `SEC_PLAYER=0, SEC_MODERATOR=1, SEC_GAMEMASTER=2,
SEC_ADMINISTRATOR=3, SEC_CONSOLE=4`.

| command | exists | SEC_* | AllowConsole | handler | offline named target? | citation |
|---|---|---|---|---|---|---|
| `.tele name <player> <telename>` | yes | `SEC_MODERATOR` | **true** | `HandleTeleNameCommand` | **yes** — writes position straight to `characters` | table `src/game/Chat/Chat.cpp:829`; handler `src/game/Chat/Level1.cpp:1503`; offline branch `:1544-1552` |
| `.tele <telename>` (self) | yes | `SEC_MODERATOR` | false | `HandleTeleCommand` | n/a | `src/game/Chat/Chat.cpp:831` |
| `.tele group` | yes | `SEC_MODERATOR` | false | `HandleTeleGroupCommand` | no (uses selection) | `src/game/Chat/Chat.cpp:830` |
| `.go xyz` (and every `.go *`) | yes | `SEC_MODERATOR` (`next` = ADMIN) | **false** — every row | `HandleGo*Command` | no | `src/game/Chat/Chat.cpp:310-325` |
| `.send items <name> "subj" "body" id[:count] …` | yes | `SEC_ADMINISTRATOR` | **true** | `HandleSendItemsCommand` | **yes** | table `:734`; handler `src/game/Chat/Level3.cpp:6557`; `MailReceiver(receiver, receiver_guid)` `:6575` |
| max items per `.send items` | **12** | — | — | `MAX_MAIL_ITEMS` | — | `src/game/Mails/Mail.h:49` `#define MAX_MAIL_ITEMS 12`; enforced `src/game/Chat/Level3.cpp:6534` |
| `.send mail <name> "subj" "body"` | yes | `SEC_MODERATOR` | **true** | `HandleSendMailCommand` | yes (same extractor) | table `:735`; handler `src/game/Chat/Level1.cpp:1477` |
| `.send money` | yes | `SEC_ADMINISTRATOR` | true | `HandleSendMoneyCommand` | yes | `:737`; `src/game/Chat/Level3.cpp:6637` |
| `.additem` | yes | `SEC_ADMINISTRATOR` | **false** | `HandleAddItemCommand` | no | `src/game/Chat/Chat.cpp:1029` |
| `.revive [name]` | yes | `SEC_ADMINISTRATOR` | **true** | `HandleReviveCommand` | **yes** — `ConvertCorpseForPlayer` on the guid | `:1003`; `src/game/Chat/Level3.cpp:3407`, offline branch `:3419-3421` |
| `.levelup` | yes | `SEC_ADMINISTRATOR` | **false** | `HandleLevelUpCommand` | — | `:1026` |
| `.character level <name> <lvl>` | yes | `SEC_ADMINISTRATOR` | **true** | `HandleCharacterLevelCommand` | **yes** — `Player::GetLevelFromDB(target_guid)` | `:162`; `src/game/Chat/Level3.cpp:3934`, `:3961` |
| `.modify money` | yes | `SEC_MODERATOR` | **false** | `HandleModifyMoneyCommand` | no | `src/game/Chat/Chat.cpp:469` |
| `.character rename <name>` | yes | `SEC_GAMEMASTER` | **true** | `HandleCharacterRenameCommand` | **yes** — sets `at_login` bit in `characters` | `:163`; `src/game/Chat/Level2.cpp:3627`, offline branch `:3645-3654` |
| `.character erase` | yes | `SEC_CONSOLE` | true | `HandleCharacterEraseCommand` | yes | `:161` |
| `.account create <user> <pass> [expansion]` | yes | **`SEC_CONSOLE`** | true | `HandleAccountCreateCommand` | n/a | `:82`; handler `src/game/Chat/Chat.cpp:3754` |
| `.account set gmlevel <acct> <n>` | yes | **`SEC_CONSOLE`** | true | `HandleAccountSetGmLevelCommand` | n/a | `:74`; handler `src/game/Chat/Level3.cpp:1080` |
| — `n` range accepted | 0..3 only | — | — | — | — | `src/game/Chat/Level3.cpp:1098` `    if (gm < SEC_PLAYER \|\| gm > SEC_ADMINISTRATOR)` |
| — caller must outrank the new level | yes, strictly | — | — | — | — | `:1112` `    if (AccountTypes(gm) >= plSecurity)` (so SOAP at `SEC_CONSOLE` can grant up to 3) |
| — write target | `realmd.account.gmlevel` | — | — | — | — | `:1126` `    LoginDatabase.PExecute("UPDATE account SET gmlevel = '%i' WHERE id = '%u'", gm, targetAccountId);` |
| `.account set addon` | yes | `SEC_ADMINISTRATOR` | true | `HandleAccountSetAddonCommand` | n/a | `src/game/Chat/Chat.cpp:73` |
| `.account onlinelist` | yes | `SEC_CONSOLE` | true | `HandleAccountOnlineListCommand` | n/a | `:84`; query `src/game/Chat/Chat.cpp:3848` |
| `.server info` | yes | `SEC_PLAYER` | **true** | `HandleServerInfoCommand` | n/a | `:814`; handler `src/game/Chat/Level0.cpp:89` |
| — what it prints | version, world-DB version, EventAI version, connected/max/queued, uptime | — | — | — | — | `src/game/Chat/Level0.cpp:104-107` |
| `.pinfo <name>` | yes | `SEC_GAMEMASTER` | true | `HandlePInfoCommand` | yes | `src/game/Chat/Chat.cpp:1040` |

Named-target semantics (all the "offline?" answers above rest on this one function):
`src/game/Chat/Chat.cpp:3337-3388` `ChatHandler::ExtractPlayerTarget` — with a name argument it
resolves online via `sObjectMgr.GetPlayer(name)` and, failing that,
`sObjectMgr.GetPlayerGuidByName(name)` (`:3349`, `:3356`); with **no** argument it falls back to
`getSelectedPlayer()` (`:3367`), which from a console/SOAP context yields nothing and the command
fails with `LANG_PLAYER_NOT_FOUND` (`:3382`).

## D. Playerbots (`cmangos/playerbots` @ `993f180`, built as `src/modules/Bots`)

Registration is `#ifdef ENABLE_PLAYERBOTS` (`src/game/Chat/Chat.cpp:954-961`), defined by
`-DBUILD_PLAYERBOTS` (`src/game/CMakeLists.txt:160-162`).

| question | answer | citation | verbatim |
|---|---|---|---|
| player-facing bot command | `.bot <cmd> [name]`, `SEC_PLAYER`, **AllowConsole false** | `src/game/Chat/Chat.cpp:959` | `        { "bot",              SEC_PLAYER,        false, &ChatHandler::HandlePlayerbotCommand,          "", nullptr },` |
| console-facing bot command | `.rndbot <cmd> [name] [params]`, `SEC_GAMEMASTER`, **AllowConsole true** | `src/game/Chat/Chat.cpp:958` | `        { "rndbot",           SEC_GAMEMASTER,    true,  &ChatHandler::HandleRandomPlayerbotCommand,    "", nullptr },` |
| second `.bot` row | present but behind `BUILD_DEPRECATED_PLAYERBOT`, mutually exclusive with `BUILD_PLAYERBOTS` | `src/game/Chat/Chat.cpp:1058-1060`; `CMakeLists.txt:332-335` | `#ifdef BUILD_DEPRECATED_PLAYERBOT` … `    set(BUILD_DEPRECATED_PLAYERBOT OFF)` |
| does `.bot` need a live session? | **yes, explicitly** | `playerbots/playerbot/PlayerbotMgr.cpp:583-589` | `    WorldSession *m_session = handler->GetSession();` / `    if (!m_session)` / `        handler->PSendSysMessage("You may only add bots from an active session");` / `        return false;` |
| …so what does a console/SOAP `.bot add` return? | the command is filtered out before the handler runs (`AllowConsole == false` → `isAvailable` false), so the caller gets the "no such command" path, **not** the session message | `src/game/Chat/Chat.cpp:3553` | `    if (!cmd.AllowConsole)` |
| does `.rndbot` need a live session? | no | `playerbots/playerbot/RandomPlayerbotMgr.cpp:3519` | `    std::list<std::string> messages = sRandomPlayerbotMgr.HandlePlayerbotCommand(args, handler->GetSession() ? handler->GetSession()->GetPlayer():nullptr, static_cast<CliHandler*>(handler) ? static_cast<CliHandler*>(handler)->GetAccessLevel() : SEC_PLAYER);` |
| does `.rndbot` output reach the caller? | **only when the caller has a non-zero account id** (SOAP, RA, in-game). From the **stdin console** (account id 0) the output goes to the log only. | `playerbots/playerbot/RandomPlayerbotMgr.cpp:3396-3401`, `:3446-3451` | `    bool isRA = false;` / `    if (handler->GetSession()) //Client command` / `        isRA = true;` / `    else if (static_cast<CliHandler*>(handler) && static_cast<CliHandler*>(handler)->GetAccountId()) //RA call with account.` / `        isRA = true;` … `            sLog.outString("%s", msg.c_str());` / `            if(isRA)` / `                handler->SendSysMessage(msg.c_str());` |
| `.rndbot` sub-commands, holder level | `list, help, reload, tweak, self, spoof, p, g, r, rl, create, group` | `playerbots/playerbot/PlayerbotMgr.cpp:27-38` | `    m_holderHandlers["list"] = &PlayerbotHolder::HandleList;` … `    m_holderHandlers["group"] = &PlayerbotHolder::HandleGroup;` |
| `.rndbot` sub-commands, per-bot level | `add, login, remove, logout, rm, delete, gear, equip, train, learn, food, drink, potions, pots, consumes, regs, prepare, init, enchants, ammo, pet, levelup, level, random, always, debug, c, w, cmd, test, do, record, read, clear` | `playerbots/playerbot/PlayerbotMgr.cpp:43-82` | `    m_botCommandHandlers["add"] = &PlayerbotHolder::HandleBotAddLogin;` … |
| `.rndbot` console-only sub-commands | `help, reset, stats, update, pid, diff, clean map, login debug` (no-player) and `init, upgrade, refresh, teleport, rpg, revive, grind, change_strategy, remove` (per-bot sweep) | `playerbots/playerbot/RandomPlayerbotMgr.cpp:3424-3432`, `:3458-3466` | `    handlers["help"] = &RandomPlayerbotMgr::HandleHelp;` … `    playerHandlers["remove"] = &RandomPlayerbotMgr::HandleRemove;` |
| add-bot text and what it does without a master | `.rndbot add <charname>` → `AddRandomBot(guid)` when the bot's account is a random-bot account; otherwise `"Not in your account"` | `playerbots/playerbot/PlayerbotMgr.cpp:1824-1829`, `:793` | `    if (isRandomAccount)` / `        sRandomPlayerbotMgr.AddRandomBot(guid);` / `    else if (isMasterAccount \|\| sPlayerbotAIConfig.allowMultiAccountAltBots)` / `        AddPlayerBot(guid, masterAccountId);` / `    else` / `        return "Not in your account";` … `            out << ProcessBotCommand(command, member, ObjectGuid(), useSecurity >= SEC_GAMEMASTER, -1, -1, param);` |
| bot ownership gate | own-account bots, guild bots (if allowed), or admin | `playerbots/playerbot/PlayerbotMgr.cpp:538-547` | `    if (!isRandomAccount && (!isMasterAccount && !admin && masterguid))` / `        if (master && (!sPlayerbotAIConfig.allowGuildBots \|\| !masterGuildId \|\| (masterGuildId && master->GetGuildIdFromDB(guid) != masterGuildId)))` / `            return "Not in your guild or account";` / `    if (!isRandomAccount && this == &sRandomPlayerbotMgr && !admin)` / `        return "Can not control alt-bots with this command.";` |
| console can borrow a live player's identity | **yes** — `.rndbot spoof <PlayerName>` sets a sticky master for later master-less calls | `playerbots/playerbot/PlayerbotMgr.cpp:2826`, `:628-629` | `    m_spoofGuid = guid;` / `    if (!master && m_spoofGuid)` / `        master = sObjectMgr.GetPlayer(m_spoofGuid);` |
| spoof requires the target to be online | yes | `playerbots/playerbot/PlayerbotMgr.cpp:2817-2822` | `    Player* player = sObjectMgr.GetPlayer(guid, false);` / `    if (!player)` / `        messages.push_back("Player '" + param + "' found but is not online.");` |
| console can speak into a party as a player | **yes** — `.rndbot p <PlayerName> <message>` queues a `CMSG_MESSAGECHAT`/`CHAT_MSG_PARTY` packet on that player's session | `playerbots/playerbot/PlayerbotMgr.cpp:1623-1626`, `:1666-1673` | `    if (!master)` / `        botName = param.substr(0, param.find(" "));` / `        master = sObjectAccessor.FindPlayerByName(botName.c_str());` … `    WorldPacket packet_template(CMSG_MESSAGECHAT);` / `    packet_template << CHAT_MSG_PARTY;` / `    master->GetSession()->QueuePacket(std::move(packetPtr));` |
| — no group → | `"Sender is not in a group"` | `playerbots/playerbot/PlayerbotMgr.cpp:1637-1638` | `    if (!master->GetGroup())` / `        return {"Sender is not in a group"};` |
| — bare `.rndbot p <name>` returns the roster | yes | `playerbots/playerbot/PlayerbotMgr.cpp:1663` | `        return {"Party with " + leaderName + " as leader" + (otherMembers.empty() ? "" : " and " + otherMembers)};` |
| console can push a chat command into one bot | **yes** — `.rndbot cmd <botname> <bot command>` | `playerbots/playerbot/PlayerbotMgr.cpp:1462-1480` | `std::string PlayerbotHolder::HandleConsoleCmd(Player* bot, Player* master, const std::string param)` / `    if (!helper.ParseChatCommand(param, master ? master : bot))` / `        return "command failed";` |
| console can whisper as a bot | yes — `.rndbot w <botname> <message>` | `playerbots/playerbot/PlayerbotMgr.cpp:1446-1455` | `    packet_template << CHAT_MSG_WHISPER;` … `    sender->GetSession()->QueuePacket(std::move(packetPtr));` |
| bot-count query | `.rndbot list [prefix]` — one string of currently-loaded bots (name/class/online marker) | `playerbots/playerbot/PlayerbotMgr.cpp:1162-1167`, `:819-856` | `    messages.push_back(ListBots(master, param));` / `    for (auto& itr : playerBots)` … `        online[name] = "+";` |
| `.rndbot stats` from console/SOAP | **fails** — `param` is empty for a session-less caller | `playerbots/playerbot/RandomPlayerbotMgr.cpp:3442-3443`, `:4466-4469`; `playerbot/strategy/NamedObjectContext.h:104` | `        if (prefix == "stats")` / `            param = handler->GetSession() ? std::to_string(handler->GetSession()->GetPlayer()->GetObjectGuid()) : "";` … `    if (!Qualified::isValidNumberString(param))` / `        return {"Stats: Error parsing " + param};` / `            bool valid = !str.empty();` |
| extra TCP channel | **yes** — a raw line-oriented "Playerbot Command Server", no authentication | `playerbots/playerbot/PlayerbotCommandServer.cpp:42-57`, `:59-68` | `            std::string response = sRandomPlayerbotMgr.HandleRemoteCommand(request) + "\n";` / `    tcp::acceptor a(io_context, tcp::endpoint(tcp::v4(), port));` |
| — its wire format | `<command>,<bot guid>`; only reaches one loaded bot's AI | `playerbots/playerbot/RandomPlayerbotMgr.cpp:3986-4005` | `    std::string::iterator pos = find(request.begin(), request.end(), ',');` / `        std::ostringstream out; out << "invalid request: " << request;` / `    uint32 guid = std::atoi(std::string(pos + 1, request.end()).c_str());` / `    if (!bot) return "invalid guid";` |
| — it does **not** go through the world queue | correct — it runs on its own boost thread | `playerbots/playerbot/PlayerbotCommandServer.cpp:66`, `:90-93` | `        boost::thread t(boost::bind(session, sock));` / `    std::thread serverThread(Run);` / `    serverThread.detach();` |
| — port key and default | `AiPlayerbot.CommandServerPort`, default **0 = disabled**, commented out in the dist | `playerbots/playerbot/PlayerbotAIConfig.cpp:277`; `playerbot/aiplayerbot.conf.dist.in:972-973` | `    commandServerPort = config.GetIntDefault("AiPlayerbot.CommandServerPort", 0);` / `# Command server port, 0 - disabled` / `#AiPlayerbot.CommandServerPort = 8888` |
| — started from | `RandomPlayerbotMgr` | `playerbots/playerbot/RandomPlayerbotMgr.cpp:269` | `        sPlayerbotCommandServer.Start();` |

Bot-account config keys (dist defaults; Yu'lon's overrides are in Step 1):

| key | dist value | code default | citation |
|---|---|---|---|
| `AiPlayerbot.Enabled` | `1` | — | `playerbot/aiplayerbot.conf.dist.in:9` |
| `AiPlayerbot.RandomBotAutologin` | `1` | — | `:12` |
| `AiPlayerbot.RandomBotLoginAtStartup` | `1` | — | `:15` |
| `AiPlayerbot.RandomBotAutoCreate` | `1` | — | `:48` |
| `AiPlayerbot.BotAutologin` | `0` | — | `:33` |
| `AiPlayerbot.MinRandomBots` / `MaxRandomBots` | `1000` / `1000` | — | `:51-52` |
| `AiPlayerbot.RandomBotAccountPrefix` | `RNDBOT` | `"rndbot"` | `:57`; `PlayerbotAIConfig.cpp:500` |
| `AiPlayerbot.RandomBotAccountCount` | `200` | `50` | `:58`; `PlayerbotAIConfig.cpp:501` |
| `AiPlayerbot.AllowGuildBots` | `1` | — | `:784` |
| `AiPlayerbot.AllowMultiAccountAltBots` | `1` | — | `:787` |

Random-bot account naming: `<prefix><n>`, created through the normal account manager, with the
account id pushed into an in-memory list, not a marker column:
`playerbots/playerbot/RandomPlayerbotMgr.cpp:4597-4599`
`        accountNameStr << sPlayerbotAIConfig.randomBotAccountPrefix << accountNumber;`;
`:4617` `            AccountOpResult result = sAccountMgr.CreateAccount(accountName, password, max_expansion);`;
`:4628` `                    sPlayerbotAIConfig.randomBotAccounts.push_back(accountId);`;
membership test `playerbot/PlayerbotAIConfig.cpp:833-835`
`    return find(randomBotAccounts.begin(), randomBotAccounts.end(), id) != randomBotAccounts.end();`.
The reverse query the module itself uses to find *non*-bot accounts is
`playerbot/PlayerbotAIConfig.cpp:939`
`    auto results = LoginDatabase.PQuery("SELECT username, id FROM account where username not like '%s%%'", randomBotAccountPrefix.c_str());`.

## E. Database schema

Yu'lon DB names: auth `realmd`, characters `characters`, world `mangos`, plus `logs`.

| object | DB | citation | verbatim |
|---|---|---|---|
| `item_template` | `mangos` (world) | `sql/base/mangos.sql:2962` | `CREATE TABLE \`item_template\` (` |
| — searchable name column | `mangos` | `sql/base/mangos.sql:2967` | `  \`name\` varchar(255) NOT NULL DEFAULT '',` |
| `game_tele` (named teleport targets for `.tele name`) | `mangos` | `sql/base/mangos.sql:1983`, `:1990` | `CREATE TABLE \`game_tele\` (` / `  \`name\` varchar(100) NOT NULL DEFAULT '',` |
| `characters` | `characters` | `sql/base/characters.sql:791` | `CREATE TABLE \`characters\` (` |
| — `online` column | `characters` | `sql/base/characters.sql:811`, index `:859` | `  \`online\` tinyint(3) unsigned NOT NULL DEFAULT '0',` / `  KEY \`idx_online\` (\`online\`),` |
| — account link / name / level | `characters` | `sql/base/characters.sql:793`, `:794`, `:798` | `  \`account\` int(11) unsigned NOT NULL DEFAULT '0' COMMENT 'Account Identifier',` / `  \`name\` varchar(12) NOT NULL DEFAULT '',` / `  \`level\` tinyint(3) unsigned NOT NULL DEFAULT '0',` |
| `account` | `realmd` | `sql/base/realmd.sql:43` | `CREATE TABLE \`account\` (` |
| — GM level column (there is **no** `account_access` table) | `realmd` | `sql/base/realmd.sql:46`, index `:67` | `  \`gmlevel\` tinyint(3) unsigned NOT NULL DEFAULT '0',` / `  KEY \`idx_gmlevel\` (\`gmlevel\`)` |
| — username (the bot-account marker is a `username` prefix) | `realmd` | `sql/base/realmd.sql:45` | `  \`username\` varchar(32) NOT NULL DEFAULT '',` |
| `realmlist` | `realmd` | `sql/base/realmd.sql:177` | `CREATE TABLE \`realmlist\` (` |
| — seeded row | `realmd` | `sql/base/realmd.sql:199` | `(1,'MaNGOS','127.0.0.1',8085,1,0,1,0,0,'');` |
| `ai_playerbot_random_bots` (bot event/state store) | `characters` | `playerbots/sql/characters/ai_playerbot_random_bots.sql:2-15` | `CREATE TABLE \`ai_playerbot_random_bots\` (` / `  \`owner\` bigint(20) NOT NULL,` / `  \`bot\` bigint(20) NOT NULL,` / `  \`event\` varchar(45) DEFAULT NULL,` |

Note on `ai_playerbot_random_bots`: it is an event/value store keyed by `owner`/`bot`/`event`, not
a roster of bot accounts. There is no bot-account marker column anywhere; the only durable marker
is the `account.username` prefix.

## F. Eluna / Lua / addon channel

| question | answer | citation |
|---|---|---|
| Eluna in the tree? | **no** — grep for `eluna\|ELUNA\|BUILD_ELUNA` over `*.txt`/`*.cmake`/`*.md` outside `dep/` returns nothing | (empty result at this SHA) |
| addon chat channel exists? | yes | `src/game/Globals/SharedDefines.h:1569` `    CHAT_MSG_ADDON                  = 0xFFFFFFFF,` |
| gated by config | `AddonChannel`, default true, dist `= 1` | `src/game/World/World.cpp:486` `    setConfig(CONFIG_BOOL_ADDON_CHANNEL, "AddonChannel", true);`; `src/mangosd/mangosd.conf.dist.in:286` `AddonChannel = 1` |
| addon messages skip the chat sanitiser | yes | `src/game/Chat/ChatHandler.cpp:57-58` `    // skip remaining checks for addon messages or higher sec level accounts` / `    if (addon \|\| GetSecurity() > SEC_PLAYER)` |
| server-side hook to *send* an addon message to a client | not read; no `SavedVariables` reference exists in the tree | — |

---

# Vanilla — `cmangos/mangos-classic` @ `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`
(same playerbots module SHA as TBC)

## A. Remote command channels

| question | answer | citation | verbatim |
|---|---|---|---|
| SOAP present | yes | `src/mangosd/MaNGOSsoap.cpp:75` | `int ns1__executeCommand(soap* soap, char* command, char** result)` |
| compiled unconditionally | yes | `src/mangosd/CMakeLists.txt:24` | `    MaNGOSsoap.cpp` |
| SOAP defaults in `.conf.dist` | off / 127.0.0.1 / 7878 | `src/mangosd/mangosd.conf.dist.in:1724-1726` | `SOAP.Enabled = 0` / `SOAP.IP = 127.0.0.1` / `SOAP.Port = 7878` |
| SOAP start + bind defaults in code | same keys | `src/mangosd/Master.cpp:248-249` | `        if (sConfig.GetBoolDefault("SOAP.Enabled", false))` / `            soapThread.reset(new SOAPThread(sConfig.GetStringDefault("SOAP.IP", "127.0.0.1"), sConfig.GetIntDefault("SOAP.Port", 7878)));` |
| SOAP auth | basic user+pass, then gmlevel | `src/mangosd/MaNGOSsoap.cpp:78`, `:84`, `:91`, `:97` | `    if (!soap->userid \|\| !soap->passwd)` / `    auto const accountId = sAccountMgr.GetId(soap->userid);` / `    if (!sAccountMgr.CheckPassword(accountId, soap->passwd))` / `    if (sAccountMgr.GetSecurity(accountId) < SOAPThread::MinLevel)` |
| SOAP minimum level | `SEC_ADMINISTRATOR` (= 3) | `src/mangosd/MaNGOSsoap.h:49` | `        static const AccountTypes MinLevel = AccountTypes::SEC_ADMINISTRATOR;` |
| level column | `realmd.account.gmlevel` | `src/game/Accounts/AccountMgr.cpp:197` | `    auto queryResult = LoginDatabase.PQuery("SELECT gmlevel FROM account WHERE id = '%u'", acc_id);` |
| access level of the executed command | `SEC_CONSOLE` | `src/mangosd/MaNGOSsoap.cpp:113` | `    sWorld.QueueCliCommand(new CliCommandHolder(accountId, SEC_CONSOLE, command,` |
| world thread? | yes, via the CLI queue | `src/game/World/World.cpp` (`ProcessCliCommands` called from `World::Update`); handler construction `src/mangosd/MaNGOSsoap.cpp:113` | — |
| returns output? | yes; faults carry the same text | `src/mangosd/MaNGOSsoap.cpp:135`, `:140` | `        auto ret = soap_sender_fault(soap, printBuffer, printBuffer);` / `    *result = printBuffer;` |
| RA present | yes | `src/mangosd/RASocket.cpp:113` | `    auto const minLevel = static_cast<AccountTypes>(sConfig.GetIntDefault("RA.MinLevel", AccountTypes::SEC_ADMINISTRATOR));` |
| RA defaults | off / `0.0.0.0` / 3443 / MinLevel 3 / Secure 1 / Restricted 1 | `src/mangosd/mangosd.conf.dist.in:1717-1722` | `Ra.Enable = 0` / `Ra.IP = 0.0.0.0` / `Ra.Port = 3443` / `Ra.MinLevel = 3` / `Ra.Secure = 1` / `Ra.Restricted = 1` |
| RA start key | `Ra.Enable` | `src/mangosd/Master.cpp:240` | `        bool raEnable = sConfig.GetBoolDefault("Ra.Enable", false);` |
| RA elevation to console | only when not restricted | `src/mangosd/RASocket.cpp:154`, `:50` | `                m_accountLevel = SEC_CONSOLE;` / `        m_restricted = sConfig.GetBoolDefault("RA.Restricted", true);` |
| RA queues to the world thread | yes | `src/mangosd/RASocket.cpp:196` | `                sWorld.QueueCliCommand(new CliCommandHolder(m_accountId, m_accountLevel, m_input.c_str(),` |

## B. Console

| question | answer | citation | verbatim |
|---|---|---|---|
| stdin console | yes | `src/mangosd/Master.cpp:160` | `    if (sConfig.GetBoolDefault("Console.Enable", true))` |
| default | on (`Console.Enable = 1`) | `src/mangosd/mangosd.conf.dist.in:1716` | `Console.Enable = 1` |
| level / account id | `SEC_CONSOLE` / `0` | `src/mangosd/CliRunnable.cpp:147` | `            sWorld.QueueCliCommand(new CliCommandHolder(0, SEC_CONSOLE, command.c_str(), &utf8print, &commandFinished));` |
| prompt | `mangos>` | `src/mangosd/CliRunnable.cpp:62` | `    printf("mangos>");` |

## C. Commands Phase 8 needs

Same gate: `src/game/Chat/Chat.cpp:3486-3494` (`CliHandler::isAvailable`, identical text to TBC).
Same `AccountTypes` numbering (`src/shared/Common.h:124-128`).
Same runtime override from the world DB `command` table: `src/game/Chat/Chat.cpp:1028`
`        auto queryResult = WorldDatabase.Query("SELECT name,security,help FROM command");`.

| command | exists | SEC_* | AllowConsole | handler | offline named target? | citation |
|---|---|---|---|---|---|---|
| `.tele name` | yes | `SEC_MODERATOR` | **true** | `HandleTeleNameCommand` | **yes** — `Player::SavePositionInDB` | table `src/game/Chat/Chat.cpp:810`; handler `src/game/Chat/Level1.cpp:1438`, offline branch `:1478-1487` |
| `.tele` (self) / `.tele group` | yes | `SEC_MODERATOR` | false | — | — | `:812`, `:811` |
| `.go *` | yes | `SEC_MODERATOR` (`next` ADMIN) | **false**, all rows | — | no | `src/game/Chat/Chat.cpp:306-321` |
| `.send items` | yes | `SEC_ADMINISTRATOR` | **true** | `HandleSendItemsCommand` | **yes** | `:724`; handler `src/game/Chat/Level3.cpp:6416`, `:6434` |
| **max items per `.send items`** | **1** — this is the sharpest TBC/Vanilla divergence | — | — | — | — | `src/game/Mails/Mail.h:49` `#define MAX_MAIL_ITEMS 1`; enforced `src/game/Chat/Level3.cpp:6393` |
| `.send mail` | yes | `SEC_MODERATOR` | true | `HandleSendMailCommand` | yes | `:725` |
| `.send money` | yes | `SEC_ADMINISTRATOR` | true | `HandleSendMoneyCommand` | yes | `:727` |
| `.additem` | yes | `SEC_ADMINISTRATOR` | **false** | `HandleAddItemCommand` | no | `:988`; handler `src/game/Chat/Level3.cpp:1602` |
| `.revive [name]` | yes | `SEC_ADMINISTRATOR` | **true** | `HandleReviveCommand` | **yes** | `:962`; handler `src/game/Chat/Level3.cpp:3281`, offline branch `:3293-3295` |
| `.levelup` | yes | `SEC_ADMINISTRATOR` | **false** | `HandleLevelUpCommand` | — | `:985`; handler `src/game/Chat/Level3.cpp:3843` |
| `.character level` | yes | `SEC_ADMINISTRATOR` | **true** | `HandleCharacterLevelCommand` | yes | `:162`; handler `src/game/Chat/Level3.cpp:3795` |
| `.modify money` | yes | `SEC_MODERATOR` | **false** | `HandleModifyMoneyCommand` | no | `src/game/Chat/Chat.cpp:465` |
| `.character rename` | yes | `SEC_GAMEMASTER` | **true** | `HandleCharacterRenameCommand` | yes | `:163`; handler `src/game/Chat/Level2.cpp:3609` |
| `.character titles` | **absent** (TBC has it) | — | — | — | — | `src/game/Chat/Chat.cpp:158-166` (five rows only) |
| `.character erase` | yes | `SEC_CONSOLE` | true | — | — | `:161` |
| `.account create` | yes | **`SEC_CONSOLE`** | true | `HandleAccountCreateCommand` | n/a | `:82`; handler `src/game/Chat/Chat.cpp:3672` |
| `.account set gmlevel` | yes | **`SEC_CONSOLE`** | true | `HandleAccountSetGmLevelCommand` | n/a | `:74`; handler `src/game/Chat/Level3.cpp:1041`; write `:1087` `    LoginDatabase.PExecute("UPDATE account SET gmlevel = '%i' WHERE id = '%u'", gm, targetAccountId);` |
| `.account set addon` | yes | `SEC_ADMINISTRATOR` | true | `HandleAccountSetAddonCommand` | n/a | `:73` |
| `.account onlinelist` | yes | `SEC_CONSOLE` | true | — | n/a | `:84` |
| `.server info` | yes | `SEC_PLAYER` | **true** | `HandleServerInfoCommand` | n/a | `:795`; handler `src/game/Chat/Level0.cpp:88` |
| `.pinfo` | yes | `SEC_GAMEMASTER` | true | `HandlePInfoCommand` | yes | `:999` |

## D. Playerbots

Same module SHA, so every `cmangos/playerbots` citation in § TBC D applies unchanged. The
*host-side* registration differs only in the placement of the rows:

| question | answer | citation | verbatim |
|---|---|---|---|
| `.rndbot` | `SEC_GAMEMASTER`, AllowConsole **true** | `src/game/Chat/Chat.cpp:917` | `        { "rndbot",           SEC_GAMEMASTER,    true,  &ChatHandler::HandleRandomPlayerbotCommand,     "", NULL },` |
| `.bot` | `SEC_PLAYER`, AllowConsole **false** | `src/game/Chat/Chat.cpp:918` | `        { "bot",              SEC_PLAYER,        false, &ChatHandler::HandlePlayerbotCommand,               "", NULL },` |
| guard | `#ifdef ENABLE_PLAYERBOTS` | `src/game/Chat/Chat.cpp:913-920`; `src/game/CMakeLists.txt:161-162` | `if (BUILD_PLAYERBOTS)` / `  add_definitions(-DENABLE_PLAYERBOTS)` |

Yu'lon's Vanilla `aiplayerbot.conf` patch carries `"match_commented": true` and three extra
level-sync keys that TBC's does not (Step 1) — a per-game install difference, not a tree difference.

## E. Database schema

| object | DB | citation | verbatim |
|---|---|---|---|
| `item_template` | `mangos` | `sql/base/mangos.sql:2658` | `CREATE TABLE \`item_template\` (` |
| `game_tele` | `mangos` | `sql/base/mangos.sql:1875` | `CREATE TABLE \`game_tele\` (` |
| `characters` | `characters` | `sql/base/characters.sql:640` | `CREATE TABLE \`characters\` (` |
| — `online` | `characters` | `sql/base/characters.sql:659` | `  \`online\` tinyint(3) unsigned NOT NULL DEFAULT '0',` |
| `account` | `realmd` | `sql/base/realmd.sql:43` | `CREATE TABLE \`account\` (` |
| — `gmlevel` | `realmd` | `sql/base/realmd.sql:46` | `  \`gmlevel\` tinyint(3) unsigned NOT NULL DEFAULT '0',` |
| `realmlist` | `realmd` | `sql/base/realmd.sql:177` | `CREATE TABLE \`realmlist\` (` |
| `ai_playerbot_random_bots` | `characters` | `playerbots/sql/characters/ai_playerbot_random_bots.sql:2` | `CREATE TABLE \`ai_playerbot_random_bots\` (` |

No `account_access` table. Bot accounts are marked only by the `account.username` prefix.

## F. Eluna / Lua / addon channel

| question | answer | citation |
|---|---|---|
| Eluna | **no** — no `eluna`/`ELUNA`/`BUILD_ELUNA` outside `dep/` at this SHA | (empty grep) |
| addon channel | yes, `CHAT_MSG_ADDON` | `src/game/Globals/SharedDefines.h:1547` |
| config | `AddonChannel`, default true, dist `= 1` | `src/game/World/World.cpp:480`; `src/mangosd/mangosd.conf.dist.in:277` |

---

# Tortoise — `Shyalya/tortoise-wow` @ `7c0fb278f3f8966422f219e6f5035cb09b76ada7`
(branch `playerbots-integration-gh`; bots vendored in-tree at `modules/mod-playerbots`)

This is **not** a CMaNGOS tree. It is a Turtle-WoW V2 / Nostalrius-lineage core: different
`AccountTypes` numbering, different auth-DB schema, commands in `src/game/Commands/Commands.cpp`
rather than `Level0-3.cpp`. Nothing in this section may be inherited from the two sections above.

## A. Remote command channels

| question | answer | citation | verbatim |
|---|---|---|---|
| SOAP present? | **NO** | `src/mangosd/CMakeLists.txt:19-30` | `set (EXECUTABLE_SRCS ` / `	CliRunnable.h` / `	DynamicModules.h` / `	Master.h` / `	WorldRunnable.h` / `	CliRunnable.cpp` / `	DynamicModules.cpp` / `	Main.cpp` / `	Master.cpp` / `	WorldRunnable.cpp` — no `MaNGOSsoap.cpp`, no `soapC.cpp` |
| gsoap built? | **no** — the `dep/src/gsoap` directory exists but is never added | `dep/src/CMakeLists.txt:19-25` | `if (MINGW)` / `    add_subdirectory(libseh)` / `endif()` / `add_subdirectory(g3dlite)` / `if(WIN32)` / `    add_subdirectory(zlib)` / `endif()` |
| any `SOAP.*` conf key? | **no** | grep over `src/mangosd/mangosd.conf.dist.in` | only two matches for `soap` in the whole tree outside `dep/`, both comments: `src/game/World.h:841` `    uint32 m_cliAccountId;                                  // 0 for console and real account id for RA/soap` and `modules/mod-dungeon-clear/src/TestRun/DcTestDriver.cpp:114` |
| RA / telnet present? | **NO** — no `RASocket.cpp`/`.h`, no `Ra.*` conf key | `src/mangosd/` listing; only stale comment `src/mangosd/CliRunnable.cpp:441` | `    // processed in RASocket` |
| **so what remote channel exists?** | a **database command queue**: rows in the login DB's `pending_commands` table are polled and executed at `SEC_CONSOLE` | `src/game/World.cpp:2724`, `:3947` | `        LoginDatabase.AsyncPQuery(this, &World::LoadPendingCommands, "SELECT \`id\`, \`command\` FROM \`pending_commands\` WHERE \`realm_id\`=%u && \`run_at_time\` <= %u", realmID, GetGameTime());` / `        QueueCliCommand(new CliCommandHolder(0, SEC_CONSOLE, nullptr, command.c_str(), &utf8print, &commandFinished));` |
| — poll cadence | **60 s** | `src/game/World.cpp:845` | `    m_timers[WUPDATE_COMMANDS].SetInterval(1 * MINUTE * IN_MILLISECONDS);` |
| — row is deleted after queueing | yes | `src/game/World.cpp:3948` | `        LoginDatabase.PExecute("DELETE FROM \`pending_commands\` WHERE \`id\`=%u", id);` |
| — does the caller get the output? | **no** — output goes to the process's stdout printer, nothing is written back to the DB | `src/game/World.cpp:3947` (`&utf8print`, `&commandFinished`); `:3946` | `        sLog.outBasic("Loaded command %u from database: %s", id, command.c_str());` |
| — schema | login DB (`tw_logon`) | `sql/create_databases.sql:2572-2578` | `CREATE TABLE \`pending_commands\` (` / `  \`id\` int(10) unsigned NOT NULL AUTO_INCREMENT COMMENT 'auto incrementing identifier for the row',` / `  \`realm_id\` int(10) unsigned DEFAULT NULL COMMENT 'id of the realm that should run the command',` / `  \`command\` varchar(250) NOT NULL DEFAULT '' COMMENT 'full comand with parameters',` / `  \`run_at_time\` int(10) unsigned NOT NULL DEFAULT 0 COMMENT 'unixtime',` |
| — command length cap | **250 chars** | `sql/create_databases.sql:2575` | (above) |
| executed on the world thread? | yes | `src/game/World.cpp:3079`, `:3913`, `:3921-3922` | `    ProcessCliCommands();` / `void World::ProcessCliCommands()` / `        CliHandler handler(command->m_cliAccountId, command->m_cliAccessLevel, callbackArg, zprint);` / `        handler.ParseCommands(command->m_command);` |

## B. Console

| question | answer | citation | verbatim |
|---|---|---|---|
| stdin console | yes | `src/mangosd/Master.cpp:559-562` | `    if (sConfig.GetBoolDefault("Console.Enable", true))` / `    {` / `        ///- Launch CliRunnable thread` / `        cliThread = new std::thread(CliRunnable());` |
| default | on; `Console.Enable = 1` in the dist | `src/mangosd/mangosd.conf.dist.in:2042` | `Console.Enable = 1` |
| level / account id | `SEC_CONSOLE` (= **6** here) / `0` | `src/mangosd/CliRunnable.cpp:586` | `            sWorld.QueueCliCommand(new CliCommandHolder(0, SEC_CONSOLE, nullptr, command.c_str(), &utf8print, &commandFinished));` |
| prompt | `mangos>` | `src/mangosd/CliRunnable.cpp:528` | `    printf("mangos>");` |
| input | `fgets`, 256-byte buffer | `src/mangosd/CliRunnable.cpp:518`, `:563` | `    char commandbuf[256];` / `        char *command_str = fgets(commandbuf,sizeof(commandbuf),stdin);` |

## C. Commands Phase 8 needs

Console gate, identical in text to CMaNGOS: `src/game/Chat/Chat.cpp:3723-3731`
```
bool CliHandler::isAvailable(ChatCommand const& cmd) const
{
    // skip non-console commands in console case
    if (!cmd.AllowConsole)
        return false;

    // normal case
    return GetAccessLevel() >= (AccountTypes)cmd.SecurityLevel;
}
```
Command struct carries three extra fields (`Flags`, `FullName`, `PermissionMask`) but the first
three positions are the same: `src/game/Chat/Chat.h:62-74`.

**`AccountTypes` is different from CMaNGOS** — `src/shared/Common.h:184-193`:
```
enum AccountTypes
{
    SEC_PLAYER         = 0,
    SEC_OBSERVER       = 1,
    SEC_MODERATOR      = 2,
    SEC_DEVELOPER      = 3,
    SEC_ADMINISTRATOR  = 4,
    SEC_SIGMACHAD      = 5,
    SEC_CONSOLE        = 6,                               
};
```
There is **no** `SELECT name,security,help FROM command` in `src/game/Chat/Chat.cpp` at this SHA
(grep for `FROM command` returns nothing) — the table is not overridable from the DB the way it is
on CMaNGOS. It *is* extensible from a `scriptCommandTable` (`src/game/Chat/Chat.cpp:1018`).

| command | exists | SEC_* | AllowConsole | handler | offline named target? | citation |
|---|---|---|---|---|---|---|
| `.tele name <player> <telename>` | yes | `SEC_DEVELOPER` (3) | **true** | `HandleTeleNameCommand` | **yes** — `Player::SavePositionInDB` | table `src/game/Chat/Chat.cpp:716`; handler `src/game/Commands/Commands.cpp:7993`, offline branch `:8033-8042` |
| `.tele` (self) | yes | `SEC_OBSERVER` (1) | false | `HandleTeleCommand` | — | `:718` |
| `.tele group` | yes | `SEC_DEVELOPER` | false | — | — | `:717` |
| `.go *` | yes | `SEC_OBSERVER`/`SEC_DEVELOPER` | **false**, all rows | — | no | `src/game/Chat/Chat.cpp:234-248` |
| — `.go xy`, `.go zonexy`, `.go grid`, `.go warp`, `.go next` | **absent**; `.go target/forward/up/corpse` present instead | — | — | — | — | `src/game/Chat/Chat.cpp:236-246` |
| `.send items` | yes | `SEC_DEVELOPER` | **true** | `HandleSendItemsCommand` | **yes** | `:663`; handler `src/game/Commands/Commands.cpp:5521`, `:5539` |
| **max items per `.send items`** | **1** | — | — | — | — | `src/game/Mail/Mail.h:51` `#define MAX_MAIL_ITEMS 1` |
| `.send mail` | yes | `SEC_MODERATOR` (2) | true | `HandleSendMailCommand` | yes | `:664` |
| `.send money` | yes | `SEC_DEVELOPER` | true | `HandleSendMoneyCommand` | yes | `:666` |
| `.send mass *` | **absent** (CMaNGOS has it) | — | — | — | — | `src/game/Chat/Chat.cpp:661-668` (four rows) |
| `.additem` | yes | `SEC_DEVELOPER` | **false** | `HandleAddItemCommand` | no | `:926`; handler `src/game/Commands/Commands.cpp:1387` |
| `.revive [name]` | yes | `SEC_DEVELOPER` | **true** | `HandleReviveCommand` | **yes**, and it says so | `:894`; handler `src/game/Commands/Commands.cpp:3024`, offline branch `:3037-3043` `        PSendSysMessage(LANG_CHARACTER_REVIVED_OFFLINE, playerLink(playername).c_str());` |
| `.levelup` | yes | `SEC_DEVELOPER` | **false** | `HandleLevelUpCommand` | — | `:923` |
| `.character level` | **ABSENT** — the `character` sub-table has no `level` row | — | — | — | — | `src/game/Chat/Chat.cpp:151-165` (`deleted, erase, getname, diffitems, reputation, hasitem, fillflys, clean, itemlog, mail, inactivity`) |
| `.modify money` | yes | `SEC_DEVELOPER` | **false** | `HandleModifyMoneyCommand` | no | `src/game/Chat/Chat.cpp:373` |
| character rename | yes but as a **top-level `.rename`**, not `.character rename` | `SEC_MODERATOR` | **true** | `HandleCharacterRenameCommand` | yes | `src/game/Chat/Chat.cpp:850` `        { "rename",         SEC_MODERATOR,       true,  &ChatHandler::HandleCharacterRenameCommand,     "", nullptr},`; handler `src/game/Commands/Commands.cpp:12564` |
| `.account create <user> <pass>` | yes | **`SEC_ADMINISTRATOR` (4)**, not `SEC_CONSOLE` | true | `HandleAccountCreateCommand` | n/a | `src/game/Chat/Chat.cpp:77`; handler `src/mangosd/CliRunnable.cpp:455` |
| — no expansion argument | correct, 2-arg only | — | — | — | — | `src/mangosd/CliRunnable.cpp:458-461`, `:467` |
| — password storage | `sha_pass_hash` (SHA1 of user:pass), **not** SRP `v`/`s` | — | — | — | — | `src/game/AccountMgr.cpp:59` `    if (!LoginDatabase.PExecute("INSERT INTO account(username,sha_pass_hash,joindate) VALUES('%s','%s',NOW())", username.c_str(), CalculateShaPassHash(username, password).c_str()))` |
| `.account set gmlevel <acct> <n>` | yes | **`SEC_ADMINISTRATOR` (4)** | true | `HandleAccountSetGmLevelCommand` | n/a | `src/game/Chat/Chat.cpp:69`; handler `src/game/Commands/Commands.cpp:275` |
| — `n` range | 0..4 (`SEC_PLAYER`..`SEC_ADMINISTRATOR` on *this* enum) | — | — | — | — | `src/game/Commands/Commands.cpp:293` `    if (gm < SEC_PLAYER \|\| gm > SEC_ADMINISTRATOR)` |
| — caller rank check is `>` not `>=` | yes — a caller may grant its own level | — | — | — | — | `src/game/Commands/Commands.cpp:307` `    if (AccountTypes(gm) > plSecurity)` |
| — write target | **`account.rank`**, not `account.gmlevel` | — | — | — | — | `src/game/Commands/Commands.cpp:321` `    sAccountMgr.SetSecurity(targetAccountId, AccountTypes(gm));` → `src/game/AccountMgr.cpp:261` `    LoginDatabase.PExecute("UPDATE \`account\` SET \`rank\` = '%u' WHERE (\`id\` = '%u')", sec, accId);` |
| `.account set addon` | **ABSENT** | — | — | — | — | `src/game/Chat/Chat.cpp:67-72` (only `gmlevel`, `password`) |
| `.account onlinelist` | **ABSENT**; `.account getname` (`SEC_OBSERVER`, console) exists instead | — | — | — | — | `src/game/Chat/Chat.cpp:74-86` |
| `.account delete` | yes | `SEC_CONSOLE` (6) | true | — | — | `:78` |
| `.server info` | yes | `SEC_PLAYER` | **true** | `HandleServerInfoCommand` | n/a | `:705`; handler `src/game/Commands/Commands.cpp:6355` |
| — what it prints from a console caller | core revision line, `Players online: N. Max online: M.`, uptime | — | — | — | — | `src/game/Commands/Commands.cpp:6357-6369` `    if (!m_session \|\| m_session->GetSecurity() != SEC_PLAYER)` / `        SendSysMessage("Core revision: " REVISION_HASH " / " REVISION_DATE " / " _ENDIAN_PLATFORM);` … `    PSendSysMessage("Players online: %i. Max online: %i.", activeClientsNum, maxActiveClientsNum);` |
| — diff/queue detail requires an in-game session | yes, `GetSession()` is required | — | — | — | — | `src/game/Commands/Commands.cpp:6372` `    if (GetSession() && GetSession()->GetSecurity() >= SEC_MODERATOR)` |
| `.server motd`, `.server plimit`, `.server log`, `.server set` | **absent** | — | — | — | — | `src/game/Chat/Chat.cpp:699-710` |
| `.pinfo <name>` | yes | `SEC_MODERATOR` | true | `HandlePInfoCommand` | yes | `src/game/Chat/Chat.cpp:935` |

## D. Playerbots (in-tree `modules/mod-playerbots`)

The vendored module is the same code family as `cmangos/playerbots`, with local additions
(`summon`/`recall`/`come`, `SC_LOG` tracing). Cited here from the Tortoise tree only.

| question | answer | citation | verbatim |
|---|---|---|---|
| `.bot` | `SEC_PLAYER`, AllowConsole **false** | `src/game/Chat/Chat.cpp:1011` | `        { "bot",            SEC_PLAYER,           false, &ChatHandler::HandlePlayerbotCommand,           "", nullptr },` |
| `.rndbot` | **`SEC_PLAYER`** (lower than CMaNGOS's `SEC_GAMEMASTER`), AllowConsole **true** | `src/game/Chat/Chat.cpp:1012` and the comment above it at `:1008-1010` | `        { "rndbot",         SEC_PLAYER,          true,  &ChatHandler::HandleRandomPlayerbotCommand,     "", nullptr },` / `        // Bot module commands. .rndbot is SEC_PLAYER so a single human can` / `        // manage their own random-bot pool without keeping a GM alt logged in;` / `        // a server operator who wants tighter control can raise it.` |
| `.ahbot` / `.perfmon` | `SEC_MODERATOR`, console true | `src/game/Chat/Chat.cpp:1013-1014` | `        { "ahbot",          SEC_MODERATOR,       true,  &ChatHandler::HandleAhBotCommand,               "", nullptr },` |
| `.bot` needs a live session | **yes** | `modules/mod-playerbots/src/playerbot/PlayerbotMgr.cpp:971-977` | `    WorldSession *m_session = handler->GetSession();` / `    if (!m_session)` / `        handler->PSendSysMessage("You may only add bots from an active session");` / `        return false;` |
| `.rndbot` output reaches a session-less caller? | only if the caller's account id is non-zero. The console (id 0) and the `pending_commands` queue (id 0) both get **log-only** output. | `modules/mod-playerbots/src/playerbot/RandomPlayerbotMgr.cpp:3620-3625` | `    bool isRA = false;` / `    if (handler->GetSession()) //Client command` / `        isRA = true;` / `    else if (static_cast<CliHandler*>(handler) && static_cast<CliHandler*>(handler)->GetAccountId()) //RA call with account.` / `        isRA = true;` |
| holder sub-commands | `list, help, reload, tweak, self, spoof, p, g, r, rl, create, group` | `modules/mod-playerbots/src/playerbot/PlayerbotMgr.cpp:293-304` | `    m_holderHandlers["list"] = &PlayerbotHolder::HandleList;` … |
| per-bot sub-commands | as cmangos plus `summon`/`recall`/`come` | `modules/mod-playerbots/src/playerbot/PlayerbotMgr.cpp:309-349` | `    m_botCommandHandlers["summon"] = &PlayerbotHolder::HandleBotSummon;` / `    m_botCommandHandlers["recall"] = &PlayerbotHolder::HandleBotSummon;` / `    m_botCommandHandlers["come"]   = &PlayerbotHolder::HandleBotSummon;` |
| `.rndbot p <PlayerName> <msg>` party relay | present, same null-master fallback | `modules/mod-playerbots/src/playerbot/PlayerbotMgr.cpp:2007-2027` | `    if (!master)` / `        botName = param.substr(0, param.find(" "));` / `        master = sObjectAccessor.FindPlayerByName(botName.c_str());` / `    if (!master)` / `        return {"No sender found"};` / `    if (!master->GetGroup())` / `        return {"Sender is not in a group"};` |
| `.rndbot spoof`, `.rndbot cmd`, `.rndbot w` | present | `modules/mod-playerbots/src/playerbot/PlayerbotMgr.cpp:298`, `:346`, `:345` | `    m_holderHandlers["spoof"] = &PlayerbotHolder::HandleSpoof;` / `    m_botCommandHandlers["cmd"] = &PlayerbotHolder::HandleConsoleCmd;` / `    m_botCommandHandlers["w"] = &PlayerbotHolder::HandleConsoleWhisper;` |
| add-bot logic | same three-way branch as cmangos | `modules/mod-playerbots/src/playerbot/PlayerbotMgr.cpp:2196`, `:2209`, `:2217-2219` | `std::string PlayerbotHolder::HandleBotAddLogin(Player* bot, Player* master, const std::string param)` / `    if (!Qualified::isValidNumberString(param))` / `    uint32 botAccount = sObjectMgr.GetPlayerAccountIdByGUID(guid);` / `    bool isMasterAccount = (masterAccountId == botAccount);` / `    bool isRandomAccount = sPlayerbotAIConfig.IsInRandomAccountList(botAccount);` |
| `.rndbot stats` from console | fails the same way (empty param) | `modules/mod-playerbots/src/playerbot/RandomPlayerbotMgr.cpp:4731-4734` | `    if (!Qualified::isValidNumberString(param))` / `        return {"Stats: Error parsing " + param};` |
| Playerbot Command Server | present, started from `RandomPlayerbotMgr` | `modules/mod-playerbots/src/playerbot/PlayerbotCommandServer.cpp`; `RandomPlayerbotMgr.cpp:275` | `        sPlayerbotCommandServer.Start();` |
| — port key/default | `AiPlayerbot.CommandServerPort`, default 0, commented out in the dist | `modules/mod-playerbots/src/playerbot/PlayerbotAIConfig.cpp:358`; `src/playerbot/aiplayerbot.conf.dist.in:1151` | `    commandServerPort = config.GetIntDefault("AiPlayerbot.CommandServerPort", 0);` / `#AiPlayerbot.CommandServerPort = 8888` |

Bot-account config (dist values in **this** tree; Yu'lon's install overrides are in Step 1):

| key | Tortoise dist value | code default | citation |
|---|---|---|---|
| `AiPlayerbot.Enabled` | `1` | — | `modules/mod-playerbots/src/playerbot/aiplayerbot.conf.dist.in:12` |
| `AiPlayerbot.RandomBotAutologin` | `1` | — | `:18` |
| `AiPlayerbot.BotAutologin` | `0` | — | `:39` |
| `AiPlayerbot.MinRandomBots` / `MaxRandomBots` | `1000` / `1000` | — | `:57-58` |
| `AiPlayerbot.RandomBotAccountPrefix` | `RNDBOT` | `"rndbot"` | `:63`; `PlayerbotAIConfig.cpp:545` |
| `AiPlayerbot.RandomBotAccountCount` | **`500`** (TBC/Vanilla dist says 200) | `50` | `:64`; `PlayerbotAIConfig.cpp:546` |

## E. Database schema

Yu'lon DB names: auth `tw_logon`, characters `tw_char`, world `tw_world`, plus `tw_logs`.

| object | DB | citation | verbatim |
|---|---|---|---|
| `item_template` | `tw_world` | `sql/base/tw_world_item_template.sql:26`, `:30` | `CREATE TABLE \`item_template\` (` / `  \`name\` varchar(255) NOT NULL DEFAULT '',` |
| — also in the combined dump | — | `sql/create_databases.sql:6542` | `CREATE TABLE \`item_template\` (` |
| — has a `description` column CMaNGOS lacks | yes | `sql/base/tw_world_item_template.sql:31` | `  \`description\` varchar(255) NOT NULL DEFAULT '',` |
| — display column is `display_id`, not `displayid` | yes | `sql/base/tw_world_item_template.sql:32` | `  \`display_id\` mediumint(8) unsigned NOT NULL DEFAULT 0,` |
| `game_tele` | `tw_world` | `sql/base/tw_world_game_tele.sql:26`, `:33` | `CREATE TABLE \`game_tele\` (` / `  \`name\` varchar(100) NOT NULL DEFAULT '',` |
| `characters` | `tw_char` | `sql/create_databases.sql:963` | `CREATE TABLE \`characters\` (` |
| — `online` column | `tw_char` | `sql/create_databases.sql:982` | `  \`online\` tinyint(3) unsigned NOT NULL DEFAULT 0,` |
| — `account` / `name` | `tw_char` | `sql/create_databases.sql:964-966` | `  \`account\` int(10) unsigned NOT NULL DEFAULT 0 COMMENT 'Account Identifier',` / `  \`name\` varchar(12) NOT NULL DEFAULT '',` |
| `account` | `tw_logon` | `sql/create_databases.sql:2089` | `CREATE TABLE \`account\` (` |
| — **there is no `gmlevel` column**; GM level lives in `rank` | `tw_logon` | `sql/create_databases.sql:2093` | `  \`rank\` int(10) unsigned NOT NULL DEFAULT 0,` |
| — separate free-text `security` column also exists | `tw_logon` | `sql/create_databases.sql:2116` | `  \`security\` varchar(255) DEFAULT NULL,` |
| — account-level `online` flag | `tw_logon` | `sql/create_databases.sql:2105` | `  \`online\` tinyint(4) NOT NULL DEFAULT 0,` |
| — password hash column | `tw_logon` | `sql/create_databases.sql:2092` | `  \`sha_pass_hash\` varchar(40) NOT NULL,` |
| — no `account_access` table | confirmed (grep returns nothing) | — | — |
| `realmlist` | `tw_logon` | `sql/create_databases.sql:2647`, `:2657` | `CREATE TABLE \`realmlist\` (` / `  \`realmbuilds\` varchar(64) NOT NULL DEFAULT '5875',` |
| `pending_commands` (the remote-command channel) | `tw_logon` | `sql/create_databases.sql:2572` | `CREATE TABLE \`pending_commands\` (` |
| `ai_playerbot_random_bots` | `tw_char` (catalog phase "playerbots characters") | `modules/mod-playerbots/sql/characters/ai_playerbot_random_bots.sql` | (table present; same shape as cmangos) |
| bot-account marker | none — only the `account.username` `RNDBOT%` prefix | — | — |

`AccountMgr::GetSecurity` in this tree reads an in-memory map, not the DB, so a direct SQL write to
`account.rank` is not picked up until the cache is refilled: `src/game/AccountMgr.cpp:250-256`
`    std::map<uint32, AccountTypes>::const_iterator it = m_accountSecurity.find(acc_id);` /
`    if (it == m_accountSecurity.end())` / `        return SEC_PLAYER;`.

## F. Eluna / Lua / addon channel

| question | answer | citation | verbatim |
|---|---|---|---|
| Eluna integrated? | **yes**, as the submodule `src/modules/Eluna`, pinned to the same SHA the catalog pins | `docs/ELUNA.md:3-7` | `This branch integrates [Eluna](https://github.com/ElunaLuaEngine/Eluna) into` / `the Turtle WoW MaNGOS core as the \`src/modules/Eluna\` Git submodule. The` / `submodule is pinned to commit \`1b06f28ff3a00054d915d824c725fb4283fee74d\`, the` |
| build switch and default | `BUILD_ELUNA`, **ON** | `CMakeLists.txt:43` | `option(BUILD_ELUNA "Build the Eluna Lua scripting engine" ON)` |
| Lua runtime | `lua52` by default, selectable | `CMakeLists.txt:45-46` | `set(ELUNA_LUA_VERSION "lua52" CACHE STRING "Lua runtime used by Eluna (lua51, lua52, lua53, or lua54)")` |
| build fails if the submodule is missing | yes | `CMakeLists.txt:49-51` | `  if(NOT EXISTS "${CMAKE_SOURCE_DIR}/src/modules/Eluna/LuaEngine.h")` / `      "Eluna submodule is missing. Run: git submodule update --init --recursive src/modules/Eluna")` |
| backend chosen | Eluna's **VMaNGOS** compatibility backend, `ELUNA_EXPANSION=0` | `docs/ELUNA.md:10-14` | `The host selects Eluna's VMaNGOS compatibility backend (\`ELUNA_VMANGOS\`) and` / `the vanilla client expansion (\`ELUNA_EXPANSION=0\`) because that backend most` / `closely matches this fork's classic API surface.` |
| runtime switch, independent of the build switch | `Eluna.Enabled` | `docs/ELUNA.md:47-49`; `src/mangosd/mangosd.conf.dist.in:2304` | `\`BUILD_ELUNA\` is the compile-time switch; \`Eluna.Enabled\` is the independent` / `runtime switch.` / `Eluna.Enabled = 1` |
| script path default | `./lua_scripts` relative to the server cwd | `src/mangosd/mangosd.conf.dist.in:2310` | `Eluna.ScriptPath = "./lua_scripts"` |
| `.reload eluna` | enabled by default, min security 3 (`SEC_DEVELOPER` on this enum) | `src/mangosd/mangosd.conf.dist.in:2306`, `:2313`, `:2288`, `:2300` | `Eluna.ReloadCommand = true` / `Eluna.ReloadSecurityLevel = 3` / `#     Enables the \`.reload eluna\` command. Default: true.` / `#     Minimum account security for \`.reload eluna\` (0 player .. 4 console).` |
| other Eluna keys shipped | `TraceBack`, `UseUnsafeMethods`, `UseDeprecatedMethods`, `OnlyOnMaps`, `RequirePaths`, `RequireCPaths`, `ElunaErrorLogFile` | `src/mangosd/mangosd.conf.dist.in:2305-2314` | (block as listed) |
| Yu'lon patches any Eluna key at install? | **no** — no `Eluna.*` key appears in the catalog's `mangosd.conf` key table (Step 1) | — | — |
| addon chat channel | yes, `AddonChannel`, default true, dist `= 1` | `src/game/World.cpp:1063`; `src/mangosd/mangosd.conf.dist.in:276` | `    setConfig(CONFIG_BOOL_ADDON_CHANNEL, "AddonChannel", true);` / `AddonChannel = 1` |
| `SavedVariables` reference in the tree | none found | — | — |

---

# Facts I could not establish, and why

1. **Whether Yu'lon's built images actually expose a SOAP or RA port for TBC/Vanilla.** The
   catalog patches neither `SOAP.Enabled` nor `Ra.Enable`, and `ports` lists only `auth`, `world`,
   `db`. So the *code* supports both and the *install* enables neither. Whether the compose files
   publish 7878/3443 is a Yu'lon-side question (`composegen.py`, the `*.yml.tmpl` templates), not a
   CMaNGOS-tree question, and I did not read those files — that is outside the brief.

2. **The exact `soap_sender_fault` wire shape a Python client sees.** I read that the failure path
   returns the buffered command output as both fault string and fault detail
   (`MaNGOSsoap.cpp:135`), but the gSOAP-generated envelope is in `soapC.cpp`, which is generated
   code I did not read. A caller-side decision about how to distinguish "command failed" from
   "transport failed" needs either that file read or a live probe. **Needs a judgement / live test —
   stopping here on that item.**

3. **Whether Tortoise's `pending_commands` channel can be used concurrently with the stdin
   console safely.** Both enqueue at `SEC_CONSOLE` into the same `cliCmdQueue`, and
   `ProcessCliCommands` drains under no visible lock (`src/game/World.cpp:3913-3928` uses
   `cliCmdQueue.next(command)`, whose locking lives in the queue template I did not open). No
   ordering or interleaving guarantee is stated anywhere in the tree.

4. **What the Tortoise `pending_commands` row's `realm_id` must be for a single-realm Yu'lon
   install.** The query filters on the running realm's `realmID` (`src/game/World.cpp:2724`), and
   Yu'lon's SQL seeds `realmlist` id 1 for Tortoise, but I did not read where `realmID` is set from
   (`realmd.conf`/`mangosd.conf` key) — that would need `src/game/World.cpp` / `Main.cpp` config
   plumbing I did not open.

5. **Whether `.rndbot add <name>` succeeds end to end from a session-less caller.** I established
   the code path reaches `AddRandomBot(guid)` for a random-account bot with `master == nullptr`
   (`playerbots/playerbot/PlayerbotMgr.cpp:793`, `:1824-1825`), but `AddRandomBot`'s own
   preconditions (bot pool caps, `RandomBotAutologin`, map/grid readiness) were not traced, and the
   result is not reported back to a console caller (`isRA == false`). Proving it works is a live
   test, not a read. **Needs a live test — stopping here on that item.**

6. **Bot-count queries for a dashboard.** `.rndbot list` returns loaded bots as one string
   (`playerbots/playerbot/PlayerbotMgr.cpp:819-856`), and `.rndbot stats` refuses a session-less
   caller. Whether counting via SQL (`characters.online` joined to `account.username LIKE
   'RNDBOT%'`) matches the module's own notion of "loaded bot" is not something the source states;
   the module's roster is the in-memory `playerBots` map, which is not persisted.

7. **Tortoise: whether `.rndbot` at `SEC_PLAYER` is reachable from `pending_commands`.** It is
   `AllowConsole = true` and `SEC_PLAYER ≤ SEC_CONSOLE`, so `isAvailable` passes — but with account
   id 0 the module's `isRA` is false and no output returns. Whether the command *ran* would have to
   be read out of the world log, not the channel.

8. **Whether `.character level` has any Tortoise equivalent.** I confirmed the row is absent from
   `characterCommandTable` and that `.levelup` is `AllowConsole = false`. I did not exhaustively
   search Tortoise's ~1000-row command table for a differently named level-setter, only for the
   handler names `HandleCharacterLevelCommand` (absent from `Chat.cpp` entirely) and `"levelup"`.

9. **The `mangos-tbc` / `mangos-classic` `command` table's shipped rows.** Both cores load
   security overrides from the world DB `command` table at startup, so any level in § C can be
   different at runtime depending on what `tbc-db`/`classic-db` ships. I skipped the DB repos per
   the brief, so the effective levels on a Yu'lon install are not established here.

10. **Tortoise SOAP/RA: "absent" is proved by absence.** No `MaNGOSsoap.*`, no `RASocket.*`, no
    `gsoap` subdirectory added, no `SOAP.*`/`Ra.*` conf key. Absence of a grep hit is weaker
    evidence than a positive citation; the two positive artefacts I can point at are
    `src/mangosd/CMakeLists.txt:19-30` (the complete source list) and `dep/src/CMakeLists.txt:19-25`
    (the complete dependency list), both of which are short enough to be exhaustive.
