# T26 measure — what mod-playerbots accepts for adding a NAMED existing character as a bot
### yulon-ubuntu2, 2026-09-10, 19:33:50–19:47:46 CEST (the box's own clock, read by `date` inside each step script — `lib.sh:13`)

**This is a measuring record, not a gate and not a spec.** It answers the three questions
`pyplan/tickets/T26-my-party-adds-a-named-character-as-a-bot.md` puts before the spec. It is written
outside the repo on purpose (the brief), and **no tracked file was changed and nothing on the box was
written**: no conf, no Lua script, no account, no character, no guild, no group, no SQL write, no
restart of the world, the authserver or the database. Section 7 reads the box back against section 0's
own numbers.

**Times** are the box's clock, taken by the script that wrote the line. **Every `docker logs` window**
in steps 04 and 05 was opened with `date -u +%Y-%m-%dT%H:%M:%SZ` (trailing `Z`) and proved empty of the
pattern before the press — the T3 trap of 2026-09-09, memory note `docker-logs-since-is-client-local`.
Every box action was announced first in the box's Claude activity terminal (`lib.sh:15`, called at the
top and bottom of every step); the terminal was launched at 19:32 and there is exactly one.

The box answers to `ssh yulon-ubuntu2`; its own `hostname` is `yulon-ubuntu`, which is what every
capture header prints. One box, two names.

## 0. What was pressed, in order

| # | at | what | capture |
|---|---|---|---|
| 01 | 19:33:50–19:33:52 | the box before anything | `01-ground.log` |
| 02 | 19:37:59 | the module's own source at `b949b50b` | `02-source.log` |
| 03 | 19:38:49 | the conf as shipped, and the env that could override it | `03-conf.log` |
| 04 | 19:41:43–19:42:28 | **`bot add` over the app's SOAP seam**, four spellings and three cases | `04-soap.log` |
| 05 | 19:43:22–19:44:01 | **the same permission path over the app's Lua relay**, three cases | `05-relay.log`, `05-grouprows-*.txt` |
| 06 | 19:46:33 | the picker's columns, and what the module would accept per master | `06-picker.log` |
| 07 | 19:47:05 | the box read back | `07-box-as-found.log` |
| 08 | 19:47:46 | this lane's own directory off the box | `08-cleanup.log` |
| 09 | 19:52:29 | this folder grepped for secrets | `09-no-secrets.txt` |
| 10 | 19:52 | the box's own activity log for this lane | `10-activity-terminal.txt` |

Scripts: `lib.sh`, `01-ground.sh` … `08-cleanup.sh`, and the driver `t26press.py`.

**`t26press.py` is not `t13press.py`.** T13's driver imported the app (`channel_setup.InstallChannel.
live_channel`) and needed a 12 M copy of the worktree plus a 676 M venv on the box; both were deleted
at T13's cleanup and this lane writes nothing. So the same wire is spoken directly out of the standard
library: the envelope, the `urn:AC` namespace, the Basic header and the fault-before-result
classification are `pylauncher/yulon/soap.py`'s, field for field, and the endpoint is read from the
app's **own** credential file — `~/.local/share/yulon/credentials/wow-wotlk-243c46e3.json`, written by
`channel_setup.save_credential`, the file `live_channel` itself reads. The account is the app's own
`YULON_243C46E3` (id 101, gm level 3). `YULONADMIN` (id 103) was not needed and was not used. **The
password was never printed, never echoed and never put in argv** (`t26press.py:14`, `:29-33`, `:71`, `:80`).

---

## 1. The command, and where it lives

**There is no `.bot add`.** The command is `.playerbots bot add <Name>` — one subcommand of one
command table, on `mod-playerbots` at commit `b949b50bfcdd4fab937781bac2d7765e39330e4b`
(2026-09-04), at `~/wowserver/modules/mod-playerbots` (`02-source.log:5-9`). `bot add Nore` and
`.bot add Nore` both come back `Command 'bot add Nore' does not exist` (`04-soap.log`, section 2).

`src/Script/PlayerbotCommandScript.cpp:35-46` (`02-source.log:50-61`):

```cpp
        static ChatCommandTable playerbotsCommandTable = {
            {"bot", HandlePlayerbotCommand, SEC_PLAYER, Console::No},
            {"gtask", HandleGuildTaskCommand, SEC_GAMEMASTER, Console::Yes},
            {"pmon", HandlePerfMonCommand, SEC_GAMEMASTER, Console::Yes},
            {"rndbot", HandleRandomPlayerbotCommand, SEC_GAMEMASTER, Console::Yes},
            {"debug", playerbotsDebugCommandTable},
            {"account", playerbotsAccountCommandTable},
        };
