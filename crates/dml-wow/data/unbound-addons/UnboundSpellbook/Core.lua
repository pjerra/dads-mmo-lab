UnboundSpellbook = UnboundSpellbook or {}
local USB = UnboundSpellbook

USB.BOOK_TYPE = BOOKTYPE_SPELL or "spell"
USB.entriesByClass = {}
USB.allEntries = {}
USB.bookTabs = {}
USB.bookTabsByKey = {}
USB.refreshCallbacks = {}
USB.scanning = false
USB.scanProgress = 0
USB.scanTotal = 0
USB.lastKnownCount = 0

local function RankNumber(rankText)
    if not rankText then
        return 0
    end
    return tonumber(string.match(rankText, "(%d+)")) or 0
end

local PROFESSION_ID_SET = {}
for _, spellID in ipairs(USB.PROFESSION_SPELL_IDS or {}) do
    PROFESSION_ID_SET[spellID] = true
end

local function IsProfessionSpell(spellID, spellName)
    if spellID and PROFESSION_ID_SET[spellID] then
        return true
    end
    if spellName and USB.PROFESSION_NAMES and USB.PROFESSION_NAMES[spellName] then
        return true
    end
    return false
end

-- Every class-list ID in one set: on this server tab 1 is stuffed with
-- whole class spell sets at all ranks, and extra school-tab sets appear
-- per unlocked class; both are recognised (and hidden) through this.
local CLASS_ID_SET = {}
for _, classSpellIDs in pairs(USB.CLASS_ACTIVE_SPELL_IDS or {}) do
    for _, spellID in ipairs(classSpellIDs) do
        CLASS_ID_SET[spellID] = true
    end
end

-- ShouldReplace for book entries, which may lack a spell ID.
local function BetterBookRank(existing, candidate)
    if not existing then
        return true
    end
    if candidate.rankNumber ~= existing.rankNumber then
        return candidate.rankNumber > existing.rankNumber
    end
    return (candidate.spellID or 0) > (existing.spellID or 0)
end

local function ShouldReplace(existing, candidate)
    if not existing then
        return true
    end

    if candidate.rankNumber > existing.rankNumber then
        return true
    end

    if candidate.rankNumber == existing.rankNumber
        and candidate.spellID > existing.spellID then
        return true
    end

    return false
end

function USB:Message(text)
    if DEFAULT_CHAT_FRAME then
        DEFAULT_CHAT_FRAME:AddMessage("|cff66ff66Unbound Spellbook:|r " .. text)
    end
end

function USB:Error(text)
    if UIErrorsFrame then
        UIErrorsFrame:AddMessage(text, 1, 0.25, 0.25, 1)
    end
    self:Message("|cffff6666" .. text .. "|r")
end

function USB:RegisterRefreshCallback(callback)
    table.insert(self.refreshCallbacks, callback)
end

function USB:NotifyRefresh()
    for _, callback in ipairs(self.refreshCallbacks) do
        callback()
    end
end

function USB:EnsureDB()
    if not UnboundSpellbookDB then
        UnboundSpellbookDB = {}
    end

    if not UnboundSpellbookDB.selectedTab then
        local _, playerClass = UnitClass("player")
        UnboundSpellbookDB.selectedTab = playerClass or "ALL"
    end

    return UnboundSpellbookDB
end

local scanJobs = {}
local scanIndex = 1
local classMaps = {}
local allMap = {}

local function FinalizeScan()
    USB.entriesByClass = {}

    for _, classKey in ipairs(USB.CLASS_ORDER) do
        local entries = {}
        for _, entry in pairs(classMaps[classKey]) do
            table.insert(entries, entry)
        end
        table.sort(entries, function(a, b)
            return a.name < b.name
        end)
        USB.entriesByClass[classKey] = entries
    end

    USB.allEntries = {}
    for _, entry in pairs(allMap) do
        table.insert(USB.allEntries, entry)
    end
    table.sort(USB.allEntries, function(a, b)
        return a.name < b.name
    end)

    USB.lastKnownCount = #USB.allEntries
    USB:ScanBookTabs()
    USB.scanning = false
    USB.scanProgress = USB.scanTotal
    USB:NotifyRefresh()
end

local scanFrame = CreateFrame("Frame")
scanFrame:Hide()

