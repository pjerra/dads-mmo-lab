//! Generic conf-file engine — moved out of the launcher's `dml::config`
//! (cargo-workspace refactor, Task 5). This is the game-agnostic HALF of that
//! module's value-read/value-write logic: parsing `Key = value` conf files and
//! writing them back with BYTE-PARITY to the bash CLI's oracle behavior.
//! Everything AzerothCore-specific (env-var naming, `worldserver.conf`/
//! module-conf routing, `ConfigReader`, the direct-conf-key validators, the
//! live-reload command allowlist, and — since Task 7 — the compose-override
//! env reader/writers, which hardcode the `ac-worldserver` service key) lives
//! in `dml_wow::config`.
//!
//! `dml_wow::config` re-exports every name below (`pub use
//! dml_core::conf::{…};`) so every existing call site keeps compiling
//! unchanged, whether it reaches these via a bare import or a fully-qualified
//! `config::`-prefixed path.
//!
//! Byte-parity is LOAD-BEARING for `conf_write`: a live parity test compares
//! its output against a bash/awk oracle, so nothing here should be
//! "improved" — only ported, verbatim, out of the old module.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Strip the surrounding quotes off a raw conf value the way the `config list`
/// read path does — `_cfg_conf_read` / `_cfg_conf_get_var` (40-config.sh:377,
/// 429) run `${v%\"}` then `${v#\"}`: remove at most ONE trailing `"`, then at
/// most ONE leading `"`, INDEPENDENTLY (not a matched-pair check). A lone or
/// unbalanced quote is therefore trimmed on that side only — reproduced exactly.
pub fn strip_conf_quotes(v: &str) -> String {
    let v = v.strip_suffix('"').unwrap_or(v);
    let v = v.strip_prefix('"').unwrap_or(v);
    v.to_string()
}

/// Parse a conf file's active `Key = value` lines into a map (last occurrence
/// wins, AC semantics), value RAW (quotes preserved). A port of
/// `_cfg_conf_load_file` (40-config.sh:404): a line qualifies when — after
/// stripping a trailing CR and leading blanks — it is `<Key>[blanks]= …` with
/// `<Key>` drawn from `[A-Za-z0-9_.]`. Key = the token before the first `=`
/// (trailing blanks trimmed); value = everything after the first `=`, trimmed of
/// leading and trailing blanks (space/tab only — never newlines).
pub fn parse_conf(content: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for raw_line in content.split('\n') {
        // Strip a single trailing CR (\r\n line ends); leave other bytes.
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        // Skip leading blanks (space/tab).
        let s = line.trim_start_matches([' ', '\t']);
        // Gate: one-or-more key chars, optional blanks, then '='.
        let key_len = s
            .char_indices()
            .find(|(_, c)| !(c.is_ascii_alphanumeric() || *c == '_' || *c == '.'))
            .map(|(i, _)| i)
            .unwrap_or(s.len());
        if key_len == 0 {
            continue; // no key chars at column 0 -> not an assignment
        }
        let key = &s[..key_len];
        let after = s[key_len..].trim_start_matches([' ', '\t']);
        let Some(value_part) = after.strip_prefix('=') else {
            continue; // key chars not followed by '=' -> not an assignment
        };
        let value = value_part.trim_matches([' ', '\t']);
        map.insert(key.to_string(), value.to_string());
    }
    map
}

// ---------------------------------------------------------------------------
// WRITE side (Task B1): comment-preserving conf-file edits. `conf_write` is
// BYTE-PARITY with `_cfg_conf_write` (comments/spacing/CRLF matter —
// hand-edited files). The semantic-parity override-YAML env writers that used
// to sit here moved to `dml_wow::config` in Task 7 (they hardcode the
// `ac-worldserver` compose service key).
// ---------------------------------------------------------------------------