```

`SEC_PLAYER` — any player may use it. `Console::No` — **no console, RA or SOAP caller may**, and the
core's USAGE list hides what it may not invoke, which is why the reply names only the three
`Console::Yes` siblings. Its entry point, `src/Bot/PlayerbotMgr.cpp:860-874` (`02-source.log:73-87`):

```cpp
bool PlayerbotMgr::HandlePlayerbotMgrCommand(ChatHandler* handler, char const* args)
{
    if (!sPlayerbotAIConfig.enabled) { … "Playerbot system is currently disabled!"; return false; }
    WorldSession* m_session = handler->GetSession();
    if (!m_session)
    {
        handler->PSendSysMessage("You may only add bots from an active session");
        return false;
    }
    Player* player = m_session->GetPlayer();
    PlayerbotMgr* mgr = GET_PLAYERBOT_MGR(player);
    if (!mgr)
    {
        handler->PSendSysMessage("You cannot control bots yet");
        return false;
    }
```

**The master is always the calling session's own player.** No argument targets another master. The
name argument is only the character to be added, resolved at `PlayerbotMgr.cpp:1281-1325`
(`02-source.log:222-268`) by `normalizePlayerName` + `sCharacterCache->GetCharacterGuidByName`, with
`add` taking exactly one character (`addaccount` takes every character of an account instead), and a
`*`/`!` form for group members / all bots.

## 2. The permission rules — five, not three

They are all in one `if`, in `PlayerbotHolder::AddPlayerBot`,
`src/Bot/PlayerbotMgr.cpp:101-116` (`02-source.log:162-177`):

```cpp
    bool isRndbot = !masterAccountId;
    bool sameAccount = sPlayerbotAIConfig.allowAccountBots && accountId == masterAccountId;
    Guild* guild = masterPlayer ? sGuildMgr->GetGuildById(masterPlayer->GetGuildId()) : nullptr;
    bool sameGuild = sPlayerbotAIConfig.allowGuildBots && guild && guild->GetMember(playerGuid);
    bool addClassBot = sRandomPlayerbotMgr.IsAddclassBot(playerGuid.GetCounter());
    bool linkedAccount = sPlayerbotAIConfig.allowTrustedAccountBots && IsAccountLinked(accountId, masterAccountId);

    bool allowed = true;
    …
    if (!isRndbot && !sameAccount && !sameGuild && !addClassBot && !linkedAccount)
    {
        allowed = false;
        out << "Failure: You are not allowed to control bot " << botName.c_str();
    }
```

| # | rule | what it means | conf key | shipped |
|---|---|---|---|---|
| 1 | `sameAccount` | the character is on the master's **own account** — the owner's own alts | `AiPlayerbot.AllowAccountBots` | `1` |
| 2 | `sameGuild` | the character is in the **guild the master is in right now** — `guild->GetMember(guid)` on the master's *online* guild | `AiPlayerbot.AllowGuildBots` | `1` |
| 3 | `addClassBot` | the character is in the module's **addclass pool** — reserved bot characters, allowed to anybody | (no allow flag; see below) | pool of 500 |
| 4 | `linkedAccount` | the two accounts are **linked** in `acore_playerbots.playerbots_account_links` — this is the friends-and-family route | `AiPlayerbot.AllowTrustedAccountBots` | `1` |
| 5 | `isRndbot` | `masterAccountId == 0`, i.e. the random-bot manager adding its own — never a person | — | — |

**There is no "friends" key and no `AllowPlayerBots` key.** The exhaustive greps are
`02-source.log:340-353`. Every `AiPlayerbot.Allow*` key in the tree, in full:
`AllowAccountBots` (:184), `AllowGuildBots` (:185), `AllowTrustedAccountBots` (:186),
`AllowedLogFiles` (:566), `AllowSummonInCombat` (:635), `AllowSummonWhenMasterIsDead` (:636),
`AllowSummonWhenBotIsDead` (:637), `AllowLearnTrainerSpells` (:718) — only the first three appear
anywhere on the add path. The only keys with `friend` in the name are
`AiPlayerbot.BotActiveAloneForceWhenIsFriend` and `AiPlayerbot.LevelBrackets.IgnoreFriendListed`,
neither of which gates adding, and there is **no** key with `PlayerBots` in the name (the grep
returns nothing).

**Two more refusals sit beside those five.** The cap, `PlayerbotMgr.cpp:132-135`
(`02-source.log:193-196`): `count >= maxAddedBots` → `"Failure: You have added too many bots (more
than 40)"`. And the short-circuit before any rule is evaluated, `PlayerbotMgr.cpp:685-686`
(`02-source.log:121-122`): `if (ObjectAccessor::FindPlayer(guid)) return "player already logged in";`
— **an online character can never be added.**

