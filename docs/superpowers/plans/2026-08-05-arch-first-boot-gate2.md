# Task 10 live gate, round 2 — first-boot re-proof on a genuinely fresh image

Date: 2026-08-05. Branch: `feat/arch-wsl-backend`. Throwaway distro name: `dml-arch-gate2`.
`dml-arch` (the user's real, working server) was never read from, written to, or touched by
any command in this run. `dml-arch-test` (the leftover throwaway from the 2026-08-04 gate)
was also left completely alone — it is not this run's distro and nothing here names it.
**No `wsl --unregister` of any kind was executed against anything.** Run was unattended
(user asleep, pre-authorised); PowerShell was used for every `wsl.exe` invocation specifically
to avoid the Git Bash path-mangling problem the 2026-08-04 gate hit.

## What this run is for

The 2026-08-04 gate (`docs/superpowers/plans/2026-08-04-arch-wsl-backend-gate.md`) found three
first-boot bugs on a real fresh image and the operator hand-corrected all of them live:

1. sudoers drop-in written before the `sudo` package was installed → `/etc/sudoers.d` did not
   exist → `sh: line 1: /etc/sudoers.d/99-dml: No such file or directory` (exit 1)
2. pacman keyring never initialized → the first package install failed with
   `required key missing from keyring` (exit 1)
3. no restart boundary after writing `/etc/wsl.conf` → systemd was not PID 1 in the boot that
   wrote it → `systemctl enable --now docker` would have failed

All three were fixed in `crates/dml-core/src/distro.rs` (commits visible in this branch's log,
including `fc2d59d fix(core): the first-boot sequence carries its own restart boundary`) but
never re-proven on a machine. **This run executes exactly what `first_boot_steps("dml")`
returns, in the order it returns it, with no improvisation and no workaround** — the point is
to find out whether the fixes hold on a real image, not to get a green run by any means.

Steps were read verbatim out of `crates/dml-core/src/distro.rs` as it stands on this branch
(the current `first_boot_steps` order: `wsl-conf → restart → useradd → pacman-key →
pacman-sync → sudoers → docker-group → docker-enable`) and transcribed into real `wsl.exe`
argv — no step was reordered, skipped, merged, or added to. Where a first-boot step runs `sh
-c "..."`, the exact string (including the embedded literal newline inside the single-quoted
`printf` argument) was reproduced.

## Step 1: confirm the throwaway name is free

```
wsl --list --verbose
```

Output: `dml-arch` (default, Stopped, v2), `docker-desktop` (Stopped, v2), `dml-arch-test`
(Stopped, v2 — the 2026-08-04 gate's leftover). **No `dml-arch-gate2`.** Confirmed free,
proceeded. Neither `dml-arch` nor `dml-arch-test` was touched at any point in this run.

## Step 2: create the distro

```
wsl --install archlinux --name dml-arch-gate2 --no-launch
```

- Exit: **0**
- Start: `2026-08-05T00:53:15.254+02:00`, End: `2026-08-05T00:53:34.830+02:00` —
  **wall-clock ≈ 19.58 seconds**
- Output: `Downloading: Arch Linux` / `Installing: Arch Linux` / `Distribution successfully
  installed. It can be launched via 'wsl.exe -d dml-arch-gate2'`
- `wsl --install` prints no download size (same as the prior gate). As a proxy: the resulting
  `ext4.vhdx` (found via `HKCU:\Software\Microsoft\Windows\CurrentVersion\Lxss\{guid}`,
  `BasePath=C:\Users\perzi\AppData\Local\wsl\{7e722017-cc1d-4d30-9d5a-2a404e99ccff}`) was
  **683,671,552 bytes (652 MB)** immediately after install — identical to the 2026-08-04
  gate's proxy figure for the same base rootfs image.
- `wsl --list --verbose` afterward confirmed `dml-arch-gate2` present, Stopped, v2.

## Step 3: first-boot sequence — `first_boot_steps("dml")`, transcribed verbatim

Every command below is the literal argv from `crates/dml-core/src/distro.rs` as of this
branch, crossed over `wsl.exe -d dml-arch-gate2 -u root --exec` for `InDistro` steps, or run
directly on the host for the one `RestartDistro` step. **Executed once, in order, no retries,
no corrections, no steps skipped.**

| # | id | Command | Exit | Notes |
|---|---|---|---|---|
| 1 | `wsl-conf` | `-u root --exec sh -c "printf %s '[boot]\nsystemd=true\n' > /etc/wsl.conf"` | **0** | First try. |
| 2 | `restart` | `wsl --terminate dml-arch-gate2` | **0** | First try. THE boundary — applies the systemd setting before anything that needs it. |
| 3 | `useradd` | `-u root --exec useradd -m -G wheel dml` | **0** | First try. Distro auto-boots fresh (systemd now PID 1) on this call since it was stopped by the restart. |
| 4 | `pacman-key` | `-u root --exec sh -c "pacman-key --init && pacman-key --populate archlinux"` | **0** | First try, **6.61s**. `Locally signed 5 keys`, `Disabled 38 keys`, `next trustdb check due at 2026-10-21`. |
| 5 | `pacman-sync` | `-u root --exec pacman -Syu --noconfirm --needed docker docker-compose docker-buildx git sudo` | **0** | First try, **9.64s**. No keyring error, no retry needed (contrast with 2026-08-04, which needed two tries here). |
| 6 | `sudoers` | `-u root --exec sh -c "mkdir -p /etc/sudoers.d && printf %s 'dml ALL=(ALL) NOPASSWD: ALL\n' > /etc/sudoers.d/99-dml && chmod 0440 /etc/sudoers.d/99-dml"` | **0** | First try. `/etc/sudoers.d` already existed (created by the `sudo` package in step 5, which ran first). |
| 7 | `docker-group` | `-u root --exec usermod -aG docker dml` | **0** | First try. |
| 8 | `docker-enable` | `-u root --exec systemctl enable --now docker` | **0** | First try. `Created symlink '/etc/systemd/system/multi-user.target.wants/docker.service' → '/usr/lib/systemd/system/docker.service'`. systemd was PID 1 (the restart in step 2 already applied), so this needed no manual intervention. |

**Zero hand-corrections across all 8 steps.** Every exit code was 0 on the first and only
attempt, in the exact order the current code returns.

`pacman-sync`'s package resolution (18 packages incl. transitive deps): **docker
`1:29.7.1-1`**, **docker-buildx `0.36.0-1`**, **docker-compose `5.4.0-1`**, git `2.55.0-1`,
sudo `1.9.17.p2-6`, plus containerd, runc, perl, shadow, etc. Total download size **114.01
MiB**, total installed size **454.98 MiB**, net upgrade **440.73 MiB** — identical figures to
the 2026-08-04 gate, meaning the upstream mirror snapshot had not moved in the intervening
24 hours.

## Step 4: set the default user

```
wsl --manage dml-arch-gate2 --set-default-user dml
```

Exit: **0**. First try.

## Step 5: prove the daemon and the chain

All run as `-d dml-arch-gate2 -u dml` (the now-default, unprivileged user) to prove the whole
chain — not just root's view of it:

| Check | Result | Exit |
|---|---|---|
| `systemctl is-active --quiet docker` | (silent) | **0** |
| `docker info --format "{{.ServerVersion}} {{.Driver}}"` | `29.7.1 overlayfs` | **0** |
| `docker buildx version` | `github.com/docker/buildx 0.36.0 df28b0a0b6a44453a87bd53c438432f4120962c9` | **0** |
| `docker compose version` | `Docker Compose version 5.4.0` | **0** |
| `sudo -n true` | (silent) | **0** |
| `docker ps` (extra, read-only) | empty table, header only | **0** — proves the `dml` user's docker-group membership works in practice, not just in `/etc/group` |
| `id dml` (extra, read-only) | `uid=1000(dml) gid=1000(dml) groups=1000(dml),998(wheel),969(docker)` | **0** |

**Version comparison against the pinned known-good set** (`crates/dml-core/src/distro.rs`
doc comment, pinned 2026-08-04):

| Package | Pinned known-good | Observed | Drift |
|---|---|---|---|
| docker | 29.6.1 | **29.7.1** | +1 minor-ish (upstream bump) |
| docker-compose | 5.3.1 | **5.4.0** | +1 minor |
| docker-buildx | 0.35.0 | **0.36.0** | +1 minor |

All three ahead of the pinned set, and identical to what the 2026-08-04 gate observed — this
is the rolling-release risk the design already names, not a gate failure; recorded here so it
stays visible.

## Supplementary read-back (after the sequence had already fully succeeded)

Not part of `first_boot_steps` — read-only confirmation checks run after step 5, for the
record:

- `cat /etc/wsl.conf` (as `dml`): `[boot]\nsystemd=true\n` — exit **0**, matches `WSL_CONF`
  exactly.
- `cat /etc/sudoers.d/99-dml` (as `dml`, unprivileged): **exit 1**, `Permission denied`.
  `ls -l /etc/sudoers.d/99-dml` (as `dml`): **exit 2**, `Permission denied`. **This is
  correct, expected behaviour, not a defect** — `/etc/sudoers.d` and the drop-in are
  root-only by design (`chmod 0440`, owned by `root:root`), so an unprivileged user cannot
  traverse or read it. It is direct evidence the permissions are correctly restrictive, not a
  bug in the first-boot sequence; `sudo -n true` (above, exit 0) is the correct proof that the
  rule itself works.
- Re-run as root: `cat /etc/sudoers.d/99-dml` → `dml ALL=(ALL) NOPASSWD: ALL`, exit **0**.
  `ls -l` → `-r--r----- 1 root root 28 Aug  5 00:55 /etc/sudoers.d/99-dml`, exit **0**. Exact
  content and size (28 bytes) match the 2026-08-04 gate.
- One earlier attempt chained five read-only commands together in a single `sh -c "a; echo
  ---; b; ..."` call and produced garbled/truncated console output (a display artifact of the
  chained call, not a real failure — re-running each command individually, above, produced
  clean, unambiguous results). Noted here for transparency; it has no bearing on the
  first-boot sequence, which had already completed with all exit 0 before this diagnostic
  detour started.

## Timing summary

Precisely instrumented:

- Step 2 (`wsl --install`): **19.58 s**
- `pacman-key`: **6.61 s**
- `pacman-sync`: **9.64 s** (absolute: `2026-08-05T00:55:24.577+02:00` →
  `2026-08-05T00:55:34.220+02:00`)

The remaining first-boot steps (`wsl-conf`, `restart`, `useradd`, `sudoers`, `docker-group`,
`docker-enable`) and `set-default-user` were not individually wrapped in timers, but every one
returned promptly on a single attempt with no waiting and no retries — consistent with the
2026-08-04 gate, where these same steps were never the slow part.

Total session wall-clock, install start (`00:53:15.25`) to the last verification read
(`00:58:50.91`): **≈335.7 s (5 m 36 s)**. This figure is an upper bound on the whole gate
session, not a tight measurement of provisioning alone — it includes the two informational
`wsl --list --verbose`/registry checks, ordinary inter-command overhead, and the supplementary
read-back detour described above (including the one garbled/re-run diagnostic). The
provisioning-only critical path (install + 8 first-boot steps + set-default-user) is
substantially shorter than the session total; the three precisely-measured heavy steps alone
sum to **35.83 s**.

## Verdict on each of the three previously-found bugs

1. **Sudoers-before-sudo-package ordering — FIXED, confirmed live.** The code now runs
   `pacman-sync` (which installs `sudo`) before `sudoers`. The `sudoers` step succeeded on the
   first try, exit 0 — no "No such file or directory," because `/etc/sudoers.d` already
   existed by the time the step ran.
2. **Missing pacman keyring init — FIXED, confirmed live.** The code now has a dedicated
   `pacman-key` step (`pacman-key --init && pacman-key --populate archlinux`) before
   `pacman-sync`. It ran exit 0 in 6.61 s, and the subsequent `pacman-sync` succeeded on its
   **first** attempt (0 retries) — contrast with 2026-08-04, where the first `pacman -Syu`
   failed with `required key missing from keyring` and had to be retried after a manual key
   init.
3. **Missing restart boundary before `docker-enable` — FIXED, confirmed live.** The code now
   has an explicit `restart` step (`wsl --terminate dml-arch-gate2`) immediately after
   `wsl-conf`, before anything that needs systemd. `docker-enable` succeeded on the first try,
   exit 0, `Created symlink .../docker.service` — no manual `wsl --terminate` was needed
   anywhere in this run, unlike 2026-08-04 where the operator had to insert it by hand.

**The sequence ran end-to-end with NO hand-correction of any kind.** All 8 first-boot steps,
the install, the set-default-user call, and all 5 prescribed verification checks returned exit
0 on their first and only attempt, in the exact order `first_boot_steps("dml")` returns today.
The only non-zero exits in this entire run (`cat`/`ls` as the unprivileged `dml` user against
the root-only sudoers drop-in) were supplementary checks outside the prescribed sequence and
are themselves confirmation of correct, restrictive file permissions — not defects.

## Cleanup

**Not performed in this run**, per the standing safety rules for this task: no
`wsl --unregister` of any kind was executed against `dml-arch-gate2`, `dml-arch`,
`dml-arch-test`, or anything else. `dml-arch-gate2` was left `Running` (its docker daemon is
up) and registered for the controller to inspect and dispose of.
