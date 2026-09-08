"""8.9a live gate: uninstall and purge, WoW WotLK, on yulon-ubuntu.

Four clauses, each pressed from a state its own action does not produce, on the
real install at /home/pk/wowserver behind the Hyper-V checkpoint
`8.9a-purge-gate-2026-09-08b`:

    stage3   a purge of a RUNNING server refuses and says to stop it first
    stage4   an install whose ownership cannot be proved is refused, not removed
    stage1   ticked   -- the characters survive the purge and are still readable
    stage2   unticked -- nothing remains: container, volume, image, folder, record

Stages 3 and 4 destroy nothing and run first, on the box exactly as the
checkpoint has it. Stages 1 and 2 each end with a restore.

Nothing below fakes a seam that reaches the machine. The presses go through the
real `main.build_window()` -- the same window a user gets -- so the record, the
tab teardown and the catalog tile are the app's own, not this script's.

`seed` writes the `state.json` record this install never had: it was built by
the 7.2 gate outside the launcher, so the app has no memory of it and would show
no tab to press. Written with `AppState.remember` / `save_state`, the app's own
API, and read back before every press.
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import time
from datetime import datetime
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE / "pylauncher"))

SERVER_DIR = Path("/home/pk/wowserver")
GAME = "wow-wotlk"
PROJECT = "yulon-wow-wotlk-243c46e3"
DB_VOLUME = f"{PROJECT}_db-data"
SHOTS = HERE / "shots"


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
    subprocess.run([str(Path.home() / "bin" / "claude-say"), text], check=False)


# -- reading the world -------------------------------------------------------


def characters() -> str:
    """How many characters the live database holds, asked of the database itself."""
    return sh(
        "docker",
        "exec",
        "ac-database",
        "mysql",
        "-uroot",
        "-ppassword",
        "-N",
        "-e",
        "select count(*), min(name), max(name) from acore_characters.characters;",
    )


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
    size = sh("du", "-sh", str(SERVER_DIR)) if exists else "(gone)"
    say(f"{SERVER_DIR} exists: {exists}   {size}")
    say(f"ownership answer for {SERVER_DIR}: {server_dir_claim(SERVER_DIR).name}")
    sp = state_path()
    say(f"{sp} exists: {sp.exists()}")
    st = load_state()
    say(f"state.json installs: {[(i.game, str(i.server_dir)) for i in st.installs]}")
    if "ac-database" in ps and "Up" in ps:
        say(f"characters in acore_characters (count, first, last): {characters()}")
    say("===== end ground =====")
    say()
    return {"ps": ps, "volumes": vols, "images": imgs, "folder": exists}


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
    # Shown, so `isVisible()` on a child answers about the widget and not about
    # a window that was never mapped. Offscreen, but really shown.
    window.show()
    app.processEvents()
    views = [w for w in window.findChildren(ControllerView)]
    catalog_view = window.findChild(CatalogView)
    say(f"window built: {len(views)} controller tab(s), catalog view: {catalog_view is not None}")
    return app, window, views, catalog_view


def close_window(window) -> None:
    """The app's own exit path, so a live QThread is joined rather than aborted."""
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


# -- seeding -----------------------------------------------------------------


def seed() -> int:
    """Write the state.json record this install never had. The app's own API."""
    from yulon.state import AppState, KnownInstall, load_state, save_state, state_path

    st: AppState = load_state()
    st.remember(KnownInstall(game=GAME, server_dir=SERVER_DIR, client_dir=None))
    path = save_state(st)
    say(f"seeded {path}: {path.read_text()}")
    return 0


# -- clause 3: a running server refuses --------------------------------------


def stage3() -> int:
    announce("8.9a clause 3: pressing Uninstall while the server is running")
    before = ground("clause 3, BEFORE the press -- the server is RUNNING")
    if "ac-worldserver" not in before["ps"] or "Up" not in before["ps"]:
        say("ABORT: the world is not running, so this clause has no ground")
        return 2

    app, window, views, catalog_view = open_window()
    say(tile_state(catalog_view))
    if not views:
        say("ABORT: no controller tab; run `seed` first")
        return 2
    view = views[0]
    say(f"tab for: {view.services.controller.server_dir}")
    say(f"uninstall button present: {view.uninstall_button is not None}")

    say("-- press 1: Uninstall... (this calls plan(), which must READ ONLY)")
    view.uninstall_button.click()
    pump(app, lambda: "Working out" not in view.uninstall_label.text(), 180, "plan came back")
    say("--- what the tab shows ---")
    print(view.uninstall_label.text(), flush=True)
    say("--- end ---")
    say(f"confirm button visible (would a press be offered?): "
        f"{view.uninstall_confirm_button.isVisible()}")

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
    same = all(before[k] == after[k] for k in ("ps", "volumes", "images", "folder"))
    say(f"CLAUSE 5 (plan reads only) + clause 3 left nothing changed: {same}")
    close_window(window)
    return 0