### How rule 3 is decided, and why it matters to the picker

`IsAddclassBot` is pure cache membership (`RandomPlayerbotMgr.cpp:2154-2175`,
`02-source.log:392-413`), and the cache is built by `PrepareAddclassCache`
(`RandomPlayerbotMgr.cpp:1719-1751`, `02-source.log:356-388`):

```cpp
    // Using accounts marked as type 2 (AddClass)
    for (uint32 accountId : addClassTypeAccounts)
        …
            QueryResult results = CharacterDatabase.Query(
                "SELECT guid, race FROM characters "
                "WHERE account = {} AND class = '{}' AND online = 0", accountId, claz);
```

and `addClassTypeAccounts` comes from `acore_playerbots.playerbots_account_type` over the accounts
whose username starts with the prefix (`RandomPlayerbotMgr.cpp:496-509`, `02-source.log:421-434`):

```cpp
    QueryResult allAccounts = LoginDatabase.Query(
        "SELECT id FROM account WHERE username LIKE '{}%%' ORDER BY id",
        sPlayerbotAIConfig.randomBotAccountPrefix.c_str());
```

So **`AiPlayerbot.RandomBotAccountPrefix` does not gate `add` directly.** It decides which accounts
are bot accounts; those are then split by `playerbots_account_type` into type 1 (random-bot pool) and
type 2 (addclass pool), and only type 2's offline characters become "addable by anyone" through rule 3.

### The linked-account route in full

`.playerbots account link <accountName> <securityKey>` is the in-game half
(`PlayerbotCommandScript.cpp:29-32`, all four `SEC_PLAYER, Console::No`). What it does,
`PlayerbotMgr.cpp:1840-1885`:

* the key is SHA-256'd and compared with `acore_playerbots.playerbots_account_keys.security_key`
  (set by `.playerbots account setKey`);
* on a match it writes **both directions** into `acore_playerbots.playerbots_account_links`
  (`INSERT IGNORE … (account_id, linked_account_id)`).

`IsAccountLinked` (`PlayerbotMgr.cpp:191-196`, `02-source.log:216-221`) is one `SELECT 1` against that
table. **The table is a plain two-column link table the launcher could write itself** — nothing in the
check needs the command to have been the thing that wrote the row. On this box both tables are empty
(`03-conf.log`, section "the playerbots database").

### What "added" actually does to the party

On a successful add the module logs the bot in and then **queues a group invite itself**
(`PlayerbotMgr.cpp:545-575`): if the master has no group it creates one, if the master's group has
five it converts to a raid first, then `group->AddMember(target)`
(`Script/WorldThr/PlayerbotOperations.h:28-90`, the add itself at `:78`). So "add" is one command, not add-then-invite. There
is **no faction check** anywhere on that path — read, not measured.

## 3. The conf as shipped on this box

There is **no `playerbots.conf`** — only `playerbots.conf.dist`, which is what the world loads, and
`grep` finds no `AiPlayerbot.*` key set anywhere else under `env/dist/etc/` (`03-conf.log`, sections 1
and "is any AiPlayerbot key set anywhere ELSE"). The four `AC_AI_*` env vars the running container
carries are the bot population and the command-server port only —
`AC_AI_PLAYERBOT_MAX_RANDOM_BOTS=500`, `MIN=500`, `RANDOM_BOT_AUTOLOGIN=1`, `COMMAND_SERVER_PORT=0`
— **none of them touches a permission key** (`03-conf.log`, last two sections of the conf part).

