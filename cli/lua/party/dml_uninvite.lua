--[[ ============================================================
  dml_uninvite.lua — Dad's MMO Lab "kick a bot from my party" relay
  (AGPL-3.0, part of dads-mmo-lab; reimplemented, not copied.)

  Registers a console/SOAP-callable command:
      dml_uninvite <botName>

  Calls bot:RemoveFromGroup() (AC Player::RemoveFromGroup) directly
  via Eluna — /uninvite is an opcode-layer slash command that
  Player:RunCommand cannot drive, and there is no SOAP kick command.
  The bot knows its own group; remaining members get the leave packet.
============================================================ --]]

local function OnUninviteCommand(event, player, command)
    if player ~= nil then return end

    local bname = command:match("^dml_uninvite%s+(%S+)$")
    if not bname then return end

    local b = GetPlayerByName(bname)
    if not b then
        print(string.format("[dml_uninvite] bot not found: %s", bname))
        return false
    end
    b:RemoveFromGroup()
    print(string.format("[dml_uninvite] removed %s from group", bname))
    return false
end

RegisterPlayerEvent(42, OnUninviteCommand)
print("[dml_uninvite] loaded -- group-remove relay ready")
