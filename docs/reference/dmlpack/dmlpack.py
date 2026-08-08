#!/usr/bin/env python3
"""dmlpack -- archive a known-working private game server, restore it elsewhere.

    dmlpack.py pack      <game> [--out DIR]     build the archive (never deletes)
    dmlpack.py verify    <pack>                 stream + re-check every sha256
    dmlpack.py restore   <pack> [--dry-run]     pre-flight, extract, regenerate, register
    dmlpack.py shortcuts <pack>                 register just the Steam shortcuts
    dmlpack.py repair    <pack>                 fix an install that restored but won't start
    dmlpack.py list      [DIR]                  inventory archives
    dmlpack.py reclaim   <pack>                 delete originals (hard-gated, see below)
    dmlpack.py deck2-ok  <game>                 record that a restore was proven on Deck #2

A .dmlpack is an uncompressed tar whose first member is manifest.json, so the
manifest reads in milliseconds without touching the ~10 GiB payload.

SELF-CONTAINED BY DESIGN. This file imports nothing outside the stdlib and reads
no sibling scripts -- not evejs-shortcut.py, not dml-launcher-lib.sh. The point of
a pack is that it restores on a *second* Steam Deck that has none of the fleet's
helpers, so everything needed (binary-VDF writer, appid hash, launcher lib) either
lives in here or rides inside the archive.

RECLAIM IS GATED. Standing rule, 2026-08-06: nothing is deleted on this Deck until
a pack has been proven to install on the second Steam Deck. `reclaim` refuses to
run until `deck2-ok <game>` has been recorded, and then still demands a full verify
in the same run plus the game name typed out.
"""

from __future__ import annotations

import argparse
import fnmatch
import glob as globmod
import hashlib
import io
import json
import os
import re
import shutil
import struct
import subprocess
import sys
import tarfile
import time
import zlib
from pathlib import Path

TOOL_VERSION = "1.0.0"
DMLPACK_VERSION = 1
SELF_DIR = Path(__file__).resolve().parent
MANIFEST_DIR = SELF_DIR / "manifests"
STATE_DIR = SELF_DIR / "state"
TOOLS_IMAGE = "dml-pack-tools:local"
TOOLS_DOCKERFILE = SELF_DIR / "tools.Dockerfile"
MANIFEST_MEMBER = "manifest.json"
CHUNK = 4 * 1024 * 1024

# Compressed-size guesses, used only for the free-space precheck.
KIND_FACTOR = {"qcow2": 0.65, "tar_zst": 0.75, "files_tar_zst": 1.0,
               "docker_volume": 0.5, "root_tar_zst": 0.5, "docker_image": 0.45}


# --------------------------------------------------------------------------
# output helpers (same 8 the fleet installers copy-paste; there is no shared lib)
# --------------------------------------------------------------------------

_TTY = sys.stdout.isatty() and os.environ.get("NO_COLOR") is None
def _c(code: str, s: str) -> str:
    return f"\033[{code}m{s}\033[0m" if _TTY else s

def print_header(msg: str) -> None:
    line = "=" * min(len(msg) + 4, 78)
    print("\n" + _c("1;36", line))
    print(_c("1;36", f"  {msg}"))
    print(_c("1;36", line))

def print_step(msg: str) -> None:    print(_c("1;34", "==> ") + msg)
def print_success(msg: str) -> None: print(_c("1;32", "  OK  ") + msg)
def print_warning(msg: str) -> None: print(_c("1;33", "  !!  ") + msg)
def print_error(msg: str) -> None:   print(_c("1;31", " FAIL ") + msg, file=sys.stderr)
def print_info(msg: str) -> None:    print(_c("0;36", "  ..  ") + msg)

def ask_yes_no(prompt: str, default: bool = False) -> bool:
    if not sys.stdin.isatty():
        return default
    suffix = " [Y/n] " if default else " [y/N] "
    ans = input(prompt + suffix).strip().lower()
    return default if not ans else ans.startswith("y")

def press_enter() -> None:
    if sys.stdin.isatty():
        input("\nPress Enter to continue... ")


def human(n: float) -> str:
    for unit in ("B", "KiB", "MiB", "GiB", "TiB"):
        if abs(n) < 1024:
            return f"{n:.1f} {unit}" if unit != "B" else f"{int(n)} B"
        n /= 1024
    return f"{n:.1f} PiB"


def expand(p: str | Path) -> Path:
    return Path(os.path.expanduser(str(p)))


def contract(p: str | Path) -> str:
    s = str(p)
    home = str(Path.home())
    return "~" + s[len(home):] if s.startswith(home) else s


def run(cmd: list[str], check: bool = True, capture: bool = False,
        cwd: str | Path | None = None) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, check=check, cwd=cwd,
                          stdout=subprocess.PIPE if capture else None,
                          stderr=subprocess.PIPE if capture else None,
                          text=True)


def _excluded(rel: str, patterns: list[str]) -> bool:
    """Match the way `tar --exclude` does.

    A path is excluded if it matches a pattern directly, if its basename matches a
    pattern that has no slash, or if ANY ancestor directory matches -- tar drops a
    directory's whole subtree, so `--exclude=server/logs` also drops
    `server/logs/errors.log`. dir_size() never depended on that last rule because it
    prunes directories before descending, but anything using this as a standalone
    "would tar have excluded this?" predicate does.
    """
    parts = rel.split("/")
    prefixes = ["/".join(parts[:i]) for i in range(1, len(parts) + 1)]
    for pat in patterns:
        pat = pat.rstrip("/")
        for prefix in prefixes:
            if fnmatch.fnmatch(prefix, pat):
                return True
        if "/" not in pat and any(fnmatch.fnmatch(p, pat) for p in parts):
            return True
    return False


def dir_size(path: Path, base: Path | None = None, exclude: list[str] | None = None) -> int:
    """Bytes under `path`, honouring the member's tar excludes.

    Without this the estimate double-counts: ~/dekaron-server contains the 15.4 GiB
    storage/ qcow2 that the server-dir member explicitly excludes (it ships as its
    own member), which inflated the free-space precheck by ~11 GiB.
    """
    exclude = exclude or []
    base = base or path.parent
    if path.is_file():
        return path.stat().st_size
    total = 0
    for root, dirs, files in os.walk(path, onerror=lambda e: None):
        rel_root = os.path.relpath(root, base)
        dirs[:] = [d for d in dirs
                   if not _excluded(os.path.normpath(os.path.join(rel_root, d)), exclude)]
        for f in files:
            rel = os.path.normpath(os.path.join(rel_root, f))
            if _excluded(rel, exclude):
                continue
            fp = Path(root) / f
            try:
                if not fp.is_symlink():
                    total += fp.stat().st_size
            except OSError:
                pass
    return total


def member_raw_size(m: dict) -> int:
    """Uncompressed bytes this member will hold -- what restore needs on disk."""
    if m["kind"] == "docker_volume":
        return volume_size(m["volume"])
    if m["kind"] == "docker_image":
        return image_size(m["image"])
    if m["kind"] == "root_tar_zst":
        return root_dir_size(expand(m["base"]), m["sources"])
    if m["kind"] == "tar_zst":
        base = expand(m["base"])
        return sum(dir_size(base / s, base, m.get("exclude", [])) for s in m["sources"])
    return sum(dir_size(p) for p in member_sources(m))


def same_filesystem(a: Path, b: Path) -> bool:
    def dev(p: Path) -> int:
        while not p.exists() and p != p.parent:
            p = p.parent
        return os.stat(p).st_dev
    return dev(a) == dev(b)


class Progress:
    """One line per member, rewritten in place; a newline only when it finishes."""

    def __init__(self, label: str, total: int | None = None, interval: float = 3.0):
        self.label, self.total, self.interval = label, total, interval
        self.done = 0
        self.start = self.last = time.time()

    def add(self, n: int) -> None:
        self.done += n
        now = time.time()
        if now - self.last >= self.interval:
            self.last = now
            self._draw(now)

    def _draw(self, now: float) -> None:
        rate = self.done / max(now - self.start, 0.001)
        msg = f"  ..  {self.label}: {human(self.done)}"
        if self.total:
            msg += f" / ~{human(self.total)} ({100 * self.done / self.total:.0f}%)"
        msg += f"  {human(rate)}/s"
        if _TTY:
            print(msg.ljust(78)[:78], end="\r", flush=True)
        else:
            print(msg, flush=True)

    def finish(self) -> None:
        el = time.time() - self.start
        if _TTY:
            print(" " * 78, end="\r")
        print_success(f"{self.label}: {human(self.done)} in {el / 60:.1f} min "
                      f"({human(self.done / max(el, 0.001))}/s)")


def sha256_file(path: Path, label: str = "") -> tuple[str, int]:
    h = hashlib.sha256()
    size = path.stat().st_size
    prog = Progress(label or f"hashing {path.name}", size) if label else None
    with path.open("rb") as fh:
        while True:
            b = fh.read(CHUNK)
            if not b:
                break
            h.update(b)
            if prog:
                prog.add(len(b))
    if prog:
        prog.finish()
    return h.hexdigest(), size


# --------------------------------------------------------------------------
# binary VDF -- Steam's shortcuts.vdf
# Types: 0x00 nested map, 0x01 string, 0x02 int32 LE, 0x08 end-of-map.
# A valid document ends 08 08: one END closes "shortcuts", one closes the root.
# A file missing that second byte makes Steam drop EVERY non-Steam shortcut.
# --------------------------------------------------------------------------

MAP, STR, INT, END = 0x00, 0x01, 0x02, 0x08


def _read_cstr(buf: bytes, i: int) -> tuple[str, int]:
    j = buf.index(b"\x00", i)
    return buf[i:j].decode("utf-8", "replace"), j + 1


def vdf_parse(buf: bytes, i: int = 0) -> tuple[dict, int]:
    out: dict = {}
    while i < len(buf):
        t = buf[i]
        i += 1
        if t == END:
            return out, i
        key, i = _read_cstr(buf, i)
        if t == MAP:
            out[key], i = vdf_parse(buf, i)
        elif t == STR:
            out[key], i = _read_cstr(buf, i)
        elif t == INT:
            out[key] = struct.unpack_from("<i", buf, i)[0]
            i += 4
        else:
            raise ValueError(f"unknown VDF type 0x{t:02x} at offset {i - 1}")
    return out, i


def vdf_serialize(d: dict) -> bytes:
    parts = bytearray()
    for key, val in d.items():
        kb = key.encode("utf-8") + b"\x00"
        if isinstance(val, dict):
            parts += bytes([MAP]) + kb + vdf_serialize(val) + bytes([END])
        elif isinstance(val, int):
            parts += bytes([INT]) + kb + struct.pack("<i", val)
        else:
            parts += bytes([STR]) + kb + str(val).encode("utf-8") + b"\x00"
    return bytes(parts)


