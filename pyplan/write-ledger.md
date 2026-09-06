# The write ledger

Every place `pylauncher/yulon/` can change something outside this process: a
database, a file, a directory. One row per `module::function::callee`, which is
what `pylauncher/tests/test_write_ledger.py` enumerates over the syntax tree.

**The test fails in both directions.** A write that is not in this table fails
it, so a new one cannot arrive unnoticed; and a row here that no longer resolves
to a call fails it too, so a deleted write cannot leave a line behind claiming
it still happens. One direction alone rots: the first lets the code outgrow the
table, the second lets the table outlive the code.

**Why the table is keyed by function and not by line.** A ledger keyed to line
numbers is rewritten by every edit above it, and a table that churns is a table
people stop reading. Two writes of the same kind in one function are one row.

**What counts as a write, and what deliberately does not.** The callee list is
in `tests/write_sites.py` with its reasoning. Two choices worth repeating here:
`mkdir` is not a write, because creating an empty directory puts no content
anywhere and content still needs one of the calls that is listed; and a bare
`.replace(` with exactly one positional argument IS a write, because
`Path.replace` is how five modules save atomically — the first version of the
walker excluded every bare `.replace(` to keep `str.replace` out and silently
lost seven real writes with them.

**"World may be running"** is the column owner answer 7 (2026-09-06) is about:
no Phase 8 feature writes `characters` or `world` while the world server is up.
A row reading "**yes, and unguarded**" is a write that predates that answer and
is named here rather than quietly grandfathered; 8.7 is where the applier's
guard lands.

Generated rows are checked against the tree by the test, not by hand. The
descriptions are written by hand.

