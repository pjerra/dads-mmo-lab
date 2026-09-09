"""The rollback's RESTORE arm, pressed live on yulon-ubuntu2 (2026-09-09).

`_keep_rollback()` and `_let_go()` were pressed live on 2026-09-08
(`pyplan/gates/rebuild-live-yulon-ubuntu2-2026-09-09/`). `_restore_rollback()`
was not, because nothing that night produced a build that failed to come up.
This gate manufactures exactly one such build and presses the same control.

**How the build is broken, and why that shape.** One line is added to
AzerothCore's own `src/server/apps/worldserver/Main.cpp` in the checkout the
build reads: the worldserver prints a marker and returns 1 as its first act. It
COMPILES -- so `stage_build` succeeds, `built` is True and the live tags move to
the new images, which is the state `_restore_rollback` exists for -- and the
container it produces exits at once, so `wait_for_ready()` sees a crash loop and
the ready stage raises. Nothing in `native.py` is patched and no container is
killed by hand: either of those would prove the harness rather than the feature.

Every step reads its GROUND first and writes it down, and the ground of the
whole press is written to `ground.json` so the readings afterwards are compared
against the RECORDED start state rather than against a second derivation of it.

The two readings that a single process cannot honestly take are taken by two
others: `watch_tags.sh` polls the daemon every three seconds from outside the
press, and `docker events` records every image tag and untag with the daemon's
own timestamps -- which is the only instrument fast enough for the `-failed`
tags, whose whole life is the few milliseconds between `_restore_rollback()`
naming the new build and `_let_go()` letting that name go.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import time
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

REPO = Path(os.environ.get("GATE_REPO", "/home/pk/lane-restore"))
sys.path.insert(0, str(REPO / "pylauncher"))

SERVER = Path(os.environ.get("GATE_SERVER", "/home/pk/wowserver"))
OUT = Path(os.environ.get("GATE_OUT", "/home/pk/restore-gate-out"))
OUT.mkdir(parents=True, exist_ok=True)
GROUND = OUT / "ground.json"

WORLD = "ac-worldserver"
AUTH = "ac-authserver"
DB = "ac-database"
PING = "dml_bridge_ping"
SABOTAGE_MARKER = "YULON-GATE-SABOTAGE"
MAIN_CPP = SERVER / "src/server/apps/worldserver/Main.cpp"


def say(text: str = "") -> None:
    print(text, flush=True)


def stamp(text: str) -> None:
    say(f"[{time.strftime('%H:%M:%SZ', time.gmtime())}] {text}")


def sh(argv: list[str], timeout: float = 120.0) -> subprocess.CompletedProcess[str]:
    return subprocess.run(argv, capture_output=True, text=True, timeout=timeout)


# --------------------------------------------------------------- the readings


def world_state() -> str:
    """Alive, its pid, its restarts and the IMAGE ID under it -- at every capture.

    The image id is the point of this gate: the clause is that the images under
    the running stack at the end are the ones from the ground, and a container
    that is up on a different id is not that. A server that has died also
    photographs exactly like a refusal, so liveness is printed beside every
    reading rather than assumed.
    """
    return container_state(WORLD)


def container_state(name: str) -> str:
    proc = sh(
        [
            "docker",
            "inspect",
            "-f",
            "{{.State.Status}} id={{slice .Id 0 12}} pid={{.State.Pid}} "
            "restarts={{.RestartCount}} started={{.State.StartedAt}} "
            "image={{slice .Image 7 19}} exit={{.State.ExitCode}}",
            name,
        ],
        timeout=30,
    )
    return (proc.stdout or proc.stderr).strip()


def images() -> list[str]:
    proc = sh(
        ["docker", "images", "--format", "{{.Repository}}:{{.Tag}} {{.ID}} {{.Size}}"],
        timeout=30,
    )
    return sorted(line for line in proc.stdout.splitlines() if line.strip())


def install_image_ids() -> dict[str, str]:
    """The four `native-<install id>` tags this install builds, tag -> image id."""
    out: dict[str, str] = {}
    for line in images():
        ref, image_id, *_ = line.split()
        if ref.startswith("yulon.local/ac-wotlk-") and "-rollback" not in ref and "-failed" not in ref:
            out[ref] = image_id
    return out


def suffix_tags(suffix: str) -> list[str]:
    return [line for line in images() if suffix in line.split()[0]]


def bridge_scripts() -> list[str]:
    from yulon import party

    dest = party.dest_dir(SERVER)
    if not dest.is_dir():
        return []
    return sorted(p.name for p in dest.glob("*.lua"))


def services():
    from yulon.catalog.catalog import load_catalog
    from yulon.ui.controller_view import ControllerServices

    entry = load_catalog().get("wow-wotlk")
    return entry, ControllerServices.for_entry(entry, SERVER)


def ask_server(command: str) -> str:
    _entry, svc = services()
    channel = svc.channel_setup.live_channel() if svc.channel_setup is not None else None
    if channel is None:
        return "(no saved credential: the command channel is not set up on this install)"
    answer = channel.send(command)
    return (
        f"outcome={answer.outcome!r} denied={answer.denied} "
        f"indeterminate={answer.indeterminate} reason={answer.reason!r}\n"
        f"    text: {answer.text!r}"
    )


def db_password() -> str:
    """The install's database password, through the app's own seam, never printed.

    WotLK's is the catalogue's FIXED value (`install_wiring.fixed_db_password`),
    which is also what `resolve_secrets()` hands every stage. An override in
    this project's `.env` wins if a port-conflict remedy or a hand edit put one
    there, which is the order compose itself reads them in.
    """
    env = SERVER / ".env"
    if env.is_file():
        for line in env.read_text(encoding="utf-8", errors="replace").splitlines():
            if line.startswith("DB_ROOT_PASSWORD="):
                return line.split("=", 1)[1].strip()
    from yulon import install_wiring

    entry, _svc = services()
    return install_wiring.fixed_db_password(entry)


def sql(query: str) -> str:
    """One query against the install's own database, through `docker exec`.

    The password is passed in the container's environment and never printed:
    a gate log that carries a generated password is a gate log that cannot be
    committed (bug-checklist, 2026-09-08).
    """
    secret = db_password()
    if not secret:
        return "(could not read the database password)"
    proc = sh(
        [
            "docker",
            "exec",
            "-e",
            f"MYSQL_PWD={secret}",
            DB,
            "mysql",
            "-N",
            "-B",
            "--user=root",
            "-e",
            query,
        ],
        timeout=120,
    )
    text = (proc.stdout or proc.stderr).strip()
    return text.replace(secret, "***")


def db_readings() -> dict[str, str]:
    """What must be the same afterwards, and what says whether the DB was written.

    The `updates` row counts are the measurement behind the sentence
    `_restore_rollback()` prints about the database: an image rollback does not
    put back what the new build's updater wrote. Whether it wrote anything on
    THIS press is a fact about these three numbers, not an opinion.
    """
    return {
        "realmlist": sql("SELECT id,name,address,port,localAddress FROM acore_auth.realmlist;"),
        "accounts_yulon": sql(
            "SELECT a.id,a.username,IFNULL(aa.gmlevel,-1) FROM acore_auth.account a "
            "LEFT JOIN acore_auth.account_access aa ON aa.id=a.id WHERE a.username LIKE 'YULON%';"
        ),
        "characters": sql("SELECT COUNT(*) FROM acore_characters.characters;"),
        "mail_rows": sql("SELECT COUNT(*) FROM acore_characters.mail;"),
        "mail_by_receiver": sql(
            "SELECT receiver, COUNT(*) FROM acore_characters.mail GROUP BY receiver ORDER BY receiver;"
        ),
        "world_updates": sql("SELECT COUNT(*) FROM acore_world.updates;"),
        "characters_updates": sql("SELECT COUNT(*) FROM acore_characters.updates;"),
        "auth_updates": sql("SELECT COUNT(*) FROM acore_auth.updates;"),
        "world_updates_last": sql(
            "SELECT name,timestamp FROM acore_world.updates ORDER BY timestamp DESC LIMIT 3;"
        ),
    }


def conf_lines() -> list[str]:
    """The two worldserver.conf lines the state block names. Values, not secrets."""
    conf = SERVER / "env/dist/etc/worldserver.conf"
    if not conf.is_file():
        return ["(worldserver.conf does not exist)"]
    wanted = ("SOAP.Enabled", "SOAP.IP", "SOAP.Port", "Logger.ALE")
    return [
        line.strip()
        for line in conf.read_text(encoding="utf-8", errors="replace").splitlines()
        if line.strip().startswith(wanted)
    ]


def world_log_has(marker: str) -> bool:
    """Is `marker` anywhere in the world's CURRENT run?

    `--since <this run's StartedAt>` and not `--tail N`: a populated playerbots
    realm prints bot statistics every thirty seconds, so the ready banner is
    tens of thousands of lines back within minutes and a `--tail 400` reading
    answers "no" about a server that printed it. Press 1's `after` capture
    carries that wrong answer, which is why this reads the run instead.
    """
    started = sh(
        ["docker", "inspect", "-f", "{{.State.StartedAt}}", WORLD], timeout=30
    ).stdout.strip()
    argv = ["docker", "logs", WORLD]
    if started:
        argv[2:2] = ["--since", started]
    proc = sh(argv, timeout=120)
    return marker in (proc.stdout + proc.stderr)


def sabotage_state() -> dict[str, object]:
    text = MAIN_CPP.read_text(encoding="utf-8", errors="replace") if MAIN_CPP.is_file() else ""
    return {
        "main_cpp_carries_the_marker": SABOTAGE_MARKER in text,
        "running_world_prints_the_marker": world_log_has(SABOTAGE_MARKER),
        "git_status": sh(["git", "-C", str(SERVER), "status", "--porcelain", "--", str(MAIN_CPP)]).stdout.strip(),
    }


# --------------------------------------------------------------------- ground


def step_ground() -> None:
    say("=== GROUND, read before anything is sabotaged or compiled ===")
    stamp(f"server_dir={SERVER}  repo={REPO}")
    ids = install_image_ids()
    ground = {
        "read_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "install_image_ids": ids,
        "world": world_state(),
        "auth": container_state(AUTH),
        "database": container_state(DB),
        "rollback_tags": suffix_tags("-rollback"),
        "failed_tags": suffix_tags("-failed"),
        "bridge_scripts": bridge_scripts(),
        "conf_lines": conf_lines(),
        "sabotage": sabotage_state(),
        "db": db_readings(),
        "images": images(),
    }
    say("the four images this install builds, tag -> id:")
    for ref, image_id in sorted(ids.items()):
        say(f"  {ref}  {image_id}")
    say(f"world:    {ground['world']}")
    say(f"auth:     {ground['auth']}")
    say(f"database: {ground['database']}")
    say(f"-rollback tags (must be none): {ground['rollback_tags']}")
    say(f"-failed   tags (must be none): {ground['failed_tags']}")
    say(f"bridge scripts on disk: {ground['bridge_scripts']}")
    say("worldserver.conf, the lines the state block names:")
    for line in ground["conf_lines"]:  # type: ignore[union-attr]
        say(f"  {line}")
    say("the sabotage marker, before anything is written:")
    for key, value in ground["sabotage"].items():  # type: ignore[union-attr]
        say(f"  {key} = {value!r}")
    say("the database, read through the app's own install secret:")
    for key, value in ground["db"].items():  # type: ignore[union-attr]
        say(f"  {key}:")
        for line in str(value).splitlines():
            say(f"      {line}")
    say(f"the server asked {PING!r}:")
    say(f"  {ask_server(PING)}")
    say(f"world at the end of the ground reading: {world_state()}")
    GROUND.write_text(json.dumps(ground, indent=2), encoding="utf-8")
    say(f"ground written to {GROUND}")


# ------------------------------------------------------------------- sabotage


def step_sabotage() -> None:
    """Break the BUILD's OUTPUT: one line in the checkout the build reads.

    Ground first, and it is not a formality -- if the marker were already in
    the file, or the running server already printed it, this step would be
    asserting its own start state.
    """
    say("=== SABOTAGE: the build's own source, so the build's OUTPUT is what fails ===")
    before = sabotage_state()
    for key, value in before.items():
        say(f"  ground: {key} = {value!r}")
    if before["main_cpp_carries_the_marker"]:
        say("GROUND FAILS: Main.cpp already carries the marker; this step would prove nothing")
        raise SystemExit(2)
    if before["running_world_prints_the_marker"]:
        say("GROUND FAILS: the running world already prints the marker")
        raise SystemExit(2)
    text = MAIN_CPP.read_text(encoding="utf-8")
    anchor = "int main(int argc, char** argv)\n{\n"
    if anchor not in text:
        say(f"GROUND FAILS: {MAIN_CPP} does not carry the expected entry point")
        raise SystemExit(2)
    patch = (
        anchor
        + "    // YULON GATE 2026-09-09: a deliberately broken BUILD OUTPUT for the\n"
        + "    // rollback-restore press. This compiles; the worldserver it produces\n"
        + "    // exits at once, so the rebuild's readiness wait fails and\n"
        + "    // `_restore_rollback()` has to put the previous build back. Reverted\n"
        + "    // with `git checkout --` the moment the press is over.\n"
        + "    if (argc >= 0)\n"
        + "    {\n"
        + f'        std::cerr << "{SABOTAGE_MARKER}: this worldserver build exits immediately '
        + '(rollback-restore press, 2026-09-09)" << std::endl;\n'
        + "        return 1;\n"
        + "    }\n"
    )
    MAIN_CPP.write_text(text.replace(anchor, patch, 1), encoding="utf-8")
    say("--- the diff, as git sees it ---")
    say(sh(["git", "-C", str(SERVER), "diff", "--", str(MAIN_CPP)]).stdout)
    after = sabotage_state()
    for key, value in after.items():
        say(f"  after: {key} = {value!r}")
    say(f"world (untouched by this step): {world_state()}")


def step_revert() -> None:
    """Put the checkout back. The image rollback does not do this and cannot."""
    say("=== REVERT the sabotage from the owner's checkout ===")
    before = sabotage_state()
    for key, value in before.items():
        say(f"  ground: {key} = {value!r}")
    say(sh(["git", "-C", str(SERVER), "checkout", "--", str(MAIN_CPP)]).stderr.strip())
    after = sabotage_state()
    for key, value in after.items():
        say(f"  after: {key} = {value!r}")
    say(f"world: {world_state()}")


# ---------------------------------------------------------------- instruments


def step_probe_events() -> None:
    """Prove the tag instrument works BEFORE the press depends on it.

    `docker events` is the only reading fast enough for the `-failed` tags, and
    an instrument nobody tested is how a gate ends up with a dead probe and a
    confident sentence. One harmless tag on an image this install does not use,
    then its removal, read back out of the event stream.
    """
    say("=== INSTRUMENT PROBE: does this daemon report image tag/untag events? ===")
    probe = "yulon-gate-events-probe:1"
    since = str(int(time.time()) - 5)
    say(f"ground: {probe} on the daemon = {any(probe in line for line in images())}")
    say(sh(["docker", "tag", "mysql:8.4", probe]).stderr.strip())
    say(f"tagged: {probe} on the daemon = {any(probe in line for line in images())}")
    say(sh(["docker", "image", "rm", probe]).stdout.strip())
    say(f"removed: {probe} on the daemon = {any(probe in line for line in images())}")
    # `--until` is a WHOLE second and the window is exclusive of what comes
    # after it: the first version of this probe passed `int(time.time())`, which
    # truncates DOWN, so the two events it had just caused fell outside its own
    # window and the probe reported the instrument dead. Measured here, and the
    # reason the probe is a step of its own rather than a comment.
    until = str(int(time.time()) + 2)
    time.sleep(2)
    seen = sh(
        [
            "docker",
            "events",
            "--since",
            since,
            "--until",
            until,
            "--filter",
            "type=image",
            "--format",
            "{{.Time}} {{.Action}} {{.Actor.Attributes.name}}",
        ],
        timeout=60,
    )
    say("--- the event stream for those two acts ---")
    say(seen.stdout.strip() or "(nothing: this instrument would prove nothing)")


# ---------------------------------------------------------------------- press


def step_press() -> None:
    """THE PRESS. The refusal IS the result here, so it is quoted in full."""
    from yulon.install_wiring import rebuild_for_app

    entry, _svc = services()
    ground = json.loads(GROUND.read_text(encoding="utf-8"))
    say("=== THE PRESS: rebuild_for_app(entry, server_dir)(cancel), on a build that cannot come up ===")
    say(f"ground (from {GROUND}, read {ground['read_at']}):")
    for ref, image_id in sorted(ground["install_image_ids"].items()):
        say(f"  {ref}  {image_id}")
    say(f"  world: {ground['world']}")
    say(f"now:   world = {world_state()}")
    say(f"now:   install image ids = {json.dumps(install_image_ids(), indent=2)}")
    say(f"now:   -rollback tags = {suffix_tags('-rollback')}")
    say(f"now:   -failed   tags = {suffix_tags('-failed')}")
    say(f"now:   Main.cpp carries the marker = {sabotage_state()['main_cpp_carries_the_marker']}")
    say("--- the press ---")
    started = time.time()
    rebuild = rebuild_for_app(entry, SERVER)
    outcome = "returned cleanly"
    message = ""
    try:
        for line in rebuild(None):
            stamp(line)
    except Exception as exc:  # noqa: BLE001 - the refusal IS the result here
        outcome = f"raised {type(exc).__name__}"
        message = str(exc)
        stamp(f"PRESS {outcome} after {time.time() - started:.1f}s")
        say("--- the sentence the user reads, verbatim ---")
        say(message)
        say("--- end of the sentence ---")
    else:
        stamp(f"PRESS RETURNED CLEANLY in {time.time() - started:.1f}s")
    say(f"after: world = {world_state()}")
    say(f"after: auth  = {container_state(AUTH)}")
    say(f"after: install image ids = {json.dumps(install_image_ids(), indent=2)}")
    say(f"after: -rollback tags (must be none) = {suffix_tags('-rollback')}")
    say(f"after: -failed   tags (must be none) = {suffix_tags('-failed')}")
    state_file = SERVER / ".yulon-install.json"
    if state_file.is_file():
        recorded = json.loads(state_file.read_text(encoding="utf-8")).get("last_error")
        say("--- last_error recorded in the install's own state file ---")
        say(repr(recorded))
    (OUT / "press-message.txt").write_text(message, encoding="utf-8")
    say(f"PRESS OUTCOME: {outcome}")


# ---------------------------------------------------------------------- after


def step_after() -> None:
    """Every clause, against the RECORDED ground rather than a second derivation."""
    ground = json.loads(GROUND.read_text(encoding="utf-8"))
    say("=== AFTER: the clauses, against ground.json ===")
    now_ids = install_image_ids()
    was = ground["install_image_ids"]
    say("the four images under the install's own tags:")
    for ref in sorted(set(was) | set(now_ids)):
        same = "SAME" if was.get(ref) == now_ids.get(ref) else "DIFFERENT"
        say(f"  {ref}\n      ground {was.get(ref)}   now {now_ids.get(ref)}   -> {same}")
    say(f"world now:   {world_state()}")
    say(f"world ground:{ground['world']}")
    say(f"auth now:    {container_state(AUTH)}")
    say(f"auth ground: {ground['auth']}")
    say(f"-rollback tags now (must be none): {suffix_tags('-rollback')}")
    say(f"-failed   tags now (must be none): {suffix_tags('-failed')}")
    say(f"bridge scripts now:   {bridge_scripts()}")
    say(f"bridge scripts ground:{ground['bridge_scripts']}")
    say("worldserver.conf now:")
    for line in conf_lines():
        say(f"  {line}")
    say(f"the world's log still prints the sabotage marker: {world_log_has(SABOTAGE_MARKER)}")
    say(f"the world's log prints its ready banner: {world_log_has('ready...')}")
    say(f"the server asked {PING!r}:")
    say(f"  {ask_server(PING)}")
    say("the database, now against the ground:")
    now_db = db_readings()
    for key, value in now_db.items():
        same = "SAME" if str(ground["db"].get(key)) == str(value) else "CHANGED"
        say(f"  {key}: {same}")
        if same == "CHANGED":
            say(f"      ground: {ground['db'].get(key)!r}")
            say(f"      now:    {value!r}")
        else:
            for line in str(value).splitlines():
                say(f"      {line}")
    say(f"world at the end of the after reading: {world_state()}")


def step_shot_panel() -> None:
    """The app's own rebuild panel, showing the press's own recorded lines.

    NOT a second press: the widget is the real `LogPanel` from the real
    `ControllerView`, and the bytes in it are the ones `press.log` recorded
    while the press ran, header included. Said here and in the README, because
    a render that pretended to be the live panel would be exactly the false
    artefact this folder is supposed to avoid.
    """
    from PySide6.QtWidgets import QApplication
    from yulon.ui.controller_view import ControllerView

    app = QApplication.instance() or QApplication([])
    entry, svc = services()
    view = ControllerView(entry, svc, status_poll_ms=0)
    view.resize(1180, 860)
    for index in range(view._tabs.count()):
        if view._tabs.tabText(index) == os.environ.get("GATE_SHOT_TAB", "Modules"):
            view._tabs.setCurrentIndex(index)
            break
    source = OUT / os.environ.get("GATE_SHOT_FILE", "press.log")
    tail = int(os.environ.get("GATE_SHOT_TAIL", "0"))
    lines = source.read_text(encoding="utf-8", errors="replace").splitlines()
    for line in lines[-tail:] if tail else lines:
        view.rebuild_log.append(line)
    message = (OUT / "press-message.txt").read_text(encoding="utf-8", errors="replace")
    # `_on_finished()`'s own spelling for a source that raised: `("FAILED: ") + message`.
    view.rebuild_log._status.setText(f"FAILED: {message}")
    app.processEvents()
    path = OUT / os.environ.get("GATE_SHOT_NAME", "panel.png")
    view.grab().save(str(path))
    say(f"SHOT {path}  {WORLD} at capture: {world_state()}")


STEPS = {
    "ground": step_ground,
    "probe-events": step_probe_events,
    "sabotage": step_sabotage,
    "press": step_press,
    "after": step_after,
    "revert": step_revert,
    "panel": step_shot_panel,
}


def main() -> int:
    from yulon.log import configure, use_utf8_streams

    use_utf8_streams()
    configure()
    if len(sys.argv) < 2 or sys.argv[1] not in STEPS:
        say(f"usage: restore_gate.py <{'|'.join(STEPS)}>")
        return 2
    STEPS[sys.argv[1]]()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