def vdf_dump(root: dict) -> bytes:
    return vdf_serialize(root) + bytes([END])


def gen_appid(quoted_exe: str, appname: str) -> int:
    """Steam's non-Steam appid: crc32(exe + appname) with the high bit set.

    Restoring a shortcut under the same name+exe regenerates the identical
    appid, so the grid artwork filenames line up for free. Never rename.
    """
    return (zlib.crc32((quoted_exe + appname).encode("utf-8")) | 0x80000000) & 0xFFFFFFFF


def to_signed(v: int) -> int:
    return v - 0x100000000 if v >= 0x80000000 else v


def quoted(s: str) -> str:
    return s if s.startswith('"') else f'"{s}"'


# --------------------------------------------------------------------------
# manifests / packs
# --------------------------------------------------------------------------

def load_spec(game: str) -> dict:
    p = Path(game) if game.endswith(".json") else MANIFEST_DIR / f"{game}.json"
    if not p.exists():
        avail = ", ".join(sorted(x.stem for x in MANIFEST_DIR.glob("*.json"))) or "(none)"
        raise SystemExit(f"no manifest for {game!r} at {p}\navailable: {avail}")
    return json.loads(p.read_text())


def read_pack_manifest(pack: Path) -> dict:
    """Read manifest.json without unpacking the payload -- it is the first member."""
    with tarfile.open(pack, "r|") as tf:
        for m in tf:
            if m.name == MANIFEST_MEMBER:
                fh = tf.extractfile(m)
                assert fh is not None
                return json.loads(fh.read())
            raise SystemExit(f"{pack.name}: first member is {m.name!r}, expected manifest.json")
    raise SystemExit(f"{pack.name}: no manifest.json")


def default_archive_dir() -> Path:
    """Archives live on the SD card. /home has no room to stage a copy."""
    for cand in sorted(Path("/run/media/deck").glob("*")):
        if cand.is_dir() and os.access(cand, os.W_OK):
            return cand / "dmlpack"
    raise SystemExit("no writable SD mount found under /run/media/deck -- pass --out")


def ensure_tools_image() -> None:
    r = run(["docker", "image", "inspect", TOOLS_IMAGE], check=False, capture=True)
    if r.returncode == 0:
        return
    print_step(f"building {TOOLS_IMAGE} (qemu-img is not installed on the host)")
    run(["docker", "build", "-f", str(TOOLS_DOCKERFILE), "-t", TOOLS_IMAGE, str(SELF_DIR)])
    print_success(f"{TOOLS_IMAGE} built")


def docker_uid_args() -> list[str]:
    # Without this, qemu-img's output lands root-owned and the later tar fails.
    return ["--user", f"{os.getuid()}:{os.getgid()}"]


# --------------------------------------------------------------------------
# member builders
# --------------------------------------------------------------------------

def member_sources(m: dict) -> list[Path]:
    kind = m["kind"]
    if kind == "qcow2":
        return [expand(m["source"])]
    if kind in ("tar_zst", "root_tar_zst"):
        return [expand(m["base"]) / s for s in m["sources"]]
    if kind in ("docker_volume", "docker_image"):
        return []
    if kind == "files_tar_zst":
        out = []
        for f in m["files"]:
            if "glob" in f:
                out += [Path(x) for x in sorted(globmod.glob(str(expand_steam(f["glob"]))))]
            else:
                out.append(expand(f["src"]))
        return out
    raise SystemExit(f"unknown member kind {kind!r}")


def build_qcow2(m: dict, out: Path) -> None:
    src = expand(m["source"])
    part = out.with_suffix(out.suffix + ".part")
    ensure_tools_image()
    print_info(f"qemu-img convert -c  {contract(src)}  ({human(src.stat().st_size)} raw)")
    print_info("this drops unallocated clusters and zstd-compresses; expect 20-35 min")
    t0 = time.time()
    run(["docker", "run", "--rm", *docker_uid_args(),
         "-v", f"{src.parent}:/in:ro", "-v", f"{part.parent}:/out",
         TOOLS_IMAGE,
         "qemu-img", "convert", "-p", "-O", "qcow2", "-c",
         "-o", "compression_type=zstd",
         f"/in/{src.name}", f"/out/{part.name}"])
    part.rename(out)
    print_success(f"{m['path']}: {human(out.stat().st_size)} in {(time.time() - t0) / 60:.1f} min")


def _stream_to(cmd_stdout, part: Path, label: str, est: int | None) -> None:
    """Drain a pipe into .part, hashing nothing here -- sha256 happens on the final file."""
    prog = Progress(label, est)
    with part.open("wb") as fh:
        while True:
            b = cmd_stdout.read(CHUNK)
            if not b:
                break
            fh.write(b)
            prog.add(len(b))
    prog.finish()


def build_tar_zst(m: dict, out: Path) -> None:
    """tar | zstd as two processes.

    Deliberately not `tar -I 'zstd -T0 -12'`: -I's argument handling varies with
    tar version, and an explicit pipe lets us show progress on the compressed side.
    """
    base = expand(m["base"])
    part = out.with_suffix(out.suffix + ".part")
    est = member_raw_size(m)
    tar_cmd = ["tar", "-cf", "-", "-C", str(base)]
    for ex in m.get("exclude", []):
        tar_cmd.append(f"--exclude={ex}")
    tar_cmd += m["sources"]
    print_info(f"tar -C {contract(base)} {' '.join(m['sources'])}  ({human(est)} raw)")
    p1 = subprocess.Popen(tar_cmd, stdout=subprocess.PIPE)
    lvl = m.get("zstd_level", 12)
    p2 = subprocess.Popen(["zstd", "-T0", f"-{lvl}", "-c", "-q"],
                          stdin=p1.stdout, stdout=subprocess.PIPE)
    assert p1.stdout is not None and p2.stdout is not None
    p1.stdout.close()
    _stream_to(p2.stdout, part, m["path"], int(est * 0.75))
    rc2, rc1 = p2.wait(), p1.wait()
    # GNU tar returns 1 for "file changed as we read it" -- fatal for an archive
    # that is supposed to be a byte-exact restore, so treat it as an error.
    if rc1 != 0 or rc2 != 0:
        part.unlink(missing_ok=True)
        raise SystemExit(f"{m['path']}: tar rc={rc1} zstd rc={rc2}")
    part.rename(out)


def volume_size(name: str) -> int:
    r = run(["docker", "run", "--rm", "-v", f"{name}:/v:ro", "alpine",
             "du", "-sb", "/v"], check=False, capture=True)
    try:
        return int(r.stdout.split()[0])
    except (ValueError, IndexError):
        return 0


def image_size(tag: str) -> int:
    r = run(["docker", "image", "inspect", "--format", "{{.Size}}", tag],
            check=False, capture=True)
    try:
        return int(r.stdout.strip())
    except (ValueError, AttributeError):
        return 0


def image_id(tag: str) -> str:
    """Content digest, not the tag -- a retag must not read as 'unchanged'."""
    r = run(["docker", "image", "inspect", "--format", "{{.Id}}", tag],
            check=False, capture=True)
    return r.stdout.strip() if r.returncode == 0 else ""


def root_dir_size(base: Path, sources: list[str]) -> int:
    """Size a tree the `deck` user cannot fully read, from inside a container.

    dir_size() walks as the invoking user, so on a 0750 root-owned tree it
    silently returns only the readable part -- Talisman's MySQL datadir would
    have measured ~180 MB of top-level files with every database missing.
    """
    r = run(["docker", "run", "--rm", "-v", f"{base}:/v:ro", "alpine",
             "du", "-sb", *[f"/v/{s}" for s in sources]], check=False, capture=True)
    total = 0
    for line in r.stdout.splitlines():
        try:
            total += int(line.split()[0])
        except (ValueError, IndexError):
            pass
    return total


def build_docker_volume(m: dict, out: Path) -> None:
    """Archive a Docker named volume through a throwaway container.

    The volume must be at rest -- pack refuses if any container in
    recipe.docker.containers is running, which covers the database.
    """
    part = out.with_suffix(out.suffix + ".part")
    vol = m["volume"]
    est = volume_size(vol)
    print_info(f"docker volume {vol} ({human(est)} raw)")
    p1 = subprocess.Popen(
        ["docker", "run", "--rm", "-v", f"{vol}:/v:ro", "alpine", "tar", "-C", "/v", "-cf", "-", "."],
        stdout=subprocess.PIPE)
    p2 = subprocess.Popen(["zstd", f"-T0", f"-{m.get('zstd_level', 12)}", "-c", "-q"],
                          stdin=p1.stdout, stdout=subprocess.PIPE)
    assert p1.stdout is not None and p2.stdout is not None
    p1.stdout.close()
    _stream_to(p2.stdout, part, m["path"], int(est * 0.5))
    if p2.wait() != 0 or p1.wait() != 0:
        part.unlink(missing_ok=True)
        raise SystemExit(f"{m['path']}: volume export failed")
    part.rename(out)


def build_root_tar_zst(m: dict, out: Path) -> None:
    """Archive a host directory whose contents `deck` cannot read, via a root container.

    Talisman's MySQL datadir is a bind mount owned by uid 999 with 0750
    subdirectories, one per database. A host-side `tar` run as deck cannot open
    them: GNU tar prints "Permission denied" per directory and exits 2, and had
    it exited 0 the archive would have been a complete-looking backup with every
    database missing. Tarring from inside a root container reads everything.

    --numeric-owner is what makes the restore work: the datadir must come back
    owned by uid 999, and alpine's /etc/passwd has no such user, so symbolic
    ownership would be remapped and MySQL would refuse to start on it.

    Tars the source by name from its parent, so the directory's own ownership
    and mode are carried in the archive too -- no chown step on restore.
    """
    base = expand(m["base"])
    part = out.with_suffix(out.suffix + ".part")
    est = root_dir_size(base, m["sources"])
    print_info(f"root tar {contract(base)}: {', '.join(m['sources'])} ({human(est)} raw)")
    tar_cmd = ["tar", "-C", "/v", "--numeric-owner", "-cf", "-"]
    for ex in m.get("exclude", []):
        tar_cmd.append(f"--exclude={ex}")
    tar_cmd += m["sources"]
    p1 = subprocess.Popen(
        ["docker", "run", "--rm", "-v", f"{base}:/v:ro", "alpine", *tar_cmd],
        stdout=subprocess.PIPE)
    p2 = subprocess.Popen(["zstd", "-T0", f"-{m.get('zstd_level', 12)}", "-c", "-q"],
                          stdin=p1.stdout, stdout=subprocess.PIPE)
    assert p1.stdout is not None and p2.stdout is not None
    p1.stdout.close()
    _stream_to(p2.stdout, part, m["path"], int(est * 0.5))
    rc2, rc1 = p2.wait(), p1.wait()
    if rc1 != 0 or rc2 != 0:
        part.unlink(missing_ok=True)
        raise SystemExit(f"{m['path']}: root tar rc={rc1} zstd rc={rc2}")
    part.rename(out)


