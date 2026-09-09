"""T4 step 2 -- press the app's own rebuild on the live Tortoise install at ~/tortoise-server.

Two subcommands, because the ticket's "through the app's own control" needs both halves and
they are not the same act:

`dockerfile`
    Runs the family's OWN `write-dockerfile` stage. It is needed because
    `native.StagedInstaller.rebuild_stages()` is `build, recreate, ready` -- it deliberately
    excludes `write-dockerfile` and `generate-compose` ("rewrites files a running server is
    using") -- so a rebuild compiles the Dockerfile ALREADY on disk. The install at
    ~/tortoise-server rendered its Dockerfile on 2026-09-07, before `3a1ed6ee` added the
    `INSERT IGNORE` rewrite to `wow-tortoise/native/Dockerfile.tmpl`, so a bare Rebuild
    press cannot carry the fix this ticket exists to install. Same renderer, same marker
    rule, one stage.

`rebuild`
    `install_wiring.rebuild_for_app(entry, server_dir)` -- the exact callable the Server
    tab's Rebuild button drives -- streamed to stdout with its lines timestamped.

Neither prints the install's generated database password: `dockerfile` asserts the rendered
file does not contain it and prints that assertion rather than the file.
"""

from __future__ import annotations

import sys
import time
from pathlib import Path

from yulon import install_wiring
from yulon.catalog import native
from yulon.catalog.catalog import load_catalog

SERVER_DIR = Path.home() / "tortoise-server"
GAME = "wow-tortoise"


def _stamp(line: str) -> None:
    print(f"[{time.strftime('%H:%M:%SZ', time.gmtime())}] {line}", flush=True)


def _entry():
    return load_catalog().get(GAME)


def press_dockerfile() -> int:
    entry = _entry()
    engine = install_wiring.installer_for_app(entry)
    state = native.read_state(SERVER_DIR, valid=engine.stage_names())
    if state is None:
        _stamp(f"REFUSING: no install state in {SERVER_DIR}")
        return 2
    before = (SERVER_DIR / "Dockerfile").read_text(encoding="utf-8")
    ctx = native.StageContext(
        server_dir=SERVER_DIR,
        client_dir=None,
        state=state,
        cancel=None,
        secrets=engine.resolve_secrets(SERVER_DIR),
    )
    stage = engine.stage_named("write-dockerfile")
    _stamp(f"--- stage {stage.name} (the app's own renderer) ---")
    for line in stage.run(ctx):
        _stamp(line)
    after = (SERVER_DIR / "Dockerfile").read_text(encoding="utf-8")
    pw = (SERVER_DIR / ".db_password").read_text(encoding="utf-8").strip()
    _stamp(f"Dockerfile changed: {before != after}")
    _stamp(f"the generated database password is absent from the Dockerfile: {pw not in after}")
    for name, text in (("before", before), ("after", after)):
        froms = [ln for ln in text.splitlines() if ln.startswith("FROM ")]
        ignore = [ln for ln in text.splitlines() if "INSERT IGNORE" in ln]
        _stamp(f"{name}: FROM {froms} | INSERT IGNORE lines {len(ignore)}")
    return 0


def press_rebuild() -> int:
    entry = _entry()
    rebuild = install_wiring.rebuild_for_app(entry, SERVER_DIR)
    started = time.time()
    _stamp("--- install_wiring.rebuild_for_app: the Server tab's Rebuild, pressed ---")
    try:
        for line in rebuild(None):
            _stamp(line)
    except Exception as exc:  # InstallerError is the sentence a user reads
        _stamp(f"REBUILD FAILED: {type(exc).__name__}: {exc}")
        _stamp(f"elapsed: {time.time() - started:.0f}s")
        return 1
    _stamp(f"REBUILD OK. elapsed: {time.time() - started:.0f}s")
    return 0


if __name__ == "__main__":
    what = sys.argv[1] if len(sys.argv) > 1 else ""
    if what == "dockerfile":
        sys.exit(press_dockerfile())
    if what == "rebuild":
        sys.exit(press_rebuild())
    print(__doc__)
    sys.exit(2)
