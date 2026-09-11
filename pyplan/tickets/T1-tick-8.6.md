# T1 — Tick 8.6, and correct the two gate lines that name a dead box

**Status:** ACCEPTED and MERGED (lead, 2026-09-09 11:05) -- merge `65db5139`, lead's close `ab574594`, suite green behind it
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

## Review 1 (Codex adversarial, 2026-09-09 09:14) — REWORK

- [high] `pyplan/checklist.md:2500` still says "This route has never been recorded working" and "that rebuild is the owner's and is not automatic"; the appended Gated clause and the exit review refute both. A ticked line that contradicts its own evidence misrepresents the product the moment the box is checked; a later paragraph does not correct false prose on the line itself.
- Must-fix: rewrite both sentences on `:2500` in the exit review's dated formulation (failures 2026-08-20 and 2026-09-08 for a measured reason; answered 2026-09-09 once the engine was compiled in; the app's Rebuild control is user-initiated and was pressed live, while the manifest's rebuild flag is still not applied automatically). Re-run the docs-pin checks.
- Fable reviewer's verdict pending; the rejection body will carry both.

## Review 2 (cold Fable reviewer, 2026-09-09 09:18) — REWORK

Independently the same finding: the additions are sound and every citation resolves (shas, folders, filenames, the 06:05:53/06:05:55 timestamps, 16 + 3 = 19 tests); the two present-tense sentences left on `:2500` are false on the day of the tick, and the spec's "keep the whole existing sentence" contradicted its own item 4 and the exit review it cites. The spec was wrong, not the hand. Notes: correction 1 (`:2470`) is the same fact in the same file and no other ticket owns it; the "five scripts loaded" claim cites files that show a directory listing, the log lines are in `rebuild-live-…/README.md:189-195` and `9-clause5-server-debug.txt`; "19 tests over `8820984c` and `daeae0a5`" reads as if both added tests; test paths spelled `tests/…` against the line's `pylauncher/…` convention.

## Rejection (lead, round 1) — must-fixes, all in `pyplan/checklist.md`

1. `:2500` — rewrite the bold "This route has never been recorded working …" as the dated record (exit review correction 3): failed 2026-08-20 and 2026-09-08 for a measured reason (the engine was not in the image); answered 2026-09-09 once it was compiled in. Keep the "a negative answer is a legitimate outcome" stance, in the past tense.
2. `:2500` — apply correction 4: the applier still does not act on the manifest's rebuild flag; the app's own Rebuild control exists (`controller_view.py:1587`, `:4220-4221`) and was pressed live 2026-09-08/09; what remains the owner's is the decision to press it. Dated.
3. `:2470` — apply correction 1 (the WotLK box is `yulon-ubuntu2`; `yulon-ubuntu` OffCritical since 2026-09-08), so the file does not name a dead box on one line and call it dead on another. The lead widens the ticket's items to include it.
4. `:2500` — the "worldserver's own log naming all five deployed bridge scripts as loaded" parenthesis: cite `rebuild-live-yulon-ubuntu2-2026-09-09/README.md:189-195` (or `9-clause5-server-debug.txt`) for the `loaded` lines; the files named now show the listing on disk.
5. Optional, not blocking: "19 tests over …" → say all nineteen landed in `8820984c`; spell the test paths `pylauncher/tests/…`.

Amend the single commit (the lead reads "one commit" as one at merge), re-run the gate, report again.

## Report, round 2 (hand, 2026-09-09 09:31)

- sha `6b57f0ab` (amended, one commit, base `e2912690`); gate `=== --checks: ALL GREEN ===`, 3769 passed; diff `pyplan/checklist.md | 24 ++++++++++++++++--------`
- All five items done: `:2500` bold rewritten as the dated record ("twice recorded failing before it was recorded working", 2026-08-20 / 2026-09-08 with the measured cause, answered 2026-09-09); correction 4 applied with the control named and dated; `:2470` corrected (box `yulon-ubuntu2`; `yulon-ubuntu` OffCritical since 2026-09-08; the hostname trap carried with `7.10-rerun-ubuntu2-2026-09-08/run.log:7`); the loaded-lines claim now cites `rebuild-live-…/README.md:189-195` and `9-clause5-server-debug.txt:17-21`; the nineteen tests attributed to `8820984c` alone with `pylauncher/tests/…` paths.
- Deviations: (1) the reviewer's `controller_view.py:1587` / `:4220-4221` do not resolve on this tree -- the control is `REBUILD_BUTTON_LABEL` at `:1648`, button at `:4320-4321`; cited those. (2) one extra sentence at `:2470` ("No gate restores a checkpoint on yulon-ubuntu …") past-tensed, beyond correction 1, because leaving it live reinstated the rot. (3) correction 6 (`:2487`, 8.2e's SOAP clause) is false and on no ticket's items -- **lead's follow-up**. (4) the panel-live folder records no tree sha; `daeae0a5` cited as the surface's commit.

## Review 3 (Codex adversarial, round 2, 2026-09-09 09:44) — REWORK, a new finding

