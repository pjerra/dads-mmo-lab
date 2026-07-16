
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
    local _pc
    _pc="$(_check_port_conflicts || true)"
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
            _stream_cmd docker compose down || rc=$?
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
    docker compose down
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
        _stream_cmd docker compose down || rc=$?
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
            --coords) json_err BAD_ARG "Coordinate teleport is not available yet" "Use --to <named location>; coords need an offline DB path (planned)."; exit 1 ;;
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
      accounts)
        # Read-only list of real player accounts and their characters.
        # The 250 RNDBOT* ambient-bot accounts and AHBOT are noise for the
        # GUI's character picker; SOAP-only accounts (e.g. DMLSOAP) simply
        # have no characters and are harmless to include.
        sql="SELECT a.id, a.username, COALESCE(c.guid,''), COALESCE(c.name,''), COALESCE(c.level,'')
             FROM acore_auth.account a
             LEFT JOIN characters c ON c.account = a.id
             WHERE a.username NOT LIKE 'RNDBOT%' AND a.username <> 'AHBOT'
             ORDER BY a.id, c.level DESC;"
        rows="$(db_chars_query "$sql")" \
          || { json_err DB_UNREACHABLE "Could not reach the characters/auth database" "Is ac-database running?"; exit 1; }
        json_ok "{\"accounts\":$(printf '%s' "$rows" | _accounts_rows_to_json)}"
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
        sql="SELECT c.name,c.level,c.class,c.money,ci.slot,it.entry,it.name,it.Quality,it.ItemLevel,it.displayid
             FROM characters c
             JOIN character_inventory ci ON ci.guid=c.guid AND ci.bag=0 AND ci.slot BETWEEN 0 AND 18
             JOIN item_instance ii ON ii.guid=ci.item
             JOIN acore_world.item_template it ON it.entry=ii.itemEntry
             WHERE c.name='$(sql_escape "$char")' ORDER BY ci.slot;"
        rows="$(db_chars_query "$sql")" || { json_err DB_UNREACHABLE "Could not reach the characters database" ""; exit 1; }
        [[ -n "$rows" ]] || { json_err NOT_FOUND "No such character or no equipped items: $char" ""; exit 1; }
        cname=""; clevel=0; cclass=0; cmoney=0
        first=1; eq='['
        while IFS=$'\t' read -r nm lvl cls money slot entry iname q ilvl disp; do
          [[ -z "$nm" ]] && continue
          cname="$nm"; clevel="$lvl"; cclass="$cls"; cmoney="$money"
          [[ $first -eq 0 ]] && eq+=','
          eq+="{\"slot\":$slot,\"entry\":$entry,\"name\":\"$(json_escape "$iname")\",\"quality\":$q,\"item_level\":$ilvl,\"displayid\":$disp}"
          first=0
        done <<< "$rows"
        eq+=']'
        # last_saved: rows reflect the character table as of its last save to
        # the DB -- for a character currently online, that can lag their true
        # live state until their next auto-save/logout. Live-accurate data
        # would need a SOAP .pinfo call (future refinement, not built here).
        json_ok "{\"name\":\"$(json_escape "$cname")\",\"level\":$clevel,\"class\":$cclass,\"gold\":$((cmoney/10000)),\"note\":\"last_saved\",\"equipped\":$eq}"
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
              rreq=true
              if [[ "$key" == "server.motd" ]]; then
                rreq=false
                val="$motd_live"
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
              elif [[ "$key" == "bots.population" ]]; then
                _cfg_env_write AC_AI_PLAYERBOT_MIN_RANDOM_BOTS "$value"
                _cfg_env_write AC_AI_PLAYERBOT_MAX_RANDOM_BOTS "$value"
              else
                _cfg_env_write "$env" "$value"
              fi
              json_ok "{\"changed\":$CFG_CHANGED,\"restart_required\":$CFG_CHANGED}"
            fi
            ;;
          raw-read)
            fname=""
            [[ "${1:-}" == "--file" ]] && { _need_flag_val "$1" $#; fname="$2"; shift 2; }
            [[ -n "$fname" ]] || { json_err BAD_ARG "Missing --file <name>" ""; exit 1; }
            _cfg_preamble
            fpath="$(_cfg_file_path "$fname")" \
              || { json_err NOT_FOUND "Not an editable file: $fname" "Editable: .env, docker-compose.override.yml, playerbots.conf, mod_ahbot.conf, mod_ale.conf"; exit 1; }
            [[ -f "$fpath" ]] || { json_err NOT_FOUND "File does not exist yet: $fname" ""; exit 1; }
            json_ok "{\"file\":\"$(json_escape "$fname")\",\"content\":\"$(json_escape "$(cat "$fpath")")\"}"
            ;;
          raw-write)
            fname=""
            [[ "${1:-}" == "--file" ]] && { _need_flag_val "$1" $#; fname="$2"; shift 2; }
            [[ -n "$fname" ]] || { json_err BAD_ARG "Missing --file <name>" ""; exit 1; }
            _cfg_preamble
            fpath="$(_cfg_file_path "$fname")" \
              || { json_err NOT_FOUND "Not an editable file: $fname" "Editable: .env, docker-compose.override.yml, playerbots.conf, mod_ahbot.conf, mod_ale.conf"; exit 1; }
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
            case "$class" in
              warrior|paladin|hunter|rogue|priest|shaman|mage|warlock|druid) : ;;
              *) json_err BAD_ARG "Invalid class: $class" "One of: warrior paladin hunter rogue priest shaman mage warlock druid"; exit 1 ;;
            esac
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
            tries="${DML_PARTY_POLL_TRIES:-12}"; slp="${DML_PARTY_POLL_SLEEP:-0.5}"
            newguid=""; i=0
            while (( i < tries )); do
              now="$(_party_group_member_guids "$pguid" | tr '\n' ' ')"
              for g in $now; do
                [[ "$g" == "$pguid" ]] && continue
                case " $before " in *" $g "*) : ;; *) newguid="$g"; break ;; esac
              done
              [[ -n "$newguid" ]] && break
              i=$(( i + 1 ))
              [[ "$slp" != "0" ]] && sleep "$slp"
            done
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
          *)
            json_err UNKNOWN_COMMAND "Unknown party subcommand: $psub" "Try: dml wow party online --json"
            exit 1
            ;;
        esac
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