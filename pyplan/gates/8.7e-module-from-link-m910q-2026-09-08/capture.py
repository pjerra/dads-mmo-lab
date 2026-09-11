"""Photograph the Modules tab's two new buttons on m910q (lane C, 8.7e).

Run from the remote checkout's `pylauncher/` with `QT_QPA_PLATFORM=offscreen`
and an output directory as `argv[1]`. Every frame logs the process PID and
whether it is still alive at the moment of the grab, into `alive.txt`, because
a Qt process that has died photographs like a refusal.

WHAT THIS CAN AND CANNOT PROVE, stated here rather than in a README nobody
opens beside the pictures: lanes A and B of the design are on no branch, so
this app cannot clone or copy a module from a user-supplied source. Frames 1-3
are the SHIPPED wiring, driven through `ControllerServices.for_entry()` -- the
real object the real app builds. Frames 5-8 are driven through the test
suite's own fakes and photograph the VIEW, not an install: no repository was
cloned, no folder copied, no conf activated and no SQL reported.
"""

from __future__ import annotations

import json
import os
import sys
import time
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

from PySide6.QtCore import QTimer  # noqa: E402
from PySide6.QtWidgets import QApplication  # noqa: E402

OUT = Path(sys.argv[1])
OUT.mkdir(parents=True, exist_ok=True)
PID = os.getpid()
ALIVE = OUT / "alive.txt"


def alive() -> str:
    try:
        os.kill(PID, 0)
        return "alive"
    except OSError:
        return "DEAD"


def shot(widget: object, name: str, note: str) -> None:
    """Grab one widget to `<name>.png` and log the process's own liveness."""
    pix = widget.grab()  # type: ignore[attr-defined]
    path = OUT / f"{name}.png"
    pix.save(str(path))
    stamp = time.strftime("%Y-%m-%dT%H:%M:%S%z")
    with ALIVE.open("a", encoding="utf-8") as fh:
        fh.write(f"{stamp} pid={PID} {alive()} {path.name} {pix.width()}x{pix.height()} {note}\n")
    print(f"  {path.name}  {pix.width()}x{pix.height()}  pid {PID} {alive()}")


