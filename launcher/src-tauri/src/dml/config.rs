//! Native-mode WoW config **value** reader (spike: `spike/docker-desktop-native`).
//!
//! Task 2 of the native migration: the Settings/Tuning pages need the same data
//! as `dml wow config list --json` (`.data.settings[]`, 66 rows) but WITHOUT
//! paying the bash-on-Windows fork tax (~2.1s per `config list`: bash startup +
//! yq startup + a fork per row). The static half of each row already comes for
//! free from the cheap `dml wow config registry --json` arm (Task 1); this
//! module supplies the ONE dynamic field the registry leaves as `""` — `value`
//! — by reading the runtime files DIRECTLY in Rust (microseconds, no engine).
//!
//! The value-resolution logic here is a faithful port of the `config list`
//! emitter in `cli/src/40-config.sh` + `cli/src/90-main.sh` (the `list` arm).
//! Each helper documents the exact bash it mirrors. A cargo parity test
//! (`config_parity.rs`, file-gated) asserts the assembled JSON deep-equals a
//! real `dml wow config list --json` run against the on-disk native files.
//!
//! Docker Desktop may be CLOSED while this runs: every read here is a pure file
//! read and needs no engine. The one field bash `list` sources from the DB
//! (`server.motd`, from `acore_auth.motd`) has no file to read, so — exactly
//! like `list` does when the DB is unreachable — it falls back to the registry
//! default. See `compute_value`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// The single title this launcher drives (mirrors `LAN_TITLE` in lib.rs and
/// `_wow_server_dir`'s `$GAMES_DIR/wow-server-playerbots`).
pub const TITLE: &str = "wow-server-playerbots";

/// AC docker env-bridge name for a conf key — a port of `_cfg_env_name_for`
/// (40-config.sh:592). Rule: prefix `AC_`, dots become `_`, a `_` is inserted
/// before an uppercase letter that follows a lowercase letter or a digit
/// (camelCase word boundary), then the whole thing is uppercased.
///
/// `Rate.XP.Kill` -> `AC_RATE_XP_KILL`,
/// `AiPlayerbot.MaxRandomBots` -> `AC_AI_PLAYERBOT_MAX_RANDOM_BOTS`,
/// `AIPlayerbot.GuildFeedback` -> `AC_AIPLAYERBOT_GUILD_FEEDBACK`.
pub fn env_name_for(key: &str) -> String {
    let mut out = String::with_capacity(key.len() + 4);
    let mut prev: Option<char> = None;
    for c in key.chars() {
        if c == '.' {
            out.push('_');
        } else if c.is_ascii_uppercase()
            && matches!(prev, Some(p) if p.is_ascii_lowercase() || p.is_ascii_digit())
        {
            out.push('_');
            out.push(c);
        } else {
            out.push(c);
        }
        prev = Some(c);
    }
    format!("AC_{}", out.to_ascii_uppercase())
}

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

/// yq `tostring` semantics for a scalar override value, so a bare `0.0.0.0` and
/// a quoted `"7878"` both round-trip to the string bash's `_cfg_env_load_map`
/// would have stored. Non-scalars never appear in an AC env map; they degrade to
/// an empty string rather than panicking.
fn yaml_scalar_to_string(v: &serde_yaml_ng::Value) -> String {
    match v {
        serde_yaml_ng::Value::String(s) => s.clone(),
        serde_yaml_ng::Value::Bool(b) => b.to_string(),
        serde_yaml_ng::Value::Number(n) => n.to_string(),
        serde_yaml_ng::Value::Null => "null".to_string(),
        _ => String::new(),
    }
}

