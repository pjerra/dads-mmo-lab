//! The Wrath Unbound CLIENT addons, embedded — and installed into the player's
//! WoW folder rather than the server's.
//!
//! ## Why this exists at all
//!
//! v1.4.0's cross-class talent system is only half a feature on the server.
//! `unbound_addon_sync.lua` validates picks that arrive over an addon message;
//! nothing *sends* those messages without the client UI. A server with the
//! bridge and no addon has a talent system the player cannot see or use, and
//! nothing in the game says why. So the add-on install ends by putting these
//! three addons in place, which turns "installed" into something the user can
//! actually go and look at.
//!
//! ## Three things that make this different from [`crate::unbound_payload`]
//!
//! 1. **It writes OUTSIDE the server directory** — into
//!    `<client>/Interface/AddOns/`, a path the launcher already knows because
//!    the realmlist feature uses it ([`crate::clientpath`]). That is the user's
//!    game install, so the rule here is: only ever touch the three addon
//!    directories this file ships, never anything else under `AddOns/`.
//! 2. **`include_bytes!`, not `include_str!`.** Five of the 43 files are `.blp`
//!    textures (WoW's own image format, 144 KB of the 643 KB). They are not
//!    UTF-8 and `include_str!` refuses to compile against them.
//! 3. **The whole tree is marked BINARY in `.gitattributes`.** Not because the
//!    Lua is binary — because these bytes are vendored third-party files that
//!    this repo never edits, and a fingerprint pin over them must not depend on
//!    whether a checkout applied EOL conversion. `-text` makes the bytes
//!    identical on every machine, which is the property the pin needs.
//!
//! ## What "fully automatic" does and does not mean
//!
//! The install runs this LAST and treats a failure as a warning, never as a
//! reason to fail a 30–90 minute rebuild that otherwise succeeded. If no client
//! path is configured it says so and skips, because guessing where somebody's
//! WoW lives and writing into it is not a thing to do quietly.

use std::path::{Path, PathBuf};

/// One addon file: its path relative to `Interface/AddOns/`, and its bytes.
#[derive(Debug, Clone, Copy)]
pub struct AddonFile {
    pub rel: &'static str,
    pub body: &'static [u8],
}

macro_rules! addon {
    ($rel:literal) => {
        AddonFile { rel: $rel, body: include_bytes!(concat!("../data/unbound-addons/", $rel)) }
    };
}

/// The three addon directories this ships. Nothing outside these names is ever
/// created, overwritten or removed under the player's `AddOns/`.
pub const ADDON_DIRS: [&str; 3] =
    ["UnboundSpellbook", "multiclass-resources", "multiclass-talents-ui"];

include!("unbound_addons_manifest.rs");

/// `<client>/Interface/AddOns` — where WoW loads addons from.
pub fn addons_dir(client_dir: &Path) -> PathBuf {
    client_dir.join("Interface").join("AddOns")
}

/// What an install attempt did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddonInstall {
    pub addons_dir: String,
    pub files: usize,
    pub addons: Vec<String>,
}

