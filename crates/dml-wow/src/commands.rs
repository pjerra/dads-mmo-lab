//! Native-mode **in-game command reference** reader (`wow commands`, task
//! D1a of the read chunk). Faithful port of the `commands` arm
//! (`cli/src/90-main.sh:5734-5795`) + the static case-table in
//! `cli/src/47-commands.sh` (`_cmd_block_for`, copied VERBATIM from the DML
//! server manager — see that file's own header comment for the "do not
//! hand-edit the text" rule this port also honors).
//!
//! Shape: `{"mods":[{"key","name","text"}]}` — one entry per module that is
//! BOTH installed AND has a text block in [`cmd_block_for`]. Install-state
//! detection is delegated to [`super::modules::ModuleReader`] (the same
//! reader `wow_module_read` uses) so the two arms can never disagree about
//! what "installed" means.

use std::collections::HashSet;
use std::path::Path;

use serde_json::{json, Value};

use super::modules::{valid_cpp_key, ModuleReader};

/// A port of `_cmd_block_for` (`cli/src/47-commands.sh:22-410`): the static
/// case-table of cheat-sheet text blocks, one per mod key. `None` for any
/// key with no block (mirrors the bash case statement falling through with
/// no match, which `_cmd_block_for` "$key" then prints as an empty string).
///
/// Each block is the `printf '%s\n' <lines…>` argument list joined with
/// `\n` and NO trailing newline — exactly what `ctext="$(_cmd_block_for
/// "$mk")"` captures (command substitution strips all trailing newlines).
pub fn cmd_block_for(key: &str) -> Option<String> {
    let lines: &[&str] = match key {
        "mod-1v1-arena" => &[
            "1v1 Arena",
            "Private one-on-one arena duels. Players can queue from anywhere, challenge others, and track wins/losses. Fully automatic — requires no GM setup beyond placing the Battlemaster NPC.",
            "",
            "Commands:",
            ".q1v1 rated              — Queue for a rated 1v1 arena match",
            ".q1v1 unrated            — Queue for an unrated 1v1 arena match",
            ".q1v1 stats              — View your personal 1v1 win/loss statistics",
            "",
            "NPC Spawn Commands (worldserver console; prefix with . for in-game GM):",
            "npc add 999991 0 -8828.3 630.2 94.1 3.7   — Stormwind Arena Battlemaster (Alliance)",
            "npc add 999991 1 1592.5 -4401.5 6.6 4.5  — Orgrimmar Arena Battlemaster (Horde)",
        ],
        "mod-arac" => &[
            "All Races All Classes (ARAC)",
            "Unlocks all race/class combinations not normally available — Night Elf Warrior, Undead Paladin, etc. DATA-ONLY: no worldserver rebuild required. Requires a world DB SQL import and updated DBC files on both server and client.",
            "",
            "Install: click Install on the Modules page (arac.sql is applied automatically",
            "by the server on its next cold start), then \"Apply client patch\" on the same",
            "row — it copies the server DBC files into the data volume and Patch-A.MPQ",
            "into your WoW client Data/ folder. Restart the server afterwards.",
            "",
            "Manual fallback: copy modules/mod-arac/Patch-A.MPQ into <client>/Data/.",
            "Commands: (none — all race/class combos unlocked at character creation)",
        ],
        "mod-dungeon-master" => &[
            "Dungeon Master",
            "Procedural roguelike dungeon challenge system. Talk to the Dungeon Master NPC (entry 500000), pick a difficulty tier (Novice → Grandmaster), creature theme, and dungeon — then enter a repopulated instance scaled to your level. Roguelike mode chains dungeons with escalating difficulty, Mythic+-style affixes, and stacking stat buffs.",
            "",
            "37 dungeons, 9 creature themes, 6 difficulty tiers, party + solo support.",
            "NPC auto-spawns in all major cities on server start. SQL auto-applied on next start.",
            "",
            "GM Commands:",
            "[GM] .dm reload           — Reload Dungeon Master config",
            "[GM] .dm status           — Show active dungeon sessions",
            "[GM] .dm list             — List available dungeons",
            "[GM] .dm end              — Force-end active session",
            "[GM] .dm clearcooldown    — Clear per-character run cooldown",
            "",
            "NPC Spawn Commands (worldserver console; prefix with . for in-game GM):",
            "npc add 500000            — Dungeon Master NPC (auto-placed in all capitals at start)",
        ],
        "mod-ah-bot" => &[
            "Auction House Bot",
            "Populates the Auction House with NPC-driven listings so players always have items to buy and sell. Configurable pricing, quantity, and categories. Requires a dedicated bot character.",
            "",
            "Commands (GM Rank 3+ only):",
            "[GM] .ahbotoptions minprice <n>     — Set minimum item price",
            "[GM] .ahbotoptions maxprice <n>     — Set maximum item price",
            "[GM] .ahbotoptions mintime <n>      — Set minimum auction duration (hours)",
            "[GM] .ahbotoptions maxtime <n>      — Set maximum auction duration (hours)",
            "[GM] .ahbotoptions minbidprice <n>  — Minimum bid price multiplier",
            "[GM] .ahbotoptions maxbidprice <n>  — Maximum bid price multiplier",
            "[GM] .ahbotoptions maxstack <n>     — Max stack size per listing",
            "[GM] .ahbotoptions minitem <n>      — Minimum different items in AH",
            "[GM] .ahbotoptions maxitem <n>      — Maximum different items in AH",
            "[GM] .ahbotoptions buyprice <n>     — Buyout price multiplier",
            "[GM] .ahbotoptions seller <n>       — Enable/disable bot as seller (1/0)",
            "[GM] .ahbotoptions buyer <n>        — Enable/disable bot as buyer (1/0)",
            "[GM] .ahbotoptions allfaction <n>   — Apply to all factions (1/0)",
            "[GM] .ahbotoptions alliance <n>     — Configure Alliance AH separately",
            "[GM] .ahbotoptions horde <n>        — Configure Horde AH separately",
        ],
        "mod-autobalance" => &[
            "AutoBalance",
            "Dynamically scales dungeon and raid difficulty based on player count, so solo or small-group play is viable. Supports per-dungeon overrides and level scaling.",
            "",
            "Commands (GM only):",
            "[GM] .autobalance getoffset     — Get difficulty offset for current map",
            "[GM] .autobalance setoffset <n> — Set difficulty offset for current map",
            "[GM] .autobalance mapstat       — Show AutoBalance stats for current map",
            "[GM] .autobalance creaturestat  — Show AutoBalance stats for target creature",
            "",
            "Aliases: .ab getoffset / .ab setoffset / .ab mapstat / .ab creaturestat",
        ],
        "mod-challenge-modes" => &[
            "Challenge Modes",
            "Adds optional self-imposed difficulty rules — Hardcore (death = permadeath), Semi-Hardcore, Ironman, and more. Players opt-in via an NPC (Shrine of Challenge) or the game settings menu. Requires EnablePlayerSettings = 1 in worldserver.conf.",
            "",
            "Source: nl-saw fork — OnPlayerResurrect signature patched automatically on install.",
            "",
            "Commands: (none — all functionality is accessed through the Shrine of Challenge NPC or in-game Settings menu)",
            "",
            "Configuration: edit challenge_modes.conf after a worldserver rebuild.",
        ],
        "mod-solocraft" => &[
            "SoloCraft",
            "Automatically buffs solo players in group content (dungeons, raids) to make progression viable. Scales stats dynamically — fully automatic, no player commands needed.",
            "",
            "Commands: (none — fully automatic)",
        ],
        "mod-transmog" => &[
            "Transmogrification",
            "Lets players change the visual appearance of gear without changing stats. Requires a Transmogrifier NPC placed in the world (entry 190010). Optionally portable and/or toggleable via commands.",
            "",
            "Commands:",
            ".transmog                — Show current transmog status",
            ".transmog sync           — Sync transmog visuals",
            ".transmog portable       — Toggle portable transmog (interact from anywhere)",
            ".transmog interface      — Open transmog UI without the NPC",
            ".transmog disclaimer     — Show transmog usage disclaimer",
            "[GM] .transmog add <id>  — Add item to transmog collection by item ID",
            "[GM] .transmog check     — Check transmog database integrity",
            "[GM] .transmog reload    — Reload transmog script",
            "",
            "NPC Spawn Commands (worldserver console; prefix with . for in-game GM):",
            "npc add 190010 0 -8831.3 628.2 94.1 3.7   — Transmogrifier NPC, Stormwind (Alliance)",
            "npc add 190010 1 1595.0 -4401.5 6.9 4.5  — Transmogrifier NPC, Orgrimmar (Horde)",
        ],
        "mod-talentbutton" => &[
            "Talent Button",
            "Enables Dual Talent Specialization at level 10 (retail: level 40). Adds a button to unlearn talents from anywhere in the world — no class trainer visit required. Uses server-side script injection to provide seamless in-game UI integration.",
            "",
            "IMPORTANT: Requires an UNPATCHED WoW 3.3.5a client. Clients patched with tools",
            "like RCEPatcher will block the script injection and the button will not appear.",
            "",
            "Configuration: TalentButton.Enable = 1 in mod_talentbutton.conf",
            "",
            "Commands: (none — talent reset button available in the in-game talent UI)",
        ],
        "mod-individual-progression" => &[
            "Individual Progression",
            "NOTICE: This module repository could not be verified on GitHub — installation may fail.",
            "Tracks each player's individual content progression, unlocking new tiers as they complete content.",
            "",
            "Commands: (check module conf after install if the module is available)",
        ],
        "mod-player-bot-level-brackets" => &[
            "Player Bot Level Brackets",
            "Restricts AI PlayerBots to operate within configured level brackets, preventing high-level bots from trivialising lower-level content. Requires the Playerbots module.",
            "",
            "Commands:",
            "[ADMIN] .reload          — Reload server scripts (applies bracket config changes without restart)",
        ],
        "mod-npc-beastmaster" => &[
            "NPC Beastmaster",
            "Adds a Beastmaster NPC that allows Hunters to tame any pet, including exotic and normally-untameable creatures. Can be summoned anywhere via .beastmaster or placed permanently in capitals (entry 601026).",
            "",
            "Commands:",
            ".beastmaster             — Summon the Beastmaster NPC to your current location",
            ".petname rename <name>   — Rename your current pet",
            ".petname cancel          — Cancel a pending pet rename",
            "",
            "NPC Spawn Commands (worldserver console; prefix with . for in-game GM):",
            "npc add 601026 0 -8825.3 632.2 94.1 3.7   — White Fang (Beastmaster), Stormwind (Alliance)",
            "npc add 601026 1 1597.0 -4402.0 7.3 4.5  — White Fang (Beastmaster), Orgrimmar (Horde)",
        ],
        "mod-quest-loot-party" => &[
            "Quest Loot Party",
            "Distributes quest item loot to ALL eligible party members when any one member",
            "loots the item — eliminating repeated boss kills when questing in a group.",
            "",
            "Configuration: QuestParty.Enable (on/off), QuestParty.Message (notify players)",
            "",
            "Commands: (none — fully automatic)",
        ],
        "mod-aoe-loot" => &[
            "AoE Loot",
            "Allows players to loot all nearby corpses simultaneously with a single loot action. Toggleable per-player. Significantly speeds up grinding and farming. No GM configuration needed.",
            "",
            "Commands:",
            ".aoeloot on              — Enable AoE looting for yourself",
            ".aoeloot off             — Disable AoE looting for yourself",
        ],
        "mod-learn-spells" => &[
            "Learn Spells on Level Up",
            "Automatically teaches players all class spells when they level up, eliminating the need to visit trainers. Configurable to include/exclude specific spell types.",
            "",
            "Commands: (none — fully automatic on level-up)",
        ],
        "mod-junk-to-gold" => &[
            "Junk to Gold",
            "Automatically sells all grey (junk) items in a player's bags when they interact with any vendor. Saves time and removes inventory clutter without manual selling.",
            "",
            "Commands: (none — automatic when visiting any vendor)",
        ],
        "battlepass" => &[
            "Battle Pass (ALE)",
            "A complete seasonal XP progression system. Players earn XP from kills, quests,",
            "PvP, dungeons, and daily logins, then claim rewards (items, gold, titles, spells).",
            "Requires the Battle Pass NPC (entry 90100) placed in the world and the client",
            "addon installed in WoW Interface/AddOns/BattlePass/.",
            "",
            "Commands:",
            ".bp                      — Show your current Battle Pass progress",
            ".bp rewards              — List available rewards",
            ".bp claim <level>        — Claim reward for a specific level",
            ".bp claimall             — Claim all currently available rewards",
            ".bp preview [level]      — Preview upcoming rewards",
            "[GM] .bpadmin addxp <amount> [player]   — Grant XP to a player",
            "[GM] .bpadmin setlevel <level> [player] — Set a player's level",
            "[GM] .bpadmin unclaim <level> [player]  — Un-claim a reward",
            "[GM] .bpadmin reset [player]            — Reset a player's progress",
            "[GM] .bpadmin reload                    — Reload Battle Pass config",
            "[GM] .bpadmin stats                     — Show server-wide stats",
            "",
            "NPC Spawn Commands (worldserver console; prefix with . for in-game GM):",
            "npc add 90100 0 -8819.3 636.2 94.1 3.7   — Battle Pass NPC, Stormwind (Alliance)",
            "npc add 90100 1 1594.5 -4404.0 6.8 4.5  — Battle Pass NPC, Orgrimmar (Horde)",
        ],
        "paragon" => &[
            "Paragon Anniversary (ALE)",
            "A Paragon reputation and anniversary reward system. Players earn paragon points via reputation grinds and receive bonus rewards on server anniversary dates.",
            "",
            "Commands:",
            ".test                    — Debug command (WARNING: no GM guard — any player can use this; appears to be a debug leftover in source code; monitor for abuse)",
        ],
        "bmah" => &[
            "Black Market Auction House (ALE)",
            "Adds a Black Market Auction House NPC that lists rare and exclusive items at auction. Requires a companion client addon (AzerothCore-wotlk-client-modifications). NPC entry: 2069430.",
            "",
            "Commands: (none — all interaction is through the NPC gossip menu)",
            "",
            "NPC Spawn Commands (worldserver console; prefix with . for in-game GM):",
            "npc add 2069430 0 -8816.3 638.2 94.1 3.7   — Black Market AH Auctioneer, Stormwind (Alliance)",
            "npc add 2069430 1 1597.5 -4404.5 7.5 4.5  — Black Market AH Auctioneer, Orgrimmar (Horde)",
        ],
        "lootpet" => &[
            "Loot Pet (ALE)",
            "Summons a companion pet that automatically picks up nearby loot. Combines well with AoE Loot. Fully automatic once the Lua script is deployed. Uses creature entry 34587 internally.",
            "",
            "Commands: (none — the pet is summoned and loots automatically)",
        ],
        "accountwide" => &[
            "Account Wide (ALE)",
            "Synchronises playtime, reputation, achievements, currencies, and other data across all characters on the same account. Requires a characters DB schema. Individual systems are enabled per-config.",
            "",
            "Commands:",
            ".playtime                — Show total account-wide play time",
            ".played                  — Show account-wide played time",
            ".awplaytime              — Show account-wide play time (primary alias)",
            ".accountplaytime         — Show account-wide play time (alternate alias)",
            ".awplayed                — Show account-wide played time (alternate alias)",
        ],
        "activechat" => &[
            "Azeroth Chatter (ALE)",
            "Fills world chat with ambient, lore-grounded RP chatter from a roster of recurring named residents — each with a faction, role, and personality. Time-of-day, seasonal, and event-aware. Far richer than the original ActiveChat.",
            "",
            "Commands: (none — chat fires on server timers automatically)",
        ],
        "levelupreward" => &[
            "Level Up Reward (ALE)",
            "Awards a random class-appropriate equippable item on every level-up. Quality is rolled (10% epic / 25% rare / 65% uncommon) with a fallback chain. Armor type scales with level (e.g. Mail → Plate at 40). First level-up also teaches all class weapon proficiencies.",
            "",
            "Commands: (none — reward is granted automatically on level-up)",
        ],
        "sod" => &[
            "Season of Discovery Buff (ALE)",
            "Tiered Discoverer's Delight XP bonus that scales down as players level. Awards +300% at levels 1-10, stepping down to +50% at 71-79. Level 80 gets no buff. Auto-updates on level-up. Requires server DBC files and client MPQ patches — installed automatically. Sourced from the Dad's MMO Lab ALE-Kegs collection.",
            "",
            "Files required: Server Files/dbc/*.dbc (server), Client Files/data/Patch-Z.MPQ + enUS/patch-enUS-3.MPQ + Interface/Icons/Buff_SoD.blp (client)",
            "",
            "Commands: (none — buff applied automatically on login and level-up)",
        ],
        "sitmeanrest" => &[
            "Sit Means Rest (ALE)",
            "Automatically applies the Rested XP bonus whenever a player sits down, mimicking inn-style resting anywhere. Duration and regen spell are configurable.",
            "",
            "Commands: (none — triggered automatically by the /sit emote or sitting action)",
        ],
        "unlimitedammo" => &[
            "Unlimited Ammo (ALE)",
            "Allows Hunters and other ranged classes to shoot without consuming ammo. Toggle is per-session. Ships with ENABLED = false and must be configured to activate.",
            "",
            "Commands:",
            ".ua                      — Enable unlimited ammo for yourself (current session only; no .ua off)",
        ],
        "portals-capitals" => &[
            "Portals in All Capitals (SQL Mod)",
            "Adds portal game objects to every faction capital city, allowing quick travel between Stormwind, Orgrimmar, and other capitals without a Mage.",
            "",
            "Commands: (none — portals are world objects; interact with them directly in game)",
        ],
        "hearthstone-cd" => &[
            "Hearthstone Cooldown (SQL Mod)",
            "Reduces the Hearthstone cooldown from the WotLK default of 30 minutes. Options: 1 second, 1 minute, 5 minutes, 15 minutes, or 30 minutes (WotLK default).",
            "",
            "Commands: (none — takes effect after a server restart or .reload spells in-game)",
        ],
        "rare-drops" => &[
            "Rare Drops (SQL Mod)",
            "Increases the drop rates of rare-quality items from mobs and bosses, making gear progression feel more rewarding for private servers.",
            "",
            "Commands: (none — passive database modification, no restart needed)",
        ],
        "lvl1-mounts" => &[
            "Level 1 Mounts (SQL Mod)",
            "Lowers the level requirement on all mounts to level 1, so players can ride from the very start of the game. No restart needed.",
            "",
            "Commands: (none — passive database change, effective immediately)",
        ],
        "all-stackables" => &[
            "All Stackables (SQL Mod)",
            "Increases the maximum stack size on all stackable items (potions, reagents, trade goods, etc.) to a configurable amount (default 200).",
            "",
            "Commands: (none — passive database change, takes effect after server restart)",
        ],
        "buff-mobs" => &[
            "Buff Mobs (SQL Mod)",
            "Increases all creature HP, damage, armor, and attack speed (HP×2, DMG×1.5, ARM×1.5) for a more challenging experience. Mutually exclusive with Nerf Mobs, XBuff Mobs, and Baby Mobs.",
            "",
            "Commands: (none — database multipliers applied to creature_template)",
        ],
        "xbuff-mobs" => &[
            "XBuff Mobs — Extreme Difficulty (SQL Mod)",
            "Significantly increases all creature stats (HP×4, DMG×2, ARM×2). The hardest mob difficulty option. Mutually exclusive with all other mob tweak SQL mods.",
            "",
            "Commands: (none — database multipliers applied to creature_template)",
        ],
        "nerf-mobs" => &[
            "Nerf Mobs (SQL Mod)",
            "Reduces all creature stats (HP×0.5, DMG×0.75, ARM×0.75) for an easier experience. Mutually exclusive with Buff Mobs, XBuff Mobs, and Baby Mobs.",
            "",
            "Commands: (none — database multipliers applied to creature_template)",
        ],
        "baby-mobs" => &[
            "Baby Mobs — Trivial Difficulty (SQL Mod)",
            "Drastically reduces all creature stats (HP×0.25, DMG×0.25, ARM×0.25). Best for testing or casual play. Mutually exclusive with all other mob tweak SQL mods.",
            "",
            "Commands: (none — database multipliers applied to creature_template)",
        ],
        "npc-teleporter" => &[
            "NPC Teleporter (SQL Mod)",
            "Adds a teleporter NPC to capital cities and/or starting zones, allowing players to fast-travel to major locations including raids and dungeons. ONY_LEVEL controls Onyxia level requirement.",
            "",
            "Commands: (none — all interaction through the NPC gossip teleport menu)",
        ],
        "xp-rates" => &[
            "XP Rates (SQL Mod)",
            "Adjusts kill, quest, and exploration XP multipliers in worldserver.conf. Requires a reload config or server restart to apply changes.",
            "",
            "Commands: (none — edits worldserver.conf; use .reload config in-game to apply without a full restart)",
        ],
        "mod-custom-login" => &[
            "Custom Login (SQL Mod / Module)",
            "Gives new characters starter items, spells, or buffs on first login. Configurable via mod_customlogin.conf.",
            "",
            "Commands: (none — triggers automatically on first character login)",
        ],
        "mod-ale" => &[
            "AzerothCore Lua Engine (ALE)",
            "The core C++ module that enables Lua scripting on AzerothCore. Required by all ALE Lua Mods. Exposes server events to Lua scripts placed in the lua_scripts/ directory.",
            "",
            "Commands:",
            "[GM] .reload ale         — Reload all Lua scripts without restarting the worldserver",
        ],
        _ => return None,
    };
    Some(lines.join("\n"))
}

