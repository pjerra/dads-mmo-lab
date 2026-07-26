//! Native-mode **module-list** reader (spike: `spike/docker-desktop-native`).
//!
//! Task 2 sibling of [`super::config`] / [`super::tuning`]: the Modules page
//! needs the same data as `dml wow module list --json` (`.data` =
//! `{families:{cpp,lua,sql}, rebuild_pending, ale_ready}`) but WITHOUT the
//! bash-on-Windows fork tax. The static half of every row comes for free from
//! the cheap `dml wow module catalog --json` arm (Task 1); this module supplies
//! every DYNAMIC field by reading the runtime files directly in Rust.
//!
//! It is a faithful port of the `module list` emitter (90-main.sh:4420) and the
//! helpers in `cli/src/70-modules.sh` (`_cpp_installed`, `_module_conf_state`,
//! `_module_conf_dist`, `_rebuild_pending_*`, `_lua_cloned`, `_lua_deployed`,
//! `_lua_warn`, `_sql_installed`, the custom-clone scan, `ale_ready`). A
//! file-gated cargo parity test (`module_parity.rs`) asserts the assembled JSON
//! deep-equals a real `dml wow module list --json` run against the on-disk
//! native files.
//!
//! head/head_date: reproduced with ONE `git -C <dir> log -1 --format=%h%x1f%cs`
//! per installed clone — byte-identical to the bash `_mod_head_json_var` (same
//! git binary, same abbreviation rule), and the reason a pure-`gix` read was NOT
//! used: git's `%h` short-sha length is repo-specific disambiguation that gix's
//! `short_id` is not guaranteed to reproduce, which would break the deep-equal
//! parity gate. These are LOCAL git reads (no fetch, no docker); Docker Desktop
//! may be closed.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{json, Value};

/// Paragon's read-only advisory string — verbatim from `_lua_warn_var`
/// (70-modules.sh:280). Surfaced only when the paragon script is DEPLOYED.
const PARAGON_WARN: &str = "Heads-up: the Paragon script adds a \".test\" chat command with no GM permission check — any player can run it (an upstream debug leftover). Remove or guard it before opening your server to other people.";

/// Custom (non-registry) cpp clone description — verbatim from the `module
/// list` custom branch (90-main.sh:4495).
const CUSTOM_DESC: &str = "Custom module (cloned from a URL you provided).";

/// `^mod-[a-z0-9-]{1,64}$` — a port of `_valid_cpp_key` (70-modules.sh:124).
/// Used to admit a directory under `modules/` as a custom cpp clone.
pub fn valid_cpp_key(s: &str) -> bool {
    let Some(rest) = s.strip_prefix("mod-") else {
        return false;
    };
    let n = rest.chars().count();
    (1..=64).contains(&n)
        && rest.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Web-page URL from a registry/remote clone URL — a port of
/// `_mod_weburl_json_var` (90-main.sh:4428): strip ONE trailing `.git`; the
/// empty result becomes JSON `null`.
fn weburl(raw: &str) -> Value {
    let u = raw.strip_suffix(".git").unwrap_or(raw);
    if u.is_empty() {
        Value::Null
    } else {
        Value::String(u.to_string())
    }
}

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Run a LOCAL `git -C <dir> <args…>` and return trimmed stdout, or `None` on
/// spawn failure / nonzero exit / empty output — the graceful `|| true`
/// behaviour the bash relies on. No window flashes on Windows.
fn git_capture(dir: &Path, args: &[&str]) -> Option<String> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(dir).args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    let s = s.trim_end_matches(['\n', '\r']).to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Bounded recursive search mirroring `find <root> -maxdepth 4 -type f -name
/// <target>` (the `_module_conf_dist` fallback, 70-modules.sh:241): true when a
/// file named `target` exists anywhere within 4 path components below `root`.
/// Existence is all the caller needs (ready vs needs-rebuild), so traversal
/// order is irrelevant.
fn dist_present(root: &Path, target: &str) -> bool {
    // `dir` sits at depth `dir_depth`; its entries are at `dir_depth + 1`.
    fn rec(dir: &Path, target: &str, dir_depth: usize) -> bool {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return false;
        };
        for e in rd.flatten() {
            let entry_depth = dir_depth + 1;
            if entry_depth > 4 {
                continue; // beyond -maxdepth 4
            }
            let Ok(ft) = e.file_type() else {
                continue;
            };
            if ft.is_file() {
                if e.file_name().to_string_lossy() == target {
                    return true;
                }
            } else if ft.is_dir() && entry_depth < 4 {
                // children of this dir would sit at entry_depth + 1 <= 4
                if rec(&e.path(), target, entry_depth) {
                    return true;
                }
            }
        }
        false
    }
    rec(root, target, 0)
}

