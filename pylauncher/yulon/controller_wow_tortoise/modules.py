"""Module / mod management for Tortoise, driven by JSON manifests (roadmap 8.7d).

The per-game binding only — the TBC sibling's shape, with this fork's facts:
the `wow-tortoise` game id, where its bundled manifests live, the GitHub
location they are refreshed from. Loading and validating
(`yulon.manifest_store`) and applying (`yulon.apply`) are shared and
game-agnostic (style-guide §4).

**What a "module" is here.** The same two shapes as TBC, for the same reason:
this fork compiles no loadable modules either, so an item is

* a **configuration activation** — `conf[].keys` written into
  `etc/mangosd.conf`, which the installer materialises out of the image; or
* a **SQL mod** — `sql[].statement` run against `tw_world`.

**What is NOT inherited from TBC, and this is the whole reason 8.7d is its own
box.** Three facts were measured against this fork's own source on m910q,
2026-09-08, and each of them contradicts the sibling:

1. **There is no `.server motd`.** TBC's gate manifest was chosen because
   `.server motd` states the value back on the console. This fork's
   `serverCommandTable` is `corpses / exit / idlerestart / idleshutdown / info /
   resetallraids / restart / shutdown` (`src/game/Chat/Chat.cpp:700-711`) and
   `motd` appears in no command table at all. The item whose key this server
   DOES report is `perf-report`: `.perf intervalreport` prints
   `Performance report interval is <n>` from the conf key `Perf.ReportInterval`
   (`Commands.cpp:19146-19154`, command row `Chat.cpp:837` with
   `allowConsole = true`).
2. **`world` is `tw_world`**, not `mangos` and not `acore_world`, and this
   install's database password is generated into `<server_dir>/.db_password` —
   so `sql` is REQUIRED here exactly as it is on TBC, and for the same closed
   bug: a second derivation of a password is how a runner authenticates with the
   wrong one.
3. **This fork runs its own database auto-updater at every startup**, inside the
   worldserver, and cancels the world with `exit(1)` on a single failed
   migration. `controller_wow_tortoise.autoupdate` is the guard checklist 2504
   asks for, and `applier()` below returns a `GuardedApplier` rather than a
   plain `Applier` so the object the Modules tab holds is the guarded one. That
   is the only structural difference from the TBC binding.

`modules`/`ale`/`kegs` indexes exist and are empty: "there are none" is a fact
worth writing down, and without the files the Modules tab prints
`!! could not load modules: manifest file missing` for each of the three.
"""

from __future__ import annotations

from collections.abc import Callable, Mapping
from pathlib import Path

from yulon import resources
from yulon.apply import ApplyReport, SqlRunner
from yulon.controller_wow_tortoise import autoupdate
from yulon.controller_wow_tortoise.autoupdate import Arming, GuardedApplier
from yulon.git import Git
from yulon.manifest import Manifest, ManifestType
from yulon.manifest_store import (
    HttpGet,
    ManifestFetcher,
    ManifestStore,
    RefreshResult,
    load_manifest,
    urllib_get,
)

GAME = "wow-tortoise"

# The bundled manifest tree that ships with the app (source tree or PyInstaller bundle).
BUNDLED_MANIFESTS_DIR = resources.manifests_dir()

# Raw-GitHub prefix that `<game>/<family>.json` is joined onto when refreshing.
MANIFEST_BASE_URL = (
    "https://raw.githubusercontent.com/DadsMmoLab/dads-mmo-lab/main/pylauncher/manifests"
)


def load_module(manifest_path: Path) -> Manifest:
    """Load and validate a single manifest file into a typed `Manifest`."""
    return load_manifest(manifest_path)


def store(root: Path = BUNDLED_MANIFESTS_DIR) -> ManifestStore:
    """The Tortoise manifest store over `root` (bundled by default, or a refreshed cache)."""
    return ManifestStore(root, GAME)


def fetcher(cache_root: Path, http: HttpGet = urllib_get) -> ManifestFetcher:
    """A fetcher that mirrors the Tortoise manifests from GitHub into `cache_root`."""
    return ManifestFetcher(MANIFEST_BASE_URL, cache_root, http)


def refresh(cache_root: Path, kind: ManifestType, http: HttpGet = urllib_get) -> RefreshResult:
    """Refresh one family of Tortoise manifests into `cache_root` (ETag-revalidated)."""
    return fetcher(cache_root, http).refresh(GAME, kind)


def applier(
    server_dir: Path,
    *,
    sql: SqlRunner | None,
    arming: Callable[[], Arming],
    world_running: Callable[[], bool | None],
    start_database: Callable[[], bool] | None = None,
    git: Git | None = None,
    client_dir: Path | None = None,
) -> GuardedApplier:
    """A GUARDED `Applier` for the Tortoise install at `server_dir`.

    `sql` is required rather than defaulted, exactly as on TBC: this game's
    database password is generated at install time into
    `<server_dir>/.db_password` and its schemas are `tw_*`, so the caller that
    already holds the correct `DockerSql` hands it over and `sql=None` means
    "no database", which reports every SQL step as skipped rather than running
    it somewhere else.

    `arming` and `world_running` are the two readings checklist 2504's guard
    needs, and they are callables rather than values because the answer changes
    between the moment the tab is built and the moment a user presses Install —
    a world can be started or stopped in between, and a guard that decided at
    construction time would be guarding a fact about the past. `read_arming()`
    is the real one; a caller with no running install passes something cheap.

    `world_running` answers TWO guards on this game and one everywhere else,
    which is why it is three-valued here since T7. `GuardedApplier`'s own check
    is checklist 2504's (would this restart re-enter the fork's auto-updater?)
    and the base's is 8.7a's (is a live world holding these tables?). They read
    the same fact and must not be able to disagree about it, so one callable
    goes to both — the subclass used to swallow the keyword, leaving 8.7a's
    guard at `None` on this game as on the other three.

    No `dbc=`: `server_dbc` copies DBC files out of a clone, and nothing in
    `manifests/wow-tortoise/` clones anything.
    """
    return autoupdate.guarded_applier(
        server_dir,
        sql=sql,
        arming=arming,
        world_running=world_running,
        start_database=start_database,
        git=git,
        client_dir=client_dir,
    )


def apply_module(
    manifest: Manifest,
    server_dir: Path,
    values: Mapping[str, str] | None = None,
    *,
    sql: SqlRunner | None,
    arming: Callable[[], Arming],
    world_running: Callable[[], bool | None],
    start_database: Callable[[], bool] | None = None,
    client_dir: Path | None = None,
) -> ApplyReport:
    """Install `manifest` into the Tortoise server at `server_dir`, guard first.

    Returns the `ApplyReport`; the caller decides about the restart it names
    (call down / signal up — this function never touches Docker's lifecycle
    itself). Raises `autoupdate.AutoUpdateRefused` rather than returning a
    report when that restart would re-enter this fork's own auto-updater.
    """
    return applier(
        server_dir,
        sql=sql,
        arming=arming,
        world_running=world_running,
        start_database=start_database,
        client_dir=client_dir,
    ).install(manifest, values)
