"""8.9a live gate, the WINDOWS half: the read-only bit on git objects, and the
two refusals, pressed on a real Windows install.

This is `pyplan/gates/8.9a-wotlk-yulon-ubuntu-2026-09-08/gate89a.py` with its
constants changed and ONE stage added -- `readonly`, which is the whole reason
the checklist gives the Windows half its own line:

    readonly  the ground (git packs really are ReadOnly here), then the control
              (a plain `shutil.rmtree` STOPS on that bit), then the fix
              (`purge.remove_tree` deletes the same tree), then the failure
              (`remove_tree` RAISES naming the path when it still cannot finish)
    stage4    an install whose ownership cannot be proved is refused, not removed
    stage3    a purge of a RUNNING server refuses and says to stop it first
    stage2    unticked -- the real press, through the app's own buttons
    locked    the real press with a file HELD OPEN inside the tree: the app must
              NAME the path it could not delete rather than report success

The control in `readonly` is not decoration. `remove_tree`'s retry is only
worth anything if the thing it retries past actually stops the first attempt on
THIS box -- a step whose assertion is already true before its action runs proves
nothing. So the plain `shutil.rmtree` is run first, on an identical copy, and
its exception is recorded.

Nothing below fakes a seam that reaches the machine. `stage2` and `locked` press
the real `main.build_window()` -- the real tab, the real buttons, the real
`state.json`.
"""

from __future__ import annotations

import os
import platform as pyplatform
import shutil
import subprocess
import sys
import time
from datetime import datetime
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
# Qt's offscreen platform uses its own font database, and PySide6 ships no
# fonts: without this every glyph in every screenshot is a tofu box, which is
# an artefact that shows nothing while looking like evidence. Measured on this
# box 2026-09-08 -- the first four shots of this gate came out unreadable and
# were retaken.
os.environ.setdefault("QT_QPA_FONTDIR", r"C:\Windows\Fonts")

HERE = Path(__file__).resolve().parent
sys.path.insert(0, r"C:\gate\src89a\pylauncher")

SERVER_DIR = Path(r"D:\wow-server")
GAME = "wow-wotlk"
PROJECT = "yulon-wow-wotlk-643123e5"
DB_VOLUME = f"{PROJECT}_db-data"
CLIENT_VOLUME = f"{PROJECT}_client-data"
SHOTS = HERE / "shots"
SCRATCH = Path(r"C:\gate89a")
ACTIVITY = Path(r"C:\gate\gate89a-activity.log")


def stamp() -> str:
    return datetime.now().strftime("%H:%M:%S")


def say(line: str = "") -> None:
    if line:
        print(f"[{stamp()}] {line}", flush=True)
    else:
        print(flush=True)


def sh(*argv: str) -> str:
    out = subprocess.run(list(argv), capture_output=True, text=True)
    return (out.stdout + out.stderr).strip()


def announce(text: str) -> None:
    """This box has no `claude-say`; the announcement goes to a file and the log."""
    say(f"ANNOUNCE: {text}")
    try:
        with ACTIVITY.open("a", encoding="utf-8") as handle:
            handle.write(f"{datetime.now().isoformat()}  {text}\n")
    except OSError:
        pass


# -- reading the world -------------------------------------------------------


def attrs(path: Path) -> str:
    """The Windows attribute letters for one file, asked of the OS."""
    out = subprocess.run(
        ["cmd", "/c", "attrib", str(path)], capture_output=True, text=True
    )
    return out.stdout.strip() or out.stderr.strip()


def readonly_census(root: Path, limit: int = 400000) -> tuple[int, int, list[Path]]:
    """How many files under `root` carry the read-only bit. Asked of the filesystem."""
    total = 0
    ro = 0
    samples: list[Path] = []
    for parent, _dirs, files in os.walk(root):
        for name in files:
            target = Path(parent) / name
            total += 1
            try:
                if not (target.stat().st_mode & 0o200):
                    ro += 1
                    if len(samples) < 6:
                        samples.append(target)
            except OSError:
                continue
            if total >= limit:
                return total, ro, samples
    return total, ro, samples


