# Design — a module from a link or a folder (Modules tab, WoW WotLK)

> Written 2026-09-08 on `yulon-phase8b` at `e40f4590`, as a design only: no code exists for it.
> Inputs: `pyplan/checklist.md` (the 8.7 boxes and what they measured), `phase8-owner-answers-2026-09-08.md`,
> `phase8-decisions.md`, `phase8-parity-decisions.md`, `style-guide.md`, `contribution.md`,
> `write-ledger.md`, `bug-checklist.md`, `rust-prior-art.md`, and the tree itself (`manifest.py`,
> `manifest_store.py`, `apply.py`, `git.py`, `docker.py`, `ui/controller_view.py`,
> `controller_wow_wotlk/modules.py`, the 21 shipped `manifests/wow-wotlk/modules/*.json`). Prior
> art read at `origin/rust-main`; every `RUST` citation below is `path:line` on that branch.
> Paths are relative to `pylauncher/`. Nothing here was run.
>
> **Where the plan is silent, this page says so rather than deciding quietly.** Three silences,
> named up front: (1) no `8.x` box in `checklist.md` names this feature — the 8.7 line covers update
> checks and the CMaNGOS manifests, and `phase8-parity-decisions.md` §"The cut" lists module
> management "beyond install/remove" as *update checks, manifests for the three CMaNGOS games,
> tuning knobs, config editor, settings page, account-wide sharing* (owner answer 8iii) and does not
> mention a user-supplied source at all; the box, its number and its gate line are the owner's
> session's to write. (2) `README.md` §3a says a manifest's `repo` must point at a legitimate
> open-source project and `manifest.py` spells that as three hosts; neither page says what a link
> the USER pastes may point at — decided below (§1.3), with the reason. (3) No page says what happens
> to a custom module's record on Remove — decided below (§2.4), with the rust-main behaviour as the
> reason.

---

## 0. The decision

**Two buttons on the Modules tab — "Install from link…" and "Install from folder…" — each
DERIVE a `Manifest` the shipped schema already understands, hand it to the SAME `Applier.install()`
every shipped manifest goes through, PERSIST it under the app's config directory so the list shows
it on the next start, and draw it in the list as a custom module. Nothing new is taught to the
applier about what a module is: the only thing it learns is a second way to put a folder at
`modules/<id>` (a copy instead of a clone) and a hook to finish a manifest from what the clone
turned out to contain. Remove, "Check for updates", "Apply module SQL" and the rebuild control all
work on the result because it is a folder under `modules/` with this app's own claim in it, which is
all any of them ever looked at.**

What rust-main did, in one paragraph, so the divergences below are legible: `dml wow module
install --url <u>` derived a key from the URL's basename and refused anything not `mod-*`
(`RUST crates/dml-wow/src/modmgr.rs:1762-1782`, `modules.rs:42-50`); cloned it `--depth 1`
(`modmgr.rs:1812-1820`); activated a `.conf.dist` only if a REGISTRY knew the module's conf name, so
a custom module got `conf: "none"` (`moduletail.rs:142-150`, `modules.rs:308`); persisted nothing —
custom modules were re-discovered on every list from `modules/*/.git` plus `git remote get-url
origin` (`modules.rs:279-312`, `:198-202`); and listed them with the description *"Custom module
(cloned from a URL you provided)."* (`modules.rs:38`, verbatim from `cli/src/90-main.sh:5394`). It had
**no folder route at all** — grepped for on 2026-09-08: nothing. The brief that asked for this page
quoted the description as "from a link you provided"; the line on the branch says "cloned from a
URL you provided", and that is what this design uses for links.

---

## 1. Deriving a manifest from a repository or folder that has none

Lives in a **new `yulon/module_source.py`** (lane A). Pure functions over strings and a directory
read; no Qt, no git, no Docker, so every refusal sentence is asserted without a display.

### 1.1 The id — from the repository or folder name, `mod-*` or refused

```
_CUSTOM_ID = re.compile(r"^mod-[a-z0-9-]{1,64}$")
```

rust-main's rule, ported exactly (`RUST modules.rs:40-50`: `^mod-[a-z0-9-]{1,64}$`; the tests at
`:390-397` pin `mod-` alone, `mod-UPPER` and `mod-under_score` as refused). The candidate is the
URL's last path segment with a trailing `/` and ONE trailing `.git` stripped, lower-cased
(`RUST modmgr.rs:198-207`); for a folder it is the folder's basename, lower-cased. It must also pass
`manifest.Slug`, which it does by construction (`_CUSTOM_ID` is a subset of `_SLUG`).

Why `mod-*` and not any slug: AzerothCore's `modules/` directory holds `CMakeLists.txt`,
`ModulesLoader.cpp.in.cmake` and friends beside the modules (read off yulon-ubuntu 2026-09-07,
`docker.allowed_modules()` docstring), every one of the 21 shipped module ids starts `mod-`, and a
lower-cased basename is also what the folder under `modules/` will be called — so the rule is the
one thing that keeps a pasted `https://github.com/you/Tools.git` from putting a folder named
`tools` next to the build system's own files.

