"""8.4d round two: the clause round one asserted from a reading, and the
photographs round one's own stages did not take.

RUN 2026-09-08 on m910q against the live Tortoise stack, on top of the round-one
run recorded in `transcript.txt`. Its own output is `transcript-round2.txt`.

Usage:  ~/gate81b-venv/bin/python gate84d-round2.py <stage>

    spelling   the defect: the absent-group sentence NAMES a command, and no run
               had ever sent it in the order the sentence uses. Both orders are
               pressed here and the sentence follows what the server answered.
    absent2    re-photographs 1- and 2- with whatever `spelling` settled, because
               a photograph of a sentence is only evidence of the sentence in it.
    tabshots   the four actions this tree DOES draw, pressed through the tab that
               draws them, each photographed with the server's own reply on it.
               Round one pressed all four at the seam and photographed none.

Every stage takes its ground first and asserts a change against it. The subject
of every stage is re-read immediately before it is acted on, because
`RandomPlayerbotMgr` re-levels and re-positions its bots while this runs.
"""

from __future__ import annotations

import os
import re
import sys
import time
from datetime import UTC, datetime
from pathlib import Path

sys.path.insert(0, str(Path.home() / "gate84d" / "pylauncher"))

from yulon import play as play_module  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.controller_wow_tortoise import console as tortoise_console  # noqa: E402
from yulon.ui.controller_view import ControllerServices  # noqa: E402

SERVER_DIR = Path.home() / "tortoise-server"
ENTRY = load_catalog().get("wow-tortoise")
WOTLK = load_catalog().get("wow-wotlk")
_SQL_TRACE = re.compile(r"^\s*\[\d+ ms\] SQL: ")
SHOTS = Path.home() / "gate84d-tortoise-shots"


def stamp() -> str:
    return datetime.now(UTC).strftime("%H:%M:%SZ")


def say(text: str) -> None:
    print(f"[{stamp()}] {text}", flush=True)


def reader():
    from yulon.ui import controller_view as view_module  # noqa: PLC0415

    password = view_module._db_password(ENTRY, SERVER_DIR)
    return view_module._sql_for(ENTRY, password, wsl_distro=None)


def sql(statement: str) -> str:
    return reader().query("characters", statement).strip()


def row(guid: int) -> dict[str, str]:
    text = sql(
        "SELECT guid,name,level,map,zone,online,at_login,"
        "ROUND(position_x,2),ROUND(position_y,2),ROUND(position_z,2) "
        f"FROM tw_char.characters WHERE guid = {guid};"
    )
    if not text:
        return {}
    keys = ("guid", "name", "level", "map", "zone", "online", "at_login", "x", "y", "z")
    return dict(zip(keys, text.split("\t"), strict=False))


def mails(guid: int) -> int:
    return int(sql(f"SELECT COUNT(*) FROM tw_char.mail WHERE receiver = {guid};") or 0)


def corpse_rows(guid: int) -> int:
    return int(sql(f"SELECT COUNT(*) FROM tw_char.corpse WHERE player = {guid};") or 0)


def console(command: str, window: float = 8.0) -> tuple[bool, str]:
    reply = tortoise_console.send(command, window=window)
    return bool(reply.prompted), "\n".join(reply.lines)


def quiet(text: str) -> list[str]:
    return [line for line in text.splitlines() if not _SQL_TRACE.match(line)]


def services(entry=None) -> ControllerServices:
    return ControllerServices.for_entry(entry or ENTRY, SERVER_DIR)


def play() -> play_module.InstallPlay:
    made = services()
    assert made.play is not None, "the Tortoise entry carries no play block"
    return made.play


