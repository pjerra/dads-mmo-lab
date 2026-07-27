//! Native-mode **item-info / entity-info** reader (`wow item-info`, `wow
//! entity-info`, task D1b). Faithful port of `cli/src/46-iteminfo.sh`
//! (`_iteminfo_one`, `_iteminfo_local`, `_iteminfo_display_id`,
//! `_entity_one`) + the arms at `cli/src/90-main.sh:2192-2251`.
//!
//! Wowhead tooltip JSON + zamimg icon bytes are fetched over `reqwest`
//! (blocking, bounded ~10s — the CLI's `curl --max-time 10`) and disk-cached
//! under the SAME `~/.dml/wowhead-cache` tree the bash CLI reads/writes
//! (`dml::cachestatus::cache_dir()`), so a cache warmed by either backend is
//! reused by the other:
//!   - `tooltips/<id>.json` (items) / `tooltips/<kind>-<id>.json` (entities)
//!   - `icons/<icon>.jpg`
//!   - `xml/<id>.xml` (items only — Wowhead's 3D `display_id`)
//!
//! Items additionally fall back to a minimal tooltip built from the local
//! `item_template` DB row (`_iteminfo_local`) when Wowhead has no data for a
//! custom/server-only entry. Entities (`spell`/`achievement`) have NO DB
//! fallback — unknown-to-wowhead is `{"id":N,"source":"unavailable"}`.
//!
//! Every fetch degrades gracefully: a transport failure, a non-200, or a
//! response that doesn't look like the expected shape never surfaces as an
//! error to the caller — it just falls through to the next source, exactly
//! like the bash arms (`_iteminfo_one`/`_entity_one` "never fail").
//!
//! This module is split pure-parser-first: every parsing/shape/envelope
//! decision (tooltip validity, icon-name extraction, display-id extraction,
//! the local-fallback HTML tooltip, the JSON envelopes, dedup order) is a
//! plain function taking already-fetched bytes/rows, so it's unit-testable
//! without any network or DB. The network/DB/filesystem glue
//! (`item_info_one`, `entity_info_one`, `read_item_info`, `read_entity_info`)
//! composes those pure pieces and is exercised by the skip-guarded live
//! tests at the bottom (gated on real reachability, like
//! `dml::db`'s `live_db_roundtrip_when_reachable`).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use serde_json::{json, Value};

use super::db;

// --- config / bases (env-overridable, matching the CLI's `${VAR:-default}`) -

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).ok().filter(|s| !s.is_empty()).unwrap_or_else(|| default.to_string())
}

/// `_wowhead_base` (`46-iteminfo.sh:8`).
fn wowhead_base() -> String {
    env_or("DML_WOWHEAD_BASE", "https://nether.wowhead.com")
}
/// `_zamimg_base` (`46-iteminfo.sh:9`).
fn zamimg_base() -> String {
    env_or("DML_ZAMIMG_BASE", "https://wow.zamimg.com")
}
/// `_wowhead_xml_base` (`46-iteminfo.sh:12`).
fn wowhead_xml_base() -> String {
    env_or("DML_WOWHEAD_XML_BASE", "https://www.wowhead.com")
}

// --- pure parsers -----------------------------------------------------------

/// `_iteminfo_one`/`_entity_one`'s poisoned-cache gate (`46-iteminfo.sh:134`,
/// `:175`): a tooltip body is only trusted when it's brace-wrapped AND
/// carries both a `"name":"` and a `"tooltip":"` key — an error body (or any
/// other non-tooltip JSON) fails this and gets dropped rather than embedded
/// verbatim. No trim: mirrors the bash glob (`$raw == \{*\}`) exactly against
/// the raw cached/fetched bytes.
pub fn is_valid_tooltip_json(raw: &str) -> bool {
    raw.starts_with('{')
        && raw.ends_with('}')
        && raw.contains("\"name\":\"")
        && raw.contains("\"tooltip\":\"")
}

/// First `[A-Za-z0-9_.-]+` run following a literal `"icon":"` (port of the
/// bash regex `\"icon\":\"([A-Za-z0-9_.-]+)\"`, `46-iteminfo.sh:135`/`:176`).
/// `None` when the key is absent or immediately followed by zero charset
/// chars.
pub fn extract_icon_name(raw: &str) -> Option<String> {
    const KEY: &str = "\"icon\":\"";
    let start = raw.find(KEY)? + KEY.len();
    let bytes = raw.as_bytes();
    let mut end = start;
    while end < bytes.len() {
        let c = bytes[end];
        if c.is_ascii_alphanumeric() || c == b'_' || c == b'.' || c == b'-' {
            end += 1;
        } else {
            break;
        }
    }
    if end == start {
        None
    } else {
        Some(raw[start..end].to_string())
    }
}

/// `grep -q '<wowhead>'` (`_iteminfo_display_id`, `46-iteminfo.sh:55`) — the
/// XML-looks-like-wowhead gate that decides whether a freshly-fetched item
/// XML body gets persisted to the cache at all.
pub fn xml_looks_like_wowhead(raw: &str) -> bool {
    raw.contains("<wowhead>")
}