### 1.2 Type, name, description, build, prompts

| field | value | why |
|---|---|---|
| `type` | `"module"` | The only family a bare repository can be: it is cloned into `modules/` and compiled. ALE scripts, kegs and SQL mods need knowledge (deploy targets, the statement) no repository carries. |
| `name` | the id | rust-main: `"name": key` (`RUST modules.rs:303`). Nothing better is known. |
| `description` | link: `"Custom module (cloned from a URL you provided)."` — `RUST modules.rs:38` verbatim. Folder: `"Custom module (copied from a folder you provided)."` — new; no prior art. | The list line is `[{type}] {name} — {description}` (`controller_view.py:4084-4086`), so the description IS the custom marker and the view needs no new code to draw it. |
| `build.rebuild` | `True` | It is a C++ module; `_format_report` says so with the rebuild sentence (`controller_view.py:4611-4625`). `build.restart` stays `False`: the derivation cannot know. |
| `prompts` | `()` | Nothing is known to ask. `_module_values()` therefore opens no dialog and passes `None`, byte-for-byte the shipped path (`controller_view.py:4114-4137`). |
| `requires`, `conflicts_with`, `deploy`, `patches`, `client`, `server_dbc`, `npcs` | `()` | Unknown; not invented. |
| `notes` | one line: `"Derived by Yu'lon from <url or path> on <date>; nothing here was written by the module's author."` | Tacit knowledge for a human reading the file (`Manifest.notes` docstring). |
| `game` | the entry's id, `wow-wotlk` | Passed in; §4 says why only this one. |

### 1.3 `source` — the link, and the allowed-host rule for a pasted one

**A link is a `Source`, unchanged.** `derive_link(text, game)` strips whitespace and hands `text`
to `Source(repo=text)`; whatever `Source._repo_is_allowed` accepts is accepted — `owner/name` for
GitHub, or `https://` on `github.com`, `gitlab.com`, `codeberg.org` — and whatever it refuses is
refused with this page's sentence (§3.4), which quotes `ALLOWED_REPO_HOSTS` rather than retyping
the hosts. `branch`, `rev`, `sparse_path` stay `None`; `depth` stays the default `1`
(`RUST modmgr.rs:1812-1820` cloned `--depth 1` too).

