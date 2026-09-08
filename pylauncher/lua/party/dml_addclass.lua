--[[ ============================================================
  dml_addclass.lua — Dad's MMO Lab "add a class-bot to my party" relay
  (AGPL-3.0, part of dads-mmo-lab; reimplemented from the behavioral
   reference, not copied.)

  Registers a console/SOAP-callable command:
      dml_addclass <playerName> <classname> [gender]

  Runs `.playerbots bot addclass <classname> [gender]` AS IF
  <playerName> typed it in-game. .playerbots requires a live player
  session (m_session->GetPlayer()), so it cannot be run from SOAP
  directly — Eluna's Player:RunCommand bridges a SOAP-triggered
  command into the player's own session.

  NB: the entry point is `.playerbots bot` (PlayerbotMgr's
  HandlePlayerbotCommand owns the `addclass` sub-keyword), so the
  command is `playerbots bot addclass <class>`, NOT the intuitive
  `playerbots addclass` which the chat framework rejects with USAGE.

  MEASURED on yulon-ubuntu, 2026-09-08, on mod-playerbots b949b50b, and
  recorded here because it corroborates the paragraph above from the other
  side. Asked over SOAP, `playerbots bot addclass mage` came back:

      ### USAGE: .playerbots ...
      Possible subcommands:
      |- playerbots debug ...   |- playerbots gtask
      |- playerbots pmon        |- playerbots rndbot

  `bot` is NOT in that list -- but it is in the command table
  (PlayerbotCommandScript.cpp:36), declared SEC_PLAYER, Console::No. The
  USAGE list is filtered by IsInvokerVisible(handler), so a console/SOAP
  caller cannot SEE `bot`, let alone run it. That is the whole reason this
  file exists: RunCommand runs it inside the player's own session, where it
  is visible. Do not read the USAGE reply as "the subcommand is gone".

  Also measured on that tree, and DIFFERENT from the bash launcher's
  answer: `addclass` accepts dk as a tenth class (PlayerbotMgr.cpp:1089),
  gated on the master's level reaching the heroic start level
  (PlayerbotMgr.cpp:1156). The bash `_valid_bot_class` excluded
  deathknight; that was its tree's fact, not this one's.
============================================================ --]]

-- PLAYER_EVENT_ON_COMMAND = 42 (https://www.azerothcore.org/eluna/Hooks.html)
local function OnAddclassCommand(event, player, command)
    if player ~= nil then return end  -- console / SOAP origin only

    local pname, classname, gender =
        command:match("^dml_addclass%s+(%S+)%s+(%S+)%s+(%S+)$")
    if not pname then
        pname, classname = command:match("^dml_addclass%s+(%S+)%s+(%S+)$")
        gender = nil
    end
    if not pname then return end  -- not our command

    local p = GetPlayerByName(pname)
    if not p then
        print(string.format("[dml_addclass] player not found/offline: %s", pname))
        return false
    end

    local cmd
    if gender then
        cmd = string.format("playerbots bot addclass %s %s", classname, gender)
    else
        cmd = string.format("playerbots bot addclass %s", classname)
    end
    p:RunCommand(cmd)
    print(string.format("[dml_addclass] %s ran: .%s", pname, cmd))
    return false
end

RegisterPlayerEvent(42, OnAddclassCommand)
print("[dml_addclass] loaded -- addclass relay ready")
