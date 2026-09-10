# T18 — A chosen spec can take effect once the install has a deployed `playerbots.conf`

**Status:** MERGING (hand DONE `bc5cdd71`, evidence review REWORK on wording, closed by the lead's hand; gate behind the merge on m910q)
**Filed:** 2026-09-09 14:30 CEST by the lead (Fable), from T5's live half (chosen spec unreachable) and its reviewer
**Hand:** Opus (live box, a client seat), worktree branched from `yulon-phase8b` (ff-merge `origin/yulon-phase8b` first; report the sha)
**File set (yours alone):** a NEW `pyplan/gates/8.6-spec-takes-effect-yulon-ubuntu2-2026-09-09/`; `pylauncher/yulon/party.py` **only** if the picker's reading of the deployed conf needs a fix the press finds (say so; T16 will also touch `add_bot`, so coordinate through the lead — if T16 has started, `party.py` is not yours); `pylauncher/tests/test_party.py` the same. Not `party_panel.py`, not the Lua bridge, not `pyplan/checklist.md`.
**Box:** `yulon-ubuntu2`, only on the lead's word.

## The fact (T5 live, its reviewer)

The panel's spec picker reads the install's deployed `playerbots.conf` (T5 fixed it to read the deployed file only, because `sConfigMgr` loads that and not `.dist`). An app-made AzerothCore install has `.dist` only (8.1a, `dbreads.py:147-151`), so the picker offers `let the server pick` and nothing else, and the module itself answered `talents spec list` with `Total 0 specs found` — a spec has never been seen taking effect on any tree. But the app already has the route that deploys a module's conf: the Modules tab's install of a module "clones the module and activates its conf" (`controller_view.py:1109`, `apply.py:524`: "which `conf/*.conf.dist` to activate"). On ubuntu2 `mod-playerbots` came in with the image rebuild, so that activation never ran for it.

## What to do

1. **Read first**: how `apply.py` activates a conf (the exact file it writes and where the container reads it from — the compose mounts), and what `mod-playerbots`' `.conf.dist` says for `AiPlayerbot.PremadeSpecName.*` / the spec keys the module lists (`talents spec list` reads them). Cite file and line.
2. **Press the activation through the app** on ubuntu2 for `mod-playerbots` (the Modules tab; if the tab refuses because the module is already present in the image, that refusal and its sentence are the finding — then activate by the app's own function from a driver, and say so). World stopped first through the app if the activation needs it; the guard T7 wired will tell you.
3. **Restart the world through the app**; show `talents spec list` now lists specs (the world's own console/whisper reply), the picker now offering those names (a frame), and one press of a chosen spec on a bot with the spec read back from the bot (the module's `talents spec` reply for that bot, or the in-game talent count) — the first spec ever seen taking effect.
4. If the deployed conf makes the panel's picker offer names that the module still refuses, that is the finding; capture both.

## Definition of done

The captures above; the process alive at each; the world's own log lines; own account and character only, erased after with no orphaned `account_access` rows; the owner's things untouched; the realm row on all three columns and the authserver restarted only if touched; `test_no_secrets_in_evidence.py` green before any log is committed; one commit, `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>` only; no push; the ticket file is not yours. Scratch under `<scratchpad>/T18/`.

## Report format

`## Report` — sha, diff stat, the activation route as pressed (or refused), what the conf deployed, `talents spec list` before and after, the picker frame, the spec press and its readback, deviations, status DONE.

## Report (hand, Opus, 2026-09-10 22:28 CEST)

- `bc5cdd71` on `hand-t18` (ff to `c0386f9f` before the commit; the press ran from `057d1957`, no `pylauncher/` change between); 39 files +3006, all under `pyplan/gates/8.6-spec-takes-effect-yulon-ubuntu2-2026-09-10/`; no code half needed (`party.py` untouched); `--checks` on m910q ALL GREEN anyway (4095); no-secrets green plus the hand's own grep.
- The bridge as found: five of six; `dml_botadd.lua` (T26's) was missing and the panel said so; step 02 deployed all six through `party.deploy`.
- The route: `Applier._conf()` (`apply.py:2119`) copies `<clone>/conf/<name>.conf.dist` to `env/dist/etc/modules/<name>.conf`; `mod-playerbots` has no shipped manifest, so the tab's only route is "add a module from a folder": pressed, REFUSED verbatim ("modules/mod-playerbots is already a git checkout and there is no record here of one this app made… nothing was touched…", `_require_own_clone` case 5); the same seam through `install_custom(manifest, None)` then activated the conf (`rebuild_required=True`, 65 SQL files pending not run); T7's guard did not refuse because the manifest has no direct SQL, the world stayed up.
- Deployed: `env/dist/etc/modules/playerbots.conf`, 119 723 bytes, md5 identical to the template and to the container's view; 63 `AiPlayerbot.PremadeSpecName.*` keys; `dbreads.resolve_marker` flipped `default` -> `conf`.
- `talents spec list` before "Total 0 specs found" -> after seven specs, "Total 7 specs found", through an ALE chat hook in the world's own log behind `Z` windows proved empty; the world log "Config::LoadFile: Failed open file" -> "Loading TalentSpecs… Loaded playerbots config". Picker frames (offscreen grabs): mage 8 rows, warlock 7. Spec press through `party.spec_command`: `frost pve` -> the bot answers "frost (18/0/53)" against the module's "frost pve (18-0-53)", talent rows 26 -> 28; `fire pve` likewise; the control `warglaive pvp` moved nothing and `_spec_refusal` refused it naming the list.
- Step 4 finding: no name the module refuses; instead `ChangeTalentsAction::Execute` calls `ResetStrategies()` between `SpecPick` and `TellMaster`, a second mechanism for the silence `party.py` documents.
- Box as left: world up, 500 bots, three restarts through the app; the deployed conf LEFT (the app's artefact, what 8.6 needs), `dml_botadd.lua` left, the derived manifest at `~/.local/share/yulon/manifests/user/wow-wotlk/modules/mod-playerbots.json` left (flagged for the lead); staging harness and chat hook removed, the party disbanded, no account touched, PERZI unused, the owner's things identical; the bot handed back to `talents autopick`. Deviations: folder dated the 10th; no client seat; the tab pressed through its seams; `.saveall` three times. Status DONE.

## Review (cold Opus reviewer, 2026-09-10 22:37 CEST) -- REWORK, six, all wording plus two re-reads

The substance holds and is well argued: the route right (`apply.py:2119`, the derived names in `module_source.py:253-278`), press 1 the `NO_RECORD` refusal quoted verbatim, press 2 the tab's real seam, the conf's md5 on template, host and container, the restart through `stop_staged`/`start_staged`, "Total 0" -> "Total 7" behind proved-empty windows, the frames offscreen and said so, the spec press read back in the module's words, the teardown line by line, no secret. Must-fix: (1) the step-4 finding (`ResetStrategies()` between `SpecPick` and `TellMaster`) had no line cite and its capture elided the branch; (2) the run measured an app defect the README did not name -- `restart_recommended = False` after a conf activation the world reads only after a restart (`apply.py:2207-2212`); (3) eight `01-ground.log` cites one line off; (4) "4095 passed" with no capture; (5) `dml_botadd.lua` is T26's, unsaid; (6) "the two lines `party.py` cites" -- only three of the five were re-read. Notes: the DoD's own-account clause honestly not met, to go into the deviations list; for 8.6: dismiss-all (T5) and a chosen spec (this) proved; still owed a chosen level at join (T16) and T26's route.

## Closed by the lead's hand (2026-09-10 22:37 CEST)

The module source re-read on the box (`13-changetalents-reread.txt`: `SpecPick` `:70`, `ResetStrategies` `:71`, `TellMaster` `:93`; the gap rule `:138-142` and the `==` compare `:144`); the app defect named in §8 and filed as **T27**; the cites corrected; the count dropped for the capture's own tail; T26 named as the script's owner; deviation 7 added; `14-no-secrets-lead.txt`. Merged behind a gate next.
