"""The 8.9b live gate: uninstall and purge, WoW Vanilla (CMaNGOS), m910q.

Usage:  ~/gate81b-venv/bin/python gate89b.py <stage>

    baseline    the machine at rest, before anything is removed
    unknown     a claim that will not read -> refused, twice, for two reasons
    unclaimed   a folder with no record at all -> refused
    running     a purge of a running server -> refused, naming its containers
    plan        the plan reads only: nothing on the machine moves
    ticked      Keep my characters: the volume survives, and so does its key
    key         THE BOX'S REASON TO EXIST -- the kept password opens the kept volume
    reinstall   a reinstall to the same folder is no longer refused, and finds them
    unticked    nothing remains: container, volume, image, network, folder, record

Subject: the throwaway install `setup89b.py` builds at `~/gate89b/server`, NOT
`~/vanilla-75b`. Read `setup89b.py`'s docstring for what is genuine about it and
what is copied; the short version is that every OBJECT is real and the bytes were
cloned rather than compiled.

**Why this box exists at all, when 8.9a is already ticked.** `wow-wotlk`'s
database password is a FIXED value in `catalog.json`, so a ticked purge there
keeps nothing and the copy-and-recall path in `yulon/dbsecret.py` was never
pressed. Every CMaNGOS entry GENERATES a password per install into
`<server_dir>/.db_password` -- inside the folder a ticked purge deletes -- and
`composegen.install_id()` is a digest of the absolute path, so the reinstall
comes back to the SAME volume. Without the copy, "Keep my characters" keeps a
database nothing can ever open. `stage_key` and `stage_reinstall` are that path,
pressed.

**Ground before every action.** Each stage records what is true before it acts
and refuses to run when its assertion is already the starting state. The
baseline file is the reference every later diff is taken against.

**Alive at every capture.** A screenshot of a dead server photographs exactly
like a refusal, so `alive()` prints `docker ps` beside every grab -- including in
the stages where the containers are SUPPOSED to be gone, which is the reading
that would otherwise be ambiguous.
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
SHOTS = HERE
GAME = "wow-vanilla"
CLONE = Path.home() / "gate89b" / "server"
ORIGINAL = Path.home() / "vanilla-75b"
SCRATCH = Path.home() / "gate89b" / "scratch"
FOLDER_TAR = Path.home() / "gate89b" / "server-before-purge.tar"
STATE_FILE = ".yulon-install.json"
CONFIG = Path.home() / ".local" / "share" / "yulon"


def say(message: str) -> None:
    print(f"[{time.strftime('%H:%M:%SZ', time.gmtime())}] {message}", flush=True)


def sh(*args: str) -> str:
    return subprocess.run(args, capture_output=True, text=True, check=False).stdout.strip()


def alive(where: str) -> None:
    rows = [r for r in sh("docker", "ps", "--format", "{{.Names}} {{.Status}}").splitlines()
            if r.startswith("vanilla-")]
    say(f"alive at {where}: {rows if rows else 'no vanilla-* container is running'}")


# --------------------------------------------------------------- the app


def entry():
    from yulon.catalog.catalog import load_catalog

    return load_catalog().get(GAME)


def services(server_dir: Path = CLONE):
    from yulon.ui.controller_view import ControllerServices

    return ControllerServices.for_entry(entry(), server_dir)


def view_for(made):
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    from PySide6.QtWidgets import QApplication

    from yulon.ui.controller_view import ControllerView
    from yulon.ui.widgets.job import run_inline

    app = QApplication.instance() or QApplication([])
    view = ControllerView(entry(), made, status_poll_ms=0, job_runner=run_inline)
    view.resize(1000, 700)
    view._gate_app = app
    view._tabs.setCurrentIndex(0)
    return view


def project(server_dir: Path = CLONE) -> str:
    from yulon.catalog import composegen

    return composegen.project_name(GAME, server_dir)


def volume(server_dir: Path = CLONE) -> str:
    return f"{project(server_dir)}_db-data"


def listing() -> dict[str, list[str]]:
    """Every Docker listing this box's definition of done names, plus the record."""
    from yulon import state as app_state

    return {
        "containers": sorted(sh("docker", "ps", "-a", "--format", "{{.Names}}").splitlines()),
        "volumes": sorted(sh("docker", "volume", "ls", "--format", "{{.Name}}").splitlines()),
        "images": sorted(sh("docker", "image", "ls", "--format",
                            "{{.Repository}}:{{.Tag}}").splitlines()),
        "networks": sorted(sh("docker", "network", "ls", "--format", "{{.Name}}").splitlines()),
        "record": sorted(str(i.server_dir) for i in app_state.load_state().installs),
    }


