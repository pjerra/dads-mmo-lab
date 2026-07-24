//! Native-mode module-**tuning** reader (spike: `spike/docker-desktop-native`).
//!
//! Task 2 sibling of [`super::config`]: the Tuning tab needs the same data as
//! `dml wow config tuning-list --json` (`.data.settings[]`, 13 curated activator
//! knobs) but WITHOUT the bash-on-Windows fork tax. The static half of each row
//! comes for free from the cheap `dml wow config tuning-registry --json` arm
//! (Task 1); this module supplies the two dynamic fields the registry leaves
//! placeholdered — `value` and `installed` — by reading the runtime files
//! DIRECTLY in Rust (no bash, no docker, no fork).
//!
//! It is a faithful port of the `config tuning-list` emitter (90-main.sh:2775)
//! plus the `_mtune_rows` table and the `_lua_cfg_read` / `_mtune_to_json`
//! helpers in `cli/src/40-config.sh`. A file-gated cargo parity test
//! (`tuning_parity.rs`) asserts the assembled JSON deep-equals a real
//! `dml wow config tuning-list --json` run against the on-disk native files.
//!
//! The one field the registry does NOT carry is each row's `confkey` (the exact
//! key token inside its `.conf`/`.lua`), so [`tuning_confkey`] embeds that small
//! key→token map verbatim from `_mtune_rows`. Every other field flows through
//! from the registry row byte-for-byte, so `value`/`installed` are the only
//! fields this reader can ever change.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::dml::config::{self, parse_conf, strip_conf_quotes};

/// The conf/lua key token for one tuning row key — the `confkey` column of
/// `_mtune_rows` (40-config.sh:878), which the `tuning-registry` arm does not
/// emit. Kept in lock-step with that heredoc (13 rows); the parity test guards
/// any drift. Unknown key → `None` (the row then passes through untouched).
pub fn tuning_confkey(key: &str) -> Option<&'static str> {
    Some(match key {
        "beastmaster.enable" => "BeastMaster.Enable",
        "beastmaster.hunter_only" => "BeastMaster.HunterOnly",
        "beastmaster.allowed_classes" => "BeastMaster.AllowedClasses",
        "beastmaster.min_level" => "BeastMaster.MinLevel",
        "learnspells.enable" => "LearnSpells.Enable",
        "learnspells.announce" => "LearnSpells.Announce",
        "learnspells.on_first_login" => "LearnSpells.OnFirstLogin",
        "learnspells.max_level" => "LearnSpells.MaxLevel",
        "unlimitedammo.enabled" => "UnlimitedAmmoNamespace.ENABLED",
        "unlimitedammo.max_ammo" => "UnlimitedAmmoNamespace.MAX_AMMO",
        "unlimitedammo.min_threshold" => "UnlimitedAmmoNamespace.MIN_AMMO_THRESHOLD",
        "sitmeansrest.duration" => "DURATION",
        "sitmeansrest.regen_aura" => "REGEN_AURA",
        _ => return None,
    })
}

/// Lua file value → display/JSON form — a port of `_mtune_to_json`
/// (40-config.sh:900). Only the `bool` type is translated (`true`→`1`,
/// `false`→`0`); every other type (int/list) passes through verbatim.
pub fn mtune_to_json(ty: &str, fileval: &str) -> String {
    if ty == "bool" {
        match fileval {
            "true" => "1".to_string(),
            "false" => "0".to_string(),
            other => other.to_string(),
        }
    } else {
        fileval.to_string()
    }
}

