-- dml_autobuff.lua — ALE (Eluna-family) server-side auto-buffer for dad's server.
--
-- Opt-in per character: type  #buffs on  in any chat. Every 10s the script
-- re-applies missing buffs to the player and every summon they own nearby, as
-- DIRECT AURAS (Unit:AddAura) — no cast, no mana, no reagents, auto-renews on
-- expiry. Exclusive groups (paladin aura, hunter aspect, mage armor, warlock
-- armor, shaman shield) apply the character's saved choice; everything
-- stackable applies at the highest rank the character has learned.
--
-- Deliberately NOT auto-applied: seals, stances, presences, druid forms —
-- the player's own macros manage those.
--
-- Opt-in doubles as the bot filter: playerbots/citizens never type #buffs,
-- so the loop skips the ~1400 bot Players entirely.
--
-- Persistence: acore_characters.dml_autobuff (created below; sanctioned write
-- surface, user-approved 2026-08-15). Choices come only from the KEYWORD
-- tables here, never raw user text, so nothing user-typed reaches SQL.
--
-- ALE quirks honoured: no SetData/GetData (state kept in Lua tables + DB),
-- AddAura/GetCreaturesInRange wrapped in pcall with CastSpell fallback.

local UPDATE_MS   = 10000
local SUMMON_YDS  = 45

-- ---------------------------------------------------------------- buff data
-- Chains are TOP RANK FIRST; the first HasSpell hit wins.

local SHARED = { -- player + all owned summons
  { name = "Power Word: Fortitude", ids = {48161,25389,10938,10937,2791,1245,1244,1243} },
  { name = "Divine Spirit",         ids = {48073,25312,27841,14819,14818,14752} },
  { name = "Shadow Protection",     ids = {48169,25433,10958,10957,976} },
  { name = "Mark of the Wild",      ids = {48469,26990,9885,9884,8907,5234,6756,5232,1126} },
  { name = "Thorns",                ids = {53307,26992,9910,9756,8914,1075,782,467} },
  { name = "Arcane Intellect",      ids = {42995,27126,10157,10156,1461,1460,1459} },
  { name = "Blessing of Kings",     ids = {20217} },
  { name = "Blessing of Might",     ids = {48932,48931,27140,25291,19838,19837,19836,19835,19834,19740} },
  { name = "Blessing of Wisdom",    ids = {48936,48935,27142,25290,19854,19853,19852,19850,19742} },
  { name = "Blessing of Sanctuary", ids = {20911} },
  { name = "Battle Shout",          ids = {47436,2048,25289,11551,11550,11549,6192,5242,6673} },
  { name = "Commanding Shout",      ids = {47440,47439,469} },
  { name = "Horn of Winter",        ids = {57623,57330} },
}

local PLAYER_ONLY = {
  { name = "Inner Fire", ids = {48168,48040,25431,10952,10951,1006,602,7128,588} },
}

-- Exclusive groups: one choice per group, player-only targets.
local GROUPS = {
  aura = {
    label = "Paladin aura", default = "retribution",
    choices = { devotion = 48942, retribution = 54043, concentration = 19746,
                crusader = 32223, shadowresist = 48943, frostresist = 48945,
                fireresist = 48947 },
  },
  aspect = {
    label = "Hunter aspect", default = "dragonhawk",
    choices = { dragonhawk = 61847, hawk = 27044, monkey = 13163, cheetah = 5118,
                pack = 13159, viper = 34074, wild = 49071 },
  },
  magearmor = {
    label = "Mage armor", default = "molten",
    choices = { molten = 43046, mage = 43024, ice = 43008, frost = 168 },
  },
  lockarmor = {
    label = "Warlock armor", default = "fel",
    choices = { fel = 47893, demon = 47889 },
  },
  shield = {
    label = "Shaman shield", default = "lightning",
    choices = { lightning = 49281, water = 57960, earth = 49284 },
  },
}

local GROUP_ORDER = { "aura", "aspect", "magearmor", "lockarmor", "shield" }

-- ---------------------------------------------------------------- state + db

local state = {} -- [guidLow] = { enabled=bool, aura=key, aspect=key, ... }

local function defaultState()
  local s = { enabled = false }
  for key, grp in pairs(GROUPS) do s[key] = grp.default end
  return s
end

CharDBExecute([[
CREATE TABLE IF NOT EXISTS `dml_autobuff` (
  `guid`      INT UNSIGNED NOT NULL PRIMARY KEY,
  `enabled`   TINYINT      NOT NULL DEFAULT 0,
  `aura`      VARCHAR(24)  NOT NULL DEFAULT 'retribution',
  `aspect`    VARCHAR(24)  NOT NULL DEFAULT 'dragonhawk',
  `magearmor` VARCHAR(24)  NOT NULL DEFAULT 'molten',
  `lockarmor` VARCHAR(24)  NOT NULL DEFAULT 'fel',
  `shield`    VARCHAR(24)  NOT NULL DEFAULT 'lightning'
)
]])

local function loadState(guid)
  local s = defaultState()
  local q = CharDBQuery("SELECT enabled, aura, aspect, magearmor, lockarmor, shield FROM dml_autobuff WHERE guid = " .. guid)
  if q then
    s.enabled  = q:GetUInt32(0) == 1
    local cols = { "aura", "aspect", "magearmor", "lockarmor", "shield" }
    for i, key in ipairs(cols) do
      local v = q:GetString(i)
      if GROUPS[key].choices[v] then s[key] = v end
    end
  end
  state[guid] = s
  return s
