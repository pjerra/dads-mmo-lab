# 8.7a live: the module importer's button, and the refusal that makes it honest

**Box:** yulon-ubuntu (AzerothCore WotLK, `/home/pk/wowserver`), stack UP throughout.
**Date:** 2026-09-08, 02:02 VM local. **Tree:** this lane's worktree at `898592eb`,
exported with `git archive`. **Python:** `/home/pk/laneA/.venv` (PySide6 6.11.2), offscreen.
**Nothing was stopped, started, rebuilt or written.** The stack was left exactly as found.

## What was proved

A real click on the Modules tab's new **Apply module SQL** button, on a
`ControllerServices.for_entry()` built for the live install — no fakes below the
view — while `ac-worldserver` and `ac-authserver` were running.

| Claim | Evidence in `transcript.txt` |
| --- | --- |
| The route has a call site now | `services.module_sql wired: True`, button present and enabled |
| It reaches THIS install | `allowed_modules(/home/pk/wowserver) = 'mod-playerbots'` — read off the live disk, not the image |
| 8.7a's clause: refused while the world runs | `FAILED: ac-worldserver, ac-authserver are running … Press Stop first` |
| The refusal is what the USER reads | it is the whole of `module_report`, verbatim |
| Nothing ran | `containers created by the press: []` |
| Nothing was claimed | the report contains no count and no "applied" |
| The tab recovers | `button enabled again: True`, 0.4 s |

The refusal names **both** servers, not just the world. That is
`docker.apply_module_sql()` reading `spec.world` and `spec.auth` out of the live
census rather than the one container the rule is written about.

## What was NOT proved here, and by whom it was

The **applying** half — that with the servers stopped the importer really writes
the pending module SQL — was measured by the lane that owns 8.7a's server side,
on this same box on 2026-09-07: `compose up ac-db-import` logged
`Loading modules: all`, applied nothing, `acore_world.updates` stayed 2967; the
same container with `-e AC_UPDATES_ALLOWED_MODULES=mod-aoe-loot` logged
`>> Applying update aoe_loot_module_string.sql` and moved it to 2968.

This lane did **not** re-run that, and deliberately: proving it needs the world
stopped, and this run was holding a live stack it was told to put back. So the
button's happy path is proved by construction (its argv is
`run_one_shot(allowed_modules=…)`, pinned in unit tests) and by that
measurement, not by a second live apply.

## Where this control cannot help, said plainly

`importer_sees_modules()` answered **True** here — this install's compose file
binds `./modules` into `ac-db-import`.

It answers **False** on an install adopted from the DML bash launcher, and that
is the install a real user is most likely to have. Both captures are in
`pylauncher/tests/data/`:

| capture | `ac-db-import` image | mounts on `ac-db-import` | `./modules` |
| --- | --- | --- | --- |
| `wotlk-compose-config.json` (this app's own generated compose) | `yulon.local/ac-wotlk-db-import:native-243c46e3` | `./env/dist/etc`, `./env/dist/logs`, `./modules` | mounted |
| `wotlk-compose-config-script.json` (DML bash installer, Fedora 2026-08-31) | `acore/ac-wotlk-db-import:master` | `./env/dist/etc`, `./env/dist/logs` | **absent** |

On that second shape the importer resolves every allowed module name against the
modules compiled into the stock image, so a module cloned onto the host
afterwards is invisible to it whatever `AC_UPDATES_ALLOWED_MODULES` says. The
button therefore **does not run** there: `docker.apply_module_sql()` refuses
before starting anything, and the tab shows that refusal — which names the
missing mount and says to add it, because this app does not rewrite a compose
file it did not write. That is the honest outcome. The dishonest one, and the
reason the check exists, is a run that exits 0 having applied nothing.

## Reproducing

```
scp gate87a_ui.py <box>:~/gate87a-ui/          # beside an exported pylauncher/
ssh <box> 'cd ~/gate87a-ui && ~/laneA/.venv/bin/python gate87a_ui.py'
```

Exit 0 is PASS. It is only a valid run of THIS clause while the world is up: with
the stack stopped the same script would reach the importer, which is a different
test and writes.
