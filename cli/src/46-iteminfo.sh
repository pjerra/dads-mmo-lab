# ---------------------------------------------------------------------------
# Item tooltip/icon info (Round E): wowhead-sourced, disk-cached, with a
# local item_template fallback for server-custom items. jq is test-only, so
# wowhead JSON is embedded verbatim; only the icon NAME is regex-extracted.
# The verb never fails on network/DB trouble -- degradation is per-item.
# ---------------------------------------------------------------------------

_wowhead_base()     { echo "${DML_WOWHEAD_BASE:-https://nether.wowhead.com}"; }
_zamimg_base()      { echo "${DML_ZAMIMG_BASE:-https://wow.zamimg.com}"; }
# The item XML (displayId source) lives on the main site, NOT the tooltip
# host -- nether.wowhead.com 404s on /wotlk/item=N&xml (curl-verified).
_wowhead_xml_base() { echo "${DML_WOWHEAD_XML_BASE:-https://www.wowhead.com}"; }
_iteminfo_cache()   { echo "$HOME/.dml/wowhead-cache"; }

# Fetch url ($1) into file ($2); echoes the http code ("000" on transport
# failure). Body goes straight to disk (icons are binary -- a bash variable
# would eat NUL bytes). Always returns 0.
_iteminfo_fetch() {
    local code
    if code="$(curl -s -o "$2" -w '%{http_code}' --max-time 10 "$1" </dev/null 2>/dev/null)"; then :; else code=000; fi
    printf '%s' "$code"
    return 0
}

_iteminfo_stat_name() {
    case "$1" in
        3) echo "Agility" ;; 4) echo "Strength" ;; 5) echo "Intellect" ;;
        6) echo "Spirit" ;; 7) echo "Stamina" ;;
        12) echo "Defense Rating" ;; 13) echo "Dodge Rating" ;;
        14) echo "Parry Rating" ;; 15) echo "Block Rating" ;;
        31) echo "Hit Rating" ;; 32) echo "Critical Strike Rating" ;;
        35) echo "Resilience Rating" ;; 36) echo "Haste Rating" ;;
        37) echo "Expertise Rating" ;; 38) echo "Attack Power" ;;
        43) echo "Mana per 5 sec." ;; 45) echo "Spell Power" ;;
        *) echo "Stat $1" ;;
    esac
}

