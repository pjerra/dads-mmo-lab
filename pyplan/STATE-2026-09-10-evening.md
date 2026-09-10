# State, 2026-09-10 evening — handoff before a context compaction

Written by the lead (Fable) at 2026-09-10 22:47 CEST, WSL session, tip `c2141397` on `origin/yulon-phase8b` (everything pushed, the tree clean but for the two hand worktrees below). Supersedes `STATE-2026-09-10-morning.md` for the board; the process, budget rules and gate command there still hold, with two amendments: the gate box is **m910q** (`YULON_TEST_BOX=m910q`) — `yulon-fedora` is shut down on the owner's word; and every finished Codex review leaves a `codex` process behind, `pkill -f codex-linux-x64/vendor` after each (memory `background-waiters-pile-up`). Reviews that outrun the foreground window die when moved to the background: launch them with `run_in_background` from the start, no trailing `pkill` in the same chain.

## Amendment 2026-09-10 23:30 CEST, before the compaction

- **8.8 is TICKED** (T17 merged `33f9a4ff`, the tick on the stand-in in `pyplan/checklist.md`; Phase 8 at **28 of 29**). Only 8.6 is open: T16 (running) and T26's live half.
- **Resume in Big Picture (owner's report, evening):** reproduced on `yulon-arch` and traced to Steam, not Yu'lon — `pyplan/gates/8.8-steam-resume-yulon-arch-2026-09-10/`; the 8.8 tick paragraph and T17 carry the note. Open item: the Deck/gamescope path is unproven here. `yulon-arch` has no server Yu'lon knows (no `state.json`); `~/tortoise-vm` (4.1 GB, containers from 09-08) is adoptable with "Use existing…" on the WoW Tortoise tile.
- **T29** filed (`24f5deba`) and running: an Opus hand on `.claude/worktrees/t29`, `hand-t29`, live on `yulon-arch` only — Half 1 adopts `~/tortoise-vm` through the WoW Tortoise tile and gets the Steam client entry to the login screen; Half 2 runs Steam inside gamescope and presses Resume (the Deck path). On its report: cold Opus review of the evidence, merge behind a gate, push; then the 8.8 tick paragraph loses its "unproven here" clause if Half 2 holds.
- **T28** filed and running: the catalog's two tile columns are unequal (the owner's frame from `yulon-arch`); a Sonnet hand on `.claude/worktrees/t28`, `hand-t28`; on its report one Codex review, merge behind a gate, push, then `git -C ~/y8 pull` on `yulon-arch` so the Steam entry shows the fix.
- `~/y8` on `yulon-arch` was moved to the branch tip (`803a686`) on the owner's word; its four local edits from the 8th are kept as `~/y8-local-edits-20260910-212210.patch` and `stash@{0}` in that tree.
- T27 filed (a conf activation never recommends a restart), open, Sonnet, unit only.

## Two hands are running right now — their reports arrive as task notifications