`~/wowserver/env/dist/etc/modules/playerbots.conf.dist`, verbatim, with its own comments:

```
136:AiPlayerbot.MaxAddedBots = 40           # "The maximum number of bots that a player can control simultaneously"
144:AiPlayerbot.GroupInvitationPermission = 1
149:AiPlayerbot.KeepAltsInGroup = 0         # "Keep altbots in the party even when the master leaves"
152:AiPlayerbot.BotAutologin = 0            # "Auto-login all player alts as altbots on player login"
155:AiPlayerbot.AllowAccountBots = 1        # "Allow/deny inviting altbots from the player's account"
158:AiPlayerbot.AllowGuildBots = 1          # "Allow/deny inviting altbots in the player's guild"
161:AiPlayerbot.AllowTrustedAccountBots = 1 # "Allow linking accounts for shared altbot control"
 97:AiPlayerbot.RandomBotAccountCount = 0
102:AiPlayerbot.AddClassAccountPoolSize = 50
106:AiPlayerbot.RandomBotAccountPrefix = "rndbot"   # "Prefix for created bot accounts (of any type). Do not change this prefix while there are existing bot accounts."
189:AiPlayerbot.SelfBotLevel = 1
 83:AiPlayerbot.Enabled = 1
```

**All three allow-flags are on as shipped.** So on this install the rules refuse only a character that
is on another account, not in the master's guild, not in the addclass pool and not linked.

And the DB behind them (`03-conf.log`): `acore_playerbots.playerbots_account_type` holds 100 rows —
**50 accounts of type 1 (random-bot pool) and 50 of type 2 (addclass pool)** — matching the 100
`RNDBOT%` accounts in `acore_auth`; `playerbots_account_links` and `playerbots_account_keys` are both
**empty**.

One thing worth carrying into the spec: **`AiPlayerbot.BotAutologin = 1` would make the whole "own
alts" case need no command at all.** `PlayerbotMgr::OnPlayerLogin` (`PlayerbotMgr.cpp:1635-1686`, the flag at `:1661`)
builds `add <name>,<name>,…` from `SELECT name FROM characters WHERE account = <the player's>` and
runs it at login.

## 4. The three cases over the app's SOAP seam — all refused, and not by the rules

`04-soap.log`. Every press had its window proved empty first; the world logged **nothing** for any of
them (`(no matching line)` under each), which is itself the finding: the command never reached the
module.

The seam is alive in the same run — `dml_bridge_ping` → `outcome=answered`,
`text='DML-BRIDGE-READY dml_bridge_ping\r\n'`, and the world's own line
`[dml_bridge_ping] DML-BRIDGE-READY dml_bridge_ping` (`04-soap.log`, "the seam is alive").

| case | command sent | reply, verbatim | world log |
|---|---|---|---|
| (a) same account — master `Jurnaar` (guid 1, acct 1), target `Nore` (guid 2, acct 1) | `.playerbots bot add Nore` | `refused` HTTP 500 — `### USAGE: .playerbots ...\r\nPossible subcommands:\r\n\|- playerbots debug ...\r\n\|- playerbots gtask\r\n\|- playerbots pmon\r\n\|- playerbots rndbot\r\n` | none |
| (a) again, no dot | `playerbots bot add Nore` | the same USAGE, `refused` HTTP 500 | none |
| (a) again, the ticket's spelling | `.bot add Nore` | `Command 'bot add Nore' does not exist\r\n` | none |
| (a) again | `bot add Nore` | `Command 'bot add Nore' does not exist\r\n` | none |
| (b) other account, no guild — master `Jurnaar` (acct 1, no guild), target `Grirmirn` (guid 11, acct 2, no guild) | `.playerbots bot add Grirmirn` | the same USAGE, `refused` HTTP 500 | none |
| (c) guild mate on another account — master `Dalio` (guid 4, acct 1, guild 12 *Violet Dawn*), target `Jordanik` (guid 24, acct 3, guild 12) | `.playerbots bot add Jordanik` | the same USAGE, `refused` HTTP 500 | none |
| the siblings | `.playerbots bot list`, `.playerbots account linkedAccounts` | the same USAGE | none |
| the control | `.playerbots rndbot stats` | **`answered` HTTP 200**, `text=''` | none |