# Wowhead's 3D display id for an item entry, from the wotlk item XML (the
# tooltip JSON carries NO displayId -- only the XML's <icon displayId="N">
# attribute has it). Fetched once, disk-cached under xml/. Prints the bare
# numeric id, or nothing when wowhead doesn't know the item / the attribute
# is missing/zero-less-worthy / the fetch failed. Never fails.
# NB: unknown items still answer HTTP 200 with an <error> body -- that IS
# cached (a definitive "wowhead has no such item"), while non-200 bodies and
# 200s that don't look like wowhead XML (CDN interstitials) are dropped so a
# transient error page can't poison the cache.
_iteminfo_display_id() {
    local entry="$1" cache xf raw code
    cache="$(_iteminfo_cache)"
    xf="$cache/xml/$entry.xml"
    if [[ ! -f "$xf" ]]; then
        mkdir -p "$cache/xml"
        code="$(_iteminfo_fetch "$(_wowhead_xml_base)/wotlk/item=$entry&xml" "$xf.tmp.$$")"
        if [[ "$code" == 200 ]] && grep -q '<wowhead>' "$xf.tmp.$$" 2>/dev/null; then
            mv "$xf.tmp.$$" "$xf"
        else
            rm -f "$xf.tmp.$$"
        fi
    fi
    [[ -f "$xf" ]] || return 0
    raw="$(cat "$xf")"
    if [[ "$raw" =~ displayId=\"([0-9]+)\" ]]; then
        printf '%s' "${BASH_REMATCH[1]}"
    fi
    return 0
}

# The optional `,"display_id":N` envelope fragment for one entry -- empty
# when wowhead has no (positive) display id. Numeric regex-gate: only a
# strictly positive integer is ever embedded (the XML serves displayId="0"
# for non-displayable kinds like gems).
_iteminfo_did_field() {
    local did
    did="$(_iteminfo_display_id "$1")"
    if [[ "$did" =~ ^[1-9][0-9]*$ ]]; then
        printf ',"display_id":%s' "$did"
    fi
    return 0
}

# Minimal in-game-style tooltip from item_template (custom/unknown-to-wowhead
# items). Prints a "local" item object, or "unavailable" when the DB can't
# answer either. $2 (optional) = pre-formatted extra envelope fields
# (`,"k":v...`) appended verbatim before the closing brace.
_iteminfo_local() {
    local entry="$1" extra="${2:-}" row
    if row="$(db_world_query "SELECT name,Quality,ItemLevel,armor,RequiredLevel,dmg_min1,dmg_max1,delay,stat_type1,stat_value1,stat_type2,stat_value2,stat_type3,stat_value3,stat_type4,stat_value4,stat_type5,stat_value5 FROM item_template WHERE entry=$entry LIMIT 1;")"; then :; else row=""; fi
    if [[ -z "$row" ]]; then
        printf '{"entry":%s,"source":"unavailable"%s}' "$entry" "$extra"
        return 0
    fi
    local name q ilvl armor rlvl dmin dmax delay t1 v1 t2 v2 t3 v3 t4 v4 t5 v5
    IFS=$'\t' read -r name q ilvl armor rlvl dmin dmax delay t1 v1 t2 v2 t3 v3 t4 v4 t5 v5 <<< "$row"
    local hname html pair st sv spd
    hname="$(_xml_escape "$name")"
    html="<b class=\"q$q\">$hname</b><br><span class=\"q\">Item Level $ilvl</span>"
    [[ "$armor" -gt 0 ]] 2>/dev/null && html+="<br><span class=\"q1\">$armor Armor</span>"
    if awk "BEGIN{exit !($dmax > 0)}" 2>/dev/null; then
        spd="$(awk "BEGIN{printf \"%.2f\", $delay/1000}")"
        html+="<br><span class=\"q1\">$dmin - $dmax Damage</span> <span class=\"q1\">Speed $spd</span>"
    fi
    for pair in "$t1:$v1" "$t2:$v2" "$t3:$v3" "$t4:$v4" "$t5:$v5"; do
        st="${pair%%:*}"; sv="${pair##*:}"
        [[ -z "$st" || -z "$sv" || "$sv" == 0 ]] && continue
        html+="<br><span class=\"q1\">+$sv $(_iteminfo_stat_name "$st")</span>"
    done
    [[ "$rlvl" -gt 0 ]] 2>/dev/null && html+="<br><span class=\"q1\">Requires Level $rlvl</span>"
    printf '{"entry":%s,"source":"local","name":"%s","quality":%s,"tooltip_html":"%s"%s}' \
        "$entry" "$(json_escape "$name")" "$q" "$(json_escape "$html")" "$extra"
    return 0
}

# One item object (wowhead -> local -> unavailable). Never fails. Every
# source variant may carry `display_id` (wowhead's own 3D display id from
# the item XML) -- the launcher's model viewer uses it to heal items whose
# server displayid has no Wowhead model data (e.g. both Warglaives: server
# 45479/45481 vs wowhead 45150/45146). Fetch ORDER is load-bearing for the
# test curl stub's sequential responder: tooltip, icon, then xml.
_iteminfo_one() {
    local entry="$1" cache tj raw code icon="" iconfile iconjson=null b64json=null didfield
    cache="$(_iteminfo_cache)"
    tj="$cache/tooltips/$entry.json"
    if [[ ! -f "$tj" ]]; then
        code="$(_iteminfo_fetch "$(_wowhead_base)/wotlk/tooltip/item/$entry?dataEnv=8&locale=0" "$tj.tmp")"
        if [[ "$code" == 200 ]]; then mv "$tj.tmp" "$tj"; else rm -f "$tj.tmp"; fi
    fi
    if [[ -f "$tj" ]]; then
        raw="$(cat "$tj")"
        # wowhead's tooltip contract always carries both "name" and "tooltip";
        # brace-wrapped junk lacking them (e.g. an error body) falls through
        # to the poisoned-cache drop below instead of being embedded verbatim
        # and permanently corrupting the batch envelope for every future call.
        if [[ "$raw" == \{*\} && "$raw" == *'"name":"'* && "$raw" == *'"tooltip":"'* ]]; then
            if [[ "$raw" =~ \"icon\":\"([A-Za-z0-9_.-]+)\" ]]; then icon="${BASH_REMATCH[1]}"; fi
            if [[ -n "$icon" ]]; then
                iconfile="$cache/icons/$icon.jpg"
                if [[ ! -f "$iconfile" ]]; then
                    code="$(_iteminfo_fetch "$(_zamimg_base)/images/wow/icons/large/$icon.jpg" "$iconfile.tmp.$$")"
                    if [[ "$code" == 200 ]]; then mv "$iconfile.tmp.$$" "$iconfile"; else rm -f "$iconfile.tmp.$$"; fi
                fi
                if [[ -f "$iconfile" ]]; then
                    b64json="\"$(base64 -w0 < "$iconfile")\""
                fi
                iconjson="\"$icon\""
            fi
            didfield="$(_iteminfo_did_field "$entry")"
            printf '{"entry":%s,"source":"wowhead","icon":%s,"icon_b64":%s,"wowhead":%s%s}' \
                "$entry" "$iconjson" "$b64json" "$raw" "$didfield"
            return 0
        fi
        rm -f "$tj"   # poisoned cache entry (non-JSON body)
    fi
    # Local fallback still asks the XML: a transient tooltip-host failure
    # must not cost the viewer its display-id translation.
    didfield="$(_iteminfo_did_field "$entry")"
    _iteminfo_local "$entry" "$didfield"
    return 0
}

# --- kind-generalized wowhead entities (Round G: spell|achievement) --------
# Same fetch/cache/icon machinery as items, but NO local fallback -- these
# kinds have no names in the server DB. Cache key carries the kind so ids
# can't collide across kinds (items keep their legacy un-prefixed files).
_entity_one() {
    local kind="$1" id="$2" cache tj raw code icon="" iconfile iconjson=null b64json=null
    cache="$(_iteminfo_cache)"
    tj="$cache/tooltips/$kind-$id.json"
    if [[ ! -f "$tj" ]]; then
        code="$(_iteminfo_fetch "$(_wowhead_base)/wotlk/tooltip/$kind/$id?dataEnv=8&locale=0" "$tj.tmp")"
        if [[ "$code" == 200 ]]; then mv "$tj.tmp" "$tj"; else rm -f "$tj.tmp"; fi
    fi
    if [[ -f "$tj" ]]; then
        raw="$(cat "$tj")"
        if [[ "$raw" == \{*\} && "$raw" == *'"name":"'* && "$raw" == *'"tooltip":"'* ]]; then
            if [[ "$raw" =~ \"icon\":\"([A-Za-z0-9_.-]+)\" ]]; then icon="${BASH_REMATCH[1]}"; fi
            if [[ -n "$icon" ]]; then
                iconfile="$cache/icons/$icon.jpg"
                if [[ ! -f "$iconfile" ]]; then
                    code="$(_iteminfo_fetch "$(_zamimg_base)/images/wow/icons/large/$icon.jpg" "$iconfile.tmp.$$")"
                    if [[ "$code" == 200 ]]; then mv "$iconfile.tmp.$$" "$iconfile"; else rm -f "$iconfile.tmp.$$"; fi
                fi
                if [[ -f "$iconfile" ]]; then
                    b64json="\"$(base64 -w0 < "$iconfile")\""
                fi
                iconjson="\"$icon\""
            fi
            printf '{"id":%s,"source":"wowhead","icon":%s,"icon_b64":%s,"wowhead":%s}' \
                "$id" "$iconjson" "$b64json" "$raw"
            return 0
        fi
        rm -f "$tj"   # poisoned cache entry
    fi
    printf '{"id":%s,"source":"unavailable"}' "$id"
    return 0
}
