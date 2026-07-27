//! clap arg surface for `dml-wow` — the WoW/AzerothCore per-game CLI.
//!
//! Kept intentionally thin: this module ONLY defines the parse tree
//! (`Cli`/`Cmd`). Dispatching a parsed `Cmd` to its `dml-wow` call lives in
//! `run.rs`; printing the resulting envelope lives in `out.rs`. No business
//! logic here — see the crate-level doc comment.

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "dml-wow",
    version,
    about = "DML per-game CLI for the WoW (AzerothCore + playerbots) server — JSON envelopes on stdout"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Cmd,
}

#[derive(Subcommand, Debug)]
pub enum Cmd {
    /// CLI + contract version envelope
    Version,
    /// Full server status (containers, SOAP, bots, ports)
    Status,
    /// SOAP server-info fields only
    ServerInfo,
    /// Last worldserver console lines
    ConsoleTail {
        /// Bounded 1..=1000, matching both sibling implementations:
        /// `launcher/src-tauri/src/lib.rs`'s native-mode `--lines` gate and
        /// the bash CLI's identical check (`cli/dml`'s `console-tail` arm).
        /// A gate here (clap's own `value_parser`) keeps `run.rs` thin and
        /// routes an out-of-range value through the ordinary
        /// BAD_ARGS/usage-error/exit-2 path rather than reaching
        /// `read_console_tail`/`docker logs --tail` unguarded.
        #[arg(long, default_value_t = 200, value_parser = clap::value_parser!(u32).range(1..=1000))]
        lines: u32,
    },
    /// Server settings — the curated registry and the raw conf-file editor
    Config {
        #[command(subcommand)]
        cmd: ConfigCmd,
    },
    /// Guided module tuning — the 13 curated activator knobs
    Tuning {
        #[command(subcommand)]
        cmd: TuningCmd,
    },
    /// Optional server modules (cpp / lua / sql)
    Module {
        #[command(subcommand)]
        cmd: ModuleCmd,
    },

    // -- Task 12: read-only database page reads (`dml_wow::{pages,paperdoll,
    // stats,iteminfo}`) --------------------------------------------------

    /// Real (non-bot) characters currently online (`pages::read_players_online`)
    PlayersOnline,
    /// Every real account + its characters (`pages::read_accounts`)
    Accounts,
    /// Filtered playerbots browser page (`pages::read_bots`)
    Bots {
        /// Name prefix (1-12 letters/digits/underscore)
        #[arg(long)]
        name: Option<String>,
        /// Class id: one of 1-9 or 11 (10 never shipped a class)
        #[arg(long)]
        class: Option<u32>,
        /// Inclusive lower level bound
        #[arg(long)]
        min_level: Option<u32>,
        /// Inclusive upper level bound
        #[arg(long)]
        max_level: Option<u32>,
        /// Online bots only
        #[arg(long)]
        online: bool,
        /// Page size, clamped to 1..=200 (default 50, matching bash)
        #[arg(long)]
        limit: Option<u32>,
        #[arg(long, default_value_t = 0)]
        offset: u32,
    },
    /// `game_tele` locations, optionally filtered by name (`pages::read_teleport_list`)
    TeleportList {
        #[arg(long)]
        search: Option<String>,
    },
    /// `item_template` search (`pages::read_items_search`)
    ItemsSearch {
        /// Required, non-empty (a LIKE-wrapped substring match)
        #[arg(long)]
        name: String,
        #[arg(long)]
        quality: Option<u32>,
        /// Inclusive lower required-level bound
        #[arg(long)]
        min_level: Option<u32>,
        /// Inclusive upper required-level bound
        #[arg(long)]
        max_level: Option<u32>,
    },
    /// One character's equipped gear + appearance (`paperdoll::read_paperdoll`)
    Paperdoll {
        /// 1-12 letters/digits/underscore
        name: String,
    },
    /// One character's achievement/talent summary (`pages::read_char_progress`)
    CharProgress {
        /// 1-12 letters/digits/underscore
        name: String,
    },
    /// One character's full earned-achievement list (`pages::read_achievements`)
    Achievements {
        /// 1-12 letters/digits/underscore
        name: String,
    },
    /// The Statistics page envelope — 19 queries, 18 run concurrently (`stats::read_stats`)
    Stats,
    /// Wowhead tooltip/icon info for one or more item entries (`iteminfo::read_item_info`)
    ItemInfo {
        /// Comma-separated item entry ids, e.g. `25,116,6948` (max 25)
        #[arg(value_parser = parse_item_ids)]
        ids: ItemIds,
    },
}