def restore_root_tar_zst(m: dict, blob: Path) -> None:
    """Unpack a root-owned tree back into place, preserving numeric ownership."""
    base = expand(m["restore_to"])
    base.mkdir(parents=True, exist_ok=True)
    p1 = subprocess.Popen(["zstd", "-d", "-c", "-q", str(blob)], stdout=subprocess.PIPE)
    p2 = subprocess.Popen(["docker", "run", "--rm", "-i", "-v", f"{base}:/v", "alpine",
                           "tar", "-C", "/v", "--numeric-owner", "-xf", "-"],
                          stdin=p1.stdout)
    assert p1.stdout is not None
    p1.stdout.close()
    if p2.wait() != 0 or p1.wait() != 0:
        raise SystemExit(f"{m['path']}: root tar import failed")
    print_success(f"{contract(base)}: {', '.join(m['sources'])} (ownership preserved)")


def restore_docker_volume(m: dict, blob: Path, manifest: dict | None = None) -> None:
    """Recreate a named volume and stream the archived contents into it.

    The volume is created with compose's own labels. Without them compose prints
    `volume "x" already exists but was not created by Docker Compose` on every
    start -- harmless, but it looks like a fault and invites someone to "fix" it
    by deleting the volume, which would throw away the database.
    """
    vol = m["volume"]
    cmd = ["docker", "volume", "create"]
    project = m.get("compose_project")
    if project is None and manifest:
        d = manifest.get("recipe", {}).get("docker", {})
        project = expand(d.get("compose_dir", manifest.get("server_dir", "~"))).name
    short = m.get("compose_volume") or (
        vol[len(project) + 1:] if project and vol.startswith(project + "_") else None)
    if project and short:
        cmd += ["--label", f"com.docker.compose.project={project}",
                "--label", f"com.docker.compose.volume={short}"]
    run(cmd + [vol], capture=True)
    p1 = subprocess.Popen(["zstd", "-d", "-c", "-q", str(blob)], stdout=subprocess.PIPE)
    p2 = subprocess.Popen(["docker", "run", "--rm", "-i", "-v", f"{vol}:/v", "alpine",
                           "tar", "-C", "/v", "-xf", "-"], stdin=p1.stdout)
    assert p1.stdout is not None
    p1.stdout.close()
    if p2.wait() != 0 or p1.wait() != 0:
        raise SystemExit(f"{m['path']}: volume import failed")
    print_success(f"docker volume {vol} restored")


def build_docker_image(m: dict, out: Path) -> None:
    """Archive a built image with `docker save`, for servers compiled inside the image.

    Only worth doing when the Dockerfile actually COMPILES something. Turtle WoW
    V1 ships its mangosd as a file on the host and its Dockerfile is apt-only, so
    a rebuild there costs minutes and this kind would just waste space. V2's
    Dockerfile is a real two-stage compile whose binaries exist nowhere but inside
    the image (CMAKE_INSTALL_PREFIX=/opt/turtle is baked in), so rebuilding on a
    fresh Deck is a multi-hour job. That is the case this kind exists for.

    `docker save` writes layers UNCOMPRESSED, so zstd earns its keep here.
    """
    tag = m["image"]
    part = out.with_suffix(out.suffix + ".part")
    est = image_size(tag)
    if not est:
        raise SystemExit(f"{m['path']}: image {tag!r} not present locally -- nothing to save")
    print_info(f"docker save {tag} ({human(est)} raw)")
    p1 = subprocess.Popen(["docker", "save", tag], stdout=subprocess.PIPE)
    p2 = subprocess.Popen(["zstd", "-T0", f"-{m.get('zstd_level', 12)}", "-c", "-q"],
                          stdin=p1.stdout, stdout=subprocess.PIPE)
    assert p1.stdout is not None and p2.stdout is not None
    p1.stdout.close()
    _stream_to(p2.stdout, part, m["path"], int(est * KIND_FACTOR["docker_image"]))
    if p2.wait() != 0 or p1.wait() != 0:
        part.unlink(missing_ok=True)
        raise SystemExit(f"{m['path']}: image save failed")
    part.rename(out)


def restore_docker_image(m: dict, blob: Path) -> None:
    """Load the saved image back, then prove the tag really exists.

    `docker load` exits 0 while printing "Loaded image ID: sha256:..." when the
    stream carries no repo tag, which would leave compose trying to PULL a
    local-only tag. Checking the tag afterwards is the difference between a
    restore that works and one that fails hours later at first boot.
    """
    tag = m["image"]
    p1 = subprocess.Popen(["zstd", "-d", "-c", "-q", str(blob)], stdout=subprocess.PIPE)
    p2 = subprocess.Popen(["docker", "load"], stdin=p1.stdout)
    assert p1.stdout is not None
    p1.stdout.close()
    if p2.wait() != 0 or p1.wait() != 0:
        raise SystemExit(f"{m['path']}: docker load failed")
    if not image_id(tag):
        raise SystemExit(f"{m['path']}: docker load finished but tag {tag!r} is still missing "
                         "-- the archive carried an untagged image")
    print_success(f"docker image {tag} loaded ({human(image_size(tag))})")


def build_files_tar_zst(m: dict, out: Path, stage: Path) -> None:
    """Small scattered files (scripts, icon, grid art, handoff doc) into one member.

    They are copied into a scratch tree first so the archive paths are stable and
    restore can place each one independently.
    """
    part = out.with_suffix(out.suffix + ".part")
    scratch = stage / f".scratch-{Path(m['path']).name}"
    if scratch.exists():
        shutil.rmtree(scratch)
    n = 0
    for f in m["files"]:
        if "glob" in f:
            hits = sorted(globmod.glob(str(expand_steam(f["glob"]))))
            if not hits:
                print_warning(f"glob matched nothing: {f['glob']}")
            for hit in hits:
                dst = scratch / f["arc_dir"] / Path(hit).name
                dst.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(hit, dst)
                n += 1
        else:
            src = expand(f["src"])
            if not src.exists():
                raise SystemExit(f"extras source missing: {contract(src)}")
            dst = scratch / f["arc"]
            dst.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(src, dst)
            n += 1
    p1 = subprocess.Popen(["tar", "-cf", "-", "-C", str(scratch), "."], stdout=subprocess.PIPE)
    p2 = subprocess.Popen(["zstd", "-T0", "-19", "-c", "-q"],
                          stdin=p1.stdout, stdout=subprocess.PIPE)
    assert p1.stdout is not None and p2.stdout is not None
    p1.stdout.close()
    _stream_to(p2.stdout, part, m["path"], None)
    if p2.wait() != 0 or p1.wait() != 0:
        part.unlink(missing_ok=True)
        raise SystemExit(f"{m['path']}: extras tar failed")
    part.rename(out)
    shutil.rmtree(scratch)
    print_info(f"{n} file(s) in extras")


# --------------------------------------------------------------------------
# pack
# --------------------------------------------------------------------------

def preflight_pack(spec: dict, outdir: Path) -> int:
    print_step("pre-flight")
    problems = []

    if same_filesystem(outdir, Path.home()):
        problems.append(f"destination {contract(outdir)} is on the same filesystem as /home. "
                        "Archives must go to the SD card -- /home has no room to stage a copy.")

    est = 0
    for m in spec["members"]:
        raw = member_raw_size(m)
        est += int(raw * KIND_FACTOR[m["kind"]])
    # staging + the assembled tar both exist at once, plus headroom.
    need = int(est * 2 * 1.15)
    free = shutil.disk_usage(outdir if outdir.exists() else outdir.parent).free
    print_info(f"estimated archive ~{human(est)}; peak need {human(need)} (staging + tar); "
               f"free {human(free)}")
    if free < need:
        problems.append(f"not enough space at {contract(outdir)}: need ~{human(need)}, have {human(free)}")

    # A live guest means the qcow2 is being written under us.
    for c in spec.get("recipe", {}).get("docker", {}).get("containers", []):
        r = run(["docker", "ps", "--filter", f"name=^{c}$", "--format", "{{.Names}}"],
                check=False, capture=True)
        if r.returncode == 0 and r.stdout.strip():
            problems.append(f"container {c!r} is RUNNING -- stop it first "
                            f"(ACPI shutdown, never a hard kill: SQL Server MDFs)")

    # A docker_image member has no path on disk, so the missing-source loop below
    # cannot see it; an absent image would otherwise only surface mid-archive.
    for m in spec["members"]:
        if m["kind"] == "docker_image" and not image_id(m["image"]):
            problems.append(f"image {m['image']!r} is not present locally -- "
                            "build it before packing")

    for m in spec["members"]:
        for p in member_sources(m):
            if not p.exists():
                problems.append(f"source missing: {contract(p)}")

    for p in problems:
        print_error(p)
    if not problems:
        print_success("pre-flight clean")
    return len(problems)


