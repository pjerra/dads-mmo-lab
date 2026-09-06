# AzerothCore / WotLK source read — at Yu'lon's pinned revisions

Read 2026-09-06. Nothing here comes from `master`; every citation is at a SHA listed below.
Paths in sections A/B/D/E/F are relative to the fetched tree root of the repo named in the header of the section.

## Fetched SHAs

| repo | pinned by | rev asked for | rev fetched | tree |
|---|---|---|---|---|
| `mod-playerbots/azerothcore-wotlk` | catalog `emulator.sources[0].rev` | `413bea61a85e20d9caef7d66fc601a661fdddd9d` (branch `Playerbot`) | `413bea61a85e20d9caef7d66fc601a661fdddd9d` | `…/scratchpad/ac-trees/core` |
| `mod-playerbots/mod-playerbots` | catalog `emulator.sources[1].rev` | `b949b50bfcdd4fab937781bac2d7765e39330e4b` (branch `master`) | `b949b50bfcdd4fab937781bac2d7765e39330e4b` | `…/scratchpad/ac-trees/mod-playerbots` |
| `azerothcore/mod-ale` | **NOT PINNED** — `manifests/wow-wotlk/modules/mod-ale.json` names only `source.repo` | default HEAD | `c3de79426b03b02d2762536d727f8d40b0f8f24a`, committed `2026-09-06T09:07:30+02:00` | `…/scratchpad/ac-trees/mod-ale` |

**mod-ale is a moving target.** Yu'lon's manifest carries `"source": {"repo": "azerothcore/mod-ale"}` and no `rev`/`branch`
(`pylauncher/manifests/wow-wotlk/modules/mod-ale.json:9-11`). Everything in section E is a read of today's HEAD, which was
committed the same day it was read. Treat E as "as of 2026-09-06", not as a pin.

---

## Step 1 — the catalog pins (verbatim)

All from `/c/Users/perzi/dml-phase8`.

### `pylauncher/yulon/catalog/catalog.json`, `wow-wotlk` entry

| field | value | line |
|---|---|---|
| `emulator.sources[0]` | `repo: "mod-playerbots/azerothcore-wotlk"`, `branch: "Playerbot"`, `depth: null`, `dest: "."`, `rev: "413bea61a85e20d9caef7d66fc601a661fdddd9d"` | `catalog.json:12-18` |
| `emulator.sources[1]` | `repo: "mod-playerbots/mod-playerbots"`, `branch: "master"`, `dest: "modules/mod-playerbots"`, `rev: "b949b50bfcdd4fab937781bac2d7765e39330e4b"` | `catalog.json:19-24` |
| `ports` | `auth: 3724`, `world: 8085`, `db: 3306` | `catalog.json:69-73` |
| `databases` | `auth: acore_auth`, `characters: acore_characters`, `world: acore_world`, `extra: [acore_playerbots]`, `playerbots: acore_playerbots`, `ale: acore_ale` | `catalog.json:74-83` |
| `containers` | `db: ac-database`, `auth: ac-authserver`, `world: ac-worldserver`, `db_import: ac-db-import`, `client_data: ac-client-data-init` | `catalog.json:62-68` |
| `install.native.azerothcore.world_env` | exactly two keys — see below | `catalog.json:56-60` |
| anything named `soap` | **NONE.** `grep -ni soap catalog.json` returns nothing. | — |

`world_env`, verbatim (`catalog.json:57-59`):
```
              "AC_AI_PLAYERBOT_MIN_RANDOM_BOTS": "500",
              "AC_AI_PLAYERBOT_MAX_RANDOM_BOTS": "500"
```

The SOAP port is **not** a catalog field. It is a model default:
`pylauncher/yulon/catalog/catalog.py:670` — `    soap_port: int = Field(default=7878, gt=0, lt=65536)`
fed into the template at `pylauncher/yulon/catalog/composegen.py:369` — `            "SOAP_PORT": str(native.soap_port),`

### `pylauncher/catalog/installers/wow-wotlk/native/base.yml.tmpl`, lines 240–270 (verbatim)

```
    tty: true
    restart: unless-stopped
    # Measured, not inherited: three shutdowns of a populated playerbots realm
    # on yulon-ubuntu (2026-08-23) took 90.7s, 73.4s and 58.3s, almost all of
    # it draining 7400-7700 queued character saves. Compose's 10s default
    # SIGKILLs that mid-save; see docker.STOP_GRACE_SECONDS, which is the same
    # 300 seconds spelled for the CLI stop path.
    stop_grace_period: 5m
    environment:
      AC_DATA_DIR: "/azerothcore/env/dist/data"
      AC_LOGS_DIR: "/azerothcore/env/dist/logs"
      AC_LOGIN_DATABASE_INFO: "ac-database;3306;root;${DB_ROOT_PASSWORD:-{{DB_PASSWORD}}};acore_auth"
      AC_WORLD_DATABASE_INFO: "ac-database;3306;root;${DB_ROOT_PASSWORD:-{{DB_PASSWORD}}};acore_world"
      AC_CHARACTER_DATABASE_INFO: "ac-database;3306;root;${DB_ROOT_PASSWORD:-{{DB_PASSWORD}}};acore_characters"
      AC_PLAYERBOTS_DATABASE_INFO: "ac-database;3306;root;${DB_ROOT_PASSWORD:-{{DB_PASSWORD}}};acore_playerbots"
      # Runtime settings live in docker-compose.override.yml.
    ports:
      - "${DOCKER_WORLD_EXTERNAL_PORT:-{{WORLD_PORT}}}:8085"
      # SOAP is pinned to loopback by the DEFAULT VALUE, never by an IP prefix
      # on the mapping: a later SOAP setup writes the WHOLE host binding into
      # .env as DOCKER_SOAP_EXTERNAL_PORT=127.0.0.1:7878, and a literal prefix
      # here would render 127.0.0.1:127.0.0.1:7878:7878. Compose also
      # CONCATENATES ports: across files — never add a second entry for this
      # port anywhere else.
      - "${DOCKER_SOAP_EXTERNAL_PORT:-127.0.0.1:{{SOAP_PORT}}}:7878"
    {{CONTAINER_USER}}
    volumes:
      # env/dist/etc must be bound out to the host: it is where the module
      # system reads and writes worldserver.conf and every module conf.
      - ./env/dist/etc:/azerothcore/env/dist/etc{{BIND_LABEL}}
      - ./env/dist/logs:/azerothcore/env/dist/logs{{BIND_LABEL}}
```

### `override.yml.tmpl` (full, 23 lines)

Body: `services: → ac-worldserver: → volumes: [- ./modules:/azerothcore/modules{{BIND_LABEL}}] → environment: {{ENVIRONMENT}}`
(`override.yml.tmpl:13-23`).

`{{ENVIRONMENT}}` is filled from `{**DEFAULT_WORLD_ENV, **entry_env}` (`composegen.py:361`), where
`DEFAULT_WORLD_ENV` is (`composegen.py:100-103`):
```
DEFAULT_WORLD_ENV: Mapping[str, str] = {
    "AC_PLAYERBOTS_UPDATES_ENABLE_DATABASES": "1",
    "AC_AI_PLAYERBOT_RANDOM_BOT_AUTOLOGIN": "1",
}
```
The rendered result is checked in at `pylauncher/tests/data/wotlk-rendered/docker-compose.override.yml:23-26`:
```
      AC_AI_PLAYERBOT_MAX_RANDOM_BOTS: "500"
      AC_AI_PLAYERBOT_MIN_RANDOM_BOTS: "500"
      AC_AI_PLAYERBOT_RANDOM_BOT_AUTOLOGIN: "1"
      AC_PLAYERBOTS_UPDATES_ENABLE_DATABASES: "1"
```

**Which env vars reach the worldserver: exactly ten.** Six from `base.yml.tmpl:249-254` (`AC_DATA_DIR`, `AC_LOGS_DIR`,
four `AC_*_DATABASE_INFO`) plus the four above. **No `AC_SOAP_*` is set anywhere in the repository** — the only
`AC_SOAP_*`-shaped thing is the *host port publication* `${DOCKER_SOAP_EXTERNAL_PORT:-127.0.0.1:7878}:7878`, which
publishes a container port the server is not listening on unless someone turns SOAP on by another route.

---

## A. SOAP (repo: core @ 413bea61)