/// The parsed form of `item-info`'s positional argument. A thin newtype
/// rather than a bare `Vec<u32>` field: clap's derive treats a `Vec<T>`
/// field as MULTIPLE occurrences of a scalar `T` (one `u32` per argv token),
/// not one token parsed into a collection — wrapping opts back into "one
/// token, custom parser", the same way `console-tail`'s `--lines` stays a
/// plain `u32` field with a `.range()`-adapted parser rather than a `Vec`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemIds(pub Vec<u32>);

/// `dml-wow item-info`'s positional argument, `<ID>[,<ID>...]`. A CLI-argv
/// shape decision (bad ids are a usage error, exit 2), the same idiom as
/// `ConsoleTail`'s `--lines` range gate above: mirrors the bash CLI's own
/// gate on `--entries` (`90-main.sh`'s `item-info` arm — `^[0-9]+(,[0-9]+)*$`
/// then reject a count over 25) as a clap value_parser, so a malformed or
/// oversized id list never reaches `iteminfo::read_item_info` unguarded and
/// `run.rs`'s dispatch arm stays a single call.
fn parse_item_ids(raw: &str) -> Result<ItemIds, String> {
    let mut ids = Vec::new();
    for part in raw.split(',') {
        let id: u32 = part.parse().map_err(|_| {
            format!("not a valid item id: {part:?} (want comma-separated ids, e.g. 25,116,6948)")
        })?;
        ids.push(id);
    }
    if ids.len() > 25 {
        return Err(format!("too many ids ({}) -- 25 max per call", ids.len()));
    }
    Ok(ItemIds(ids))
}

/// `dml-wow config …` — mirrors the bash CLI's `dml wow config` arms, with
/// the raw-file trio renamed to plain verbs (`files`/`read`/`write` for
/// bash's `files`/`raw-read`/`raw-write`) now that they no longer share a
/// namespace with the curated `list`/`get`/`set`.
#[derive(Subcommand, Debug)]
pub enum ConfigCmd {
    /// Every curated setting with its live value (bash: `config list`)
    List,
    /// One curated setting with its live value; NOT_FOUND if the key is
    /// not in the registry. No bash equivalent — a convenience filter over
    /// the same registry + reader `list` uses.
    Get {
        /// Registry key, e.g. `rates.xp_kill`
        key: String,
    },
    /// Change one setting (bash: `config set --key … --value …`). A
    /// `conf:<file>.conf:<Key>` key takes the direct module-conf route.
    Set {
        /// Registry key, or `conf:<file>.conf:<Key>` for a direct write
        key: String,
        /// New value
        value: String,
    },
    /// The static registry only — no values read, no files touched
    /// (bash: `config registry`)
    Registry,
    /// Which files the raw editor may open (bash: `config files`)
    Files,
    /// Print one editable file's contents (bash: `config raw-read`)
    Read {
        /// File name as reported by `config files`
        name: String,
    },
    /// Overwrite one editable file with the body read from STDIN
    /// (bash: `config raw-write`). `.env` and `docker-compose.override.yml`
    /// are read-only here and are rejected — see the SECURITY note on
    /// `dml_wow::config::raw_write`.
    Write {
        /// File name as reported by `config files`
        name: String,
    },
}

/// `dml-wow tuning …` — bash's `config tuning-list` / `config tuning-set`.
#[derive(Subcommand, Debug)]
pub enum TuningCmd {
    /// Every tuning knob with its live value + installed state
    List,
    /// Change one tuning knob (conf-backed or lua-backed)
    Set {
        /// Tuning key, e.g. `sitmeansrest.duration`
        key: String,
        /// New value
        value: String,
    },
}

