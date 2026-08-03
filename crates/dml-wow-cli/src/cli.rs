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

    /// Real (non-bot) characters currently online
    PlayersOnline,
    /// Every real account + its characters
    Accounts,
    /// Filtered playerbots browser page
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
    /// Teleport destinations (`game_tele`), optionally filtered by name
    TeleportList {
        #[arg(long)]
        search: Option<String>,
    },
    /// Search the item database (`item_template`)
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
    /// One character's equipped gear + appearance
    Paperdoll {
        /// 1-12 letters/digits/underscore
        name: String,
    },
    /// One character's achievement/talent summary
    CharProgress {
        /// 1-12 letters/digits/underscore
        name: String,
    },
    /// One character's full earned-achievement list
    Achievements {
        /// 1-12 letters/digits/underscore
        name: String,
    },
    /// Server statistics — 19 queries, 18 of them run concurrently
    Stats,
    /// Wowhead tooltip/icon info for one or more item entries
    ItemInfo {
        /// Comma-separated item entry ids, e.g. `25,116,6948` (max 25 —
        /// enforced in `run.rs`, not here; see [`parse_item_ids`])
        #[arg(value_parser = parse_item_ids)]
        ids: ItemIds,
    },

    // -- Task 13: SOAP write actions (`dml_wow::{soap_cmds,party}`) ---------
    //
    // Every numeric/name/flag argument below is DELIBERATELY typed loosely
    // enough to reach its `dml-wow` builder, because the builders are the
    // single source of the validation rules (and their exact BAD_ARG wording).
    // A clap-level allowlist here would be a second copy that could drift —
    // the opposite of what `console-tail --lines` does, and for a reason: that
    // one is a plain numeric RANGE with no library-side check behind it,
    // whereas these all have one. See `run.rs::dispatch` for the guards that
    // must run BEFORE any SOAP/DB call.
    /// Run one raw worldserver console command over SOAP
    Console {
        /// The command — every remaining token, joined with single spaces
        /// (e.g. `dml-wow console server info`). `allow_hyphen_values`: this
        /// is a RAW PASSTHROUGH arm, so a token starting with `-` (e.g. the
        /// `-1` in `account set gmlevel bob 3 -1`, which is literally what
        /// `account_set_gm_cmd` itself emits) must reach the worldserver, not
        /// be rejected by clap as an unknown flag — both siblings (the
        /// launcher's single IPC string, bash's `--command "..."`) accept it.
        #[arg(required = true, num_args = 1.., allow_hyphen_values = true)]
        command: Vec<String>,
    },
    /// Game-account administration over SOAP (create/password/GM level/delete)
    Account {
        #[command(subcommand)]
        cmd: AccountCmd,
    },
    /// GM actions against one character
    Gm {
        #[command(subcommand)]
        cmd: GmCmd,
    },
    /// Send in-game mail with item attachments
    MailItem {
        /// Recipient character name (1-12 letters/digits/underscore)
        to: String,
        /// One or more `itemid:count` specs. Separate tokens and/or
        /// comma-separated lists both work — they are joined and then split
        /// with the SAME `split_mail_items` rule bash uses on `--items`.
        #[arg(required = true, num_args = 1..)]
        items: Vec<String>,
        /// Mail subject (quotes and line breaks are stripped by the builder)
        #[arg(long, default_value = "Dad's MMO Lab")]
        subject: String,
        /// Mail body (same sanitization as `--subject`)
        #[arg(long, alias = "text", default_value = "Enjoy!")]
        body: String,
    },
    /// Teleport a character to a named `game_tele` location
    Teleport {
        /// Character name (1-12 letters/digits/underscore)
        char_name: String,
        /// Destination token as listed by `teleport-list`
        to: String,
    },
    /// Set the message of the day — applies live, no restart
    Motd {
        /// The new MOTD text. `allow_hyphen_values`: a MOTD is free text and
        /// may legitimately start with `-` (e.g. "-50% XP weekend") — same
        /// reasoning as `Console::command` above.
        #[arg(allow_hyphen_values = true)]
        text: String,
    },
    /// Playerbot party management
    Party {
        #[command(subcommand)]
        cmd: PartyCmd,
    },

    // -- Task 14: lifecycle, module mutations, backup/restore, destructive --
    //
    // Same loose-typing doctrine as Task 13's block above: a value whose rule
    // already lives in `dml-wow` (a module family, a `--level`, a module key)
    // arrives here unfiltered so the library owns the wording of its own
    // rejection. What clap DOES enforce is argv SHAPE — a flag that must be
    // present at all (`module remove --key`), same class of check as Task 12's
    // `items-search --name`.
    //
    // `--yes` is the exception, and the reason this block exists as its own
    // section: four of these subcommands are IRREVERSIBLE, and their gate is
    // the CLI's own (neither sibling has one — the launcher's gate is a
    // typed-id/two-click confirm UI, and its native commands hardcode
    // `confirm = true`). See `run.rs::confirm_refusal`.
    /// Start the server — brings the Docker Desktop engine up first.
    /// STREAMS NDJSON events.
    Start {
        /// Title id. Defaults to this CLI's own title, the one every other
        /// subcommand resolves implicitly.
        #[arg(long, default_value = dml_wow::config::TITLE)]
        id: String,
    },
    /// Stop the server — STREAMS NDJSON events. Also stops Docker Desktop
    /// unless `--no-stop-engine` is given (the launcher's default-ON
    /// "Stop Docker Desktop when the server stops" preference).
    Stop {
        #[arg(long, default_value = dml_wow::config::TITLE)]
        id: String,
        /// Stop the Docker Desktop engine afterwards (the default).
        #[arg(long)]
        stop_engine: bool,
        /// Leave the Docker Desktop engine running afterwards.
        #[arg(long, conflicts_with = "stop_engine")]
        no_stop_engine: bool,
    },
    /// Restart the server — STREAMS NDJSON events. No engine wrapping (a
    /// restart assumes the engine is already up, matching the launcher).
    Restart {
        #[arg(long, default_value = dml_wow::config::TITLE)]
        id: String,
        // Why it is a no-op here: dml_wow::lifecycle::SKIP_SAVEALL_NOTE.
        /// "Faster restart": skip the pre-stop saveall. A no-op on the native
        /// compose path.
        #[arg(long)]
        no_saveall: bool,
    },
    /// Database backups: create / list / validate / delete / restore
    Backup {
        #[command(subcommand)]
        cmd: BackupCmd,
    },
    /// Reclaim Docker disk space (build cache, dangling images, stale
    /// volumes). DESTRUCTIVE — requires `--yes`. STREAMS NDJSON events.
    DockerClean {
        // NOT range-gated here: destructive::docker_clean_stream owns that
        // check and its exact BAD_ARG wording — unlike `console-tail --lines`,
        // which has no library-side check behind it.
        /// How much to reclaim: 1, 2 or 3
        #[arg(long)]
        level: u8,
        #[arg(long)]
        yes: bool,
    },
    /// Delete every random playerbot and rebuild the population from the
    /// current settings. DESTRUCTIVE — requires `--yes` AND `--ack flush`.
    /// STREAMS NDJSON events.
    BotsFlush {
        /// The typed acknowledgement. Must be exactly `flush`; defaults to
        /// empty so an omitted one is a CONFIRM_REQUIRED domain refusal
        /// (exit 1), not a clap usage error (exit 2) — bash's own `btack=""`.
        #[arg(long, default_value = "")]
        ack: String,
        #[arg(long)]
        yes: bool,
    },
    /// Uninstall a title: its containers, its directory and its launcher.
    /// DESTRUCTIVE — requires `--yes`. STREAMS NDJSON events.
    GamesRemove {
        /// Title id — REQUIRED and never defaulted, unlike `start`/`stop`/
        /// `restart`: typing the id out is itself part of the confirmation,
        /// mirroring the launcher's typed-id UI gate.
        id: String,
        /// Keep the ~6 GB downloaded client-data volume for a faster reinstall.
        #[arg(long)]
        keep_data: bool,
        /// Also delete the AzerothCore/MySQL images (~3-5 GB).
        #[arg(long)]
        remove_images: bool,
        #[arg(long)]
        yes: bool,
    },
    /// Update AzerothCore + mod-playerbots from git — STREAMS NDJSON events.
    SelfUpdate {
        #[command(flatten)]
        backup: BackupChoice,
    },

    // -- Task 15: misc reads + the interactive `install` passthrough --------
    //
    // `action` stays a raw String, deliberately NOT a clap subcommand/
    // allowlist -- same loose-typing doctrine as `account set-gm`'s `level`
    // (Task 13): `dml_wow::lan::validate_lan_request` owns the closed
    // LAN_ACTIONS allowlist and the IP-vs-hostname shape check (Controller
    // ruling D1 -- `lan_action` itself `.expect()`s/`unreachable!()`s on
    // unvalidated input, see `run.rs::dispatch`'s `Lan` arm and the task
    // report's D1 section), so an unknown action must reach it as a domain
    // BAD_ARG/exit-1, never a clap usage-error/exit-2.
    /// LAN address control for this CLI's fixed AC title (on/off/status/
    /// refresh) — native-mode `dml lan`.
    Lan {
        /// on | off | status | refresh
        action: String,
        /// Required for on/refresh; ignored (and may be omitted) for
        /// off/status.
        ip: Option<String>,
        /// Internet-play addressing — only honored when action == on
        /// (`validate_lan_request`'s own narrowing).
        #[arg(long)]
        internet: bool,
        /// This host's LAN address, written to realmlist.localAddress so
        /// players inside the house are not routed out to the public
        /// address (which only works if the router hairpins NAT). Private
        /// IPv4 only; `off` always reverts it to 127.0.0.1.
        #[arg(long)]
        local: Option<String>,
    },
    /// Wowhead item-info cache size/wipe
    Cache {
        #[command(subcommand)]
        cmd: CacheCmd,
    },
    /// The saved WoW client folder
    ClientPath {
        #[command(subcommand)]
        cmd: ClientPathCmd,
    },
    /// Account-wide sharing flags
    Accountwide {
        #[command(subcommand)]
        cmd: AccountwideCmd,
    },
    /// The in-game `.` commands cheat sheet
    Commands,
    /// Interactively install a title — STDIO PASSTHROUGH, not NDJSON: the
    /// installer is an interactive bash script and would starve on a
    /// captured pipe. Prints NO envelope on success (the installer's own
    /// raw output IS the output); an `INSTALL_PREREQS` envelope only when Git
    /// Bash or the dml script can't be found, before anything is spawned.
    Install {
        /// Title id to install. Defaults to this CLI's own title.
        /// Validated with the launcher's own `[A-Za-z0-9._-]+` rule
        /// (`run.rs::valid_game_id`) — a CLI-specific hardening addition:
        /// `games_install` itself (`launcher/src-tauri/src/lib.rs`) carries
        /// NO id guard of its own (only a "busy" check, not applicable to a
        /// one-shot process) — see the task report's `install` section.
        #[arg(default_value = dml_wow::config::TITLE)]
        id: String,
    },
    /// Install a WoW server NATIVELY on Docker Desktop, with no WSL distro
    /// anywhere — STREAMS NDJSON events, and is RESUMABLE: run it again after
    /// a cancel, a crash or a reboot and it continues from the first stage
    /// that did not finish (Docker's build cache does the rest).
    ///
    /// Distinct from `install`, which stays exactly as it was: the
    /// interactive WSL/Linux route, stdio passthrough, no envelope. This one
    /// is native-only BY DESIGN and has no bash mirror — bash's
    /// `_installers_supported()` deliberately refuses on Windows.
    ///
    /// `--id` rather than `install`'s positional argument: this command is
    /// what `start`/`stop`/`restart` follow on from, and they all name their
    /// title with `--id`.
    InstallNative {
        /// Title id — ALSO the directory name created under `DML_GAMES_DIR`,
        /// which is why it is validated (`run.rs::valid_game_id`) before it is
        /// joined onto anything. The engine re-checks it with a stricter rule
        /// that additionally refuses `.` and `..`.
        #[arg(long, default_value = dml_wow::config::TITLE)]
        id: String,
        /// Install even though this PC is below the recommended hardware
        /// floor. Only ever downgrades the RAM/disk refusals to warnings that
        /// still carry their numbers; it can never clear the Docker or git
        /// refusals, because no amount of insisting makes up for those.
        #[arg(long)]
        allow_underspec: bool,
    },
    /// Import a WSL export into a native Docker Desktop stack.
    ///
    /// The other half of the migration: `export-from-wsl.sh` runs INSIDE the
    /// distro and writes a payload into the target folder; this turns that
    /// payload into a running native stack. Native-only, no bash mirror, same
    /// reason as `install-native` -- this exists for the PC that has no distro.
    ///
    /// Restoring the dump writes character data, so it REFUSES a target that
    /// already holds characters, and refuses equally when it could not find
    /// out. There is no `--replace`: overwriting a server that is already
    /// there is `wow backup restore`'s job, and that one takes a safety dump.
    MigrateImport {
        /// Title id -- ALSO the folder under `DML_GAMES_DIR` the export must
        /// already be sitting in, because the folder NAME is the id every
        /// other command looks the server up by.
        #[arg(long, default_value = dml_wow::migrate::DEFAULT_TITLE_ID)]
        id: String,
    },
    /// What a migration import would do, or got through: is the export
    /// complete, what is missing, which stage a resume starts from.
    MigrateStatus {
        #[arg(long, default_value = dml_wow::migrate::DEFAULT_TITLE_ID)]
        id: String,
    },
    /// The Wrath Unbound multi-class add-on: layer it onto an EXISTING native
    /// WotLK Playerbots server, or take it back off. Both directions rebuild
    /// the worldserver (30-90 minutes, `pct` events during the compile) and
    /// both are RESUMABLE. Native-only, like install-native — the WSL route
    /// keeps the interactive `dml unbound` script.
    Unbound {
        #[command(subcommand)]
        cmd: UnboundCmd,
    },
}

