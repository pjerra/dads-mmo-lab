# Autobuff + reagent-free + multi-summons — deploy (staged 2026-08-15)

Everything is in `deploy-all.patch` (verified `git apply --check` clean on the
VM; Lua parses clean under lua5.1). **Do not deploy while a rebuild is running.**

## What ships

1. `env/dist/etc/modules/lua_scripts/dml_autobuff.lua` — opt-in auto-buffer
   (`#buffs on` in-game). Lands on the live host mount; loads at worldserver start.
2. `modules/mod-unbound/src/UnboundReagentFree.cpp` + loader edit — strips
   casting reagents (soul shards, candles, powders, symbols, seeds, runes,
   ankh, corpse dust) at startup. Crafting mats untouched.
3. `mod-multiclass-summons` — already in the image, nothing to do.

## Steps (after the user's own rebuild settles)

1. Apply (paste-block if the classifier balks):
   `ssh -i ~/.ssh/dml_vm perzi@100.99.161.102 "git -C C:/Users/perzi/dml-native/wow-server-playerbots apply -v -" < deploy-all.patch`
2. Rebuild worldserver (the C++ needs it; recorded docker recipe, quoted `set` form).
3. ONE restart ships this + the parked Feral Spirit coexist fix (already applied
   to source 2026-08-15, so any rebuild after that date includes it).

## Verify after restart

- Worldserver log: `[Unbound] reagent-free: stripped casting reagents from N spells`
  (expect N in the low hundreds) and `[dml_autobuff] loaded`.
- In-game: `#buffs on` → buffs land on player + pet within 10 s, no mana/reagents.
- `#buffs aspect hawk` switches and persists across relog (`dml_autobuff` table).
- Summon Voidwalker with imp out and zero shards: both demons up (multiclass-summons
  + reagent-free together).
- Feral Spirit with hunter pet out: wolves + pet coexist, Call Pet not blocked.
