//! Native-mode **account-wide sharing configurator** (`wow accountwide
//! get|set`, Chunk 2 task 5). Faithful port of the `accountwide)` case
//! (`cli/src/90-main.sh:4095-4237`) + the `_aw_*` helpers
//! (`cli/src/70-modules.sh:590-703`): reads/writes the `ENABLE_*` flags in
//! the DEPLOYED accountwide lua files under
//! `<server_dir>/env/dist/etc/modules/lua_scripts/accountwide/` — never the
//! repo (that family clones the upstream repo fresh at install time and
//! copies the lua in). Both verbs are plain JSON, no streaming.
//!
//! `flag_read`/`flag_write` are line-editors over `local <FLAG> = true|false`
//! lua assignments — ported statement-for-statement from the awk/sed oracle
//! (see each function's doc comment for the exact bash it mirrors), so a
//! flag name that happens to be a PREFIX of another identifier on the same
//! line is guarded exactly the way the oracle guards it (by requiring
//! optional blanks then `=` to immediately follow the matched head), not by
//! a word-boundary regex.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

/// One row of the subsystem registry — `_aw_registry`'s heredoc
/// (`70-modules.sh:612-629`), `flag|file|group|parent|label|explain`.
/// `group` nests money/achievement sub-toggles under their parent in the
/// GUI; `parent` is the flag that must be ON for a sub-toggle to matter
/// (`None` = a top-level system). Flag names + files are the upstream
/// HEAD's, verbatim (Aldori15/azerothcore-eluna-accountwide).
pub struct AwRow {
    pub flag: &'static str,
    pub file: &'static str,
    pub group: &'static str,
    pub parent: Option<&'static str>,
    pub label: &'static str,
    pub explain: &'static str,
}

/// `_aw_registry` (`70-modules.sh:612-629`), 14 rows, in registry order
/// (`accountwide get`'s `subsystems` array preserves this order).
pub const REGISTRY: &[AwRow] = &[
    AwRow { flag: "ENABLE_ACCOUNTWIDE_COMPLETED_ACHIEVEMENTS", file: "AccountAchievements.lua", group: "achievements", parent: None, label: "Achievements", explain: "Earn an achievement on one character and every character on the account gets it too." },
    AwRow { flag: "ENABLE_ACCOUNTWIDE_CRITERIA_PROGRESS", file: "AccountAchievements.lua", group: "achievements", parent: Some("ENABLE_ACCOUNTWIDE_COMPLETED_ACHIEVEMENTS"), label: "Achievement progress", explain: "Also share partial progress toward achievements, not just the finished ones." },
    AwRow { flag: "ENABLE_ACCOUNTWIDE_REALM_FIRST_ACHIEVEMENTS", file: "AccountAchievements.lua", group: "achievements", parent: Some("ENABLE_ACCOUNTWIDE_COMPLETED_ACHIEVEMENTS"), label: "Realm-first achievements", explain: "Include the special \"realm first\" achievements in the sharing." },
    AwRow { flag: "ENABLE_ACCOUNTWIDE_CURRENCY", file: "AccountCurrency.lua", group: "currency", parent: None, label: "Currency", explain: "Share badges and tokens (like dungeon emblems) across all your characters." },
    AwRow { flag: "ENABLE_ACCOUNTWIDE_MONEY", file: "AccountMoney.lua", group: "money", parent: None, label: "Gold", explain: "Keep one shared gold pool for every character on the account." },
    AwRow { flag: "ENABLE_REALTIME_TICK", file: "AccountMoney.lua", group: "money", parent: Some("ENABLE_ACCOUNTWIDE_MONEY"), label: "Live gold sync", explain: "Update the shared gold every few seconds while you play, not only on logout." },
    AwRow { flag: "ENABLE_ALTBOT_REALTIME_TICK", file: "AccountMoney.lua", group: "money", parent: Some("ENABLE_REALTIME_TICK"), label: "Live gold sync for alt-bots", explain: "Also live-sync the shared gold for your alt playerbots." },
    AwRow { flag: "ENABLE_ACCOUNTWIDE_MOUNTS", file: "AccountMounts.lua", group: "mounts", parent: None, label: "Mounts", explain: "Learn a mount on one character and all your characters can ride it." },
    AwRow { flag: "ENABLE_ACCOUNTWIDE_PETS", file: "AccountPets.lua", group: "pets", parent: None, label: "Battle pets", explain: "Share companion and vanity pets across all your characters." },
    AwRow { flag: "ENABLE_ACCOUNTWIDE_PLAYTIME", file: "AccountPlaytime.lua", group: "playtime", parent: None, label: "Playtime", explain: "Adds a .playtime command that totals time played across the whole account." },
    AwRow { flag: "ENABLE_ACCOUNTWIDE_PROFESSIONS", file: "AccountProfessions.lua", group: "professions", parent: None, label: "Professions", explain: "Share learned professions and their skill level across all your characters." },
    AwRow { flag: "ENABLE_ACCOUNTWIDE_PVP_RANK", file: "AccountPvPRank.lua", group: "pvp", parent: None, label: "PvP rank", explain: "Share honor kills and honor/arena points across the account." },
    AwRow { flag: "ENABLE_ACCOUNTWIDE_TAXI_PATHS", file: "AccountTaxiPaths.lua", group: "taxi", parent: None, label: "Flight paths", explain: "Share discovered flight points per faction. Needs a special mod-ale fork - may not work on this server." },
    AwRow { flag: "ENABLE_ACCOUNTWIDE_TITLES", file: "AccountTitles.lua", group: "titles", parent: None, label: "Titles", explain: "Earn a title on one character and every character on the account can use it." },
];

