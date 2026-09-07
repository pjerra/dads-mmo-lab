"""How many attachments does ONE mail carry here? Asked by sending too many."""
import sys
from pathlib import Path

sys.path.insert(0, "/home/pk/gate83b/pylauncher")
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui import controller_view as v  # noqa: E402
from yulon.ui.controller_view import ControllerServices  # noqa: E402

E = load_catalog().get("wow-tbc")
D = Path.home() / "tbc-7.4c"
sql = v._sql_for(E, v._db_password(E, D), wsl_distro=None)
channel = ControllerServices.for_entry(E, D).channel_setup.live_channel()
S = E.schema_map()
Q = chr(39)
NAME = "Ddsasd"


def mails() -> int:
    return int(
        sql.query(
            "characters",
            f"SELECT COUNT(*) FROM {S['characters']}.mail m "
            f"JOIN {S['characters']}.characters c ON c.guid = m.receiver "
            f"WHERE c.name = {Q}{NAME}{Q};",
        ).strip()
        or 0
    )


def items_in_mail() -> str:
    return sql.query(
        "characters",
        f"SELECT m.id, COUNT(mi.item_guid) FROM {S['characters']}.mail m "
        f"JOIN {S['characters']}.mail_items mi ON mi.mail_id = m.id "
        f"JOIN {S['characters']}.characters c ON c.guid = m.receiver "
        f"WHERE c.name = {Q}{NAME}{Q} GROUP BY m.id ORDER BY m.id DESC LIMIT 4;",
    ).replace("\n", " | ")


for count in (12, 13):
    had = mails()
    attach = " ".join(f"2589:{n + 1}" for n in range(count))
    answer = channel.send(f'send items {NAME} "cap {count}" "how many" {attach}')
    print(f"{count} items -> {answer.outcome}: {(answer.text or '').strip()[:70]!r}")
    print(f"   mails {had} -> {mails()}   per mail: {items_in_mail()}")
