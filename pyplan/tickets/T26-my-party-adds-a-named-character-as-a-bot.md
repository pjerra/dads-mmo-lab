# T26 — My Party adds a chosen existing character as a bot (own alts, other accounts, friends' and family's characters)

**Status:** OPEN (spec to be written by the lead; waits on T13's live half for the box)
**Filed:** 2026-09-10 18:26 CEST by the lead (Fable), from the owner's answer on 8.6 through the question tool: "want to be able to choose alts, that way we can play with altbots there … Alts also from another account, or friends/family chars. This is only for AzerothCore as I know of."
**Hand:** Opus (a feature with a live half), worktree branched from `yulon-phase8b`
**Box:** `yulon-ubuntu2` (WotLK; the AzerothCore playerbots module is the only tree with the route), after T13's live half and T23

## What the owner wants

Beside "Add a bot of a chosen class", My Party offers "Add this character": a named existing character joins the party as a bot -- the player's own alt on the same account, a character on another account, or a friend's or family member's character. This is the playerbots module's alt-bot route (`.bot add <name>` / `.playerbots bot add`, with the module's own rules on whose characters may be added: same account, guild mates, and the `AiPlayerbot.AllowGuildBots` / `RandomBotAccount` settings -- read the fork's source on the box before writing the spec, never from memory).

## Before the spec

- Measure on `yulon-ubuntu2`: what `.bot add <name>` accepts for (a) an alt on the same account, (b) a character on a different account not in the guild, (c) a guild mate on another account; which conf keys gate each; whether the bridge can issue it through the existing Lua/SOAP seam or needs a new script.
- Decide the picker's source: the characters table (name, account, level, class) filtered to what the module will accept, with the refusal named when it will not.
- 8.6 stays open until this works live with T16 and T18.
