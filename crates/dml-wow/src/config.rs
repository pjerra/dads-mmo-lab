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

use dml_core::error::CmdError;
use dml_core::proc::output_bounded;

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

/// [`strip_conf_quotes`] and [`parse_conf`] now live in `dml_core::conf`
/// (cargo-workspace refactor, Task 5) — they are generic conf-file parsing
/// with no AC-specific routing knowledge. Re-exported here under the same
/// names so every call site (this module's own `ConfigReader`, `tuning`,
/// `lib.rs`) keeps compiling unchanged.
pub use dml_core::conf::{parse_conf, strip_conf_quotes};

// ---------------------------------------------------------------------------
// Compose-override env map (Task 7): `parse_override_env` and the two writers
// further down came BACK out of `dml_core::conf`, because all three hardcode
// the `ac-worldserver` compose service key — the last runtime AzerothCore
// dependency left in the core crate. The failure mode for a non-AC caller was
// SILENT (the writer's `ensure_yaml_mapping` would auto-vivify a bogus
// `services.ac-worldserver.environment` block rather than erroring), so they
// belong on the game side. Bodies and tests are unchanged.
// ---------------------------------------------------------------------------

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

/// `conf_write` (BYTE-PARITY with `_cfg_conf_write`)/`override_env_write`/
/// `override_env_remove` (SEMANTIC-parity with `_cfg_env_write`/
/// `_cfg_env_remove`) and their private plumbing (`unquote_conf_matched`,
/// `conf_read_raw`, `atomic_write`, `ensure_yaml_mapping`) now live in
/// `dml_core::conf` (cargo-workspace refactor, Task 5) — none of it is
/// AC-specific. `atomic_write` is additionally re-exported (it was
/// `pub(crate)` while this module lived in the launcher crate; the dml-wow
/// split, Task 7, widens it to `pub`) because `lib.rs`'s
/// `wow_config_raw_write_native` calls it directly, not just through the
/// three writers above.
pub use dml_core::conf::conf_write;
pub use dml_core::conf::atomic_write;

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

/// Small `CmdError` builder to keep the `set)` port's many typed errors
/// readable — every field the bash `json_err`/exit-1 pairs carry, in one call.
pub fn cfgset_err(code: &str, message: impl Into<String>, hint: impl Into<String>) -> CmdError {
    CmdError { code: code.into(), message: message.into(), hint: hint.into() }
}

/// Legacy-env cleanup for one derived `AC_*` name against the override —
/// shared by both routes. Mirrors `_cfg_env_read` non-empty check +
/// `_cfg_env_remove` (`40-config.sh:212-220`, `276-281`): a present-but-empty
/// override value reads as "absent" too, matching bash's `[[ -n … ]]` gate.
/// Returns `Ok(true)` iff the key WAS present (and is now removed).
pub fn cfgset_clean_legacy_env(override_path: &std::path::Path, ename: &str) -> Result<bool, CmdError> {
    let present = std::fs::read_to_string(override_path)
        .ok()
        .and_then(|t| crate::config::parse_override_env(&t).get(ename).cloned())
        .map(|v| !v.is_empty())
        .unwrap_or(false);
    if !present {
        return Ok(false);
    }
    crate::config::override_env_remove(override_path, ename).map_err(|e| {
        cfgset_err("WRITE_FAILED", format!("Could not update the config override: {e}"), "")
    })?;
    Ok(true)
}

/// Port of `_cfg_env_frozen` (`40-config.sh:266-271`): `true` iff the
/// running `ac-worldserver` container's CREATION-TIME environment carries
/// `ename`. Degrades to `false` (not frozen) on ANY error/timeout/docker-down
/// — matches the bash `return 1` fallback exactly (a clean override with no
/// frozen legacy env is the correct read when there is no way to ask). Bounds
/// the `docker inspect` call so a hung/absent engine can never stall a save;
/// the check runs on a helper thread that is left to finish (or hang) on its
/// own past the timeout rather than being joined, so the caller is never
/// blocked longer than `timeout`.
pub fn env_frozen(ename: &str) -> bool {
    env_frozen_with(
        &crate::native::docker_program(),
        "ac-worldserver",
        ename,
        std::time::Duration::from_secs(3),
    )
}

pub fn env_frozen_with(
    program: &std::ffi::OsStr,
    container: &str,
    ename: &str,
    timeout: std::time::Duration,
) -> bool {
    let ename_prefix = format!("{ename}=");
    let mut cmd = std::process::Command::new(program);
    cmd.args(["inspect", "-f", "{{range .Config.Env}}{{println .}}{{end}}", container]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    match output_bounded(cmd, timeout) {
        Some(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout);
            text.lines().any(|l| l.starts_with(ename_prefix.as_str()))
        }
        _ => false,
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

    #[test]
    fn env_frozen_with_missing_binary_degrades_to_false() {
        // No such executable -> the spawn itself fails -> degrades to `false`,
        // matching the bash oracle's docker-down fallback. No live docker/
        // server needed (this is the whole point of the degrade rule).
        let program = std::ffi::OsStr::new("dml-b2a-definitely-not-a-real-binary-xyz");
        assert!(!env_frozen_with(
            program,
            "ac-worldserver",
            "AC_RATE_XP_KILL",
            std::time::Duration::from_millis(500)
        ));
    }
}
