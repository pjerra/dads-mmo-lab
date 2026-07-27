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
use std::sync::{Arc, Mutex};

use serde_json::Value;

use dml_core::error::CmdError;

use crate::config::{self, atomic_write, cfgset_err, parse_conf, strip_conf_quotes};

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

/// Display/JSON value → Lua file form — a port of `_mtune_to_lua`
/// (40-config.sh:910), the exact inverse of [`mtune_to_json`]. Only the `bool`
/// type is translated (`1`→`true`, `0`→`false`); every other type (int/list)
/// passes through verbatim.
pub fn mtune_to_lua(ty: &str, jsonval: &str) -> String {
    if ty == "bool" {
        match jsonval {
            "1" => "true".to_string(),
            "0" => "false".to_string(),
            other => other.to_string(),
        }
    } else {
        jsonval.to_string()
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

// ---------------------------------------------------------------------------
// The lua-backend WRITE path — the symmetric sibling of [`lua_cfg_read`],
// ported to Rust by Task 11 (controller ruling D2). Before this, the lua
// branch of [`tuning_set`] shelled the bash CLI through a `DmlRunner`, which
// contradicted the workspace plan's central promise that the standalone
// `dml-wow` CLI never needs the bash script for manage commands — and it was
// FIVE of the 13 embedded tuning rows, not the 2 the inherited TODO claimed
// (unlimitedammo.{enabled,max_ammo,min_threshold}, sitmeansrest.{duration,
// regen_aura}).
//
// Oracle: `_lua_cfg_write` (`cli/src/40-config.sh:1000-1033`) and its awk
// `is_key_line`/`rebuild` pair. `tuning_write_parity.rs` byte-compares the
// files this produces against the ones the real bash CLI produces for the
// same edits.
// ---------------------------------------------------------------------------

/// Whether one record is an (uncommented) `<key> [blanks] =` assignment line —
/// the awk `is_key_line` function inside `_lua_cfg_write` (40-config.sh:1008).
/// One trailing `\r` is dropped, leading blanks are skipped, then the key must
/// match LITERALLY at position 1 (awk `index(body,k)==1`) and be followed by
/// optional blanks and an `=`. Deliberately the same shape gate
/// [`lua_cfg_read`] applies, so read and write always agree on which lines are
/// candidates.
fn is_lua_key_line(rec: &str, key: &str) -> bool {
    let s = rec.strip_suffix('\r').unwrap_or(rec);
    let body = s.trim_start_matches([' ', '\t']);
    match body.strip_prefix(key) {
        Some(rest) => rest.trim_start_matches([' ', '\t']).starts_with('='),
        None => false,
    }
}

/// Rebuild one key line around a new value — the awk `rebuild` function
/// (40-config.sh:1016). Preserves, byte-for-byte: the leading whitespace, the
/// exact spacing around `=`, everything from the first ` `/`\t`/`,`/`;` after
/// the value onward (a table comma and/or an inline `-- comment`), and a
/// trailing `\r`.
///
/// NOTE the deliberate asymmetry with [`lua_cfg_read`], faithfully carried
/// over: the value token ends at the first `[ \t,;]` ONLY. `--` is NOT a
/// terminator here, so a comment glued straight onto the value with no
/// separator (`V = 7--c`) is swallowed into the replaced token and lost,
/// exactly as the awk does it. Every real tuning file separates its comments
/// with a space or a comma, so this never fires in practice — but a port that
/// "fixed" it would diverge from the oracle byte-for-byte.
///
/// Only ever called for a record [`is_lua_key_line`] accepted; if the `=` is
/// somehow absent the `eqlen = 0` fallback mirrors awk's failed `match()`.
fn rebuild_lua_line(rec: &str, key: &str, fileval: &str) -> String {
    let (s, cr) = match rec.strip_suffix('\r') {
        Some(t) => (t, "\r"),
        None => (rec, ""),
    };
    let trimmed = s.trim_start_matches([' ', '\t']);
    let lead = &s[..s.len() - trimmed.len()];
    let after = trimmed.get(key.len()..).unwrap_or("");
    // awk: if (match(after, /^[ \t]*=[ \t]*/)) { eqlen = RLENGTH }
    let a = after.trim_start_matches([' ', '\t']);
    let pre = after.len() - a.len();
    let eqlen = match a.strip_prefix('=') {
        Some(a2) => pre + 1 + (a2.len() - a2.trim_start_matches([' ', '\t']).len()),
        None => 0,
    };
    let eqpart = &after[..eqlen];
    let rest = &after[eqlen..];
    // awk: vlen = length(rest); if (match(rest, /[ \t,;]/)) vlen = RSTART - 1
    let vlen = rest.find([' ', '\t', ',', ';']).unwrap_or(rest.len());
    let trailer = &rest[vlen..];
    format!("{lead}{key}{eqpart}{fileval}{trailer}{cr}")
}

/// Patch the LAST `<key> = …` line of a Lua config's TEXT — the pure core of
/// `_lua_cfg_write`'s awk program (40-config.sh:1039-1046). Editing the LAST
/// occurrence (not the first) matches [`lua_cfg_read`] and Lua's own
/// last-assignment-wins load semantics, so a write round-trips through a read.
///
/// Records follow awk's model, identical to
/// [`dml_core::conf::conf_write`]'s: split on `'\n'`, dropping a single
/// trailing empty record, and every emitted record gets awk's `ORS` newline
/// back — so a file that ended without a trailing newline gains exactly one,
/// and CRLF line endings survive untouched (each record keeps its own `\r`).
/// With no matching key line at all, awk's `last` stays 0 and every record is
/// echoed verbatim; [`lua_cfg_write`] never gets that far (its `cur` check
/// fails first), but the behaviour is preserved anyway.
pub fn lua_cfg_rewrite(content: &str, key: &str, fileval: &str) -> String {
    let recs: Vec<&str> = if content.is_empty() {
        Vec::new()
    } else {
        let mut v: Vec<&str> = content.split('\n').collect();
        if content.ends_with('\n') {
            v.pop();
        }
        v
    };
    let last = recs.iter().rposition(|rec| is_lua_key_line(rec, key));
    let mut out = String::with_capacity(content.len() + fileval.len() + 1);
    for (i, rec) in recs.iter().enumerate() {
        if Some(i) == last {
            out.push_str(&rebuild_lua_line(rec, key, fileval));
        } else {
            out.push_str(rec);
        }
        out.push('\n');
    }
    out
}

/// What one [`lua_cfg_write`] attempt did — the bash oracle's
/// `(return code, MTUNE_CHANGED)` pair as a single value.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum LuaWrite {
    /// The value moved and the file was rewritten (bash: `return 0` with
    /// `MTUNE_CHANGED=true`).
    Changed,
    /// The key already held this value — the file is byte-untouched (bash:
    /// `return 0` with `MTUNE_CHANGED` left `false`).
    Unchanged,
    /// bash `return 1`: the file is missing/unreadable, the key line is
    /// absent or commented, or the patch failed to verify. The original file
    /// is never modified in this case. The CALLER distinguishes "key absent"
    /// from "write failed" by re-reading the file, exactly like the oracle's
    /// `tuning-set` arm does (`90-main.sh:2926-2930`).
    Failed,
}

/// In-place value replacement in a deployed ALE `.lua` script — a faithful
/// port of `_lua_cfg_write` (40-config.sh:1000). `fileval` must ALREADY be in
/// file form (`true`/`false` or a validated integer — see [`mtune_to_lua`]),
/// so the reconstructed line is safe.
///
/// Order of operations mirrors the oracle exactly:
/// 1. read the current value; empty (absent file, absent/commented key, or an
///    empty value token) → [`LuaWrite::Failed`], nothing written;
/// 2. current == requested → [`LuaWrite::Unchanged`], nothing written;
/// 3. patch, then RE-VERIFY by reading the patched text back — a patch that
///    doesn't read back as the requested value is discarded
///    ([`LuaWrite::Failed`]) rather than applied, so a bad edit can never
///    truncate or corrupt the live script;
/// 4. only then write, atomically (tmp + rename, like the oracle's
///    `tmp`+`mv`).
///
/// The one deliberate divergence: the oracle verifies its tmp FILE and would
/// still report success if the final `mv` failed; this verifies the patched
/// TEXT and reports [`LuaWrite::Failed`] if the atomic write itself fails —
/// strictly more honest, and unobservable in every non-pathological case.
pub fn lua_cfg_write(path: &Path, key: &str, fileval: &str) -> LuaWrite {
    let Ok(content) = std::fs::read_to_string(path) else {
        return LuaWrite::Failed;
    };
    let cur = lua_cfg_read(&content, key);
    if cur.is_empty() {
        return LuaWrite::Failed;
    }
    if cur == fileval {
        return LuaWrite::Unchanged;
    }
    let patched = lua_cfg_rewrite(&content, key, fileval);
    if lua_cfg_read(&patched, key) != fileval {
        return LuaWrite::Failed;
    }
    if atomic_write(path, &patched).is_err() {
        return LuaWrite::Failed;
    }
    LuaWrite::Changed
}

/// Deployed ALE script path for a lua-backend tuning file — the free-function
/// form of `_lua_cfg_path` (40-config.sh:897), shared by [`TuningReader`]'s
/// read path and [`tuning_set`]'s write path so both resolve the same file.
pub fn lua_path_in(title_dir: &Path, file: &str) -> PathBuf {
    title_dir
        .join("env")
        .join("dist")
        .join("etc")
        .join("modules")
        .join("lua_scripts")
        .join(file)
}

/// The lua backend's advisory text, verbatim from `mtreload`
/// (90-main.sh:2918): a lua tuning change applies live, no restart needed.
pub const LUA_RELOAD_HINT: &str = ".reload ale (Console page) or restart the server to apply";

/// Validate + normalize a `tuning-set` value against its row's `type`/`min`/
/// `max` — a faithful port of the validation switch in the `tuning-set)` case
/// (90-main.sh:2879-2895), used by `launcher_lib::wow_config_tuning_set_native`
/// (Task B2b) for BOTH backends (the oracle validates once, before branching
/// on `conf`/`lua`). `min`/`max` are only consulted for `type == "int"` (the
/// other two types have no range). On success returns the value to WRITE:
/// `int` is leading-zero-stripped (`"007"` -> `"7"`, matching bash's
/// `$((10#$value))` arithmetic-expansion normalization); `bool`/`list` (and
/// any unknown type, which the registry never emits) pass through unchanged.
/// On failure returns the exact oracle `BAD_ARG` message text (the shape
/// check and the range check share one message for `int`, exactly like the
/// oracle — both read `{label} must be a whole number between {min} and
/// {max}, got: {value}`).
pub fn validate_tuning_value(
    ty: &str,
    value: &str,
    label: &str,
    min: i64,
    max: i64,
) -> Result<String, String> {
    match ty {
        "bool" => {
            if value == "0" || value == "1" {
                Ok(value.to_string())
            } else {
                Err(format!("{label} takes 1 (on) or 0 (off), got: {value}"))
            }
        }
        "int" => {
            let shape_ok = !value.is_empty() && value.chars().all(|c| c.is_ascii_digit());
            let parsed = if shape_ok { value.parse::<i64>().ok() } else { None };
            match parsed {
                Some(n) if n >= min && n <= max => Ok(n.to_string()),
                _ => Err(format!(
                    "{label} must be a whole number between {min} and {max}, got: {value}"
                )),
            }
        }
        "list" => {
            // ^[0-9]+(,[0-9]+)*$: every comma-separated part non-empty digits
            // only -- rejects leading/trailing/doubled commas via the
            // explicit `!part.is_empty()` check (an empty string vacuously
            // passes `.all()` over its (zero) chars, so that guard is load
            // bearing, not redundant).
            let ok = !value.is_empty()
                && value.split(',').all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()));
            if ok {
                Ok(value.to_string())
            } else {
                Err(format!(
                    "{label} must be comma-separated numbers (e.g. 3,8) or 0 for all, got: {value}"
                ))
            }
        }
        _ => Ok(value.to_string()),
    }
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

    /// Deployed ALE script path for a lua-backend tuning file — delegates to
    /// the free-function [`lua_path_in`] (`_lua_cfg_path`, 40-config.sh:897)
    /// so the read path and [`tuning_set`]'s write path can never drift.
    fn lua_path(&self, file: &str) -> PathBuf {
        lua_path_in(&self.title_dir, file)
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

// ---------------------------------------------------------------------------
// `config tuning-set` (Task B2b) — moved out of the launcher's `lib.rs` by
// the cargo-workspace refactor (Task 9b); made fully native (no `DmlRunner`,
// no bash) by Task 11 / controller ruling D2.
// ---------------------------------------------------------------------------

/// `dml wow config tuning-set` port (`90-main.sh:2859-2934`, Task B2b).
/// Looks up the row in the tuning registry, validates by `type` (shared by
/// BOTH backends, exactly like the oracle validates once before branching),
/// then writes it. BOTH backends are now fully native:
///
///  - `conf` mirrors `config::config_set_curated`'s conf-write tail almost
///    verbatim, minus the ahbot/bots.population special-case companion writes
///    that don't apply to tuning rows;
///  - `lua` line-replaces the DEPLOYED ALE script through [`lua_cfg_write`]
///    (Task 11 / ruling D2 — this arm used to shell `dml wow config
///    tuning-set` through an `Arc<DmlRunner>`, which broke the workspace
///    plan's promise that the standalone CLI never needs the bash script for
///    manage commands, for 5 of the 13 rows).
///
/// `config_lock` and `title_dir` are resolved by the caller, neither of them
/// a Tauri type.
pub fn tuning_set(
    key: String,
    value: String,
    config_lock: Arc<Mutex<()>>,
    title_dir: PathBuf,
) -> Result<serde_json::Value, CmdError> {
    // Serializes against every other native conf/override write -- see
    // the doc comment on `AppState::config_lock`.
    let _guard = config_lock.lock().unwrap_or_else(|e| e.into_inner());
    let rows = crate::registry::tuning_registry_rows();
    let row = rows
        .iter()
        .find(|r| r.get("key").and_then(|v| v.as_str()) == Some(key.as_str()))
        .ok_or_else(|| {
            cfgset_err(
                "NOT_FOUND",
                format!("Unknown tuning setting: {key}"),
                "See: dml wow config tuning-list --json",
            )
        })?;

    let backend = row.get("backend").and_then(|v| v.as_str()).unwrap_or("");
    let file = row.get("file").and_then(|v| v.as_str()).unwrap_or("");
    let ty = row.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let label = row.get("label").and_then(|v| v.as_str()).unwrap_or(key.as_str());
    let module = row.get("module").and_then(|v| v.as_str()).unwrap_or("");
    let min = row.get("min").and_then(|v| v.as_i64()).unwrap_or(0);
    let max = row.get("max").and_then(|v| v.as_i64()).unwrap_or(0);
    // The registry JSON carries no `confkey` column (Task 1's cheap
    // sibling arm deliberately omits it); `tuning_confkey` is the source
    // of truth, kept in lock-step with `_mtune_rows` by `tuning.rs`'s own
    // parity test. Every row the registry can ever emit has an entry
    // here, so this is unreachable in practice -- defensive, not a real
    // oracle branch.
    let confkey = crate::tuning::tuning_confkey(&key).ok_or_else(|| {
        cfgset_err(
            "NOT_FOUND",
            format!("Unknown tuning setting: {key}"),
            "See: dml wow config tuning-list --json",
        )
    })?;

    let norm_value = crate::tuning::validate_tuning_value(ty, &value, label, min, max)
        .map_err(|msg| cfgset_err("BAD_ARG", msg, ""))?;

    // `_wow_server_dir` re-check (`90-main.sh:2888-2892`): the oracle runs
    // this AFTER type/range validation but BEFORE either backend branch
    // (it feeds the lua branch's `_lua_cfg_path` too, at 2913) -- for the
    // conf backend specifically, that puts it before `_cfg_conf_ensure`'s
    // own NOT_INSTALLED check, so a not-installed server reports the
    // oracle's uniform "server not installed" rather than "{module} is
    // not installed". It used to sit BELOW the lua branch here (that branch
    // shelled the CLI, which ran its own copy of the check); now that the
    // lua write is native too, the check moved back UP to the oracle's own
    // position so both backends share it, exactly like the bash does.
    if !crate::config::wow_server_installed(&title_dir) {
        return Err(cfgset_err(
            "NOT_FOUND",
            "WoW Playerbots server not installed",
            "Install it first, then re-run.",
        ));
    }

    if backend == "lua" {
        // Lua backend (`90-main.sh:2909-2932`): line-replace the DEPLOYED
        // script; applies live via `.reload ale`. This family has no `.dist`
        // to seed from, so an absent file is NOT_INSTALLED (no `conf_ensure`
        // equivalent), and an absent KEY inside a present file is NOT_FOUND.
        let lpath = lua_path_in(&title_dir, file);
        if !lpath.is_file() {
            return Err(cfgset_err(
                "NOT_INSTALLED",
                format!("{module} is not installed"),
                format!("Install {module} from the Modules page (Lua scripts) first, then reopen this page."),
            ));
        }
        let fileval = mtune_to_lua(ty, &norm_value);
        return match lua_cfg_write(&lpath, confkey, &fileval) {
            LuaWrite::Changed => Ok(serde_json::json!({
                "key": key, "backend": "lua", "changed": true,
                "restart_required": false, "applied": "reload-ale",
                "reload": LUA_RELOAD_HINT,
            })),
            LuaWrite::Unchanged => Ok(serde_json::json!({
                "key": key, "backend": "lua", "changed": false,
                "restart_required": false, "applied": "none",
            })),
            // The oracle discriminates its single `return 1` by RE-READING
            // the (untouched) file: still no value for this key -> the key
            // simply isn't in this build of the module; a readable value ->
            // the patch itself failed.
            LuaWrite::Failed => {
                let content = std::fs::read_to_string(&lpath).unwrap_or_default();
                if lua_cfg_read(&content, confkey).is_empty() {
                    Err(cfgset_err(
                        "NOT_FOUND",
                        format!("{confkey} is not present in {file}"),
                        format!("This setting may not exist in the installed version of {module}."),
                    ))
                } else {
                    Err(cfgset_err(
                        "WRITE_FAILED",
                        format!("Could not update {confkey} in {file}"),
                        format!("Edit the file manually or reinstall {module}."),
                    ))
                }
            }
        };
    }

    // backend == "conf" -- same `_cfg_conf_path` resolution as B2a's
    // curated route (`conf_path_in`: worldserver/authserver.conf under
    // `env/dist/etc/`, else `env/dist/etc/modules/{file}`).
    let cpath = crate::config::conf_path_in(&title_dir, file);
    let ensured = crate::config::conf_ensure(&cpath)
        .map_err(|e| cfgset_err("WRITE_FAILED", format!("Could not write {file}: {e}"), ""))?;
    if !ensured {
        return Err(cfgset_err(
            "NOT_INSTALLED",
            format!("{module} is not installed"),
            format!("Install {module} from the Modules page first, then reopen this page."),
        ));
    }
    let changed = crate::config::conf_write(&cpath, confkey, &norm_value)
        .map_err(|e| cfgset_err("WRITE_FAILED", format!("Could not write {file}: {e}"), ""))?;

    Ok(if changed {
        serde_json::json!({
            "key": key, "backend": "conf", "changed": true,
            "restart_required": true, "applied": "restart",
        })
    } else {
        serde_json::json!({
            "key": key, "backend": "conf", "changed": false,
            "restart_required": false, "applied": "none",
        })
    })
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

    // -- lua WRITE path (Task 11 / ruling D2: the `_lua_cfg_write` port) -----

    #[test]
    fn mtune_to_lua_translates_only_bool_and_inverts_to_json() {
        assert_eq!(mtune_to_lua("bool", "1"), "true");
        assert_eq!(mtune_to_lua("bool", "0"), "false");
        assert_eq!(mtune_to_lua("bool", "weird"), "weird");
        assert_eq!(mtune_to_lua("int", "1000"), "1000");
        assert_eq!(mtune_to_lua("list", "1,2,3"), "1,2,3");
        // Exact inverse of `_mtune_to_json` for the two real bool forms.
        assert_eq!(mtune_to_json("bool", &mtune_to_lua("bool", "1")), "1");
        assert_eq!(mtune_to_json("bool", &mtune_to_lua("bool", "0")), "0");
    }

    #[test]
    fn is_lua_key_line_matches_the_read_gate() {
        assert!(is_lua_key_line("UnlimitedAmmoNamespace.ENABLED = false", "UnlimitedAmmoNamespace.ENABLED"));
        assert!(is_lua_key_line("    DURATION = 20,", "DURATION"));
        assert!(is_lua_key_line("\tDURATION\t=\t20", "DURATION"));
        assert!(is_lua_key_line("DURATION =\r", "DURATION"));
        // A longer key must not be matched by a shorter probe.
        assert!(!is_lua_key_line("DURATION_MAX = 9", "DURATION"));
        // Commented out -> the key is not at position 1 of the trimmed body.
        assert!(!is_lua_key_line("-- DURATION = 20", "DURATION"));
        // No `=` at all.
        assert!(!is_lua_key_line("DURATION", "DURATION"));
    }

    #[test]
    fn rebuild_lua_line_preserves_lead_spacing_and_trailer() {
        // Plain namespaced assignment.
        assert_eq!(
            rebuild_lua_line("UnlimitedAmmoNamespace.ENABLED = false", "UnlimitedAmmoNamespace.ENABLED", "true"),
            "UnlimitedAmmoNamespace.ENABLED = true"
        );
        // Indented table key with a trailing comma AND an inline comment.
        assert_eq!(
            rebuild_lua_line("    DURATION = 20, -- seconds", "DURATION", "45"),
            "    DURATION = 45, -- seconds"
        );
        // Semicolon trailer.
        assert_eq!(rebuild_lua_line("  REGEN_AURA = 25990;", "REGEN_AURA", "1"), "  REGEN_AURA = 1;");
        // Unusual `=` spacing is preserved byte-for-byte.
        assert_eq!(rebuild_lua_line("\tX\t=\t\t7 -- c", "X", "9"), "\tX\t=\t\t9 -- c");
        assert_eq!(rebuild_lua_line("X=7", "X", "9"), "X=9");
        // CRLF survives.
        assert_eq!(rebuild_lua_line("X = 7\r", "X", "9"), "X = 9\r");
        // Faithful oracle quirk: a `--` comment GLUED to the value with no
        // separator is part of the value token and is lost (awk's rebuild
        // only splits on [ \t,;], unlike its read which also splits on `--`).
        assert_eq!(rebuild_lua_line("V = 7--c", "V", "9"), "V = 9");
    }

    #[test]
    fn lua_cfg_rewrite_edits_only_the_last_occurrence() {
        let c = "X = 1\nY = 2\nX = 3\n";
        assert_eq!(lua_cfg_rewrite(c, "X", "9"), "X = 1\nY = 2\nX = 9\n");
        // ...and a read of the result returns the new value (round trip).
        assert_eq!(lua_cfg_read(&lua_cfg_rewrite(c, "X", "9"), "X"), "9");
    }

    #[test]
    fn lua_cfg_rewrite_matches_awks_record_model() {
        // No trailing newline in -> exactly one added (awk's ORS per record).
        assert_eq!(lua_cfg_rewrite("X = 1", "X", "2"), "X = 2\n");
        // Empty input -> zero records -> empty output (never a phantom line).
        assert_eq!(lua_cfg_rewrite("", "X", "2"), "");
        // CRLF file: untouched lines keep their own \r verbatim.
        assert_eq!(lua_cfg_rewrite("A = 1\r\nX = 1\r\n", "X", "2"), "A = 1\r\nX = 2\r\n");
        // No matching key -> every record echoed verbatim (awk's last == 0).
        assert_eq!(lua_cfg_rewrite("A = 1\nB = 2\n", "X", "9"), "A = 1\nB = 2\n");
        // Blank lines and comments pass through untouched.
        assert_eq!(
            lua_cfg_rewrite("-- head\n\nX = 1\n\n-- tail\n", "X", "2"),
            "-- head\n\nX = 2\n\n-- tail\n"
        );
    }

    fn tmp_lua(name: &str, body: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dml-luawrite-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("Script.lua");
        std::fs::write(&p, body).unwrap();
        p
    }

    #[test]
    fn lua_cfg_write_changed_unchanged_and_absent_key() {
        let p = tmp_lua("basic", "UnlimitedAmmoNamespace.ENABLED = false\n");
        // First write moves the value.
        assert_eq!(lua_cfg_write(&p, "UnlimitedAmmoNamespace.ENABLED", "true"), LuaWrite::Changed);
        assert_eq!(
            std::fs::read_to_string(&p).unwrap(),
            "UnlimitedAmmoNamespace.ENABLED = true\n"
        );
        // Same value again is a true no-op: byte-identical, reported Unchanged.
        let before = std::fs::read(&p).unwrap();
        assert_eq!(lua_cfg_write(&p, "UnlimitedAmmoNamespace.ENABLED", "true"), LuaWrite::Unchanged);
        assert_eq!(std::fs::read(&p).unwrap(), before);
        // An absent key is the oracle's `return 1`, and never touches the file.
        assert_eq!(lua_cfg_write(&p, "UnlimitedAmmoNamespace.MAX_AMMO", "1000"), LuaWrite::Failed);
        assert_eq!(std::fs::read(&p).unwrap(), before);
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }

    #[test]
    fn lua_cfg_write_missing_file_is_failed() {
        let missing = std::env::temp_dir().join(format!("dml-luawrite-none-{}.lua", std::process::id()));
        let _ = std::fs::remove_file(&missing);
        assert_eq!(lua_cfg_write(&missing, "X", "1"), LuaWrite::Failed);
        assert!(!missing.exists(), "a failed write must not create the file");
    }

    #[test]
    fn lua_cfg_write_preserves_comments_and_leaves_no_tmp_files() {
        let p = tmp_lua(
            "fmt",
            "-- SitMeansRest\nlocal CONFIG = {\n    DURATION = 20, -- seconds\n    REGEN_AURA = 25990,\n}\n",
        );
        assert_eq!(lua_cfg_write(&p, "DURATION", "45"), LuaWrite::Changed);
        assert_eq!(
            std::fs::read_to_string(&p).unwrap(),
            "-- SitMeansRest\nlocal CONFIG = {\n    DURATION = 45, -- seconds\n    REGEN_AURA = 25990,\n}\n"
        );
        // The atomic tmp+rename must leave nothing behind next to the file.
        let leftovers: Vec<String> = std::fs::read_dir(p.parent().unwrap())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n != "Script.lua")
            .collect();
        assert!(leftovers.is_empty(), "unexpected leftovers: {leftovers:?}");
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }

    #[test]
    fn lua_cfg_write_round_trips_through_the_reader() {
        // The write targets the LAST occurrence precisely because the reader
        // does; targeting the first would leave the effective value unchanged.
        let p = tmp_lua("last", "DURATION = 20\nDURATION = 30\n");
        assert_eq!(lua_cfg_write(&p, "DURATION", "45"), LuaWrite::Changed);
        let after = std::fs::read_to_string(&p).unwrap();
        assert_eq!(after, "DURATION = 20\nDURATION = 45\n");
        assert_eq!(lua_cfg_read(&after, "DURATION"), "45");
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }

    #[test]
    fn validate_tuning_value_bool_accepts_only_0_or_1() {
        assert_eq!(validate_tuning_value("bool", "0", "Enable auto-learn", 0, 0), Ok("0".to_string()));
        assert_eq!(validate_tuning_value("bool", "1", "Enable auto-learn", 0, 0), Ok("1".to_string()));
        assert_eq!(
            validate_tuning_value("bool", "2", "Enable auto-learn", 0, 0),
            Err("Enable auto-learn takes 1 (on) or 0 (off), got: 2".to_string())
        );
        assert_eq!(
            validate_tuning_value("bool", "true", "Enable auto-learn", 0, 0),
            Err("Enable auto-learn takes 1 (on) or 0 (off), got: true".to_string())
        );
    }

    #[test]
    fn validate_tuning_value_int_strips_leading_zeros_and_checks_range() {
        // "007" with range [0,100] -> accepted, normalized to "7".
        assert_eq!(validate_tuning_value("int", "007", "Minimum level", 0, 100), Ok("7".to_string()));
        assert_eq!(validate_tuning_value("int", "0", "Minimum level", 0, 100), Ok("0".to_string()));
        assert_eq!(validate_tuning_value("int", "100", "Minimum level", 0, 100), Ok("100".to_string()));
    }

    #[test]
    fn validate_tuning_value_int_rejects_out_of_range_shape_and_negative() {
        let want = "Minimum level must be a whole number between 0 and 100, got: 101".to_string();
        assert_eq!(validate_tuning_value("int", "101", "Minimum level", 0, 100), Err(want));
        // Non-digit shape.
        let want2 = "Minimum level must be a whole number between 0 and 100, got: abc".to_string();
        assert_eq!(validate_tuning_value("int", "abc", "Minimum level", 0, 100), Err(want2));
        // Negative sign fails the digit-only shape gate.
        let want3 = "Minimum level must be a whole number between 0 and 100, got: -5".to_string();
        assert_eq!(validate_tuning_value("int", "-5", "Minimum level", 0, 100), Err(want3));
        // Empty value.
        let want4 = "Minimum level must be a whole number between 0 and 100, got: ".to_string();
        assert_eq!(validate_tuning_value("int", "", "Minimum level", 0, 100), Err(want4));
    }

    #[test]
    fn validate_tuning_value_list_accepts_csv_rejects_bad_shape() {
        assert_eq!(validate_tuning_value("list", "0", "Allowed classes", 0, 0), Ok("0".to_string()));
        assert_eq!(validate_tuning_value("list", "3,8", "Allowed classes", 0, 0), Ok("3,8".to_string()));
        let want = "Allowed classes must be comma-separated numbers (e.g. 3,8) or 0 for all, got: 3,,8"
            .to_string();
        assert_eq!(validate_tuning_value("list", "3,,8", "Allowed classes", 0, 0), Err(want));
        let want2 = "Allowed classes must be comma-separated numbers (e.g. 3,8) or 0 for all, got: 3,8,"
            .to_string();
        assert_eq!(validate_tuning_value("list", "3,8,", "Allowed classes", 0, 0), Err(want2));
        let want3 = "Allowed classes must be comma-separated numbers (e.g. 3,8) or 0 for all, got: "
            .to_string();
        assert_eq!(validate_tuning_value("list", "", "Allowed classes", 0, 0), Err(want3));
    }
}