def view_for(made: ControllerServices, entry=None):
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    from PySide6.QtWidgets import QApplication  # noqa: PLC0415

    from yulon.ui.controller_view import ControllerView  # noqa: PLC0415
    from yulon.ui.widgets.job import run_inline  # noqa: PLC0415

    app = QApplication.instance() or QApplication([])
    view = ControllerView(entry or ENTRY, made, status_poll_ms=0, job_runner=run_inline)
    view.resize(980, 760)
    view._tabs.setCurrentIndex(
        next(i for i in range(view._tabs.count()) if view._tabs.tabText(i) == "Characters")
    )
    view._gate_app = app
    SHOTS.mkdir(parents=True, exist_ok=True)
    return view


def select(view, name: str) -> int:
    from PySide6.QtCore import Qt  # noqa: PLC0415

    for i in range(view.character_list.count()):
        item = view.character_list.item(i)
        if str(item.data(Qt.ItemDataRole.UserRole) or "") == name:
            view.character_list.setCurrentRow(i)
            return i
    raise AssertionError(f"{name} is not in the tab's character list")


# ------------------------------------------------------------------ spelling


def stage_spelling() -> None:
    """The absent-group sentence names `.rndbot ... level`. In which order?

    Round one sent `rndbot <bot> level 60` ONCE, got `<bot>: level - character
    not found`, and concluded that the number was ignored. Between two runs of
    its `absent` stage somebody then rewrote the sentence from
    `.rndbot <bot> level` to `.rndbot level <bot>` -- and never sent the second
    form. So the shipped sentence names a command spelling that no run has
    pressed, which is the shape of a claim with nothing behind it.

    Both orders are sent here, on a subject read immediately before, and the
    sentence follows the answer rather than the other way round.
    """
    prompted, _ = console("server info")
    assert prompted, "no console -- every reading below would be ambiguous"

    name, guid = sql(
        "SELECT name,guid FROM tw_char.characters WHERE online = 1 AND level BETWEEN 5 AND 45 "
        "ORDER BY guid LIMIT 1;"
    ).split("\t")[:2]
    before = row(int(guid))
    say(f"GROUND: {before} -- both commands below ask for level 60")
    assert int(before["level"]) != 60, "a subject already at 60 cannot show a number being ignored"

    answers: dict[str, str] = {}
    for command in (f"rndbot {name} level 60", f"rndbot level {name} 60", f"rndbot level {name}"):
        ok, text = console(command)
        say(f"`{command}` -> prompted={ok}")
        lines = quiet(text)
        for line in lines:
            say(f"  | {line}")
        answers[command] = "\n".join(lines)
        time.sleep(2)

    time.sleep(4)
    after = row(int(guid))
    say(f"the subject after all three: level {before['level']} -> {after['level']}")
    assert int(after["level"]) != 60, (
        "one of these DID move an existing character to the level it was handed -- "
        "the whole box rests on none of them doing that"
    )

    # Which order the fork's own parser accepts is the only thing that decides
    # the sentence. `character not found` names the SUBJECT it failed to find,
    # so the order that reports the BOT's name as the missing character is the
    # order in which the bot's name was read as a command word.
    for command, text in answers.items():
        found = "character not found" not in text.lower()
        say(f"  {command!r}: the fork's parser accepted the subject: {found}")

    say(
        "RECORDED: the sentence must name the order this server answers to, and "
        "the README says which one that is and quotes the reply it came from."
    )


# ------------------------------------------------------------------- absent2


