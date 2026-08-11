# Modules-page round — design (2026-08-11)

User-approved design for the six-item Modules-page improvement batch
(user dictated 2026-08-11, clarified via five decisions the same day).
Branch: `rust-main` (the active integration line since the core-family
merge). One round, one plan.

## The six user items and how they map

| # | User ask | Design section |
|---|---|---|
| 1 | Installed modules/lua/sql come first | §3 layout |
| 2 | Conf activation happens automatically | §2 auto-conf |
| 3 | Page clearer and easier to read | §3 layout |
| 4 | Click module/text to open its config/tuner | §4 click-to-open |
| 5 | "Needs setup" notices with guided setup (e.g. AHBot) | §5 setup metadata |
| 6 | Does module update work? | §6 update honesty (audit verdict: WORKS-WITH-CAVEATS) |

## Scope rules

- CLI-visible behaviour changes land on BOTH surfaces (bash `cli/` +
  Rust `crates/`) with tests on both — the repo mirror rule.
- Layout, click routing and setup rendering are launcher-only.
- OUT of this round (filed to the post-smoke roadmap, not forgotten):
  - Lua/SQL update paths (lua updates only by re-Install; SQL mods
    cannot update at all — Install refuses with EXISTS).
  - Streaming the native `git pull` (today a bounded collect — the
    launcher shows a frozen terminal up to 20 min on a slow pull).
  - The upstream-PS1 adoptables (separate analysis, separate filing).

## §2 Auto-conf activation (item 2)

Decision: **in the CLI install itself**, not a launcher auto-click.

- `module install` (cpp family) finishes by activating the module's
  conf: copy `<module>.conf.dist` → `<module>.conf` ONLY when no
  `.conf` exists. Never overwrites an existing conf (user edits are
  sacred), idempotent on re-runs. A note is streamed into the install
  terminal naming the activated file.
- Reuse the existing conf-activate arm's logic (both surfaces already
  have it behind the manual command); the install arm calls the same
  helper — no second implementation.
- Launcher catch-up: on Modules page load, any INSTALLED cpp module
  whose list row reports `conf: "ready"` is activated automatically —
  one sequential pass after the list refresh, notes surfaced in the
  page note line. This heals existing installs without clicks.
- UI: the manual "Activate conf" button is removed. The row shows the
  conf filename (which is also the click-to-open zone, §4).
- Tests: bats + cargo pin (a) fresh install activates, (b) existing
  `.conf` is NOT overwritten, (c) missing `.conf.dist` is not an error.
  Mutation proof on the never-overwrite branch.

## §3 Layout: Installed/Available split per family (items 1 + 3)

Decision: **split inside each family card**, not one cross-family card.

- Each family card (C++ modules / Lua scripts / SQL mods) renders an
  **Installed (N)** section first, then a collapsible **Available (N)**
  catalog. Collapsed by default when the family has ≥1 installed
  module; expanded when it has none (a fresh server should present the
  catalog, not an empty section). Expand state is component state —
  no persistence in v1.
- Lua rows: the two badges (Cloned/Deployed) collapse into ONE status
  chip — "Installed" (cloned+deployed), "Cloned, not deployed",
  "Not installed".
- Row anatomy (installed): name + GitHub link · description · one
  status chip · update/needs-setup chips · conf filename · actions
  right-aligned. Uninstalled rows: name/link/desc + Install.
- The rebuild card stays pinned at the top of the Modules tab,
  unchanged.
- ALE-missing note keeps its current behaviour (catalog visible,
  rows disabled) inside the new Available section.
- Tests: vitest pins the partition (installed first, counts right,
  default expand rules) as pure helpers, not DOM assertions where
  avoidable.

## §4 Click-to-open (item 4)

Decision: **both click zones**.

- Module NAME (installed rows only) → switch to the Tuning tab and
  scroll to that module's card. Mechanism: a small shared runes store
  ("pending tuning target": module key). ModuleManager sets it and
  flips `tab`; ModuleTuning, on becoming active, consumes it, expands
  that module's card and scrolls it into view. Store is cleared after
  consumption (no sticky re-scrolls).
- CONF FILENAME text on the row → switch to the Config files tab with
  that file preloaded. ModuleFiles accepts a preselect input by the
  same pending-target pattern.
- Hover affordances (underline/cursor) make both zones discoverable.
  Uninstalled rows have no click zones.
- Tests: vitest on the store contract (set → consume-once → cleared);
  wiring asserted by component tests where the existing suite has
  precedent.

## §5 Needs-setup notices (item 5)