def main() -> int:
    app = QApplication([])
    from yulon.catalog.catalog import load_catalog
    from yulon.ui.controller_view import (
        MODULE_CUSTOM_NO_ROUTE,
        MODULE_FOLDER_BUTTON_LABEL,
        MODULE_LINK_BUTTON_LABEL,
        MODULE_LINK_DIALOG_TITLE,
        ControllerServices,
        ControllerView,
        ask_module_link,
    )

    wotlk = load_catalog().get("wow-wotlk")
    server_dir = Path(sys.argv[2])
    server_dir.mkdir(parents=True, exist_ok=True)

    # ---------------------------------------------------------------- ground
    services = ControllerServices.for_entry(wotlk, server_dir)
    seams = {
        name: getattr(services, name, "MISSING FIELD")
        for name in (
            "module_from_link",
            "module_from_folder",
            "module_install_custom",
            "module_forget",
            "store",
            "applier",
            "module_updates",
            "module_sql",
        )
    }
    readable = {k: ("None" if v is None else type(v).__name__) for k, v in seams.items()}
    (OUT / "seams.json").write_text(json.dumps(readable, indent=2) + "\n", encoding="utf-8")
    print("for_entry() seams:", json.dumps(readable))

    view = ControllerView(wotlk, services, status_poll_ms=0)
    view.resize(1100, 760)
    view._tabs.setCurrentIndex(_modules_tab(view))
    view.show()
    app.processEvents()
    shot(view, "2-modules-tab-buttons-dead", "shipped for_entry wiring; no custom-module route")

    assert view.module_link_button.text() == MODULE_LINK_BUTTON_LABEL
    assert view.module_folder_button.text() == MODULE_FOLDER_BUTTON_LABEL
    assert not view.module_link_button.isEnabled()
    assert not view.module_folder_button.isEnabled()
    assert view.module_link_button.toolTip() == MODULE_CUSTOM_NO_ROUTE
    (OUT / "3-dead-buttons.txt").write_text(
        "\n".join(
            [
                f"link button   label={view.module_link_button.text()!r}",
                f"link button   enabled={view.module_link_button.isEnabled()}",
                f"link button   tooltip={view.module_link_button.toolTip()!r}",
                f"folder button label={view.module_folder_button.text()!r}",
                f"folder button enabled={view.module_folder_button.isEnabled()}",
                f"folder button tooltip={view.module_folder_button.toolTip()!r}",
            ]
        )
        + "\n",
        encoding="utf-8",
    )

    # ------------------------------------------- the real dialog, mid-modal
    grabbed: list[str] = []

    def snap_modal() -> None:
        dialog = QApplication.activeModalWidget()
        if dialog is None:
            grabbed.append("no modal widget was up")
            return
        shot(dialog, "4-link-dialog", "the real QInputDialog.getText, photographed mid-modal")
        grabbed.append(dialog.windowTitle())
        dialog.reject()

    QTimer.singleShot(400, snap_modal)
    answer = ask_module_link(view, MODULE_LINK_DIALOG_TITLE)
    (OUT / "4-link-dialog.txt").write_text(
        f"activeModalWidget title: {grabbed}\nask_module_link returned: {answer!r} "
        "(rejected dialog == cancel == None)\n",
        encoding="utf-8",
    )

    # --------------------------------------- the view, driven through fakes
    sys.path.insert(0, str(Path.cwd()))
    import tests.test_controller_view as T
    from yulon import runner
    from yulon.ui import controller_view as cv
    from yulon.ui.widgets.job import run_inline

    cv.threaded_job_runner = lambda parent: run_inline
    ps = T._Ps()
    runner.run = ps
    faked = T._services(ps, server_dir, [])
    route = T._with_custom_route(faked)
    fake_view = ControllerView(
        wotlk,
        faked,
        status_poll_ms=0,
        link_asker=lambda parent, title: "https://github.com/azerothcore/mod-eluna",
        folder_asker=lambda parent, title: server_dir / "mod-hand-made",
    )
    fake_view.resize(1100, 760)
    fake_view._tabs.setCurrentIndex(_modules_tab(fake_view))
    fake_view.show()
    app.processEvents()
    shot(fake_view, "5-buttons-live-when-wired", "the same two buttons with a route wired")

    fake_view.install_module_from_link()
    _show_row(fake_view, "mod-eluna")
    app.processEvents()
    shot(fake_view, "6-link-report-and-custom-row", "FAKE route: nothing was cloned")

    (server_dir / "mod-hand-made").mkdir(exist_ok=True)
    fake_view.install_module_from_folder()
    _show_row(fake_view, "mod-hand-made")
    app.processEvents()
    shot(fake_view, "7-folder-report-and-custom-row", "FAKE route: nothing was copied")

    # The route is wired BEFORE the view is built, or the frame would show two
    # greyed buttons beside a report that a press produced -- a picture that
    # contradicts itself, which is the class of artefact 8.2d shipped once.
    bad_services = T._services(ps, server_dir, [])
    bad = T._with_custom_route(
        bad_services,
        refusal=(
            "The repository is named 'tools', and a custom module must be named "
            "mod-<something> in lowercase letters, digits and hyphens -- for example "
            "https://github.com/you/mod-my-thing. Nothing on this machine was changed."
        ),
    )
    refused = ControllerView(
        wotlk,
        bad_services,
        status_poll_ms=0,
        link_asker=lambda parent, title: "https://github.com/you/tools",
    )
    refused.resize(1100, 760)
    refused._tabs.setCurrentIndex(_modules_tab(refused))
    refused.show()
    refused.install_module_from_link()
    app.processEvents()
    shot(refused, "8-refused-not-mod", "FAKE route: the refusal sentence, nothing queued")

    (OUT / "fake-route-calls.json").write_text(
        json.dumps(
            {
                "derived_from": [str(x) for x in route.derived_from],
                "installed": [[i, str(f)] for i, f in route.installed],
                "refused_route_installed": [[i, str(f)] for i, f in bad.installed],
            },
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )
    print("done")
    return 0


def _show_row(view: object, item_id: str) -> None:
    """Select and scroll to the row for `item_id`, so the frame shows its subject.

    The custom row is APPENDED after the 21 shipped ones (a user layer follows
    the bundled index), so a list left at the top photographs a claim it does
    not contain.
    """
    listing = view.module_list  # type: ignore[attr-defined]
    rows = [i for i in range(listing.count()) if listing.item(i).data(256) == item_id]
    assert rows, f"{item_id} is in no row after the install"
    listing.setCurrentRow(rows[0])
    listing.scrollToItem(listing.item(rows[0]))


def _modules_tab(view: object) -> int:
    tabs = view._tabs  # type: ignore[attr-defined]
    for i in range(tabs.count()):
        if tabs.tabText(i) == "Modules":
            return i
    raise AssertionError("no Modules tab")


if __name__ == "__main__":
    raise SystemExit(main())
