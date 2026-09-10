# T18 — A chosen spec can take effect once the install has a deployed `playerbots.conf`

**Status:** IN PROGRESS (Opus hand on `hand-t18` since 2026-09-10 21:32 CEST, live half first on `yulon-ubuntu2`)
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