/// `dml-wow module …` — bash's `module list` / `module catalog`.
#[derive(Subcommand, Debug)]
pub enum ModuleCmd {
    /// Every module with its live install/deploy/rebuild state
    List,
    /// The static catalog only — no state read, no files touched
    Catalog,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_version() {
        let c = Cli::try_parse_from(["dml-wow", "version"]).unwrap();
        assert!(matches!(c.command, Cmd::Version));
    }

    #[test]
    fn unknown_subcommand_is_usage_error() {
        assert!(Cli::try_parse_from(["dml-wow", "definitely-not-a-cmd"]).is_err());
    }

    #[test]
    fn console_tail_takes_lines() {
        let c = Cli::try_parse_from(["dml-wow", "console-tail", "--lines", "50"]).unwrap();
        assert!(matches!(c.command, Cmd::ConsoleTail { lines: 50 }));
    }

    #[test]
    fn console_tail_default_lines_is_200() {
        let c = Cli::try_parse_from(["dml-wow", "console-tail"]).unwrap();
        assert!(matches!(c.command, Cmd::ConsoleTail { lines: 200 }));
    }

    #[test]
    fn parses_status_and_server_info() {
        assert!(matches!(
            Cli::try_parse_from(["dml-wow", "status"]).unwrap().command,
            Cmd::Status
        ));
        assert!(matches!(
            Cli::try_parse_from(["dml-wow", "server-info"]).unwrap().command,
            Cmd::ServerInfo
        ));
    }

    #[test]
    fn no_subcommand_is_usage_error() {
        assert!(Cli::try_parse_from(["dml-wow"]).is_err());
    }

    // -- `--lines` range gate (1..=1000, matching the launcher/bash-CLI
    // siblings) -- Task 10 review Finding 1.

    #[test]
    fn console_tail_lines_zero_is_usage_error() {
        assert!(Cli::try_parse_from(["dml-wow", "console-tail", "--lines", "0"]).is_err());
    }

    #[test]
    fn console_tail_lines_over_1000_is_usage_error() {
        assert!(Cli::try_parse_from(["dml-wow", "console-tail", "--lines", "1001"]).is_err());
    }

    #[test]
    fn console_tail_lines_far_over_range_is_usage_error() {
        // The brief's own out-of-range example -- well past u32, still a
        // clean usage error rather than an unbounded value reaching docker.
        assert!(Cli::try_parse_from(["dml-wow", "console-tail", "--lines", "4000000000"]).is_err());
    }

    #[test]
    fn console_tail_lines_boundary_values_are_accepted() {
        let low = Cli::try_parse_from(["dml-wow", "console-tail", "--lines", "1"]).unwrap();
        assert!(matches!(low.command, Cmd::ConsoleTail { lines: 1 }));
        let high = Cli::try_parse_from(["dml-wow", "console-tail", "--lines", "1000"]).unwrap();
        assert!(matches!(high.command, Cmd::ConsoleTail { lines: 1000 }));
    }

    // -- Task 11: config / tuning / module ---------------------------------

    /// Parse `argv` and return the `ConfigCmd`, or panic with the parse error.
    fn config_of(argv: &[&str]) -> ConfigCmd {
        match Cli::try_parse_from(argv).map(|c| c.command) {
            Ok(Cmd::Config { cmd }) => cmd,
            other => panic!("expected a Config subcommand from {argv:?}, got: {other:?}"),
        }
    }

    #[test]
    fn parses_config_valueless_arms() {
        assert!(matches!(config_of(&["dml-wow", "config", "list"]), ConfigCmd::List));
        assert!(matches!(config_of(&["dml-wow", "config", "registry"]), ConfigCmd::Registry));
        assert!(matches!(config_of(&["dml-wow", "config", "files"]), ConfigCmd::Files));
    }

    #[test]
    fn parses_config_get_with_key() {
        match config_of(&["dml-wow", "config", "get", "rates.xp_kill"]) {
            ConfigCmd::Get { key } => assert_eq!(key, "rates.xp_kill"),
            other => panic!("expected Get, got {other:?}"),
        }
    }