| question | answer | citation | verbatim line |
|---|---|---|---|
| `SOAP.Enabled` default in `worldserver.conf.dist` | `0` (disabled) | `src/server/apps/worldserver/worldserver.conf.dist:453` | `SOAP.Enabled = 0` |
| doc block for it | header spells the key `SOAP.Enable`, the setting is `SOAP.Enabled` | `worldserver.conf.dist:448-451` | `#    SOAP.Enable` / `#        Description: Enable soap service` / `#        Default:     0 - (Disabled)` / `#                     1 - (Enabled)` |
| `SOAP.IP` default | `"127.0.0.1"` | `worldserver.conf.dist:460` | `SOAP.IP = "127.0.0.1"` |
| `SOAP.Port` default | `7878` | `worldserver.conf.dist:467` | `SOAP.Port = 7878` |
| compiled-in fallbacks (used if the key is absent from the conf) | same three | `src/server/apps/worldserver/Main.cpp:337,339` | `    if (sConfigMgr->GetOption<bool>("SOAP.Enabled", false))` / `        soapThread.reset(new std::thread(ACSoapThread, sConfigMgr->GetOption<std::string>("SOAP.IP", "127.0.0.1"), uint16(sConfigMgr->GetOption<int32>("SOAP.Port", 7878))),` |
| Does the docker entrypoint map `AC_SOAP_*`? | **No — the entrypoint maps nothing.** It only copies `env/ref/etc/*` into `$CONF_DIR`, copies `<component>.conf.dist` → `<component>.conf` if absent, and `exec "$@"`. | `apps/docker/entrypoint.sh:40,46-50,54` | `cp -rnv /azerothcore/env/ref/etc/* "$CONF_DIR"` … `if [[ -f "$CONF_DIST" ]]; then` / `    cp -vn "$CONF_DIST" "$CONF"` / `else` / `    touch "$CONF"` / `fi` … `exec "$@"` |
| Then what maps `AC_*`? | **The C++ config manager, generically.** Every ini key `X` is looked up as env var `"AC_" + upper_snake(X)`. | `src/common/Configuration/Config.cpp:435-438` | `    std::string GetEnvVarName(std::string const& configName)` / `    {` / `        return "AC_" + IniKeyToEnvVarKey(configName);` / `    }` |
| the transform | `.`, `-` and space → `_`; lower→UPPER boundary inserts `_`; digit boundaries insert `_` | `Config.cpp:370-374` (doc) and `:391-394` (separators) | `    //   SomeConfig => SOME_CONFIG` / `    //   myNestedConfig.opt1 => MY_NESTED_CONFIG_OPT_1` / `    //   LogDB.Opt.ClearTime => LOG_DB_OPT_CLEAR_TIME` … `            if (curr == ' ' || curr == '.' || curr == '-')` / `            {` / `                result += '_';` / `                continue;` |
| applied at load, to keys already in the file | yes, `OverrideWithEnvVariablesIfAny()` walks `_configOptions` | `Config.cpp:510,516,521-527` | `std::vector<std::string> ConfigMgr::OverrideWithEnvVariablesIfAny()` … `    for (auto& itr : _configOptions)` … `        Optional<std::string> envVar = EnvVarForIniKey(itr.first);` / `        if (!envVar)` / `            continue;` / `        itr.second = *envVar;` |
| and at read time, even for keys absent from the file | yes — env wins over both the file value and the compiled default | `Config.cpp:540-552` | `    auto envVarName = GetEnvVarName(name);` / `    Optional<std::string> envVar = GetEnvFromCache(name, envVarName);` / `    if (envVar)` … `        strValue = *envVar;` |
| so the env names are | `AC_SOAP_ENABLED`, `AC_SOAP_IP`, `AC_SOAP_PORT` (also `AC_RA_ENABLE`, `AC_CONSOLE_ENABLE`, and `AiPlayerbot.MinRandomBots` → `AC_AI_PLAYERBOT_MIN_RANDOM_BOTS`, which is what the catalog already uses and is the proof the rule is generic) | derived from `Config.cpp:375-433` + `catalog.json:57` | — |
| where the SOAP handler lives | `src/server/apps/worldserver/ACSoap/ACSoap.cpp` (there is no `RemoteAccess/` SOAP; `RemoteAccess/` holds `RASession.{cpp,h}` only) | `ACSoap/ACSoap.cpp` | — |
| what it authenticates against | account name + password (SRP6 via `AccountMgr::CheckPassword`), 401 if either is missing or wrong | `ACSoap.cpp:84-101` | `    if (!soap->userid || !soap->passwd)` … `    uint32 accountId = AccountMgr::GetId(soap->userid);` … `    if (!AccountMgr::CheckPassword(accountId, soap->passwd))` … `        return 401;` |
| minimum security level | **`SEC_ADMINISTRATOR` (3)** — anything lower gets HTTP 403 | `ACSoap.cpp:103-107` | `    if (AccountMgr::GetSecurity(accountId) < SEC_ADMINISTRATOR)` / `    {` / `        LOG_DEBUG("network.soap", "ACSoap: {}'s gmlevel is too low", soap->userid);` / `        return 403;` |
| which table the level comes from | `account_access.gmlevel`, **realm-agnostic** (no `RealmID` filter on this path); missing row ⇒ `SEC_PLAYER` | `src/server/game/Accounts/AccountMgr.cpp:257-264` and `src/server/database/Database/Implementation/LoginDatabase.cpp:101` | `uint32 AccountMgr::GetSecurity(uint32 accountId)` … `    return (result) ? (*result)[0].Get<uint8>() : uint32(SEC_PLAYER);` ; `    PrepareStatement(LOGIN_GET_ACCOUNT_ACCESS_GMLEVEL, "SELECT gmlevel FROM account_access WHERE id = ?", CONNECTION_SYNCH);` |
| at what level the command then executes | **no level at all — the console path bypasses the check.** `CliHandler` has `m_session == nullptr`, and every security gate short-circuits on `IsConsole()`. A `Console::Yes` command is allowed outright. | `src/server/game/Chat/Chat.h:247`; `Chat.cpp:52-56,58-62`; `src/server/game/Chat/ChatCommands/ChatCommand.cpp:513-517` | `    bool IsConsole() const { return (m_session == nullptr); }` ; `bool ChatHandler::IsAvailable(uint32 securityLevel) const` / `{` / `    // check security level only for simple  command (without child commands)` / `    return IsConsole() ? true : m_session->GetSecurity() >= AccountTypes(securityLevel);` ; `    if (who.IsConsole() && (_permission.AllowConsole == Acore::ChatCommands::Console::No))` / `        return false;` / ` ` / `    if (who.IsConsole() && (_permission.AllowConsole == Acore::ChatCommands::Console::Yes))` / `        return true;` |
| queued onto the world thread? | yes | `ACSoap.cpp:120-121` | `    // commands are executed in the world thread. We have to wait for them to be completed` / `    sWorld->QueueCliCommand(new CliCommandHolder(connection.get(), command, &SOAPCommand::print, &SOAPCommand::commandFinished));` |
| blocks until done? | yes, with a 1-second-poll shutdown escape | `ACSoap.cpp:125-130` | `    std::future<void> finished = connection->finishedPromise.get_future();` / `    while (finished.wait_for(std::chrono::seconds(1)) != std::future_status::ready)` / `    {` / `        if (World::IsStopped())` / `            return soap_receiver_fault(soap, "Server is shutting down", "Command aborted: the server is shutting down");` / `    }` |
| output returned in the SOAP result? | yes — the accumulated print buffer is the `result` string | `ACSoap.cpp:133-137` | `    char* printBuffer = soap_strdup(soap, connection->m_printBuffer.c_str());` / `    if (connection->hasCommandSucceeded())` / `    {` / `        *result = printBuffer;` / `        return SOAP_OK;` |
| failure signalling | **SOAP fault**, sender-fault, with the same print buffer as both faultstring and detail | `ACSoap.cpp:139-140` | `    else` / `        return soap_sender_fault(soap, printBuffer, printBuffer);` |
| empty command | sender fault before anything is queued | `ACSoap.cpp:109-110` | `    if (!command || !*command)` / `        return soap_sender_fault(soap, "Command can not be empty", "The supplied command was an empty string");` |
| success/failure is decided by | `!handler.HasSentErrorMessage()` on the world side | `src/server/game/World/World.cpp:1653-1654` | `        if (command->m_commandFinished)` / `            command->m_commandFinished(callbackArg, !handler.HasSentErrorMessage());` |
| bind failure | logs and stops the world with `ERROR_EXIT_CODE` | `ACSoap.cpp:41-46` | `    if (!soap_valid_socket(soap_bind(&soap, host.c_str(), port, 100)))` / `    {` / `        LOG_ERROR("network.soap", "ACSoap: couldn't bind to {}:{}", host, port);` / `        // graceful shutdown: exit() here would destroy SocketMgr before StopNetwork() and assert` / `        World::StopNow(ERROR_EXIT_CODE);` / `        return;` / `    }` |
| RA exists? | yes, telnet remote console | `worldserver.conf.dist:419-445` | `#    Ra.Enable` / `#        Description: Enable remote console (telnet).` … `Ra.Enable = 0` … `Ra.IP = "0.0.0.0"` … `Ra.Port = 3443` … `Ra.MinLevel = 3` |
| RA auth | `account_access.gmlevel >= Ra.MinLevel` **and** the row's `RealmID` must be `-1`; password by SRP6 | `src/server/apps/worldserver/RemoteAccess/RASession.cpp:141,146-149,174` | `    if (fields[1].Get<uint8>() < sConfigMgr->GetOption<int32>("Ra.MinLevel", 3))` … `    else if (fields[2].Get<int32>() != -1)` / `    {` / `        LOG_INFO("commands.ra", "User {} has to be assigned on all realms (with RealmID = '-1')", user);` / `        return false;` … `        if (Acore::Crypto::SRP6::CheckLogin(safe_user, safe_pass, salt, verifier))` |
| `Console.Enable` default | `1` (enabled) in the dist file; compiled fallback `true` | `worldserver.conf.dist:265`; `Main.cpp:400,402` | `Console.Enable = 1` ; `    if (sConfigMgr->GetOption<bool>("Console.Enable", true) && (m_ServiceStatus == -1)/* need disable console in service mode*/)` |

