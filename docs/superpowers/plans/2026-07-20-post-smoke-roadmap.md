# Post-Smoke-Test Roadmap

**Status:** planning only — no building until the user has smoke-tested.

## Context

Two days of heavy building landed a lot at once:

- **50 features are locked (untested); 17 are tested.** Everything built
  2026-07-19/20 — the second-pass features and all four improvement groups —
  has never run against a live server.
- The branch is **394 commits ahead of `main`** and has never been merged.
- The server is currently **stopped** (stopped on request after the overnight
  run).

Live testing has repeatedly been where the real bugs surfaced (the stale `dml`
backend that silently broke Bot World; the 30-minute world-restart hang; the
character-reset-on-restart bug). So the sequencing below puts verification
first and treats "fix what testing finds" as a funded round, not an
afterthought.

---

## Round 0 — Before the smoke test (docs only, no code)

Do this first; it unblocks the test itself.

1. **SMOKE-TESTS.md navigation is stale.** It was rewritten for the tabbed
   layout ("Settings → Bot World tab", "Bots → My Party tab"), but the sidebar
   redesign promoted those tabs to sidebar dropdown children. Rewrite the step
   wording for the accordion sidebar (e.g. "Config ▸ Bot World").
2. **Check every new feature has a row.** The overnight features + the four
   improvement groups added flags; confirm §21+ covers each locked flag, and
   that no flag exists without a row (and vice versa).
3. *(Optional, do before sharing)* **README + FEATURES.md are out of date** —
   both predate ~50 features and the sidebar redesign. Anyone you send the
   project to today reads a stale description.

## Round 1 — Fix what the smoke test surfaces

Reserve real capacity here. Historically this is the highest-value round: live
testing finds what stubs cannot. Expect the locked flags to flip in batches as
rows pass.

## Round 2 — Known polish tail (small, already identified)

### Server-required UX (user request, 2026-07-21)

Many pages need the server running (Console, Players-online, GM tools, live
config-apply, item DB, Bot Browser, etc.); with the server stopped they error
or show empty/broken states. User wants either (a) grey out those sidebar
items when the server is down, or (b) let you open the page and greet you with
a clear "start the server first" message.

**DECIDED (user, 2026-07-21): (b) the friendly in-page greeting**, not
grey-out. Reasons: grey-out is confusing (no reason shown, looks broken),
blocks exploring what a page does, and needs the same per-page metadata anyway;
the greeting is discoverable and can carry a **Start server** button right
there, and several pages already do a rough version (Console's offline note,
Home's start card).

Build plan: a shared `<ServerRequired>` placeholder (message + Start button),
shown reactively off the existing `server-status` store when the verdict is not
`online`. Gate ONLY the pages that are useless while stopped — Console,
Character view, Teleport, GM Tools, Item Database, My Party, Bot Browser,
Accounts (all need the DB/SOAP up). Do NOT blanket-gate the mixed/offline-ok
pages — Config (file edits apply on next start), Modules/Library install,
Tools disk tools, Backups' Verify, Help — they stay usable stopped (they can
keep their own inline notices where a specific action needs the server).
Small-medium, frontend-only, no new flag (read-only UX). Not started — waiting
on the user's "build now vs after smoke testing".


Concrete items deferred from the improvements review, all low-severity but
real:

- `world-restart` precondition is broader than the hang requires — it also
  refuses a valid crashed-world / healthy-DB recovery restart.
- `party online` interpolates class/level into JSON unguarded — the same
  latent invalid-JSON bug already fixed in `players online`.
- Keep-awake never re-engages after it releases on a poll-failure streak, even
  once polling recovers while still online.
- The MySQL-expose generated script lacks the elevation self-check that the
  shrink-disk script got.
- Cosmetics: "1 bots" pluralization in the backup summary; taskbar progress
  isn't wired into every long streamed op; the Teleport empty state blames the
  filter even when the fetch errored.
- Spec picker/validator drift for non-lowercase custom spec names (the picker
  can offer a name `_valid_bot_spec` then rejects).

### Statistics page (user request, 2026-07-21 — DECIDED, build after smoke testing)

Beyond-parity feature (verified: The Lab has NO stats page). Scoped against the
live DB — every stat below ran sub-second. User picked the groups:

- **INCLUDE:** World Population (family vs bots, level spread, class/faction
  split, guilds) · Economy (gold in circulation, richest, AH stock, mail) ·
  Family's Journey (playtime, last seen, achievements, quests, honor,
  professions) · Server History (boots, lifetime hours, longest session,
  connection peak — from acore_auth.uptime) · Bot Watch (busiest zones,
  bots per continent, combined bot playtime).
- **EXCLUDED:** Fun & Games (casino/transmog/pets) — user opted out.
- Implementation notes (from scoping run wf_1f4d916a): new read-only `wow
  stats` CLI arm → wow_stats Tauri cmd → api.ts → Statistics.svelte in the
  Server sidebar group; load-on-mount + Refresh (NOT the 7s poll); needs
  client-side id→name maps (class/race exist in $lib/wow; zone map = biggest
  cost, drives Bot Watch's medium effort); label AH stock as shop stock (it is
  100% ahbot); arena ratings are seeded fiction — never show a ladder; deaths
  are not tracked in stock AC (impossible); server-stopped → the in-page
  greeting. Read-only only — never extend the sanctioned-write list.

## Round 3 — Shareable release / installer

**The highest-value new capability**, given the stated goal of sharing this.
Today a recipient needs WSL2 + Node + Rust to run it from source.

- `npm run tauri build` → NSIS installer.
- Decide the signing story (unsigned = SmartScreen warning; a cert removes it).
- Publish via GitHub Releases so there's a real download link.
- This is also the prerequisite for launcher self-update (Round 5).

## Round 4 — Features blocked on install state

These were skipped because the box lacks the prerequisite, not because they're
hard. Smoke testing may resolve the blockers.

- **Battle Pass config editor** — needs the Battle Pass ALE mod actually
  deployed (the `battlepass_config` table doesn't exist yet).
- **Accountwide configurator live validation** — the module isn't installed on
  the box; the feature is fixture-tested only.
- **NPC Teleporter capital/starting-zone toggles** — needs the module install
  flow reworked (the toggles live only in `wow-manage.sh`).

## Round 5 — Larger backlog (ranked)

- **Launcher self-update** — needs Round 3 first (signed release manifests).
- **Per-character export/import (.dmlbak)** — genuinely useful for moving a
  kid's character; the GUID-remap step is the risky part and deserves careful,
  tested work rather than a quick pass.
- **Adopt / migrate an existing hand-built server** into the launcher.
- **NPCBots server variant** (separate bot mod, prebuilt-vs-compile choice).
- **Guest / visiting system** — only sensible after Tailscale play-together is
  proven working in practice.
- **Controller support** (ConsolePortLK + mapper) — large, Deck-centric
  upstream; a Windows port is a research task on its own.
- **HD graphics packs** — BLOCKED: the pinned source (btground.tk) is dead
  (HTTP 503, no Wayback copy). Needs a live mirror or it stays impossible.

## Round 6 — Merge + housekeeping

- Decide when the 394 commits land on `main`.
- Consolidate `docs/SMOKE-TESTS.md` (it has grown to 21+ sections) and prune
  superseded specs in `docs/superpowers/specs/`.

---

## Sequencing rationale

Verify → fix → package → extend. Building more on 50 unverified features
compounds risk, and several of them touch real data (bot flush, ARAC client
patching, backup restore). Round 3 is pulled ahead of the larger features
because it converts everything already built into something actually
shareable, which is what the project is for.
