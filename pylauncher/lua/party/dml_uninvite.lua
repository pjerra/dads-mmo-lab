--[[ ============================================================
  dml_uninvite.lua — Dad's MMO Lab "kick a bot from my party" relay
  (AGPL-3.0, part of dads-mmo-lab; reimplemented, not copied.)

  Registers a console/SOAP-callable command:
      dml_uninvite <playerName> <botName>

  Calls bot:RemoveFromGroup() (AC Player::RemoveFromGroup) directly
  via Eluna — /uninvite is an opcode-layer slash command that
  Player:RunCommand cannot drive, and there is no SOAP kick command.
  The bot knows its own group; remaining members get the leave packet.

  T13 (Fable, from T5's round-3 Codex review): <playerName> is new.
  party.py's InstallParty.remove_all re-reads the group table and
  refuses to act unless the fresh guid set is exactly what a person
  confirmed -- but the interval between that read and this command
  landing is real, and nothing further Python reads narrows it to
  zero: the bot manager moves bots on its own timers, so a bot can
  join a DIFFERENT master's party in that window. Before this ticket
  the old one-argument form trusted whatever group the bot happened
  to occupy WHEN THIS RAN, so such a bot was kicked from a party
  nobody confirmed. This is the only place left that can still see
  the group at the moment it acts, so the check moves here: before
  RemoveFromGroup(), verify <playerName> actually leads the group
  <botName> is in right now.

  Nothing pinned from this fork's mod-playerbots was on this box to
  read against (server modules live on the test VMs, never this
  laptop -- see this script family's sibling `party.py` header for
  why). This is reasoned instead from the Eluna Group API this
  bridge already reaches for -- Player:GetGroup(), Group:
  GetLeaderGUID(), Player:GetGUID() -- rather than measured against
  the fork's own master-resolution code, and proving it against a
  real party is the live half's job. A bot never leads its own group
  in this app's flow (dml_addclass invites it into the MASTER's
  group, never the reverse), so "the group <playerName> leads" and
  "the group <playerName> is in" are the same test here.

  On refusal it answers over the SAME wire the question came in on
  (handler:SendSysMessage — see dml_bridge_ping.lua's header for why
  print() alone never reaches a SOAP caller: this hook returns false
  either way, so the core's error flag never trips and the reply is
  always a SOAP <result>, not a <faultstring>), in the exact words
  party.py's dismiss() reads back (`_uninvite_moved_marker`):
      "<botName> is not in <playerName>'s party now"
============================================================ --]]

local function OnUninviteCommand(event, player, command, handler)
    if player ~= nil then return end

    local pname, bname = command:match("^dml_uninvite%s+(%S+)%s+(%S+)$")
    if not pname then return end

    local p = GetPlayerByName(pname)
    if not p then
        print(string.format("[dml_uninvite] player not found/offline: %s", pname))
        return false
    end
    local b = GetPlayerByName(bname)
    if not b then
        print(string.format("[dml_uninvite] bot not found: %s", bname))
        return false
    end

    local g = b:GetGroup()
    if g == nil or g:GetLeaderGUID() ~= p:GetGUID() then
        local msg = string.format("%s is not in %s's party now", bname, pname)
        if handler ~= nil then
            handler:SendSysMessage(msg)
        end
        print("[dml_uninvite] " .. msg)
        return false
    end

    b:RemoveFromGroup()
    print(string.format("[dml_uninvite] removed %s from group", bname))
    return false
end

RegisterPlayerEvent(42, OnUninviteCommand)
print("[dml_uninvite] loaded -- group-remove relay ready")