def cmd_pack(args) -> int:
    spec = load_spec(args.game)
    game = spec["game"]
    outdir = expand(args.out) if args.out else default_archive_dir()
    outdir.mkdir(parents=True, exist_ok=True)

    print_header(f"dmlpack pack -- {spec['display_name']}")
    if preflight_pack(spec, outdir):
        return 2

    stage = outdir / f".stage-{game}"
    stage.mkdir(parents=True, exist_ok=True)
    pack_path = outdir / f"{game}-{args.version}.dmlpack"
    if pack_path.exists() and not args.force:
        print_error(f"{pack_path.name} already exists (use --force to rebuild)")
        return 2

    members_out = []
    for m in spec["members"]:
        dest = stage / m["path"]
        dest.parent.mkdir(parents=True, exist_ok=True)
        meta_path = dest.with_suffix(dest.suffix + ".meta")

        if dest.exists() and meta_path.exists():
            meta = json.loads(meta_path.read_text())
            print_success(f"{m['path']}: already built, skipping ({human(meta['bytes'])})")
        else:
            print_step(f"building {m['path']}")
            dest.unlink(missing_ok=True)
            if m["kind"] == "qcow2":
                build_qcow2(m, dest)
            elif m["kind"] == "tar_zst":
                build_tar_zst(m, dest)
            elif m["kind"] == "root_tar_zst":
                build_root_tar_zst(m, dest)
            elif m["kind"] == "docker_volume":
                build_docker_volume(m, dest)
            elif m["kind"] == "docker_image":
                build_docker_image(m, dest)
            else:
                build_files_tar_zst(m, dest, stage)
            digest, size = sha256_file(dest, f"sha256 {m['path']}")
            meta = {"sha256": digest, "bytes": size,
                    "restore_bytes": member_raw_size(m),
                    "source_mtimes": {contract(p): int(p.stat().st_mtime)
                                      for p in member_sources(m)[:64]}}
            # A docker_image has no source path, so source_mtimes is empty and
            # --check-live would silently pass it. Pin the content digest instead.
            if m["kind"] == "docker_image":
                meta["source_image_id"] = image_id(m["image"])
            meta_path.write_text(json.dumps(meta, indent=2))

        entry = dict(m)
        entry.update(sha256=meta["sha256"], bytes=meta["bytes"],
                     restore_bytes=meta["restore_bytes"],
                     source_mtimes=meta["source_mtimes"])
        if meta.get("source_image_id"):
            entry["source_image_id"] = meta["source_image_id"]
        members_out.append(entry)

    print_step("capturing live host state into the manifest")
    snapshot_live_shortcuts(spec.get("recipe", {}))
    if not spec.get("recipe", {}).get("host_ports"):
        ports = parse_compose_ports_from_manifest({"recipe": spec.get("recipe", {})})
        if ports:
            spec.setdefault("recipe", {})["host_ports"] = ports
            print_success(f"host ports from docker-compose.yml: {', '.join(map(str, ports))}")

    manifest = {k: v for k, v in spec.items() if k != "members"}
    manifest.update(
        dmlpack_version=DMLPACK_VERSION,
        tool_version=TOOL_VERSION,
        # Recorded so reclaim can tell WHICH version of this game was proven on
        # Deck #2. Without it the deck2-ok flag was per-game and a 2.0 pack
        # inherited 1.0's proof.
        pack_version=args.version,
        packed_at=time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        packed_from=os.uname().nodename,
        members=members_out,
    )

    print_step(f"assembling {pack_path.name}")
    part = pack_path.with_suffix(".dmlpack.part")
    total = sum(m["bytes"] for m in members_out)
    prog = Progress("tar assembly", total)
    with tarfile.open(part, "w", format=tarfile.PAX_FORMAT) as tf:
        blob = json.dumps(manifest, indent=2).encode()
        ti = tarfile.TarInfo(MANIFEST_MEMBER)
        ti.size, ti.mtime, ti.mode = len(blob), int(time.time()), 0o644
        tf.addfile(ti, io.BytesIO(blob))
        for m in members_out:
            src = stage / m["path"]
            ti = tf.gettarinfo(str(src), arcname=m["path"])
            ti.mode = 0o644
            with src.open("rb") as fh:
                # addfile() reads straight through; wrap to keep the progress line moving.
                tf.addfile(ti, _Counting(fh, prog))
    prog.finish()
    part.rename(pack_path)

    # A second copy of the manifest on /home: the SD holds both the archive and
    # (for some games) the only copy of the upstream pack. If the card dies, at
    # least the recipe survives.
    STATE_DIR.mkdir(parents=True, exist_ok=True)
    (STATE_DIR / f"{game}-{args.version}.manifest.json").write_text(json.dumps(manifest, indent=2))

    if not args.keep_stage:
        shutil.rmtree(stage)
        print_info("staging removed")

    print_success(f"{pack_path}  ({human(pack_path.stat().st_size)})")
    print_info(f"manifest copy: {contract(STATE_DIR / f'{game}-{args.version}.manifest.json')}")
    print_info(f"next: {sys.argv[0]} verify {pack_path}")
    return 0


class _Counting(io.RawIOBase):
    """Pass-through reader that ticks a Progress as tarfile pulls bytes."""

    def __init__(self, fh, prog: Progress):
        self.fh, self.prog = fh, prog

    def read(self, n: int = -1) -> bytes:
        b = self.fh.read(n)
        self.prog.add(len(b))
        return b

    def readable(self) -> bool:
        return True


# --------------------------------------------------------------------------
# verify
# --------------------------------------------------------------------------

def cmd_verify(args) -> int:
    pack = expand(args.pack)
    print_header(f"dmlpack verify -- {pack.name}")
    if not pack.exists():
        print_error(f"no such pack: {pack}")
        return 2

    manifest = None
    seen: dict[str, tuple[str, int]] = {}
    with tarfile.open(pack, "r|") as tf:
        for m in tf:
            fh = tf.extractfile(m)
            if fh is None:
                continue
            if m.name == MANIFEST_MEMBER:
                manifest = json.loads(fh.read())
                print_info(f"{manifest['display_name']} packed {manifest['packed_at']} "
                           f"on {manifest['packed_from']} (tool {manifest.get('tool_version', '?')})")
                continue
            if manifest is None:
                print_error("manifest.json is not the first member")
                return 3
            h = hashlib.sha256()
            n = 0
            prog = Progress(f"verify {m.name}", m.size)
            while True:
                b = fh.read(CHUNK)
                if not b:
                    break
                h.update(b)
                n += len(b)
                prog.add(len(b))
            if _TTY:
                print(" " * 78, end="\r")
            seen[m.name] = (h.hexdigest(), n)

    if manifest is None:
        print_error("no manifest.json in archive")
        return 3

    bad = 0
    for m in manifest["members"]:
        got = seen.get(m["path"])
        if got is None:
            print_error(f"{m['path']}: MISSING from archive")
            bad += 1
        elif got[0] != m["sha256"]:
            print_error(f"{m['path']}: sha256 MISMATCH")
            print_info(f"     expected {m['sha256']}")
            print_info(f"     got      {got[0]}")
            bad += 1
        elif got[1] != m["bytes"]:
            print_error(f"{m['path']}: size mismatch {got[1]} != {m['bytes']}")
            bad += 1
        else:
            print_success(f"{m['path']}: sha256 OK ({human(m['bytes'])})")

    extra = set(seen) - {m["path"] for m in manifest["members"]}
    for e in sorted(extra):
        print_warning(f"{e}: in archive but not in manifest")

    if args.check_live:
        check_live_drift(manifest)

    if bad:
        print_error(f"{bad} member(s) FAILED")
        return 3
    print_success("all members verified")
    return 0


def check_live_drift(manifest: dict) -> None:
    """Warn when the live install has moved on since the archive was built."""
    print_step("comparing against the live install")
    drift = 0
    for m in manifest["members"]:
        if m.get("source_image_id"):
            live = image_id(m["image"])
            if not live:
                print_warning(f"image {m['image']}: no longer present on this machine")
                drift += 1
            elif live != m["source_image_id"]:
                print_warning(f"image {m['image']}: rebuilt since packing "
                              f"({live[7:19]} != {m['source_image_id'][7:19]})")
                drift += 1
        for path, mtime in (m.get("source_mtimes") or {}).items():
            p = expand(path)
            if not p.exists():
                print_warning(f"{path}: no longer present on this machine")
                drift += 1
            elif int(p.stat().st_mtime) != mtime:
                print_warning(f"{path}: modified since packing "
                              f"({time.strftime('%Y-%m-%d %H:%M', time.localtime(p.stat().st_mtime))})")
                drift += 1
    if drift:
        print_warning(f"{drift} path(s) drifted -- the live install no longer matches this archive; re-pack")
    else:
        print_success("live install matches the archive")


# --------------------------------------------------------------------------
# restore
# --------------------------------------------------------------------------

def port_holders(ports: list[int]) -> dict[int, str]:
    """Which of these ports are already bound, and by what.

    This is the 2026-08-05 failure caught in advance: an orphaned portbridge.py
    from a parked session still holding 3306 while its container was long gone.
    """
    r = run(["ss", "-ltnp"], check=False, capture=True)
    held: dict[int, str] = {}
    if r.returncode != 0:
        return held
    for line in r.stdout.splitlines()[1:]:
        mm = re.search(r":(\d+)\s", line)
        if not mm:
            continue
        port = int(mm.group(1))
        if port in ports:
            who = re.search(r'users:\(\("([^"]+)",pid=(\d+)', line)
            held[port] = f'{who.group(1)} (pid {who.group(2)})' if who else "unknown process"
    return held


def preflight_restore(manifest: dict, pack: Path, tmpdir: Path) -> int:
    print_step("pre-flight")
    problems, warnings = [], []
    recipe = manifest.get("recipe", {})

    # Space, grouped by FILESYSTEM (st_dev) rather than by destination path.
    # Dekaron restores 15.4 GiB + 5.6 GiB + 4.8 MiB to three different paths that
    # are all on /home; checking them separately would let each pass while their
    # sum does not fit.
    need: dict[int, tuple[Path, int]] = {}
    for m in manifest["members"]:
        dest = expand(m.get("restore_to", "~"))
        while not dest.exists() and dest != dest.parent:
            dest = dest.parent
        dev = os.stat(dest).st_dev
        prev_path, prev_bytes = need.get(dev, (dest, 0))
        need[dev] = (prev_path, prev_bytes + m.get("restore_bytes", m["bytes"]))
    for dev, (root, bytes_needed) in need.items():
        free = shutil.disk_usage(root).free
        ok = free > bytes_needed * 1.1
        (print_success if ok else print_error)(
            f"space on the filesystem holding {contract(root)}: "
            f"need ~{human(bytes_needed)} total, free {human(free)}")
        if not ok:
            problems.append(f"insufficient space on the filesystem holding {contract(root)}")

    free_tmp = shutil.disk_usage(tmpdir if tmpdir.exists() else tmpdir.parent).free
    biggest = max(m["bytes"] for m in manifest["members"])
    if free_tmp < biggest * 1.2:
        problems.append(f"scratch space at {contract(tmpdir)}: need ~{human(biggest)}, free {human(free_tmp)}")
    else:
        print_success(f"scratch space at {contract(tmpdir)}: {human(free_tmp)} free")

    # ports
    ports = recipe.get("host_ports") or parse_compose_ports_from_manifest(manifest)
    if ports:
        held = port_holders(ports)
        if held:
            for p, who in sorted(held.items()):
                print_error(f"port {p} already bound by {who}")
                problems.append(f"port {p} in use")
        else:
            print_success(f"all {len(ports)} host port(s) free: {', '.join(map(str, ports))}")

    # docker
    if run(["docker", "info"], check=False, capture=True).returncode != 0:
        problems.append("docker is not running")
    else:
        print_success("docker is running")

    # runtime deps (Proton)
    for dep in recipe.get("runtime_deps", []):
        p = expand(dep["path"])
        if p.exists():
            print_success(f"runtime dep present: {contract(p)}")
        else:
            problems.append(f"missing runtime dep {contract(p)} -- install it via Steam first")

    # steam must be closed before shortcuts.vdf is touched
    if recipe.get("steam", {}).get("shortcuts"):
        if steam_is_running():
            problems.append("Steam is RUNNING -- close it fully or it silently overwrites "
                            "shortcuts.vdf on exit, discarding the restored entries")
        else:
            print_success("Steam is closed")

    # existing targets
    for m in manifest["members"]:
        dest = expand(m["restore_to"]) if "restore_to" in m else None
        if dest and dest.exists() and m["kind"] == "qcow2":
            warnings.append(f"{contract(dest)} already exists and will be OVERWRITTEN")
        if dest and m["kind"] == "root_tar_zst":
            # restore_to is the PARENT here, so the existing-data check has to look
            # at the sources inside it -- this is usually a live database.
            for s in m.get("sources", []):
                if (dest / s).exists():
                    warnings.append(f"{contract(dest / s)} already exists and will be "
                                    "OVERWRITTEN (this is server data, not config)")
        if dest and m["kind"] == "tar_zst" and m.get("if_absent_only"):
            for s in m.get("sources", []):
                if (dest / s).exists():
                    warnings.append(f"{contract(dest / s)} already present -- will be KEPT, "
                                    "not replaced (shared runtime)")
    for w in warnings:
        print_warning(w)

    for p in problems:
        print_error(p)
    if not problems:
        print_success("pre-flight clean")
    return len(problems)


