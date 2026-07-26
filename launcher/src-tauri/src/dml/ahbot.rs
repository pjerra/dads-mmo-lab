//! Native-mode `wow ahbot repair` pure helpers (Chunk 2 task C2c item 8 --
//! see `.superpowers/sdd/chunk2-decisions.md`). A faithful port of the
//! module-detection and fork-specific conf-key logic from
//! `90-main.sh:4239-4416`.
//!
//! WHY A SEPARATE MODULE. The DB lookup (`SELECT guid, account ...`), the
//! `mod_ahbot.conf` writes (via `dml::config::conf_write`, REUSED not
//! reimplemented -- Subsystem B1), the legacy-env cleanup, and the SOAP
//! `reload config` call all need real I/O and a `Channel` to stream NDJSON
//! events, so that orchestration lives in
//! `lib.rs::wow_ahbot_repair_native_blocking`, alongside every other native
//! streamed command's blocking half. What's left here -- which fork is
//! installed, and which conf keys to write for it -- is pure and testable
//! without a live server or filesystem writes.
//!
//! FORK DIFFERENCE (verified against each fork's `conf/mod_ahbot.conf.dist`,
//! 2026-07-20, carried over from the bash comment at `90-main.sh:4337-4345`):
//! the original `azerothcore/mod-ah-bot` keys a single `Account` + `GUID`
//! with separate `Enable{Seller,Buyer}`; the `NathanHandley/mod-ah-bot-plus`
//! fork renamed these -- no `Account`, a plural `GUIDs`, and the buyer switch
//! nested under `Buyer.Enabled` -- only `EnableSeller` is shared.

use std::path::Path;

/// The one manual step the automation can never do (character creation is
/// client-side only) -- surfaced both as an error hint and in the `done`
/// payload, verbatim (`90-main.sh:4271`).
pub const MANUAL_STEPS: &str = "Create a separate account for the bot (Accounts page), log into the game with it once, create ONE character, log out, then pick that character here.";

/// Which AH-bot fork is installed under `<server dir>/modules` -- a plain-dir
/// presence check, mirroring `90-main.sh:4295-4297`. `mod-ah-bot-plus` wins
/// if somehow both are present ("install one or the other, not both").
pub fn detect_module(server_dir: &Path) -> Option<&'static str> {
    if server_dir.join("modules").join("mod-ah-bot-plus").is_dir() {
        Some("mod-ah-bot-plus")
    } else if server_dir.join("modules").join("mod-ah-bot").is_dir() {
        Some("mod-ah-bot")
    } else {
        None
    }
}

/// Fork-specific `mod_ahbot.conf` keys to write, in write order -- a port of
/// the `ah_keys` array build (`90-main.sh:4346-4350`). Any `module` other
/// than `"mod-ah-bot-plus"` gets the original fork's keys (matches the
/// bash's `if/else`, not an exhaustive match on both names).
pub fn conf_keys(module: &str, guid: u64, account: u64) -> Vec<(&'static str, String)> {
    if module == "mod-ah-bot-plus" {
        vec![
            ("AuctionHouseBot.GUIDs", guid.to_string()),
            ("AuctionHouseBot.EnableSeller", "1".to_string()),
            ("AuctionHouseBot.Buyer.Enabled", "1".to_string()),
        ]
    } else {
        vec![
            ("AuctionHouseBot.Account", account.to_string()),
            ("AuctionHouseBot.GUID", guid.to_string()),
            ("AuctionHouseBot.EnableSeller", "1".to_string()),
            ("AuctionHouseBot.EnableBuyer", "1".to_string()),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp_dir(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("dml_ahbot_test_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn detect_module_none_when_neither_installed() {
        let dir = tmp_dir("none");
        std::fs::create_dir_all(dir.join("modules")).unwrap();
        assert_eq!(detect_module(&dir), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_module_plain_fork() {
        let dir = tmp_dir("plain");
        std::fs::create_dir_all(dir.join("modules").join("mod-ah-bot")).unwrap();
        assert_eq!(detect_module(&dir), Some("mod-ah-bot"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_module_plus_fork() {
        let dir = tmp_dir("plus");
        std::fs::create_dir_all(dir.join("modules").join("mod-ah-bot-plus")).unwrap();
        assert_eq!(detect_module(&dir), Some("mod-ah-bot-plus"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_module_plus_wins_when_both_present() {
        let dir = tmp_dir("both");
        std::fs::create_dir_all(dir.join("modules").join("mod-ah-bot")).unwrap();
        std::fs::create_dir_all(dir.join("modules").join("mod-ah-bot-plus")).unwrap();
        assert_eq!(detect_module(&dir), Some("mod-ah-bot-plus"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_module_ignores_a_file_named_like_the_module() {
        // A plain-dir check (`is_dir()`) must not be fooled by a stray file.
        let dir = tmp_dir("file_not_dir");
        std::fs::create_dir_all(dir.join("modules")).unwrap();
        std::fs::write(dir.join("modules").join("mod-ah-bot"), b"not a dir").unwrap();
        assert_eq!(detect_module(&dir), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn conf_keys_plus_fork_shape() {
        let keys = conf_keys("mod-ah-bot-plus", 42, 7);
        assert_eq!(
            keys,
            vec![
                ("AuctionHouseBot.GUIDs", "42".to_string()),
                ("AuctionHouseBot.EnableSeller", "1".to_string()),
                ("AuctionHouseBot.Buyer.Enabled", "1".to_string()),
            ]
        );
    }

    #[test]
    fn conf_keys_plain_fork_shape() {
        let keys = conf_keys("mod-ah-bot", 42, 7);
        assert_eq!(
            keys,
            vec![
                ("AuctionHouseBot.Account", "7".to_string()),
                ("AuctionHouseBot.GUID", "42".to_string()),
                ("AuctionHouseBot.EnableSeller", "1".to_string()),
                ("AuctionHouseBot.EnableBuyer", "1".to_string()),
            ]
        );
    }
}
