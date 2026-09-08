--[[ ============================================================
  dml_bridge_ping.lua -- "is the bridge actually there?", answered by the
  bridge itself. Part of dads-mmo-lab (AGPL-3.0).

  Registers a console/SOAP-callable command:

      dml_bridge_ping

  This script exists because of what happened on 2026-08-20: the bridge
  deploy reported success and every bridge command answered "Command does
  not exist". A deploy reporting success proves that files were copied. It
  proves nothing about the Lua engine having read them -- which is the only
  thing that matters, and which has four independent ways to be false (the
  engine not compiled in, ALE.Enabled off, ALE.ScriptPath pointing
  somewhere else, the world not restarted since the copy).

  So the arrival is proved by the SCRIPT ANSWERING. Two ways at once,
  because they fail differently:

    * handler:SendSysMessage() writes into the caller's own reply buffer.
      For a SOAP caller that buffer IS the <result> element, so the answer
      comes back on the same wire as the question (AC ACSoap.cpp:133-140,
      ALE ChatHandlerMethods.h:29). And because the hook returns false the
      core never sets the error flag, so the reply flips from a
      <faultstring> to a <result>: absent and present cannot be confused.
    * print() goes to the worldserver's own log, which is what an operator
      reading a log after the fact has.

  The word DML-BRIDGE-READY is deliberately not a word any server command
  says. "ok" would have been indistinguishable from some other command on
  the server answering to this name.
============================================================ --]]

-- PLAYER_EVENT_ON_COMMAND = 42. ALE pushes (player, text, handler); the
-- player is nil ONLY for handler:IsConsole(), which is the console, RA and
-- SOAP (ALE PlayerHooks.cpp:42-62). See the guard below.
local function OnBridgePingCommand(event, player, command, handler)
    if player ~= nil then return end  -- console / SOAP origin only

    if not command:match("^dml_bridge_ping%s*$") then return end

    local answer = "DML-BRIDGE-READY dml_bridge_ping"
    if handler ~= nil then
        handler:SendSysMessage(answer)
    end
    print("[dml_bridge_ping] " .. answer)
    return false
end

RegisterPlayerEvent(42, OnBridgePingCommand)
print("[dml_bridge_ping] loaded -- DML-BRIDGE-READY on request")