Decision: **catalog-driven metadata**, not hardcoded panels, not
docs-links.

- `crates/dml-wow/data/module-catalog.json` gains an OPTIONAL `setup`
  block per module:

  ```json
  "setup": {
    "summary": "AHBot needs an auction-house character before it does anything.",
    "steps": ["Create a dedicated account+character (or pick an existing one).",
               "Set AuctionHouseBot.Account / .GUID in mod_ahbot.conf.",
               "Restart the world server."],
    "actions": [{"type": "open-tuner", "key": "mod-ahbot"},
                 {"type": "copy-command", "label": "Create account", "command": "account create ahbot <password>"}]
  }
  ```

  Action types in v1: `open-tuner` (jumps via §4's store),
  `open-files` (conf file), `place-npc`, `fixit`, `copy-command`
  (copies to clipboard with a "copied" flash). Each action type maps
  to machinery that ALREADY exists; no new mutation surfaces.
- The module list arm passes `setup` through ADDITIVELY on both
  surfaces (bash emits it in the list JSON; Rust serves it native).
  Additive = older frontends ignore it; the TermEvent/JSON contract
  gains optional fields only.
- UI: installed modules with a `setup` block show an amber
  **Needs setup** chip; the row gains a Setup button opening a panel
  (summary, numbered steps, action buttons, "Mark as done").
  "Done" is stored launcher-side per server in `launcher.json`
  (keyed `setupDone.<module-key>`); no DB probing in v1 — a static
  checklist the user dismisses. Cheap auto-probes (e.g. AHBot conf
  keys non-zero) can arrive in a later round without changing the
  metadata shape.
- First catalog entries: mod-ahbot (the motivating case), battlepass
  (NPC placement — wraps the existing fixit), bmah / mod-1v1-arena /
  mod-npc-beastmaster / mod-transmog (wrap the existing Place-NPC
  button), mod-arac (client patch step + restart).
- Tests: catalog schema pinned (serde test rejects unknown action
  types both surfaces — bash validates in the list arm); vitest for
  chip/panel rendering and the done-persistence contract.

## §6 Update honesty (item 6)

Audit verdict (2026-08-11 read-only audit): C++-only, bash side
proven by 13 real-git bats tests, Rust side (what the native launcher
runs) has ZERO pull-path tests; button feature-locked "untested" by
default; four real defects. Decision: **fix the four defects + Rust
tests in this round**; the button STAYS locked until the user runs
one live update (untested-stays-locked rule).

- (a) **False SQL advisory.** The "module SQL (if any) is applied
  automatically by the server's db-import on next start" line is only
  emitted when a rebuild was actually MARKED (pending_rebuild write
  succeeded). Data-only updates (mod-arac) get the honest line: its
  SQL is NOT auto-applied on native — name the Repair/manual path.
  Same correction on the install arm's copy. Both surfaces.
- (b) **pending_rebuild honesty.** The done payload's
  `pending_rebuild` reflects the marker write's REAL result. A failed
  write emits a visible warn naming the marker path (the banner that
  will not light must not be the only signal). Both surfaces.
- (c) **Staged-edits backup.** The local-changes patch switches to
  `git diff --binary HEAD` so staged edits are captured (today a
  `git add`-ed edit produces a 0-byte patch while the copy says "your
  edits are safe"). Both surfaces.
- (d) **Rust pull-path tests.** Real-git fixtures (tempdir + bare
  origin, the bats suite's shape) covering: changed → marker written +
  pending_rebuild true; up-to-date → no marker; dirty worktree →
  patch written, edits reapplied; STAGED edits → patch non-empty
  (pins (c)); mod-playerbots refusal; guard-before-NOT_FOUND ordering.
- UI: `updateDoneNote` derives from the (now honest) payload; no
  copy claims SQL auto-apply unless the payload says so.

## Verification

- cargo workspace + bats + vitest, run in the safe order (never bats
  concurrent with cargo — the cli/dml rebuild trap). Mutation proofs
  on: never-overwrite conf, advisory-only-when-marked, patch captures
  staged edits.
- Live gates (user): one real module update on a real server (then
  unlock `module-update`), one fresh module install showing auto-conf
  in the terminal, click-through of the new layout + one setup panel
  (AHBot) end to end.

## Out-of-scope ledger (filed, not dropped)

- Lua/SQL labelled update paths.
- Streaming native git pull.
- Setup auto-probes (auto-clearing the Needs-setup chip).
- Conf-drift advisory after module update (new keys in `.conf.dist`
  not reflected in the active conf).