scanFrame:SetScript("OnUpdate", function(self)
    if not USB.scanning then
        self:Hide()
        return
    end

    -- Spread the direct checks over several frames so a huge multiclass
    -- character does not freeze the UI.
    local processed = 0
    local perFrame = 160

    while scanIndex <= #scanJobs and processed < perFrame do
        local job = scanJobs[scanIndex]
        scanIndex = scanIndex + 1
        processed = processed + 1
        USB.scanProgress = scanIndex - 1

        if IsSpellKnown(job.spellID) then
            local spellName, rankText, icon = GetSpellInfo(job.spellID)

            if spellName and spellName ~= "" then
                local entry = {
                    classKey = job.classKey,
                    spellID = job.spellID,
                    name = spellName,
                    rankText = rankText or "",
                    rankNumber = RankNumber(rankText),
                    icon = icon,
                }

                local currentClassEntry = classMaps[job.classKey][spellName]
                if ShouldReplace(currentClassEntry, entry) then
                    classMaps[job.classKey][spellName] = entry
                end

                local currentAllEntry = allMap[spellName]
                if ShouldReplace(currentAllEntry, entry) then
                    allMap[spellName] = entry
                end
            end
        end
    end

    USB:NotifyRefresh()

    if scanIndex > #scanJobs then
        FinalizeScan()
        self:Hide()
    end
end)

function USB:StartDirectScan()
    self:EnsureDB()

    scanJobs = {}
    classMaps = {}
    allMap = {}

    for _, classKey in ipairs(self.CLASS_ORDER) do
        classMaps[classKey] = {}

        for _, spellID in ipairs(self.CLASS_ACTIVE_SPELL_IDS[classKey]) do
            table.insert(scanJobs, {
                classKey = classKey,
                spellID = spellID,
            })
        end
    end

    scanIndex = 1
    self.scanTotal = #scanJobs
    self.scanProgress = 0
    self.scanning = true

    self:NotifyRefresh()
    scanFrame:Show()
end

function USB:GetEntries(tabKey)
    if tabKey == "ALL" then
        return self.allEntries
    end

    local bookTab = self.bookTabsByKey[tabKey]
    if bookTab then
        return bookTab.entries
    end

    return self.entriesByClass[tabKey] or {}
end

-- Spellbook-driven tabs -- always at most three, fixed: General,
-- Professions, GM. This server grants entire class sets (all ranks) into
-- spellbook tab 1 and injects whole school-tab sets per unlocked class,
-- so a raw mirror is unusable noise. General keeps only tab 1's spells
-- that are neither professions nor in any class ID list, collapsed to
-- their highest known rank; Professions is split out of tab 1; every
-- remaining non-school tab merges into the one GM tab, whatever the
-- server calls its internal tabs (school tabs are recognised by their
-- enUS names, with a conservative content tiebreaker for unknown names).
-- Runs from FinalizeScan, so the existing SPELLS_CHANGED wiring keeps
-- these live; a tab with nothing to show is simply not kept.

-- On this client an invalid slot makes GetSpellName THROW instead of
-- returning nil, and unreadable slots show up in TWO ways: GM tab sizes
-- overshoot the real array (failures at the tail), and server-granted
-- spells missing from the client's DBCs throw MID-tab (found live: a
-- first-failure break hid everything sorted after them, professions
-- included). So a failed slot is SKIPPED, and only a long unbroken run
-- of failures is treated as the end of the array.
local BOOK_SLOT_MISS_LIMIT = 50

-- A tab whose every slot is unlistable still renders -- as honest
-- "Unknown spell" placeholder rows (such spells exist only server-side,
-- so each slot read throws). Reported sizes overshoot for exactly these
-- tabs, so the placeholder row count cannot trust spellCount blindly;
-- the cap is a shared budget across everything merged into the GM tab.
local BOOK_PLACEHOLDER_CAP = 48

