# Title Install & Remove — Design Spec (Round D)

**Date:** 2026-07-18
**Branch:** `feat/dml-launcher-windows`
**Status:** Design review waived (standing user instruction); final round of the modules/console/titles phase.

## Problem

The user wants to install and remove whole game titles from the launcher, "the same way the
DML does". The DML's installers (`guides/*/install-*.sh`) are 900–2000-line **interactive**
bash scripts — 6–17 blocking prompts each (`ask_yes_no`, free-text `read -r`), no
non-interactive mode, no env seams. Retrofitting silent modes into five huge scripts we
cannot live-test would be high-risk surgery; running them **unchanged inside the launcher
with a real interactive terminal** is both safer and more literally "the same way".

Verified preconditions: the `dml` user has passwordless sudo in dml-arch (installers'
internal `sudo` calls won't hang on a hidden tty password prompt), and every prompt is
line-oriented (`read -r` from stdin — a plain pipe works, no PTY needed).

## Design

### CLI (`cli/src/80-titles.sh` + arms in `90-main.sh` under `games …`)

**Registry** (`_title_registry`): `id|display name|installer script|server dir` —

```
wow-server-playerbots|WoW WotLK (Playerbots)|install-wow-wotlk.sh|games
wow-vanilla-server|WoW Vanilla|install-wow-vanilla.sh|home
wow-tbc-server|WoW TBC|install-wow-tbc.sh|home
maplestory-server|MapleStory v83|install-maplestory.sh|home
runescape-server|RuneScape|install-runescape.sh|home
muonline-server|MU Online|install-muonline.sh|home
```

`games` = installer already manages `~/games/` itself (the wotlk script does);
`home` = legacy layout `$HOME/<id>` — after a successful install the CLI symlinks
`$GAMES_DIR/<id>` → `$HOME/<id>` so the existing scan/start/stop machinery sees it.
(`unbound-wrath` is a wotlk addon, `Steam-Update-Fix` a utility — excluded.)

Installer scripts live at `${DML_INSTALLERS_DIR:-/usr/local/share/dml/installers}`
(installed by `cli/dev-install.ps1` alongside the lua bridges — same
`/usr/local/share/dml` root, root-owned; the env var is the test seam).

**`dml games catalog --json`** — request-response: one row per registry entry:
`{id, name, installed (server dir exists), running ("running"|"stopped"|null when not
installed), script_available (installer file exists)}`.

**`dml games install <id>`** — **TEXT MODE ONLY** (with `--json` → `BAD_ARG` "interactive
— run it from the launcher's install terminal or a real terminal"): validates id against
the registry, `EXISTS` error when already installed, `NO_SCRIPT` when the installer file
is missing (hint: re-run cli/dev-install.ps1), then runs
`bash "$installers_dir/$script" 2>&1` with stdin/stdout passed straight through (the
interactive session IS the contract), and on script exit 0 creates the `home`-kind
symlink (`ln -sfn`, skipped when the server dir wasn't actually created — a user who
answered "n" to the final confirm gets no phantom entry). Exit code passes through.

**`dml games remove <id> [--yes] --json`** — NDJSON streamed. Registry-validated id;
`NOT_FOUND` when not installed. Without `--yes` → `CONFIRM_REQUIRED` error whose message
lists exactly what would be deleted. With it: `docker compose down` in the server dir when
a compose file exists (tolerated failure — docker may be down), delete the server dir
(resolving the `~/games/<id>` symlink to its `$HOME` target first — BOTH the link and the
target go), delete `$HOME/<prefix>-launcher.sh` if present (the installers create these;
prefix per registry id, e.g. `wow-vanilla-launcher.sh` — a fifth registry field
`launcher file` carries the exact name, empty when none). Backups under `~/.dml` are
NEVER touched. Done payload `{id, removed:true}`.

### Rust — interactive runner (the round's one new capability)

`DmlRunner` gains `run_interactive(args, on_event) -> (ChildStdin handle, join)` used by a
new `games_install` command:

- Spawns via a **new command builder without `--json`** (`command_raw`), `stdin(piped)`,
  `stdout(piped)`, `stderr(null)` (the CLI arm already merges the script's stderr with
  `2>&1`).
- Reader is **chunk-based, not line-based** (prompts don't end in newlines): read up to
  4 KiB, decode via the existing `decode_wsl_output`, emit `{event:"chunk", text}` —
  no NDJSON parsing, installer output is raw text. On EOF, `wait` → emit
  `{event:"exit", code}`.
- `AppState` gains `install: Mutex<Option<InstallHandle>>` (child stdin + process handle).
  Commands: `games_install(id, on_event)` (errors `BUSY` if a session is active),
  `games_install_input(text)` (writes `text + "\n"`, `NO_SESSION` error when none),
  `games_install_cancel()` (kills the child; reader thread then emits the exit event and
  clears the handle). The handle is cleared on natural exit too.
- api.ts: `gamesCatalog()`, `gamesInstall(id, onChunk)` (Channel of chunk/exit events),
  `gamesInstallInput(text)`, `gamesInstallCancel()`, `gamesRemove(id, onEvent)`
  (streamed, always sent with `--yes` — the UI's typed confirm IS the consent step).

### Library page

- **Installed** section (existing rows keep start/stop) gains a Remove button per row →
  two-step **typed confirm** (type the title id exactly; the confirm text names what gets
  deleted and that backups are kept) → streams `gamesRemove` into the shared Terminal.
- **Available titles** section: catalog rows not installed — name + Install button
  (disabled when `!script_available`, title hint "re-run cli/dev-install.ps1").
- **Install terminal** (new component `InstallTerminal.svelte`): monospace scrollback fed
  by chunk events (ANSI escapes stripped client-side; autoscroll sticky like Console),
  a text input + Send row (Enter submits; input disabled after exit), a Cancel button
  (two-step: "Cancelling mid-install can leave a partial install behind. Cancel anyway?").
  On exit: success/failure note from the exit code; catalog refreshes.
- Only one install session at a time (BUSY guard both in Rust state and UI).

### Testing

- **bats** (`games-titles.bats`): catalog shape incl. installed/script_available
  (DML_INSTALLERS_DIR seam + fixture dirs); install arm — unknown id, EXISTS, NO_SCRIPT,
  `--json` rejection, happy path running a fake installer script that reads one stdin
  line and creates the server dir (assert symlink created for `home` kind + exit code
  passthrough + the script actually received the stdin line); remove — CONFIRM_REQUIRED
  lists targets, `--yes` deletes dir+symlink+launcher file, compose down attempted when
  compose exists (call log), backups untouched.
- **cargo**: `run_interactive` fixture test (a .cmd script that prompts, reads a line,
  echoes it, exits 0) — chunk events observed, stdin write round-trips, exit event carries
  the code; BUSY/NO_SESSION state-machine tests where cheap.
- **vitest/check**: existing pins stay green (no nav changes — Library already exists).
- **Live gate (batched)**: install MapleStory (smallest, no giant client download risk?)
  or re-run the wotlk installer against the existing install (it detects + reuses),
  answer prompts in the launcher terminal, cancel mid-install once, remove a test title.

### Out of scope

PTY emulation (line-pipe suffices — verified), non-interactive installer modes,
progress parsing of installer output, parallel installs, uninstall of the wotlk addon
(`unbound-wrath`), Windows-side client installation.