**The three cases are indistinguishable over SOAP**, and the last row is why: `rndbot` is
`Console::Yes` and answers, so the `.playerbots` prefix, the account and its gm level 3 are all fine.
`bot` is `Console::No`, so the caller cannot see it, let alone run it. **No group row was created by
any of it** — `group_member` = 0 before and after (`04-soap.log`, last section), `online` = 500.

This matches, on this fork and this box, what `dml_addclass.lua`'s own header recorded on 2026-09-08
and what the memory note `my-party-soap-limitation` says. It is now measured again at `b949b50b`.

## 5. The other route — the app's Lua relay — reaches the module and still cannot add

`05-relay.log`. The app already ships a relay for exactly this shape: `dml_addclass.lua` and
`dml_login.lua` run a `.playerbots bot …` subcommand **inside a player's own session** through ALE's
`Player:RunCommand`. **Nothing was deployed and the world was not restarted for this step**:
`dml_login.lua` was already on the box from T13's run (md5 `29d64c67…`), and `login` enters
mod-playerbots through the *same branch* as `add` — `PlayerbotMgr.cpp:683`:

```cpp
    if (cmd == "add" || cmd == "addaccount" || cmd == "login")
```

so it exercises the same permission path.

**The API, quoted.** `mod-ale` at `319f43ed` (2026-09-07),
`src/LuaEngine/methods/PlayerMethods.h:3531-3547` (the function at `:3538`):

```cpp
    /**
    * Run a chat command as if the player typed it into the chat
    *
    * @param string command: text to display in chat or console
    */
    int RunCommand(lua_State* L, Player* player)
    {
        auto command = ALE::CHECKVAL<std::string>(L, 2);
        // In _ParseCommands which is used below no leading . or ! is allowed for the command string.
        if (command[0] == '.' || command[0] == '!') { command = command.substr(1); }
        auto handler = ChatHandler(player->GetSession());
        handler._ParseCommands(command);
        return 0;
    }
```

That is the seam: a `ChatHandler` built on **the player's own session**, so `handler->GetSession()` is
not null and `HandlePlayerbotMgrCommand`'s first guard is passed. (ALE also has a *global*
`RunCommand`, `methods/GlobalMethods.h:1405-1417`, which queues a `CliCommandHolder(nullptr, …)` —
that one is the console context again, and its output goes to the ALE log.)

**The three cases, each with an empty log window first, the group table and the online count before
and after** (`05-relay.log`, `05-grouprows-*.txt` — all six files are 0 bytes, i.e. no group row
existed at any point):

| case | command | reply | world's own log line | joined the party? | target online after |
|---|---|---|---|---|---|
| (a) same account — `Jurnaar` → `Nore` (both acct 1) | `dml_login Jurnaar Nore` | `outcome=answered`, `text=''` | `[dml_login] Jurnaar ran: .playerbots bot login Nore` | **no** — `group_member` 0 rows before and after | 1 (was already 1) |
| (b) other account, no guild — `Jurnaar` (acct 1) → `Rinmu` (guid 501, acct 51 `RNDBOT50`, **offline**) | `dml_login Jurnaar Rinmu` | `outcome=answered`, `text=''` | `[dml_login] Jurnaar ran: .playerbots bot login Rinmu` | **no** — 0 rows before and after | **0 — it never logged in** |
| (c) guild mate on another account — `Dalio` (acct 1, guild 12) → `Jordanik` (guid 24, acct 3, guild 12) | `dml_login Dalio Jordanik` | `outcome=answered`, `text=''` | `[dml_login] Dalio ran: .playerbots bot login Jordanik` | **no** — 0 rows before and after | 1 (was already 1) |

No `playerbot`/`Playerbot` line appeared in any of the three windows either — the grep in
`05-relay.sh` covers both.