def save_baseline() -> None:
    (HERE / "00-baseline.json").write_text(
        json.dumps(listing(), indent=2), encoding="utf-8", newline="\n"
    )


def baseline() -> dict[str, list[str]]:
    return json.loads((HERE / "00-baseline.json").read_text(encoding="utf-8"))


def diff_against_baseline(label: str) -> dict[str, dict[str, list[str]]]:
    was, now = baseline(), listing()
    out: dict[str, dict[str, list[str]]] = {}
    for key in was:
        gone = sorted(set(was[key]) - set(now[key]))
        new = sorted(set(now[key]) - set(was[key]))
        out[key] = {"gone": gone, "new": new}
        say(f"{label}: {key}: gone={gone} new={new}")
    return out


def press_plan(view):
    """Press Uninstall... and hand back what the label says. Removes nothing."""
    view.show_uninstall_plan()
    return view.uninstall_label.text()


def press_uninstall(view, *, keep: bool) -> str:
    view.keep_characters_check.setChecked(keep)
    view.show_uninstall_plan()
    view.run_uninstall()
    return view.uninstall_label.text()


# ------------------------------------------------------------------ stages


def stage_baseline() -> None:
    alive("baseline")
    save_baseline()
    was = baseline()
    for key, rows in was.items():
        say(f"{key}: {rows}")
    say(f"clone project {project()} volume {volume()}")
    say(f"folder size   {sh('du', '-sh', str(CLONE))}")
    say(f".db_password  {sh('sha256sum', str(CLONE / '.db_password'))}")
    say(f"claim         {(CLONE / STATE_FILE).read_text(encoding='utf-8')}")
    assert volume() in was["volumes"], "the subject has no database volume"
    assert str(CLONE) in was["record"], "state.json does not name the subject"
    say(f"tarring the folder aside for the reinstall step -> {FOLDER_TAR}")
    subprocess.run(["tar", "cf", str(FOLDER_TAR), "-C", str(CLONE.parent), CLONE.name], check=True)
    say(f"tar: {sh('du', '-sh', str(FOLDER_TAR))}")
    say("BASELINE RECORDED")


def stage_unknown() -> None:
    """A claim that will not read is not permission, and the two reasons differ."""
    alive("unknown")
    claim = CLONE / STATE_FILE
    original = claim.read_text(encoding="utf-8")
    try:
        claim.write_text("", encoding="utf-8")
        say("ground: the claim file is now zero bytes")
        text = press_plan(view_for(services()))
        say(f"the tab says: {text}")
        assert "cannot read" in text and "Nothing was removed" in text, text
        assert CLONE.is_dir(), "the folder was touched by a refusal"

        claim.write_text(json.dumps({**json.loads(original), "version": 99}), encoding="utf-8")
        say("ground: the claim now says version 99 -- a record from a NEWER build")
        view = view_for(services())
        newer = press_plan(view)
        say(f"the tab says: {newer}")
        assert "cannot read" in newer, newer
        assert newer != text, "both UNKNOWNs gave the same advice; the reason is not carried"
        assert "99" in newer, "the refusal does not say the record is a newer build's"
        view.grab().save(str(SHOTS / "1-refused-unreadable-record.png"))
        alive("1-refused-unreadable-record.png")
    finally:
        claim.write_text(original, encoding="utf-8")
    say("UNKNOWN PASSED: refused twice, with two different reasons, nothing removed")


def stage_unclaimed() -> None:
    """A folder with no record at all is not Yu'lon's to delete."""
    alive("unclaimed")
    if SCRATCH.exists():
        shutil.rmtree(SCRATCH)
    SCRATCH.mkdir(parents=True)
    (SCRATCH / "docker-compose.yml").write_text("name: someone-elses\n", encoding="utf-8")
    (SCRATCH / "data").mkdir()
    say(f"ground: {SCRATCH} looks like a server and has no {STATE_FILE}")
    view = view_for(services(SCRATCH))
    text = press_plan(view)
    say(f"the tab says: {text}")
    assert "no install record" in text and "Nothing was removed" in text, text
    assert (SCRATCH / "docker-compose.yml").is_file(), "the refusal touched the folder"
    view.grab().save(str(SHOTS / "2-refused-no-record.png"))
    alive("2-refused-no-record.png")
    say("UNCLAIMED PASSED: refused, and the folder is still there")


