//! The Wrath Unbound add-on payload: the files the installer writes into a
//! server directory, embedded in the binary.
//!
//! ## Where these bytes came from
//!
//! `guides/unbound-wrath/install-wrath-unbound-addon.sh` is 3124 lines, but
//! only ~406 of them are logic. 1898 are heredocs writing C++, SQL, Lua and a
//! git patch into the server directory. Those are payload, not code, so they
//! live under `../data/unbound/` as real files and are embedded here with
//! `include_str!` — the same shape as the config/tuning/catalog snapshots in
//! [`crate::registry`].
//!
//! Extraction was proven, not reviewed: the extractor rebuilt the original
//! installer from the extracted files and diffed it against the source,
//! byte-identical. A mangled, mis-ranged or skipped body cannot produce that.
//!
//! ## Two things that will silently break this
//!
//! 1. **Line endings.** Six lines of `unbound-core-access.patch` are a SINGLE
//!    SPACE — unified-diff context for blank source lines. `git apply` compares
//!    context exactly, so a CRLF checkout turns each into `" \r"` and the patch
//!    stops applying, against a server the user already plays on, mid-rebuild.
//!    `.gitattributes` pins the whole tree to `eol=lf` (recursively — the
//!    `data/*.json` glob does not reach a subdirectory) and
//!    [`tests::the_payload_is_byte_pinned`] fails on any CR byte.
//! 2. **Formatters.** Nothing may reformat `data/unbound/`. The trailing
//!    whitespace is load-bearing in exactly the file where it looks like lint.
//!
//! ## What is deliberately NOT here
//!
//! `mod_ale.conf` — the one interpolated heredoc in the source script. Its nine
//! keys are written through the byte-parity conf engine instead
//! (`dml_core::conf`), which collapses the bash's write-fresh and
//! repair-in-place branches into one idempotent path and cannot produce the
//! duplicate-key or sed-matched-nothing outcomes the bash could.
//!
//! No `CMakeLists.txt` either, and that is correct rather than missing:
//! AzerothCore's `modules/CMakeLists.txt` globs module sources with
//! `CollectSourceFiles()`, and 4 of the 5 modules on a stock DML server ship
//! none. The loader we DO ship defines `Addmod_unboundScripts()`, which is
//! exactly the symbol `ConfigureScriptLoader` generates for a directory named
//! `mod-unbound`.

/// Where the module tree lives inside a server directory.
pub const MODULE_REL: &str = "modules/mod-unbound";

/// The add-on version these bytes were extracted from. Bump only together with
/// a re-extraction — it is what `UNBOUND_VERSION_MISMATCH` compares against.
pub const ADDON_VERSION: &str = "1.2.2";

/// One payload file: where it goes (relative to the SERVER dir) and its bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PayloadFile {
    /// Destination relative to the server directory, forward-slashed.
    pub dest: &'static str,
    pub body: &'static str,
}

macro_rules! payload {
    ($dest:literal, $src:literal) => {
        PayloadFile { dest: $dest, body: include_str!(concat!("../data/unbound/", $src)) }
    };
}

/// Every file the install writes, in no particular order — staging order does
/// not matter because nothing reads these until the rebuild. (SQL APPLICATION
/// order very much does matter: see [`SQL_ORDER`].)
pub const MANIFEST: &[PayloadFile] = &[
    payload!("modules/mod-unbound/src/UnboundSystem.cpp", "module/src/UnboundSystem.cpp"),
    payload!(
        "modules/mod-unbound/src/UnboundSystem_loader.cpp",
        "module/src/UnboundSystem_loader.cpp"
    ),
    payload!("modules/mod-unbound/npc_setup.sql", "module/npc_setup.sql"),
    payload!(
        "modules/mod-unbound/unbound-core-access.patch",
        "module/unbound-core-access.patch"
    ),
    payload!(
        "modules/mod-unbound/data/sql/db-world/01_unbound_world.sql",
        "module/sql/db-world/01_unbound_world.sql"
    ),
    payload!(
        "modules/mod-unbound/data/sql/db-world/02_fix_catalog_req_level.sql",
        "module/sql/db-world/02_fix_catalog_req_level.sql"
    ),
    payload!(
        "modules/mod-unbound/data/sql/db-world/03_creation_gift_spells.sql",
        "module/sql/db-world/03_creation_gift_spells.sql"
    ),
    payload!(
        "modules/mod-unbound/data/sql/db-world/04_catalog_druid_forms.sql",
        "module/sql/db-world/04_catalog_druid_forms.sql"
    ),
    payload!(
        "modules/mod-unbound/data/sql/db-world/05_individual_purchase_prereqs.sql",
        "module/sql/db-world/05_individual_purchase_prereqs.sql"
    ),
    payload!(
        "modules/mod-unbound/data/sql/db-world/06_universal_skill_access.sql",
        "module/sql/db-world/06_universal_skill_access.sql"
    ),
    payload!(
        "modules/mod-unbound/data/sql/db-world/07_mentor_stone.sql",
        "module/sql/db-world/07_mentor_stone.sql"
    ),
    payload!(
        "modules/mod-unbound/data/sql/db-world/08_catalog_additions.sql",
        "module/sql/db-world/08_catalog_additions.sql"
    ),
    payload!(
        "modules/mod-unbound/data/sql/db-world/10_catalog_audit_fixes.sql",
        "module/sql/db-world/10_catalog_audit_fixes.sql"
    ),
    payload!(
        "modules/mod-unbound/data/sql/db-world/11_catalog_gap_additions.sql",
        "module/sql/db-world/11_catalog_gap_additions.sql"
    ),
    payload!(
        "modules/mod-unbound/data/sql/db-world/12_mount_spell_fix.sql",
        "module/sql/db-world/12_mount_spell_fix.sql"
    ),
    payload!(
        "modules/mod-unbound/data/sql/db-world/13_flight_form_fix.sql",
        "module/sql/db-world/13_flight_form_fix.sql"
    ),
    payload!(
        "modules/mod-unbound/data/sql/db-world/14_judgement_fix.sql",
        "module/sql/db-world/14_judgement_fix.sql"
    ),
    payload!(
        "modules/mod-unbound/data/sql/db-characters/01_unbound_characters.sql",
        "module/sql/db-characters/01_unbound_characters.sql"
    ),
    payload!(
        "env/dist/etc/modules/lua_scripts/unbound_mentor.lua",
        "lua/unbound_mentor.lua"
    ),
];