**The relay reached the module and the module refused — because of the MASTER, not the target.**
`Rinmu` in case (b) is offline and in the addclass pool, so rule 3 would have allowed it for anybody;
it did not come online. The reason the source gives is one line up from the rules,
`PlayerbotMgr.cpp:870-874`: `GET_PLAYERBOT_MGR(player)` → `"You cannot control bots yet"`. And
`GET_PLAYERBOT_MGR` can only ever answer for a **non-bot** session —
`src/Script/Playerbots.cpp:82-87` (`02-source.log:272-277`):

```cpp
    void OnPlayerLogin(Player* player) override
    {
        if (!player->GetSession()->IsBot())
        {
            PlayerbotsMgr::instance().AddPlayerbotData(player, false);
```

`AddPlayerbotData(player, false)` is the **only** thing that ever puts a row in `_playerbotsMgrMap`
(`PlayerbotMgr.cpp:1731-1761`), and `PlayerbotsMgr::GetPlayerbotMgr` reads only that map
(`:1803-1817`); bots go to `_playerbotsAIMap` instead (`AddPlayerbotData(bot, true)`, `:468`).

**Every one of the 500 characters online on this box is a bot** — `server info` in the same run says
`Connected players: 0. Characters in world: 500.` (`05-relay.log`, section 2). So this box has no
master the module will serve, and that is what these three presses measured.

**How far that goes, honestly.** What is *measured* is: the relay ran, the module was reached, and
none of the three cases produced a login or a group row. What is *read from the source* is why. The
"You cannot control bots yet" string itself was **not captured**: it is sent with
`handler->PSendSysMessage` into the master's own session, and a bot session has no client to print it
and no log line. Two other explanations were considered and are **not excluded by this run**: that
`_ParseCommands` on a bot session does not dispatch at all, and (for case (a) and (c) only) the
`"player already logged in"` short-circuit at `:684`, which fits those two on its own — but not
case (b), whose target was offline. **This is also the answer to the question T13's folder left open**
("why `dml_addclass` made nothing", T13 README §6 and `03b-addclass-cause.log`): the master being a
bot is sufficient on its own, and the `CharactersPerRealm` explanation is no longer needed.

## 6. What the picker needs

`06-picker.log`.

**The columns, and which database each lives in.** The launcher's `DockerSql` asks one database at a
time, so this is three reads, not one join:

| what the picker shows | column | table | database |
|---|---|---|---|
| the name it sends | `name varchar(12)` | `characters` | `acore_characters` |
| which account | `account int unsigned` | `characters` | `acore_characters` |
| level | `level tinyint unsigned` | `characters` | `acore_characters` |
| class, race, gender | `class`, `race`, `gender` (all `tinyint unsigned`) | `characters` | `acore_characters` |
| **online** — decides addability | `online tinyint unsigned` | `characters` | `acore_characters` |
| the row key | `guid int unsigned` | `characters` | `acore_characters` |
| **guild** | `guildid` + `guid` | `guild_member` (there is **no** guild column on `characters` — the grep returns nothing) | `acore_characters` |
| the guild's name | `name` | `guild` | `acore_characters` |
| the account's **name** | `username varchar(32)`, keyed by `id` | `account` | **`acore_auth`** |
| "is it a bot account, and which pool" | `account_id`, `account_type tinyint` | `playerbots_account_type` | **`acore_playerbots`** |
| "is this account linked to mine" | `account_id`, `linked_account_id` | `playerbots_account_links` | **`acore_playerbots`** |

**The filter**, transcribed from `PlayerbotMgr.cpp:101-116` and `:684` with this box's shipped values
(all three allow-flags `1`):

```
offer(target, master) =
      target.online = 0                                     -- :685, else "player already logged in"
  AND target.guid <> master.guid                            -- :1314 skips the master himself
  AND ( target.account = master.account                     -- rule 1, AllowAccountBots = 1
      OR target.guildid = master.guildid (master online)    -- rule 2, AllowGuildBots = 1
      OR target.account_type = 2 (addclass pool)            -- rule 3
      OR (target.account, master.account) in account_links )-- rule 4, AllowTrustedAccountBots = 1
  AND master's current bot count < 40                       -- AiPlayerbot.MaxAddedBots
```

and the refusal to name when it will not, in the module's own words, is
`Failure: You are not allowed to control bot <Name>` (`:115`) or
`Failure: You have added too many bots (more than 40)` (`:134`) or `player already logged in` (`:686`).