/// `dml-wow unbound …` — `dml_wow::unbound`.
#[derive(Subcommand, Debug)]
pub enum UnboundCmd {
    /// Install (or resume installing) the add-on. STREAMS NDJSON.
    ///
    /// Refuses without `--accept-data-changes`, because the migrations
    /// delete and re-seed rows other mods may also touch: it is consent,
    /// collected by the caller, verified here.
    Install {
        #[arg(long, default_value = dml_wow::unbound::DEFAULT_TITLE_ID)]
        id: String,
        /// Accept the add-on's data changes (playercreateinfo_spell_custom
        /// re-seeded for 9 class masks, skillraceclassinfo_dbc ID >= 10000
        /// replaced, catalog rows replaced).
        #[arg(long)]
        accept_data_changes: bool,
        /// Re-run every stage over an install this engine believes is
        /// already complete (takes a fresh safety backup first).
        #[arg(long)]
        repair: bool,
    },
    /// Uninstall (or resume uninstalling) the add-on. STREAMS NDJSON; the
    /// done event carries a `residue` array naming what could not be
    /// reverted — this command never pretends to be a perfect inverse.
    Uninstall {
        #[arg(long, default_value = dml_wow::unbound::DEFAULT_TITLE_ID)]
        id: String,
        /// Accept the destructive half: unbound_character_unlocks (character
        /// class-unlock progression) is DROPPED, and the add-on's own wide
        /// revert deletes run.
        #[arg(long)]
        accept_data_changes: bool,
        /// Run the revert even when no trace of an install is found. Safe —
        /// every statement is IF-EXISTS-shaped — but it is a decision, so it
        /// is a flag.
        #[arg(long)]
        force: bool,
    },
    /// Install/uninstall progress and on-disk evidence, from the state file
    /// and the six patched files. No docker, no DB — works while stopped.
    Status {
        #[arg(long, default_value = dml_wow::unbound::DEFAULT_TITLE_ID)]
        id: String,
    },
    /// The CLIENT addons (talent UI, spellbook, shared resources).
    ///
    /// `unbound install` already puts these into your own client
    /// automatically. These arms exist for the other two cases: repairing your
    /// client without a 90-minute rebuild, and producing a copy to hand to
    /// other players.
    Addons {
        #[command(subcommand)]
        cmd: AddonsCmd,
    },
}

/// `dml-wow unbound addons …` — `dml_wow::unbound_addons`.
#[derive(Subcommand, Debug)]
pub enum AddonsCmd {
    /// Install into your own WoW client's Interface/AddOns.
    Install {
        /// Client folder. Defaults to the one saved by `client-path set`.
        #[arg(long)]
        client: Option<String>,
    },
    /// Write the addon folders somewhere you can zip and share.
    ///
    /// A folder rather than a .zip: re-zipping would be a second artifact that
    /// can drift from the embedded bytes, and a folder is what the recipient
    /// needs anyway.
    Export {
        /// Destination directory (created if missing).
        dir: String,
    },
}

