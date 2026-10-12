"""T748: a NEW WotLK or Unbound install gets a generated database root password.

Before this change every WotLK and Unbound install shared the published root
password `password` (catalog.json, `"mode": "fixed"`). Now a fresh folder mints
`wotlk-` or `unbound-` plus 16 hex, the same mechanism the CMaNGOS families use:
`.db_password` (0600) and `.env` carry it, and the compose text never does.

An install made before this change has no `.db_password`. It keeps `password`
(or the `DB_ROOT_PASSWORD` already in its own `.env`), untouched.

Every test here is about one reader or one step of that rule, and each reads
the password the way the app does at that point: through the install's own
folder, not through a constant.
"""

from __future__ import annotations

import os
import stat
import subprocess
from pathlib import Path

import pytest

from tests.support_native import ENTRY, Recorder, engine, install
from yulon import dbsecret, resources
from yulon.apply import mysql_env
from yulon.catalog import composegen, native
from yulon.catalog.catalog import load_catalog
from yulon.catalog.families.azerothcore import AzerothCoreInstaller
from yulon.install_wiring import DEFAULT_DB_ROOT_PASSWORD

GENERATED = "wotlk-1a2b3c4d5e6f7a8b"
"""A value shaped like `prefix + token_hex(8)`, for the tests that write one by hand."""


def db_volume(server_dir: Path) -> str:
    """The database volume compose gives this folder: `<project>_db-data`."""
    install_id = composegen.install_id(server_dir, platform_id=lambda: "linux")
    project = composegen.project_name(
        ENTRY.id, server_dir, platform_id=lambda: "linux", install_id=install_id
    )
    return f"{project}_db-data"


def stage_context(server_dir: Path) -> native.StageContext:
    """A `StageContext` for calling one stage body directly, as the spine would build it."""
    return native.StageContext(
        server_dir=server_dir,
        client_dir=None,
        state=native.InstallState(
            game_id=ENTRY.id,
            install_id=composegen.install_id(server_dir, platform_id=lambda: "linux"),
            family="azerothcore",
            completed=(),
        ),
        cancel=None,
        secrets=native.Secrets(db_password=GENERATED),
    )


def db_password_stage(eng: native.StagedInstaller) -> native.Stage:
    """The `db-password` stage by name, failing with the names there are when it is absent."""
    by_name = {stage.name: stage for stage in eng.stages()}
    assert "db-password" in by_name, sorted(by_name)
    return by_name["db-password"]


def an_old_folder(server_dir: Path) -> None:
    """A folder an install made BEFORE this change leaves behind: a compose file this app wrote,
    and no record."""
    server_dir.mkdir(parents=True, exist_ok=True)
    (server_dir / composegen.BASE_FILE).write_text(
        composegen.GENERATED_MARKER + "\nservices: {}\n", encoding="utf-8"
    )
    (server_dir / native.STATE_FILE).write_text("{}\n", encoding="utf-8")


# -- the install path ------------------------------------------------------


def test_a_new_wotlk_install_writes_a_generated_password_to_its_file(tmp_path: Path) -> None:
    """RED through the real `run()`: the secret is minted, recorded, and not the published one."""
    server_dir = tmp_path / "wow"
    install(Recorder(images=False), server_dir)

    record = server_dir / ".db_password"
    assert record.is_file(), "a new install must record its own password in .db_password"
    text = record.read_text(encoding="utf-8")
    assert text.endswith("\n"), "one line, with a trailing newline, as the families write it"
    secret = text.strip()
    assert secret.startswith("wotlk-"), secret
    assert len(secret) == len("wotlk-") + 16, secret
    assert secret != DEFAULT_DB_ROOT_PASSWORD
    if os.name == "posix":
        assert stat.S_IMODE(record.stat().st_mode) == 0o600