def ground(label: str) -> dict[str, object]:
    """Read the world and print it. This is what a clause starts FROM."""
    from yulon.apply import server_dir_claim
    from yulon.state import load_state, state_path

    say(f"===== GROUND: {label} =====")
    ps = sh("docker", "ps", "-a", "--format", "{{.Names}}\t{{.Status}}")
    say(f"docker ps -a:\n{ps or '(no containers at all)'}")
    vols = sh("docker", "volume", "ls", "--format", "{{.Name}}")
    say(f"docker volume ls:\n{vols or '(no volumes at all)'}")
    imgs = sh("docker", "images", "--format", "{{.Repository}}:{{.Tag}}")
    say(f"docker images:\n{imgs or '(no images at all)'}")
    exists = SERVER_DIR.exists()
    say(f"{SERVER_DIR} exists: {exists}")
    if exists:
        claim = SERVER_DIR / ".yulon-install.json"
        say(f"  .yulon-install.json present: {claim.exists()}")
        say(f"  .git present: {(SERVER_DIR / '.git').exists()}")
    say(f"ownership answer for {SERVER_DIR}: {server_dir_claim(SERVER_DIR).name}")
    sp = state_path()
    say(f"{sp} exists: {sp.exists()}")
    st = load_state()
    say(f"state.json installs: {[(i.game, str(i.server_dir)) for i in st.installs]}")
    names = sh("docker", "ps", "-a", "--format", "{{.Names}}")
    say("===== end ground =====")
    say()
    # Two fields in this listing move on their own between two readings of an
    # UNCHANGED machine, and both made a true "nothing was touched" clause read
    # False on this driver's first two runs:
    #   * `docker ps -a`'s Status carries an uptime ("Up 13 minutes" ->
    #     "Up 14 minutes"), so the comparison uses names only;
    #   * `docker images` does not order its output stably -- the authserver and
    #     worldserver rows swapped between two calls a second apart -- so every
    #     compared listing is sorted.
    # `ps` and the raw listings stay in the log because a reader wants them.
    return {
        "ps": ps,
        "names": sorted(names.splitlines()),
        "volumes": sorted(vols.splitlines()),
        "images": sorted(imgs.splitlines()),
        "folder": exists,
    }


# -- Qt helpers --------------------------------------------------------------


def pump(app, done, timeout: float, what: str) -> bool:
    started = time.monotonic()
    while time.monotonic() - started < timeout:
        app.processEvents()
        if done():
            say(f"  {what}: after {time.monotonic() - started:.1f}s")
            return True
        time.sleep(0.05)
    say(f"  {what}: TIMED OUT after {timeout:.0f}s")
    return False


def open_window():
    """The real main window, from the real state.json."""
    from PySide6.QtWidgets import QApplication

    import main as yulon_main
    from yulon.ui.catalog_view import CatalogView
    from yulon.ui.controller_view import ControllerView

    app = QApplication.instance() or QApplication([])
    window = yulon_main.build_window()
    window.show()
    app.processEvents()
    views = list(window.findChildren(ControllerView))
    catalog_view = window.findChild(CatalogView)
    say(f"window built: {len(views)} controller tab(s), catalog view: {catalog_view is not None}")
    return app, window, views, catalog_view


def view_for(views, server_dir: Path = SERVER_DIR):
    """The tab for THIS install, chosen by folder rather than by position.

    `state.json` on this box also carries two records for folders that no
    longer exist (the 2026-09-03 TBC-on-Windows gate's), so `views[0]` is not
    reliably the subject.
    """
    for view in views:
        if Path(view.services.controller.server_dir) == server_dir:
            return view
    return None


def close_window(window) -> None:
    import main as yulon_main

    yulon_main._stop_background_threads(window)
    window.close()


def tile_state(catalog_view) -> str:
    b = catalog_view.button_for(GAME)
    return f"tile[{GAME}] text={b.text()!r} enabled={b.isEnabled()} tooltip={b.toolTip()!r}"


def shoot(catalog_view, name: str) -> None:
    SHOTS.mkdir(parents=True, exist_ok=True)
    button = catalog_view.button_for(GAME)
    tile = button.parentWidget()
    tile.grab().save(str(SHOTS / f"{name}-tile.png"))
    catalog_view.grab().save(str(SHOTS / f"{name}-catalog.png"))
    say(f"  screenshots: {name}-tile.png, {name}-catalog.png")