def parse_compose_ports_from_manifest(manifest: dict) -> list[int]:
    """host_ports is parsed from docker-compose.yml, never hand-typed."""
    d = manifest.get("recipe", {}).get("docker", {})
    compose = expand(d.get("compose_dir", "~")) / d.get("compose_file", "docker-compose.yml")
    if not compose.exists():
        return []
    ports = []
    for line in compose.read_text().splitlines():
        line = line.strip()
        if not line.startswith("-"):
            continue
        mm = re.match(r'-\s*"?(?:(\d+\.\d+\.\d+\.\d+):)?(\d+):(\d+)"?', line)
        if mm:
            ports.append(int(mm.group(2)))
    return sorted(set(ports))


def extract_member(pack: Path, name: str, dest: Path) -> None:
    with tarfile.open(pack, "r|") as tf:
        for m in tf:
            if m.name != name:
                continue
            fh = tf.extractfile(m)
            assert fh is not None
            prog = Progress(f"extract {name}", m.size)
            with dest.open("wb") as out:
                while True:
                    b = fh.read(CHUNK)
                    if not b:
                        break
                    out.write(b)
                    prog.add(len(b))
            prog.finish()
            return
    raise SystemExit(f"member {name} not found in {pack}")


def cmd_restore(args) -> int:
    pack = expand(args.pack)
    if not pack.exists():
        print_error(f"no such pack: {pack}")
        return 2
    manifest = read_pack_manifest(pack)
    recipe = manifest.get("recipe", {})
    tmpdir = expand(args.tmp) if args.tmp else pack.parent / ".dmlpack-tmp"

    print_header(f"dmlpack restore -- {manifest['display_name']}")
    print_info(f"packed {manifest['packed_at']} on {manifest['packed_from']}")

    tmpdir.mkdir(parents=True, exist_ok=True)
    if preflight_restore(manifest, pack, tmpdir):
        print_error("pre-flight failed; nothing written")
        return 2

    if args.dry_run:
        print_step("dry run -- stopping here, nothing written")
        print_info("members that would be restored:")
        for m in manifest["members"]:
            print_info(f"  {m['path']:<34} -> {m.get('restore_to', '(per-file)')} "
                       f"({human(m.get('restore_bytes', m['bytes']))})")
        for sc in recipe.get("steam", {}).get("shortcuts", []):
            appid = gen_appid(quoted(sc["exe"]), sc["name"])
            print_info(f"  shortcut {sc['name']!r} -> appid {appid}")
        return 0

    # 1. payload
    for m in manifest["members"]:
        print_step(f"restoring {m['path']}")
        scratch = tmpdir / Path(m["path"]).name
        extract_member(pack, m["path"], scratch)

        if m["kind"] == "qcow2":
            dest = expand(m["restore_to"])
            dest.parent.mkdir(parents=True, exist_ok=True)
            ensure_tools_image()
            # Back to UNCOMPRESSED: compressed qcow2 clusters decompress on every
            # write, which is wrong under a live SQL Server guest.
            print_info("qemu-img convert back to uncompressed")
            run(["docker", "run", "--rm", *docker_uid_args(),
                 "-v", f"{scratch.parent}:/in:ro", "-v", f"{dest.parent}:/out",
                 TOOLS_IMAGE,
                 "qemu-img", "convert", "-p", "-O", "qcow2",
                 f"/in/{scratch.name}", f"/out/{dest.name}"])
            print_success(f"{contract(dest)} ({human(dest.stat().st_size)})")

        elif m["kind"] == "docker_volume":
            restore_docker_volume(m, scratch, manifest)

        elif m["kind"] == "docker_image":
            restore_docker_image(m, scratch)

        elif m["kind"] == "root_tar_zst":
            restore_root_tar_zst(m, scratch)

        elif m["kind"] == "tar_zst":
            dest = expand(m["restore_to"])
            # Shared runtimes (GE-Proton is used by more than one game's client)
            # must never be downgraded to the copy frozen into this archive.
            if m.get("if_absent_only"):
                present = [s for s in m.get("sources", []) if (dest / s).exists()]
                if present:
                    print_warning(f"{', '.join(present)} already in {contract(dest)} "
                                  "-- kept (shared, if_absent_only)")
                    scratch.unlink(missing_ok=True)
                    continue
            dest.mkdir(parents=True, exist_ok=True)
            p1 = subprocess.Popen(["zstd", "-d", "-c", "-q", str(scratch)], stdout=subprocess.PIPE)
            p2 = subprocess.Popen(["tar", "-xf", "-", "-C", str(dest)], stdin=p1.stdout)
            assert p1.stdout is not None
            p1.stdout.close()
            if p2.wait() != 0 or p1.wait() != 0:
                raise SystemExit(f"{m['path']}: extract failed")
            print_success(f"{contract(dest)}")

        else:  # files_tar_zst
            unpacked = tmpdir / "extras"
            if unpacked.exists():
                shutil.rmtree(unpacked)
            unpacked.mkdir(parents=True)
            p1 = subprocess.Popen(["zstd", "-d", "-c", "-q", str(scratch)], stdout=subprocess.PIPE)
            p2 = subprocess.Popen(["tar", "-xf", "-", "-C", str(unpacked)], stdin=p1.stdout)
            assert p1.stdout is not None
            p1.stdout.close()
            if p2.wait() != 0 or p1.wait() != 0:
                raise SystemExit(f"{m['path']}: extract failed")
            place_extras(m, unpacked)

        scratch.unlink(missing_ok=True)

    # 2. docker image + named volumes
    restore_docker(manifest, tmpdir)

    # 3. client config edits (only if they didn't survive)
    apply_client_edits(recipe)

    # 4. steam shortcuts
    shortcuts_ok = True
    if recipe.get("steam", {}).get("shortcuts"):
        shortcuts_ok = register_shortcuts(recipe["steam"])

    # 5. host state that lives outside $HOME
    report_host_state(recipe)

    shutil.rmtree(tmpdir, ignore_errors=True)

    print_header("restore complete")
    for acc in manifest.get("accounts", []):
        print_info(f"account: {acc['user']} / {acc['pass']}  -- {acc.get('note', '')}")
    notes = manifest.get("notes_file")
    if notes:
        for m in manifest["members"]:
            for f in m.get("files", []):
                if f.get("arc") == notes:
                    print_info(f"handoff doc: {f['restore_to']}")
    prefix = recipe.get("client", {}).get("prefix", {})
    if prefix.get("regenerate"):
        print_info(f"Proton prefix {prefix['path']} was NOT archived -- "
                   "the client script recreates it on first launch (expect a slow first start)")
    guest = recipe.get("guest", {})
    if guest.get("boot_seconds"):
        print_info(f"guest needs ~{guest['boot_seconds']}s to boot; the launcher gates on "
                   f"port {guest.get('health_port')}")
    if not shortcuts_ok:
        print_warning("Steam shortcuts were NOT registered -- everything else restored.")
        print_warning(f"Close Steam, then: {sys.argv[0]} shortcuts {args.pack}")
        return 1
    print_info("Start Steam, then launch the server shortcut. The real test is character select.")
    return 0


def place_extras(m: dict, unpacked: Path) -> None:
    done_dirs = set()
    for f in m["files"]:
        if "glob" in f:
            key = (f["arc_dir"], f["restore_dir"])
            if key in done_dirs:
                continue
            done_dirs.add(key)
            src_dir = unpacked / f["arc_dir"]
            dst_dir = expand_steam(f["restore_dir"])
            dst_dir.mkdir(parents=True, exist_ok=True)
            n = 0
            for item in sorted(src_dir.glob("*")):
                shutil.copy2(item, dst_dir / item.name)
                n += 1
            print_success(f"{n} file(s) -> {contract(dst_dir)}")
        else:
            src = unpacked / f["arc"]
            dst = expand(f["restore_to"])
            if f.get("if_absent_only") and dst.exists():
                # dml-launcher-lib.sh is shared by 25 launchers. Never clobber a
                # newer copy with the one frozen into this archive.
                print_warning(f"{contract(dst)} exists -- kept (shared file, if_absent_only)")
                continue
            dst.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(src, dst)
            if f.get("mode"):
                os.chmod(dst, int(f["mode"], 8))
            print_success(contract(dst))


def compose_services(compose: Path) -> list[dict]:
    """Minimal compose parser: service name, its image tag, whether it has a build.

    Stdlib only -- there is no yaml module on SteamOS. These compose files are
    simple and machine-written; this reads indentation rather than pretending to
    be a real YAML parser.
    """
    if not compose.exists():
        return []
    out: list[dict] = []
    in_services = False
    svc_indent = None
    cur: dict | None = None
    for raw in compose.read_text().splitlines():
        if not raw.strip() or raw.lstrip().startswith("#"):
            continue
        indent = len(raw) - len(raw.lstrip())
        key = raw.strip().rstrip(":")
        if indent == 0:
            in_services = raw.startswith("services:")
            if cur:
                out.append(cur)
                cur = None
            svc_indent = None
            continue
        if not in_services:
            continue
        if svc_indent is None:
            svc_indent = indent
        if indent == svc_indent and raw.rstrip().endswith(":"):
            if cur:
                out.append(cur)
            cur = {"name": key, "image": None, "build": False}
            continue
        if cur is None:
            continue
        stripped = raw.strip()
        if stripped.startswith("image:"):
            cur["image"] = stripped.split(":", 1)[1].strip().strip('"\'')
        elif stripped.startswith("build:"):
            cur["build"] = True
    if cur:
        out.append(cur)
    return out