/// Parse `.services.ac-worldserver.environment` from the override YAML into a
/// KEY -> value(string) map — the Rust equivalent of one `_cfg_env_load_map`
/// (40-config.sh:189) yq dump, read ONCE. A missing file/section, or YAML that
/// fails to parse, yields an empty map (a valid "nothing overridden" answer,
/// matching the bash `// {}` fallback).
pub fn parse_override_env(yaml_text: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Ok(doc) = serde_yaml_ng::from_str::<serde_yaml_ng::Value>(yaml_text) else {
        return map;
    };
    let env = doc
        .get("services")
        .and_then(|s| s.get("ac-worldserver"))
        .and_then(|w| w.get("environment"));
    if let Some(serde_yaml_ng::Value::Mapping(m)) = env {
        for (k, v) in m {
            if let Some(key) = k.as_str() {
                map.insert(key.to_string(), yaml_scalar_to_string(v));
            }
        }
    }
    map
}

/// Split a `conf:[<file>.conf:]<Key>` env-column spec into `(file, key)`, a port
/// of `_cfg_conf_route` (40-config.sh:291). The file defaults to
/// `worldserver.conf`; when the spec carries a `<file>.conf:` prefix that file
/// wins. Returns `None` when the column is not a `conf:` spec at all.
pub fn route_conf(env: &str) -> Option<(String, String)> {
    let spec = env.strip_prefix("conf:")?;
    // Mirror bash exactly: `*.conf:*` gates on ".conf:" appearing, and when it
    // does the file is `${spec%%:*}` (up to the FIRST colon) and the key is
    // `${spec#*:}` (after the first colon). For every real registry spec the
    // first colon is the one right after ".conf", so this equals a ".conf:"
    // split — but the first-colon rule is what the bash actually does.
    if spec.contains(".conf:") {
        let colon = spec.find(':').expect(".conf: implies a colon");
        let file = &spec[..colon];
        let key = &spec[colon + 1..];
        Some((file.to_string(), key.to_string()))
    } else {
        Some(("worldserver.conf".to_string(), spec.to_string()))
    }
}

/// Host path of a conf file under a title dir — the free-function form of
/// `_cfg_conf_path` (40-config.sh:308), shared by the config reader and the
/// module-tuning reader (`dml::tuning`). worldserver/authserver live directly
/// in `env/dist/etc`; every other conf (all module confs) under
/// `env/dist/etc/modules`.
pub fn conf_path_in(title_dir: &Path, file: &str) -> PathBuf {
    let etc = title_dir.join("env").join("dist").join("etc");
    match file {
        "worldserver.conf" | "authserver.conf" => etc.join(file),
        _ => etc.join("modules").join(file),
    }
}

/// Reads live config VALUES straight off the native runtime files — no bash, no
/// yq, no fork, no engine. Built once per `wow_config_read` call: the override
/// env map is parsed up front (single file read); conf files are read lazily and
/// memoised so a 66-row assembly touches each conf at most once.
pub struct ConfigReader {
    title_dir: PathBuf,
    env_map: HashMap<String, String>,
    conf_cache: HashMap<PathBuf, HashMap<String, String>>,
}

impl ConfigReader {
    /// Title dir from `DML_GAMES_DIR` + `wow-server-playerbots` (the task's
    /// resolution, matching `_wow_server_dir`). Absent `DML_GAMES_DIR` yields a
    /// bare relative path, so reads simply miss and values fall back to
    /// defaults rather than panicking.
    pub fn title_dir_from_env() -> PathBuf {
        let base = std::env::var_os("DML_GAMES_DIR")
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        base.join(TITLE)
    }

    /// Construct from an explicit title dir, eagerly loading the override env map.
    pub fn for_title(title_dir: impl Into<PathBuf>) -> Self {
        let title_dir = title_dir.into();
        let override_path = title_dir.join("docker-compose.override.yml");
        let env_map = std::fs::read_to_string(&override_path)
            .map(|t| parse_override_env(&t))
            .unwrap_or_default();
        ConfigReader { title_dir, env_map, conf_cache: HashMap::new() }
    }

    /// Convenience: build from `DML_GAMES_DIR`.
    pub fn from_env() -> Self {
        Self::for_title(Self::title_dir_from_env())
    }

    /// Host path of a conf file — `_cfg_conf_path` (40-config.sh:308):
    /// worldserver/authserver live in `env/dist/etc`, every other conf under
    /// `env/dist/etc/modules`.
    fn conf_path(&self, file: &str) -> PathBuf {
        conf_path_in(&self.title_dir, file)
    }

