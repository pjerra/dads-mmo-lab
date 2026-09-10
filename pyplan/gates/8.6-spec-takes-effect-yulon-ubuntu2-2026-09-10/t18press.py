"""T18 live half -- deploying `playerbots.conf` through the app, and a spec press.

Runs ON `yulon-ubuntu2` against `~/wowserver`, out of a copy of the T18
worktree at `~/t18-live`, so every press is THIS ticket's code and not the
box clone's older tree.

`build()` is T13's `t13press.py:build()`, which is 8.6's own `gate86b.build()`
-- the three objects `controller_view._for_wotlk` assembles for the running
app, from the same inputs. `modules()` is the other half and is new here: the
`Applier` that factory hands the Modules tab (`controller_view.py:1095-1108`),
built by the same `wotlk_modules.applier(...)` call with the same
`world_running` and `start_database` seams, so the activation below is pressed
against the applier the tab presses and not a second one.

Nothing here writes a command string of its own. `deploy` calls
`party.deploy`, `spec` calls `party.spec_command`, `activate*` call
`wotlk_modules.install_custom(applier)` -- the Modules tab's own custom-install
seam -- and `stop`/`start` call `docker.stop_staged`/`docker.start_staged`,
which are the Server tab's Stop and Start.

    liveness                     container status, one line
    ground [name...]             facts, preconditions, group rows per name
    deploy                       party.deploy(lua_root(), dest_dir) -- the app's
                                 own "Enable My Party" deploy seam
    sql <statement>              one read against a schema of this install
    send <command...>            one command over the seam's own channel
    whisper <master> <bot> <..>  dml_whisper, built by party._whisper's shape
    spec <master> <bot> <name>   party.spec_command(...) over that channel
    specs [class]                InstallParty.specs() -- what the picker offers
    conf                         where the app looks for the deployed conf, and
                                 what read_spec_names() finds in it
    activate-folder              the Modules tab's folder route, verbatim:
                                 derive_folder(clone) then install_custom(m, folder)
    activate                     install_custom(m, None) -- the same seam with
                                 no copy, because the bytes are already at the
                                 clone path
    forget                       drop the derived manifest from the user layer
    stop / start                 the app's own Stop and Start
    frame <master> <class> <png> the shipped PartyPanel, offscreen, grabbed
"""

from __future__ import annotations

import sys
from pathlib import Path

TREE = Path(__file__).resolve().parents[3]  # .../t18-live
sys.path.insert(0, str(TREE / "pylauncher"))

from yulon import channel_setup, dbreads, docker, party, resources, useraccounts  # noqa: E402
from yulon.catalog import composegen  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.controller_wow_wotlk import accounts as wotlk_accounts  # noqa: E402
from yulon.controller_wow_wotlk import modules as wotlk_modules  # noqa: E402
from yulon.ui import controller_view  # noqa: E402

SERVER = Path.home() / "wowserver"
ENTRY = load_catalog().get("wow-wotlk")
MODULE_CLONE = SERVER / "modules" / "mod-playerbots"


def build() -> tuple[party.InstallParty, useraccounts.InstallAccounts, object]:
    """The same three objects `_for_wotlk` assembles, from the same inputs."""
    spec = ENTRY.container_spec()
    password = controller_view._db_password(ENTRY, SERVER)  # noqa: SLF001
    sql = controller_view._sql_for(ENTRY, password, wsl_distro=None)  # noqa: SLF001
    install_id = composegen.install_id(SERVER)
    channel = channel_setup.InstallChannel(
        ENTRY,
        SERVER,
        templates_root=resources.installers_dir(),
        install_id=install_id,
        create=lambda name, pw, level: wotlk_accounts.create_account(
            sql, name, pw, gm_level=level, scheme=ENTRY.accounts.scheme or "azerothcore"
        ),
        reset=lambda name, pw: wotlk_accounts.reset_own_password(sql, name, pw),
        channel_for=lambda endpoint: __import__(
            "yulon.channel", fromlist=["SoapChannel"]
        ).SoapChannel(
            endpoint=endpoint,
            state_of=lambda: docker.container_state(spec.world),
        ),
    )
    seam = party.InstallParty(
        ENTRY,
        SERVER,
        sql=sql,
        channel_for_saved=channel.live_channel,
        container=spec.world,
        world_running=lambda: docker.container_state(spec.world).settled,
        link_writer=sql,
    )
    accounts = useraccounts.InstallAccounts(
        ENTRY,
        SERVER,
        sql=sql,
        channel_for_saved=channel.live_channel,
        app_account=channel_setup.account_name(install_id),
    )
    return seam, accounts, sql


