# ---------------------------------------------------------------------------
# Server console helpers: log-tail sanitizing + SOAP reply decoding.
# All read-only text filters -- no docker/SOAP calls live here.
# ---------------------------------------------------------------------------

# Filter: strips ANSI CSI escape sequences (color codes, ESC[?2004h
# bracketed-paste noise) and carriage returns from stdin. The $'…' quoting
# embeds a real ESC byte so this works on any sed, not just ones that
# understand \x1b in patterns.
_strip_ansi() {
    sed -E $'s/\x1b\\[[0-9;?]*[a-zA-Z]//g' | tr -d '\r'
}

# Decodes the XML entities soap_parse_result leaves in <result> text so the
# console shows real characters. &amp; is decoded LAST so "&amp;lt;" cannot
# double-decode. NB: a bare & in a ${var//pat/repl} replacement is a
# backreference (bash 5.2 patsub_replacement) -- hence the \&.
_soap_text_decode() {
    local s="${1-}"
    s=${s//&#xD;/}
    s=${s//&lt;/<}
    s=${s//&gt;/>}
    s=${s//&quot;/\"}
    s=${s//&amp;/\&}
    printf '%s' "$s"
    return 0
}

# stdin: sanitized log lines. stdout: a JSON array of strings.
_console_lines_json() {
    local line out="" first=1
    while IFS= read -r line; do
        if [[ $first == 1 ]]; then first=0; else out="$out,"; fi
        out="$out\"$(json_escape "$line")\""
    done
    printf '[%s]' "$out"
    return 0
}