/// The one flag `accountwide set` handles as a pick-one special case instead
/// of a plain registry row.
pub const REPUTATION_FLAG: &str = "ENABLE_ACCOUNTWIDE_REPUTATION";

/// `.reload ale`/restart hint every successful `set` reply carries
/// (`90-main.sh:4172`, `4218`, `4232`).
const RELOAD_HINT: &str = ".reload ale (Console page) or restart the server to apply";

/// `_aw_dir` (`70-modules.sh:600`): deployed accountwide script dir for a
/// resolved server dir.
pub fn aw_dir(server_dir: &Path) -> PathBuf {
    server_dir.join("env").join("dist").join("etc").join("modules").join("lua_scripts").join("accountwide")
}

/// `_aw_installed` (`70-modules.sh:603`): is the accountwide payload
/// deployed?
pub fn aw_installed(server_dir: &Path) -> bool {
    aw_dir(server_dir).is_dir()
}

/// `_aw_valid_flag` (`70-modules.sh:632`): `^ENABLE_[A-Z0-9_]{1,60}$`.
/// Flag-name allowlist (identifier-safe -> never a line-editor
/// regex-injection vector).
pub fn valid_flag(flag: &str) -> bool {
    let Some(rest) = flag.strip_prefix("ENABLE_") else { return false };
    let n = rest.chars().count();
    (1..=60).contains(&n) && rest.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// `_aw_flag_read` (`70-modules.sh:641-663`): "on" | "off" | `None` (flag
/// absent / file absent / not a recognized boolean token). Matches an
/// UNCOMMENTED `local <flag> = true|false` line (leading whitespace
/// tolerated; a `-- local ...` comment line is naturally skipped since the
/// leading-whitespace-stripped line then starts with `--`, not `local`),
/// LAST occurrence wins. The `rest` guard (optional blanks then `=`
/// immediately after the matched head) is what makes a flag name that is a
/// literal PREFIX of a longer identifier safe — not a word-boundary regex.
pub fn flag_read(file: &Path, flag: &str) -> Option<&'static str> {
    let content = std::fs::read_to_string(file).ok()?;
    let head = format!("local {flag}");
    let mut result: Option<&'static str> = None;
    for raw_line in content.split('\n') {
        let s = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        let s = s.trim_start_matches([' ', '\t']);
        let Some(rest) = s.strip_prefix(head.as_str()) else { continue };
        let rb = rest.as_bytes();
        let mut idx = 0;
        while idx < rb.len() && (rb[idx] == b' ' || rb[idx] == b'\t') {
            idx += 1;
        }
        if idx >= rb.len() || rb[idx] != b'=' {
            continue;
        }
        idx += 1;
        while idx < rb.len() && (rb[idx] == b' ' || rb[idx] == b'\t') {
            idx += 1;
        }
        // tok = rest-after-'='; cut at first whitespace, then "--", then ";"
        // -- IN THAT ORDER (each cut operates on the already-shortened tok,
        // mirroring the awk oracle's three sequential `sub()` calls).
        let mut tok = &rest[idx..];
        if let Some(p) = tok.find([' ', '\t']) {
            tok = &tok[..p];
        }
        if let Some(p) = tok.find("--") {
            tok = &tok[..p];
        }
        if let Some(p) = tok.find(';') {
            tok = &tok[..p];
        }
        match tok {
            "true" => result = Some("on"),
            "false" => result = Some("off"),
            _ => {}
        }
    }
    result
}

