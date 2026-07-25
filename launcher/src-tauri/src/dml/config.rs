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

fn atomic_write(path: &Path, content: &str) -> std::io::Result<()> {
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
///
/// SAFETY: a fresh minimal document is only ever built when the file is
/// genuinely absent or blank — the bash oracle's `yq -i` on an existing file
/// that fails to parse leaves the file untouched, and this must too. An
/// existing, non-blank file that fails to parse as YAML (or parses but its
/// root isn't a mapping — a shape that could never have come from THIS
/// writer) is an error, never silently replaced: doing otherwise would
/// discard every other service/volume/env-var already in the file. Mirrors
/// `override_env_remove`'s "can't make sense of it -> don't touch it" stance,
/// just surfaced as an `Err` here (a write caller has to know its edit was
/// NOT applied; `remove`'s equivalent no-op is safe because it has nothing to
/// apply).
pub fn override_env_write(path: &Path, key: &str, value: &str) -> std::io::Result<bool> {
    let existing_text = std::fs::read_to_string(path).ok();
    let cur = existing_text
        .as_deref()
        .map(parse_override_env)
        .unwrap_or_default();
    if cur.get(key).map(String::as_str) == Some(value) {
        return Ok(false);
    }

    let mut doc: serde_yaml_ng::Value = match existing_text.as_deref() {
        Some(t) if !t.trim().is_empty() => serde_yaml_ng::from_str(t).map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "{} exists but is not valid YAML; refusing to overwrite it: {e}",
                    path.display()
                ),
            )
        })?,
        _ => serde_yaml_ng::Value::Mapping(serde_yaml_ng::Mapping::new()),
    };
    if !matches!(doc, serde_yaml_ng::Value::Mapping(_)) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "{} exists but its root is not a YAML mapping; refusing to overwrite it",
                path.display()
            ),
        ));
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

// ---------------------------------------------------------------------------
// `wow_config_set_native` (Task B2a) — pure routing/path/validation helpers.
// The Tauri command itself (`lib.rs`) orchestrates these plus the SOAP/DB
// side effects; everything below is filesystem-shape logic ported straight
// from the `set)` case (`90-main.sh:2344-2561`) and its `40-config.sh`
// dependents, kept here so it is unit-testable without Tauri or a server.
// ---------------------------------------------------------------------------

/// Whether the WoW Playerbots title is installed — a port of `_wow_server_dir`
/// (`90-main.sh:106-110`) reduced to the boolean the `set`/`tuning-set`
/// preamble actually needs (the oracle only ever checks `[[ -z "$cfg_sdir" ]]`
/// here, never uses the resolved path itself). Mirrors `_has_compose`
/// (`90-main.sh:9-15`) + `_resolve_compose_dir` (`90-main.sh:61-71`): the
/// title's base dir must exist, and either it or its first subdir carrying a
/// compose file counts as installed.
pub fn wow_server_installed(title_dir: &Path) -> bool {
    fn has_compose(dir: &Path) -> bool {
        ["docker-compose.yml", "docker-compose.yaml", "compose.yml", "compose.yaml"]
            .iter()
            .any(|name| dir.join(name).is_file())
    }
    if !title_dir.is_dir() {
        return false;
    }
    if has_compose(title_dir) {
        return true;
    }
    let Ok(entries) = std::fs::read_dir(title_dir) else {
        return false;
    };
    entries
        .flatten()
        .any(|entry| entry.path().is_dir() && has_compose(&entry.path()))
}

/// The direct-route (`conf:...`) conf files that stay curated-rows-only — a
/// port of the `set)` case's core-conf `case` guard (`90-main.sh:2367-2372`).
pub fn is_core_conf_file(file: &str) -> bool {
    matches!(
        file,
        "worldserver.conf" | "authserver.conf" | ".env" | "docker-compose.override.yml"
    )
}

/// Direct-route conf-key shape gate — `^[A-Za-z0-9_.]+$` (`90-main.sh:2373`).
pub fn is_valid_direct_conf_key(key: &str) -> bool {
    !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.'))
}