def modules() -> object:
    """The `Applier` the Modules tab holds -- `controller_view.py:1095-1108`.

    The same call, the same two seams. `client_dir=None` because this install
    has no client registered and the conf step does not read one; a DIFFERENT
    applier is never built for the activation, because a custom module must not
    be installed against a second one (`install_custom`'s own docstring).
    """
    spec = ENTRY.container_spec()
    password = controller_view._db_password(ENTRY, SERVER)  # noqa: SLF001
    sql = controller_view._sql_for(ENTRY, password, wsl_distro=None)  # noqa: SLF001
    return wotlk_modules.applier(
        SERVER,
        sql=sql,
        client_dir=None,
        world_running=lambda: docker.world_running(spec.world, wsl_distro=None),
        start_database=lambda: docker.start_database(
            spec, SERVER, because="no SQL was run", wsl_distro=None
        ),
    )


def liveness() -> str:
    spec = ENTRY.container_spec()
    state = docker.container_state(spec.world)
    return (
        f"worldserver {state.status or 'unknown'} "
        f"started={state.started_at} restarts={state.restart_count}"
    )


CHARS = "acore_characters"


def _rows(sql: object, statement: str) -> str:
    return sql.query("characters", statement).strip()  # type: ignore[attr-defined]


def _report(report: object) -> None:
    print(f"report.action  = {report.action!r}")  # type: ignore[attr-defined]
    print(f"report.item    = {report.item_id!r}")  # type: ignore[attr-defined]
    for line in report.done:  # type: ignore[attr-defined]
        print(f"    done    : {line}")
    for line in report.skipped:  # type: ignore[attr-defined]
        print(f"    skipped : {line}")
    for pending in report.pending_sql:  # type: ignore[attr-defined]
        print(f"    pending : {pending}")
    print(f"report.rebuild_required     = {report.rebuild_required}")  # type: ignore[attr-defined]
    print(f"report.restart_recommended  = {report.restart_recommended}")  # type: ignore[attr-defined]


def _derived() -> object:
    """The manifest the Modules tab's "add a module from a folder" would derive."""
    manifest = wotlk_modules.derive_folder(MODULE_CLONE)
    print(f"derive_folder({MODULE_CLONE}) -> id={manifest.id!r} type={manifest.type!r}")
    print(f"    source={manifest.source!r}")
    print(f"    origin={manifest.origin!r}")
    return manifest


