
_require_docker() {
    if ! docker info &>/dev/null; then
        echo "[dml] Docker is not running. Try: sudo systemctl start docker" >&2
        exit 1
    fi
}

_has_compose() {
    local dir="$1" name
    for name in docker-compose.yml docker-compose.yaml compose.yml compose.yaml; do
        [[ -f "$dir/$name" ]] && return 0
    done
    return 1
}

_compose_running() {
    local dir="$1" name compose_file=""
    for name in docker-compose.yml docker-compose.yaml compose.yml compose.yaml; do
        if [[ -f "$dir/$name" ]]; then compose_file="$dir/$name"; break; fi
    done
    [[ -z "$compose_file" ]] && echo 0 && return
    { docker compose -f "$compose_file" ps --status running -q 2>/dev/null || true; } | wc -l | tr -d '[:space:]'
}

# Prints one "id<TAB>compose_dir" line per installed title (compose_dir may be
# empty for install.sh-only titles). Mirrors the legacy list/status scan rules.
_scan_games() {
    [[ -d "$GAMES_DIR" ]] || return 0
    local dir subdir title
    declare -A _scan_seen=()
    for dir in "$GAMES_DIR"/*/; do
        [[ -d "$dir" ]] || continue
        title=$(basename "$dir")
        [[ -n "${_scan_seen[$title]:-}" ]] && continue
        if _has_compose "$dir"; then
            printf '%s\t%s\n' "$title" "${dir%/}"
            _scan_seen["$title"]=1
        elif [[ -f "$dir/install.sh" ]]; then
            printf '%s\t%s\n' "$title" ""
            _scan_seen["$title"]=1
        else
            for subdir in "$dir"*/; do
                [[ -d "$subdir" ]] || continue
                if _has_compose "$subdir"; then
                    printf '%s\t%s\n' "$title" "${subdir%/}"
                    _scan_seen["$title"]=1
                    break
                elif [[ -f "$subdir/install.sh" ]]; then
                    printf '%s\t%s\n' "$title" ""
                    _scan_seen["$title"]=1
                    break
                fi
            done
        fi
    done
}

# Echoes the compose dir for a title dir (itself, or first subdir with a
# compose file). Echoes nothing if none found. Mirrors legacy start/stop.
_resolve_compose_dir() {
    local dir="$1" subdir
    if _has_compose "$dir"; then echo "${dir%/}"; return 0; fi
    for subdir in "$dir"*/; do
        if [[ -d "$subdir" ]] && _has_compose "$subdir"; then
            echo "${subdir%/}"
            return 0
        fi
    done
    return 0
}

# Compose dir of the WoW Playerbots title, or empty.
_wow_server_dir() {
    local dir="$GAMES_DIR/wow-server-playerbots"
    [[ -d "$dir" ]] || return 0
    _resolve_compose_dir "$dir/"
}

# Character name / item-spec validators for `wow mail-item`. Both feed
# straight into a SOAP console command string (see soap_exec) -- an
# unvalidated name or spec would be command-injection-equivalent, so each is
# checked against an allowlist regex before any command string is built.
_valid_charname() { [[ "$1" =~ ^[A-Za-z0-9_]{1,12}$ ]]; }
_valid_item_spec() { [[ "$1" =~ ^[0-9]+:[0-9]+$ ]]; }

# Coordinate validator for `wow teleport-coords`: up to 5 integer digits
# (implies |v| < 100000) plus an explicit magnitude cap of 20000, checked via
# awk since bash arithmetic doesn't do floats. Exit status IS the signal
# (same pattern as _valid_charname).
_valid_coord() {
    [[ "$1" =~ ^-?[0-9]{1,5}(\.[0-9]+)?$ ]] || return 1
    awk -v v="$1" 'BEGIN{ if (v<0) v=-v; exit (v>20000) }'
}

