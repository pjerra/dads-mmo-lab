# T60 — 32 of 37 cloned manifests pin nothing, and nothing checks the 5 that do

**Status:** FILED — scoped, not started. Two decisions are the owner's (below).
**Filed:** 2026-09-14 by the lead. The "bigger half" T52 left open, plus a gap #164's review found.
**Branch:** `ticket/t60-manifests-pin-nothing`, from `upstream/Yulon` 936beda5.

## The count

Read from `pylauncher/manifests/*/*/*.json` on the #164 branch (upstream plus the Loot Pet repin):

| game | pinned (`rev`) | `branch` only | unpinned | no `source` |
|---|---|---|---|---|
| wow-wotlk | 3 | 0 | **32** | 5 |
| wow-tortoise | 2 | 0 | 0 | 4 |
| wow-tbc | 0 | 0 | 0 | 5 |
| wow-vanilla | 0 | 0 | 0 | 5 |

Pinned: `wow-wotlk/modules/mod-ale`, `wow-wotlk/ale/lootpet`, `wow-wotlk/ale/sitmeanrest`,
`wow-tortoise/mods/tortoise-bots-manager`, `wow-tortoise/mods/tortoise-gm-manager`.

The mechanism is already built: `Source.rev` accepts only a full 40-hex SHA and
`git.CloneSpec.rev` honours it (roadmap 7.3). What is missing is using it, and checking it.

## Why it matters

T52: `mod-challenge-modes` was unpinned; its upstream `master` kept a hook signature AzerothCore
had changed, and a user's rebuild died at object 1270 of 1922, three times across two versions.
Every unpinned entry is that failure waiting for its upstream to move. Two machines installing the
same module a week apart can build different code.

## And the pins that exist are trusted blind

#164's review round 2: the catalog test builds its fake clone FROM the manifest, so a `rev` that
does not exist, or that does not contain the `src` a deploy names, passes every test and fails
only on a user's machine. Loot Pet's pin was checked by hand (`gh api .../git/trees/<rev>`,
recorded in `pyplan/gates/t59-lootpet-broken-in-upstream-2026-09-14/pin-provenance.txt`).
Nothing repeats that check.

## Owner decisions

1. **Pin to what.** The tip of each repository's default branch on the day of pinning (records
   today's behaviour, changes nothing for users), or a revision this app has actually built
   against its AzerothCore pin (stronger, costs a build per module on a test box).
2. **Who moves a pin.** Bumped by hand in a PR per module, or a scheduled job that proposes bumps.
   Either way a pin that never moves stops getting upstream fixes, which is the opposite failure.

## Definition of done (once decided)

1. Every cloned manifest carries a `rev`, or an entry in a named allow-list saying why not.
   A catalog test enforces it, so a new unpinned entry fails CI rather than a user's build.
2. A check that asks the repository, for every pinned manifest: does `rev` exist, and does its
   tree hold every `deploy[].src` (and `source.sparse_path`, where set)? Marked so the narrow
   suite stays offline — it needs the network, not Docker.
3. Where the Modules tab shows a version, a pinned entry says it is pinned.

## Not in scope

Compiling each module against the core — that is decision 1's stronger option, and a test box's
job, not CI's.