/// `commands` (`90-main.sh:5734-5795`): walks the cpp/lua/sql registries (+
/// custom cpp clones under `modules_dir`) from the cached module catalog,
/// keeping only mods that are BOTH installed AND have a [`cmd_block_for`]
/// block. `reader` supplies install-state (same `ModuleReader` `wow_module_
/// read` uses); `catalog` is the cached `dml wow module catalog --json`
/// `.data` object (`.families.{cpp,lua,sql}`, each row carrying at least
/// `key`/`name`).
pub fn assemble_commands(catalog: &Value, reader: &ModuleReader, modules_dir: &Path) -> Value {
    let fam = &catalog["families"];
    let mut mods: Vec<Value> = Vec::new();

    // --- cpp: registry rows first (order preserved) -------------------------
    let mut cpp_seen: HashSet<String> = HashSet::new();
    for row in fam["cpp"].as_array().cloned().unwrap_or_default() {
        let key = row.get("key").and_then(Value::as_str).unwrap_or("").to_string();
        if key.is_empty() {
            continue;
        }
        cpp_seen.insert(key.clone());
        if !reader.cpp_installed(&key) {
            continue;
        }
        let Some(text) = cmd_block_for(&key) else { continue };
        let name = row.get("name").and_then(Value::as_str).unwrap_or(&key).to_string();
        mods.push(json!({"key": key, "name": name, "text": text}));
    }

    // --- cpp: custom clones under modules/ (valid mod-* key, has .git, not
    // already a registry key). Sorted ascending to match the bash `*/` glob
    // (same convention as `ModuleReader::assemble`'s custom-clone scan). ---
    let mut customs: Vec<String> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(modules_dir) {
        for e in rd.flatten() {
            if !e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            if !e.path().join(".git").is_dir() {
                continue;
            }
            let name = e.file_name().to_string_lossy().to_string();
            if cpp_seen.contains(&name) || !valid_cpp_key(&name) {
                continue;
            }
            customs.push(name);
        }
    }
    customs.sort();
    for key in customs {
        let Some(text) = cmd_block_for(&key) else { continue };
        mods.push(json!({"key": key.clone(), "name": key, "text": text}));
    }

    // --- lua registry ---------------------------------------------------------
    for row in fam["lua"].as_array().cloned().unwrap_or_default() {
        let key = row.get("key").and_then(Value::as_str).unwrap_or("").to_string();
        if key.is_empty() || !reader.lua_cloned(&key) {
            continue;
        }
        let Some(text) = cmd_block_for(&key) else { continue };
        let name = row.get("name").and_then(Value::as_str).unwrap_or(&key).to_string();
        mods.push(json!({"key": key, "name": name, "text": text}));
    }

    // --- sql registry ---------------------------------------------------------
    for row in fam["sql"].as_array().cloned().unwrap_or_default() {
        let key = row.get("key").and_then(Value::as_str).unwrap_or("").to_string();
        if key.is_empty() || !reader.sql_installed(&key) {
            continue;
        }
        let Some(text) = cmd_block_for(&key) else { continue };
        let name = row.get("name").and_then(Value::as_str).unwrap_or(&key).to_string();
        mods.push(json!({"key": key, "name": name, "text": text}));
    }

    json!({"mods": mods})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmd_block_for_known_key_matches_verbatim_text() {
        let text = cmd_block_for("mod-1v1-arena").expect("block");
        assert!(text.starts_with("1v1 Arena\n"));
        assert!(text.contains("Stormwind Arena Battlemaster (Alliance)"));
        // No trailing newline (command-substitution semantics).
        assert!(!text.ends_with('\n'));
    }

    /// Every shipped `npc add` line must put the NPC on the FLOOR.
    ///
    /// Found live 2026-08-17, and it had shipped in every one of these blocks:
    /// all five Orgrimmar spawns used `z = 17.5`, which is the height of the
    /// upper city (the Drag) while their x/y sit in the Valley of Strength,
    /// where the floor is 6.5-9.1. The NPC spawned 7-10 yards in the air --
    /// visible, out of interaction range, so right-clicking it did nothing.
    /// The Stormwind lines were correct all along (94.1 matches that floor),
    /// which is what made the bug look like a per-module fault rather than
    /// one wrong constant repeated five times.
    ///
    /// The bug reached users twice over: `moduletail::npc_coord_specs` parses
    /// these same lines for the "Place NPC in capitals" button, AND the block
    /// is shown on the Commands page for people to type by hand. One wrong
    /// number, two broken paths.
    ///
    /// The floors below are measured, not chosen: each is the range of
    /// `creature.position_z` for real Blizzard NPCs standing in that capital's
    /// service area on the live server. A spawn more than 3 yards above the
    /// TOP of that range is airborne; more than 3 below the bottom is buried.
    #[test]
    fn every_shipped_npc_spawn_stands_on_the_floor() {
        // (map, floor_low, floor_high) — map 0 = Stormwind trade district,
        // map 1 = Orgrimmar Valley of Strength.
        const FLOORS: [(u32, f64, f64); 2] = [(0, 94.1, 95.7), (1, 6.5, 9.2)];
        const SLACK: f64 = 3.0;

        let keys = [
            "mod-1v1-arena", "mod-arac", "mod-dungeon-master", "mod-ah-bot", "mod-autobalance",
            "mod-challenge-modes", "mod-solocraft", "mod-transmog", "mod-talentbutton",
            "mod-individual-progression", "mod-player-bot-level-brackets", "mod-npc-beastmaster",
            "mod-quest-loot-party", "mod-aoe-loot", "mod-learn-spells", "mod-junk-to-gold",
            "battlepass", "paragon", "bmah", "lootpet",
        ];
        let mut checked = 0;
        for key in keys {
            let Some(block) = cmd_block_for(key) else { continue };
            for line in block.lines() {
                let t: Vec<&str> = line.split_whitespace().collect();
                // Same shape npc_coord_specs requires: `npc add <entry> <map> <x> <y> <z> <o>`.
                if t.len() < 8 || t[0] != "npc" || t[1] != "add" {
                    continue;
                }
                let (Ok(map), Ok(z)) = (t[3].parse::<u32>(), t[6].parse::<f64>()) else { continue };
                let Some(&(_, lo, hi)) = FLOORS.iter().find(|(m, _, _)| *m == map) else { continue };
                assert!(
                    z >= lo - SLACK && z <= hi + SLACK,
                    "{key}: spawn z={z} on map {map} is off the floor ({lo}..{hi}). \
                     Airborne NPCs are visible but un-right-clickable, which reads as \
                     'the module is broken'. Line: {line}"
                );
                checked += 1;
            }
        }
        // A guard that silently checked nothing would pass forever.
        assert!(checked >= 10, "expected to check many spawn lines, only saw {checked}");
    }

    #[test]
    fn cmd_block_for_unknown_key_is_none() {
        assert_eq!(cmd_block_for("mod-does-not-exist"), None);
        // Registered module with NO cheat-sheet block (verified against
        // `_module_registry_cpp`/`70-modules.sh`).
        assert_eq!(cmd_block_for("mod-ah-bot-plus"), None);
    }

    #[test]
    fn cmd_block_for_preserves_blank_lines_and_apostrophes() {
        let text = cmd_block_for("mod-junk-to-gold").expect("block");
        assert!(text.contains("a player's bags"));
        let lines: Vec<&str> = text.split('\n').collect();
        assert_eq!(lines[2], ""); // blank separator line, verbatim
    }

    fn touch_git(dir: &Path) {
        std::fs::create_dir_all(dir.join(".git")).unwrap();
    }

    #[test]
    fn assemble_commands_gates_on_installed_and_block_presence() {
        let base = std::env::temp_dir().join(format!("dml-cmds-{}", std::process::id()));
        let modules_dir = base.join("modules");
        std::fs::create_dir_all(&modules_dir).unwrap();
        // mod-solocraft: installed + has a block -> included.
        touch_git(&modules_dir.join("mod-solocraft"));
        // mod-ah-bot-plus: installed but NO block -> excluded.
        touch_git(&modules_dir.join("mod-ah-bot-plus"));
        // mod-transmog: has a block but NOT installed -> excluded.

        let catalog = json!({
            "families": {
                "cpp": [
                    {"key": "mod-solocraft", "name": "Solocraft"},
                    {"key": "mod-ah-bot-plus", "name": "AH Bot Plus"},
                    {"key": "mod-transmog", "name": "Transmog"},
                ],
                "lua": [
                    {"key": "sod", "name": "Season of Discovery"},
                ],
                "sql": [
                    {"key": "rare-drops", "name": "Rare Drops"},
                ],
            }
        });
        let reader = ModuleReader::for_title(&base);
        let data = assemble_commands(&catalog, &reader, &modules_dir);
        let mods = data["mods"].as_array().unwrap();
        let keys: Vec<&str> = mods.iter().map(|m| m["key"].as_str().unwrap()).collect();
        assert_eq!(keys, vec!["mod-solocraft"]);
        assert_eq!(mods[0]["name"], "Solocraft");
        assert!(mods[0]["text"].as_str().unwrap().starts_with("SoloCraft\n"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn assemble_commands_includes_custom_clone_when_key_has_a_block() {
        let base = std::env::temp_dir().join(format!("dml-cmds-custom-{}", std::process::id()));
        let modules_dir = base.join("modules");
        std::fs::create_dir_all(&modules_dir).unwrap();
        // A "custom" clone whose directory name happens to match a static
        // block key that is NOT in the (synthetic, empty-here) registry.
        touch_git(&modules_dir.join("mod-ale"));
        let catalog = json!({"families": {"cpp": [], "lua": [], "sql": []}});
        let reader = ModuleReader::for_title(&base);
        let data = assemble_commands(&catalog, &reader, &modules_dir);
        let mods = data["mods"].as_array().unwrap();
        assert_eq!(mods.len(), 1);
        assert_eq!(mods[0]["key"], "mod-ale");
        assert_eq!(mods[0]["name"], "mod-ale");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn assemble_commands_dedups_registry_key_against_custom_scan() {
        let base = std::env::temp_dir().join(format!("dml-cmds-dedup-{}", std::process::id()));
        let modules_dir = base.join("modules");
        std::fs::create_dir_all(&modules_dir).unwrap();
        touch_git(&modules_dir.join("mod-solocraft"));
        let catalog = json!({
            "families": {
                "cpp": [{"key": "mod-solocraft", "name": "Solocraft"}],
                "lua": [],
                "sql": [],
            }
        });
        let reader = ModuleReader::for_title(&base);
        let data = assemble_commands(&catalog, &reader, &modules_dir);
        let mods = data["mods"].as_array().unwrap();
        // Exactly one row -- the registry row, not a duplicate custom row.
        assert_eq!(mods.len(), 1);
        let _ = std::fs::remove_dir_all(&base);
    }
}