def stage_running() -> None:
    """A purge of a running server refuses and says to stop it first."""
    alive("running")
    running = [r for r in sh("docker", "ps", "--format", "{{.Names}}").splitlines()
               if r.startswith("vanilla-")]
    assert len(running) == 3, f"ground: expected three containers up, saw {running}"
    say(f"ground: {running} are up")

    view = view_for(services())
    text = press_plan(view)
    say(f"the tab says: {text}")
    for name in ("vanilla-mangosd", "vanilla-realmd", "vanilla-db"):
        assert name in text, f"the refusal does not name {name}: {text}"
    assert view.uninstall_confirm_button.isHidden(), "a refusal still offered the button"
    view.grab().save(str(SHOTS / "3-refused-server-running.png"))
    alive("3-refused-server-running.png")

    still = [r for r in sh("docker", "ps", "--format", "{{.Names}}").splitlines()
             if r.startswith("vanilla-")]
    assert sorted(still) == sorted(running), f"the refusal changed the containers: {still}"
    say("RUNNING PASSED: refused by name, and all three are still up")


def stage_plan() -> None:
    """The plan reads only. Compared field by field, not eyeballed."""
    say("stopping the subject through the app's own controller, so a plan is possible")
    services().controller.stop()
    alive("plan")
    before = listing()
    view = view_for(services())
    text = press_plan(view)
    say("the tab says:")
    for line in text.splitlines():
        say(f"  | {line}")
    view.grab().save(str(SHOTS / "4-the-plan.png"))
    alive("4-the-plan.png")

    assert not view.uninstall_confirm_button.isHidden(), "a good plan hid its own button"
    assert str(CLONE) in text
    assert project() in text
    assert volume() in text
    after = listing()
    for key in before:
        assert before[key] == after[key], f"{key} changed while only planning: {before[key]} -> {after[key]}"
    say("PLAN PASSED: named the folder, the project and the volume, and moved nothing")


def stage_ticked() -> None:
    """Keep my characters: the volume survives and its password is copied out."""
    alive("ticked")
    from yulon import dbsecret
    from yulon.catalog import composegen

    secret_path = dbsecret.secret_path(GAME, composegen.install_id(CLONE))
    if secret_path.exists():
        secret_path.unlink()
    say(f"ground: no kept password at {secret_path}")
    was = listing()
    assert volume() in was["volumes"]
    (image,) = composegen.built_image_refs(entry(), CLONE)
    assert any(row.startswith(image.split(":")[0]) for row in was["images"])
    password = (CLONE / ".db_password").read_text(encoding="utf-8").strip()
    say(f"ground: the folder holds a {len(password)}-character password and the volume exists")

    view = view_for(services())
    said = press_uninstall(view, keep=True)
    say("the tab says:")
    for line in said.split(". "):
        say(f"  | {line}")
    view.grab().save(str(SHOTS / "5-ticked-what-was-kept.png"))
    alive("5-ticked-what-was-kept.png")

    now = listing()
    assert volume() in now["volumes"], "a TICKED purge removed the database volume"
    assert image not in now["images"], "the built image survived"
    assert "mariadb:11" in now["images"], "the SHARED database image was removed (defect 2)"
    assert not CLONE.exists(), "the folder is still there"
    assert str(CLONE) not in now["record"], "state.json still names the install"
    assert f"{project()}_vanilla-net" not in now["networks"], "the network survived (defect 6)"
    assert not [c for c in now["containers"] if c.startswith("vanilla-")], now["containers"]
    assert secret_path.is_file(), "the password that opens the kept volume was NOT copied out"
    say(f"the kept password file: {secret_path}")
    say(f"  {secret_path.read_text(encoding='utf-8').replace(password, '<the password>')}")
    assert str(ORIGINAL) in sh("ls", str(ORIGINAL.parent)) or ORIGINAL.is_dir()
    assert (ORIGINAL / ".db_password").is_file(), "the NEIGHBOUR install was touched"
    assert volume(ORIGINAL) in now["volumes"], "the neighbour's volume was removed"
    diff_against_baseline("ticked")
    say("TICKED PASSED: volume kept, key kept outside the folder, everything else gone")


