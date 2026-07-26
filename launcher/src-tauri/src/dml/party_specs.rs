//! Native-mode **party premade-spec** reader (`wow party specs`, task D1a).
//! Faithful port of the `party specs` arm (`cli/src/90-main.sh:3042-3066`) +
//! `_party_pb_conf`/`_party_spec_rows` (`cli/src/50-party.sh:105-195`): a
//! pure filesystem parse of the deployed `playerbots.conf`'s premade specs,
//! no DB/SOAP/docker involved.
//!
//! Shape: `{"source":"playerbots.conf"|"playerbots.conf.dist","specs":[
//! {class_id,class,specno,name,link,tree}]}`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

/// `_class_name_from_id` (`50-party.sh:105-112`): `characters.class` id ->
/// the class name `party add --class` accepts. `None` for unsupported ids
/// (6 = deathknight, and anything else outside the WotLK 9-class set).
pub fn class_name_from_id(id: i64) -> Option<&'static str> {
    match id {
        1 => Some("warrior"),
        2 => Some("paladin"),
        3 => Some("hunter"),
        4 => Some("rogue"),
        5 => Some("priest"),
        7 => Some("shaman"),
        8 => Some("mage"),
        9 => Some("warlock"),
        11 => Some("druid"),
        _ => None,
    }
}

