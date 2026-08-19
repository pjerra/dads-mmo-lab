//! The Wrath Unbound add-on payload: the files the installer writes into a
//! server directory, embedded in the binary.
//!
//! ## Where these bytes came from
//!
//! `guides/unbound-wrath/install-wrath-unbound-addon.sh` (**v1.4.0**) is 5140
//! lines, and roughly 4000 of them are heredocs writing C++, SQL, Lua and a git
//! patch into the server directory. Those are payload, not code, so they live
//! under `../data/unbound/` as real files and are embedded here with
//! `include_str!` — the same shape as the config/tuning/catalog snapshots in
//! [`crate::registry`].
//!
//! ## What v1.4.0 added over the 1.2.2 this port started from
//!
//! Two whole subsystems, and neither needed a new install STAGE — both are
//! files, so they arrive through the same manifest loop:
//!
//! * **Cross-class talents** (`lua/unbound_addon_sync.lua` +
//!   `lua/unbound_talent_data.lua`, 58 KB): a server-side bridge that validates
//!   talent picks from the client addon — allowlist, tier gating, prereqs, rank
//!   order and a shared point pool. The Mentor also sells talent points now,
//!   which is why `unbound_mentor.lua` grew 820 → 924 lines.
//! * **`mod-multiclass-summons`** (5 files under `summons/`): a SECOND C++
//!   module fixing warlock/mage/DK pet conflicts for multiclass characters.
//!
//! NB that module **does** ship its own `CMakeLists.txt` (an `AC_ADD_SCRIPT`
//! pair), unlike mod-unbound — so the "AzerothCore globs module sources" note
//! below explains why mod-unbound needs none, not a rule that modules never
//! have one. Its SQL sits under `data/sql/db-world/base/`, the one module path
//! AzerothCore's own DBUpdater auto-applies at startup.
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
//! No `CMakeLists.txt` for **mod-unbound**, and that is correct rather than
//! missing: AzerothCore's `modules/CMakeLists.txt` globs module sources with
//! `CollectSourceFiles()`, and 4 of the 5 modules on a stock DML server ship
//! none. The loader we DO ship defines `Addmod_unboundScripts()`, which is
//! exactly the symbol `ConfigureScriptLoader` generates for a directory named
//! `mod-unbound`. (mod-multiclass-summons ships one because it wants explicit
//! `AC_ADD_SCRIPT` entries; that file is part of the payload, not invented.)

/// Where the module tree lives inside a server directory.
pub const MODULE_REL: &str = "modules/mod-unbound";

/// The add-on version these bytes were extracted from. Bump only together with
/// a re-extraction — it is what `UNBOUND_VERSION_MISMATCH` compares against.
pub const ADDON_VERSION: &str = "1.4.0";

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
    payload!("env/dist/etc/modules/lua_scripts/unbound_addon_sync.lua", "lua/unbound_addon_sync.lua"),
    payload!("env/dist/etc/modules/lua_scripts/unbound_mentor.lua", "lua/unbound_mentor.lua"),
    payload!("env/dist/etc/modules/lua_scripts/unbound_talent_data.lua", "lua/unbound_talent_data.lua"),
    payload!("modules/mod-unbound/npc_setup.sql", "module/npc_setup.sql"),
    payload!("modules/mod-unbound/data/sql/db-characters/01_unbound_characters.sql", "module/sql/db-characters/01_unbound_characters.sql"),
    payload!("modules/mod-unbound/data/sql/db-world/01_unbound_world.sql", "module/sql/db-world/01_unbound_world.sql"),
    payload!("modules/mod-unbound/data/sql/db-world/02_fix_catalog_req_level.sql", "module/sql/db-world/02_fix_catalog_req_level.sql"),
    payload!("modules/mod-unbound/data/sql/db-world/03_creation_gift_spells.sql", "module/sql/db-world/03_creation_gift_spells.sql"),
    payload!("modules/mod-unbound/data/sql/db-world/04_catalog_druid_forms.sql", "module/sql/db-world/04_catalog_druid_forms.sql"),
    payload!("modules/mod-unbound/data/sql/db-world/05_individual_purchase_prereqs.sql", "module/sql/db-world/05_individual_purchase_prereqs.sql"),
    payload!("modules/mod-unbound/data/sql/db-world/06_universal_skill_access.sql", "module/sql/db-world/06_universal_skill_access.sql"),
    payload!("modules/mod-unbound/data/sql/db-world/07_mentor_stone.sql", "module/sql/db-world/07_mentor_stone.sql"),
    payload!("modules/mod-unbound/data/sql/db-world/08_catalog_additions.sql", "module/sql/db-world/08_catalog_additions.sql"),
    payload!("modules/mod-unbound/data/sql/db-world/10_catalog_audit_fixes.sql", "module/sql/db-world/10_catalog_audit_fixes.sql"),
    payload!("modules/mod-unbound/data/sql/db-world/11_catalog_gap_additions.sql", "module/sql/db-world/11_catalog_gap_additions.sql"),
    payload!("modules/mod-unbound/data/sql/db-world/12_mount_spell_fix.sql", "module/sql/db-world/12_mount_spell_fix.sql"),
    payload!("modules/mod-unbound/data/sql/db-world/13_flight_form_fix.sql", "module/sql/db-world/13_flight_form_fix.sql"),
    payload!("modules/mod-unbound/data/sql/db-world/14_judgement_fix.sql", "module/sql/db-world/14_judgement_fix.sql"),
    payload!("modules/mod-unbound/src/UnboundSystem.cpp", "module/src/UnboundSystem.cpp"),
    payload!("modules/mod-unbound/src/UnboundSystem_loader.cpp", "module/src/UnboundSystem_loader.cpp"),
    payload!("modules/mod-unbound/unbound-core-access.patch", "module/unbound-core-access.patch"),
    payload!("modules/mod-multiclass-summons/CMakeLists.txt", "summons/CMakeLists.txt"),
    payload!("modules/mod-multiclass-summons/data/sql/db-world/base/multiclass_summons.sql", "summons/data/sql/db-world/base/multiclass_summons.sql"),
    payload!("modules/mod-multiclass-summons/src/mod_multiclass_summons_loader.cpp", "summons/src/mod_multiclass_summons_loader.cpp"),
    payload!("modules/mod-multiclass-summons/src/multiclass_pet_fix.cpp", "summons/src/multiclass_pet_fix.cpp"),
    payload!("modules/mod-multiclass-summons/src/multiclass_pet_fix_loader.h", "summons/src/multiclass_pet_fix_loader.h"),
];