def shoot_tab(view, name: str) -> None:
    """The uninstall panel as the user sees it: the words a refusal is made of.

    Guarded, because a SUCCESSFUL uninstall deletes this widget: the window
    drops the tab, and `view.grab()` then raises `libshiboken: Internal C++
    object (ControllerView) already deleted`. The first run of this driver
    crashed there after the one press that worked, and lost the catalog shot
    that press existed to take -- so the catalog is grabbed first now, and this
    says plainly when there is no tab left to photograph.
    """
    SHOTS.mkdir(parents=True, exist_ok=True)
    try:
        view.grab().save(str(SHOTS / f"{name}-tab.png"))
    except RuntimeError as exc:
        say(f"  no {name}-tab.png: the tab is gone ({exc})")
        return
    say(f"  screenshot: {name}-tab.png")


# -- seeding -----------------------------------------------------------------


def seed() -> int:
    """Write the state.json record this install never had. The app's own API."""
    from yulon.state import AppState, KnownInstall, load_state, save_state, state_path

    st: AppState = load_state()
    say(f"state.json BEFORE: {[(i.game, str(i.server_dir)) for i in st.installs]}")
    st.remember(KnownInstall(game=GAME, server_dir=SERVER_DIR, client_dir=None))
    path = save_state(st)
    say(f"seeded {path}: {path.read_text()}")
    return 0


# -- the Windows clause: the read-only bit -----------------------------------


def _control_tree(dest: Path, source_files: list[Path]) -> list[Path]:
    """A small tree carrying this install's OWN read-only git objects.

    Copied with `shutil.copy2`, which copies the mode -- and on Windows the
    read-only attribute is what a cleared owner-write bit means. The attribute
    is re-read from the copy afterwards rather than assumed.
    """
    if dest.exists():
        _force_remove(dest)
    (dest / ".git" / "objects" / "pack").mkdir(parents=True)
    (dest / "src").mkdir()
    (dest / "src" / "writable.txt").write_text("an ordinary file\n")
    copied = []
    for src in source_files:
        target = dest / ".git" / "objects" / "pack" / src.name
        shutil.copy2(src, target)
        copied.append(target)
    return copied


def _force_remove(path: Path) -> None:
    """Teardown for the driver's own scratch, never for the subject."""
    for parent, dirs, files in os.walk(path):
        for name in (*dirs, *files):
            try:
                os.chmod(Path(parent) / name, 0o700)
            except OSError:
                pass
    shutil.rmtree(path, ignore_errors=True)