---

## B. The command table (repo: core @ 413bea61)

**This fork does not use `SEC_*` in the command tables.** Every entry carries an `rbac::RBAC_PERM_COMMAND_*` id.
The `SEC_*` column below is derived: `rbac_default_permissions` grants exactly four rows —
`(3,192),(2,193),(1,194),(0,195)` (`data/sql/base/db_auth/rbac_default_permissions.sql:40-43`, verbatim
`(3,192,-1),` / `(2,193,-1),` / `(1,194,-1),` / `(0,195,-1);`) — and those four role permissions are
`192 = 'Role: Sec Level Administrator'`, `193 = Gamemaster`, `194 = Moderator`, `195 = Player`
(`rbac_permissions.sql:88-91`). Each links to a command bundle: `192→196` (`rbac_linked_permissions.sql:46`),
`193→197` (`:52`), `194→198` (`:89`), `195→199` (`:98`), with `196 = 'Role: Administrator Commands'`,
`197 = Gamemaster`, `198 = Moderator`, `199 = Player` (`rbac_permissions.sql:92-95`). So membership of bundle
196/197/198/199 is the effective minimum SEC level, and the roles nest (192⊃193⊃194⊃195, `rbac_linked_permissions.sql:45,51,88`).

**Caveat that outranks the whole table:** the `command` table in `acore_world` overrides the compiled level at load,
with only a warning — `ChatCommand.cpp:118-124`, verbatim `            if (cmd->_invoker && (cmd->_permission.RequiredLevel != secLevel))` /
`            {` / `                LOG_WARN("sql.sql", "Table `command` has permission {} for '{}' which does not match the core ({}). Overriding.",` /
`                    secLevel, name, cmd->_permission.RequiredLevel);` / ` ` / `                cmd->_permission.RequiredLevel = secLevel;` / `            }`.
A dashboard cannot infer a live server's levels from this source read.

**Leading dot: the console/SOAP parser strips it, and also `!`.**
`src/server/game/Chat/Chat.cpp:981-991`:
```
bool CliHandler::ParseCommands(std::string_view str)
{
    if (str.empty())
        return false;

    // Console allows using commands both with and without leading indicator
    if (str[0] == '.' || str[0] == '!')
        str = str.substr(1);

    return _ParseCommands(str);
}
```
(The in-game handler *requires* one: `Chat.cpp:255-256` — `    if ((text[0] != '!') && (text[0] != '.'))` / `        return false;`.)

**Offline targeting**: every handler typed `Optional<PlayerIdentifier>` resolves a bare character NAME through the
character cache when the player is not online — `ChatCommandTags.cpp:122-129`, verbatim
`        if ((_player = ObjectAccessor::FindPlayerByName(_name)))` / `        {` / `            _guid = _player->GetGUID();` / `        }` /
`        else if (!(_guid = sCharacterCache->GetCharacterGuidByName(_name)))` / `        {` /
`            return FormatAcoreString(handler, LANG_CMDPARSER_CHAR_NAME_NO_EXIST, _name);` / `        }`.
It also accepts a raw low GUID (`:97-108`) and a `|Hplayer:|h` hyperlink (`:114-115`). Omitting the argument falls back
to `FromTargetOrSelf`, which from the console yields nothing (`ChatCommandTags.cpp:138-152`, `handler->GetPlayer()` is null),
so **from SOAP the name argument is mandatory** for all of these.

