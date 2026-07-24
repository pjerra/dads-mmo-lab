# ---------------------------------------------------------------------------
# JSON / NDJSON emit helpers (machine-readable mode for the DML Launcher).
# Pure bash — no jq at runtime. DML_JSON is set by the arg parser (see main).
# ---------------------------------------------------------------------------
DML_JSON="${DML_JSON:-0}"

json_escape() {
    local s="${1-}"
    s=${s//\\/\\\\}
    s=${s//\"/\\\"}
    s=${s//$'\n'/\\n}
    s=${s//$'\r'/\\r}
    s=${s//$'\t'/\\t}
    # Strip remaining ASCII control chars JSON forbids unescaped
    printf '%s' "$s" | tr -d '\000-\010\013\014\016-\037'
}

# NO-FORK sibling of json_escape: same transform, but returns via the global
# REPLY using ONLY bash parameter expansion (no `printf | tr` pipe, no command
# substitution). Call it WITHOUT `$()`. Hot per-row emitters use this to avoid
# a ~165ms process spawn per field on native Git Bash. It MUST stay
# byte-identical to json_escape -- the bats suite pins that (see json_escape_var
# torture tests). _JSON_CTRL_CLASS is a glob bracket of exactly the control
# bytes tr strips (1-8, 11, 12, 14-31; NUL can never appear in a bash string),
# so `${s//$_JSON_CTRL_CLASS/}` reproduces `tr -d '\000-\010\013\014\016-\037'`.
_JSON_CTRL_CLASS=$'[\001\002\003\004\005\006\007\010\013\014\016\017\020\021\022\023\024\025\026\027\030\031\032\033\034\035\036\037]'
json_escape_var() {
    local s="${1-}"
    s=${s//\\/\\\\}
    s=${s//\"/\\\"}
    s=${s//$'\n'/\\n}
    s=${s//$'\r'/\\r}
    s=${s//$'\t'/\\t}
    s=${s//$_JSON_CTRL_CLASS/}
    REPLY=$s
}

json_ok() {
    local data="${1:-null}"
    printf '{"ok":true,"data":%s}\n' "$data"
}

json_err() {
    local code="$1" msg="$2" hint="${3:-}"
    printf '{"ok":false,"error":{"code":"%s","message":"%s","hint":"%s"}}\n' \
        "$code" "$(json_escape "$msg")" "$(json_escape "$hint")"
}

ndjson_event() {
    printf '{%s}\n' "$1"
}

ndjson_line() {
    local level="$1" text="$2"
    ndjson_event "\"event\":\"line\",\"level\":\"$level\",\"text\":\"$(json_escape "$text")\""
}

ndjson_section_start() {
    ndjson_event "\"event\":\"section_start\",\"name\":\"$(json_escape "$1")\""
}

ndjson_section_end() {
    ndjson_event "\"event\":\"section_end\",\"name\":\"$(json_escape "$1")\",\"status\":\"$2\""
}

ndjson_done() {
    local data="${1:-null}"
    ndjson_event "\"event\":\"done\",\"data\":$data"
}

ndjson_error() {
    local code="$1" msg="$2" hint="${3:-}"
    ndjson_event "\"event\":\"error\",\"error\":{\"code\":\"$code\",\"message\":\"$(json_escape "$msg")\",\"hint\":\"$(json_escape "$hint")\"}"
}
