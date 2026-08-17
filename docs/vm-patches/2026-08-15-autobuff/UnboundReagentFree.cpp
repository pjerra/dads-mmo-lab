#include "Log.h"
#include "ScriptMgr.h"
#include "SpellInfo.h"
#include "SpellMgr.h"
#include <unordered_set>

// UnboundReagentFree — strip CASTING reagents server-wide (user-approved scope
// 2026-08-15): soul shards, candles, powders, symbols, seeds, runes, ankh,
// corpse dust, infernal/demonic stones. Profession crafting materials are NOT
// reagents on crafting spells (those are itemized via the trade-skill system's
// spell reagents too — which is exactly why this list is a curated ITEM set and
// not a blanket wipe: only spells whose reagent is one of these consumables
// are touched, so blacksmithing mats etc. stay required).
//
// Runs at world startup, after the spell store is loaded (same pattern as
// mod-multiclass-summons' attribute fix-up). This is what lets warlock demons
// chain-summon shard-free through mod-multiclass-summons.

namespace
{
    std::unordered_set<int32> const kCastingReagents = {
        6265,  // Soul Shard
        5565,  // Infernal Stone
        16583, // Demonic Figurine
        17020, // Arcane Powder
        17021, // Wild Berries
        17026, // Wild Thornroot
        22148, // Wild Quillvine
        17028, // Holy Candle
        17029, // Sacred Candle
        17030, // Ankh
        17031, // Rune of Teleportation
        17032, // Rune of Portals
        17033, // Symbol of Divinity
        21177, // Symbol of Kings
        17034, // Maple Seed
        17035, // Stranglethorn Seed
        17036, // Ashwood Seed
        17037, // Hornbeam Seed
        17038, // Ironwood Seed
        22147, // Flintweed Seed
        37201, // Corpse Dust
        17057, // Shiny Fish Scales
        17058, // Fish Oil
        44605, // Wild Spineleaf   (WotLK Gift of the Wild top rank)
        44614, // Starleaf Seed    (WotLK Rebirth top rank)
        44615, // Devout Candle    (WotLK Prayer of Fortitude top rank)
    };
}

class UnboundReagentFree : public WorldScript
{
public:
    UnboundReagentFree() : WorldScript("UnboundReagentFree") { }

    void OnStartup() override
    {
        uint32 changed = 0;
        for (uint32 id = 0; id < sSpellMgr->GetSpellInfoStoreSize(); ++id)
        {
            SpellInfo const* info = sSpellMgr->GetSpellInfo(id);
            if (!info)
                continue;

            bool touched = false;
            SpellInfo* mut = const_cast<SpellInfo*>(info);
            for (uint8 i = 0; i < MAX_SPELL_REAGENTS; ++i)
            {
                if (mut->Reagent[i] > 0 && kCastingReagents.count(mut->Reagent[i]))
                {
                    mut->Reagent[i] = 0;
                    mut->ReagentCount[i] = 0;
                    touched = true;
                }
            }

            if (touched)
                ++changed;
        }

        LOG_INFO("module", "[Unbound] reagent-free: stripped casting reagents from {} spells", changed);
    }
};

void AddUnboundReagentFree()
{
    new UnboundReagentFree();
}