def stage_absent2() -> None:
    """S2 again, so the photograph shows the sentence that actually ships.

    Ground is the SAME app build on WotLK, which HAS the control. Without that
    first frame this stage photographs an empty space, and an empty space is
    what a broken tab looks like too.
    """
    say(f"wow-wotlk set_level_command: {WOTLK.play.set_level_command!r}")
    say(f"wow-tortoise set_level_command: {ENTRY.play.set_level_command!r}")
    assert WOTLK.play.set_level_command, "the ground tree must HAVE the control"
    assert ENTRY.play.set_level_command is None, "this tree must not"

    name = sql(
        "SELECT name FROM tw_char.characters WHERE online = 1 ORDER BY guid LIMIT 1;"
    ).split("\t")[0]

    made = services()
    view = view_for(made)
    view.refresh_characters()
    select(view, name)

    for label, widget in (
        ("the Level spinbox", view.new_level),
        ("the Set level button", view.set_level_button),
    ):
        say(
            f"{label}: hidden={widget.isHidden()} "
            f"would_be_drawn={widget in view.character_buttons()}"
        )
        assert widget.isHidden(), f"{label} is drawn on a tree that has no such command"
    say(f"the sentence: hidden={view.set_level_absent.isHidden()}")
    assert not view.set_level_absent.isHidden(), "the absent group shows no reason"
    say(f"the sentence reads: {view.set_level_absent.text()!r}")
    assert view.set_level_absent.text() == ENTRY.play.set_level_absent_reason

    say(f"the buttons this tree DOES draw, each naming {name}:")
    for b in view.character_buttons():
        say(f"  | {b.text()!r} enabled={b.isEnabled()}")

    view.grab().save(str(SHOTS / "2-no-set-level-and-its-reason.png"))
    say("photographed 2-no-set-level-and-its-reason.png")

    ground = view_for(services(WOTLK), entry=WOTLK)
    say(
        "the ground build's Level row: "
        f"spinbox={not ground.new_level.isHidden()} "
        f"button={ground.set_level_button in ground.character_buttons()}"
    )
    assert not ground.new_level.isHidden(), "the ground tree draws no spinbox either"
    ground.grab().save(str(SHOTS / "1-ground-a-tree-that-has-it.png"))
    say("photographed 1-ground-a-tree-that-has-it.png")


# ------------------------------------------------------------------ tabshots


def _press(view, name: str, what: str, slot, shot: str) -> str:
    say(f"pressing {what} on {name} through the tab")
    slot()
    reported = view.character_report.text()
    say(f"  the tab reads: {reported.splitlines()[-1] if reported else ''!r}")
    view.grab().save(str(SHOTS / shot))
    say(f"photographed {shot}")
    return reported


