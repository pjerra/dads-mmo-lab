# Task 10 live gate — provision a distro from nothing

Date: 2026-08-04. Branch: `feat/arch-wsl-backend`. Throwaway distro name: `dml-arch-test`.
`dml-arch` (the user's real distro) was never touched by any `--unregister` in this run.

This is a run log with real exit codes and real output, not a "worked fine" summary.
Two of the brief's steps were corrected before running (Steps 5/7, per the task
instructions); two MORE bugs were found live during the run and are recorded below —
both are first-run failures every future user will hit on a truly fresh install.

## Prerequisite: build a Linux `dml-wow` ELF (not in the brief)

No Linux binary existed anywhere in the repo. Built one inside the user's existing,
already-provisioned `dml-arch` distro (sanctioned — user approved installing rust there).

| Command | Exit | Notes |
|---|---|---|
| `pacman -S --noconfirm --needed rust` (1st try) | **1** | Stale mirror state: `fastly.mirror.pkgbuild.com` / `geo.mirror.pkgbuild.com` both 404'd on `rust-1:1.96.1-1`, `llvm-libs-22.1.8-1`, `libgit2-1:1.9.4-1`. |
| `pacman -Syy --noconfirm` | 0 | Refreshed package databases. |
| `pacman -S --noconfirm --needed rust` (2nd try) | 0 | Installed rust `1:1.97.1-1` + deps (llvm-libs 22.1.8-2, libgit2 1.9.6-1, lld 22.1.8-1, compiler-rt 22.1.8-1, llhttp 9.3.1-1, libedit 20260512_3.1-1). Download 120.42 MiB. |
| `CARGO_TARGET_DIR=$HOME/target cargo build -p dml-wow-cli --release` | 0 | `Finished \`release\` profile [optimized] target(s) in 51.69s` (cargo's own reported time; full wall time incl. crate downloads was a few minutes longer). |
| `file $HOME/target/release/dml-wow` | 0 | `ELF 64-bit LSB pie executable, x86-64, ... dynamically linked, ... for GNU/Linux 4.4.0, not stripped`. Size 12,647,632 bytes. |
| `cp "$HOME/target/release/dml-wow" /mnt/c/.../target/dml-wow-linux` | 0 | Two earlier attempts failed (exit 1/2) because Git Bash expanded `$HOME` on the **Windows** side before ever reaching `wsl.exe`; fixed by wrapping in `sh -c '...'` so `$HOME` expands inside WSL. |

## Step 1: confirm the throwaway name is free

```
wsl --list --verbose
```

Output: `dml-arch` (Stopped, v2), `docker-desktop` (Stopped, v2). **No `dml-arch-test`.** Confirmed free, proceeded.

## Step 2: create the distro

```
wsl --install archlinux --name dml-arch-test --no-launch
```

- Exit: 0
- Start: `2026-08-04T20:49:55Z`, End: `2026-08-04T20:50:15Z` — **wall-clock ≈ 20 seconds**
- Output: `Downloading: Arch Linux` / `Installing: Arch Linux` / `Distribution successfully installed.`
- `wsl --install` does not print a download size. As a proxy, the resulting `ext4.vhdx`
  (`%LOCALAPPDATA%\wsl\{guid}\ext4.vhdx`) was **652 MB** immediately after install,
  before any first-boot step ran.
- `wsl --list --verbose` afterward confirmed `dml-arch-test` present, Stopped, v2.

## Step 3: first-boot sequence

| # | Command | Exit | Notes |
|---|---|---|---|
| a | write `/etc/wsl.conf` (`[boot]\nsystemd=true\n`) as root | 0 | |
| b | `wsl --terminate dml-arch-test` | 0 | Applies the systemd setting. |
| c | `useradd -m -G wheel dml` as root | 0 | |
| d | write `/etc/sudoers.d/99-dml` + `chmod 0440` (1st try) | **1** | `sh: line 1: /etc/sudoers.d/99-dml: No such file or directory`. **Real bug**: the `sudo` package is not installed at this point in the brief's ordering, so `/etc/sudoers.d` does not exist on a fresh Arch WSL image. Confirmed via `ls -la /etc/sudoers.d` → "No such file or directory". |
| e | `pacman -Syu --noconfirm --needed docker docker-compose docker-buildx git sudo` (1st try) | **1** | `warning: Public keyring not found; have you run 'pacman-key --init'?` / `error: keyring is not writable` / `error: required key missing from keyring`. **Second real bug**: a fresh Arch WSL image's pacman keyring is never initialized. |
| e-fix1 | `pacman-key --init` as root | 0 | |
| e-fix2 | `pacman-key --populate archlinux` as root | 0 | |
| e (retry) | `pacman -Syu --noconfirm --needed docker docker-compose docker-buildx git sudo` | 0 | Installed: **docker 1:29.7.1-1**, **docker-buildx 0.36.0-1**, **docker-compose 5.4.0-1**, git 2.55.0-1, **sudo 1.9.17.p2-6**, + transitive deps (containerd 2.3.3-1, runc 1.5.1-1, perl 5.42.2-1, shadow 4.20.0.arch1-1, etc). Download 114.01 MiB, installed size 454.98 MiB. |
| d (retry) | write `/etc/sudoers.d/99-dml` + `chmod 0440` | 0 | Verified: `cat` shows `dml ALL=(ALL) NOPASSWD: ALL`; `ls -l` shows `-r--r----- 1 root root 28`. |
| f | `usermod -aG docker dml` as root | 0 | |
| g | `systemctl enable --now docker` as root | 0 | `Created symlink .../docker.service`. |
| h | `wsl --manage dml-arch-test --set-default-user dml` | 0 | |

**Ordering fix required for a truly fresh image:** packages (including `sudo`) must be
installed, and the pacman keyring initialized, **before** the sudoers.d write — the
brief's literal step order (sudoers before pacman) fails on a fresh Arch rootfs. Both
failures were worked around live and every command was re-run to exit 0.

## Step 4: prove the daemon and the chain

| Check | Result | Exit |
|---|---|---|
| `systemctl is-active --quiet docker` | (silent) | 0 |
| `docker info --format "{{.ServerVersion}} {{.Driver}}"` | `29.7.1 overlayfs` | 0 |
| `docker buildx version` | `github.com/docker/buildx 0.36.0 df28b0a0b6a44453a87bd53c438432f4120962c9` | 0 |
| `docker compose version` | `Docker Compose version 5.4.0` | 0 |
| `sudo -n true` | (silent) | 0 |

**Version comparison against the pinned known-good set:**

| Package | Pinned known-good | Observed | Drift |
|---|---|---|---|
| docker | 29.6.1 | **29.7.1** | +1 patch-ish (upstream minor bump) |
| docker-compose | 5.3.1 | **5.4.0** | +1 minor |
| docker-buildx | 0.35.0 | **0.36.0** | +1 minor |

All three are ahead of the pinned set. This is the rolling-release risk the spec names,
not a gate failure — recorded here so it stays visible per the spec's intent.

## Step 5 (corrected): deploy the binary and round-trip the chain

Paths used:
- Source ELF (built in `dml-arch`, copied to Windows): `C:\Users\perzi\dads-mmo-lab\target\dml-wow-linux`
- Installed into `dml-arch-test` at: `/usr/local/bin/dml-wow`

| Command | Exit | Notes |
|---|---|---|
| `install -m 0755 /mnt/c/.../target/dml-wow-linux /usr/local/bin/dml-wow` (1st try, as root) | **1** | `install: cannot stat 'C:/Program Files/Git/mnt/c/Users/perzi/dads-mmo-lab/target/dml-wow-linux'`. Git Bash's automatic POSIX-path-to-Windows-path conversion mangled the `/mnt/c/...` argument before it ever reached `wsl.exe`. Not a distro/CLI bug — an artifact of running `wsl.exe` from Git Bash. |
| same, with `MSYS_NO_PATHCONV=1` | 0 | Disabling Git Bash's path conversion fixed it. |
| `dml-wow version` (as `dml`) | 0 | `{"data":{"backend":"native","contract":"dml-json-v3","version":"0.1.0"},"ok":true}` |
| titles-count loop (`~/games/*/` checked for `docker-compose.yml`/`.yaml`/`compose.yml`/`.yaml`) | 0 | Output: `0` |

**Result: `dml-wow version` returns a valid `dml-json-v3` envelope, and zero titles are
provisioned. That combination is `SetupState::NoTitles` — the correct end state for a
freshly provisioned distro with no server yet.**

## Timing summary

- Prerequisite (rust install + release build in `dml-arch`): not separately wall-clocked;
  `cargo build` itself reported 51.69s, total including package downloads was a few
  minutes.
- Step 2 (`wsl --install`): **~20 seconds**, 652 MB on-disk footprint immediately after.
- Steps 2 through 5 combined (distro create → binary round-trip, including the two
  live bug investigations and fixes): `2026-08-04T20:49:55Z` → `2026-08-04T20:53:38Z`
  = **≈3 minutes 43 seconds**.

## Outcome

The distro reached a provisioned state: systemd active, docker daemon running under
overlayfs, buildx and compose present, passwordless sudo working for the `dml` user,
and the native `dml-wow` binary runs and reports `SetupState::NoTitles`. **Two real
first-run bugs were found and must be fixed in the actual provisioning code** (not just
worked around here) before this ships to a real first-time user:

1. **Sudoers-before-sudo-package ordering.** `first_boot_steps("dml")` must install
   packages (or at least `sudo`) before writing `/etc/sudoers.d/99-dml`, or reorder so
   the sudoers write happens after the pacman step.
2. **Missing `pacman-key --init` / `--populate archlinux`.** A fresh Arch WSL rootfs has
   no initialized pacman keyring; the first `pacman -Syu` on any package will fail with
   `keyring is not writable` / `required key missing from keyring` until these two
   commands have been run. This must be added as its own first-boot step, before the
   first package install.

Neither bug is present in the corrected Step 5 (binary deploy) — that one's only failure
was an artifact of Git Bash's own path handling when invoking `wsl.exe` from a Windows
shell (`MSYS_NO_PATHCONV=1`), not something the shipped provisioning code will hit, since
production spawns `wsl.exe` directly rather than through Git Bash.

## Cleanup

**Not performed in this run, per explicit instruction.** `dml-arch-test` was left
running/registered for the operator to verify and unregister by hand. No
`wsl --unregister` command of any kind was executed against any distro during this run.
