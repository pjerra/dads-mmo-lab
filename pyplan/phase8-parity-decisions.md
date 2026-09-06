# Phase 8 — Lab and Hypeer parity: decisions and why

> Scoped 2026-09-06 on branch `yulon-phase8`, before Phase 7 exits, on the owner's decision of
> 2026-09-04 that Phase 8 is scoped now and its code waits for this scoping to be reviewed
> (memory record `phase8-kickoff-prompt.md`; the brief is `pyplan/phase8-kickoff.md`). Companion to
> `pyplan/roadmap.md` §8, which is not edited; Appendix A holds the proposed §8 text. Sibling of
> `pyplan/phase8-decisions.md`, the uninstall/purge page of 2026-08-31, whose two owner answers are
> copied verbatim below as group (g) and which this page does not reopen.
>
> Method, the one `phase7-decisions.md` used: five read-only source reads produced
> `pyplan/phase8-delta.md` (one row per feature, mechanism measured per emulator tree at the
> catalog's pinned revisions; reports in `pyplan/phase8-reads/`); the owner answered the eight
> questions below before any design existed; three designs were then written independently from
> three angles and scored by three judges. Sections after "The cut" are written once the judge
> panel has reported; until then this page records the inputs to the design, nothing more.

---

## The owner's answers (2026-09-06)

Asked one at a time with `AskUserQuestion`, each with a recommendation first, before any design.
They are decisions, not preferences, and the rest of this page does not re-litigate them. The
option text is the owner's answer verbatim; where the owner typed a free answer, that text is
quoted first.

| # | Question | Answer |
|---|---|---|
| 1 | **Timing.** Does Phase 8 *code* wait for the Phase 7 exit box (Baerthe's), or may `8.1` start when its own prerequisites are met? | **"8.1 may start now, on yulon-phase8 stacked on #143"** |
| 2a | **Feature cut, group (a):** teleport, item mail, GM tools, gear sets. | **"v1 Phase 8"** — heal, summon-NPC and coordinate teleport were named in the question as refused unless Q7 or a Lua bridge changes that. |
| 2b | **Group (b):** My Party and Browse Bots. | **"v1 Phase 8, WotLK first"** |
| 2c | **Group (c):** character sheet (gear, 3D paperdoll, talent trees, achievements). | **"Phase 9"** |
| 2d | **Group (d):** Steam integration. | **"v1 Phase 8, Linux/Steam Deck only"** |
| 2e | **Group (e):** auto-stop when the client exits. | **"Later (after v1)"** |
| 2f | **Group (f):** doctor and shell. | **"Doctor in Phase 9; shell refused"** |
| 2g | **Group (g):** uninstall/purge, decided 2026-08-31 — does it stay in Phase 8? | **"Stays in Phase 8"** |
| 3 | **Family coverage.** All four servers before a box ticks, or WotLK first with the CMaNGOS-lineage servers as their own steps? | **"Mechanism verified per family before design; WotLK first in delivery; one box per family"** |
| 4 | **Bots on the CMaNGOS family.** Are bot features WotLK-only for v1? | **"My Party WotLK-only; Browse Bots on all four"** |
| 5 | **Client-side writes.** May a Phase 8 feature write into the client folder (an addon under `Interface/AddOns`, `realmlist.wtf`)? | **"Not allowed; server-side routes only"** |
| 6 | **Server-side channel.** Is SOAP on loopback plus an app-owned GM-3 account an acceptable prerequisite (WotLK, TBC, Vanilla; Tortoise has no SOAP)? | **"Yes: SOAP on loopback + an app account, as 8.1"** |
| 7 | **Database writes.** May a Phase 8 feature write to `characters` or `world` while the worldserver runs? | **"No direct writes to characters/world while running; reads are fine"** |
| 8i | **Added by the delta table — Hypeer-only rows:** live dashboard and the log snapshot before stop. | **"v1 Phase 8"** |
| 8ii | Accounts list, set password, GM level. | **"v1 Phase 8"** |
| 8iii | Module management beyond install/remove (update checks, manifests for the three CMaNGOS games, tuning knobs, config editor, settings page, account-wide sharing). | First **"Drop the account wide"**; on the clarifying question, **"Update checks + CMaNGOS manifests in Phase 8; knobs, editor, settings in Phase 9; account-wide refused"** |
| 8iv | Automatic backups, self-update of the server sources, single-instance guard, autostart, adopting a server folder the app did not create. | **"None in Phase 8: single-instance in Phase 9, the rest later or refused"** |

### Group (g), copied verbatim from `pyplan/phase8-decisions.md` (2026-08-31)

| # | Question | Answer |
|---|---|---|
| 1 | Beyond the server folder and its Docker objects, what else should purge remove? | **Only the launcher's own state record.** Not firewall rules, not module files in the WoW client, not Docker itself. |
| 2 | Should characters survive an uninstall? | **One button with a "Keep my characters" checkbox**, unticked by default. Not two buttons; not an unconditional wipe. |

---

## The cut

What the answers put into Phase 8, in the step order the delta table proposes (the design round
may reorder; it may not add or remove a feature without a new owner answer):

| Step | Delivers | Families and platforms | Answer |
|---|---|---|---|
| 8.1 | The command channel: SOAP on `127.0.0.1` enabled in the installed config, an app-owned GM-3 account verified by a round-trip, a typed command layer with the per-tree facts as data | WotLK, TBC, Vanilla over SOAP; Tortoise stays on the attach console + MySQL reads (it has no SOAP) | Q6 |
| 8.2 | Live dashboard (players, bots, uptime, restart-loop) and the worldserver log snapshot before every stop | all four; one box per family | Q8i, Q3 |
| 8.3 | Accounts: list, set password, GM level | all four; one box per family | Q8ii, Q3 |
| 8.4 | Named teleport, item search, item mail, revive, set level, rename, mailed money, gear-set presets | all four; one box per family; Tortoise has no set-level and Vanilla/Tortoise mail one item per message | Q2a, Q3 |
| 8.5 | Browse Bots | all four; one box per family | Q4 |
| 8.6 | My Party | WotLK only, by a server-side route (the mod-ale Lua bridge over SOAP); no client addon | Q2b, Q4, Q5 |
| 8.7 | Module update checks; module manifests for TBC, Vanilla and Tortoise | all four | Q8iii |
| 8.8 | Steam integration | Linux / Steam Deck only | Q2d |
| 8.9 | Uninstall / purge, as `phase8-decisions.md` | all four; gated on WotLK and one CMaNGOS game | Q2g |

**Phase 9:** character sheet (Q2c); doctor (Q2f); tuning knobs, config editor, settings page
(Q8iii); single-instance guard (Q8iv); console history and autocomplete; a read-only realmlist
check.

**Later, after v1:** auto-stop when the client exits and keep-awake while the server runs (Q2e);
automatic backups, self-update of the server sources, autostart, adopting a foreign install (Q8iv).

**Refused:** a shell (Q2f); account-wide sharing (Q8iii); coordinate teleport (Q7); any client
folder write (Q5); heal and summon-NPC unless the bridge 8.6 adopts covers them (Q2a); `.additem`
(online-only on every tree); the Playerbot Command Server as a channel (unauthenticated, off the
world thread); Play Together (a third-party service).

**Phase 8 code may start now** (Q1), on `yulon-phase8` stacked on PR #143, before the Phase 7
exit box is ticked; it still waits for this scoping's review cycle (the 2026-09-04 decision).

---

## What this overturns, by name

| Where | What it said | Now |
|---|---|---|
| `pyplan/README.md` §9 | "My Party / bot group builder", "Item database + in-game mail", "Teleport / GM in-game tools" — out of scope for v1, "deferred, not refused" | Deliberate v1 expansion (Q2a, Q2b): each is an `8.x` step with its own definition of done. Struck through on that page with the date, the way the Linux-native line was. |
| `pyplan/roadmap.md` "Out of scope (do not start these in v1)" | the same three features, plus "Full native reimplementation of installers on Linux" (already overturned by Phase 7 and never struck there) | **Not edited.** Appendix A carries the replacement text for §8 and for that list. |
| `pyplan/roadmap.md` §8 "Ordering rule: Phase 8 must not begin until Phase 7 exits" | | Overturned by Q1 for the code (2026-09-06) and by the 2026-09-04 decision for the scoping. Appendix A rewords it. |
| `pyplan/phase7-decisions.md` "What the installer does not do" — "No account creation, no SOAP, no realmlist writer in the engine" | | Still true of the **install engine**. 8.1 enables SOAP in the installed config and creates the app's account from the controller side, after `ready`; the engine's stage list does not change. |
| `pyplan/phase6-decisions.md` "Account creation — the finding that changes the plan": SOAP cannot create the first account | | Still true and load-bearing: 8.1's app account is written through `accounts.py`'s SRP6 row exactly because of it. |
| `pyplan/checklist.md` Phase 8 preamble "[blocked] on Phase 7 … Scope still TBD" | | Replaced by the numbered `8.x` boxes; the ticked 2026-08-21 identification box is kept. |

---

*The sections that follow — the decision, why and what was rejected, architecture and module
layout, per-feature detail, per-family data, delivery order and gates, blast radius, tests, risks,
what the implementer should NOT build yet, doc changes, Appendix A (proposed roadmap §8) and
Appendix B (the judge panel) — are written after the design round and the judges report.*