/// Reads live module state straight off the native runtime files (+ local git
/// for installed clones). No bash, no docker, no yq.
pub struct ModuleReader {
    title_dir: PathBuf,
    /// `.dml-rebuild-pending` lines in file order (drives the top-level
    /// `rebuild_pending` list, `_rebuild_pending_json`).
    pending: Vec<String>,
    /// Same lines as a set (per-row `pending_rebuild`, `_rebuild_pending_has`).
    pending_set: HashSet<String>,
}

impl ModuleReader {
    pub fn from_env() -> Self {
        Self::for_title(crate::dml::config::ConfigReader::title_dir_from_env())
    }

    pub fn for_title(title_dir: impl Into<PathBuf>) -> Self {
        let title_dir = title_dir.into();
        let (pending, pending_set) = load_pending(&title_dir);
        ModuleReader { title_dir, pending, pending_set }
    }

    fn modules_dir(&self) -> PathBuf {
        self.title_dir.join("modules")
    }

    /// `_cpp_installed` (70-modules.sh:138): `modules/<key>/.git` is a dir.
    /// `pub(crate)`: also reused by `dml::commands::assemble_commands` (task
    /// D1a) for the same install-state gate `commands` needs.
    pub(crate) fn cpp_installed(&self, key: &str) -> bool {
        self.modules_dir().join(key).join(".git").is_dir()
    }

    /// `_module_conf_state_var` (70-modules.sh:221): none | active | ready |
    /// needs-rebuild. `conf_name` is the row's already-resolved conf basename
    /// (== `_module_conf_name_var(key)`); an empty/missing name is `none`.
    fn conf_state(&self, conf_name: Option<&str>, key: &str) -> String {
        let name = match conf_name {
            Some(n) if !n.is_empty() => n,
            _ => return "none".to_string(),
        };
        let active = self.title_dir.join("env").join("dist").join("etc").join("modules").join(name);
        if active.is_file() {
            return "active".to_string();
        }
        let target = format!("{name}.dist");
        let root = self.modules_dir().join(key);
        if dist_present(&root, &target) {
            "ready".to_string()
        } else {
            "needs-rebuild".to_string()
        }
    }

    /// `(head, head_date)` for a clone dir — `_mod_head_json_var`
    /// (90-main.sh:4440): both `null` unless `<dir>/.git` exists and the one
    /// `git log -1 --format=%h%x1f%cs` returns a non-empty line.
    fn head(&self, dir: &Path) -> (Value, Value) {
        if !dir.join(".git").is_dir() {
            return (Value::Null, Value::Null);
        }
        match git_capture(dir, &["log", "-1", "--format=%h%x1f%cs"]) {
            Some(line) => {
                // bash: h="${line%%\x1f*}", hd="${line#*\x1f}"
                let mut parts = line.splitn(2, '\u{1f}');
                let h = parts.next().unwrap_or("");
                let hd = parts.next().unwrap_or("");
                (str_or_null(h), str_or_null(hd))
            }
            None => (Value::Null, Value::Null),
        }
    }

    /// Custom clone's origin remote as a web URL — `git remote get-url origin`
    /// piped through `_mod_weburl_json_var` (90-main.sh:4491-4493).
    fn custom_url(&self, dir: &Path) -> Value {
        let remote = git_capture(dir, &["remote", "get-url", "origin"]).unwrap_or_default();
        weburl(&remote)
    }

    /// `_lua_cloned` (70-modules.sh:264): `ale_scripts/<key>/.git` is a dir.
    /// `pub(crate)`: also reused by `dml::commands::assemble_commands` (task
    /// D1a).
    pub(crate) fn lua_cloned(&self, key: &str) -> bool {
        self.title_dir.join("ale_scripts").join(key).join(".git").is_dir()
    }

