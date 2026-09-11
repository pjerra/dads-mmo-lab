"""The two-mail clause, with the real client in the loop.

8.4c pressed the gear-set button at twelve pieces and at three, and both
promises held. What it could NOT do is make a character wear exactly two: the
app has no unequip, and writing inventory rows by hand would be the gate
inventing its own evidence.

The client can. Unequipping a shirt is an ordinary player action, and it is the
only way the ground for this clause gets made honestly -- so the sequence is

    ground <name>   read the button BEFORE the client touches anything
    (a person unequips one piece in the client)
    ground <name>   read it again: the number the CLIENT changed
    press  <name>   press it, and count what arrives

`ground` runs `saveall` first. For an ONLINE character the row is not the truth
until the world is asked to write it down -- 8.4c measured that on this same
server (`character level` answered 60 while the row still read 6), and it is
exactly as true of an unequip: the piece leaves the player object at once and
the `character_inventory` row the app reads follows only at the next save.

Usage:  python two-mails.py <ground|press> <character name>
"""

from __future__ import annotations

import os
import sys
import time
from datetime import UTC, datetime
from pathlib import Path

sys.path.insert(0, str(Path.home() / "gate84c" / "pylauncher"))

from yulon import commands  # noqa: E402,F401
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui.controller_view import ControllerServices  # noqa: E402

SERVER_DIR = Path.home() / "vanilla-75b"
ENTRY = load_catalog().get("wow-vanilla")
SHOTS = Path.home() / "gate84c-client-shots"


def stamp() -> str:
    return datetime.now(UTC).strftime("%H:%M:%SZ")


def say(text: str) -> None:
    print(f"[{stamp()}] {text}", flush=True)


def services() -> ControllerServices:
    return ControllerServices.for_entry(ENTRY, SERVER_DIR)


def channel():
    setup = services().channel_setup
    live = setup.live_channel()
    assert live is not None, "no live command channel"
    return live


def sql(db: str, statement: str) -> str:
    from yulon.ui import controller_view as view_module

    reader = view_module._sql_for(
        ENTRY, view_module._db_password(ENTRY, SERVER_DIR), wsl_distro=None
    )
    return reader.query(db, statement).strip()


def schemas() -> dict[str, str]:
    return ENTRY.schema_map()


def mails(name: str) -> int:
    return int(
        sql(
            "characters",
            f"SELECT COUNT(*) FROM {schemas()['characters']}.mail m "
            f"JOIN {schemas()['characters']}.characters c ON c.guid = m.receiver "
            f"WHERE c.name = '{name}';",
        )
        or 0
    )


def worn_rows(name: str) -> str:
    """What the DATABASE says is on the character, slot by slot.

    Read beside the app's own answer so that "the app says two" and "the server
    says two" are two readings and not one.
    """
    return sql(
        "characters",
        "SELECT GROUP_CONCAT(CONCAT(i.slot, ':', i.item_template) ORDER BY i.slot) "
        f"FROM {schemas()['characters']}.character_inventory i "
        f"JOIN {schemas()['characters']}.characters c ON c.guid = i.guid "
        f"WHERE c.name = '{name}' AND i.bag = 0 AND i.slot < 19;",
    )


def button_label(name: str, shot: str | None = None) -> str:
    """The gear button's own text, off the real view -- not `gear_set_size`.

    The promise a person reads is drawn by `ControllerView`, and 8.4c found a
    defect that lived entirely in the gap between the two (a dead statement the
    view turned into "is wearing nothing"). So the reading is the label.
    """
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    from PySide6.QtWidgets import QApplication

    from yulon.ui.controller_view import ControllerView
    from yulon.ui.widgets.job import run_inline

    app = QApplication.instance() or QApplication([])
    view = ControllerView(ENTRY, services(), status_poll_ms=0, job_runner=run_inline)
    view.resize(980, 700)
    view._tabs.setCurrentIndex(
        next(i for i in range(view._tabs.count()) if view._tabs.tabText(i) == "Characters")
    )
    view.refresh_characters()
    hit = [
        i
        for i in range(view.character_list.count())
        if view.character_list.item(i).text().startswith(name)
    ]
    assert hit, f"{name} is not in the app's character list"
    view.character_list.setCurrentRow(hit[0])
    label = view.send_gear_button.text()
    if shot:
        SHOTS.mkdir(parents=True, exist_ok=True)
        view.grab().save(str(SHOTS / shot))
        say(f"photographed {SHOTS / shot}")
    del app
    return label


