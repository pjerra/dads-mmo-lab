# scripts/vm-deploy — pushing builds to a remote Windows server

Two scripts for driving a real server box (dad's VM) from a dev machine over
SSH. Both encode failures that already happened; the comments in each say
which, and are worth reading before simplifying anything.

| Script | Job |
|---|---|
| `deploy-launcher.sh` | Install a locally built launcher on the remote box and restart it |
| `run-detached.sh` | Start a long job (rebuild, install) so it survives the SSH session |

## The four traps, in one place

**1. Windows OpenSSH kills the child process tree when the session ends.**
A 90-minute `module rebuild` launched over SSH died about a minute after
disconnecting — twice — and its captured stdout simply stopped mid-sentence,
which reads exactly like a crash rather than a kill. Use `run-detached.sh`,
which hands ownership to a scheduled task.

**2. `schtasks /tr` loses the quoting around a path containing spaces.**
`C:\Users\...\DML Launcher\launcher.exe` became `C:\Users\...\DML` plus a
stray argument and failed with `0x80070002` (file not found). Both scripts
therefore point the task at a wrapper `.cmd` on a space-free path.

**3. A GUI app started over SSH lands in session 0, invisible on the user's
desktop.** The restart goes through `schtasks /it` so the window appears where
the user can actually see it. This is also required because Docker Desktop is
per-user on this box: its engine is only reachable from the logged-on session.

**4. PowerShell's `Start-Job` cannot stream a live process's stdout.** The
`Process` object does not survive the job boundary, so nothing drains the pipe
and the child dies when it fills. Redirect to a file and tail that.

## Guard design

`deploy-launcher.sh` refuses when stopping the launcher would kill work, and
that judgement is narrower than it first appears:

* The question is **not** "is a build running" but "would stopping the
  launcher kill it". A rebuild started from the Modules page is a child of
  `launcher.exe`; one started by `run-detached.sh` hangs off `cmd.exe` and is
  unaffected. Deploying alongside the latter is safe and allowed.
* The test is process **age**, not existence. The launcher always has a
  transient `docker.exe` child because it polls server status every few
  seconds, so an existence check refused every deploy forever. A poll lives
  seconds; a build lives an hour. Reading `CommandLine` does not separate them
  either — CIM returns it empty for these short-lived children.

## Ordering rules

* **Back up before stopping anything**, so "undo" exists ahead of the first
  irreversible step rather than after it.
* **Judge the install by effect.** The installer exits 0 on paths that changed
  nothing; only the binary's own size and timestamp prove a swap happened.

## Example

```bash
# from the repo root, after: cd launcher && npm run tauri build
scripts/vm-deploy/deploy-launcher.sh --host perzi@100.99.161.102

# start a rebuild that outlives this shell
scripts/vm-deploy/run-detached.sh --host perzi@100.99.161.102 \
    --name dml-rebuild-now \
    --env 'DML_GAMES_DIR=C:\Users\perzi\dml-native' \
    --cmd 'dml-wow.exe module rebuild --backup'
```