def stage_readonly() -> int:
    announce("8.9a Windows clause: the read-only bit on git objects")
    from yulon import purge

    git_dir = SERVER_DIR / ".git"
    say("===== GROUND: does this install's .git really hold read-only files? =====")
    say(f"{git_dir} exists: {git_dir.exists()}")
    packs = sorted((git_dir / "objects" / "pack").glob("*"))
    for p in packs:
        say(f"  {p.name}  {p.stat().st_size} bytes   attrib: {attrs(p)}")
    total, ro, samples = readonly_census(git_dir)
    say(f"  files under .git: {total}, of them read-only: {ro}")
    for s in samples:
        say(f"    read-only sample: {s}   attrib: {attrs(s)}")
    if ro == 0:
        say("ABORT: nothing under .git is read-only, so this box cannot press this clause")
        return 2
    say("===== end ground =====")
    say()

    # The small, real read-only files this control is built from. The 1.3 GB
    # .pack is skipped: the bit is the subject, not the byte count.
    smalls = [p for p in packs if p.suffix in (".idx", ".rev")]
    say(f"control files (this install's own read-only git objects): {[p.name for p in smalls]}")

    SCRATCH.mkdir(parents=True, exist_ok=True)

    # -- control: does the bit stop a plain delete on THIS box? --------------
    a = SCRATCH / "control-plain"
    copied = _control_tree(a, smalls)
    say("-- CONTROL: a plain shutil.rmtree over the same read-only objects")
    for c in copied:
        say(f"   copy carries: {attrs(c)}")
    plain_ok = None
    try:
        shutil.rmtree(a)
        plain_ok = True
        say("   shutil.rmtree SUCCEEDED -- the read-only bit does NOT stop a delete here")
    except OSError as exc:
        plain_ok = False
        say(f"   shutil.rmtree raised {type(exc).__name__}: {exc}")
    say(f"   {a} still there: {a.exists()}")
    say()

    # -- the fix: purge.remove_tree over an identical tree -------------------
    b = SCRATCH / "control-remove-tree"
    copied = _control_tree(b, smalls)
    say("-- THE FIX: purge.remove_tree over an identical tree")
    for c in copied:
        say(f"   copy carries: {attrs(c)}")
    try:
        purge.remove_tree(b)
        say("   remove_tree returned; no exception")
    except purge.PurgeError as exc:
        say(f"   remove_tree RAISED PurgeError: {exc}")
    say(f"   {b} still there: {b.exists()}")
    say()

    # -- the failure: it must RAISE, not report success ----------------------
    c = SCRATCH / "control-locked"
    copied = _control_tree(c, smalls)
    locked = c / "src" / "held-open.txt"
    locked.write_text("a program is holding this open\n")
    say("-- THE FAILURE PATH: a file held open, which Windows will not let go")
    handle = open(locked, "rb")  # noqa: SIM115 -- the lock IS the fixture
    try:
        raised = ""
        try:
            purge.remove_tree(c)
            say("   remove_tree returned WITHOUT raising")
        except purge.PurgeError as exc:
            raised = str(exc)
            say("   remove_tree raised PurgeError:")
            print(raised, flush=True)
        say(f"   {c} still there: {c.exists()}")
        say(f"   the held-open file still there: {locked.exists()}")
        say(f"   the message names the folder: {str(c) in raised}")
        silent = (not raised) and c.exists()
        say(f"   SILENT SUCCESS WITH FILES STILL ON DISK (the failure): {silent}")
    finally:
        handle.close()
    _force_remove(c)
    say()
    say(f"summary: plain rmtree deleted the read-only tree: {plain_ok}")
    say(f"summary: purge.remove_tree deleted the read-only tree: {not b.exists()}")
    return 0


# -- clause: ownership cannot be proved --------------------------------------


DECOYS = {
    "unknown": SCRATCH / "decoy-unknown",
    "unclaimed": SCRATCH / "decoy-unclaimed",
}


def make_decoys() -> None:
    for kind, path in DECOYS.items():
        if path.exists():
            _force_remove(path)
        path.mkdir(parents=True)
        (path / "docker-compose.yml").write_text("services: {}\n")
        (path / "MY-PRECIOUS-SAVES.txt").write_text("a user's hand-built server lives here\n")
        if kind == "unknown":
            (path / ".yulon-install.json").write_text("{ this is not json")


def stage4() -> int:
    announce("8.9a clause 4 on Windows: ownership that cannot be proved")
    from yulon import purge
    from yulon.apply import server_dir_claim
    from yulon.catalog import composegen
    from yulon.catalog.catalog import load_catalog

    make_decoys()
    say("===== GROUND: clause 4, BEFORE the press =====")
    for kind, path in DECOYS.items():
        say(f"{kind}: {path} contents {sorted(p.name for p in path.iterdir())}")
        say(f"{kind}: ownership answer = {server_dir_claim(path).name}")
    real_before = ground("clause 4, the REAL install before the decoy presses")

    entry = load_catalog().get(GAME)
    forgot: list[str] = []
    refusals: dict[str, str] = {}
    for kind, path in DECOYS.items():
        say(f"-- decoy {kind} ({path}) --")
        un = purge.Uninstaller(
            game=GAME,
            server_dir=path,
            spec=entry.container_spec(),
            image_refs=composegen.built_image_refs(entry, path),
            forget=lambda k=kind: forgot.append(k),
        )
        plan = un.plan()
        refusals[kind] = plan.refusal
        say(f"plan().refusal:\n{plan.refusal}")
        say(
            f"plan() resolved anything else? project={plan.project!r} "
            f"containers={plan.containers} volumes={plan.volumes}"
        )
        try:
            un.run(keep_characters=False)
            say("NO REFUSAL -- run() went ahead")
        except purge.PurgeError as exc:
            say(f"run() refused:\n{exc}")
        say(
            f"{path} still there: {path.exists()} "
            f"contents {sorted(p.name for p in path.iterdir()) if path.exists() else '(gone)'}"
        )
        say()

    say(f"the two refusals read the same: {refusals['unknown'] == refusals['unclaimed']}")
    say(f"forget() was called for: {forgot or 'nothing'}")
    real_after = ground("clause 4, the REAL install after the decoy presses")
    same = all(
        real_before[k] == real_after[k] for k in ("names", "volumes", "images", "folder")
    )
    say(f"the real install in another folder was untouched: {same}")
    for path in DECOYS.values():
        _force_remove(path)
    return 0


