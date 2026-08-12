# Modules-page round 2 — design

Date: 2026-08-12. Status: approved by user (interactive Q&A; "go").
Builds directly on round 1 (`2026-08-11-modules-page-round-design.md`) on the
same branch `feat/modules-page-round`; one user click-through gate covers both.

## The five user items

1. **The needs-setup chip IS the click target.** Clicking the chip opens that
   module's setup panel. The separate setup link/text row from round 1 is
   removed. The chip gets hover/focus affordance (it's a button now).

2. **Conf-activation is silent on success.** The round-1 auto-conf catch-up
   notice disappears for successful activations (activation still happens and
   still logs to the console stream). The ONLY surfaced case is failure: the
   catch-up (or install-path) outcome `NoDistYet`/`NoConf`/error shows a
   warning chip on that module's row — "conf not activated", with the reason
   and the manual `conf-activate` hint in its tooltip/panel. CLI surfaces are
   untouched (their info lines are contract-free stream text).

3. **Uniform action column + Config tuning.** Every installed row shows the
   same right-aligned action set, same order/width/position:
   **Config tuning · Repair · Remove**. The raw `.conf` filename text leaves
   the row. "Config tuning" routes to the module's Tuning-tab section (the
   round-1 nav store already does this — the entry point moves from the name
   link to an explicit button; the name stays a button too). On the Tuning
   tab, each module section gains an **"Open config file"** button that opens
   that module's ACTIVE conf in the existing file-viewing surface
   (ModuleFiles/editor). The raw-write allowlist is untouched — files outside
   `playerbots.conf`/`mod_ahbot.conf`/`mod_ale.conf` open read-only.

4. **One list style.** Installed and catalog sections share one row skeleton
   (shared Svelte snippet/component): name+status left, chips center-left,
   actions right — identical chip sizing/colors, equal row heights, paddings
   and separators across all sections. Catalog rows keep their own action
   (Install) but in the same column geometry.

5. **Disable without removing (honest subset).** Rows whose module has a
   REGISTRY-DECLARED master switch in `tuning-registry.json` (`*.enable` /
   `*.enabled` keys, conf or lua backend) get an Enable/Disable toggle in the
   action column. Toggling writes through the EXISTING tuner write path for
   that backend (no new commands, no new write surface) and raises the
   existing restart banner (conf-backend) or redeploy note (lua-backend).
   `mod-playerbots` never gets a toggle (core-coupled). Modules without a
   declared switch show NO toggle; their Config-tuning panel is the honest
   answer. Runtime `.dist` Enable-key detection is an explicit NON-GOAL of
   this round (would need a new contract field on `module list` and a mirror
   obligation on both surfaces — deferred until asked for).

## Non-goals

- No CLI/bash/Rust behaviour change anywhere in this round — launcher-only.
- No new Tauri commands; only existing api.ts calls are used.
- No `.dist` Enable-key runtime detection (see item 5).
- No changes to Goldshire/city-bots or any server-side behavior.

## Testing

Pure helpers pinned by vitest (row-model builder, chip model, toggle
eligibility from the tuning registry, failure-chip derivation from catch-up
outcomes); `svelte-check` clean; the existing 800-test suite stays green.
User click-through remains the final gate (covers rounds 1+2).