def test_a_new_wotlk_install_hands_compose_its_password_through_env_only(tmp_path: Path) -> None:
    """The secret is in `.env`, and the compose text names it nowhere, not even as a default."""
    server_dir = tmp_path / "wow"
    install(Recorder(images=False), server_dir)

    assert (server_dir / ".db_password").is_file(), "no record, so there is no secret to check"
    secret = (server_dir / ".db_password").read_text(encoding="utf-8").strip()
    env = (server_dir / ".env").read_text(encoding="utf-8")
    assert f"DB_ROOT_PASSWORD={secret}" in env.splitlines()

    for name in composegen.COMPOSE_FILES:
        path = server_dir / name
        if not path.is_file():
            continue
        text = path.read_text(encoding="utf-8")
        assert secret not in text, f"the generated password reached {name}"
        assert "${DB_ROOT_PASSWORD:-" not in text, f"{name} still falls back to a literal"
    base = (server_dir / composegen.BASE_FILE).read_text(encoding="utf-8")
    assert "${DB_ROOT_PASSWORD:?" in base


def test_the_db_password_stage_is_never_recorded_and_comes_before_the_compose_file() -> None:
    """Ordering is what makes "a compose file implies a record" true for every new install.

    The record exists before `generate-compose` writes the compose file, and before the
    database starts."""
    names = [stage.name for stage in engine(Recorder()).stages()]
    assert names.index("db-password") < names.index("generate-compose") < names.index("start-db")
    assert names.index("clone-core") < names.index("db-password"), "the clone needs an empty folder"
    stage = db_password_stage(engine(Recorder()))
    assert stage.recorded is False


def test_a_fresh_folder_mints_and_writes_a_password_the_stage_can_check(tmp_path: Path) -> None:
    """The stage itself, not only the run: a fresh folder gets a `wotlk-` value written."""
    server_dir = tmp_path / "wow"
    server_dir.mkdir()
    stage = db_password_stage(engine(Recorder()))
    lines = list(stage.run(stage_context(server_dir)))
    assert (server_dir / ".db_password").read_text(encoding="utf-8") == GENERATED + "\n"
    assert any(".db_password" in line for line in lines)


def test_a_fresh_wotlk_folder_refuses_to_mint_next_to_an_existing_database(tmp_path: Path) -> None:
    """A volume that already holds a database was initialised with a password this folder does
    not know. Minting a new one there would lock the install out of its own characters."""
    server_dir = tmp_path / "wow"
    server_dir.mkdir()
    rec = Recorder()
    rec.volumes.add(db_volume(server_dir))
    stage = db_password_stage(engine(rec))
    with pytest.raises(native.InstallerError) as refusal:
        list(stage.run(stage_context(server_dir)))
    assert ".db_password" in str(refusal.value)
    assert db_volume(server_dir) in str(refusal.value)
    assert not (server_dir / ".db_password").exists()


def test_an_old_folder_keeps_the_published_password_and_the_stage_writes_nothing(
    tmp_path: Path,
) -> None:
    """The rule for an install made before this change: no record, and its own compose file,
    means `password`."""
    server_dir = tmp_path / "wow"
    an_old_folder(server_dir)
    eng = engine(Recorder())
    assert eng.resolve_secrets(server_dir).db_password == DEFAULT_DB_ROOT_PASSWORD
    list(db_password_stage(eng).run(stage_context(server_dir)))
    assert not (server_dir / ".db_password").exists(), "an old install must not get a record"


def test_a_copy_kept_when_a_new_wotlk_install_was_purged_is_written_back(tmp_path: Path) -> None:
    """ "Keep my characters" on a new install: the same password comes back with the same volume."""
    server_dir = tmp_path / "wow"
    server_dir.mkdir()
    install_id = composegen.install_id(server_dir, platform_id=lambda: "linux")
    dbsecret.remember(
        ENTRY.id,
        install_id,
        password=GENERATED,
        volume=db_volume(server_dir),
    )
    rec = Recorder()
    rec.volumes.add(db_volume(server_dir))
    eng = engine(rec)
    assert eng.resolve_secrets(server_dir).db_password == GENERATED
    list(db_password_stage(eng).run(stage_context(server_dir)))
    assert (server_dir / ".db_password").read_text(encoding="utf-8") == GENERATED + "\n"


