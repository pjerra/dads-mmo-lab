# T23 — The 7.10 clause-35 runner has two fail-open paths (restart window, firewall backup)

**Status:** MERGING (hand DONE `cc7a877f`, evidence review ACCEPT; gate behind the merge on m910q)
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

## Report (hand, Opus, 2026-09-10 19:30 CEST)

- `cc7a877f` on `hand-t23` (base `2432b65a`); 72 files +2985, all under the new `pyplan/gates/7.10-clause35-ubuntu2-2026-09-10/`; the 09-09 folder and `pylauncher/` untouched; no app fix needed; the full gate not run (nothing under `pylauncher/` or `tests/` changed, stated in `gates.txt`); `test_no_secrets_in_evidence.py` 4 passed.
- The boundary rule (`run-t3.sh:339-349`, enforced `:364-398`): the only window read is the restarted container's own `StartedAt`, taken after `docker restart` returned, used only if it differs from the value read before; unreadable or unchanged -> no window read, RESTORATION FAILURE; the stamp (RFC3339, UTC, `Z`) handed to Docker printed verbatim; the precheck stays, labelled not load-bearing.
- The backup rule (`:498-508`, enforced `:509-524`; the trap's half `:233-243`, enforced `:245-263`): both firewall files copied, chowned, `cmp`-verified before anything that can press Apply, else stop with status 70 and no driver; once `APPLY_PHASE_ENTERED` (`:578`), a missing, empty or short backup is a RESTORATION FAILURE, never `[RESTORED]`.
- Captures: `mut-backup-refused/` (`chattr +i` on both targets, status 70, no driver output, `/etc/ufw` hashes identical); `mut-backup-lost/` (copies verified, Apply phase entered, both deleted, status 90, the trap's "backup is missing, empty or short ... NOT evidence that nothing changed"); `mut-stale-window/` (a line planted in the gap with the authserver silenced by `Logger.root` 4->3 for one run, conf restored by `cmp`; status 90; `contrast.txt`: the same log one second apart -- the round-4 clock window returns the planted line, the `StartedAt` window returns nothing); `failrestore/` (failing driver plus `T3_REALM_ROW_ID=9999`: two failures, the row put back by hand in ~45 s); `failclosed/` (status 1, box restored and verified); `green/` (33 OK, 0 FAIL). Every capture carries `docker inspect` lines; the world at pid 91802 throughout.
- Box as left: world up on T13's process, never touched; realm row and port as before; ufw inactive, rule hashes unchanged, `root:root 640`; both conf md5s unchanged; accounts 101-103 only, the widget account gone; PERZI unused; 1001 characters; authserver restarted nine times; the transfer dir and conf copy off the box. A `realmlist.flag = 2` reading 37 s after the last restart settled to 0 within 45 s, recorded as a false alarm with the settled reading committed. The box's LAN address moved to 172.26.8.248 (recorded, the clause does not depend on it).
- Deviations: a sixth run (the trap's half of finding 2); two exercise knobs (`T23_PLANT_IN_GAP`, `T23_DROP_BACKUP_AFTER_APPLY_PHASE`); the silencer edits `authserver.conf` for one run; the box's lane copies now hold the T23 runner with round 4's kept beside; unexercised: the SIGINT/SIGTERM path and `cmp`'s failing direction. Status DONE.

## Review (cold Opus reviewer, 2026-09-10 19:36 CEST) -- ACCEPT, no must-fixes

Finding 1 closed: `StartedAt` read before and after, refused if unmoved, handed to Docker verbatim as RFC3339Nano with `Z`; the plant is provably in the gap and `contrast.txt` shows the clock window seeing it and the `StartedAt` window not; the silence is real (conf back byte for byte, the container up throughout) and the control restart with the voice back finds the line with the same stamp shape. Finding 2 closed: `cp`, `chown`, `cmp` each `|| die 70`, no driver output under the refusal; the trap records a RESTORATION FAILURE for a lost backup after the Apply phase. The three named runs, the box as left, the knobs (inert unset, printed in every header), scope and secrets all check. Notes: the restart count was off by one (fixed by the lead, `afdddd58`); the trap's staleness test is size-only (a hash beside the byte count would close it; not overclaimed); the file set named the 09-09 folder and the hand shipped a new one instead (registered, better); `PERZI last_login` has no before-reading in this folder.
