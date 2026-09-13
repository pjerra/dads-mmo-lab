# T45 — the desktop renders every size 40% larger than it is authored

**Status:** FILED — diagnosed, fix written, PR to upstream
**Filed:** 2026-09-13 by the lead, from the owner: "the sizes in the app on desktop is off. I think when baerthe added deck friendly and changed something."
**Owner of the file:** upstream. `yulon/ui/theme.py` is Baerthe's and every fork ticket forbids editing it, so this goes to `DadsMmoLab:Yulon` as its own PR rather than into any of ours.

## The mechanism

`theme.py` cannot use relative font units — Qt's QSS resolves `em`, `%` and the CSS
keywords to a fixed value — so the whole stylesheet is **generated** per window width:

```python
REFERENCE_WIDTH = 1280
def scale_for_width(width): return max(0.7, min(1.4, width / REFERENCE_WIDTH))
def _px(base, scale):       return f"{max(MIN_FONT_PX, round(base * scale))}px"
```

A window wider than the reference therefore renders **larger** than the authored size, up
to 1.4×. On a maximised 1920 desktop the scale is `min(1.4, 1.5)` = **1.4** exactly.

## What Pass 11 changed, and why it only hurts the desktop

`b2a50344` ("Pass 11 ux") did two things at once:

1. Added the Deck floors — `MIN_FONT_PX = 12` (replacing a bare `max(8, …)`) and
   `TOUCH_TARGET_PX = 32`, with `_touch()` easing control sizes by `scale**0.5`. All of
   that is right, and the docstrings argue it well: 8px progress labels on a 215 PPI panel
   are unreadable.
2. **Also raised the base sizes** — `_px(13, scale)` → `_px(14, scale)` and similar; the
   diff removes 4 `_px(...)` bases and adds 11.

Those two overlap, and the overlap is the defect:

| window | scale | base 14 renders | what the floor does |
|---|---|---|---|
| Steam Deck / 960 min | 0.70–0.75 | 10px | **floored to 12px** — the bump is invisible |
| 1280 reference | 1.00 | 14px | floor inactive |
| 1920 desktop | **1.40** | **20px** | floor inactive |

On the handheld the floor was already guaranteeing legibility, so raising the base buys
almost nothing. On the desktop the same raise is multiplied by 1.4 and compounds across
every label, button, tab and input at once — which is what the owner is looking at.

## The fix: scale down, never up

```python
return max(0.7, min(1.0, width / REFERENCE_WIDTH))
```

One bound. The sizes are authored at `REFERENCE_WIDTH`, so a window at or above that width
should render them **as authored** — a 24" monitor is not viewed 40% further away than a
handheld, and nothing in the theme's own reasoning argues for growing past the design size.
Narrower windows still scale down exactly as before and are still caught by
`MIN_FONT_PX`/`TOUCH_TARGET_PX`, so **the Steam Deck is untouched**: at 1280 and below the
new bound is never the active one.

It is one character of behaviour change and it moves only windows wider than 1280 — which
is precisely the population that reported the problem.

Deliberately NOT done: reverting Pass 11's base-size raises. They are Baerthe's judgement
about readability at the reference width, they are what the Deck floors were tuned
alongside, and with the cap corrected they render at the size he chose.

## Evidence

Measured against the running app on `yulon-win11` (1920×1080, maximised — the screenshots in
`pyplan/gates/t44-modules-gap-2026-09-13/` are it), and by arithmetic on the generator:

```
base 11 -> 15px today, 11px fixed        base 15 -> 21px today, 15px fixed
base 12 -> 17px today, 12px fixed        base 16 -> 22px today, 16px fixed
base 13 -> 18px today, 13px fixed        base 18 -> 25px today, 18px fixed
base 14 -> 20px today, 14px fixed        base 20 -> 28px today, 20px fixed
```

At the 960 minimum every one of them is floored to 12–14px, before and after, unchanged.