**The allow-list applies to a pasted link, and this is a decision the pages do not make.**
`README.md` §3a ("Manifests must not reference or link to piracy sources… reject any manifest whose
`repo` isn't an allowed source", lines 102-103) is about what the app SHIPS; rust-main accepted any
`https://` URL (`RUST modmgr.rs:188-193`). Two reasons this design keeps the narrower rule for user
input: the derived manifest is PERSISTED and read back through `parse_manifest` on every start
(§2), so a link the store's validator would refuse on reload must be refused at the press or the
list shows `!! could not load modules: …` next morning with nothing the user can do about it; and
widening `ALLOWED_REPO_HOSTS` is a one-tuple change in `manifest.py` if the owner wants it, whereas
a second validator for "user links" would be the kind of duplicate `style-guide.md` §4 forbids.
**Flagged for the owner:** a self-hosted Gitea/Forgejo link is refused today, by this rule.

**A local folder is NOT a `Source` variant.** Three reasons. `Source` means "where content is
cloned from" and its `url` property feeds `git.CloneSpec` — a path is not a clone URL and a
`file://` one would drag `RunnerGit`/`ContainerGit`, their HTTP/1.1 and autocrlf pins and the
container mount logic into a job that is a `copytree`. `manifest_store.py` and `README.md` §3a
treat `repo` as the piracy fence; a local path is the user's own disk and should not need to pass
through — or weaken — that fence. And `same_repo()`/`remote_url()`, which every ownership guard
asks about a `source`, have no meaning for a copy. So a folder-derived manifest has `source=None`
and records where it came from in a new, small, machine-read field:

```python
class Origin(_Strict):
    """How a CUSTOM manifest came to exist: derived by this app, not shipped by the project."""
    kind: Literal["link", "folder"]
    path: str | None = None      # the folder it was copied from (kind="folder"); None for a link
    added: str                    # ISO date, for a human and for the notes line

class Manifest(_Strict):
    ...
    origin: Origin | None = None
```

`_shape_by_type` relaxes in one clause: `type="module"` requires a `source` **unless**
`origin is not None and origin.kind == "folder"`. Every shipped manifest has `origin=None` and is
validated exactly as before (`test_module_ale_keg_require_a_source_and_keg_requires_sparse_path`
stays green unchanged). The schema copy `manifests/schema/manifest.schema.json` is regenerated
(`test_checked_in_json_schema_is_current`).

### 1.4 Conf — discovered from `conf/*.conf.dist`, activated, no keys written

For each `<clone>/conf/<name>.conf.dist` (top level of `conf/` only, sorted):

```json
{"file": "env/dist/etc/modules/<name>.conf", "template": "conf/<name>.conf.dist", "keys": []}
```

That is the shape all 20 shipped conf-bearing manifests use (grepped 2026-09-08: every `template`
is `conf/<name>.conf.dist`, every `file` is `env/dist/etc/modules/<name>.conf`), and
`Applier._conf()` then does exactly what it does for them: copy the template into place if the
target does not exist, write no keys (`apply.py:1665-1686`). rust-main activated a fresh clone's
conf "with defaults" and never overwrote an existing one (`RUST modmgr.rs:1848-1858`) — same
outcome — but only when a registry knew the conf NAME, which a custom module never had
(`moduletail.rs:142-150`); discovering the name from the glob is the one place this page does more
than the prior art. No bounded deep `find` (rust-main's `maxdepth 4`, `moduletail.rs:104-136`): a
`.conf.dist` two levels down is somebody's example, not the module's, and guessing would activate
it.

### 1.5 SQL — discovered from `data/sql/<dbdir>/`, one step per database, left to the importer

For each directory under `<clone>/data/sql/`, mapped by name and otherwise ignored:

| directory | `db` |
|---|---|
| `db-world`, `db_world` | `world` |
| `db-characters`, `db_characters` | `characters` |
| `db-auth`, `db_auth` | `auth` |
| `playerbots` | `playerbots` |

— `RUST modmgr.rs:2625-2633` (`sql_dbdir_target`), both spellings, verbatim. Each becomes

```json
{"db": "<db>", "path": "data/sql/<dbdir>/**/*.sql", "applied_by": "db-import"}
```

`applied_by="db-import"` because that is the default the schema documents for C++ modules ("applying
those by hand breaks that tracking", `SqlStep` docstring) and what 20 of the 21 shipped module
steps say; the step is then REPORTED as pending with the files the glob found (`apply.py:1623-1648`,
`PendingSql`) and applied by the module-SQL route that already exists, whose importer is handed the
folder list from disk (`docker.allowed_modules()`, `docker.apply_module_sql()`). The glob is
recursive (`**`) where the shipped manifests' is flat, because a derivation cannot know whether the
author used `base/`/`updates/` subfolders (rust-main read `updates/` only, `modmgr.rs:2581-2608`)
and the only thing the glob decides is what the REPORT lists — `pathlib.Path.glob` resolves it
(`_is_glob()` sees the `*`), and whether the importer applies a nested file is the importer's
business and shows in its own `>> Applying update` lines. A `data/sql/` with no mapped directory
yields no steps; a module with none at all yields none.

### 1.6 Two functions, one hook — because a link's contents are not known until it is cloned

```python
def derive_link(text: str, game: str, *, today: date) -> Manifest      # minimal, or DeriveError
def derive_folder(path: Path, game: str, *, today: date) -> Manifest   # minimal, or DeriveError
def complete(manifest: Manifest, clone: Path) -> Manifest              # + conf + sql, from disk
```

`derive_*` return the id, name, type, game, description, source/origin, build and notes — enough
to clone or copy, nothing that needs the files. `complete()` reads the folder that now sits at
`modules/<id>` and returns the manifest with §1.4 and §1.5 filled in. It is handed to the applier
as a hook (§B) so the SAME `install()` pass that put the folder there activates the conf it found
and reports the SQL it found; there is no second clone, no `configure()` call and no report that
describes a manifest other than the one applied. `derive_folder` reads the source folder before
anything is copied and refuses (§3.4) a path that is not a directory, or a directory holding none of
`src/`, `conf/`, `data/` — a module contributes code, configuration or data, and a folder with none
of the three is the wrong folder. That rule is this page's; no page names one.

---

## 2. Persisting the derived manifest

### 2.1 Where

```
<config_dir()>/manifests/user/<game>/modules.json          # the family index
<config_dir()>/manifests/user/<game>/modules/<id>.json     # one file per custom module
```

`platform.config_dir()`'s own docstring already reserves it for "cached manifests"
(`platform.py:71`); `credentials/`, `logs/`, `downloads/` and `state.json` live beside it. The
layout under `user/` is byte-for-byte the store's own (`FAMILY_FILES`, `ManifestStore.index_path`,
`item_path`), so the user layer is read by the class that exists with no new parser. Per game
because the store is per game; per kind because the index is per kind — only `modules` is ever
written by this feature, and the other three family indexes simply do not exist there.

### 2.2 How — atomically, and the index rebuilt from the directory

`persist(user_root, manifest)` writes `<id>.json.tmp` then `os.replace`s it over `<id>.json`, then
rewrites `modules.json` from `sorted(p.stem for p in (user_root/game/"modules").glob("*.json"))`
the same way. The index is DERIVED from the files so a crash between the two writes leaves a file
the next rebuild picks up rather than an index naming a file that is not there; and the
`tmp`→`replace` shape is `write_clone_claim()`'s, for its reasons (`apply.py:244-283`). Three write
sites, all in `module_source.py`, all rows in `write-ledger.md` (§5). `forget(user_root, manifest)`
unlinks the item file and rewrites the index; returns `True` if a file was there.

### 2.3 How the store merges — a user file can never shadow a shipped id

```python
class ManifestStore:
    def __init__(self, root: Path, game: str, user_root: Path | None = None) -> None
```

`load_all(kind)` yields the bundled index's items in index order, then the user index's items
**whose id is not in the bundled index**, in user-index order; a user file whose id IS shipped is
skipped with a `logger.warning` naming the file. `load(kind, id)` reads the bundled file if the
bundled index lists the id, else the user file. A MISSING user index (nothing ever persisted, or
the first start) is an empty second layer, not an error; a user index that is present and does not
parse is a `ManifestError`, which the tab already draws as `!! could not load modules: …`
(`controller_view.py:4076-4079`). `relative_files()` and `ManifestFetcher` are untouched: the
fetcher mirrors the project's tree into ITS cache root and never reads or writes under `user/`.
"Bundled or cached" is whatever `root` the caller passes today (`wotlk_modules.store()` passes the
bundled dir; no refresh call site exists in the app — grepped 2026-09-08), and the user layer sits
over either.

`persist()` refuses an id the bundled store lists, before writing (§3.4's sentence), so the shadow
rule is enforced at the press as well as at the read. Both directions, because a file can also
arrive by hand.

### 2.4 What Remove does with the record

**Remove of a custom module also forgets its manifest.** rust-main listed a custom module only
while its clone existed under `modules/` (`RUST modules.rs:279-297`: `.git` present, valid key, no
registry row) — remove the folder, the row is gone. A shipped manifest is an OFFER and stays listed
whether or not it is installed; a custom one is a RECORD of something the user brought, and a record
of a folder that is gone would be a list entry whose Install selected re-clones a link the user
already decided against. The forget runs after `Applier.remove()` returned, never before (a refused
remove keeps the record, so the module is still reachable — the same ordering `purge.py` uses for
`state.forget()`, `phase8-decisions.md` §"`state.forget()` runs last"). No page decides this;
recorded here as the decision and its reason.

---

## 3. The UI — two buttons, two dialogs, one report

### 3.1 The buttons

Two `QPushButton`s in the Modules tab's button row, after "Remove selected" and before the
module-SQL button: `"Install from link…"` and `"Install from folder…"`. The ellipsis is the tab's
own convention for "this opens a dialog first" (`controller_view.py:3999-4001`, the rebuild
button). Enabled only when `services.module_from_link` / `services.module_from_folder` are not
`None` (§3.5), which `for_entry()` wires for `wow-wotlk` alone — the same shape as
`module_updates` and `module_sql`, and for the same reason: a visibly dead control beats a press
that explains itself (`test_a_game_with_no_module_checkouts_gets_no_update_button`). Tooltips name
the refusal the user is most likely to meet, before the click, the way `MODULE_SQL_TIP` does:

- link: *"Paste an https link to a module repository on github.com, gitlab.com or codeberg.org.
  Its name must start with mod-. The module is cloned into this server's modules folder; it does
  nothing until the server is rebuilt."*
- folder: *"Choose a folder on this computer holding a module (its name must start with mod-). It
  is copied into this server's modules folder; the original is not touched, and it does nothing
  until the server is rebuilt."*

### 3.2 The dialogs — seams, with Qt defaults

```python
LinkAsker = Callable[[QWidget, str], str | None]         # (parent, title) -> the text, or cancelled
FolderAsker = Callable[[QWidget, str], Path | None]      # (parent, title) -> the folder, or cancelled
```

Injected into `ControllerView.__init__` beside `prompt_asker`, defaulting to `QInputDialog.getText`
(placeholder `https://github.com/you/mod-my-thing`, the example rust-main printed,
`RUST modmgr.rs:1777`) and `QFileDialog.getExistingDirectory`. Injected for the reason
`prompt_asker` is: a test with the real one sits on a modal dialog forever
(`test_modules_tab_lists_manifests_and_installs_selected`'s comment, 2026-09-07). Cancel changes
nothing and the report says so, in the tab's existing sentence shape:
`install from link: cancelled — nothing on this machine was changed.`

**The folder dialog picks a directory only.** A `.zip` is not taken in v1 — §4 says why.

### 3.3 The flow — the shipped one, with the derivation in front of it

```
press → ask (dialog) → cancelled? report, stop
      → derive on the GUI thread (pure; a refusal is immediate and queues no job)
      → refused? module_report + action_failed, stop; nothing was written
      → _module_pending = "install from link mod-x" (or "from folder")
      → _run( applier.install(m0, None, folder=FolderSource(path, copier) | None,
                               complete=services.module_complete),
              _module_done, _module_failed )
      → _module_done: _format_report(report), then reload_modules()
```

Same `_module_values()` gate (asks nothing: `prompts=()`), same `_run()` off the GUI thread, same
`_module_done`/`_module_failed` slots, same `_format_report` — so the report is the one Install
selected prints, with the C++-module rebuild sentence and the pending-SQL lines the applier found.
`reload_modules()` after a success is the one new line in the done slot: the store reads disk, the
manifest was persisted inside `complete()` (§1.6, §B), so the list now carries
`[module] mod-x — Custom module (cloned from a URL you provided).` and `selected_manifest()` on it
returns a real `Manifest` for Remove. No confirmation: Install selected has none (only the rebuild
does, `rebuild_server()`), and this is Install selected with the manifest typed in.

Remove on a custom module: `_module_action("remove")` unchanged, then in `_module_done`, when the
action was a remove, `services.module_forget(manifest)`; if it returns `True`, `reload_modules()`.
The view never reads `manifest.origin` — the forget seam answers whether there was anything to
forget — so the view has no `isinstance`/field test on lane A's schema.

### 3.4 What an invalid link or folder says — exact sentences, asserted without Qt

Raised as `module_source.DeriveError(str)` and shown verbatim in `module_report`, plus
`action_failed.emit`. Every one ends in the tab's own clause, *"Nothing on this machine was
changed."*, because it is true of every one: the derivation happens before any write.

| case | sentence |
|---|---|
| empty text | `Paste a link first. Nothing on this machine was changed.` |
| not accepted by `Source` | `{text} is not a link this app can clone from — it takes an https link on {", ".join(ALLOWED_REPO_HOSTS)}, or owner/name for github.com. Nothing on this machine was changed.` |
| basename not `mod-*` (link) | `The repository is named {name!r}, and a custom module must be named mod-<something> in lowercase letters, digits and hyphens — for example https://github.com/you/mod-my-thing. Nothing on this machine was changed.` |
| basename not `mod-*` (folder) | `The folder is named {name!r}, and a custom module must be named mod-<something> in lowercase letters, digits and hyphens — rename the folder and choose it again. Nothing on this machine was changed.` |
| id is shipped | `{id} is a module this app already ships — select it in the list and press Install selected. Nothing on this machine was changed.` (raised by `persist()` too, for a file that arrives by hand) |
| folder is not a directory | `{path} is not a folder this app can read. Nothing on this machine was changed.` |
| folder has none of `src/`, `conf/`, `data/` | `{path} does not look like a module: it has no src, conf or data folder. Nothing on this machine was changed.` |
| folder is inside this server's `modules/` (raised by the copier, §B) | `{path} is already inside this server's modules folder — a module is copied into it, not from it. Nothing on this machine was changed.` |

Tested in `tests/test_module_source.py` with no Qt import: each row is one test, each asserting the
sentence and that nothing under a `tmp_path` user root exists afterwards.

### 3.5 Services

```python
@dataclass
class ControllerServices:
    ...
    module_from_link: Callable[[str], Manifest] | None = None       # derive_link, bound to the game
    module_from_folder: Callable[[Path], Manifest] | None = None    # derive_folder, bound to the game
    module_complete: Callable[[Manifest, Path], Manifest] | None = None   # complete + persist
    module_copy_folder: Callable[[Path, Path], None] | None = None  # the copier the applier is handed
    module_forget: Callable[[Manifest], bool] | None = None
```

Plain callables, faked in the view's tests, wired in `for_entry()`'s `wow-wotlk` branch from
`controller_wow_wotlk/modules.py`'s bindings (lane A: `derive_link(text)`, `derive_folder(path)`,
`complete(manifest, clone)`, `copy_folder(src, dest)`, `forget(manifest)`, each closing over
`GAME` and `USER_MANIFESTS_DIR`). `store()` in that module gains `user_root=USER_MANIFESTS_DIR`, so
the view's existing `wotlk_modules.store()` call returns the merged store with no view change.

---

## 4. What is NOT done, and why

- **No build flags, CMake options or per-module compile settings.** A custom module is cloned or
  copied and the rebuild compiles whatever `modules/` holds, exactly as a shipped one. rust-main had
  none either.
- **No CMaNGOS-family custom modules.** The three CMaNGOS games have no `modules/` directory, no
  `include.sh` convention and no module importer; on those trees "a module is a configuration key
  or a SQL mod, never a directory" (checklist 8.7b, gated 2026-09-08), and neither of those can be
  derived from a repository that carries no manifest. The buttons are wired for `wow-wotlk` only
  and disabled elsewhere with the tooltip *"Only WoW WotLK takes custom modules — on this game a
  module is a configuration key or a SQL mod, and those ship as manifests."* `bug-checklist.md` §46
  (no compliant way to install a SQL mod on CMaNGOS while owner answer 7 stands) is unaffected and
  unresolved.
- **No `.zip`.** Qt's native pickers choose a directory or a file, not either; a second button for
  an archive doubles the surface for something the user does with one right-click. Recorded rather
  than half-wired.
- **Only `type="module"`.** No ALE script, keg or SQL mod from a link.
- **No branch, tag or commit choice.** A link is the default branch's tip at depth 1, like rust-main
  (`RUST modmgr.rs:1812-1820`). A user wanting a pin writes a manifest.
- **No conf keys, prompts, `requires`, `conflicts_with`, NPCs, client files or DBCs** are derived.
  The conf is activated with the author's defaults and nothing is written into it; tuning is Phase 9
  (owner answer 8iii).
- **No authentication.** git never prompts (`git._no_prompt_env()`), so a private repository fails
  with git's own last words in the report.
- **No "apply update" for custom modules beyond what 8.7a builds** — they are folders under
  `modules/` with a claim, so whatever 8.7a's update press does to a shipped clone it does to these;
  a folder-copied module has no `.git` and reports `not a git checkout — nothing to compare`
  (`ModuleUpdate.line`), which is the truth.
- **No edit or re-derive of a persisted manifest.** To bring a newer version of a folder, choose
  the same folder again: the guard passes (this app's own claim), the copier replaces the copy, and
  `complete()` re-derives and re-persists.
- **No rebuild is pressed by this feature and none by its gate** (owner rule). The live press stops
  at the report that names the rebuild; §6 gives the command that would settle the compile clause.

---

## 5. The tests, named, and the mutation each must catch

`tests/test_module_source.py` (lane A, no Qt, no git, no Docker):

| test | mutation it catches |
|---|---|
| `test_a_link_yields_a_module_manifest_named_for_its_repository` | id/name not lower-cased basename; `.git` or trailing `/` left in the id; `type` not `module`; `build.rebuild` False |
| `test_a_link_whose_repository_is_not_named_mod_is_refused_by_name` | the `mod-` rule dropped or widened (`mod-` alone, `mod-UPPER`, `mod-under_score` each refused — rust-main's own vectors) |
| `test_a_link_off_the_allowed_hosts_is_refused_with_the_hosts_named` | the derive bypassing `Source`; the sentence not naming `ALLOWED_REPO_HOSTS` |
| `test_an_owner_name_slug_is_a_github_link` | slug support dropped |
| `test_a_folder_yields_a_module_manifest_with_no_source_and_a_folder_origin` | `source` invented for a folder; `origin.kind` wrong; `origin.path` not the folder |
| `test_a_folder_with_none_of_src_conf_or_data_is_refused_and_one_with_any_is_not` | the "looks like a module" rule dropped or over-tightened (a `conf/`-only folder must pass) |
| `test_a_missing_folder_is_refused_before_anything_is_read` | the not-a-directory refusal dropped |
| `test_complete_finds_every_conf_dist_and_maps_it_to_the_modules_conf_dir` | template or file path built wrong; a `.conf.dist` two levels down picked up |
| `test_complete_maps_each_sql_directory_to_its_database_and_ignores_the_rest` | one of the eight spellings → db rows wrong; an unmapped directory (`db-foo`) producing a step; `applied_by` not `db-import`; glob not recursive |
| `test_complete_keeps_the_id_type_and_game_it_was_given` | complete() returning a manifest for another item |
| `test_persist_writes_the_item_and_rebuilds_the_index_from_the_files` | index not rebuilt; item not in it; a second persist duplicating the entry |
| `test_persist_is_atomic_and_a_torn_write_leaves_the_previous_file` | `write_text` straight onto the final name (assert via a `tmp` that raises mid-write, the shape of `test_a_torn_claim_write_never_leaves_a_file_that_reads_as_unknown`) |
| `test_persist_refuses_a_shipped_id_and_writes_nothing` | the shadow rule dropped at the write |
| `test_forget_removes_the_file_and_the_index_entry_and_says_whether_it_did` | forget returning True for a shipped id; the index left naming a gone file |
| `test_every_refusal_sentence_ends_with_nothing_changed_and_no_file_exists` | a refusal path that persisted first |
| `test_copy_folder_replaces_an_existing_copy_and_never_carries_git_metadata` | `.git` copied (a copy is a snapshot); an existing copy merged over instead of replaced |
| `test_copy_folder_refuses_a_source_inside_the_destination` | copying `modules/mod-x` onto itself |

`tests/test_manifest.py` (lane A): `test_a_folder_origin_module_needs_no_source_and_a_link_one_still_does`
(the validator relaxation exactly one clause wide), `test_origin_is_optional_and_every_shipped_manifest_has_none`.
`tests/test_manifest_store.py` (lane A): `test_user_items_follow_bundled_items_and_a_user_file_never_shadows_a_shipped_id`
(order; shadow skipped with a warning; `load()` prefers bundled), `test_a_missing_user_index_is_an_empty_layer_and_a_broken_one_is_an_error`.

`tests/test_apply.py` (lane B):

| test | mutation it catches |
|---|---|
| `test_install_from_a_folder_copies_through_the_seam_and_then_walks_the_same_steps` | the copier not called; `git.clone` called for a folder; claim not written; `include.sh` not touched; conf/sql steps skipped |
| `test_a_folder_install_needs_a_copier_and_says_so` | `FolderSource` accepted with no copier and silently skipped |
| `test_the_complete_hook_runs_after_the_folder_is_there_and_the_steps_read_what_it_returned` | hook called before the clone/copy; the ORIGINAL manifest's (empty) conf/sql used after the hook |
| `test_a_complete_hook_that_changes_the_id_is_refused` | the returned manifest's id/type/game not checked against the input's |
| `test_remove_deletes_a_copied_folder_this_app_claimed_even_without_git` | `_require_own_clone`'s no-`.git` branch still refusing an OWNED claim |
| `test_remove_still_refuses_a_hand_made_folder_without_a_claim` | the relaxation letting any non-git folder through |
| `test_a_second_folder_install_over_this_apps_own_copy_is_allowed_and_a_strangers_is_not` | the install guard for the no-`.git` OWNED case |
| `test_a_folder_source_never_asks_git_anything` | `remote_url`/`is_unmodified` reached for a copy |

`tests/test_controller_view.py` (lane C, fakes only):

| test | mutation it catches |
|---|---|
| `test_the_link_button_derives_installs_and_relists_as_a_custom_module` | the applier not called; `reload_modules()` not called; the custom description absent from the list after |
| `test_a_refused_link_says_the_sentence_and_installs_nothing` | derive error swallowed; job queued anyway |
| `test_cancelling_the_link_dialog_changes_nothing` | cancel treated as empty text |
| `test_the_folder_button_hands_the_applier_a_folder_source_and_the_copier` | `folder=` not passed; copier not passed |
| `test_a_game_with_no_custom_module_route_gets_dead_buttons_that_do_nothing_when_pressed` | buttons enabled with `None` services; a press raising |
| `test_removing_a_custom_module_forgets_it_and_removing_a_shipped_one_does_not` | forget not called; called before the remove; list not reloaded when it returned True |
| `test_the_custom_install_report_is_the_one_install_selected_prints` | a second report formatter |

`tests/test_write_ledger.py` is not edited: it fails by itself if lane A's three write sites are
not in `write-ledger.md`, and that is the point of it.

---

## 6. The file sets — three lanes, disjoint

A file appears in exactly one set. `pyplan/write-ledger.md` is in lane A's set ONLY, because every
new write site lands in `module_source.py`: lane B adds none (the copy goes through a seam whose real
implementation is lane A's, the claim and `include.sh` are written by the functions and rows that
already exist).

**Lane A — derive, persist, merge (`file_sets.a`)**
- `pylauncher/yulon/manifest.py` — `Origin`, `Manifest.origin`, the one-clause validator relaxation
- `pylauncher/yulon/manifest_store.py` — `user_root`, the merged `load_all`/`load`
- `pylauncher/yulon/module_source.py` — NEW: `derive_link`, `derive_folder`, `complete`, `persist`,
  `forget`, `copy_folder`, `DeriveError`, the sentences, `USER_MANIFESTS_DIR` layout helpers
- `pylauncher/yulon/controller_wow_wotlk/modules.py` — `store()` gains `user_root`; the five
  game-bound callables §3.5 names
- `pylauncher/manifests/schema/manifest.schema.json` — regenerated
- `pylauncher/tests/test_module_source.py` — NEW
- `pylauncher/tests/test_manifest.py`
- `pylauncher/tests/test_manifest_store.py`
- `pyplan/write-ledger.md` — three rows: `module_source.py::persist::write_text`,
  `module_source.py::persist::os.replace`, `module_source.py::persist::unlink`, plus
  `module_source.py::forget::unlink` and `module_source.py::copy_folder::shutil.copytree` /
  `::shutil.rmtree` (the replace-existing case) — "world may be running: yes" on all; none touches a
  database

**Lane B — the applier's second way to fill `modules/<id>` (`file_sets.b`)**
- `pylauncher/yulon/apply.py` — `FolderCopier` Protocol (`copy(src, dest) -> None`), `FolderSource`
  (`path`, `copier`), `Completer = Callable[[Manifest, Path], Manifest]`;
  `Applier.install(manifest, values=None, *, folder: FolderSource | None = None,
  complete: Completer | None = None)`; `_require_own_clone`'s no-`.git` branch accepting an OWNED
  claim (and, for `remove`, `claim_written_by_this_app`); no new write callee
- `pylauncher/tests/test_apply.py`
- `yulon/git.py` is **not touched**: nothing about a copy is git's, and `CloneSpec` is unchanged.

**Lane C — the tab and the live press (`file_sets.c`)**
- `pylauncher/yulon/ui/controller_view.py` — the two buttons, the two askers, the five services,
  the flow §3.3, the `for_entry` wiring for `wow-wotlk`
- `pylauncher/tests/test_controller_view.py`
- `pyplan/gates/module-from-link-wotlk-yulon-ubuntu-<date>/` — NEW folder: the screenshots, the
  driver script, `README.md`

**Order.** Lanes A and B share no name: A's `copy_folder` is a plain `(Path, Path) -> None` that B
types as its own `FolderCopier` Protocol, and A never calls `Applier.install`'s new keywords — the
view does. **Lane C's `for_entry` wiring references A's bindings and B's keywords, and its gate runs
mypy, so lane C is green only on a worktree that already contains A and B**; its unit tests are
fake-driven and would pass without them, but the gate would not. Run A and B first (in parallel),
merge both to `yulon-phase8b`, then C. If the orchestrator runs all three at once, C's red is this
sequencing and not a defect.

### §B — the applier change, precisely (so lane B needs no second reading of §1-§3)

```python
class FolderCopier(Protocol):
    def __call__(self, src: Path, dest: Path) -> None: ...   # dest may exist and is REPLACED; never copies .git

@dataclass(frozen=True)
class FolderSource:
    path: Path
    copier: FolderCopier

Completer = Callable[[Manifest, Path], Manifest]

def install(self, manifest, values=None, *, folder: FolderSource | None = None,
            complete: Completer | None = None) -> ApplyReport:
```

Inside `install()`, in the branch that today reads `else: self._require_own_clone(...); self.git.clone(...)`:
if `folder` is given, `self._require_own_clone(manifest, clone, "install")` then
`folder.copier(folder.path, clone)` in place of `self.git.clone(CloneSpec(...))`, the `done` line
reading `copy {folder.path} → modules/<id>`; then the existing `write_clone_claim(clone,
item_id=manifest.id, url=manifest.source.url if manifest.source else "")` and the existing
`include.sh` touch — the same statements, so the ledger rows `apply.py::install::touch` and the
three `write_clone_claim` rows resolve exactly as before. A `folder` on a manifest that HAS a
`source` is an `ApplyError` ("one source, not two"); a manifest with neither `source` nor `folder`
takes today's sourceless path. Then, if `complete` is given: `manifest = complete(manifest, clone)`,
refused with `ApplyError` if the returned `id`, `type` or `game` differ; every later step (`_deploy`,
`_patches`, `_sql`, `_conf`, `_client`, `_dbc`, `_report`) reads the returned manifest. `OSError`
from the copier is wrapped as `ApplyError`, the one failure vocabulary. `_require_own_clone`: in the
`not (clone / ".git").is_dir()` branch, before the leftovers refusal, `read_clone_claim()` is asked;
`OWNED` returns; `UNKNOWN` with `action == "remove"` and `claim_written_by_this_app()` returns with
the relocation warning the git branch already logs; everything else falls through to today's
refusal. A copied module therefore has the same three-answer ownership as a cloned one, from the
same file.

---

## 7. The live press (lane C) — what is read first, and what each frame must show

**Box:** `yulon-ubuntu` (AzerothCore WotLK at `/home/pk/wowserver`; the 8.7a UI gate was captured
there). No rebuild is pressed. The module is one the catalog does NOT ship — read the 21 ids first
and pick a small public `mod-*` repository on github.com with a `conf/*.conf.dist` and a
`data/sql/db-world/` (record which, and what its tree holds, BEFORE deriving; the derive's result is
then a claim the tree can refute). Announce every step with `claude-say`.

**Ground, recorded before the first press and refused if already true:**
- `ls /home/pk/wowserver/modules/` — the chosen id must be ABSENT (else the install clause's
  assertion is already true and the step proves nothing);
- `~/.local/share/yulon/manifests/user/` — must be ABSENT (first persist);
- the Modules list, photographed, must not contain the id (`1-list-before.png`); a "before" frame
  taken after the press is the 8.2d false artefact and is not this gate's.

**Frames, one per clause, each with the app's PID alive logged beside it:**
`2-link-dialog.png`, `3-link-report.png` (the `✓ clone`, `✓ activate … from conf/…`, the pending-SQL
lines with a file count, the C++ rebuild sentence), `4-list-after-link.png` (the custom line),
`5-user-manifest.txt` (`cat` of the persisted file), `6-restart-list.png` (the app restarted; the
line still there — the persistence clause), `7-updates-report.png` ("Check for updates" showing the
custom module's row), `8-remove-report.png`, `9-list-after-remove.png` (line gone; `ls modules/`
without the id; the user file gone), then the folder half against a copy of the same repository
placed under `/home/pk/mod-<x>` with its `.git` deleted: `10-folder-dialog.png`,
`11-folder-report.png` (`✓ copy …`), `12-list-after-folder.png` (the *copied from a folder* line),
`13-updates-not-a-checkout.png` (`not a git checkout — nothing to compare`), `14-folder-removed.png`.
Refusals, each its own frame: `15-refused-not-mod.png` (a link to a non-`mod-` repo),
`16-refused-shipped.png` (a link to `azerothcore/mod-aoe-loot`), `17-refused-host.png`.

**The clause this gate does not close, and the command that would:** whether the cloned module
compiles and its conf is read by the running server is the rebuild's, and the rebuild is the
owner's to press (owner rule; and 8.7a's own rollback is "unit-tested through the machine double
only; no live rebuild pressed it yet"). The press that would settle it, from the app: *Modules tab →
"Rebuild the server…" → Yes*, on `yulon-ubuntu`, after this gate's `4-` frame and before its `8-`.
Nothing on this page assumes its outcome.