/// Strip ONE MATCHED pair of surrounding double quotes — a port of
/// `_cfg_unquote` (40-config.sh:339). Requires BOTH ends to be `"` AND
/// `len >= 2`; an unbalanced value (`"3` or `3"`) is left untouched. This is
/// deliberately NOT the same rule as `strip_conf_quotes` (which trims each
/// side independently, for the READ path's `${v%\"}`/`${v#\"}`) — the WRITE
/// side's cur/new compare needs the stricter matched-pair rule the bash
/// oracle actually uses.
fn unquote_conf_matched(s: &str) -> &str {
    let b = s.as_bytes();
    if b.len() >= 2 && b[0] == b'"' && b[b.len() - 1] == b'"' {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

/// RAW (quote-preserved) value of `key` in a conf file — a port of
/// `_cfg_conf_read_raw` (40-config.sh:352). `""` when the file or key is
/// absent. Unlike `parse_conf` (whose key gate is a regex character class),
/// this matches the awk oracle's `index(s, k) == 1` — a LITERAL PREFIX test,
/// so a `.` in `key` is a literal dot, never a wildcard. Last matching line
/// wins. Used only by `conf_write`, to see the currently-stored value.
fn conf_read_raw(path: &Path, key: &str) -> String {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return String::new(),
    };
    let mut val = String::new();
    for raw_line in content.split('\n') {
        let s = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        let s = s.trim_start_matches([' ', '\t']);
        let Some(rest) = s.strip_prefix(key) else { continue };
        let rest = rest.trim_start_matches([' ', '\t']);
        let Some(v) = rest.strip_prefix('=') else { continue };
        val = v.trim_matches([' ', '\t']).to_string();
    }
    val
}

/// Atomic write: write `content` to a sibling temp file
/// (`{path}.tmp.{pid}.{seq}`), then rename it over `path`. Any error removes
/// the temp file and propagates — the original is never truncated or left
/// half-written. Shared by `conf_write` and the override-YAML writers.
///
/// The bash oracle's equivalent tmp name (`$1.tmp.$$`) is naturally unique
/// per call because every `dml wow config set` invocation forks a fresh bash
/// process. This app is long-lived — every conf-write shares ONE pid for the
/// whole session — so the pid alone would let two concurrent writes to the
/// same conf file collide on the same tmp path. `NEXT_TMP_SEQ` restores
/// per-call uniqueness without relying on the pid at all.
static NEXT_TMP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

// `pub` (not `pub(crate)`, its pre-move visibility inside launcher's
// `dml::config`): `launcher/src-tauri/src/lib.rs`'s `wow_config_raw_write_native`
// calls this directly (not just through `conf_write`), and so do
// `dml_wow::config`'s override-YAML writers, so it must cross the crate
// boundary. `dml_wow::config` re-exports it under the same name.
pub fn atomic_write(path: &Path, content: &str) -> std::io::Result<()> {
    let seq = NEXT_TMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut tmp_os = path.as_os_str().to_os_string();
    tmp_os.push(format!(".tmp.{}.{}", std::process::id(), seq));
    let tmp_path = PathBuf::from(tmp_os);
    if let Err(e) = std::fs::write(&tmp_path, content) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(e);
    }
    if let Err(e) = std::fs::rename(&tmp_path, path) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(e);
    }
    Ok(())
}

/// Comment-preserving in-place edit of an AC `.conf`. Returns `Ok(changed)`.
/// Byte-parity target: `_cfg_conf_write` (40-config.sh:448), the awk oracle.
///
/// 1. Read the CURRENT raw (quote-preserved) value via `conf_read_raw`.
/// 2. Unquote both current and new value with the MATCHED-pair rule
///    (`unquote_conf_matched`, i.e. `_cfg_unquote`).
/// 3. No-op (`Ok(false)`, no write at all) when the unquoted values are
///    equal — a pure quote-toggle write never touches the file.
/// 4. The output value is (re-)quoted iff the caller passed a quoted
///    `value` OR the stored line was quoted — so a legitimate edit that
///    needs quoting (spaces etc.) never silently loses it.
/// 5. Rewrite every record: the file is split on `'\n'` with a single
///    trailing empty record dropped (awk's per-record `ORS` model, not
///    `str::lines()` — this is what makes CRLF / no-trailing-newline byte
///    parity work). A record whose trimmed form is `<key>[blanks]=...`
///    (literal prefix, exactly like `conf_read_raw`) is replaced by the
///    canonical `"{key} = {out_val}"` (every matching duplicate rewritten,
///    no `\r`); every other record is emitted byte-for-byte unchanged
///    (keeps its own leading whitespace and trailing `\r`). If the key was
///    never matched, the canonical line is appended. Output always ends
///    with exactly one `'\n'`.
/// 6. Atomic tmp+rename write (`atomic_write`).
pub fn conf_write(path: &Path, key: &str, value: &str) -> std::io::Result<bool> {
    let curq = conf_read_raw(path, key);
    let cur = unquote_conf_matched(&curq);
    let newq = value;
    let new = unquote_conf_matched(newq);
    if cur == new {
        return Ok(false);
    }
    let out_val = if newq != new || curq.as_str() != cur {
        format!("\"{new}\"")
    } else {
        new.to_string()
    };

    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };

    // Awk's record model: split on '\n', dropping a single trailing empty
    // record when the content ends in '\n' (so "a\n" is ONE record, not
    // ["a", ""]). A totally empty file has ZERO records, not one phantom
    // blank record.
    let recs: Vec<&str> = if content.is_empty() {
        Vec::new()
    } else {
        let mut v: Vec<&str> = content.split('\n').collect();
        if content.ends_with('\n') {
            v.pop();
        }
        v
    };

    let canonical = format!("{key} = {out_val}");
    let mut done = false;
    let mut out_lines: Vec<&str> = Vec::with_capacity(recs.len() + 1);
    for &rec in &recs {
        let s = rec.strip_suffix('\r').unwrap_or(rec);
        let s = s.trim_start_matches([' ', '\t']);
        let is_match = match s.strip_prefix(key) {
            Some(rest) => rest.trim_start_matches([' ', '\t']).starts_with('='),
            None => false,
        };
        if is_match {
            out_lines.push(canonical.as_str());
            done = true;
        } else {
            out_lines.push(rec);
        }
    }
    if !done {
        out_lines.push(canonical.as_str());
    }

    let mut out_content = out_lines.join("\n");
    out_content.push('\n');

    atomic_write(path, &out_content)?;
    Ok(true)
}

/// Single-line gate for a direct-route conf value (`90-main.sh:2389-2391`):
/// rejects any value containing `\n` or `\r`.
pub fn is_single_line(value: &str) -> bool {
    !value.contains('\n') && !value.contains('\r')
}