def ensure_compose_images(manifest: dict) -> None:
    """Make sure every image the compose file names actually exists locally.

    The trap this exists for (hit on Deck #2, 2026-08-06): a service can name a
    locally-built tag like `dml/tortoise-wow:local` while having NO build section,
    because the image was built by hand with `docker build -t ... .` long ago.
    `docker compose build` then builds nothing at all and `up` tries to PULL the
    tag from a registry where it has never existed:

        pull access denied for dml/tortoise-wow, repository does not exist

    So: for any referenced image that is missing locally, build it from the
    Dockerfile that shipped in the pack.
    """
    d = manifest.get("recipe", {}).get("docker", {})
    compose_dir = expand(d.get("compose_dir", manifest.get("server_dir", "~")))
    compose = compose_dir / d.get("compose_file", "docker-compose.yml")

    wanted: list[tuple[str, str]] = []  # (tag, why)
    for spec in d.get("build_images", []):
        wanted.append((spec["tag"], "manifest build_images"))
    for svc in compose_services(compose):
        if svc["image"] and not svc["build"]:
            wanted.append((svc["image"], f"service {svc['name']}"))

    seen: set[str] = set()
    for tag, why in wanted:
        if tag in seen:
            continue
        seen.add(tag)
        if run(["docker", "image", "inspect", tag], check=False, capture=True).returncode == 0:
            print_success(f"image present: {tag}")
            continue
        # Only build tags that are demonstrably OURS. A missing `mariadb:10.6` is
        # pullable and must never be "rebuilt" from the game's Dockerfile -- that
        # would tag an unrelated image as MariaDB and the DB would start empty.
        spec = next((s for s in d.get("build_images", []) if s["tag"] == tag), None)
        is_ours = bool(spec) or tag == d.get("image") or tag.endswith(":local")
        if not is_ours:
            print_info(f"image {tag} not local; it is a registry image, compose will pull it ({why})")
            continue
        context = expand(spec["context"]) if spec else compose_dir
        dockerfile = context / (spec.get("dockerfile", "Dockerfile") if spec else "Dockerfile")
        if not dockerfile.exists():
            print_error(f"{tag} is a local-only image but no Dockerfile shipped to rebuild it")
            print_info(f"expected {contract(dockerfile)} -- the server cannot start without this image")
            continue
        print_step(f"building missing image {tag} from {contract(dockerfile)}")
        print_info("compose would otherwise try to PULL this tag and fail")
        r = run(["docker", "build", "-t", tag, "-f", str(dockerfile), str(context)], check=False)
        if r.returncode != 0 or run(["docker", "image", "inspect", tag],
                                    check=False, capture=True).returncode != 0:
            print_error(f"could not build {tag} -- the server will not start without it")
        else:
            print_success(f"built {tag}")


def restore_docker(manifest: dict, tmpdir: Path) -> None:
    d = manifest.get("recipe", {}).get("docker", {})
    if not d:
        return
    compose_dir = expand(d.get("compose_dir", manifest.get("server_dir", "~")))
    if d.get("build"):
        print_step(f"docker compose build in {contract(compose_dir)}")
        run(["docker", "compose", "build"], cwd=compose_dir)
        # NB: this is a no-op for services that name a prebuilt tag with no
        # build section -- ensure_compose_images() is what actually guarantees
        # the images exist.
    ensure_compose_images(manifest)

    for vol in d.get("named_volumes", []):
        name = vol if isinstance(vol, str) else vol["name"]
        member = None if isinstance(vol, str) else vol.get("member")
        print_step(f"restoring named volume {name}")
        run(["docker", "volume", "create", name], capture=True)
        if member:
            blob = tmpdir / Path(member).name
            run(["docker", "run", "--rm", "-v", f"{name}:/v",
                 "-v", f"{blob.parent}:/in:ro", "alpine",
                 "tar", "-C", "/v", "-xf", f"/in/{blob.name}"])
        print_success(f"volume {name} ready")


def apply_client_edits(recipe: dict) -> None:
    client = recipe.get("client", {})
    edits = client.get("edits") or []
    if not edits:
        return
    root = expand(client["root"])
    print_step("checking client config edits")
    for e in edits:
        p = root / e["file"]
        if not p.exists():
            print_warning(f"missing client file {e['file']}")
            continue
        try:
            text = p.read_text(errors="surrogateescape")
        except OSError as exc:
            print_warning(f"{e['file']}: {exc}")
            continue
        if e["to"] in text and e["from"] not in text:
            print_success(f"{e['file']}: already points at {e['to']}")
        elif e["from"] in text:
            p.write_text(text.replace(e["from"], e["to"]), errors="surrogateescape")
            print_success(f"{e['file']}: {e['from']} -> {e['to']}")
        else:
            print_warning(f"{e['file']}: neither {e['from']!r} nor {e['to']!r} found -- check by hand")


def steam_is_running() -> bool:
    return run(["pgrep", "-x", "steam"], check=False, capture=True).returncode == 0


# Set by --steam-user; otherwise the account is detected per-machine.
_STEAM_USER_OVERRIDE: str | None = None


def steam_roots() -> list[Path]:
    return [
        Path.home() / ".local/share/Steam/userdata",
        Path.home() / ".steam/steam/userdata",
        Path.home() / ".var/app/com.valvesoftware.Steam/data/Steam/userdata",  # flatpak
    ]


def steam_config_dir(explicit: str | None = None) -> Path | None:
    """THIS machine's Steam userdata config dir, or None if Steam isn't installed.

    The userdata id is a Steam *account* id, not a machine id, so it differs on
    someone else's computer. Nothing in a pack actually depends on it -- appids are
    crc32(exe+appname) -- so it is detected at run time rather than baked in.

    Picks the account that has a shortcuts.vdf, then the most recently used.
    """
    want = explicit or _STEAM_USER_OVERRIDE
    cands: list[tuple[bool, float, Path]] = []
    for root in steam_roots():
        if not root.is_dir():
            continue
        for d in root.iterdir():
            if not d.is_dir() or not d.name.isdigit() or d.name == "0":
                continue
            if want and d.name != str(want):
                continue
            cfg = d / "config"
            if not cfg.is_dir():
                continue
            mtimes = [f.stat().st_mtime for f in (cfg / "localconfig.vdf", cfg / "shortcuts.vdf")
                      if f.exists()]
            cands.append(((cfg / "shortcuts.vdf").exists(), max(mtimes, default=0.0), cfg))
    if not cands:
        return None
    cands.sort(key=lambda t: (t[0], t[1]), reverse=True)
    return cands[0][2]


def resolve_steam(p: str, cfg: Path | None = None) -> str:
    """Point a Steam path at this machine's account.

    Handles both the `{steam_config}` token (what manifests should use) and a
    literal `.../userdata/<id>/config/...` baked in by an older pack.
    """
    cfg = cfg if cfg is not None else steam_config_dir()
    if cfg is None:
        return p
    p = p.replace("{steam_config}", str(cfg))
    return re.sub(r"userdata/\d+/config", f"userdata/{cfg.parent.name}/config", p)


def expand_steam(p: str | Path) -> Path:
    return expand(resolve_steam(str(p)))


def steam_paths(userdata_id: str | None = None) -> tuple[Path, Path]:
    cfg = steam_config_dir()
    if cfg is None and userdata_id:  # no Steam here; fall back to the packed layout
        cfg = Path.home() / ".local/share/Steam/userdata" / str(userdata_id) / "config"
    if cfg is None:
        cfg = Path.home() / ".local/share/Steam/userdata/unknown/config"
    return cfg / "shortcuts.vdf", cfg / "grid"


def snapshot_live_shortcuts(recipe: dict) -> None:
    """Capture each shortcut's ACTUAL vdf entry into the manifest at pack time.

    Same principle as parsing host_ports out of docker-compose.yml: don't hand-type
    what is already on disk. Hand-written entries drift from reality -- the live
    "Dekaron Server" stores StartDir unquoted ("/usr/bin/") while the other two are
    quoted, and a constructed entry would silently "restore" a different shortcut
    than the one that is known to work.
    """
    steam = recipe.get("steam")
    if not steam or not steam.get("shortcuts"):
        return
    vdf_path, _ = steam_paths(steam.get("userdata_id"))
    if not vdf_path.exists():
        print_warning(f"{contract(vdf_path)} not found -- shortcuts will be reconstructed on restore")
        return
    root, _ = vdf_parse(vdf_path.read_bytes())
    live = {}
    for entry in root.get("shortcuts", {}).values():
        nm = entry.get("AppName") or entry.get("appname")
        if nm:
            live[nm] = entry
    for sc in steam["shortcuts"]:
        entry = live.get(sc["name"])
        if entry is None:
            print_warning(f"shortcut {sc['name']!r} is not in the live shortcuts.vdf "
                          "-- it will be reconstructed from the manifest on restore")
            continue
        sc["vdf"] = dict(entry)
        # The grid-art filenames only line up if the appid is the crc32 of the
        # stored Exe+AppName. Flag it loudly if this entry was made some other way.
        expect = to_signed(gen_appid(entry.get("Exe", ""), sc["name"]))
        if entry.get("appid") != expect:
            print_warning(f"{sc['name']}: stored appid {entry.get('appid')} != crc32-derived "
                          f"{expect}; grid art may not follow on restore")
        print_success(f"captured live shortcut {sc['name']!r} (appid {entry.get('appid')})")