def stage_ground(name: str, shot: str | None) -> None:
    answer = channel().send("saveall")
    say(f"saveall   : {answer.outcome} {(answer.text or '').strip()[:60]!r}")
    time.sleep(2)
    say(f"the server's own inventory rows for {name}: {worn_rows(name)}")
    pieces, promised = services().play.gear_set_size(name)
    say(f"the app says {name} wears {pieces} pieces and the button promises {promised} mails")
    say(f"the button reads: {button_label(name, shot)!r}")
    say(f"{name} has {mails(name)} mails right now")


def stage_press(name: str) -> None:
    pieces, promised = services().play.gear_set_size(name)
    label = button_label(name, "press-promise.png")
    had = mails(name)
    say(f"BEFORE the press: {pieces} pieces, promise {promised}, mails {had}")
    say(f"the button reads: {label!r}")
    assert pieces > 1, f"{name} wears {pieces} pieces, which fits in one mail and proves nothing"
    assert promised == pieces, f"{pieces} pieces on a cap-of-one tree is {pieces} mails, not {promised}"
    tab = services().play
    out = tab.send_gear_set(
        name, to=name, subject="Your gear", body="Everything you were wearing"
    )
    say(f"send gear : done={out.done} {(out.text or out.problem).strip()[:160]!r}")
    deadline = time.monotonic() + 40
    now = had
    while time.monotonic() < deadline:
        now = mails(name)
        if now >= had + promised:
            break
        time.sleep(0.5)
    say(f"mails {had} -> {now}; arrived {now - had}, promised {promised}")
    assert out.done, out.problem
    assert now - had == promised, f"promised {promised}, {now - had} arrived"
    fullest = sql(
        "characters",
        f"SELECT MAX(n) FROM (SELECT COUNT(*) AS n FROM {schemas()['characters']}.mail_items mi "
        f"JOIN {schemas()['characters']}.mail m ON m.id = mi.mail_id "
        f"JOIN {schemas()['characters']}.characters c ON c.guid = m.receiver "
        f"WHERE c.name = '{name}' GROUP BY mi.mail_id) AS per;",
    )
    say(f"the fullest mail {name} has holds {fullest} item(s)")
    newest = sql(
        "characters",
        "SELECT GROUP_CONCAT(CONCAT(m.id, '=', mi.item_template) ORDER BY m.id) "
        f"FROM {schemas()['characters']}.mail m "
        f"JOIN {schemas()['characters']}.mail_items mi ON mi.mail_id = m.id "
        f"JOIN {schemas()['characters']}.characters c ON c.guid = m.receiver "
        f"WHERE c.name = '{name}' AND m.subject = 'Your gear' "
        f"ORDER BY m.id DESC LIMIT {promised};",
    )
    say(f"the mails this press made: {newest}")
    assert fullest == "1", f"a mail on this tree carried {fullest} items"
    say(f"clause PASSED: {pieces} pieces, {promised} promised, {now - had} arrived, one item each")


if __name__ == "__main__":
    what = sys.argv[1]
    who = sys.argv[2]
    if what == "ground":
        stage_ground(who, sys.argv[3] if len(sys.argv) > 3 else None)
    elif what == "press":
        stage_press(who)
    else:
        raise SystemExit(f"no stage called {what}")
