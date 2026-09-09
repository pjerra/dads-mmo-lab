# T12 — No account path defaults an unrecognised scheme to AzerothCore's columns

**Status:** ACCEPT (Fable 2026-09-09 13:12 CEST; Codex owed until 15:04) -- merging
**Filed:** 2026-09-09 17:00 by the lead (Fable), from T9's hand (finding 3) and its two reviewers
**Hand:** Opus, worktree branched from `yulon-phase8b` (ff-merge `origin/yulon-phase8b` first; report the sha)
**File set (yours alone):** `pylauncher/yulon/controller_wow_wotlk/accounts.py`, `pylauncher/tests/test_accounts.py`, and `pylauncher/yulon/ui/controller_view.py` **only** at the two `create_account` call sites that pass `scheme=entry.accounts.scheme or "azerothcore"` (T9's reviewer: near lines 886 and 996 on `yulon-phase8b`; verify) plus the test that pins them. Not the Tortoise binding, not `pyplan/checklist.md`.
**Box:** none.

## The fact (T9, both reviewers)

T9 removed the fall-through in `reset_own_password` — an unrecognised scheme is refused by name. The same shape survives in three more places in the same module: `_account_row`'s final `else` (writes `salt, verifier, expansion, reg_mail, email`), `_grant_gm` and `_gm_level` (test `scheme in ("mangos_sha", "mangos_srp6")` and otherwise go to `account_access`). And on the UI side, two `create_account` call sites pass `scheme=entry.accounts.scheme or "azerothcore"` — the same default, for an entry whose scheme is `None` (the Tortoise binding refuses that case; the shared UI path does not).

Both reviewers read these as **not live for validated catalog data**: `catalog.py` types `accounts.scheme` as `Literal["azerothcore","mangos_sha","mangos_srp6"] | None`, so no shipped entry can carry a fourth string. The route to the bug is a code change adding a fourth `Scheme` value without adding branches — on a mangos-shaped table the create dies loudly with `Unknown column`, on a table that happens to have `salt`/`verifier` it writes a row that never authenticates. Low priority; the same defect class T9 fixed, closed everywhere it appears.

Also from T9's reviewer: the `Known: azerothcore, mangos_sha, mangos_srp6` list in the refusal message is hand-typed and the test pins it verbatim, so a fourth `Scheme` value leaves the message stale with no test noticing — `typing.get_args(Scheme)` keeps it honest.

## What to build

- Explicit branches in `_account_row`, `_grant_gm` and `_gm_level`, each ending in a refusal that names the scheme and the known list; the known list derived from `Scheme` once and shared with `reset_own_password`'s message.
- The two UI call sites: an entry whose `scheme` is `None` is refused with the app's sentence (which the Tortoise binding already has — read `controller_wow_tortoise/accounts.py` for the wording), not defaulted.
- TDD: for each of the three functions, one test with a `_Recorder` and `scheme="mangos_srp7"` asserting `AccountError` naming it and `statements == []`; a test that the known list equals `get_args(Scheme)`; the mutation each catches. AzerothCore, `mangos_sha` and `mangos_srp6` behaviour unchanged, their tests untouched.

## Definition of done

`--checks` ALL GREEN from your worktree's `pylauncher/` (announce on `yulon-fedora` first). One commit, `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>` only; no push; the ticket file is not yours. Scratch files under `<scratchpad>/T12/`.

## Report format (final message)

`## Report` — sha, gate last line, diff stat, the three refusals quoted from their tests, the UI sentence, deviations, status DONE.

## Report (hand, 2026-09-09 12:50 CEST)

- Base `60f12b62` (ff-merged); one commit `eea840b6` on `worktree-agent-aa05571bfe9a4db6a`; gate ALL GREEN (yulon-fedora, 3845 passed); diff 4 files +314/-48 (`test_accounts.py` +134, `test_controller_view.py` +40, `controller_wow_wotlk/accounts.py` +170/-48ish, `ui/controller_view.py` +18/-).
- The three refusals (all with `statements == []`): `'mangos_srp7' is not an account scheme this app knows how to write an account row for, so nothing was written. Known: azerothcore, mangos_sha, mangos_srp6.` / `... grant a GM level on, so nothing was written. ...` / `... read a GM level from, so nothing was read. ...`. `KNOWN_SCHEMES = ", ".join(get_args(Scheme))` reproduces T9's hand-typed list byte for byte; T9's test untouched.
- UI sentence, via new `accounts.checked_scheme(declared, game)` raising `NotImplementedError` with the Tortoise binding's wording: `wow-example declares no account scheme, so this app does not know which columns its account table has. Nothing was written. Create the account at the worldserver console instead.`; both WotLK call sites pass `checked_scheme(entry.accounts.scheme, entry.id)`.
- Mutations (all RED/GREEN, pycache purged): the `else` fall-through restored in `_insert_statement`, `_grant_gm`, `_gm_level`; `get_args(Scheme)[:2]`; `checked_scheme` returning `"azerothcore"` for `None`; each UI site reverted separately (one controller-view test pins both); the insert built after the id lookup (kills the `statements == []` clause).
- Rust: `origin/rust-main` `crates/dml-wow/src/account_write.rs` is AzerothCore-only (`INSERT INTO account (username, salt, verifier)`), no scheme dispatch; nothing to carry over.
- Deviations: `_account_row`'s dispatch extracted into module-private `_insert_statement()` called *before* the id lookup (what makes `statements == []` reachable; a salt/verifier derived even when the name is taken, commented); `checked_scheme()` lives in the shared writer so `controller_view.py` is touched only at the two sites -- the sentence now exists twice (Tortoise's own copy outside the set, identical today; a follow-up could collapse it); the one added controller-view test reads `services.channel_setup._create` (private; `InstallChannel` keeps its `create` seam with no accessor) and says so.

## Review (cold Fable reviewer, 2026-09-09 13:12 CEST) -- ACCEPT

No `else` remains in the module; `_gm_level` refuses before its SELECT; the ordering change is safe (validated input first, pure derivation, no exception type changed, the three INSERT texts byte-identical); `get_args` keeps declaration order so T9's pinned string holds; the UI sentence is literally the Tortoise one and `entry.id` matches `game.GAME`; both UI sites surface the `NotImplementedError` as a sentence (`job.py:60`, `controller_view.py:2314/2402`), never a crash; the controller-view test pins both sites; four files, trailer clean.
Notes: `Scheme` at `accounts.py:176` is a local re-declaration of the catalog Literal (`catalog.py:1005`) -- mypy at both UI sites catches drift, a follow-up could pin `get_args(Accounts.model_fields["scheme"].annotation) == get_args(Scheme)`; `create_account`'s `Raises:` block lacks T9's "no fall-through" sentence (style only); `assert len(known) == 3` is the one line a fourth scheme edits by hand; `passwordcheck.py:69/:122` return `None` for an unknown scheme by design, not a fall-through.
