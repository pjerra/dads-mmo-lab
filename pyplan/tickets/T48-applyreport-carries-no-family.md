# T48 — `ApplyReport` names an id but not the family it belongs to

**Status:** FILED — T44's open finding 3. The small one.
**Filed:** 2026-09-13 by the lead.
**Branch:** `fix/t44-open-findings`.

## The gap

`ApplyReport` (`yulon/apply.py:818`) carries:

```python
action: When
item_id: str
```

T42 round 2 established that an id alone does not identify a row — two families can ship the
same id — and keyed the view's own state by `(family, id)`:

```python
# yulon/ui/controller_view.py
self._manifests[(manifest.type, manifest.id)] = manifest
```

The report did not follow. So a report cannot be attributed back to the row that produced
it when two rows share an id: the session facts are keyed one way and the report is keyed
the other.

## Why it has not bitten yet

`apply_module()` is called with a `Manifest`, which carries its own `type`, so every
*producer* of a report knows the family at the moment it builds one. Only the report itself
forgets. That makes this cheap — the value is in hand at every construction site — and it
also means nothing today is wrong on screen; it is a latent mis-attribution waiting for two
same-named rows.

## Definition of done

1. **`ApplyReport` carries `family: ManifestType`**, set at every construction site from the
   manifest already in scope.
2. **Every consumer that matches a report to a row matches on `(family, item_id)`** — found
   by grep, not by memory, and the list of sites goes on the ticket.
3. A test builds two manifests with the same id in different families, runs both, and asserts
   each report lands on its own row.

## Evidence the ticket owes

The gate's last line and a named mutation per test. The mutation that matters: drop `family`
from the match key and see the test fail — if it still passes, the fixture is not two rows
that share an id.
