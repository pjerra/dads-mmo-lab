//! `unbound export` — a READ-ONLY snapshot of everything a player has earned
//! through Wrath Unbound, so it can be kept outside the server and re-applied
//! by hand if the add-on ever has to be removed and reinstalled.
//!
//! ## Why this is small
//!
//! Most of Unbound's tables are CONTENT, not progress, and the installer
//! recreates them from its own SQL every time:
//!
//! * `acore_world.unbound_milestones` — the price ladder (free at 5, 3g at 25,
//!   80g at 50, 300g at 70, 1500g at 80). Config.
//! * `acore_world.unbound_class_catalog` — which spells each class may buy and
//!   what they cost. Content, derived from the Playerbots trainer templates.
//!
//! Exactly TWO things are per-player and cannot be regenerated:
//!
//! 1. **`acore_characters.unbound_character_unlocks`** — one row per
//!    `(char_guid, class_id)`. Its own schema comment says "Never deleted —
//!    additive only", which is as close to a definition of progress as this
//!    add-on has.
//! 2. **The cross-class spells those unlocks were spent on**, which do NOT
//!    live in an Unbound table at all: they are ordinary
//!    `acore_characters.character_spell` rows. They survive an uninstall in
//!    the database and are then stripped at the character's next login,
//!    because the uninstall restores `ValidateSkillLearnedBySpells` to
//!    AzerothCore's default of 1. So a snapshot that captured only the unlock
//!    ledger would look complete and quietly lose every spell.
//!
//! ## Why the join is cross-schema
//!
//! "A cross-class spell" is not a property of `character_spell` — it is the
//! intersection of what the character KNOWS (characters schema) with what
//! Unbound SELLS (world schema), minus what the character's own class would
//! have taught them anyway. Pulling `character_spell` whole and filtering in
//! Rust would mean shipping every spell of every character across the wire —
//! on a 2000-bot server that is hundreds of thousands of rows to find a few
//! hundred. MySQL joins across schemas on the same server, so the filter runs
//! where the data is.
//!
//! ## Names
//!
//! The schema names are RESOLVED (`Database::name`), not the `acore_*`
//! literals `unbound::REVERT_SQL` deliberately hardcodes. That exception
//! exists because those statements are ports of the add-on's own SQL payload,
//! whose CONTENT hardcodes the standard names internally — a half-rename
//! breaks worse than either consistent state. These two queries are ours, they
//! splice no payload, and a renamed server is exactly the one where a
//! hardcoded name would silently export nothing.
//!
//! READ-ONLY: this module issues `SELECT` and nothing else, so it adds no
//! surface to THE MySQL WRITE POLICY. Re-importing a snapshot WOULD be a write
//! surface and is deliberately not built here — that is a user decision first.

use crate::db::{self, Database, DbConfig, DbError, QueryResult, SqlValue};
use serde_json::{json, Value};

/// The unlock ledger, in its own schema order.
pub const UNLOCKS_SQL: &str = "SELECT char_guid, class_id, unlocked_at_level, unlocked_ts \
     FROM unbound_character_unlocks ORDER BY char_guid, class_id";

/// Build the cross-class-spell query for a pair of resolved schema names.
///
/// TWO conditions, and the second was learned the expensive way — by running
/// the first version against the real server (2026-08-14).
///
/// * `c.class_id <> ch.class` — a catalog spell matching the character's OWN
///   class is one their trainer teaches anyway, not something they bought.
/// * **The join through `unbound_character_unlocks`** — the spell must come
///   from a class THIS character has actually unlocked. Without it the query
///   answered **5904** rows on a server where exactly **ONE** character has
///   ever used the Mentor: `characters` holds two thousand BOTS, the catalog
///   holds spells many classes legitimately know, and every incidental
///   overlap counted as purchased progress. Tightened, the same server
///   answers 1750 rows for that one character — ~219 per unlocked class
///   against a 1928-spell, 9-class catalog, which is the shape it should be.
///
/// The lesson generalises: a derived set is only progress if it is derived
/// from the ledger. `unbound_character_unlocks` IS the ledger; anything that
/// does not join through it is guessing from a shape.
pub fn spells_sql(characters_db: &str, world_db: &str) -> String {
    format!(
        "SELECT cs.guid, cs.spell, ch.class AS native_class, c.class_id AS from_class \
         FROM `{characters_db}`.character_spell cs \
         JOIN `{characters_db}`.characters ch ON ch.guid = cs.guid \
         JOIN `{characters_db}`.unbound_character_unlocks u ON u.char_guid = cs.guid \
         JOIN `{world_db}`.unbound_class_catalog c ON c.spell_id = cs.spell AND c.class_id = u.class_id \
         WHERE c.class_id <> ch.class \
         ORDER BY cs.guid, cs.spell"
    )
}

