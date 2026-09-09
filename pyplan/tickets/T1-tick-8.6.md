# T1 — Tick 8.6, and correct the two gate lines that name a dead box

**Status:** DONE (hand reported 2026-09-09 09:02; awaiting review)
**Filed:** 2026-09-09 08:40 by the lead (Fable)
**Hand:** Opus, worktree branched from `yulon-phase8b` (ff-merge `origin/yulon-phase8b` first; report the sha)
**File set (yours alone):** `pyplan/checklist.md`. Nothing else — a second file is a finding for your report, not an edit.
**Box:** none.

## Why

The Phase 8 exit line (`pyplan/checklist.md:2508`) requires every `8.x` box ticked with the evidence its gate line names. 8.6 (`checklist.md:2500`) now has every clause pressed, and its one remaining blocker — "no capability reachable only from a script" — was closed last night by a real panel that drove a real server. The judge (`pyplan/phase8-exit-review-2026-09-09.md`, corrections 1 and 2 under "Present-tense sentences … made false") wrote the exact corrections; nobody has applied them because ticking is the lead's session's job. It is now this ticket's.

## The evidence the tick must cite, all on `origin/yulon-phase8b` at `b8e9a96a` or later

| clause of 8.6 | folder |
| --- | --- |
| Q1–Q3: engine in the binary, `mod_ale.conf`, `dml_bridge_ping` answers | `pyplan/gates/rebuild-live-yulon-ubuntu2-2026-09-09/` and `8.6-wotlk-yulon-ubuntu2-2026-09-09/part-1-rebuild.md` |
| Q4: a bot of a chosen class in a real client's party frame | `pyplan/gates/8.6-wotlk-yulon-ubuntu2-2026-09-09/` (`2-party-frame-with-bot.png`, `3-party-frame-after-dismiss.png`, `part-2-party.md`) |
| the surface (exit clause 3) | `pylauncher/yulon/ui/widgets/party_panel.py`, wired at `controller_view.py::_build_my_party_group`, 19 offscreen tests (`8820984c`, `daeae0a5`) |
| the surface pressed live | `pyplan/gates/8.6-panel-live-yulon-ubuntu2-2026-09-09/` — read empty, Add (Kiteema, mage), row arrives on the re-read, Dismiss, three refusals; the README says what it does NOT show |

## What to write

1. **Tick `checklist.md:2500`** in the style of its ticked siblings (8.5c, 8.7b above and below it): `- [x] 8.6 …`, keep the whole existing sentence, and append a **Gated 2026-09-09** clause that names the box, the code sha, each folder above, and — in the past tense — what was measured: the engine absent then present; the bridge "does not exist" then `DML-BRIDGE-READY`; Kiteema in the party frame; the panel's row arriving one reading late (that detail is the proof it reads the group table rather than its own wish). Name what is NOT shown: one class of ten pressed; panel and client photographed separately; the restore arm's own gaps are 8.7a's, not this box's.
2. **Correct the gate box and folder names on `:2500` and `:2501`** exactly as correction 2 in the exit review says: `yulon-ubuntu` died with the host's D: drive on 2026-09-08; the presses ran on `yulon-ubuntu2`; the folders are named for it. Correct 8.7a's line **only** in its gate/folder names — do not tick 8.7a (that is T2's press and the lead's tick).
3. **The false sentence in the sibling folder** — `pyplan/gates/8.6-wotlk-yulon-ubuntu2-2026-09-09/part-2-party.md:188` says the Qt widget does not exist. Do not edit that page. The correction already stands in `8.6-panel-live-…/README.md`; the tick must cite the panel-live folder beside the older one so a reader lands on the correction.
4. Every claim in your text must resolve to a file and, where it is a number or a name, to the line that carries it. Present tense rots: write what happened, dated.

## Definition of done

- `git diff` touches `pyplan/checklist.md` only.
- `YULON_TEST_BOX=yulon-fedora bash /c/Users/perzi/run-tests-vm.sh --checks` from your worktree's `pylauncher/` ends `=== --checks: ALL GREEN ===` — `test_docs_pins` widens when a box ticks, so a citation that does not resolve turns the gate red, which is the point.
- Committed in your worktree, one commit, message in the repository's voice (see `git log -5`), `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>` only — no session link, no "Generated with" footer.

## Report format (mark DONE by appending a `## Report` section here)

Sha, the gate's last line, the diff stat, and **deviations flagged** — anything you decided the spec left silent, anything you could not cite.

## Report (hand, 2026-09-09 09:02)

- sha `886f7f93` on `worktree-agent-ab5bae55cb4fe4a92`, ff-merged from `e2912690`
- gate `=== --checks: ALL GREEN ===`, 3769 passed, 6 skipped
- diff `pyplan/checklist.md | 4 ++--` (lines 2500 and 2501)
- 8.6 ticked with a Gated 2026-09-09 clause: box `yulon-ubuntu2`, code `daeae0a5` (surface) and `5a7e8baa`; all three folders; past-tense readings (engine False then True; "does not exist" then `DML-BRIDGE-READY`; Jilsur in the client's party frame; Kiteema through the panel with the row arriving at 06:05:55 after the join at 06:05:53); NOT-shown paragraph. Gate box and folder names corrected on 2500 and 2501. 8.7a not ticked.
- Deviations flagged by the hand: (1) correction 3 NOT applied -- `:2500` still says "This route has never been recorded working" ahead of the Gated clause; (2) correction 4 NOT applied -- `:2500` still says "that rebuild is the owner's and is not automatic"; (3) correction 1 (`:2470`) not applied, out of the ticket's items; (4) no tree sha recorded in the panel-live folder, `daeae0a5` used and labelled as the surface's commit; (5) test paths spelled `tests/...`; (6) `OffCritical` wording from correction 1.