# -- clause 4: ownership cannot be proved ------------------------------------


DECOYS = {
    "unknown": Path("/home/pk/decoy-unknown"),
    "unclaimed": Path("/home/pk/decoy-unclaimed"),
}


def make_decoys() -> None:
    """Two folders no purge may touch, and a canary file in each."""
    for kind, path in DECOYS.items():
        if path.exists():
            shutil.rmtree(path)
        path.mkdir(parents=True)
        (path / "docker-compose.yml").write_text("services: {}\n")
        (path / "MY-PRECIOUS-SAVES.txt").write_text("a user's hand-built server lives here\n")
        if kind == "unknown":
            # A record that IS there and cannot be read: the UNKNOWN case.
            (path / ".yulon-install.json").write_text("{ this is not json")


def stage4() -> int:
    announce("8.9a clause 4: pressing Uninstall on installs whose ownership cannot be proved")
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
        say(f"plan().refusal:\n{plan.refusal}")
        say(f"plan() resolved anything else? project={plan.project!r} "
            f"containers={plan.containers} volumes={plan.volumes}")
        try:
            un.run(keep_characters=False)
            say("NO REFUSAL -- run() went ahead")
        except purge.PurgeError as exc:
            say(f"run() refused:\n{exc}")
        say(f"{path} still there: {path.exists()} "
            f"contents {sorted(p.name for p in path.iterdir()) if path.exists() else '(gone)'}")
        say()

    say(f"forget() was called for: {forgot or 'nothing'}")
    real_after = ground("clause 4, the REAL install after the decoy presses")
    same = all(real_before[k] == real_after[k] for k in ("ps", "volumes", "images", "folder"))
    say(f"the real install in another folder was untouched: {same}")
    for path in DECOYS.values():
        shutil.rmtree(path, ignore_errors=True)
    return 0


# -- the two destructive presses ---------------------------------------------


def stop_the_server(app, view) -> None:
    say("-- pressing Stop (the app's own button)")
    say(f"stop button enabled: {view.stop_button.isEnabled()}")
    if "Up " not in sh("docker", "ps", "--format", "{{.Names}}\t{{.Status}}"):
        say("  nothing is running; no stop needed")
        return
    view.stop_button.click()
    pump(app, lambda: not view._busy, 600, "stop finished")
    say(f"status label: {view.status_label.text()!r}")
    say(f"problem label: {view.problem_label.text()!r}")
    say(f"docker ps -a now:\n{sh('docker', 'ps', '-a', '--format', '{{.Names}}\t{{.Status}}')}")


def press_uninstall(app, window, views, catalog_view, *, keep: bool) -> int:
    view = views[0]
    view.keep_characters_check.setChecked(keep)
    say(f'"Keep my characters" is ticked: {view.keep_characters_check.isChecked()}')
    say(f"BEFORE: {tile_state(catalog_view)}")
    shoot(catalog_view, "before" if keep else "unticked-before")

    stop_the_server(app, view)

    say("-- press: Uninstall... (plan)")
    view.uninstall_button.click()
    pump(app, lambda: "Working out" not in view.uninstall_label.text(), 300, "plan came back")
    say("--- the plan the user is shown ---")
    print(view.uninstall_label.text(), flush=True)
    say("--- end ---")
    say(f"confirm button shown to the user: {view.uninstall_confirm_button.isVisible()}")
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
    pump(app, lambda: bool(gone) or bool(failures), 900, "uninstall finished")
    say(f"took {time.monotonic() - started:.1f}s")
    say("--- what the tab says ---")
    print(view.uninstall_label.text(), flush=True)
    say("--- end ---")
    say(f"uninstalled signal carried: {gone}")
    say(f"action_failed carried: {failures}")
    app.processEvents()
    say(f"AFTER: {tile_state(catalog_view)}")
    shoot(catalog_view, "after" if keep else "unticked-after")
    say(f"the tab is gone from the window: "
        f"{view not in window.findChildren(type(view))}")
    return 0


