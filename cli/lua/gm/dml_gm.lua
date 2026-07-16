--[[
    dml_gm.lua -- Dad's MMO Lab launcher GM bridge.
    License: AGPL-3.0-only (same as the repo).
    Reimplemented for DML; behavioral reference: The Lab's GM relay.
    See docs/superpowers/specs/2026-07-16-gm-tools-design.md.

    Console/SOAP-only commands for editing a LOGGED-IN character without
    a relog. Direct DB UPDATEs only take effect on next login (the
    worldserver caches Player state and overwrites the row on logout);
    these mutate the live Player object instead, then SaveToDB() so the
    change also survives a crash.

        dml_gm_health <name> <pct>     -- HP to pct of max (floor 1 HP)
        dml_gm_money  <name> <copper>  -- absolute coinage
        dml_gm_revive <name>           -- resurrect, full HP, no sickness
]]--

local function find_online(name)
    local p = GetPlayerByName(name)
    if not p then
        print(string.format("[dml_gm] player not online: %s", name))
    end
    return p
end

local function OnGmCommand(event, player, command)
    -- Console/SOAP origin only: a real player typing this must never match.
    if player ~= nil then return end

    -- dml_gm_health <name> <pct>
    local hname, hpct = command:match("^dml_gm_health%s+(%S+)%s+(%S+)$")
    if hname then
        local p = find_online(hname)
        if not p then return false end
        local pct = tonumber(hpct)
        if not pct then return false end
        local max_hp = p:GetMaxHealth()
        local new_hp = math.floor(max_hp * pct / 100)
        if new_hp < 1 then new_hp = 1 end
        p:SetHealth(new_hp)
        p:SaveToDB()
        print(string.format("[dml_gm] %s HP -> %d/%d", hname, new_hp, max_hp))
        return false
    end

    -- dml_gm_money <name> <copper>
    local mname, mcopper = command:match("^dml_gm_money%s+(%S+)%s+(%S+)$")
    if mname then
        local p = find_online(mname)
        if not p then return false end
        local copper = tonumber(mcopper)
        if not copper or copper < 0 or copper ~= math.floor(copper) then return false end
        p:SetCoinage(copper)
        p:SaveToDB()
        print(string.format("[dml_gm] %s coinage -> %d", mname, copper))
        return false
    end

    -- dml_gm_revive <name>
    local rname = command:match("^dml_gm_revive%s+(%S+)$")
    if rname then
        local p = find_online(rname)
        if not p then return false end
        -- 1.0 = full HP; false = no resurrection sickness (a launcher-
        -- initiated revive should not penalize the player).
        p:ResurrectPlayer(1.0, false)
        p:SetHealth(p:GetMaxHealth())
        p:SaveToDB()
        print(string.format("[dml_gm] revived %s", rname))
        return false
    end
end

RegisterPlayerEvent(42, OnGmCommand)
print("[dml_gm] loaded")