/// Which database a migration is applied to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlDb {
    World,
    Characters,
}

impl SqlDb {
    pub fn database(self) -> &'static str {
        match self {
            SqlDb::World => "acore_world",
            SqlDb::Characters => "acore_characters",
        }
    }
}

/// The migrations, **in application order**, as `(dest, database)`.
///
/// Two things about this list are load-bearing and neither is obvious:
///
/// * **There is no `09`.** db-world runs 01–08 then 10–14. The gap is real and
///   matches the source script's own array. Renumbering to "fix" it would apply
///   a migration that does not exist and skip one that does.
/// * **`npc_setup.sql` is LAST, and it must be.** It creates the Mentor's
///   `creature_template` row (entry 900001). `unbound_mentor.lua` calls
///   `RegisterCreatureGossipEvent(900001, …)` at load, which crashes the Lua
///   state if the template is missing — and that crash prevents the
///   `[UNBOUND] Prereq map built.` marker from ever appearing, so the readiness
///   wait times out on a server that is otherwise fine. The source script
///   documents this in a comment written by someone who clearly hit it.
pub const SQL_ORDER: &[(&str, SqlDb)] = &[
    ("modules/mod-unbound/data/sql/db-world/01_unbound_world.sql", SqlDb::World),
    ("modules/mod-unbound/data/sql/db-world/02_fix_catalog_req_level.sql", SqlDb::World),
    ("modules/mod-unbound/data/sql/db-world/03_creation_gift_spells.sql", SqlDb::World),
    ("modules/mod-unbound/data/sql/db-world/04_catalog_druid_forms.sql", SqlDb::World),
    ("modules/mod-unbound/data/sql/db-world/05_individual_purchase_prereqs.sql", SqlDb::World),
    ("modules/mod-unbound/data/sql/db-world/06_universal_skill_access.sql", SqlDb::World),
    ("modules/mod-unbound/data/sql/db-world/07_mentor_stone.sql", SqlDb::World),
    ("modules/mod-unbound/data/sql/db-world/08_catalog_additions.sql", SqlDb::World),
    ("modules/mod-unbound/data/sql/db-world/10_catalog_audit_fixes.sql", SqlDb::World),
    ("modules/mod-unbound/data/sql/db-world/11_catalog_gap_additions.sql", SqlDb::World),
    ("modules/mod-unbound/data/sql/db-world/12_mount_spell_fix.sql", SqlDb::World),
    ("modules/mod-unbound/data/sql/db-world/13_flight_form_fix.sql", SqlDb::World),
    ("modules/mod-unbound/data/sql/db-world/14_judgement_fix.sql", SqlDb::World),
    (
        "modules/mod-unbound/data/sql/db-characters/01_unbound_characters.sql",
        SqlDb::Characters,
    ),
    ("modules/mod-unbound/npc_setup.sql", SqlDb::World),
];

/// The core patch, by destination — the one payload file that is applied with
/// `git apply` rather than merely written.
pub const PATCH_DEST: &str = "modules/mod-unbound/unbound-core-access.patch";

/// Look a payload file up by destination.
pub fn file(dest: &str) -> Option<&'static PayloadFile> {
    MANIFEST.iter().find(|f| f.dest == dest)
}

