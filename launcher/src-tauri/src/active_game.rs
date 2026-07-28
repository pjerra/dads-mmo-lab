//! Which server the LIFECYCLE surfaces act on.
//!
//! Scope, deliberately narrow: only Home's status card + Start/Stop/Restart,
//! the sidebar chip and the tray follow this. The WoW-specific pages (GM
//! Tools, My Party, Item Database, Character, Teleport, Backups, Config) stay
//! bound to the WoW title — they are meaningless for a MapleStory or
//! RuneScape server, so re-pointing them would only produce confident-looking
//! nonsense.
//!
//! The stored choice is `launcher.json`'s `activeGame` (launcher state); the
//! resolution below turns it into an id that is actually installed RIGHT NOW.

/// Pick the active server from the stored choice and the currently installed
/// ids.
///
/// The fallback exists so that a user who has never chosen still gets a
/// defined answer instead of the UI improvising one:
///
/// 1. the stored id, **if it is still installed** — a title can be removed
///    while it is the active one, and pointing Start at a deleted directory
///    would fail in a way nobody could explain;
/// 2. else the WoW Playerbots title, when installed — every other page in the
///    launcher is hard-bound to it, so the lifecycle card agreeing with the
///    rest of the UI is the least surprising default, and it is the title the
///    installer sets up first;
/// 3. else the first installed id in sorted order — DETERMINISTIC on purpose:
///    picking "whatever the scan returned first" would let two launches with
///    the same servers land on different ones;
/// 4. else `None` — genuinely nothing installed, which the Library page
///    already has an empty state for.
pub fn resolve(stored: Option<&str>, installed: &[String]) -> Option<String> {
    if let Some(s) = stored.filter(|s| installed.iter().any(|i| i == s)) {
        return Some(s.to_string());
    }
    let wow = dml_wow::config::TITLE;
    if installed.iter().any(|i| i == wow) {
        return Some(wow.to_string());
    }
    let mut sorted: Vec<&String> = installed.iter().collect();
    sorted.sort();
    sorted.first().map(|s| (*s).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_stored_choice_that_is_still_installed_wins() {
        let got = resolve(Some("maplestory-server"), &ids(&["wow-server-playerbots", "maplestory-server"]));
        assert_eq!(got.as_deref(), Some("maplestory-server"));
    }

    #[test]
    fn a_stored_choice_that_is_gone_is_dropped_not_returned() {
        // The whole point: a removed title must not keep driving Start/Stop.
        let got = resolve(Some("runescape-server"), &ids(&["wow-server-playerbots"]));
        assert_eq!(got.as_deref(), Some("wow-server-playerbots"));
    }

    #[test]
    fn with_no_stored_choice_the_wow_title_is_the_default() {
        let got = resolve(None, &ids(&["maplestory-server", "wow-server-playerbots"]));
        assert_eq!(got.as_deref(), Some("wow-server-playerbots"));
    }

    #[test]
    fn without_the_wow_title_the_first_sorted_id_is_the_default() {
        // Deterministic across launches regardless of scan order.
        let a = resolve(None, &ids(&["runescape-server", "maplestory-server"]));
        let b = resolve(None, &ids(&["maplestory-server", "runescape-server"]));
        assert_eq!(a.as_deref(), Some("maplestory-server"));
        assert_eq!(a, b, "the fallback must not depend on list order");
    }

    #[test]
    fn nothing_installed_resolves_to_none() {
        assert_eq!(resolve(None, &[]), None);
        assert_eq!(resolve(Some("wow-server-playerbots"), &[]), None);
    }
}
