--[[ ============================================================
  t16_stage.lua -- the T16 live half's STAGING + CHAT-CAPTURE harness.
  T18's `t18_stage.lua`, unchanged in shape and re-headed for this lane.
  Not part of the app, not shipped, not in `party.BRIDGE_SCRIPTS`, and
  removed from the server (with a world restart) by this lane's teardown.

  WHY A HARNESS AT ALL, AND WHY THE PANEL'S OWN ADD COULD NOT BE PRESSED.

  `party.add_bot` adds a bot with `dml_addclass`, which is
  `.playerbots bot addclass <class>` run inside the MASTER's own session
  (dml_addclass.lua). That command reaches its handler only through
  `PlayerbotMgr::HandlePlayerbotMgrCommand`
  (mod-playerbots src/Bot/PlayerbotMgr.cpp:865-889), whose third guard is

      PlayerbotMgr* mgr = GET_PLAYERBOT_MGR(player);
      if (!mgr) { handler->PSendSysMessage("You cannot control bots yet"); return false; }

  and `GET_PLAYERBOT_MGR` answers only for a NON-bot session: a playerbot
  gets a `PlayerbotAI` and no `PlayerbotMgr`. Every character in the world
  on this box is a playerbot (`server info`: "Connected players: 0.
  Characters in world: 500."), and no client can be driven here, so
  `dml_addclass` has no master it can work for -- pressed twice at the
  start of this run, it logged `[dml_addclass] Jurnaar ran: .playerbots
  bot addclass mage`, logged a bot in exactly zero times, and formed no
  group. T26's own measurement found the same wall from the other side
  (`8.6-altbot-measure-.../README.md`, section on `GET_PLAYERBOT_MGR`).

  So the party in this run is made the one way a bot CAN get a bot
  master: a real core group INVITE. `AcceptInvitationAction.cpp:51` --
  `if (sRandomPlayerbotMgr.IsRandomBot(bot)) botAI->SetMaster(inviter);`
  -- is the single line in the module that sets a master without a real
  player, and it is reached only from a real invite packet, which is why
  this harness invites rather than using Eluna's `Player:GroupCreate`
  (T13's harness; `PlayerbotAI::FindNewMaster` returns only a REAL
  player, so a group made that way leaves `master == nullptr`).

  AN EAR, as well. The module answers a whisper through
  `PlayerbotAI::TellMasterNoFacing`, which builds a chat packet for
  `master->SendDirectMessage` -- a socket-less bot session drops it. Its
  other branch, for a bot master with the `debug` non-combat strategy,
  calls `bot->Say(...)`; `Player::Say` calls
  `sScriptMgr->OnPlayerCanUseChat` (AzerothCore
  src/server/game/Entities/Player/Player.cpp:9570), ALE forwards that to
  PLAYER_EVENT_ON_CHAT (mod-ale/src/ALE_SC.cpp:654, Hooks.h:183), and
  this file prints it into the worldserver log.

  It never sends a bot command of its own, never answers in the words
  `party.py` parses, and registers a command name no part of the app sends.

      dml_t16_stage watch   <a> [b]        log only these names' chat
      dml_t16_stage unwatch                log nobody's
      dml_t16_stage invite  <inviter> <invitee>
      dml_t16_stage show    <player> [other]
      dml_t16_stage disband <player>
============================================================ --]]

local watched = {}
local watching = false

local function log(msg)
    print("[t16_stage] " .. msg)
end

-- A name is interesting if it is watched. Both ends are checked so a reply
-- addressed to a watched master is kept even when the speaker is not watched.
local function interesting(a, b)
    if not watching then return false end
    if a ~= nil and watched[a:lower()] then return true end
    if b ~= nil and watched[b:lower()] then return true end
    return false
end

-- PLAYER_EVENT_ON_CHAT = 18 -- (event, player, msg, type, lang).
local function OnChat(event, player, msg, chatType, lang)
    if player == nil then return end
    local name = player:GetName()
    if interesting(name, nil) then
        print(string.format("[t16_chat] SAY   %s: %s", name, msg))
    end
end

-- PLAYER_EVENT_ON_WHISPER = 19 -- (event, player, msg, type, lang, receiver).
local function OnWhisper(event, player, msg, chatType, lang, receiver)
    if player == nil then return end
    local from = player:GetName()
    local to = receiver ~= nil and receiver:GetName() or "?"
    if interesting(from, to) then
        print(string.format("[t16_chat] WHISP %s -> %s: %s", from, to, msg))
    end
end

local function OnStageCommand(event, player, command, handler)
    if player ~= nil then return end

    local verb, a, b = command:match("^dml_t16_stage%s+(%S+)%s*(%S*)%s*(%S*)$")
    if not verb then return end
    if a == "" then a = nil end
    if b == "" then b = nil end

    local function say(msg)
        if handler ~= nil then
            handler:SendSysMessage(msg)
        end
        log(msg)
    end

    if verb == "unwatch" then
        watched = {}
        watching = false
        say("stage unwatch: chat capture off")
        return false
    end

    if verb == "watch" then
        watched = {}
        watching = false
        if a then watched[a:lower()] = true; watching = true end
        if b then watched[b:lower()] = true; watching = true end
        say(string.format("stage watch: %s %s", tostring(a), tostring(b)))
        return false
    end

    if not a then
        say(string.format("stage: %s needs a player name", verb))
        return false
    end

    local pa = GetPlayerByName(a)
    if not pa then
        say(string.format("stage: %s not found or offline", a))
        return false
    end
    local pb = nil
    if b then
        pb = GetPlayerByName(b)
        if not pb then
            say(string.format("stage: %s not found or offline", b))
            return false
        end
    end

    if verb == "invite" then
        if not pb then
            say("stage invite: needs <inviter> <invitee>")
            return false
        end
        local ok, err = pcall(function() pa:GroupInvite(pb) end)
        say(string.format("stage invite %s -> %s ok=%s err=%s", a, b, tostring(ok), tostring(err)))
    elseif verb == "disband" then
        local g = pa:GetGroup()
        if g == nil then
            say(string.format("stage disband: %s has no group", a))
            return false
        end
        local ok, err = pcall(function() g:Disband() end)
        say(string.format("stage disband %s's group ok=%s err=%s", a, tostring(ok), tostring(err)))
    elseif verb == "show" then
        local g = pa:GetGroup()
        if g == nil then
            say(string.format("stage show: %s is in no group", a))
            return false
        end
        local leader = "?"
        pcall(function() leader = tostring(g:GetLeaderGUID()) end)
        local msg = string.format(
            "stage show: %s is grouped; members=%s leaderGUID=%s",
            a, tostring(g:GetMembersCount()), leader
        )
        if pb then
            local ism = "?"
            pcall(function() ism = tostring(g:IsMember(pb:GetGUID())) end)
            msg = msg .. string.format("; IsMember(%s)=%s", b, ism)
        end
        say(msg)
    else
        say(string.format("stage: unknown verb %s", verb))
    end
    return false
end

RegisterPlayerEvent(42, OnStageCommand)
RegisterPlayerEvent(18, OnChat)
RegisterPlayerEvent(19, OnWhisper)
print("[t16_stage] loaded -- T16 live-half staging + chat-capture harness (not part of the app)")