/// First `displayId="N"` attribute's digit run (port of the bash regex
/// `displayId=\"([0-9]+)\"`, `_iteminfo_display_id`, `46-iteminfo.sh:63-65`).
/// Raw digits only — the strictly-positive/no-leading-zero filter is
/// [`extract_positive_display_id`] (matching `_iteminfo_did_field`'s
/// SEPARATE `^[1-9][0-9]*$` gate, `46-iteminfo.sh:76`).
pub fn extract_display_id_digits(raw: &str) -> Option<String> {
    const KEY: &str = "displayId=\"";
    let start = raw.find(KEY)? + KEY.len();
    let bytes = raw.as_bytes();
    let mut end = start;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    if end == start {
        None
    } else {
        Some(raw[start..end].to_string())
    }
}

/// `_iteminfo_did_field`'s embed gate (`46-iteminfo.sh:73-80`): only a
/// strictly positive, non-leading-zero integer digit run is ever emitted as
/// `display_id` (a bare `displayId="0"` — e.g. gems — never qualifies).
pub fn extract_positive_display_id(raw: &str) -> Option<u32> {
    let digits = extract_display_id_digits(raw)?;
    if digits.starts_with('0') {
        return None;
    }
    digits.parse::<u32>().ok()
}

/// `_iteminfo_stat_name` (`46-iteminfo.sh:25-37`) — verbatim stat-type ->
/// display-name table, `"Stat N"` for anything unlisted.
pub fn stat_name(stat_type: i64) -> String {
    match stat_type {
        3 => "Agility".into(),
        4 => "Strength".into(),
        5 => "Intellect".into(),
        6 => "Spirit".into(),
        7 => "Stamina".into(),
        12 => "Defense Rating".into(),
        13 => "Dodge Rating".into(),
        14 => "Parry Rating".into(),
        15 => "Block Rating".into(),
        31 => "Hit Rating".into(),
        32 => "Critical Strike Rating".into(),
        35 => "Resilience Rating".into(),
        36 => "Haste Rating".into(),
        37 => "Expertise Rating".into(),
        38 => "Attack Power".into(),
        43 => "Mana per 5 sec.".into(),
        45 => "Spell Power".into(),
        other => format!("Stat {other}"),
    }
}

/// `_xml_escape` (`20-soap.sh:32-44`) — the 3-entity subset the local
/// tooltip HTML needs (item names never carry `"`/`'` that matter here).
pub fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// One `item_template` row, already decoded from [`db::SqlValue`] cells —
/// the pure shape [`build_local_tooltip_html`] and [`local_item_json`] work
/// from, independent of the DB driver.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalItemRow {
    pub name: String,
    pub quality: i64,
    pub item_level: i64,
    pub armor: i64,
    pub required_level: i64,
    /// Raw DB text form (bash never reformats these — they're interpolated
    /// verbatim into the tooltip HTML).
    pub dmg_min_text: String,
    pub dmg_max_text: String,
    /// Parsed for the `dmax > 0` weapon-damage gate only.
    pub dmg_max: f64,
    pub delay: i64,
    /// (stat_type, stat_value) x5, in column order.
    pub stats: [(i64, i64); 5],
}

/// The `item_template` SELECT's column order (`_iteminfo_local`,
/// `46-iteminfo.sh:88`) — shared by the query builder and [`parse_local_row`]
/// so they can never drift apart.
pub const LOCAL_ITEM_COLUMNS: &str = "name,Quality,ItemLevel,armor,RequiredLevel,dmg_min1,dmg_max1,delay,\
stat_type1,stat_value1,stat_type2,stat_value2,stat_type3,stat_value3,stat_type4,stat_value4,stat_type5,stat_value5";

fn sql_i64(v: &db::SqlValue) -> i64 {
    match v {
        db::SqlValue::Int(i) => *i,
        db::SqlValue::Text(s) => s.trim().parse::<i64>().unwrap_or(0),
        db::SqlValue::Null => 0,
    }
}
fn sql_f64(v: &db::SqlValue) -> f64 {
    match v {
        db::SqlValue::Int(i) => *i as f64,
        db::SqlValue::Text(s) => s.trim().parse::<f64>().unwrap_or(0.0),
        db::SqlValue::Null => 0.0,
    }
}
fn sql_text(v: &db::SqlValue) -> String {
    match v {
        db::SqlValue::Text(s) => s.clone(),
        db::SqlValue::Int(i) => i.to_string(),
        db::SqlValue::Null => String::new(),
    }
}

/// Decode one `item_template` result row (18 cells, [`LOCAL_ITEM_COLUMNS`]
/// order) into a [`LocalItemRow`]. `None` for a short/malformed row (a
/// defensive guard — a real `SELECT` from that column list always yields
/// exactly 18 cells).
pub fn parse_local_row(cells: &[db::SqlValue]) -> Option<LocalItemRow> {
    if cells.len() < 18 {
        return None;
    }
    Some(LocalItemRow {
        name: sql_text(&cells[0]),
        quality: sql_i64(&cells[1]),
        item_level: sql_i64(&cells[2]),
        armor: sql_i64(&cells[3]),
        required_level: sql_i64(&cells[4]),
        dmg_min_text: sql_text(&cells[5]),
        dmg_max_text: sql_text(&cells[6]),
        dmg_max: sql_f64(&cells[6]),
        delay: sql_i64(&cells[7]),
        stats: [
            (sql_i64(&cells[8]), sql_i64(&cells[9])),
            (sql_i64(&cells[10]), sql_i64(&cells[11])),
            (sql_i64(&cells[12]), sql_i64(&cells[13])),
            (sql_i64(&cells[14]), sql_i64(&cells[15])),
            (sql_i64(&cells[16]), sql_i64(&cells[17])),
        ],
    })
}

