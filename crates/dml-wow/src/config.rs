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
//!
//! WRITES. Everything from [`cfg_installed_err`] down is the `config set` /
//! `config raw-read` / `config raw-write` half, moved out of the launcher's
//! `lib.rs` by the cargo-workspace refactor (Task 9b) so the standalone CLI
//! can drive it too: [`config_set`] routes a key to [`config_set_direct`]
//! (the `conf:` module-conf route) or [`config_set_curated`] (the registry
//! route, including `server.motd`'s SOAP path and the ahbot/bots.population
//! companion writes), and [`raw_read`]/[`raw_write`] are the raw file
//! editor. Every writer takes the caller's `config_lock` (and, where SOAP is
//! involved, `soap_lock`) as a plain `Arc<Mutex<()>>` — std, not Tauri.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use dml_core::error::CmdError;
use dml_core::proc::output_bounded;

use super::db::{db_err_to_cmd, sql_row_int};
use super::soap_cmds::motd_result;

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
    /// The BASE compose file's `ac-worldserver` environment, kept SEPARATE
    /// from `env_map` on purpose — only [`ConfigReader::database_info_value`]
    /// may read it. See that method for why [`ConfigReader::compute_value`]
    /// must not.
    base_env_map: HashMap<String, String>,
    conf_cache: HashMap<PathBuf, HashMap<String, String>>,
}

impl ConfigReader {
    /// Title dir for the one title this crate drives — `<games dir>/
    /// wow-server-playerbots`, matching bash's `_wow_server_dir`.
    ///
    /// A DELEGATION, not a resolution. It used to carry its own copy of the
    /// games-dir lookup, and that copy fell back to `PathBuf::from(".")` — the
    /// process's current working directory — while
    /// [`dml_core::compose::games_dir_from_env`] and bash's
    /// `${DML_GAMES_DIR:-$HOME/games}` (`cli/src/00-head.sh:9`) both fall back
    /// to `$HOME/games`. For a bare CLI invocation nothing exports
    /// `DML_GAMES_DIR` and a Windows-side value does not cross the `wsl.exe`
    /// boundary (no `WSLENV` entry), so the fallback is what runs, and the
    /// cwd is never the games dir. The result was the worst available shape:
    /// `ok:true` with registry DEFAULTS — the Config page reporting 1x XP on a
    /// 3x server, 500 bots on a 2000-bot server, with no error, no warning and
    /// no empty result to notice (live differential smoke, 2026-08-04, on the
    /// arch-backend sibling branch).
    ///
    /// Roughly twenty call sites across this crate route through here (config,
    /// tuning, modules, ahbot, bridge, lan, maint, modmgr, restore,
    /// destructive, and `db::DbConfig::from_env`, which is why the DB verbs
    /// would otherwise dial 3306 instead of the port in the title's own
    /// `.env`). They are all fixed by this one line, and they must stay that
    /// way: the games dir is decided ONCE, in `dml_core::compose`.
    pub fn title_dir_from_env() -> PathBuf {
        dml_core::compose::title_dir_for_id(TITLE)
    }