/// Max-length gate for a direct-route conf value (`90-main.sh:2392-2394`):
/// `<= 200` chars (matches bash `${#value}`, a character count).
pub fn within_max_len(value: &str, max: usize) -> bool {
    value.chars().count() <= max
}

/// Sibling `<path>.dist` — the `.dist` half of the raw-read/raw-reset/
/// raw-write allowlist checks (`90-main.sh:2701,2724,2746` etc, all of which
/// spell `"$fpath.dist"` inline). Factored out so every call site derives the
/// same path the same way.
pub fn dist_sibling(path: &Path) -> PathBuf {
    let mut os = path.as_os_str().to_os_string();
    os.push(".dist");
    PathBuf::from(os)
}

/// Sibling `<path>.bak` — the automatic backup raw-reset/raw-write take
/// before overwriting an existing file (`90-main.sh:2727,2769`: `cp -p
/// "$fpath" "$fpath.bak"`).
pub fn bak_sibling(path: &Path) -> PathBuf {
    let mut os = path.as_os_str().to_os_string();
    os.push(".bak");
    PathBuf::from(os)
}

// ---------------------------------------------------------------------------
// pb-keys / conf-keys (Part 5a): the searchable all-keys browsers. Shared
// row-shape logic + the `_conf_help_lines` comment-doc extractor.
// ---------------------------------------------------------------------------

/// Whether leading-blank-trimmed `s` starts with `<key>[ \t]*=` — the ONE
/// key-detection gate every awk helper in `40-config.sh` shares
/// (`_cfg_conf_load_file`/`_pb_kv_lines`/`_conf_help_lines`'s
/// `^[A-Za-z0-9_.]+[ \t]*=`). Returns `(key, value_after_eq)`, where
/// `value_after_eq` still carries ITS OWN leading/trailing blanks — callers
/// trim per their own needs (some want the raw value, `_conf_help_lines`
/// never needs it at all).
fn match_key_eq(s: &str) -> Option<(&str, &str)> {
    let key_len = s
        .char_indices()
        .find(|(_, c)| !(c.is_ascii_alphanumeric() || *c == '_' || *c == '.'))
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    if key_len == 0 {
        return None;
    }
    let key = &s[..key_len];
    let after = s[key_len..].trim_start_matches([' ', '\t']);
    let value = after.strip_prefix('=')?;
    Some((key, value))
}

/// Every active `Key = value` line as `(key, value, 1-based line number)` —
/// a port of `_pb_kv_lines` (`40-config.sh:484-500`) PLUS the pb-keys/
/// conf-keys arms' own dedup loop (`90-main.sh:2581-2584`/`2641-2644`):
/// duplicate keys keep their FIRST position in the returned `Vec`, but the
/// LAST occurrence's value/line wins (bash associative-array overwrite,
/// walked in file order).
pub fn kv_rows(content: &str) -> Vec<(String, String, usize)> {
    let mut order: Vec<String> = Vec::new();
    let mut vals: HashMap<String, (String, usize)> = HashMap::new();
    for (idx, raw_line) in content.split('\n').enumerate() {
        let lineno = idx + 1;
        let s = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        let s = s.trim_start_matches([' ', '\t']);
        let Some((key, after_eq)) = match_key_eq(s) else { continue };
        let value = after_eq.trim_matches([' ', '\t']).to_string();
        if !vals.contains_key(key) {
            order.push(key.to_string());
        }
        vals.insert(key.to_string(), (value, lineno));
    }
    order
        .into_iter()
        .map(|k| {
            let (v, l) = vals.remove(&k).expect("key was just pushed to order from the same map");
            (k, v, l)
        })
        .collect()
}

/// One row of the pb-keys/conf-keys browser.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyBrowserRow {
    pub key: String,
    pub value: String,
    pub default: Option<String>,
    pub line: usize,
}

/// Shared row assembly for pb-keys (`90-main.sh:2562-2606`) and conf-keys
/// (`90-main.sh:2607-2669`) — the two arms are otherwise IDENTICAL in this
/// part of their logic (conf-keys additionally attaches `help`, assembled
/// separately). `src_content` is the file actually being shown (the live
/// conf if it exists, else its `.dist`); `src_is_dist` is whether that shown
/// file IS the `.dist` (no live conf yet). Default-value derivation
/// (`dv=`/`90-main.sh:2592-2597,2657-2662`): when `src_is_dist`, default
/// equals the row's OWN value (nothing else to compare against); otherwise
/// default is the SAME key's value in `dist_content` (only supplied by the
/// caller when the `.dist` actually exists on disk) — `None` when that key
/// is absent from the dist (bash's `${_pb_def[$k]+x}` EXISTENCE test, not a
/// non-empty check: a dist row with an empty value still counts as present).
pub fn key_browser_rows(src_content: &str, dist_content: Option<&str>, src_is_dist: bool) -> Vec<KeyBrowserRow> {
    let dist_map: HashMap<String, String> = match dist_content {
        Some(c) => kv_rows(c).into_iter().map(|(k, v, _)| (k, v)).collect(),
        None => HashMap::new(),
    };
    kv_rows(src_content)
        .into_iter()
        .map(|(key, value, line)| {
            let default =
                if src_is_dist { Some(value.clone()) } else { dist_map.get(&key).cloned() };
            KeyBrowserRow { key, value, default, line }
        })
        .collect()
}