def main() -> int:  # noqa: C901, PLR0911, PLR0912, PLR0915 - one verb per branch
    args = sys.argv[1:]
    if not args:
        print(__doc__)
        return 2
    verb = args[0]

    if verb == "liveness":
        print(liveness())
        return 0

    if verb == "deploy":
        root = party.lua_root()
        dest = party.dest_dir(SERVER)
        print(f"lua_root  = {root}")
        print(f"dest_dir  = {dest}")
        result = party.deploy(root, dest)
        print(f"deploy -> changed={result.changed} names={result.names}")
        return 0

    if verb == "conf":
        # Where the app looks, and what it finds -- `party.PLAYERBOTS_CONF` and
        # `party.spec_names()`, the two the picker is built out of.
        target = SERVER / party.PLAYERBOTS_CONF
        print(f"party.PLAYERBOTS_CONF = {party.PLAYERBOTS_CONF}")
        print(f"deployed conf         = {target}")
        print(f"exists                = {target.exists()}")
        dist = target.with_suffix(".conf.dist")
        print(f"template beside it    = {dist}  exists={dist.exists()}")
        names = party.spec_names(SERVER)
        print(f"read_spec_names -> {len(names)} class ids with a contiguous run from 0")
        for klass, listed in sorted(names.items()):
            print(f"    class {klass:>2}: {listed}")
        marker = dbreads.resolve_marker(ENTRY, SERVER)
        print(f"dbreads.resolve_marker -> {marker.marker} {marker.problem}")
        return 0

    if verb == "activate-folder":
        # The Modules tab's folder route exactly as the tab presses it: the
        # manifest the view derives, and the seam the view calls with the folder
        # the user chose. Whatever comes back -- report or refusal -- is the
        # finding.
        manifest = _derived()
        install = wotlk_modules.install_custom(modules())
        try:
            report = install(manifest, MODULE_CLONE)
        except Exception as exc:  # noqa: BLE001 - the refusal IS the measurement
            print(f"REFUSED {type(exc).__name__}: {exc}")
            return 3
        _report(report)
        return 0

    if verb == "activate":
        manifest = _derived()
        install = wotlk_modules.install_custom(modules())
        try:
            report = install(manifest, None)
        except Exception as exc:  # noqa: BLE001
            print(f"REFUSED {type(exc).__name__}: {exc}")
            return 3
        _report(report)
        return 0

    if verb == "forget":
        manifest = wotlk_modules.derive_folder(MODULE_CLONE)
        print(f"forget({manifest.id!r}) -> {wotlk_modules.forget(manifest)}")
        return 0

    if verb in ("stop", "start"):
        spec = ENTRY.container_spec()
        if verb == "stop":
            print(f"stop_staged -> {docker.stop_staged(spec, SERVER)}")
        else:
            print(f"start_staged -> {docker.start_staged(spec, SERVER)}")
        print(liveness())
        return 0

    if verb == "frame":
        return _frame(args[1], args[2], Path(args[3]))

    seam, _accounts, sql = build()
    print(liveness())

    if verb == "ground":
        facts = seam.facts()
        print(f"facts: {facts}")
        print(f"ready: {party.ready(facts)}  blocker: {party.blocker(facts)}")
        state = seam.state(args[1]) if len(args) > 1 else None
        if state is not None:
            for check in state.checks:
                print(f"    {'ok ' if check.met else 'NO '}{check.name}")
        for name in args[1:]:
            print(f"{name}: online guid = {seam.online_guid(name)}")
            print(f"{name}: seam.members -> {seam.members(name)}")
        return 0

    if verb == "specs":
        classes = args[1:] or list(party.BOT_CLASSES)
        for klass in classes:
            print(f"seam.specs({klass!r}) = {seam.specs(klass)}")
        print(f"seam.max_level() = {seam.max_level()}")
        return 0

    if verb == "sql":
        print(_rows(sql, args[1]))
        return 0

    if verb == "grouprows":
        print(
            _rows(
                sql,
                "SELECT gm.guid, gm.memberGuid, c.name, gm.memberFlags, gm.subgroup, gm.roles "
                f"FROM {CHARS}.group_member gm JOIN {CHARS}.characters c "
                "ON c.guid = gm.memberGuid ORDER BY gm.guid, gm.memberGuid;",
            )
        )
        return 0

    channel = seam._channel_for_saved()  # noqa: SLF001 - inside the seam's own house
    if channel is None:
        print("no channel")
        return 1

    if verb == "spec":
        command = party.spec_command(args[1], args[2], " ".join(args[3:]))
    elif verb == "whisper":
        command = f"dml_whisper {args[1]} {args[2]} " + " ".join(args[3:])
    elif verb == "send":
        command = " ".join(args[1:])
    else:
        print(f"unknown: {verb}")
        return 2

    print(f"COMMAND: {command}")
    answer = channel.send(command)
    print(f"outcome={answer.outcome} reason={answer.reason!r}")
    print(f"text={answer.text!r}")
    return 0


