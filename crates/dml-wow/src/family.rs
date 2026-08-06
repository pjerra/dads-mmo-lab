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