/// `dml-wow cache …` — `dml_wow::cachestatus`.
#[derive(Subcommand, Debug)]
pub enum CacheCmd {
    /// Wowhead item-info cache size
    Status,
    /// Wipe the wowhead item-info cache
    Clean,
}

/// `dml-wow client-path …` — `dml_wow::clientpath`. `set` is the only writer;
/// its `dir` is unvalidated here (loose-typing doctrine again) — the library
/// owns the "no such folder" / "doesn't look like a WoW client" wording.
#[derive(Subcommand, Debug)]
pub enum ClientPathCmd {
    /// The saved client folder, if any
    Get,
    /// Save a new client folder
    Set { dir: String },
    /// Scan common install locations for a WoW client
    Detect,
}

/// `dml-wow accountwide …` — `dml_wow::accountwide`.
#[derive(Subcommand, Debug)]
pub enum AccountwideCmd {
    /// Installed state + every subsystem's on/off value
    Get,
    /// Flip one flag (or the reputation pick-one, with an optional
    /// `--variant`)
    Set {
        /// e.g. ENABLE_ACCOUNTWIDE_MOUNTS
        key: String,
        /// on | off
        value: String,
        #[arg(long)]
        variant: Option<String>,
    },
}

/// The `--backup` / `--no-backup` pair, flattened into every subcommand whose
/// `dml-wow` orchestration takes a `backup: Option<bool>`.
///
/// `None` (NEITHER flag) is a REAL, load-bearing third state, not a missing
/// value to default away: `modmgr::module_rebuild_stream` and
/// `modmgr::install_lua`/`install_sql` answer it with their own
/// `BAD_ARG "Pick --backup or --no-backup"` (module SQL lands during the
/// operation, so the choice must be explicit), while `install_cpp`/`remove_cpp`/
/// `remove_lua` answer the OPPOSITE way — they reject a backup flag that IS
/// present ("cpp installs don't take backup flags"). Both rules are the
/// library's; collapsing `None` into a default here would silently break both.
#[derive(clap::Args, Debug, Clone, Default)]
pub struct BackupChoice {
    /// Take a safety database backup first
    #[arg(long)]
    pub backup: bool,
    /// Skip the safety database backup
    #[arg(long = "no-backup", conflicts_with = "backup")]
    pub no_backup: bool,
}

impl BackupChoice {
    /// `Some(true)`/`Some(false)` when exactly one flag was given, `None` when
    /// neither was — see the struct's doc comment for why `None` matters.
    /// (`--backup --no-backup` together is a clap conflict, so the two can
    /// never both be set.)
    pub fn choice(&self) -> Option<bool> {
        match (self.backup, self.no_backup) {
            (true, _) => Some(true),
            (_, true) => Some(false),
            _ => None,
        }
    }
}

/// `dml-wow backup …` — the launcher's native backup family
/// (`wow_backup_{create,list,validate,delete,restore}_native`).
#[derive(Subcommand, Debug)]
pub enum BackupCmd {
    /// Take a new gzipped mysqldump — STREAMS NDJSON events
    Create {
        /// Also dump `acore_world` (much larger, and only needed if you have
        /// edited world data by hand)
        #[arg(long)]
        include_world: bool,
        // Sanitized/bounded by dml_wow::backup::sanitize_backup_name.
        /// Display name for the Backups page; empty or absent gets an auto name
        #[arg(long)]
        name: Option<String>,
    },
    /// Every backup in `~/.dml/backups`, newest first
    List,
    /// Check one backup's gzip integrity and SQL markers
    Validate {
        /// File name as reported by `backup list`
        file: String,
    },
    /// Delete one backup file (and its sidecar)
    Delete {
        /// File name as reported by `backup list`
        file: String,
    },
    /// Overwrite the live databases from a backup. DESTRUCTIVE — requires
    /// `--yes`. STREAMS NDJSON events.
    Restore {
        /// File name as reported by `backup list`
        file: String,
        #[arg(long)]
        yes: bool,
    },
}

/// `dml-wow account …` — the four SOAP account actions
/// (`soap_cmds::account_*_cmd`). Mirrors bash's `dml wow account <sub>` arms
/// with the flags turned into positionals, matching this CLI's own house style
/// (Task 12's `paperdoll <NAME>` for bash's `--char`).
#[derive(Subcommand, Debug)]
pub enum AccountCmd {
    /// Create an account (3-20 char user, 4-16 char password)
    Create { user: String, pass: String },
    /// Change an account's password
    SetPassword { user: String, pass: String },
    /// Set an account's GM level
    SetGm {
        user: String,
        /// 0-3. A STRING, not a number: `account_set_gm_cmd` matches the
        /// literal `0|1|2|3` exactly like bash's regex does, so an
        /// out-of-range value has to arrive intact to earn its BAD_ARG
        /// ("--level must be 0-3") instead of dying as a clap parse error.
        level: String,
    },
    /// Delete an account (refuses `admin`, the launcher's own SOAP account)
    Delete { user: String },
}

/// `dml-wow gm …` — the six GM builders. `level`/`at-login` are stock AC
/// console commands and work on OFFLINE characters; `gold`/`heal`/`revive`/
/// `summon` go through the DML server bridges and REQUIRE the character
/// online (checked in `run.rs` before the fire, same order as the oracle).
#[derive(Subcommand, Debug)]
pub enum GmCmd {
    /// Set a character's level (1-255; works offline)
    #[command(allow_negative_numbers = true)]
    Level { player: String, level: i32 },
    /// Give whole gold (0-214748; character must be online)
    #[command(allow_negative_numbers = true)]
    Gold { player: String, gold: i32 },
    /// Heal to full (character must be online)
    Heal { player: String },
    /// Revive (character must be online)
    Revive { player: String },
    /// Summon a creature next to the character (must be online)
    #[command(allow_negative_numbers = true)]
    Summon { player: String, entry: i32 },
    /// Set an at-next-login flag: rename | customize | changerace | changefaction
    AtLogin { player: String, flag: String },
}

/// `dml-wow party …` — the playerbot party arms this task ports. NOT here
/// (still launcher-only, see the task report): `dismiss-all`, `preset-show`,
/// `preset-import`.
#[derive(Subcommand, Debug)]
pub enum PartyCmd {
    /// Spawn a bot of `class` and invite it to `player`'s party
    Add {
        player: String,
        /// warrior|paladin|hunter|rogue|priest|shaman|mage|warlock|druid
        class: String,
        #[arg(long)]
        gender: Option<String>,
        /// A premade spec name, e.g. `frost pve`
        #[arg(long)]
        spec: Option<String>,
    },
    /// Kick one bot from the party and send it to log out
    Kick { player: String, bot: String },
    /// Log a bot back in and re-invite it
    Relogin { player: String, bot: String },
    /// Whisper one bot a maintenance action: gear | talents | maintain | spec
    Botcmd {
        player: String,
        bot: String,
        action: String,
        /// Required when `action` is `spec`
        #[arg(long)]
        spec: Option<String>,
    },
    /// Save the caller's current bot party as a named class-list preset
    PresetSave { player: String, name: String },
    /// List saved presets
    PresetList,
    /// Delete a saved preset
    PresetDelete { name: String },
    /// Replace the current party with a preset — STREAMS NDJSON events
    PresetLoad { player: String, name: String },
}

/// The parsed form of `item-info`'s positional argument. A thin newtype
/// rather than a bare `Vec<u32>` field: clap's derive treats a `Vec<T>`
/// field as MULTIPLE occurrences of a scalar `T` (one `u32` per argv token),
/// not one token parsed into a collection — wrapping opts back into "one
/// token, custom parser", the same way `console-tail`'s `--lines` stays a
/// plain `u32` field with a `.range()`-adapted parser rather than a `Vec`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemIds(pub Vec<u32>);

