//! Which emulator family a title's server is.
//!
//! THE SPINE: the family says which questions to ask; the installed server says
//! what the answers are. This type never holds a value the server already knows
//! — database names, container names and ports all come from the install, which
//! is what keeps this from becoming the recorded TWO-RESOLVERS-FOR-ONE-VALUE
//! bug.
//!
//! An enum rather than a string so that adding a family is a COMPILE ERROR at
//! every match rather than a silent fallthrough. `backend::from_override`'s
//! `_ => Backend::Wsl` catch-all is the live counter-example: it makes
//! `DML_BACKEND=auto` resolve Native and then run as Wsl.

/// A family this launcher can operate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreFamily {
    AzerothCore,
}

/// What inference concluded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FamilyVerdict {
    /// Identified, and this launcher can operate it.
    Known(CoreFamily),
    /// Identified, and this launcher cannot operate it YET (sub-projects #2/#3).
    /// Carrying the name is the whole point: the user gets "vanilla is not
    /// supported yet" instead of "unknown".
    Unsupported { family: &'static str },
    /// Nothing in the evidence identifies a family. NOT a default.
    Unknown,
}

/// Error code for [`FamilyVerdict::Unknown`].
pub const ERR_FAMILY_UNKNOWN: &str = "TITLE_FAMILY_UNKNOWN";
/// Error code for [`FamilyVerdict::Unsupported`].
pub const ERR_FAMILY_UNSUPPORTED: &str = "TITLE_FAMILY_UNSUPPORTED";

/// AzerothCore's two identifying containers. EXACT matches only.
const AC_MARKERS: &[&str] = &["ac-worldserver", "ac-authserver"];
/// CMaNGOS names its world/auth servers `<title>-mangosd` / `<title>-realmd`.
const CMANGOS_SUFFIXES: &[&str] = &["-mangosd", "-realmd"];