/// Current file value of a `<key> = …` Lua assignment — a faithful port of
/// `_lua_cfg_read` (40-config.sh:927). Returns `""` when the key line is absent
/// or commented. Handles both column-0 namespaced keys
/// (`UnlimitedAmmoNamespace.ENABLED = false`) and indented bare table keys with
/// a trailing comma (`    DURATION = 20,`). The value token is everything after
/// `=` up to the first whitespace / comma / semicolon / inline `--` comment;
/// LAST occurrence wins (Lua load semantics). The key is matched literally
/// (never as a regex), and — like the awk — a longer key whose line starts with
/// this key but continues with more identifier chars (not blanks-then-`=`) is
/// correctly rejected.
pub fn lua_cfg_read(content: &str, key: &str) -> String {
    let mut found: Option<String> = None;
    for raw_line in content.split('\n') {
        // awk: sub(/\r$/,""); sub(/^[ \t]+/,"")
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        let s = line.trim_start_matches([' ', '\t']);
        // index(s, k) == 1  ->  s starts with the key
        let Some(rest) = s.strip_prefix(key) else {
            continue;
        };
        // rest ~ /^[ \t]*=/  then  sub(/^[ \t]*=[ \t]*/, "", rest)
        let after_blanks = rest.trim_start_matches([' ', '\t']);
        let Some(val) = after_blanks.strip_prefix('=') else {
            continue;
        };
        let val = val.trim_start_matches([' ', '\t']);
        // tok = val, then successive right-trims (order-faithful to the awk).
        let mut tok = val.to_string();
        if let Some(i) = tok.find([' ', '\t']) {
            tok.truncate(i);
        }
        if let Some(i) = tok.find(',') {
            tok.truncate(i);
        }
        if let Some(i) = tok.find(';') {
            tok.truncate(i);
        }
        if let Some(i) = tok.find("--") {
            tok.truncate(i);
        }
        if !tok.is_empty() {
            found = Some(tok);
        }
    }
    found.unwrap_or_default()
}

/// Reads live tuning VALUES + installed-state straight off the native runtime
/// files. Conf reads are memoised (parse-once per file); lua files are small
/// and read on demand.
pub struct TuningReader {
    title_dir: PathBuf,
    conf_cache: HashMap<PathBuf, HashMap<String, String>>,
}

impl TuningReader {
    /// Title dir from `DML_GAMES_DIR` (mirrors `ConfigReader::from_env`).
    pub fn from_env() -> Self {
        Self::for_title(config::ConfigReader::title_dir_from_env())
    }

    pub fn for_title(title_dir: impl Into<PathBuf>) -> Self {
        TuningReader { title_dir: title_dir.into(), conf_cache: HashMap::new() }
    }

    /// Quote-stripped value of `key` in the conf at `path`, memoising the parse.
    /// `""` when the file or key is absent — so an empty answer still triggers
    /// the caller's `.dist` fallback, exactly like `_cfg_conf_get_var`.
    fn conf_value(&mut self, path: &Path, key: &str) -> String {
        if !self.conf_cache.contains_key(path) {
            let parsed = std::fs::read_to_string(path).map(|t| parse_conf(&t)).unwrap_or_default();
            self.conf_cache.insert(path.to_path_buf(), parsed);
        }
        let raw = self.conf_cache[path].get(key).cloned().unwrap_or_default();
        strip_conf_quotes(&raw)
    }

    /// Deployed ALE script path for a lua-backend tuning file — a port of
    /// `_lua_cfg_path` (40-config.sh:897).
    fn lua_path(&self, file: &str) -> PathBuf {
        self.title_dir
            .join("env")
            .join("dist")
            .join("etc")
            .join("modules")
            .join("lua_scripts")
            .join(file)
    }

    /// Compute `(value, installed)` for one tuning row, mirroring the
    /// `config tuning-list` emitter (90-main.sh:2789-2813).
    ///
    ///  - `conf` backend: installed = the live `.conf` OR its `.conf.dist`
    ///    exists (a module ships its conf in either form). value = live conf →
    ///    `.dist` → registry default.
    ///  - `lua` backend: installed = the deployed `.lua` exists. value =
    ///    `_lua_cfg_read` (translated to display form) → registry default.
    pub fn compute(
        &mut self,
        backend: &str,
        ty: &str,
        default: &str,
        file: &str,
        confkey: &str,
    ) -> (String, bool) {
        if backend == "conf" {
            let path = config::conf_path_in(&self.title_dir, file);
            let dist = path.with_file_name(format!("{file}.dist"));
            let installed = path.is_file() || dist.is_file();
            let mut val = self.conf_value(&path, confkey);
            if val.is_empty() {
                val = self.conf_value(&dist, confkey);
            }
            if val.is_empty() {
                val = default.to_string();
            }
            (val, installed)
        } else {
            let path = self.lua_path(file);
            let installed = path.is_file();
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            let raw = lua_cfg_read(&content, confkey);
            let val = if !raw.is_empty() { mtune_to_json(ty, &raw) } else { default.to_string() };
            (val, installed)
        }
    }