    /// `_lua_deployed` (70-modules.sh:248): per-key file/dir presence under
    /// `env/dist/etc/modules/lua_scripts`.
    fn lua_deployed(&self, key: &str) -> bool {
        let lua = self.title_dir.join("env").join("dist").join("etc").join("modules").join("lua_scripts");
        match key {
            "accountwide" => lua.join("accountwide").is_dir(),
            "activechat" => lua.join("AzerothChatter").is_dir(),
            "battlepass" => lua.join("battlepass").is_dir(),
            "bmah" => lua.join("BMAH.lua").is_file(),
            "lootpet" => lua.join("LootPet.lua").is_file(),
            "paragon" => lua.join("paragon").is_dir(),
            "sitmeanrest" => lua.join("SitMeansRest.lua").is_file(),
            "sod" => lua.join("SOD.lua").is_file(),
            "unlimitedammo" => lua.join("UnlimitedAmmo.lua").is_file(),
            _ => false,
        }
    }

    /// `_lua_warn_var` (70-modules.sh:275): only paragon, only when deployed.
    fn lua_warn(&self, key: &str) -> Value {
        if key == "paragon" && self.lua_deployed("paragon") {
            Value::String(PARAGON_WARN.to_string())
        } else {
            Value::Null
        }
    }

    /// `_sql_installed` (70-modules.sh:288): the `.installed` marker exists.
    /// `pub(crate)`: also reused by `dml::commands::assemble_commands` (task
    /// D1a).
    pub(crate) fn sql_installed(&self, key: &str) -> bool {
        self.title_dir
            .join("sql_scripts")
            .join("installed")
            .join(format!("{key}.installed"))
            .is_file()
    }

    /// Assemble the full `module list` `.data` from the cached catalog `.data`,
    /// filling every dynamic field. Registry rows keep their static fields
    /// byte-for-byte (Task 1 pinned those equal to `module list` minus dynamic
    /// state); custom cpp clones are discovered + appended exactly as the bash
    /// emitter does.
    pub fn assemble(&self, catalog: &Value) -> Value {
        let fam = &catalog["families"];

        // --- cpp: registry rows first (order preserved), then custom clones ---
        let cpp_reg = fam["cpp"].as_array().cloned().unwrap_or_default();
        let mut reg_keys: HashSet<String> = HashSet::new();
        let mut cpp_out: Vec<Value> = Vec::with_capacity(cpp_reg.len());
        for row in &cpp_reg {
            let key = row.get("key").and_then(Value::as_str).unwrap_or("").to_string();
            reg_keys.insert(key.clone());
            let mut r = row.clone();
            let dir = self.modules_dir().join(&key);
            let conf_name = row.get("conf_name").and_then(Value::as_str);
            let conf = self.conf_state(conf_name, &key);
            let (head, head_date) = self.head(&dir);
            if let Some(o) = r.as_object_mut() {
                o.insert("installed".into(), Value::Bool(self.cpp_installed(&key)));
                o.insert("pending_rebuild".into(), Value::Bool(self.pending_set.contains(&key)));
                o.insert("conf".into(), Value::String(conf));
                o.insert("head".into(), head);
                o.insert("head_date".into(), head_date);
            }
            cpp_out.push(r);
        }
        // Custom clones: dirs under modules/ with a .git, a valid mod-* key,
        // and NO registry row. Sorted ascending to match the bash `*/` glob.
        let mut customs: Vec<String> = Vec::new();
        if let Ok(rd) = std::fs::read_dir(self.modules_dir()) {
            for e in rd.flatten() {
                if !e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    continue;
                }
                if !e.path().join(".git").is_dir() {
                    continue;
                }
                let name = e.file_name().to_string_lossy().to_string();
                if reg_keys.contains(&name) || !valid_cpp_key(&name) {
                    continue;
                }
                customs.push(name);
            }
        }
        customs.sort();
        for key in &customs {
            let dir = self.modules_dir().join(key);
            let (head, head_date) = self.head(&dir);
            cpp_out.push(json!({
                "key": key,
                "name": key,
                "desc": CUSTOM_DESC,
                "url": self.custom_url(&dir),
                "installed": true,
                "pending_rebuild": self.pending_set.contains(key),
                "conf": "none",
                "conf_name": Value::Null,
                "custom": true,
                "head": head,
                "head_date": head_date,
            }));
        }