| command (full syntax) | RBAC perm | derived SEC | Console | handler | offline by name? | citation |
|---|---|---|---|---|---|---|
| `.teleport name [#player] $home\|<tele>` | `RBAC_PERM_COMMAND_TELE_NAME` (740) | GAMEMASTER (2) | **Yes** | `HandleTeleNameCommand` | **Yes** — writes the position straight to the DB | table `cs_tele.cpp:46`; handler `cs_tele.cpp:163`; offline branch `cs_tele.cpp:146-156` |
| — the offline write | | | | | | `cs_tele.cpp:156` — `                Player::SavePositionInDB({ mapId, pos }, sMapMgr->GetZoneId(PHASEMASK_NORMAL, { mapId, pos }), player.GetGUID(), nullptr);` |
| — `$home` offline | reads `CHAR_SEL_CHAR_HOMEBIND`, writes back | | | | Yes | `cs_tele.cpp:176-186` |
| `.teleport <tele>` (self) | `RBAC_PERM_COMMAND_TELE` (737) | GAMEMASTER (2) | **No** | `HandleTeleCommand` | n/a | `cs_tele.cpp:54`; handler `cs_tele.cpp:272` |
| `.teleport add <name>` | `..._TELE_ADD` | — | **No** | `HandleTeleAddCommand` | n/a | `cs_tele.cpp:50` |
| `.teleport del <name>` | `..._TELE_DEL` | — | **Yes** | `HandleTeleDelCommand` | n/a | `cs_tele.cpp:51` |
| `.teleport group <tele>` | `..._TELE_GROUP` | — | **No** | `HandleTeleGroupCommand` | no (needs a selected player) | `cs_tele.cpp:53`, `:207-212` |
| `.go xyz <x> <y> [z] [mapId] [o]` | `RBAC_PERM_COMMAND_GO` (377) | GAMEMASTER (2) | **No** | `HandleGoXYZCommand` | n/a — every `.go` subcommand is `Console::No` and teleports *the caller* | `cs_go.cpp:53`; handler `cs_go.cpp:432`; the whole table `cs_go.cpp:43-55` |
| `.send items [#player] "subject" "text" itemid[:count] …` | `..._SEND_ITEMS` (484) | GAMEMASTER (2) | **Yes** | `HandleSendItemsCommand` | **Yes** | `cs_send.cpp:40`; handler `cs_send.cpp:54` |
| — item cap | `MAX_MAIL_ITEMS` = **12** | | | | | `src/server/game/Mails/Mail.h:33` — `#define MAX_MAIL_ITEMS 12`; enforced at `cs_send.cpp:110-114` — `            if (itemList.size() > MAX_MAIL_ITEMS)` |
| — sender when from console | the *recipient's own* guid | | | | | `cs_send.cpp:117-118` — `        // If the message is sent from console, set it as sent by the target itself, like the other Customer Support mails.` / `        ObjectGuid::LowType senderGuid = handler->GetSession() ? handler->GetSession()->GetPlayer()->GetGUID().GetCounter() : target->GetGUID().GetCounter();` |
| `.send mail [#player] "subject" "text"` | `..._SEND_MAIL` (485) | GAMEMASTER (2) | **Yes** | `HandleSendMailCommand` | **Yes** | `cs_send.cpp:41`; handler `cs_send.cpp:141` |
| `.send money [#player] "subject" "text" <money>` | `..._SEND_MONEY` (487) | GAMEMASTER (2) | **Yes** | `HandleSendMoneyCommand` | **Yes** | `cs_send.cpp:43`; handler `cs_send.cpp:192` |
| `.send message [#player] <text>` | `..._SEND_MESSAGE` | — | **Yes** | `HandleSendMessageCommand` | **No** — bails unless connected | `cs_send.cpp:42`; `cs_send.cpp:173` — `        if (!target || !target->IsConnected())` |
| `.additem [#player] <itemid> [count]` | `..._ADDITEM` (488) | GAMEMASTER (2) | **Yes** | `HandleAddItemCommand` | argument parses offline, but the body needs a live `Player` to receive the item — see note | `cs_misc.cpp:130`; handler `cs_misc.cpp:1724` |
| `.additem set <itemsetid>` | `..._ADDITEMSET` | — | **No** | `HandleAddItemSetCommand` | n/a | `cs_misc.cpp:131` |
| `.revive [#player]` | `..._REVIVE` (523) | GAMEMASTER (2) | **Yes** | `HandleReviveCommand` | **Yes** — explicit offline branch | `cs_misc.cpp:115`; `cs_misc.cpp:1217-1221` — `        else` / `        {` / `            CharacterDatabaseTransaction trans(nullptr);` / `            Player::OfflineResurrect(target->GetGUID(), trans);` / `        }` |
| `.character level [#player] <level>` | `..._CHARACTER_LEVEL` (283) | GAMEMASTER (2) | **Yes** | `HandleCharacterLevelCommand` | **Yes** — level read from the cache when offline | `cs_character.cpp:74`; `cs_character.cpp:447` — `        uint8 oldlevel = player->IsConnected() ? player->GetConnectedPlayer()->GetLevel() : sCharacterCache->GetCharacterLevelByGuid(player->GetGUID());`; clamp at `:452-453` to `DEFAULT_MAX_LEVEL` |
| `.levelup [#player] <levels>` | `..._LEVELUP` (287) | GAMEMASTER (2) | **No** | `HandleLevelUpCommand` | (moot — console-blocked) | `cs_character.cpp:83` — `            { "levelup",        HandleLevelUpCommand, rbac::RBAC_PERM_COMMAND_LEVELUP, Console::No },`; handler `:766`, clamp to `STRONG_MAX_LEVEL` at `:780-781` |
| `.modify money <amount>` | `..._MODIFY_MONEY` (554) | GAMEMASTER (2) | **No** | `HandleModifyMoneyCommand` | no — no player argument at all | `cs_modify.cpp:54`; handler `cs_modify.cpp:557` — `    static bool HandleModifyMoneyCommand(ChatHandler* handler, Tail money)` |
| `.character rename [#player] [reserveName] [newName]` | `..._CHARACTER_RENAME` (284) | GAMEMASTER (2) | **Yes** | `HandleCharacterRenameCommand` | **Yes** | `cs_character.cpp:75`; handler `:332` |
| `.character customize [#player]` | `..._CHARACTER_CUSTOMIZE` (274) | GAMEMASTER (2) | **Yes** | `HandleCharacterCustomizeCommand` | **name yes, but no fallback** — uses `FromTarget`, not `FromTargetOrSelf`, so from console the name is required | `cs_character.cpp:67`; `:464,467` — `            player = PlayerIdentifier::FromTarget(handler);` |
| `.character changerace [#player]` | `..._CHARACTER_CHANGERACE` (276) | GAMEMASTER (2) | **Yes** | `HandleCharacterChangeRaceCommand` | same as above | `cs_character.cpp:69`; handler `:512` |
| `.character changefaction [#player]` | `..._CHARACTER_CHANGEFACTION` (275) | GAMEMASTER (2) | **Yes** | `HandleCharacterChangeFactionCommand` | same as above | `cs_character.cpp:68`; handler `:488` |
| `.account create <name> <password> [email]` | `..._ACCOUNT_CREATE` (219) | **ADMINISTRATOR (3)** | **Yes** | `HandleAccountCreateCommand` | n/a | `cs_account.cpp:92`; handler `:280-296`; bundle 196 at `rbac_linked_permissions.sql:106` |
| `.account set password <account> <pw> <pw-confirm>` | `..._ACCOUNT_SET_PASSWORD` (229) | **ADMINISTRATOR (3)** | **Yes** | `HandleAccountSetPasswordCommand` | n/a — **three args, the third is the confirmation** | `cs_account.cpp:59`; handler `:1044`, `:1050-1055` |
| `.account set gmlevel [account] <level> [realmId]` | `..._ACCOUNT_SET_SECLEVEL` (228) | **ADMINISTRATOR (3)** | **Yes** | `HandleAccountSetGmLevelCommand` | n/a — with only two args it needs a *selected player*, so from console give all three | `cs_account.cpp:58`; handler `:925`, `:939-944` — `        if (arg1 && !arg3)` / `        {` / `            if (!handler->getSelectedPlayer())` / `                return false;` |
| `.account delete <account>` | `..._ACCOUNT_DELETE` (220) | **ADMINISTRATOR (3)** | **Yes** | `HandleAccountDeleteCommand` | n/a | `cs_account.cpp:93`; handler `:330` |
| `.account onlinelist` | `..._ACCOUNT_ONLINE_LIST` (224) | **ADMINISTRATOR (3)** | **Yes** | `HandleAccountOnlineListCommand` | n/a | `cs_account.cpp:96`; handler `:381`; queries `CHAR_SEL_CHARACTER_ONLINE` then `LOGIN_SEL_ACCOUNT_INFO` per account (`:384,408`) |
| `.server info` | `..._SERVER_INFO` (725) | **PLAYER (0)** | **Yes** | `HandleServerInfoCommand` | n/a | `cs_server.cpp:89`; bundle 199 at `rbac_linked_permissions.sql:663` |
| `.server set motd [realmId] <locale> <text>` | `..._SERVER_SET_MOTD` (733) | **ADMINISTRATOR (3)** | **Yes** | `HandleServerSetMotdCommand` | n/a — **locale is mandatory and validated** | `cs_server.cpp:77`; handler `:535`, `:546-552` |
| `.reload config` | `..._RELOAD_CONFIG` (630) | **ADMINISTRATOR (3)** | **Yes** | `HandleReloadConfigCommand` | n/a | `cs_reload.cpp:91`; handler `:351-354` — `        sWorld->LoadConfigSettings(true);` |
| `.saveall` | `..._SAVEALL` (524) | **ADMINISTRATOR (3)** | **Yes** | `HandleSaveAllCommand` | n/a — no args | `cs_misc.cpp:123`; handler `:1402-1407` — `        ObjectAccessor::SaveAllPlayers();` |
| `.pinfo [#player]` | `..._PINFO` (517) | **MODERATOR (1)** | **Yes** | `HandlePInfoCommand` | **Yes** | `cs_misc.cpp:135`; handler `:2058`; bundle 198 at `rbac_linked_permissions.sql:619` |
| `.lookup item <namePart>` | `..._LOOKUP_ITEM` (447) | **PLAYER (0)** | **Yes** | `HandleLookupItemCommand` | n/a | `cs_lookup.cpp:53`; handler `:455`; iterates `sObjectMgr->GetItemTemplateStore()` in memory (`:477`), capped by `CONFIG_MAX_RESULTS_LOOKUP_COMMANDS` (`:474`) |

`.server info` prints, in order (`cs_server.cpp:275-294`): full git version; `Connected players: {}. Characters in world: {}.`
(plus `Queue: {}.` when non-zero); `Connection peak: {}.`; the security-limit line; uptime; `Update time diff: {}ms. Last {} diffs summary:`
then `|- Mean:`, `|- Median:`, `|- Percentiles (95, 99, max):`; and a shutdown-time-left line only while shutting down.

Note on `.additem` from the console: the `Optional<PlayerIdentifier>` resolves an offline name, but the handler then
falls through to inventory/mail logic requiring a live `Player`; I did not read past `cs_misc.cpp:1745`, so
**"additem to an offline character" is unproven either way** — see the closing section.

---

## C. mod-playerbots @ b949b50b

### The `.playerbots` command table — complete, verbatim

`src/Script/PlayerbotCommandScript.cpp:24-46`:
```
        static ChatCommandTable playerbotsDebugCommandTable = {
            {"bg", HandleDebugBGCommand, SEC_GAMEMASTER, Console::Yes},
        };

        static ChatCommandTable playerbotsAccountCommandTable = {
            {"setKey", HandleSetSecurityKeyCommand, SEC_PLAYER, Console::No},
            {"link", HandleLinkAccountCommand, SEC_PLAYER, Console::No},
            {"linkedAccounts", HandleViewLinkedAccountsCommand, SEC_PLAYER, Console::No},
            {"unlink", HandleUnlinkAccountCommand, SEC_PLAYER, Console::No},
        };

        static ChatCommandTable playerbotsCommandTable = {
            {"bot", HandlePlayerbotCommand, SEC_PLAYER, Console::No},
            {"gtask", HandleGuildTaskCommand, SEC_GAMEMASTER, Console::Yes},
            {"pmon", HandlePerfMonCommand, SEC_GAMEMASTER, Console::Yes},
            {"rndbot", HandleRandomPlayerbotCommand, SEC_GAMEMASTER, Console::Yes},
            {"debug", playerbotsDebugCommandTable},
            {"account", playerbotsAccountCommandTable},
        };

        static ChatCommandTable commandTable = {
            {"playerbots", playerbotsCommandTable},
        };
```
Unlike the core, this module uses raw `SEC_*` (not RBAC ids), so `ChatCommand.cpp:520` (`_permission.RequiredLevel >= rbac::RBAC_PERM_COMMAND_RBAC`) is false and `IsAvailable` applies — which is still bypassed for console.