# Account username/password validators for `wow account create|set-password|
# set-gm`. Same rationale as the two above: these values are spliced
# straight into a SOAP console command string (see soap_exec), so each is
# checked against a strict allowlist BEFORE any command string is built. The
# character classes deliberately exclude whitespace, quotes, and XML
# metacharacters (</>/&) -- an unvalidated value here would be
# command-injection-equivalent (a space lets an attacker append additional
# console tokens to `account create`/`account set password`/`account set
# gmlevel`).
_valid_account_user() { [[ "$1" =~ ^[A-Za-z0-9_]{3,20}$ ]]; }
_valid_account_pass() { [[ "$1" =~ ^[A-Za-z0-9_@#%+=!-]{4,16}$ ]]; }

# Arity guard for value-taking flags. Under the global `set -u`, reading $2
# when a value flag is the LAST token aborts the whole script with a bare
# "$2: unbound variable" on stderr and NO JSON envelope -- breaking the
# documented one-envelope-always contract. Call as `_need_flag_val "$1" $#`
# before consuming $2 in any flag parser.
_need_flag_val() {
    [[ "$2" -ge 2 ]] && return 0
    json_err BAD_ARG "Missing value for $1" "Every value flag needs an argument, e.g. $1 <value>"
    exit 1
}

# Runs a command, streaming its combined output. In JSON mode each line
# becomes an NDJSON "line" event; in text mode lines pass through unchanged.
# Returns the command's exit code (set -o pipefail is active globally).
_stream_cmd() {
    if [[ "$DML_JSON" == 1 ]]; then
        "$@" 2>&1 | while IFS= read -r _l; do ndjson_line info "$_l"; done
    else
        "$@" 2>&1
    fi
}

# Shared guard for games start/stop/restart. Sets gid, dir, compose_dir or
# emits the right error (respecting DML_JSON) and exits 1.
_games_resolve_or_fail() {
    gid="${1:-}"
    if [[ -z "$gid" ]]; then
        if [[ "$DML_JSON" == 1 ]]; then
            ndjson_error NOT_FOUND "Missing title" "Usage: dml games <start|stop|restart> <title> --json"
        else
            echo "Usage: dml games <start|stop|restart> <title>" >&2
        fi
        exit 1
    fi
    dir="$GAMES_DIR/$gid"
    if [[ ! -d "$dir" ]]; then
        if [[ "$DML_JSON" == 1 ]]; then ndjson_error NOT_FOUND "Title not found: $gid" "Run: dml games list --json"
        else echo "[dml] ERROR: Title not found: $gid" >&2; fi
        exit 1
    fi
    compose_dir="$(_resolve_compose_dir "$dir/")"
    if [[ -z "$compose_dir" ]]; then
        if [[ "$DML_JSON" == 1 ]]; then ndjson_error NO_COMPOSE "No compose file found in $gid or its subdirectories." "Reinstall the title or check $dir"
        else echo "[dml] ERROR: No compose file found in $gid or its subdirectories." >&2; fi
        exit 1
    fi
    if ! docker info &>/dev/null; then
        if [[ "$DML_JSON" == 1 ]]; then ndjson_error DOCKER_DOWN "Docker is not running." "Try: sudo systemctl start docker (or dml doctor)"
        else echo "[dml] Docker is not running. Try: sudo systemctl start docker" >&2; fi
        exit 1
    fi
}

# Start or restart with hook support. $1 = title, $2 = start|restart
_games_start_impl() {
    local mode="$2"
    _games_resolve_or_fail "$1"
    [[ "$DML_JSON" == 1 ]] && ndjson_section_start "$mode"
    cd "$compose_dir"
    # Cold starts only: during a restart the ports are (expectedly) held by
    # this server's own still-running containers, so the conflict check would
    # cry wolf on every healthy restart. (The 3306 remap inside it is also
    # moot on restart -- `docker start` reuses the existing port bindings.)
    local _pc=""
    if [[ "$mode" == "start" ]]; then
        _pc="$(_check_port_conflicts || true)"
    fi
    if [[ -n "$_pc" ]]; then
        if [[ "$DML_JSON" == 1 ]]; then
            while IFS= read -r _l; do ndjson_line warn "$_l"; done <<< "$_pc"
        else
            printf '%s\n' "$_pc"
        fi
    fi
    local rc=0
    if [[ -x "./dml-start.sh" ]]; then
        _stream_cmd bash ./dml-start.sh "$mode" || rc=$?
    else
        if [[ "$mode" == "restart" ]]; then
            # -t 180: game servers (AC saves characters during graceful
            # shutdown) need far more than docker's 10s default before the
            # force-kill -- an early SIGKILL loses everything since the
            # last periodic save.
            _stream_cmd docker compose down -t 180 || rc=$?
        fi
        [[ $rc -eq 0 ]] && { _stream_cmd docker compose up -d || rc=$?; }
    fi
    if [[ $rc -ne 0 ]]; then
        if [[ "$DML_JSON" == 1 ]]; then
            ndjson_section_end "$mode" error
            ndjson_error START_FAILED "$gid failed to $mode (exit $rc)" "Check logs: docker compose logs, or dml doctor"
        else
            echo "[dml] ERROR: $gid failed to $mode (exit $rc)" >&2
        fi
        exit 1
    fi
    if [[ "$DML_JSON" == 1 ]]; then
        ndjson_section_end "$mode" ok
        ndjson_done "{\"id\":\"$(json_escape "$gid")\",\"state\":\"running\"}"
    else
        echo "[dml] $gid ${mode}ed"
    fi
}

_check_port_conflicts() {
    local in_use
    in_use=$(ss -tlnp 2>/dev/null)

    # DB port: remap silently -- safe to move because clients never connect to it directly
    if echo "$in_use" | grep -q ':3306[[:space:]]'; then
        if ! grep -q 'DOCKER_DB_EXTERNAL_PORT' .env 2>/dev/null; then
            printf 'DOCKER_DB_EXTERNAL_PORT=13306\n' >> .env
            echo "[dml] Port 3306 in use -- remapped DB host port to 13306"
        fi
    fi

    # Game server ports: warn only -- clients connect to fixed ports, cannot silently remap
    local _ports=(
        "3724:WoW auth/login server (TrinityCore, AzerothCore, MaNGOS)"
        "8085:WoW world server (TrinityCore, AzerothCore)"
        "7878:WoW SOAP API (TrinityCore, AzerothCore)"
        "4000:EverQuest zone server (EQEmu)"
        "5998:EverQuest login server (EQEmu)"
        "5999:EverQuest login server (EQEmu)"
        "9000:EverQuest world/zone server (EQEmu)"
        "2593:Ultima Online game server (ServUO / RunUO)"
        "7171:Tibia game server (OpenTibia / OTServBR)"
        "6112:Blizzard legacy port (Warcraft III / Diablo II)"
        "43594:RuneScape private server (RSPS)"
        "2106:Lineage II login server (L2J)"
        "7777:Lineage II game server (L2J)"
        "54230:Final Fantasy XI auth server (Darkstar)"
        "54231:Final Fantasy XI game server (Darkstar)"
        "44453:Star Wars Galaxies login server"
        "44462:Star Wars Galaxies connection server"
    )
    local entry port desc
    for entry in "${_ports[@]}"; do
        port="${entry%%:*}"
        desc="${entry#*:}"
        if echo "$in_use" | grep -q ":${port}[[:space:]]"; then
            echo "[WARN] Port $port is already in use -- $desc."
            echo "[WARN]   Stop whatever is using port $port before starting this server."
        fi
    done
}

# --- machine-readable mode: strip --json from argv anywhere -----------------
DML_JSON=0
_args=()
for _a in "$@"; do
    if [[ "$_a" == "--json" ]]; then DML_JSON=1; else _args+=("$_a"); fi
done
set -- ${_args[@]+"${_args[@]}"}
unset _args _a
# ---------------------------------------------------------------------------

cmd="${1:-help}"
shift || true

case "$cmd" in
  doctor)
    echo "[dml] Checking DML environment..."
    errors=0

    if systemctl is-system-running 2>/dev/null | grep -qE "running|degraded"; then
        echo "[ok]  systemd is running"
    else
        echo "[WARN] systemd is not running -- from Windows run: wsl --shutdown, then reopen"
        errors=$((errors + 1))
    fi

    if docker info &>/dev/null; then
        echo "[ok]  Docker Engine is running"
    else
        echo "[WARN] Docker is not responding -- try: sudo systemctl start docker"
        errors=$((errors + 1))
    fi

    free_kb=$(df /home --output=avail 2>/dev/null | tail -1 | tr -d ' ')
    if [[ "$free_kb" =~ ^[0-9]+$ ]]; then
        free_gb=$(( free_kb / 1024 / 1024 ))
        if (( free_gb >= 20 )); then
            echo "[ok]  Disk space: ${free_gb} GB free on ext4"
        else
            echo "[WARN] Low disk space: ${free_gb} GB free under /home (need 20+ GB for most titles)"
            errors=$((errors + 1))
        fi
    else
        echo "[WARN] Could not read disk space for /home"
        errors=$((errors + 1))
    fi

    if curl -fsS --max-time 5 https://www.google.com > /dev/null 2>&1; then
        echo "[ok]  Internet connection"
    else
        echo "[WARN] No internet connection detected"
        errors=$((errors + 1))
    fi

    if (( errors == 0 )); then
        echo "[ok]  Environment healthy. Run 'dml run <url>' to install a title."
    else
        echo "[dml] Found $errors warning(s) above."
    fi
    ;;

  list)
    if [[ ! -d "$GAMES_DIR" ]]; then
        echo "[dml] No titles installed yet. Run 'dml run <url>' to install one."
        exit 0
    fi
    found=0
    declare -A _list_seen
    for dir in "$GAMES_DIR"/*/; do
        [[ -d "$dir" ]] || continue
        title=$(basename "$dir")
        if _has_compose "$dir" || [[ -f "$dir/install.sh" ]]; then
            echo "$title"
            found=$((found + 1))
            _list_seen["$title"]=1
        else
            for subdir in "$dir"*/; do
                [[ -d "$subdir" ]] || continue
                [[ -n "${_list_seen[$title]:-}" ]] && continue
                if _has_compose "$subdir" || [[ -f "$subdir/install.sh" ]]; then
                    echo "$title"
                    found=$((found + 1))
                    _list_seen["$title"]=1
                    break
                fi
            done
        fi
    done
    if [[ $found -eq 0 ]]; then
        echo "[dml] No titles found in $GAMES_DIR"
    fi
    ;;

  status)
    target="${1:-}"
    if [[ -n "$target" ]]; then
        dir="$GAMES_DIR/$target"
        if [[ ! -d "$dir" ]]; then echo "not-found"; exit 1; fi
        compose_dir="$dir"
        if ! _has_compose "$dir"; then
            for subdir in "$dir"*/; do
                if [[ -d "$subdir" ]] && _has_compose "$subdir"; then
                    compose_dir="$subdir"; break
                fi
            done
        fi
        if _has_compose "$compose_dir"; then
            count=$(_compose_running "$compose_dir")
            if [[ "$count" -gt 0 ]]; then echo "running"; else echo "stopped"; fi
        else
            echo "stopped"
        fi
    else
        [[ ! -d "$GAMES_DIR" ]] && exit 0
        declare -A _seen
        for dir in "$GAMES_DIR"/*/; do
            [[ -d "$dir" ]] || continue
            title=$(basename "$dir")
            if _has_compose "$dir"; then
                count=$(_compose_running "$dir")
                if [[ "$count" -gt 0 ]]; then echo "$title:running"; else echo "$title:stopped"; fi
                _seen["$title"]=1
            else
                # One level deeper -- catches repos with compose file in a subdirectory
                for subdir in "$dir"*/; do
                    [[ -d "$subdir" ]] || continue
                    _has_compose "$subdir" || continue
                    [[ -n "${_seen[$title]:-}" ]] && continue
                    count=$(_compose_running "$subdir")
                    if [[ "$count" -gt 0 ]]; then echo "$title:running"; else echo "$title:stopped"; fi
                    _seen["$title"]=1
                    break
                done
            fi
        done
        # Fallback: catch running Compose projects not found by directory scan
        while IFS= read -r project; do
            [[ -z "$project" ]] && continue
            [[ -n "${_seen[$project]:-}" ]] && continue
            echo "$project:running"
        done < <(docker ps --format '{{index .Labels "com.docker.compose.project"}}' 2>/dev/null | sort -u | grep -v '^$')
    fi
    ;;

  start)
    title="${1:?Usage: dml start <title>}"
    dir="$GAMES_DIR/$title"
    if [[ ! -d "$dir" ]]; then echo "[dml] ERROR: Title not found: $title" >&2; exit 1; fi
    compose_dir="$dir"
    if ! _has_compose "$dir"; then
        for subdir in "$dir"*/; do
            if [[ -d "$subdir" ]] && _has_compose "$subdir"; then
                compose_dir="$subdir"; break
            fi
        done
    fi
    if ! _has_compose "$compose_dir"; then
        echo "[dml] ERROR: No compose file found in $title or its subdirectories." >&2; exit 1
    fi
    _require_docker
    cd "$compose_dir"
    _check_port_conflicts
    echo "[dml] Starting $title..."
    docker compose up -d
    echo "[dml] $title started"
    ;;

  stop)
    title="${1:?Usage: dml stop <title>}"
    dir="$GAMES_DIR/$title"
    if [[ ! -d "$dir" ]]; then echo "[dml] ERROR: Title not found: $title" >&2; exit 1; fi
    compose_dir="$dir"
    if ! _has_compose "$dir"; then
        for subdir in "$dir"*/; do
            if [[ -d "$subdir" ]] && _has_compose "$subdir"; then
                compose_dir="$subdir"; break
            fi
        done
    fi
    if ! _has_compose "$compose_dir"; then
        echo "[dml] ERROR: No compose file found in $title or its subdirectories." >&2; exit 1
    fi
    _require_docker
    cd "$compose_dir"
    echo "[dml] Stopping $title..."
    docker compose down -t 180
    echo "[dml] $title stopped"
    ;;

  scan)
    _require_docker
    echo "[dml] Scanning for all running containers in dml-arch..."
    echo ""

    total=$(docker ps -q 2>/dev/null | wc -l | tr -d '[:space:]')
    if [[ "$total" -eq 0 ]]; then
        echo "[dml] No running containers found."
        exit 0
    fi

    declare -A _known_ports
    _known_ports["3306"]="MySQL/MariaDB"
    _known_ports["3724"]="WoW auth/login"
    _known_ports["8085"]="WoW world server"
    _known_ports["7878"]="WoW SOAP API"
    _known_ports["4000"]="EQ zone (EQEmu)"
    _known_ports["5998"]="EQ login (EQEmu)"
    _known_ports["5999"]="EQ login (EQEmu)"
    _known_ports["9000"]="EQ world (EQEmu)"
    _known_ports["2593"]="Ultima Online"
    _known_ports["7171"]="Tibia"
    _known_ports["6112"]="Blizzard legacy"
    _known_ports["43594"]="RuneScape (RSPS)"
    _known_ports["2106"]="Lineage II login"
    _known_ports["7777"]="Lineage II game"
    _known_ports["54230"]="FFXI auth"
    _known_ports["54231"]="FFXI game"
    _known_ports["44453"]="SWG login"
    _known_ports["44462"]="SWG connection"

    prev_project="__unset__"
    while IFS='|' read -r cid cname project; do
        if [[ "$project" != "$prev_project" ]]; then
            [[ "$prev_project" != "__unset__" ]] && echo ""
            if [[ -z "$project" ]]; then
                echo "[ standalone containers -- no compose project ]"
            else
                echo "[ project: $project ]"
            fi
            prev_project="$project"
        fi
        printf "  %-40s  %s\n" "$cname" "$cid"
        while IFS= read -r pline; do
            [[ -z "$pline" ]] && continue
            hostport=$(echo "$pline" | grep -oE ':[0-9]+$' | tr -d ':')
            note="${_known_ports[$hostport]:-}"
            if [[ -n "$note" ]]; then
                printf "    %-36s  [%s]\n" "$pline" "$note"
            else
                printf "    %s\n" "$pline"
            fi
        done < <(docker port "$cid" 2>/dev/null)
    done < <(docker ps --format '{{.ID}}|{{.Names}}|{{index .Labels "com.docker.compose.project"}}' \
             2>/dev/null | sort -t'|' -k3)

    echo ""
    echo "[dml] To stop a project: dml kill <project-name>  or  dml kill --all"
    ;;

  kill)
    _require_docker
    target="${1:-}"
    if [[ -z "$target" ]]; then
        echo "[dml] Usage: dml kill <project-name> | --all" >&2
        exit 1
    fi

    if [[ "$target" == "--all" ]]; then
        running=$(docker ps -q 2>/dev/null)
        if [[ -z "$running" ]]; then
            echo "[dml] No running containers to stop."
            exit 0
        fi
        count=$(echo "$running" | wc -l | tr -d '[:space:]')
        echo "[dml] Stopping $count running container(s)..."
        echo "$running" | xargs docker stop 2>/dev/null || true
        echo "$running" | xargs docker rm -f 2>/dev/null || true
        docker network prune -f 2>/dev/null || true
        echo "[ok]  All containers stopped, removed, and orphaned networks pruned."
    else
        # Find containers by project label -- works with any compose version, no directory needed
        containers=$(docker ps -q --filter "label=com.docker.compose.project=$target" 2>/dev/null)
        if [[ -z "$containers" ]]; then
            echo "[dml] ERROR: No running containers found for project '$target'." >&2
            echo "[dml]   Run 'dml scan' to see what is currently running." >&2
            exit 1
        fi
        count=$(echo "$containers" | wc -l | tr -d '[:space:]')
        echo "[dml] Stopping $count container(s) for project '$target'..."
        echo "$containers" | xargs docker stop 2>/dev/null || true
        # Compose down cleans up networks and volumes; fall back to direct rm if unavailable
        if ! docker compose -p "$target" down 2>/dev/null; then
            echo "$containers" | xargs docker rm -f 2>/dev/null || true
        fi
        echo "[ok]  '$target' stopped."
    fi
    ;;

  clean)
    _require_docker
    yes_flag="${1:-}"
    _confirm() {
        local prompt="$1" ans
        if [[ "$yes_flag" == "--yes" ]]; then return 0; fi
        read -rp "    $prompt [y/N] " ans
        [[ "$ans" =~ ^[Yy] ]]
    }

    echo "[dml] Running DML cleanup..."
    echo ""

    # 1. Stop DML-managed containers (compose-project containers only; standalone containers not touched)
    running=$(docker ps -q --filter "label=com.docker.compose.project" 2>/dev/null)
    if [[ -n "$running" ]]; then
        count=$(echo "$running" | wc -l | tr -d '[:space:]')
        echo "[dml] $count Docker Compose container(s) found:"
        docker ps --filter "label=com.docker.compose.project" \
            --format '  {{.Names}}  (project: {{index .Labels "com.docker.compose.project"}})' 2>/dev/null
        echo ""
        echo "  Note: standalone containers not part of a compose project are not affected."
        echo ""
        if _confirm "Stop these containers?"; then
            echo "$running" | xargs docker stop 2>/dev/null || true
            echo "$running" | xargs docker rm -f 2>/dev/null || true
            echo "[ok]  Containers stopped."
        fi
    else
        echo "[ok]  No running Docker Compose containers found."
    fi
    echo ""

    # 2. Identify and optionally remove incomplete install directories
    if [[ -d "$GAMES_DIR" ]]; then
        echo "[dml] Checking $GAMES_DIR for incomplete installs..."
        declare -a incomplete
        for dir in "$GAMES_DIR"/*/; do
            [[ -d "$dir" ]] || continue
            if ! _has_compose "$dir" && [[ ! -f "$dir/install.sh" ]]; then
                found_nested=0
                for subdir in "$dir"*/; do
                    if [[ -d "$subdir" ]] && ( _has_compose "$subdir" || [[ -f "$subdir/install.sh" ]] ); then
                        found_nested=1; break
                    fi
                done
                [[ $found_nested -eq 0 ]] && incomplete+=("$dir")
            fi
        done

        if [[ ${#incomplete[@]} -gt 0 ]]; then
            echo "[dml] Incomplete directories (no compose file or install.sh found):"
            for d in "${incomplete[@]}"; do echo "    $(basename "$d")  ($d)"; done
            echo ""
            if _confirm "Remove these directories?"; then
                for d in "${incomplete[@]}"; do
                    [[ -z "$d" ]] && continue
                    rm -rf "$d" && echo "[ok]  Removed: $(basename "$d")"
                done
            fi
        else
            echo "[ok]  No incomplete install directories found."
        fi
    fi
    echo ""

    # 3. Docker prune
    dangling=$(docker images -f dangling=true -q 2>/dev/null | wc -l | tr -d '[:space:]')
    stopped_ct=$(docker ps -a -q --filter status=exited 2>/dev/null | wc -l | tr -d '[:space:]')
    echo "[dml] Docker: $dangling dangling image(s), $stopped_ct exited container(s)."
    if [[ "$dangling" -gt 0 || "$stopped_ct" -gt 0 ]]; then
        if _confirm "Run docker system prune? Warning: removes ALL unused Docker resources system-wide, not just DML ones."; then
            docker system prune -f
            echo "[ok]  Docker pruned."
        fi
    else
        echo "[ok]  Docker is already clean."
    fi
    echo ""
    echo "[ok]  Cleanup complete."
    ;;

  shell)
    exec bash --login
    ;;

  run)
    target="${1:?Usage: dml run <git-url|local-path>}"
    _require_docker
    mkdir -p "$GAMES_DIR"

    if [[ "$target" == /* ]]; then
        if [[ ! -d "$target" ]]; then
            echo "[dml] ERROR: Local path not found: $target" >&2; exit 1
        fi
        repo_name=$(basename "$target")
        clone_dir="$GAMES_DIR/$repo_name"
        if [[ -d "$clone_dir" ]]; then
            echo "[dml] $repo_name already exists in games dir -- skipping copy"
        else
            echo "[dml] Copying $target -> $clone_dir ..."
            cp -r "$target" "$clone_dir"
        fi
    else
        repo_name=$(basename "$target" .git)
        clone_dir="$GAMES_DIR/$repo_name"
        if [[ -d "$clone_dir/.git" ]]; then
            echo "[dml] $repo_name already cloned -- pulling latest"
            git -C "$clone_dir" pull
        else
            echo "[dml] Cloning $target ..."
            git clone "$target" "$clone_dir"
        fi
    fi

    entrypoint="install.sh"
    if [[ -f "$clone_dir/dml.manifest" ]]; then
        declared=$(jq -r '.entrypoint // empty' "$clone_dir/dml.manifest" 2>/dev/null || true)
        [[ -n "$declared" ]] && entrypoint="$declared"
    fi

    if [[ ! -f "$clone_dir/$entrypoint" ]]; then
        echo "[dml] ERROR: $entrypoint not found in $repo_name" >&2
        echo "[dml] This repo may not follow the DML convention (install.sh at root)." >&2
        exit 1
    fi

    echo "[dml] Starting $entrypoint from $repo_name ..."
    cd "$clone_dir"
    exec bash "$entrypoint"
    ;;

  manage)
    # Open the WoW Server Manager (wow-manage.sh) -- a self-contained TUI for
    # AzerothCore WoW servers (modules, AH bot, server controls). It is NOT
    # embedded in this installer: it is 7000+ lines and updated independently,
    # so we pull the latest copy from GitHub on each launch and cache it under
    # ~/.dml. That keeps the available options current without re-shipping the
    # launcher. Offline, we fall back to the last cached copy.
    manage_url="https://raw.githubusercontent.com/DadsMmoLab/dads-mmo-lab/main/guides/wow-wotlk/wow-manage.sh"
    cache_dir="$HOME/.dml"
    cache_file="$cache_dir/wow-manage.sh"
    mkdir -p "$cache_dir"

    tmp_file="$cache_file.download.$$"
    if curl -fsSL --max-time 30 -o "$tmp_file" "$manage_url" 2>/dev/null; then
        # Trust the download only if it looks like the manager. A captive
        # portal login page or truncated transfer would otherwise clobber a
        # known-good cached copy. (No pipe here -- a SIGPIPE under pipefail
        # would read as a false validation failure.)
        first_line=$(head -1 "$tmp_file")
        if [[ "$first_line" == *bash* ]] && grep -q 'MANAGER_VERSION=' "$tmp_file"; then
            mv -f "$tmp_file" "$cache_file"
            echo "[dml] WoW Server Manager is up to date (latest from GitHub)."
        else
            rm -f "$tmp_file"
            echo "[dml] Downloaded manager failed validation -- keeping existing copy." >&2
        fi
    else
        rm -f "$tmp_file"
        [[ -f "$cache_file" ]] && echo "[dml] Offline -- using the cached WoW Server Manager." >&2
    fi

    if [[ ! -f "$cache_file" ]]; then
        echo "[dml] ERROR: Could not download the WoW Server Manager and no cached copy exists." >&2
        echo "[dml] Check your internet connection and try 'dml manage' again." >&2
        exit 1
    fi

    chmod +x "$cache_file" 2>/dev/null || true
    exec bash "$cache_file"
    ;;

  unbound)
    # Layer the Wrath Unbound multi-class add-on onto an existing WotLK
    # Playerbots server. Same fetch-and-run model as 'dml manage': the
    # installer is maintained upstream and pulled fresh each launch (validated,
    # cached under ~/.dml, offline falls back to the cached copy). It force-
    # rebuilds the worldserver, so it is left to run interactively in a
    # terminal -- the tray only offers it for the running wow-server-playerbots
    # title.
    unbound_url="https://raw.githubusercontent.com/DadsMmoLab/dads-mmo-lab/main/guides/unbound-wrath/install-wrath-unbound-addon.sh"
    cache_dir="$HOME/.dml"
    cache_file="$cache_dir/install-wrath-unbound-addon.sh"
    mkdir -p "$cache_dir"

    tmp_file="$cache_file.download.$$"
    if curl -fsSL --max-time 30 -o "$tmp_file" "$unbound_url" 2>/dev/null; then
        first_line=$(head -1 "$tmp_file")
        if [[ "$first_line" == *bash* ]] && grep -q 'WIZARD_VERSION=' "$tmp_file"; then
            mv -f "$tmp_file" "$cache_file"
            echo "[dml] Wrath Unbound add-on installer is up to date (latest from GitHub)."
        else
            rm -f "$tmp_file"
            echo "[dml] Downloaded add-on installer failed validation -- keeping existing copy." >&2
        fi
    else
        rm -f "$tmp_file"
        [[ -f "$cache_file" ]] && echo "[dml] Offline -- using the cached Wrath Unbound add-on installer." >&2
    fi

    if [[ ! -f "$cache_file" ]]; then
        echo "[dml] ERROR: Could not download the Wrath Unbound add-on installer and no cached copy exists." >&2
        echo "[dml] Check your internet connection and try 'dml unbound' again." >&2
        exit 1
    fi

    chmod +x "$cache_file" 2>/dev/null || true
    exec bash "$cache_file"
    ;;

  unbound-remove)
    # Uninstall the Wrath Unbound add-on: drops its tables, reverts the
    # core-engine patch and worldserver.conf, removes module files, and
    # rebuilds the worldserver without the module. Same fetch-and-run model as
    # 'dml unbound'. The uninstaller has no version constant, so we validate on
    # a structural sentinel (its detect_server_dir function) instead. It is
    # interactive and prompts for confirmation before destructive steps.
    unbound_rm_url="https://raw.githubusercontent.com/DadsMmoLab/dads-mmo-lab/main/guides/unbound-wrath/uninstall-wrath-unbound-addon.sh"
    cache_dir="$HOME/.dml"
    cache_file="$cache_dir/uninstall-wrath-unbound-addon.sh"
    mkdir -p "$cache_dir"

    tmp_file="$cache_file.download.$$"
    if curl -fsSL --max-time 30 -o "$tmp_file" "$unbound_rm_url" 2>/dev/null; then
        first_line=$(head -1 "$tmp_file")
        if [[ "$first_line" == *bash* ]] && grep -q 'detect_server_dir' "$tmp_file"; then
            mv -f "$tmp_file" "$cache_file"
            echo "[dml] Wrath Unbound uninstaller is up to date (latest from GitHub)."
        else
            rm -f "$tmp_file"
            echo "[dml] Downloaded uninstaller failed validation -- keeping existing copy." >&2
        fi
    else
        rm -f "$tmp_file"
        [[ -f "$cache_file" ]] && echo "[dml] Offline -- using the cached Wrath Unbound uninstaller." >&2
    fi

    if [[ ! -f "$cache_file" ]]; then
        echo "[dml] ERROR: Could not download the Wrath Unbound uninstaller and no cached copy exists." >&2
        echo "[dml] Check your internet connection and try 'dml unbound-remove' again." >&2
        exit 1
    fi

    chmod +x "$cache_file" 2>/dev/null || true
    exec bash "$cache_file"
    ;;

  lan)
    # dml lan <title> on <ip> | off | status | refresh <ip>
    #
    # LAN play = point the realm's advertised address at the Windows host's
    # LAN IP so other PCs on the home network can reach the world server.
    # The Windows side (portproxy + firewall, set up by Install-DML.ps1)
    # carries LAN traffic to 127.0.0.1; this command only flips the address
    # the auth server hands to clients (acore_auth.realmlist).
    #
    # Messages go to STDOUT even on failure -- the DML Launcher tray only
    # captures stdout, and these are user-facing results, not diagnostics.
    title="${1:-}"
    action="${2:-}"
    lan_usage="[dml] Usage: dml lan <title> on <lan-ip> | off | status | refresh <lan-ip>"
    if [[ -z "$title" || -z "$action" ]]; then echo "$lan_usage"; exit 1; fi

    # Validate arguments up front -- the database wait below can take a
    # while, and a usage mistake should fail instantly, not after it.
    ip="${3:-}"
    case "$action" in
      on|refresh)
        if [[ -z "$ip" ]]; then echo "$lan_usage"; exit 1; fi ;;
      off|status) ;;
      *) echo "$lan_usage"; exit 1 ;;
    esac

    dir="$GAMES_DIR/$title"
    if [[ ! -d "$dir" ]]; then echo "[dml] ERROR: Title not found: $title"; exit 1; fi
    compose_dir="$dir"
    if ! _has_compose "$dir"; then
        for subdir in "$dir"*/; do
            if [[ -d "$subdir" ]] && _has_compose "$subdir"; then
                compose_dir="$subdir"; break
            fi
        done
    fi
    if ! _has_compose "$compose_dir"; then
        echo "[dml] ERROR: No compose file found in $title or its subdirectories."; exit 1
    fi
    _require_docker
    cd "$compose_dir"

    # Two server families support LAN play. Both advertise their address in a
    # 'realmlist' row (id=1), so everything below the family split is shared;
    # only the database container, name, and credentials differ:
    #   * AzerothCore  -- 'ac-database' service, DB acore_auth, root/password
    #   * Tortoise WoW -- MaNGOS-Zero 'db' service, DB tw_logon; the MariaDB
    #                     root password is field 4 of LoginDatabase.Info in
    #                     etc/mangosd.conf ("db;3306;mangos;<pw>;tw_logon").
    services=$(docker compose config --services 2>/dev/null || true)
    if echo "$services" | grep -qx 'ac-database'; then
        db=$(docker compose ps -q ac-database 2>/dev/null | head -1 || true)
        if [[ -z "$db" ]]; then
            echo "[dml] ERROR: '$title' is not running. Start the server first, then change LAN settings."
            exit 1
        fi
        _lan_sql() { docker exec "$db" mysql -uroot -ppassword acore_auth -sN -e "$1" 2>/dev/null; }
    else
        # MaNGOS family. Only Tortoise (login DB 'tw_logon') is verified;
        # CMaNGOS titles (DB 'realmd') fall through to "not supported yet".
        conf="$compose_dir/etc/mangosd.conf"
        login_info=$(grep -m1 -E '^[[:space:]]*LoginDatabase\.Info' "$conf" 2>/dev/null || true)
        login_db=$(printf '%s' "$login_info" | cut -d';' -f5 | tr -d '"[:space:]')
        if [[ "$login_db" != "tw_logon" ]]; then
            echo "[dml] LAN play is not supported for '$title' yet."
            echo "[dml] (Currently supported: AzerothCore servers and Tortoise WoW.)"
            exit 1
        fi
        db=$(docker compose ps -q db 2>/dev/null | head -1 || true)
        if [[ -z "$db" ]]; then
            echo "[dml] ERROR: '$title' is not running. Start the server first, then change LAN settings."
            exit 1
        fi
        dbpw=$(printf '%s' "$login_info" | cut -d';' -f4)
        _lan_sql() { docker exec "$db" mariadb -uroot -p"$dbpw" tw_logon -sN -e "$1" 2>/dev/null; }
    fi

    # The database can lag the containers (first boot imports take a while).
    # 'refresh' is fired automatically by the tray right after 'dml start',
    # so it gets a long budget; interactive actions get a short one.
    if [[ "$action" == "refresh" ]]; then _lan_tries=60; _lan_gap=10; else _lan_tries=18; _lan_gap=5; fi
    _n=0
    until _lan_sql "SELECT 1" >/dev/null 2>&1; do
        _n=$((_n + 1))
        if (( _n >= _lan_tries )); then
            echo "[dml] ERROR: The realm database is not answering yet. Wait for the server to finish starting, then try again."
            exit 1
        fi
        sleep "$_lan_gap"
    done

    _lan_set() {
        local ip="$1" newaddr
        if [[ ! "$ip" =~ ^[0-9]{1,3}(\.[0-9]{1,3}){3}$ ]]; then
            echo "[dml] ERROR: '$ip' does not look like an IPv4 address."; exit 1
        fi
        if ! _lan_sql "UPDATE realmlist SET address='$ip' WHERE id=1;"; then
            echo "[dml] ERROR: Could not update the realm address."; exit 1
        fi
        # Read back what actually landed: an UPDATE that matches no row
        # (realm id != 1) still exits 0, and reporting success on a no-op
        # would leave the user chasing ghosts on the other PCs.
        newaddr=$(_lan_sql "SELECT address FROM realmlist WHERE id=1;" || true)
        if [[ "$newaddr" != "$ip" ]]; then
            echo "[dml] ERROR: The realm address did not change (no realm with id 1?)."
            echo "[dml]   Wanted '$ip' but the database says '${newaddr:-nothing}'."
            exit 1
        fi
    }

    current=$(_lan_sql "SELECT address FROM realmlist WHERE id=1;" || true)
    case "$action" in
      on)
        _lan_set "$ip"
        echo "[ok] LAN play ENABLED for $title."
        echo ""
        echo "Other PCs on your network: set realmlist $ip"
        echo "(in realmlist.wtf inside the WoW client folder)"
        echo ""
        echo "This PC keeps working with 127.0.0.1 or $ip -- both reach the server."
        ;;
      off)
        _lan_set "127.0.0.1"
        echo "[ok] LAN play DISABLED for $title."
        echo "The server only accepts world connections from this PC again."
        ;;
      status)
        if [[ -z "$current" ]]; then
            echo "[dml] ERROR: Could not read the realm address from the database."
            exit 1
        elif [[ "$current" == "127.0.0.1" ]]; then
            echo "LAN play: OFF (realm address 127.0.0.1 -- this PC only)"
        else
            echo "LAN play: ON  (realm address $current)"
            echo "Other PCs use: set realmlist $current"
        fi
        ;;
      refresh)
        # Re-point an already-LAN-enabled realm at the host's current IP
        # (DHCP can hand the PC a new address between sessions). No-op when
        # LAN play is off. Called automatically by the tray after each start.
        if [[ -z "$current" || "$current" == "127.0.0.1" ]]; then
            echo "[dml] LAN play is off for $title -- nothing to refresh."
            exit 0
        fi
        if [[ "$current" == "$ip" ]]; then
            echo "[ok] LAN address already current ($ip)."
            exit 0
        fi
        # Only rewrite private (LAN) addresses. A public IP means the user
        # set up internet hosting by hand -- clobbering it with the LAN IP
        # on every start would silently lock their friends out.
        if [[ ! "$current" =~ ^(192\.168\.|10\.|172\.(1[6-9]|2[0-9]|3[01])\.) ]]; then
            echo "[dml] Realm address $current is not a LAN address -- leaving it alone."
            exit 0
        fi
        _lan_set "$ip"
        echo "[ok] LAN address refreshed: $current -> $ip"
        ;;
      *)
        echo "$lan_usage"; exit 1
        ;;
    esac
    ;;

  games)
    sub="${1:-list}"
    shift || true
    case "$sub" in
      list)
        first=1
        out='{"games":['
        while IFS=$'\t' read -r gid gdir; do
            [[ -z "$gid" ]] && continue
            running=false
            if [[ -n "$gdir" ]] && [[ "$(_compose_running "$gdir")" -gt 0 ]]; then
                running=true
            fi
            [[ $first -eq 0 ]] && out+=','
            out+="{\"id\":\"$(json_escape "$gid")\",\"path\":\"$(json_escape "${gdir:-$GAMES_DIR/$gid}")\",\"running\":$running}"
            first=0
        done < <(_scan_games)
        out+=']}'
        json_ok "$out"
        ;;
      status)
        gid="${1:-}"
        if [[ -z "$gid" ]]; then
            json_err NOT_FOUND "Missing title" "Usage: dml games status <title> --json"
            exit 1
        fi
        dir="$GAMES_DIR/$gid"
        if [[ ! -d "$dir" ]]; then
            json_err NOT_FOUND "Title not found: $gid" "Run: dml games list --json"
            exit 1
        fi
        compose_dir="$(_resolve_compose_dir "$dir/")"
        state=stopped
        if [[ -n "$compose_dir" ]] && [[ "$(_compose_running "$compose_dir")" -gt 0 ]]; then
            state=running
        fi
        json_ok "{\"id\":\"$(json_escape "$gid")\",\"state\":\"$state\"}"
        ;;
      start)
        _games_start_impl "${1:-}" start
        ;;
      restart)
        _games_start_impl "${1:-}" restart
        ;;
      stop)
        _games_resolve_or_fail "${1:-}"
        [[ "$DML_JSON" == 1 ]] && ndjson_section_start stop
        cd "$compose_dir"
        rc=0
        _stream_cmd docker compose down -t 180 || rc=$?
        if [[ $rc -ne 0 ]]; then
            if [[ "$DML_JSON" == 1 ]]; then
                ndjson_section_end stop error
                ndjson_error STOP_FAILED "$gid failed to stop (exit $rc)" "Try: dml kill $gid"
            else
                echo "[dml] ERROR: $gid failed to stop (exit $rc)" >&2
            fi
            exit 1
        fi
        if [[ "$DML_JSON" == 1 ]]; then
            ndjson_section_end stop ok
            ndjson_done "{\"id\":\"$(json_escape "$gid")\",\"state\":\"stopped\"}"
        else
            echo "[dml] $gid stopped"
        fi
        ;;
      catalog)
        tout='['; first=1
        while IFS='|' read -r tid tname tscript tkind tlauncher; do
          [[ -z "$tid" ]] && continue
          tinst=false; _title_installed "$tid" && tinst=true
          tscriptok=false; [[ -f "$(_installers_dir)/$tscript" ]] && tscriptok=true
          trun=null
          if [[ "$tinst" == true ]]; then
            tdir="$GAMES_DIR/$tid"; [[ -d "$tdir" ]] || tdir="$HOME/$tid"
            tcompose="$(_resolve_compose_dir "$tdir/")"
            if [[ -n "$tcompose" ]] && [[ "$(_compose_running "$tcompose")" -gt 0 ]]; then
              trun='"running"'
            else
              trun='"stopped"'
            fi
          fi
          [[ $first -eq 0 ]] && tout+=','
          tout+="{\"id\":\"$tid\",\"name\":\"$(json_escape "$tname")\",\"installed\":$tinst,\"running\":$trun,\"script_available\":$tscriptok}"
          first=0
        done < <(_title_registry)
        tout+=']'
        json_ok "{\"titles\":$tout}"
        ;;
      install)
        gid="${1:-}"
        if [[ "$DML_JSON" == 1 ]]; then
          json_err BAD_ARG "games install is interactive" "Run it from the launcher's install terminal or a real terminal (no --json)."
          exit 1
        fi
        trow="$(_title_row "$gid")"
        if [[ -z "$trow" ]]; then
          echo "[dml] ERROR: unknown title: $gid"; exit 1
        fi
        if _title_installed "$gid"; then
          echo "[dml] ERROR: $gid is already installed"; exit 1
        fi
        tscript="$(printf '%s' "$trow" | cut -d'|' -f3)"
        tkind="$(printf '%s' "$trow" | cut -d'|' -f4)"
        tfile="$(_installers_dir)/$tscript"
        if [[ ! -f "$tfile" ]]; then
          echo "[dml] ERROR: installer script not found: $tfile (re-run cli/dev-install.ps1)"; exit 1
        fi
        rc=0
        bash "$tfile" 2>&1 || rc=$?
        if [[ $rc -eq 0 && "$tkind" == home && -d "$HOME/$gid" ]]; then
          mkdir -p "$GAMES_DIR"
          ln -sfn "$HOME/$gid" "$GAMES_DIR/$gid"
          echo "[dml] linked $GAMES_DIR/$gid -> $HOME/$gid"
        fi
        exit "$rc"
        ;;
      remove)
        gid="${1:-}"; shift || true
        confirm=0
        [[ "${1:-}" == "--yes" ]] && confirm=1
        [[ "$DML_JSON" == 1 ]] && ndjson_section_start games-remove
        trow="$(_title_row "$gid")"
        if [[ -z "$trow" ]]; then
          ndjson_section_end games-remove error
          ndjson_error BAD_ARG "Unknown title: $gid" ""; exit 1
        fi
        tkind="$(printf '%s' "$trow" | cut -d'|' -f4)"
        tlauncher="$(printf '%s' "$trow" | cut -d'|' -f5)"
        if ! _title_installed "$gid"; then
          ndjson_section_end games-remove error
          ndjson_error NOT_FOUND "$gid is not installed" ""; exit 1
        fi
        targets=""
        [[ -e "$GAMES_DIR/$gid" || -L "$GAMES_DIR/$gid" ]] && targets+="$GAMES_DIR/$gid "
        if [[ -L "$GAMES_DIR/$gid" ]]; then
          tres="$(readlink -f "$GAMES_DIR/$gid" 2>/dev/null || true)"
          [[ -n "$tres" ]] && targets+="-> $tres "
        fi
        [[ -d "$HOME/$gid" && ! -L "$HOME/$gid" ]] && targets+="$HOME/$gid "
        [[ -n "$tlauncher" && -e "$HOME/$tlauncher" ]] && targets+="$HOME/$tlauncher"
        if [[ "$confirm" != 1 ]]; then
          ndjson_section_end games-remove error
          ndjson_error CONFIRM_REQUIRED "Removing $gid deletes: $targets" "Re-run with --yes. Backups under ~/.dml are kept."
          exit 1
        fi
        tdir="$GAMES_DIR/$gid"; [[ -d "$tdir" ]] || tdir="$HOME/$gid"
        tcompose="$(_resolve_compose_dir "$tdir/")"
        if [[ -n "$tcompose" ]]; then
          ndjson_line info "stopping $gid..."
          (cd "$tcompose" && docker compose down >/dev/null 2>&1) || true
        fi
        if [[ -L "$GAMES_DIR/$gid" ]]; then
          ttarget="$(readlink -f "$GAMES_DIR/$gid" 2>/dev/null || true)"
          rm -f "$GAMES_DIR/$gid"
          [[ -n "$ttarget" && -d "$ttarget" ]] && rm -rf "$ttarget"
        elif [[ -d "$GAMES_DIR/$gid" ]]; then
          rm -rf "$GAMES_DIR/$gid"
        fi
        [[ -d "$HOME/$gid" ]] && rm -rf "$HOME/$gid"
        [[ -n "$tlauncher" ]] && rm -f "$HOME/$tlauncher"
        ndjson_line info "removed (backups under ~/.dml are kept)"
        ndjson_section_end games-remove ok
        ndjson_done "{\"id\":\"$(json_escape "$gid")\",\"removed\":true}"
        ;;
      *)
        json_err UNKNOWN_COMMAND "Unknown games subcommand: $sub" "Try: dml games list --json"
        exit 1
        ;;
    esac
    ;;

  wow)
    wsub="${1:-}"
    shift || true
    case "$wsub" in
      soap-setup)
        # yq (mikefarah v4) is required for the YAML merge below and is not
        # provisioned by the dml-arch installer -- fail with a clean envelope
        # rather than a bare "command not found" (exit 127). DML_YQ_BIN is an
        # override seam for tests (mirrors DML_GAMES_DIR from Plan 1).
        DML_YQ_BIN="${DML_YQ_BIN:-yq}"
        if ! command -v "$DML_YQ_BIN" >/dev/null 2>&1; then
            json_err MISSING_DEP "yq is required for soap-setup but not installed" "Run: pacman -S go-yq (inside dml-arch as root)"
            exit 1
        fi
        sdir="$(_wow_server_dir)"
        if [[ -z "$sdir" ]]; then
            json_err NOT_FOUND "WoW Playerbots server not installed" "Install it first, then re-run."
            exit 1
        fi
        ovr="$sdir/docker-compose.override.yml"
        [[ -f "$ovr" ]] || printf 'services:\n  ac-worldserver:\n    environment:\n' > "$ovr"
        envf="$sdir/.env"
        # The base compose file already publishes
        # "${DOCKER_SOAP_EXTERNAL_PORT:-7878}:7878" for ac-worldserver.
        # Compose CONCATENATES ports: lists across base+override, so adding a
        # ports: entry here would create a second 0.0.0.0 binding alongside
        # this one, not replace it. Instead we pin the *value* of the
        # variable the base file already reads, via the compose project's
        # .env -- that yields exactly one mapping, host_ip 127.0.0.1.
        soap_line="DOCKER_SOAP_EXTERNAL_PORT=127.0.0.1:7878"
        changed=false

        # Merge the SOAP env into the EXISTING ac-worldserver service with yq
        # — never a second top-level `services:` block (that would be a
        # duplicate YAML key and silently drop the existing playerbot env).
        # changed reflects the FULL desired state, not just one key, so a
        # partially-applied override (e.g. from a future manual edit) is
        # still detected and repaired.
        if ! "$DML_YQ_BIN" -e '
              .services.ac-worldserver.environment.AC_SOAP_ENABLED == "1" and
              .services.ac-worldserver.environment.AC_SOAP_IP == "0.0.0.0" and
              .services.ac-worldserver.environment.AC_SOAP_PORT == "7878"
            ' "$ovr" >/dev/null 2>&1; then
            "$DML_YQ_BIN" -i '
              .services.ac-worldserver.environment.AC_SOAP_ENABLED = "1" |
              .services.ac-worldserver.environment.AC_SOAP_IP = "0.0.0.0" |
              .services.ac-worldserver.environment.AC_SOAP_PORT = "7878"
            ' "$ovr"
            changed=true
        fi

        # Pin the SOAP port mapping to localhost via .env, never clobbering
        # unrelated lines that might already be there.
        if [[ ! -f "$envf" ]]; then
            printf '%s\n' "$soap_line" > "$envf"
            changed=true
        elif ! grep -qxF "$soap_line" "$envf"; then
            if grep -q '^DOCKER_SOAP_EXTERNAL_PORT=' "$envf"; then
                tmp="$envf.tmp.$$"
                awk -v line="$soap_line" '
                  /^DOCKER_SOAP_EXTERNAL_PORT=/ { print line; next }
                  { print }
                ' "$envf" > "$tmp" && mv "$tmp" "$envf"
            else
                [[ -s "$envf" ]] && [[ "$(tail -c1 "$envf")" != $'\n' ]] && printf '\n' >> "$envf"
                printf '%s\n' "$soap_line" >> "$envf"
            fi
            changed=true
        fi

        json_ok "{\"changed\":$changed,\"restart_required\":$changed}"
        ;;
      soap-exec)
        cmd="${1:-}"
        [[ -n "$cmd" ]] || { json_err BAD_ARG "Missing console command" "Usage: dml wow soap-exec \"<command>\" --json"; exit 1; }
        # Guarded assignment: 00-head.sh has `set -euo pipefail` active for
        # this whole script. An unguarded `out="$(soap_exec "$cmd")"` would
        # make bash abort right here on any non-zero soap_exec exit (fault,
        # auth failure, unreachable) -- before the case below ever runs -- so
        # the failure must be captured inside a conditional.
        if out="$(soap_exec "$cmd")"; then rc=0; else rc=$?; fi
        case "$rc" in
          0) json_ok "{\"result\":\"$(json_escape "$out")\"}" ;;
          2) json_err SOAP_FAULT "$out" "The worldserver rejected the command." ; exit 1 ;;
          3) json_err SOAP_AUTH "SOAP authentication failed" "Check admin account / gmlevel 3." ; exit 1 ;;
          *) json_err SOAP_UNREACHABLE "Could not reach SOAP at $(soap_url)" "Is the worldserver running with SOAP enabled? Run: dml wow soap-setup" ; exit 1 ;;
        esac
        ;;
      server-info)
        # Down is an answer, not an error: unreachable/fault -> online:false.
        # Only auth failure stays an error (creds are wrong, not the server).
        if out="$(soap_exec 'server info')"; then rc=0; else rc=$?; fi
        case "$rc" in
          0) json_ok "$(printf '%s' "$out" | _parse_server_info)" ;;
          3) json_err SOAP_AUTH "SOAP authentication failed" "Check ~/.dml/soap.env" ; exit 1 ;;
          *) json_ok '{"online":false,"version":null,"players":null,"uptime":null,"mean_ms":null,"median_ms":null}' ;;
        esac
        ;;
      server-detail)
        # Container state first, SOAP second -- the four-state verdict.
        # Read-only; down/booting are answers, so this verb never errors.
        detail_rows="$(_detail_container_rows)"
        detail_world_state=""; detail_containers=""
        while IFS='|' read -r dc_name dc_state dc_status; do
          case "$dc_name" in
            ac-worldserver) dc_role=world ;;
            ac-authserver) dc_role=auth ;;
            *) dc_role=database ;;
          esac
          [[ "$dc_name" == ac-worldserver ]] && detail_world_state="$dc_state"
          dc_entry="$(printf '{"name":"%s","role":"%s","state":"%s","status":"%s"}' \
            "$(json_escape "$dc_name")" "$dc_role" "$(json_escape "$dc_state")" "$(json_escape "$dc_status")")"
          if [[ -z "$detail_containers" ]]; then detail_containers="$dc_entry"
          else detail_containers="$detail_containers,$dc_entry"; fi
        done <<< "$detail_rows"
        detail_ready=false
        if [[ "$detail_world_state" == running ]] && _world_ready; then detail_ready=true; fi
        detail_reach=false; detail_auth=null
        detail_stats='"version":null,"players":null,"uptime":null,"mean_ms":null,"median_ms":null'
        if [[ "$detail_world_state" == running ]]; then
          if out="$(soap_exec 'server info')"; then rc=0; else rc=$?; fi
          case "$rc" in
            0) detail_reach=true; detail_auth=true
               detail_stats="$(printf '%s' "$out" | _parse_server_info_fields)" ;;
            2) detail_reach=true; detail_auth=true ;;
            3) detail_reach=true; detail_auth=false ;;
            *) detail_reach=false ;;
          esac
        fi
        if [[ "$detail_world_state" != running ]]; then detail_verdict=stopped
        elif [[ "$detail_reach" == true ]]; then detail_verdict=online
        elif [[ "$detail_ready" == true ]]; then detail_verdict=soap_unreachable
        else detail_verdict=starting; fi
        detail_pw="$(_host_port_json ac-worldserver 8085)"
        detail_pa="$(_host_port_json ac-authserver 3724)"
        detail_psp="$(_host_port_json ac-worldserver 7878)"
        detail_pd="$(_host_port_json ac-database 3306)"
        detail_bots="$(_bots_counts "$detail_world_state")"
        json_ok "{\"verdict\":\"$detail_verdict\",\"containers\":[$detail_containers],\"world_ready\":$detail_ready,\"soap\":{\"reachable\":$detail_reach,\"auth_ok\":$detail_auth,$detail_stats},$detail_bots,\"ports\":{\"world\":$detail_pw,\"auth\":$detail_pa,\"soap\":$detail_psp,\"db\":$detail_pd}}"
        ;;
      docker-usage)
        # Read-only: raw `docker system df` lines, one JSON envelope. No
        # server-dir requirement -- this reports host-wide Docker disk use.
        if ! docker info >/dev/null 2>&1; then
          json_err DOCKER_DOWN "Docker is not running" "Start Docker in the distro first."
          exit 1
        fi
        dfout=""
        dfout="$(docker system df 2>&1)" || true
        dfarr='['; dffirst=1
        while IFS= read -r dfl || [[ -n "$dfl" ]]; do
          [[ -z "$dfl" ]] && continue
          [[ $dffirst -eq 0 ]] && dfarr+=','
          dfarr+="\"$(json_escape "$dfl")\""
          dffirst=0
        done <<< "$dfout"
        dfarr+=']'
        json_ok "{\"lines\":$dfarr}"
        ;;
      docker-clean)
        # Port of the manager's cleanup_docker (guides/wow-wotlk/wow-manage.sh
        # ~6688-6769): reclaims disk space from stale build cache / CMake
        # volumes / dangling images. Best-effort throughout -- once past
        # validation/docker-down/no-server, every failure degrades to a warn
        # line rather than aborting (a partial clean is still useful).
        dclevel=""
        while [[ $# -gt 0 ]]; do
          case "$1" in
            --level) _need_flag_val "$1" $#; dclevel="$2"; shift 2 ;;
            *) json_err BAD_ARG "Unknown flag: $1" "Usage: dml wow docker-clean --level 1|2|3 --json"; exit 1 ;;
          esac
        done
        if [[ ! "$dclevel" =~ ^[1-3]$ ]]; then
          json_err BAD_ARG "Level must be 1, 2, or 3" "Usage: dml wow docker-clean --level 1|2|3 --json"
          exit 1
        fi
        [[ "$DML_JSON" == 1 ]] && ndjson_section_start docker-clean
        sdir="$(_wow_server_dir)"
        if [[ -z "$sdir" ]]; then
          ndjson_section_end docker-clean error
          ndjson_error NOT_FOUND "WoW Playerbots server not installed" "Install it first."
          exit 1
        fi
        if ! docker info >/dev/null 2>&1; then
          ndjson_section_end docker-clean error
          ndjson_error DOCKER_DOWN "Docker is not running" "Start Docker in the distro first."
          exit 1
        fi
        ndjson_line info "protecting the database volume..."
        (cd "$sdir" && docker compose up -d ac-database >/dev/null 2>&1) || ndjson_line warn "could not start ac-database -- continuing"
        ndjson_line info "stopping worldserver..."
        (cd "$sdir" && docker compose stop -t 180 ac-worldserver >/dev/null 2>&1) || ndjson_line warn "could not stop worldserver -- continuing"
        ndjson_line info "pruning build cache..."
        dcbrc=0
        if dcbout="$(cd "$sdir" && docker builder prune -af 2>&1)"; then :; else dcbrc=$?; fi
        if [[ -n "$dcbout" ]]; then
          while IFS= read -r dcl || [[ -n "$dcl" ]]; do
            [[ -n "$dcl" ]] && ndjson_line info "$dcl"
          done <<< "$dcbout"
        fi
        [[ "$dcbrc" -ne 0 ]] && ndjson_line warn "build cache prune exited $dcbrc -- may already be empty"
        if [[ "$dclevel" -ge 2 ]]; then
          ndjson_line info "identifying build volume..."
          dcproject="$(basename "$sdir" | tr '[:upper:]' '[:lower:]' | tr -cd 'a-z0-9-')"
          dcvol=""
          dcvol="$(docker volume ls --format '{{.Name}}' 2>/dev/null | grep -E "^${dcproject}.*(ac.build|build)" | head -1)" || true
          if [[ -n "$dcvol" ]]; then
            ndjson_line info "removing build volume: $dcvol"
            if docker volume rm "$dcvol" >/dev/null 2>&1; then
              ndjson_line info "build volume removed -- CMake cache cleared."
            else
              ndjson_line warn "could not remove $dcvol (may still be in use)"
            fi
          else
            ndjson_line info "no build volume found matching '${dcproject}*build' -- nothing to remove"
          fi
        fi
        if [[ "$dclevel" -ge 3 ]]; then
          ndjson_line info "pruning unused images..."
          dcirc=0
          if dciout="$(docker image prune -af 2>&1)"; then :; else dcirc=$?; fi
          if [[ -n "$dciout" ]]; then
            while IFS= read -r dcl || [[ -n "$dcl" ]]; do
              [[ -n "$dcl" ]] && ndjson_line info "$dcl"
            done <<< "$dciout"
          fi
          [[ "$dcirc" -ne 0 ]] && ndjson_line warn "image prune exited $dcirc"
        fi
        ndjson_line info "Next rebuild will be a full recompile (30-90 min)."
        ndjson_section_end docker-clean ok
        ndjson_done "{\"level\":$dclevel,\"cleaned\":true}"
        ;;
      console-tail)
        # Read-only worldserver log tail for the Console page. Down is an
        # answer: docker/container unavailable -> available:false, exit 0.
        lines=200
        while [[ $# -gt 0 ]]; do
          case "$1" in
            --lines) _need_flag_val "$1" $#; lines="$2"; shift 2 ;;
            *) json_err BAD_ARG "Unknown flag: $1" "Usage: dml wow console-tail [--lines N] --json"; exit 1 ;;
          esac
        done
        if [[ ! "$lines" =~ ^[0-9]+$ ]]; then
          json_err BAD_ARG "--lines must be a number" "Usage: dml wow console-tail [--lines N] --json"; exit 1
        fi
        lines=$((10#$lines))
        if (( lines < 1 || lines > 1000 )); then
          json_err BAD_ARG "--lines must be 1-1000" "Usage: dml wow console-tail [--lines N] --json"; exit 1
        fi
        if raw="$(docker logs --tail "$lines" ac-worldserver 2>&1)"; then
          if [[ -n "$raw" ]]; then
            arr="$(printf '%s\n' "$raw" | _strip_ansi | _console_lines_json)"
          else
            arr="[]"
          fi
          json_ok "{\"available\":true,\"lines\":$arr}"
        else
          json_ok '{"available":false,"lines":[]}'
        fi
        ;;
      console-send)
        # The manual GM console: free text is DELIBERATE here (same
        # capability as the public `wow soap-exec`; the closed-allowlist
        # rule binds canned/automated actions, not the operator console).
        # The text reaches SOAP only via soap_exec's XML escaping.
        cmd=""
        while [[ $# -gt 0 ]]; do
          case "$1" in
            --command) _need_flag_val "$1" $#; cmd="$2"; shift 2 ;;
            *) json_err BAD_ARG "Unknown flag: $1" "Usage: dml wow console-send --command \"<text>\" --json"; exit 1 ;;
          esac
        done
        if [[ -z "${cmd//[[:space:]]/}" ]]; then
          json_err BAD_ARG "console-send requires a non-empty --command" "Example: dml wow console-send --command \"server info\" --json"; exit 1
        fi
        if out="$(soap_exec "$cmd")"; then rc=0; else rc=$?; fi
        case "$rc" in
          0) json_ok "{\"result\":\"$(json_escape "$(_soap_text_decode "$out")")\"}" ;;
          2) json_err SOAP_FAULT "$(_soap_text_decode "$out")" "The worldserver rejected the command." ; exit 1 ;;
          3) json_err SOAP_AUTH "SOAP authentication failed" "Check ~/.dml/soap.env" ; exit 1 ;;
          *) json_err SOAP_UNREACHABLE "Could not reach SOAP at $(soap_url)" "Is the worldserver running with SOAP enabled? Run: dml wow soap-setup" ; exit 1 ;;
        esac
        ;;
      items)
        isub="${1:-}"; shift || true
        case "$isub" in
          search)
            name=""; quality="-"; minl="-"; maxl="-"; limit=50
            while [[ $# -gt 0 ]]; do
              case "$1" in
                --name) _need_flag_val "$1" $#; name="$2"; shift 2 ;;
                --quality) _need_flag_val "$1" $#; quality="$2"; shift 2 ;;
                --min-level) _need_flag_val "$1" $#; minl="$2"; shift 2 ;;
                --max-level) _need_flag_val "$1" $#; maxl="$2"; shift 2 ;;
                --limit) _need_flag_val "$1" $#; limit="$2"; shift 2 ;;
                *) json_err BAD_ARG "Unknown flag: $1" "See: dml wow items search --name <text>"; exit 1 ;;
              esac
            done
            # --name is required: an empty/whitespace-only name would fall
            # through to where="1=1" in build_item_search_sql and silently
            # browse up to --limit rows of item_template instead of erroring.
            if [[ -z "${name//[[:space:]]/}" ]]; then
              json_err BAD_ARG "items search requires a non-empty --name" "Example: dml wow items search --name hearthstone --json"
              exit 1
            fi
            # Numeric flags are inlined (unquoted) into the SQL text by
            # build_item_search_sql -- validate they are pure digits (or the
            # "-" sentinel) here, before that happens, so a crafted
            # --quality "0 OR 1=1" can't inject.
            for v in "$quality" "$minl" "$maxl" "$limit"; do
              [[ "$v" == "-" || "$v" =~ ^[0-9]+$ ]] || { json_err BAD_ARG "Numeric flag expected, got: $v" ""; exit 1; }
            done
            sql="$(build_item_search_sql "$name" "$quality" "$minl" "$maxl" "$limit")"
            rows="$(db_world_query "$sql")" || {
              json_err DB_UNREACHABLE "Could not query the item database" "Is ac-database running? Try: dml games status wow-server-playerbots"; exit 1; }
            json_ok "{\"items\":$(printf '%s' "$rows" | _items_rows_to_json)}"
            ;;
          *) json_err BAD_ARG "Unknown items subcommand: $isub" "Try: dml wow items search --name <text>"; exit 1 ;;
        esac
        ;;
      mail-item)
        to=""; items=""; subject="Dad's MMO Lab"; body="Enjoy!"
        while [[ $# -gt 0 ]]; do
          case "$1" in
            --to) _need_flag_val "$1" $#; to="$2"; shift 2 ;;
            --items) _need_flag_val "$1" $#; items="$2"; shift 2 ;;
            --subject) _need_flag_val "$1" $#; subject="$2"; shift 2 ;;
            --body) _need_flag_val "$1" $#; body="$2"; shift 2 ;;
            *) json_err BAD_ARG "Unknown flag: $1" ""; exit 1 ;;
          esac
        done
        _valid_charname "$to" || { json_err BAD_ARG "Invalid character name: $to" "1-12 letters/digits/underscore."; exit 1; }
        IFS=',' read -ra specs <<< "$items"
        [[ "${#specs[@]}" -ge 1 && "${#specs[@]}" -le 12 ]] || { json_err BAD_ARG "Provide 1-12 items as id:count[,id:count…]" ""; exit 1; }
        attach=""
        for s in "${specs[@]}"; do
          _valid_item_spec "$s" || { json_err BAD_ARG "Malformed item spec: $s" "Use itemid:count"; exit 1; }
          attach+=" $s"
        done
        # subject/body are placed inside double quotes in the console command;
        # strip any double quotes to keep the command well-formed. Also strip
        # CR/LF (replaced with a space, not deleted, so words don't glue
        # together) -- an embedded newline would otherwise survive bash,
        # _xml_escape (which only escapes &/</>), and curl --data-binary
        # verbatim, reaching the worldserver console-command text. That's the
        # AC #2695 `.send items` crash surface: a newline in the argument can
        # be read as a second console command.
        subject="${subject//\"/}"; subject="${subject//$'\n'/ }"; subject="${subject//$'\r'/ }"
        body="${body//\"/}"; body="${body//$'\n'/ }"; body="${body//$'\r'/ }"
        # The receiver is deliberately UNQUOTED: AC's modern ChatCommands
        # parser (PlayerIdentifier) does not strip double quotes -- a quoted
        # name arrives as the literal token "Name" and the command fails
        # with its usage text (found live, 2026-07-15). Only #subject/#text
        # are QuotedString args that REQUIRE quotes. $to is safe unquoted:
        # it already passed the strict _valid_charname allowlist above.
        cmd="send items $to \"$subject\" \"$body\"$attach"
        # Guarded assignment: 00-head.sh has `set -euo pipefail` active for
        # this whole script (same reason as the identical guard on wow
        # soap-exec above). An unguarded `out="$(soap_exec "$cmd")"; rc=$?`
        # would make bash abort right here on any non-zero soap_exec exit
        # (fault, auth failure, unreachable) -- before rc=$? or the case below
        # ever runs -- so the failure must be captured inside a conditional.
        if out="$(soap_exec "$cmd")"; then rc=0; else rc=$?; fi
        case "$rc" in
          0) json_ok "{\"sent\":true,\"to\":\"$(json_escape "$to")\",\"attachments\":${#specs[@]}}" ;;
          2) json_err SOAP_FAULT "$out" "The server rejected the mail command." ; exit 1 ;;
          3) json_err SOAP_AUTH "SOAP authentication failed" "" ; exit 1 ;;
          *) json_err SOAP_UNREACHABLE "Could not reach the server" "Run: dml wow soap-setup, then start the server." ; exit 1 ;;
        esac
        ;;
      teleport-list)
        search=""
        [[ "${1:-}" == "--search" ]] && { _need_flag_val "$1" $#; search="$2"; shift 2; }
        where="1=1"
        [[ -n "$search" ]] && where="name LIKE '%$(sql_escape "$search")%'"
        sql="SELECT name,position_x,position_y,position_z,map FROM game_tele WHERE $where ORDER BY name LIMIT 500;"
        rows="$(db_world_query "$sql")" || { json_err DB_UNREACHABLE "Could not query teleport locations" ""; exit 1; }
        first=1; out='['
        while IFS=$'\t' read -r nm x y z mp; do
          [[ -z "$nm" ]] && continue
          [[ $first -eq 0 ]] && out+=','
          out+="{\"name\":\"$(json_escape "$nm")\",\"x\":$x,\"y\":$y,\"z\":$z,\"map\":$mp}"
          first=0
        done <<< "$rows"
        out+=']'
        json_ok "{\"locations\":$out}"
        ;;
      teleport)
        char=""; to=""
        while [[ $# -gt 0 ]]; do
          case "$1" in
            --char) _need_flag_val "$1" $#; char="$2"; shift 2 ;;
            --to) _need_flag_val "$1" $#; to="$2"; shift 2 ;;
            --coords) json_err BAD_ARG "Coordinate teleport is not available here" "Use: dml wow teleport-coords --char … --map … --x … --y … --z …"; exit 1 ;;
            *) json_err BAD_ARG "Unknown flag: $1" ""; exit 1 ;;
          esac
        done
        _valid_charname "$char" || { json_err BAD_ARG "Invalid character name: $char" ""; exit 1; }
        [[ -n "$to" ]] || { json_err BAD_ARG "Missing --to <location>" "List with: dml wow teleport-list --json"; exit 1; }
        # --to is allowlist-validated instead of quote-wrapped: AC's modern
        # ChatCommands parser does NOT strip double quotes around
        # PlayerIdentifier/GameTele args -- a quoted token arrives with the
        # quotes as literal characters and the command fails (found live,
        # 2026-07-15: `teleport name "Testen" "Orgrimmar"` -> "Character
        # '\"testen\"' does not exist"). So both tokens go UNQUOTED, and to
        # keep that safe --to must be a single clean token. Rejecting (not
        # sanitizing) also closes the AC #2695 newline surface here: a CR/LF
        # or quote in --to is now BAD_ARG before any command is built.
        # game_tele coverage: 1983/1989 stock names match this charset; the
        # 6 space-containing oddballs remain reachable via AC's partial-name
        # match on their first word.
        if [[ ! "$to" =~ ^[A-Za-z0-9_-]+$ ]]; then
          json_err BAD_ARG "Invalid location name: $to" "Single token, letters/digits/_/- only; list names with: dml wow teleport-list --json"
          exit 1
        fi
        # Guarded assignment: 00-head.sh has `set -euo pipefail` active for
        # this whole script (same reason as the identical guard on wow
        # soap-exec / mail-item above). An unguarded `out="$(soap_exec
        # "$cmd")"; rc=$?` would make bash abort right here on any non-zero
        # soap_exec exit (fault, auth failure, unreachable) -- before rc=$?
        # or the case below ever runs -- so the failure must be captured
        # inside a conditional.
        if out="$(soap_exec "teleport name $char $to")"; then rc=0; else rc=$?; fi
        case "$rc" in
          0) json_ok "{\"teleported\":true,\"char\":\"$(json_escape "$char")\",\"to\":\"$(json_escape "$to")\"}" ;;
          2) json_err SOAP_FAULT "$out" "Unknown location? See dml wow teleport-list." ; exit 1 ;;
          3) json_err SOAP_AUTH "SOAP authentication failed" "" ; exit 1 ;;
          *) json_err SOAP_UNREACHABLE "Could not reach the server" "" ; exit 1 ;;
        esac
        ;;
      teleport-coords)
        # THIRD sanctioned direct MySQL write (see 30-db.sh / 60-backup.sh
        # headers): writes characters.position_x/y/z/map/orientation
        # directly via _chars_write_stmt, bypassing SOAP entirely. Only safe
        # OFFLINE -- a live worldserver holds its own in-memory position and
        # would clobber this write on the character's next auto-save/logout,
        # so an online character is rejected (CHAR_ONLINE) before any write.
        char=""; map=""; x=""; y=""; z=""
        while [[ $# -gt 0 ]]; do
          case "$1" in
            --char) _need_flag_val "$1" $#; char="$2"; shift 2 ;;
            --map) _need_flag_val "$1" $#; map="$2"; shift 2 ;;
            --x) _need_flag_val "$1" $#; x="$2"; shift 2 ;;
            --y) _need_flag_val "$1" $#; y="$2"; shift 2 ;;
            --z) _need_flag_val "$1" $#; z="$2"; shift 2 ;;
            *) json_err BAD_ARG "Unknown flag: $1" ""; exit 1 ;;
          esac
        done
        # All validators run BEFORE any SQL is built -- see _valid_coord /
        # _valid_charname above.
        _valid_charname "$char" || { json_err BAD_ARG "Invalid character name: $char" ""; exit 1; }
        [[ "$map" =~ ^[0-9]{1,3}$ ]] || { json_err BAD_ARG "Invalid map id: $map" "A map id is 1-3 digits, e.g. --map 0 for Eastern Kingdoms."; exit 1; }
        _valid_coord "$x" || { json_err BAD_ARG "Invalid coordinate: $x" "Coordinates are plain numbers with a magnitude of 20000 or less."; exit 1; }
        _valid_coord "$y" || { json_err BAD_ARG "Invalid coordinate: $y" "Coordinates are plain numbers with a magnitude of 20000 or less."; exit 1; }
        _valid_coord "$z" || { json_err BAD_ARG "Invalid coordinate: $z" "Coordinates are plain numbers with a magnitude of 20000 or less."; exit 1; }
        row="$(db_chars_query "SELECT guid, online FROM characters WHERE name='$(sql_escape "$char")' LIMIT 1;")" \
          || { json_err DB_UNREACHABLE "Could not reach the characters database" "Is ac-database running?"; exit 1; }
        [[ -n "$row" ]] || { json_err NOT_FOUND "No such character: $char" ""; exit 1; }
        IFS=$'\t' read -r guid online <<< "$row"
        [[ "$guid" =~ ^[0-9]+$ ]] || { json_err DB_UNREACHABLE "Unexpected character lookup result" ""; exit 1; }
        if [[ "$online" != "0" ]]; then
          json_err CHAR_ONLINE "Character must be logged out: $char" "Character must be logged out."
          exit 1
        fi
        sql="UPDATE characters SET position_x=$x, position_y=$y, position_z=$z, map=$map, orientation=0 WHERE guid=$guid;"
        _chars_write_stmt "$sql" \
          || { json_err DB_UNREACHABLE "Could not update the character's position" "Is ac-database running?"; exit 1; }
        json_ok "{\"teleported\":true,\"char\":\"$(json_escape "$char")\",\"map\":$map,\"x\":$x,\"y\":$y,\"z\":$z}"
        ;;
      accounts)
        # Read-only list of real player accounts and their characters.
        # The 250 RNDBOT* ambient-bot accounts and AHBOT are noise for the
        # GUI's character picker; SOAP-only accounts (e.g. DMLSOAP) simply
        # have no characters and are harmless to include. gmlevel is pulled
        # from account_access (MAX across realms, since a per-realm 0 row
        # can coexist with a real grant) so the GUI can show GM badges
        # without a second round trip.
        sql="SELECT a.id, a.username, COALESCE(g.gmlevel,0), COALESCE(c.guid,''), COALESCE(c.name,''), COALESCE(c.level,'')
             FROM acore_auth.account a
             LEFT JOIN (SELECT id, MAX(gmlevel) AS gmlevel FROM acore_auth.account_access GROUP BY id) g ON g.id = a.id
             LEFT JOIN characters c ON c.account = a.id
             WHERE a.username NOT LIKE 'RNDBOT%' AND a.username <> 'AHBOT'
             ORDER BY a.id, c.level DESC;"
        rows="$(db_chars_query "$sql")" \
          || { json_err DB_UNREACHABLE "Could not reach the characters/auth database" "Is ac-database running?"; exit 1; }
        json_ok "{\"accounts\":$(printf '%s' "$rows" | _accounts_rows_to_json)}"
        ;;
      account)
        # Account management: create / set-password / set-gm / delete. All
        # mutating and all through SOAP, never a direct database write (same
        # rule as mail-item/teleport). --user and --pass are allowlist-
        # validated BEFORE any command string is built -- see
        # _valid_account_user/_valid_account_pass above for why.
        asub="${1:-}"; shift || true
        auser=""; apass=""; alevel=""
        while [[ $# -gt 0 ]]; do
          case "$1" in
            --user) _need_flag_val "$1" $#; auser="$2"; shift 2 ;;
            --pass) _need_flag_val "$1" $#; apass="$2"; shift 2 ;;
            --level) _need_flag_val "$1" $#; alevel="$2"; shift 2 ;;
            *) json_err BAD_ARG "Unknown flag: $1" ""; exit 1 ;;
          esac
        done
        _valid_account_user "$auser" || { json_err BAD_ARG "Invalid username (3-20 letters/digits/_)" ""; exit 1; }
        case "$asub" in
          create|set-password)
            _valid_account_pass "$apass" || { json_err BAD_ARG "Invalid password (4-16 chars, letters/digits/_@#%+=!-)" ""; exit 1; }
            if [[ "$asub" == create ]]; then acmd="account create $auser $apass"
            else acmd="account set password $auser $apass $apass"; fi
            ;;
          set-gm)
            [[ "$alevel" =~ ^[0-3]$ ]] || { json_err BAD_ARG "--level must be 0-3" ""; exit 1; }
            acmd="account set gmlevel $auser $alevel -1"
            ;;
          delete)
            # Deleting an account also deletes every character on it, and
            # deleting the SOAP admin account would cut the launcher's own
            # server access -- refuse that one outright.
            if [[ "${auser,,}" == "admin" ]]; then
              json_err BAD_ARG "Refusing to delete the admin account" "The launcher uses it for server access (SOAP)."
              exit 1
            fi
            acmd="account delete $auser"
            ;;
          *) json_err UNKNOWN_COMMAND "Unknown account subcommand: $asub" "Try: dml wow account create|set-password|set-gm|delete --json"; exit 1 ;;
        esac
        # Guarded assignment: 00-head.sh has `set -euo pipefail` active for
        # this whole script (same reason as the identical guard on wow
        # soap-exec/mail-item/teleport above). An unguarded `out="$(soap_exec
        # "$acmd")"; rc=$?` would make bash abort right here on any non-zero
        # soap_exec exit (fault, auth failure, unreachable) -- before rc=$?
        # or the case below ever runs -- so the failure must be captured
        # inside a conditional.
        if out="$(soap_exec "$acmd")"; then rc=0; else rc=$?; fi
        case "$rc" in
          0)
            case "$asub" in
              create) json_ok "{\"created\":true,\"user\":\"$auser\"}" ;;
              set-password) json_ok "{\"password_set\":true,\"user\":\"$auser\"}" ;;
              set-gm) json_ok "{\"gm_set\":true,\"user\":\"$auser\",\"level\":$alevel}" ;;
              delete) json_ok "{\"deleted\":true,\"user\":\"$auser\"}" ;;
            esac ;;
          2) json_err SOAP_FAULT "$(_soap_text_decode "$out")" "The worldserver rejected the command." ; exit 1 ;;
          3) json_err SOAP_AUTH "SOAP authentication failed" "Check ~/.dml/soap.env" ; exit 1 ;;
          *) json_err SOAP_UNREACHABLE "Could not reach SOAP at $(soap_url)" "Is the worldserver running?" ; exit 1 ;;
        esac
        ;;
      characters)
        acct=""
        [[ "${1:-}" == "--account" ]] && { _need_flag_val "$1" $#; acct="$2"; shift 2; }
        [[ -n "$acct" ]] || { json_err BAD_ARG "Missing --account <name>" ""; exit 1; }
        # Test seam: DML_STUB_ACCOUNT_ID lets bats tests skip the auth-schema
        # lookup and go straight to a deterministic account id, since the
        # mysql stub answers every `docker exec` call from the same
        # DML_STUB_DB_ROWS file regardless of which schema/query asked.
        if [[ -n "${DML_STUB_ACCOUNT_ID:-}" ]]; then
          aid="$DML_STUB_ACCOUNT_ID"
        else
          aid="$(db_auth_query "SELECT id FROM account WHERE username='$(sql_escape "$acct")' LIMIT 1;")" \
            || { json_err DB_UNREACHABLE "Could not reach the auth database" ""; exit 1; }
        fi
        [[ -n "$aid" ]] || { json_err NOT_FOUND "No such account: $acct" ""; exit 1; }
        # Numeric whitelist: $aid is spliced unquoted into the SQL below, and
        # can come from the DML_STUB_ACCOUNT_ID test seam (a shipped bypass)
        # as well as the db_auth_query lookup -- guard BOTH branches before
        # it ever reaches a query string.
        [[ "$aid" =~ ^[0-9]+$ ]] || { json_err DB_UNREACHABLE "Unexpected account id" "Account lookup returned a non-numeric id"; exit 1; }
        rows="$(db_chars_query "SELECT guid,name,level,class,race,gender,money FROM characters WHERE account=$aid ORDER BY level DESC;")" \
          || { json_err DB_UNREACHABLE "Could not reach the characters database" ""; exit 1; }
        first=1; out='['
        while IFS=$'\t' read -r guid nm lvl cls race gen money; do
          [[ -z "$guid" ]] && continue
          [[ $first -eq 0 ]] && out+=','
          out+="{\"guid\":$guid,\"name\":\"$(json_escape "$nm")\",\"level\":$lvl,\"class\":$cls,\"race\":$race,\"gender\":$gen,\"gold\":$((money/10000))}"
          first=0
        done <<< "$rows"
        out+=']'
        json_ok "{\"characters\":$out}"
        ;;
      paperdoll)
        char=""
        [[ "${1:-}" == "--char" ]] && { _need_flag_val "$1" $#; char="$2"; shift 2; }
        _valid_charname "$char" || { json_err BAD_ARG "Invalid character name: $char" ""; exit 1; }
        # AC's ac-db-import (re-run on any cold `compose up`) applies core
        # migrations, one of which replaced the packed playerBytes/
        # playerBytes2 appearance columns with discrete skin/face/hairStyle/
        # hairColor/facialStyle columns. Try the migrated schema first and
        # fall back to the packed columns for pre-migration DBs -- both
        # paths emit identical JSON.
        pd_join="FROM characters c
             JOIN character_inventory ci ON ci.guid=c.guid AND ci.bag=0 AND ci.slot BETWEEN 0 AND 18
             JOIN item_instance ii ON ii.guid=ci.item
             JOIN acore_world.item_template it ON it.entry=ii.itemEntry
             WHERE c.name='$(sql_escape "$char")' ORDER BY ci.slot;"
        pd_schema=new
        sql="SELECT c.name,c.level,c.class,c.money,c.race,c.gender,c.skin,c.face,c.hairStyle,c.hairColor,c.facialStyle,ci.slot,it.entry,it.name,it.Quality,it.ItemLevel,it.displayid $pd_join"
        if rows="$(db_chars_query "$sql")"; then :; else
          pd_schema=old
          sql="SELECT c.name,c.level,c.class,c.money,c.race,c.gender,c.playerBytes,c.playerBytes2,ci.slot,it.entry,it.name,it.Quality,it.ItemLevel,it.displayid $pd_join"
          rows="$(db_chars_query "$sql")" || { json_err DB_UNREACHABLE "Could not reach the characters database" ""; exit 1; }
        fi
        [[ -n "$rows" ]] || { json_err NOT_FOUND "No such character or no equipped items: $char" ""; exit 1; }
        cname=""; clevel=0; cclass=0; cmoney=0
        crace_s=0; cgender_s=0; cpb=0; cpb2=0
        cskin=0; cface=0; chairs=0; chairc=0; cfacial=0
        first=1; eq='['
        if [[ "$pd_schema" == new ]]; then
          while IFS=$'\t' read -r nm lvl cls money crace cgender skin face hstyle hcolor facial slot entry iname q ilvl disp; do
            [[ -z "$nm" ]] && continue
            cname="$nm"; clevel="$lvl"; cclass="$cls"; cmoney="$money"
            crace_s="$crace"; cgender_s="$cgender"
            cskin="$skin"; cface="$face"; chairs="$hstyle"; chairc="$hcolor"; cfacial="$facial"
            [[ $first -eq 0 ]] && eq+=','
            eq+="{\"slot\":$slot,\"entry\":$entry,\"name\":\"$(json_escape "$iname")\",\"quality\":$q,\"item_level\":$ilvl,\"displayid\":$disp}"
            first=0
          done <<< "$rows"
        else
          while IFS=$'\t' read -r nm lvl cls money crace cgender pbytes pbytes2 slot entry iname q ilvl disp; do
            [[ -z "$nm" ]] && continue
            cname="$nm"; clevel="$lvl"; cclass="$cls"; cmoney="$money"
            crace_s="$crace"; cgender_s="$cgender"; cpb="$pbytes"; cpb2="$pbytes2"
            [[ $first -eq 0 ]] && eq+=','
            eq+="{\"slot\":$slot,\"entry\":$entry,\"name\":\"$(json_escape "$iname")\",\"quality\":$q,\"item_level\":$ilvl,\"displayid\":$disp}"
            first=0
          done <<< "$rows"
          cskin=$(( cpb & 0xFF )); cface=$(( (cpb >> 8) & 0xFF ))
          chairs=$(( (cpb >> 16) & 0xFF )); chairc=$(( (cpb >> 24) & 0xFF ))
          cfacial=$(( cpb2 & 0xFF ))
        fi
        eq+=']'
        # last_saved: rows reflect the character table as of its last save to
        # the DB -- for a character currently online, that can lag their true
        # live state until their next auto-save/logout. Live-accurate data
        # would need a SOAP .pinfo call (future refinement, not built here).
        json_ok "{\"name\":\"$(json_escape "$cname")\",\"level\":$clevel,\"class\":$cclass,\"race\":$crace_s,\"gender\":$cgender_s,\"skin\":$cskin,\"face\":$cface,\"hair_style\":$chairs,\"hair_color\":$chairc,\"facial_style\":$cfacial,\"gold\":$((cmoney/10000)),\"note\":\"last_saved\",\"equipped\":$eq}"
        ;;
      char-progress)
        char=""
        [[ "${1:-}" == "--char" ]] && { _need_flag_val "$1" $#; char="$2"; shift 2; }
        _valid_charname "$char" || { json_err BAD_ARG "Invalid character name: $char" ""; exit 1; }
        cguid="$(db_chars_query "SELECT guid FROM characters WHERE name='$(sql_escape "$char")' LIMIT 1;")" \
          || { json_err DB_UNREACHABLE "Could not reach the characters database" ""; exit 1; }
        [[ "$cguid" =~ ^[0-9]+$ ]] || { json_err NOT_FOUND "No such character: $char" ""; exit 1; }
        atrow="$(db_chars_query "SELECT activeTalentGroup, talentGroupsCount FROM characters WHERE guid=$cguid;")" \
          || { json_err DB_UNREACHABLE "Could not reach the characters database" ""; exit 1; }
        IFS=$'\t' read -r agroup gcount <<< "$atrow"
        [[ "$agroup" =~ ^[0-9]+$ ]] || agroup=0
        [[ "$gcount" =~ ^[0-9]+$ ]] || gcount=1
        atotal="$(db_chars_query "SELECT COUNT(*) FROM character_achievement WHERE guid=$cguid;")" || atotal=0
        [[ "$atotal" =~ ^[0-9]+$ ]] || atotal=0
        arecent='['; first=1
        while IFS=$'\t' read -r aid adate; do
          [[ -z "$aid" ]] && continue
          [[ "$aid" =~ ^[0-9]+$ ]] || continue
          [[ "$adate" =~ ^[0-9]+$ ]] || adate=0
          [[ $first -eq 0 ]] && arecent+=','
          arecent+="{\"id\":$aid,\"date\":$adate}"
          first=0
        done < <(db_chars_query "SELECT achievement, date FROM character_achievement WHERE guid=$cguid ORDER BY date DESC LIMIT 10;" || true)
        arecent+=']'
        tspells='['; first=1
        while IFS= read -r sid; do
          [[ -z "$sid" ]] && continue
          [[ "$sid" =~ ^[0-9]+$ ]] || continue
          [[ $first -eq 0 ]] && tspells+=','
          tspells+="$sid"
          first=0
        done < <(db_chars_query "SELECT spell FROM character_talent WHERE guid=$cguid AND (specMask & (1 << $agroup)) ORDER BY spell;" || true)
        tspells+=']'
        json_ok "{\"achievements\":{\"total\":$atotal,\"recent\":$arecent},\"talents\":{\"groups_count\":$gcount,\"active_group\":$agroup,\"spells\":$tspells}}"
        ;;
      achievements)
        # Full earned list (id + epoch date) for the achievements browser --
        # char-progress keeps its recent-10 summary; this is the complete
        # set (WotLK caps out around 1300 rows, small enough for one JSON).
        char=""
        [[ "${1:-}" == "--char" ]] && { _need_flag_val "$1" $#; char="$2"; shift 2; }
        _valid_charname "$char" || { json_err BAD_ARG "Invalid character name: $char" ""; exit 1; }
        cguid="$(db_chars_query "SELECT guid FROM characters WHERE name='$(sql_escape "$char")' LIMIT 1;")" \
          || { json_err DB_UNREACHABLE "Could not reach the characters database" ""; exit 1; }
        [[ "$cguid" =~ ^[0-9]+$ ]] || { json_err NOT_FOUND "No such character: $char" ""; exit 1; }
        aearned='['; first=1
        while IFS=$'\t' read -r aid adate; do
          [[ -z "$aid" ]] && continue
          [[ "$aid" =~ ^[0-9]+$ ]] || continue
          [[ "$adate" =~ ^[0-9]+$ ]] || adate=0
          [[ $first -eq 0 ]] && aearned+=','
          aearned+="{\"id\":$aid,\"date\":$adate}"
          first=0
        done < <(db_chars_query "SELECT achievement, date FROM character_achievement WHERE guid=$cguid ORDER BY achievement;" || true)
        aearned+=']'
        json_ok "{\"earned\":$aearned}"
        ;;
      item-info)
        entries=""
        while [[ $# -gt 0 ]]; do
          case "$1" in
            --entries) _need_flag_val "$1" $#; entries="$2"; shift 2 ;;
            *) json_err BAD_ARG "Unknown flag: $1" "Usage: dml wow item-info --entries 1,2,3 --json"; exit 1 ;;
          esac
        done
        if [[ ! "$entries" =~ ^[0-9]+(,[0-9]+)*$ ]]; then
          json_err BAD_ARG "--entries must be comma-separated item ids" ""; exit 1
        fi
        IFS=',' read -r -a earr <<< "$entries"
        if (( ${#earr[@]} > 25 )); then
          json_err BAD_ARG "--entries max 25 ids per call" ""; exit 1
        fi
        mkdir -p "$(_iteminfo_cache)/tooltips" "$(_iteminfo_cache)/icons"
        declare -A _ii_seen=()
        iout='['; first=1
        for ie in "${earr[@]}"; do
          ie=$((10#$ie))
          [[ -n "${_ii_seen[$ie]:-}" ]] && continue
          _ii_seen["$ie"]=1
          iobj="$(_iteminfo_one "$ie")"
          [[ $first -eq 0 ]] && iout+=','
          iout+="$iobj"; first=0
        done
        iout+=']'
        json_ok "{\"items\":$iout}"
        ;;
      entity-info)
        ekind=""; eids=""
        while [[ $# -gt 0 ]]; do
          case "$1" in
            --kind) _need_flag_val "$1" $#; ekind="$2"; shift 2 ;;
            --ids) _need_flag_val "$1" $#; eids="$2"; shift 2 ;;
            *) json_err BAD_ARG "Unknown flag: $1" "Usage: dml wow entity-info --kind spell|achievement --ids 1,2 --json"; exit 1 ;;
          esac
        done
        case "$ekind" in spell|achievement) ;; *) json_err BAD_ARG "--kind must be spell or achievement" ""; exit 1 ;; esac
        if [[ ! "$eids" =~ ^[0-9]+(,[0-9]+)*$ ]]; then
          json_err BAD_ARG "--ids must be comma-separated ids" ""; exit 1
        fi
        IFS=',' read -r -a eidarr <<< "$eids"
        if (( ${#eidarr[@]} > 25 )); then
          json_err BAD_ARG "--ids max 25 per call" ""; exit 1
        fi
        mkdir -p "$(_iteminfo_cache)/tooltips" "$(_iteminfo_cache)/icons"
        declare -A _ee_seen=()
        eout='['; first=1
        for eid in "${eidarr[@]}"; do
          eid=$((10#$eid))
          [[ -n "${_ee_seen[$eid]:-}" ]] && continue
          _ee_seen["$eid"]=1
          eobj="$(_entity_one "$ekind" "$eid")"
          [[ $first -eq 0 ]] && eout+=','
          eout+="$eobj"; first=0
        done
        eout+=']'
        json_ok "{\"entities\":$eout}"
        ;;
      config)
        csub="${1:-}"; shift || true
        case "$csub" in
          list)
            _cfg_preamble
            # server.motd's live value is DB-backed (acore_auth.motd; no
            # conf/env var exists in this AC build -- see the registry note).
            # Look it up ONCE before the loop: db_auth_query's `docker exec
            # -i` reads stdin, and inside the while-read loop it would swallow
            # the remaining registry rows from the process substitution.
            # Guarded (set -e): a down DB or absent docker falls back to the
            # registry default below, so `list` still answers.
            if motd_live="$(db_auth_query "SELECT text FROM motd WHERE realmid=1 LIMIT 1;")"; then :; else motd_live=""; fi
            first=1; out='['
            while IFS='|' read -r key group label type minv maxv env def explain; do
              [[ -z "$key" ]] && continue
              # Every row is restart-to-apply EXCEPT server.motd, which the
              # worldserver applies live (MotdMgr) when `set` runs over SOAP.
              # Conf rows read conf -> .dist -> registry default; their
              # restart_required stays true here (conservative) -- the SET
              # result's `applied` field is the authoritative live/restart
              # answer, since live-apply depends on SOAP being up and no
              # frozen legacy env.
              rreq=true
              if [[ "$key" == "server.motd" ]]; then
                rreq=false
                val="$motd_live"
              elif _cfg_conf_route "$env"; then
                # Truthful pre-migration read: while a legacy AC_* override
                # is still in override.yml it BEATS the conf (env bridge),
                # so show that value until a save cleans it up.
                val="$(_cfg_env_read "$(_cfg_env_name_for "$conf_key")")"
                if [[ -z "$val" ]]; then
                  cpath="$(_cfg_conf_path "$conf_file")"
                  val="$(_cfg_conf_read "$cpath" "$conf_key")"
                  [[ -n "$val" ]] || val="$(_cfg_conf_read "$cpath.dist" "$conf_key")"
                fi
              else
                val="$(_cfg_env_read "$env")"
              fi
              [[ -n "$val" ]] || val="$def"
              minj="${minv:-null}"; maxj="${maxv:-null}"
              [[ $first -eq 0 ]] && out+=','
              out+="{\"key\":\"$key\",\"group\":\"$group\",\"label\":\"$(json_escape "$label")\",\"explain\":\"$(json_escape "$explain")\",\"type\":\"$type\",\"min\":$minj,\"max\":$maxj,\"value\":\"$(json_escape "$val")\",\"default\":\"$(json_escape "$def")\",\"restart_required\":$rreq,\"env\":\"$env\"}"
              first=0
            done < <(_cfg_rows)
            out+=']'
            json_ok "{\"settings\":$out}"
            ;;
          set)
            key=""; value=""
            while [[ $# -gt 0 ]]; do
              case "$1" in
                --key) _need_flag_val "$1" $#; key="$2"; shift 2 ;;
                --value) _need_flag_val "$1" $#; value="$2"; shift 2 ;;
                *) json_err BAD_ARG "Unknown flag: $1" ""; exit 1 ;;
              esac
            done
            [[ -n "$key" ]] || { json_err BAD_ARG "Missing --key" "See: dml wow config list --json"; exit 1; }
            if [[ "$key" == conf:* ]]; then
              # DIRECT conf route (Bot World all-keys browser): the key IS a
              # `conf:playerbots.conf:<Key>` spec, no registry row involved.
              # Restricted to playerbots.conf on purpose -- worldserver.conf
              # stays curated-rows-only. No registry type means no range
              # check, so the value is shape-validated instead: single line,
              # bounded length (playerbots values are short), written
              # verbatim. Always restart-to-apply (playerbots reads its conf
              # at startup).
              _cfg_conf_route "$key" || { json_err BAD_ARG "Bad conf key: $key" ""; exit 1; }
              if [[ "$conf_file" != "playerbots.conf" ]]; then
                json_err BAD_ARG "Direct conf keys are limited to playerbots.conf" "Other settings live in the curated list: dml wow config list --json"
                exit 1
              fi
              [[ "$conf_key" =~ ^[A-Za-z0-9_.]+$ ]] \
                || { json_err BAD_ARG "Invalid conf key: $conf_key" "Letters, digits, dots and underscores only."; exit 1; }
              case "$value" in
                *$'\n'*|*$'\r'*) json_err BAD_ARG "The value must be a single line" ""; exit 1 ;;
              esac
              if (( ${#value} > 200 )); then
                json_err BAD_ARG "Value too long (max 200 characters)" ""; exit 1
              fi
              _cfg_preamble
              CFG_CHANGED=false
              cpath="$(_cfg_conf_path "$conf_file")"
              _cfg_conf_ensure "$cpath" \
                || { json_err NOT_FOUND "$conf_file not found (nor its .dist)" "Is the WoW server fully installed?"; exit 1; }
              _cfg_conf_write "$cpath" "$conf_key" "$value" \
                || { json_err WRITE_FAILED "Could not write $conf_file" ""; exit 1; }
              ename="$(_cfg_env_name_for "$conf_key")"
              if [[ -n "$(_cfg_env_read "$ename")" ]]; then
                _cfg_env_remove "$ename"
                CFG_CHANGED=true
              fi
              if [[ "$CFG_CHANGED" == true ]]; then
                json_ok '{"changed":true,"restart_required":true,"applied":"restart"}'
              else
                json_ok '{"changed":false,"restart_required":false,"applied":"none"}'
              fi
              exit 0
            fi
            row="$(_cfg_rows | grep -F "$key|" | head -1)" || true
            [[ "$row" == "$key|"* ]] || { json_err NOT_FOUND "Unknown setting: $key" "See: dml wow config list --json"; exit 1; }
            IFS='|' read -r _ group label type minv maxv env def explain <<< "$row"
            _cfg_preamble
            CFG_CHANGED=false
            case "$type" in
              float)
                _float_in_range "$value" "$minv" "$maxv" \
                  || { json_err BAD_ARG "$label must be a number between $minv and $maxv, got: $value" ""; exit 1; }
                ;;
              int)
                [[ "$value" =~ ^[0-9]+$ ]] && (( value >= minv && value <= maxv )) \
                  || { json_err BAD_ARG "$label must be a whole number between $minv and $maxv, got: $value" ""; exit 1; }
                ;;
              bool)
                [[ "$value" =~ ^[01]$ ]] \
                  || { json_err BAD_ARG "$label takes 1 (on) or 0 (off), got: $value" ""; exit 1; }
                ;;
              text)
                value="${value//\"/}"; value="${value//$'\n'/ }"; value="${value//$'\r'/ }"
                ;;
              char)
                _valid_charname "$value" \
                  || { json_err BAD_ARG "Invalid character name: $value" "1-12 letters/digits/underscore."; exit 1; }
                ;;
            esac
            if [[ "$key" == "server.motd" ]]; then
              # Motd is DB-backed and applied LIVE by the worldserver (MotdMgr)
              # -- no conf/env var exists in this AC build, so `set` goes over
              # SOAP (`.server set motd <realm> <locale> <text>`) instead of
              # the override env path, and restart_required is false. The text
              # is the command's tail, so spaces are fine unquoted -- and it
              # must NOT be quote-wrapped (AC's parser keeps quotes literal;
              # live-confirmed 2026-07-15). Guarded assignment (set -e; same
              # pattern as wow soap-exec above).
              if out="$(soap_exec "server set motd 1 enUS $value")"; then rc=0; else rc=$?; fi
              case "$rc" in
                0) json_ok '{"changed":true,"restart_required":false}' ;;
                3) json_err SOAP_AUTH "SOAP authentication failed" "Check ~/.dml/soap.env"; exit 1 ;;
                2) json_err SOAP_FAULT "$out" "The server rejected the motd command."; exit 1 ;;
                *) json_err SOAP_UNREACHABLE "Could not reach the server" "The server must be running to change the message of the day - start it first."; exit 1 ;;
              esac
            elif _cfg_conf_route "$env"; then
              # Conf-file row (see 40-config.sh registry block): write the
              # bind-mounted conf, clean any legacy AC_* env override (env
              # beats conf, so leaving it would make this save a silent
              # no-op), then try to live-apply. Live only works for
              # worldserver.conf keys (SOAP `reload config`) AND only when
              # no frozen legacy env was still present in the override --
              # the running container keeps its creation-time env either
              # way, so that case must report restart.
              cpath="$(_cfg_conf_path "$conf_file")"
              _cfg_conf_ensure "$cpath" \
                || { json_err NOT_FOUND "$conf_file not found (nor its .dist)" "Is the WoW server fully installed?"; exit 1; }
              _cfg_conf_write "$cpath" "$conf_key" "$value" \
                || { json_err WRITE_FAILED "Could not write $conf_file" ""; exit 1; }
              if [[ "$key" == "bots.population" ]]; then
                # One number drives BOTH population bounds (the row's conf
                # key is MaxRandomBots; Min follows it here).
                _cfg_conf_write "$cpath" "AiPlayerbot.MinRandomBots" "$value" \
                  || { json_err WRITE_FAILED "Could not write $conf_file" ""; exit 1; }
              fi
              envwas=false
              ename="$(_cfg_env_name_for "$conf_key")"
              if [[ -n "$(_cfg_env_read "$ename")" ]]; then
                _cfg_env_remove "$ename"
                envwas=true
                CFG_CHANGED=true
              fi
              if [[ "$key" == "bots.population" ]]; then
                ename="$(_cfg_env_name_for AiPlayerbot.MinRandomBots)"
                if [[ -n "$(_cfg_env_read "$ename")" ]]; then
                  _cfg_env_remove "$ename"
                  envwas=true
                  CFG_CHANGED=true
                fi
              fi
              applied="none"; rreq=false
              if [[ "$CFG_CHANGED" == true ]]; then
                applied="restart"; rreq=true
                if [[ "$conf_file" == "worldserver.conf" && "$envwas" == false ]]; then
                  if soap_exec "reload config" >/dev/null 2>&1; then
                    applied="live"; rreq=false
                  fi
                fi
              fi
              json_ok "{\"changed\":$CFG_CHANGED,\"restart_required\":$rreq,\"applied\":\"$applied\"}"
            else
              if [[ "$key" == "ahbot.character" ]]; then
                crow="$(db_chars_query "SELECT guid, account FROM characters WHERE name='$(sql_escape "$value")' LIMIT 1;")" \
                  || { json_err DB_UNREACHABLE "Could not look up the character" "Is ac-database running?"; exit 1; }
                [[ -n "$crow" ]] || { json_err NOT_FOUND "No such character: $value" ""; exit 1; }
                IFS=$'\t' read -r cguid cacct <<< "$crow"
                [[ "$cguid" =~ ^[0-9]+$ && "$cacct" =~ ^[0-9]+$ ]] \
                  || { json_err DB_UNREACHABLE "Unexpected character lookup result" ""; exit 1; }
                _cfg_env_write AC_AUCTION_HOUSE_BOT_GUID "$cguid"
                _cfg_env_write AC_AUCTION_HOUSE_BOT_ACCOUNT "$cacct"
              else
                _cfg_env_write "$env" "$value"
              fi
              json_ok "{\"changed\":$CFG_CHANGED,\"restart_required\":$CFG_CHANGED}"
            fi
            ;;
          pb-keys)
            # Bot World all-keys browser: every active `Key = value` line of
            # playerbots.conf (falling back to the .dist when the conf does
            # not exist yet), plus each key's .dist default when both files
            # exist. Duplicate keys keep their FIRST position but the LAST
            # value/line wins (AC read semantics). Values are the raw right-
            # hand side, trimmed -- quotes preserved so an edit round-trips
            # verbatim through `config set conf:playerbots.conf:<Key>`.
            _cfg_preamble
            pbconf="$cfg_sdir/env/dist/etc/modules/playerbots.conf"
            pbdist="$pbconf.dist"
            pbsrc="$pbconf"
            [[ -f "$pbsrc" ]] || pbsrc="$pbdist"
            if [[ ! -f "$pbsrc" ]]; then
              json_err NOT_FOUND "playerbots.conf not found (nor its .dist)" "Is the WoW server fully installed?"
              exit 1
            fi
            declare -A _pb_val=() _pb_line=() _pb_def=()
            _pb_order=()
            while IFS=$'\x1f' read -r k v ln; do
              [[ -n "${_pb_val[$k]+x}" ]] || _pb_order+=("$k")
              _pb_val["$k"]="$v"; _pb_line["$k"]="$ln"
            done < <(_pb_kv_lines "$pbsrc")
            if [[ "$pbsrc" != "$pbdist" && -f "$pbdist" ]]; then
              while IFS=$'\x1f' read -r k v ln; do
                _pb_def["$k"]="$v"
              done < <(_pb_kv_lines "$pbdist")
            fi
            first=1; out='['
            for k in ${_pb_order[@]+"${_pb_order[@]}"}; do
              dv=null
              if [[ "$pbsrc" == "$pbdist" ]]; then
                dv="\"$(json_escape "${_pb_val[$k]}")\""
              elif [[ -n "${_pb_def[$k]+x}" ]]; then
                dv="\"$(json_escape "${_pb_def[$k]}")\""
              fi
              [[ $first -eq 0 ]] && out+=','
              out+="{\"key\":\"$(json_escape "$k")\",\"value\":\"$(json_escape "${_pb_val[$k]}")\",\"default\":$dv,\"line\":${_pb_line[$k]}}"
              first=0
            done
            out+=']'
            pbsrc_name="playerbots.conf"
            [[ "$pbsrc" == "$pbdist" ]] && pbsrc_name="playerbots.conf.dist"
            json_ok "{\"source\":\"$pbsrc_name\",\"keys\":$out}"
            ;;
          files)
            # Dynamic editable-conf list (Batch 1 F3): the fixed four plus
            # every *.conf / *.conf.dist basename found under modules/
            # (deduped; the fixed names win if a module conf shadows them).
            # Powers the GUI file picker instead of its old hardcoded list.
            _cfg_preamble
            moddir="$cfg_sdir/env/dist/etc/modules"
            dynnames="$(ls -1 "$moddir" 2>/dev/null | sed 's/\.dist$//' | grep -E '^[A-Za-z0-9_.-]+\.conf$' | grep -vE '^(worldserver|authserver)\.conf$' | sort -u)" || dynnames=""
            first=1; out='['
            for fname in .env docker-compose.override.yml worldserver.conf authserver.conf $dynnames; do
              fpath="$(_cfg_file_path "$fname")" || continue
              fex=false; fdist=false; fro=false
              [[ -f "$fpath" ]] && fex=true
              [[ -f "$fpath.dist" ]] && fdist=true
              case "$fname" in .env|docker-compose.override.yml) fro=true ;; esac
              [[ $first -eq 0 ]] && out+=','
              out+="{\"name\":\"$(json_escape "$fname")\",\"exists\":$fex,\"dist\":$fdist,\"readonly\":$fro}"
              first=0
            done
            out+=']'
            json_ok "{\"files\":$out}"
            ;;
          raw-read)
            fname=""
            [[ "${1:-}" == "--file" ]] && { _need_flag_val "$1" $#; fname="$2"; shift 2; }
            [[ -n "$fname" ]] || { json_err BAD_ARG "Missing --file <name>" ""; exit 1; }
            _cfg_preamble
            fpath="$(_cfg_file_path "$fname")" \
              || { json_err NOT_FOUND "Not an editable file: $fname" "See: dml wow config files --json"; exit 1; }
            # A conf that only exists as its .dist yet reads as the dist --
            # the first save then creates the real conf (raw-write).
            if [[ ! -f "$fpath" && -f "$fpath.dist" ]]; then
              json_ok "{\"file\":\"$(json_escape "$fname")\",\"source\":\"dist\",\"content\":\"$(json_escape "$(cat "$fpath.dist")")\"}"
              exit 0
            fi
            [[ -f "$fpath" ]] || { json_err NOT_FOUND "File does not exist yet: $fname" ""; exit 1; }
            json_ok "{\"file\":\"$(json_escape "$fname")\",\"source\":\"conf\",\"content\":\"$(json_escape "$(cat "$fpath")")\"}"
            ;;
          raw-reset)
            # Reset-from-.dist (Batch 1 F3): copy <name>.conf.dist over the
            # conf, with the same automatic .bak the raw editor takes before
            # writes. .env/override have no dist and stay untouchable here.
            fname=""
            [[ "${1:-}" == "--file" ]] && { _need_flag_val "$1" $#; fname="$2"; shift 2; }
            [[ -n "$fname" ]] || { json_err BAD_ARG "Missing --file <name>" ""; exit 1; }
            _cfg_preamble
            fpath="$(_cfg_file_path "$fname")" \
              || { json_err NOT_FOUND "Not an editable file: $fname" "See: dml wow config files --json"; exit 1; }
            case "$fname" in
              .env|docker-compose.override.yml)
                json_err BAD_ARG "That file has no defaults to reset to" ""
                exit 1
                ;;
            esac
            [[ -f "$fpath.dist" ]] || { json_err NOT_FOUND "No $fname.dist to reset from" ""; exit 1; }
            bakjson=null
            if [[ -f "$fpath" ]]; then
              cp -p "$fpath" "$fpath.bak"
              bakjson="\"$(json_escape "$fname.bak")\""
            fi
            cp "$fpath.dist" "$fpath"
            json_ok "{\"reset\":true,\"file\":\"$(json_escape "$fname")\",\"backup\":$bakjson}"
            ;;
          raw-write)
            fname=""
            [[ "${1:-}" == "--file" ]] && { _need_flag_val "$1" $#; fname="$2"; shift 2; }
            [[ -n "$fname" ]] || { json_err BAD_ARG "Missing --file <name>" ""; exit 1; }
            _cfg_preamble
            fpath="$(_cfg_file_path "$fname")" \
              || { json_err NOT_FOUND "Not an editable file: $fname" "See: dml wow config files --json"; exit 1; }
            mkdir -p "$(dirname "$fpath")"
            tmp="$fpath.tmp.$$"
            cat > "$tmp"
            if [[ "$fname" == "docker-compose.override.yml" ]]; then
              # A syntactically broken override stops the whole stack from
              # even starting -- validate BEFORE touching the real file.
              if ! "$DML_YQ_BIN" e '.' "$tmp" >/dev/null 2>&1; then
                rm -f "$tmp"
                json_err BAD_ARG "That is not valid YAML - not saved" "Fix the syntax and save again."
                exit 1
              fi
            fi
            # SECURITY: .env and the compose override are readable via
            # raw-read (the allowlist above still covers all 5 names) but
            # NOT writable here. A raw-write to either, combined with
            # `games restart`, would let the Advanced Files editor drive
            # host command execution (env/volume/entrypoint injection into
            # Docker Compose). Reject BEFORE the real path is ever touched
            # (tmp is discarded, never mv'd) -- writable only via the
            # curated `config set` path / Settings tab.
            case "$fname" in
              .env|docker-compose.override.yml)
                rm -f "$tmp"
                json_err BAD_ARG "That file is read-only in the editor" "Change these settings from the Settings tab; .env and the compose override can't be overwritten here."
                exit 1
                ;;
            esac
            bakjson=null
            if [[ -f "$fpath" ]]; then
              cp -p "$fpath" "$fpath.bak"
              bakjson="\"$(json_escape "$fname.bak")\""
            fi
            mv "$tmp" "$fpath"
            json_ok "{\"written\":true,\"backup\":$bakjson}"
            ;;
          *)
            json_err BAD_ARG "Unknown config subcommand: $csub" "Try: dml wow config list --json"
            exit 1
            ;;
        esac
        ;;
      bridge-setup|party-setup|setup)
        # Streaming (NDJSON) like games start/restart. NB: matched directly
        # on $wsub ("dml wow bridge-setup"), not via a nested "party" dispatch
        # -- $wsub is the single token "bridge-setup", so a `party)` arm would
        # never match it (bash case patterns are exact/glob, not prefix).
        [[ "$DML_JSON" == 1 ]] && ndjson_section_start bridge-setup
        sdir="$(_wow_server_dir)"
        if [[ -z "$sdir" ]]; then
          if [[ "$DML_JSON" == 1 ]]; then
            ndjson_section_end bridge-setup error
            ndjson_error NOT_FOUND "WoW Playerbots server not installed" "Install it first."
          else echo "[dml] ERROR: wow server not installed" >&2; fi
          exit 1
        fi
        # Preflight: SOAP reachable (a loaded bridge needs a running,
        # SOAP-reachable server to matter). Probe with a harmless command.
        [[ "$DML_JSON" == 1 ]] && ndjson_line info "checking SOAP..."
        if out="$(soap_exec 'server info')"; then :; else
          rc=$?
          if [[ "$DML_JSON" == 1 ]]; then
            ndjson_section_end bridge-setup error
            case "$rc" in
              3) ndjson_error SOAP_AUTH "SOAP auth failed" "Check ~/.dml/soap.env" ;;
              *) ndjson_error SOAP_UNREACHABLE "Could not reach the server over SOAP" "Start the server, then re-run." ;;
            esac
          fi
          exit 1
        fi
        [[ "$DML_JSON" == 1 ]] && ndjson_line info "deploying bridge scripts..."
        changed="$(_bridge_deploy_scripts "$sdir")"
        ch=false; [[ "$changed" == changed ]] && ch=true   # bare (top-level dispatch, no `local`)
        if [[ "$DML_JSON" == 1 ]]; then
          ndjson_line info "scripts deployed (changed=$ch)"
          ndjson_section_end bridge-setup ok
          ndjson_done "{\"changed\":$ch,\"restart_required\":$ch}"
        else
          echo "[dml] bridge-setup done (changed=$ch, restart_required=$ch)"
        fi
        ;;
      party)
        psub="${1:-}"; shift || true
        case "$psub" in
          online)
            sql="SELECT c.guid, c.name, c.class, c.level
                 FROM characters c
                 WHERE c.online = 1
                   AND c.account NOT IN (
                     SELECT account_id FROM acore_playerbots.playerbots_account_type
                     WHERE account_type IN (1,2))
                 ORDER BY c.name;"
            rows="$(db_chars_query "$sql")" \
              || { json_err DB_UNREACHABLE "Could not query online characters" "Is ac-database running?"; exit 1; }
            first=1; out='['
            while IFS=$'\t' read -r guid name cls lvl || [[ -n "$guid" ]]; do
              [[ -z "$guid" ]] && continue
              [[ $first -eq 0 ]] && out+=','
              out+="{\"guid\":$guid,\"name\":\"$(json_escape "$name")\",\"class\":$cls,\"level\":$lvl}"
              first=0
            done <<< "$rows"
            out+=']'
            json_ok "{\"online\":$out}"
            ;;
          add)
            player=""; class=""; gender=""
            while [[ $# -gt 0 ]]; do
              case "$1" in
                --player) _need_flag_val "$1" $#; player="$2"; shift 2 ;;
                --class) _need_flag_val "$1" $#; class="$2"; shift 2 ;;
                --gender) _need_flag_val "$1" $#; gender="$2"; shift 2 ;;
                *) json_err BAD_ARG "Unknown flag: $1" ""; exit 1 ;;
              esac
            done
            _valid_charname "$player" || { json_err BAD_ARG "Invalid player name: $player" ""; exit 1; }
            _valid_bot_class "$class" || { json_err BAD_ARG "Invalid class: $class" "One of: warrior paladin hunter rogue priest shaman mage warlock druid"; exit 1; }
            case "$gender" in ""|male|female) : ;; *) json_err BAD_ARG "Invalid gender: $gender" "male or female"; exit 1 ;; esac
            pguid="$(_party_online_guid "$player")"
            [[ "$pguid" =~ ^[0-9]+$ ]] || { json_err NOT_FOUND "Character not online: $player" "Log the character into the game first, then try again."; exit 1; }
            # Snapshot the group's members BEFORE firing so we can spot the new one.
            before="$(_party_group_member_guids "$pguid" | tr '\n' ' ')"
            cmd="dml_addclass $player $class"; [[ -n "$gender" ]] && cmd+=" $gender"
            if out="$(soap_exec "$cmd")"; then :; else
              rc=$?
              # A fault here most likely means the bridge isn't loaded yet.
              case "$rc" in
                3) json_err SOAP_AUTH "SOAP auth failed" "Check ~/.dml/soap.env"; exit 1 ;;
                2) json_err SOAP_FAULT "The add command was rejected" "Deploy the server bridges (bridge-setup) and restart the server first."; exit 1 ;;
                *) json_err SOAP_UNREACHABLE "Could not reach the server" "Is it running?"; exit 1 ;;
              esac
            fi
            newguid="$(_party_wait_new_member "$pguid" "$before")"
            if [[ -n "$newguid" ]]; then
              botname=""
              if [[ "$newguid" =~ ^[0-9]+$ ]]; then
                botname="$(db_chars_query "SELECT name FROM characters WHERE guid=$newguid LIMIT 1;" 2>/dev/null)" || botname=""
              fi
              if [[ -n "$botname" ]]; then
                json_ok "{\"added\":true,\"joined\":true,\"bot\":\"$(json_escape "$botname")\",\"note\":null}"
              else
                json_ok "{\"added\":true,\"joined\":true,\"bot\":null,\"note\":null}"
              fi
            else
              json_ok "{\"added\":true,\"joined\":false,\"bot\":null,\"note\":\"Spawned but not attached yet -- give it a moment and Refresh.\"}"
            fi
            ;;
          list)
            player=""
            [[ "${1:-}" == "--player" ]] && { _need_flag_val "$1" $#; player="$2"; shift 2; }
            _valid_charname "$player" || { json_err BAD_ARG "Invalid player name: $player" ""; exit 1; }
            pguid="$(_party_online_guid "$player")"
            [[ "$pguid" =~ ^[0-9]+$ ]] || { json_err NOT_FOUND "Character not online: $player" "Log the character into the game first."; exit 1; }
            sql="SELECT c.guid, c.name, c.class, c.level,
                        CASE WHEN c.account IN (SELECT account_id FROM acore_playerbots.playerbots_account_type WHERE account_type IN (1,2)) THEN 1 ELSE 0 END AS is_bot
                 FROM group_member gm
                 JOIN characters c ON c.guid = gm.memberGuid
                 WHERE gm.guid = (SELECT guid FROM group_member WHERE memberGuid=$pguid LIMIT 1)
                 ORDER BY is_bot, c.name;"
            rows="$(db_chars_query "$sql")" \
              || { json_err DB_UNREACHABLE "Could not query the party" ""; exit 1; }
            first=1; out='['
            while IFS=$'\t' read -r guid name cls lvl isbot || [[ -n "$guid" ]]; do
              [[ -z "$guid" ]] && continue
              [[ $first -eq 0 ]] && out+=','
              local_bot=false; [[ "$isbot" == "1" ]] && local_bot=true
              out+="{\"guid\":$guid,\"name\":\"$(json_escape "$name")\",\"class\":$cls,\"level\":$lvl,\"is_bot\":$local_bot}"
              first=0
            done <<< "$rows"
            out+=']'
            json_ok "{\"members\":$out}"
            ;;
          kick)
            bot=""
            [[ "${1:-}" == "--bot" ]] && { _need_flag_val "$1" $#; bot="$2"; shift 2; }
            _valid_charname "$bot" || { json_err BAD_ARG "Invalid bot name: $bot" ""; exit 1; }
            _party_fire "dml_uninvite $bot" "kick"
            json_ok "{\"kicked\":true}"
            ;;
          relogin)
            player=""; bot=""
            while [[ $# -gt 0 ]]; do
              case "$1" in
                --player) _need_flag_val "$1" $#; player="$2"; shift 2 ;;
                --bot) _need_flag_val "$1" $#; bot="$2"; shift 2 ;;
                *) json_err BAD_ARG "Unknown flag: $1" ""; exit 1 ;;
              esac
            done
            _valid_charname "$player" || { json_err BAD_ARG "Invalid player name: $player" ""; exit 1; }
            _valid_charname "$bot" || { json_err BAD_ARG "Invalid bot name: $bot" ""; exit 1; }
            _party_fire "dml_login $player $bot" "relogin"
            json_ok "{\"relogged\":true}"
            ;;
          botcmd)
            player=""; bot=""; action=""
            while [[ $# -gt 0 ]]; do
              case "$1" in
                --player) _need_flag_val "$1" $#; player="$2"; shift 2 ;;
                --bot) _need_flag_val "$1" $#; bot="$2"; shift 2 ;;
                --action) _need_flag_val "$1" $#; action="$2"; shift 2 ;;
                *) json_err BAD_ARG "Unknown flag: $1" ""; exit 1 ;;
              esac
            done
            _valid_charname "$player" || { json_err BAD_ARG "Invalid player name: $player" ""; exit 1; }
            _valid_charname "$bot" || { json_err BAD_ARG "Invalid bot name: $bot" ""; exit 1; }
            # Closed allowlist -> fixed whisper strings. This is the whole
            # whisper surface: no free-text path exists.
            case "$action" in
              gear) wmsg="autogear" ;;
              talents) wmsg="talents autopick" ;;
              maintain) wmsg="maintenance" ;;
              *) json_err BAD_ARG "Invalid action: $action" "One of: gear talents maintain"; exit 1 ;;
            esac
            pguid="$(_party_online_guid "$player")"
            [[ "$pguid" =~ ^[0-9]+$ ]] || { json_err NOT_FOUND "Character not online: $player" "Log the character into the game first."; exit 1; }
            bguid="$(_party_online_guid "$bot")"
            [[ "$bguid" =~ ^[0-9]+$ ]] || { json_err NOT_FOUND "Character not online: $bot" "The bot must be in the world -- is it still in your party?"; exit 1; }
            _party_fire "dml_whisper $player $bot $wmsg" "botcmd"
            json_ok "{\"sent\":true,\"player\":\"$(json_escape "$player")\",\"bot\":\"$(json_escape "$bot")\",\"action\":\"$action\"}"
            ;;
          preset-save)
            player=""; name=""
            while [[ $# -gt 0 ]]; do
              case "$1" in
                --player) _need_flag_val "$1" $#; player="$2"; shift 2 ;;
                --name) _need_flag_val "$1" $#; name="$2"; shift 2 ;;
                *) json_err BAD_ARG "Unknown flag: $1" ""; exit 1 ;;
              esac
            done
            _valid_charname "$player" || { json_err BAD_ARG "Invalid player name: $player" ""; exit 1; }
            _valid_preset_name "$name" || { json_err BAD_ARG "Invalid preset name: $name" "Letters, digits, - and _ (max 32)."; exit 1; }
            pguid="$(_party_online_guid "$player")"
            [[ "$pguid" =~ ^[0-9]+$ ]] || { json_err NOT_FOUND "Character not online: $player" "Log the character into the game first."; exit 1; }
            sql="SELECT c.class
                 FROM group_member gm
                 JOIN characters c ON c.guid = gm.memberGuid
                 WHERE gm.guid = (SELECT guid FROM group_member WHERE memberGuid=$pguid LIMIT 1)
                   AND c.account IN (SELECT account_id FROM acore_playerbots.playerbots_account_type WHERE account_type IN (1,2))
                 ORDER BY c.name;"
            rows="$(db_chars_query "$sql")" \
              || { json_err DB_UNREACHABLE "Could not read the party" "Is ac-database running?"; exit 1; }
            names=""
            while IFS=$'\t' read -r cls || [[ -n "$cls" ]]; do
              [[ -z "$cls" ]] && continue
              cname="$(_class_name_from_id "$cls")"
              [[ -n "$cname" ]] && names+="$cname"$'\n'
            done <<< "$rows"
            [[ -n "$names" ]] || { json_err NOT_FOUND "Party has no bots to save" "Add some bots first."; exit 1; }
            pdir="$(_preset_dir)"; mkdir -p "$pdir"
            overwrote=false; [[ -f "$pdir/$name" ]] && overwrote=true
            printf '%s' "$names" > "$pdir/$name"
            jarr=""; first=1
            while IFS= read -r n || [[ -n "$n" ]]; do
              [[ -z "$n" ]] && continue
              [[ $first -eq 0 ]] && jarr+=','
              jarr+="\"$n\""; first=0
            done <<< "$names"
            json_ok "{\"saved\":true,\"name\":\"$name\",\"bots\":[$jarr],\"overwrote\":$overwrote}"
            ;;
          preset-list)
            pdir="$(_preset_dir)"
            first=1; out='['
            if [[ -d "$pdir" ]]; then
              for f in "$pdir"/*; do
                [[ -f "$f" ]] || continue
                pname="$(basename "$f")"
                _valid_preset_name "$pname" || continue
                cnt="$(grep -c . "$f" 2>/dev/null)" || cnt=0
                [[ $first -eq 0 ]] && out+=','
                out+="{\"name\":\"$pname\",\"bots\":$cnt}"
                first=0
              done
            fi
            out+=']'
            json_ok "{\"presets\":$out}"
            ;;
          preset-delete)
            name=""
            [[ "${1:-}" == "--name" ]] && { _need_flag_val "$1" $#; name="$2"; shift 2; }
            _valid_preset_name "$name" || { json_err BAD_ARG "Invalid preset name: $name" ""; exit 1; }
            pdir="$(_preset_dir)"
            [[ -f "$pdir/$name" ]] || { json_err NOT_FOUND "No preset named $name" ""; exit 1; }
            rm -f "$pdir/$name"
            json_ok "{\"deleted\":true,\"name\":\"$name\"}"
            ;;
          preset-load)
            player=""; name=""
            while [[ $# -gt 0 ]]; do
              case "$1" in
                --player) _need_flag_val "$1" $#; player="$2"; shift 2 ;;
                --name) _need_flag_val "$1" $#; name="$2"; shift 2 ;;
                *) json_err BAD_ARG "Unknown flag: $1" ""; exit 1 ;;
              esac
            done
            _valid_charname "$player" || { json_err BAD_ARG "Invalid player name: $player" ""; exit 1; }
            _valid_preset_name "$name" || { json_err BAD_ARG "Invalid preset name: $name" ""; exit 1; }
            [[ "$DML_JSON" == 1 ]] && ndjson_section_start preset-load
            pdir="$(_preset_dir)"
            if [[ ! -f "$pdir/$name" ]]; then
              if [[ "$DML_JSON" == 1 ]]; then
                ndjson_section_end preset-load error
                ndjson_error NOT_FOUND "No preset named $name" ""
              else echo "[dml] ERROR: no preset $name" >&2; fi
              exit 1
            fi
            pguid="$(_party_online_guid "$player")"
            if ! [[ "$pguid" =~ ^[0-9]+$ ]]; then
              if [[ "$DML_JSON" == 1 ]]; then
                ndjson_section_end preset-load error
                ndjson_error NOT_FOUND "Character not online: $player" "Log the character into the game first."
              else echo "[dml] ERROR: $player not online" >&2; fi
              exit 1
            fi
            # Kick phase (replace semantics): every current bot goes.
            sql="SELECT c.name
                 FROM group_member gm
                 JOIN characters c ON c.guid = gm.memberGuid
                 WHERE gm.guid = (SELECT guid FROM group_member WHERE memberGuid=$pguid LIMIT 1)
                   AND c.account IN (SELECT account_id FROM acore_playerbots.playerbots_account_type WHERE account_type IN (1,2))
                 ORDER BY c.name;"
            kicklist="$(db_chars_query "$sql")" || kicklist=""
            while IFS= read -r b || [[ -n "$b" ]]; do
              [[ -z "$b" ]] && continue
              if out="$(soap_exec "dml_uninvite $b")"; then
                [[ "$DML_JSON" == 1 ]] && ndjson_line info "kicked $b"
              else
                [[ "$DML_JSON" == 1 ]] && ndjson_line warn "could not kick $b"
              fi
            done <<< "$kicklist"
            requested=0; joined=0
            while IFS= read -r cls || [[ -n "$cls" ]]; do
              [[ -z "$cls" ]] && continue
              _valid_bot_class "$cls" || { [[ "$DML_JSON" == 1 ]] && ndjson_line warn "skipping unknown class: $cls"; continue; }
              requested=$(( requested + 1 ))
              before="$(_party_group_member_guids "$pguid" | tr '\n' ' ')"
              if out="$(soap_exec "dml_addclass $player $cls")"; then :; else
                [[ "$DML_JSON" == 1 ]] && ndjson_line warn "add $cls was rejected"
                continue
              fi
              newguid="$(_party_wait_new_member "$pguid" "$before")"
              if [[ "$newguid" =~ ^[0-9]+$ ]]; then
                joined=$(( joined + 1 ))
                bname="$(db_chars_query "SELECT name FROM characters WHERE guid=$newguid LIMIT 1;" 2>/dev/null)" || bname=""
                if [[ -n "$bname" ]]; then
                  out="$(soap_exec "dml_whisper $player $bname talents autopick")" || true
                  out="$(soap_exec "dml_whisper $player $bname autogear")" || true
                  [[ "$DML_JSON" == 1 ]] && ndjson_line info "$bname joined -- talents + gear applied"
                else
                  [[ "$DML_JSON" == 1 ]] && ndjson_line info "a $cls joined"
                fi
              else
                [[ "$DML_JSON" == 1 ]] && ndjson_line warn "$cls did not attach in time"
              fi
            done < "$pdir/$name"
            if [[ "$DML_JSON" == 1 ]]; then
              ndjson_section_end preset-load ok
              ndjson_done "{\"loaded\":true,\"requested\":$requested,\"joined\":$joined}"
            else
              echo "[dml] preset-load done ($joined/$requested joined)"
            fi
            ;;
          preset-show)
            name=""
            [[ "${1:-}" == "--name" ]] && { _need_flag_val "$1" $#; name="$2"; shift 2; }
            _valid_preset_name "$name" || { json_err BAD_ARG "Invalid preset name: $name" ""; exit 1; }
            pdir="$(_preset_dir)"
            [[ -f "$pdir/$name" ]] || { json_err NOT_FOUND "No preset named $name" ""; exit 1; }
            jarr=""; first=1
            while IFS= read -r cls || [[ -n "$cls" ]]; do
              [[ -z "$cls" ]] && continue
              [[ $first -eq 0 ]] && jarr+=','
              jarr+="\"$(json_escape "$cls")\""; first=0
            done < "$pdir/$name"
            json_ok "{\"name\":\"$(json_escape "$name")\",\"classes\":[$jarr]}"
            ;;
          preset-import)
            name=""; classes=""; force=0
            while [[ $# -gt 0 ]]; do
              case "$1" in
                --name) _need_flag_val "$1" $#; name="$2"; shift 2 ;;
                --classes) _need_flag_val "$1" $#; classes="$2"; shift 2 ;;
                --force) force=1; shift ;;
                *) json_err BAD_ARG "Unknown flag: $1" ""; exit 1 ;;
              esac
            done
            _valid_preset_name "$name" || { json_err BAD_ARG "Invalid preset name: $name" "Letters, digits, - and _ (max 32)."; exit 1; }
            [[ -n "$classes" ]] || { json_err BAD_ARG "Missing --classes <comma-separated list>" "One of: warrior paladin hunter rogue priest shaman mage warlock druid"; exit 1; }
            # Validate EVERY token before writing anything -- a bad class
            # anywhere in the list must leave the filesystem untouched.
            IFS=',' read -ra _import_classes <<< "$classes"
            lines=""
            for c in "${_import_classes[@]}"; do
              _valid_bot_class "$c" || { json_err BAD_ARG "Invalid class: $c" "One of: warrior paladin hunter rogue priest shaman mage warlock druid"; exit 1; }
              lines+="$c"$'\n'
            done
            pdir="$(_preset_dir)"
            if [[ -f "$pdir/$name" && "$force" != 1 ]]; then
              json_err EXISTS "Preset already exists: $name" "Pass --force to overwrite."
              exit 1
            fi
            mkdir -p "$pdir"
            printf '%s' "$lines" > "$pdir/$name"
            jarr=""; first=1
            for c in "${_import_classes[@]}"; do
              [[ $first -eq 0 ]] && jarr+=','
              jarr+="\"$c\""; first=0
            done
            json_ok "{\"imported\":true,\"name\":\"$(json_escape "$name")\",\"classes\":[$jarr]}"
            ;;
          *)
            json_err UNKNOWN_COMMAND "Unknown party subcommand: $psub" "Try: dml wow party online|add|list|kick|relogin|botcmd|preset-save|preset-list|preset-delete|preset-load|preset-show|preset-import --json"
            exit 1
            ;;
        esac
        ;;
      gm)
        gsub="${1:-}"; shift || true
        case "$gsub" in
          level)
            player=""; level=""
            while [[ $# -gt 0 ]]; do
              case "$1" in
                --player) _need_flag_val "$1" $#; player="$2"; shift 2 ;;
                --level) _need_flag_val "$1" $#; level="$2"; shift 2 ;;
                *) json_err BAD_ARG "Unknown flag: $1" ""; exit 1 ;;
              esac
            done
            _valid_charname "$player" || { json_err BAD_ARG "Invalid player name: $player" ""; exit 1; }
            if ! [[ "$level" =~ ^[0-9]+$ ]] || (( 10#$level < 1 || 10#$level > 255 )); then
              json_err BAD_ARG "Invalid level: $level" "Use 1-255 (your server's own max level still applies)."; exit 1
            fi
            level=$(( 10#$level ))
            # Stock AC command; works for OFFLINE characters too. Success is
            # the ok envelope itself -- the result text is not parsed.
            if out="$(soap_exec ".character level $player $level")"; then :; else
              rc=$?
              case "$rc" in
                3) json_err SOAP_AUTH "SOAP auth failed" "Check ~/.dml/soap.env"; exit 1 ;;
                2) json_err SOAP_FAULT "The level command was rejected" "Does the character exist? The server said no."; exit 1 ;;
                *) json_err SOAP_UNREACHABLE "Could not reach the server" "Is it running?"; exit 1 ;;
              esac
            fi
            json_ok "{\"leveled\":true,\"player\":\"$(json_escape "$player")\",\"level\":$level}"
            ;;
          gold)
            player=""; gold=""
            while [[ $# -gt 0 ]]; do
              case "$1" in
                --player) _need_flag_val "$1" $#; player="$2"; shift 2 ;;
                --gold) _need_flag_val "$1" $#; gold="$2"; shift 2 ;;
                *) json_err BAD_ARG "Unknown flag: $1" ""; exit 1 ;;
              esac
            done
            _valid_charname "$player" || { json_err BAD_ARG "Invalid player name: $player" ""; exit 1; }
            if ! [[ "$gold" =~ ^[0-9]+$ ]] || (( 10#$gold > 214748 )); then
              json_err BAD_ARG "Invalid gold amount: $gold" "Whole gold, 0-214748 (the WotLK money cap)."; exit 1
            fi
            gold=$(( 10#$gold ))
            _gm_require_online "$player"
            copper=$(( gold * 10000 ))
            _party_fire "dml_gm_money $player $copper" "gold"
            json_ok "{\"gold_set\":true,\"player\":\"$(json_escape "$player")\",\"gold\":$gold}"
            ;;
          heal)
            player=""
            [[ "${1:-}" == "--player" ]] && { _need_flag_val "$1" $#; player="$2"; shift 2; }
            _valid_charname "$player" || { json_err BAD_ARG "Invalid player name: $player" ""; exit 1; }
            _gm_require_online "$player"
            _party_fire "dml_gm_health $player 100" "heal"
            json_ok "{\"healed\":true,\"player\":\"$(json_escape "$player")\"}"
            ;;
          revive)
            player=""
            [[ "${1:-}" == "--player" ]] && { _need_flag_val "$1" $#; player="$2"; shift 2; }
            _valid_charname "$player" || { json_err BAD_ARG "Invalid player name: $player" ""; exit 1; }
            _gm_require_online "$player"
            _party_fire "dml_gm_revive $player" "revive"
            json_ok "{\"revived\":true,\"player\":\"$(json_escape "$player")\"}"
            ;;
          summon)
            player=""; entry=""
            while [[ $# -gt 0 ]]; do
              case "$1" in
                --player) _need_flag_val "$1" $#; player="$2"; shift 2 ;;
                --entry) _need_flag_val "$1" $#; entry="$2"; shift 2 ;;
                *) json_err BAD_ARG "Unknown flag: $1" ""; exit 1 ;;
              esac
            done
            _valid_charname "$player" || { json_err BAD_ARG "Invalid player name: $player" ""; exit 1; }
            if ! [[ "$entry" =~ ^[0-9]+$ ]] || (( 10#$entry < 1 || 10#$entry > 999999 )); then
              json_err BAD_ARG "Invalid creature entry: $entry" "Creature entry id, 1-999999."; exit 1
            fi
            entry=$(( 10#$entry ))
            # Existence + name lookup (read-only) BEFORE any SOAP fire, so a
            # bad custom entry fails with a clean message instead of an
            # in-game silent no-op.
            npcname="$(db_world_query "SELECT name FROM creature_template WHERE entry=$entry LIMIT 1;")" \
              || { json_err DB_UNREACHABLE "Could not check the creature entry" "Is ac-database running?"; exit 1; }
            [[ -n "$npcname" ]] || { json_err NOT_FOUND "No creature with entry $entry" "Check the id (creature_template.entry)."; exit 1; }
            _gm_require_online "$player"
            _party_fire "dml_summon_npc $player $entry" "summon"
            json_ok "{\"summoned\":true,\"player\":\"$(json_escape "$player")\",\"entry\":$entry,\"npc\":\"$(json_escape "$npcname")\"}"
            ;;
          at-login)
            player=""; flag=""
            while [[ $# -gt 0 ]]; do
              case "$1" in
                --player) _need_flag_val "$1" $#; player="$2"; shift 2 ;;
                --flag) _need_flag_val "$1" $#; flag="$2"; shift 2 ;;
                *) json_err BAD_ARG "Unknown flag: $1" ""; exit 1 ;;
              esac
            done
            _valid_charname "$player" || { json_err BAD_ARG "Invalid player name: $player" ""; exit 1; }
            case "$flag" in
              rename|customize|changerace|changefaction) : ;;
              *) json_err BAD_ARG "Invalid flag: $flag" "One of: rename customize changerace changefaction"; exit 1 ;;
            esac
            # Stock AC `character <flag>` commands: set a per-character flag
            # the client honors at that character's NEXT login. Works for
            # OFFLINE characters too (same family as `.character level`).
            if out="$(soap_exec "character $flag $player")"; then rc=0; else rc=$?; fi
            case "$rc" in
              0) json_ok "{\"applied\":true,\"player\":\"$(json_escape "$player")\",\"flag\":\"$flag\"}" ;;
              2) json_err SOAP_FAULT "$(_soap_text_decode "$out")" "The worldserver rejected the command." ; exit 1 ;;
              3) json_err SOAP_AUTH "SOAP auth failed" "Check ~/.dml/soap.env" ; exit 1 ;;
              *) json_err SOAP_UNREACHABLE "Could not reach the server" "Is it running?" ; exit 1 ;;
            esac
            ;;
          *)
            json_err UNKNOWN_COMMAND "Unknown gm subcommand: $gsub" "Try: dml wow gm level|gold|heal|revive|summon|at-login --json"
            exit 1
            ;;
        esac
        ;;
      backup)
        bsub="${1:-}"; shift || true
        case "$bsub" in
          create)
            incworld=0
            [[ "${1:-}" == "--include-world" ]] && { incworld=1; shift; }
            [[ "$DML_JSON" == 1 ]] && ndjson_section_start backup-create
            if ! docker info >/dev/null 2>&1; then
              if [[ "$DML_JSON" == 1 ]]; then
                ndjson_section_end backup-create error
                ndjson_error DOCKER_DOWN "Docker is not running" "Start Docker in the distro first."
              else echo "[dml] ERROR: docker down" >&2; fi
              exit 1
            fi
            bdir="$(_backup_dir)"; mkdir -p "$bdir"
            bsuffix=""
            [[ "$incworld" == 1 ]] && bsuffix="-full"
            bfile="wow-$(date -u +%Y%m%d-%H%M%S)$bsuffix.sql.gz"
            if [[ "$incworld" == 1 ]]; then
              [[ "$DML_JSON" == 1 ]] && ndjson_line info "backing up characters, bots, accounts and world..."
            else
              [[ "$DML_JSON" == 1 ]] && ndjson_line info "backing up characters, bots and accounts..."
            fi
            if ! _backup_dump_to "$bdir/$bfile" "$incworld"; then
              errtail="$(tail -c 160 "$bdir/$bfile.err" 2>/dev/null | tr -d '\r\n"\\')" || errtail=""
              rm -f "$bdir/$bfile.err"
              if [[ "$DML_JSON" == 1 ]]; then
                ndjson_section_end backup-create error
                ndjson_error BACKUP_FAILED "mysqldump failed" "$errtail"
              else echo "[dml] ERROR: mysqldump failed" >&2; fi
              exit 1
            fi
            bsize="$(stat -c %s "$bdir/$bfile" 2>/dev/null)" || bsize=0
            first=1; parr='['
            while IFS= read -r p || [[ -n "$p" ]]; do
              [[ -z "$p" ]] && continue
              [[ "$DML_JSON" == 1 ]] && ndjson_line info "pruned old backup: $p"
              [[ $first -eq 0 ]] && parr+=','
              parr+="\"$(json_escape "$p")\""; first=0
            done < <(_backup_prune)
            parr+=']'
            if [[ "$DML_JSON" == 1 ]]; then
              ndjson_section_end backup-create ok
              ndjson_done "{\"file\":\"$(json_escape "$bfile")\",\"size\":$bsize,\"world\":$([[ "$incworld" == 1 ]] && echo true || echo false),\"pruned\":$parr}"
            else echo "[dml] backup created: $bfile"; fi
            ;;
          list)
            bdir="$(_backup_dir)"
            first=1; out='['
            if [[ -d "$bdir" ]]; then
              while IFS= read -r f || [[ -n "$f" ]]; do
                [[ -z "$f" ]] && continue
                _valid_backup_name "$f" || continue
                fsize="$(stat -c %s "$bdir/$f" 2>/dev/null)" || fsize=0
                d="${f:4:8}"; t="${f:13:6}"
                created="${d:0:4}-${d:4:2}-${d:6:2} ${t:0:2}:${t:2:2}:${t:4:2}"
                bw=false
                [[ "$f" == *-full.sql.gz || "$f" == *-full-prerestore.sql.gz ]] && bw=true
                [[ $first -eq 0 ]] && out+=','
                out+="{\"file\":\"$(json_escape "$f")\",\"size\":$fsize,\"created\":\"$created\",\"world\":$bw}"
                first=0
              done < <(ls -1 "$bdir" 2>/dev/null | grep -E '\.sql\.gz$' | sort -r)
            fi
            out+=']'
            json_ok "{\"backups\":$out}"
            ;;
          delete)
            file=""
            [[ "${1:-}" == "--file" ]] && { _need_flag_val "$1" $#; file="$2"; shift 2; }
            _valid_backup_name "$file" || { json_err BAD_ARG "Invalid backup name: $file" ""; exit 1; }
            bdir="$(_backup_dir)"
            [[ -f "$bdir/$file" ]] || { json_err NOT_FOUND "No backup named $file" ""; exit 1; }
            rm -f "$bdir/$file"
            json_ok "{\"deleted\":true,\"file\":\"$(json_escape "$file")\"}"
            ;;
          restore)
            file=""
            while [[ $# -gt 0 ]]; do
              case "$1" in
                --file) _need_flag_val "$1" $#; file="$2"; shift 2 ;;
                *) json_err BAD_ARG "Unknown flag: $1" ""; exit 1 ;;
              esac
            done
            _valid_backup_name "$file" || { json_err BAD_ARG "Invalid backup name: $file" ""; exit 1; }
            [[ "$DML_JSON" == 1 ]] && ndjson_section_start backup-restore
            bdir="$(_backup_dir)"
            if [[ ! -f "$bdir/$file" ]]; then
              if [[ "$DML_JSON" == 1 ]]; then
                ndjson_section_end backup-restore error
                ndjson_error NOT_FOUND "No backup named $file" ""
              else echo "[dml] ERROR: no backup $file" >&2; fi
              exit 1
            fi
            sdir="$(_wow_server_dir)"
            if [[ -z "$sdir" ]]; then
              if [[ "$DML_JSON" == 1 ]]; then
                ndjson_section_end backup-restore error
                ndjson_error NOT_FOUND "WoW Playerbots server not installed" "Install it first."
              else echo "[dml] ERROR: wow server not installed" >&2; fi
              exit 1
            fi
            [[ "$DML_JSON" == 1 ]] && ndjson_line info "stopping the game server..."
            # Flush characters before the stop so the pre-restore safety
            # dump contains everyone's latest state (best-effort), and give
            # the world time to finish its own shutdown save.
            soap_exec 'saveall' >/dev/null 2>&1 || true
            if ! (cd "$sdir" && docker compose stop -t 180 ac-worldserver ac-authserver >/dev/null 2>&1); then
              if [[ "$DML_JSON" == 1 ]]; then
                ndjson_section_end backup-restore error
                ndjson_error BACKUP_FAILED "Could not stop the server" "Nothing was changed."
              else echo "[dml] ERROR: could not stop server" >&2; fi
              exit 1
            fi
            [[ "$DML_JSON" == 1 ]] && ndjson_line info "taking a pre-restore safety backup..."
            rincw=0; rsuffix=""
            if [[ "$file" == *-full.sql.gz || "$file" == *-full-prerestore.sql.gz ]]; then
              rincw=1; rsuffix="-full"
            fi
            safety="wow-$(date -u +%Y%m%d-%H%M%S)$rsuffix-prerestore.sql.gz"
            if ! _backup_dump_to "$bdir/$safety" "$rincw"; then
              rm -f "$bdir/$safety.err"
              (cd "$sdir" && docker compose start ac-worldserver ac-authserver >/dev/null 2>&1) || true
              if [[ "$DML_JSON" == 1 ]]; then
                ndjson_section_end backup-restore error
                ndjson_error BACKUP_FAILED "Safety backup failed -- nothing was restored" "The server was started again."
              else echo "[dml] ERROR: safety backup failed" >&2; fi
              exit 1
            fi
            [[ "$DML_JSON" == 1 ]] && ndjson_line info "restoring $file..."
            if ! gunzip -c "$bdir/$file" | docker exec -i ac-database mysql -uroot -p"$(_db_pw)"; then
              if [[ "$DML_JSON" == 1 ]]; then
                ndjson_section_end backup-restore error
                ndjson_error BACKUP_FAILED "Import failed -- the server was LEFT STOPPED" "Your pre-restore state is saved as $safety. Restore it, or start the server manually once resolved."
              else echo "[dml] ERROR: import failed; server left stopped" >&2; fi
              exit 1
            fi
            [[ "$DML_JSON" == 1 ]] && ndjson_line info "starting the game server..."
            if (cd "$sdir" && docker compose start ac-worldserver ac-authserver >/dev/null 2>&1); then :; else
              [[ "$DML_JSON" == 1 ]] && ndjson_line warn "server start failed -- start it from Home"
            fi
            if [[ "$DML_JSON" == 1 ]]; then
              ndjson_section_end backup-restore ok
              ndjson_done "{\"restored\":true,\"file\":\"$(json_escape "$file")\",\"safety_backup\":\"$(json_escape "$safety")\"}"
            else echo "[dml] restored $file (safety: $safety)"; fi
            ;;
          *)
            json_err UNKNOWN_COMMAND "Unknown backup subcommand: $bsub" "Try: dml wow backup create|list|delete|restore --json"
            exit 1
            ;;
        esac
        ;;
      bots)
        btsub="${1:-}"; shift || true
        case "$btsub" in
          flush)
            # Flush & rebuild the ambient bot population (Batch 1 F4).
            # Streaming NDJSON like games restart: (1) character backup
            # (backup-create internals, NOT a subprocess), (2) arm
            # AiPlayerbot.DeleteRandomBotAccounts = 1 + an EXIT trap that
            # ALWAYS puts it back to 0, (3) staged restart -- deletion runs
            # during boot, (4/5) restore the flag, (6) second restart to
            # rebuild the population from the current settings, (7) done.
            # Destructive: requires BOTH --yes and the typed ack --ack flush.
            btconfirm=0; btack=""
            while [[ $# -gt 0 ]]; do
              case "$1" in
                --yes) btconfirm=1; shift ;;
                --ack) _need_flag_val "$1" $#; btack="$2"; shift 2 ;;
                *) json_err BAD_ARG "Unknown flag: $1" ""; exit 1 ;;
              esac
            done
            [[ "$DML_JSON" == 1 ]] && ndjson_section_start bots-flush
            if [[ "$btconfirm" != 1 || "$btack" != "flush" ]]; then
              if [[ "$DML_JSON" == 1 ]]; then
                ndjson_section_end bots-flush error
                ndjson_error CONFIRM_REQUIRED "Flushing deletes ALL random bots' characters, auctions and mail, then rebuilds them from your settings" "Re-run with --yes --ack flush. Your own characters are untouched."
              else echo "[dml] ERROR: re-run with --yes --ack flush" >&2; fi
              exit 1
            fi
            if ! docker info >/dev/null 2>&1; then
              if [[ "$DML_JSON" == 1 ]]; then
                ndjson_section_end bots-flush error
                ndjson_error DOCKER_DOWN "Docker is not running" "Start Docker in the distro first."
              else echo "[dml] ERROR: docker down" >&2; fi
              exit 1
            fi
            sdir="$(_wow_server_dir)"
            if [[ -z "$sdir" ]]; then
              if [[ "$DML_JSON" == 1 ]]; then
                ndjson_section_end bots-flush error
                ndjson_error NOT_FOUND "WoW Playerbots server not installed" "Install it first."
              else echo "[dml] ERROR: wow server not installed" >&2; fi
              exit 1
            fi
            pbflush="$sdir/env/dist/etc/modules/playerbots.conf"
            if ! _cfg_conf_ensure "$pbflush"; then
              if [[ "$DML_JSON" == 1 ]]; then
                ndjson_section_end bots-flush error
                ndjson_error NOT_FOUND "playerbots.conf not found (nor its .dist)" "Is the WoW server fully installed?"
              else echo "[dml] ERROR: playerbots.conf missing" >&2; fi
              exit 1
            fi
            flush_t0=$SECONDS
            # (1) safety backup FIRST -- a failed dump aborts before any
            # destructive step, nothing has changed yet.
            [[ "$DML_JSON" == 1 ]] && ndjson_line info "backing up characters, bots and accounts first..."
            bdir="$(_backup_dir)"; mkdir -p "$bdir"
            bfile="wow-$(date -u +%Y%m%d-%H%M%S).sql.gz"
            if ! _backup_dump_to "$bdir/$bfile" 0; then
              errtail="$(tail -c 160 "$bdir/$bfile.err" 2>/dev/null | tr -d '\r\n"\\')" || errtail=""
              rm -f "$bdir/$bfile.err"
              if [[ "$DML_JSON" == 1 ]]; then
                ndjson_section_end bots-flush error
                ndjson_error BACKUP_FAILED "The safety backup failed - nothing was changed" "$errtail"
              else echo "[dml] ERROR: safety backup failed" >&2; fi
              exit 1
            fi
            [[ "$DML_JSON" == 1 ]] && ndjson_line info "backup created: $bfile"
            while IFS= read -r p || [[ -n "$p" ]]; do
              [[ -z "$p" ]] && continue
              [[ "$DML_JSON" == 1 ]] && ndjson_line info "pruned old backup: $p"
            done < <(_backup_prune)
            # (2) arm the delete flag; from here on the trap guarantees the
            # flag is restored no matter how this arm dies.
            CFG_CHANGED=false
            FLUSH_RESTORE_CONF="$pbflush"
            trap '_flush_restore_flag' EXIT
            if ! _cfg_conf_write "$pbflush" "AiPlayerbot.DeleteRandomBotAccounts" "1"; then
              if [[ "$DML_JSON" == 1 ]]; then
                ndjson_section_end bots-flush error
                ndjson_error WRITE_FAILED "Could not write playerbots.conf" ""
              else echo "[dml] ERROR: conf write failed" >&2; fi
              exit 1
            fi
            [[ "$DML_JSON" == 1 ]] && ndjson_line info "delete flag armed - restarting so the server wipes the random bots..."
            # (3) restart #1: the wipe happens during this boot
            if _flush_restart_authworld "$sdir" "bot deletion"; then frc=0; else frc=$?; fi
            if [[ "$frc" -ne 0 ]]; then
              if [[ "$DML_JSON" == 1 ]]; then
                ndjson_section_end bots-flush error
                if [[ "$frc" -eq 2 ]]; then
                  ndjson_error TIMEOUT "Timed out waiting for the world during bot deletion" "The delete flag was restored to 0. Check the server from Home, then try again."
                else
                  ndjson_error RESTART_FAILED "Could not restart the server for bot deletion" "The delete flag was restored to 0. Check the server from Home."
                fi
              else echo "[dml] ERROR: restart failed (flag restored)" >&2; fi
              exit 1
            fi
            # (4)+(5) bots are gone -- put the flag back BEFORE the rebuild
            # restart, or the next boot would wipe them again.
            [[ "$DML_JSON" == 1 ]] && ndjson_line info "bots deleted - restoring the setting..."
            if ! _cfg_conf_write "$pbflush" "AiPlayerbot.DeleteRandomBotAccounts" "0"; then
              if [[ "$DML_JSON" == 1 ]]; then
                ndjson_section_end bots-flush error
                ndjson_error WRITE_FAILED "Could not restore playerbots.conf - fix AiPlayerbot.DeleteRandomBotAccounts back to 0 by hand before the next restart" ""
              else echo "[dml] ERROR: conf restore failed" >&2; fi
              exit 1
            fi
            FLUSH_RESTORE_CONF=""
            # (6) restart #2: the server recreates the population from the
            # current Bot World settings during this boot
            [[ "$DML_JSON" == 1 ]] && ndjson_line info "restarting again to rebuild the bot population (this is the long part)..."
            if _flush_restart_authworld "$sdir" "bot rebuild"; then frc=0; else frc=$?; fi
            if [[ "$frc" -ne 0 ]]; then
              if [[ "$DML_JSON" == 1 ]]; then
                ndjson_section_end bots-flush error
                if [[ "$frc" -eq 2 ]]; then
                  ndjson_error TIMEOUT "Timed out waiting for the world during the rebuild" "The bots may still be logging in - check Home before retrying."
                else
                  ndjson_error RESTART_FAILED "Could not restart the server for the rebuild" "Start it from Home - the delete flag is already back at 0."
                fi
              else echo "[dml] ERROR: rebuild restart failed" >&2; fi
              exit 1
            fi
            # (7) done
            flush_elapsed=$(( SECONDS - flush_t0 ))
            if [[ "$DML_JSON" == 1 ]]; then
              ndjson_section_end bots-flush ok
              ndjson_done "{\"flushed\":true,\"backup\":\"$(json_escape "$bfile")\",\"elapsed_secs\":$flush_elapsed}"
            else
              echo "[dml] bot population flushed and rebuilt (backup: $bfile, ${flush_elapsed}s)"
            fi
            ;;
          *)
            json_err UNKNOWN_COMMAND "Unknown bots subcommand: $btsub" "Try: dml wow bots flush --yes --ack flush --json"
            exit 1
            ;;
        esac
        ;;
      module)
        msub="${1:-}"; shift || true
        case "$msub" in
          list)
            sdir="$(_wow_server_dir)"
            if [[ -z "$sdir" ]]; then
              json_err NOT_FOUND "WoW Playerbots server not installed" "Install it first."; exit 1
            fi
            # Web-page URL from a registry clone URL as a JSON value: strip a
            # trailing .git; empty -> null.
            _mod_weburl_json() {
              local u="${1%.git}"
              if [[ -n "$u" ]]; then printf '"%s"' "$(json_escape "$u")"; else printf 'null'; fi
              return 0
            }
            cpp='['; first=1
            declare -A _mod_seen=()
            while IFS='|' read -r mk mname murl msql; do
              [[ -z "$mk" ]] && continue
              _mod_seen["$mk"]=1
              inst=false; _cpp_installed "$sdir" "$mk" && inst=true
              pend=false; _rebuild_pending_has "$sdir" "$mk" && pend=true
              cstate="$(_module_conf_state "$sdir" "$mk")"
              [[ $first -eq 0 ]] && cpp+=','
              cpp+="{\"key\":\"$mk\",\"name\":\"$(json_escape "$mname")\",\"desc\":\"$(json_escape "$(_module_desc "$mk")")\",\"url\":$(_mod_weburl_json "$murl"),\"installed\":$inst,\"pending_rebuild\":$pend,\"conf\":\"$cstate\",\"custom\":false}"
              first=0
            done < <(_module_registry_cpp)
            if [[ -d "$sdir/modules" ]]; then
              for d in "$sdir/modules"/*/; do
                [[ -d "$d/.git" ]] || continue
                mk="$(basename "$d")"
                [[ -n "${_mod_seen[$mk]:-}" ]] && continue
                _valid_cpp_key "$mk" || continue
                pend=false; _rebuild_pending_has "$sdir" "$mk" && pend=true
                # Custom clones carry no registry row -- their origin remote
                # is the best available "project page" link.
                curl_origin="$(git -C "$d" remote get-url origin 2>/dev/null || true)"
                cpp+=",{\"key\":\"$mk\",\"name\":\"$(json_escape "$mk")\",\"desc\":\"Custom module (cloned from a URL you provided).\",\"url\":$(_mod_weburl_json "$curl_origin"),\"installed\":true,\"pending_rebuild\":$pend,\"conf\":\"none\",\"custom\":true}"
              done
            fi
            cpp+=']'
            lua='['; first=1
            while IFS='|' read -r mk mname murl; do
              [[ -z "$mk" ]] && continue
              cl=false; _lua_cloned "$sdir" "$mk" && cl=true
              dep=false; _lua_deployed "$sdir" "$mk" && dep=true
              lsql=false; _lua_has_sql "$mk" && lsql=true
              [[ $first -eq 0 ]] && lua+=','
              lua+="{\"key\":\"$mk\",\"name\":\"$(json_escape "$mname")\",\"desc\":\"$(json_escape "$(_module_desc "$mk")")\",\"url\":$(_mod_weburl_json "$murl"),\"cloned\":$cl,\"deployed\":$dep,\"has_sql\":$lsql}"
              first=0
            done < <(_module_registry_lua)
            lua+=']'
            sqlj='['; first=1
            while IFS='|' read -r mk mname murl mtype; do
              [[ -z "$mk" ]] && continue
              inst=false; _sql_installed "$sdir" "$mk" && inst=true
              [[ $first -eq 0 ]] && sqlj+=','
              sqlj+="{\"key\":\"$mk\",\"name\":\"$(json_escape "$mname")\",\"desc\":\"$(json_escape "$(_module_desc "$mk")")\",\"url\":$(_mod_weburl_json "$murl"),\"type\":\"$mtype\",\"installed\":$inst}"
              first=0
            done < <(_module_registry_sql)
            sqlj+=']'
            aleready=false; _cpp_installed "$sdir" mod-ale && aleready=true
            json_ok "{\"families\":{\"cpp\":$cpp,\"lua\":$lua,\"sql\":$sqlj},\"rebuild_pending\":$(_rebuild_pending_json "$sdir"),\"ale_ready\":$aleready}"
            ;;
          install)
            [[ "$DML_JSON" == 1 ]] && ndjson_section_start module-install
            family=""; mkey=""; murl=""; bkchoice=""; mvariant=""
            while [[ $# -gt 0 ]]; do
              case "$1" in
                --family) _need_flag_val "$1" $#; family="$2"; shift 2 ;;
                --key) _need_flag_val "$1" $#; mkey="$2"; shift 2 ;;
                --url) _need_flag_val "$1" $#; murl="$2"; shift 2 ;;
                --backup) bkchoice=backup; shift ;;
                --no-backup) bkchoice=nobackup; shift ;;
                --variant) _need_flag_val "$1" $#; mvariant="$2"; shift 2 ;;
                *) ndjson_section_end module-install error; ndjson_error BAD_ARG "Unknown flag: $1" ""; exit 1 ;;
              esac
            done
            sdir="$(_wow_server_dir)"
            if [[ -z "$sdir" ]]; then
              ndjson_section_end module-install error
              ndjson_error NOT_FOUND "WoW Playerbots server not installed" "Install it first."; exit 1
            fi
            case "$family" in
              cpp)
                if [[ -n "$bkchoice" ]]; then
                  ndjson_section_end module-install error
                  ndjson_error BAD_ARG "cpp installs don't take backup flags" "Module SQL lands at rebuild time -- the backup choice belongs to: dml wow module rebuild"; exit 1
                fi
                regrow=""
                if [[ -n "$murl" ]]; then
                  if [[ -n "$mkey" ]]; then
                    ndjson_section_end module-install error
                    ndjson_error BAD_ARG "--url and --key are mutually exclusive" "Custom modules derive their key from the URL."; exit 1
                  fi
                  if ! _valid_module_url "$murl"; then
                    ndjson_section_end module-install error
                    ndjson_error BAD_ARG "Invalid module URL" "https://... git URLs only"; exit 1
                  fi
                  mkey="$(_module_key_from_url "$murl")"
                  if [[ -z "$mkey" ]]; then
                    ndjson_section_end module-install error
                    ndjson_error BAD_ARG "Custom module repos must be named mod-*" "e.g. https://github.com/you/mod-my-thing.git"; exit 1
                  fi
                else
                  if ! _valid_cpp_key "$mkey"; then
                    ndjson_section_end module-install error
                    ndjson_error BAD_ARG "Invalid module key: $mkey" ""; exit 1
                  fi
                  regrow="$(_module_registry_cpp | grep -m1 -F "$mkey|" || true)"
                  [[ "$regrow" == "$mkey|"* ]] || regrow=""
                  if [[ -z "$regrow" ]]; then
                    ndjson_section_end module-install error
                    ndjson_error BAD_ARG "Unknown module: $mkey" "Pass --url for a custom module."; exit 1
                  fi
                  murl="$(printf '%s' "$regrow" | cut -d'|' -f3)"
                fi
                action=installed
                if _cpp_installed "$sdir" "$mkey"; then
                  action=updated
                  ndjson_line info "updating $mkey..."
                  if ! (cd "$sdir/modules/$mkey" && _stream_cmd git pull --depth 1); then
                    ndjson_section_end module-install error
                    ndjson_error GIT_FAILED "git pull failed for $mkey" ""; exit 1
                  fi
                else
                  mkdir -p "$sdir/modules"
                  [[ -d "$sdir/modules/$mkey" && ! -d "$sdir/modules/$mkey/.git" ]] && rm -rf "$sdir/modules/$mkey"
                  ndjson_line info "cloning $mkey..."
                  if ! _stream_cmd git clone --depth 1 "$murl" "$sdir/modules/$mkey"; then
                    ndjson_section_end module-install error
                    ndjson_error GIT_FAILED "git clone failed for $mkey" ""; exit 1
                  fi
                fi
                _rebuild_pending_add "$sdir" "$mkey"
                ndjson_line info "module SQL (if any) is applied automatically by the server's db-import on next start -- never by hand"
                ndjson_section_end module-install ok
                ndjson_done "{\"key\":\"$mkey\",\"action\":\"$action\",\"rebuild_required\":true}"
                ;;
              lua)
                lrow="$(_module_registry_lua | grep -m1 -F "$mkey|" || true)"
                if [[ "$lrow" != "$mkey|"* ]]; then
                  ndjson_section_end module-install error
                  ndjson_error BAD_ARG "Unknown lua script: $mkey" "Lua scripts come from the registry only."; exit 1
                fi
                if [[ -n "$murl" ]]; then
                  ndjson_section_end module-install error
                  ndjson_error BAD_ARG "--url is not supported for lua scripts" ""; exit 1
                fi
                if ! _cpp_installed "$sdir" mod-ale; then
                  ndjson_section_end module-install error
                  ndjson_error NOT_READY "Install the ALE module (mod-ale) first" "It's in the C++ modules list."; exit 1
                fi
                if _lua_has_sql "$mkey"; then
                  if [[ -z "$bkchoice" ]]; then
                    ndjson_section_end module-install error
                    ndjson_error BAD_ARG "Pick --backup or --no-backup" "This script applies SQL to the database."; exit 1
                  fi
                  if [[ "$bkchoice" == backup ]]; then
                    if ! _module_backup_now; then
                      ndjson_section_end module-install error
                      ndjson_error BACKUP_FAILED "Safety backup failed — nothing was installed" ""; exit 1
                    fi
                  fi
                else
                  if [[ -n "$bkchoice" ]]; then
                    ndjson_section_end module-install error
                    ndjson_error BAD_ARG "$mkey applies no SQL — backup flags don't apply" ""; exit 1
                  fi
                fi
                lurl="$(printf '%s' "$lrow" | cut -d'|' -f3)"
                if ! _lua_clone "$sdir" "$mkey" "$lurl"; then
                  ndjson_section_end module-install error
                  ndjson_error GIT_FAILED "git failed for $mkey" ""; exit 1
                fi
                if ! _lua_deploy "$sdir" "$mkey"; then
                  ndjson_section_end module-install error
                  ndjson_error DEPLOY_FAILED "Could not deploy $mkey lua files" "The clone's layout was not what the installer expects."; exit 1
                fi
                if _lua_has_sql "$mkey"; then
                  if ! _lua_apply_sql "$sdir" "$mkey"; then
                    ndjson_section_end module-install error
                    ndjson_error SQL_FAILED "SQL for $mkey failed" "Is ac-database running? The lua files are deployed; re-run install once the DB is up."; exit 1
                  fi
                fi
                _lua_client_copy "$sdir" "$mkey"
                relmsg=".reload ale (in-game or Console page)"
                [[ "$mkey" == bmah ]] && relmsg="restart the server (new creature_template rows don't hot-load)"
                ndjson_line info "done — $relmsg"
                ndjson_section_end module-install ok
                ndjson_done "{\"key\":\"$mkey\",\"action\":\"installed\",\"reload\":\"$(json_escape "$relmsg")\"}"
                ;;
              sql)
                srow="$(_module_registry_sql | grep -m1 -F "$mkey|" || true)"
                if [[ "$srow" != "$mkey|"* ]]; then
                  ndjson_section_end module-install error
                  ndjson_error BAD_ARG "Unknown SQL mod: $mkey" ""; exit 1
                fi
                if [[ -n "$murl" ]]; then
                  ndjson_section_end module-install error
                  ndjson_error BAD_ARG "--url is not supported for SQL mods" ""; exit 1
                fi
                stype="$(printf '%s' "$srow" | cut -d'|' -f4)"
                surl="$(printf '%s' "$srow" | cut -d'|' -f3)"
                _sqlmod_dirs "$sdir"
                if [[ -f "$(_sqlmod_marker "$sdir" "$mkey")" ]]; then
                  ndjson_section_end module-install error
                  ndjson_error EXISTS "$mkey is already installed" "Remove it first to re-apply."; exit 1
                fi
                if [[ -z "$bkchoice" ]]; then
                  ndjson_section_end module-install error
                  ndjson_error BAD_ARG "Pick --backup or --no-backup" "SQL mods change the world database."; exit 1
                fi
                if [[ "$stype" == clone_sql_pick ]]; then
                  case "$mvariant" in
                    1sec|1min|5min|15min|30min) ;;
                    *) ndjson_section_end module-install error
                       ndjson_error BAD_ARG "hearthstone-cd needs --variant 1sec|1min|5min|15min|30min" ""; exit 1 ;;
                  esac
                fi
                if [[ "$stype" == clone_dist ]]; then
                  [[ -z "$mvariant" ]] && mvariant=80
                  if ! [[ "$mvariant" =~ ^[0-9]+$ ]] || (( 10#$mvariant < 1 || 10#$mvariant > 80 )); then
                    ndjson_section_end module-install error
                    ndjson_error BAD_ARG "npc-teleporter --variant is a level 1-80" ""; exit 1
                  fi
                  mvariant=$((10#$mvariant))
                fi
                if [[ "$bkchoice" == backup ]]; then
                  if ! _module_backup_now; then
                    ndjson_section_end module-install error
                    ndjson_error BACKUP_FAILED "Safety backup failed — nothing was installed" ""; exit 1
                  fi
                fi
                scdir="$sdir/sql_scripts/clones/$mkey"
                if [[ -n "$surl" ]]; then
                  if [[ ! -d "$scdir/.git" ]]; then
                    [[ -d "$scdir" ]] && rm -rf "$scdir"
                    ndjson_line info "cloning $mkey..."
                    if ! _stream_cmd git clone --depth 1 "$surl" "$scdir"; then
                      ndjson_section_end module-install error
                      ndjson_error GIT_FAILED "git clone failed for $mkey" ""; exit 1
                    fi
                  fi
                fi
                sqlfail=0; sibnote=""
                case "$stype" in
                  clone_sql|clone_sql_norevert)
                    ucount=0
                    while IFS= read -r sf || [[ -n "$sf" ]]; do
                      [[ -z "$sf" ]] && continue
                      ndjson_line info "applying $(basename "$sf")..."
                      _sqlmod_run_file acore_world "$sf" || { sqlfail=1; break; }
                      ucount=$(( ucount + 1 ))
                    done < <(_sqlmod_up_files "$scdir")
                    [[ "$ucount" -eq 0 ]] && sqlfail=1
                    ;;
                  clone_sql_pick)
                    pf="$(find "$scdir" -iname "*${mvariant}*.sql" 2>/dev/null | grep -viE "[0-9]${mvariant}" | sort | head -n1)" || pf=""
                    if [[ -z "$pf" ]]; then sqlfail=1; else
                      ndjson_line info "applying $(basename "$pf")..."
                      _sqlmod_run_file acore_world "$pf" || sqlfail=1
                    fi
                    ;;
                  clone_dist)
                    gn=0
                    while IFS= read -r df || [[ -n "$df" ]]; do
                      [[ -z "$df" ]] && continue
                      gn=$(( gn + 1 ))
                      gf="$sdir/sql_scripts/clones/${mkey}_gen_$gn.sql"
                      sed "s/@ONY_LEVEL := [0-9]*/@ONY_LEVEL := $mvariant/" "$df" > "$gf" || { sqlfail=1; break; }
                      ndjson_line info "applying $(basename "$df") (level $mvariant)..."
                      _sqlmod_run_file acore_world "$gf" || { sqlfail=1; break; }
                    done < <(find "$scdir/data/sql/db-world" -name '*.dist' 2>/dev/null | sort)
                    [[ "$gn" -eq 0 ]] && sqlfail=1
                    ;;
                  tweak_world)
                    sib="$(_tweak_installed_sibling "$sdir" "$mkey")"
                    if [[ -n "$sib" ]]; then
                      ndjson_line info "removing active tweak $sib first (tweaks don't stack)..."
                      if ! _tweak_reverse "$sdir" "$sib"; then sqlfail=1; else rm -f "$(_sqlmod_marker "$sdir" "$sib")"; sibnote=" (note: the previous tweak $sib was already removed)"; fi
                    fi
                    if [[ "$sqlfail" -eq 0 ]]; then
                      read -r th td ta <<< "$(_tweak_mults "$mkey")"
                      ndjson_line info "applying $mkey (HP x$th / DMG x$td / ARM x$ta)..."
                      _tweak_apply "$th" "$td" "$ta" || sqlfail=1
                    fi
                    ;;
                esac
                if [[ "$sqlfail" -ne 0 ]]; then
                  ndjson_section_end module-install error
                  ndjson_error SQL_FAILED "SQL for $mkey failed" "Is ac-database running? Nothing was marked installed.$sibnote"; exit 1
                fi
                case "$stype" in
                  clone_sql_pick) printf 'HEARTHSTONE_COOLDOWN=%s\n' "$mvariant" > "$(_sqlmod_marker "$sdir" "$mkey")" ;;
                  tweak_world)
                    read -r th td ta <<< "$(_tweak_mults "$mkey")"
                    printf 'APPLIED_HP_MULT=%s\nAPPLIED_DMG_MULT=%s\nAPPLIED_ARM_MULT=%s\n' "$th" "$td" "$ta" > "$(_sqlmod_marker "$sdir" "$mkey")" ;;
                  *) : > "$(_sqlmod_marker "$sdir" "$mkey")" ;;
                esac
                ndjson_section_end module-install ok
                ndjson_done "{\"key\":\"$mkey\",\"action\":\"installed\",\"type\":\"$stype\"}"
                ;;
              *)
                ndjson_section_end module-install error
                ndjson_error BAD_ARG "Unknown family: $family" "cpp, lua or sql"; exit 1
                ;;
            esac
            ;;
          remove)
            [[ "$DML_JSON" == 1 ]] && ndjson_section_start module-remove
            family=""; mkey=""; bkchoice=""
            while [[ $# -gt 0 ]]; do
              case "$1" in
                --family) _need_flag_val "$1" $#; family="$2"; shift 2 ;;
                --key) _need_flag_val "$1" $#; mkey="$2"; shift 2 ;;
                --backup) bkchoice=backup; shift ;;
                --no-backup) bkchoice=nobackup; shift ;;
                *) ndjson_section_end module-remove error; ndjson_error BAD_ARG "Unknown flag: $1" ""; exit 1 ;;
              esac
            done
            sdir="$(_wow_server_dir)"
            if [[ -z "$sdir" ]]; then
              ndjson_section_end module-remove error
              ndjson_error NOT_FOUND "WoW Playerbots server not installed" ""; exit 1
            fi
            case "$family" in
              cpp)
                if [[ -n "$bkchoice" ]]; then
                  ndjson_section_end module-remove error
                  ndjson_error BAD_ARG "cpp removals don't take backup flags" "The compiled code leaves at rebuild time."; exit 1
                fi
                if ! _valid_cpp_key "$mkey" || ! _cpp_installed "$sdir" "$mkey"; then
                  ndjson_section_end module-remove error
                  ndjson_error NOT_FOUND "Module not installed: $mkey" ""; exit 1
                fi
                if [[ "$mkey" == mod-arac ]]; then
                  ndjson_line warn "mod-arac is data-only. Removing the clone does NOT revert:"
                  ndjson_line warn "  - arac.sql data already imported into acore_world"
                  ndjson_line warn "  - DBC files already copied to the server data volume"
                  ndjson_line warn "  - Patch-A.MPQ already installed in your WoW client Data/"
                fi
                rm -rf "$sdir/modules/$mkey"
                _rebuild_pending_add "$sdir" "$mkey"
                ndjson_line info "database rows from this module are kept -- removing them risks data loss"
                ndjson_section_end module-remove ok
                ndjson_done "{\"key\":\"$mkey\",\"removed\":true,\"rebuild_required\":true}"
                ;;
              lua)
                if [[ -n "$bkchoice" ]]; then
                  ndjson_section_end module-remove error
                  ndjson_error BAD_ARG "lua removal never touches the database — backup flags don't apply" "Tables the script created are kept."; exit 1
                fi
                if ! _lua_cloned "$sdir" "$mkey" && ! _lua_deployed "$sdir" "$mkey"; then
                  ndjson_section_end module-remove error
                  ndjson_error NOT_FOUND "Lua script not installed: $mkey" ""; exit 1
                fi
                rm -rf "$sdir/ale_scripts/$mkey"
                _lua_remove_deployed "$sdir" "$mkey"
                ndjson_line info "database tables created by this script are kept — removing them risks data loss"
                ndjson_line info "client-side files (if any) are kept"
                ndjson_section_end module-remove ok
                ndjson_done "{\"key\":\"$mkey\",\"removed\":true}"
                ;;
              sql)
                srow="$(_module_registry_sql | grep -m1 -F "$mkey|" || true)"
                if [[ "$srow" != "$mkey|"* ]]; then
                  ndjson_section_end module-remove error
                  ndjson_error BAD_ARG "Unknown SQL mod: $mkey" ""; exit 1
                fi
                stype="$(printf '%s' "$srow" | cut -d'|' -f4)"
                _sqlmod_dirs "$sdir"
                if [[ ! -f "$(_sqlmod_marker "$sdir" "$mkey")" ]]; then
                  ndjson_section_end module-remove error
                  ndjson_error NOT_FOUND "SQL mod not installed: $mkey" ""; exit 1
                fi
                if [[ "$stype" == clone_sql_norevert ]]; then
                  ndjson_section_end module-remove error
                  ndjson_error NO_REVERT "$mkey has no automated reversal SQL" "Restore a backup from the Backups page instead."; exit 1
                fi
                if [[ -z "$bkchoice" ]]; then
                  ndjson_section_end module-remove error
                  ndjson_error BAD_ARG "Pick --backup or --no-backup" "Removal changes the world database."; exit 1
                fi
                if [[ "$bkchoice" == backup ]]; then
                  if ! _module_backup_now; then
                    ndjson_section_end module-remove error
                    ndjson_error BACKUP_FAILED "Safety backup failed — nothing was removed" ""; exit 1
                  fi
                fi
                scdir="$sdir/sql_scripts/clones/$mkey"
                sqlfail=0
                case "$stype" in
                  clone_sql)
                    dcount=0
                    while IFS= read -r sf || [[ -n "$sf" ]]; do
                      [[ -z "$sf" ]] && continue
                      dcount=$(( dcount + 1 ))
                      ndjson_line info "applying $(basename "$sf")..."
                      _sqlmod_run_file acore_world "$sf" || { sqlfail=1; break; }
                    done < <(_sqlmod_down_files "$scdir")
                    if [[ "$dcount" -eq 0 ]]; then
                      ndjson_section_end module-remove error
                      ndjson_error NO_REVERT "$mkey's clone has no down.sql" "Restore a backup instead."; exit 1
                    fi
                    ;;
                  clone_sql_pick)
                    ndjson_line info "resetting hearthstone cooldown to the 30-minute default..."
                    _sqlmod_run_stmt acore_world "UPDATE spell_dbc SET RecoveryTime = 1800000, CategoryRecoveryTime = 1800000 WHERE Id = 8690;" || sqlfail=1
                    ;;
                  clone_dist)
                    ndjson_line info "deleting teleporter NPCs..."
                    _sqlmod_run_stmt acore_world "DELETE FROM creature WHERE id1 IN (190000,190001); DELETE FROM creature_template WHERE entry IN (190000,190001);" || sqlfail=1
                    ;;
                  tweak_world)
                    ndjson_line info "reversing $mkey multipliers..."
                    _tweak_reverse "$sdir" "$mkey" || sqlfail=1
                    ;;
                esac
                if [[ "$sqlfail" -ne 0 ]]; then
                  ndjson_section_end module-remove error
                  ndjson_error SQL_FAILED "Reversal SQL for $mkey failed" "Is ac-database running? The installed marker was kept."; exit 1
                fi
                rm -f "$(_sqlmod_marker "$sdir" "$mkey")"
                rm -rf "$scdir"
                ndjson_section_end module-remove ok
                ndjson_done "{\"key\":\"$mkey\",\"removed\":true,\"type\":\"$stype\"}"
                ;;
              *)
                ndjson_section_end module-remove error
                ndjson_error BAD_ARG "Unknown family: $family" "cpp, lua or sql"; exit 1
                ;;
            esac
            ;;
          rebuild)
            [[ "$DML_JSON" == 1 ]] && ndjson_section_start module-rebuild
            bkchoice=""
            while [[ $# -gt 0 ]]; do
              case "$1" in
                --backup) bkchoice=backup; shift ;;
                --no-backup) bkchoice=nobackup; shift ;;
                *) ndjson_section_end module-rebuild error; ndjson_error BAD_ARG "Unknown flag: $1" ""; exit 1 ;;
              esac
            done
            if [[ -z "$bkchoice" ]]; then
              ndjson_section_end module-rebuild error
              ndjson_error BAD_ARG "Pick --backup or --no-backup" "Module SQL lands during the rebuild -- decide explicitly."; exit 1
            fi
            sdir="$(_wow_server_dir)"
            if [[ -z "$sdir" ]]; then
              ndjson_section_end module-rebuild error
              ndjson_error NOT_FOUND "WoW Playerbots server not installed" ""; exit 1
            fi
            if ! docker info >/dev/null 2>&1; then
              ndjson_section_end module-rebuild error
              ndjson_error DOCKER_DOWN "Docker is not running" "Start Docker in the distro first."; exit 1
            fi
            if [[ "$bkchoice" == backup ]]; then
              if ! _module_backup_now; then
                ndjson_section_end module-rebuild error
                ndjson_error BACKUP_FAILED "Safety backup failed -- rebuild not started" ""; exit 1
              fi
            fi
            ndjson_line info "stopping worldserver..."
            (cd "$sdir" && docker compose stop -t 180 ac-worldserver >/dev/null 2>&1) || true
            ndjson_line info "building (this can take 30-90 minutes; full log: $sdir/rebuild.log)..."
            rc=0
            (cd "$sdir" && docker compose up -d --build 2>&1 | tee rebuild.log | while IFS= read -r _l; do ndjson_line info "$_l"; done; exit "${PIPESTATUS[0]}") || rc=$?
            if [[ "$rc" -ne 0 ]]; then
              ndjson_section_end module-rebuild error
              ndjson_error BUILD_FAILED "worldserver rebuild failed" "Full log: $sdir/rebuild.log"; exit 1
            fi
            _rebuild_pending_clear "$sdir"
            ndjson_section_end module-rebuild ok
            ndjson_done '{"rebuilt":true}'
            ;;
          conf)
            mkey=""
            while [[ $# -gt 0 ]]; do
              case "$1" in
                --key) _need_flag_val "$1" $#; mkey="$2"; shift 2 ;;
                *) json_err BAD_ARG "Unknown flag: $1" ""; exit 1 ;;
              esac
            done
            _valid_cpp_key "$mkey" || { json_err BAD_ARG "Invalid module key" ""; exit 1; }
            sdir="$(_wow_server_dir)"
            [[ -z "$sdir" ]] && { json_err NOT_FOUND "WoW Playerbots server not installed" ""; exit 1; }
            cname="$(_module_conf_name "$mkey")"
            cstate="$(_module_conf_state "$sdir" "$mkey")"
            cjson=null
            [[ -n "$cname" ]] && cjson="\"$(json_escape "$cname")\""
            json_ok "{\"key\":\"$mkey\",\"conf_name\":$cjson,\"state\":\"$cstate\"}"
            ;;
          conf-activate)
            mkey=""; force=0
            while [[ $# -gt 0 ]]; do
              case "$1" in
                --key) _need_flag_val "$1" $#; mkey="$2"; shift 2 ;;
                --force) force=1; shift ;;
                *) json_err BAD_ARG "Unknown flag: $1" ""; exit 1 ;;
              esac
            done
            _valid_cpp_key "$mkey" || { json_err BAD_ARG "Invalid module key" ""; exit 1; }
            sdir="$(_wow_server_dir)"
            [[ -z "$sdir" ]] && { json_err NOT_FOUND "WoW Playerbots server not installed" ""; exit 1; }
            cname="$(_module_conf_name "$mkey")"
            [[ -z "$cname" ]] && { json_err NO_CONF "$mkey has no standard conf file" ""; exit 1; }
            active="$sdir/env/dist/etc/modules/$cname"
            if [[ -f "$active" && "$force" != 1 ]]; then
              json_err EXISTS "Active conf already exists: $cname" "Pass --force to overwrite with defaults."; exit 1
            fi
            dist="$(_module_conf_dist "$sdir" "$mkey")"
            if [[ -z "$dist" ]]; then
              json_err NEEDS_REBUILD "No $cname.dist yet" "The .dist appears after a worldserver rebuild with the module present."; exit 1
            fi
            mkdir -p "$(dirname "$active")"
            cp "$dist" "$active"
            json_ok "{\"key\":\"$mkey\",\"activated\":true,\"conf_name\":\"$(json_escape "$cname")\"}"
            ;;
          tracking)
            # Read-only diagnosis (Round J) -- see _module_discover_sql_files
            # / _module_db_read in 70-modules.sh and the design doc
            # (docs/superpowers/specs/2026-07-18-module-repair-design.md).
            mkey=""
            while [[ $# -gt 0 ]]; do
              case "$1" in
                --key) _need_flag_val "$1" $#; mkey="$2"; shift 2 ;;
                *) json_err BAD_ARG "Unknown flag: $1" ""; exit 1 ;;
              esac
            done
            _valid_cpp_key "$mkey" || { json_err BAD_ARG "Invalid module key: $mkey" ""; exit 1; }
            sdir="$(_wow_server_dir)"
            [[ -z "$sdir" ]] && { json_err NOT_FOUND "WoW Playerbots server not installed" ""; exit 1; }
            _cpp_installed "$sdir" "$mkey" || { json_err NOT_FOUND "Module not installed: $mkey" "Install it first."; exit 1; }
            # Manager's exact matching (show_module_tracking): key minus the
            # mod- prefix, plus an underscored variant.
            stripped="${mkey#mod-}"
            term1="${stripped//-/_}"
            dbsj='{'; dbfirst=1
            for db_short in world characters auth; do
              rows="$(_module_db_read "$db_short" "SELECT name FROM updates WHERE name LIKE '%${stripped}%' OR name LIKE '%${term1}%';")" \
                || { json_err DB_UNREACHABLE "Could not reach the $db_short database" "Is ac-database running?"; exit 1; }
              trackedj='['; tfirst=1
              while IFS= read -r trow || [[ -n "$trow" ]]; do
                [[ -z "$trow" ]] && continue
                [[ $tfirst -eq 0 ]] && trackedj+=','
                trackedj+="\"$(json_escape "$trow")\""
                tfirst=0
              done <<< "$rows"
              trackedj+=']'
              # Per-file `tracked` is an EXACT-name lookup, independent of the
              # LIKE-based tracked_rows diagnosis above -- a file whose name
              # doesn't contain the key's LIKE terms (e.g. mod-ah-bot's
              # mod_auctionhousebot.sql) can still be genuinely tracked.
              filesj='['; ffirst=1
              discovered="$(_module_discover_sql_files "$sdir" "$mkey" "$db_short")"
              for f in $discovered; do
                [[ -z "$f" ]] && continue
                _valid_module_sql_filename "$f" || continue
                fcount="$(_module_db_read "$db_short" "SELECT COUNT(*) FROM updates WHERE name = '$f';")" || fcount=0
                istracked=false
                [[ "$fcount" =~ ^[0-9]+$ && "$fcount" -gt 0 ]] && istracked=true
                [[ $ffirst -eq 0 ]] && filesj+=','
                filesj+="{\"name\":\"$(json_escape "$f")\",\"tracked\":$istracked}"
                ffirst=0
              done
              filesj+=']'
              [[ $dbfirst -eq 0 ]] && dbsj+=','
              dbsj+="\"$db_short\":{\"tracked_rows\":$trackedj,\"files\":$filesj}"
              dbfirst=0
            done
            dbsj+='}'
            json_ok "{\"key\":\"$mkey\",\"dbs\":$dbsj}"
            ;;
          repair)
            # FOURTH sanctioned direct MySQL write (see 30-db.sh /
            # 60-backup.sh headers): INSERT/DELETE on the `updates` tracking
            # tables ONLY -- never game tables -- via the generalized
            # _db_write_stmt (30-db.sh). mark inserts a file's SHA1 so AC
            # skips it (fixes "Table X already exists"); clear deletes the
            # tracking row so AC re-applies the file (safe only for
            # idempotent SQL). Every filename (given via --files or
            # discovered) is validated BEFORE any SQL/path use -- rejecting
            # one filename aborts the whole batch with no SQL run at all.
            mkey=""; rdb=""; rmode=""; rfiles=""
            while [[ $# -gt 0 ]]; do
              case "$1" in
                --key) _need_flag_val "$1" $#; mkey="$2"; shift 2 ;;
                --db) _need_flag_val "$1" $#; rdb="$2"; shift 2 ;;
                --mode) _need_flag_val "$1" $#; rmode="$2"; shift 2 ;;
                --files) _need_flag_val "$1" $#; rfiles="$2"; shift 2 ;;
                *) json_err BAD_ARG "Unknown flag: $1" ""; exit 1 ;;
              esac
            done
            _valid_cpp_key "$mkey" || { json_err BAD_ARG "Invalid module key: $mkey" ""; exit 1; }
            case "$rdb" in
              world|characters|auth) ;;
              *) json_err BAD_ARG "Invalid --db: $rdb" "Use world, characters, or auth."; exit 1 ;;
            esac
            case "$rmode" in
              mark|clear) ;;
              *) json_err BAD_ARG "Invalid --mode: $rmode" "Use mark or clear."; exit 1 ;;
            esac
            sdir="$(_wow_server_dir)"
            [[ -z "$sdir" ]] && { json_err NOT_FOUND "WoW Playerbots server not installed" ""; exit 1; }
            _cpp_installed "$sdir" "$mkey" || { json_err NOT_FOUND "Module not installed: $mkey" "Install it first."; exit 1; }
            db_full="acore_$rdb"
            if [[ -n "$rfiles" ]]; then
              rfilelist="$rfiles"
            else
              rfilelist="$(_module_discover_sql_files "$sdir" "$mkey" "$rdb")"
            fi
            for f in $rfilelist; do
              [[ -z "$f" ]] && continue
              _valid_module_sql_filename "$f" || { json_err BAD_ARG "Invalid filename: $f" "Filenames must match ^[A-Za-z0-9._-]+\\.sql\$ (no slashes)."; exit 1; }
            done
            resultsj='['; rfirst=1
            for f in $rfilelist; do
              [[ -z "$f" ]] && continue
              [[ $rfirst -eq 0 ]] && resultsj+=','
              if [[ "$rmode" == mark ]]; then
                sqlfile="$(find "$sdir/modules/$mkey" -name "$f" 2>/dev/null | head -1)"
                if [[ -z "$sqlfile" ]]; then
                  res=file_missing
                else
                  hash="$(sha1sum "$sqlfile" | awk '{print toupper($1)}')"
                  _db_write_stmt "$db_full" "INSERT INTO updates (name, hash, state, timestamp, speed) VALUES ('$f', '$hash', 'RELEASED', NOW(), 0) ON DUPLICATE KEY UPDATE hash='$hash', state='RELEASED';" >/dev/null \
                    || { json_err DB_UNREACHABLE "Could not write to $db_full.updates" "Is ac-database running?"; exit 1; }
                  res=marked
                fi
              else
                cnt="$(_module_db_read "$rdb" "SELECT COUNT(*) FROM updates WHERE name='$f';")" \
                  || { json_err DB_UNREACHABLE "Could not reach the $rdb database" "Is ac-database running?"; exit 1; }
                cnt="${cnt//[[:space:]]/}"
                if [[ -z "$cnt" || "$cnt" == "0" ]]; then
                  res=not_tracked
                else
                  _db_write_stmt "$db_full" "DELETE FROM updates WHERE name='$f';" >/dev/null \
                    || { json_err DB_UNREACHABLE "Could not write to $db_full.updates" "Is ac-database running?"; exit 1; }
                  res=cleared
                fi
              fi
              resultsj+="{\"file\":\"$(json_escape "$f")\",\"result\":\"$res\"}"
              rfirst=0
            done
            resultsj+=']'
            json_ok "{\"key\":\"$mkey\",\"db\":\"$rdb\",\"mode\":\"$rmode\",\"results\":$resultsj}"
            ;;
          *)
            json_err UNKNOWN_COMMAND "Unknown module subcommand: $msub" "Try: dml wow module list --json"
            exit 1
            ;;
        esac
        ;;
      client-path)
        cpsub="${1:-}"; shift || true
        case "$cpsub" in
          get)
            saved=""; f="$(_client_path_file)"
            [[ -r "$f" ]] && { saved="$(cat "$f" 2>/dev/null)" || saved=""; }
            if [[ -z "$saved" ]]; then
              json_ok '{"path":null,"valid":false}'
            else
              cvalid=false; [[ -d "$saved" ]] && _client_valid "$saved" && cvalid=true
              json_ok "{\"path\":\"$(json_escape "$saved")\",\"valid\":$cvalid}"
            fi
            ;;
          set)
            cpath=""
            while [[ $# -gt 0 ]]; do
              case "$1" in
                --path) _need_flag_val "$1" $#; cpath="$2"; shift 2 ;;
                *) json_err BAD_ARG "Unknown flag: $1" ""; exit 1 ;;
              esac
            done
            [[ -z "$cpath" ]] && { json_err BAD_ARG "client-path set requires --path" ""; exit 1; }
            cpath="$(_client_win_to_wsl "$cpath")"
            [[ -d "$cpath" ]] || { json_err BAD_PATH "No such folder: $cpath" "WSL sees Windows drives as /mnt/c/..."; exit 1; }
            _client_valid "$cpath" || { json_err NOT_CLIENT "That folder doesn't look like a WoW client" "Expected Wow.exe or an Interface folder inside it."; exit 1; }
            mkdir -p "$(dirname "$(_client_path_file)")"
            printf '%s\n' "$cpath" > "$(_client_path_file)"
            json_ok "{\"path\":\"$(json_escape "$cpath")\",\"valid\":true}"
            ;;
          detect)
            cands='['; first=1
            while IFS= read -r c || [[ -n "$c" ]]; do
              [[ -z "$c" ]] && continue
              [[ $first -eq 0 ]] && cands+=','
              cands+="\"$(json_escape "$c")\""; first=0
            done < <(_client_detect)
            cands+=']'
            json_ok "{\"candidates\":$cands}"
            ;;
          *)
            json_err UNKNOWN_COMMAND "Unknown client-path subcommand: $cpsub" "Try: dml wow client-path get|set|detect --json"; exit 1 ;;
        esac
        ;;
      update-check)
        # Read-only-ish: does a `git fetch --quiet origin` per repo to
        # compute behind-counts, but never mutates the worktree (no pull,
        # no stash). See docs/superpowers/specs/
        # 2026-07-18-server-update-design.md.
        sdir="$(_wow_server_dir)"
        if [[ -z "$sdir" ]]; then
          json_err NOT_FOUND "WoW Playerbots server not installed" "Install it first, then re-run."; exit 1
        fi
        if [[ ! -d "$sdir/.git" ]]; then
          json_err GIT_MISSING "$sdir is not a git checkout" "Can't check for updates."; exit 1
        fi
        repos="[$(_wow_repo_check_json "$sdir" AzerothCore)"
        moddir="$sdir/modules/mod-playerbots"
        notej=""
        if [[ -d "$moddir/.git" ]]; then
          repos+=",$(_wow_repo_check_json "$moddir" mod-playerbots)"
        else
          notej="mod-playerbots module is not installed -- nothing to check there"
        fi
        repos+="]"
        if [[ -n "$notej" ]]; then
          json_ok "{\"repos\":$repos,\"note\":\"$(json_escape "$notej")\"}"
        else
          json_ok "{\"repos\":$repos}"
        fi
        ;;
      update)
        # Ports the manager's update_server_source (wow-manage.sh:7018-7193)
        # FAIL-CLOSED -- see the _wow_pull_repo/_wow_remote_ok header
        # comments in 70-modules.sh for the full deviation rationale. Gates
        # (server dir / git checkout / remote / branch) all run BEFORE any
        # mutation, in that order, mirroring the design doc's numbered list.
        [[ "$DML_JSON" == 1 ]] && ndjson_section_start server-update
        bkchoice=""
        while [[ $# -gt 0 ]]; do
          case "$1" in
            --backup) bkchoice=backup; shift ;;
            --no-backup) bkchoice=nobackup; shift ;;
            *) ndjson_section_end server-update error; ndjson_error BAD_ARG "Unknown flag: $1" ""; exit 1 ;;
          esac
        done
        sdir="$(_wow_server_dir)"
        if [[ -z "$sdir" ]]; then
          ndjson_section_end server-update error
          ndjson_error NOT_FOUND "WoW Playerbots server not installed" "Install it first."; exit 1
        fi
        if [[ ! -d "$sdir/.git" ]]; then
          ndjson_section_end server-update error
          ndjson_error GIT_MISSING "$sdir is not a git checkout" "Can't update from source."; exit 1
        fi
        # AzerothCore must be the custom mod-playerbots fork on the
        # Playerbot branch -- pulling upstream azerothcore/azerothcore-wotlk
        # here would break the playerbots integration. No override: this is
        # a hard error, unlike the manager's interactive "pull anyway?".
        acurl="$(_wow_git_url "$sdir")"
        if ! _wow_remote_ok "$acurl" "mod-playerbots/azerothcore-wotlk"; then
          ndjson_section_end server-update error
          ndjson_error REMOTE_MISMATCH "AzerothCore origin is not the expected Playerbots fork" "found: ${acurl:-<none>} -- pulling upstream AzerothCore would break Playerbots. Fix the remote manually, then retry."
          exit 1
        fi
        moddir="$sdir/modules/mod-playerbots"
        if [[ -d "$moddir/.git" ]]; then
          pburl="$(_wow_git_url "$moddir")"
          if ! _wow_remote_ok "$pburl" "mod-playerbots/mod-playerbots"; then
            ndjson_section_end server-update error
            ndjson_error REMOTE_MISMATCH "mod-playerbots origin is not the expected fork" "found: ${pburl:-<none>}"
            exit 1
          fi
        fi
        acbranch="$(_wow_git_branch "$sdir")"
        if [[ "$acbranch" != "Playerbot" ]]; then
          ndjson_section_end server-update error
          ndjson_error BRANCH_MISMATCH "AzerothCore checkout is on branch '$acbranch' (expected 'Playerbot')" ""
          exit 1
        fi
        if [[ -z "$bkchoice" ]]; then
          ndjson_section_end server-update error
          ndjson_error BAD_ARG "Pick --backup or --no-backup" "New core revisions can run DB migrations at next start -- decide explicitly."; exit 1
        fi
        if [[ "$bkchoice" == backup ]]; then
          if ! _module_backup_now; then
            ndjson_section_end server-update error
            ndjson_error BACKUP_FAILED "Safety backup failed -- update not started" ""; exit 1
          fi
        fi
        changed=false
        if ! _wow_pull_repo "$sdir" AzerothCore; then
          ndjson_section_end server-update error
          exit 1
        fi
        [[ "$_WOW_PULL_CHANGED" == true ]] && changed=true
        ac_summary="$_WOW_PULL_SUMMARY"
        pb_summary="skipped"
        if [[ -d "$moddir/.git" ]]; then
          if ! _wow_pull_repo "$moddir" mod-playerbots; then
            ndjson_section_end server-update error
            exit 1
          fi
          [[ "$_WOW_PULL_CHANGED" == true ]] && changed=true
          pb_summary="$_WOW_PULL_SUMMARY"
        else
          ndjson_line warn "modules/mod-playerbots not found -- skipping module update."
        fi
        if [[ "$changed" == true ]]; then
          # Display-only marker in the rebuild-pending list -- it fails
          # _valid_cpp_key (no mod- prefix) so the cpp custom-scan in
          # `module list` never renders it as a module row; only the
          # rebuild banner picks it up, and `module rebuild` clears it like
          # any other pending entry.
          _rebuild_pending_add "$sdir" core-update
          ndjson_line info "Rebuild required to compile the update -- use the rebuild banner on this page."
        fi
        ndjson_section_end server-update ok
        ndjson_done "{\"changed\":$changed,\"ac\":\"$(json_escape "$ac_summary")\",\"playerbots\":\"$(json_escape "$pb_summary")\"}"
        ;;
      commands)
        # Per-installed-mod in-game command reference (Round M). Walks the
        # same three registries + custom-cpp-clone scan as `module list`
        # (dedup via _cmd_seen mirrors _mod_seen there), keeping only mods
        # that are BOTH installed AND have a block in _cmd_block_for
        # (47-commands.sh -- verbatim copy of the manager's case table). No
        # flags are accepted.
        while [[ $# -gt 0 ]]; do
          case "$1" in
            *) json_err BAD_ARG "Unknown flag: $1" "Usage: dml wow commands --json"; exit 1 ;;
          esac
        done
        sdir="$(_wow_server_dir)"
        if [[ -z "$sdir" ]]; then
          json_err NOT_FOUND "WoW Playerbots server not installed" "Install it first."; exit 1
        fi
        cmods='['; cfirst=1
        declare -A _cmd_seen=()
        while IFS='|' read -r mk mname murl msql; do
          [[ -z "$mk" ]] && continue
          _cmd_seen["$mk"]=1
          _cpp_installed "$sdir" "$mk" || continue
          ctext="$(_cmd_block_for "$mk")"
          [[ -z "$ctext" ]] && continue
          [[ $cfirst -eq 0 ]] && cmods+=','
          cmods+="{\"key\":\"$mk\",\"name\":\"$(json_escape "$mname")\",\"text\":\"$(json_escape "$ctext")\"}"
          cfirst=0
        done < <(_module_registry_cpp)
        if [[ -d "$sdir/modules" ]]; then
          for d in "$sdir/modules"/*/; do
            [[ -d "$d/.git" ]] || continue
            mk="$(basename "$d")"
            [[ -n "${_cmd_seen[$mk]:-}" ]] && continue
            _valid_cpp_key "$mk" || continue
            ctext="$(_cmd_block_for "$mk")"
            [[ -z "$ctext" ]] && continue
            [[ $cfirst -eq 0 ]] && cmods+=','
            cmods+="{\"key\":\"$mk\",\"name\":\"$(json_escape "$mk")\",\"text\":\"$(json_escape "$ctext")\"}"
            cfirst=0
          done
        fi
        while IFS='|' read -r mk mname murl; do
          [[ -z "$mk" ]] && continue
          _lua_cloned "$sdir" "$mk" || continue
          ctext="$(_cmd_block_for "$mk")"
          [[ -z "$ctext" ]] && continue
          [[ $cfirst -eq 0 ]] && cmods+=','
          cmods+="{\"key\":\"$mk\",\"name\":\"$(json_escape "$mname")\",\"text\":\"$(json_escape "$ctext")\"}"
          cfirst=0
        done < <(_module_registry_lua)
        while IFS='|' read -r mk mname murl mtype; do
          [[ -z "$mk" ]] && continue
          _sql_installed "$sdir" "$mk" || continue
          ctext="$(_cmd_block_for "$mk")"
          [[ -z "$ctext" ]] && continue
          [[ $cfirst -eq 0 ]] && cmods+=','
          cmods+="{\"key\":\"$mk\",\"name\":\"$(json_escape "$mname")\",\"text\":\"$(json_escape "$ctext")\"}"
          cfirst=0
        done < <(_module_registry_sql)
        cmods+=']'
        json_ok "{\"mods\":$cmods}"
        ;;
      *)
        json_err UNKNOWN_COMMAND "Unknown wow subcommand: $wsub" "Try: dml wow soap-setup --json"
        exit 1
        ;;
    esac
    ;;

  version)
    if [[ "$DML_JSON" == 1 ]]; then
        json_ok "{\"version\":\"$VERSION\"}"
    else
        echo "dml v$VERSION"
    fi
    ;;

  help|--help|-h)
    echo "dml -- Dad's MMO Lab CLI v$VERSION"
    echo ""
    echo "Commands:"
    echo "  doctor                check environment health"
    echo "  list                  list installed titles"
    echo "  status [<title>]      show running/stopped status (all titles if no arg)"
    echo "  start <title>         start a title's Docker server"
    echo "  stop <title>          stop a title's Docker server"
    echo "  lan <title> <action>  LAN play: on <lan-ip> | off | status | refresh <lan-ip>"
    echo "  scan                  show all running containers and which game ports they hold"
    echo "  kill <name|--all>     force-stop by project name (no directory needed)"
    echo "  clean [--yes]         stop stuck containers, remove incomplete installs, prune Docker"
    echo "  shell                 open an interactive shell"
    echo "  run <url|path>        install a title from GitHub URL or local folder"
    echo "  manage                open the WoW Server Manager (AzerothCore; auto-updates from GitHub)"
    echo "  unbound               install/update the Wrath Unbound add-on (wow-server-playerbots only)"
    echo "  unbound-remove        uninstall the Wrath Unbound add-on (wow-server-playerbots only)"
    echo "  version               print version"
    echo ""
    echo "Game data lives in /home/dml/ (ext4), never /mnt/c."
    ;;

  *)
    if [[ "$DML_JSON" == 1 ]]; then
        json_err UNKNOWN_COMMAND "Unknown command: $cmd" "Run 'dml help' for usage."
    else
        echo "[dml] Unknown command: $cmd" >&2
        echo "Run 'dml help' for usage." >&2
    fi
    exit 1
    ;;
esac