    #[test]
    fn parses_config_set_with_key_and_value() {
        match config_of(&["dml-wow", "config", "set", "rates.xp_kill", "3"]) {
            ConfigCmd::Set { key, value } => {
                assert_eq!(key, "rates.xp_kill");
                assert_eq!(value, "3");
            }
            other => panic!("expected Set, got {other:?}"),
        }
        // A `conf:` direct-route key is just a positional string — no special
        // parsing here; `dml_wow::config::config_set` owns that routing.
        match config_of(&["dml-wow", "config", "set", "conf:mod_x.conf:Some.Key", "v"]) {
            ConfigCmd::Set { key, .. } => assert_eq!(key, "conf:mod_x.conf:Some.Key"),
            other => panic!("expected Set, got {other:?}"),
        }
    }

    #[test]
    fn parses_config_read_and_write_names() {
        match config_of(&["dml-wow", "config", "read", "playerbots.conf"]) {
            ConfigCmd::Read { name } => assert_eq!(name, "playerbots.conf"),
            other => panic!("expected Read, got {other:?}"),
        }
        // `write` takes ONLY the name — the body arrives on stdin.
        match config_of(&["dml-wow", "config", "write", ".env"]) {
            ConfigCmd::Write { name } => assert_eq!(name, ".env"),
            other => panic!("expected Write, got {other:?}"),
        }
    }

    #[test]
    fn config_arms_require_their_positionals() {
        for argv in [
            vec!["dml-wow", "config", "get"],
            vec!["dml-wow", "config", "set"],
            vec!["dml-wow", "config", "set", "only-a-key"],
            vec!["dml-wow", "config", "read"],
            vec!["dml-wow", "config", "write"],
        ] {
            assert!(Cli::try_parse_from(&argv).is_err(), "{argv:?} should be a usage error");
        }
    }

    #[test]
    fn config_without_a_subcommand_is_a_usage_error() {
        assert!(Cli::try_parse_from(["dml-wow", "config"]).is_err());
        assert!(Cli::try_parse_from(["dml-wow", "config", "nope"]).is_err());
        assert!(Cli::try_parse_from(["dml-wow", "tuning"]).is_err());
        assert!(Cli::try_parse_from(["dml-wow", "module"]).is_err());
        // The bash spellings are NOT accepted — this CLI's raw-file trio is
        // `config files/read/write`.
        assert!(Cli::try_parse_from(["dml-wow", "config", "raw-read", "x"]).is_err());
        assert!(Cli::try_parse_from(["dml-wow", "config", "tuning-list"]).is_err());
    }

    #[test]
    fn parses_tuning_arms() {
        match Cli::try_parse_from(["dml-wow", "tuning", "list"]).unwrap().command {
            Cmd::Tuning { cmd } => assert!(matches!(cmd, TuningCmd::List)),
            other => panic!("expected Tuning, got {other:?}"),
        }
        match Cli::try_parse_from(["dml-wow", "tuning", "set", "sitmeansrest.duration", "45"])
            .unwrap()
            .command
        {
            Cmd::Tuning { cmd: TuningCmd::Set { key, value } } => {
                assert_eq!(key, "sitmeansrest.duration");
                assert_eq!(value, "45");
            }
            other => panic!("expected Tuning Set, got {other:?}"),
        }
        assert!(Cli::try_parse_from(["dml-wow", "tuning", "set", "only-a-key"]).is_err());
    }

    #[test]
    fn parses_module_arms() {
        match Cli::try_parse_from(["dml-wow", "module", "list"]).unwrap().command {
            Cmd::Module { cmd } => assert!(matches!(cmd, ModuleCmd::List)),
            other => panic!("expected Module, got {other:?}"),
        }
        match Cli::try_parse_from(["dml-wow", "module", "catalog"]).unwrap().command {
            Cmd::Module { cmd } => assert!(matches!(cmd, ModuleCmd::Catalog)),
            other => panic!("expected Module, got {other:?}"),
        }
    }