| question | answer | citation | verbatim |
|---|---|---|---|
| `bot add`'s handler | `HandlePlayerbotCommand` → `PlayerbotMgr::HandlePlayerbotMgrCommand` → `PlayerbotHolder::HandlePlayerbotCommand` → `ProcessBotCommand("add", …)` → `AddPlayerBot` | `PlayerbotCommandScript.cpp:51-54`; `PlayerbotMgr.cpp:862,897,673,84` | `        return PlayerbotMgr::HandlePlayerbotMgrCommand(handler, args);` |
| the session guard — **still present at this rev** | yes | `src/Bot/PlayerbotMgr.cpp:870-875` | `    WorldSession* m_session = handler->GetSession();` / `    if (!m_session)` / `    {` / `        handler->PSendSysMessage("You may only add bots from an active session");` / `        return false;` / `    }` |
| second gate right after it | yes | `PlayerbotMgr.cpp:877-883` | `    Player* player = m_session->GetPlayer();` / `    PlayerbotMgr* mgr = GET_PLAYERBOT_MGR(player);` / `    if (!mgr)` / `    {` / `        handler->PSendSysMessage("You cannot control bots yet");` / `        return false;` / `    }` |
| and the table entry itself | `Console::No` | `PlayerbotCommandScript.cpp:36` | `            {"bot", HandlePlayerbotCommand, SEC_PLAYER, Console::No},` |
| `bot` subcommands (usage string) | `list/reload/tweak/self`, `add/addaccount/init/remove PLAYERNAME`, `addclass CLASSNAME [male\|female\|0\|1]` | `PlayerbotMgr.cpp:903-904` | `        messages.push_back("usage: list/reload/tweak/self or add/addaccount/init/remove PLAYERNAME\n");` / `        messages.push_back("usage: addclass CLASSNAME [male\|female\|0\|1]");` |
| `add` aliases | `add`, `addaccount`, `login` | `PlayerbotMgr.cpp:683` | `    if (cmd == "add" \|\| cmd == "addaccount" \|\| cmd == "login")` |
| `remove` aliases | `remove`, `logout`, `rm` | `PlayerbotMgr.cpp:707` | `    else if (cmd == "remove" \|\| cmd == "logout" \|\| cmd == "rm")` |
| `account link` / `unlink` | `.playerbots account link <accountName> <securityKey>` and `.playerbots account unlink <accountName>`, both `SEC_PLAYER`, **`Console::No`**, and both dereference `handler->GetSession()->GetPlayer()` unguarded | `PlayerbotCommandScript.cpp:30,32,139,143,183,187` | `            handler->PSendSysMessage("Usage: .playerbots account link <accountName> <securityKey>");` … `        Player* player = handler->GetSession()->GetPlayer();` … `            handler->PSendSysMessage("Usage: .playerbots account unlink <accountName>");` |
| any console-usable command that puts a bot in a NAMED player's party? | **No.** `grep -rn spoof src/` returns nothing. The only party-forming path is `AddPlayerBot(guid, masterAccountId)`, and the master is resolved from a **live session**, never from a name. | `PlayerbotMgr.cpp:98-99` | `    WorldSession* masterSession = masterAccountId ? sWorldSessionMgr->FindSession(masterAccountId) : nullptr;` / `    Player* masterPlayer = masterSession ? masterSession->GetPlayer() : nullptr;` |
| — and `masterAccountId == 0` means "random bot", not "party of X" | | `PlayerbotMgr.cpp:101` | `    bool isRndbot = !masterAccountId;` |
| — the ownership check | | `PlayerbotMgr.cpp:112-116` | `    if (!isRndbot && !sameAccount && !sameGuild && !addClassBot && !linkedAccount)` / `    {` / `        allowed = false;` / `        out << "Failure: You are not allowed to control bot " << botName.c_str();` / `    }` |
| — the per-master cap | `maxAddedBots` | `PlayerbotMgr.cpp:131-136` | `        uint32 count = mgr->GetPlayerbotsCount() + loadingForMaster;` / `        if (count >= uint32(PlayerbotAIConfig::instance().maxAddedBots))` |
| `.playerbots rndbot <cmd>` — the console one | `SEC_GAMEMASTER`, `Console::Yes`. Handler ignores the `ChatHandler` entirely and logs to `LOG_INFO`/`LOG_ERROR` — **so its output never reaches a SOAP caller**. | `RandomPlayerbotMgr.cpp:2377` | `bool RandomPlayerbotMgr::HandlePlayerbotConsoleCommand(ChatHandler* /*handler*/, char const* args)` |
| — direct subcommands | `reset`, `stats`, `reload`, `update` | `RandomPlayerbotMgr.cpp:2393,2401,2408,2414` | `    if (cmd == "reset")` … `    if (cmd == "stats")` … `    if (cmd == "reload")` … `    if (cmd == "update")` |
| — per-bot handlers | `init`, `clear`, `levelup`/`level`, `refresh`, `teleport`, `revive`, `grind`, `change_strategy` | `RandomPlayerbotMgr.cpp:2422-2430` | `    handlers["init"] = &RandomPlayerbotMgr::RandomizeFirst;` / `    handlers["clear"] = &RandomPlayerbotMgr::Clear;` / `    handlers["levelup"] = handlers["level"] = &RandomPlayerbotMgr::IncreaseLevel;` / `    handlers["refresh"] = &RandomPlayerbotMgr::Refresh;` / `    handlers["teleport"] = &RandomPlayerbotMgr::RandomTeleportForLevel;` / … / `    handlers["revive"] = &RandomPlayerbotMgr::Revive;` / `    handlers["grind"] = &RandomPlayerbotMgr::RandomTeleport;` / `    handlers["change_strategy"] = &RandomPlayerbotMgr::ChangeStrategy;` |
| — **the usage string lies**: it advertises `add`/`remove`, which are not in the handler map | | usage `RandomPlayerbotMgr.cpp:2387` vs map `:2420-2431` | `        LOG_ERROR("playerbots", "Usage: rndbot stats/update/reset/init/refresh/add/remove");` |
| — those handlers only touch bots that are **already online**, and only random bots | | `RandomPlayerbotMgr.cpp:2454-2460,2467-2471` | `                    if (!sRandomPlayerbotMgr.IsRandomBot(guid.GetCounter()))` / `                    {` / `                        continue;` / `                    }` / `                    Player* bot = ObjectAccessor::FindPlayer(guid);` / `                    if (!bot)` / `                        continue;` … `        if (botIds.empty())` / `        {` / `            LOG_INFO("playerbots", "Nothing to do");` / `            return false;` / `        }` |
| — the name argument is a SQL `LIKE` pattern, defaulting to `%` (all bots) | | `RandomPlayerbotMgr.cpp:2438,2445-2446` | `        std::string const name = cmd.size() > prefix.size() + 1 ? cmd.substr(1 + prefix.size()) : "%";` … `            if (QueryResult results = CharacterDatabase.Query(` / `                    "SELECT guid FROM characters WHERE account = {} AND name like '{}'", account, name.c_str()))` |
| `PlayerbotCommandServer` | a raw line-oriented TCP server, **no authentication of any kind**, bound to `tcp::v4()` = `0.0.0.0` | `src/Bot/Cmd/PlayerbotCommandServer.cpp:62,47-51` | `    tcp::acceptor a(io_service, tcp::endpoint(tcp::v4(), port));` ; `        while (ReadLine(sock, &buffer, &request))` / `        {` / `            std::string const response = RandomPlayerbotMgr::instance().HandleRemoteCommand(request) + "\n";` |
| — what a line must look like | `<command>,<bot low GUID>` | `RandomPlayerbotMgr.cpp:2981-2999` | `    std::string::const_iterator pos = std::find(request.begin(), request.end(), ',');` … `    std::string const command = std::string(request.begin(), pos);` / `    ObjectGuid guid = ObjectGuid::Create<HighGuid::Player>(atoi(std::string(pos + 1, request.end()).c_str()));` / `    Player* bot = GetPlayerBot(guid);` / `    if (!bot)` / `        return "invalid guid";` … `    return botAI->HandleRemoteCommand(command);` |
| — it runs on its **own** detached thread, not the world thread | | `PlayerbotCommandServer.cpp:67,96-97` | `        boost::thread t(boost::bind(session, sock));` ; `    std::thread serverThread(Run);` / `    serverThread.detach();` |
| — disabled when the port is 0 | | `PlayerbotCommandServer.cpp:73-76` | `    if (!sPlayerbotAIConfig.commandServerPort)` / `    {` / `        return;` / `    }` |

### Config defaults (conf.dist value, then the compiled fallback)