/// Write every addon file under `<client>/Interface/AddOns/`, overwriting.
///
/// Overwriting is correct rather than lazy: these ship WITH the server-side
/// bridge and the two must match — an addon left at an older version against a
/// newer bridge is exactly the mismatch that produces "my talents do nothing"
/// with no error anywhere. Only the three [`ADDON_DIRS`] are touched.
pub fn install_addons(client_dir: &Path) -> Result<AddonInstall, String> {
    let dir = addons_dir(client_dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for f in ADDON_FILES {
        let dest = dir.join(f.rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        // Not `conf::atomic_write`: that is a &str API and these are bytes.
        std::fs::write(&dest, f.body).map_err(|e| format!("{}: {e}", dest.display()))?;
    }
    Ok(AddonInstall {
        addons_dir: dir.display().to_string(),
        files: ADDON_FILES.len(),
        addons: ADDON_DIRS.iter().map(|s| s.to_string()).collect(),
    })
}

/// Write the same tree to any folder — for handing to other players.
///
/// A folder rather than a `.zip` on purpose: re-zipping would mean a new
/// dependency and a second artifact that can drift from the embedded bytes,
/// and a folder is what the recipient has to end up with anyway. Windows can
/// compress it in one right-click if the user wants to send it on.
pub fn export_addons(dest_dir: &Path) -> Result<AddonInstall, String> {
    std::fs::create_dir_all(dest_dir).map_err(|e| format!("{}: {e}", dest_dir.display()))?;
    for f in ADDON_FILES {
        let dest = dest_dir.join(f.rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&dest, f.body).map_err(|e| format!("{}: {e}", dest.display()))?;
    }
    Ok(AddonInstall {
        addons_dir: dest_dir.display().to_string(),
        files: ADDON_FILES.len(),
        addons: ADDON_DIRS.iter().map(|s| s.to_string()).collect(),
    })
}

/// FNV-1a 64 over every `(rel, len, body)` in manifest order — the same
/// one-number pin [`crate::unbound_payload`] uses, for the same reason.
pub fn addons_fingerprint() -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |bytes: &[u8]| {
        for b in bytes {
            h ^= *b as u64;
            h = h.wrapping_mul(0x1000_0000_01b3);
        }
    };
    for f in ADDON_FILES {
        eat(f.rel.as_bytes());
        eat(&(f.body.len() as u64).to_le_bytes());
        eat(f.body);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Re-baselined 2026-08-14. The original value pinned
    /// WrathUnbound-Addons.zip as supplied 2026-08-02, but UnboundSpellbook
    /// has been deliberately hand-edited in seven rounds since, and the pin
    /// was never moved with them -- so this test had been RED on the branch
    /// for six commits and was protecting nothing. It no longer means "byte
    /// identical to the upstream zip" (that stopped being true at round 1);
    /// it means "the payload changed only when someone meant it to". Move it
    /// deliberately with each edit, or it silently rots back to useless.
    /// Moved 2026-08-17 for the macro-pool round: UnboundSpellbook now
    /// reclaims its own /cast macros in place instead of spending one
    /// client macro slot per capped spell and never taking it back.
    const FINGERPRINT: u64 = 0x4d7e_2665_b272_ba5f;

    #[test]
    fn the_addon_payload_is_byte_pinned() {
        assert_eq!(ADDON_FILES.len(), 43, "addon file count changed");
        let total: usize = ADDON_FILES.iter().map(|f| f.body.len()).sum();
        assert_eq!(total, 685_375, "addon total byte count changed");
        assert_eq!(
            addons_fingerprint(),
            FINGERPRINT,
            "addon contents changed -- re-extract deliberately or revert"
        );
    }

    #[test]
    fn every_file_belongs_to_one_of_the_three_addons() {
        // The install writes into the player's OWN game folder. A path that
        // escaped its addon directory would put a file somewhere nobody asked
        // for, so this is a containment check, not bookkeeping.
        for f in ADDON_FILES {
            let top = f.rel.split('/').next().unwrap_or("");
            assert!(
                ADDON_DIRS.contains(&top),
                "{} is outside the three shipped addons",
                f.rel
            );
            assert!(!f.rel.contains(".."), "{} contains a parent-dir hop", f.rel);
            assert!(!f.rel.starts_with('/'), "{} is absolute", f.rel);
        }
        for name in ADDON_DIRS {
            assert!(
                ADDON_FILES.iter().any(|f| f.rel.starts_with(name)),
                "{name} ships no files"
            );
        }
    }

    #[test]
    fn each_addon_ships_the_toc_wow_needs_to_load_it() {
        // WoW loads an addon only if <Dir>/<Dir>.toc exists. Without it the
        // files sit there and the addon simply never appears -- the exact
        // silent-nothing this whole feature exists to avoid.
        for name in ADDON_DIRS {
            let toc = format!("{name}/{name}.toc");
            assert!(
                ADDON_FILES.iter().any(|f| f.rel == toc),
                "{toc} is missing -- WoW would never load {name}"
            );
        }
    }

    #[test]
    fn the_binary_art_survived_being_embedded() {
        // .blp is WoW's texture format and is NOT UTF-8. If a checkout or a
        // well-meaning tool ran EOL conversion over it, the bytes shift and the
        // UI renders blank -- so assert the magic, not just the presence.
        let blps: Vec<&AddonFile> =
            ADDON_FILES.iter().filter(|f| f.rel.ends_with(".blp")).collect();
        assert_eq!(blps.len(), 5, "expected 5 .blp textures");
        for f in blps {
            assert!(f.body.starts_with(b"BLP2"), "{} lost its BLP2 header", f.rel);
        }
    }

    #[test]
    fn addons_dir_is_the_path_wow_actually_reads() {
        let d = addons_dir(Path::new("C:/wow335ahd"));
        assert!(d.ends_with("AddOns"));
        assert!(d.to_string_lossy().replace('\\', "/").ends_with("Interface/AddOns"));
    }

    #[test]
    fn installing_writes_every_file_and_touches_nothing_else() {
        let root = std::env::temp_dir()
            .join(format!("dml-addons-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let addons = addons_dir(&root);
        std::fs::create_dir_all(&addons).unwrap();
        // A stranger's addon, which must survive untouched.
        std::fs::create_dir_all(addons.join("SomeoneElsesAddon")).unwrap();
        std::fs::write(addons.join("SomeoneElsesAddon/x.lua"), b"keep me").unwrap();

        let out = install_addons(&root).expect("install");
        assert_eq!(out.files, ADDON_FILES.len());
        for f in ADDON_FILES {
            let p = addons.join(f.rel);
            assert_eq!(std::fs::read(&p).unwrap(), f.body, "{} did not land", f.rel);
        }
        assert_eq!(
            std::fs::read(addons.join("SomeoneElsesAddon/x.lua")).unwrap(),
            b"keep me",
            "an unrelated addon was modified"
        );

        // Re-running overwrites rather than failing: the addons must track the
        // server-side bridge, and a stale one is the mismatch that reads as
        // "my talents do nothing".
        std::fs::write(addons.join("UnboundSpellbook/Core.lua"), b"stale").unwrap();
        install_addons(&root).expect("reinstall");
        let core = ADDON_FILES.iter().find(|f| f.rel == "UnboundSpellbook/Core.lua").unwrap();
        assert_eq!(std::fs::read(addons.join("UnboundSpellbook/Core.lua")).unwrap(), core.body);

        let _ = std::fs::remove_dir_all(&root);
    }
}