/// `dml-wow item-info`'s positional argument, `<ID>[,<ID>...]` — FORMAT only
/// (`^[0-9]+(,[0-9]+)*$`-equivalent): each comma-separated segment must be a
/// bare non-negative integer, mirroring the bash CLI's own gate on
/// `--entries` (`90-main.sh`'s `item-info` arm). A malformed token really is
/// an argv-shape problem, so it stays a clap usage error (exit 2) — same
/// idiom as `ConsoleTail`'s `--lines` range gate above.
///
/// Deliberately NOT the 25-id cap (review finding 2): a cardinality cap is
/// the same class of check as `bots --class`'s allowlist or
/// `items-search --name`'s non-empty rule, both of which this same file
/// routes through `run.rs` as a `BAD_ARG`/exit-1 domain rejection rather
/// than a clap usage error — the launcher's `wow_item_info_read` and the
/// bash arm agree (`BAD_ARG`/exit-1-equivalent for both). Enforcing the cap
/// here would have made `item-info` the only cardinality check in this file
/// that disagreed with itself as well as with both siblings. See
/// `run.rs::dispatch`'s `Cmd::ItemInfo` arm for the cap check itself.
fn parse_item_ids(raw: &str) -> Result<ItemIds, String> {
    let mut ids = Vec::new();
    for part in raw.split(',') {
        let id: u32 = part.parse().map_err(|_| {
            format!("not a valid item id: {part:?} (want comma-separated ids, e.g. 25,116,6948)")
        })?;
        ids.push(id);
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
    // SECURITY: the read-only allowlist lives on dml_wow::config::raw_write.
    /// Overwrite one editable file with the body read from STDIN
    /// (bash: `config raw-write`). `.env` and `docker-compose.override.yml`
    /// are read-only here and are rejected.
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

/// `dml-wow module …` — bash's `module list` / `module catalog` (Task 11) plus
/// the five MUTATING arms (Task 14).
///
/// The mutating arms keep bash's FLAG spelling (`--family`/`--key`/`--url`/
/// `--variant`/`--db`/`--mode`/`--files`) rather than this CLI's usual
/// positional house style, and deliberately: the library's own rejections name
/// those flags in their message/hint text ("--url and --key are mutually
/// exclusive", "Invalid --db: x", "Usage: dml wow module update --key
/// mod-<name>"). Positionals would have made every one of those hints a lie.
#[derive(Subcommand, Debug)]
pub enum ModuleCmd {
    /// Every module with its live install/deploy/rebuild state
    List,
    /// The static catalog only — no state read, no files touched
    Catalog,
    /// Install (or pull) one module — STREAMS NDJSON events
    Install {
        // Unvalidated here: modmgr::module_install_stream owns the allowlist
        // and its "Unknown family: x" wording.
        /// Module family: cpp | lua | sql
        #[arg(long)]
        family: String,
        // The mutual exclusion is enforced by modmgr::install_cpp.
        /// Registry key, e.g. `mod-transmog` (mutually exclusive with `--url`)
        #[arg(long)]
        key: Option<String>,
        /// Custom C++ module git URL; the key is derived from it
        #[arg(long)]
        url: Option<String>,
        /// Family-specific variant (e.g. a hearthstone/teleporter flavour)
        #[arg(long)]
        variant: Option<String>,
        #[command(flatten)]
        backup: BackupChoice,
    },
    /// Remove one module — STREAMS NDJSON events
    Remove {
        #[arg(long)]
        family: String,
        #[arg(long)]
        key: String,
        #[command(flatten)]
        backup: BackupChoice,
    },
    /// `git pull` one installed C++ module — STREAMS NDJSON events
    Update {
        #[arg(long)]
        key: String,
    },
    /// Recompile the server with the pending module changes (30-90 min on a
    /// cold build) — STREAMS NDJSON events
    Rebuild {
        #[command(flatten)]
        backup: BackupChoice,
    },
    /// Mark/clear a module's rows in a database's `updates` tracking table
    Repair {
        #[arg(long)]
        key: String,
        /// world | characters | auth
        #[arg(long)]
        db: String,
        /// mark | clear
        #[arg(long)]
        mode: String,
        /// Space-separated `.sql` file names; omitted = discover them
        #[arg(long)]
        files: Option<String>,
    },
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

    // -- Task 13: SOAP actions, accounts, GM, mail/teleport/motd, party -----

    #[test]
    fn console_joins_every_remaining_token() {
        match Cli::try_parse_from(["dml-wow", "console", "server", "info"]).unwrap().command {
            Cmd::Console { command } => assert_eq!(command, vec!["server", "info"]),
            other => panic!("expected Console, got {other:?}"),
        }
    }

    #[test]
    fn console_requires_at_least_one_token() {
        assert!(Cli::try_parse_from(["dml-wow", "console"]).is_err());
    }

    /// A leading-hyphen token (e.g. the `-1` `account_set_gm_cmd` itself
    /// emits) must reach `command`, not be mistaken for an unknown flag —
    /// review Fix 1: `console` is a raw passthrough arm and both siblings
    /// accept this shape.
    #[test]
    fn console_accepts_leading_hyphen_tokens() {
        match Cli::try_parse_from(["dml-wow", "console", "account", "set", "gmlevel", "bob", "3", "-1"])
            .unwrap()
            .command
        {
            Cmd::Console { command } => {
                assert_eq!(command, vec!["account", "set", "gmlevel", "bob", "3", "-1"])
            }
            other => panic!("expected Console, got {other:?}"),
        }
    }

    #[test]
    fn parses_account_arms() {
        match Cli::try_parse_from(["dml-wow", "account", "create", "bob", "pw12"]).unwrap().command {
            Cmd::Account { cmd: AccountCmd::Create { user, pass } } => {
                assert_eq!(user, "bob");
                assert_eq!(pass, "pw12");
            }
            other => panic!("expected Account Create, got {other:?}"),
        }
        match Cli::try_parse_from(["dml-wow", "account", "set-password", "bob", "pw12"])
            .unwrap()
            .command
        {
            Cmd::Account { cmd: AccountCmd::SetPassword { user, .. } } => assert_eq!(user, "bob"),
            other => panic!("expected Account SetPassword, got {other:?}"),
        }
        match Cli::try_parse_from(["dml-wow", "account", "set-gm", "bob", "3"]).unwrap().command {
            Cmd::Account { cmd: AccountCmd::SetGm { user, level } } => {
                assert_eq!(user, "bob");
                // A STRING, not a number: `account_set_gm_cmd` matches the
                // literal "0".."3" exactly like bash's regex, so "9999" must
                // reach it as a domain BAD_ARG rather than dying as a clap
                // integer-parse usage error.
                assert_eq!(level, "3");
            }
            other => panic!("expected Account SetGm, got {other:?}"),
        }
        assert_eq!(
            match Cli::try_parse_from(["dml-wow", "account", "set-gm", "bob", "9999"])
                .unwrap()
                .command
            {
                Cmd::Account { cmd: AccountCmd::SetGm { level, .. } } => level,
                other => panic!("expected Account SetGm, got {other:?}"),
            },
            "9999"
        );
        match Cli::try_parse_from(["dml-wow", "account", "delete", "bob"]).unwrap().command {
            Cmd::Account { cmd: AccountCmd::Delete { user } } => assert_eq!(user, "bob"),
            other => panic!("expected Account Delete, got {other:?}"),
        }
    }

    #[test]
    fn account_arms_require_their_positionals() {
        for argv in [
            vec!["dml-wow", "account"],
            vec!["dml-wow", "account", "create"],
            vec!["dml-wow", "account", "create", "bob"],
            vec!["dml-wow", "account", "set-password", "bob"],
            vec!["dml-wow", "account", "set-gm", "bob"],
            vec!["dml-wow", "account", "delete"],
            vec!["dml-wow", "account", "nope", "bob"],
        ] {
            assert!(Cli::try_parse_from(&argv).is_err(), "{argv:?} should be a usage error");
        }
    }

    #[test]
    fn parses_gm_arms() {
        match Cli::try_parse_from(["dml-wow", "gm", "level", "Testen", "80"]).unwrap().command {
            Cmd::Gm { cmd: GmCmd::Level { player, level } } => {
                assert_eq!(player, "Testen");
                assert_eq!(level, 80);
            }
            other => panic!("expected Gm Level, got {other:?}"),
        }
        match Cli::try_parse_from(["dml-wow", "gm", "gold", "Testen", "5"]).unwrap().command {
            Cmd::Gm { cmd: GmCmd::Gold { player, gold } } => {
                assert_eq!(player, "Testen");
                assert_eq!(gold, 5);
            }
            other => panic!("expected Gm Gold, got {other:?}"),
        }
        assert!(matches!(
            Cli::try_parse_from(["dml-wow", "gm", "heal", "Testen"]).unwrap().command,
            Cmd::Gm { cmd: GmCmd::Heal { .. } }
        ));
        assert!(matches!(
            Cli::try_parse_from(["dml-wow", "gm", "revive", "Testen"]).unwrap().command,
            Cmd::Gm { cmd: GmCmd::Revive { .. } }
        ));
        match Cli::try_parse_from(["dml-wow", "gm", "summon", "Testen", "190"]).unwrap().command {
            Cmd::Gm { cmd: GmCmd::Summon { entry, .. } } => assert_eq!(entry, 190),
            other => panic!("expected Gm Summon, got {other:?}"),
        }
        match Cli::try_parse_from(["dml-wow", "gm", "at-login", "Testen", "rename"]).unwrap().command
        {
            Cmd::Gm { cmd: GmCmd::AtLogin { flag, .. } } => assert_eq!(flag, "rename"),
            other => panic!("expected Gm AtLogin, got {other:?}"),
        }
    }

    /// A NEGATIVE number must reach the builder (which answers BAD_ARG/exit 1
    /// with the value in the message) rather than being swallowed by clap as
    /// an unknown flag — hence `allow_negative_numbers` on these three arms.
    #[test]
    fn gm_numeric_arms_accept_negative_values_for_the_builder_to_reject() {
        match Cli::try_parse_from(["dml-wow", "gm", "level", "Testen", "-5"]).unwrap().command {
            Cmd::Gm { cmd: GmCmd::Level { level, .. } } => assert_eq!(level, -5),
            other => panic!("expected Gm Level, got {other:?}"),
        }
        match Cli::try_parse_from(["dml-wow", "gm", "gold", "Testen", "-1"]).unwrap().command {
            Cmd::Gm { cmd: GmCmd::Gold { gold, .. } } => assert_eq!(gold, -1),
            other => panic!("expected Gm Gold, got {other:?}"),
        }
        match Cli::try_parse_from(["dml-wow", "gm", "summon", "Testen", "-3"]).unwrap().command {
            Cmd::Gm { cmd: GmCmd::Summon { entry, .. } } => assert_eq!(entry, -3),
            other => panic!("expected Gm Summon, got {other:?}"),
        }
    }

    #[test]
    fn gm_arms_require_their_positionals_and_reject_non_numbers() {
        for argv in [
            vec!["dml-wow", "gm"],
            vec!["dml-wow", "gm", "level", "Testen"],
            vec!["dml-wow", "gm", "gold", "Testen"],
            vec!["dml-wow", "gm", "heal"],
            vec!["dml-wow", "gm", "summon", "Testen"],
            vec!["dml-wow", "gm", "at-login", "Testen"],
            vec!["dml-wow", "gm", "level", "Testen", "abc"],
            vec!["dml-wow", "gm", "return-home", "Testen"], // not ported yet
        ] {
            assert!(Cli::try_parse_from(&argv).is_err(), "{argv:?} should be a usage error");
        }
    }

    #[test]
    fn parses_mail_item_with_defaults_and_flags() {
        match Cli::try_parse_from(["dml-wow", "mail-item", "Testen", "6948:1"]).unwrap().command {
            Cmd::MailItem { to, items, subject, body } => {
                assert_eq!(to, "Testen");
                assert_eq!(items, vec!["6948:1"]);
                assert_eq!(subject, "Dad's MMO Lab");
                assert_eq!(body, "Enjoy!");
            }
            other => panic!("expected MailItem, got {other:?}"),
        }
        // Several specs as separate tokens...
        match Cli::try_parse_from([
            "dml-wow", "mail-item", "Testen", "6948:1", "2589:5", "--subject", "hi", "--body", "yo",
        ])
        .unwrap()
        .command
        {
            Cmd::MailItem { items, subject, body, .. } => {
                assert_eq!(items, vec!["6948:1", "2589:5"]);
                assert_eq!(subject, "hi");
                assert_eq!(body, "yo");
            }
            other => panic!("expected MailItem, got {other:?}"),
        }
        // ...and `--text` is accepted as an alias of `--body` (the brief's
        // spelling; `--body` is the bash/launcher one).
        match Cli::try_parse_from(["dml-wow", "mail-item", "Testen", "1:1", "--text", "yo"])
            .unwrap()
            .command
        {
            Cmd::MailItem { body, .. } => assert_eq!(body, "yo"),
            other => panic!("expected MailItem, got {other:?}"),
        }
    }

    #[test]
    fn mail_item_requires_recipient_and_at_least_one_spec() {
        assert!(Cli::try_parse_from(["dml-wow", "mail-item"]).is_err());
        assert!(Cli::try_parse_from(["dml-wow", "mail-item", "Testen"]).is_err());
    }

    #[test]
    fn parses_teleport_and_motd() {
        match Cli::try_parse_from(["dml-wow", "teleport", "Testen", "Orgrimmar"]).unwrap().command {
            Cmd::Teleport { char_name, to } => {
                assert_eq!(char_name, "Testen");
                assert_eq!(to, "Orgrimmar");
            }
            other => panic!("expected Teleport, got {other:?}"),
        }
        match Cli::try_parse_from(["dml-wow", "motd", "Welcome to the Lab!"]).unwrap().command {
            Cmd::Motd { text } => assert_eq!(text, "Welcome to the Lab!"),
            other => panic!("expected Motd, got {other:?}"),
        }
        assert!(Cli::try_parse_from(["dml-wow", "teleport", "Testen"]).is_err());
        assert!(Cli::try_parse_from(["dml-wow", "motd"]).is_err());
        // `teleport` and the Task 12 `teleport-list` reader stay distinct.
        assert!(matches!(
            Cli::try_parse_from(["dml-wow", "teleport-list"]).unwrap().command,
            Cmd::TeleportList { .. }
        ));
    }

    /// A MOTD may legitimately start with `-` (e.g. a percent-off callout) —
    /// review Fix 1, same reasoning as `console_accepts_leading_hyphen_tokens`.
    #[test]
    fn motd_accepts_leading_hyphen_text() {
        match Cli::try_parse_from(["dml-wow", "motd", "-50% XP weekend"]).unwrap().command {
            Cmd::Motd { text } => assert_eq!(text, "-50% XP weekend"),
            other => panic!("expected Motd, got {other:?}"),
        }
    }

    #[test]
    fn parses_party_arms() {
        match Cli::try_parse_from(["dml-wow", "party", "add", "Testen", "priest"]).unwrap().command {
            Cmd::Party { cmd: PartyCmd::Add { player, class, gender, spec } } => {
                assert_eq!(player, "Testen");
                assert_eq!(class, "priest");
                assert_eq!(gender, None);
                assert_eq!(spec, None);
            }
            other => panic!("expected Party Add, got {other:?}"),
        }
        match Cli::try_parse_from([
            "dml-wow", "party", "add", "Testen", "priest", "--gender", "female", "--spec",
            "shadow pve",
        ])
        .unwrap()
        .command
        {
            Cmd::Party { cmd: PartyCmd::Add { gender, spec, .. } } => {
                assert_eq!(gender.as_deref(), Some("female"));
                assert_eq!(spec.as_deref(), Some("shadow pve"));
            }
            other => panic!("expected Party Add, got {other:?}"),
        }
        match Cli::try_parse_from(["dml-wow", "party", "kick", "Testen", "Botty"]).unwrap().command {
            Cmd::Party { cmd: PartyCmd::Kick { player, bot } } => {
                assert_eq!(player, "Testen");
                assert_eq!(bot, "Botty");
            }
            other => panic!("expected Party Kick, got {other:?}"),
        }
        assert!(matches!(
            Cli::try_parse_from(["dml-wow", "party", "relogin", "Testen", "Botty"]).unwrap().command,
            Cmd::Party { cmd: PartyCmd::Relogin { .. } }
        ));
        match Cli::try_parse_from(["dml-wow", "party", "botcmd", "Testen", "Botty", "gear"])
            .unwrap()
            .command
        {
            Cmd::Party { cmd: PartyCmd::Botcmd { action, spec, .. } } => {
                assert_eq!(action, "gear");
                assert_eq!(spec, None);
            }
            other => panic!("expected Party Botcmd, got {other:?}"),
        }
        match Cli::try_parse_from(["dml-wow", "party", "preset-save", "Testen", "raiders"])
            .unwrap()
            .command
        {
            Cmd::Party { cmd: PartyCmd::PresetSave { player, name } } => {
                assert_eq!(player, "Testen");
                assert_eq!(name, "raiders");
            }
            other => panic!("expected Party PresetSave, got {other:?}"),
        }
        assert!(matches!(
            Cli::try_parse_from(["dml-wow", "party", "preset-list"]).unwrap().command,
            Cmd::Party { cmd: PartyCmd::PresetList }
        ));
        match Cli::try_parse_from(["dml-wow", "party", "preset-delete", "raiders"]).unwrap().command
        {
            Cmd::Party { cmd: PartyCmd::PresetDelete { name } } => assert_eq!(name, "raiders"),
            other => panic!("expected Party PresetDelete, got {other:?}"),
        }
        match Cli::try_parse_from(["dml-wow", "party", "preset-load", "Testen", "raiders"])
            .unwrap()
            .command
        {
            Cmd::Party { cmd: PartyCmd::PresetLoad { player, name } } => {
                assert_eq!(player, "Testen");
                assert_eq!(name, "raiders");
            }
            other => panic!("expected Party PresetLoad, got {other:?}"),
        }
    }

    #[test]
    fn party_arms_require_their_positionals() {
        for argv in [
            vec!["dml-wow", "party"],
            vec!["dml-wow", "party", "add", "Testen"],
            vec!["dml-wow", "party", "kick", "Testen"],
            vec!["dml-wow", "party", "relogin", "Testen"],
            vec!["dml-wow", "party", "botcmd", "Testen", "Botty"],
            vec!["dml-wow", "party", "preset-save", "Testen"],
            vec!["dml-wow", "party", "preset-delete"],
            vec!["dml-wow", "party", "preset-load", "Testen"],
            vec!["dml-wow", "party", "preset-list", "extra"],
            vec!["dml-wow", "party", "nope"],
        ] {
            assert!(Cli::try_parse_from(&argv).is_err(), "{argv:?} should be a usage error");
        }
    }

    // -- Task 14: lifecycle / module mutations / backup / destructive -------

    #[test]
    fn lifecycle_arms_default_the_title_id() {
        match Cli::try_parse_from(["dml-wow", "start"]).unwrap().command {
            Cmd::Start { id } => assert_eq!(id, "wow-server-playerbots"),
            other => panic!("expected Start, got {other:?}"),
        }
        match Cli::try_parse_from(["dml-wow", "stop"]).unwrap().command {
            Cmd::Stop { id, stop_engine, no_stop_engine } => {
                assert_eq!(id, "wow-server-playerbots");
                assert!(!stop_engine);
                assert!(!no_stop_engine);
            }
            other => panic!("expected Stop, got {other:?}"),
        }
        match Cli::try_parse_from(["dml-wow", "restart"]).unwrap().command {
            Cmd::Restart { id, no_saveall } => {
                assert_eq!(id, "wow-server-playerbots");
                assert!(!no_saveall);
            }
            other => panic!("expected Restart, got {other:?}"),
        }
    }

    #[test]
    fn lifecycle_arms_take_their_flags() {
        match Cli::try_parse_from(["dml-wow", "start", "--id", "wow-tbc-server"]).unwrap().command {
            Cmd::Start { id } => assert_eq!(id, "wow-tbc-server"),
            other => panic!("expected Start, got {other:?}"),
        }
        match Cli::try_parse_from(["dml-wow", "stop", "--no-stop-engine"]).unwrap().command {
            Cmd::Stop { no_stop_engine, .. } => assert!(no_stop_engine),
            other => panic!("expected Stop, got {other:?}"),
        }
        match Cli::try_parse_from(["dml-wow", "stop", "--stop-engine"]).unwrap().command {
            Cmd::Stop { stop_engine, .. } => assert!(stop_engine),
            other => panic!("expected Stop, got {other:?}"),
        }
        // The two engine flags are mutually exclusive.
        assert!(
            Cli::try_parse_from(["dml-wow", "stop", "--stop-engine", "--no-stop-engine"]).is_err()
        );
        match Cli::try_parse_from(["dml-wow", "restart", "--no-saveall"]).unwrap().command {
            Cmd::Restart { no_saveall, .. } => assert!(no_saveall),
            other => panic!("expected Restart, got {other:?}"),
        }
    }

    /// `BackupChoice` is a THREE-state flag pair, not a defaulted bool — the
    /// library rejects both "neither given" (rebuild/lua/sql) and "one given"
    /// (cpp), so `None` has to survive the parse.
    #[test]
    fn backup_choice_is_three_state() {
        let neither = BackupChoice { backup: false, no_backup: false };
        assert_eq!(neither.choice(), None);
        assert_eq!(BackupChoice { backup: true, no_backup: false }.choice(), Some(true));
        assert_eq!(BackupChoice { backup: false, no_backup: true }.choice(), Some(false));

        match Cli::try_parse_from(["dml-wow", "module", "rebuild"]).unwrap().command {
            Cmd::Module { cmd: ModuleCmd::Rebuild { backup } } => assert_eq!(backup.choice(), None),
            other => panic!("expected Module Rebuild, got {other:?}"),
        }
        match Cli::try_parse_from(["dml-wow", "module", "rebuild", "--backup"]).unwrap().command {
            Cmd::Module { cmd: ModuleCmd::Rebuild { backup } } => {
                assert_eq!(backup.choice(), Some(true))
            }
            other => panic!("expected Module Rebuild, got {other:?}"),
        }
        match Cli::try_parse_from(["dml-wow", "module", "rebuild", "--no-backup"]).unwrap().command {
            Cmd::Module { cmd: ModuleCmd::Rebuild { backup } } => {
                assert_eq!(backup.choice(), Some(false))
            }
            other => panic!("expected Module Rebuild, got {other:?}"),
        }
        // Both at once is a usage error, so `choice()` can never see (true,true).
        assert!(
            Cli::try_parse_from(["dml-wow", "module", "rebuild", "--backup", "--no-backup"])
                .is_err()
        );
    }

    #[test]
    fn parses_module_mutating_arms() {
        match Cli::try_parse_from([
            "dml-wow", "module", "install", "--family", "cpp", "--key", "mod-transmog",
        ])
        .unwrap()
        .command
        {
            Cmd::Module { cmd: ModuleCmd::Install { family, key, url, variant, backup } } => {
                assert_eq!(family, "cpp");
                assert_eq!(key.as_deref(), Some("mod-transmog"));
                assert_eq!(url, None);
                assert_eq!(variant, None);
                assert_eq!(backup.choice(), None);
            }
            other => panic!("expected Module Install, got {other:?}"),
        }
        match Cli::try_parse_from([
            "dml-wow", "module", "install", "--family", "sql", "--key", "hearthstone", "--variant",
            "dalaran", "--backup",
        ])
        .unwrap()
        .command
        {
            Cmd::Module { cmd: ModuleCmd::Install { variant, backup, .. } } => {
                assert_eq!(variant.as_deref(), Some("dalaran"));
                assert_eq!(backup.choice(), Some(true));
            }
            other => panic!("expected Module Install, got {other:?}"),
        }
        // An UNKNOWN family parses fine -- `module_install_stream` owns the
        // allowlist, exactly like Task 13's builders own theirs.
        match Cli::try_parse_from(["dml-wow", "module", "install", "--family", "perl"])
            .unwrap()
            .command
        {
            Cmd::Module { cmd: ModuleCmd::Install { family, .. } } => assert_eq!(family, "perl"),
            other => panic!("expected Module Install, got {other:?}"),
        }
        match Cli::try_parse_from([
            "dml-wow", "module", "remove", "--family", "cpp", "--key", "mod-transmog",
        ])
        .unwrap()
        .command
        {
            Cmd::Module { cmd: ModuleCmd::Remove { family, key, .. } } => {
                assert_eq!(family, "cpp");
                assert_eq!(key, "mod-transmog");
            }
            other => panic!("expected Module Remove, got {other:?}"),
        }
        match Cli::try_parse_from(["dml-wow", "module", "update", "--key", "mod-transmog"])
            .unwrap()
            .command
        {
            Cmd::Module { cmd: ModuleCmd::Update { key } } => assert_eq!(key, "mod-transmog"),
            other => panic!("expected Module Update, got {other:?}"),
        }
        match Cli::try_parse_from([
            "dml-wow", "module", "repair", "--key", "mod-transmog", "--db", "world", "--mode",
            "mark", "--files", "a.sql b.sql",
        ])
        .unwrap()
        .command
        {
            Cmd::Module { cmd: ModuleCmd::Repair { key, db, mode, files } } => {
                assert_eq!(key, "mod-transmog");
                assert_eq!(db, "world");
                assert_eq!(mode, "mark");
                assert_eq!(files.as_deref(), Some("a.sql b.sql"));
            }
            other => panic!("expected Module Repair, got {other:?}"),
        }
    }

    #[test]
    fn module_mutating_arms_require_their_flags() {
        for argv in [
            vec!["dml-wow", "module", "install"],              // --family
            vec!["dml-wow", "module", "remove", "--family", "cpp"], // --key
            vec!["dml-wow", "module", "remove", "--key", "mod-x"], // --family
            vec!["dml-wow", "module", "update"],               // --key
            vec!["dml-wow", "module", "repair", "--key", "mod-x", "--db", "world"], // --mode
            vec!["dml-wow", "module", "repair", "--db", "world", "--mode", "mark"], // --key
            vec!["dml-wow", "module", "repair", "--key", "mod-x", "--mode", "mark"], // --db
        ] {
            assert!(Cli::try_parse_from(&argv).is_err(), "{argv:?} should be a usage error");
        }
    }

    #[test]
    fn parses_backup_arms() {
        match Cli::try_parse_from(["dml-wow", "backup", "create"]).unwrap().command {
            Cmd::Backup { cmd: BackupCmd::Create { include_world, name } } => {
                assert!(!include_world);
                assert_eq!(name, None);
            }
            other => panic!("expected Backup Create, got {other:?}"),
        }
        match Cli::try_parse_from([
            "dml-wow", "backup", "create", "--include-world", "--name", "before the raid",
        ])
        .unwrap()
        .command
        {
            Cmd::Backup { cmd: BackupCmd::Create { include_world, name } } => {
                assert!(include_world);
                assert_eq!(name.as_deref(), Some("before the raid"));
            }
            other => panic!("expected Backup Create, got {other:?}"),
        }
        assert!(matches!(
            Cli::try_parse_from(["dml-wow", "backup", "list"]).unwrap().command,
            Cmd::Backup { cmd: BackupCmd::List }
        ));
        match Cli::try_parse_from(["dml-wow", "backup", "validate", "wow-20260101-000000.sql.gz"])
            .unwrap()
            .command
        {
            Cmd::Backup { cmd: BackupCmd::Validate { file } } => {
                assert_eq!(file, "wow-20260101-000000.sql.gz")
            }
            other => panic!("expected Backup Validate, got {other:?}"),
        }
        match Cli::try_parse_from(["dml-wow", "backup", "delete", "wow-20260101-000000.sql.gz"])
            .unwrap()
            .command
        {
            Cmd::Backup { cmd: BackupCmd::Delete { file } } => {
                assert_eq!(file, "wow-20260101-000000.sql.gz")
            }
            other => panic!("expected Backup Delete, got {other:?}"),
        }
        match Cli::try_parse_from(["dml-wow", "backup", "restore", "wow-x.sql.gz", "--yes"])
            .unwrap()
            .command
        {
            Cmd::Backup { cmd: BackupCmd::Restore { file, yes } } => {
                assert_eq!(file, "wow-x.sql.gz");
                assert!(yes);
            }
            other => panic!("expected Backup Restore, got {other:?}"),
        }
    }

    /// The four destructive arms all PARSE without `--yes` — the refusal is a
    /// domain CONFIRM_REQUIRED envelope (exit 1) from `run.rs`, deliberately
    /// NOT a clap usage error (exit 2): a machine consumer must be able to
    /// tell "you meant this but must confirm" apart from "you typed it wrong".
    #[test]
    fn destructive_arms_parse_without_yes_and_default_it_false() {
        match Cli::try_parse_from(["dml-wow", "backup", "restore", "wow-x.sql.gz"]).unwrap().command
        {
            Cmd::Backup { cmd: BackupCmd::Restore { yes, .. } } => assert!(!yes),
            other => panic!("expected Backup Restore, got {other:?}"),
        }
        match Cli::try_parse_from(["dml-wow", "docker-clean", "--level", "2"]).unwrap().command {
            Cmd::DockerClean { level, yes } => {
                assert_eq!(level, 2);
                assert!(!yes);
            }
            other => panic!("expected DockerClean, got {other:?}"),
        }
        match Cli::try_parse_from(["dml-wow", "bots-flush"]).unwrap().command {
            Cmd::BotsFlush { ack, yes } => {
                assert_eq!(ack, "", "an omitted --ack must reach the confirm gate, not clap");
                assert!(!yes);
            }
            other => panic!("expected BotsFlush, got {other:?}"),
        }
        match Cli::try_parse_from(["dml-wow", "games-remove", "wow-tbc-server"]).unwrap().command {
            Cmd::GamesRemove { id, keep_data, remove_images, yes } => {
                assert_eq!(id, "wow-tbc-server");
                assert!(!keep_data);
                assert!(!remove_images);
                assert!(!yes);
            }
            other => panic!("expected GamesRemove, got {other:?}"),
        }
    }

    #[test]
    fn destructive_arms_take_their_flags() {
        match Cli::try_parse_from(["dml-wow", "bots-flush", "--yes", "--ack", "flush"])
            .unwrap()
            .command
        {
            Cmd::BotsFlush { ack, yes } => {
                assert_eq!(ack, "flush");
                assert!(yes);
            }
            other => panic!("expected BotsFlush, got {other:?}"),
        }
        match Cli::try_parse_from([
            "dml-wow", "games-remove", "wow-tbc-server", "--yes", "--keep-data", "--remove-images",
        ])
        .unwrap()
        .command
        {
            Cmd::GamesRemove { keep_data, remove_images, yes, .. } => {
                assert!(keep_data);
                assert!(remove_images);
                assert!(yes);
            }
            other => panic!("expected GamesRemove, got {other:?}"),
        }
        // An out-of-range --level still PARSES (u8): the 1..=3 rule is
        // `docker_clean_stream`'s, so it must reach the library to be refused
        // in its own words -- the opposite of `console-tail --lines`.
        match Cli::try_parse_from(["dml-wow", "docker-clean", "--level", "9", "--yes"])
            .unwrap()
            .command
        {
            Cmd::DockerClean { level, yes } => {
                assert_eq!(level, 9);
                assert!(yes);
            }
            other => panic!("expected DockerClean, got {other:?}"),
        }
    }

    #[test]
    fn destructive_and_lifecycle_arms_reject_malformed_argv() {
        for argv in [
            vec!["dml-wow", "docker-clean"],                    // --level required
            vec!["dml-wow", "docker-clean", "--level", "abc"],  // not a number
            vec!["dml-wow", "docker-clean", "--level", "999"],  // does not fit u8
            vec!["dml-wow", "games-remove"],                    // id required
            vec!["dml-wow", "backup"],                          // subcommand required
            vec!["dml-wow", "backup", "restore"],               // file required
            vec!["dml-wow", "backup", "validate"],
            vec!["dml-wow", "backup", "delete"],
            vec!["dml-wow", "backup", "nope"],
            vec!["dml-wow", "start", "extra"],
            vec!["dml-wow", "self-update", "--backup", "--no-backup"],
        ] {
            assert!(Cli::try_parse_from(&argv).is_err(), "{argv:?} should be a usage error");
        }
    }

    #[test]
    fn parses_self_update() {
        match Cli::try_parse_from(["dml-wow", "self-update"]).unwrap().command {
            Cmd::SelfUpdate { backup } => assert_eq!(backup.choice(), None),
            other => panic!("expected SelfUpdate, got {other:?}"),
        }
        match Cli::try_parse_from(["dml-wow", "self-update", "--no-backup"]).unwrap().command {
            Cmd::SelfUpdate { backup } => assert_eq!(backup.choice(), Some(false)),
            other => panic!("expected SelfUpdate, got {other:?}"),
        }
    }

    /// The 25-id cap is NOT a clap-parse-time concern (review finding 2, see
    /// `parse_item_ids`'s doc comment) — a well-formed list of any length,
    /// including over 25, parses cleanly here. The cap itself is exercised
    /// as a subprocess `run.rs` BAD_ARG/exit-1 case in
    /// `tests/cli_integration.rs` (`item_info_cap_violation_is_bad_arg`).
    #[test]
    fn item_info_well_formed_ids_parse_regardless_of_count() {
        let over_25 = (1..=26).map(|n| n.to_string()).collect::<Vec<_>>().join(",");
        match Cli::try_parse_from(["dml-wow", "item-info", &over_25]).unwrap().command {
            Cmd::ItemInfo { ids } => assert_eq!(ids.0.len(), 26),
            other => panic!("expected ItemInfo, got {other:?}"),
        }
        let exactly_25 = (1..=25).map(|n| n.to_string()).collect::<Vec<_>>().join(",");
        match Cli::try_parse_from(["dml-wow", "item-info", &exactly_25]).unwrap().command {
            Cmd::ItemInfo { ids } => assert_eq!(ids.0.len(), 25),
            other => panic!("expected ItemInfo, got {other:?}"),
        }
    }

    // -- Task 15: lan / cache / client-path / accountwide / commands / install

    #[test]
    fn parses_lan_arms() {
        match Cli::try_parse_from(["dml-wow", "lan", "on", "192.168.1.50"]).unwrap().command {
            Cmd::Lan { action, ip, internet, local } => {
                assert_eq!(action, "on");
                assert_eq!(ip.as_deref(), Some("192.168.1.50"));
                assert!(!internet);
                assert_eq!(local, None); // absent unless asked for
            }
            other => panic!("expected Lan, got {other:?}"),
        }
        match Cli::try_parse_from(["dml-wow", "lan", "off"]).unwrap().command {
            Cmd::Lan { action, ip, .. } => {
                assert_eq!(action, "off");
                assert_eq!(ip, None);
            }
            other => panic!("expected Lan, got {other:?}"),
        }
        assert!(matches!(
            Cli::try_parse_from(["dml-wow", "lan", "status"]).unwrap().command,
            Cmd::Lan { action, ip: None, internet: false, local: None } if action == "status"
        ));
        match Cli::try_parse_from([
            "dml-wow", "lan", "refresh", "myserver.duckdns.org", "--internet",
        ])
        .unwrap()
        .command
        {
            Cmd::Lan { action, ip, internet, .. } => {
                assert_eq!(action, "refresh");
                assert_eq!(ip.as_deref(), Some("myserver.duckdns.org"));
                assert!(internet);
            }
            other => panic!("expected Lan, got {other:?}"),
        }
        // The internet-play LAN fix: `--local` rides alongside `--internet`
        // and carries this host's LAN address for realmlist.localAddress.
        match Cli::try_parse_from([
            "dml-wow", "lan", "on", "wow.example.org", "--internet", "--local", "192.168.1.50",
        ])
        .unwrap()
        .command
        {
            Cmd::Lan { action, ip, internet, local } => {
                assert_eq!(action, "on");
                assert_eq!(ip.as_deref(), Some("wow.example.org"));
                assert!(internet);
                assert_eq!(local.as_deref(), Some("192.168.1.50"));
            }
            other => panic!("expected Lan, got {other:?}"),
        }
    }

    /// `action` is a RAW string (review-relevant): an unknown value still
    /// PARSES here — `validate_lan_request` is what turns it into a domain
    /// BAD_ARG, not clap. Same doctrine as `account set-gm`'s out-of-range
    /// `level` (Task 13).
    #[test]
    fn lan_action_is_not_a_clap_allowlist() {
        match Cli::try_parse_from(["dml-wow", "lan", "reset"]).unwrap().command {
            Cmd::Lan { action, .. } => assert_eq!(action, "reset"),
            other => panic!("expected Lan, got {other:?}"),
        }
    }

    #[test]
    fn lan_requires_the_action_positional() {
        assert!(Cli::try_parse_from(["dml-wow", "lan"]).is_err());
    }

    #[test]
    fn parses_cache_arms() {
        assert!(matches!(
            Cli::try_parse_from(["dml-wow", "cache", "status"]).unwrap().command,
            Cmd::Cache { cmd: CacheCmd::Status }
        ));
        assert!(matches!(
            Cli::try_parse_from(["dml-wow", "cache", "clean"]).unwrap().command,
            Cmd::Cache { cmd: CacheCmd::Clean }
        ));
        assert!(Cli::try_parse_from(["dml-wow", "cache"]).is_err());
        assert!(Cli::try_parse_from(["dml-wow", "cache", "nope"]).is_err());
    }

    #[test]
    fn parses_client_path_arms() {
        assert!(matches!(
            Cli::try_parse_from(["dml-wow", "client-path", "get"]).unwrap().command,
            Cmd::ClientPath { cmd: ClientPathCmd::Get }
        ));
        match Cli::try_parse_from(["dml-wow", "client-path", "set", "C:\\Games\\WoW"])
            .unwrap()
            .command
        {
            Cmd::ClientPath { cmd: ClientPathCmd::Set { dir } } => {
                assert_eq!(dir, "C:\\Games\\WoW")
            }
            other => panic!("expected ClientPath Set, got {other:?}"),
        }
        assert!(matches!(
            Cli::try_parse_from(["dml-wow", "client-path", "detect"]).unwrap().command,
            Cmd::ClientPath { cmd: ClientPathCmd::Detect }
        ));
        assert!(Cli::try_parse_from(["dml-wow", "client-path", "set"]).is_err());
        assert!(Cli::try_parse_from(["dml-wow", "client-path"]).is_err());
    }

    #[test]
    fn parses_accountwide_arms() {
        assert!(matches!(
            Cli::try_parse_from(["dml-wow", "accountwide", "get"]).unwrap().command,
            Cmd::Accountwide { cmd: AccountwideCmd::Get }
        ));
        match Cli::try_parse_from([
            "dml-wow", "accountwide", "set", "ENABLE_ACCOUNTWIDE_MOUNTS", "on",
        ])
        .unwrap()
        .command
        {
            Cmd::Accountwide { cmd: AccountwideCmd::Set { key, value, variant } } => {
                assert_eq!(key, "ENABLE_ACCOUNTWIDE_MOUNTS");
                assert_eq!(value, "on");
                assert_eq!(variant, None);
            }
            other => panic!("expected Accountwide Set, got {other:?}"),
        }
        match Cli::try_parse_from([
            "dml-wow", "accountwide", "set", "ENABLE_ACCOUNTWIDE_REPUTATION", "on", "--variant",
            "default",
        ])
        .unwrap()
        .command
        {
            Cmd::Accountwide { cmd: AccountwideCmd::Set { variant, .. } } => {
                assert_eq!(variant.as_deref(), Some("default"))
            }
            other => panic!("expected Accountwide Set, got {other:?}"),
        }
        assert!(Cli::try_parse_from(["dml-wow", "accountwide", "set", "ENABLE_X"]).is_err());
        assert!(Cli::try_parse_from(["dml-wow", "accountwide"]).is_err());
    }

    #[test]
    fn parses_commands_and_rejects_stray_args() {
        assert!(matches!(
            Cli::try_parse_from(["dml-wow", "commands"]).unwrap().command,
            Cmd::Commands
        ));
        assert!(Cli::try_parse_from(["dml-wow", "commands", "extra"]).is_err());
    }

    #[test]
    fn parses_install_default_and_explicit_id() {
        match Cli::try_parse_from(["dml-wow", "install"]).unwrap().command {
            Cmd::Install { id } => assert_eq!(id, "wow-server-playerbots"),
            other => panic!("expected Install, got {other:?}"),
        }
        match Cli::try_parse_from(["dml-wow", "install", "maplestory-server"]).unwrap().command {
            Cmd::Install { id } => assert_eq!(id, "maplestory-server"),
            other => panic!("expected Install, got {other:?}"),
        }
        // A malformed id still PARSES here — `run.rs`'s `valid_game_id` guard
        // is what rejects it, matching `start`/`stop`/`restart`'s own id.
        match Cli::try_parse_from(["dml-wow", "install", "wow server"]).unwrap().command {
            Cmd::Install { id } => assert_eq!(id, "wow server"),
            other => panic!("expected Install, got {other:?}"),
        }
        assert!(Cli::try_parse_from(["dml-wow", "install", "a", "b"]).is_err());
    }

    #[test]
    fn parses_install_native_and_leaves_the_wsl_install_route_alone() {
        match Cli::try_parse_from(["dml-wow", "install-native"]).unwrap().command {
            Cmd::InstallNative { id, allow_underspec } => {
                assert_eq!(id, "wow-server-playerbots");
                assert!(!allow_underspec, "the hardware floor is ON unless asked otherwise");
            }
            other => panic!("expected InstallNative, got {other:?}"),
        }
        match Cli::try_parse_from([
            "dml-wow",
            "install-native",
            "--id",
            "wow-test",
            "--allow-underspec",
        ])
        .unwrap()
        .command
        {
            Cmd::InstallNative { id, allow_underspec } => {
                assert_eq!(id, "wow-test");
                assert!(allow_underspec);
            }
            other => panic!("expected InstallNative, got {other:?}"),
        }
        // A malformed id still PARSES — `run.rs`'s `valid_game_id` (and then the
        // engine's stricter `valid_title_id`) is what refuses it.
        match Cli::try_parse_from(["dml-wow", "install-native", "--id", "../escape"])
            .unwrap()
            .command
        {
            Cmd::InstallNative { id, .. } => assert_eq!(id, "../escape"),
            other => panic!("expected InstallNative, got {other:?}"),
        }
        // The id is a FLAG here, not a positional: a bare word is a usage error
        // rather than a silently-installed title of that name.
        assert!(Cli::try_parse_from(["dml-wow", "install-native", "wow-test"]).is_err());

        // The two routes must stay separate commands. This fails if anyone
        // "unifies" them, which would put the interactive WSL passthrough
        // behind the native name (or vice versa) with no other test noticing.
        assert!(matches!(
            Cli::try_parse_from(["dml-wow", "install"]).unwrap().command,
            Cmd::Install { .. }
        ));
    }
}