        // --- lua ---
        let mut lua_out: Vec<Value> = Vec::new();
        for row in fam["lua"].as_array().cloned().unwrap_or_default() {
            let key = row.get("key").and_then(Value::as_str).unwrap_or("").to_string();
            let mut r = row.clone();
            if let Some(o) = r.as_object_mut() {
                o.insert("cloned".into(), Value::Bool(self.lua_cloned(&key)));
                o.insert("deployed".into(), Value::Bool(self.lua_deployed(&key)));
                o.insert("warn".into(), self.lua_warn(&key));
            }
            lua_out.push(r);
        }

        // --- sql ---
        let mut sql_out: Vec<Value> = Vec::new();
        for row in fam["sql"].as_array().cloned().unwrap_or_default() {
            let key = row.get("key").and_then(Value::as_str).unwrap_or("").to_string();
            let mut r = row.clone();
            if let Some(o) = r.as_object_mut() {
                o.insert("installed".into(), Value::Bool(self.sql_installed(&key)));
            }
            sql_out.push(r);
        }

        let rebuild_pending: Vec<Value> =
            self.pending.iter().map(|s| Value::String(s.clone())).collect();
        let ale_ready = self.cpp_installed("mod-ale");

        json!({
            "families": { "cpp": cpp_out, "lua": lua_out, "sql": sql_out },
            "rebuild_pending": rebuild_pending,
            "ale_ready": ale_ready,
        })
    }
}

/// `null` for an empty string, else a JSON string.
fn str_or_null(s: &str) -> Value {
    if s.is_empty() {
        Value::Null
    } else {
        Value::String(s.to_string())
    }
}