/// Per-key comment-block help text, in file order — a faithful port of
/// `_conf_help_lines` (`40-config.sh:517-570`), the conf-keys browser's doc
/// extractor. Two documentation styles both feed the SAME output:
///
///  - an ADJACENT block directly above a key (blank lines between block and
///    key are fine; a fresh block after a blank/other line never bleeds
///    into a later one — see `buf` resets below);
///  - a SHARED doc block where one contiguous comment run documents many
///    keys, each introduced by its own `#    Key.Name` header line (the
///    `hdr` map) — this wins over the adjacent block when both exist for
///    the same key, and can appear ANYWHERE in the file relative to the
///    key's own assignment line (even after it), since both passes scan the
///    WHOLE file.
///
/// Decoration-only comment lines (bare `#`, `####`) contribute nothing;
/// real key names cannot double as prose (pass 1 first collects every key
/// this file actually assigns, so a header-shaped comment only counts as a
/// header when it names one of THOSE keys). Each help string is squeezed
/// (whitespace runs -> one space), capped at 400 chars, and keys with no
/// help at all are omitted from the result.
pub fn conf_help_lines(content: &str) -> Vec<(String, String)> {
    // Pass 1: the keys this conf actually assigns, in first-seen order.
    let mut korder: Vec<String> = Vec::new();
    let mut keyseen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for raw_line in content.split('\n') {
        let s = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        let s = s.trim_start_matches([' ', '\t']);
        if let Some((key, _)) = match_key_eq(s) {
            if keyseen.insert(key.to_string()) {
                korder.push(key.to_string());
            }
        }
    }

    // Pass 2: adjacent blocks (`buf`) + per-key header slices (`hdr`).
    #[derive(PartialEq)]
    enum Prev {
        Start,
        Comment,
        Blank,
        Key,
        Other,
    }
    let mut prev = Prev::Start;
    let mut buf = String::new();
    let mut cap = String::new();
    let mut hdr: HashMap<String, String> = HashMap::new();
    let mut adj: HashMap<String, String> = HashMap::new();

    for raw_line in content.split('\n') {
        let s = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        let t = s.trim_start_matches([' ', '\t']);
        if let Some(hashes_stripped) = Some(t).filter(|t| t.starts_with('#')) {
            let txt = comment_text(hashes_stripped);
            if prev != Prev::Comment {
                buf.clear();
            }
            if keyseen.contains(&txt) {
                cap = txt;
            } else if !txt.is_empty() && txt.chars().any(|c| c.is_ascii_alphanumeric()) {
                if !cap.is_empty() {
                    let e = hdr.entry(cap.clone()).or_default();
                    if e.is_empty() {
                        *e = txt.clone();
                    } else {
                        e.push(' ');
                        e.push_str(&txt);
                    }
                }
                if buf.is_empty() {
                    buf = txt;
                } else {
                    buf.push(' ');
                    buf.push_str(&txt);
                }
            }
            prev = Prev::Comment;
        } else if t.is_empty() {
            cap.clear();
            prev = Prev::Blank;
        } else if let Some((key, _)) = match_key_eq(t) {
            if !buf.is_empty() {
                adj.insert(key.to_string(), buf.clone());
            }
            buf.clear();
            cap.clear();
            prev = Prev::Key;
        } else {
            buf.clear();
            cap.clear();
            prev = Prev::Other;
        }
    }

    let mut out = Vec::new();
    for k in korder {
        let mut h = hdr.get(&k).cloned().unwrap_or_default();
        if h.is_empty() {
            h = adj.get(&k).cloned().unwrap_or_default();
        }
        if h.chars().count() > 400 {
            h = h.chars().take(400).collect();
        }
        if !h.is_empty() {
            out.push((k, h));
        }
    }
    out
}

/// Comment-text extraction for one `#`-led line — `sub(/^#+[ \t]*/,"",txt)`
/// then `sub(/[ \t]+$/,"",txt)` then `gsub(/[ \t]+/," ",txt)`: strip the
/// leading run of `#`s and any blanks right after, trim trailing blanks,
/// then squeeze every remaining internal blank run to one space.
fn comment_text(t: &str) -> String {
    let after_hash = t.trim_start_matches('#');
    let after_hash = after_hash.trim_start_matches([' ', '\t']);
    let trimmed = after_hash.trim_end_matches([' ', '\t']);
    let mut out = String::with_capacity(trimmed.len());
    let mut prev_space = false;
    for c in trimmed.chars() {
        if c == ' ' || c == '\t' {
            if !prev_space {
                out.push(' ');
            }
            prev_space = true;
        } else {
            out.push(c);
            prev_space = false;
        }
    }
    out
}