/// Which database a migration is applied to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlDb {
    World,
    Characters,
}

impl SqlDb {
    /// TASK 6 SCOPE RULING: these stay the STANDARD names on purpose — the
    /// Wrath Unbound payload is byte-pinned (FNV-1a fingerprint) third-party
    /// SQL whose own content hardcodes `acore_*` internally, so resolving
    /// only this argv while the payload hardcodes would be a half-rename
    /// that breaks worse than either consistent state. The add-on requires
    /// standard schema names end-to-end (its install guard refuses a server
    /// where `acore_world` does not answer). Do not resolve.
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

/// The multi-class summons module's spell-script registration, applied
/// best-effort by the SQL stage. Deliberately NOT in [`SQL_ORDER`]: a failure
/// must not abort the install, because AzerothCore also applies this file
/// itself from the module's `data/sql/db-world/base/` path.
pub const SUMMONS_SQL_DEST: &str =
    "modules/mod-multiclass-summons/data/sql/db-world/base/multiclass_summons.sql";

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

    /// The single pin, for add-on **1.4.0** as extracted on 2026-08-02.
    ///
    /// Regenerate ONLY together with a deliberate re-extraction, and say so in
    /// the commit message. Proven non-vacuous when written for 1.2.2 (flipping
    /// one byte of `02_fix_catalog_req_level.sql` turned it red), and it earned
    /// the keep again on the 1.4.0 re-extraction, which moved it.
    const FINGERPRINT: u64 = 0x1996_a52f_7d74_8909;

    #[test]
    fn the_payload_is_byte_pinned() {
        assert_eq!(MANIFEST.len(), 26, "payload file count changed");
        let total: usize = MANIFEST.iter().map(|f| f.body.len()).sum();
        assert_eq!(total, 174_399, "payload total byte count changed");
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
    fn every_payload_file_lands_in_one_of_the_three_trees_it_belongs_to() {
        // v1.4.0 spreads the payload across THREE destinations, and putting a
        // file in the wrong one produces a server that builds, boots, and is
        // quietly missing a feature:
        //
        //   * the ALE script dir -- the Lua the server actually SCANS. A
        //     mentor/talent script written under modules/ instead leaves a
        //     working server with no Mentor and no talent bridge.
        //   * mod-unbound -- the original C++ module, its patch and its SQL.
        //   * mod-multiclass-summons -- the v1.4.0 pet-conflict module, in its
        //     own tree, because two modules' sources must never mingle.
        const ALE: &str = "env/dist/etc/modules/lua_scripts/";
        const SUMMONS: &str = "modules/mod-multiclass-summons/";
        let (mut ale, mut unbound, mut summons) = (0, 0, 0);
        for f in MANIFEST {
            if f.dest.starts_with(ALE) {
                ale += 1;
            } else if f.dest.starts_with(SUMMONS) {
                summons += 1;
            } else if f.dest.starts_with(MODULE_REL) {
                unbound += 1;
            } else {
                panic!("{} is in none of the three known trees", f.dest);
            }
        }
        assert_eq!(ale, 3, "mentor + talent bridge + talent data");
        assert_eq!(summons, 5, "CMakeLists + 3 sources + its base SQL");
        assert_eq!(unbound, 18, "2 sources + patch + npc_setup + 14 migrations");

        // Named explicitly: a rename that silently dropped one of the three
        // would still satisfy the count above.
        for name in ["unbound_mentor.lua", "unbound_addon_sync.lua", "unbound_talent_data.lua"] {
            let dest = format!("{ALE}{name}");
            assert!(
                file(&dest).is_some(),
                "{name} is not in the ALE script path -- the server would never load it"
            );
        }
    }

    #[test]
    fn the_summons_module_keeps_its_sql_where_azerothcore_auto_applies_it() {
        // data/sql/db-world/base/ is the ONE module path AzerothCore's own
        // DBUpdater applies at startup, and it runs BEFORE
        // LoadSpellScriptNames() caches spell_script_names -- which is why the
        // module registers its script rows there rather than from C++ at
        // runtime. The source says so in a comment written by someone who
        // evidently tried the other way and had it picked up a restart late.
        let sql =
            file("modules/mod-multiclass-summons/data/sql/db-world/base/multiclass_summons.sql")
                .expect("summons SQL in manifest");
        assert!(sql.body.contains("spell_script_names"));
        assert!(sql.body.contains("spell_summon_pet_override"));
    }
}