    /// Assemble `{"settings":[…]}` from the cached tuning-registry rows, filling
    /// each row's `value` + `installed` from the live files. Every other field
    /// is carried through byte-for-byte from the registry row (Task 1 pinned
    /// those equal to `tuning-list` minus the two dynamic fields), so only
    /// `value`/`installed` can ever differ. A row whose key is not in
    /// [`tuning_confkey`], or that lacks the needed string fields, passes
    /// through unchanged (defensive; the registry never emits such a row).
    pub fn assemble(&mut self, registry_rows: &[Value]) -> Value {
        let mut out = Vec::with_capacity(registry_rows.len());
        for row in registry_rows {
            let mut row = row.clone();
            let fields = (
                row.get("key").and_then(Value::as_str).map(str::to_string),
                row.get("backend").and_then(Value::as_str).map(str::to_string),
                row.get("type").and_then(Value::as_str).map(str::to_string),
                row.get("default").and_then(Value::as_str).map(str::to_string),
                row.get("file").and_then(Value::as_str).map(str::to_string),
            );
            if let (Some(key), Some(backend), Some(ty), Some(default), Some(file)) = fields {
                if let Some(confkey) = tuning_confkey(&key) {
                    let (value, installed) =
                        self.compute(&backend, &ty, &default, &file, confkey);
                    if let Some(obj) = row.as_object_mut() {
                        obj.insert("value".to_string(), Value::String(value));
                        obj.insert("installed".to_string(), Value::Bool(installed));
                    }
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
    fn confkey_table_covers_all_13_rows() {
        for k in [
            "beastmaster.enable",
            "beastmaster.hunter_only",
            "beastmaster.allowed_classes",
            "beastmaster.min_level",
            "learnspells.enable",
            "learnspells.announce",
            "learnspells.on_first_login",
            "learnspells.max_level",
            "unlimitedammo.enabled",
            "unlimitedammo.max_ammo",
            "unlimitedammo.min_threshold",
            "sitmeansrest.duration",
            "sitmeansrest.regen_aura",
        ] {
            assert!(tuning_confkey(k).is_some(), "missing confkey for {k}");
        }
        assert_eq!(tuning_confkey("nope.nope"), None);
    }

    #[test]
    fn mtune_to_json_translates_only_bool() {
        assert_eq!(mtune_to_json("bool", "true"), "1");
        assert_eq!(mtune_to_json("bool", "false"), "0");
        assert_eq!(mtune_to_json("bool", "weird"), "weird");
        assert_eq!(mtune_to_json("int", "1000"), "1000");
        assert_eq!(mtune_to_json("list", "1,2,3"), "1,2,3");
    }

    #[test]
    fn lua_cfg_read_namespaced_and_bare_keys() {
        // Column-0 namespaced key.
        let c = "UnlimitedAmmoNamespace.ENABLED = false\nUnlimitedAmmoNamespace.MAX_AMMO = 1000\n";
        assert_eq!(lua_cfg_read(c, "UnlimitedAmmoNamespace.ENABLED"), "false");
        assert_eq!(lua_cfg_read(c, "UnlimitedAmmoNamespace.MAX_AMMO"), "1000");
        // Indented bare table key with a trailing comma + inline comment.
        let c2 = "    DURATION = 20, -- seconds\n    REGEN_AURA = 25990;\n";
        assert_eq!(lua_cfg_read(c2, "DURATION"), "20");
        assert_eq!(lua_cfg_read(c2, "REGEN_AURA"), "25990");
    }

    #[test]
    fn lua_cfg_read_last_occurrence_wins_and_absent_is_empty() {
        let c = "X = 1\nX = 2\nX = 3\n";
        assert_eq!(lua_cfg_read(c, "X"), "3");
        assert_eq!(lua_cfg_read(c, "MISSING"), "");
        // Glued inline comment with no whitespace before `--`.
        assert_eq!(lua_cfg_read("V = 7--c\n", "V"), "7");
        // A longer key's line must not match a shorter probe.
        assert_eq!(lua_cfg_read("DURATION_MAX = 9\n", "DURATION"), "");
    }

    #[test]
    fn compute_conf_backend_reads_conf_then_dist_then_default() {
        let dir = std::env::temp_dir().join(format!("dml-tune-conf-{}", std::process::id()));
        let modules = dir.join("env").join("dist").join("etc").join("modules");
        std::fs::create_dir_all(&modules).unwrap();
        // Live conf present -> installed true, live value wins.
        std::fs::write(
            modules.join("mod_npc_beastmaster.conf"),
            "BeastMaster.Enable = 0\n",
        )
        .unwrap();
        // Only a .dist for this file -> installed true, .dist value used.
        std::fs::write(
            modules.join("mod_learnspells.conf.dist"),
            "LearnSpells.MaxLevel = 60\n",
        )
        .unwrap();

        let mut r = TuningReader::for_title(&dir);
        assert_eq!(
            r.compute("conf", "bool", "1", "mod_npc_beastmaster.conf", "BeastMaster.Enable"),
            ("0".to_string(), true)
        );
        assert_eq!(
            r.compute("conf", "int", "80", "mod_learnspells.conf", "LearnSpells.MaxLevel"),
            ("60".to_string(), true)
        );
        // Key absent in both conf and dist -> default; file exists so installed.
        assert_eq!(
            r.compute("conf", "bool", "1", "mod_npc_beastmaster.conf", "BeastMaster.HunterOnly"),
            ("1".to_string(), true)
        );
        // A module with no conf and no dist at all -> not installed, default.
        assert_eq!(
            r.compute("conf", "int", "10", "mod_absent.conf", "Foo.Bar"),
            ("10".to_string(), false)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn compute_lua_backend_reads_deployed_script_then_default() {
        let dir = std::env::temp_dir().join(format!("dml-tune-lua-{}", std::process::id()));
        let lua = dir.join("env").join("dist").join("etc").join("modules").join("lua_scripts");
        std::fs::create_dir_all(&lua).unwrap();
        std::fs::write(
            lua.join("UnlimitedAmmo.lua"),
            "UnlimitedAmmoNamespace.ENABLED = true\n",
        )
        .unwrap();

        let mut r = TuningReader::for_title(&dir);
        // bool file value true -> display "1"; deployed so installed.
        assert_eq!(
            r.compute("lua", "bool", "0", "UnlimitedAmmo.lua", "UnlimitedAmmoNamespace.ENABLED"),
            ("1".to_string(), true)
        );
        // Deployed file exists but key missing -> default, still installed.
        assert_eq!(
            r.compute("lua", "int", "1000", "UnlimitedAmmo.lua", "UnlimitedAmmoNamespace.MAX_AMMO"),
            ("1000".to_string(), true)
        );
        // Script not deployed at all -> not installed, default.
        assert_eq!(
            r.compute("lua", "int", "20", "SitMeansRest.lua", "DURATION"),
            ("20".to_string(), false)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn assemble_overwrites_only_value_and_installed() {
        let dir = std::env::temp_dir().join(format!("dml-tune-asm-{}", std::process::id()));
        let modules = dir.join("env").join("dist").join("etc").join("modules");
        std::fs::create_dir_all(&modules).unwrap();
        std::fs::write(modules.join("mod_learnspells.conf"), "LearnSpells.Enable = 0\n").unwrap();

        let registry = serde_json::json!([
            {"key":"learnspells.enable","backend":"conf","module":"Learn Spells on Level-up","label":"Enable auto-learn","explain":"x","type":"bool","min":null,"max":null,"value":"","default":"1","installed":false,"file":"mod_learnspells.conf"}
        ]);
        let rows = registry.as_array().cloned().unwrap();
        let mut r = TuningReader::for_title(&dir);
        let out = r.assemble(&rows);
        let s = &out["settings"][0];
        assert_eq!(s["value"], "0");
        assert_eq!(s["installed"], true);
        // Everything else carried through untouched.
        assert_eq!(s["key"], "learnspells.enable");
        assert_eq!(s["backend"], "conf");
        assert_eq!(s["min"], Value::Null);
        assert_eq!(s["default"], "1");
        assert_eq!(s["file"], "mod_learnspells.conf");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