**What that yields on this box, counted rather than argued** (`06-picker.log`, section 4): for both
`Jurnaar` and `Dalio` — `by_same_account 0`, `by_guild 0`, `by_addclass_pool **500**`, `by_link 0`,
and 500 characters refused outright for being online. The population is
`rndbot pool (type 1)` 500 characters, all 500 online; `addclass pool (type 2)` 500 characters, none
online; `not a bot account` 1 character, offline (`06-picker.log`, section 3).

Two consequences the spec should not have to rediscover:

* **On a stock 500-bot install the picker has exactly one bucket to show: the 500 offline addclass-pool
  characters, which every rule lets anybody add.** There is no same-account case (a bot account's ten
  characters are either all online or all offline) and no guild case (every guild mate is online).
* **The only character on this box all four rules would refuse is the owner's own `Pakka`** (guid 1001,
  account 102, offline) — counted by SQL in `06-picker.log` section 5 and nothing more: no command was
  sent as it or to it, and nothing about it was written. It is the shape of every real household
  character the owner actually wants to add, and it is refused today because account 102 is neither the
  master's account, nor in the master's guild, nor in the addclass pool, nor linked.

## 7. The box, left as it was found

Every row is `01-ground.log` on the left and `07-box-as-found.log` on the right.

| claim | before | after |
|---|---|---|
| accounts | 103 | 103 |
| characters | 1001 | 1001 |
| guilds / guild_member | 20 / 300 | 20 / 300 |
| **group_member / groups** | 0 / 0 | 0 / 0 |
| online | 500 | 500 |
| `playerbots_account_links` / `_keys` | 0 / 0 | 0 / 0 |
| worldserver | `started=2026-09-10T16:48:48.793295165Z restarts=0` | the same value, `restarts=0` |
| authserver | `started=2026-09-10T17:17:41.380860867Z restarts=0` | the same value, `restarts=0` |
| database | `started=2026-09-10T10:47:55.6428276Z restarts=0` | the same value, `restarts=0` |
| the three non-bot accounts and their gm levels | 101 `YULON_243C46E3` 3, 102 `PERZI` 3, 103 `YULONADMIN` 3 | identical, and `PERZI.last_login` still `2026-09-08 23:24:16` |
| `Pakka` | guid 1001, account 102, level 6, offline | identical |
| the module conf directory | `playerbots.conf.dist` 119723 bytes, `Sep 8 22:11` | identical, md5 `bd8d55ae…`; `mod_ale.conf` md5 `b03ba521…` |
| the deployed Lua scripts | the five plus `LootPet2.lua` and one `.bak` | identical, same md5s, `dml_uninvite.lua` still `3a8dcdb5…` (T13's bytes) |
| the world answers | `DML-BRIDGE-READY` | `DML-BRIDGE-READY` |

**Nothing was created and so nothing had to be removed.** No throwaway account, character or guild was
made: no `.account create` was sent, no SQL write of any kind was issued (`lib.sh:26-27` — the two DB
helpers are `mysql -N -B -e` reads), no guild command was sent, and no party was ever built. The one
thing this lane put on the box was its own `~/t26-out` (scripts and captures, 112 K), removed at
19:47:46 and confirmed gone (`08-cleanup.log`). `~/wowserver` was never written to. The box's own
clone `~/dads-mmo-lab` still reads `182fc92a` with the one pre-existing modification to
`pylauncher/yulon/party.py` that T13 also recorded.

The Claude activity terminal was launched into the desktop session at 19:32 (there was none running);
`ps -eo args | grep -c "[c]laude-term"` answered `0` before and `1` after — one window, as the standing
order requires.

## 8. Secrets

No password is in this folder. `t26press.py` reads the credential file itself and puts the value only
into an `Authorization` header (`t26press.py:71`, `:80`); it is never printed, never echoed, never in
argv. The database root password is never named either — both SQL helpers dereference
`$MYSQL_ROOT_PASSWORD` **inside** the container (`lib.sh:26-27`), so the value never reaches this
shell or any capture. No conf file was ever `cat`'d into a log; only key **names** and their
non-secret values were grepped (memory note `gate-logs-carry-generated-passwords`). The folder was
grepped for the `<game>-<16 hex>` shape and for `passw|secret|token|sha_pass|verifier|MYSQL_|_PASS`
before this line was written; the result is `09-no-secrets.txt`. The box's own activity log for this lane is `10-activity-terminal.txt` — sixteen lines, 19:32:16 to 19:47:46.

## 9. What this folder does NOT show

* **Any of the three permission rules deciding.** Neither route could put the question to the module:
  SOAP cannot see `bot` at all (§4), and the relay needs a master the module will serve, which means a
  **non-bot session** — and this box had `Connected players: 0` throughout (§5). No WoW client was
  driven; the brief's box is `yulon-ubuntu2` and the clients live elsewhere.
* **A refusal string.** `"Failure: You are not allowed to control bot X"` and `"You cannot control bots
  yet"` are both `PSendSysMessage` into the master's session; a bot master has no client and no log
  line, so neither was captured. They are quoted from source, not from the wire.
* **A genuine "other account, not a guild mate" negative.** On this box every offline character except
  the owner's own is in the addclass pool, which rule 3 allows to everybody, so no character exists
  that a bot-free master would be refused — except `Pakka`, which the brief puts out of bounds.
  Producing one needs a throwaway account **with a character**, and a character can only be made by a
  client or by a raw SQL insert; this lane created neither, on purpose.
* **Whether a new bridge script can be loaded without a world restart.** `mod-ale` intercepts
  `reload ale` in `ALE::OnCommand` (`src/LuaEngine/hooks/PlayerHooks.cpp:42-55`, the test at `:50`) and allows it for a
  console caller — `Player* player = handler.IsConsole() ? nullptr : …; if (!player || …SEC_ADMINISTRATOR)`
  — so a SOAP `reload ale` looks like it would hot-load a new script. **It was not pressed**: it
  reloads every Lua script in the running world, including the owner's `LootPet2.lua`, and this brief
  forbade restarts. It is the cheapest thing for the next lane to press.
* **Cross-faction.** Nothing on the add path checks team; unread beyond that, unmeasured.
* **Anything on another core.** WotLK/AzerothCore, `mod-playerbots` `b949b50b`, `mod-ale` `319f43ed`,
  this box, this build.

## 10. What a new bridge script would have to do

Not a spec — the shape the measurements leave, for the lead to accept or discard.

1. **A `dml_botadd.lua` beside the existing five**, console/SOAP-only like its siblings, taking
   `dml_botadd <masterName> <characterName>` and calling
   `p:RunCommand(string.format("playerbots bot add %s", cname))` — the same three lines as
   `dml_login.lua:26-27`. `party.BRIDGE_SCRIPTS` gains a sixth name and `party.deploy` carries it; on
   today's app that means a world restart, unless `reload ale` is pressed first and works.
2. **It must answer, not only act.** `bot add` reports through the *master's* session
   (`PlayerbotMgr.cpp:889-892` loops the messages into `handler->PSendSysMessage`), which for a
   RunCommand caller is the player's client, **not** the SOAP `<result>` — so the refusal the panel
   needs to show never comes back on the wire. T13's `dml_uninvite.lua` already solved this shape: the
   script answers on `handler:SendSysMessage()` with one marker the Python side recognises. The Lua
   would have to decide the outcome itself — poll the group table / `GetPlayerByName(cname)` after the
   call, or pre-check the four rules in Lua and refuse in its own words — because the module's own
   sentence is not reachable from there.
3. **It cannot work until a real person is logged in.** Every measurement above turns on that; the
   panel's precondition list needs the master to be a client-driven session, not merely "online".
4. **The friends-and-family case has a second route that needs no command at all**: two rows in
   `acore_playerbots.playerbots_account_links` (§2). That is a write the launcher can make directly
   through the same `DockerSql` it already uses — worth weighing against asking two people to type
   `.playerbots account setKey` / `.playerbots account link` in game.
5. **The own-alts case has a third**: `AiPlayerbot.BotAutologin = 1` adds every character on the
   player's own account at login (`PlayerbotMgr.cpp:1635-1686`) — no picker, no command, and no
   choosing. It is a conf key, and this brief forbade touching conf, so it was read and not tried.
