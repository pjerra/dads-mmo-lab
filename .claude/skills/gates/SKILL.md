---
name: gates
description: Use when the user asks what is outstanding, unfinished, promised, blocked, or ready to release — or types /gates. Also use before proposing new feature work, to surface open gates and unpushed work first.
---

# Gates — the open-commitments scan

One command that answers "what is still owed in this repo" with nothing missed. Completeness is the whole point: a commitment whose only home was not consulted is a commitment lost (recorded incident: a user-approved 13-item feature batch sat invisible in gitignored `.superpowers/`).

## The homes — consult ALL of them, every run

1. Root `CLAUDE.md` AND the three nested ones: `launcher/CLAUDE.md`, `crates/CLAUDE.md`, `cli/CLAUDE.md`.
2. `docs/ROADMAP-TO-BETA.md` (the authoritative beta-blocker list — its rule: not listed = not blocking), `docs/SHIP-LIST.md` (release discipline + incident record), `docs/SMOKE-TESTS.md`.
3. Every file in `docs/superpowers/plans/` — glob it; scan each file's status header and gate sections. A plan can look filed and still be untracked.
4. The gitignored ledgers: `.superpowers/sdd/progress.md`, every `.superpowers/sdd/*/progress.md`, `.superpowers/sdd/NATIVE-TAIL-SMOKE.md`. These are often the ONLY record of a gate's true state.
5. Any `*-REPORT.md` at the repo root.
6. git: `status --porcelain` (`??` files that are the only home of a decision), `status -sb` plus `git ls-remote origin <branch>` (is the branch pushed AT ALL?), `git log --oneline rust-main..HEAD` (unmerged spread), parked branches via `git branch -vv` + merge-base.

## Output shape — ranked, source file cited per item

1. **USER GATES** — things only the human can run (live click-throughs, release-exe launch, live smokes, account steps). Give the exact command/steps for each.
2. **DATA-LOSS RISK** — unpushed commits, dirty tree, untracked files holding decisions.
3. **CODE-COMPLETE, UNGATED** — built + reviewed work waiting on its live gate.
4. **APPROVED, NOT BUILT** — user-approved or user-dictated work not started (label post-beta items as such).
5. **STALENESS** — contradictions between what CLAUDE.md/plans claim and what the ledger records (e.g. a gate listed open that the ledger marks PASSED). Report them; do not silently fix.

## Rules

- Rank hard (beta blockers first) but cap nothing — for this output, completeness overrides any list-length limit.
- An empty or missing ledger section is a finding, not a skip.
- Cite the recording file for every item; convert relative dates to absolute.