def register_shortcuts(steam: dict) -> bool:
    """Add this game's shortcuts. Returns False if nothing could be written safely."""
    print_step(f"registering {len(steam['shortcuts'])} Steam shortcut(s)")

    cfg = steam_config_dir()
    if cfg is None:
        print_warning("no Steam installation found on this machine -- skipping shortcuts")
        print_info("the server and client are fully restored; launch them with the scripts")
        print_info("in $HOME directly, or add the shortcuts later with `dmlpack.py shortcuts`")
        return True
    print_info(f"Steam account detected: {cfg.parent.name}"
               + (" (from --steam-user)" if _STEAM_USER_OVERRIDE else ""))
    vdf_path, _grid = steam_paths(steam.get("userdata_id"))

    # Re-check here, not just in pre-flight. Pre-flight ran up to ~11 minutes ago
    # and nothing stops Steam from being launched during the extraction; writing
    # underneath a running Steam loses the entries silently when it exits.
    if steam_is_running():
        print_error("Steam is RUNNING -- refusing to write shortcuts.vdf")
        print_info("Writing now would be silently undone when Steam exits.")
        print_info("Close Steam fully (check the tray), then register them with:")
        print_info(f"    {sys.argv[0]} shortcuts <pack>")
        return False
    if not vdf_path.exists():
        print_warning(f"{contract(vdf_path)} does not exist -- creating a new one")
        root: dict = {"shortcuts": {}}
    else:
        root = vdf_parse(vdf_path.read_bytes())[0]
    shortcuts = root.setdefault("shortcuts", {})
    before = len(shortcuts)

    added = 0
    for sc in steam["shortcuts"]:
        qexe, qdir = quoted(sc["exe"]), quoted(sc.get("startdir") or str(Path(sc["exe"]).parent))
        if any((e.get("AppName") or e.get("appname")) == sc["name"] for e in shortcuts.values()):
            print_info(f"{sc['name']!r} already present -- left alone (add-only)")
            continue
        if sc.get("vdf"):
            # Verbatim replay of the entry that was live when this was packed.
            entry = dict(sc["vdf"])
            appid = gen_appid(entry.get("Exe", qexe), sc["name"])
            source = "captured"
        else:
            appid = gen_appid(qexe, sc["name"])
            entry = {
                "appid": to_signed(appid),
                "AppName": sc["name"],
                "Exe": qexe,
                "StartDir": qdir,
                "icon": sc.get("icon", ""),
                "ShortcutPath": "",
                "LaunchOptions": sc.get("launchopts", ""),
                "IsHidden": 0,
                "AllowDesktopConfig": 1,
                "AllowOverlay": 1,
                "OpenVR": 0,
                "Devkit": 0,
                "DevkitGameID": "",
                "DevkitOverrideAppID": 0,
                "LastPlayTime": 0,
                "FlatpakAppID": "",
                "tags": {},
            }
            source = "reconstructed"
        shortcuts[str(max((int(k) for k in shortcuts), default=-1) + 1)] = entry
        added += 1
        print_success(f"{sc['name']}  appid {appid} ({source}; grid art: {appid}*.png)")

    if not added:
        print_info("no new shortcuts to write")
        return True

    if vdf_path.exists():
        backup = vdf_path.with_name(f"shortcuts.vdf.backup-{time.strftime('%Y%m%d-%H%M%S')}")
        shutil.copy2(vdf_path, backup)
        print_info(f"backup: {contract(backup)}")

    payload = vdf_dump(root)
    reparsed, consumed = vdf_parse(payload)
    if len(reparsed.get("shortcuts", {})) != before + added:
        print_error("round-trip verification failed; refusing to write shortcuts.vdf")
        return False
    if not payload.endswith(b"\x08\x08") or consumed != len(payload):
        print_error("bad document terminator; refusing to write shortcuts.vdf")
        return False
    vdf_path.parent.mkdir(parents=True, exist_ok=True)
    vdf_path.write_bytes(payload)
    print_success(f"{contract(vdf_path)} ({len(payload)} bytes, {before + added} shortcuts)")
    return True


def cmd_shortcuts(args) -> int:
    """Register just the Steam shortcuts from a pack.

    The recovery path for a restore that finished while Steam happened to be
    running -- re-extracting 7 GiB to redo a 15 KB file would be absurd. Also
    useful on its own if shortcuts.vdf gets clobbered later.
    """
    pack = expand(args.pack)
    if not pack.exists():
        print_error(f"no such pack: {pack}")
        return 2
    manifest = read_pack_manifest(pack)
    steam = manifest.get("recipe", {}).get("steam")
    print_header(f"dmlpack shortcuts -- {manifest['display_name']}")
    if not steam or not steam.get("shortcuts"):
        print_info("this pack defines no Steam shortcuts")
        return 0
    ok = register_shortcuts(steam)
    if ok:
        print_info("restart Steam to see them")
    return 0 if ok else 2


def cmd_repair(args) -> int:
    """Re-run the cheap post-extraction steps against an already-restored install.

    For when the payload landed fine but something after it did not: a missing
    Docker image, a volume compose will not adopt, client edits that got reverted.
    Re-running `restore` would re-extract gigabytes to fix a 15 KB problem.
    """
    pack = expand(args.pack)
    if not pack.exists():
        print_error(f"no such pack: {pack}")
        return 2
    manifest = read_pack_manifest(pack)
    recipe = manifest.get("recipe", {})
    print_header(f"dmlpack repair -- {manifest['display_name']}")
    print_info("checks the parts of a restore that live outside the archived payload")

    if run(["docker", "info"], check=False, capture=True).returncode != 0:
        print_error("docker is not running")
        return 2

    server_dir = expand(recipe.get("docker", {}).get("compose_dir",
                                                     manifest.get("server_dir", "~")))
    if not server_dir.exists():
        print_error(f"{contract(server_dir)} does not exist -- run a full restore first")
        return 2
    print_success(f"install found at {contract(server_dir)}")

    ensure_compose_images(manifest)
    apply_client_edits(recipe)
    report_host_state(recipe)
    print_header("repair complete")
    print_info("start the server shortcut again")
    return 0


def report_host_state(recipe: dict) -> None:
    """Things outside $HOME that need root -- printed, never applied silently."""
    hs = recipe.get("host_state", {})
    manual = []
    for entry in hs.get("etc_hosts", []):
        manual.append(f'/etc/hosts needs:  {entry}')
    for unit in hs.get("systemd_user_units", []):
        name = unit if isinstance(unit, str) else unit["name"]
        manual.append(f"systemctl --user enable --now {name}")
    # systemd_SYSTEM_units was declared in manifests and read NOWHERE -- the same
    # class of bug as reclaim's docker_volumes. Both Turtle packs declare
    # tortoise-radio-hosts.service here, so the one command that makes the radio's
    # /etc/hosts pins survive a SteamOS update was never printed to anyone.
    for unit in hs.get("systemd_system_units", []):
        if isinstance(unit, str):
            manual.append(f"sudo systemctl enable --now {unit}")
        else:
            manual.append(unit.get("install") or f"sudo systemctl enable --now {unit['name']}")
            if unit.get("note"):
                manual.append(f"    ^ {unit['note']}")
    # Anything else a restore cannot do for itself. Free-form because the cases do
    # not generalise: a second compose stack, a manual first-run, a device pairing.
    for step in hs.get("manual_steps", []):
        manual.append(step if isinstance(step, str) else step["cmd"])
        if isinstance(step, dict) and step.get("note"):
            manual.append(f"    ^ {step['note']}")
    for d in hs.get("sudoers_dropins", []):
        name = d if isinstance(d, str) else d["name"]
        manual.append(f"/etc/sudoers.d/{name} (must sort after SteamOS's wheel rule: "
                      "name it zz-*, validate with visudo -c)")
    for hp in hs.get("helper_processes", []):
        manual.append(f"helper process: {hp.get('start', hp)}")
    if manual:
        print_step("host state -- do these by hand (needs root or a login session)")
        for m in manual:
            print_warning(m)


# --------------------------------------------------------------------------
# list / reclaim / deck2-ok
# --------------------------------------------------------------------------

def cmd_list(args) -> int:
    d = expand(args.dir) if args.dir else default_archive_dir()
    packs = sorted(d.glob("*.dmlpack"))
    print_header(f"dmlpack archives in {contract(d)}")
    if not packs:
        print_info("none")
        return 0
    total = 0
    for p in packs:
        size = p.stat().st_size
        total += size
        try:
            man = read_pack_manifest(p)
            # deck2_flag became version-keyed on 2026-08-07 and this call site was
            # missed, so `list` reported EVERY pack as "unreadable manifest" -- the
            # TypeError was caught by the except below and mislabelled. Resolve the
            # flag exactly the way reclaim does, legacy fallback included.
            version = pack_version_of(man, p)
            flag = deck2_flag(man["game"], version)
            if not flag.exists() and version == "1.0":
                flag = legacy_deck2_flag(man["game"])
            verified = "yes" if flag.exists() else "NO"
            print(f"  {p.name:<34} {human(size):>10}  {man['display_name']:<16} "
                  f"packed {man['packed_at'][:10]}  deck2-verified: {verified}")
        except Exception as exc:
            print_warning(f"  {p.name:<34} {human(size):>10}  unreadable manifest: {exc}")
    print_info(f"{len(packs)} archive(s), {human(total)} total")
    return 0


def deck2_flag(game: str, version: str) -> Path:
    """Per (game, version). NOT per game.

    Keyed on the game alone, a 2.0 pack silently inherited 1.0's Deck #2 proof:
    pack 1.0, prove it, alter the server, pack 2.0, and reclaim would delete the
    live install on the strength of a proof earned by different bytes. The in-run
    checksum verify cannot catch that -- it proves the archive is intact, never
    that it was ever tested.
    """
    return STATE_DIR / f"{game}-{version}.deck2-verified"


def legacy_deck2_flag(game: str) -> Path:
    """The pre-2026-08-07 unversioned flag, honoured for 1.0 packs only."""
    return STATE_DIR / f"{game}.deck2-verified"


def pack_version_of(manifest: dict, pack: Path | None = None) -> str:
    """Which version of a game this pack is.

    Recorded in the manifest since 2026-08-07. Packs built before that have no
    such key, so fall back to the filename (`<game>-<version>.dmlpack`) and only
    then to "1.0" -- every pack that predates the field is a 1.0.
    """
    v = manifest.get("pack_version")
    if v:
        return str(v)
    if pack is not None:
        stem = pack.name[:-len(".dmlpack")] if pack.name.endswith(".dmlpack") else pack.stem
        game = manifest.get("game", "")
        if game and stem.startswith(game + "-"):
            return stem[len(game) + 1:]
    return "1.0"


def cmd_deck2_ok(args) -> int:
    """Record that a restore of this game was proven on the second Steam Deck.

    This is the gate on reclaim. Only run it after a pack has actually been
    restored on Deck #2 AND reached character select there.
    """
    # Prefer being handed the PACK: then the game and the version cannot drift
    # apart. A bare game name is still accepted for convenience, but it has to
    # assume a version, which is exactly how the inherited-proof bug arose.
    target = Path(os.path.expanduser(args.target))
    if target.exists() and target.name.endswith(".dmlpack"):
        manifest = read_pack_manifest(target)
        game = manifest["game"]
        version = pack_version_of(manifest, target)
        print_info(f"from pack: {target.name}")
    else:
        game = args.target
        version = args.pack_version
        print_warning(f"no pack given -- assuming version {version} of {game!r}")
        print_info("pass the .dmlpack path instead to take the version from the archive")

    print_header(f"record Deck #2 verification -- {game} {version}")
    print_info("Only confirm this if, on the SECOND Steam Deck, you have:")
    print_info("  1. restored THIS pack (this exact version)")
    print_info("  2. launched the server and seen it reach its readiness gate")
    print_info("  3. logged in and reached CHARACTER SELECT")
    if not sys.stdin.isatty():
        print_error("refusing to record non-interactively")
        return 2
    typed = input(f"\nType the game name ({game}) to confirm all three: ").strip()
    if typed != game:
        print_error("did not match; nothing recorded")
        return 2
    STATE_DIR.mkdir(parents=True, exist_ok=True)
    flag = deck2_flag(game, version)
    flag.write_text(
        f"verified on second Steam Deck at {time.strftime('%Y-%m-%dT%H:%M:%S%z')}\n"
        f"game: {game}  version: {version}\n"
        f"note: {args.note or '(none)'}\n")
    print_success(f"recorded: {contract(flag)}")
    # Since 2026-08-08 this records history, it does not unlock anything -- saying
    # "reclaim is now permitted" would imply reclaim had been waiting on it.
    print_info("recorded for the record only -- reclaim has not been gated on this since 2026-08-08")
    print_info(f"`{sys.argv[0]} list` shows this flag per pack")
    return 0


