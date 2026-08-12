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

-- Spellbook-driven tabs: the real "General" tab (tab 1) -- split into a
-- Professions tab and the rest -- plus any tab the server grants past the
-- class's three school tabs (GM accounts get an extra one on this realm).
-- The school tabs themselves stay out -- the class tabs already cover
-- those abilities from the ID lists. Runs from FinalizeScan, so the
-- existing SPELLS_CHANGED wiring keeps these live; a tab with no spells
-- is simply not kept.
local BOOK_SCHOOL_TAB_COUNT = 3

-- On this client an invalid slot makes GetSpellName THROW instead of
-- returning nil, and unreadable slots show up in TWO ways: GM tab sizes
-- overshoot the real array (failures at the tail), and server-granted
-- spells missing from the client's DBCs throw MID-tab (found live: a
-- first-failure break hid everything sorted after them, professions
-- included). So a failed slot is SKIPPED, and only a long unbroken run
-- of failures is treated as the end of the array.
local BOOK_SLOT_MISS_LIMIT = 50

function USB:ScanBookTabs()
    self.bookTabs = {}
    self.bookTabsByKey = {}

    if type(GetNumSpellTabs) ~= "function"
        or type(GetSpellTabInfo) ~= "function" then
        return
    end

    local seenNames = {}

    local function KeepBookTab(name, entries)
        if #entries == 0 then
            return
        end

        table.sort(entries, function(a, b)
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
        seenNames[name] = true
    end

    local tabCount = GetNumSpellTabs() or 0

    for tabIndex = 1, tabCount do
        local tabName, _, offset, spellCount = GetSpellTabInfo(tabIndex)
        local isExtra = tabIndex > 1 + BOOK_SCHOOL_TAB_COUNT

        if tabName and tabName ~= ""
            and (tabIndex == 1 or isExtra)
            and not seenNames[tabName] then
            local entries = {}
            local professions = {}
            local misses = 0
            offset = offset or 0
            spellCount = spellCount or 0

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

                    local entry = {
                        bookTabName = tabName,
                        slot = slot,
                        spellID = spellID,
                        name = spellName,
                        rankText = rankText or "",
                        rankNumber = RankNumber(rankText),
                        icon = icon,
                    }

                    if tabIndex == 1 and IsProfessionSpell(spellID, spellName) then
                        entry.bookTabName = "Professions"
                        table.insert(professions, entry)
                    else
                        table.insert(entries, entry)
                    end
                end
            end

            KeepBookTab(tabName, entries)
            if tabIndex == 1 then
                KeepBookTab("Professions", professions)
            end
        end
    end
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