- [high] The five round-1 edits were applied, but the tick certifies an incomplete surface. `:2500`'s own definition of done says *add a bot by class, spec and level … kick, and dismiss all*; the design (`pyplan/phase8-designs/b-users-surface.md:422`) requires chosen class/spec/level; the shipped panel says the opposite at `party_panel.py:33` ("No spec, no level, no 'dismiss all'") and `InstallParty.add()` takes class and gender only. The evidence exercised one mage at the server-chosen level with automatic talents, so it cannot prove chosen spec, chosen level, or dismiss-all.
- Must-fix: remove the tick, or an explicit owner rescope. Fable reviewer's round-2 verdict pending; the lead's decision follows both.

## Review 4 (cold Fable reviewer, round 2, 2026-09-09 10:02) — REWORK, one clause

All five must-fixes done and verified (four trees, `Get-VM` on the host, every file and line). One sentence claims a measurement never made: *"with a measured cause both times: the Lua engine was not in the image"* is unsupported for 2026-08-20 -- that attempt's only record (`pyplan/phase8-reads/hypeer.md:619-623`) attributes it to a relative `ALE.ScriptPath` and nobody read that image's binary; the cause was measured on 2026-09-08 (`8.6-wotlk-yulon-ubuntu-2026-09-08/README.md:24-25`, `:40-52`). Notes: the reviewer's own round-1 line numbers were wrong and the hand's right; the panel-live folder records no sha and the line should say so; `README.md:191-195` not `:189-195`; the three presses were all on 2026-09-08 (+0200); lines drifted (8.6 `:2508`, 8.7a `:2509`, exit `:2516`); 8.2e's false clause is at `:2495`, the lead's.

## Rejection (lead, round 2)

The two verdicts together:
1. **Withhold the tick** (Codex). `:2508` stays `- [ ] 8.6`. Keep every correction and the Gated record, retitled so the line is true: *Pressed 2026-09-09 …* proves the route (bridge answering) and the surface **for class**; *still owed*: a chosen spec, a chosen level with `characters.level` read back, and dismiss-all -- filed as `pyplan/tickets/T5-my-party-spec-level-dismiss-all.md`, and the box ticks when T5 lands. The definition-of-done text on the line is not to be weakened.
2. **The "measured cause both times" clause** (Fable): the cause was measured on 2026-09-08 only (cite `8.6-wotlk-yulon-ubuntu-2026-09-08/README.md:40-52` by name, and say that second machine was `yulon-ubuntu`, the original, on the morning its disk died); 2026-08-20's note offered a different cause (`phase8-reads/hypeer.md:619-623`, a relative `ALE.ScriptPath`) and measured none.
3. Small and true: "the folder records no sha" beside `daeae0a5`; `README.md:191-195`; the three presses on 2026-09-08.

Amend the single commit, re-run the gate, report.

## Report, round 3 (hand, 2026-09-09 10:14)

- sha `3a020861` (amended, one commit, base `e2912690`); gate `=== --checks: ALL GREEN ===`, 3769 passed; diff `pyplan/checklist.md | 24 ++++++++++++++++--------` (`:2470`, `:2508`, `:2509`)
- Tick withheld: `:2508` is `- [ ] 8.6`, definition of done untouched; record retitled "Pressed 2026-09-09 -- the route, and the surface for CLASS; the box stays open"; a "Still owed" sentence names chosen spec, chosen level with readback, dismiss all, cites `party_panel.py:33`, `b-users-surface.md:422` and T5 (`7ab2caa0`), and says the definition is not weakened to meet what was pressed.
- The 2026-08-20 cause: now records that the note offered a different cause (`hypeer.md:619-623`, relative `ALE.ScriptPath`) and measured none; the cause was measured 2026-09-08 on `yulon-ubuntu`, the original, and `8.6-wotlk-yulon-ubuntu-2026-09-08/` is cited directly (`README.md:24-25`, `:40-52`).
- Small three done: "records no sha" beside `daeae0a5`; `README.md:191-195`; "three times, all on 2026-09-08 (+0200)".
- Deviations: T5's path is on origin only until the lead merges; reviewer line numbers replaced by resolving ones (unchanged); `:2470` one past-tensed sentence (unchanged); correction 6 (8.2e) still on no ticket -- **lead's, flagged a third time**; with 8.6 unticked `test_docs_pins` no longer scans the three `8.6-*` folders, so this gate is weaker evidence about them than round 1's.

## Review 5 (Codex adversarial, round 3, 2026-09-09 10:24) — ACCEPT

8.6 genuinely unticked with its definition of done intact; the record withholds completion for chosen spec, chosen level with `characters.level` readback and dismiss-all under T5; the 2026-08-20 / 2026-09-08 correction is supported by the cited lines; the remaining citations resolve. No material findings. Fable round-3 verdict pending; merge follows both.

## Review 6 (cold Fable reviewer, round 3, 2026-09-09 10:31) — REWORK, one fact in two places