    // -- Task 12: database page reads ---------------------------------------

    #[test]
    fn parses_players_online_accounts_and_stats() {
        assert!(matches!(
            Cli::try_parse_from(["dml-wow", "players-online"]).unwrap().command,
            Cmd::PlayersOnline
        ));
        assert!(matches!(
            Cli::try_parse_from(["dml-wow", "accounts"]).unwrap().command,
            Cmd::Accounts
        ));
        assert!(matches!(Cli::try_parse_from(["dml-wow", "stats"]).unwrap().command, Cmd::Stats));
    }

    #[test]
    fn players_online_accounts_and_stats_reject_stray_args() {
        // None of these take flags/positionals -- an unexpected one is a
        // usage error, not silently ignored.
        assert!(Cli::try_parse_from(["dml-wow", "players-online", "extra"]).is_err());
        assert!(Cli::try_parse_from(["dml-wow", "accounts", "--bogus"]).is_err());
        assert!(Cli::try_parse_from(["dml-wow", "stats", "--bogus"]).is_err());
    }

    #[test]
    fn parses_bots_defaults() {
        match Cli::try_parse_from(["dml-wow", "bots"]).unwrap().command {
            Cmd::Bots { name, class, min_level, max_level, online, limit, offset } => {
                assert_eq!(name, None);
                assert_eq!(class, None);
                assert_eq!(min_level, None);
                assert_eq!(max_level, None);
                assert!(!online);
                assert_eq!(limit, None);
                assert_eq!(offset, 0);
            }
            other => panic!("expected Bots, got {other:?}"),
        }
    }

    #[test]
    fn parses_bots_all_flags() {
        match Cli::try_parse_from([
            "dml-wow", "bots", "--name", "Foo", "--class", "5", "--min-level", "10",
            "--max-level", "20", "--online", "--limit", "75", "--offset", "100",
        ])
        .unwrap()
        .command
        {
            Cmd::Bots { name, class, min_level, max_level, online, limit, offset } => {
                assert_eq!(name.as_deref(), Some("Foo"));
                assert_eq!(class, Some(5));
                assert_eq!(min_level, Some(10));
                assert_eq!(max_level, Some(20));
                assert!(online);
                assert_eq!(limit, Some(75));
                assert_eq!(offset, 100);
            }
            other => panic!("expected Bots, got {other:?}"),
        }
    }

    #[test]
    fn bots_rejects_non_numeric_flags() {
        for argv in [
            vec!["dml-wow", "bots", "--class", "not-a-number"],
            vec!["dml-wow", "bots", "--min-level", "abc"],
            vec!["dml-wow", "bots", "--limit", "-5"],
            vec!["dml-wow", "bots", "--offset", "abc"],
        ] {
            assert!(Cli::try_parse_from(&argv).is_err(), "{argv:?} should be a usage error");
        }
    }

    #[test]
    fn parses_teleport_list_default_and_search() {
        assert!(matches!(
            Cli::try_parse_from(["dml-wow", "teleport-list"]).unwrap().command,
            Cmd::TeleportList { search: None }
        ));
        match Cli::try_parse_from(["dml-wow", "teleport-list", "--search", "Orgrimmar"])
            .unwrap()
            .command
        {
            Cmd::TeleportList { search } => assert_eq!(search.as_deref(), Some("Orgrimmar")),
            other => panic!("expected TeleportList, got {other:?}"),
        }
    }

    #[test]
    fn parses_items_search_requires_name() {
        match Cli::try_parse_from(["dml-wow", "items-search", "--name", "Hearthstone"])
            .unwrap()
            .command
        {
            Cmd::ItemsSearch { name, quality, min_level, max_level } => {
                assert_eq!(name, "Hearthstone");
                assert_eq!(quality, None);
                assert_eq!(min_level, None);
                assert_eq!(max_level, None);
            }
            other => panic!("expected ItemsSearch, got {other:?}"),
        }
        // `--name` is required at the clap level -- an empty/whitespace-only
        // value still parses here (it is `run.rs`'s job to reject that, same
        // as `wow_items_search_read`'s own pre-check), but a MISSING flag is
        // a usage error.
        assert!(Cli::try_parse_from(["dml-wow", "items-search"]).is_err());
    }