/// The one direct-route conf key `wow bots flush` owns — setting it by hand
/// arms a persistent boot-time wipe. Port of the denylist `case`
/// (`90-main.sh:2382-2388`).
pub const DENYLISTED_DIRECT_CONF_KEY: &str = "AiPlayerbot.DeleteRandomBotAccounts";

/// Whether `key` is the denylisted direct-route conf key.
pub fn is_denylisted_direct_key(key: &str) -> bool {
    key == DENYLISTED_DIRECT_CONF_KEY
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

/// Module-conf name shape gate — the traversal guard from `_cfg_file_path`
/// (`40-config.sh:645-659`): `^[A-Za-z0-9_.-]+\.conf$`. No slash can match
/// this charset, so the name can never leave the modules dir.
pub fn is_module_conf_name(name: &str) -> bool {
    !name.is_empty()
        && name.ends_with(".conf")
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

/// Direct-route module-conf host path — the module-conf arm of `_cfg_file_path`
/// (`40-config.sh:645-659`; the fixed `worldserver.conf`/`authserver.conf`/
/// `.env`/override arms are unreachable here because the `set` command's
/// core-conf `case` already rejects those file names before this is called).
/// `None` when the name fails the shape gate, or when neither the conf nor
/// its `.dist` exists under `env/dist/etc/modules/`.
pub fn direct_conf_path(title_dir: &Path, name: &str) -> Option<PathBuf> {
    if !is_module_conf_name(name) {
        return None;
    }
    let p = title_dir.join("env").join("dist").join("etc").join("modules").join(name);
    let mut dist_os = p.as_os_str().to_os_string();
    dist_os.push(".dist");
    if p.exists() || PathBuf::from(dist_os).exists() {
        Some(p)
    } else {
        None
    }
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

/// The owning module's VERIFIED live-reload console command for a conf file,
/// or `None` when none is known — a port of `_conf_reload_cmd`
/// (`40-config.sh:578-584`). Deliberately tiny: only `transmog.conf` has a
/// verified reload command; everything else stays restart-to-apply.
pub fn conf_reload_cmd(file: &str) -> Option<&'static str> {
    match file {
        "transmog.conf" => Some("transmog reload"),
        _ => None,
    }
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
    fn override_env_write_refuses_to_clobber_unparseable_existing_file() {
        let path = tmp_override_path("badyaml");
        let before = "services: [this is not\n  a valid yaml mapping\n";
        std::fs::write(&path, before).unwrap();
        let err = override_env_write(&path, "AC_RATE_XP_KILL", "3").unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        // File is untouched -- nothing else in it was discarded.
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn override_env_write_refuses_to_clobber_non_mapping_root() {
        let path = tmp_override_path("nonmapping");
        let before = "- just\n- a\n- list\n";
        std::fs::write(&path, before).unwrap();
        let err = override_env_write(&path, "AC_RATE_XP_KILL", "3").unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
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

    // -- B2a: direct-route (`conf:`) validation -------------------------

    #[test]
    fn direct_route_parses_bare_and_module_conf_keys() {
        // Brief's exact examples.
        assert_eq!(
            route_conf("conf:playerbots.conf:AiPlayerbot.Foo"),
            Some(("playerbots.conf".into(), "AiPlayerbot.Foo".into()))
        );
        assert_eq!(
            route_conf("conf:Rate.XP.Kill"),
            Some(("worldserver.conf".into(), "Rate.XP.Kill".into()))
        );
    }

    #[test]
    fn wow_server_installed_requires_dir_and_a_compose_file() {
        let dir = std::env::temp_dir().join(format!("dml-serverinstalled-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        // Absent entirely -> not installed.
        assert!(!wow_server_installed(&dir));

        // Exists but no compose file anywhere -> not installed.
        std::fs::create_dir_all(&dir).unwrap();
        assert!(!wow_server_installed(&dir));

        // Compose file directly in the title dir (the real install layout).
        std::fs::write(dir.join("docker-compose.yml"), "services: {}\n").unwrap();
        assert!(wow_server_installed(&dir));

        let _ = std::fs::remove_dir_all(&dir);

        // Compose file only in a subdir -> still installed (oracle's
        // `_resolve_compose_dir` subdir fallback).
        let sub = dir.join("wow-server-playerbots");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("compose.yaml"), "services: {}\n").unwrap();
        assert!(wow_server_installed(&dir));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn is_core_conf_file_rejects_the_curated_only_four() {
        assert!(is_core_conf_file("worldserver.conf"));
        assert!(is_core_conf_file("authserver.conf"));
        assert!(is_core_conf_file(".env"));
        assert!(is_core_conf_file("docker-compose.override.yml"));
        assert!(!is_core_conf_file("playerbots.conf"));
        assert!(!is_core_conf_file("mod_ahbot.conf"));
    }

    #[test]
    fn direct_conf_key_shape_rejects_anything_outside_the_charset() {
        assert!(is_valid_direct_conf_key("AiPlayerbot.MaxRandomBots"));
        assert!(is_valid_direct_conf_key("Rate_XP.Kill123"));
        assert!(!is_valid_direct_conf_key(""));
        assert!(!is_valid_direct_conf_key("Has Space"));
        assert!(!is_valid_direct_conf_key("Has;Semicolon"));
        assert!(!is_valid_direct_conf_key("Has/Slash"));
    }

    #[test]
    fn denylist_flags_only_the_bot_flush_owned_key() {
        assert!(is_denylisted_direct_key("AiPlayerbot.DeleteRandomBotAccounts"));
        assert!(!is_denylisted_direct_key("AiPlayerbot.MaxRandomBots"));
    }

    #[test]
    fn single_line_and_max_len_gates() {
        assert!(is_single_line("plain value"));
        assert!(!is_single_line("line1\nline2"));
        assert!(!is_single_line("carriage\rreturn"));
        assert!(within_max_len(&"x".repeat(200), 200));
        assert!(!within_max_len(&"x".repeat(201), 200));
    }

    #[test]
    fn module_conf_name_shape_is_the_traversal_guard() {
        // No slash can ever match the charset, so these can't escape the
        // modules dir regardless of what the filesystem existence check does.
        assert!(!is_module_conf_name("../evil.conf"));
        assert!(!is_module_conf_name("a/b.conf"));
        assert!(!is_module_conf_name("mod_foo"));
        assert!(is_module_conf_name("mod_foo.conf"));
        assert!(is_module_conf_name("playerbots.conf"));
    }

    #[test]
    fn direct_conf_path_rejects_traversal_and_requires_existence() {
        let dir = std::env::temp_dir().join(format!("dml-directconf-test-{}", std::process::id()));
        let modules = dir.join("env").join("dist").join("etc").join("modules");
        std::fs::create_dir_all(&modules).unwrap();
        std::fs::write(modules.join("real.conf"), "A = 1\n").unwrap();
        std::fs::write(modules.join("dist_only.conf.dist"), "A = 1\n").unwrap();

        // Traversal-shaped names are rejected before any filesystem check.
        assert_eq!(direct_conf_path(&dir, "../evil.conf"), None);
        assert_eq!(direct_conf_path(&dir, "a/b.conf"), None);
        // Shape-valid but nothing on disk (conf or .dist) -> None.
        assert_eq!(direct_conf_path(&dir, "ghost.conf"), None);
        // Live conf exists.
        assert_eq!(direct_conf_path(&dir, "real.conf"), Some(modules.join("real.conf")));
        // Only the .dist exists -> the LIVE path is still returned (caller
        // seeds it via conf_ensure).
        assert_eq!(direct_conf_path(&dir, "dist_only.conf"), Some(modules.join("dist_only.conf")));

        let _ = std::fs::remove_dir_all(&dir);
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

    #[test]
    fn conf_reload_cmd_only_transmog_is_known() {
        assert_eq!(conf_reload_cmd("transmog.conf"), Some("transmog reload"));
        assert_eq!(conf_reload_cmd("playerbots.conf"), None);
        assert_eq!(conf_reload_cmd("worldserver.conf"), None);
        assert_eq!(conf_reload_cmd("mod_ahbot.conf"), None);
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
