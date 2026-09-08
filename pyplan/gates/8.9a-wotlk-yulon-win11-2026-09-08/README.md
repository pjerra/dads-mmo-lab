# 8.9a, the Windows half — uninstall and purge on `yulon-win11`, 2026-09-08

The half 8.9a's entry gives its own line: *the read-only bit on git objects is the thing that
differs*. Pressed on a real WotLK install at `D:\wow-server` (project `yulon-wow-wotlk-643123e5`,
2.4 GB, 12,405 entries) by a workflow lane on the afternoon of 2026-09-08, and finished by hand
after the laptop crash cut the lane short. The driver is `gate89a_win.py` — `gate89a.py` from the
Linux gate with its constants changed and one stage added, `readonly`.

**Verdict: the Windows half PASSED, and it found the defect it was sent for — only not the one the
entry predicted.** The read-only bit was handled by the retry already shipped. What stopped the
uninstall on this box was a symlink git had made *from inside Linux*.

## What was pressed, in order

| Stage | Log | What it proved |
|---|---|---|
| `seed` | `logs/seed.log` | the ground: the install's containers, the two volumes, four images, `state.json` row, `.yulon-install.json`, ownership `OWNED` |
| `readonly` | `logs/readonly.log` | **ground:** git packs really are ReadOnly on NTFS. **Control:** a plain `shutil.rmtree` STOPS on that bit (`deleted the read-only tree: False`). **Fix:** `purge.remove_tree` deletes the same tree (`True`). **Failure path:** with a file held open, `remove_tree` raises naming the folder (`the message names the folder: True`, `SILENT SUCCESS WITH FILES STILL ON DISK: False`) |
| `claimshot` / `stage4` | `logs/claimshot.log`, `logs/stage4.log`, shots 04–07 | an install whose ownership cannot be proved is refused, not removed: OWNED (04), a truncated record (05), a record from a newer version (06), no record at all (07); the real install in another folder untouched |
| `stage3` | `logs/stage3.log` | a purge of a RUNNING server refuses and says to stop it first; `plan()` reads only |
| `stage2` (first press) | `logs/stage2.log` | the real unticked press through the app's own buttons: containers, both volumes and the four images gone — and then `D:\wow-server could not be deleted ([WinError 1920] The file cannot be accessed by the system: 'D:\wow-server\.claude\skills\generate-pr-description')`. The app NAMED the path (that is the clause), the record stayed, the folder stayed |
| `retry` (second press, same code) | `logs/retry.log` | the same stop at the same path — a second press could not finish the job the message promised it could |
| `probe` | `probe_lxsymlink.py`, `logs/probe-lxsymlink.log` | what the entry IS, asked of the box: `fsutil` tag `0xa000001d` = `IO_REPARSE_TAG_LX_SYMLINK`; `Path.is_symlink()` False, `entry.is_dir(follow_symlinks=False)` True, `os.stat` WinError 1920; **`os.rmdir` removes it**, and `shutil.rmtree` then walks the rest |
| `retry` (third press, after the fix, `e1c4d0f5`) | `logs/retry2-after-the-fix.log`, shots `04-retry-*` | ground 12,405 entries, `.git` 28 files 0 read-only (the first press's chmod pass); the press; **`D:\wow-server exists: False`, `state.json` no longer lists it, no WotLK volume left** |

## The defect, and why the Linux gate could not have found it

AzerothCore's clone stage runs `alpine/git` in a container with the server dir bind-mounted. On a
Linux box the symlinks in the repository are symlinks. On Windows the same bind mount goes through
WSL2, and every symlink lands on NTFS as an LX reparse point that no Win32 call can open. Python
reports it as a directory it cannot enter; `shutil.rmtree` recurses and dies. Pressing again does
the same thing — the read-only retry adds a write bit to an entry that is not read-only but
unreadable.

The fix (`purge._remove_unenterable`, commit `e1c4d0f5`) runs after the read-only pass and
`os.rmdir`s every entry a walk cannot enter — the reparse point here, a directory whose mode
refuses `scandir` on POSIX (the lane's own test, skipped on Windows and as root, RED on
yulon-fedora before the change and GREEN after). `rmdir` removes a link and never its target, which
is what makes it safe over a checkout that may point outside itself.

## What is not proved here

* The **ticked** clause (kept volume, reinstall finds the characters) was pressed on the Linux
  gate; this half took the unticked press only, as its entry scopes it.
* `04-retry-after-tile.png` still reads *Installed*: this box has two OTHER WotLK installs in
  `state.json` (`C:\Users\pk\wow-server-playerbots` and one under `wow test install`), so the tile
  is right to. The negative for THIS install is the filesystem and the record, in the log.
* The first-press log carries `could not read the state of ac-worldserver` warnings from the
  status poll — the containers were already gone by then; noise, not a defect.
* The third press ran over ssh with Qt offscreen, as the first two did; no interactive session.