/// Turn a decoded result set into an array of objects keyed by column name —
/// the shape that survives a schema gaining a column, unlike positional rows.
fn rows_as_objects(r: &QueryResult) -> Vec<Value> {
    r.rows
        .iter()
        .map(|row| {
            let mut o = serde_json::Map::new();
            for (i, col) in r.columns.iter().enumerate() {
                o.insert(col.clone(), row.get(i).map(SqlValue::to_json).unwrap_or(Value::Null));
            }
            Value::Object(o)
        })
        .collect()
}

/// Assemble the snapshot envelope. Pure — the whole shape is testable from
/// fixture result sets, with no database anywhere near it.
///
/// `characters` is the count of DISTINCT characters that appear, which is the
/// only number a human can sanity-check against their own server ("we have
/// about forty players who used the Mentor").
pub fn assemble_export(unlocks: &QueryResult, spells: &QueryResult) -> Value {
    let unlock_rows = rows_as_objects(unlocks);
    let spell_rows = rows_as_objects(spells);

    let mut guids: Vec<&Value> = Vec::new();
    for row in unlock_rows.iter() {
        if let Some(g) = row.get("char_guid") {
            if !guids.contains(&g) {
                guids.push(g);
            }
        }
    }

    json!({
        "format": "dml-unbound-progress",
        "version": 1,
        "characters": guids.len(),
        "unlocks": unlock_rows,
        "cross_class_spells": spell_rows,
        "note": "Progress only. unbound_milestones and unbound_class_catalog are content and are recreated by `unbound install`.",
    })
}

