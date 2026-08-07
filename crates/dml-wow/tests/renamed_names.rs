//! Task 6 chunk R1 — the end-to-end pin for resolved schema names.
//!
//! WHY THIS FILE EXISTS AS ITS OWN TEST: no parity suite compares SQL text or
//! argv — all 18 compare JSON payloads, and on the live box BOTH surfaces
//! resolve `acore_*`, so a builder that quietly reverted to hardcoded
//! `acore_*` literals would be invisible to every existing test. This file is
//! the proof the survey said had to be NEW: a RENAMED [`DatabaseNames`] flows
//! through every user-data builder, the renamed strings come out, and no
//! `acore_` survives anywhere.

use dml_wow::db::DatabaseNames;

fn renamed() -> DatabaseNames {
    DatabaseNames {
        world: "my_world".to_string(),
        characters: "my_chars".to_string(),
        auth: "my_auth".to_string(),
        playerbots: Some("my_pb".to_string()),
    }
}

fn base_filters() -> dml_wow::pages::BotFilters {
    dml_wow::pages::BotFilters {
        name: None,
        class: None,
        min_level: None,
        max_level: None,
        online: false,
        limit: 50,
        offset: 0,
    }
}

/// Every builder, one sweep: each labelled text must carry the renamed
/// name(s) it is responsible for, and none may carry `acore_`.
#[test]
fn every_user_data_builder_carries_the_resolved_names_and_never_acore() {
    let n = renamed();

    let accounts = dml_wow::pages::accounts_sql("rndbot", &n.auth);
    assert!(accounts.contains("FROM my_auth.account a"), "accounts: {accounts}");
    assert!(accounts.contains("FROM my_auth.account_access"), "accounts: {accounts}");

    let bot = dml_wow::botid::bot_clause("c.account", "rndbot", &n.auth, n.playerbots.as_deref());
    assert!(bot.contains("my_pb.playerbots_account_type"), "bot clause: {bot}");
    assert!(bot.contains("my_auth.account"), "bot clause: {bot}");

    let probe = dml_wow::stats::probe_sql("my_pb");
    assert_eq!(probe, "SELECT 1 FROM my_pb.playerbots_account_type LIMIT 1;");

    let sys = dml_wow::stats::sys_subquery(&n.auth);
    assert!(sys.contains("SELECT id FROM my_auth.account"), "sys: {sys}");

    let doll_new = dml_wow::paperdoll::new_schema_sql(&n.world);
    let doll_old = dml_wow::paperdoll::old_schema_sql(&n.world);
    assert!(doll_new.contains("JOIN my_world.item_template it"), "paperdoll: {doll_new}");
    assert!(doll_old.contains("JOIN my_world.item_template it"), "paperdoll: {doll_old}");

    let dump = dml_wow::backup::mysqldump_args_for("db-c", "pw", true, &n);
    for schema in ["my_chars", "my_pb", "my_auth", "my_world"] {
        assert!(dump.iter().any(|a| a == schema), "dump argv misses {schema}: {dump:?}");
    }

    let (bots_where, _) = dml_wow::pages::bots_total_sql(&base_filters(), "rndbot", &n);
    assert!(bots_where.contains("my_pb.playerbots_account_type"), "bots: {bots_where}");
    assert!(bots_where.contains("my_auth.account"), "bots: {bots_where}");

    let party = dml_wow::party::bot_member_names_sql("rndbot", &n);
    assert!(party.contains("my_pb.playerbots_account_type"), "party: {party}");

    let migrate_present = dml_wow::migrate::sql_table_present(&n.characters);
    assert!(migrate_present.contains("table_schema='my_chars'"), "migrate: {migrate_present}");
    let migrate_chars = dml_wow::migrate::sql_character_count(&n.characters);
    assert!(migrate_chars.contains("FROM my_chars.characters"), "migrate: {migrate_chars}");
    let migrate_accounts = dml_wow::migrate::sql_account_count(&n.auth);
    assert!(migrate_accounts.contains("FROM my_auth.account"), "migrate: {migrate_accounts}");

    let all: Vec<(&str, String)> = vec![
        ("accounts_sql", accounts),
        ("bot_clause", bot),
        ("probe_sql", probe),
        ("sys_subquery", sys),
        ("paperdoll new_schema_sql", doll_new),
        ("paperdoll old_schema_sql", doll_old),
        ("mysqldump argv", dump.join(" ")),
        ("bots_total_sql", bots_where),
        ("bot_member_names_sql", party),
        ("migrate sql_table_present", migrate_present),
        ("migrate sql_character_count", migrate_chars),
        ("migrate sql_account_count", migrate_accounts),
    ];
    for (what, text) in all {
        assert!(
            !text.contains("acore_"),
            "{what} still hardcodes an acore_* name on a renamed server: {text}"
        );
    }
}

/// The playerbots-absent shape, swept across the surfaces that must DEGRADE
/// rather than fail or guess: the bot clause drops to prefix-only, the dump
/// set omits the schema, and the accounts picker (which never takes the name)
/// is byte-identical either way.
#[test]
fn a_playerbots_less_server_degrades_instead_of_guessing() {
    let n = DatabaseNames { playerbots: None, ..renamed() };

    let bot = dml_wow::botid::bot_clause("c.account", "rndbot", &n.auth, n.playerbots.as_deref());
    assert_eq!(bot, dml_wow::botid::bot_clause_prefix_only("c.account", "rndbot", &n.auth));
    assert!(!bot.contains("playerbots"), "no schema to qualify: {bot}");

    let dump = dml_wow::backup::mysqldump_args_for("db-c", "pw", false, &n);
    assert!(!dump.iter().any(|a| a.contains("playerbots")), "dump must omit it: {dump:?}");
    assert!(dump.iter().any(|a| a == "my_chars") && dump.iter().any(|a| a == "my_auth"));

    assert_eq!(
        dml_wow::pages::accounts_sql("rndbot", &n.auth),
        dml_wow::pages::accounts_sql("rndbot", &renamed().auth),
        "the picker never touches the playerbots schema, present or not"
    );
}
