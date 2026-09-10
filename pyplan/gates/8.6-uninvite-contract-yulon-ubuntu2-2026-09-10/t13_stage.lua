--[[ ============================================================
  t13_stage.lua -- the T13 live half's STAGING HARNESS. Not part of the
  app, not shipped, not in `party.BRIDGE_SCRIPTS`, and removed from the
  server (with a world restart) by this folder's `08-teardown.sh`.

  Why it exists. The contract under test is "dml_uninvite removes a bot
  only from the party of the master that asked", so the live half needs
  TWO parties that a person did not have to be sitting at a client to
  build. Every route the server offers a console/SOAP caller was tried
  first and none of them can make a group:

    * `.group join` / `.group leader` / `.group disband` are filtered out
      of the console's own USAGE list (`.help group` on this box answers
      with `group list` alone) and refuse when called anyway -- measured,
      `03-stage.log`.
    * `dml_addclass <bot> mage` runs and creates nothing (same log). The
      cause was NOT determined: "the master is a bot" fits, and so does
      `RNDBOT0` already holding 10 characters with `CharactersPerRealm = 10`
      (`03b-addclass-cause.log`, README section 6).
    * `.playerbots bot ...` is Console::No by design -- that is the whole
      reason `dml_addclass.lua` exists.

  So the two parties are built here, through the same Eluna Group API the
  bridge under test reads (`Player:GetGroup`, `Group:IsMember`). What this
  harness makes is a real core Group -- the same object a client-made party
  is, written to `group_member` by the core -- and `dml_uninvite.lua` never
  learns where it came from.

  What it deliberately does NOT do: it never calls RemoveFromGroup, never
  answers in the words `party.py` parses, and registers a command name no
  part of the app ever sends.

      dml_t13_stage group   <leader> <member>   leader:GroupCreate(member)
      dml_t13_stage add     <member> <newbie>   member's group :AddMember
      dml_t13_stage leader  <player>            hand leadership to <player>
      dml_t13_stage disband <player>            disband <player>'s group
      dml_t13_stage show    <player> [other]    what the server sees now
============================================================ --]]

local function OnStageCommand(event, player, command, handler)
    if player ~= nil then return end

    local verb, a, b = command:match("^dml_t13_stage%s+(%S+)%s+(%S+)%s*(%S*)$")
    if not verb then return end
    if b == "" then b = nil end

    local function say(msg)
        if handler ~= nil then
            handler:SendSysMessage(msg)
        end
        print("[t13_stage] " .. msg)
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

    if verb == "group" then
        local ok, err = pcall(function() pa:GroupCreate(pb) end)
        say(string.format("stage group %s + %s -> ok=%s err=%s", a, b, tostring(ok), tostring(err)))
    elseif verb == "add" then
        local g = pa:GetGroup()
        if g == nil then
            say(string.format("stage add: %s has no group", a))
            return false
        end
        local ok, err = pcall(function() g:AddMember(pb) end)
        say(string.format("stage add %s -> %s's group ok=%s err=%s", b, a, tostring(ok), tostring(err)))
    elseif verb == "leader" then
        local g = pa:GetGroup()
        if g == nil then
            say(string.format("stage leader: %s has no group", a))
            return false
        end
        local ok, err = pcall(function() g:SetLeader(pa:GetGUID()) end)
        say(string.format("stage leader -> %s ok=%s err=%s", a, tostring(ok), tostring(err)))
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
        local isleader = "?"
        pcall(function() isleader = tostring(g:IsLeader(pa:GetGUID())) end)
        local msg = string.format(
            "stage show: %s is grouped; members=%s leaderGUID=%s %s-is-leader=%s",
            a, tostring(g:GetMembersCount()), leader, a, isleader
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
print("[t13_stage] loaded -- T13 live-half staging harness (not part of the app)")