/// Load `.dml-rebuild-pending` into `(ordered lines, set)` — mirrors
/// `_rebuild_pending_json` (file order, skip blanks) and `_rebuild_pending_load`
/// (the set). No CR stripping: the file is bash-written LF, and `read -r` keeps
/// any stray CR verbatim, so matching that keeps set membership consistent with
/// the emitter.
fn load_pending(title_dir: &Path) -> (Vec<String>, HashSet<String>) {
    let mut v = Vec::new();
    let mut set = HashSet::new();
    let f = title_dir.join(".dml-rebuild-pending");
    if let Ok(content) = std::fs::read_to_string(&f) {
        for line in content.split('\n') {
            if line.is_empty() {
                continue;
            }
            v.push(line.to_string());
            set.insert(line.to_string());
        }
    }
    (v, set)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_cpp_key_shape() {
        assert!(valid_cpp_key("mod-playerbots"));
        assert!(valid_cpp_key("mod-a"));
        assert!(!valid_cpp_key("mod-")); // needs >=1 char after prefix
        assert!(!valid_cpp_key("playerbots")); // no mod- prefix
        assert!(!valid_cpp_key("mod-UPPER")); // lowercase only
        assert!(!valid_cpp_key("mod-under_score")); // no underscores
        assert!(valid_cpp_key(&format!("mod-{}", "a".repeat(64))));
        assert!(!valid_cpp_key(&format!("mod-{}", "a".repeat(65))));
    }

    #[test]
    fn weburl_strips_one_git_and_nulls_empty() {
        assert_eq!(weburl("https://x/y.git"), Value::String("https://x/y".into()));
        assert_eq!(weburl("https://x/y"), Value::String("https://x/y".into()));
        assert_eq!(weburl(""), Value::Null);
        // only ONE trailing .git removed
        assert_eq!(weburl("a.git.git"), Value::String("a.git".into()));
    }

    #[test]
    fn str_or_null_maps_empty() {
        assert_eq!(str_or_null(""), Value::Null);
        assert_eq!(str_or_null("abc"), Value::String("abc".into()));
    }

    #[test]
    fn dist_present_respects_maxdepth_4() {
        let root = std::env::temp_dir().join(format!("dml-dist-{}", std::process::id()));
        // depth 2: <root>/conf/foo.conf.dist  (the expected location)
        let conf = root.join("conf");
        std::fs::create_dir_all(&conf).unwrap();
        std::fs::write(conf.join("foo.conf.dist"), "x").unwrap();
        assert!(dist_present(&root, "foo.conf.dist"));
        assert!(!dist_present(&root, "bar.conf.dist"));
        // depth 5: too deep for -maxdepth 4 (a/b/c/d/deep.dist == depth 5)
        let deep = root.join("a").join("b").join("c").join("d");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("deep.dist"), "x").unwrap();
        assert!(!dist_present(&root, "deep.dist"));
        // depth 4: a/b/c/ok.dist == depth 4 -> found
        std::fs::write(root.join("a").join("b").join("c").join("ok.dist"), "x").unwrap();
        assert!(dist_present(&root, "ok.dist"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn conf_state_none_active_ready_needs_rebuild() {
        let dir = std::env::temp_dir().join(format!("dml-confstate-{}", std::process::id()));
        let etc_mods = dir.join("env").join("dist").join("etc").join("modules");
        std::fs::create_dir_all(&etc_mods).unwrap();
        let r = ModuleReader::for_title(&dir);
        // No conf_name -> none.
        assert_eq!(r.conf_state(None, "mod-junk-to-gold"), "none");
        assert_eq!(r.conf_state(Some(""), "mod-x"), "none");
        // Active deployed conf present -> active.
        std::fs::write(etc_mods.join("mod_ahbot.conf"), "x").unwrap();
        assert_eq!(r.conf_state(Some("mod_ahbot.conf"), "mod-ah-bot"), "active");
        // Not active, but the module ships a .conf.dist in its checkout -> ready.
        let checkout_conf = dir.join("modules").join("mod-learn-spells").join("conf");
        std::fs::create_dir_all(&checkout_conf).unwrap();
        std::fs::write(checkout_conf.join("mod_learnspells.conf.dist"), "x").unwrap();
        assert_eq!(r.conf_state(Some("mod_learnspells.conf"), "mod-learn-spells"), "ready");
        // Not active, no dist anywhere -> needs-rebuild.
        assert_eq!(r.conf_state(Some("Solocraft.conf"), "mod-solocraft"), "needs-rebuild");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_pending_orders_and_dedups_into_set() {
        let dir = std::env::temp_dir().join(format!("dml-pending-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(".dml-rebuild-pending"), "mod-a\n\nmod-b\ncore-update\n").unwrap();
        let (v, set) = load_pending(&dir);
        assert_eq!(v, vec!["mod-a", "mod-b", "core-update"]); // blank skipped, order kept
        assert!(set.contains("core-update"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn assemble_fills_dynamic_fields_over_catalog() {
        // A tiny catalog-shaped .data with one of each family; no files on disk
        // -> everything reads as absent (installed false, conf none/needs, etc.).
        let dir = std::env::temp_dir().join(format!("dml-mod-asm-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let catalog = json!({
            "families": {
                "cpp": [{"key":"mod-solocraft","name":"Solocraft","desc":"d","url":"https://x/mod-solocraft","installed":false,"pending_rebuild":false,"conf":"","conf_name":"Solocraft.conf","custom":false,"head":null,"head_date":null}],
                "lua": [{"key":"paragon","name":"Paragon","desc":"d","url":"https://x/p","cloned":false,"deployed":false,"has_sql":true,"warn":null}],
                "sql": [{"key":"rare-drops","name":"Rare","desc":"d","url":"https://x/r","type":"clone_sql_norevert","installed":false}]
            },
            "rebuild_pending": [],
            "ale_ready": false
        });
        let r = ModuleReader::for_title(&dir);
        let data = r.assemble(&catalog);
        let cpp = &data["families"]["cpp"][0];
        assert_eq!(cpp["installed"], false);
        assert_eq!(cpp["conf"], "needs-rebuild"); // name set, no active, no dist
        assert_eq!(cpp["head"], Value::Null);
        // Static fields survive.
        assert_eq!(cpp["name"], "Solocraft");
        assert_eq!(cpp["conf_name"], "Solocraft.conf");
        assert_eq!(data["families"]["lua"][0]["deployed"], false);
        assert_eq!(data["families"]["lua"][0]["warn"], Value::Null);
        assert_eq!(data["families"]["lua"][0]["has_sql"], true);
        assert_eq!(data["families"]["sql"][0]["installed"], false);
        assert_eq!(data["rebuild_pending"], json!([]));
        assert_eq!(data["ale_ready"], false);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
