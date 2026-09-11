"""What a WSL-made symlink is, to Windows Python, and what removes it.

The 8.9a Windows press stopped on
`D:\\wow-server\\.claude\\skills\\generate-pr-description` with
`[WinError 1920] The file cannot be accessed by the system`. `fsutil
reparsepoint query` says its tag is `0xa000001d` -- `IO_REPARSE_TAG_LX_SYMLINK`,
the shape WSL writes for a POSIX symlink. AzerothCore's clone stage runs
`alpine/git` in a container with the server dir bind-mounted, so git inside
Linux created it and native Win32 cannot read it.

This probe asks the machine which calls survive that, rather than reasoning
about it. It is read-mostly: every removal is done on a COPY of the link made
with `mklink`, never on the one in the install.
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
from pathlib import Path

LINK = Path(sys.argv[1] if len(sys.argv) > 1 else r"D:\wow-server\.claude\skills\generate-pr-description")
SCRATCH = Path(r"C:\gate89a\lxprobe")


def show(label: str, fn) -> None:
    try:
        print(f"  {label}: {fn()!r}")
    except OSError as exc:
        print(f"  {label}: {type(exc).__name__} [WinError {getattr(exc, 'winerror', None)}] {exc}")
    except Exception as exc:  # noqa: BLE001 -- a probe reports anything
        print(f"  {label}: {type(exc).__name__} {exc}")


def main() -> int:
    print(f"subject: {LINK}")
    print(f"exists (follows): {os.path.exists(LINK)}")
    print(f"lexists: {os.path.lexists(LINK)}")
    show("os.lstat", lambda: os.lstat(LINK))
    show("os.stat", lambda: os.stat(LINK))
    show("Path.is_symlink", LINK.is_symlink)
    show("Path.is_dir", LINK.is_dir)
    show("os.readlink", lambda: os.readlink(LINK))
    show("os.path.islink", lambda: os.path.islink(LINK))

    parent = LINK.parent
    print(f"\nscandir of {parent}:")
    with os.scandir(parent) as it:
        for entry in it:
            print(f"  {entry.name}")
            show("    entry.is_symlink()", entry.is_symlink)
            show("    entry.is_dir(follow_symlinks=False)", lambda e=entry: e.is_dir(follow_symlinks=False))
            show("    entry.is_file(follow_symlinks=False)", lambda e=entry: e.is_file(follow_symlinks=False))
            show("    entry.stat(follow_symlinks=False)", lambda e=entry: e.stat(follow_symlinks=False).st_mode)

    # -- what removes it. Done on copies, made by copying the reparse point
    #    with `robocopy /sl`? No: `mklink /J` cannot make an LX symlink at all,
    #    so the only real specimen is the one in the tree. These are tried in
    #    increasing order of destructiveness, and the first that works ends it.
    print("\nremoval attempts, in order, on the real link (the tree is rolled back after):")
    show("os.rmdir", lambda: os.rmdir(LINK))
    print(f"  still there: {os.path.lexists(LINK)}")
    if os.path.lexists(LINK):
        show("os.unlink", lambda: os.unlink(LINK))
        print(f"  still there: {os.path.lexists(LINK)}")
    if os.path.lexists(LINK):
        show("Path.unlink", LINK.unlink)
        print(f"  still there: {os.path.lexists(LINK)}")
    if os.path.lexists(LINK):
        out = subprocess.run(
            ["cmd", "/c", "rmdir", str(LINK)], capture_output=True, text=True
        )
        print(f"  cmd rmdir: rc={out.returncode} {out.stdout.strip()} {out.stderr.strip()}")
        print(f"  still there: {os.path.lexists(LINK)}")
    if os.path.lexists(LINK):
        out = subprocess.run(
            ["cmd", "/c", "del", "/f", "/q", str(LINK)], capture_output=True, text=True
        )
        print(f"  cmd del: rc={out.returncode} {out.stdout.strip()} {out.stderr.strip()}")
        print(f"  still there: {os.path.lexists(LINK)}")
    if os.path.lexists(LINK):
        out = subprocess.run(
            ["powershell", "-NoProfile", "-Command",
             f"[System.IO.Directory]::Delete('{LINK}')"],
            capture_output=True, text=True,
        )
        print(f"  Directory.Delete: rc={out.returncode} {out.stdout.strip()} {out.stderr.strip()}")
        print(f"  still there: {os.path.lexists(LINK)}")

    print(f"\nfinal: link still there: {os.path.lexists(LINK)}")
    if not os.path.lexists(LINK):
        print("-- with the link gone, can shutil.rmtree walk the rest of .claude? --")
        show("shutil.rmtree(.claude)", lambda: shutil.rmtree(LINK.parent.parent))
        print(f"  .claude still there: {(LINK.parent.parent).exists()}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