/// Pure single-line rewrite step of [`flag_write`] — a port of the sed
/// command's per-line matching (`70-modules.sh:679`):
/// `s/^([[:space:]]*local <flag>)[[:space:]]*=[[:space:]]*<from>/\1 = <to>/`.
/// `None` when the line doesn't match (left byte-for-byte unchanged by the
/// caller); `Some(new_line)` otherwise, preserving the line's own leading
/// whitespace and EVERYTHING after the matched `from` token verbatim (a
/// trailing `-- comment` or stray suffix survives untouched, exactly like
/// sed's non-greedy literal match).
fn flag_write_line(line: &str, flag: &str, from: &str, to: &str) -> Option<String> {
    let ws_len = line.find(|c: char| c != ' ' && c != '\t').unwrap_or(line.len());
    let after_ws = &line[ws_len..];
    let head = format!("local {flag}");
    let rest = after_ws.strip_prefix(head.as_str())?;
    let rb = rest.as_bytes();
    let mut idx = 0;
    while idx < rb.len() && (rb[idx] == b' ' || rb[idx] == b'\t') {
        idx += 1;
    }
    if idx >= rb.len() || rb[idx] != b'=' {
        return None;
    }
    idx += 1;
    while idx < rb.len() && (rb[idx] == b' ' || rb[idx] == b'\t') {
        idx += 1;
    }
    if !rest[idx..].starts_with(from) {
        return None;
    }
    let remainder = &rest[idx + from.len()..];
    Some(format!("{}{} = {}{}", &line[..ws_len], head, to, remainder))
}

/// Rewrite every matching line in `content` (every duplicate active line
/// rewritten, matching sed applying its `s///` to every line with no line
/// restriction). Preserves the file's trailing-newline-or-not shape.
fn rewrite_flag_content(content: &str, flag: &str, from: &str, to: &str) -> String {
    let had_trailing_nl = content.ends_with('\n');
    let mut lines: Vec<&str> = content.split('\n').collect();
    if had_trailing_nl {
        lines.pop();
    }
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    for line in lines {
        match flag_write_line(line, flag, from, to) {
            Some(new_line) => out.push(new_line),
            None => out.push(line.to_string()),
        }
    }
    let mut result = out.join("\n");
    if had_trailing_nl {
        result.push('\n');
    }
    result
}

/// Sibling-tmp-then-rename atomic write, unique per call (this app is
/// long-lived, unlike a fresh-forked-per-invocation bash process, so a bare
/// pid-only tmp name could collide across concurrent writers -- see
/// `dml::config::atomic_write`'s identical rationale).
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

/// Why [`flag_write`] didn't change anything.
#[derive(Debug)]
pub enum AwWriteErr {
    /// The file doesn't exist, or the flag isn't present in it — the bash
    /// oracle's `[[ -f "$file" ]] || return 1` / `[[ -z "$cur" ]] && return 1`.
    Absent,
    /// The line-edit was applied but the new value didn't verify (or the
    /// write itself failed) — the bash oracle's `grep -q` verify-then-`mv`
    /// failing, or the `sed`/`mv` step failing.
    VerifyFailed(String),
}

/// `_aw_flag_write` (`70-modules.sh:671-687`): flips `flag`'s `= true|false`
/// value in `file` to `want` ("on"/"off"). Returns `Ok(true)` iff the value
/// actually moved, `Ok(false)` when it was already `want` (a true no-op, file
/// untouched — file existence/flag-presence still required first). Verifies
/// the new value landed before committing (tmp + rename), so a failed edit
/// never truncates the file. Callers validate `flag` with [`valid_flag`]
/// first.
pub fn flag_write(file: &Path, flag: &str, want: &str) -> Result<bool, AwWriteErr> {
    if !file.is_file() {
        return Err(AwWriteErr::Absent);
    }
    let cur = flag_read(file, flag).ok_or(AwWriteErr::Absent)?;
    if cur == want {
        return Ok(false);
    }
    let (from, to) = if want == "on" { ("false", "true") } else { ("true", "false") };
    let content = std::fs::read_to_string(file).map_err(|e| AwWriteErr::VerifyFailed(e.to_string()))?;
    let new_content = rewrite_flag_content(&content, flag, from, to);

    // Verify: some line, after stripping leading whitespace, literally
    // starts with "local <flag> = <to>" -- `grep -q "^[[:space:]]*local
    // <flag> = <to>"` (`70-modules.sh:683`).
    let needle = format!("local {flag} = {to}");
    let verified = new_content
        .split('\n')
        .any(|line| line.trim_start_matches([' ', '\t']).starts_with(needle.as_str()));
    if !verified {
        return Err(AwWriteErr::VerifyFailed("patched value did not verify in the rewritten file".into()));
    }
    atomic_write(file, &new_content).map_err(|e| AwWriteErr::VerifyFailed(e.to_string()))?;
    Ok(true)
}

/// One deployed `AccountReputation*.lua` variant.
pub struct RepFile {
    pub variant: &'static str,
    pub path: PathBuf,
}

/// `_aw_rep_files` (`70-modules.sh:694-703`): every deployed
/// `AccountReputation*.lua` file in `dir`, classified `default` (basename
/// contains `Default`/`default`) or `custom` (anything else), sorted by
/// filename (the bash `for f in dir/AccountReputation*.lua` glob expands in
/// sorted order by default).
pub fn rep_files(dir: &Path) -> Vec<RepFile> {
    let mut names: Vec<String> = match std::fs::read_dir(dir) {
        Ok(rd) => rd
            .flatten()
            .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with("AccountReputation") && name.ends_with(".lua"))
            .collect(),
        Err(_) => Vec::new(),
    };
    names.sort();
    names
        .into_iter()
        .map(|name| {
            let variant = if name.contains("Default") || name.contains("default") { "default" } else { "custom" };
            RepFile { variant, path: dir.join(name) }
        })
        .collect()
}