def stage_tabshots() -> None:
    """The four actions this tree draws, pressed at the tab and photographed.

    Round one pressed every one of these at the seam -- `play().rename(...)` and
    friends -- and photographed only the two REFUSALS. A refusal photographed
    beside no success is not a contrast, and the box's visible effect is what a
    person sees when the action works.
    """
    made = services()
    view = view_for(made)
    view.refresh_characters()

    # -- rename, on an ONLINE character whose flag is clear ------------------
    rename_name, rename_guid = sql(
        "SELECT name,guid FROM tw_char.characters WHERE online = 1 AND (at_login & 1) = 0 "
        "ORDER BY guid LIMIT 1;"
    ).split("\t")[:2]
    before = row(int(rename_guid))
    say(f"GROUND for the rename: {before} -- at_login & 1 = {int(before['at_login']) & 1}")
    assert int(before["at_login"]) & 1 == 0, "the flag is already set; the press would prove nothing"
    select(view, rename_name)
    assert view.rename_button.isEnabled(), "the button is withheld for an ONLINE character"
    reported = _press(view, rename_name, "Rename at next login", view.rename_character, "5-rename-sent.png")
    assert "no such subcommand" not in reported.lower(), reported
    time.sleep(3)
    after = row(int(rename_guid))
    say(f"  after: at_login {before['at_login']} -> {after['at_login']}, name {after['name']!r}")
    assert int(after["at_login"]) & 1 == 1, "the flag did not land"
    assert after["name"] == before["name"], "THE NAME CHANGED -- this is the destructive arm"

    # -- revive, on an OFFLINE character with a corpse -----------------------
    picked = sql(
        "SELECT c.name, c.guid FROM tw_char.corpse co "
        "JOIN tw_char.characters c ON c.guid = co.player WHERE c.online = 0 LIMIT 1;"
    ).split("\t")
    revive_name, revive_guid = picked[0], int(picked[1])
    say(f"GROUND for the revive: {revive_name} (guid {revive_guid}), corpse rows = {corpse_rows(revive_guid)}")
    assert corpse_rows(revive_guid) == 1, "no corpse -- nothing for the command to remove"
    say("  watching the corpse for 12 s, so its going is the command's doing and not decay:")
    for _ in range(4):
        time.sleep(3)
        say(f"    corpse rows: {corpse_rows(revive_guid)}")
    assert corpse_rows(revive_guid) == 1, "the corpse went by itself; this subject proves nothing"
    view.refresh_characters()
    select(view, revive_name)
    assert view.revive_button.isEnabled(), "revive is withheld -- this tree's revive_offline is true"
    _press(view, revive_name, "Revive", view.revive_character, "7-revived-offline.png")
    time.sleep(3)
    say(f"  corpse rows after: {corpse_rows(revive_guid)}")
    assert corpse_rows(revive_guid) == 0, "the corpse is still there"

    # -- send gold, on an OFFLINE character ---------------------------------
    gold_name, gold_guid = sql(
        "SELECT name,guid FROM tw_char.characters WHERE online = 0 ORDER BY guid LIMIT 1;"
    ).split("\t")[:2]
    gold_before = mails(int(gold_guid))
    say(f"GROUND for the gold: {gold_name} holds {gold_before} mails")
    view.refresh_characters()
    select(view, gold_name)
    view.gold_amount.setValue(7)
    _press(view, gold_name, "Send gold (7)", view.mail_gold, "9-gold-sent.png")
    time.sleep(3)
    gold_after = mails(int(gold_guid))
    say(f"  mails: {gold_before} -> {gold_after}")
    assert gold_after == gold_before + 1, "no mail arrived"
    money = sql(
        f"SELECT money FROM tw_char.mail WHERE receiver = {gold_guid} ORDER BY id DESC LIMIT 1;"
    )
    say(f"  the newest mail's money: {money} (7 gold is 70000 copper)")
    assert money == "70000", money

    # -- the gear set, and the AFTER of the promise round one only photographed
    picked = sql(
        "SELECT c.name, ci.guid, COUNT(*) n FROM tw_char.character_inventory ci "
        "JOIN tw_char.characters c ON c.guid = ci.guid "
        "WHERE ci.bag = 0 AND ci.slot < 19 AND c.online = 0 "
        "GROUP BY ci.guid HAVING n BETWEEN 2 AND 5 ORDER BY ci.guid LIMIT 1;"
    ).split("\t")
    wearer, wear_guid, pieces = picked[0], int(picked[1]), int(picked[2])
    wear_before = mails(wear_guid)
    say(f"GROUND for the gear: {wearer} (guid {wear_guid}) wears {pieces}, holds {wear_before} mails")
    assert pieces >= 2, "one piece would not exercise a cap of one"
    view.refresh_characters()
    select(view, wearer)
    label = view.send_gear_button.text()
    say(f"  BEFORE the press the button reads: {label!r}")
    assert f"{pieces} worn items" in label and f"{pieces} mails" in label, label
    _press(view, wearer, "Send everything worn", view.send_gear_set, "8c-n-mails-arrived.png")
    time.sleep(4)
    wear_after = mails(wear_guid)
    say(f"  mails: {wear_before} -> {wear_after} (the button promised {pieces})")
    assert wear_after == wear_before + pieces, "the count does not match the promise"
    per_mail = sql(
        "SELECT MAX(n) FROM (SELECT COUNT(*) n FROM tw_char.mail_items mi "
        f"JOIN tw_char.mail m ON m.id = mi.mail_id WHERE m.receiver = {wear_guid} "
        "GROUP BY mi.mail_id) t;"
    )
    say(f"  MAX(items per mail) for this receiver: {per_mail}")
    assert per_mail == "1", "this tree's cap is one item per message"


STAGES = {
    "spelling": stage_spelling,
    "absent2": stage_absent2,
    "tabshots": stage_tabshots,
}


if __name__ == "__main__":
    which = sys.argv[1]
    say(f"=== stage {which} ===")
    STAGES[which]()
    say(f"=== stage {which} finished ===")