# -- the same refusals, in the app's own tab, for a screenshot ---------------


CLAIM = SERVER_DIR / ".yulon-install.json"
CLAIM_BACKUP = SCRATCH / "yulon-install.json.saved"


def _press_plan_only(app, view, tag: str) -> str:
    view.uninstall_button.click()
    pump(app, lambda: "Working out" not in view.uninstall_label.text(), 300, "plan came back")
    text = view.uninstall_label.text()
    say("--- what the tab shows ---")
    print(text, flush=True)
    say("--- end ---")
    say(f"confirm button visible: {view.uninstall_confirm_button.isVisible()}")
    app.processEvents()
    shoot_tab(view, tag)
    return text


def stage_claimshot() -> int:
    """The ownership answers, pressed on the REAL tab and photographed.

    Each variant is written, pressed and undone in turn, and the claim file is
    restored from a copy taken before anything was touched. Nothing here is
    destructive: `plan()` reads only, and every press but the first is expected
    to refuse.
    """
    announce("8.9a Windows: the ownership refusals as the tab shows them")
    from yulon.apply import server_dir_claim

    SCRATCH.mkdir(parents=True, exist_ok=True)
    original = CLAIM.read_bytes()
    CLAIM_BACKUP.write_bytes(original)
    say(f"saved the real claim ({len(original)} bytes) to {CLAIM_BACKUP}")
    say(f"GROUND: ownership with the real claim in place: {server_dir_claim(SERVER_DIR).name}")

    variants = {
        "04-owned": None,
        "05-unknown-truncated": b"{ this is not js",
        "06-unknown-newer-version": (
            b'{"version": 99, "game_id": "wow-wotlk", "family": "azerothcore", '
            b'"install_id": "643123e5", "completed": [], "last_error": "", '
            b'"updated_unix": 1788394235}'
        ),
        "07-unclaimed-no-record": b"",
    }
    seen: dict[str, str] = {}
    try:
        for tag, payload in variants.items():
            say(f"===== variant {tag} =====")
            if payload is None:
                CLAIM.write_bytes(original)
            elif payload == b"":
                CLAIM.unlink(missing_ok=True)
            else:
                CLAIM.write_bytes(payload)
            size = CLAIM.stat().st_size if CLAIM.exists() else 0
            say(f"claim on disk: {CLAIM.exists()} ({size} bytes)")
            say(f"ownership answer: {server_dir_claim(SERVER_DIR).name}")
            app, window, views, catalog_view = open_window()
            view = view_for(views)
            if view is None:
                say("ABORT: no controller tab for this install; run `seed` first")
                return 2
            seen[tag] = _press_plan_only(app, view, tag)
            close_window(window)
            say()
    finally:
        CLAIM.write_bytes(original)
        say(f"claim restored; ownership answer is {server_dir_claim(SERVER_DIR).name} again")

    say("--- do the refusals differ from one another? ---")
    say(
        "truncated vs newer-version read the same: "
        f"{seen.get('05-unknown-truncated') == seen.get('06-unknown-newer-version')}"
    )
    say(
        "truncated vs unclaimed read the same: "
        f"{seen.get('05-unknown-truncated') == seen.get('07-unclaimed-no-record')}"
    )
    return 0