# -- the catalog and its accessor ------------------------------------------


@pytest.mark.parametrize(("game", "prefix"), [("wow-wotlk", "wotlk-"), ("wow-unbound", "unbound-")])
def test_wotlk_and_unbound_plan_a_generated_password_with_the_published_one_as_legacy(
    game: str, prefix: str
) -> None:
    plan = load_catalog().get(game).install.password
    assert plan.mode == "generated"
    assert plan.file == ".db_password"
    assert plan.prefix == prefix
    assert plan.legacy == DEFAULT_DB_ROOT_PASSWORD


def test_db_password_is_the_record_then_the_old_folders_value_then_nothing(tmp_path: Path) -> None:
    """The accessor every reader calls: a record wins, an old folder answers `password`, a fresh
    folder answers nothing, so no caller can take a guess for a password."""
    wotlk = load_catalog().get("wow-wotlk").install
    assert wotlk.db_password(tmp_path) is None, "a fresh folder has nothing knowable"

    an_old_folder(tmp_path)
    assert wotlk.db_password(tmp_path) == DEFAULT_DB_ROOT_PASSWORD

    (tmp_path / ".db_password").write_text(GENERATED + "\n", encoding="utf-8")
    assert wotlk.db_password(tmp_path) == GENERATED


def test_an_old_folder_uses_the_db_root_password_its_own_env_already_sets(tmp_path: Path) -> None:
    """Compose already honoured `DB_ROOT_PASSWORD` in `.env`; an old folder keeps that value."""
    an_old_folder(tmp_path)
    (tmp_path / ".env").write_text("DB_ROOT_PASSWORD=mine-123\n", encoding="utf-8")
    assert load_catalog().get("wow-wotlk").install.db_password(tmp_path) == "mine-123"


# -- the readers ------------------------------------------------------------