Everything asked is present and the definition of done is byte-identical to the parent. One date is wrong twice, refuted by the folder the line cites: the bridge answered on **2026-09-08** (`rebuild-live-…/7-ping.log` against a world started `2026-09-08T20:39:08Z`; `8-ping-after-press2.log` against `21:15:35Z`; 22:42 and 23:18 +0200), so `:2508` "It answered on 2026-09-09" and `:2470` "rebuilt on 2026-09-09" are both false. The wording came from the exit review's correction 3 (`phase8-exit-review-2026-09-09.md:344-345`) and the lead's round-1 item; the logs contradict both. Notes: the Jilsur press folder is dated 09-09 while every timestamp inside is 09-08 UTC (the lead's, not this ticket's); `README.md:24-26` would be exact; the bare filenames on 2508 belong to `rebuild-live-…/` while `part-1/2` belong to `8.6-wotlk-…/`; "the box that no longer exists" -- the registration exists, the disk is gone.

## Rejection (lead, round 3)

1. `:2508` — "It answered on 2026-09-09" → "It answered the same evening, 2026-09-08, once a rebuild compiled the engine in" (or drop the date; the "all on 2026-09-08 (+0200)" clause carries it).
2. `:2470` — "rebuilt on 2026-09-09 with `mod-ale` compiled in" → "rebuilt the same night with `mod-ale` compiled in" (`8.7a-wotlk-yulon-ubuntu2-2026-09-09/README.md:6-8` says it that way).
3. Optional: `README.md:24-26`; say which folder the bare filenames sit in.
The exit review's correction 3 carried the wrong date; the lead notes it for the review's own errata.

## Report, round 4 (hand, 2026-09-09 10:42)

- sha `2a09a69b` (amended, one commit, base `e2912690`); gate ALL GREEN, 3769 passed; diff `pyplan/checklist.md | 24 ++++++++++++++++--------`
- `:2508` "answered the same evening, 2026-09-08" with the folder-name trap stated on the line (ping logs against worlds started 20:39:08Z and 21:15:35Z); `:2471` "rebuilt the same night"; `README.md:24-26`; a sentence saying which folder each bare filename sits in.
- Deviation of the hand's own: the record's heading is now "Pressed 2026-09-08/09" (rebuild and bridge on the 8th evening, client party frame and panel press on the 9th morning; sibling 8.5c spans dates the same way).
- Flagged: a clock disagreement inside the evidence -- `part-2-party.md:3` says the VM clock is UTC, `8.7a-…ubuntu2-…/README.md:8` gives a two-hour offset (lead's); correction 6 (8.2e) fourth flag (lead's); T5's path resolves on origin only; docs-pin guard narrower while unticked.

## Review 7 (Codex adversarial, round 4, 2026-09-09 10:50) — ACCEPT

Dates match the cited logs; the 2026-09-08/09 heading spans the rebuild/bridge evening and the client/panel morning accurately; `:2508`'s definition of done byte-identical to `e2912690`. No material findings. Fable round-4 verdict pending; merge follows both.

Lead's note for the merge: the clock disagreement the hand flagged is settled read-only on the box (10:34 local) -- the VM runs Europe/Oslo (+0200) and the ac-worldserver container, the one asked, runs UTC, so `part-2-party.md:3` is wrong about the shell and right about all the `…Z` timestamps the lines cite.

## Review 8 (cold Fable reviewer, round 4, 2026-09-09 10:58) — REWORK, one invented cause

All dates now consistent with every log cited; the spanning heading true; the placement sentence checked against three folders; the definition of done byte-identical. One clause added this round gave a reason the repository refutes: "named for 2026-09-09 because it was written after midnight" -- the folder, its README and both ping logs landed in `89453f9b` at 23:30 +0200 on 2026-09-08. Notes: the Jilsur press was late evening on the 8th local (between the 21:15:35Z start and the 22:24:14Z restart), not "the 9th morning" as the hand's report said -- the line never dates it; the spanning sibling is 8.4c at `:2502`, not 8.5c; `7.10-rerun-ubuntu2-2026-09-08/run.log:1` opens on 09-09 inside a folder named 09-08 (lead's).

## Closed by hand (lead, round 5, 2026-09-09 11:03)

The "after midnight" cause was the lead's own guess, written into the round-3 rejection and copied forward by the hand. Under the owner's rule (close by hand after two consecutive wording-only rounds) the lead merged the hand's commit `2a09a69b` (`65db5139`) and replaced the clause with the fact (`ab574594`: named 09-09 though committed 23:30 +0200 on the 8th, `89453f9b`). Suite behind the merge: `=== --checks: ALL GREEN ===`, 3769 passed. Hand retired, worktree removed, branch deleted.

Still the lead's, carried out of this ticket: 8.2e's false clause at `:2495` (correction 6); `part-2-party.md:3`'s "VM clock is UTC" (the VM is +0200, the containers UTC); the panel-live folder records no sha; `7.10-rerun-ubuntu2-2026-09-08/run.log:1`'s date inside a 09-08 folder.