def _frame(master: str, klass: str, out: Path) -> int:
    """The shipped `PartyPanel`, built offscreen, with its picker open on `klass`.

    Offscreen (`QT_QPA_PLATFORM=offscreen`) rather than on the VM's desktop: T5
    photographed the desktop through the Hyper-V console, and this run has no
    console to photograph. The widget, its seam and its job runner are the
    shipped ones -- the two lines are `_build_my_party_group`'s own -- so what
    the frame shows is what the tab shows; only the surface it is painted on
    differs, and the file says so.
    """
    import time

    from PySide6.QtWidgets import QApplication, QGroupBox, QMainWindow, QVBoxLayout, QWidget

    from yulon.ui.widgets.job import threaded_job_runner
    from yulon.ui.widgets.party_panel import PartyPanel

    def settle(seconds: float) -> None:
        """Spin the real event loop, because the panel reads its specs on a THREAD.

        `PartyPanel` loads the spec list through the job runner the tab hands it,
        and drops a reading whose class no longer matches ("dropped a stale spec
        reading"). A single `processEvents()` after `setCurrentIndex` therefore
        grabs an empty picker: the job has not come back yet. This waits the way
        a person does.
        """
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            QApplication.processEvents()
            time.sleep(0.02)

    seam, _accounts, _sql = build()
    app = QApplication(sys.argv[:1])
    window = QMainWindow()
    window.setWindowTitle("Yulon - My Party (T18: the picker after the conf was deployed)")
    holder = QWidget(window)
    outer = QVBoxLayout(holder)
    group = QGroupBox("My Party", holder)
    inside = QVBoxLayout(group)
    panel = PartyPanel(seam, jobs=threaded_job_runner(window), parent=group)
    inside.addWidget(panel)
    outer.addWidget(group)
    window.setCentralWidget(holder)
    window.setGeometry(0, 0, 1020, 752)
    window.show()
    settle(1.0)

    panel.character.setText(master)
    index = panel.klass.findText(klass)
    if index >= 0:
        panel.klass.setCurrentIndex(index)
    settle(4.0)

    offered = [panel.spec.itemText(i) for i in range(panel.spec.count())]
    print(f"character = {panel.character.text()!r}")
    print(f"class     = {panel.klass.currentText()!r}")
    print(f"spec picker offers {len(offered)} rows: {offered}")
    print(f"spec currentText  = {panel.spec.currentText()!r}")
    print(f"level max         = {panel.level.maximum()}")
    print(f"summary           = {panel.summary.text()!r}")

    out.parent.mkdir(parents=True, exist_ok=True)

    # Two frames, because a closed combo box shows one row and the claim is
    # about the whole list. The popup is opened and the VIEW is grabbed rather
    # than its window: an offscreen popup window has no size to paint into (a
    # 100-byte PNG, measured on this box), while the view inside it has the rows.
    panel.spec.showPopup()
    settle(1.0)
    view = panel.spec.view()
    rows = view.model().rowCount()
    view.resize(view.sizeHintForColumn(0) + 24, max(1, rows) * view.sizeHintForRow(0) + 8)
    settle(0.5)
    popup_path = out.with_name(out.stem + "-list.png")
    print(f"popup rows={rows} view size={view.width()}x{view.height()}")
    ok2 = view.grab().save(str(popup_path))
    print(f"list grab -> {popup_path} saved={ok2}")
    panel.spec.hidePopup()
    settle(0.5)

    ok = window.grab().save(str(out))
    print(f"grab -> {out} saved={ok}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