/// FNV-1a 64 over every `(dest, len, body)` in manifest order.
///
/// One number that changes if ANY byte, path or ordering changes. Cheaper to
/// maintain than 19 hardcoded lengths and strictly harder to fool: pinning only
/// the total size would let two files swap contents unnoticed.
pub fn manifest_fingerprint() -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |bytes: &[u8]| {
        for b in bytes {
            h ^= *b as u64;
            h = h.wrapping_mul(0x1000_0000_01b3);
        }
    };
    for f in MANIFEST {
        eat(f.dest.as_bytes());
        eat(&(f.body.len() as u64).to_le_bytes());
        eat(f.body.as_bytes());
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The single pin, for add-on 1.2.2 as extracted on 2026-08-01.
    ///
    /// Regenerate ONLY together with a deliberate re-extraction, and say so in
    /// the commit message. Proven non-vacuous the day it was written: flipping
    /// one byte of `02_fix_catalog_req_level.sql` turned this red.
    const FINGERPRINT: u64 = 0xd811_9e34_f27a_c082;

    #[test]
    fn the_payload_is_byte_pinned() {
        assert_eq!(MANIFEST.len(), 19, "payload file count changed");
        let total: usize = MANIFEST.iter().map(|f| f.body.len()).sum();
        assert_eq!(total, 84_976, "payload total byte count changed");
        assert_eq!(
            manifest_fingerprint(),
            FINGERPRINT,
            "payload contents changed -- re-extract deliberately or revert"
        );
    }

    #[test]
    fn no_payload_file_carries_a_carriage_return() {
        // A CRLF checkout is the failure this whole module warns about, and it
        // is invisible until `git apply` quietly stops matching.
        for f in MANIFEST {
            assert!(!f.body.contains('\r'), "CR byte in {}", f.dest);
        }
    }

    #[test]
    fn the_patch_keeps_its_six_single_space_context_lines() {
        // Unified-diff context for blank source lines. Any trailing-whitespace
        // strip -- an editor, a formatter, a well-meaning lint fix -- deletes
        // these and the patch stops applying with no useful error.
        let patch = file(PATCH_DEST).expect("patch in manifest").body;
        let n = patch.lines().filter(|l| *l == " ").count();
        assert_eq!(n, 6, "the patch's single-space context lines did not survive");
    }

    #[test]
    fn the_loader_defines_the_symbol_azerothcore_will_call() {
        // ConfigureScriptLoader generates `Add${dir}Scripts()` with `-` -> `_`,
        // so a module directory named `mod-unbound` is linked against
        // `Addmod_unboundScripts` and nothing else. If the module dir is ever
        // renamed, this is what catches it -- the build error otherwise
        // arrives 40 minutes into a rebuild.
        let loader = file("modules/mod-unbound/src/UnboundSystem_loader.cpp")
            .expect("loader in manifest")
            .body;
        assert!(loader.contains("void Addmod_unboundScripts()"), "loader symbol mismatch");
        assert!(MODULE_REL.ends_with("mod-unbound"));
    }

    #[test]
    fn every_migration_is_a_real_payload_file() {
        // SQL_ORDER and MANIFEST are written by hand and could drift apart; a
        // migration naming a file that does not exist would only surface
        // against a live database.
        for (dest, _) in SQL_ORDER {
            assert!(file(dest).is_some(), "{dest} is in SQL_ORDER but not in MANIFEST");
        }
    }

    #[test]
    fn the_migration_order_matches_the_source_script() {
        // Not a restatement of the constant: these assert the two properties
        // the bash's own comments say are load-bearing, either of which a
        // plausible "tidy-up" would break.
        let world: Vec<&str> = SQL_ORDER
            .iter()
            .filter(|(_, db)| *db == SqlDb::World)
            .map(|(d, _)| d.rsplit('/').next().unwrap())
            .collect();

        // npc_setup.sql runs LAST of all, not with its db-world siblings: it
        // creates creature_template 900001, and unbound_mentor.lua's
        // RegisterCreatureGossipEvent(900001) crashes the Lua state at load if
        // the row is missing -- which suppresses the readiness marker and times
        // the install out on a healthy server.
        assert_eq!(SQL_ORDER.last().unwrap().0, "modules/mod-unbound/npc_setup.sql");

        // There is no 09. The gap is in the source script's own array.
        assert!(!world.iter().any(|f| f.starts_with("09")), "a 09_* migration appeared");
        let numbered: Vec<&str> =
            world.iter().filter(|f| f.chars().next().unwrap().is_ascii_digit()).copied().collect();
        let mut sorted = numbered.clone();
        sorted.sort_unstable();
        assert_eq!(numbered, sorted, "db-world migrations are not in ascending file order");
        assert_eq!(numbered.len(), 13);
    }

    #[test]
    fn the_lua_lands_where_the_server_actually_reads_it() {
        // The one payload file that does NOT go under modules/. It belongs to
        // the ALE script path, and writing it into the module tree would leave
        // a server that builds, boots, and silently has no Mentor.
        let lua = file("env/dist/etc/modules/lua_scripts/unbound_mentor.lua");
        assert!(lua.is_some(), "the mentor lua moved out of the ALE script path");
        assert_eq!(
            MANIFEST.iter().filter(|f| !f.dest.starts_with(MODULE_REL)).count(),
            1,
            "exactly one payload file belongs outside the module tree"
        );
    }
}
