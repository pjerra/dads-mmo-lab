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
        let etc = self.title_dir.join("env").join("dist").join("etc");
        match file {
            "worldserver.conf" | "authserver.conf" => etc.join(file),
            _ => etc.join("modules").join(file),
        }
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
}