def test_the_console_tab_hands_a_new_install_its_own_generated_password(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """RED through the Server tab's SQL seam: Create account is sent with the folder's password.

    This is the same end-to-end check the TBC, Vanilla and Tortoise tabs already have
    (`test_a_generated_password_game_is_never_handed_the_fixed_one`), asked of WotLK.
    """
    from yulon.ui import controller_view as controller_view_module

    (tmp_path / ".db_password").write_text(GENERATED + "\n", encoding="utf-8")
    handed_to_sql: list[str] = []

    def fake_run(argv: list[str], **kwargs: object) -> subprocess.CompletedProcess[str]:
        env = kwargs.get("env")
        if isinstance(env, dict) and "MYSQL_PWD" in env:
            handed_to_sql.append(str(env["MYSQL_PWD"]))
        return subprocess.CompletedProcess(argv, 0, "" if len(handed_to_sql) < 2 else "1", "")

    monkeypatch.setattr(subprocess, "run", fake_run)
    services = controller_view_module.ControllerServices.for_wotlk(ENTRY, tmp_path, None)
    services.create_account("bob", "hunter2", 0)

    assert handed_to_sql, "nothing was sent to mysql at all"
    assert set(handed_to_sql) == {GENERATED}, handed_to_sql


def test_the_backup_seam_hands_mysql_the_folders_password(tmp_path: Path) -> None:
    """RED through the Backup and Restore seam: `DockerMysql` is built with the new password,
    and the password reaches the client only as `MYSQL_PWD` in its environment."""
    from yulon.ui import controller_view as controller_view_module

    (tmp_path / ".db_password").write_text(GENERATED + "\n", encoding="utf-8")
    password = controller_view_module._db_password(ENTRY, tmp_path)
    mysql = controller_view_module._mysql_for(ENTRY, password, wsl_distro=None)
    assert mysql.root_password == GENERATED
    assert mysql_env(mysql.root_password)["MYSQL_PWD"] == GENERATED


def test_the_backup_snapshot_for_an_update_reads_the_folders_password(tmp_path: Path) -> None:
    """The update route's database copy (`install_wiring._MaintenanceSnapshot`) is a backup too."""
    from yulon import install_wiring

    (tmp_path / ".db_password").write_text(GENERATED + "\n", encoding="utf-8")
    snapshot = install_wiring._MaintenanceSnapshot(ENTRY, wsl_distro=None)
    assert snapshot._mysql(tmp_path).root_password == GENERATED


def test_module_sql_with_no_runner_given_uses_the_folders_password(tmp_path: Path) -> None:
    """`modules.applier()` builds its own `DockerSql` when no runner is passed; that must be the
    folder's password, not the constant."""
    from yulon.controller_wow_wotlk import modules

    (tmp_path / ".db_password").write_text(GENERATED + "\n", encoding="utf-8")
    applier = modules.applier(tmp_path, world_running=lambda: False)
    assert applier.sql is not None
    assert applier.sql.root_password == GENERATED


def test_the_import_probe_reads_the_password_of_the_folder_it_is_given(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """The import gate asks the database with the folder's password. It is the one seam that
    gets no folder today; this asks it for one."""
    from yulon import install_wiring
    from yulon.controller_wow_wotlk import repair as wotlk_repair

    (tmp_path / ".db_password").write_text(GENERATED + "\n", encoding="utf-8")
    seen: list[str] = []

    def fake_import_state(sql: object, mysql: object, _health: object) -> object:
        seen.append(str(getattr(sql, "root_password", "?")))
        return object()

    monkeypatch.setattr(wotlk_repair, "import_state", fake_import_state)
    probe, _reset = install_wiring.import_gate_for(ENTRY, server_dir=tmp_path)
    assert probe is not None
    probe()
    assert seen == [GENERATED]


def test_reset_defaults_reads_an_old_folders_published_password(tmp_path: Path) -> None:
    """Reset server settings to default writes the password into the confs. An old folder's
    is `password`."""
    from yulon import reset_defaults

    an_old_folder(tmp_path)
    seams = platform_seams_for_tests()
    assert reset_defaults._password(ENTRY, tmp_path, seams) == DEFAULT_DB_ROOT_PASSWORD


def platform_seams_for_tests() -> object:
    """The Seams `reset_defaults` takes, pinned to Linux as the other tests pin it."""
    from yulon.catalog import native as native_module

    return native_module.Seams(platform_id=lambda: "linux")


def test_the_shipped_wotlk_template_asks_for_the_password_from_env() -> None:
    """A template that spells the old `${DB_ROOT_PASSWORD:-{{DB_PASSWORD}}}` is a fixed-password
    template, and a generated one must not carry the token at all."""
    template = resources.installers_dir() / "wow-wotlk" / "native" / "base.yml.tmpl"
    text = template.read_text(encoding="utf-8")
    assert "{{DB_PASSWORD}}" not in text
    assert "${DB_ROOT_PASSWORD:-" not in text
    assert "${DB_ROOT_PASSWORD:?" in text


def test_the_generated_password_is_never_printed_by_the_install(tmp_path: Path) -> None:
    """Not in the install's own log lines: a support file and the console echo carry those."""
    server_dir = tmp_path / "wow"
    lines = install(Recorder(images=False), server_dir)
    secret = (server_dir / ".db_password").read_text(encoding="utf-8").strip()
    printed = "\n".join(lines)
    assert secret not in printed
    assert secret.removeprefix("wotlk-") not in printed


def test_the_engines_import_gate_answers_for_the_folder_it_is_given(tmp_path: Path) -> None:
    """The install engine's refusals and import stage ask the gate for their own folder."""
    seen: list[Path] = []

    def gate(server_dir: Path) -> tuple[object, object]:
        seen.append(server_dir)
        return (None, None)

    eng = AzerothCoreInstaller(
        ENTRY,
        installers_root=resources.installers_dir(),
        import_gate=gate,  # type: ignore[arg-type]
        seams=Recorder().seams(),
    )
    folder = tmp_path / "wow"
    eng._gate_for(folder)
    assert seen == [folder]