/// `_party_pb_conf` (`50-party.sh:136-145`): the deployed `playerbots.conf`
/// under a resolved server dir, falling back to its shipped `.dist`. `None`
/// when the title isn't installed (mirrors `_wow_server_dir` via
/// [`super::maint::resolve_server_dir`]) or neither file exists — matching
/// the single NOT_FOUND the CLI arm raises for both cases ("playerbots.conf
/// not found (nor its .dist)").
pub fn find_conf(title_dir: &Path) -> Option<(PathBuf, &'static str)> {
    let server_dir = super::maint::resolve_server_dir(title_dir)?;
    let conf = server_dir.join("env").join("dist").join("etc").join("modules").join("playerbots.conf");
    if conf.is_file() {
        return Some((conf, "playerbots.conf"));
    }
    let mut dist_name = conf.file_name()?.to_os_string();
    dist_name.push(".dist");
    let dist = conf.with_file_name(dist_name);
    if dist.is_file() {
        return Some((dist, "playerbots.conf.dist"));
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecRow {
    pub class_id: i64,
    pub specno: i64,
    pub name: String,
    /// `None` when no (or an empty) `PremadeSpecLink` line matched this
    /// (class,specno) — mirrors the bash `[[ -n "$slink" ]]` null gate.
    pub link: Option<String>,
    pub tree: Option<String>,
}

/// awk's `+0` numeric string coercion: the value of the longest LEADING
/// `[+-]?[0-9]+` prefix, or 0 if there is none. Good enough for the
/// well-formed `AiPlayerbot.PremadeSpec{Name,Link}.<n>.<n>[.<n>]` keys this
/// parses; a malformed key degrades to 0 exactly like the awk it mirrors.
fn awk_int(s: &str) -> i64 {
    let bytes = s.as_bytes();
    let mut i = 0;
    let neg = match bytes.first() {
        Some(b'-') => {
            i += 1;
            true
        }
        Some(b'+') => {
            i += 1;
            false
        }
        _ => false,
    };
    let start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == start {
        return 0;
    }
    let val: i64 = s[start..i].parse().unwrap_or(0);
    if neg {
        -val
    } else {
        val
    }
}

/// Sum of ASCII-digit characters anywhere in `s`, non-digits ignored — a
/// port of the awk `digsum()` helper (`50-party.sh:173`).
fn digsum(s: &str) -> i64 {
    s.bytes().filter(u8::is_ascii_digit).map(|b| i64::from(b - b'0')).sum()
}

/// `a/b/c` talent-tree point distribution for a `PremadeSpecLink` value —
/// the dash-separated groups' `digsum`s, missing groups treated as 0.
fn tree_for_link(link: &str) -> String {
    let groups: Vec<&str> = link.split('-').collect();
    let a = groups.first().copied().map(digsum).unwrap_or(0);
    let b = groups.get(1).copied().map(digsum).unwrap_or(0);
    let c = groups.get(2).copied().map(digsum).unwrap_or(0);
    format!("{a}/{b}/{c}")
}

/// `_party_spec_rows` (`50-party.sh:171-195`): parse a `playerbots.conf`'s
/// `AiPlayerbot.PremadeSpecName.<cls>.<spc>` / `…PremadeSpecLink.<cls>.<spc>
/// .<lvl>` lines into one row per (class,specno) that has a non-empty NAME
/// (a Link with no matching non-empty Name never surfaces — same as the
/// awk, which only iterates keys `seen` by the Name branch). Class 6
/// (deathknight) is excluded. The highest-`<lvl>` Link wins per key (ties
/// keep the LAST one seen, `>=`). Sorted by (class_id, specno) ascending.
pub fn parse_spec_rows(content: &str) -> Vec<SpecRow> {
    let mut names: HashMap<(i64, i64), String> = HashMap::new();
    let mut best_level: HashMap<(i64, i64), i64> = HashMap::new();
    let mut links: HashMap<(i64, i64), String> = HashMap::new();

    for raw_line in content.split('\n') {
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        let s = line.trim_start_matches([' ', '\t']);
        let is_name = s.starts_with("AiPlayerbot.PremadeSpecName.");
        let is_link = !is_name && s.starts_with("AiPlayerbot.PremadeSpecLink.");
        if !is_name && !is_link {
            continue;
        }
        let Some(eq) = s.find('=') else { continue };
        let key = s[..eq].trim_end_matches([' ', '\t']);
        let val = s[eq + 1..].trim_matches([' ', '\t']);
        // key = "AiPlayerbot" . {"PremadeSpecName"|"PremadeSpecLink"} . cls . spc [ . lvl ]
        let parts: Vec<&str> = key.split('.').collect();
        let cls = parts.get(2).copied().map(awk_int).unwrap_or(0);
        let spc = parts.get(3).copied().map(awk_int).unwrap_or(0);
        if cls == 6 {
            continue;
        }
        let k = (cls, spc);
        if is_name {
            if !val.is_empty() {
                names.insert(k, val.to_string());
            }
        } else {
            let lvl = parts.get(4).copied().map(awk_int).unwrap_or(0);
            let cur = *best_level.get(&k).unwrap_or(&0);
            if lvl >= cur {
                best_level.insert(k, lvl);
                links.insert(k, val.to_string());
            }
        }
    }

    let mut keys: Vec<(i64, i64)> = names.keys().copied().collect();
    keys.sort_unstable();
    keys.into_iter()
        .map(|k| {
            let name = names.get(&k).cloned().unwrap_or_default();
            let link_raw = links.get(&k).cloned().unwrap_or_default();
            let (link, tree) = if link_raw.is_empty() {
                (None, None)
            } else {
                (Some(link_raw.clone()), Some(tree_for_link(&link_raw)))
            };
            SpecRow { class_id: k.0, specno: k.1, name, link, tree }
        })
        .collect()
}

/// Assembles the full `party specs` `.data` payload from raw conf `content`
/// + its `source_name` (`"playerbots.conf"` or `"playerbots.conf.dist"`).
/// Rows whose class id doesn't resolve via [`class_name_from_id`] are
/// dropped, matching the arm's `[[ -n "$cname" ]] || continue` gate.
pub fn build_specs_value(content: &str, source_name: &str) -> Value {
    let specs: Vec<Value> = parse_spec_rows(content)
        .into_iter()
        .filter_map(|r| {
            let cname = class_name_from_id(r.class_id)?;
            Some(json!({
                "class_id": r.class_id,
                "class": cname,
                "specno": r.specno,
                "name": r.name,
                "link": r.link,
                "tree": r.tree,
            }))
        })
        .collect();
    json!({"source": source_name, "specs": specs})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_name_from_id_maps_9_classes_excludes_deathknight() {
        assert_eq!(class_name_from_id(1), Some("warrior"));
        assert_eq!(class_name_from_id(11), Some("druid"));
        assert_eq!(class_name_from_id(6), None); // deathknight
        assert_eq!(class_name_from_id(0), None);
        assert_eq!(class_name_from_id(99), None);
    }

    #[test]
    fn awk_int_leading_numeric_prefix() {
        assert_eq!(awk_int("3"), 3);
        assert_eq!(awk_int("-5"), -5);
        assert_eq!(awk_int(""), 0);
        assert_eq!(awk_int("abc"), 0);
    }

    #[test]
    fn digsum_sums_digit_chars_only() {
        assert_eq!(digsum("0500002105"), 0 + 5 + 0 + 0 + 0 + 0 + 2 + 1 + 0 + 5);
        assert_eq!(digsum(""), 0);
        assert_eq!(digsum("--"), 0);
    }

    #[test]
    fn tree_for_link_pads_missing_groups_with_zero() {
        assert_eq!(tree_for_link("05"), "5/0/0");
        assert_eq!(tree_for_link("05-03"), "5/3/0");
        assert_eq!(tree_for_link("05-03-01"), "5/3/1");
        // a 4th group is ignored, only the first 3 matter.
        assert_eq!(tree_for_link("05-03-01-09"), "5/3/1");
    }

    const SAMPLE_CONF: &str = "\
# comment line, ignored
AiPlayerbot.PremadeSpecName.1.0 = arms pve
AiPlayerbot.PremadeSpecLink.1.0.60 = 05-03-00\r
AiPlayerbot.PremadeSpecLink.1.0.80 = 05-05-00
AiPlayerbot.PremadeSpecName.1.1 = fury pve
AiPlayerbot.PremadeSpecName.6.0 = deathknight spec -- must be excluded
  AiPlayerbot.PremadeSpecName.2.0   =   holy pve
AiPlayerbot.PremadeSpecName.99.0 = unmapped class
AiPlayerbot.PremadeSpecLink.2.0.10 =
";

    #[test]
    fn parse_spec_rows_dedups_by_highest_level_link_excludes_dk() {
        let rows = parse_spec_rows(SAMPLE_CONF);
        // (1,0), (1,1), (2,0), (99,0) -- deathknight (6,0) excluded.
        assert_eq!(rows.len(), 4);
        let r10 = rows.iter().find(|r| r.class_id == 1 && r.specno == 0).unwrap();
        assert_eq!(r10.name, "arms pve");
        // Level 80 (>=60) wins over the level-60 line seen first.
        assert_eq!(r10.link.as_deref(), Some("05-05-00"));
        assert_eq!(r10.tree.as_deref(), Some("5/5/0"));

        let r11 = rows.iter().find(|r| r.class_id == 1 && r.specno == 1).unwrap();
        assert_eq!(r11.name, "fury pve");
        assert_eq!(r11.link, None);
        assert_eq!(r11.tree, None);

        // Leading-whitespace line + spaced '=' still parses; an EMPTY link
        // value nulls out both link and tree.
        let r20 = rows.iter().find(|r| r.class_id == 2 && r.specno == 0).unwrap();
        assert_eq!(r20.name, "holy pve");
        assert_eq!(r20.link, None);

        assert!(rows.iter().all(|r| r.class_id != 6));
    }

    #[test]
    fn parse_spec_rows_sorted_by_class_then_specno() {
        let conf = "\
AiPlayerbot.PremadeSpecName.9.1 = destro pvp
AiPlayerbot.PremadeSpecName.1.0 = arms pve
AiPlayerbot.PremadeSpecName.1.5 = prot pvp
";
        let rows = parse_spec_rows(conf);
        let ordered: Vec<(i64, i64)> = rows.iter().map(|r| (r.class_id, r.specno)).collect();
        assert_eq!(ordered, vec![(1, 0), (1, 5), (9, 1)]);
    }

    #[test]
    fn parse_spec_rows_link_only_no_name_never_surfaces() {
        let conf = "AiPlayerbot.PremadeSpecLink.1.0.60 = 05-03-00\n";
        assert_eq!(parse_spec_rows(conf), vec![]);
    }

    #[test]
    fn build_specs_value_drops_unmapped_class_and_shapes_json() {
        let v = build_specs_value(SAMPLE_CONF, "playerbots.conf");
        assert_eq!(v["source"], "playerbots.conf");
        let specs = v["specs"].as_array().unwrap();
        // (99,0) dropped: class_name_from_id(99) is None.
        assert_eq!(specs.len(), 3);
        assert!(specs.iter().all(|s| s["class_id"] != 99));
        let arms = specs.iter().find(|s| s["specno"] == 0 && s["class_id"] == 1).unwrap();
        assert_eq!(arms["class"], "warrior");
        assert_eq!(arms["name"], "arms pve");
        assert_eq!(arms["link"], "05-05-00");
        assert_eq!(arms["tree"], "5/5/0");
        let fury = specs.iter().find(|s| s["specno"] == 1).unwrap();
        assert_eq!(fury["link"], Value::Null);
        assert_eq!(fury["tree"], Value::Null);
    }

    #[test]
    fn find_conf_prefers_deployed_over_dist_and_none_when_neither() {
        let base = std::env::temp_dir().join(format!("dml-partyspecs-{}", std::process::id()));
        let title = base.join("wow-server-playerbots");
        let modules_dir = title.join("env").join("dist").join("etc").join("modules");
        std::fs::create_dir_all(&modules_dir).unwrap();
        std::fs::write(title.join("docker-compose.yml"), "x").unwrap();

        // Neither playerbots.conf nor .dist -> None (title installed, but no conf).
        assert_eq!(find_conf(&title), None);

        // Only .dist present -> that, with the .dist source name.
        std::fs::write(modules_dir.join("playerbots.conf.dist"), "x").unwrap();
        let (p, src) = find_conf(&title).expect("dist conf");
        assert!(p.ends_with("playerbots.conf.dist"));
        assert_eq!(src, "playerbots.conf.dist");

        // Deployed conf present -> preferred over .dist.
        std::fs::write(modules_dir.join("playerbots.conf"), "x").unwrap();
        let (p, src) = find_conf(&title).expect("deployed conf");
        assert!(p.ends_with("playerbots.conf") && !p.ends_with(".conf.dist"));
        assert_eq!(src, "playerbots.conf");

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn find_conf_none_when_title_not_installed() {
        let base = std::env::temp_dir().join(format!("dml-partyspecs-missing-{}", std::process::id()));
        // Deliberately do not create `base` -- resolve_server_dir requires
        // title_dir.is_dir().
        assert_eq!(find_conf(&base), None);
    }
}