function USB:ScanBookTabs()
    self.bookTabs = {}
    self.bookTabsByKey = {}

    if type(GetNumSpellTabs) ~= "function"
        or type(GetSpellTabInfo) ~= "function" then
        return
    end

    local function KeepBookTab(name, entries)
        if #entries == 0 then
            return
        end

        table.sort(entries, function(a, b)
            if a.placeholder ~= b.placeholder then
                return not a.placeholder    -- readable entries first
            end
            if a.placeholder then
                return a.slot < b.slot      -- placeholders in book order
            end
            if a.passive ~= b.passive then
                return not a.passive        -- actives read before passives
            end
            if a.name == b.name then
                return a.rankNumber < b.rankNumber
            end
            return a.name < b.name
        end)

        local bookTab = {
            key = "BOOK:" .. name,
            name = name,
            entries = entries,
        }
        table.insert(self.bookTabs, bookTab)
        self.bookTabsByKey[bookTab.key] = bookTab
    end

    local tabCount = GetNumSpellTabs() or 0

    local generalName = "General"
    local generalByName = {}
    local professions = {}
    local gmEntries = {}
    local gmPlaceholderCount = 0
    local gmReadableCount = 0
    local suspectTabs = {}

    for tabIndex = 1, tabCount do
        local tabName, _, offset, spellCount = GetSpellTabInfo(tabIndex)
        offset = offset or 0
        spellCount = spellCount or 0

        if tabName and tabName ~= "" and spellCount > 0 then
            if tabIndex == 1 then
                generalName = tabName
            end

            local entries = {}
            local classHits = 0
            local misses = 0

            for slot = offset + 1, offset + spellCount do
                local ok, spellName, rankText = pcall(GetSpellName, slot, self.BOOK_TYPE)

                if not ok or not spellName or spellName == "" then
                    -- Unreadable slot: skip it (see BOOK_SLOT_MISS_LIMIT).
                    misses = misses + 1
                    if misses >= BOOK_SLOT_MISS_LIMIT then
                        break
                    end
                else
                    misses = 0

                    local spellID = nil
                    if GetSpellLink then
                        local okLink, link = pcall(GetSpellLink, slot, self.BOOK_TYPE)
                        if okLink and link then
                            spellID = tonumber(string.match(link, "spell:(%d+)"))
                        end
                    end

                    local icon = nil
                    if spellID then
                        local _, _, spellIcon = GetSpellInfo(spellID)
                        icon = spellIcon
                    end
                    if not icon and GetSpellTexture then
                        local okTexture, texture = pcall(GetSpellTexture, slot, self.BOOK_TYPE)
                        if okTexture then
                            icon = texture
                        end
                    end

                    -- Degenerate slot: a non-empty name with no ID, no icon
                    -- and no rank is this client's noise, not a spell.
                    if spellID or icon or (rankText or "") ~= "" then
                        local passive = false
                        if IsPassiveSpell then
                            local okPassive, isPassive = pcall(IsPassiveSpell, slot, self.BOOK_TYPE)
                            if okPassive then
                                passive = not not isPassive
                            end
                        end

                        local entry = {
                            bookTabName = tabName,
                            slot = slot,
                            spellID = spellID,
                            name = spellName,
                            rankText = rankText or "",
                            rankNumber = RankNumber(rankText),
                            icon = icon,
                            passive = passive,
                        }

                        if tabIndex == 1 then
                            if IsProfessionSpell(spellID, spellName) then
                                entry.bookTabName = "Professions"
                                table.insert(professions, entry)
                            elseif spellID and CLASS_ID_SET[spellID] then
                                -- Class ability: the class tabs already list
                                -- it. This is the bulk of the General-tab
                                -- noise on a server that grants whole class
                                -- sets there.
                            elseif BetterBookRank(generalByName[spellName], entry) then
                                -- Collapse to the highest known rank, the
                                -- same way the class tabs do.
                                generalByName[spellName] = entry
                            end
                        else
                            table.insert(entries, entry)
                            if spellID and CLASS_ID_SET[spellID] then
                                classHits = classHits + 1
                            end
                        end
                    end
                end
            end

            if tabIndex > 1 then
                -- School-tab sets are recognised by NAME first (the enUS
                -- set in ClassData): the server injects one per unlocked
                -- class, and the class tabs already cover those spells.
                -- Content is only a tiebreaker for UNKNOWN names, and a
                -- conservative one: a real school tab is essentially 100%
                -- class-list spells, so anything under 90% class hits
                -- keeps its entries.
                if USB.SCHOOL_TAB_NAMES and USB.SCHOOL_TAB_NAMES[tabName] then
                    -- School tab: hidden.
                elseif #entries == 0 then
                    -- Nothing listable in the whole tab: its spells
                    -- exist only server-side. Represent it with honest
                    -- placeholders; the tooltip still tries the slot.
                    local room = BOOK_PLACEHOLDER_CAP - gmPlaceholderCount
                    local placeholderCount = spellCount
                    if placeholderCount > room then
                        placeholderCount = room
                    end

                    for index = 1, placeholderCount do
                        local slot = offset + index
                        gmPlaceholderCount = gmPlaceholderCount + 1
                        table.insert(gmEntries, {
                            bookTabName = tabName,
                            slot = slot,
                            name = "Unknown spell (slot " .. slot .. ")",
                            rankText = "",
                            rankNumber = 0,
                            icon = "Interface\\Icons\\INV_Misc_QuestionMark",
                            passive = false,
                            placeholder = true,
                        })
                    end
                elseif classHits * 10 >= #entries * 9 then
                    -- Unknown name but overwhelmingly class spells: a
                    -- suspected school tab. Held back rather than dropped
                    -- -- see the guard below the tab loop.
                    table.insert(suspectTabs, entries)
                else
                    for _, entry in ipairs(entries) do
                        table.insert(gmEntries, entry)
                    end
                    gmReadableCount = gmReadableCount + #entries
                end
            end
        end
    end

    -- Final guard: the content tiebreaker must NEVER leave the GM tab
    -- without a single readable entry -- better one suspicious tab shown
    -- than the user's GM spells vanishing.
    if gmReadableCount == 0 and #suspectTabs > 0 then
        for _, suspect in ipairs(suspectTabs) do
            for _, entry in ipairs(suspect) do
                table.insert(gmEntries, entry)
            end
        end
    end

    local generalEntries = {}
    for _, entry in pairs(generalByName) do
        table.insert(generalEntries, entry)
    end

    -- Exactly three book tabs, at most: General, Professions, GM. A tab
    -- with nothing to show is dropped by KeepBookTab.
    KeepBookTab(generalName, generalEntries)
    KeepBookTab("Professions", professions)
    KeepBookTab("GM", gmEntries)
