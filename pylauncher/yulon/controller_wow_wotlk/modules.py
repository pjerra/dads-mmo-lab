"""Module / ALE / mod / keg management for WotLK, driven by JSON manifests (roadmap 2.3).

This is the per-game binding only: the WotLK game id, where its bundled
manifests live, the GitHub location they are refreshed from, and the
AzerothCore database container the SQL steps run in. The behavior —
loading/validating (`yulon.manifest_store`), applying (`yulon.apply`) — is
shared and game-agnostic (style-guide §4). Nothing here knows what any
particular module does; that is all in `manifests/wow-wotlk/` (§3).
"""

from __future__ import annotations

from collections.abc import Callable, Mapping
from pathlib import Path

from yulon import docker, resources
from yulon.apply import Applier, ApplyReport, DbcCopier, DockerSql, SqlRunner
from yulon.controller_wow_wotlk import docker_ctl
from yulon.git import Git

# Explicit re-export (the `as` form is what mypy's --no-implicit-reexport asks for):
# the value lives in `install_wiring` since 7.1, and this name stays so no importer moves twice.
from yulon.install_wiring import DEFAULT_DB_ROOT_PASSWORD as DEFAULT_DB_ROOT_PASSWORD
from yulon.manifest import Manifest, ManifestType
from yulon.manifest_store import (
    HttpGet,
    ManifestFetcher,
    ManifestStore,
    RefreshResult,
    load_manifest,
    urllib_get,
)

GAME = "wow-wotlk"

# The bundled manifest tree that ships with the app (source tree or PyInstaller bundle).
BUNDLED_MANIFESTS_DIR = resources.manifests_dir()

# Raw-GitHub prefix that `<game>/<family>.json` is joined onto when refreshing.
MANIFEST_BASE_URL = (
    "https://raw.githubusercontent.com/DadsMmoLab/dads-mmo-lab/main/pylauncher/manifests"
)


def load_module(manifest_path: Path) -> Manifest:
    """Load and validate a single manifest file into a typed `Manifest`.

    Raises `yulon.manifest_store.ManifestError` for a missing/unreadable file and
    `pydantic.ValidationError` for anything the schema rejects — including a
    `source.repo` outside the allow-list (README §3a).
    """
    return load_manifest(manifest_path)


def store(root: Path = BUNDLED_MANIFESTS_DIR) -> ManifestStore:
    """The WotLK manifest store over `root` (bundled by default, or a refreshed cache)."""
    return ManifestStore(root, GAME)


def fetcher(cache_root: Path, http: HttpGet = urllib_get) -> ManifestFetcher:
    """A fetcher that mirrors the WotLK manifests from GitHub into `cache_root`."""
    return ManifestFetcher(MANIFEST_BASE_URL, cache_root, http)


def refresh(cache_root: Path, kind: ManifestType, http: HttpGet = urllib_get) -> RefreshResult:
    """Refresh one family of WotLK manifests into `cache_root` (ETag-revalidated)."""
    return fetcher(cache_root, http).refresh(GAME, kind)


def applier(
    server_dir: Path,
    *,
    git: Git | None = None,
    sql: SqlRunner | None = None,
    client_dir: Path | None = None,
    dbc: DbcCopier | None = None,
    db_root_password: str = DEFAULT_DB_ROOT_PASSWORD,
) -> Applier:
    """An `Applier` for the WotLK install at `server_dir`.

    Defaults to running direct SQL through the WotLK DB container
    (`docker_ctl.SPEC.db`); pass `sql=None` explicitly via a caller that has no
    database to get every SQL step reported as skipped instead.
    """
    runner: SqlRunner | None = sql
    if runner is None:
        runner = DockerSql(docker_ctl.SPEC.db, db_root_password, client=docker_ctl.DB_CLIENT)
    return Applier(server_dir, git=git, sql=runner, client_dir=client_dir, dbc=dbc)


def apply_module(
    manifest: Manifest,
    server_dir: Path,
    values: Mapping[str, str] | None = None,
    *,
    client_dir: Path | None = None,
) -> ApplyReport:
    """Install `manifest` into the WotLK server at `server_dir` (the roadmap 2.3 entry point).

    Returns the `ApplyReport`; the caller decides about the rebuild/restart it
    names (call down / signal up — this function never touches Docker's
    lifecycle itself).
    """
    return applier(server_dir, client_dir=client_dir).install(manifest, values)


def apply_module_sql(
    server_dir: Path,
    *,
    output: Callable[[str], None] | None = None,
    wsl_distro: str | None = None,
) -> docker.AttachedRun:
    """Run the AzerothCore importer over `server_dir` for the modules that are ON DISK.

    The route that was missing, and the reason it had to exist. `apply_module()`
    installs a C++ module by cloning it and activating its conf; its
    `data/sql/db-world/*.sql` is left "to `ac-db-import` on next start", which
    is a promise nothing in this app kept — `docker.start_staged()` names the
    three long-running services so a Start can never reach the importer, and
    `docker.repair_import()` refuses any install whose databases are already
    complete. Measured on yulon-ubuntu 2026-09-07: even when the importer IS
    run, it logs `Loading modules: all` and applies nothing, because `"all"` is
    the list compiled into the image and not the folder on disk
    (`docker.allowed_modules()` has the numbers).

    Per-game and nothing else: which compose service imports is
    `docker_ctl.SPEC.import_service`, asked rather than respelled here
    (style-guide §3). Every refusal — ownership, strangers, and 8.7a's "not
    while the world is running" — belongs to `docker.apply_module_sql()`, so
    this route and the repair cannot come to disagree about whose install it is
    or when it is safe to write.

    `output` receives the importer's lines as they arrive; the `>> Applying
    update <file>.sql` lines among them are the only evidence that a module's
    SQL was applied, so a caller reporting to a user should pass one.

    `wsl_distro` says which daemon this install's containers are inside, and it
    is here because the route below takes one and this binding is what a caller
    holding the install reaches it through. Without it the whole run — the
    `compose config` probe, the database start and the importer itself — goes
    to the Windows host's Docker, which has never heard of `ac-database`, and
    the user reads "Docker could not be found on this machine" from a tab whose
    other buttons are working (the shape of the 2026-08-27 Discord report).
    `test_controller_view.py`'s seam scan is what found this one, on the day
    its first call site was written.
    """
    return docker.apply_module_sql(
        docker_ctl.SPEC, server_dir, output=output, wsl_distro=wsl_distro
    )