/// Pure: infer the family from a compose file's `container_name:` values.
///
/// Feed this the output of [`crate::install_native::parse_stack_owners`], never
/// a bare grep of the file — the repo already ate a false refusal from a compose
/// that merely MENTIONED an AC image.
///
/// A stack showing BOTH families is `Unknown`, not a majority vote: guessing
/// there sends `urn:AC` at a MaNGOS server and reads `acore_characters` from a
/// database called `characters`, which fails in the silently-wrong direction.
pub fn family_from_container_names<'a>(
    names: impl Iterator<Item = &'a str>,
) -> FamilyVerdict {
    let mut saw_ac = false;
    let mut saw_cmangos = false;
    for n in names {
        let n = n.trim();
        if AC_MARKERS.contains(&n) {
            saw_ac = true;
        }
        if CMANGOS_SUFFIXES.iter().any(|s| n.ends_with(s)) {
            saw_cmangos = true;
        }
    }
    match (saw_ac, saw_cmangos) {
        (true, false) => FamilyVerdict::Known(CoreFamily::AzerothCore),
        (false, true) => FamilyVerdict::Unsupported { family: "CMaNGOS" },
        _ => FamilyVerdict::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn azerothcore_is_identified_by_its_worldserver() {
        let names = ["ac-database", "ac-worldserver", "ac-authserver"];
        assert_eq!(
            family_from_container_names(names.iter().copied()),
            FamilyVerdict::Known(CoreFamily::AzerothCore)
        );
    }

    /// RECOGNISED BUT NOT SUPPORTED is a different answer from UNKNOWN, and the
    /// difference is what the user reads. "Vanilla servers are not supported
    /// yet" is true and actionable; "unknown server type" is neither.
    #[test]
    fn a_cmangos_stack_is_recognised_and_refused_by_name() {
        for names in [
            vec!["vanilla-db", "vanilla-mangosd", "vanilla-realmd"],
            vec!["tbc-db", "tbc-mangosd", "tbc-realmd"],
        ] {
            assert_eq!(
                family_from_container_names(names.iter().copied()),
                FamilyVerdict::Unsupported { family: "CMaNGOS" },
                "{names:?}"
            );
        }
    }

    #[test]
    fn nothing_identifiable_is_unknown_not_a_default() {
        for names in [vec![], vec!["mysql"], vec!["some-other-game-db"]] {
            assert_eq!(
                family_from_container_names(names.iter().copied()),
                FamilyVerdict::Unknown,
                "{names:?}"
            );
        }
    }

    /// A compose file holding BOTH families is not a majority vote. Guessing
    /// here sends urn:AC at a MaNGOS server (or the reverse) and every read
    /// lands in the wrong database while answering ok:true.
    #[test]
    fn a_mixed_stack_refuses_rather_than_picking_one() {
        let names = ["ac-worldserver", "vanilla-mangosd"];
        assert_eq!(
            family_from_container_names(names.iter().copied()),
            FamilyVerdict::Unknown
        );
    }

    /// EXACT names, never "contains". The repo already ate a false refusal from
    /// a compose file that merely MENTIONED an AC image, which is why
    /// parse_stack_owners anchors on the container_name key.
    #[test]
    fn a_lookalike_name_does_not_match() {
        for n in ["not-ac-worldserver", "ac-worldserver-backup", "mangosd"] {
            assert_eq!(
                family_from_container_names([n].iter().copied()),
                FamilyVerdict::Unknown,
                "{n} must not match"
            );
        }
    }
}

/// EXACTLY ONE PLACE DECIDES A TITLE'S FAMILY.
///
/// Modelled on the games-dir incident: two resolvers answered one question,
/// they agreed on the happy path, and only the FALLBACK disagreed — so every
/// read fell through to defaults and answered `ok:true` with numbers that were
/// not the server's. A second family resolver would be worse: it decides which
/// DATABASE to read and which SOAP namespace to send.
///
/// A runtime directory walk, not a fixed file list, because the failure mode is
/// a second resolver arriving in a file this test has never heard of.
#[cfg(test)]
mod resolver_scan_tests {
    use std::collections::BTreeSet;

    /// WHAT THIS CAN AND CANNOT CATCH — read before changing it.
    ///
    /// The obvious marker list (`"ac-worldserver"`, `"-mangosd"`, …) is WRONG
    /// and was tried first: those container names legitimately appear in 10+
    /// files, because `composegen` generates them and `lifecycle` stops them.
    /// Mentioning a container name is not deciding a family.
    ///
    /// So this guard pins two narrower things: exactly one production CALL of
    /// the resolver, and the marker tables living only in `family.rs`. It
    /// catches the two realistic accidents — a second caller with its own
    /// fallback (the games-dir shape) and a copy-pasted marker table. It does
    /// NOT catch a cleverly-rewritten independent implementation; nothing
    /// textual would. That residue is covered by review, and it is named here
    /// rather than papered over.
    const RESOLVER_CALL: &str = "family_from_container_names(";
    const MARKER_TABLES: &[&str] = &["AC_MARKERS", "CMANGOS_SUFFIXES"];

    /// The ONLY file allowed to define the tables.
    const OWNER: &str = "family.rs";

    fn strip_comments(src: &str) -> String {
        let mut out = String::with_capacity(src.len());
        let b: Vec<char> = src.chars().collect();
        let (mut i, mut in_str, mut in_line, mut in_block) = (0usize, false, false, 0usize);
        while i < b.len() {
            let c = b[i];
            let next = b.get(i + 1).copied().unwrap_or('\0');
            if in_line {
                if c == '\n' { in_line = false; out.push(c); }
            } else if in_block > 0 {
                if c == '*' && next == '/' { in_block -= 1; i += 2; continue; }
                if c == '/' && next == '*' { in_block += 1; i += 2; continue; }
                if c == '\n' { out.push(c); }
            } else if in_str {
                out.push(c);
                if c == '\\' { if let Some(n) = b.get(i + 1) { out.push(*n); } i += 2; continue; }
                if c == '"' { in_str = false; }
            } else if c == '/' && next == '/' {
                in_line = true;
            } else if c == '/' && next == '*' {
                in_block = 1; i += 2; continue;
            } else {
                if c == '"' { in_str = true; }
                out.push(c);
            }
            i += 1;
        }
        out
    }

    #[test]
    fn only_family_rs_decides_what_a_stack_is() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut scanned = 0usize;
        let mut calls = 0usize;
        let mut offenders: BTreeSet<String> = BTreeSet::new();
        for entry in std::fs::read_dir(&dir).expect("crates/dml-wow/src is unreadable") {
            let path = entry.expect("dir entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            scanned += 1;
            let src = strip_comments(&std::fs::read_to_string(&path).expect("read"));
            // Production half only: the tests in family.rs call the resolver
            // many times and must not count.
            let production = src.split("#[cfg(test)]").next().unwrap_or("");
            calls += production.matches(RESOLVER_CALL).count();
            if name == OWNER {
                continue;
            }
            for m in MARKER_TABLES {
                if src.contains(m) {
                    offenders.insert(format!("{name} contains the marker table {m:?}"));
                }
            }
        }
        // NON-VACUITY: a walk that found nothing would pass against anything.
        assert!(
            scanned >= 40,
            "the directory walk found only {scanned} .rs files — the scan is broken, not the code"
        );
        assert!(
            offenders.is_empty(),
            "a SECOND place carries the family marker table: {offenders:?}\n\
             Two resolvers that agree on the happy path and differ in the fallback is \
             the games-dir incident, and this one picks the database and the SOAP \
             namespace."
        );
        // The resolver's DEFINITION in family.rs contributes one occurrence
        // (`pub fn family_from_container_names(`), so one call site means two.
        assert!(
            calls <= 2,
            "{RESOLVER_CALL} appears {calls} times in production (definition + call sites). \
             More than one caller is how two fallbacks diverge."
        );
    }

    /// The stripper must not be fooled by prose — this file's own doc comments
    /// name every marker above.
    #[test]
    fn the_stripper_removes_comments_and_keeps_code() {
        assert!(!strip_comments("// ac-worldserver\n").contains("ac-worldserver"));
        assert!(!strip_comments("/* -mangosd */").contains("-mangosd"));
        assert!(strip_comments("let s = \"ac-worldserver\";").contains("ac-worldserver"));
        assert!(strip_comments("let s = \"// not a comment\";").contains("// not a comment"));
    }
}
