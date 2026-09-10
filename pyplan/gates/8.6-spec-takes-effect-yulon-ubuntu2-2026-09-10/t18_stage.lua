--[[ ============================================================
  t18_stage.lua -- the T18 live half's STAGING + CHAT-CAPTURE harness.
  Not part of the app, not shipped, not in `party.BRIDGE_SCRIPTS`, and
  removed from the server (with a world restart) by `08-teardown.sh`.

  Why it exists -- TWO jobs, both forced by what the module does.

  1. A MASTER. `talents spec <name>` reaches `ChangeTalentsAction` only if
     `PlayerbotSecurity::LevelFor(from)` answers ALLOW_ALL, and for a random
     bot that means one thing: `botAI->GetMaster() == from`, in the same
     group (mod-playerbots `src/Mgr/Security/PlayerbotSecurity.cpp`,
     LevelFor). A group made with Eluna's `Player:GroupCreate` -- T13's
     harness -- does NOT set a master: `PlayerbotAI::FindNewMaster`
     (`src/Bot/PlayerbotAI.cpp`) returns only a REAL player, so a bot-led
     group leaves `master == nullptr` and every command is refused in
     silence (`CheckLevelFor`: `if (silent || (fromBotAI && !IsSelfBot(from)))
     return false;`).

     The one line in the module that sets a master WITHOUT a real player is
     `AcceptInvitationAction.cpp:51` --
     `if (sRandomPlayerbotMgr.IsRandomBot(bot)) botAI->SetMaster(inviter);`
     -- reached only from a real core group INVITE. So this harness invites
     rather than groups: `Player:GroupInvite`, the packet the bot's own
     accept action listens for. What it makes is a real core Group with a
     real playerbot master, and nothing downstream learns where it came
     from.

  2. AN EAR. The module answers `talents spec list` through
     `PlayerbotAI::TellMasterNoFacing`, which builds a chat packet and calls
     `master->SendDirectMessage` -- a socket-less bot session drops it, and
     nothing this app can read ever sees the reply (that is exactly what T5
     had to photograph a game client for). The same function has one other
     branch: with a bot master and the `debug` non-combat strategy on, the
     bot `Say`s the text instead. `Player::Say` calls
     `sScriptMgr->OnPlayerCanUseChat` (AzerothCore
     `src/server/game/Entities/Player/Player.cpp:9570`), ALE forwards that to
     `PLAYER_EVENT_ON_CHAT` (`mod-ale/src/ALE_SC.cpp:654`,
     `Hooks.h:183`), and this file prints it into the worldserver log. The
     reply is then the WORLD's own log line.

  It never sends a bot command of its own, never answers in the words
  `party.py` parses, and registers a command name no part of the app sends.

      dml_t18_stage watch   <a> [b]        log only these names' chat
      dml_t18_stage unwatch                log nobody's
      dml_t18_stage invite  <inviter> <invitee>
      dml_t18_stage show    <player> [other]
      dml_t18_stage disband <player>
============================================================ --]]

local watched = {}
local watching = false

local function log(msg)
    print("[t18_stage] " .. msg)
end

-- A name is interesting if it is watched. Both ends are checked so a reply
-- addressed to a watched master is kept even when the speaker is not watched.
local function interesting(a, b)
    if not watching then return false end
    if a ~= nil and watched[a:lower()] then return true end
    if b ~= nil and watched[b:lower()] then return true end
    return false
end

-- PLAYER_EVENT_ON_CHAT = 18 -- (event, player, msg, type, lang). Fired by
-- Player::Say/Yell/Emote. This is the branch the module's reply takes once
-- the bot has a bot master and the `debug` strategy.
local function OnChat(event, player, msg, chatType, lang)
    if player == nil then return end
    local name = player:GetName()
    if interesting(name, nil) then
        print(string.format("[t18_chat] SAY   %s: %s", name, msg))
    end
end

-- PLAYER_EVENT_ON_WHISPER = 19 -- (event, player, msg, type, lang, receiver).
-- Fired by Player::Whisper: our own dml_whisper commands go out through it,
-- and so does `PlayerbotSecurity::CheckLevelFor`'s spoken refusal.
local function OnWhisper(event, player, msg, chatType, lang, receiver)
    if player == nil then return end
    local from = player:GetName()
    local to = receiver ~= nil and receiver:GetName() or "?"
    if interesting(from, to) then
        print(string.format("[t18_chat] WHISP %s -> %s: %s", from, to, msg))
    end
end

local function OnStageCommand(event, player, command, handler)
    if player ~= nil then return end

    local verb, a, b = command:match("^dml_t18_stage%s+(%S+)%s*(%S*)%s*(%S*)$")
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
print("[t18_stage] loaded -- T18 live-half staging + chat-capture harness (not part of the app)")