# -- bringing the ground for clause 3 into existence -------------------------


TBC_CONTAINERS = ("tbc-mangosd", "tbc-realmd", "tbc-db")


def stage_startup() -> int:
    """Start THIS install through the app's own Start button.

    The box boots with the TBC stack up (a restart policy from an earlier
    gate), and this VM has 10 GB: two emulators and their databases do not fit
    beside each other, so the neighbour is stopped first. It is stopped, not
    removed -- and the checkpoint restore at the end of this gate is what puts
    it back running.
    """
    announce("8.9a Windows: starting the WotLK stack for the running-server clause")
    say("-- stopping the neighbouring TBC stack first (this box has 10 GB)")
    say(sh("docker", "stop", *TBC_CONTAINERS))
    ground("startup, after the neighbour was stopped")

    app, window, views, catalog_view = open_window()
    view = view_for(views)
    if view is None:
        say("ABORT: no controller tab for this install; run `seed` first")
        return 2
    say(f"start button enabled: {view.start_button.isEnabled()}")
    say("-- press: Start")
    started = time.monotonic()
    view.start_button.click()

    def containers_up() -> bool:
        out = sh("docker", "ps", "--format", "{{.Names}}\t{{.Status}}")
        return out.count("ac-") >= 3 and not view._busy

    pump(app, containers_up, 2400, "start finished")
    say(f"took {time.monotonic() - started:.1f}s")
    say(f"status label: {view.status_label.text()!r}")
    say(f"problem label: {view.problem_label.text()!r}")
    close_window(window)
    ground("startup, after Start")
    return 0


# -- clause: a running server refuses ----------------------------------------


def stage3() -> int:
    announce("8.9a clause 3 on Windows: pressing Uninstall while the server is running")
    before = ground("clause 3, BEFORE the press -- the server should be RUNNING")
    if "ac-worldserver" not in before["ps"] or "Up" not in before["ps"]:
        say("ABORT: the world is not running, so this clause has no ground")
        return 2

    app, window, views, catalog_view = open_window()
    say(tile_state(catalog_view))
    view = view_for(views)
    if view is None:
        say("ABORT: no controller tab for this install; run `seed` first")
        return 2
    say(f"tab for: {view.services.controller.server_dir}")

    say("-- press 1: Uninstall... (this calls plan(), which must READ ONLY)")
    view.uninstall_button.click()
    pump(app, lambda: "Working out" not in view.uninstall_label.text(), 300, "plan came back")
    say("--- what the tab shows ---")
    print(view.uninstall_label.text(), flush=True)
    say("--- end ---")
    say(
        f"confirm button visible (would a press be offered?): "
        f"{view.uninstall_confirm_button.isVisible()}"
    )
    app.processEvents()
    shoot_tab(view, "01-running-server-refusal")

    say("-- press 2: the destructive call itself, run(keep_characters=False)")
    from yulon import purge

    try:
        report = view.services.uninstall.run(keep_characters=False)
        say(f"NO REFUSAL -- it ran: {report}")
    except purge.PurgeError as exc:
        say("--- run() refused with ---")
        print(str(exc), flush=True)
        say("--- end ---")

    after = ground("clause 3, AFTER both presses")
    same = all(before[k] == after[k] for k in ("names", "volumes", "images", "folder"))
    say(f"plan() reads only + clause 3 left nothing changed: {same}")
    close_window(window)
    return 0


# -- the destructive presses -------------------------------------------------


def stop_the_server(app, view) -> None:
    say("-- pressing Stop (the app's own button)")
    say(f"stop button enabled: {view.stop_button.isEnabled()}")
    if "Up " not in sh("docker", "ps", "--format", "{{.Names}}\t{{.Status}}"):
        say("  nothing is running; no stop needed")
        return
    view.stop_button.click()
    pump(app, lambda: not view._busy, 900, "stop finished")
    say(f"status label: {view.status_label.text()!r}")
    say(f"problem label: {view.problem_label.text()!r}")
    now = sh("docker", "ps", "-a", "--format", "{{.Names}}\t{{.Status}}")
    say(f"docker ps -a now:\n{now}")


