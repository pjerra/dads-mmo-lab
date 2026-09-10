# T19 — The updates button cannot reach a marker-less install, and prints the install's cancel note

**Status:** OPEN, the first half waits on the owner's answer (open question 1 on the 2026-09-10 STATE page); the second half is free to take
**Filed:** 2026-09-10 15:14 CEST by the lead (Fable), from T14's live half (findings 1-2, confirmed by the evidence reviewer against the source)
**Hand:** Sonnet for the second half alone; Opus if the first half becomes a probe change (it touches the import gate every CMaNGOS install goes through). Worktree branched from `yulon-phase8b` (ff-merge `origin/yulon-phase8b` first; report the sha)
**File set (yours alone):** second half: `pylauncher/yulon/catalog/native.py` **only** at `update_stages()` (`:2165-2168` at `1f44a995`) and its test file. First half, once decided: `pylauncher/yulon/catalog/sqlplan.py` at `MarkerGate.probe` (`:1251-1279`) and `pylauncher/yulon/catalog/native.py` at `import_reads_as_finished` (`:401`), or a new adopt action beside the updates button in `controller_view.py` — not both; their tests; the T11 ticket's claim corrected by the lead, not the hand.
**Box:** `m910q` for the first half's press, only on the lead's word and the owner's; the install there is the owner's.

## The facts (T14 live, `pyplan/gates/tortoise-updates-button-m910q-2026-09-09/`)

1. **The second arm of `import_reads_as_finished` is dead through the CMaNGOS gate.** `MarkerGate.probe` returns `ImportState("populated", ...)` with `complete` at its default False (`sqlplan.py:1279`); `complete=True` is set on the `imported` branch alone (`:1265`). The owner's m910q Tortoise install has no `yulon_install` table in any schema while its state file records `import` completed, so it reads `populated` incomplete and the button refuses — the install the T10 -> T11 -> T14 chain was built for. T11's claim that the flagged phase reaches a shell-script install "which reads `populated`" is false on this gate. Two remedies, the owner's call: (a) the probe answers completeness on the `populated` branch from per-schema table counts it already asks elsewhere; (b) an explicit "Adopt this install as imported" action that writes the marker row with the owner's consent (a write nobody has consented to yet — the hand was right not to make it).
2. **The press prints `IMPORT_CANCEL_NOTE`** ("Databases left half-written are detected and cleared…"): `cmangos.py:247` builds `Stage("import", ..., cancel_note=IMPORT_CANCEL_NOTE)`, the spine yields any `cancel_note` after `--- <name>` (`native.py:2044-2046`), and `update_stages()` reuses the stage changing only `recorded`. The note is false on this route: `_only_the_rerunnable_phases` never calls `stage_import()`, so `gate.reset()` is unreachable.

## What to build

- Second half, now: `replace(stage_named("import"), recorded=False, cancel_note="")` at `native.py:2167`; a test pinning the panel's lines for an updates press (no cancel note); the mutation is the old `replace`.
- First half, after the owner picks (a) or (b): the chosen change with the tests its side needs — for (a) a probe test per schema-count answer and the T11 fixture green; for (b) the confirmation naming the row it writes and a refusal when the world is up; then the m910q press again, the same captures as T14's live half, and this time the two route lines.

## Definition of done

`--checks` ALL GREEN from your worktree's `pylauncher/` (announce on the gate box first; `YULON_TEST_BOX=m910q` while the Hyper-V host is down); the tests red first. One commit per half, `Co-Authored-By: Claude <model> <noreply@anthropic.com>` only; no push; the ticket file is not yours. Scratch under `<scratchpad>/T19/`.
