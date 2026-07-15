# ---------------------------------------------------------------------------
# AzerothCore SOAP client. Mutating GM commands go through here.
# SOAP is synchronous on the single world thread — every call is serialized
# under an flock so the CLI never issues concurrent commands.
# ---------------------------------------------------------------------------
soap_url()  { echo "${DML_SOAP_URL:-http://127.0.0.1:7878/}"; }
soap_user() { echo "${DML_SOAP_USER:-admin}"; }
soap_pass() { echo "${DML_SOAP_PASS:-admin}"; }

# XML-escape stdin argument.
_xml_escape() {
    local s="${1-}"
    # NB: & must be escaped as \& in each replacement below. Bash's
    # ${var//pattern/replacement} treats a bare & in the replacement as a
    # backreference to the matched text (like sed) -- so an unescaped
    # `${s//</&lt;}` yields "<lt;" (the matched "<" substituted in) instead
    # of the literal string "&lt;". Verified empirically; not just a style
    # nit -- this silently broke soap_envelope's escaping of "<" and ">".
    s=${s//&/\&amp;}
    s=${s//</\&lt;}
    s=${s//>/\&gt;}
    printf '%s' "$s"
}

soap_envelope() {
    local cmd; cmd="$(_xml_escape "$1")"
    cat <<EOF
<?xml version="1.0" encoding="utf-8"?>
<SOAP-ENV:Envelope xmlns:SOAP-ENV="http://schemas.xmlsoap.org/soap/envelope/" xmlns:ns1="urn:AC">
  <SOAP-ENV:Body>
    <ns1:executeCommand><command>$cmd</command></ns1:executeCommand>
  </SOAP-ENV:Body>
</SOAP-ENV:Envelope>
EOF
}

# Prints <result> text (exit 0), or faultstring (exit 2) if a fault body.
soap_parse_result() {
    local xml="$1"
    if [[ "$xml" == *"<faultstring>"* ]]; then
        local f="${xml#*<faultstring>}"; f="${f%%</faultstring>*}"
        printf '%s' "$f"
        return 2
    fi
    if [[ "$xml" == *"<result>"* ]]; then
        local r="${xml#*<result>}"; r="${r%%</result>*}"
        printf '%s' "$r"
        return 0
    fi
    printf '%s' "$xml"
    return 2
}

# soap_exec <command> -> prints result text; exit 0 ok / 2 fault / 3 auth / 4 unreachable
soap_exec() {
    local cmd="$1" body resp code lockdir="$HOME/.dml"
    mkdir -p "$lockdir"
    body="$(soap_envelope "$cmd")"
    exec {lockfd}>>"$lockdir/soap.lock"
    flock "$lockfd"
    # Guarded assignment: under `set -e` (active in the built dml script) an
    # unguarded `resp="$(...)"` would abort the whole script the instant curl
    # exits non-zero (connection refused, timeout, etc.), before we ever get
    # a chance to classify the failure below. The if/else keeps the non-zero
    # exit local to this check.
    if resp="$(printf '%s' "$body" | curl -s -w '\n%{http_code}' \
        --max-time 30 \
        -u "$(soap_user):$(soap_pass)" \
        -H 'Content-Type: application/xml' \
        --data-binary @- "$(soap_url)" 2>/dev/null)"; then
        code=0
    else
        code=$?
    fi
    flock -u "$lockfd"
    exec {lockfd}>&-
    if [[ $code -ne 0 ]]; then
        return 4
    fi
    local http="${resp##*$'\n'}" xml="${resp%$'\n'*}"
    if [[ "$http" == "401" ]]; then return 3; fi
    local out rc
    if out="$(soap_parse_result "$xml")"; then
        rc=0
    else
        rc=$?
    fi
    printf '%s' "$out"
    return "$rc"
}
