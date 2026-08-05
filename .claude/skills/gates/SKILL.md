---
name: gates
description: Use when the user asks what is outstanding, unfinished, promised, blocked, or ready to release — or types /gates. Also use before proposing new feature work, to surface open gates and unpushed work first.
---

# Gates — the open-commitments scan

One command that answers "what is still owed in this repo" with nothing missed. Completeness is the whole point: a commitment whose only home was not consulted is a commitment lost (recorded incident: a user-approved 13-item feature batch sat invisible in gitignored `.superpowers/`).

Standing context for this branch, so findings are ranked against the right target: `rust-main` is THE integration line, the v0.1.0 target is **WoW Playerbots on the NATIVE (Docker Desktop) backend**, and the standing user policy is **NO merge to `main`** — "unmerged to main" is therefore never a finding.

## The homes — consult ALL of them, every run

1. **Every `CLAUDE.md` in the tree** — glob `CLAUDE.md` and `*/CLAUDE.md`; do not assume a fixed set, it changes (the root file was split into `launcher/`, `crates/` and `cli/` nested files on 2026-08-06). Follow the root file's link list too: a link to a nested `CLAUDE.md` that does not exist is itself a finding to report.
2. `docs/ROADMAP-TO-BETA.md` (the authoritative beta-blocker list — its rule: not listed = not blocking), `docs/SHIP-LIST.md` (release discipline + incident record), `docs/SMOKE-TESTS.md`. All three exist on this branch.
3. Every file in `docs/superpowers/plans/` — glob it (27 files today); scan each file's status header and gate sections. A plan can look filed and still be untracked. Same for `docs/superpowers/specs/`.
4. The gitignored ledgers: `.superpowers/sdd/progress.md`, every `.superpowers/sdd/*/progress.md`, `.superpowers/sdd/NATIVE-TAIL-SMOKE.md`, and the loose `*-report.md` / `*-brief.md` under `.superpowers/sdd/`. These are often the ONLY record of a gate's true state.
   **`.superpowers/` is gitignored and therefore NOT branch-scoped** — the same working directory carries ledgers belonging to OTHER branches (e.g. `2026-08-04-arch-wsl-backend/`, `2026-08-05-server-survival/` are sibling-branch work, not `rust-main`'s). Attribute every ledger to a branch before reporting it as an open `rust-main` gate, or you will invent blockers.
5. Any `*-REPORT.md` at the repo root (none today — the pattern still fires when one lands).
6. git: `git status --porcelain` (`??` files that are the only home of a decision), `git status -sb` plus `git ls-remote origin rust-main` (is the branch pushed AT ALL?), unpushed spread via `git log --oneline origin/rust-main..HEAD` — **not `rust-main..HEAD`, which is empty by construction while HEAD is `rust-main`** — and parked/sibling branches via `git branch -vv` + merge-base (today: `feat/arch-wsl-backend`, `feat/multi-server-tray` marked PARKED, `desk/scratch`, and several `worktree-agent-*` branches).

## Output shape — ranked, source file cited per item

1. **USER GATES** — things only the human can run (live click-throughs, release-exe launch, live smokes, account steps). Give the exact command/steps for each.
2. **DATA-LOSS RISK** — unpushed commits, dirty tree, untracked files holding decisions.
3. **CODE-COMPLETE, UNGATED** — built + reviewed work waiting on its live gate.
4. **APPROVED, NOT BUILT** — user-approved or user-dictated work not started (label post-beta items as such).
5. **STALENESS** — contradictions between what a `CLAUDE.md`/plan claims and what the ledger records (e.g. a gate listed open that the ledger marks PASSED). Report them; do not silently fix.

## Rules

- Rank hard (beta blockers first) but cap nothing — for this output, completeness overrides any list-length limit.
- An empty or missing ledger section is a finding, not a skip.
- Cite the recording file for every item; convert relative dates to absolute.
