--[[ ============================================================
  dml_login.lua — Dad's MMO Lab "bring a bot back online" relay
  (AGPL-3.0, part of dads-mmo-lab; reimplemented, not copied.)

  Registers a console/SOAP-callable command:
      dml_login <playerName> <botName>

  Runs `.playerbots bot login <botName>` as <playerName>, logging the
  bot back in under the player's session after the player relogged.
  mod-playerbots then auto-rejoins it to the master's group. Like
  addclass, `bot login` needs a live master session, so it goes
  through Player:RunCommand.
============================================================ --]]

local function OnLoginCommand(event, player, command)
    if player ~= nil then return end

    local pname, bname = command:match("^dml_login%s+(%S+)%s+(%S+)$")
    if not pname then return end

    local p = GetPlayerByName(pname)
    if not p then
        print(string.format("[dml_login] player not found/offline: %s", pname))
        return false
    end
    p:RunCommand(string.format("playerbots bot login %s", bname))
    print(string.format("[dml_login] %s ran: .playerbots bot login %s", pname, bname))
    return false
end

RegisterPlayerEvent(42, OnLoginCommand)
print("[dml_login] loaded -- bring-online relay ready")