    /// Quote-stripped value of `key` in `path`, memoising the parsed file. `""`
    /// when the file or key is absent (so an empty answer still triggers the
    /// caller's `.dist` fallback, exactly like `_cfg_conf_get_var`).
    fn conf_value(&mut self, path: &Path, key: &str) -> String {
        if !self.conf_cache.contains_key(path) {
            let parsed = std::fs::read_to_string(path)
                .map(|t| parse_conf(&t))
                .unwrap_or_default();
            self.conf_cache.insert(path.to_path_buf(), parsed);
        }
        let raw = self.conf_cache[path].get(key).cloned().unwrap_or_default();
        strip_conf_quotes(&raw)
    }

    /// Compute the live `value` for one registry row, mirroring the `config list`
    /// emitter (90-main.sh:2281-2300):
    ///  - `server.motd`: DB-backed in bash; file-only here, so the value stays
    ///    empty and the default fallback below supplies the registry default —
    ///    identical to what `list` emits when the DB is unreachable (the
    ///    Docker-closed case this native path targets).
    ///  - `conf:` rows: a legacy `AC_*` override still on disk BEATS the conf
    ///    (AC env bridge), so try the derived env name first; else the live conf;
    ///    else the `.dist`.
    ///  - any other env column: read it straight from the override env map.
    ///  - finally, an empty result falls back to the registry default.
    pub fn compute_value(&mut self, key: &str, env: &str, default: &str) -> String {
        let mut val = if key == "server.motd" {
            String::new()
        } else if let Some((file, conf_key)) = route_conf(env) {
            let from_env = self.env_map.get(&env_name_for(&conf_key)).cloned().unwrap_or_default();
            if !from_env.is_empty() {
                from_env
            } else {
                let path = self.conf_path(&file);
                let live = self.conf_value(&path, &conf_key);
                if !live.is_empty() {
                    live
                } else {
                    let dist = path.with_file_name(format!("{file}.dist"));
                    self.conf_value(&dist, &conf_key)
                }
            }
        } else if env == "-" {
            String::new()
        } else {
            self.env_map.get(env).cloned().unwrap_or_default()
        };
        if val.is_empty() {
            val = default.to_string();
        }
        val
    }

    /// Assemble `{"settings":[…]}` from the cached registry rows, filling each
    /// row's `value` from the live files. Every static field is carried through
    /// byte-for-byte from the registry row (which Task 1 pinned equal to
    /// `config list` minus values), so only `value` can ever differ. Rows that
    /// are not objects, or lack the needed string fields, are passed through
    /// unchanged (defensive; the registry never emits such rows).
    pub fn assemble(&mut self, registry_rows: &[Value]) -> Value {
        let mut out = Vec::with_capacity(registry_rows.len());
        for row in registry_rows {
            let mut row = row.clone();
            if let (Some(key), Some(env), Some(default)) = (
                row.get("key").and_then(Value::as_str).map(str::to_string),
                row.get("env").and_then(Value::as_str).map(str::to_string),
                row.get("default").and_then(Value::as_str).map(str::to_string),
            ) {
                let value = self.compute_value(&key, &env, &default);
                if let Some(obj) = row.as_object_mut() {
                    obj.insert("value".to_string(), Value::String(value));
                }
            }
            out.push(row);
        }
        serde_json::json!({ "settings": Value::Array(out) })
    }
}

// ---------------------------------------------------------------------------
// WRITE side (Task B1): comment-preserving conf-file edits + semantic-parity
// override-YAML env edits. `conf_write` is BYTE-PARITY with `_cfg_conf_write`
// (comments/spacing/CRLF matter — hand-edited files); `override_env_write`/
// `override_env_remove` are SEMANTIC-parity with `_cfg_env_write`/
// `_cfg_env_remove` (the override is machine-generated — see the caveat on
// `override_env_write`).
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

