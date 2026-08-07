//! Native-mode direct-MySQL reader for the character PAPERDOLL (Task 4, spike:
//! `spike/docker-desktop-native`).
//!
//! WHY. In WSL mode the character sheet's paperdoll is served by `dml wow
//! paperdoll --char <name>`, whose `docker exec ac-database mysql` read plus a
//! per-equipped-item fork costs the usual bash-on-Windows tax on every character
//! switch (the Dashboard auto-loads a paperdoll on each selection). In NATIVE
//! mode this reader runs the same joins over a DIRECT MySQL connection (via
//! [`super::db`]) and assembles the identical JSON.
//!
//! FAITHFUL PORT of the `paperdoll` arm (cli/src/90-main.sh ~2044-2133):
//!  - The character row + equipped items come from ONE join
//!    (`characters ⋈ character_inventory ⋈ item_instance ⋈ item_template`),
//!    equipment slots 0-18, ordered by slot.
//!  - SCHEMA FALLBACK: AC's migration split the packed `playerBytes`/
//!    `playerBytes2` appearance columns into discrete `skin/face/hairStyle/
//!    hairColor/facialStyle`. The reader tries the NEW schema first and, when
//!    those columns don't exist, falls back to the packed columns and unpacks
//!    them with the same bit masks the bash uses — both paths emit identical
//!    JSON.
//!  - Numbers splice through [`super::pages::num_token`] (the raw-numeric-splice
//!    fidelity the CLI's `"level":$lvl` produces); `gold = money / 10000`
//!    (integer division), `note` is the literal `"last_saved"`.
//!
//! DOCUMENTED DIVERGENCE (side-effect only, not a JSON field). The CLI arm, for
//! a character who is currently ONLINE, first fires a rate-limited SOAP
//! `saveall` to flush live gear to the DB before reading. This native reader
//! does NOT issue SOAP — it is a pure MySQL read by design (no engine console
//! dependency). The JSON SHAPE is identical; only a currently-online character's
//! gear could be marginally staler than a CLI read that happened to win the
//! saveall race. The parity test therefore exercises OFFLINE characters, where
//! the CLI also skips the flush, so the two reads are byte-identical.
//!
//! NOT-FOUND. An empty result set (no such character, or a character with no
//! equipped items) is [`Ok(None)`], which the caller maps to the CLI's
//! `NOT_FOUND` envelope. A DB failure is [`DbError`] (mapped to
//! `DB_UNREACHABLE`, as the arm does).

use serde_json::{json, Value};

use super::db::{self, Database, DbConfig, DbError, QueryResult};
use super::pages::{cell_text, num_token};

