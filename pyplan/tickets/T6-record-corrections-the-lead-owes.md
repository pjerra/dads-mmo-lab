# T6 — Four record corrections the lead owes, each found by a reviewer on another ticket

**Status:** OPEN
**Filed:** 2026-09-09 11:10 by the lead (Fable), from T1's eight reviews
**Hand:** Opus, worktree branched from `yulon-phase8b` (ff-merge `origin/yulon-phase8b` first; report the sha — expected `d70642c6` or later)
**File set (yours alone):** `pyplan/checklist.md` (line 2495 only), `pyplan/gates/8.6-panel-live-yulon-ubuntu2-2026-09-09/README.md`, `pyplan/gates/7.10-rerun-ubuntu2-2026-09-08/README.md`. No other file; no other line of the checklist.
**Box:** none.

## Why

T1's reviewers, across four rounds, named four things that are false or missing in the record and belonged to no ticket. Each is small. Together they are the same class of defect T1 existed to repair: a sentence that was true the hour it was written and is read as true today. This project's own lesson is that an incomplete artefact reads as fact.

## The four, with the fact behind each

1. **`pyplan/checklist.md:2495` — 8.2e's opening clause is false on both halves.** It says *"no SOAP exists on this core and none is added; the entry declares the attach console and the tab says so instead of offering a set-up button."* The fork re-added SOAP on 2026-09-07 (`src/mangosd/MaNGOSsoap.cpp`, `src/mangosd/soap/`, commit `3f9a062` — see `pyplan/tortoise-upstream-switch.md`); the catalog pin moved to `3a8472e` on 2026-09-08 (`2da4c516`); the entry became a SOAP entry the same day (`c7e577c7`: `operations.channel = "soap"`, port 7878, rank 4). The set-up button now exists on this entry — `test_the_soap_trees_keep_their_button` (`pylauncher/tests/test_controller_view.py`, find its current line) asserts it — and `CONSOLE_CHANNEL_SENTENCE` (`controller_view.py`, find the line) is reached by no shipped entry at all. The 2026-09-07 press was true when made; the subject changed under it. **Do not untick 8.2e and do not delete its record.** Append a dated correction in the style T1's line 2508 now uses: what was true on 2026-09-07, what changed on 09-07/09-08 with the shas, where the SOAP press lives (`pyplan/gates/tortoise-soap-yulon-arch-2026-09-09/`, a fresh install; and the startup-rank fact that folder proves), and what the console sentence now describes (a core the catalog no longer ships). Read the exit review's correction 6 whole (`pyplan/phase8-exit-review-2026-09-09.md`, the "Present-tense sentences" section, item 6) — it has the wording and the citations; verify each line number on today's tree before you write it, because two of that review's other line numbers were already found stale.
2. **`8.6-panel-live-…/README.md` — the VM's clock.** The sibling page `8.6-wotlk-yulon-ubuntu2-2026-09-09/part-2-party.md:3` says *"the VM's clock is UTC"*. Measured by the lead on the box at 10:34 local on 2026-09-09: `date` → `CEST`, `timedatectl` → `Europe/Oslo (+0200)`, `docker exec ac-worldserver date` → `UTC`. So the VM shell is +0200 and every container is UTC; every `…Z` timestamp the record cites is a container's. Add that as a dated paragraph to the panel-live README's corrections (it already corrects `part-2-party.md:188`); do not edit the historical page.
3. **`8.6-panel-live-…/README.md` — no tree sha.** The folder records no commit sha for the code that was pressed (its lane was cut off). The nearest is `daeae0a5` (03:38 +0200, the surface's last commit before the 06:00 press; a dozen commits sit between, none touching `party_panel.py` or `controller_view.py` — verify with `git log --oneline daeae0a5..d70642c6 -- pylauncher/yulon/ui/widgets/party_panel.py pylauncher/yulon/ui/controller_view.py`). Say so in the README, as a record, not as the press's sha.
4. **`7.10-rerun-ubuntu2-2026-09-08/README.md` — a date inside a folder named for the day before.** Its `run.log:1` opens `2026-09-09T00:34:39+02:00` in a folder named `…-2026-09-08`. Add one dated sentence to that README saying the run started after midnight local on the 9th, the folder is named for the evening it was staged, and (read `run.log` and the README's own timeline first) which timestamps are local and which are UTC. Do not rename the folder — the checklist cites it by name.

## Rules

- Every sentence you add is dated and in the past tense where it records, present tense only where it states a still-true fact you verified today. Every citation resolves on your tree; where the exit review's line numbers are stale, cite the resolving line and name the symbol.
- Do not tick, untick or reflow any checklist line other than 2495's own sentence(s).
- Gate: `YULON_TEST_BOX=yulon-fedora bash /c/Users/perzi/run-tests-vm.sh --checks` from your worktree's `pylauncher/` (announce on `yulon-fedora` first); ALL GREEN.
- One commit, `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>` only; no push; the ticket file is not yours to edit.

## Report format (final message)

`## Report` — sha, gate last line, diff stat, the four items each with the line you wrote and what you verified for it, deviations flagged, status DONE.