/// Atomic write: write `content` to a sibling temp file (`{path}.tmp.{pid}`),
/// then rename it over `path`. Any error removes the temp file and
/// propagates — the original is never truncated or left half-written.
/// Shared by `conf_write` and the override-YAML writers.
fn atomic_write(path: &Path, content: &str) -> std::io::Result<()> {
    let mut tmp_os = path.as_os_str().to_os_string();
    tmp_os.push(format!(".tmp.{}", std::process::id()));
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

/// Ensure `parent[key]` is a `Mapping`, creating one (or replacing a
/// non-Mapping value) as needed, and return a mutable reference to it. Used
/// to auto-vivify the `services.ac-worldserver.environment` nesting.
fn ensure_yaml_mapping<'a>(
    parent: &'a mut serde_yaml_ng::Mapping,
    key: &str,
) -> &'a mut serde_yaml_ng::Mapping {
    let needs_insert = !matches!(parent.get(key), Some(serde_yaml_ng::Value::Mapping(_)));
    if needs_insert {
        parent.insert(
            serde_yaml_ng::Value::String(key.to_string()),
            serde_yaml_ng::Value::Mapping(serde_yaml_ng::Mapping::new()),
        );
    }
    match parent.get_mut(key) {
        Some(serde_yaml_ng::Value::Mapping(m)) => m,
        _ => unreachable!("just ensured a Mapping"),
    }
}

/// Set `.services.ac-worldserver.environment[key] = value` in an override
/// compose YAML, creating the nested maps when absent. Returns `Ok(changed)`.
/// Semantic-parity target: `_cfg_env_write` (40-config.sh:240).
///
/// Compares against the CURRENT value first via the existing
/// `parse_override_env` reader — an unchanged value is a true no-op
/// (`Ok(false)`, file untouched), matching the bash oracle's
/// `[[ "$cur" == "$2" ]] && return 0` short-circuit. The written value is
/// ALWAYS a YAML string scalar (never int/float/bool), matching yq's
/// `strenv()` semantics.
///
/// KNOWN CAVEAT: `serde_yaml_ng` does not preserve comments and may choose
/// different scalar quoting than mikefarah `yq` (the bash oracle's engine).
/// The override file is machine-generated (no hand comments expected), so
/// this is acceptable — the B3 parity test compares this file SEMANTICALLY
/// (parsed env-map equality), NOT byte-for-byte, unlike `conf_write`.
pub fn override_env_write(path: &Path, key: &str, value: &str) -> std::io::Result<bool> {
    let existing_text = std::fs::read_to_string(path).ok();
    let cur = existing_text
        .as_deref()
        .map(parse_override_env)
        .unwrap_or_default();
    if cur.get(key).map(String::as_str) == Some(value) {
        return Ok(false);
    }

    let mut doc: serde_yaml_ng::Value = existing_text
        .as_deref()
        .filter(|t| !t.trim().is_empty())
        .and_then(|t| serde_yaml_ng::from_str(t).ok())
        .unwrap_or_else(|| serde_yaml_ng::Value::Mapping(serde_yaml_ng::Mapping::new()));
    if !matches!(doc, serde_yaml_ng::Value::Mapping(_)) {
        doc = serde_yaml_ng::Value::Mapping(serde_yaml_ng::Mapping::new());
    }

    let root = doc.as_mapping_mut().expect("just ensured a Mapping");
    let services = ensure_yaml_mapping(root, "services");
    let worldserver = ensure_yaml_mapping(services, "ac-worldserver");
    let environment = ensure_yaml_mapping(worldserver, "environment");
    environment.insert(
        serde_yaml_ng::Value::String(key.to_string()),
        serde_yaml_ng::Value::String(value.to_string()),
    );

    let text = serde_yaml_ng::to_string(&doc)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    atomic_write(path, &text)?;
    Ok(true)
}