| site | writes | world may be running? |
|---|---|---|
| `apply.py::_apply_patch::write_text` | a patched source file in the clone | install time |
| `apply.py::_client::shutil.copy2` | one file into the user's WoW client folder | yes |
| `apply.py::_client::shutil.copytree` | a directory into the user's WoW client folder | yes |
| `apply.py::_conf::shutil.copy2` | a module's conf template into the server's etc | yes |
| `apply.py::_deploy::replace` | a deployed file renamed onto its final name | yes |
| `apply.py::_deploy::shutil.copy2` | one deployed module file into the server dir | yes |
| `apply.py::_deploy::shutil.copytree` | a deployed module directory into the server dir | yes |
| `apply.py::_rm::shutil.rmtree` | a directory a manifest's `rm` step names | yes |
| `apply.py::_rm::unlink` | a file a manifest's `rm` step names | yes |
| `apply.py::_run_sql::run_file` | a module manifest's `.sql` file, into the database its step names | **yes, and unguarded** — as above; a crash leaves it half applied |
| `apply.py::_run_sql::run_statement` | a module manifest's inline SQL, into the database its step names | **yes, and unguarded** — owner answer 7 makes this 8.7's guard |
| `apply.py::_set_conf_key::write_text` | one key in a server conf file, byte-preserving elsewhere | yes — the file changes now, the value arrives at the next world start |
| `apply.py::install::touch` | the marker that records a module as installed | yes |
| `apply.py::remove::shutil.rmtree` | the clone of a module being removed | yes |
| `apply.py::write_clone_claim::os.replace` | the claim file renamed into place | yes |
| `apply.py::write_clone_claim::unlink` | the temp claim file after a failure | yes |
| `apply.py::write_clone_claim::write_text` | the clone-claim file, to a temp name | yes |
| `catalog/composegen.py::write_dotenv::os.replace` | `.env` renamed into place | install time |
| `catalog/composegen.py::write_dotenv::unlink` | the temp `.env` after a failure | install time |
| `catalog/composegen.py::write_dotenv::write_text` | the merged `.env`, to a temp name | install time |
| `catalog/composegen.py::write_plan::write_text` | the rendered compose files in the server dir | install time; a running stack keeps what it started with |
| `catalog/families/conf.py::_clear::shutil.rmtree` | a staging directory being cleared | install time |
| `catalog/families/conf.py::_clear::unlink` | a staged file being cleared | install time |
| `catalog/families/conf.py::_write::os.replace` | a conf file renamed into place | yes |
| `catalog/families/conf.py::_write::unlink` | the temp conf file after a failure | yes |
| `catalog/families/conf.py::materialise::os.chmod` | that conf file's mode | install time |
| `catalog/families/conf.py::materialise::shutil.move` | a `.dist` conf copied out of the image, `.dist` stripped | install time |
| `catalog/families/conf.py::materialise::shutil.rmtree` | the staging directory afterwards | install time |
| `catalog/families/dockerfile.py::write::write_text` | a generated Dockerfile in the clone | install time |
| `catalog/families/extract.py::_remove_tree::shutil.rmtree` | an extraction directory being replaced | install time |
| `catalog/families/extract.py::write_evidence::os.replace` | that evidence file renamed into place | install time |
| `catalog/families/extract.py::write_evidence::unlink` | the temp evidence file after a failure | install time |
| `catalog/families/extract.py::write_evidence::write_text` | the extraction evidence file, to a temp name | install time |
| `catalog/families/patch.py::apply::write_bytes` | a patched file in the clone | install time |
| `catalog/native.py::write_state::os.replace` | that record renamed into place | install time |
| `catalog/native.py::write_state::unlink` | the temp record after a failure | install time |
| `catalog/native.py::write_state::write_text` | the install's own stage record, to a temp name | install time |
| `controller_wow_wotlk/accounts.py::_account_row::run_statement` | an account row in the auth database (create, and its counters) | yes |
| `controller_wow_wotlk/accounts.py::_run::run_statement` | the auth database, for the account statements this module builds | yes |
| `controller_wow_wotlk/maintenance.py::_dump_one::open(wb)` | one database dump, to a `.partial` | yes — a hot backup |
| `controller_wow_wotlk/maintenance.py::_dump_one::os.replace` | that dump renamed once it verifies | yes |
| `controller_wow_wotlk/maintenance.py::_dump_one::unlink` | the `.partial` after a failed dump | yes |
| `controller_wow_wotlk/maintenance.py::_write_marker::os.replace` | that marker renamed into place | no — as above |
| `controller_wow_wotlk/maintenance.py::_write_marker::unlink` | the temp marker after a failure | no — as above |
| `controller_wow_wotlk/maintenance.py::_write_marker::write_text` | the interrupted-restore marker, to a temp name | no — restore refuses while the server is up |
| `controller_wow_wotlk/maintenance.py::forget_interrupted_restore::unlink` | the marker, when the user chooses to forget it | yes — it only removes the record |
| `controller_wow_wotlk/maintenance.py::restore::unlink` | the marker, once the restore finished | no — as above |
| `controller_wow_wotlk/repair.py::reset_unfinished::run_statement` | `DROP DATABASE` on a half-imported schema | no — refused outright on a populated database |
| `docker.py::pin_project_name::os.replace` | `.env` renamed into place | yes |
| `docker.py::pin_project_name::unlink` | the temp `.env` after a failure | yes |
| `docker.py::pin_project_name::write_bytes` | the compose project pin appended to `.env`, to a temp name | yes |
| `git.py::_sparse_clone::write_text` | the clone's sparse-checkout list | install time |
| `git.py::clone::shutil.rmtree` | a non-git leftover at the clone destination | install time |
| `logsnap.py::_prune::unlink` | **new (8.1a)** older snapshots of this install, never the one just written | yes — as above |
| `logsnap.py::capture::os.replace` | **new (8.1a)** that snapshot renamed once the bytes are down | yes — as above |
| `logsnap.py::_discard::unlink` | **new (8.1a)** the `.partial` after a failed snapshot, through a helper that cannot itself raise | yes — as above |
| `logsnap.py::capture::write_text` | **new (8.1a)** the worldserver log snapshot, to a `.partial` | yes — it runs immediately before the stop |
| `manifest_store.py::_fetch_one::replace` | that manifest renamed into place | n/a |
| `manifest_store.py::_fetch_one::unlink` | the ETag file when the server answers without one | n/a |
| `manifest_store.py::_fetch_one::write_bytes` | a downloaded manifest, to a temp name | n/a |
| `manifest_store.py::_fetch_one::write_text` | the manifest's ETag file | n/a |
| `networking.py::apply::run_statement` | the realmlist row: the address a client is sent to | yes |
| `networking.py::record_network_intent::os.replace` | that record renamed into place | yes |
| `networking.py::record_network_intent::unlink` | the temp record after a failure | yes |
| `networking.py::record_network_intent::write_text` | the network-intent record, to a temp name | yes |
| `networking.py::write_client_realmlist::write_text` | `realmlist.wtf` in the user's client folder | yes |
| `platform.py::_download_curl::unlink` | the `.part` file after a failed download | n/a |
| `platform.py::download_verified::replace` | a verified download renamed onto its final name | n/a |
| `state.py::load_state::replace` | an unreadable `state.json` moved aside to a backup | n/a |
| `state.py::save_state::replace` | `state.json` renamed into place | n/a |
| `state.py::save_state::write_text` | `state.json`, to a temp name | n/a — the app's own record |