| key | `conf/playerbots.conf.dist` | line | compiled fallback in `src/PlayerbotAIConfig.cpp` | line |
|---|---|---|---|---|
| `AiPlayerbot.Enabled` | `AiPlayerbot.Enabled = 1` | 83 | `true` | 83 |
| `AiPlayerbot.CommandServerPort` | `AiPlayerbot.CommandServerPort = 8888` | 2517 | `8888` | 468 |
| `AiPlayerbot.BotAutologin` | `AiPlayerbot.BotAutologin = 0` | 152 | `false` | 243 |
| `AiPlayerbot.RandomBotAutologin` | `AiPlayerbot.RandomBotAutologin = 1` | 86 | `true` | 244 |
| `AiPlayerbot.RandomBotAccountPrefix` | `AiPlayerbot.RandomBotAccountPrefix = "rndbot"` | 106 | `"rndbot"` | 576 |
| `AiPlayerbot.RandomBotAccountCount` | `AiPlayerbot.RandomBotAccountCount = 0` (0 = auto-calculate) | 97 | `0` | 577 |
| `AiPlayerbot.MinRandomBots` | `AiPlayerbot.MinRandomBots = 500` | 89 | `500` | 245 |
| `AiPlayerbot.MaxRandomBots` | `AiPlayerbot.MaxRandomBots = 500` | 90 | `500` | 246 |
| `AiPlayerbot.MaxAddedBots` | `AiPlayerbot.MaxAddedBots = 40` | 136 | `40` | 644 |
| `AiPlayerbot.AllowGuildBots` | `AiPlayerbot.AllowGuildBots = 1` | 158 | `true` | 185 |
| `AiPlayerbot.AllowAccountBots` | `AiPlayerbot.AllowAccountBots = 1` | 155 | `true` | 184 |
| **`AiPlayerbot.AllowMultiAccountAltBots`** | **does not exist at this rev.** The nearest key is `AiPlayerbot.AllowTrustedAccountBots = 1` | 161 | `true` | 186 |
| `AiPlayerbot.PremadeSpecName.<class>.<specno>` | documented, then ~20 classes' worth of concrete values, e.g. `AiPlayerbot.PremadeSpecName.1.0 = arms pve` | 1764, 1778 | — | — |
| `AiPlayerbot.PremadeSpecLink.<class>.<specno>.<level>` | e.g. `AiPlayerbot.PremadeSpecLink.1.0.60 = 3022032023335100002012211231241` | 1765, 1780 | — | — |
| `AiPlayerbot.PremadeSpecGlyph.<class>.<specno>` | e.g. `AiPlayerbot.PremadeSpecGlyph.1.0 = 43418,43395,43423,43399,43397,43421` | 1766, 1779 | — | — |
| — ranges | `# 0 <= specno < 20, 1 <= level <= 80` | 1768 | — | — |

Note the catalog sets `AC_AI_PLAYERBOT_MIN/MAX_RANDOM_BOTS=500`, which at this rev **equals** the shipped conf default.
The env var is therefore currently redundant but not wrong; if the module's default moves, only the env var holds the number.

### Bot-account marker tables in `acore_playerbots`

`data/sql/playerbots/base/playerbots_account_type.sql:1-7`:
```
DROP TABLE IF EXISTS `playerbots_account_type`;
CREATE TABLE `playerbots_account_type` (
    `account_id` int unsigned NOT NULL,
    `account_type` tinyint unsigned NOT NULL DEFAULT 0 COMMENT '0 = unassigned, 1 = RNDbot, 2 = AddClass',
    `assignment_date` timestamp NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (`account_id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci COMMENT='Playerbot account type assignments';