/// Remove `key` from `.services.ac-worldserver.environment` if present.
/// Idempotent: an absent file, absent section, or absent key are all
/// `Ok(false)` — never an error. Semantic-parity target: `_cfg_env_remove`
/// (40-config.sh:276).
pub fn override_env_remove(path: &Path, key: &str) -> std::io::Result<bool> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Ok(false);
    };
    if text.trim().is_empty() {
        return Ok(false);
    }
    let Ok(mut doc) = serde_yaml_ng::from_str::<serde_yaml_ng::Value>(&text) else {
        return Ok(false);
    };
    let removed = doc
        .as_mapping_mut()
        .and_then(|root| root.get_mut("services"))
        .and_then(|v| v.as_mapping_mut())
        .and_then(|services| services.get_mut("ac-worldserver"))
        .and_then(|v| v.as_mapping_mut())
        .and_then(|worldserver| worldserver.get_mut("environment"))
        .and_then(|v| v.as_mapping_mut())
        .and_then(|environment| environment.remove(key));

    if removed.is_none() {
        return Ok(false);
    }
    let text = serde_yaml_ng::to_string(&doc)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    atomic_write(path, &text)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_name_for_matches_bash_derivation() {
        // Cases lifted from real registry conf keys + the derivation comment.
        assert_eq!(env_name_for("Rate.XP.Kill"), "AC_RATE_XP_KILL");
        assert_eq!(env_name_for("Rate.MoveSpeed.Player"), "AC_RATE_MOVE_SPEED_PLAYER");
        assert_eq!(
            env_name_for("AiPlayerbot.MaxRandomBots"),
            "AC_AI_PLAYERBOT_MAX_RANDOM_BOTS"
        );
        // Capital-I run: no boundary inside "AIPlayerbot" (I follows A, not a
        // lowercase/digit), so it stays glued — the deliberate quirk the bash
        // comment calls out for AIPlayerbot.GuildFeedback.
        assert_eq!(
            env_name_for("AIPlayerbot.GuildFeedback"),
            "AC_AIPLAYERBOT_GUILD_FEEDBACK"
        );
        assert_eq!(
            env_name_for("AllowTwoSide.Accounts"),
            "AC_ALLOW_TWO_SIDE_ACCOUNTS"
        );
        assert_eq!(
            env_name_for("AuctionHouseBot.EnableSeller"),
            "AC_AUCTION_HOUSE_BOT_ENABLE_SELLER"
        );
    }

    #[test]
    fn env_name_for_digit_before_upper_is_a_boundary() {
        // A digit is a word boundary just like a lowercase letter.
        assert_eq!(env_name_for("Foo2Bar"), "AC_FOO2_BAR");
    }

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
    fn route_conf_defaults_and_explicit_file() {
        assert_eq!(
            route_conf("conf:Rate.XP.Kill"),
            Some(("worldserver.conf".into(), "Rate.XP.Kill".into()))
        );
        assert_eq!(
            route_conf("conf:playerbots.conf:AiPlayerbot.MaxRandomBots"),
            Some(("playerbots.conf".into(), "AiPlayerbot.MaxRandomBots".into()))
        );
        assert_eq!(
            route_conf("conf:mod_ahbot.conf:AuctionHouseBot.GUID"),
            Some(("mod_ahbot.conf".into(), "AuctionHouseBot.GUID".into()))
        );
        assert_eq!(route_conf("-"), None);
        assert_eq!(route_conf("AC_SOMETHING"), None);
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

    #[test]
    fn parse_override_env_reads_the_worldserver_map() {
        let yaml = "\
services:
  ac-worldserver:
    volumes:
      - ./modules:/azerothcore/modules
    environment:
      AC_RATE_XP_KILL: \"3\"
      AC_SOAP_IP: 0.0.0.0
      AC_SOAP_PORT: \"7878\"
      AC_AI_PLAYERBOT_MAX_RANDOM_BOTS: \"2000\"
";
        let m = parse_override_env(yaml);
        assert_eq!(m.get("AC_RATE_XP_KILL").map(String::as_str), Some("3"));
        // Bare IP is a YAML string, kept verbatim.
        assert_eq!(m.get("AC_SOAP_IP").map(String::as_str), Some("0.0.0.0"));
        assert_eq!(m.get("AC_SOAP_PORT").map(String::as_str), Some("7878"));
        assert_eq!(m.get("AC_AI_PLAYERBOT_MAX_RANDOM_BOTS").map(String::as_str), Some("2000"));
    }

    #[test]
    fn parse_override_env_empty_or_broken_is_empty_map() {
        assert!(parse_override_env("").is_empty());
        assert!(parse_override_env("services: {}").is_empty());
        assert!(parse_override_env(": : not : yaml : [").is_empty());
    }

    #[test]
    fn compute_value_env_beats_conf_then_default_fallback() {
        // Build a reader over a temp title dir with a known override + conf.
        let dir = std::env::temp_dir().join(format!("dml-cfg-test-{}", std::process::id()));
        let etc = dir.join("env").join("dist").join("etc");
        let modules = etc.join("modules");
        std::fs::create_dir_all(&modules).unwrap();
        std::fs::write(
            dir.join("docker-compose.override.yml"),
            "services:\n  ac-worldserver:\n    environment:\n      AC_RATE_XP_KILL: \"5\"\n",
        )
        .unwrap();
        std::fs::write(etc.join("worldserver.conf"), "Rate.XP.Kill = 3\nRate.Honor = 2\n").unwrap();
        std::fs::write(
            modules.join("playerbots.conf"),
            "AiPlayerbot.MaxRandomBots = \"800\"\n",
        )
        .unwrap();
        // Only the .dist has this one -> exercises the .dist fallback.
        std::fs::write(
            modules.join("mod_ahbot.conf.dist"),
            "AuctionHouseBot.EnableSeller = 1\n",
        )
        .unwrap();

        let mut r = ConfigReader::for_title(&dir);
        // env override present -> beats the conf's 3.
        assert_eq!(r.compute_value("rates.xp_kill", "conf:Rate.XP.Kill", "1"), "5");
        // no env override -> live conf value (quotes stripped).
        assert_eq!(
            r.compute_value("bots.population", "conf:playerbots.conf:AiPlayerbot.MaxRandomBots", "500"),
            "800"
        );
        // live conf value present, no env.
        assert_eq!(r.compute_value("rates.honor", "conf:Rate.Honor", "1"), "2");
        // no env, no live conf -> .dist fallback.
        assert_eq!(
            r.compute_value("ahbot.seller", "conf:mod_ahbot.conf:AuctionHouseBot.EnableSeller", "0"),
            "1"
        );
        // absent everywhere -> registry default.
        assert_eq!(
            r.compute_value("rates.gold", "conf:Rate.Drop.Money", "1"),
            "1"
        );
        // server.motd is DB-backed (file-only) -> default.
        assert_eq!(r.compute_value("server.motd", "-", "Welcome!"), "Welcome!");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn assemble_overwrites_only_the_value_field() {
        let dir = std::env::temp_dir().join(format!("dml-cfg-asm-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("docker-compose.override.yml"),
            "services:\n  ac-worldserver:\n    environment:\n      AC_RATE_XP_KILL: \"7\"\n",
        )
        .unwrap();
        let registry = serde_json::json!([
            {"key":"rates.xp_kill","group":"Rates","label":"XP","explain":"x","type":"float","min":0.5,"max":20,"value":"","default":"1","restart_required":true,"env":"conf:Rate.XP.Kill"}
        ]);
        let rows = registry.as_array().cloned().unwrap();
        let mut r = ConfigReader::for_title(&dir);
        let out = r.assemble(&rows);
        let s = &out["settings"][0];
        assert_eq!(s["value"], "7");
        // Every other field carried through untouched.
        assert_eq!(s["key"], "rates.xp_kill");
        assert_eq!(s["min"], 0.5);
        assert_eq!(s["max"], 20);
        assert_eq!(s["restart_required"], true);
        assert_eq!(s["env"], "conf:Rate.XP.Kill");
        let _ = std::fs::remove_dir_all(&dir);
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

    // -- override_env_write / override_env_remove -----------------------

    fn tmp_override_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("dml-envwrite-test-{}-{}", std::process::id(), name));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("docker-compose.override.yml")
    }

    #[test]
    fn override_env_write_new_key_into_minimal_override() {
        let path = tmp_override_path("newkey");
        std::fs::write(
            &path,
            "services:\n  ac-worldserver:\n    environment:\n      AC_SOAP_IP: 0.0.0.0\n",
        )
        .unwrap();
        let changed = override_env_write(&path, "AC_RATE_XP_KILL", "3").unwrap();
        assert!(changed);
        let text = std::fs::read_to_string(&path).unwrap();
        let m = parse_override_env(&text);
        assert_eq!(m.get("AC_RATE_XP_KILL").map(String::as_str), Some("3"));
        // Other keys survive.
        assert_eq!(m.get("AC_SOAP_IP").map(String::as_str), Some("0.0.0.0"));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn override_env_write_same_value_is_noop() {
        let path = tmp_override_path("samevalue");
        std::fs::write(
            &path,
            "services:\n  ac-worldserver:\n    environment:\n      AC_RATE_XP_KILL: \"3\"\n",
        )
        .unwrap();
        let before = std::fs::read_to_string(&path).unwrap();
        let changed = override_env_write(&path, "AC_RATE_XP_KILL", "3").unwrap();
        assert!(!changed);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn override_env_write_creates_full_nesting_from_absent_file() {
        let path = tmp_override_path("absent");
        assert!(!path.exists());
        let changed = override_env_write(&path, "AC_RATE_XP_KILL", "3").unwrap();
        assert!(changed);
        let text = std::fs::read_to_string(&path).unwrap();
        let m = parse_override_env(&text);
        assert_eq!(m.len(), 1);
        assert_eq!(m.get("AC_RATE_XP_KILL").map(String::as_str), Some("3"));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn override_env_remove_present_key_removes_it() {
        let path = tmp_override_path("removepresent");
        std::fs::write(
            &path,
            "services:\n  ac-worldserver:\n    environment:\n      AC_RATE_XP_KILL: \"3\"\n      AC_SOAP_IP: 0.0.0.0\n",
        )
        .unwrap();
        let removed = override_env_remove(&path, "AC_RATE_XP_KILL").unwrap();
        assert!(removed);
        let text = std::fs::read_to_string(&path).unwrap();
        let m = parse_override_env(&text);
        assert!(!m.contains_key("AC_RATE_XP_KILL"));
        // Other key survives.
        assert_eq!(m.get("AC_SOAP_IP").map(String::as_str), Some("0.0.0.0"));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn override_env_remove_absent_key_is_noop() {
        let path = tmp_override_path("removeabsent");
        std::fs::write(
            &path,
            "services:\n  ac-worldserver:\n    environment:\n      AC_SOAP_IP: 0.0.0.0\n",
        )
        .unwrap();
        let before = std::fs::read_to_string(&path).unwrap();
        let removed = override_env_remove(&path, "AC_DOES_NOT_EXIST").unwrap();
        assert!(!removed);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn override_env_remove_absent_file_is_noop_not_error() {
        let path = tmp_override_path("removenofile");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
        // Directory doesn't exist at all -> read fails -> Ok(false), never an error.
        let removed = override_env_remove(&path, "AC_SOAP_IP").unwrap();
        assert!(!removed);
    }

    #[test]
    fn override_env_write_preserves_other_top_level_keys() {
        let path = tmp_override_path("othertopkeys");
        std::fs::write(
            &path,
            "services:\n  ac-worldserver:\n    volumes:\n      - ./modules:/azerothcore/modules\n    environment:\n      AC_SOAP_IP: 0.0.0.0\n",
        )
        .unwrap();
        let changed = override_env_write(&path, "AC_RATE_XP_KILL", "3").unwrap();
        assert!(changed);
        let text = std::fs::read_to_string(&path).unwrap();
        // The volumes list under the same service survives the edit.
        assert!(text.contains("volumes"));
        assert!(text.contains("azerothcore/modules"));
        let m = parse_override_env(&text);
        assert_eq!(m.get("AC_RATE_XP_KILL").map(String::as_str), Some("3"));
        assert_eq!(m.get("AC_SOAP_IP").map(String::as_str), Some("0.0.0.0"));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
