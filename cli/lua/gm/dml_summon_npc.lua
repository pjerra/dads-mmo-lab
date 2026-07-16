--[[
    dml_summon_npc.lua -- Dad's MMO Lab launcher summon bridge.
    License: AGPL-3.0-only (same as the repo).
    Reimplemented for DML; behavioral reference: The Lab's summon relay.
    See docs/superpowers/specs/2026-07-16-summon-npcs-design.md.

    One console/SOAP-only command:

        dml_summon_npc <playerName> <creatureEntry>

    Temp-spawns <creatureEntry> just in front of the ONLINE player.
    Spawn type 8 = TEMPSUMMON_TIMED_DESPAWN with a 300000 ms timer --
    the creature vanishes after 5 minutes no matter what, so repeated
    summons can't litter the world. No DB writes.

    Why a bridge: `.npc add` needs an in-world GM session with a
    position, which SOAP doesn't have -- Eluna routes through the
    player's own position instead (same pattern as the other bridges).
]]--

local function OnSummonCommand(event, player, command)
    -- Console/SOAP origin only: a real player typing this must never match.
    if player ~= nil then return end

    local pname, entry = command:match("^dml_summon_npc%s+(%S+)%s+(%d+)$")
    if not pname then return end

    local p = GetPlayerByName(pname)
    if not p then
        print(string.format("[dml_summon_npc] player not online: %s", pname))
        return false
    end

    local e = tonumber(entry)
    local x, y, z, o = p:GetX(), p:GetY(), p:GetZ(), p:GetO()
    -- Drop it just in front of the player so it isn't standing inside them.
    local fx = x + math.cos(o) * 2.0
    local fy = y + math.sin(o) * 2.0

    -- WorldObject:SpawnCreature(entry, x, y, z, o, spawnType, despawnTimer)
    p:SpawnCreature(e, fx, fy, z, o, 8, 300000)
    print(string.format("[dml_summon_npc] %s -> npc %d", pname, e))
    return false
end

RegisterPlayerEvent(42, OnSummonCommand)
print("[dml_summon_npc] loaded")