    /// Construct from an explicit title dir, eagerly loading the override env
    /// map (and, separately, the base compose's — see [`Self::database_info_value`]).
    pub fn for_title(title_dir: impl Into<PathBuf>) -> Self {
        let title_dir = title_dir.into();
        let override_path = title_dir.join("docker-compose.override.yml");
        let env_map = std::fs::read_to_string(&override_path)
            .map(|t| parse_override_env(&t))
            .unwrap_or_default();
        // Which file IS the base compose is `dml_core::compose`'s decision
        // (four recognised names), not a second literal here.
        let base_env_map = dml_core::compose::compose_file_name(&title_dir)
            .and_then(|name| std::fs::read_to_string(title_dir.join(name)).ok())
            .map(|t| parse_override_env(&t))
            .unwrap_or_default();
        ConfigReader { title_dir, env_map, base_env_map, conf_cache: HashMap::new() }
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

    /// The live value of one `*DatabaseInfo` conf key
    /// (`LoginDatabaseInfo`/`WorldDatabaseInfo`/`CharacterDatabaseInfo`),
    /// resolved the way the SERVER resolves it — which is NOT the way
    /// [`Self::compute_value`] does.
    ///
    /// Same three tiers, in the same order (env → live conf → `.dist`), and
    /// the same [`env_name_for`] derivation, but the env tier is the FULL
    /// compose environment for `ac-worldserver`: `docker-compose.override.yml`
    /// first, then the BASE compose file. That difference is load-bearing in
    /// BOTH directions:
    ///
    ///  * Every DML-generated native install writes
    ///    `AC_LOGIN_DATABASE_INFO`/`AC_WORLD_DATABASE_INFO`/
    ///    `AC_CHARACTER_DATABASE_INFO` into the BASE compose
    ///    (`data/native-compose.yml.tmpl`), never into the override — and an
    ///    `AC_*` env var OVERRIDES the matching conf key (composegen's
    ///    shadowing rule). Reading only the conf therefore answers with a value
    ///    worldserver PROVABLY never reads: on a real install the on-disk
    ///    `worldserver.conf` is byte-identical to its stock `.dist` and still
    ///    says `127.0.0.1;3306;acore;acore;acore_auth` — a host that container
    ///    cannot even reach. Renaming a schema the only way a native install
    ///    exposes (edit the compose env, `docker compose up`) has to be visible
    ///    here or the reader connects to the wrong database.
    ///  * [`Self::compute_value`] must NOT gain the base map in exchange: it is
    ///    a parity port of bash `_cfg_env_load_map` (40-config.sh:189), which
    ///    loads the OVERRIDE alone, and the `config list` parity suite pins it.
    ///
    /// `None` — never a guessed name — when no tier answers.
    pub fn database_info_value(&mut self, conf_key: &str) -> Option<String> {
        let ename = env_name_for(conf_key);
        for map in [&self.env_map, &self.base_env_map] {
            if let Some(v) = map.get(&ename).filter(|v| !v.is_empty()) {
                return Some(v.clone());
            }
        }
        // `conf_path_in` has NO `.dist` fallback of its own — every caller adds
        // one (`compute_value` above, `tuning.rs`) — and this one must too: a
        // `worldserver.conf` that exists but omits a `*DatabaseInfo` key is
        // LEGAL, because AC merges the `.conf` over the `.conf.dist`.
        let path = self.conf_path("worldserver.conf");
        let live = self.conf_value(&path, conf_key);
        if !live.is_empty() {
            return Some(live);
        }
        let dist = path.with_file_name("worldserver.conf.dist");
        let from_dist = self.conf_value(&dist, conf_key);
        if from_dist.is_empty() { None } else { Some(from_dist) }
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

    /// Single-row sibling of [`ConfigReader::assemble`]: the ONE registry row
    /// whose `key` matches, with its live `value` filled in exactly as
    /// `assemble` would, or `None` when no such row exists. Added for the
    /// standalone CLI's `config get <KEY>` (Task 11), which maps `None` to a
    /// `NOT_FOUND` envelope; the bash CLI has no single-row arm, so this is
    /// deliberately just a filter over the same registry + the same
    /// [`ConfigReader::compute_value`] — never a second resolution path.
    pub fn assemble_key(&mut self, registry_rows: &[Value], key: &str) -> Option<Value> {
        let row = registry_rows
            .iter()
            .find(|r| r.get("key").and_then(Value::as_str) == Some(key))?;
        let assembled = self.assemble(std::slice::from_ref(row));
        assembled
            .get("settings")
            .and_then(Value::as_array)
            .and_then(|rows| rows.first())
            .cloned()
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

// ---------------------------------------------------------------------------
// NATIVE-MODE `config set` / `config raw-read` / `config raw-write` — moved
// out of the launcher's `lib.rs` by the cargo-workspace refactor (Task 9b).
//
// Shared `_cfg_preamble` equivalent: every arm below needs the WoW Playerbots
// title installed (native has no yq dependency to check). Read paths take no
// lock; every writer takes the caller's `config_lock`, because Settings,
// Module Tuning and the raw editor can all target the same conf file.
//
// `config set` routes like the oracle does (`90-main.sh:2344-2561`):
//   A) `key` starts with `conf:` — a DIRECT module-conf write, no registry
//      row involved (`config_set_direct`).
//   B) otherwise — a CURATED registry-row write: SOAP for `server.motd`, a
//      conf-file write for `conf:` env columns (with `ahbot.character` and
//      `bots.population`'s special-case companion writes), or an override-env
//      write for anything else (`config_set_curated`).
// Both routes share `env_frozen` (the `_cfg_env_frozen` port) to decide
// whether a legacy AC_* env still beats the conf in the RUNNING container.
// ---------------------------------------------------------------------------

/// The four fields every `config set` WRITE route answers with, built in ONE
/// place so the two routes cannot drift -- port of the twin bash blocks
/// (`90-main.sh:2962-3003` direct, `3115-3131` curated).
///
/// `apply_needed` exists because a bare `restart_required:true` was not enough
/// information, and the gap cost a real bug: the launcher's restart button calls
/// world-restart, i.e. `docker restart -t 300 ac-worldserver`, which restarts
/// the SAME container -- and a container's environment is fixed when it is
/// CREATED. So when a save REMOVES a legacy AC_* env key that was shadowing
/// this conf row, the file ends up correct, the container keeps the old value
/// anyway, and the user watches the change revert (found live on the Bot World
/// page, five times, 2026-07-29). `env_was` already records that an env key was
/// present (in the override, or still frozen into the running container), so
/// this needs no docker call of its own.
///
/// NB do NOT "fix" this by adding `compose up -d` to world-restart: recreation
/// stops the container on compose's short default timeout, and `-t 300` exists
/// precisely so the world can save every character first.
fn cfgset_outcome(changed: bool, applied: &str, env_was: bool) -> serde_json::Value {
    // `applied == "live"` means the running world has already re-read the conf,
    // so nothing further is needed.
    let (restart_required, apply_needed) = if !changed || applied == "live" {
        (false, "none")
    } else if env_was {
        (true, "recreate")
    } else {
        (true, "world-restart")
    };
    serde_json::json!({
        "changed": changed,
        "restart_required": restart_required,
        "applied": applied,
        "apply_needed": apply_needed,
    })
}

pub fn cfg_installed_err() -> CmdError {
    CmdError {
        code: "NOT_FOUND".into(),
        message: "WoW Playerbots server not installed".into(),
        hint: "Install it first, then re-run.".into(),
    }
}

pub fn cfg_not_editable_err(fname: &str) -> CmdError {
    CmdError {
        code: "NOT_FOUND".into(),
        message: format!("Not an editable file: {fname}"),
        hint: "See: dml wow config files --json".into(),
    }
}

/// `[[ -n "$fname" ]] || { json_err BAD_ARG "Missing --file <name>" ""; exit 1; }`
/// -- shared verbatim by the raw-read/raw-reset/raw-write arms
/// (`90-main.sh:2695,2714,2735`; NOTE the EMPTY hint here, unlike conf-keys'
/// own missing-file message which carries a "See: ..." hint).
pub fn cfg_missing_file_err() -> CmdError {
    CmdError { code: "BAD_ARG".into(), message: "Missing --file <name>".into(), hint: String::new() }
}

/// The oracle's `raw-read` arm never reads the file directly -- it captures
/// it through a `$(cat "$fpath")` command substitution
/// (`90-main.sh:2702,2706`), and POSIX command substitution strips EVERY
/// trailing newline from the captured text (not just one). A plain
/// `std::fs::read_to_string` keeps the file's real trailing newline(s), so
/// without this the native reader would hand back one extra `\n` at EOF for
/// any conf that (like every conf in this codebase) ends in a newline --
/// caught live by `part5a_parity.rs`.
pub fn strip_command_sub_trailing_newlines(s: &str) -> &str {
    s.trim_end_matches('\n')
}

/// Route A — the direct `conf:` route (`90-main.sh:2354-2438`). `full_key` is
/// the ORIGINAL `--key` value (including the `conf:` prefix), used verbatim in
/// the "Bad conf key" message exactly like the oracle.
pub fn config_set_direct(
    title_dir: &std::path::Path,
    full_key: &str,
    value: &str,
    soap_lock: &Arc<std::sync::Mutex<()>>,
) -> Result<serde_json::Value, CmdError> {
    let Some((conf_file, conf_key)) = crate::config::route_conf(full_key) else {
        return Err(cfgset_err("BAD_ARG", format!("Bad conf key: {full_key}"), ""));
    };
    if crate::config::is_core_conf_file(&conf_file) {
        return Err(cfgset_err(
            "BAD_ARG",
            "Direct conf keys are limited to module confs",
            "Core server settings live in the curated list: dml wow config list --json",
        ));
    }
    if !crate::config::is_valid_direct_conf_key(&conf_key) {
        return Err(cfgset_err(
            "BAD_ARG",
            format!("Invalid conf key: {conf_key}"),
            "Letters, digits, dots and underscores only.",
        ));
    }
    if crate::config::is_denylisted_direct_key(&conf_key) {
        return Err(cfgset_err(
            "BAD_ARG",
            format!("{conf_key} is managed by the bot flush tool"),
            "Use: dml wow bots flush --yes --ack flush (backs your characters up first and always disarms the flag afterwards).",
        ));
    }
    if !crate::config::is_single_line(value) {
        return Err(cfgset_err("BAD_ARG", "The value must be a single line", ""));
    }
    if !crate::config::within_max_len(value, 200) {
        return Err(cfgset_err("BAD_ARG", "Value too long (max 200 characters)", ""));
    }

    let cpath = crate::config::direct_conf_path(title_dir, &conf_file).ok_or_else(|| {
        cfgset_err(
            "NOT_FOUND",
            format!("Not an editable module conf: {conf_file}"),
            "See: dml wow config files --json",
        )
    })?;
    let ensured = crate::config::conf_ensure(&cpath)
        .map_err(|e| cfgset_err("WRITE_FAILED", format!("Could not write {conf_file}: {e}"), ""))?;
    if !ensured {
        return Err(cfgset_err(
            "NOT_FOUND",
            format!("{conf_file} not found (nor its .dist)"),
            "Is the WoW server fully installed?",
        ));
    }
    let mut changed = crate::config::conf_write(&cpath, &conf_key, value)
        .map_err(|e| cfgset_err("WRITE_FAILED", format!("Could not write {conf_file}: {e}"), ""))?;

    let ename = crate::config::env_name_for(&conf_key);
    let override_path = title_dir.join("docker-compose.override.yml");
    let mut env_was = cfgset_clean_legacy_env(&override_path, &ename)?;
    if env_was {
        changed = true;
    }

    let mut applied = "none".to_string();
    if changed {
        applied = "restart".to_string();
        if let Some(reload_cmd) = crate::config::conf_reload_cmd(&conf_file) {
            if !env_was && env_frozen(&ename) {
                env_was = true;
            }
            if !env_was {
                let _guard = soap_lock.lock().unwrap_or_else(|e| e.into_inner());
                let soap_cfg = crate::soap::SoapConfig::load();
                let outcome = crate::soap::exec(&soap_cfg, reload_cmd);
                if matches!(outcome, crate::soap::SoapOutcome::Ok(_)) {
                    applied = "live".to_string();
                }
            }
        }
    }
    Ok(cfgset_outcome(changed, &applied, env_was))
}

/// Route B — the curated registry-row route (`90-main.sh:2440-2560`).
pub fn config_set_curated(
    title_dir: &std::path::Path,
    key: &str,
    raw_value: &str,
    rows: &[serde_json::Value],
    soap_lock: &Arc<std::sync::Mutex<()>>,
) -> Result<serde_json::Value, CmdError> {
    let row = rows
        .iter()
        .find(|r| r.get("key").and_then(|v| v.as_str()) == Some(key))
        .ok_or_else(|| {
            cfgset_err("NOT_FOUND", format!("Unknown setting: {key}"), "See: dml wow config list --json")
        })?;
    let kind = row.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let label = row.get("label").and_then(|v| v.as_str()).unwrap_or(key);
    let env = row.get("env").and_then(|v| v.as_str()).unwrap_or("");
    let min_num = row.get("min").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let max_num = row.get("max").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let min_disp = row.get("min").cloned().unwrap_or(serde_json::Value::Null);
    let max_disp = row.get("max").cloned().unwrap_or(serde_json::Value::Null);

    // `_cfg_preamble` (`40-config.sh:161-174`), inlined: the oracle resolves
    // `cfg_sdir` right after parsing the row and BEFORE any type/range
    // validation (`90-main.sh:2440-2445`), so a not-installed server always
    // wins over a bad value with the SAME top-level verdict the oracle gives.
    if !crate::config::wow_server_installed(title_dir) {
        return Err(cfgset_err(
            "NOT_FOUND",
            "WoW Playerbots server not installed",
            "Install it first, then re-run.",
        ));
    }

    let mut value = raw_value.to_string();
    match kind {
        "float" => {
            if !crate::config::float_in_range(&value, min_num, max_num) {
                return Err(cfgset_err(
                    "BAD_ARG",
                    format!("{label} must be a number between {min_disp} and {max_disp}, got: {value}"),
                    "",
                ));
            }
        }
        "int" => {
            if !crate::config::int_in_range(&value, min_num as i64, max_num as i64) {
                return Err(cfgset_err(
                    "BAD_ARG",
                    format!("{label} must be a whole number between {min_disp} and {max_disp}, got: {value}"),
                    "",
                ));
            }
        }
        "bool" => {
            if !crate::config::is_bool01(&value) {
                return Err(cfgset_err(
                    "BAD_ARG",
                    format!("{label} takes 1 (on) or 0 (off), got: {value}"),
                    "",
                ));
            }
        }
        "text" => {
            value = crate::config::sanitize_text_value(&value);
        }
        "char" => {
            if !crate::soap_cmds::valid_charname(&value) {
                return Err(cfgset_err(
                    "BAD_ARG",
                    format!("Invalid character name: {value}"),
                    "1-12 letters/digits/underscore.",
                ));
            }
        }
        _ => {}
    }

    if key == "server.motd" {
        let cmd = crate::soap_cmds::motd_cmd(&value);
        let _guard = soap_lock.lock().unwrap_or_else(|e| e.into_inner());
        let soap_cfg = crate::soap::SoapConfig::load();
        let outcome = crate::soap::exec(&soap_cfg, &cmd);
        motd_result(outcome)?;
        return Ok(serde_json::json!({ "changed": true, "restart_required": false }));
    }

    if let Some((conf_file, conf_key)) = crate::config::route_conf(env) {
        let mut write_value = value.clone();
        let mut extra_writes: Vec<(String, String)> = Vec::new();

        if key == "ahbot.character" {
            let db_cfg = crate::db::DbConfig::from_env();
            let params: Vec<mysql::Value> = vec![mysql::Value::from(value.as_str())];
            let res = crate::db::query_with_params(
                &db_cfg,
                crate::db::Database::Characters,
                "SELECT guid, account FROM characters WHERE name = ? LIMIT 1",
                params,
            )
            .map_err(db_err_to_cmd)?;
            let row0 = res
                .rows
                .first()
                .ok_or_else(|| cfgset_err("NOT_FOUND", format!("No such character: {value}"), ""))?;
            let guid = sql_row_int(row0.first()).filter(|g| *g >= 0);
            let acct = sql_row_int(row0.get(1)).filter(|a| *a >= 0);
            let (guid, acct) = match (guid, acct) {
                (Some(g), Some(a)) => (g, a),
                _ => {
                    return Err(cfgset_err(
                        "DB_UNREACHABLE",
                        "Unexpected character lookup result",
                        "",
                    ))
                }
            };
            write_value = guid.to_string();
            extra_writes.push(("AuctionHouseBot.Account".to_string(), acct.to_string()));
        }

        let cpath = crate::config::conf_path_in(title_dir, &conf_file);
        let ensured = crate::config::conf_ensure(&cpath)
            .map_err(|e| cfgset_err("WRITE_FAILED", format!("Could not write {conf_file}: {e}"), ""))?;
        if !ensured {
            return Err(cfgset_err(
                "NOT_FOUND",
                format!("{conf_file} not found (nor its .dist)"),
                "Is the WoW server fully installed?",
            ));
        }
        let mut changed = crate::config::conf_write(&cpath, &conf_key, &write_value)
            .map_err(|e| cfgset_err("WRITE_FAILED", format!("Could not write {conf_file}: {e}"), ""))?;

        for (k, v) in &extra_writes {
            let c = crate::config::conf_write(&cpath, k, v)
                .map_err(|e| cfgset_err("WRITE_FAILED", format!("Could not write {conf_file}: {e}"), ""))?;
            changed = changed || c;
        }
        if key == "bots.population" {
            let c = crate::config::conf_write(&cpath, "AiPlayerbot.MinRandomBots", &write_value)
                .map_err(|e| cfgset_err("WRITE_FAILED", format!("Could not write {conf_file}: {e}"), ""))?;
            changed = changed || c;
        }

        let mut envnames = vec![crate::config::env_name_for(&conf_key)];
        if key == "bots.population" {
            envnames.push(crate::config::env_name_for("AiPlayerbot.MinRandomBots"));
        }
        if key == "ahbot.character" {
            envnames.push(crate::config::env_name_for("AuctionHouseBot.Account"));
        }

        let override_path = title_dir.join("docker-compose.override.yml");
        let mut env_was = false;
        for ename in &envnames {
            if cfgset_clean_legacy_env(&override_path, ename)? {
                env_was = true;
                changed = true;
            }
        }
        if !env_was {
            for ename in &envnames {
                if env_frozen(ename) {
                    env_was = true;
                    break;
                }
            }
        }

        let mut applied = "none".to_string();
        if changed {
            applied = "restart".to_string();
            if (conf_file == "worldserver.conf" || conf_file == "mod_ahbot.conf") && !env_was {
                let _guard = soap_lock.lock().unwrap_or_else(|e| e.into_inner());
                let soap_cfg = crate::soap::SoapConfig::load();
                let outcome = crate::soap::exec(&soap_cfg, "reload config");
                if matches!(outcome, crate::soap::SoapOutcome::Ok(_)) {
                    applied = "live".to_string();
                }
            }
        }
        return Ok(cfgset_outcome(changed, &applied, env_was));
    }

    // Non-conf env column (currently unreachable — every real registry row is
    // either `conf:` or `server.motd`'s `-`; kept for oracle parity).
    let override_path = title_dir.join("docker-compose.override.yml");
    let changed = crate::config::override_env_write(&override_path, env, &value)
        .map_err(|e| cfgset_err("WRITE_FAILED", format!("Could not write the config override: {e}"), ""))?;
    Ok(serde_json::json!({ "changed": changed, "restart_required": changed }))
}

/// NATIVE-MODE `config files` (`90-main.sh:2670-2691`, Batch 1 F3): the
/// dynamic editable-file list that powers the GUI/CLI file picker — the fixed
/// four (`.env`, the compose override, `worldserver.conf`, `authserver.conf`)
/// plus every `*.conf` basename found under `env/dist/etc/modules/`, whether
/// it exists as the conf itself or only as its `.conf.dist`. Read path, so it
/// takes no lock. Added by Task 11: `raw_read`/`raw_write` were hoisted in
/// Task 9b but their `files` sibling was still bash-only, and the standalone
/// CLI needs all three.
///
/// Faithful to the oracle pipeline, step for step:
///  - `ls -1 <moddir>` (files AND dirs; a dir simply fails the shape gate),
///  - `sed 's/\.dist$//'` — strip ONE trailing `.dist`, so `foo.conf.dist`
///    and `foo.conf` both fold to `foo.conf`,
///  - `grep -E '^[A-Za-z0-9_.-]+\.conf$'` — the same shape gate
///    [`is_module_conf_name`] applies (this drops `lua_scripts`,
///    `transmog.conf.bak`, …),
///  - `grep -vE '^(worldserver|authserver)\.conf$'` — those two are emitted
///    from the fixed list instead, one directory up,
///  - `sort -u`. Rust's `BTreeSet<String>` orders by BYTE value, i.e. the C
///    locale. Every module conf AC ships is lowercase ASCII, so this only
///    diverges from a locale-aware `sort` for a hypothetical mixed-case
///    module conf name.
///
/// Each name is then resolved through [`cfg_file_path`] (an unresolvable name
/// is skipped, like the oracle's `|| continue`), and reported as
/// `{name, exists, dist, readonly}` — `readonly` being true for exactly the
/// two files [`raw_write`] refuses to overwrite.
pub fn config_files(title_dir: &Path) -> Result<serde_json::Value, CmdError> {
    if !wow_server_installed(title_dir) {
        return Err(cfg_installed_err());
    }
    let moddir = title_dir.join("env").join("dist").join("etc").join("modules");
    let mut dynnames: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    if let Ok(rd) = std::fs::read_dir(&moddir) {
        for entry in rd.flatten() {
            let raw = entry.file_name().to_string_lossy().to_string();
            let name = raw.strip_suffix(".dist").unwrap_or(&raw).to_string();
            if !is_module_conf_name(&name) {
                continue;
            }
            if name == "worldserver.conf" || name == "authserver.conf" {
                continue;
            }
            dynnames.insert(name);
        }
    }

    let fixed = [".env", "docker-compose.override.yml", "worldserver.conf", "authserver.conf"];
    let mut files = Vec::with_capacity(fixed.len() + dynnames.len());
    for name in fixed.iter().map(|s| s.to_string()).chain(dynnames) {
        let Some(fpath) = cfg_file_path(title_dir, &name) else {
            continue;
        };
        files.push(serde_json::json!({
            "name": name,
            "exists": fpath.is_file(),
            "dist": dist_sibling(&fpath).is_file(),
            "readonly": matches!(name.as_str(), ".env" | "docker-compose.override.yml"),
        }));
    }
    Ok(serde_json::json!({ "files": files }))
}

/// NATIVE-MODE `config raw-read` (`90-main.sh:2692-2707`, Part 5a): read an
/// allowlisted file, falling back to its `.dist` when only that exists yet
/// (the first save then creates the real conf via [`raw_write`]). Read path,
/// so it takes no config lock. Moved out of the launcher's `lib.rs` by the
/// cargo-workspace refactor (Task 9b).
pub fn raw_read(file: String) -> Result<serde_json::Value, CmdError> {
    if file.is_empty() {
        return Err(cfg_missing_file_err());
    }
    let title_dir = crate::config::ConfigReader::title_dir_from_env();
    if !crate::config::wow_server_installed(&title_dir) {
        return Err(cfg_installed_err());
    }
    let fpath = crate::config::cfg_file_path(&title_dir, &file)
        .ok_or_else(|| cfg_not_editable_err(&file))?;
    let dist = crate::config::dist_sibling(&fpath);
    if !fpath.is_file() && dist.is_file() {
        let content = std::fs::read_to_string(&dist).unwrap_or_default();
        return Ok(serde_json::json!({
            "file": file,
            "source": "dist",
            "content": strip_command_sub_trailing_newlines(&content),
        }));
    }
    if !fpath.is_file() {
        return Err(CmdError {
            code: "NOT_FOUND".into(),
            message: format!("File does not exist yet: {file}"),
            hint: String::new(),
        });
    }
    let content = std::fs::read_to_string(&fpath).unwrap_or_default();
    Ok(serde_json::json!({
        "file": file,
        "source": "conf",
        "content": strip_command_sub_trailing_newlines(&content),
    }))
}

/// NATIVE-MODE `config raw-write` (`90-main.sh:2733-2774`, Part 5a):
/// full-file write with the arm's exact guards, in the SAME order as the
/// oracle -- override-YAML syntax validation happens BEFORE the `.env`/
/// override read-only rejection (so submitting broken YAML for the override
/// reports "not valid YAML", never "read-only"), tmp+rename atomic, with an
/// automatic `.bak` of any existing file. THAT ORDER IS LOAD-BEARING; the
/// SECURITY comment inside says why the two files are read-only at all.
/// `config_lock` serializes against every other native conf/override write
/// (Settings, Module Tuning and the raw editor can all target one file) --
/// the caller supplies it, same shape as every other hoisted body that
/// needs it. Moved out of the launcher's `lib.rs` (Task 9b).
pub fn raw_write(
    file: String,
    content: String,
    config_lock: Arc<Mutex<()>>,
) -> Result<serde_json::Value, CmdError> {
    if file.is_empty() {
        return Err(cfg_missing_file_err());
    }
    let _guard = config_lock.lock().unwrap_or_else(|e| e.into_inner());
    let title_dir = crate::config::ConfigReader::title_dir_from_env();
    if !crate::config::wow_server_installed(&title_dir) {
        return Err(cfg_installed_err());
    }
    let fpath = crate::config::cfg_file_path(&title_dir, &file)
        .ok_or_else(|| cfg_not_editable_err(&file))?;

    if file == "docker-compose.override.yml"
        && serde_yaml_ng::from_str::<serde_yaml_ng::Value>(&content).is_err()
    {
        return Err(CmdError {
            code: "BAD_ARG".into(),
            message: "That is not valid YAML - not saved".into(),
            hint: "Fix the syntax and save again.".into(),
        });
    }
    // SECURITY: same posture as `_cfg_file_path`'s writable module-conf
    // allowlist -- `.env`/the compose override are readable (raw-read)
    // but NOT writable here (see `cli/src/90-main.sh:2752-2765`'s own
    // comment for the exact rationale: this + `games restart` would let
    // the editor drive host command execution).
    if matches!(file.as_str(), ".env" | "docker-compose.override.yml") {
        return Err(CmdError {
            code: "BAD_ARG".into(),
            message: "That file is read-only in the editor".into(),
            hint: "Change these settings from the Settings tab; .env and the compose override can't be overwritten here.".into(),
        });
    }

    if let Some(parent) = fpath.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut backup = serde_json::Value::Null;
    if fpath.is_file() {
        let bak = crate::config::bak_sibling(&fpath);
        std::fs::copy(&fpath, &bak).map_err(|e| CmdError {
            code: "WRITE_FAILED".into(),
            message: format!("Could not write {file}: {e}"),
            hint: String::new(),
        })?;
        backup = serde_json::Value::String(format!("{file}.bak"));
    }
    crate::config::atomic_write(&fpath, &content).map_err(|e| CmdError {
        code: "WRITE_FAILED".into(),
        message: format!("Could not write {file}: {e}"),
        hint: String::new(),
    })?;
    Ok(serde_json::json!({ "written": true, "backup": backup }))
}

/// `dml wow config set` port (`90-main.sh:2344-2561`, Task B2a): the route
/// decision itself -- a `conf:`-prefixed key goes to [`config_set_direct`],
/// anything else to [`config_set_curated`] against the registry rows. Moved
/// out of the launcher's `lib.rs` by the cargo-workspace refactor (Task 9b);
/// both locks and `title_dir` are resolved by the caller, none of them Tauri
/// types.
pub fn config_set(
    key: String,
    value: String,
    soap_lock: Arc<Mutex<()>>,
    config_lock: Arc<Mutex<()>>,
    title_dir: PathBuf,
) -> Result<serde_json::Value, CmdError> {
    // Serializes against every other native conf/override write (Settings
    // AND Module Tuning can both target the same conf file) -- see the
    // doc comment on `AppState::config_lock`.
    let _guard = config_lock.lock().unwrap_or_else(|e| e.into_inner());
    if key.starts_with("conf:") {
        return config_set_direct(&title_dir, &key, &value, &soap_lock);
    }
    let rows = crate::registry::config_registry_rows();
    config_set_curated(&title_dir, &key, &value, rows, &soap_lock)
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

    #[test]
    fn strip_command_sub_trailing_newlines_strips_every_trailing_newline() {
        assert_eq!(strip_command_sub_trailing_newlines("A = 1\n"), "A = 1");
        // Command substitution strips EVERY trailing newline, not just one.
        assert_eq!(strip_command_sub_trailing_newlines("A = 1\n\n\n"), "A = 1");
        assert_eq!(strip_command_sub_trailing_newlines("A = 1"), "A = 1");
        assert_eq!(strip_command_sub_trailing_newlines(""), "");
        // Only trailing -- interior blank lines are untouched.
        assert_eq!(strip_command_sub_trailing_newlines("A = 1\n\nB = 2\n"), "A = 1\n\nB = 2");
        // CR right before the final \n is not itself a newline char -- left alone.
        assert_eq!(strip_command_sub_trailing_newlines("A = 1\r\n"), "A = 1\r");
    }

    // -- config_set_direct / config_set_curated ---------------------------

    struct TmpTitleDir(std::path::PathBuf);
    impl TmpTitleDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("dml-b2a-cfgset-test-{}-{}", std::process::id(), name));
            let _ = std::fs::remove_dir_all(&dir);
            let modules = dir.join("env").join("dist").join("etc").join("modules");
            std::fs::create_dir_all(&modules).unwrap();
            // Marks the title as "installed" for `wow_server_installed`'s
            // compose-file check, matching a real title dir.
            std::fs::write(dir.join("docker-compose.yml"), "services: {}\n").unwrap();
            TmpTitleDir(dir)
        }
        fn modules_dir(&self) -> std::path::PathBuf {
            self.0.join("env").join("dist").join("etc").join("modules")
        }
        fn write_module_conf(&self, name: &str, content: &str) {
            std::fs::write(self.modules_dir().join(name), content).unwrap();
        }
        fn write_override(&self, yaml: &str) {
            std::fs::write(self.0.join("docker-compose.override.yml"), yaml).unwrap();
        }
        fn read_module_conf(&self, name: &str) -> String {
            std::fs::read_to_string(self.modules_dir().join(name)).unwrap()
        }
    }
    impl Drop for TmpTitleDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn no_soap_lock() -> Arc<std::sync::Mutex<()>> {
        Arc::new(std::sync::Mutex::new(()))
    }

    #[test]
    fn config_set_direct_rejects_core_conf() {
        let t = TmpTitleDir::new("core");
        let e = config_set_direct(&t.0, "conf:Rate.XP.Kill", "3", &no_soap_lock()).unwrap_err();
        assert_eq!(e.code, "BAD_ARG");
        assert_eq!(e.message, "Direct conf keys are limited to module confs");
        assert_eq!(
            e.hint,
            "Core server settings live in the curated list: dml wow config list --json"
        );
    }

    #[test]
    fn config_set_direct_rejects_denylisted_key() {
        let t = TmpTitleDir::new("denylist");
        let e = config_set_direct(
            &t.0,
            "conf:playerbots.conf:AiPlayerbot.DeleteRandomBotAccounts",
            "1",
            &no_soap_lock(),
        )
        .unwrap_err();
        assert_eq!(e.code, "BAD_ARG");
        assert_eq!(
            e.message,
            "AiPlayerbot.DeleteRandomBotAccounts is managed by the bot flush tool"
        );
        assert!(e.hint.starts_with("Use: dml wow bots flush"));
    }

    #[test]
    fn config_set_direct_rejects_bad_conf_key_shape() {
        let t = TmpTitleDir::new("shape");
        let e =
            config_set_direct(&t.0, "conf:playerbots.conf:Has Space", "1", &no_soap_lock()).unwrap_err();
        assert_eq!(e.code, "BAD_ARG");
        assert_eq!(e.message, "Invalid conf key: Has Space");
        assert_eq!(e.hint, "Letters, digits, dots and underscores only.");
    }

    #[test]
    fn config_set_direct_rejects_multiline_and_overlong_values() {
        let t = TmpTitleDir::new("valueshape");
        let e = config_set_direct(&t.0, "conf:playerbots.conf:AiPlayerbot.Foo", "a\nb", &no_soap_lock())
            .unwrap_err();
        assert_eq!(e.code, "BAD_ARG");
        assert_eq!(e.message, "The value must be a single line");

        let long = "x".repeat(201);
        let e =
            config_set_direct(&t.0, "conf:playerbots.conf:AiPlayerbot.Foo", &long, &no_soap_lock())
                .unwrap_err();
        assert_eq!(e.code, "BAD_ARG");
        assert_eq!(e.message, "Value too long (max 200 characters)");
    }

    #[test]
    fn config_set_direct_not_found_when_conf_and_dist_both_absent() {
        let t = TmpTitleDir::new("notfound");
        let e =
            config_set_direct(&t.0, "conf:ghost.conf:Some.Key", "1", &no_soap_lock()).unwrap_err();
        assert_eq!(e.code, "NOT_FOUND");
        assert_eq!(e.message, "Not an editable module conf: ghost.conf");
        assert_eq!(e.hint, "See: dml wow config files --json");
    }

    #[test]
    fn config_set_direct_writes_conf_and_reports_restart_when_no_reload_cmd() {
        let t = TmpTitleDir::new("success");
        t.write_module_conf("playerbots.conf", "AiPlayerbot.Foo = 1\n");
        let out =
            config_set_direct(&t.0, "conf:playerbots.conf:AiPlayerbot.Foo", "42", &no_soap_lock())
                .unwrap();
        assert_eq!(out["changed"], true);
        assert_eq!(out["restart_required"], true);
        assert_eq!(out["applied"], "restart");
        // Nothing shadowed this row, so restarting the world process is enough.
        assert_eq!(out["apply_needed"], "world-restart");
        assert_eq!(t.read_module_conf("playerbots.conf"), "AiPlayerbot.Foo = 42\n");
    }

    #[test]
    fn config_set_direct_cleans_legacy_env_and_still_reports_restart() {
        let t = TmpTitleDir::new("legacyenv");
        t.write_module_conf("playerbots.conf", "AiPlayerbot.Foo = 1\n");
        t.write_override(
            "services:\n  ac-worldserver:\n    environment:\n      AC_AI_PLAYERBOT_FOO: \"1\"\n",
        );
        let out =
            config_set_direct(&t.0, "conf:playerbots.conf:AiPlayerbot.Foo", "42", &no_soap_lock())
                .unwrap();
        assert_eq!(out["changed"], true);
        assert_eq!(out["restart_required"], true);
        assert_eq!(out["applied"], "restart");
        // THE REGRESSION: removing a shadowing AC_* env key cannot be applied by
        // restarting the same container, because its environment was fixed at
        // create time. Only a recreate (down->up) lands this save -- reporting
        // "world-restart" here is what made the user's change silently revert.
        assert_eq!(out["apply_needed"], "recreate");
        let text = std::fs::read_to_string(t.0.join("docker-compose.override.yml")).unwrap();
        assert!(!crate::config::parse_override_env(&text).contains_key("AC_AI_PLAYERBOT_FOO"));
    }

    fn curated_row(
        key: &str,
        kind: &str,
        min: serde_json::Value,
        max: serde_json::Value,
        env: &str,
        label: &str,
    ) -> serde_json::Value {
        serde_json::json!({
            "key": key, "group": "Test", "label": label, "explain": "x",
            "type": kind, "min": min, "max": max, "value": "", "default": "0",
            "restart_required": true, "env": env,
        })
    }

    #[test]
    fn config_set_curated_unknown_key_is_not_found() {
        let t = TmpTitleDir::new("curated-unknown");
        let rows = vec![curated_row("rates.xp_kill", "float", 0.5.into(), 20.into(), "conf:Rate.XP.Kill", "XP")];
        let e =
            config_set_curated(&t.0, "no.such.key", "1", &rows, &no_soap_lock()).unwrap_err();
        assert_eq!(e.code, "NOT_FOUND");
        assert_eq!(e.message, "Unknown setting: no.such.key");
        assert_eq!(e.hint, "See: dml wow config list --json");
    }

    #[test]
    fn config_set_curated_not_found_when_server_not_installed() {
        // A title dir that doesn't exist at all -- `wow_server_installed`
        // must fail closed, mirroring `_wow_server_dir` (`90-main.sh:106-110`)
        // returning empty when `$GAMES_DIR/wow-server-playerbots` is absent.
        let missing_dir = std::env::temp_dir()
            .join(format!("dml-b2a-cfgset-test-{}-not-installed-at-all", std::process::id()));
        let _ = std::fs::remove_dir_all(&missing_dir);
        let rows = vec![curated_row("rates.xp_kill", "float", 0.5.into(), 20.into(), "conf:Rate.XP.Kill", "XP")];
        let e = config_set_curated(&missing_dir, "rates.xp_kill", "3", &rows, &no_soap_lock())
            .unwrap_err();
        assert_eq!(e.code, "NOT_FOUND");
        assert_eq!(e.message, "WoW Playerbots server not installed");
        assert_eq!(e.hint, "Install it first, then re-run.");
    }

    #[test]
    fn config_set_curated_not_installed_check_wins_over_bad_arg() {
        // Server not installed AND the value is out of range -- the oracle's
        // `_cfg_preamble` (`90-main.sh:2443`) runs BEFORE the type/range
        // switch (`90-main.sh:2445`), so the top-level verdict must be
        // NOT_FOUND, never BAD_ARG.
        let missing_dir = std::env::temp_dir()
            .join(format!("dml-b2a-cfgset-test-{}-not-installed-badval", std::process::id()));
        let _ = std::fs::remove_dir_all(&missing_dir);
        let rows = vec![curated_row(
            "rates.xp_kill",
            "float",
            0.5.into(),
            20.into(),
            "conf:Rate.XP.Kill",
            "XP from kills",
        )];
        let e = config_set_curated(&missing_dir, "rates.xp_kill", "999.9", &rows, &no_soap_lock())
            .unwrap_err();
        assert_eq!(e.code, "NOT_FOUND");
        assert_eq!(e.message, "WoW Playerbots server not installed");
    }

    #[test]
    fn config_set_curated_motd_not_found_when_server_not_installed() {
        // The `server.motd` sub-branch has no conf-write of its own (it goes
        // straight to SOAP) -- confirm it still gets the install-check gate
        // instead of ever reaching the SOAP call.
        let missing_dir = std::env::temp_dir()
            .join(format!("dml-b2a-cfgset-test-{}-not-installed-motd", std::process::id()));
        let _ = std::fs::remove_dir_all(&missing_dir);
        let rows = vec![curated_row(
            "server.motd",
            "text",
            serde_json::Value::Null,
            serde_json::Value::Null,
            "-",
            "Message of the day",
        )];
        let e = config_set_curated(&missing_dir, "server.motd", "hi", &rows, &no_soap_lock())
            .unwrap_err();
        assert_eq!(e.code, "NOT_FOUND");
        assert_eq!(e.message, "WoW Playerbots server not installed");
    }

    #[test]
    fn config_set_curated_float_out_of_range_is_bad_arg() {
        let t = TmpTitleDir::new("curated-floatrange");
        let rows = vec![curated_row(
            "rates.xp_kill",
            "float",
            0.5.into(),
            20.into(),
            "conf:Rate.XP.Kill",
            "XP from kills",
        )];
        let e = config_set_curated(&t.0, "rates.xp_kill", "20.5", &rows, &no_soap_lock()).unwrap_err();
        assert_eq!(e.code, "BAD_ARG");
        assert_eq!(e.message, "XP from kills must be a number between 0.5 and 20, got: 20.5");
        assert_eq!(e.hint, "");
    }

    #[test]
    fn config_set_curated_bool_non01_is_bad_arg() {
        let t = TmpTitleDir::new("curated-bool");
        let rows = vec![curated_row(
            "crossfaction.group",
            "bool",
            serde_json::Value::Null,
            serde_json::Value::Null,
            "conf:AllowTwoSide.Interaction.Group",
            "Group across factions",
        )];
        let e =
            config_set_curated(&t.0, "crossfaction.group", "2", &rows, &no_soap_lock()).unwrap_err();
        assert_eq!(e.code, "BAD_ARG");
        assert_eq!(e.message, "Group across factions takes 1 (on) or 0 (off), got: 2");
    }

    #[test]
    fn config_set_curated_int_out_of_range_is_bad_arg() {
        let t = TmpTitleDir::new("curated-int");
        let rows = vec![curated_row(
            "bots.population",
            "int",
            0.into(),
            3000.into(),
            "conf:playerbots.conf:AiPlayerbot.MaxRandomBots",
            "World bot population",
        )];
        let e = config_set_curated(&t.0, "bots.population", "3001", &rows, &no_soap_lock()).unwrap_err();
        assert_eq!(e.code, "BAD_ARG");
        assert_eq!(
            e.message,
            "World bot population must be a whole number between 0 and 3000, got: 3001"
        );
    }

    #[test]
    fn config_set_curated_char_invalid_is_bad_arg() {
        let t = TmpTitleDir::new("curated-char");
        let rows = vec![curated_row(
            "ahbot.character",
            "char",
            serde_json::Value::Null,
            serde_json::Value::Null,
            "conf:mod_ahbot.conf:AuctionHouseBot.GUID",
            "Seller character",
        )];
        let e = config_set_curated(&t.0, "ahbot.character", "bad name!", &rows, &no_soap_lock())
            .unwrap_err();
        assert_eq!(e.code, "BAD_ARG");
        assert_eq!(e.message, "Invalid character name: bad name!");
        assert_eq!(e.hint, "1-12 letters/digits/underscore.");
    }

    #[test]
    fn config_set_curated_text_row_sanitizes_and_writes_conf() {
        // Deliberately NOT worldserver.conf/mod_ahbot.conf -- keeps this test
        // network-free (those two conf files are the only ones that attempt a
        // live SOAP `reload config`, which is smoke-gated per the brief).
        let t = TmpTitleDir::new("curated-text");
        t.write_module_conf("mod_something.conf", "Some.Text = old\n");
        let rows = vec![curated_row(
            "some.text",
            "text",
            serde_json::Value::Null,
            serde_json::Value::Null,
            "conf:mod_something.conf:Some.Text",
            "Some text",
        )];
        let out = config_set_curated(
            &t.0,
            "some.text",
            "has \"quotes\" and\nnewline",
            &rows,
            &no_soap_lock(),
        )
        .unwrap();
        assert_eq!(out["changed"], true);
        assert_eq!(out["restart_required"], true);
        assert_eq!(out["applied"], "restart");
        assert_eq!(t.read_module_conf("mod_something.conf"), "Some.Text = has quotes and newline\n");
    }

    #[test]
    fn config_set_curated_bots_population_writes_min_and_max() {
        let t = TmpTitleDir::new("curated-botspop");
        t.write_module_conf(
            "playerbots.conf",
            "AiPlayerbot.MaxRandomBots = 500\nAiPlayerbot.MinRandomBots = 500\n",
        );
        let rows = vec![curated_row(
            "bots.population",
            "int",
            0.into(),
            3000.into(),
            "conf:playerbots.conf:AiPlayerbot.MaxRandomBots",
            "World bot population",
        )];
        let out =
            config_set_curated(&t.0, "bots.population", "800", &rows, &no_soap_lock()).unwrap();
        assert_eq!(out["changed"], true);
        assert_eq!(out["applied"], "restart");
        let text = t.read_module_conf("playerbots.conf");
        assert!(text.contains("AiPlayerbot.MaxRandomBots = 800"));
        assert!(text.contains("AiPlayerbot.MinRandomBots = 800"));
        assert_eq!(out["apply_needed"], "world-restart");
    }

    /// The user-facing bug, in its original shape: the WSL installer writes BOTH
    /// population bounds as AC_* env keys into the compose override, so the very
    /// first Bot World save has to migrate them -- and a save that removes an env
    /// key needs the container RECREATED, which the launcher's restart button
    /// cannot do. Reported as "world-restart", the change appeared to revert.
    #[test]
    fn a_population_save_that_migrates_the_installers_env_keys_demands_a_recreate() {
        let t = TmpTitleDir::new("curated-botspop-shadowed");
        t.write_module_conf(
            "playerbots.conf",
            "AiPlayerbot.MaxRandomBots = 500\nAiPlayerbot.MinRandomBots = 500\n",
        );
        // Verbatim the two names `guides/wow-wotlk/install-wow-wotlk.sh` writes.
        t.write_override(concat!(
            "services:\n  ac-worldserver:\n    environment:\n",
            "      AC_AI_PLAYERBOT_MAX_RANDOM_BOTS: \"500\"\n",
            "      AC_AI_PLAYERBOT_MIN_RANDOM_BOTS: \"500\"\n",
        ));
        let rows = vec![curated_row(
            "bots.population",
            "int",
            0.into(),
            3000.into(),
            "conf:playerbots.conf:AiPlayerbot.MaxRandomBots",
            "World bot population",
        )];
        let out =
            config_set_curated(&t.0, "bots.population", "800", &rows, &no_soap_lock()).unwrap();
        assert_eq!(out["changed"], true);
        assert_eq!(out["restart_required"], true);
        assert_eq!(out["apply_needed"], "recreate");
        // Both shadowing keys are gone -- Min is the companion write, and leaving
        // it would keep the floor at 500 while the ceiling moved to 800.
        let text = std::fs::read_to_string(t.0.join("docker-compose.override.yml")).unwrap();
        let env = crate::config::parse_override_env(&text);
        assert!(!env.contains_key("AC_AI_PLAYERBOT_MAX_RANDOM_BOTS"));
        assert!(!env.contains_key("AC_AI_PLAYERBOT_MIN_RANDOM_BOTS"));
    }

    #[test]
    fn config_set_curated_non_conf_env_column_writes_override() {
        let t = TmpTitleDir::new("curated-envcol");
        // No real registry row has a bare env column today (every row is
        // `conf:` or motd's `-`) -- this exercises the oracle's fallback
        // `else` branch (`90-main.sh:2557-2560`) for parity anyway.
        let rows = vec![curated_row(
            "made.up",
            "int",
            0.into(),
            10.into(),
            "AC_MADE_UP",
            "Made up",
        )];
        let out = config_set_curated(&t.0, "made.up", "5", &rows, &no_soap_lock()).unwrap();
        assert_eq!(out["changed"], true);
        assert_eq!(out["restart_required"], true);
        // NOTE: no `applied` field on this branch, matching the bash oracle.
        assert!(out.get("applied").is_none());
        let text = std::fs::read_to_string(t.0.join("docker-compose.override.yml")).unwrap();
        assert_eq!(
            crate::config::parse_override_env(&text).get("AC_MADE_UP").map(String::as_str),
            Some("5")
        );
    }

    // -- `config files` + `assemble_key` (Task 11, the standalone CLI) ------

    #[test]
    fn config_files_lists_the_fixed_four_plus_module_confs() {
        let t = TmpTitleDir::new("files");
        // A live conf, a dist-only conf, both forms of a third, plus three
        // entries that must all be filtered out by the shape gate.
        t.write_module_conf("mod_ahbot.conf", "x\n");
        t.write_module_conf("mod_learnspells.conf.dist", "x\n");
        t.write_module_conf("transmog.conf", "x\n");
        t.write_module_conf("transmog.conf.dist", "x\n");
        t.write_module_conf("transmog.conf.bak", "x\n"); // not a .conf after the .dist strip
        std::fs::create_dir_all(t.modules_dir().join("lua_scripts")).unwrap(); // a dir
        t.write_module_conf("worldserver.conf", "x\n"); // excluded here; fixed entry instead
        std::fs::write(t.0.join(".env"), "A=1\n").unwrap();

        let out = config_files(&t.0).unwrap();
        let files = out["files"].as_array().unwrap();
        let names: Vec<&str> = files.iter().map(|f| f["name"].as_str().unwrap()).collect();
        assert_eq!(
            names,
            vec![
                ".env",
                "docker-compose.override.yml",
                "worldserver.conf",
                "authserver.conf",
                // module confs, deduped and byte-sorted
                "mod_ahbot.conf",
                "mod_learnspells.conf",
                "transmog.conf",
            ]
        );

        let by_name = |n: &str| files.iter().find(|f| f["name"] == n).unwrap().clone();
        // The two protected files are the only read-only ones.
        assert_eq!(by_name(".env")["readonly"], true);
        assert_eq!(by_name(".env")["exists"], true);
        assert_eq!(by_name("docker-compose.override.yml")["readonly"], true);
        assert_eq!(by_name("docker-compose.override.yml")["exists"], false);
        assert_eq!(by_name("mod_ahbot.conf")["readonly"], false);
        // exists/dist are reported independently.
        assert_eq!(by_name("mod_ahbot.conf")["exists"], true);
        assert_eq!(by_name("mod_ahbot.conf")["dist"], false);
        assert_eq!(by_name("mod_learnspells.conf")["exists"], false);
        assert_eq!(by_name("mod_learnspells.conf")["dist"], true);
        assert_eq!(by_name("transmog.conf")["exists"], true);
        assert_eq!(by_name("transmog.conf")["dist"], true);
        // The fixed worldserver.conf entry resolves one dir UP, so the decoy
        // copy inside modules/ is not what got reported.
        assert_eq!(by_name("worldserver.conf")["exists"], false);
    }

    #[test]
    fn config_files_requires_an_installed_title() {
        let dir = std::env::temp_dir().join(format!("dml-files-noinst-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap(); // exists, but no compose file
        let e = config_files(&dir).unwrap_err();
        assert_eq!(e.code, "NOT_FOUND");
        assert_eq!(e.message, "WoW Playerbots server not installed");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn assemble_key_returns_one_filled_row_or_none() {
        let t = TmpTitleDir::new("assemble-key");
        t.write_module_conf("playerbots.conf", "AiPlayerbot.MaxRandomBots = 800\n");
        let rows = vec![
            curated_row(
                "bots.population",
                "int",
                0.into(),
                3000.into(),
                "conf:playerbots.conf:AiPlayerbot.MaxRandomBots",
                "World bot population",
            ),
            curated_row("other.row", "int", 0.into(), 1.into(), "conf:Other.Key", "Other"),
        ];
        let mut reader = ConfigReader::for_title(&t.0);
        let got = reader.assemble_key(&rows, "bots.population").expect("row present");
        // Identical to what the full assemble produces for that row.
        let mut reader2 = ConfigReader::for_title(&t.0);
        let all = reader2.assemble(&rows);
        assert_eq!(got, all["settings"][0]);
        assert_eq!(got["key"], "bots.population");
        assert_eq!(got["value"], "800");
        assert!(reader.assemble_key(&rows, "no.such.key").is_none());
    }
}
