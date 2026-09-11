--[[
    dml_whisper.lua -- Dad's MMO Lab launcher whisper-as-player bridge.
    License: AGPL-3.0-only (same as the repo).
    Reimplemented for DML; behavioral reference: The Lab's whisper relay.
    See docs/superpowers/specs/2026-07-17-my-party-phase2-design.md.

    One console/SOAP-only command:

        dml_whisper <playerName> <botName> <message...>

    Sends <message> as a /whisper FROM the player TO the bot, exactly as
    if the player had typed it. mod-playerbots accepts its bot commands
    (autogear, talents autopick, maintenance, ...) only as whispers from
    a player session -- SOAP has no way to spoof player chat; Eluna's
    Player:Whisper does (it routes through core Player::Whisper, which
    fires the module's chat hook).
]]--

local function OnWhisperCommand(event, player, command)
    -- Console/SOAP origin only: chat parses always carry a non-nil
    -- player, so in-game chat can never trigger this.
    if player ~= nil then return end

    -- Greedy third capture: the message may contain spaces.
    local pname, bname, msg = command:match("^dml_whisper%s+(%S+)%s+(%S+)%s+(.+)$")
    if not pname then return end

    local p = GetPlayerByName(pname)
    if not p then
        print(string.format("[dml_whisper] player not online: %s", pname))
        return false
    end
    local b = GetPlayerByName(bname)
    if not b then
        print(string.format("[dml_whisper] bot not online: %s", bname))
        return false
    end

    -- Language 0 = universal (core forces whispers universal anyway).
    p:Whisper(msg, 0, b)
    print(string.format("[dml_whisper] %s -> %s: %s", pname, bname, msg))
    return false
end

RegisterPlayerEvent(42, OnWhisperCommand)
print("[dml_whisper] loaded")
