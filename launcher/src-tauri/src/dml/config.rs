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

/// [`strip_conf_quotes`], [`parse_conf`], and [`parse_override_env`] now live
/// in `dml_core::conf` (cargo-workspace refactor, Task 5) — they are generic
/// conf-file/YAML parsing with no AC-specific routing knowledge. Re-exported
/// here under the same names so every call site (this module's own
/// `ConfigReader`, `dml::tuning`, `lib.rs`) keeps compiling unchanged.
pub use dml_core::conf::{parse_conf, parse_override_env, strip_conf_quotes};

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

/// `conf_write` (BYTE-PARITY with `_cfg_conf_write`)/`override_env_write`/
/// `override_env_remove` (SEMANTIC-parity with `_cfg_env_write`/
/// `_cfg_env_remove`) and their private plumbing (`unquote_conf_matched`,
/// `conf_read_raw`, `atomic_write`, `ensure_yaml_mapping`) now live in
/// `dml_core::conf` (cargo-workspace refactor, Task 5) — none of it is
/// AC-specific. `atomic_write` is additionally re-exported `pub(crate)`
/// (its pre-move visibility) because `lib.rs`'s `wow_config_raw_write_native`
/// calls it directly, not just through the three writers above.
pub use dml_core::conf::{conf_write, override_env_remove, override_env_write};
pub(crate) use dml_core::conf::atomic_write;

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

/// [`is_single_line`]/[`within_max_len`] now live in `dml_core::conf`
/// (cargo-workspace refactor, Task 5) — generic value-shape gates with no AC
/// knowledge. Re-exported here under the same names.
pub use dml_core::conf::{is_single_line, within_max_len};

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

/// [`dist_sibling`]/[`bak_sibling`] now live in `dml_core::conf`
/// (cargo-workspace refactor, Task 5) — plain path-suffix helpers, no AC
/// knowledge. Re-exported here under the same names.
pub use dml_core::conf::{bak_sibling, dist_sibling};

/// Full editable-file path resolution — a faithful port of `_cfg_file_path`
/// (`40-config.sh:645-659`), the traversal guard shared by raw-read/
/// raw-write/raw-reset (Part 5a) AND (via [`direct_conf_path`], already the
/// module-conf arm of this same bash function) `wow config set`'s direct
/// route. `.env`/the compose override/`worldserver.conf`/`authserver.conf`
/// are FIXED names, resolved unconditionally — exactly like the bash arms,
/// which never `return 1` for these four; existence is the CALLER's problem.
/// Any other name falls to [`direct_conf_path`]: it must be
/// `^[A-Za-z0-9_.-]+\.conf$` (no slash can ever match, so this is the
/// traversal guard) AND have a conf OR its `.dist` already on disk under
/// `env/dist/etc/modules/`.
pub fn cfg_file_path(title_dir: &Path, name: &str) -> Option<PathBuf> {
    match name {
        ".env" => Some(title_dir.join(".env")),
        "docker-compose.override.yml" => Some(title_dir.join("docker-compose.override.yml")),
        "worldserver.conf" | "authserver.conf" => {
            Some(title_dir.join("env").join("dist").join("etc").join(name))
        }
        _ => direct_conf_path(title_dir, name),
    }
}

/// The pb-keys/conf-keys searchable all-keys browsers (`match_key_eq`,
/// `kv_rows`, `KeyBrowserRow`, `key_browser_rows`, `conf_help_lines`,
/// `comment_text`) plus `conf_ensure` now live in `dml_core::conf`
/// (cargo-workspace refactor, Task 5) — generic `Key = value` row/comment
/// parsing with no AC-specific routing knowledge. Re-exported here under the
/// same names.
pub use dml_core::conf::{conf_ensure, conf_help_lines, key_browser_rows, kv_rows, KeyBrowserRow};

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

/// The curated-row value validators (`float_in_range`/`int_in_range`/
/// `is_bool01`/`sanitize_text_value`) now live in `dml_core::conf`
/// (cargo-workspace refactor, Task 5) — plain shape/range checks with no AC
/// knowledge. Re-exported here under the same names.
pub use dml_core::conf::{float_in_range, int_in_range, is_bool01, sanitize_text_value};

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
    fn conf_reload_cmd_only_transmog_is_known() {
        assert_eq!(conf_reload_cmd("transmog.conf"), Some("transmog reload"));
        assert_eq!(conf_reload_cmd("playerbots.conf"), None);
        assert_eq!(conf_reload_cmd("worldserver.conf"), None);
        assert_eq!(conf_reload_cmd("mod_ahbot.conf"), None);
    }

    // -- Part 5a: cfg_file_path -------------------------------------------

    #[test]
    fn cfg_file_path_fixed_names_never_fail() {
        let dir = std::env::temp_dir().join(format!("dml-cfgfilepath-fixed-{}", std::process::id()));
        // None of these exist on disk -- the fixed arms resolve unconditionally.
        assert_eq!(cfg_file_path(&dir, ".env"), Some(dir.join(".env")));
        assert_eq!(
            cfg_file_path(&dir, "docker-compose.override.yml"),
            Some(dir.join("docker-compose.override.yml"))
        );
        assert_eq!(
            cfg_file_path(&dir, "worldserver.conf"),
            Some(dir.join("env").join("dist").join("etc").join("worldserver.conf"))
        );
        assert_eq!(
            cfg_file_path(&dir, "authserver.conf"),
            Some(dir.join("env").join("dist").join("etc").join("authserver.conf"))
        );
    }

    #[test]
    fn cfg_file_path_module_conf_delegates_to_direct_conf_path() {
        let dir = std::env::temp_dir().join(format!("dml-cfgfilepath-mod-{}", std::process::id()));
        let modules = dir.join("env").join("dist").join("etc").join("modules");
        std::fs::create_dir_all(&modules).unwrap();
        std::fs::write(modules.join("playerbots.conf"), "A = 1\n").unwrap();
        assert_eq!(cfg_file_path(&dir, "playerbots.conf"), Some(modules.join("playerbots.conf")));
        // Traversal-shaped / nonexistent module confs still fail through
        // direct_conf_path's own guards.
        assert_eq!(cfg_file_path(&dir, "../evil.conf"), None);
        assert_eq!(cfg_file_path(&dir, "ghost.conf"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