def stage1() -> int:
    """Ticked: the characters survive."""
    announce("8.9a clause 1: uninstall with Keep my characters TICKED")
    before = ground("clause 1 TICKED, BEFORE -- stack up, both volumes, folder, record")
    if DB_VOLUME not in before["volumes"] or not before["folder"]:
        say("ABORT: the ground for this clause is not there")
        return 2
    chars_before = characters()
    say(f"characters BEFORE the purge: {chars_before}")

    app, window, views, catalog_view = open_window()
    if not views:
        say("ABORT: no controller tab; run `seed` first")
        return 2
    rc = press_uninstall(app, window, views, catalog_view, keep=True)
    if rc:
        return rc

    after = ground("clause 1 TICKED, AFTER")
    say("--- the negatives and the one positive ---")
    say(f"folder gone: {not SERVER_DIR.exists()}")
    say(f"db volume KEPT: {DB_VOLUME in after['volumes']}")
    say(f"client-data volume gone: {PROJECT + '_client-data' not in after['volumes']}")
    say(f"yulon.local images gone: {'yulon.local' not in after['images']}")
    say(f"ac- containers gone: {'ac-' not in after['ps']}")

    say("-- proving the characters are still readable THROUGH the kept volume")
    say("   (the password a reinstall would resolve, asked of the app, not typed here)")
    from yulon.catalog.catalog import load_catalog
    from yulon.install_wiring import installer_for_app

    engine = installer_for_app(load_catalog().get(GAME))
    secrets = engine.resolve_secrets(SERVER_DIR)
    say(f"   engine.resolve_secrets({SERVER_DIR}).db_password is set: "
        f"{bool(secrets.db_password)}")
    say(sh("docker", "rm", "-f", "gate89a-db"))
    say(sh(
        "docker", "run", "-d", "--name", "gate89a-db",
        "-v", f"{DB_VOLUME}:/var/lib/mysql",
        "-e", f"MYSQL_ROOT_PASSWORD={secrets.db_password}",
        "mysql:8.4",
    ))
    ok = False
    for _ in range(120):
        probe = sh("docker", "exec", "gate89a-db", "mysql", "-uroot",
                   f"-p{secrets.db_password}", "-N", "-e", "select 1;")
        if probe.strip().endswith("1"):
            ok = True
            break
        time.sleep(2)
    say(f"   the kept volume opened with that password: {ok}")
    say("   " + sh(
        "docker", "exec", "gate89a-db", "mysql", "-uroot", f"-p{secrets.db_password}", "-N",
        "-e", "select count(*), min(name), max(name) from acore_characters.characters;",
    ))
    say("   first ten characters, read out of the volume that survived the purge:")
    print(sh(
        "docker", "exec", "gate89a-db", "mysql", "-uroot", f"-p{secrets.db_password}",
        "-e", "select guid,name,race,class,level from acore_characters.characters "
              "order by guid limit 10;",
    ), flush=True)
    say("   accounts that can log in to them:")
    print(sh(
        "docker", "exec", "gate89a-db", "mysql", "-uroot", f"-p{secrets.db_password}",
        "-e", "select count(*) as accounts from acore_auth.account;",
    ), flush=True)
    say(sh("docker", "rm", "-f", "gate89a-db"))
    say(f"characters BEFORE the purge were: {chars_before}")
    close_window(window)
    return 0


def stage2() -> int:
    """Unticked: nothing remains."""
    announce("8.9a clause 2: uninstall with Keep my characters UNTICKED")
    before = ground("clause 2 UNTICKED, BEFORE -- stack up, both volumes, folder, record")
    if DB_VOLUME not in before["volumes"] or not before["folder"]:
        say("ABORT: the ground for this clause is not there")
        return 2

    app, window, views, catalog_view = open_window()
    if not views:
        say("ABORT: no controller tab; run `seed` first")
        return 2
    rc = press_uninstall(app, window, views, catalog_view, keep=False)
    if rc:
        return rc

    after = ground("clause 2 UNTICKED, AFTER")
    say("--- the negative, proved by asking Docker, not by reading the report ---")
    say(f"no container:  ac- in `docker ps -a`: {'ac-' in after['ps']}")
    say(f"no volume:     {PROJECT} in `docker volume ls`: {PROJECT in after['volumes']}")
    say(f"no image:      yulon.local in `docker images`: {'yulon.local' in after['images']}")
    say(f"no folder:     {SERVER_DIR} exists: {SERVER_DIR.exists()}")
    from yulon.state import load_state

    st = load_state()
    say(f"no record:     state.json installs = "
        f"{[(i.game, str(i.server_dir)) for i in st.installs]}")
    say("raw `docker volume ls`:")
    print(sh("docker", "volume", "ls"), flush=True)
    say("raw `docker ps -a`:")
    print(sh("docker", "ps", "-a"), flush=True)
    say("raw `docker images`:")
    print(sh("docker", "images"), flush=True)
    close_window(window)
    return 0


STAGES = {
    "ground": lambda: (ground("on demand"), 0)[1],
    "seed": seed,
    "stage3": stage3,
    "stage4": stage4,
    "stage1": stage1,
    "stage2": stage2,
}


if __name__ == "__main__":
    which = sys.argv[1] if len(sys.argv) > 1 else "ground"
    say(f"### 8.9a gate driver, stage {which}, {datetime.now().astimezone().isoformat()} ###")
    say(f"### box: {os.uname().nodename}, tree: {HERE} ###")
    raise SystemExit(STAGES[which]())
