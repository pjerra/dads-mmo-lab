# State, 2026-09-10 morning — handoff for a fresh session

Written by the lead (Fable) at 10:45 CEST after the 2026-09-09 exit pass stalled on the monthly spend limit at ~14:30 the day before. A new session (WSL + rtk, or anywhere) resumes from this page, the tickets under `pyplan/tickets/`, the memory folder, and the worktrees named below. Nothing here needs the old transcript.

## The process in force (owner's "runes", 2026-09-09)

Lead writes the spec and files the ticket → starts it (branch, worktree, seat) → an ephemeral hand implements in its worktree, runs the suite there, commits to its branch, marks DONE with a report → a cold reviewer with the diff and the spec writes ACCEPT or REWORK → REWORK: the lead rejects the ticket with the must-fixes, the same hand fixes → ACCEPT: merge into `yulon-phase8b` ("master"; mainline `Yulon` is Baerthe's), the full suite behind the merge, red reverts → the lead closes the ticket, retires the hand, removes the worktree → step 8: title the CHANGELOG's Unreleased, version bump, tag `v…Public`, push, GitHub release (ask the owner for the version and the branch first).

**Budget rules (owner, 2026-09-09 13:00-13:05):** one reviewer per ticket — Codex adversarial (its own OpenAI limit; it hit its cooldown at 12:10 on the 9th), or a cold **Opus** reviewer while Codex is on cooldown, Fable only for live-box or >300-line tickets; **two-round cap**, after round 2 the lead fixes the remaining must-fixes by hand and merges; **Sonnet hands** for small tickets. Lanes ≤3 worktrees. Hands write scratch under `<scratchpad>/<ticket>/`. No "Generated with Claude Code" footer and no `Claude-Session:` line anywhere reaching GitHub — `Co-Authored-By: Claude <model> <noreply@anthropic.com>` alone. Every merge: `--checks` on `yulon-fedora` ALL GREEN behind it, then push. Stamps come from the clock in the script that writes them (`date +"%H:%M CEST"`), never typed.

**Gate:** from a worktree's `pylauncher/`: `YULON_TEST_BOX=yulon-fedora bash /c/Users/perzi/run-tests-vm.sh --checks` → must end `=== --checks: ALL GREEN ===`. Announce on a box first (`ssh <box> claude-say "..."`; on ubuntu2 and m910q it is `~/claude-say`). Never `docker` on the laptop; never bare `pytest` (`-m "not integration"`); never a server on the laptop; ask before any rebuild; one server at a time on m910q, stopped when done.

## Board

| Ticket | State | Where |
|---|---|---|
| T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 T11 T12 T15 | CLOSED, merged, worktrees removed | tip `3b4408fa` on `origin/yulon-phase8b` (suite 3936 green at `1a446392`, on m910q) |
| **T13** (dml_uninvite verifies the master) | code half MERGED `1a446392` (round 2 `0d832391`, Opus ACCEPT, gate on m910q; worktree and branch removed); live half OPEN, blocked on `yulon-ubuntu2` (the Hyper-V host `desktop-fp27auv` was offline on the tailnet on the 10th) | live half next: redeploy the five Lua scripts, restart the world, the two whispers with the world's log lines into `pyplan/gates/8.6-uninvite-contract-yulon-ubuntu2-<date>/`; carry the reviewer's free guard `if b == p then return refuse(...)` into that commit; its note 5 (`_rows_only` turns a failed group read into `()` and the poll reads it as removed) wants its own ticket |
| **T14** (Apply pending database updates button) | code half MERGED `ed035e41` (+ lead edits `f2c22b4e`, `f31a1d2d`); live half DONE at `5b507b00` in `.claude/worktrees/agent-aaec74d437014a0e0` — **the press refused on the owner's m910q install** (no `yulon_install` marker; the probe never reports populated+complete) | the Opus evidence review died on the limit before reading anything. Next: one reviewer (Opus) on `5b507b00`'s folder `pyplan/gates/tortoise-updates-button-m910q-2026-09-09/`, then merge, close, retire; file **T19** from its findings 1-2 (probe completeness for a populated unmarked database, or an "adopt as imported" action; and `update_stages()` must pass `cancel_note=""` so the install's cancel note is not printed on this route). |
| T16 (chosen level does not hold at join) | OPEN, waits for T13 to merge (shares `party.py`) and the box | spec on the ticket; live measurement first |
| T17 (8.8 Steam on a fullscreen-Steam stand-in) | BLOCKED on the owner: a Steam login for `yulon-arch`, or a captured `shortcuts.vdf` | spec on the ticket |
| T18 (deploy `playerbots.conf` so a spec can take effect) | OPEN, after T13's live half; needs the box and a client seat | spec on the ticket |

**Owed Codex passes** (ran out of limit before them; Codex-side cost only): T7 code r2 (`a6e2aff6`), T12 (`eea840b6`), T3 r4 (`a4454ec0`), T11 (`c5e7d78c`+`d90eae50`), T8 r3 (`b6178611`), T14 code (`34a9ebe2`). Command: `node "C:/Users/perzi/.claude/plugins/cache/openai-codex/codex/1.0.5/scripts/codex-companion.mjs" adversarial-review "<prompt>"` in the background; strip `[codex]` lines; record on the ticket; a must-fix becomes a follow-up ticket. Their branches are kept until then; every other merged branch may be deleted.

## Checklist and records

Phase 8: **27 of 29** boxes. 8.7a ticked 2026-09-09 (`8dad8eda`). 8.6 open with an honest record (dismiss-all proved through the panel; chosen spec unreachable until the app deploys `playerbots.conf` — T18; chosen level sent at join is accepted and not written — T16). 8.8 open (T17). `CHANGELOG.md` Unreleased carries every merged feature with its gate folder; the lead titles it at release. PR **DadsMmoLab #146** (`yulon-phase8b` → `Yulon`) is a **draft** by the owner's order until the exit pass closes; its body is regenerated from `<scratchpad>/lead/pr146-new-body.md` and refreshed after each merge (`gh pr edit 146 --repo DadsMmoLab/dads-mmo-lab --body-file …`).

## Boxes, as last left

- `yulon-ubuntu2` (WotLK, 100.99.204.5): world up pid 538052 (500/500 bots), auth up, realm row `100.99.204.5|100.99.204.5|255.255.255.0`; accounts `101 YULON_243C46E3, 102 PERZI, 103 YULONADMIN` (YULONPANEL removed by the lead 2026-09-09 ~14:05 via SOAP); `~/t7` worktree and `~/t7-gate-out/world-six-tables-before.sql` (7 MB) left for a reviewer, removable. Owner's things untouched (PERZI/Pakka, `LootPet2.lua` under `~/wowserver/env/dist/etc/modules/lua_scripts/`, `Logger.ALE` at `worldserver.conf:706`). SOAP admin account YULONADMIN on loopback 7878 (the owner knows the password; it is not written here).
- `m910q` (Tortoise, the owner's install): **every container exited** (as the owner left it before the 9th); `guild_bank_money.money` still `int(11)`; `idx_owner_bot_event` present. No press writes there without the owner's word.
- `yulon-fedora`: the gate box. `yulon-arch`: healthy, candidate for T17.

## The morning report

Artifact "Phase 8 Closing Night": https://claude.ai/code/artifact/85d5b01e-a039-4160-8fcb-7c845cf8778f — still the night-before version; the 2026-09-09 daytime section is NOT in it yet. How to build it and what to add: `<scratchpad 5c3f610a…>/lead/REPORT-HOWTO.md` (template `closing-night.tmpl.html` + `shots-morning/*.png` in the old scratchpad `e6db1900…`, `{{IMG:name}}` → data URI, publish with the Artifact tool to the URL above). Frames to add: T5's `panel-5/6/9`, `client-4`; T7's three renderings; T3's `failrestore/restore.log:14,17,21`; T14's three widget grabs.

## Open questions for the owner (also in `<old scratchpad>/morning-questions-2026-09-09.md`, 178 lines)

1. The m910q install has no import marker: teach the probe completeness (T19, my lean), or an "adopt as imported" press that writes the marker with your consent, or leave it hand-patched.
2. 8.6: does it count as done for the release with T16 and T18 open?
3. 8.8: a Steam login for the VM, or a captured `shortcuts.vdf` (T17).
4. Release: version (0.6.60 vs 0.7.0) and branch (`yulon-phase8b`) for step 8.
5. The redundant `idx_owner_bot_event` on m910q: drop it (`ALTER TABLE tw_char.ai_playerbot_random_bots DROP INDEX idx_owner_bot_event;`, world stopped) — note a future press of the T11 route re-creates it.
6. The DML VM is live and needs an explicit yes before anything touches it.

## Scratch paths (Windows; `/mnt/c/...` from WSL)

- This session's lead scripts: `C:\Users\perzi\AppData\Local\Temp\claude\C--Users-perzi-dads-mmo-lab\5c3f610a-f81d-40c7-8610-23867cdf66fb\scratchpad\lead\` — `gate-merge.sh <merge-sha> "<label>"` (suite behind a merge, push on green), `t*_merge.py` (the pattern for ticket bookkeeping), `pr146-new-body.md`, `REPORT-HOWTO.md`.
- The old scratchpad `…\e6db1900-f1cd-4099-995e-e1ef6bec1004\scratchpad\`: the report template, images, `morning-questions-2026-09-09.md`.
- Memory: `C:\Users\perzi\.claude\projects\C--Users-perzi-dads-mmo-lab\memory\` (copy the folder to the WSL project dir `~/.claude/projects/-mnt-c-Users-perzi-dads-mmo-lab/memory/`).