def press_uninstall(app, window, views, catalog_view, *, keep: bool, tag: str) -> int:
    view = view_for(views)
    if view is None:
        say("ABORT: no controller tab for this install; run `seed` first")
        return 2
    view.keep_characters_check.setChecked(keep)
    say(f'"Keep my characters" is ticked: {view.keep_characters_check.isChecked()}')
    say(f"BEFORE: {tile_state(catalog_view)}")
    shoot(catalog_view, f"{tag}-before")

    stop_the_server(app, view)

    say("-- press: Uninstall... (plan)")
    view.uninstall_button.click()
    pump(app, lambda: "Working out" not in view.uninstall_label.text(), 600, "plan came back")
    say("--- the plan the user is shown ---")
    print(view.uninstall_label.text(), flush=True)
    say("--- end ---")
    say(f"confirm button shown to the user: {view.uninstall_confirm_button.isVisible()}")
    app.processEvents()
    shoot_tab(view, f"{tag}-plan")
    if view._uninstall_plan is None:
        say("REFUSED at plan time; nothing to confirm")
        return 3

    gone: list[tuple] = []
    view.uninstalled.connect(lambda g, d: gone.append((g, d)))
    failures: list[str] = []
    view.action_failed.connect(failures.append)

    say("-- press: Uninstall this server (the destructive one)")
    started = time.monotonic()
    view.uninstall_confirm_button.click()
    pump(app, lambda: bool(gone) or bool(failures), 1800, "uninstall finished")
    say(f"took {time.monotonic() - started:.1f}s")
    say("--- what the tab says ---")
    print(view.uninstall_label.text(), flush=True)
    say("--- end ---")
    say(f"uninstalled signal carried: {gone}")
    say(f"action_failed carried: {failures}")
    app.processEvents()
    # The catalog FIRST: on a successful uninstall the tab is destroyed, and the
    # tile reading "Install" again is the visible effect this press exists to
    # photograph.
    say(f"AFTER: {tile_state(catalog_view)}")
    shoot(catalog_view, f"{tag}-after")
    shoot_tab(view, f"{tag}-after")
    from yulon.ui.controller_view import ControllerView

    left = [Path(v.services.controller.server_dir) for v in window.findChildren(ControllerView)]
    say(f"tabs left in the window: {[str(x) for x in left]}")
    say(f"the tab for {SERVER_DIR} is gone from the window: {SERVER_DIR not in left}")
    return 0


def stage2() -> int:
    """Unticked, no lock: the folder must be GONE, read-only packs and all."""
    announce("8.9a clause 2 on Windows: uninstall with Keep my characters UNTICKED")
    before = ground("clause 2 UNTICKED, BEFORE")
    if not before["folder"]:
        say("ABORT: the folder is not there, so this clause has no ground")
        return 2
    total, ro, _ = readonly_census(SERVER_DIR / ".git")
    say(f"GROUND: .git holds {total} files, {ro} of them read-only")
    if ro == 0:
        say("ABORT: no read-only file under .git; this press would prove nothing about Windows")
        return 2

    app, window, views, catalog_view = open_window()
    rc = press_uninstall(app, window, views, catalog_view, keep=False, tag="02-unticked")
    if rc:
        return rc

    after = ground("clause 2 UNTICKED, AFTER")
    say("--- the negative, proved by asking the filesystem and Docker, not the report ---")
    say(f"no folder:     {SERVER_DIR} exists: {SERVER_DIR.exists()}")
    say(f"no container:  ac- in `docker ps -a`: {'ac-' in after['ps']}")
    say(f"no volume:     {PROJECT} in `docker volume ls`: "
        f"{any(PROJECT in v for v in after['volumes'])}")
    say(f"no image:      yulon.local/ac- in `docker images`: "
        f"{any('yulon.local/ac-' in i for i in after['images'])}")
    from yulon.state import load_state

    st = load_state()
    say(f"no record:     state.json installs = {[(i.game, str(i.server_dir)) for i in st.installs]}")
    say("raw `docker volume ls`:")
    print(sh("docker", "volume", "ls"), flush=True)
    say("raw `docker images`:")
    print(sh("docker", "images"), flush=True)
    close_window(window)
    return 0