/// Port of `_cfg_conf_ensure` (`40-config.sh:326-334`): seed the live conf
/// from its `.dist` (`cp {p}.dist {p}`) when only the `.dist` exists. Returns
/// `Ok(true)` when the conf exists afterward (already there, or just seeded),
/// `Ok(false)` when neither the conf nor its `.dist` exists (nothing to seed
/// from — the caller reports `NOT_FOUND`, never a write attempt).
pub fn conf_ensure(path: &Path) -> std::io::Result<bool> {
    if path.exists() {
        return Ok(true);
    }
    let mut dist_os = path.as_os_str().to_os_string();
    dist_os.push(".dist");
    let dist = PathBuf::from(dist_os);
    if !dist.exists() {
        return Ok(false);
    }
    std::fs::copy(&dist, path)?;
    Ok(true)
}

/// Shape-gated unsigned-decimal range check for `type: float` registry rows —
/// a port of `_float_in_range` (`40-config.sh:630-633`): the value must match
/// `^[0-9]+([.][0-9]+)?$` (no sign, no exponent) before it is even parsed,
/// then must fall within `[min, max]`.
pub fn float_in_range(value: &str, min: f64, max: f64) -> bool {
    if !is_unsigned_decimal_shape(value) {
        return false;
    }
    match value.parse::<f64>() {
        Ok(v) => v >= min && v <= max,
        Err(_) => false,
    }
}

fn is_unsigned_decimal_shape(s: &str) -> bool {
    let mut parts = s.splitn(2, '.');
    let int_part = parts.next().unwrap_or("");
    if int_part.is_empty() || !int_part.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    if let Some(frac) = parts.next() {
        if frac.is_empty() || !frac.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }
    }
    true
}

/// Shape-gated unsigned-integer range check for `type: int` registry rows —
/// a port of the `set)` case's int arm (`90-main.sh:2450-2452`): the value
/// must match `^[0-9]+$` then fall within `[min, max]`.
pub fn int_in_range(value: &str, min: i64, max: i64) -> bool {
    if value.is_empty() || !value.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    match value.parse::<i64>() {
        Ok(v) => v >= min && v <= max,
        Err(_) => false,
    }
}

/// `type: bool` registry-row shape check — a port of the `set)` case's bool
/// arm (`90-main.sh:2454-2456`): exactly `"0"` or `"1"`.
pub fn is_bool01(value: &str) -> bool {
    value == "0" || value == "1"
}