```

`data/sql/playerbots/base/playerbots_random_bots.sql:1-15`:
```
DROP TABLE IF EXISTS `playerbots_random_bots`;
CREATE TABLE `playerbots_random_bots` (
  `id` INT(11) NOT NULL AUTO_INCREMENT,
  `owner` INT(11) NOT NULL,
  `bot` INT(11) NOT NULL,
  `time` INT(11) NOT NULL,
  `validIn` INT(11) DEFAULT NULL,
  `event` varchar(45) DEFAULT NULL,
  `value` INT(11) DEFAULT NULL,
  `data` varchar(255) DEFAULT NULL,
  PRIMARY KEY (`id`),
  KEY `owner` (`owner`),
  KEY `bot` (`bot`),
  KEY `event` (`event`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8;
```
This is the **event store**, not a roster: one row per (owner, bot, event) with a TTL (`validIn`). `.playerbots rndbot reset`
deletes from it (`RandomPlayerbotMgr.cpp:2395`, `PLAYERBOTS_DEL_RANDOM_BOTS`).

`data/sql/playerbots/base/playerbots_account_links.sql:1-9`:
```
DROP TABLE IF EXISTS `playerbots_account_links`;

CREATE TABLE `playerbots_account_links` (
    `id` INT AUTO_INCREMENT PRIMARY KEY,
    `account_id` INT NOT NULL,
    `linked_account_id` INT NOT NULL,
    `created_at` TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    UNIQUE KEY `account_link` (`account_id`, `linked_account_id`)
) ENGINE=INNODB DEFAULT CHARSET=latin1;
```
(Note the charset inconsistency: `latin1` here, `utf8` in `playerbots_random_bots`, `utf8mb4` in `playerbots_account_type`.)

### `AddRandomBots` preconditions (brief trace)

`RandomPlayerbotMgr.cpp:638-720`. Gate: `currentBots.size() < GetEventValue(0, "bot_count")` (`:640,643`); batch size clamped
by `sPlayerbotAIConfig.randomBotsPerInterval` (`:647`); faction split by `randomBotAllianceRatio`/`randomBotHordeRatio` (`:653-662`);
candidate accounts are `rndBotTypeAccounts` — i.e. only accounts marked type 1 in `playerbots_account_type` — optionally
sub-sampled when `enablePeriodicOnlineOffline` (`:666-686`); candidate characters come from `CHAR_SEL_CHARS_BY_ACCOUNT_ID`
per account, then shuffled for class balance (`:698-720`).

### Gearing / spec'ing a bot from outside a session

| question | answer | citation |
|---|---|---|
| `autogear` | a **bot chat action**, reached only through the AI's chat-command pipeline; never a `ChatCommand`. Referenced as a user-typed party command in a comment: `// Optional explicit ilvl override: '/p autogear bis 55'.` | `src/Ai/Base/Actions/TrainerAction.cpp:333` |
| `talents autopick` / `talents spec` | same — an `Action` named `"talents"`, registered in the chat-action context | `src/Ai/Base/Actions/ChangeTalentsAction.h:18` (`ChangeTalentsAction(PlayerbotAI* botAI, std::string const name = "talents")`); `src/Ai/Base/ChatActionContext.h:134`; usage string `src/Ai/Base/Actions/ChangeTalentsAction.cpp:101` — `    out << "Talents usage: talents switch <1/2>, talents autopick, talents spec list, "` |
| whisper **only**? | **No — whisper, party/raid, guild and some channels**, but in every case from a live `Player*` | `src/Script/Playerbots.cpp:174-197` (whisper), `:199-217` (group), `:219-243` (guild), `:245-255` (channel) |
| can any of it come from the console/SOAP? | **No.** The entry point is `PlayerbotAI::HandleCommand(uint32 type, std::string const& text, Player& fromPlayer, uint32 lang)` — a `Player&`, not a handler. | `src/Bot/PlayerbotAI.cpp:600` |
| addon traffic is explicitly rejected | | `PlayerbotAI.cpp:611-612,616-617` — `    if (type == CHAT_MSG_ADDON)` / `        return;` … `    else if (lang == LANG_ADDON)  // Other addon messages should not command bots.` / `        return;` |
| the one gearing path that *is* a chat command | `.playerbots bot initself[=uncommon\|rare\|epic\|legendary\|<gs>]` gears the **caller**, GM-only, and is `Console::No` | `PlayerbotMgr.cpp:918-1010`; e.g. `:920` — `        if (master->CanBeGameMaster())` |

---

## D. Schema facts (repo: core @ 413bea61)

`acore_world.item_template` — `data/sql/base/db_world/item_template.sql`:

| column | line | verbatim |
|---|---|---|
| — | 23 | `CREATE TABLE `item_template` (` |
| `entry` | 24 | `  `entry` int unsigned NOT NULL DEFAULT '0',` |
| `class` | 25 | `  `class` tinyint unsigned NOT NULL DEFAULT '0',` |
| `subclass` | 26 | `  `subclass` tinyint unsigned NOT NULL DEFAULT '0',` |
| `name` | 28 | `  `name` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci NOT NULL DEFAULT '',` |
| `displayid` | 29 | `  `displayid` int unsigned NOT NULL DEFAULT '0',` |
| `Quality` | 30 | `  `Quality` tinyint unsigned NOT NULL DEFAULT '0',` |
| `InventoryType` | 36 | `  `InventoryType` tinyint unsigned NOT NULL DEFAULT '0',` |
| `ItemLevel` | 39 | `  `ItemLevel` smallint unsigned NOT NULL DEFAULT '0',` |
| `RequiredLevel` | 40 | `  `RequiredLevel` tinyint unsigned NOT NULL DEFAULT '0',` |

`acore_world.game_tele` — `data/sql/base/db_world/game_tele.sql:23-32`, verbatim:
```
CREATE TABLE `game_tele` (
  `id` int unsigned NOT NULL,
  `position_x` float NOT NULL DEFAULT '0',
  `position_y` float NOT NULL DEFAULT '0',
  `position_z` float NOT NULL DEFAULT '0',
  `orientation` float NOT NULL DEFAULT '0',
  `map` smallint unsigned NOT NULL DEFAULT '0',
  `name` varchar(100) CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci NOT NULL DEFAULT '',
  PRIMARY KEY (`id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci COMMENT='Tele Command';
```

`acore_characters.characters` — `data/sql/base/db_characters/characters.sql`:

| column | line | verbatim |
|---|---|---|
| `guid` | 24 | `  `guid` int unsigned NOT NULL DEFAULT '0' COMMENT 'Global Unique Identifier',` |
| `account` | 25 | `  `account` int unsigned NOT NULL DEFAULT '0' COMMENT 'Account Identifier',` |
| `name` | 26 | `  `name` varchar(12) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL,` |
| `level` | 30 | `  `level` tinyint unsigned NOT NULL DEFAULT '0',` |
| `position_x` | 41 | `  `position_x` float NOT NULL DEFAULT '0',` |
| `position_y` | 42 | `  `position_y` float NOT NULL DEFAULT '0',` |
| `position_z` | 43 | `  `position_z` float NOT NULL DEFAULT '0',` |
| `map` | 44 | `  `map` smallint unsigned NOT NULL DEFAULT '0' COMMENT 'Map Identifier',` |
| `online` | 49 | `  `online` tinyint unsigned NOT NULL DEFAULT '0',` |
| `at_login` | 65 | `  `at_login` smallint unsigned NOT NULL DEFAULT '0',` |
| indexes | 104-107 | `  PRIMARY KEY (`guid`),` / `  KEY `idx_account` (`account`),` / `  KEY `idx_online` (`online`),` / `  KEY `idx_name` (`name`)` |

`character_inventory` — `data/sql/base/db_characters/character_inventory.sql:23-31`:
```
CREATE TABLE `character_inventory` (
  `guid` int unsigned NOT NULL DEFAULT '0' COMMENT 'Global Unique Identifier',
  `bag` int unsigned NOT NULL DEFAULT '0',
  `slot` tinyint unsigned NOT NULL DEFAULT '0',
  `item` int unsigned NOT NULL DEFAULT '0' COMMENT 'Item Global Unique Identifier',
  PRIMARY KEY (`item`),
  UNIQUE KEY `guid` (`guid`,`bag`,`slot`),
  KEY `idx_guid` (`guid`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci COMMENT='Player System';
```

`item_instance` — `data/sql/base/db_characters/item_instance.sql:23-40`; the join columns are
`  `guid` int unsigned NOT NULL DEFAULT '0',` (:24), `  `itemEntry` int unsigned DEFAULT '0',` (:25),
`  `owner_guid` int unsigned NOT NULL DEFAULT '0',` (:26), `  `count` int unsigned NOT NULL DEFAULT '1',` (:29),
`  `enchantments` text … NOT NULL,` (:33), `  `randomPropertyId` smallint NOT NULL DEFAULT '0',` (:34),
`  `durability` smallint unsigned NOT NULL DEFAULT '0',` (:35), and `  KEY `idx_owner_guid` (`owner_guid`)` (:39).

**The equipped-gear join** is
`character_inventory ci JOIN item_instance ii ON ii.guid = ci.item JOIN item_template it ON it.entry = ii.itemEntry`
filtered to `ci.bag = 0 AND ci.slot < 19`, because equipped slots are bag 0 and
`src/server/game/Entities/Player/Player.h:681` — `    EQUIPMENT_SLOT_END          = 19`.
(The `< 19` bound is my derivation from that constant; I did not find a query in the tree that spells the filter.)

`character_talent` — `data/sql/base/db_characters/character_talent.sql:23-28`:
```
CREATE TABLE `character_talent` (
  `guid` int unsigned NOT NULL,
  `spell` int unsigned NOT NULL,
  `specMask` tinyint unsigned NOT NULL DEFAULT '0',
  PRIMARY KEY (`guid`,`spell`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
```

`character_achievement` — `data/sql/base/db_characters/character_achievement.sql:23-28`:
```
CREATE TABLE `character_achievement` (
  `guid` int unsigned NOT NULL,
  `achievement` smallint unsigned NOT NULL,
  `date` int unsigned NOT NULL DEFAULT '0',
  PRIMARY KEY (`guid`,`achievement`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
```

`acore_auth.account` — `data/sql/base/db_auth/account.sql`:

| column | line | verbatim |
|---|---|---|
| `id` | 24 | `  `id` int unsigned NOT NULL AUTO_INCREMENT COMMENT 'Identifier',` |
| `username` | 25 | `  `username` varchar(32) CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci NOT NULL DEFAULT '',` |
| `salt` | 26 | `  `salt` binary(32) NOT NULL,` |
| `verifier` | 27 | `  `verifier` binary(32) NOT NULL,` |
| `last_ip` | 33 | `  `last_ip` varchar(15) CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci NOT NULL DEFAULT '127.0.0.1',` |
| `online` | 39 | `  `online` int unsigned NOT NULL DEFAULT '0',` |
| `expansion` | 40 | `  `expansion` tinyint unsigned NOT NULL DEFAULT '2',` |

There is **no `sha_pass_hash`** column at this rev — auth is SRP6 `salt`/`verifier` only. `last_ip` is `varchar(15)`,
so it cannot hold an IPv6 address.

`account_access` — `data/sql/base/db_auth/account_access.sql:23-29`:
```
CREATE TABLE `account_access` (
  `id` int unsigned NOT NULL,
  `gmlevel` tinyint unsigned NOT NULL,
  `RealmID` int NOT NULL DEFAULT '-1',
  `comment` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci DEFAULT '',
  PRIMARY KEY (`id`,`RealmID`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
```
A GM row is `(id, gmlevel, -1)`; RA additionally *requires* `RealmID = -1` (`RASession.cpp:146-149`), SOAP does not care
(it uses the realm-agnostic `LOGIN_GET_ACCOUNT_ACCESS_GMLEVEL`).

`realmlist` — `data/sql/base/db_auth/realmlist.sql:23-39`; notable columns
`  `address` varchar(255) … NOT NULL DEFAULT '127.0.0.1',` (:26),
`  `localAddress` … DEFAULT '127.0.0.1',` (:27), `  `localSubnetMask` … DEFAULT '255.255.255.0',` (:28),
`  `port` smallint unsigned NOT NULL DEFAULT '8085',` (:29),
`  `allowedSecurityLevel` tinyint unsigned NOT NULL DEFAULT '0',` (:33),
`  `gamebuild` int unsigned NOT NULL DEFAULT '12340',` (:35).

`uptime` — `data/sql/base/db_auth/uptime.sql:23-30`:
```
CREATE TABLE `uptime` (
  `realmid` int unsigned NOT NULL,
  `starttime` int unsigned NOT NULL DEFAULT '0',
  `uptime` int unsigned NOT NULL DEFAULT '0',
  `maxplayers` smallint unsigned NOT NULL DEFAULT '0',
  `revision` varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci NOT NULL DEFAULT 'AzerothCore',
  PRIMARY KEY (`realmid`,`starttime`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci COMMENT='Uptime system';
```
`uptime` lives in **`acore_auth`**, not `acore_characters`.

---

## E. Eluna / mod-ale / Lua

| question | answer | citation |
|---|---|---|
| does the pinned core ship an Eluna/ALE engine? | **No.** `grep -rln 'Eluna\|LuaEngine\|ALE\.'` over `core/src` (`.cpp`/`.h`) returns **zero files**. `core/modules/` holds only the module-loader scaffolding (`CMakeLists.txt`, `create_module.sh`, `how_to_make_a_module.md`, `ModulesLoader.cpp.in.cmake`, `ModulesPCH.h`, `ModulesScriptLoader.h`). | core @ 413bea61 |
| so ALE is a separate module | yes | `pylauncher/manifests/wow-wotlk/modules/mod-ale.json` |
| which repo/branch does Yu'lon name? | `azerothcore/mod-ale`, **no branch, no rev** | `mod-ale.json:9-11` — `  "source": {` / `    "repo": "azerothcore/mod-ale"` / `  },` |
| `ALE.ScriptPath` default in `conf/mod_ale.conf.dist` | `"lua_scripts"` — a **relative** path | mod-ale @ c3de7942 `conf/mod_ale.conf.dist:65` — `ALE.ScriptPath = "lua_scripts"` |
| how does it resolve? | it is handed to boost.filesystem as-is, so it resolves against the **process CWD**; the only rewriting is `~` → `$HOME` on non-Windows | `src/LuaEngine/LuaEngine.cpp:120,124-129,135` — `    lua_folderpath = ALEConfig::GetInstance().GetScriptPath();` … `#ifndef ALE_WINDOWS` / `    if (lua_folderpath[0] == '~')` / `        if (const char* home = getenv("HOME"))` / `            lua_folderpath.replace(0, 1, home);` / `#endif` / `    ALE_LOG_INFO("[ALE]: Searching scripts from `{}`", lua_folderpath);` … `    GetScripts(lua_folderpath);` |
| Yu'lon overrides it to an absolute container path, twice | conf key default **and** a `when: configure` regex patch | `mod-ale.json:17-19` (`"key": "ALE.ScriptPath"`, `"default": "\"/azerothcore/env/dist/etc/modules/lua_scripts\""`, note `"container-visible path; quoted; always re-applied on configure"`) and `:29-35` (`"replace": "ALE.ScriptPath = \"/azerothcore/env/dist/etc/modules/lua_scripts\""`, `"note": "wow-manage.sh 2386-2436; also mkdir env/dist/etc/modules/lua_scripts"`) |
| `ALE.Enabled`: conf comment vs compiled default | **they disagree.** The dist file says default `true` and sets `true`; the compiled fallback is the string `"false"`. So a missing conf file means ALE is OFF. | conf `mod_ale.conf.dist:8` — `#       Default:    true  - (enabled)`; `:63` — `ALE.Enabled = true`; code `src/LuaEngine/ALEConfig.cpp:20` — `    SetConfigValue<bool>(ALEConfigValues::ENABLED,                    "ALE.Enabled",            "false");` |
| **Yu'lon's manifest patches a key that does not exist** | the manifest lists `ALE.EnableLuaEngine` as a managed conf key; the shipped `mod_ale.conf.dist` has no such key (its keys are `ALE.Enabled`, `TraceBack`, `ScriptPath`, `PlayerAnnounceReload`, `RequirePaths`, `RequireCPaths`, `AutoReload`, `AutoReloadInterval`, `BytecodeCache`) | `mod-ale.json:21-23` vs `mod_ale.conf.dist:63-71` |
| is there a `.reload ale` command? | **yes, but it is not a `ChatCommand`.** ALE registers no `ChatCommandTable` at all (`grep -rn 'ChatCommandTable\|Console::' mod-ale/src` → nothing). It hooks the core's `OnTryExecuteCommand` and string-matches the text. | `src/ALE_SC.cpp:370-382` — `    ALE_CommandSC() : CommandSC("ALE_CommandSC", {` / `        ALLCOMMANDHOOK_ON_TRY_EXECUTE_COMMAND` / `    }) { }` / … / `        if (!sALE->OnCommand(handler, std::string(cmdStr).c_str()))` |
| — the match, and its console behaviour | prefix match on the lowercased text; console (`player == nullptr`) is allowed, otherwise `SEC_ADMINISTRATOR` | `src/LuaEngine/hooks/PlayerHooks.cpp:44-54` — `    Player* player = handler.IsConsole() ? nullptr : handler.GetSession()->GetPlayer();` / `    // If from console, player is NULL` / `    if (!player \|\| player->GetSession()->GetSecurity() >= SEC_ADMINISTRATOR)` / `    {` / `        std::string reload = text;` / `        std::transform(reload.begin(), reload.end(), reload.begin(), ::tolower);` / `        if (reload.find("reload ale") == 0)` / `        {` / `            ReloadALE();` / `            return false;` / `        }` / `    }` |
| — so from SOAP the string is `reload ale` | the core hook fires inside `TryExecuteCommand` (`core/src/server/game/Chat/ChatCommands/ChatCommand.cpp:326` — `    if (!sScriptMgr->OnTryExecuteCommand(handler, cmdStr))`), which the CLI handler reaches after stripping the leading `.` | — |
| — `reload eluna` | not present | — |
| `CHAT_MSG_ADDON` handling in the core | it is only listed among the chat types exempted from the "2 hours played" anti-spam gate; there is no addon-message routing beyond normal chat delivery | `core/src/server/game/Handlers/ChatHandler.cpp:128-145` — `    // pussywizard: chatting on most chat types requires 2 hours played to prevent spam/abuse` / `    if (!HasPermission(rbac::RBAC_PERM_SKIP_CHECK_CHAT_CHANNEL_REQ))` / `    {` / `        switch (type)` / `        {` / `            case CHAT_MSG_ADDON:` … |
| — the other core reference | a default parameter for creature text, unrelated to client addons | `core/src/server/game/Texts/CreatureTextMgr.h:108`, `CreatureTextMgr.cpp:257,312` |
| `SavedVariables` | **nothing.** `grep -rn SavedVariables core/src` returns zero hits. (Expected — that is a client-side WTF concept.) | core @ 413bea61 |

---

## F. Threading — all three paths converge on the world thread

| path | queues? | citation | verbatim |
|---|---|---|---|
| local console (`CliRunnable`) | yes | `src/server/apps/worldserver/CommandLine/CliRunnable.cpp:231` | `            sWorld->QueueCliCommand(new CliCommandHolder(nullptr, command.c_str(), &utf8print, &commandFinished));` |
| RA (telnet) | yes, and waits | `src/server/apps/worldserver/RemoteAccess/RASession.cpp:200-204` | `    CliCommandHolder* cmd = new CliCommandHolder(this, command.c_str(), &RASession::CommandPrint, &RASession::CommandFinished);` / `    sWorld->QueueCliCommand(cmd);` / ` ` / `    // Wait for the command to finish` / `    _commandExecuting->get_future().wait();` |
| SOAP | yes, and waits (with shutdown escape) | `src/server/apps/worldserver/ACSoap/ACSoap.cpp:121,125-130` | see section A |
| the queue | a single `_cliCmdQueue` on `World` | `src/server/game/World/World.h:221` | `    void QueueCliCommand(CliCommandHolder* commandHolder) override { _cliCmdQueue.add(commandHolder); }` |
| drained on the world thread | yes, once per `World::Update` | `src/server/game/World/World.cpp:1341` | `        ProcessCliCommands();` |
| the drain loop | constructs a fresh `CliHandler` per command, parses, then fires the completion callback | `World.cpp:1641-1657` | `void World::ProcessCliCommands()` / `{` / … / `    while (_cliCmdQueue.next(command))` / `    {` / `        LOG_DEBUG("server.worldserver", "CLI command under processing...");` / `        zprint = command->m_print;` / `        callbackArg = command->m_callbackArg;` / `        CliHandler handler(callbackArg, zprint);` / `        handler.ParseCommands(command->m_command);` / `        if (command->m_commandFinished)` / `            command->m_commandFinished(callbackArg, !handler.HasSentErrorMessage());` / `        delete command;` / `    }` / `}` |

**The one exception:** `PlayerbotCommandServer` (section C) does **not** queue. It runs `HandleRemoteCommand` on a detached
boost thread and touches `ObjectAccessor`/`PlayerbotAI` directly (`PlayerbotCommandServer.cpp:49,67`;
`RandomPlayerbotMgr.cpp:2991-2999`). It is off the world thread, unauthenticated, and bound to `0.0.0.0:8888` by default.

---

## Facts I could not establish, and why

1. **`.additem` against an offline character.** The parameter type resolves an offline name (`ChatCommandTags.cpp:122-129`),
   but I read `cs_misc.cpp` only to line 1745; whether the body mails/stores the item or bails without a live `Player`
   is unread. Needs `cs_misc.cpp:1745-1820`.
2. **The live SEC level of any command.** Section B's SEC column is derived from the shipped `rbac_default_permissions` /
   `rbac_linked_permissions` seed data. `acore_world.command` overrides `RequiredLevel` at load with only a `LOG_WARN`
   (`ChatCommand.cpp:118-124`), and rows in `rbac_account_permissions` change it per account. A running server's levels
   are a database question, not a source question. **Judgement call, flagged, not made.**
3. **`AiPlayerbot.AllowMultiAccountAltBots`.** Does not exist at rev b949b50b. Whether the brief meant
   `AllowTrustedAccountBots` (line 161, default 1) or `AllowAccountBots` (line 155, default 1) is a judgement I did not make.
4. **mod-ale is unpinned, so section E has no revision guarantee.** `mod-ale.json` names only `azerothcore/mod-ale`.
   I read HEAD `c3de79426b03b02d2762536d727f8d40b0f8f24a`, committed 2026-09-06T09:07:30+02:00 — the same day.
   A different day's install gets different code. Pinning it is a decision, not a fact; not made here.
5. **`ALE.ScriptPath` relative resolution.** I established that boost.filesystem receives the string unmodified
   (`LuaEngine.cpp:120,135`) and therefore resolves it against the process CWD. I did **not** measure what the
   worldserver's CWD is inside Yu'lon's container, so the concrete failure mode of the shipped `"lua_scripts"` default
   is untested. Yu'lon overrides it to an absolute path anyway.
6. **The equipped-gear `slot < 19` filter** is my derivation from `EQUIPMENT_SLOT_END = 19` (`Player.h:681`).
   I did not find a query in the tree that spells that predicate, so it is inference, not a citation.
7. **`AC_SOAP_*` from a *user-edited* `.env`.** `base.yml.tmpl:264` publishes `${DOCKER_SOAP_EXTERNAL_PORT:-127.0.0.1:7878}:7878`,
   and the comment at `:259-260` says "a later SOAP setup writes the WHOLE host binding into .env". No such writer exists
   in the tree today: `grep -rn 'DOCKER_SOAP_EXTERNAL_PORT' pylauncher/ --include=*.py` returns exactly two hits, both
   assertions in tests (`pylauncher/tests/test_composegen.py:254`, `pylauncher/tests/test_compose_fixture.py:314`).
   Whether that setup is planned or abandoned is not answerable from source.
8. **Nothing was compiled or run.** No docker, no server, no client. Every "default" above is a source or dist-file
   default, not an observed runtime value.