/// `_iteminfo_local`'s HTML builder (`46-iteminfo.sh:93-108`), verbatim:
/// name/quality/ilvl header, armor line (if `armor > 0`), weapon damage +
/// speed line (if `dmg_max > 0`, speed = `delay/1000` to 2dp), each nonzero
/// stat (`+N <StatName>`, skipped only when the VALUE is zero — matching the
/// bash's `"$sv" == 0` check, not the stat type), then a "Requires Level"
/// line (if `required_level > 0`).
pub fn build_local_tooltip_html(row: &LocalItemRow) -> String {
    let hname = xml_escape(&row.name);
    let mut html = format!(
        "<b class=\"q{}\">{}</b><br><span class=\"q\">Item Level {}</span>",
        row.quality, hname, row.item_level
    );
    if row.armor > 0 {
        html.push_str(&format!("<br><span class=\"q1\">{} Armor</span>", row.armor));
    }
    if row.dmg_max > 0.0 {
        let spd = row.delay as f64 / 1000.0;
        html.push_str(&format!(
            "<br><span class=\"q1\">{} - {} Damage</span> <span class=\"q1\">Speed {:.2}</span>",
            row.dmg_min_text, row.dmg_max_text, spd
        ));
    }
    for &(st, sv) in &row.stats {
        if sv == 0 {
            continue;
        }
        html.push_str(&format!("<br><span class=\"q1\">+{} {}</span>", sv, stat_name(st)));
    }
    if row.required_level > 0 {
        html.push_str(&format!("<br><span class=\"q1\">Requires Level {}</span>", row.required_level));
    }
    html
}

/// `_iteminfo_local`'s envelope (`46-iteminfo.sh:86-112`): `"local"` when a
/// row was found, `"unavailable"` otherwise — either way `display_id` (from
/// the item XML, resolved independently of the DB lookup) rides along when
/// present, matching `_iteminfo_one`'s always-computed `$didfield`.
pub fn local_item_json(entry: u32, row: Option<&LocalItemRow>, display_id: Option<u32>) -> Value {
    let mut v = match row {
        None => json!({"entry": entry, "source": "unavailable"}),
        Some(r) => json!({
            "entry": entry,
            "source": "local",
            "name": r.name,
            "quality": r.quality,
            "tooltip_html": build_local_tooltip_html(r),
        }),
    };
    if let Some(did) = display_id {
        v["display_id"] = json!(did);
    }
    v
}

/// `_iteminfo_one`'s wowhead-hit envelope (`46-iteminfo.sh:148-149`):
/// `icon`/`icon_b64` are `null` when no icon name was extracted or the icon
/// bytes couldn't be fetched/cached; `wowhead` carries the tooltip JSON
/// verbatim (re-encoded through `serde_json`, semantically identical to the
/// bash's byte-verbatim embed); `display_id` rides along when present.
pub fn wowhead_item_json(
    entry: u32,
    icon: Option<&str>,
    icon_b64: Option<&str>,
    wowhead: &Value,
    display_id: Option<u32>,
) -> Value {
    let mut v = json!({
        "entry": entry,
        "source": "wowhead",
        "icon": icon,
        "icon_b64": icon_b64,
        "wowhead": wowhead,
    });
    if let Some(did) = display_id {
        v["display_id"] = json!(did);
    }
    v
}

/// `_entity_one`'s wowhead-hit envelope (`46-iteminfo.sh:188-189`) — same
/// shape as [`wowhead_item_json`] but keyed `id` and with NO `display_id`
/// (entities have no item XML to source one from).
pub fn wowhead_entity_json(id: u32, icon: Option<&str>, icon_b64: Option<&str>, wowhead: &Value) -> Value {
    json!({
        "id": id,
        "source": "wowhead",
        "icon": icon,
        "icon_b64": icon_b64,
        "wowhead": wowhead,
    })
}

/// `_entity_one`'s no-fallback miss (`46-iteminfo.sh:194`).
pub fn unavailable_entity_json(id: u32) -> Value {
    json!({"id": id, "source": "unavailable"})
}

/// The `declare -A _ii_seen=()` / `_ee_seen=()` loops in the `item-info` /
/// `entity-info` arms (`90-main.sh:2209-2215`, `:2244-2250`): duplicates
/// dropped, first-seen order preserved.
pub fn dedupe_preserve_order(ids: &[u32]) -> Vec<u32> {
    let mut seen = HashSet::new();
    ids.iter().copied().filter(|id| seen.insert(*id)).collect()
}

// --- network + cache + DB glue (not unit-tested directly; see the ---------
// --- skip-guarded live tests below) ----------------------------------------

const FETCH_TIMEOUT: Duration = Duration::from_secs(10);

fn client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder().timeout(FETCH_TIMEOUT).build().expect("reqwest client")
}

/// One bounded fetch. `Some(bytes)` ONLY on an exact HTTP 200 (mirrors every
/// `_iteminfo_fetch` call site's `[[ "$code" == 200 ]]` check) — any other
/// status or transport failure is `None`, never a hard error.
fn fetch_200(url: &str) -> Option<Vec<u8>> {
    let resp = client().get(url).send().ok()?;
    if resp.status().as_u16() != 200 {
        return None;
    }
    resp.bytes().ok().map(|b| b.to_vec())
}

