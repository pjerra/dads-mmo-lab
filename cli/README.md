# dml CLI

Canonical source for the `dml` CLI that runs inside the `dml-arch` WSL distro
(and, on Linux/Steam Deck, any bash host). Built as a single file:

    bash build.sh        # cat src/*.sh > dml
    ./dev-install.ps1    # (Windows) install into dml-arch + print version

Bootstrap installs still come from Install-DML.ps1 (embedded v2.6.0); the DML
Launcher dev-installs this newer CLI over it. Do not edit `dml` directly —
edit `src/*.sh` and rebuild.

## Machine-readable contract (--json)

Add `--json` anywhere in argv. Two shapes:

**Envelopes** (single JSON object, one line):
- ok:    `{"ok":true,"data":{...}}` — exit 0
- error: `{"ok":false,"error":{"code":"NOT_FOUND","message":"...","hint":"..."}}` — exit 1

**NDJSON streams** (long-running commands: `games start|stop|restart`):
one JSON object per line — `section_start`, `line` (level: info|warn|error),
`section_end`, then exactly one terminal `done` (exit 0) or `error` (exit 1).
`pct` is reserved for installers.

Error codes: UNKNOWN_COMMAND, NOT_FOUND, NO_COMPOSE, DOCKER_DOWN,
START_FAILED, STOP_FAILED.

Commands:
- `dml games list --json` → `{"games":[{"id","path","running"}]}`
- `dml games status <id> --json` → `{"id","state":"running"|"stopped"}`
- `dml games start|restart|stop <id> --json` → NDJSON stream
- `dml version --json` → `{"version":"3.0.0"}`

`dml games list` and `dml games status` are JSON-first: they always emit JSON,
even without `--json`. A JSON-mode `games start|stop|restart|status` call with
a missing title still emits a terminal error (NDJSON `error` event, or the
`status` envelope) with code `NOT_FOUND` — parser-side synthesis of a missing
terminal event is never needed.

`games start|restart` run `<compose_dir>/dml-start.sh <mode>` when present
(staged AzerothCore start that avoids re-running ac-db-import); otherwise
`docker compose up -d` / `down`.

Tests: `bats tests/` inside the distro; `tests/windows-smoke.ps1` from Windows.