    #[test]
    fn parses_items_search_all_flags() {
        match Cli::try_parse_from([
            "dml-wow", "items-search", "--name", "Sword", "--quality", "4", "--min-level", "10",
            "--max-level", "60",
        ])
        .unwrap()
        .command
        {
            Cmd::ItemsSearch { name, quality, min_level, max_level } => {
                assert_eq!(name, "Sword");
                assert_eq!(quality, Some(4));
                assert_eq!(min_level, Some(10));
                assert_eq!(max_level, Some(60));
            }
            other => panic!("expected ItemsSearch, got {other:?}"),
        }
    }

    #[test]
    fn parses_paperdoll_char_progress_achievements_names() {
        match Cli::try_parse_from(["dml-wow", "paperdoll", "Hypeer"]).unwrap().command {
            Cmd::Paperdoll { name } => assert_eq!(name, "Hypeer"),
            other => panic!("expected Paperdoll, got {other:?}"),
        }
        match Cli::try_parse_from(["dml-wow", "char-progress", "Hypeer"]).unwrap().command {
            Cmd::CharProgress { name } => assert_eq!(name, "Hypeer"),
            other => panic!("expected CharProgress, got {other:?}"),
        }
        match Cli::try_parse_from(["dml-wow", "achievements", "Hypeer"]).unwrap().command {
            Cmd::Achievements { name } => assert_eq!(name, "Hypeer"),
            other => panic!("expected Achievements, got {other:?}"),
        }
    }

    #[test]
    fn paperdoll_char_progress_achievements_require_the_name_positional() {
        assert!(Cli::try_parse_from(["dml-wow", "paperdoll"]).is_err());
        assert!(Cli::try_parse_from(["dml-wow", "char-progress"]).is_err());
        assert!(Cli::try_parse_from(["dml-wow", "achievements"]).is_err());
    }

    #[test]
    fn parses_item_info_single_and_multiple_ids() {
        match Cli::try_parse_from(["dml-wow", "item-info", "25"]).unwrap().command {
            Cmd::ItemInfo { ids } => assert_eq!(ids.0, vec![25]),
            other => panic!("expected ItemInfo, got {other:?}"),
        }
        match Cli::try_parse_from(["dml-wow", "item-info", "25,116,6948"]).unwrap().command {
            Cmd::ItemInfo { ids } => assert_eq!(ids.0, vec![25, 116, 6948]),
            other => panic!("expected ItemInfo, got {other:?}"),
        }
    }

    #[test]
    fn item_info_requires_the_ids_positional() {
        assert!(Cli::try_parse_from(["dml-wow", "item-info"]).is_err());
    }

    #[test]
    fn item_info_rejects_malformed_ids() {
        for argv in [
            vec!["dml-wow", "item-info", "abc"],
            vec!["dml-wow", "item-info", "25,"],
            vec!["dml-wow", "item-info", "25,,116"],
            vec!["dml-wow", "item-info", "25, 116"], // whitespace not allowed
            vec!["dml-wow", "item-info", ""],
        ] {
            assert!(Cli::try_parse_from(&argv).is_err(), "{argv:?} should be a usage error");
        }
    }

    #[test]
    fn item_info_rejects_more_than_25_ids() {
        let too_many = (1..=26).map(|n| n.to_string()).collect::<Vec<_>>().join(",");
        assert!(Cli::try_parse_from(["dml-wow", "item-info", &too_many]).is_err());
        // Exactly 25 is fine.
        let ok = (1..=25).map(|n| n.to_string()).collect::<Vec<_>>().join(",");
        match Cli::try_parse_from(["dml-wow", "item-info", &ok]).unwrap().command {
            Cmd::ItemInfo { ids } => assert_eq!(ids.0.len(), 25),
            other => panic!("expected ItemInfo, got {other:?}"),
        }
    }
}
