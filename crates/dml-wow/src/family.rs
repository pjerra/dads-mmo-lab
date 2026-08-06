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
    /// CORRECTED 2026-08-06. The first draft matched the literal
    /// `"family_from_container_names("` — but the definition is
    /// `pub fn family_from_container_names<'a>(`, where the lifetime sits
    /// between the name and the paren, so the literal never matched it. That
    /// was an ACCIDENTAL exclusion: remove the lifetime later and the count
    /// silently shifts by one for no visible reason. Match the bare symbol
    /// instead and exclude definitions EXPLICITLY via [`is_definition`].
    const RESOLVER_SYMBOL: &str = "family_from_container_names";
    const MARKER_TABLES: &[&str] = &["AC_MARKERS", "CMANGOS_SUFFIXES"];

    /// The ONLY file allowed to define the tables.
    const OWNER: &str = "family.rs";

    /// True when the occurrence of the resolver's name at byte offset `at`
    /// in `src` is its own `fn` definition rather than a call site — i.e.
    /// the text immediately before it, trimmed, ends with `fn`. Same idiom
    /// this repo uses elsewhere (`vocab_coverage_tests`) to tell a
    /// definition apart from a use.
    fn is_definition(src: &str, at: usize) -> bool {
        src[..at].trim_end().ends_with("fn")
    }

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

    /// One past the end of the item starting at `from` — the first `;` at
    /// depth zero, or the `}` that closes its first block. String and char
    /// literals are skipped so a brace or semicolon inside one (an assert
    /// message with `{name}` in it, say) cannot unbalance the match.
    ///
    /// CORRECTED 2026-08-06 (round 2): ported verbatim from
    /// `launcher/src-tauri/src/lib.rs`'s `end_of_item` (used there by
    /// `strip_cfg_test` inside `mod stop_outcome_scan_tests`), not
    /// reinvented — `dml-wow` cannot depend on the launcher crate (the
    /// dependency runs the other way: launcher depends on dml-wow) and the
    /// function lives in a `#[cfg(test)]`-private module there besides, so a
    /// literal import was not possible. Writing a second string-aware brace
    /// matcher independently would have been the exact two-resolvers shape
    /// this whole guard exists to catch, so this is a port, not a rewrite.
    fn end_of_item(b: &[char], from: usize) -> usize {
        let mut i = from;
        let mut depth = 0usize;
        while i < b.len() {
            match b[i] {
                '"' => {
                    i += 1;
                    while i < b.len() {
                        if b[i] == '\\' {
                            i += 2;
                            continue;
                        }
                        if b[i] == '"' {
                            break;
                        }
                        i += 1;
                    }
                }
                '\'' if i + 2 < b.len() && (b[i + 2] == '\'' || b[i + 1] == '\\') => {
                    i += 1;
                    while i < b.len() {
                        if b[i] == '\\' {
                            i += 2;
                            continue;
                        }
                        if b[i] == '\'' {
                            break;
                        }
                        i += 1;
                    }
                }
                '{' => depth += 1,
                '}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return i + 1;
                    }
                }
                ';' if depth == 0 => return i + 1,
                _ => {}
            }
            i += 1;
        }
        b.len()
    }

    /// Source with every `#[cfg(test)]` item removed, brace-matched — NOT
    /// cut at the first marker.
    ///
    /// CORRECTED 2026-08-06 (round 2): the first draft used
    /// `src.split("#[cfg(test)]").next()`, which truncates at the FIRST test
    /// module and makes every production line below it invisible to the
    /// scan. `family.rs` itself has two `#[cfg(test)]` modules with the real
    /// `family_from_container_names` definition between them, and
    /// `native.rs`/`tuning.rs` in this same crate are shaped the same way —
    /// this was not hypothetical. Two planted callers placed AFTER the
    /// file's first test module sailed through undetected; the identical
    /// callers placed before it were caught. Same idiom as
    /// `launcher/src-tauri/src/lib.rs`'s `strip_cfg_test` — ported, see
    /// `end_of_item`'s doc comment for why this is not a second
    /// implementation of the same stripper.
    ///
    /// Input must already be comment-stripped by [`strip_comments`], so a
    /// `{` inside prose cannot unbalance the match; string and char literals
    /// inside surviving code are still handled here, by `end_of_item`.
    fn strip_cfg_test(code: &str) -> String {
        let attr: Vec<char> = "#[cfg(test)]".chars().collect();
        let b: Vec<char> = code.chars().collect();
        let mut out = String::with_capacity(code.len());
        let mut i = 0usize;
        while i < b.len() {
            if b[i..].starts_with(&attr[..]) {
                i = end_of_item(&b, i + attr.len());
                continue;
            }
            out.push(b[i]);
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
            // many times and must not count. Brace-matched removal, not
            // cut-at-first — see strip_cfg_test's doc comment.
            let production = strip_cfg_test(&src);
            for (at, _) in production.match_indices(RESOLVER_SYMBOL) {
                if !is_definition(&production, at) {
                    calls += 1;
                }
            }
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
        // CORRECTED 2026-08-06. The old cap was <= 2, reasoning that the
        // definition contributed one occurrence and one caller a second. Both
        // halves were wrong: the definition's `<'a>` meant it never counted
        // (see RESOLVER_SYMBOL above), and with the real production count at
        // 0 today (no consumer exists yet on this branch — the first real
        // caller arrives with a later increment; do NOT read 0 as "the guard
        // is broken"), a cap of 2 was near-vacuous: it took THREE planted
        // callers to trip it. `is_definition` now excludes the definition
        // explicitly rather than by accident, so the cap can be the actual
        // invariant: AT MOST ONE production call site, full stop.
        assert!(
            calls <= 1,
            "the resolver is called from {calls} production sites. At most ONE may call it: \
             two callers with different fallbacks is exactly the games-dir incident this \
             guard exists to prevent, and this one picks the database and the SOAP namespace."
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
