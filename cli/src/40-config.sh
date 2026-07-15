# ---------------------------------------------------------------------------
# WoW config registry + server-info parsing for the DML Launcher.
# (Registry + config verbs land in this file too — see `wow config` in main.)
# ---------------------------------------------------------------------------

# Parses the raw text of the SOAP `server info` result (stdin) into a JSON
# object (stdout). The raw text carries literal `&#xD;` entities because
# soap_parse_result extracts the <result> text without XML-decoding it.
# Unparseable fields become null rather than an error -- the Dashboard
# renders "unknown" for those instead of failing the whole card.
_parse_server_info() {
    local raw line version="" players="" uptime="" mean="" median=""
    raw="$(cat)"
    raw="${raw//&#xD;/}"
    while IFS= read -r line; do
        case "$line" in
            AzerothCore\ rev.*) version="${line#AzerothCore rev. }" ;;
            Connected\ players:*) players="${line#Connected players: }"; players="${players%%.*}" ;;
            Server\ uptime:*) uptime="${line#Server uptime: }" ;;
            *'|- Mean:'*) mean="${line#*Mean: }"; mean="${mean%%ms*}" ;;
            *'|- Median:'*) median="${line#*Median: }"; median="${median%%ms*}" ;;
        esac
    done <<< "$raw"
    [[ "$players" =~ ^[0-9]+$ ]] || players=null
    [[ "$mean" =~ ^[0-9]+$ ]] || mean=null
    [[ "$median" =~ ^[0-9]+$ ]] || median=null
    local vjson=null ujson=null
    [[ -n "$version" ]] && vjson="\"$(json_escape "$version")\""
    [[ -n "$uptime" ]] && ujson="\"$(json_escape "$uptime")\""
    printf '{"online":true,"version":%s,"players":%s,"uptime":%s,"mean_ms":%s,"median_ms":%s}' \
        "$vjson" "$players" "$ujson" "$mean" "$median"
    return 0
}