/// `true` when `name` passes the CLI's `_valid_charname` allowlist
/// (`^[A-Za-z0-9_]{1,12}$`). The command layer rejects anything else with
/// `BAD_ARG` before reaching the DB, exactly like the arm.
pub fn valid_charname(name: &str) -> bool {
    let len = name.chars().count();
    (1..=12).contains(&len)
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// The shared FROM/JOIN/WHERE tail of both paperdoll SELECTs (port of the
/// `pd_join` heredoc), with `name` as a bound `?` parameter rather than a
/// spliced-and-escaped literal (finding #1) — the caller ([`read_paperdoll`])
/// binds the already-[`valid_charname`]-checked name to this placeholder.
/// `world` is the RESOLVED world-schema name (Task 6; charset-gated by
/// [`crate::db::parse_database_info`]) — the query runs on the Characters
/// connection, so the cross-schema join must carry the name in-text.
fn join_tail(world: &str) -> String {
    format!(
        "FROM characters c \
         JOIN character_inventory ci ON ci.guid=c.guid AND ci.bag=0 AND ci.slot BETWEEN 0 AND 18 \
         JOIN item_instance ii ON ii.guid=ci.item \
         JOIN {world}.item_template it ON it.entry=ii.itemEntry \
         WHERE c.name=? ORDER BY ci.slot;"
    )
}

/// The NEW-schema SELECT (discrete appearance columns). 17 columns. `?` binds
/// to the character name — see [`join_tail`].
pub fn new_schema_sql(world: &str) -> String {
    format!(
        "SELECT c.name,c.level,c.class,c.money,c.race,c.gender,\
         c.skin,c.face,c.hairStyle,c.hairColor,c.facialStyle,\
         ci.slot,it.entry,it.name,it.Quality,it.ItemLevel,it.displayid {}",
        join_tail(world)
    )
}

/// The OLD-schema SELECT (packed `playerBytes`/`playerBytes2`). 15 columns. `?`
/// binds to the character name — see [`join_tail`].
pub fn old_schema_sql(world: &str) -> String {
    format!(
        "SELECT c.name,c.level,c.class,c.money,c.race,c.gender,\
         c.playerBytes,c.playerBytes2,\
         ci.slot,it.entry,it.name,it.Quality,it.ItemLevel,it.displayid {}",
        join_tail(world)
    )
}

/// Assemble the paperdoll `data` payload from a NEW-schema result set. `None`
/// when the set is empty (NOT_FOUND). Char-level fields come from the first
/// non-empty-name row (all rows carry the same character columns); each row
/// contributes one equipped item.
pub fn assemble_new(res: &QueryResult) -> Option<Value> {
    let texts = row_texts(res);
    let rows: Vec<&Vec<String>> = texts.iter().filter(|r| !cell(r, 0).is_empty()).collect();
    let first = rows.first()?;
    let equipped: Vec<Value> = rows.iter().map(|r| equipped_item(r, 11)).collect();
    Some(assemble_common(
        first,
        // skin,face,hair_style,hair_color,facial_style from discrete columns
        num_token(cell(first, 6)),
        num_token(cell(first, 7)),
        num_token(cell(first, 8)),
        num_token(cell(first, 9)),
        num_token(cell(first, 10)),
        equipped,
    ))
}

/// Assemble from an OLD-schema result set, unpacking the appearance bytes with
/// the same masks the bash uses (`skin = pb & 0xFF`, `face = (pb>>8)&0xFF`,
/// `hair_style = (pb>>16)&0xFF`, `hair_color = (pb>>24)&0xFF`,
/// `facial_style = pb2 & 0xFF`).
pub fn assemble_old(res: &QueryResult) -> Option<Value> {
    let texts = row_texts(res);
    let rows: Vec<&Vec<String>> = texts.iter().filter(|r| !cell(r, 0).is_empty()).collect();
    let first = rows.first()?;
    let equipped: Vec<Value> = rows.iter().map(|r| equipped_item(r, 8)).collect();
    let pb: i64 = cell(first, 6).parse().unwrap_or(0);
    let pb2: i64 = cell(first, 7).parse().unwrap_or(0);
    Some(assemble_common(
        first,
        json!(pb & 0xFF),
        json!((pb >> 8) & 0xFF),
        json!((pb >> 16) & 0xFF),
        json!((pb >> 24) & 0xFF),
        json!(pb2 & 0xFF),
        equipped,
    ))
}

/// The shared envelope both schemas emit — everything except the five
/// appearance fields, which each caller supplies pre-computed.
fn assemble_common(
    first: &[String],
    skin: Value,
    face: Value,
    hair_style: Value,
    hair_color: Value,
    facial_style: Value,
    equipped: Vec<Value>,
) -> Value {
    let money: i64 = cell(first, 3).parse().unwrap_or(0);
    json!({
        "name": cell(first, 0),
        "level": num_token(cell(first, 1)),
        "class": num_token(cell(first, 2)),
        "race": num_token(cell(first, 4)),
        "gender": num_token(cell(first, 5)),
        "skin": skin,
        "face": face,
        "hair_style": hair_style,
        "hair_color": hair_color,
        "facial_style": facial_style,
        "gold": money / 10000,
        "note": "last_saved",
        "equipped": equipped,
    })
}

/// Convert a whole result set to per-cell text (`mysql -N -B` form) once, so the
/// assembly reads plain strings.
fn row_texts(res: &QueryResult) -> Vec<Vec<String>> {
    res.rows
        .iter()
        .map(|r| r.iter().map(cell_text).collect())
        .collect()
}

/// Cell text at `idx`, or `""` when the row is shorter (defensive).
fn cell(row: &[String], idx: usize) -> &str {
    row.get(idx).map(String::as_str).unwrap_or("")
}

/// One equipped-item object `{slot,entry,name,quality,item_level,displayid}` from
/// a stringified row, given the column offset of `slot` (11 new, 8 old). The
/// five numeric fields splice through [`num_token`] like the CLI's raw splices;
/// `name` is the item-name string.
fn equipped_item(row: &[String], base: usize) -> Value {
    json!({
        "slot": num_token(cell(row, base)),
        "entry": num_token(cell(row, base + 1)),
        "name": cell(row, base + 2),
        "quality": num_token(cell(row, base + 3)),
        "item_level": num_token(cell(row, base + 4)),
        "displayid": num_token(cell(row, base + 5)),
    })
}

/// Read a character's paperdoll over a direct MySQL connection.
///
/// Tries the NEW appearance schema first; on a query error (columns absent on a
/// pre-migration DB) falls back to the OLD packed schema. `Ok(None)` = the
/// character has no such row / no equipped items (caller -> `NOT_FOUND`); a
/// connection/other failure is the [`DbError`] (caller -> `DB_UNREACHABLE`).
/// `name` must already be [`valid_charname`]-validated by the caller.
pub fn read_paperdoll(cfg: &DbConfig, name: &str) -> Result<Option<Value>, DbError> {
    let names = cfg.names()?;
    let params: Vec<mysql::Value> = vec![mysql::Value::from(name)];
    match db::query_with_params(cfg, Database::Characters, &new_schema_sql(&names.world), params.clone()) {
        Ok(res) => Ok(assemble_new(&res)),
        Err(DbError::Query(_)) => {
            // NEW columns absent -> old packed schema. A genuine unreachable-DB
            // error would have been DbError::Unreachable and is propagated above
            // by matching only Query(_) here; the old attempt then surfaces its
            // own error faithfully (both bash attempts error -> DB_UNREACHABLE).
            let res = db::query_with_params(cfg, Database::Characters, &old_schema_sql(&names.world), params)?;
            Ok(assemble_old(&res))
        }
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::SqlValue;

    fn t(s: &str) -> SqlValue {
        SqlValue::Text(s.to_string())
    }
    fn i(n: i64) -> SqlValue {
        SqlValue::Int(n)
    }
    fn qr(rows: Vec<Vec<SqlValue>>) -> QueryResult {
        QueryResult { columns: vec![], rows }
    }

    #[test]
    fn valid_charname_matches_bash_allowlist() {
        assert!(valid_charname("Hypeer"));
        assert!(valid_charname("Bot_01"));
        assert!(valid_charname("A"));
        assert!(valid_charname("Abcdefghijkl")); // 12
        assert!(!valid_charname("")); // 0
        assert!(!valid_charname("Abcdefghijklm")); // 13
        assert!(!valid_charname("O'Brien")); // apostrophe
        assert!(!valid_charname("na me")); // space
    }

    /// Re-anchored (Task 6): the world qualifier is the RESOLVED name the
    /// builder was handed — asserted with a renamed value so a fallback to
    /// `acore_world` goes red.
    #[test]
    fn sql_contains_join_and_slot_range() {
        let s = new_schema_sql("my_world");
        assert!(s.contains("c.skin,c.face,c.hairStyle,c.hairColor,c.facialStyle"));
        assert!(s.contains("ci.slot BETWEEN 0 AND 18"));
        assert!(s.contains("JOIN my_world.item_template it"), "got: {s}");
        assert!(!s.contains("acore_"), "got: {s}");
        // finding #1: the name is a bound `?` parameter, not a spliced literal
        // -- no character name ever appears in the SQL text itself.
        assert!(s.contains("WHERE c.name=? ORDER BY ci.slot;"));
        let o = old_schema_sql("my_world");
        assert!(o.contains("c.playerBytes,c.playerBytes2"));
        assert!(o.contains("JOIN my_world.item_template it"), "got: {o}");
        assert!(o.contains("WHERE c.name=? ORDER BY ci.slot;"));
    }

    #[test]
    fn assemble_new_shapes_character_and_gear() {
        // name,level,class,money,race,gender,skin,face,hairStyle,hairColor,
        // facialStyle,slot,entry,it.name,Quality,ItemLevel,displayid
        let res = qr(vec![
            vec![
                t("Hypeer"), i(80), i(2), i(1234567), i(1), i(0), i(3), i(4), i(5), i(6), i(7),
                i(0), i(49623), t("Cataclysm's Edge"), i(4), i(284), i(48297),
            ],
            vec![
                t("Hypeer"), i(80), i(2), i(1234567), i(1), i(0), i(3), i(4), i(5), i(6), i(7),
                i(15), i(50179), t("Sandals of Rich Purpose"), i(4), i(272), i(58312),
            ],
        ]);
        let got = assemble_new(&res).expect("some");
        assert_eq!(
            got,
            json!({
                "name":"Hypeer","level":80,"class":2,"race":1,"gender":0,
                "skin":3,"face":4,"hair_style":5,"hair_color":6,"facial_style":7,
                "gold":123, // 1234567 / 10000
                "note":"last_saved",
                "equipped":[
                    {"slot":0,"entry":49623,"name":"Cataclysm's Edge","quality":4,"item_level":284,"displayid":48297},
                    {"slot":15,"entry":50179,"name":"Sandals of Rich Purpose","quality":4,"item_level":272,"displayid":58312}
                ]
            })
        );
    }

    #[test]
    fn assemble_old_unpacks_appearance_bytes() {
        // playerBytes packs skin|face|hair_style|hair_color across the 4 bytes;
        // playerBytes2 low byte = facial_style.
        let pb = 0x06_05_04_03i64; // hair_color=6, hair_style=5, face=4, skin=3
        let pb2 = 0x07i64;
        let res = qr(vec![vec![
            t("Oldie"), i(70), i(1), i(90000), i(3), i(1),
            i(pb), i(pb2),
            i(1), i(40000), t("Necklace"), i(3), i(150), i(12345),
        ]]);
        let got = assemble_old(&res).expect("some");
        assert_eq!(got["skin"], json!(3));
        assert_eq!(got["face"], json!(4));
        assert_eq!(got["hair_style"], json!(5));
        assert_eq!(got["hair_color"], json!(6));
        assert_eq!(got["facial_style"], json!(7));
        assert_eq!(got["gold"], json!(9)); // 90000/10000
        assert_eq!(got["equipped"][0]["slot"], json!(1));
        assert_eq!(got["equipped"][0]["name"], json!("Necklace"));
    }

    #[test]
    fn empty_result_is_none() {
        assert!(assemble_new(&qr(vec![])).is_none());
        assert!(assemble_old(&qr(vec![])).is_none());
    }
}