def cmd_reclaim(args) -> int:
    pack = expand(args.pack)
    if not pack.exists():
        print_error(f"no such pack: {pack}")
        return 2
    manifest = read_pack_manifest(pack)
    game = manifest["game"]
    print_header(f"dmlpack reclaim -- {manifest['display_name']}")

    # The Deck #2 proof gate was REMOVED on 2026-08-08 at Joshua's instruction.
    #
    # It existed (2026-08-06) because a pack was assumed to be the only other copy
    # of a stack that cost days to build, so nothing could be deleted until that
    # copy was proven to reconstitute elsewhere. That premise is false: every
    # .dmlpack is backed up to several hard drives and his personal online storage.
    # The gate was guarding against a single-copy risk that does not exist, and it
    # made reclaim -- the whole point of packing -- unusable without ceremony.
    #
    # `deck2-ok` still exists as a command and still records flags: the information
    # is worth having, it just no longer authorizes anything. `cmd_list` reports it.
    version = pack_version_of(manifest, pack)
    flag = deck2_flag(game, version)
    if not flag.exists() and version == "1.0":
        flag = legacy_deck2_flag(game)
    if flag.exists():
        print_info(f"Deck #2 verification on record ({game} {version}): "
                   f"{flag.read_text().splitlines()[0]}")
    else:
        print_info(f"no Deck #2 record for {game} {version} -- informational only, not a gate")

    # The remaining archive check. Default-on because it is the one thing that
    # catches a truncated or corrupt pack BEFORE the live copy is gone; skippable
    # because on a bulk reclaim it is minutes of SD reads per game and the packs
    # are backed up elsewhere anyway.
    if getattr(args, "skip_verify", False):
        print_warning("skipping the archive verify at your request (--skip-verify)")
        print_info("nothing will confirm this pack is intact before the live copy is deleted")
    else:
        print_step("full verify (not cached; --skip-verify to bypass)")
        rc = cmd_verify(argparse.Namespace(pack=str(pack), check_live=False))
        if rc != 0:
            print_error("verify failed -- refusing to delete anything")
            return 3

    targets = [expand(p) for p in manifest.get("reclaim", {}).get("paths", [])]
    never = {expand(p) for p in manifest.get("reclaim", {}).get("never_touch", [])}
    print_step("these paths will be DELETED")
    total = 0
    live = []
    for t in targets:
        if t in never:
            print_warning(f"skipping protected path {contract(t)}")
            continue
        if not t.exists():
            print_info(f"{contract(t)} -- already gone")
            continue
        size = dir_size(t)
        total += size
        live.append(t)
        print(f"    {contract(t):<44} {human(size):>10}")
    for img in manifest.get("reclaim", {}).get("docker_images", []):
        print(f"    docker image {img}")

    # Named volumes were declared in manifests but never acted on -- the key was
    # read nowhere in this file. That left EVE's 2.1 GB evejs-xeve-data behind on
    # reclaim AND, worse, left a stale volume that a later restore would extract
    # INTO rather than replace, silently merging two generations of a database.
    vols = list(manifest.get("reclaim", {}).get("docker_volumes", []))
    live_vols = []
    for v in vols:
        r = run(["docker", "volume", "inspect", v], check=False, capture=True)
        if r.returncode != 0:
            print_info(f"docker volume {v} -- already gone")
            continue
        vsize = volume_size(v)
        total += vsize
        live_vols.append(v)
        print(f"    docker volume {v:<30} {human(vsize):>10}")

    if not live and not live_vols:
        print_info("nothing to delete")
        return 0
    print_info(f"total to reclaim: {human(total)}")

    # A path that a RUNNING container has mounted must not be deleted underneath
    # it. reclaim removes the containers named in recipe.docker.containers, but
    # anything else holding a bind mount is invisible to that -- Turtle WoW's
    # reclaim list includes ~/tortoise-radio while the tortoise-radio container
    # (deliberately not in containers[]) bind-mounts ~/tortoise-radio/music and
    # runs continuously.
    holders: dict[str, list[str]] = {}
    r = run(["docker", "ps", "-q"], check=False, capture=True)
    for cid in r.stdout.split():
        info = run(["docker", "inspect", cid, "--format",
                    "{{.Name}}\t{{range .Mounts}}{{.Source}}\n{{end}}"],
                   check=False, capture=True).stdout
        if not info.strip():
            continue
        name, _, sources = info.partition("\t")
        name = name.strip().lstrip("/")
        for src in sources.split("\n"):
            src = src.strip()
            if not src:
                continue
            sp = Path(src)
            for t in live:
                # equal, or the mount lives underneath a directory we would delete
                if sp == t or t in sp.parents:
                    holders.setdefault(contract(t), []).append(f"{name} ({src})")
    if holders:
        print_error("REFUSING: a RUNNING container has these paths mounted")
        for path, who in holders.items():
            print_info(f"  {path}")
            for w in sorted(set(who)):
                print_info(f"      held by {w}")
        print_info("Stop those containers first, or remove the path from reclaim.paths.")
        print_info("Deleting a live container's bind mount corrupts it silently.")
        return 2

    for hp in manifest.get("recipe", {}).get("host_state", {}).get("helper_processes", []):
        pat = hp.get("pgrep") if isinstance(hp, dict) else hp
        r = run(["pgrep", "-f", pat], check=False, capture=True)
        if r.returncode == 0:
            print_warning(f"helper process still running ({pat}): pids {r.stdout.split()}")
            print_info("stop it before reclaiming -- an orphan holding a port is invisible state")
            return 2

    for c in manifest.get("recipe", {}).get("docker", {}).get("containers", []):
        r = run(["docker", "ps", "-a", "--filter", f"name=^{c}$", "--format", "{{.Names}}"],
                check=False, capture=True)
        if r.stdout.strip():
            print_info(f"removing container {c}")
            if not args.dry_run:
                run(["docker", "rm", "-f", c], check=False, capture=True)

    if args.dry_run:
        print_step("dry run -- nothing deleted")
        return 0

    if not sys.stdin.isatty():
        print_error("refusing to delete non-interactively")
        return 2
    typed = input(f"\nType the game name ({game}) to delete the paths above: ").strip()
    if typed != game:
        print_error("did not match; nothing deleted")
        return 2

    for t in live:
        print_info(f"removing {contract(t)}")
        if t.is_dir():
            shutil.rmtree(t)
        else:
            t.unlink()
    for img in manifest.get("reclaim", {}).get("docker_images", []):
        run(["docker", "image", "rm", img], check=False, capture=True)
    for v in live_vols:
        print_info(f"removing docker volume {v}")
        rc = run(["docker", "volume", "rm", v], check=False, capture=True)
        if rc.returncode != 0:
            # Docker refuses while any container -- even a stopped one -- still
            # references it. Say so instead of reporting a clean reclaim.
            print_warning(f"could not remove volume {v}: {rc.stderr.strip() or 'in use'}")
            print_info("a stopped container may still reference it: docker rm <container>")

    print_success(f"reclaimed {human(total)}")
    run(["df", "-h", str(Path.home())], check=False)
    return 0


# --------------------------------------------------------------------------

def main() -> int:
    ap = argparse.ArgumentParser(prog="dmlpack.py", description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)

    p = sub.add_parser("pack", help="build an archive from the live install")
    p.add_argument("game")
    p.add_argument("--out", help="destination dir (default: SD card)")
    p.add_argument("--version", default="1.0")
    p.add_argument("--force", action="store_true", help="rebuild even if the pack exists")
    p.add_argument("--keep-stage", action="store_true", help="keep staging dir for debugging")
    p.set_defaults(func=cmd_pack)

    p = sub.add_parser("verify", help="stream the archive and re-check every sha256")
    p.add_argument("pack")
    p.add_argument("--check-live", action="store_true",
                   help="also warn if the live install has drifted since packing")
    p.set_defaults(func=cmd_verify)

    p = sub.add_parser("restore", help="pre-flight, extract, regenerate, register")
    p.add_argument("pack")
    p.add_argument("--dry-run", action="store_true", help="pre-flight report only, writes nothing")
    p.add_argument("--tmp", help="scratch dir (default: alongside the pack)")
    p.add_argument("--steam-user", help="Steam userdata id to register shortcuts under "
                                        "(default: auto-detect this machine's account)")
    p.set_defaults(func=cmd_restore)

    p = sub.add_parser("shortcuts", help="register just the Steam shortcuts from a pack")
    p.add_argument("pack")
    p.add_argument("--steam-user", help="Steam userdata id (default: auto-detect)")
    p.set_defaults(func=cmd_shortcuts)

    p = sub.add_parser("repair", help="re-run post-extraction steps on an existing install")
    p.add_argument("pack")
    p.set_defaults(func=cmd_repair)

    p = sub.add_parser("list", help="inventory archives")
    p.add_argument("dir", nargs="?")
    p.set_defaults(func=cmd_list)

    p = sub.add_parser("reclaim", help="delete the originals (verify + typed name)")
    p.add_argument("pack")
    p.add_argument("--dry-run", action="store_true")
    p.add_argument("--skip-verify", action="store_true",
                   help="skip the full archive verify before deleting (bulk reclaims)")
    p.set_defaults(func=cmd_reclaim)

    p = sub.add_parser("deck2-ok", help="record that a restore was proven on the second Deck "
                                        "(informational since 2026-08-08; no longer a gate)")
    p.add_argument("target", help="path to the .dmlpack (preferred -- carries the version), "
                                  "or a bare game name")
    p.add_argument("--pack-version", default="1.0",
                   help="only used when a bare game name is given (default 1.0)")
    p.add_argument("--note")
    p.set_defaults(func=cmd_deck2_ok)

    args = ap.parse_args()
    global _STEAM_USER_OVERRIDE
    _STEAM_USER_OVERRIDE = getattr(args, "steam_user", None)
    try:
        return args.func(args)
    except KeyboardInterrupt:
        print()
        print_warning("interrupted -- staging is kept, re-run to resume")
        return 130


if __name__ == "__main__":
    raise SystemExit(main())