def stage_retry() -> int:
    """The clause `_undeletable()` promises: a second press can finish the job.

    Its ground is a HALF-DELETED tree -- what the first press left when
    `shutil.rmtree` stopped on the WSL symlink -- with the record still in
    `state.json` and the claim file still on disk. This stage does not check
    the read-only census, because the first press's retry already cleared every
    read-only bit in the tree: asserting it here would be asserting the start
    state.
    """
    announce("8.9a Windows: the second press, on the tree the first one left half-deleted")
    before = ground("retry, BEFORE -- a half-deleted tree")
    if not before["folder"]:
        say("ABORT: the folder is not there, so this clause has no ground")
        return 2
    left = sum(len(files) + len(dirs) for _p, dirs, files in os.walk(SERVER_DIR))
    say(f"GROUND: {left} entries still under {SERVER_DIR}")
    total, ro, _ = readonly_census(SERVER_DIR / ".git")
    say(f"GROUND: .git holds {total} files, {ro} of them read-only "
        f"(the first press's retry cleared the bit; that is why this is not 3)")

    app, window, views, catalog_view = open_window()
    rc = press_uninstall(app, window, views, catalog_view, keep=False, tag="04-retry")
    if rc:
        return rc

    after = ground("retry, AFTER")
    say("--- the negative, proved by asking the filesystem, not the report ---")
    say(f"no folder:     {SERVER_DIR} exists: {SERVER_DIR.exists()}")
    from yulon.state import load_state

    st = load_state()
    say(f"no record:     state.json installs = {[(i.game, str(i.server_dir)) for i in st.installs]}")
    say(f"volumes now:   {after['volumes'] or ['(none)']}")
    close_window(window)
    return 0


def stage_locked() -> int:
    """The same press with a file HELD OPEN: the app must name the path it could not delete."""
    announce("8.9a Windows failure path: uninstall with a file held open inside the tree")
    before = ground("locked press, BEFORE")
    if not before["folder"]:
        say("ABORT: the folder is not there, so this clause has no ground")
        return 2

    victim = SERVER_DIR / "GATE89A-HELD-OPEN.txt"
    victim.write_text("a program is holding this open while the uninstall runs\n")
    handle = open(victim, "rb")  # noqa: SIM115 -- the lock IS the fixture
    say(f"holding open: {victim} (attrib: {attrs(victim)})")
    try:
        app, window, views, catalog_view = open_window()
        rc = press_uninstall(app, window, views, catalog_view, keep=False, tag="03-locked")
        say(f"press_uninstall returned {rc}")
        after = ground("locked press, AFTER")
        say("--- what the machine looks like now ---")
        say(f"folder still there: {SERVER_DIR.exists()}")
        say(f"the held-open file still there: {victim.exists()}")
        claim = SERVER_DIR / ".yulon-install.json"
        say(f".yulon-install.json still there: {claim.exists()}")
        from yulon.apply import server_dir_claim
        from yulon.state import load_state

        say(f"ownership answer NOW: {server_dir_claim(SERVER_DIR).name}")
        st = load_state()
        say(f"state.json installs: {[(i.game, str(i.server_dir)) for i in st.installs]}")
        say(f"containers: {after['ps'] or '(none)'}")
        say(f"volumes: {after['volumes'] or ['(none)']}")
        close_window(window)
    finally:
        handle.close()
    return 0


STAGES = {
    "ground": lambda: (ground("on demand"), 0)[1],
    "seed": seed,
    "readonly": stage_readonly,
    "stage4": stage4,
    "claimshot": stage_claimshot,
    "startup": stage_startup,
    "stage3": stage3,
    "stage2": stage2,
    "retry": stage_retry,
    "locked": stage_locked,
}


if __name__ == "__main__":
    which = sys.argv[1] if len(sys.argv) > 1 else "ground"
    say(f"### 8.9a WINDOWS gate driver, stage {which}, {datetime.now().astimezone().isoformat()} ###")
    say(f"### box: {pyplatform.node()}, {pyplatform.platform()}, tree: {HERE} ###")
    raise SystemExit(STAGES[which]())
