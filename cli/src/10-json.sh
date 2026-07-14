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