fn file_basename(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Typed error for `set_flag`, carrying the same `(code, message, hint)`
/// triple every bash `json_err` call in the `accountwide set)` arm does.
#[derive(Debug)]
pub struct AwErr {
    pub code: &'static str,
    pub message: String,
    pub hint: String,
}

impl AwErr {
    fn new(code: &'static str, message: impl Into<String>, hint: impl Into<String>) -> Self {
        AwErr { code, message: message.into(), hint: hint.into() }
    }
}

/// `accountwide get` (`90-main.sh:4109-4133`): `{"installed":false,
/// "subsystems":[],"reputation":{"present":false,"value":"off","variants":
/// [],"active":null}}` when the payload isn't deployed (NOT an error --
/// caller already confirmed the server itself is installed); otherwise
/// `{"installed":true,"subsystems":[...],"reputation":{...}}`. A registry
/// row whose flag isn't present in its file (SKIP-if-absent -- not every
/// upstream version ships every subsystem) is omitted from `subsystems`
/// entirely, matching the bash arm's `[[ -z "$awval" ]] && continue`.
pub fn build_get(server_dir: &Path) -> Value {
    if !aw_installed(server_dir) {
        return json!({
            "installed": false,
            "subsystems": [],
            "reputation": {"present": false, "value": "off", "variants": [], "active": Value::Null},
        });
    }
    let awdir = aw_dir(server_dir);
    let mut subsystems = Vec::new();
    for row in REGISTRY {
        let Some(value) = flag_read(&awdir.join(row.file), row.flag) else { continue };
        subsystems.push(json!({
            "key": row.flag,
            "file": row.file,
            "group": row.group,
            "parent": row.parent,
            "label": row.label,
            "explain": row.explain,
            "value": value,
        }));
    }

    let reps = rep_files(&awdir);
    let mut present = false;
    let mut value = "off";
    let mut active: Option<&'static str> = None;
    let mut variants: Vec<&'static str> = Vec::new();
    for r in &reps {
        present = true;
        variants.push(r.variant);
        if flag_read(&r.path, REPUTATION_FLAG) == Some("on") {
            value = "on";
            active = Some(r.variant);
        }
    }

    json!({
        "installed": true,
        "subsystems": subsystems,
        "reputation": {"present": present, "value": value, "variants": variants, "active": active},
    })
}

/// `accountwide set` (`90-main.sh:4172-4231`). `server_dir` is assumed
/// already resolved (the caller does the "WoW Playerbots server not
/// installed" `NOT_FOUND` check first, matching the bash arm's ordering:
/// `--value`/flag-shape validation happens BEFORE the server-dir resolve, so
/// callers should validate those two first too — see `lib.rs`'s
/// `wow_accountwide_set_native`).
pub fn set_flag(server_dir: &Path, key: &str, value: &str, variant: Option<&str>) -> Result<Value, AwErr> {
    if value != "on" && value != "off" {
        return Err(AwErr::new("BAD_ARG", "--value must be on or off", ""));
    }
    if !valid_flag(key) {
        return Err(AwErr::new(
            "BAD_ARG",
            format!("Invalid flag name: {key}"),
            "Flags look like ENABLE_ACCOUNTWIDE_MOUNTS -- see: dml wow accountwide get --json",
        ));
    }
    if !aw_installed(server_dir) {
        return Err(AwErr::new(
            "NOT_INSTALLED",
            "Accountwide is not installed",
            "Install it from the Modules page (Lua family), then reopen this page.",
        ));
    }
    let awdir = aw_dir(server_dir);

    if key == REPUTATION_FLAG {
        return set_reputation(&awdir, value, variant);
    }

    // Generic (non-reputation) flag: must be a known registry row.
    let Some(row) = REGISTRY.iter().find(|r| r.flag == key) else {
        return Err(AwErr::new(
            "BAD_ARG",
            format!("Unknown accountwide flag: {key}"),
            "See: dml wow accountwide get --json",
        ));
    };
    let file_path = awdir.join(row.file);
    match flag_write(&file_path, key, value) {
        Ok(changed) => Ok(json!({"key": key, "value": value, "changed": changed, "reload": RELOAD_HINT})),
        Err(AwWriteErr::Absent) => Err(AwErr::new(
            "NOT_FOUND",
            format!("{key} is not present in {}", row.file),
            "This subsystem may not exist in the installed accountwide version.",
        )),
        Err(AwWriteErr::VerifyFailed(_)) => Err(AwErr::new(
            "WRITE_FAILED",
            format!("Could not update {key} in {}", row.file),
            "Edit the file manually or reinstall accountwide.",
        )),
    }
}

/// The `ENABLE_ACCOUNTWIDE_REPUTATION` pick-one special case
/// (`90-main.sh:4175-4218`): `set on` keeps the chosen variant + deletes the
/// other(s); `set off` clears the flag in every present variant, deleting
/// none.
fn set_reputation(awdir: &Path, value: &str, variant: Option<&str>) -> Result<Value, AwErr> {
    let reps = rep_files(awdir);
    if reps.is_empty() {
        return Err(AwErr::new(
            "NOT_FOUND",
            "No AccountReputation lua file is deployed",
            "Reputation sharing isn't available in this install.",
        ));
    }

    if value == "off" {
        let mut changed = false;
        for r in &reps {
            if flag_read(&r.path, REPUTATION_FLAG) == Some("off") {
                continue;
            }
            match flag_write(&r.path, REPUTATION_FLAG, "off") {
                Ok(c) => changed = changed || c,
                Err(_) => {
                    return Err(AwErr::new(
                        "WRITE_FAILED",
                        "Could not disable reputation sharing",
                        "Edit the AccountReputation lua manually.",
                    ));
                }
            }
        }
        return Ok(json!({
            "key": REPUTATION_FLAG, "value": "off", "changed": changed, "reload": RELOAD_HINT,
        }));
    }

    // value == "on": choose a variant (default to the sole one present).
    let chosen_idx = if reps.len() == 1 {
        if let Some(v) = variant {
            if v != reps[0].variant {
                return Err(AwErr::new(
                    "BAD_ARG",
                    format!("Only the {} reputation variant is deployed", reps[0].variant),
                    format!("Pass --variant {} or omit it.", reps[0].variant),
                ));
            }
        }
        0
    } else {
        let v = match variant {
            Some("default") | Some("custom") => variant.unwrap(),
            _ => {
                return Err(AwErr::new(
                    "BAD_ARG",
                    "Two reputation variants are deployed -- pass --variant default|custom",
                    "",
                ));
            }
        };
        match reps.iter().position(|r| r.variant == v) {
            Some(i) => i,
            None => {
                return Err(AwErr::new("NOT_FOUND", format!("The {v} reputation variant is not deployed"), ""));
            }
        }
    };

    let mut removed = Vec::new();
    let mut changed = false;
    for (i, r) in reps.iter().enumerate() {
        if i == chosen_idx {
            continue;
        }
        match std::fs::remove_file(&r.path) {
            Ok(()) => {
                removed.push(file_basename(&r.path));
                changed = true;
            }
            Err(_) => {
                return Err(AwErr::new(
                    "WRITE_FAILED",
                    "Could not remove the other reputation variant",
                    "Both files would load and conflict -- remove one manually.",
                ));
            }
        }
    }

    let chosen = &reps[chosen_idx];
    if flag_write(&chosen.path, REPUTATION_FLAG, "on").is_err() {
        return Err(AwErr::new(
            "WRITE_FAILED",
            format!("Could not enable reputation sharing in {}", file_basename(&chosen.path)),
            "",
        ));
    }

    Ok(json!({
        "key": REPUTATION_FLAG,
        "value": "on",
        "variant": file_basename(&chosen.path),
        "removed": removed,
        "changed": changed,
        "reload": RELOAD_HINT,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("dml-accountwide-{name}-{}", std::process::id()))
    }

    // -- valid_flag ------------------------------------------------------

    #[test]
    fn valid_flag_shape_gate() {
        assert!(valid_flag("ENABLE_ACCOUNTWIDE_MOUNTS"));
        assert!(valid_flag("ENABLE_X"));
        assert!(!valid_flag("ENABLE_"));
        assert!(!valid_flag("DISABLE_MOUNTS"));
        assert!(!valid_flag("ENABLE_lowercase"));
        assert!(!valid_flag(""));
        // 60 chars after ENABLE_ is fine, 61 is not.
        let ok60 = format!("ENABLE_{}", "A".repeat(60));
        let bad61 = format!("ENABLE_{}", "A".repeat(61));
        assert!(valid_flag(&ok60));
        assert!(!valid_flag(&bad61));
    }

    // -- flag_read ---------------------------------------------------------

    #[test]
    fn flag_read_basic_true_false_and_absent() {
        let d = tmp("read-basic");
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("Account.lua");
        std::fs::write(&f, "local ENABLE_FOO = true\nlocal ENABLE_BAR = false\n").unwrap();
        assert_eq!(flag_read(&f, "ENABLE_FOO"), Some("on"));
        assert_eq!(flag_read(&f, "ENABLE_BAR"), Some("off"));
        assert_eq!(flag_read(&f, "ENABLE_MISSING"), None);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn flag_read_missing_file_is_none() {
        let d = tmp("read-missing");
        assert_eq!(flag_read(&d.join("nope.lua"), "ENABLE_FOO"), None);
    }

    #[test]
    fn flag_read_commented_line_is_skipped() {
        let d = tmp("read-comment");
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("Account.lua");
        std::fs::write(&f, "-- local ENABLE_FOO = true\n").unwrap();
        assert_eq!(flag_read(&f, "ENABLE_FOO"), None);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn flag_read_prefix_guard_rejects_longer_identifier() {
        let d = tmp("read-prefix");
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("Account.lua");
        std::fs::write(&f, "local ENABLE_FOOBAR = true\n").unwrap();
        assert_eq!(flag_read(&f, "ENABLE_FOO"), None);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn flag_read_last_occurrence_wins() {
        let d = tmp("read-dup");
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("Account.lua");
        std::fs::write(&f, "local ENABLE_FOO = true\nlocal ENABLE_FOO = false\n").unwrap();
        assert_eq!(flag_read(&f, "ENABLE_FOO"), Some("off"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn flag_read_trailing_comment_and_semicolon_tolerated() {
        let d = tmp("read-trail");
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("Account.lua");
        std::fs::write(
            &f,
            "local ENABLE_A = true -- some comment\nlocal ENABLE_B = false;\n  local ENABLE_C\t=\ttrue\r\n",
        )
        .unwrap();
        assert_eq!(flag_read(&f, "ENABLE_A"), Some("on"));
        assert_eq!(flag_read(&f, "ENABLE_B"), Some("off"));
        assert_eq!(flag_read(&f, "ENABLE_C"), Some("on"));
        let _ = std::fs::remove_dir_all(&d);
    }

    // -- flag_write_line (pure) ---------------------------------------------

    #[test]
    fn flag_write_line_replaces_value_keeps_remainder() {
        let out = flag_write_line("local ENABLE_FOO = false", "ENABLE_FOO", "false", "true");
        assert_eq!(out.as_deref(), Some("local ENABLE_FOO = true"));

        let out2 = flag_write_line("  local ENABLE_FOO = false -- x", "ENABLE_FOO", "false", "true");
        assert_eq!(out2.as_deref(), Some("  local ENABLE_FOO = true -- x"));
    }

    #[test]
    fn flag_write_line_no_match_returns_none() {
        assert_eq!(flag_write_line("local OTHER = false", "ENABLE_FOO", "false", "true"), None);
        assert_eq!(flag_write_line("-- local ENABLE_FOO = false", "ENABLE_FOO", "false", "true"), None);
        // prefix guard
        assert_eq!(flag_write_line("local ENABLE_FOOBAR = false", "ENABLE_FOO", "false", "true"), None);
    }

    // -- flag_write ----------------------------------------------------------

    struct TmpFile(PathBuf);
    impl TmpFile {
        fn new(name: &str, content: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("dml-aw-write-{}-{}", std::process::id(), name));
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join("Account.lua");
            std::fs::write(&path, content).unwrap();
            TmpFile(path)
        }
        fn read(&self) -> String {
            std::fs::read_to_string(&self.0).unwrap()
        }
    }
    impl Drop for TmpFile {
        fn drop(&mut self) {
            if let Some(dir) = self.0.parent() {
                let _ = std::fs::remove_dir_all(dir);
            }
        }
    }

    #[test]
    fn flag_write_toggles_and_reports_changed() {
        let f = TmpFile::new("toggle", "local ENABLE_FOO = false\nOther = 1\n");
        let changed = flag_write(&f.0, "ENABLE_FOO", "on").unwrap();
        assert!(changed);
        assert_eq!(f.read(), "local ENABLE_FOO = true\nOther = 1\n");
    }

    #[test]
    fn flag_write_already_matching_is_noop() {
        let f = TmpFile::new("noop", "local ENABLE_FOO = true\n");
        let before = f.read();
        let changed = flag_write(&f.0, "ENABLE_FOO", "on").unwrap();
        assert!(!changed);
        assert_eq!(f.read(), before);
    }

    #[test]
    fn flag_write_absent_flag_errors() {
        let f = TmpFile::new("absent", "local OTHER = true\n");
        let err = flag_write(&f.0, "ENABLE_FOO", "on");
        assert!(matches!(err, Err(AwWriteErr::Absent)));
    }

    #[test]
    fn flag_write_missing_file_errors_absent() {
        let dir = tmp("write-missing-file");
        let path = dir.join("nope.lua");
        let err = flag_write(&path, "ENABLE_FOO", "on");
        assert!(matches!(err, Err(AwWriteErr::Absent)));
    }

    #[test]
    fn flag_write_duplicate_lines_all_rewritten() {
        let f = TmpFile::new(
            "dup",
            "local ENABLE_FOO = false\nOther = 1\nlocal ENABLE_FOO = false\n",
        );
        let changed = flag_write(&f.0, "ENABLE_FOO", "on").unwrap();
        assert!(changed);
        assert_eq!(f.read(), "local ENABLE_FOO = true\nOther = 1\nlocal ENABLE_FOO = true\n");
    }

    // -- rep_files -------------------------------------------------------

    #[test]
    fn rep_files_classifies_default_and_custom_sorted() {
        let d = tmp("repfiles");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("AccountReputation_Custom.lua"), "x").unwrap();
        std::fs::write(d.join("AccountReputation_Default.lua"), "x").unwrap();
        std::fs::write(d.join("NotIt.lua"), "x").unwrap();
        let reps = rep_files(&d);
        assert_eq!(reps.len(), 2);
        // Sorted by filename: "_Custom" < "_Default" alphabetically.
        assert_eq!(reps[0].variant, "custom");
        assert_eq!(reps[1].variant, "default");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn rep_files_empty_dir_or_missing_dir_is_empty() {
        let d = tmp("repfiles-empty");
        assert!(rep_files(&d).is_empty());
        std::fs::create_dir_all(&d).unwrap();
        assert!(rep_files(&d).is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    // -- build_get ---------------------------------------------------------

    fn deploy_minimal(server_dir: &Path) -> PathBuf {
        let awdir = aw_dir(server_dir);
        std::fs::create_dir_all(&awdir).unwrap();
        std::fs::write(awdir.join("AccountMounts.lua"), "local ENABLE_ACCOUNTWIDE_MOUNTS = true\n").unwrap();
        std::fs::write(awdir.join("AccountPets.lua"), "local ENABLE_ACCOUNTWIDE_PETS = false\n").unwrap();
        // AccountAchievements.lua deliberately absent -> its 3 rows skipped.
        awdir
    }

    #[test]
    fn build_get_not_installed_reports_installed_false() {
        let d = tmp("get-not-installed");
        std::fs::create_dir_all(&d).unwrap();
        let v = build_get(&d);
        assert_eq!(v["installed"], false);
        assert_eq!(v["subsystems"], json!([]));
        assert_eq!(v["reputation"]["present"], false);
        assert_eq!(v["reputation"]["value"], "off");
        assert_eq!(v["reputation"]["active"], Value::Null);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn build_get_installed_reports_present_flags_only() {
        let d = tmp("get-installed");
        deploy_minimal(&d);
        let v = build_get(&d);
        assert_eq!(v["installed"], true);
        let subs = v["subsystems"].as_array().unwrap();
        // Only mounts + pets present -> 2 rows (achievements' 3 rows skipped).
        assert_eq!(subs.len(), 2);
        assert_eq!(subs[0]["key"], "ENABLE_ACCOUNTWIDE_MOUNTS");
        assert_eq!(subs[0]["value"], "on");
        assert_eq!(subs[0]["parent"], Value::Null);
        assert_eq!(subs[1]["key"], "ENABLE_ACCOUNTWIDE_PETS");
        assert_eq!(subs[1]["value"], "off");
        assert_eq!(v["reputation"]["present"], false);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn build_get_reputation_variant_active() {
        let d = tmp("get-rep");
        let awdir = deploy_minimal(&d);
        std::fs::write(
            awdir.join("AccountReputation_Default.lua"),
            "local ENABLE_ACCOUNTWIDE_REPUTATION = true\n",
        )
        .unwrap();
        let v = build_get(&d);
        assert_eq!(v["reputation"]["present"], true);
        assert_eq!(v["reputation"]["value"], "on");
        assert_eq!(v["reputation"]["active"], "default");
        assert_eq!(v["reputation"]["variants"], json!(["default"]));
        let _ = std::fs::remove_dir_all(&d);
    }

    // -- set_flag: validation ------------------------------------------------

    #[test]
    fn set_flag_bad_value_or_flag_shape() {
        let d = tmp("set-badarg");
        let e1 = set_flag(&d, "ENABLE_ACCOUNTWIDE_MOUNTS", "maybe", None).unwrap_err();
        assert_eq!(e1.code, "BAD_ARG");
        let e2 = set_flag(&d, "NOT_A_FLAG", "on", None).unwrap_err();
        assert_eq!(e2.code, "BAD_ARG");
    }

    #[test]
    fn set_flag_not_installed() {
        let d = tmp("set-not-installed");
        std::fs::create_dir_all(&d).unwrap();
        let e = set_flag(&d, "ENABLE_ACCOUNTWIDE_MOUNTS", "on", None).unwrap_err();
        assert_eq!(e.code, "NOT_INSTALLED");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn set_flag_unknown_registry_flag() {
        let d = tmp("set-unknown");
        deploy_minimal(&d);
        let e = set_flag(&d, "ENABLE_NOT_IN_REGISTRY", "on", None).unwrap_err();
        assert_eq!(e.code, "BAD_ARG");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn set_flag_generic_writes_and_reports_changed() {
        let d = tmp("set-generic");
        let awdir = deploy_minimal(&d);
        let v = set_flag(&d, "ENABLE_ACCOUNTWIDE_PETS", "on", None).unwrap();
        assert_eq!(v["key"], "ENABLE_ACCOUNTWIDE_PETS");
        assert_eq!(v["value"], "on");
        assert_eq!(v["changed"], true);
        assert_eq!(flag_read(&awdir.join("AccountPets.lua"), "ENABLE_ACCOUNTWIDE_PETS"), Some("on"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn set_flag_generic_absent_in_deployed_version() {
        let d = tmp("set-generic-absent");
        // Achievements file never deployed at all.
        deploy_minimal(&d);
        let e = set_flag(&d, "ENABLE_ACCOUNTWIDE_COMPLETED_ACHIEVEMENTS", "on", None).unwrap_err();
        assert_eq!(e.code, "NOT_FOUND");
        let _ = std::fs::remove_dir_all(&d);
    }

    // -- set_flag: reputation -------------------------------------------------

    #[test]
    fn set_reputation_no_files_deployed() {
        let d = tmp("rep-none");
        deploy_minimal(&d);
        let e = set_flag(&d, REPUTATION_FLAG, "on", None).unwrap_err();
        assert_eq!(e.code, "NOT_FOUND");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn set_reputation_single_variant_on_default_variant_arg() {
        let d = tmp("rep-single");
        let awdir = deploy_minimal(&d);
        std::fs::write(
            awdir.join("AccountReputation_Default.lua"),
            "local ENABLE_ACCOUNTWIDE_REPUTATION = false\n",
        )
        .unwrap();
        let v = set_flag(&d, REPUTATION_FLAG, "on", None).unwrap();
        assert_eq!(v["value"], "on");
        assert_eq!(v["variant"], "AccountReputation_Default.lua");
        assert_eq!(v["removed"], json!([]));
        assert_eq!(
            flag_read(&awdir.join("AccountReputation_Default.lua"), REPUTATION_FLAG),
            Some("on")
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn set_reputation_single_variant_mismatched_variant_arg_errors() {
        let d = tmp("rep-single-mismatch");
        let awdir = deploy_minimal(&d);
        std::fs::write(
            awdir.join("AccountReputation_Default.lua"),
            "local ENABLE_ACCOUNTWIDE_REPUTATION = false\n",
        )
        .unwrap();
        let e = set_flag(&d, REPUTATION_FLAG, "on", Some("custom")).unwrap_err();
        assert_eq!(e.code, "BAD_ARG");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn set_reputation_two_variants_requires_variant_arg() {
        let d = tmp("rep-two-noarg");
        let awdir = deploy_minimal(&d);
        std::fs::write(awdir.join("AccountReputation_Default.lua"), "local ENABLE_ACCOUNTWIDE_REPUTATION = false\n").unwrap();
        std::fs::write(awdir.join("AccountReputation_Custom.lua"), "local ENABLE_ACCOUNTWIDE_REPUTATION = false\n").unwrap();
        let e = set_flag(&d, REPUTATION_FLAG, "on", None).unwrap_err();
        assert_eq!(e.code, "BAD_ARG");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn set_reputation_two_variants_picks_chosen_removes_other() {
        let d = tmp("rep-two-pick");
        let awdir = deploy_minimal(&d);
        std::fs::write(awdir.join("AccountReputation_Default.lua"), "local ENABLE_ACCOUNTWIDE_REPUTATION = false\n").unwrap();
        std::fs::write(awdir.join("AccountReputation_Custom.lua"), "local ENABLE_ACCOUNTWIDE_REPUTATION = false\n").unwrap();
        let v = set_flag(&d, REPUTATION_FLAG, "on", Some("custom")).unwrap();
        assert_eq!(v["variant"], "AccountReputation_Custom.lua");
        assert_eq!(v["removed"], json!(["AccountReputation_Default.lua"]));
        assert!(!awdir.join("AccountReputation_Default.lua").exists());
        assert_eq!(
            flag_read(&awdir.join("AccountReputation_Custom.lua"), REPUTATION_FLAG),
            Some("on")
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn set_reputation_off_clears_without_deleting() {
        let d = tmp("rep-off");
        let awdir = deploy_minimal(&d);
        std::fs::write(awdir.join("AccountReputation_Default.lua"), "local ENABLE_ACCOUNTWIDE_REPUTATION = true\n").unwrap();
        let v = set_flag(&d, REPUTATION_FLAG, "off", None).unwrap();
        assert_eq!(v["value"], "off");
        assert_eq!(v["changed"], true);
        assert!(awdir.join("AccountReputation_Default.lua").exists());
        assert_eq!(
            flag_read(&awdir.join("AccountReputation_Default.lua"), REPUTATION_FLAG),
            Some("off")
        );
        let _ = std::fs::remove_dir_all(&d);
    }
}
