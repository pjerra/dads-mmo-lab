# T23 — The 7.10 clause-35 runner has two fail-open paths (restart window, firewall backup)

**Status:** IN PROGRESS (Opus hand on `hand-t23` since 2026-09-10 18:59 CEST; the host is back, the box free after T13's live half)
**Filed:** 2026-09-10 15:06 CEST by the lead (Fable), from the owed Codex adversarial pass on T3 round 4 (`a4454ec0`), two high findings
**Hand:** Opus (a live-box runner; both fixes are argued against a running server), worktree branched from `yulon-phase8b` (ff-merge `origin/yulon-phase8b` first; report the sha)
**File set (yours alone):** `pyplan/gates/7.10-clause35-ubuntu2-2026-09-09/run-t3.sh` and the folder's README, plus a NEW capture folder `pyplan/gates/7.10-clause35-ubuntu2-<date>/` for the reruns. Nothing under `pylauncher/`.
**Box:** `yulon-ubuntu2`, only on the lead's word, when the host is back.

## The findings (Codex, on `a4454ec0`)

1. **The restart window opens before the restart.** `run-t3.sh:243-249` captures and checks the stamp; `docker restart` starts at `:250`. An `Added realm` line emitted in that gap is read by the later `docker logs --since` query as evidence the restarted server announced itself; if the restarted process announces nothing, the pre-restart line makes the restoration verification pass. The zero-count precheck does not close the gap.
2. **A failed firewall backup does not stop the Apply.** The two privileged copies and their `chown` at `:395-404` are unchecked and the script runs without `set -e`, so the driver that presses Apply runs regardless; the exit trap reads a missing `ufw-user.rules.before` as proof nothing changed and records `[RESTORED]` while the live firewall may already be altered.

## What to build

- (1) Bound the window by the restarted container's own `StartedAt` (read it AFTER `docker restart` returns, compare it to the value read before, refuse if unchanged), and query `docker logs --since <that stamp, with Z>` — `docker logs --since` reads a zone-less stamp in the client's zone (memory `docker-logs-since-is-client-local`); print what Docker is given. Mutation: a matching line planted in the gap with the restarted server silenced must make the run RED.
- (2) Before the driver: both backups copied, owned, and byte-compared to the live files, or the run stops with the reason and the driver never starts. In the trap: once the Apply phase was entered, a missing or short backup is a restoration FAILURE, recorded as such, never `[RESTORED]`. Mutation: both copies forced to fail -> the driver is never invoked.
- Rerun the three captures (green, failed driver, failed restoration) plus the two mutations, the process alive at each capture, the owner's things untouched; `test_no_secrets_in_evidence.py` green before any log is committed.

## Definition of done

The five captures with the world's own log lines; the README states the boundary rule and the backup rule. One commit, `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>` only; no push; the ticket file is not yours. Scratch under `<scratchpad>/T23/`.