def stage_key() -> None:
    """THE CLAUSE THIS BOX EXISTS FOR: the kept password opens the kept volume.

    8.9a proved a volume survived and that a file was written. It could not
    prove the file is USEFUL, because on a `fixed` family there is nothing to
    keep. Here the folder that held the only copy has been deleted, so this is
    the only thing standing between the user and a database nobody can open.

    A mariadb container is started on the kept volume with nothing but the
    recalled secret, and the characters are counted.
    """
    alive("key")
    from yulon import dbsecret
    from yulon.catalog import composegen

    kept = dbsecret.recall(GAME, composegen.install_id(CLONE))
    assert kept is not None, "there is no kept password; the volume is unopenable"
    say(f"recalled a password for volume {kept.volume!r}")
    assert kept.volume == volume(), f"the copy names {kept.volume}, not {volume()}"
    assert not CLONE.exists(), "ground: the folder that held the original is gone"
    assert not (CLONE / ".db_password").exists()

    say("starting a database on the KEPT volume with nothing but the recalled password")
    sh("docker", "rm", "-f", "gate89b-probe")
    sh("docker", "run", "-d", "--name", "gate89b-probe",
       "-v", f"{kept.volume}:/var/lib/mysql",
       "-e", f"MARIADB_ROOT_PASSWORD={kept.password}", "mariadb:11")
    counts = ""
    for _ in range(60):
        time.sleep(5)
        counts = sh("docker", "exec", "gate89b-probe", "mariadb", "-uroot",
                    f"-p{kept.password}", "-N", "-B", "-e",
                    "SELECT COUNT(*) FROM characters.characters; "
                    "SELECT COUNT(*) FROM realmd.account;")
        if counts:
            break
    say(f"the kept volume answered: characters={counts.splitlines()}")
    assert counts, "the recalled password did not open the kept volume"
    characters = int(counts.splitlines()[0])
    assert characters > 0, "the volume opened but holds no characters"
    say(f"WRONG-PASSWORD CONTROL: the same query with a wrong password")
    wrong = subprocess.run(
        ["docker", "exec", "gate89b-probe", "mariadb", "-uroot", "-pnot-the-password",
         "-N", "-B", "-e", "SELECT COUNT(*) FROM characters.characters;"],
        capture_output=True, text=True, check=False,
    )
    say(f"  exit={wrong.returncode} stderr={wrong.stderr.strip()[:120]}")
    assert wrong.returncode != 0, "a wrong password also worked; this proves nothing"
    sh("docker", "rm", "-f", "gate89b-probe")
    say(f"KEY PASSED: {characters} characters, reachable only with the password the purge kept")


def stage_reinstall() -> None:
    """The refusal a reinstall used to hit is gone, and the server comes back with them.

    Two halves, and the first is the one that would fail without `dbsecret`:
    `CmangosInstaller._db_password` refuses to mint a new password beside a
    volume that already exists. The folder is restored from the tar taken at
    baseline and `.db_password` is DELETED, which is exactly the state a
    reinstall to this folder starts from -- then the shipped stage body is run.

    **What this does not prove**, said here rather than left to be noticed: the
    folder came from a tar, not from a fresh clone-and-compile. What a real
    reinstall press would add is 8 stages of build time; what it would NOT
    change is the stage under test, which reads a password and a volume name.

    **And a finding the first run of this stage produced.** A TICKED purge
    removes the install's built image -- correctly, it is this install's alone --
    so `docker compose up` afterwards has nothing to run, and this stage hung on
    its readiness poll while compose tried to PULL from `yulon.local`, a registry
    that does not exist:

        failed to resolve reference "yulon.local/cmangos-vanilla-server:
        native-e9e233c0": dial tcp: lookup yulon.local: no such host

    So "reinstall to the same folder to find those characters again", the
    sentence the dialog prints, costs a full recompile on this tree even though
    every character survived. The image is retagged here for the same reason
    `setup89b.py` retags one -- the bytes are identical and the compile is the
    owner's to start -- and the cost is named in the README rather than hidden
    behind a green step.
    """
    alive("reinstall")
    from yulon.catalog import native
    from yulon.catalog.installer import InstallerError, installer_for

    assert not CLONE.exists(), "ground: the folder must be gone before it is restored"
    say(f"restoring the folder from {FOLDER_TAR}")
    subprocess.run(["tar", "xf", str(FOLDER_TAR), "-C", str(CLONE.parent)], check=True)
    (CLONE / ".db_password").unlink()
    say("deleted .db_password: this is the state a reinstall to this folder starts in")

    installer = installer_for(entry())
    secrets = installer.resolve_secrets(CLONE)
    ctx = native.StageContext(
        server_dir=CLONE,
        client_dir=None,
        state=native.read_state(CLONE, valid=[s.name for s in installer.stages()]),
        cancel=None,
        secrets=secrets,
    )
    try:
        for line in installer._db_password(ctx):
            say(f"  | {line}")
    except InstallerError as exc:
        raise SystemExit(f"THE REINSTALL IS STILL REFUSED: {exc}") from exc
    assert (CLONE / ".db_password").is_file(), "the stage wrote nothing back"
    say("the db-password stage accepted the kept copy and wrote the file back")

    from yulon.catalog import composegen

    (image,) = composegen.built_image_refs(entry(), CLONE)
    listed = sh("docker", "image", "ls", "--format", "{{.Repository}}:{{.Tag}}").splitlines()
    say(f"the built image {image} after the purge: {'GONE' if image not in listed else 'here'}")
    assert image not in listed, "ground: the purge was supposed to have removed this image"
    (source,) = composegen.built_image_refs(entry(), ORIGINAL)
    say(f"retagging {source} -> {image} INSTEAD of recompiling (see this stage's docstring)")
    sh("docker", "tag", source, image)

    say("bringing the install up again on the volume the purge kept")
    sh("docker", "compose", "-f", str(CLONE / "docker-compose.yml"),
       "-f", str(CLONE / "docker-compose.override.yml"), "up", "-d")
    mark = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(time.time() - 5))
    for _ in range(360):
        if "Avg Diff:" in sh("docker", "logs", "--since", mark, "vanilla-mangosd"):
            break
        time.sleep(5)
    else:
        raise SystemExit("the reinstalled server did not come up")
    alive("after the reinstall")

    from yulon import state as app_state

    loaded = app_state.load_state()
    loaded.remember(app_state.KnownInstall(game=GAME, server_dir=CLONE))
    app_state.save_state(loaded)

    made = services()
    page = made.bots.page()
    say(f"the app's Bots tab counts {page.total} bot characters on the recovered database")
    view = view_for(made)
    view._tabs.setCurrentIndex(
        next(i for i in range(view._tabs.count()) if view._tabs.tabText(i) == "Characters")
    )
    view.refresh_characters()
    view.grab().save(str(SHOTS / "6-characters-after-the-reinstall.png"))
    alive("6-characters-after-the-reinstall.png")
    say(f"the Characters tab lists {view.character_list.count()} row(s)")
    assert view.character_list.count() > 0, "the recovered database shows no characters"
    say("REINSTALL PASSED: not refused, came up on the kept volume, characters are there")