/// `type: text` registry-row sanitizer — a port of the `set)` case's text arm
/// (`90-main.sh:2458-2460`): every `"` is REMOVED (not escaped), and every
/// `\n`/`\r` becomes a single space.
pub fn sanitize_text_value(value: &str) -> String {
    value
        .chars()
        .filter(|&c| c != '"')
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_conf_quotes_is_independent_single_strip() {
        assert_eq!(strip_conf_quotes("\"3\""), "3");
        assert_eq!(strip_conf_quotes("3"), "3");
        // Unbalanced: one side only, matching the bash `%\"` then `#\"`.
        assert_eq!(strip_conf_quotes("\"3"), "3");
        assert_eq!(strip_conf_quotes("3\""), "3");
        // Inner quotes untouched.
        assert_eq!(strip_conf_quotes("\"a\"b\""), "a\"b");
        assert_eq!(strip_conf_quotes(""), "");
    }

    #[test]
    fn parse_conf_matches_load_file_semantics() {
        let content = "\
# a comment
Rate.XP.Kill = 3\r
  Indented.Key\t=\t\"quoted value\"
Not A Key = skip
Dup.Key = first
Dup.Key = second
Blank.Val =
NoEquals.Line
=leadingeq
";
        let m = parse_conf(content);
        assert_eq!(m.get("Rate.XP.Kill").map(String::as_str), Some("3"));
        // Leading blanks stripped; key trimmed; value trimmed but quotes RAW.
        assert_eq!(m.get("Indented.Key").map(String::as_str), Some("\"quoted value\""));
        // "Not A Key" has a space inside the key region -> not an assignment.
        assert_eq!(m.get("Not"), None);
        assert_eq!(m.get("Not A Key"), None);
        // Last occurrence wins.
        assert_eq!(m.get("Dup.Key").map(String::as_str), Some("second"));
        // Empty value is a valid assignment.
        assert_eq!(m.get("Blank.Val").map(String::as_str), Some(""));
        // Non-assignment lines never appear.
        assert_eq!(m.get("NoEquals.Line"), None);
        assert!(!m.contains_key(""));
    }

    // -- conf_write ----------------------------------------------------

    /// Fresh scratch file per test, auto-removed on drop.
    struct TmpConf(PathBuf);
    impl TmpConf {
        fn new(name: &str, content: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("dml-cfgwrite-test-{}-{}", std::process::id(), name));
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join("test.conf");
            std::fs::write(&path, content).unwrap();
            TmpConf(path)
        }
        fn read(&self) -> String {
            std::fs::read_to_string(&self.0).unwrap()
        }
    }
    impl Drop for TmpConf {
        fn drop(&mut self) {
            if let Some(dir) = self.0.parent() {
                let _ = std::fs::remove_dir_all(dir);
            }
        }
    }

    #[test]
    fn conf_write_existing_key_sets_new_value() {
        let f = TmpConf::new(
            "existing",
            "# a comment\nRate.XP.Kill = 1\nRate.Honor = 1\n",
        );
        let changed = conf_write(&f.0, "Rate.XP.Kill", "2").unwrap();
        assert!(changed);
        assert_eq!(
            f.read(),
            "# a comment\nRate.XP.Kill = 2\nRate.Honor = 1\n"
        );
    }

    #[test]
    fn conf_write_pure_quote_toggle_is_noop() {
        let f = TmpConf::new("quotetoggle", "Foo = \"bar\"\nOther = 1\n");
        let before = f.read();
        let changed = conf_write(&f.0, "Foo", "bar").unwrap();
        assert!(!changed);
        assert_eq!(f.read(), before, "file must be byte-identical after a no-op");
        // No tmp file left behind.
        let mut tmp_os = f.0.as_os_str().to_os_string();
        tmp_os.push(format!(".tmp.{}", std::process::id()));
        assert!(!PathBuf::from(tmp_os).exists());
    }

    #[test]
    fn conf_write_quotes_new_value_when_caller_quotes() {
        let f = TmpConf::new("quotepres", "Foo = bar\n");
        let changed = conf_write(&f.0, "Foo", "\"baz qux\"").unwrap();
        assert!(changed);
        assert_eq!(f.read(), "Foo = \"baz qux\"\n");
    }

    #[test]
    fn conf_write_appends_when_key_absent() {
        let f = TmpConf::new("append", "Existing.Key = 1\n");
        let changed = conf_write(&f.0, "NewKey", "5").unwrap();
        assert!(changed);
        assert_eq!(f.read(), "Existing.Key = 1\nNewKey = 5\n");
    }

    #[test]
    fn conf_write_duplicate_active_lines_both_rewritten() {
        let f = TmpConf::new(
            "dup",
            "Rate.XP.Kill = 1\nOther = 1\nRate.XP.Kill = 1\n",
        );
        let changed = conf_write(&f.0, "Rate.XP.Kill", "2").unwrap();
        assert!(changed);
        assert_eq!(
            f.read(),
            "Rate.XP.Kill = 2\nOther = 1\nRate.XP.Kill = 2\n"
        );
    }

    #[test]
    fn conf_write_weird_spacing_rewritten_canonical() {
        let f = TmpConf::new("spacing", "  Rate.XP.Kill=1\nOther = 1\n");
        let changed = conf_write(&f.0, "Rate.XP.Kill", "2").unwrap();
        assert!(changed);
        assert_eq!(f.read(), "Rate.XP.Kill = 2\nOther = 1\n");
    }

    #[test]
    fn conf_write_crlf_line_loses_cr_others_keep_it() {
        let f = TmpConf::new("crlf", "A = x\r\nFoo = 1\r\nB = y\r\n");
        let changed = conf_write(&f.0, "Foo", "2").unwrap();
        assert!(changed);
        assert_eq!(f.read(), "A = x\r\nFoo = 2\nB = y\r\n");
    }

    #[test]
    fn conf_write_no_trailing_newline_gains_one() {
        let f = TmpConf::new("notrail", "A = 1");
        let changed = conf_write(&f.0, "A", "2").unwrap();
        assert!(changed);
        assert_eq!(f.read(), "A = 2\n");
    }

    #[test]
    fn conf_write_missing_file_creates_it_with_appended_line() {
        let dir = std::env::temp_dir()
            .join(format!("dml-cfgwrite-test-{}-missing", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("absent.conf");
        assert!(!path.exists());
        let changed = conf_write(&path, "NewKey", "5").unwrap();
        assert!(changed);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "NewKey = 5\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Regression for the shared-pid tmp-name collision (`atomic_write` used
    /// to name its scratch file `{path}.tmp.{pid}` only — constant for the
    /// whole process — so two concurrent writers to the SAME conf file raced
    /// on the SAME tmp path). Fires many overlapping writers at one file from
    /// different threads (all sharing this process's pid, exactly like two
    /// Tauri commands would) and asserts every completed write left the file
    /// holding exactly one clean, complete value — never empty, truncated, or
    /// a mix of two writers' bytes.
    #[test]
    fn conf_write_concurrent_writers_never_tear_the_file() {
        let dir = std::env::temp_dir()
            .join(format!("dml-cfgwrite-test-{}-concurrent", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = std::sync::Arc::new(dir.join("racy.conf"));
        std::fs::write(&*path, "A = 0\n").unwrap();

        let handles: Vec<_> = (0..8)
            .map(|i| {
                let path = std::sync::Arc::clone(&path);
                std::thread::spawn(move || {
                    for n in 0..25 {
                        let _ = conf_write(&path, "A", &format!("{}", i * 100 + n));
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }

        let text = std::fs::read_to_string(&*path).unwrap();
        // Exactly one well-formed "A = <n>" line, never torn/duplicated/empty.
        let lines: Vec<&str> = text.split('\n').filter(|l| !l.is_empty()).collect();
        assert_eq!(lines.len(), 1, "file was torn by a racing writer: {text:?}");
        assert!(
            regex_like_a_equals_number(lines[0]),
            "final line is not a clean `A = <number>`: {:?}",
            lines[0]
        );
        // No leftover tmp files (every writer's tmp got renamed or cleaned up).
        let leftover: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp."))
            .collect();
        assert!(leftover.is_empty(), "leftover tmp files: {leftover:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Minimal shape check for the concurrency test above — no regex crate in
    /// this workspace, so just walk the expected `A = <digits>` shape by hand.
    fn regex_like_a_equals_number(line: &str) -> bool {
        let Some(rest) = line.strip_prefix("A = ") else { return false };
        !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit())
    }

    // -- B2a: direct-route (`conf:`) validation gates --------------------

    #[test]
    fn single_line_and_max_len_gates() {
        assert!(is_single_line("plain value"));
        assert!(!is_single_line("line1\nline2"));
        assert!(!is_single_line("carriage\rreturn"));
        assert!(within_max_len(&"x".repeat(200), 200));
        assert!(!within_max_len(&"x".repeat(201), 200));
    }

    #[test]
    fn conf_ensure_seeds_from_dist_once_and_reports_neither_present() {
        let dir = std::env::temp_dir().join(format!("dml-confensure-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let live = dir.join("mod_foo.conf");
        let dist = dir.join("mod_foo.conf.dist");

        // Neither exists.
        assert_eq!(conf_ensure(&live).unwrap(), false);

        // Only .dist -> seeded.
        std::fs::write(&dist, "AiPlayerbot.Foo = 1\n").unwrap();
        assert_eq!(conf_ensure(&live).unwrap(), true);
        assert_eq!(std::fs::read_to_string(&live).unwrap(), "AiPlayerbot.Foo = 1\n");

        // Already exists -> no-op true, dist untouched.
        std::fs::write(&live, "AiPlayerbot.Foo = 2\n").unwrap();
        assert_eq!(conf_ensure(&live).unwrap(), true);
        assert_eq!(std::fs::read_to_string(&live).unwrap(), "AiPlayerbot.Foo = 2\n");

        let _ = std::fs::remove_dir_all(&dir);
    }

    // -- B2a: curated-row value validation -------------------------------

    #[test]
    fn float_in_range_rejects_bad_shape_and_out_of_range() {
        assert!(float_in_range("3", 0.5, 20.0));
        assert!(float_in_range("0.5", 0.5, 20.0));
        assert!(float_in_range("20", 0.5, 20.0));
        assert!(!float_in_range("20.1", 0.5, 20.0));
        assert!(!float_in_range("0.4", 0.5, 20.0));
        // Shape rejects: sign, exponent, trailing dot, empty.
        assert!(!float_in_range("-1", 0.5, 20.0));
        assert!(!float_in_range("1e5", 0.5, 20.0));
        assert!(!float_in_range("1.", 0.5, 20.0));
        assert!(!float_in_range("", 0.5, 20.0));
    }

    #[test]
    fn int_in_range_rejects_bad_shape_and_out_of_range() {
        assert!(int_in_range("500", 0, 3000));
        assert!(int_in_range("0", 0, 3000));
        assert!(int_in_range("3000", 0, 3000));
        assert!(!int_in_range("3001", 0, 3000));
        assert!(!int_in_range("-1", 0, 3000));
        assert!(!int_in_range("1.5", 0, 3000));
        assert!(!int_in_range("", 0, 3000));
    }

    #[test]
    fn is_bool01_accepts_only_0_or_1() {
        assert!(is_bool01("0"));
        assert!(is_bool01("1"));
        assert!(!is_bool01("2"));
        assert!(!is_bool01("true"));
        assert!(!is_bool01(""));
    }

    #[test]
    fn sanitize_text_value_strips_quotes_and_replaces_newlines() {
        assert_eq!(sanitize_text_value("plain"), "plain");
        assert_eq!(sanitize_text_value("has \"quotes\""), "has quotes");
        assert_eq!(sanitize_text_value("line1\nline2"), "line1 line2");
        assert_eq!(sanitize_text_value("cr\rlf\n"), "cr lf ");
    }

    // -- Part 5a: dist_sibling / bak_sibling ------------------------------

    #[test]
    fn dist_and_bak_sibling_append_suffix() {
        let p = Path::new("/a/b/playerbots.conf");
        assert_eq!(dist_sibling(p), PathBuf::from("/a/b/playerbots.conf.dist"));
        assert_eq!(bak_sibling(p), PathBuf::from("/a/b/playerbots.conf.bak"));
    }

    // -- Part 5a: kv_rows (pb-keys / conf-keys row parser) ----------------

    #[test]
    fn kv_rows_dedup_keeps_first_position_last_value_and_line() {
        let content = "\
# a comment
Rate.XP.Kill = 1\r
  Indented.Key\t=\t\"quoted value\"
Dup.Key = first
Other = mid
Dup.Key = second
";
        let rows = kv_rows(content);
        let keys: Vec<&str> = rows.iter().map(|(k, _, _)| k.as_str()).collect();
        // First-seen order: Dup.Key appears once, at its FIRST position.
        assert_eq!(keys, vec!["Rate.XP.Kill", "Indented.Key", "Dup.Key", "Other"]);
        let dup = rows.iter().find(|(k, _, _)| k == "Dup.Key").unwrap();
        // Last occurrence's value AND line number win.
        assert_eq!(dup.1, "second");
        assert_eq!(dup.2, 6);
        let indented = rows.iter().find(|(k, _, _)| k == "Indented.Key").unwrap();
        assert_eq!(indented.1, "\"quoted value\"");
        assert_eq!(indented.2, 3);
    }

    #[test]
    fn kv_rows_skips_non_assignment_lines() {
        let content = "# comment\nNot A Key = skip\n\n=leadingeq\nReal.Key = 1\n";
        let rows = kv_rows(content);
        assert_eq!(rows, vec![("Real.Key".to_string(), "1".to_string(), 5)]);
    }

    // -- Part 5a: key_browser_rows (shared pb-keys/conf-keys default logic) -

    #[test]
    fn key_browser_rows_src_is_dist_defaults_to_own_value() {
        let src = "AiPlayerbot.MaxRandomBots = 500\n";
        let rows = key_browser_rows(src, None, true);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].value, "500");
        assert_eq!(rows[0].default.as_deref(), Some("500"));
    }

    #[test]
    fn key_browser_rows_conf_over_dist_fills_default_from_dist() {
        let src = "AiPlayerbot.MaxRandomBots = 800\nLive.Only = 1\n";
        let dist = "AiPlayerbot.MaxRandomBots = 500\n";
        let rows = key_browser_rows(src, Some(dist), false);
        let max_bots = rows.iter().find(|r| r.key == "AiPlayerbot.MaxRandomBots").unwrap();
        assert_eq!(max_bots.value, "800");
        assert_eq!(max_bots.default.as_deref(), Some("500"));
        // A key the live conf added that the dist never had -> default null.
        let live_only = rows.iter().find(|r| r.key == "Live.Only").unwrap();
        assert_eq!(live_only.default, None);
    }

    #[test]
    fn key_browser_rows_no_dist_content_is_all_null_defaults() {
        let rows = key_browser_rows("A = 1\n", None, false);
        assert_eq!(rows[0].default, None);
    }

    #[test]
    fn key_browser_rows_dist_key_present_with_empty_value_counts_as_present() {
        // Bash's `${_pb_def[$k]+x}` is an EXISTENCE test, not non-empty --
        // an empty dist value still counts as "there is a default".
        let src = "Blank.Val = 1\n";
        let dist = "Blank.Val =\n";
        let rows = key_browser_rows(src, Some(dist), false);
        assert_eq!(rows[0].default.as_deref(), Some(""));
    }

    // -- Part 5a: conf_help_lines (awk-oracle-verified fixtures) ----------
    //
    // Each fixture's expected output was captured by running the ACTUAL
    // `_conf_help_lines` awk function (copied verbatim from 40-config.sh)
    // against the same text via `gawk` on this dev box -- these are not
    // hand-guessed expectations.

    #[test]
    fn conf_help_lines_adjacent_and_shared_doc_block_styles() {
        let content = "\
# Adjacent block style
# Multiplier for XP gains
# second line of same block
Rate.XP.Kill = 1

# Header doc block style
#    AiPlayerbot.MaxRandomBots
# Max number of bots
# more text
#    AiPlayerbot.MinRandomBots
# Min number of bots
AiPlayerbot.MaxRandomBots = 500
AiPlayerbot.MinRandomBots = 100

# decoration only
####
NoHelp.Key = 1
";
        let help = conf_help_lines(content);
        assert_eq!(
            help,
            vec![
                (
                    "Rate.XP.Kill".to_string(),
                    "Adjacent block style Multiplier for XP gains second line of same block".to_string()
                ),
                ("AiPlayerbot.MaxRandomBots".to_string(), "Max number of bots more text".to_string()),
                ("AiPlayerbot.MinRandomBots".to_string(), "Min number of bots".to_string()),
                ("NoHelp.Key".to_string(), "decoration only".to_string()),
            ]
        );
    }

    #[test]
    fn conf_help_lines_pure_decoration_yields_no_help() {
        let content = "\
####
#
Bare.Key = 1

# text before blank line
# more text

BlankBroken.Key = 1
";
        let help = conf_help_lines(content);
        // Bare.Key's only preceding comments are pure decoration -> omitted
        // entirely (not even an empty-string entry).
        assert_eq!(
            help,
            vec![("BlankBroken.Key".to_string(), "text before blank line more text".to_string())]
        );
    }

    #[test]
    fn conf_help_lines_crlf_and_header_redeclared_later_wins() {
        let content = "# Doc for A\r\nA.Key = 1\r\n\r\n#    A.Key\r\n# repeated header later\r\nB.Key = 2\r\n";
        let help = conf_help_lines(content);
        // The header block AFTER the key (re-naming A.Key) wins over the
        // earlier adjacent block, and its trailing prose also becomes
        // B.Key's adjacent-block help (buf accumulates every non-header
        // comment line in the run, regardless of which key `cap` names).
        assert_eq!(
            help,
            vec![
                ("A.Key".to_string(), "repeated header later".to_string()),
                ("B.Key".to_string(), "repeated header later".to_string()),
            ]
        );
    }

    #[test]
    fn conf_help_lines_caps_at_400_chars() {
        let long_word = "x".repeat(500);
        let content = format!("# {long_word}\nLong.Key = 1\n");
        let help = conf_help_lines(&content);
        assert_eq!(help.len(), 1);
        assert_eq!(help[0].0, "Long.Key");
        assert_eq!(help[0].1.chars().count(), 400);
    }

    #[test]
    fn conf_help_lines_empty_content_is_empty() {
        assert_eq!(conf_help_lines(""), Vec::new());
    }
}
