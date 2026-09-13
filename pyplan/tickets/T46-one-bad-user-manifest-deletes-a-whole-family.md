# T46 — one bad user manifest deletes a whole family, and that is why `Not for this game` cannot exist

**Status:** FILED — the first of T44's four open findings, taken up on the owner's word.
**Filed:** 2026-09-13 by the lead, from T44's open finding 1 and its own code comment at
`yulon/ui/widgets/modules_panel.py:126`.
**Branch:** `fix/t44-open-findings`, cut from `feat/modules-and-tuning-tabs` (PR 153), because
the rows this changes are T44's. It becomes a follow-up PR once 153 merges.

## The mechanism

`ManifestStore.load_all()` is a generator that yields through `_load_at()`:

```python
def _load_at(self, path, kind, item_id) -> Manifest:
    manifest = load_manifest(path)
    if manifest.id != item_id or manifest.type != kind or manifest.game != self.game:
        raise ManifestError(...)
    return manifest
```

Both callers force it whole, and both catch at **family** scope:

| caller | what the catch costs |
|---|---|
| `ui/controller_view.py:5908` — `list(store.load_all(kind))` | the family's rows are replaced by one `!! could not load modules: …` line |
| `controller_wow_wotlk/modules.py:261` — fills `branches` | every module **after** the bad one loses its branch, silently |

So ONE unparseable or mis-declared file — and the user layer is where user-derived
files live — takes down all ~20 shipped modules of that family.

**Measured, not assumed, about the second caller:** its `branches` dict is built *outside*
the `try`, so entries added before the raise survive, and the bundled pass runs first. The
loss is therefore partial — the user items after the failure — not total, and it draws no
error anywhere. An earlier draft of this ticket said "every module's branch is dropped";
that was wrong, and the code is the reason it is wrong.

## The store already argues the fix, one method up

`user_index_items()` was given exactly this reasoning when the user layer was added:

> A MISSING index is an empty second layer, never an error: on a machine that has never
> derived a module there is nothing to read, and **raising would draw `!! could not load
> modules: …` over the whole tab and take every shipped module down with it.**

That is the same sentence this ticket is about. It was applied to the index and not to the
items the index lists.

## The contract, and the line it draws

The fix is NOT "never raise". The distinction is **who wrote the file**:

- **A bundled manifest that will not load is an app bug.** It is a file this app ships and
  tests. It must stay loud — a shipped catalog that does not parse is not a condition to
  render politely around.
- **A user manifest that will not load is user-derived data.** It is skipped, named, and
  the rest of the family still renders.

`load_all()` therefore keeps raising for the bundled pass and the bundled index, and
skips-and-reports for the user pass.

## What this buys, in order

1. A bad user manifest costs its own row, not twenty.
2. `module_updates()` stops silently answering "none behind" because of a file it could
   have skipped.
3. **`Not for this game` becomes reachable**, which is what T44 item 5 asked for. A user
   manifest whose `game` is not this store's is the one skip that has something worth
   showing: today the badge is documented as deliberately absent because no such row can
   exist. Once the skip is per-item, it can. Whether the row is *shown* greyed or only
   reported is a rendering decision this ticket takes second.

## Definition of done

One commit per item, TDD, each test with its named mutation.

1. **The store skips a bad user item and names it** — `load_all()` returns the manifests and
   the skips rather than raising for the user pass. Both callers updated; the bundled pass
   and both index paths still raise.
2. **`module_updates()` keeps the branches it could read** — a skipped manifest costs its
   own entry and nothing after it. Item 1 gives this for free, so this item is a test that
   proves it rather than a second change; it is here because "for free" is a claim.
3. **The panel reports skips without losing the family** — the `!!` line is now per item and
   the other rows are drawn.
4. **`Not for this game`**, as a real row from a real file on disk, with the comment at
   `modules_panel.py:126` replaced by the test that makes it true.

## Evidence the ticket owes

- The gate's last line, a named mutation per test.
- A fixture that is a REAL foreign-game manifest written to a user root — not a `Manifest`
  injected into the builder, which is what round 1 of T44 did and what its comment calls
  "coverage of a row nothing on disk can produce, which READS as a guarantee and is not one".