end

function USB:FindNativeSlot(entry)
    if not entry or not GetNumSpellTabs or not GetSpellTabInfo then
        return nil
    end

    local tabCount = GetNumSpellTabs() or 0
    local nameFallback = nil

    for tabIndex = 1, tabCount do
        local _, _, offset, spellCount = GetSpellTabInfo(tabIndex)
        offset = offset or 0
        spellCount = spellCount or 0

        local misses = 0

        for slot = offset + 1, offset + spellCount do
            local ok, bookName, bookRank = pcall(GetSpellName, slot, self.BOOK_TYPE)

            if not ok then
                -- Unreadable slot: skip it (see BOOK_SLOT_MISS_LIMIT).
                bookName = nil
                misses = misses + 1
                if misses >= BOOK_SLOT_MISS_LIMIT then
                    break
                end
            else
                misses = 0
            end

            if bookName == entry.name then
                if not nameFallback then
                    nameFallback = slot
                end

                if GetSpellLink then
                    local okLink, link = pcall(GetSpellLink, slot, self.BOOK_TYPE)
                    if not okLink then
                        link = nil
                    end
                    local linkedID = link and tonumber(string.match(link, "spell:(%d+)"))
                    if linkedID == entry.spellID then
                        return slot
                    end
                end

                if entry.rankText == "" or bookRank == entry.rankText then
                    return slot
                end
            end
        end
    end

    return nameFallback
end

function USB:PickupEntry(entry)
    if not entry then
        return
    end

    if InCombatLockdown and InCombatLockdown() then
        self:Error("Leave combat before dragging a spell.")
        return
    end

    if entry.placeholder then
        self:Error("The client cannot read this spell, so it cannot be dragged.")
        return
    end

    -- Spellbook-tab entries carry their own slot; re-check it still holds
    -- the same spell before trusting it, then fall back to the name search.
    local slot
    if entry.slot then
        local ok, bookName = pcall(GetSpellName, entry.slot, self.BOOK_TYPE)
        if ok and bookName == entry.name then
            slot = entry.slot
        end
    end
    slot = slot or self:FindNativeSlot(entry)
    if not slot then
        self:Error(
            "The ability is known, but the native client did not expose its spellbook slot."
        )
        return
    end

    if PickupSpell then
        PickupSpell(slot, self.BOOK_TYPE)
    else
        self:Error("PickupSpell is unavailable on this client.")
    end
end

local scheduleFrame = CreateFrame("Frame")
scheduleFrame:Hide()
local countdown = 0

function USB:ScheduleScan(delay)
    countdown = delay or 0.25
    scheduleFrame:Show()
end

scheduleFrame:SetScript("OnUpdate", function(self, elapsed)
    countdown = countdown - elapsed
    if countdown <= 0 then
        self:Hide()
        USB:StartDirectScan()
    end
end)

local eventFrame = CreateFrame("Frame")
eventFrame:RegisterEvent("PLAYER_LOGIN")
eventFrame:RegisterEvent("PLAYER_ENTERING_WORLD")
eventFrame:RegisterEvent("SPELLS_CHANGED")
eventFrame:RegisterEvent("LEARNED_SPELL_IN_TAB")

eventFrame:SetScript("OnEvent", function(_, event)
    if event == "PLAYER_LOGIN" or event == "PLAYER_ENTERING_WORLD" then
        USB:ScheduleScan(0.75)
    else
        USB:ScheduleScan(0.25)
    end
end)

SLASH_UNBOUNDSPELLBOOKRESCAN1 = "/usbkrescan"
SlashCmdList["UNBOUNDSPELLBOOKRESCAN"] = function()
    USB:StartDirectScan()
    USB:Message("direct known-spell scan started.")
end