/// Read both sets and assemble the snapshot. SYNCHRONOUS and BLOCKING, like
/// every other reader in this crate.
pub fn export(cfg: &DbConfig) -> Result<Value, DbError> {
    let unresolved =
        || DbError::NamesUnresolved("the server's own config could not answer the schema names".to_string());
    let names = cfg.db_names.as_ref().ok_or_else(unresolved)?;
    // `expect`-free: the three core names are required fields, so both arms
    // are always `Some` -- but a `?` here beats a panic if that ever changes.
    let characters_db = Database::Characters.name(names).ok_or_else(unresolved)?.to_string();
    let world_db = Database::World.name(names).ok_or_else(unresolved)?.to_string();

    let unlocks = db::query(cfg, Database::Characters, UNLOCKS_SQL)?;
    let spells = db::query(cfg, Database::Characters, &spells_sql(&characters_db, &world_db))?;
    Ok(assemble_export(&unlocks, &spells))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn qr(columns: &[&str], rows: Vec<Vec<SqlValue>>) -> QueryResult {
        QueryResult { columns: columns.iter().map(|s| s.to_string()).collect(), rows }
    }

    #[test]
    fn spells_sql_only_takes_spells_from_a_class_that_is_not_the_characters_own() {
        // The one condition that makes this a PROGRESS export rather than a
        // dump of everything the character knows. Without it, every character
        // exports their own class's whole trainer list and a re-import would
        // hand out spells nobody bought.
        let sql = spells_sql("acore_characters", "acore_world");
        assert!(sql.contains("c.class_id <> ch.class"), "the cross-class filter is missing: {sql}");
    }

    #[test]
    fn spells_sql_joins_through_the_unlock_ledger() {
        // Measured on the live server: WITHOUT this join the query answered
        // 5904 rows on a box where exactly one character has ever used the
        // Mentor -- it was counting two thousand bots' incidental catalog
        // overlaps as purchased progress. A snapshot that inflated by three
        // orders of magnitude would still have LOOKED like a working export.
        let sql = spells_sql("acore_characters", "acore_world");
        assert!(
            sql.contains("unbound_character_unlocks u ON u.char_guid = cs.guid"),
            "the export must derive spells from the unlock LEDGER, not from catalog shape: {sql}"
        );
        assert!(
            sql.contains("c.class_id = u.class_id"),
            "the spell must come from a class this character actually unlocked: {sql}"
        );
    }

    #[test]
    fn spells_sql_uses_the_resolved_schema_names_on_both_sides() {
        // A renamed server is exactly the one where hardcoded `acore_*` names
        // would silently export ZERO rows and look like an empty server.
        let sql = spells_sql("dads_chars", "dads_world");
        assert!(sql.contains("`dads_chars`.character_spell"), "characters schema not resolved: {sql}");
        assert!(sql.contains("`dads_chars`.characters"), "characters schema not resolved: {sql}");
        assert!(sql.contains("`dads_world`.unbound_class_catalog"), "world schema not resolved: {sql}");
        assert!(!sql.contains("acore_"), "a hardcoded schema name leaked in: {sql}");
    }

    #[test]
    fn both_queries_are_read_only() {
        // This module's whole claim to needing no MySQL-write sanction.
        for sql in [UNLOCKS_SQL.to_string(), spells_sql("c", "w")] {
            let upper = sql.to_uppercase();
            for banned in ["INSERT", "UPDATE ", "DELETE", "DROP", "ALTER", "REPLACE", "TRUNCATE", "CREATE"] {
                assert!(!upper.contains(banned), "{banned} found in a supposedly read-only query: {sql}");
            }
            assert!(upper.trim_start().starts_with("SELECT"), "not a SELECT: {sql}");
        }
    }

    #[test]
    fn assemble_export_keys_rows_by_column_name_and_counts_distinct_characters() {
        let unlocks = qr(
            &["char_guid", "class_id", "unlocked_at_level"],
            vec![
                vec![SqlValue::Int(7), SqlValue::Int(2), SqlValue::Int(25)],
                vec![SqlValue::Int(7), SqlValue::Int(8), SqlValue::Int(50)],
                vec![SqlValue::Int(9), SqlValue::Int(4), SqlValue::Int(5)],
            ],
        );
        let spells = qr(
            &["guid", "spell", "native_class", "from_class"],
            vec![vec![SqlValue::Int(7), SqlValue::Int(133), SqlValue::Int(1), SqlValue::Int(8)]],
        );

        let out = assemble_export(&unlocks, &spells);
        // Two guids across three unlock rows -- the count is DISTINCT
        // characters, not rows, because that is the number a human can check
        // against their own server.
        assert_eq!(out["characters"], json!(2));
        assert_eq!(out["unlocks"].as_array().unwrap().len(), 3);
        assert_eq!(out["unlocks"][0]["class_id"], json!(2));
        assert_eq!(out["unlocks"][0]["unlocked_at_level"], json!(25));
        assert_eq!(out["cross_class_spells"][0]["spell"], json!(133));
        assert_eq!(out["format"], json!("dml-unbound-progress"));
    }

    #[test]
    fn assemble_export_on_a_server_nobody_has_used_is_empty_not_broken() {
        let out = assemble_export(&qr(&["char_guid"], vec![]), &qr(&["guid"], vec![]));
        assert_eq!(out["characters"], json!(0));
        assert_eq!(out["unlocks"].as_array().unwrap().len(), 0);
        assert_eq!(out["cross_class_spells"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn assemble_export_survives_a_schema_that_gained_a_column() {
        // Rows are keyed by column NAME, so a future `unlocked_by` column
        // arrives in the snapshot instead of shifting every value one place.
        let unlocks = qr(
            &["char_guid", "class_id", "unlocked_at_level", "unlocked_by"],
            vec![vec![SqlValue::Int(1), SqlValue::Int(3), SqlValue::Int(5), SqlValue::Text("mentor".into())]],
        );
        let out = assemble_export(&unlocks, &qr(&["guid"], vec![]));
        assert_eq!(out["unlocks"][0]["class_id"], json!(3), "a new column shifted the existing ones");
        assert_eq!(out["unlocks"][0]["unlocked_by"], json!("mentor"));
    }
}