def stage_unticked() -> None:
    """Unticked: no container, volume, image, network, folder or record remains."""
    alive("unticked")
    was = listing()
    assert CLONE.is_dir(), "ground: there must be an install to remove"
    assert volume() in was["volumes"], "ground: there must be a volume to remove"
    from yulon.catalog import composegen

    (image,) = composegen.built_image_refs(entry(), CLONE)
    assert any(row == image for row in was["images"]), f"ground: {image} is not there to remove"
    net = f"{project()}_vanilla-net"
    say(f"ground: folder, {volume()}, {image}, {net} and the record all exist")

    say("stopping the server first -- a purge of a running one refuses, which stage 3 proved")
    services().controller.stop()
    view = view_for(services())
    said = press_uninstall(view, keep=False)
    say("the tab says:")
    for line in said.split(". "):
        say(f"  | {line}")
    view.grab().save(str(SHOTS / "7-unticked-nothing-remains.png"))
    alive("7-unticked-nothing-remains.png")

    now = listing()
    assert not CLONE.exists(), "the folder remains"
    assert volume() not in now["volumes"], "the database volume remains"
    assert image not in now["images"], "the built image remains"
    assert net not in now["networks"], "the network remains"
    assert str(CLONE) not in now["record"], "state.json still names it"
    assert not [c for c in now["containers"] if c.startswith("vanilla-")], now["containers"]
    say("and what must NOT have gone:")
    assert "mariadb:11" in now["images"], "the shared database image was removed"
    assert volume(ORIGINAL) in now["volumes"], "the neighbour's volume was removed"
    assert ORIGINAL.is_dir() and (ORIGINAL / "data").is_dir(), "the neighbour folder was touched"
    tbc = "yulon-wow-tbc-37f13213_db-data"
    assert tbc in now["volumes"], "the TBC install's volume was removed"
    diff_against_baseline("unticked")
    say("UNTICKED PASSED: nothing of this install remains, and the neighbours are intact")


STAGES = {
    "baseline": stage_baseline,
    "unknown": stage_unknown,
    "unclaimed": stage_unclaimed,
    "running": stage_running,
    "plan": stage_plan,
    "ticked": stage_ticked,
    "key": stage_key,
    "reinstall": stage_reinstall,
    "unticked": stage_unticked,
}

if __name__ == "__main__":
    if len(sys.argv) != 2 or sys.argv[1] not in STAGES:
        raise SystemExit(f"usage: python gate89b.py <{'|'.join(STAGES)}>")
    STAGES[sys.argv[1]]()
