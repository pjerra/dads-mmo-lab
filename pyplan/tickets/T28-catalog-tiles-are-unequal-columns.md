# T28 — The catalog's two tile columns take unequal widths, so the tiles come out different sizes

**Status:** IN PROGRESS (Sonnet hand on `hand-t28` since 2026-09-10 23:20 CEST)
**Filed:** 2026-09-10 23:20 CEST by the lead (Fable), from the owner looking at the Yu'lon window Steam opened on `yulon-arch` ("the different servers on catalog is wrong sizes")
**Hand:** Sonnet (a layout rule and its test), worktree branched from `yulon-phase8b` (ff-merge `origin/yulon-phase8b` first; report the sha)
**File set (yours alone):** `pylauncher/yulon/ui/catalog_view.py` **only** at the grid built in `__init__` (`:320-323` at `da3fd899`) and `_tile_text` / `_tile` if a size policy is needed; its test file (name it). Not the Install button's wiring, not the tile's sentences.
**Box:** none; unit only (offscreen). The frame is `pyplan/gates/catalog-tiles-yulon-arch-2026-09-10/catalog-two-columns-unequal.png` (the VM's tree at `2da4c51`, 2026-09-08; the grid code is unchanged at the tip).

## The fact

`QGridLayout` with no column stretch gives each column the width its content asks for, and word-wrapped `QLabel`s ask in proportion to their text: the left column (WotLK, Vanilla) is drawn narrow and the right (TBC, Tortoise, whose descriptions are longer) wide, the left tiles' text wrapping to a sliver. `_tile_text`'s wrapping (v0.6.51) stopped the tiles pushing the buttons off screen; it did not make the columns equal.

## What to build

- Equal columns: `grid.setColumnStretch(0, 1)` and `(1, 1)` (or for every column the row count implies), and the tile frames a size policy that lets them share the width (`QSizePolicy.Policy.Preferred` horizontally with the labels' `minimumSizeHint` not driving the column — say what was needed; word-wrapped labels report a minimum width from their longest word, which is fine). Every row's tiles the same width within a pixel; the tile's height its content's.
- TDD, offscreen: build the view with the shipped catalog, resize to the width the window opens at, assert every tile's `width()` equal within 1 px across the row and the two columns equal; the mutation is the stretch removed (the widths diverge). A second test with a catalog of one long-description and one short-description entry, same assertion.

## Definition of done

`--checks` ALL GREEN from your worktree's `pylauncher/` (announce on the gate box first); the test red first. One commit, `Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>` only; no push; the ticket file is not yours. Scratch under `<scratchpad>/T28/`.

## Hand's reports, reviews and the lead's check (2026-09-11)

Round 1 (`2e4ea723`, Sonnet): `grid.setColumnStretch(0, 1)` / `(1, 1)` on the catalog grid; two tests measuring the tiles through a bare `QSplitter` at 1100x750 (before: 224/451, after: 338/337); gate ALL GREEN. Codex round 1: needs-attention — the fixture was not `build_window()`'s geometry (no tab frame, no `setCollapsible`, no stretch factors, no `_CATALOG_MIN_WIDTH`), so it proved one synthetic allocation. Round 2 (`88c840af`): `main.build_catalog_tab(window, catalog_view, log_panel)` extracted so `build_window()` and the tests share the production tab/splitter code; width matrix at the minimum (400 px viewport: 188/188), the default allocation (502: 239/239) and wide (880: 428/428); three focused tests red under the mutation (125/353) and green restored; gate ALL GREEN (4141 passed). Codex round 2: **approve**, no material findings (it could not run the suite in its sandbox). Lead's check in the worktree: 58 catalog-view tests green; both `setColumnStretch` lines removed → exactly 3 failed, 55 passed; restored → 58 passed. ACCEPT.

## Closed (lead, 2026-09-11 02:20 CEST)

Merged `--no-ff` as `1853a254`; gate on m910q behind the merge `=== --checks: ALL GREEN ===`; pushed. Worktree and branch removed. Owed: `git -C ~/y8 pull --ff-only` on `yulon-arch` once T29's hand is off the box, so the Steam server entry runs the fix (the owner's frame was taken from that tree).
