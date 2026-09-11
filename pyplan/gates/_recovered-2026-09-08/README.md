# Recovered from four orphaned worktrees, 2026-09-08

**What this is.** On the morning of 2026-09-08 the orchestrating session stopped two workflow runs
mid-flight — once to add the rule that every lane reads `origin/rust-main` first, once to add the
rule that every lane reads `pyplan/` first — and relaunched. The stopped runs' worktrees stayed on
disk. Their lanes had kept working until the stop, and two of them had **finished live gates** whose
evidence was untracked in those worktrees, on branches nothing pointed at. A retrospective audit
the same afternoon found it, hours before an ordinary `git worktree prune` would have destroyed
roughly two hours of live gating on a stack that is expensive to re-take.

Everything untracked or modified in those four worktrees was copied here, unedited, before
anything else was done. Each subfolder carries its worktree's `STATUS.txt`, `modified.patch` and
`unmerged-commits.txt` as they stood.

**What was promoted onto the branch, and from where:**

| From | To | Why |
|---|---|---|
| `wf_1b4e012f-87c-1/…/8.4d-tortoise-m910q-2026-09-08/` | `pyplan/gates/8.4d-tortoise-m910q-2026-09-08/` | the first lane's complete gate: README, transcript, three real Turtle-client captures |
| `wf_53488134-63e-1/…/8.4d-…/` — six files the first lacked | the same folder | round 2: the rename, the offline revive, the gold mail, the gear count, with their own script and transcript; provenance written into the README |
| `wf_1b4e012f-87c-1/…/8.5d-…/` — four shots, three driver logs, README and driver edits | `pyplan/gates/8.5d-tortoise-m910q-2026-09-08/` | **8.5d's clause 4**, which the branch believed was still open: `[Sietta]: Level 5 Goblin Rogue`, three subjects, three answers |
| `wf_1b4e012f-87c-1/modified.patch` — `catalog.json` + `test_play_actions.py` | the branch | the `.rndbot level <bot>` argument-order correction, with the live console exchange recorded in the test; `63e-1` carried the identical hunk |

**What stays here and is not promoted, on the owner's rule of 2026-09-08 — *diff against what merged,
keep only what is new*:**

* `wf_53488134-63e-2/…/8.7a-…-lane2/` and `wf_1b4e012f-87c-2/…/8.7a-…/` — two earlier 8.7a attempts.
  The 8.7a lane that **did** merge (`pyplan/gates/8.7a-wotlk-yulon-ubuntu-2026-09-08/`, 15 logs, 9
  frames, README) superseded both. They are kept because nobody has read them line by line against
  the merged folder yet, and deleting unread evidence is the mistake this folder exists to record.
* `wf_1b4e012f-87c-2` commit `bbc97705`, *"8.7a's first clause had no code: how far behind each module
  is"* — a second implementation of `commits_behind` that the merge lane correctly refused as a
  duplicate of the one that merged.

**The four worktrees themselves can now be removed.** Nothing they held is only there any more.

**The lesson, so it is not learned a third time:** stopping a workflow does not stop its lanes'
*work* from existing — it stops the process that would have merged it. The two memory notes
`worktree-isolation-branches-from-main` and `phase8-orphan-worktree-87c-2` record the mechanism.