/// Atomic-ish cache write (tmp + rename); best-effort — a failure here just
/// means the next call re-fetches, same as the bash `mv`/`rm -f` dance.
fn cache_store(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut tmp = path.as_os_str().to_os_string();
    tmp.push(format!(".tmp.{}", std::process::id()));
    let tmp = PathBuf::from(tmp);
    if std::fs::write(&tmp, bytes).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    } else {
        let _ = std::fs::remove_file(&tmp);
    }
}

/// Cache-or-fetch a tooltip body (item OR entity — same `tooltips/` tree,
/// different cache-file naming per caller): cache hit -> its bytes as text;
/// else fetch, cache unconditionally on 200 (content validity is judged by
/// the CALLER, same split as bash's write-then-validate-on-read).
fn fetch_or_load_text(cache_file: &Path, url: &str) -> Option<String> {
    if let Ok(bytes) = std::fs::read(cache_file) {
        return Some(String::from_utf8_lossy(&bytes).into_owned());
    }
    let bytes = fetch_200(url)?;
    cache_store(cache_file, &bytes);
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// Icon resolution shared by items and entities (`_iteminfo_one`/
/// `_entity_one`'s identical icon block): extract the icon name from the
/// tooltip body, cache-or-fetch its jpg, base64-encode if the bytes are in
/// hand. Returns `(icon_name, icon_b64)` — `icon_name` is `Some` as soon as
/// the tooltip names one, independent of whether the byte fetch succeeded
/// (matching `iconjson` being set unconditionally once `$icon` is nonempty).
fn resolve_icon(cache_root: &Path, tooltip_raw: &str) -> (Option<String>, Option<String>) {
    let Some(icon) = extract_icon_name(tooltip_raw) else {
        return (None, None);
    };
    let icon_file = cache_root.join("icons").join(format!("{icon}.jpg"));
    let bytes = if let Ok(b) = std::fs::read(&icon_file) {
        Some(b)
    } else {
        let url = format!("{}/images/wow/icons/large/{icon}.jpg", zamimg_base());
        let fetched = fetch_200(&url);
        if let Some(ref b) = fetched {
            cache_store(&icon_file, b);
        }
        fetched
    };
    let b64 = bytes.map(|b| BASE64.encode(b));
    (Some(icon), b64)
}

/// `_iteminfo_display_id` (`46-iteminfo.sh:48-67`): cache hit -> extract from
/// the cached file; cache miss -> fetch, and ONLY persist + extract when the
/// body passes [`xml_looks_like_wowhead`] (a non-200/invalid fetch leaves
/// the cache untouched AND yields no display id for this call — mirroring
/// the bash's `[[ -f "$xf" ]] || return 0` after a failed `mv`).
fn resolve_display_id(cache_root: &Path, entry: u32) -> Option<u32> {
    let xf = cache_root.join("xml").join(format!("{entry}.xml"));
    if let Ok(bytes) = std::fs::read(&xf) {
        let text = String::from_utf8_lossy(&bytes).into_owned();
        return extract_positive_display_id(&text);
    }
    let url = format!("{}/wotlk/item={entry}&xml", wowhead_xml_base());
    let bytes = fetch_200(&url)?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    if !xml_looks_like_wowhead(&text) {
        return None;
    }
    cache_store(&xf, &bytes);
    extract_positive_display_id(&text)
}

fn fetch_local_row(cfg: &db::DbConfig, entry: u32) -> Option<LocalItemRow> {
    let sql = format!("SELECT {LOCAL_ITEM_COLUMNS} FROM item_template WHERE entry=? LIMIT 1");
    let params: Vec<mysql::Value> = vec![mysql::Value::from(entry as u64)];
    let res = db::query_with_params(cfg, db::Database::World, &sql, params).ok()?;
    let row = res.rows.into_iter().next()?;
    parse_local_row(&row)
}

/// One item's full pipeline (`_iteminfo_one`, `46-iteminfo.sh:120-159`):
/// wowhead tooltip -> icon -> display_id -> envelope, falling back to the
/// local DB row (still trying display_id first) when wowhead has nothing
/// usable. `db_cfg: None` skips the DB fallback outright (e.g. server known
/// to be down) rather than paying a doomed connect attempt per item.
pub fn item_info_one(cache_root: &Path, db_cfg: Option<&db::DbConfig>, entry: u32) -> Value {
    let tj = cache_root.join("tooltips").join(format!("{entry}.json"));
    let url = format!("{}/wotlk/tooltip/item/{entry}?dataEnv=8&locale=0", wowhead_base());
    if let Some(raw) = fetch_or_load_text(&tj, &url) {
        if is_valid_tooltip_json(&raw) {
            let (icon, icon_b64) = resolve_icon(cache_root, &raw);
            let wowhead_val: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
            let did = resolve_display_id(cache_root, entry);
            return wowhead_item_json(entry, icon.as_deref(), icon_b64.as_deref(), &wowhead_val, did);
        }
        let _ = std::fs::remove_file(&tj); // poisoned cache entry (non-JSON/error body)
    }
    let did = resolve_display_id(cache_root, entry);
    let row = db_cfg.and_then(|cfg| fetch_local_row(cfg, entry));
    local_item_json(entry, row.as_ref(), did)
}

/// One spell/achievement's full pipeline (`_entity_one`, `46-iteminfo.sh:
/// 165-196`): same tooltip/icon machinery as items, no local/DB fallback and
/// no display_id (unknown -> `{"id":N,"source":"unavailable"}`).
pub fn entity_info_one(cache_root: &Path, kind: &str, id: u32) -> Value {
    let tj = cache_root.join("tooltips").join(format!("{kind}-{id}.json"));
    let url = format!("{}/wotlk/tooltip/{kind}/{id}?dataEnv=8&locale=0", wowhead_base());
    if let Some(raw) = fetch_or_load_text(&tj, &url) {
        if is_valid_tooltip_json(&raw) {
            let (icon, icon_b64) = resolve_icon(cache_root, &raw);
            let wowhead_val: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
            return wowhead_entity_json(id, icon.as_deref(), icon_b64.as_deref(), &wowhead_val);
        }
        let _ = std::fs::remove_file(&tj); // poisoned cache entry
    }
    unavailable_entity_json(id)
}

/// `item-info` arm (`90-main.sh:2192-2220`): dedupe (first-seen order),
/// ensure the cache subdirs exist, fetch each entry. Max-25/BAD_ARG
/// validation is the caller's (Tauri command) job, matching where the CLI
/// arm itself validates before reaching this loop.
pub fn read_item_info(cache_root: &Path, db_cfg: Option<&db::DbConfig>, entries: &[u32]) -> Value {
    let _ = std::fs::create_dir_all(cache_root.join("tooltips"));
    let _ = std::fs::create_dir_all(cache_root.join("icons"));
    let items: Vec<Value> =
        dedupe_preserve_order(entries).into_iter().map(|e| item_info_one(cache_root, db_cfg, e)).collect();
    json!({"items": items})
}

/// `entity-info` arm (`90-main.sh:2221-2251`).
pub fn read_entity_info(cache_root: &Path, kind: &str, ids: &[u32]) -> Value {
    let _ = std::fs::create_dir_all(cache_root.join("tooltips"));
    let _ = std::fs::create_dir_all(cache_root.join("icons"));
    let entities: Vec<Value> =
        dedupe_preserve_order(ids).into_iter().map(|id| entity_info_one(cache_root, kind, id)).collect();
    json!({"entities": entities})
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- is_valid_tooltip_json ----------------------------------------------

    #[test]
    fn valid_tooltip_requires_brace_wrap_and_both_keys() {
        assert!(is_valid_tooltip_json(r#"{"name":"Foo","tooltip":"<b>Foo</b>","icon":"x"}"#));
        assert!(!is_valid_tooltip_json(r#"{"error":"no such item"}"#));
        assert!(!is_valid_tooltip_json(r#"{"name":"Foo"}"#)); // missing tooltip key
        assert!(!is_valid_tooltip_json(r#"{"tooltip":"x"}"#)); // missing name key
        assert!(!is_valid_tooltip_json("not json at all"));
        assert!(!is_valid_tooltip_json(r#"{"name":"Foo","tooltip":"x""#)); // no closing brace
        assert!(!is_valid_tooltip_json(""));
    }

    // --- extract_icon_name ---------------------------------------------------

    #[test]
    fn extract_icon_name_finds_first_match() {
        assert_eq!(
            extract_icon_name(r#"{"name":"X","icon":"inv_sword_39","tooltip":"y"}"#),
            Some("inv_sword_39".into())
        );
        assert_eq!(extract_icon_name(r#"{"icon":"a.b-c_D2"}"#), Some("a.b-c_D2".into()));
        assert_eq!(extract_icon_name(r#"{"name":"no icon key"}"#), None);
        assert_eq!(extract_icon_name(r#"{"icon":""}"#), None); // zero charset chars
        // First occurrence wins even if a second "icon" key appears later.
        assert_eq!(extract_icon_name(r#"{"icon":"first","other":{"icon":"second"}}"#), Some("first".into()));
    }

    // --- xml_looks_like_wowhead ----------------------------------------------

    #[test]
    fn xml_marker_gate() {
        assert!(xml_looks_like_wowhead("<wowhead><item><displayId>1</displayId></item></wowhead>"));
        // An unknown-item error body still counts if wrapped -- a definitive
        // "wowhead has no such item" answer IS cacheable.
        assert!(xml_looks_like_wowhead("<wowhead><error>no such item</error></wowhead>"));
        assert!(!xml_looks_like_wowhead("<html>cloudflare interstitial</html>"));
        assert!(!xml_looks_like_wowhead(""));
    }

    // --- display id extraction ------------------------------------------------

    #[test]
    fn extract_display_id_digits_finds_first_attribute() {
        assert_eq!(
            extract_display_id_digits(r#"<icon displayId="45150" someOther="x"/>"#),
            Some("45150".into())
        );
        assert_eq!(extract_display_id_digits(r#"<icon displayId="0"/>"#), Some("0".into()));
        assert_eq!(extract_display_id_digits("<wowhead><error/></wowhead>"), None);
    }

    #[test]
    fn extract_positive_display_id_rejects_zero_and_leading_zero() {
        assert_eq!(extract_positive_display_id(r#"displayId="45150""#), Some(45150));
        assert_eq!(extract_positive_display_id(r#"displayId="0""#), None);
        assert_eq!(extract_positive_display_id(r#"displayId="045""#), None); // leading zero
        assert_eq!(extract_positive_display_id("no attribute here"), None);
    }

    // --- stat_name -------------------------------------------------------------

    #[test]
    fn stat_name_table_matches_bash_verbatim() {
        assert_eq!(stat_name(3), "Agility");
        assert_eq!(stat_name(4), "Strength");
        assert_eq!(stat_name(5), "Intellect");
        assert_eq!(stat_name(6), "Spirit");
        assert_eq!(stat_name(7), "Stamina");
        assert_eq!(stat_name(12), "Defense Rating");
        assert_eq!(stat_name(13), "Dodge Rating");
        assert_eq!(stat_name(14), "Parry Rating");
        assert_eq!(stat_name(15), "Block Rating");
        assert_eq!(stat_name(31), "Hit Rating");
        assert_eq!(stat_name(32), "Critical Strike Rating");
        assert_eq!(stat_name(35), "Resilience Rating");
        assert_eq!(stat_name(36), "Haste Rating");
        assert_eq!(stat_name(37), "Expertise Rating");
        assert_eq!(stat_name(38), "Attack Power");
        assert_eq!(stat_name(43), "Mana per 5 sec.");
        assert_eq!(stat_name(45), "Spell Power");
        assert_eq!(stat_name(99), "Stat 99");
        assert_eq!(stat_name(0), "Stat 0");
    }

    // --- xml_escape --------------------------------------------------------

    #[test]
    fn xml_escape_amp_lt_gt_only_no_double_escape() {
        assert_eq!(xml_escape("Sword & Shield"), "Sword &amp; Shield");
        assert_eq!(xml_escape("<Rare>"), "&lt;Rare&gt;");
        assert_eq!(xml_escape("A & <B>"), "A &amp; &lt;B&gt;");
        assert_eq!(xml_escape("plain"), "plain");
    }

    // --- parse_local_row -----------------------------------------------------

    fn row_cells(
        name: &str,
        quality: i64,
        ilvl: i64,
        armor: i64,
        rlvl: i64,
        dmin: &str,
        dmax: &str,
        delay: i64,
        stats: [(i64, i64); 5],
    ) -> Vec<db::SqlValue> {
        let mut v = vec![
            db::SqlValue::Text(name.to_string()),
            db::SqlValue::Int(quality),
            db::SqlValue::Int(ilvl),
            db::SqlValue::Int(armor),
            db::SqlValue::Int(rlvl),
            db::SqlValue::Text(dmin.to_string()),
            db::SqlValue::Text(dmax.to_string()),
            db::SqlValue::Int(delay),
        ];
        for (t, val) in stats {
            v.push(db::SqlValue::Int(t));
            v.push(db::SqlValue::Int(val));
        }
        v
    }

    #[test]
    fn parse_local_row_decodes_all_18_columns() {
        let cells = row_cells(
            "Test Sword",
            3,
            60,
            0,
            55,
            "10",
            "20",
            2600,
            [(4, 5), (0, 0), (0, 0), (0, 0), (0, 0)],
        );
        let row = parse_local_row(&cells).unwrap();
        assert_eq!(row.name, "Test Sword");
        assert_eq!(row.quality, 3);
        assert_eq!(row.item_level, 60);
        assert_eq!(row.armor, 0);
        assert_eq!(row.required_level, 55);
        assert_eq!(row.dmg_min_text, "10");
        assert_eq!(row.dmg_max_text, "20");
        assert_eq!(row.dmg_max, 20.0);
        assert_eq!(row.delay, 2600);
        assert_eq!(row.stats, [(4, 5), (0, 0), (0, 0), (0, 0), (0, 0)]);
    }

    #[test]
    fn parse_local_row_null_cells_degrade_to_zero_or_empty() {
        let cells = vec![
            db::SqlValue::Null,
            db::SqlValue::Null,
            db::SqlValue::Null,
            db::SqlValue::Null,
            db::SqlValue::Null,
            db::SqlValue::Null,
            db::SqlValue::Null,
            db::SqlValue::Null,
            db::SqlValue::Null,
            db::SqlValue::Null,
            db::SqlValue::Null,
            db::SqlValue::Null,
            db::SqlValue::Null,
            db::SqlValue::Null,
            db::SqlValue::Null,
            db::SqlValue::Null,
            db::SqlValue::Null,
            db::SqlValue::Null,
        ];
        let row = parse_local_row(&cells).unwrap();
        assert_eq!(row.name, "");
        assert_eq!(row.quality, 0);
        assert_eq!(row.dmg_max, 0.0);
    }

    #[test]
    fn parse_local_row_short_row_is_none() {
        assert_eq!(parse_local_row(&[db::SqlValue::Text("x".into())]), None);
    }

    // --- build_local_tooltip_html --------------------------------------------

    #[test]
    fn local_tooltip_html_armor_piece() {
        let row = LocalItemRow {
            name: "Rusty Helm".into(),
            quality: 1,
            item_level: 10,
            armor: 25,
            required_level: 5,
            dmg_min_text: "0".into(),
            dmg_max_text: "0".into(),
            dmg_max: 0.0,
            delay: 0,
            stats: [(4, 3), (0, 0), (0, 0), (0, 0), (0, 0)],
        };
        let html = build_local_tooltip_html(&row);
        assert_eq!(
            html,
            "<b class=\"q1\">Rusty Helm</b><br><span class=\"q\">Item Level 10</span>\
<br><span class=\"q1\">25 Armor</span>\
<br><span class=\"q1\">+3 Strength</span>\
<br><span class=\"q1\">Requires Level 5</span>"
        );
    }

    #[test]
    fn local_tooltip_html_weapon_with_speed() {
        let row = LocalItemRow {
            name: "Test Axe".into(),
            quality: 2,
            item_level: 30,
            armor: 0,
            required_level: 0,
            dmg_min_text: "15".into(),
            dmg_max_text: "25".into(),
            dmg_max: 25.0,
            delay: 2600,
            stats: [(0, 0); 5],
        };
        let html = build_local_tooltip_html(&row);
        assert!(html.contains("<br><span class=\"q1\">15 - 25 Damage</span> <span class=\"q1\">Speed 2.60</span>"));
        assert!(!html.contains("Armor"));
        assert!(!html.contains("Requires Level"));
    }

    #[test]
    fn local_tooltip_html_skips_zero_stat_value_but_not_zero_stat_type() {
        // Bash's skip condition checks the VALUE (`"$sv" == 0`), never the
        // stat TYPE -- a (0, 5) pair still renders as "Stat 0".
        let row = LocalItemRow {
            name: "Odd Item".into(),
            quality: 0,
            item_level: 1,
            armor: 0,
            required_level: 0,
            dmg_min_text: "0".into(),
            dmg_max_text: "0".into(),
            dmg_max: 0.0,
            delay: 0,
            stats: [(4, 0), (0, 5), (7, 2), (0, 0), (0, 0)],
        };
        let html = build_local_tooltip_html(&row);
        assert!(!html.contains("Strength")); // (4, 0) skipped: value is zero
        assert!(html.contains("+5 Stat 0")); // (0, 5) kept: type 0, nonzero value
        assert!(html.contains("+2 Stamina"));
    }

    #[test]
    fn local_tooltip_html_escapes_name() {
        let row = LocalItemRow {
            name: "A & <B>".into(),
            quality: 0,
            item_level: 1,
            armor: 0,
            required_level: 0,
            dmg_min_text: "0".into(),
            dmg_max_text: "0".into(),
            dmg_max: 0.0,
            delay: 0,
            stats: [(0, 0); 5],
        };
        assert!(build_local_tooltip_html(&row).starts_with("<b class=\"q0\">A &amp; &lt;B&gt;</b>"));
    }

    // --- envelope builders -----------------------------------------------------

    #[test]
    fn local_item_json_unavailable_carries_optional_display_id() {
        assert_eq!(local_item_json(42, None, None), json!({"entry": 42, "source": "unavailable"}));
        assert_eq!(
            local_item_json(42, None, Some(45150)),
            json!({"entry": 42, "source": "unavailable", "display_id": 45150})
        );
    }

    #[test]
    fn local_item_json_local_row_shape() {
        let row = LocalItemRow {
            name: "Custom Blade".into(),
            quality: 4,
            item_level: 80,
            armor: 0,
            required_level: 0,
            dmg_min_text: "1".into(),
            dmg_max_text: "2".into(),
            dmg_max: 2.0,
            delay: 1000,
            stats: [(0, 0); 5],
        };
        let v = local_item_json(7, Some(&row), None);
        assert_eq!(v["entry"], 7);
        assert_eq!(v["source"], "local");
        assert_eq!(v["name"], "Custom Blade");
        assert_eq!(v["quality"], 4);
        assert!(v["tooltip_html"].as_str().unwrap().contains("Custom Blade"));
        assert!(v.get("display_id").is_none());
    }

    #[test]
    fn wowhead_item_json_shape_with_and_without_display_id() {
        let raw = json!({"name": "X", "quality": 1, "icon": "inv_x", "tooltip": "<b>X</b>"});
        let v = wowhead_item_json(9, Some("inv_x"), Some("YmFzZTY0"), &raw, None);
        assert_eq!(v["entry"], 9);
        assert_eq!(v["source"], "wowhead");
        assert_eq!(v["icon"], "inv_x");
        assert_eq!(v["icon_b64"], "YmFzZTY0");
        assert_eq!(v["wowhead"], raw);
        assert!(v.get("display_id").is_none());

        let v2 = wowhead_item_json(9, None, None, &raw, Some(123));
        assert_eq!(v2["icon"], Value::Null);
        assert_eq!(v2["icon_b64"], Value::Null);
        assert_eq!(v2["display_id"], 123);
    }

    #[test]
    fn wowhead_entity_json_shape_has_no_display_id_field() {
        let raw = json!({"name": "Fireball", "quality": 1, "icon": "spell_fire", "tooltip": "y"});
        let v = wowhead_entity_json(133, Some("spell_fire"), None, &raw);
        assert_eq!(v["id"], 133);
        assert_eq!(v["source"], "wowhead");
        assert!(v.get("display_id").is_none());
    }

    #[test]
    fn unavailable_entity_json_shape() {
        assert_eq!(unavailable_entity_json(5), json!({"id": 5, "source": "unavailable"}));
    }

    // --- dedupe_preserve_order --------------------------------------------------

    #[test]
    fn dedupe_preserve_order_drops_dupes_keeps_first_seen_order() {
        assert_eq!(dedupe_preserve_order(&[1, 2, 1, 3, 2, 4]), vec![1, 2, 3, 4]);
        assert_eq!(dedupe_preserve_order(&[]), Vec::<u32>::new());
        assert_eq!(dedupe_preserve_order(&[5]), vec![5]);
    }

    // --- fetch/cache glue (filesystem-only, no network) -------------------------

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("dml-iteminfo-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn fetch_or_load_text_reads_cache_without_network() {
        let dir = tmp("fetch-cache");
        let f = dir.join("tooltips").join("123.json");
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, r#"{"name":"Cached","tooltip":"x"}"#).unwrap();
        // The URL is deliberately bogus (unroutable) -- a cache hit must
        // never touch it.
        let got = fetch_or_load_text(&f, "http://127.0.0.1.invalid/should-not-be-fetched");
        assert_eq!(got.as_deref(), Some(r#"{"name":"Cached","tooltip":"x"}"#));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_icon_no_icon_key_is_none_none() {
        let dir = tmp("no-icon");
        std::fs::create_dir_all(&dir).unwrap();
        let (icon, b64) = resolve_icon(&dir, r#"{"name":"X","tooltip":"y"}"#);
        assert_eq!(icon, None);
        assert_eq!(b64, None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_icon_cache_hit_without_network() {
        let dir = tmp("icon-cache-hit");
        let icons = dir.join("icons");
        std::fs::create_dir_all(&icons).unwrap();
        std::fs::write(icons.join("inv_sword_39.jpg"), b"\xFF\xD8fakejpg").unwrap();
        let (icon, b64) = resolve_icon(&dir, r#"{"name":"X","icon":"inv_sword_39","tooltip":"y"}"#);
        assert_eq!(icon.as_deref(), Some("inv_sword_39"));
        assert_eq!(b64.as_deref(), Some(BASE64.encode(b"\xFF\xD8fakejpg").as_str()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_display_id_cache_hit_without_network() {
        let dir = tmp("did-cache-hit");
        let xml = dir.join("xml");
        std::fs::create_dir_all(&xml).unwrap();
        std::fs::write(xml.join("45150.xml"), r#"<wowhead><item><icon displayId="45150"/></item></wowhead>"#)
            .unwrap();
        assert_eq!(resolve_display_id(&dir, 45150), Some(45150));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_item_info_dedupes_and_falls_back_when_wowhead_unreachable() {
        // Points the wowhead/xml bases at an unroutable loopback port (fast
        // "connection refused", not a timeout) so this stays offline and
        // fast while still exercising the REAL fetch/cache glue (not just
        // dedupe_preserve_order in isolation), verifying both the dedup and
        // the end-to-end "no wowhead, no DB" -> unavailable envelope shape.
        let _guard = ENV_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tmp("read-item-info-dedupe");
        std::env::set_var("DML_WOWHEAD_BASE", "http://127.0.0.1:1");
        std::env::set_var("DML_WOWHEAD_XML_BASE", "http://127.0.0.1:1");
        let v = read_item_info(&dir, None, &[111, 111, 222]);
        std::env::remove_var("DML_WOWHEAD_BASE");
        std::env::remove_var("DML_WOWHEAD_XML_BASE");
        let items = v["items"].as_array().unwrap();
        assert_eq!(items.len(), 2, "duplicate entry must be dropped");
        assert_eq!(items[0]["entry"], 111);
        assert_eq!(items[0]["source"], "unavailable");
        assert_eq!(items[1]["entry"], 222);
        assert_eq!(items[1]["source"], "unavailable");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // --- skip-guarded live smoke tests (real network required) ------------------

    // Serializes every test in this file that mutates the process-global
    // DML_WOWHEAD_BASE/DML_WOWHEAD_XML_BASE env vars (the offline dedupe
    // test above, plus the live smoke tests below) -- cargo runs tests in
    // this binary on multiple threads, and an unguarded set_var here would
    // otherwise be able to poison a concurrently-running live test's base
    // URL (or vice versa), producing flaky failures unrelated to either
    // test's own assertions.
    static ENV_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn internet_reachable() -> bool {
        std::net::TcpStream::connect_timeout(
            &"1.1.1.1:443".parse().unwrap(),
            Duration::from_millis(500),
        )
        .is_ok()
    }

    /// Live smoke: a well-known WotLK item (25 = Worn Shortsword) should
    /// resolve from wowhead with an icon and a display_id. Skipped
    /// (not failed) when there's no live internet on this box.
    #[test]
    fn live_item_info_known_item_resolves_from_wowhead() {
        let _guard = ENV_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        if !internet_reachable() {
            eprintln!("skipping live_item_info_known_item_resolves_from_wowhead: no internet");
            return;
        }
        let dir = tmp("live-item");
        let v = item_info_one(&dir, None, 25);
        assert_eq!(v["source"], "wowhead", "item 25 should be known to wowhead: {v}");
        assert!(v["wowhead"]["name"].is_string());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Live smoke: a well-known spell (133 = Fireball) should resolve.
    #[test]
    fn live_entity_info_known_spell_resolves_from_wowhead() {
        let _guard = ENV_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        if !internet_reachable() {
            eprintln!("skipping live_entity_info_known_spell_resolves_from_wowhead: no internet");
            return;
        }
        let dir = tmp("live-entity");
        let v = entity_info_one(&dir, "spell", 133);
        assert_eq!(v["source"], "wowhead", "spell 133 should be known to wowhead: {v}");
        assert!(v["wowhead"]["name"].is_string());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Live smoke: an id with no chance of existing on wowhead resolves to
    /// "unavailable" for an entity (no DB fallback exists for entities).
    #[test]
    fn live_entity_info_unknown_id_is_unavailable() {
        let _guard = ENV_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        if !internet_reachable() {
            eprintln!("skipping live_entity_info_unknown_id_is_unavailable: no internet");
            return;
        }
        let dir = tmp("live-entity-unknown");
        let v = entity_info_one(&dir, "spell", 999_999_999);
        assert_eq!(v["source"], "unavailable");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