end

local function saveState(guid)
  local s = state[guid]
  if not s then return end
  -- every value below comes from our own tables (booleans + validated keys)
  CharDBExecute(string.format(
    "REPLACE INTO dml_autobuff (guid, enabled, aura, aspect, magearmor, lockarmor, shield) VALUES (%d, %d, '%s', '%s', '%s', '%s', '%s')",
    guid, s.enabled and 1 or 0, s.aura, s.aspect, s.magearmor, s.lockarmor, s.shield))
end

local function getState(player)
  local guid = player:GetGUIDLow()
  return state[guid] or loadState(guid)
end

-- ---------------------------------------------------------------- buffing

local function applyAura(caster, target, spellId)
  if target:HasAura(spellId) then return end
  local ok = pcall(function() caster:AddAura(spellId, target) end)
  if not ok then
    -- ALE build without AddAura: triggered cast is still free/instant
    pcall(function() caster:CastSpell(target, spellId, true) end)
  end
end

local function highestKnown(player, ids)
  for _, id in ipairs(ids) do
    if player:HasSpell(id) then return id end
  end
end

local function ownedSummons(player)
  local out = {}
  local ok, list = pcall(function() return player:GetCreaturesInRange(SUMMON_YDS) end)
  if not ok or type(list) ~= "table" then return out end
  local myGuid = player:GetGUID()
  for _, c in ipairs(list) do
    local okOwn, owned = pcall(function() return c:GetOwnerGUID() == myGuid end)
    if okOwn and owned and c:IsAlive() then out[#out + 1] = c end
  end
  return out
end

local function buffPlayer(player)
  if player:IsDead() then return end
  local s = getState(player)

  local targets = { player }
  for _, c in ipairs(ownedSummons(player)) do targets[#targets + 1] = c end

  for _, line in ipairs(SHARED) do
    local id = highestKnown(player, line.ids)
    if id then
      for _, t in ipairs(targets) do applyAura(player, t, id) end
    end
  end

  for _, line in ipairs(PLAYER_ONLY) do
    local id = highestKnown(player, line.ids)
    if id then applyAura(player, player, id) end
  end

  for key, grp in pairs(GROUPS) do
    local id = grp.choices[s[key]]
    if id and player:HasSpell(id) then applyAura(player, player, id) end
  end
end

CreateLuaEvent(function()
  local ok, players = pcall(GetPlayersInWorld)
  if not ok or type(players) ~= "table" then return end
  for _, p in ipairs(players) do
    local st = state[p:GetGUIDLow()]
    if st and st.enabled then
      pcall(buffPlayer, p)
    end
  end
end, UPDATE_MS, 0)

-- ---------------------------------------------------------------- chat command

local function msgTo(player, text)
  player:SendBroadcastMessage("|cff33ff99[autobuff]|r " .. text)
end

local function showStatus(player)
  local s = getState(player)
  msgTo(player, "auto-buff is " .. (s.enabled and "|cff00ff00ON|r" or "|cffff0000OFF|r") .. "  (#buffs on / #buffs off)")
  for _, key in ipairs(GROUP_ORDER) do
    local grp = GROUPS[key]
    local opts = {}
    for k in pairs(grp.choices) do opts[#opts + 1] = k end
    table.sort(opts)
    msgTo(player, string.format("%s: |cffffff00%s|r  (#buffs %s <%s>)",
      grp.label, s[key], key, table.concat(opts, "|")))
  end
end

local function onChat(event, player, msg)
  local cmd = msg:match("^#buffs%s*(.*)$")
  if not cmd then return end
  cmd = cmd:lower()

  local s = getState(player)
  local guid = player:GetGUIDLow()

  if cmd == "" or cmd == "list" or cmd == "status" then
    showStatus(player)
  elseif cmd == "on" then
    s.enabled = true
    saveState(guid)
    msgTo(player, "ON — buffing you and your summons every " .. (UPDATE_MS / 1000) .. "s.")
    pcall(buffPlayer, player)
  elseif cmd == "off" then
    s.enabled = false
    saveState(guid)
    msgTo(player, "OFF.")
  else
    local group, choice = cmd:match("^(%S+)%s+(%S+)$")
    if group and GROUPS[group] then
      if GROUPS[group].choices[choice] then
        s[group] = choice
        saveState(guid)
        msgTo(player, GROUPS[group].label .. " -> " .. choice)
        if s.enabled then pcall(buffPlayer, player) end
      else
        local opts = {}
        for k in pairs(GROUPS[group].choices) do opts[#opts + 1] = k end
        table.sort(opts)
        msgTo(player, "unknown choice. options: " .. table.concat(opts, ", "))
      end
    else
      msgTo(player, "usage: #buffs | #buffs on | #buffs off | #buffs <aura|aspect|magearmor|lockarmor|shield> <choice>")
    end
  end
  return false -- suppress the chat line
end

local PLAYER_EVENT_ON_CHAT   = 18
local PLAYER_EVENT_ON_LOGIN  = 3
local PLAYER_EVENT_ON_LOGOUT = 4

RegisterPlayerEvent(PLAYER_EVENT_ON_CHAT, onChat)
RegisterPlayerEvent(PLAYER_EVENT_ON_LOGIN, function(event, player)
  loadState(player:GetGUIDLow())
end)
RegisterPlayerEvent(PLAYER_EVENT_ON_LOGOUT, function(event, player)
  state[player:GetGUIDLow()] = nil
end)

print("[dml_autobuff] loaded — opt in with #buffs on")
