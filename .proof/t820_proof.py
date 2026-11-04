#!/usr/bin/env python3
"""T820 proof: tuning.write_raw is atomic. Scratch script, NOT committed.

Usage: $HOME/y313/bin/python .proof/t820_proof.py
Child mode (used by case c only): t820_proof.py --child <mode> <dir> <marker>
"""
import errno
import hashlib
import os
import signal
import stat
import subprocess
import sys
import time
from pathlib import Path

REPO = Path(os.path.expanduser("~/wt-proofd/pylauncher"))
sys.path.insert(0, str(REPO))
from yulon import tuning  # noqa: E402

SELF = Path(__file__).resolve()
WORK = SELF.parent / "work"
RESULTS = []  # (case, verdict, detail)


def sha(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


def conf_text(n_lines: int, crlf: bool = False) -> bytes:
    nl = "\r\n" if crlf else "\n"
    rows = ["# worldserver.conf-like fixture (synthetic, T820 proof)", "[worldserver]", 'DataDir = "./"', "Rate.XP.Kill = 1"]
    rows += [f"Proof.Key.{i:06d} = value-{i:06d} padding-padding" for i in range(n_lines)]
    return (nl.join(rows) + nl).encode("utf-8")


def read_text(p: Path) -> str:
    with open(p, encoding="utf-8", newline="") as f:
        return f.read()


def make_edit(original: str) -> str:
    return original.replace("Rate.XP.Kill = 1", "Rate.XP.Kill = 2", 1)


def fresh(name: str) -> Path:
    d = WORK / name
    if d.exists():
        os.chmod(d, 0o700)
        for p in d.iterdir():
            p.unlink()
    d.mkdir(parents=True, exist_ok=True)
    os.chmod(d, 0o700)
    return d


def listing(d: Path) -> list[str]:
    return sorted(p.name for p in d.iterdir())


def record(case: str, ok: bool, detail: str) -> None:
    RESULTS.append((case, "PASS" if ok else "FAIL", detail))
    print(f"[{'PASS' if ok else 'FAIL'}] {case}: {detail}", flush=True)


# ---- fault-injection helpers -------------------------------------------------

def is_conf_temp(path: Path, name: str) -> bool:
    return Path(name).resolve() == path.with_name(path.name + tuning.TEMP_SUFFIX).resolve()


class HalfThenFull:
    """Writes half of one write() call, then raises ENOSPC (disk full mid-write)."""

    def __init__(self, f):
        self._f = f

    def write(self, s):
        self._f.write(s[: len(s) // 2])
        self._f.flush()
        raise OSError(errno.ENOSPC, "PROOF injected: No space left on device")

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        self._f.close()
        return False

    def __getattr__(self, name):
        return getattr(self._f, name)


class SlowWriter:
    """Child only: writes in 1 KiB chunks with a pause, logging bytes so the parent can time the kill."""

    def __init__(self, f, marker: Path):
        self._f = f
        self._marker = marker
        self._n = 0

    def write(self, s):
        for i in range(0, len(s), 1024):
            chunk = s[i : i + 1024]
            self._f.write(chunk)
            self._f.flush()
            self._n += len(chunk.encode("utf-8"))
            self._marker.write_text(str(self._n))
            time.sleep(0.01)

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        self._f.close()
        return False

    def __getattr__(self, name):
        return getattr(self._f, name)


# ---- cases ---------------------------------------------------------------------

def case_a() -> None:
    d = fresh("a")
    path = d / "worldserver.conf"
    path.write_bytes(conf_text(300))
    os.chmod(path, 0o640)
    orig = path.read_bytes()
    original = read_text(path)
    edited = make_edit(original)
    expected = tuning.save_text(original, edited)
    backup = tuning.write_raw(path, edited, original=original, root=d)
    got = path.read_bytes()
    baks = tuning.backups_of(path)
    mode = stat.S_IMODE(path.stat().st_mode)
    ok = (
        got == expected.encode("utf-8")
        and expected == edited
        and len(baks) == 1
        and baks[0] == backup
        and baks[0].read_bytes() == orig
        and mode == 0o640
        and listing(d) == sorted(["worldserver.conf", backup.name])
    )
    record(
        "a1 normal save (LF)",
        ok,
        f"orig sha={sha(orig)[:16]} new sha={sha(got)[:16]} expected==edited:{expected == edited} "
        f"backup={backup.name} backup sha={sha(baks[0].read_bytes())[:16] if baks else None} "
        f"mode={oct(mode)} files={listing(d)}",
    )


def case_a2() -> None:
    d = fresh("a2")
    path = d / "worldserver.conf"
    path.write_bytes(conf_text(50, crlf=True))
    orig = path.read_bytes()
    original = read_text(path)
    edited = make_edit(original)
    expected = tuning.save_text(original, edited)
    tuning.write_raw(path, edited, original=original, root=d)
    got = path.read_bytes()
    bare_lf = got.count(b"\n") - got.count(b"\r\n")
    ok = got == expected.encode("utf-8") and bare_lf == 0 and b"Rate.XP.Kill = 2\r\n" in got
    record(
        "a2 normal save (CRLF, no mixed endings)",
        ok,
        f"new sha={sha(got)[:16]} bare-LF count={bare_lf} edited line present={b'Rate.XP.Kill = 2' in got}",
    )


def case_b() -> None:
    d = fresh("b")
    path = d / "worldserver.conf"
    path.write_bytes(conf_text(300))
    orig = path.read_bytes()
    original = read_text(path)
    edited = make_edit(original)
    expected = tuning.save_text(original, edited).encode("utf-8")
    real_replace = os.replace
    seen = {}

    def boom(src, dst, *a, **k):
        if Path(dst).resolve() == path.resolve():
            seen["temp_size"] = os.path.getsize(src)
            seen["temp_is_full"] = os.path.getsize(src) == len(expected)
            raise OSError(errno.EIO, "PROOF injected: replace failed after temp was written")
        return real_replace(src, dst, *a, **k)

    os.replace = boom
    raised = None
    try:
        tuning.write_raw(path, edited, original=original, root=d)
    except OSError as e:
        raised = e
    finally:
        os.replace = real_replace
    after = path.read_bytes()
    baks = tuning.backups_of(path)
    ok = (
        raised is not None
        and seen.get("temp_is_full") is True
        and sha(after) == sha(orig)
        and after == orig
        and listing(d) == sorted(["worldserver.conf", baks[0].name]) and len(baks) == 1
        and baks[0].read_bytes() == orig
    )
    record(
        "b fault: os.replace raises after full temp written",
        ok,
        f"raised={type(raised).__name__}:{raised} temp_size={seen.get('temp_size')} "
        f"(expected {len(expected)}) orig sha={sha(orig)[:16]} after sha={sha(after)[:16]} "
        f"identical={after == orig} files={listing(d)}",
    )


def case_b2() -> None:
    d = fresh("b2")
    path = d / "worldserver.conf"
    path.write_bytes(conf_text(300))
    orig = path.read_bytes()
    original = read_text(path)
    edited = make_edit(original)
    real_fdopen = os.fdopen

    def fake_fdopen(fd, *a, **k):
        f = real_fdopen(fd, *a, **k)
        if is_conf_temp(path, os.readlink(f"/proc/self/fd/{fd}")):
            return HalfThenFull(f)
        return f

    os.fdopen = fake_fdopen
    raised = None
    try:
        tuning.write_raw(path, edited, original=original, root=d)
    except OSError as e:
        raised = e
    finally:
        os.fdopen = real_fdopen
    after = path.read_bytes()
    baks = tuning.backups_of(path)
    ok = (
        raised is not None
        and raised.errno == errno.ENOSPC
        and after == orig
        and listing(d) == sorted(["worldserver.conf", baks[0].name]) and len(baks) == 1
        and baks[0].read_bytes() == orig
    )
    record(
        "b2 disk full (ENOSPC injected mid-write of temp)",
        ok,
        f"raised={type(raised).__name__} errno={getattr(raised, 'errno', None)} orig sha={sha(orig)[:16]} "
        f"after sha={sha(after)[:16]} identical={after == orig} files={listing(d)}",
    )


def child(mode: str, d: Path, marker: Path) -> None:
    path = d / "worldserver.conf"
    original = read_text(path)
    edited = make_edit(original)
    real_replace = os.replace
    real_fdopen = os.fdopen

    if mode == "pre-replace":
        def hang(src, dst, *a, **k):
            if Path(dst).resolve() == path.resolve():
                marker.write_text(str(os.path.getsize(src)))
                time.sleep(120)
            return real_replace(src, dst, *a, **k)
        os.replace = hang
    elif mode == "mid-write":
        def fake_fdopen(fd, *a, **k):
            f = real_fdopen(fd, *a, **k)
            if is_conf_temp(path, os.readlink(f"/proc/self/fd/{fd}")):
                return SlowWriter(f, marker)
            return f
        os.fdopen = fake_fdopen
    tuning.write_raw(path, edited, original=original, root=d)


def case_c(mode: str, n_lines: int, wait_for) -> None:
    name = "c-" + mode
    d = fresh(name)
    path = d / "worldserver.conf"
    path.write_bytes(conf_text(n_lines))
    orig = path.read_bytes()
    original = read_text(path)
    expected = tuning.save_text(original, make_edit(original)).encode("utf-8")
    marker = d.parent / f"{name}.marker"
    marker.unlink(missing_ok=True)
    proc = subprocess.Popen(
        [sys.executable, str(SELF), "--child", mode, str(d), str(marker)],
        stdout=subprocess.DEVNULL, stderr=subprocess.PIPE,
    )
    deadline = time.time() + 60
    while time.time() < deadline and not wait_for(marker):
        time.sleep(0.02)
    reached = wait_for(marker)
    proc.send_signal(signal.SIGKILL)
    rc = proc.wait()
    stderr_tail = proc.stderr.read().decode(errors="replace")[-200:] if proc.stderr else ""
    after = path.read_bytes()
    temp = path.with_name(path.name + tuning.TEMP_SUFFIX)
    temp_size = temp.stat().st_size if temp.exists() else None
    intact = after == orig
    # A follow-up Save by the next session must succeed and clear the stale temp.
    tuning.write_raw(path, make_edit(original), original=original, root=d)
    follow = path.read_bytes() == expected and not temp.exists()
    record(
        f"c SIGKILL {mode}",
        reached and rc == -signal.SIGKILL and intact and follow,
        f"kill reached={reached} rc={rc} (SIGKILL=-9) orig sha={sha(orig)[:16]} after sha={sha(after)[:16]} "
        f"identical={intact} leftover temp size={temp_size} of full {len(expected)} "
        f"follow-up save ok={follow} child stderr tail={stderr_tail!r}",
    )


def case_d() -> None:
    d = fresh("d")
    path = d / "worldserver.conf"
    path.write_bytes(conf_text(300))
    orig = path.read_bytes()
    original = read_text(path)
    edited = make_edit(original)
    before = listing(d)
    os.chmod(d, 0o500)  # read + search only: no new files, like a read-only folder
    raised = None
    try:
        if os.geteuid() == 0:
            raise RuntimeError("running as root: read-only folder cannot be simulated")
        tuning.write_raw(path, edited, original=original, root=d)
    except Exception as e:  # noqa: BLE001 - we want to see what the caller gets
        raised = e
    finally:
        os.chmod(d, 0o700)
    after = path.read_bytes()
    ok = (
        raised is not None
        and isinstance(raised, OSError)
        and after == orig
        and listing(d) == before
    )
    record(
        "d read-only folder (dir 0500)",
        ok,
        f"raised={type(raised).__name__}: {raised} orig sha={sha(orig)[:16]} after sha={sha(after)[:16]} "
        f"identical={after == orig} listing unchanged={listing(d) == before}",
    )


def main() -> None:
    if len(sys.argv) > 1 and sys.argv[1] == "--child":
        child(sys.argv[2], Path(sys.argv[3]), Path(sys.argv[4]))
        return
    WORK.mkdir(parents=True, exist_ok=True)
    print(f"yulon.tuning from: {tuning.__file__}")
    print(f"uid={os.geteuid()} python={sys.version.split()[0]}")
    case_a()
    case_a2()
    case_b()
    case_b2()
    # Pre-replace: the temp is complete and fsynced, the process hangs before the rename.
    case_c("pre-replace", 300, lambda m: m.exists())
    # Mid-write: ~1.8 MB of conf, killed once the temp holds at least 512 KiB.
    case_c("mid-write", 40000, lambda m: m.exists() and int(m.read_text() or 0) >= 512 * 1024)
    case_d()
    failed = [r for r in RESULTS if r[1] != "PASS"]
    print(f"SUMMARY: {len(RESULTS) - len(failed)}/{len(RESULTS)} PASS")


if __name__ == "__main__":
    main()