| Worktree / branch | Ticket | State | On the report |
|---|---|---|---|
| ~~`.claude/worktrees/t17`~~ (removed) | T17 / 8.8 Steam | CLOSED, merged; round 2's two last findings closed by the lead's hand | (done) |
| (was) | | round 2: five code fixes (atomic write, surrogateescape on the write, an unknown VDF byte as a refusal, CRLF preserved, a frame name) plus `OSError` at `add()` and version-aware Proton ordering; no box re-run | one Codex review of the round-2 diff (code only); ACCEPT -> merge, gate on m910q, push, close, **tick 8.8** in `pyplan/checklist.md` citing `8.8-steam-shortcuts-yulon-arch-2026-09-10/` and `8.8-steam-read-…`; REWORK -> the lead fixes by hand (the cap) |
| `.claude/worktrees/t28`, `hand-t28` (Sonnet) | T28 (equal catalog columns) | the failing test written, the layout change next | one Codex review -> merge, gate, push, pull the tree on `yulon-arch` |
| `.claude/worktrees/t16`, `hand-t16` (Opus) | T16 (chosen level does not hold at join) | live measurement first on `yulon-ubuntu2` (the timing table, the module's source), then the code in `party.py` at `add_bot()`'s level step, then a second press | one cold review (Opus, it touches the box) -> merge behind a gate; with T16 in, 8.6 still needs T26's live half |

Ticket status lines and the reports/reviews are on the tickets themselves; the lead records each report, review and close there, in the shape every ticket already shows.

## Board

- **CLOSED and merged today:** T13 (both halves), T14, T18, T19 (second half, the adopt press, the m910q press: the owner's install adopted and updated), T20, T21, T22, T23, T24, T25, T26 (unit half). The six owed Codex passes ran and became T20–T25.
- **Open:** T16 (running), T17 (running), T26's live half (needs a client seat on `yulon-win11` and a world restart on `yulon-ubuntu2` for the sixth bridge script — that script is already deployed there by T18), T27 (a conf activation never recommends a restart; Sonnet, unit only, spec on the ticket).
- **Phase 8: 28 of 29** — 8.8 ticked 2026-09-10 23:01 CEST on the stand-in (T17 merged `33f9a4ff`). 8.6 ticks when T16 and T26's live half hold (dismiss-all by T5, a chosen spec by T18 are proved).
- **Release:** none cut here — upstream chooses the version when Baerthe merges; PR **146** stays a draft, its body refreshed after every merge from `<scratchpad>/lead/pr146-new-body.md` through `gh api -X PATCH …/pulls/146 --input pr146-patch.json` (`gh pr edit` fails silently on this repo — memory `gh-pr-edit-fails-on-projectcards`), verified by grep.
- **The report artifact** "Phase 8 Closing Night" (https://claude.ai/code/artifact/85d5b01e-a039-4160-8fcb-7c845cf8778f) was republished at version 3 with the 9th's frames and the six questions, the login card cut; it does not yet carry the 10th's afternoon/evening (T13 live, T18, T19 press, T17). Build: `<scratchpad>/report/build.py` (template + `{{IMG:…}}`; the night images from `night-images.json`).

## Owner's answers on the 10th (all through the question tool)

Teach the probe -> after three failed rounds, the adopt press instead (done; the probe attempts kept as local branch `record/t19-probe-unmerged`). 8.6: not ticked, plus the new requirement T26 (alts, other accounts', friends' characters as bots; AzerothCore only). 8.8: the owner signed into Steam on `yulon-arch` himself (Big Picture, userdata id 18347166 — by id in every record, never by name). Release: upstream's call. The redundant index on m910q dropped (and re-created by T19's updates press, by the fork's own file; the owner knew). "Use fable on this round only" for T19's third probe round. Autonomous box work with frames for the report. Fedora shut down when idle.

## Boxes, as last left

- `yulon-ubuntu2` (WotLK): world/auth/db UP, 500 bots; six bridge scripts deployed; `playerbots.conf` deployed by T18 (left on purpose) and a derived manifest under `~/.local/share/yulon/manifests/user/wow-wotlk/modules/mod-playerbots.json` (left; `forget` drops it); T16 working there now. Owner's things untouched.
- `yulon-arch`: 12 GB (was 16), Steam running in Big Picture, the owner signed in, the two Yu'lon entries present with artwork; GE-Proton11-6 in `compatibilitytools.d`, Qt xcb libs and `~/bin/claude-say` installed by T17's hand; the Tortoise client under `~/clients/TurtleWoW`.
- `yulon-fedora`: OFF (owner). `yulon-win11-gate`: OFF (stopped to free host RAM; its 149 GB checkpoint chain could not be moved off D:, no volume has room). `yulon-win11`: OFF, the client seat T26/T16 may need.
- `m910q`: the gate box; the owner's Tortoise install adopted and updated, all containers exited, `r6` up (the owner's).
- The Hyper-V host: D: (all VMs' disk files except the four Yu'lon work VMs) retried I/O on 9/2, 9/7, 9/8 and is quiet since the 9/10 boot; the NVMe boot SSD throws a 17-event bad-block burst after each boot; file systems clean by `chkdsk /scan`; the owner advised to back up C:, replace the NVMe, reseat SATA cables (memory `vm-host-disk-health`).

## Scratch (this session): `/tmp/claude-1000/-home-perzi/17f38be4-34e7-4624-8bc2-aa1cbbf598ff/scratchpad/` — `lead/pr146-new-body.md`, `report/`, `T*/` per ticket, the gate logs `*-gate.log`.
