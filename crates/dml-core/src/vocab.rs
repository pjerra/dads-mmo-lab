//! The launcher's command vocabulary, translated for `Backend::Arch`.
//!
//! # Why this module exists
//!
//! The launcher's 100-odd call sites speak the BASH CLI's vocabulary — `games
//! list`, `games status <id>`, `wow server-detail`, `games <action> <id>` — and
//! [`crate::runner::DmlRunner::command`] appends `--json`. `dml-wow` defines
//! none of that: no `games`/`wow` namespace, no global `--json` (verified
//! against the real binary: `dml-wow version --json` → "unexpected argument").
//! That mismatch, not anything about the distro, is why `Backend::Arch` was
//! built, reviewed, proven on real hardware — and still could not be the
//! default.
//!
//! # The shape of the fix, and the property that makes it safe
//!
//! This is a PURE table: `fn(&[&str]) -> Translation`, no I/O, no spawn. It is
//! consulted by the Arch runner and by nothing else. `Backend::Wsl` and
//! `Backend::Native` never reach it, so a bug in this file cannot alter their
//! argv — that byte-identity is STRUCTURAL, not a promise to remember. This
//! branch already broke `Wsl` once by changing what it routed to; the fix must
//! not depend on anyone recalling that.
//!
//! # The fallback is the default, and that is the whole incremental story
//!
//! `provision.rs` installs BOTH binaries into `dml-arch` (`/usr/local/bin/dml`
//! and `/usr/local/bin/dml-wow`), and every verb the launcher sends is — by
//! construction, because `Backend::Wsl` works — one the bash `dml` in that same
//! distro already answers. So an unclassified verb routes to bash IN THE SAME
//! DISTRO, against the same `dockerd` and the same `$HOME/games`: today's
//! behaviour, unchanged. There is deliberately NO `Unsupported` outcome. The
//! place a missing classification must hurt is the BUILD (the launcher crate's
//! `every_launcher_call_site_is_classified`), never the user's Tools page.
//!
//! If a verb ever becomes genuinely unavailable — say the bash CLI stops being
//! deployed — add a `Target::Refuse { code, .. }` then, and give `RunnerError`
//! a matching variant. Not before: a refusal arm with nothing to refuse is dead
//! code that will rot.

/// Which program inside `dml-arch` answers this verb.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// `/usr/local/bin/dml-wow` — the Rust CLI. Never receives `--json`.
    DmlWow,
    /// `/usr/local/bin/dml` — the bash CLI, argv unchanged, `--json` appended
    /// exactly as `Backend::Wsl` does today.
    Bash,
}

/// What to run, and with what argv.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Translation {
    pub target: Target,
    pub argv: Vec<String>,
}

/// What happens to the tokens left over after the verb and the
/// [`Row::positionals`] have been consumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tail {
    /// Pass them through in order (`games remove <id> --yes` keeps `<id>` as a
    /// positional, which is what `dml-wow games-remove` wants).
    Keep,
    /// The first BARE token becomes `--id <it>`; flags still pass through.
    /// `dml-wow`'s `start`/`stop`/`restart` name their title with `--id`, where
    /// bash takes it positionally.
    IdFlag,
}

/// One verb's classification and, for a `DmlWow` row, its rewrite.
pub struct Row {
    /// The bash-CLI verb tokens the launcher sends, exactly as its argv
    /// literals spell them. This is also the key the launcher's coverage test
    /// matches against, so it must be the LEADING RUN OF LITERALS at the call
    /// site — not a longer form including values.
    pub verb: &'static [&'static str],
    pub target: Target,
    /// Does this verb stream NDJSON (`run_stream`) rather than return one
    /// envelope? Pinned against `run.rs` by the CLI crate's T3.
    pub streams: bool,
    /// The `run.rs` dispatch arm, e.g. `Cmd::Restart` or `BackupCmd::Create`.
    /// Empty for `Bash` rows — bash has no clap tree to pin.
    pub arm: &'static str,
    /// The `dml-wow` verb tokens that REPLACE [`Self::verb`].
    pub head: &'static [&'static str],
    /// Flags whose VALUE becomes a positional, in this order, right after
    /// [`Self::head`]. An absent optional flag simply contributes nothing.
    pub positionals: &'static [&'static str],
    /// Tokens appended after everything else — the two safety additions
    /// (`--yes`, `--no-stop-engine`) live here.
    pub append: &'static [&'static str],
    pub tail: Tail,
    /// A REALISTIC bash-side argv for this verb, values and all. The CLI
    /// crate's T1 feeds `translate_for_arch(sample)` straight into `dml-wow`'s
    /// real clap tree, so the expected output is never hand-written — the
    /// oracle is the clap derive itself.
    pub sample: &'static [&'static str],
}

/// Shorthand for the many rows that are a plain verb rename.
const NONE: &[&str] = &[];

macro_rules! row {
    (
        $verb:expr, $target:expr, $streams:expr, $arm:expr,
        $head:expr, $positionals:expr, $append:expr, $tail:expr, $sample:expr
    ) => {
        Row {
            verb: $verb,
            target: $target,
            streams: $streams,
            arm: $arm,
            head: $head,
            positionals: $positionals,
            append: $append,
            tail: $tail,
            sample: $sample,
        }
    };
}

/// A `dml-wow` row: verb → head, with optional flag→positional promotion.
macro_rules! wow {
    ($verb:expr, $arm:expr, $head:expr, $sample:expr) => {
        row!($verb, Target::DmlWow, false, $arm, $head, NONE, NONE, Tail::Keep, $sample)
    };
    ($verb:expr, $arm:expr, $head:expr, pos = $pos:expr, $sample:expr) => {
        row!($verb, Target::DmlWow, false, $arm, $head, $pos, NONE, Tail::Keep, $sample)
    };
}

/// A `dml-wow` row that STREAMS NDJSON.
macro_rules! wow_s {
    ($verb:expr, $arm:expr, $head:expr, $sample:expr) => {
        row!($verb, Target::DmlWow, true, $arm, $head, NONE, NONE, Tail::Keep, $sample)
    };
    ($verb:expr, $arm:expr, $head:expr, pos = $pos:expr, $sample:expr) => {
        row!($verb, Target::DmlWow, true, $arm, $head, $pos, NONE, Tail::Keep, $sample)
    };
}

/// A row that stays on the bash CLI, argv untouched.
// NB the `stream` arm MUST come first: `bash!(stream &[…], …)` otherwise
// matches the plain arm, because `stream &[…]` parses as a bitwise-and
// expression rather than a marker token.
macro_rules! bash {
    (stream $verb:expr, $sample:expr) => {
        row!($verb, Target::Bash, true, "", NONE, NONE, NONE, Tail::Keep, $sample)
    };
    ($verb:expr, $sample:expr) => {
        row!($verb, Target::Bash, false, "", NONE, NONE, NONE, Tail::Keep, $sample)
    };
}

/// EVERY verb the launcher sends, explicitly classified.
///
/// A verb absent from this table still WORKS (it falls back to bash) — what it
/// fails is the launcher's coverage test. That asymmetry is deliberate: see the
/// module doc.
pub static TABLE: &[Row] = &[
    // -- group A: maps mechanically to a `dml-wow` arm --------------------
    wow!(&["version"], "Cmd::Version", &["version"], &["version"]),
    wow!(&["wow", "server-detail"], "Cmd::Status", &["status"], &["wow", "server-detail"]),
    wow!(&["wow", "server-info"], "Cmd::ServerInfo", &["server-info"], &["wow", "server-info"]),
    wow!(&["wow", "stats"], "Cmd::Stats", &["stats"], &["wow", "stats"]),
    wow!(
        &["wow", "console-tail"],
        "Cmd::ConsoleTail",
        &["console-tail"],
        &["wow", "console-tail", "--lines", "200"]
    ),
    wow!(
        &["wow", "console-send"],
        "Cmd::Console",
        &["console"],
        pos = &["--command"],
        &["wow", "console-send", "--command", "server info"]
    ),
    wow!(&["wow", "accounts"], "Cmd::Accounts", &["accounts"], &["wow", "accounts"]),
    wow!(
        &["wow", "account", "create"],
        "AccountCmd::Create",
        &["account", "create"],
        pos = &["--user", "--pass"],
        &["wow", "account", "create", "--user", "bob", "--pass", "hunter2"]
    ),
    wow!(
        &["wow", "account", "set-password"],
        "AccountCmd::SetPassword",
        &["account", "set-password"],
        pos = &["--user", "--pass"],
        &["wow", "account", "set-password", "--user", "bob", "--pass", "hunter2"]
    ),
    wow!(
        &["wow", "account", "set-gm"],
        "AccountCmd::SetGm",
        &["account", "set-gm"],
        pos = &["--user", "--level"],
        &["wow", "account", "set-gm", "--user", "bob", "--level", "3"]
    ),
    wow!(
        &["wow", "account", "delete"],
        "AccountCmd::Delete",
        &["account", "delete"],
        pos = &["--user"],
        &["wow", "account", "delete", "--user", "bob"]
    ),
    wow!(&["wow", "commands"], "Cmd::Commands", &["commands"], &["wow", "commands"]),
    wow!(&["wow", "module", "list"], "ModuleCmd::List", &["module", "list"], &["wow", "module", "list"]),
    wow!(
        &["wow", "module", "repair"],
        "ModuleCmd::Repair",
        &["module", "repair"],
        &["wow", "module", "repair", "--key", "mod-transmog", "--db", "world", "--mode", "mark"]
    ),
    wow!(
        &["wow", "client-path", "get"],
        "ClientPathCmd::Get",
        &["client-path", "get"],
        &["wow", "client-path", "get"]
    ),
    wow!(
        &["wow", "client-path", "set"],
        "ClientPathCmd::Set",
        &["client-path", "set"],
        pos = &["--path"],
        &["wow", "client-path", "set", "--path", "C:/Games/WoW"]
    ),
    wow!(
        &["wow", "client-path", "detect"],
        "ClientPathCmd::Detect",
        &["client-path", "detect"],
        &["wow", "client-path", "detect"]
    ),
    wow!(
        &["wow", "items", "search"],
        "Cmd::ItemsSearch",
        &["items-search"],
        &["wow", "items", "search", "--name", "thunder", "--quality", "4", "--min-level", "1", "--max-level", "80"]
    ),
    wow!(
        &["wow", "mail-item"],
        "Cmd::MailItem",
        &["mail-item"],
        pos = &["--to", "--items"],
        &["wow", "mail-item", "--to", "Bob", "--items", "25:1", "--subject", "hi", "--body", "there"]
    ),
    wow!(
        &["wow", "teleport-list"],
        "Cmd::TeleportList",
        &["teleport-list"],
        &["wow", "teleport-list", "--search", "storm"]
    ),
    wow!(
        &["wow", "teleport"],
        "Cmd::Teleport",
        &["teleport"],
        pos = &["--char", "--to"],
        &["wow", "teleport", "--char", "Bob", "--to", "stormwind"]
    ),
    wow!(
        &["wow", "paperdoll"],
        "Cmd::Paperdoll",
        &["paperdoll"],
        pos = &["--char"],
        &["wow", "paperdoll", "--char", "Bob"]
    ),
    wow!(
        &["wow", "item-info"],
        "Cmd::ItemInfo",
        &["item-info"],
        pos = &["--entries"],
        &["wow", "item-info", "--entries", "25,116,6948"]
    ),
    wow!(
        &["wow", "char-progress"],
        "Cmd::CharProgress",
        &["char-progress"],
        pos = &["--char"],
        &["wow", "char-progress", "--char", "Bob"]
    ),
    wow!(
        &["wow", "achievements"],
        "Cmd::Achievements",
        &["achievements"],
        pos = &["--char"],
        &["wow", "achievements", "--char", "Bob"]
    ),
    wow!(&["wow", "config", "list"], "ConfigCmd::List", &["config", "list"], &["wow", "config", "list"]),
    wow!(
        &["wow", "config", "set"],
        "ConfigCmd::Set",
        &["config", "set"],
        pos = &["--key", "--value"],
        &["wow", "config", "set", "--key", "rates.xp_kill", "--value", "3"]
    ),
    wow!(&["wow", "config", "files"], "ConfigCmd::Files", &["config", "files"], &["wow", "config", "files"]),
    wow!(
        &["wow", "config", "raw-read"],
        "ConfigCmd::Read",
        &["config", "read"],
        pos = &["--file"],
        &["wow", "config", "raw-read", "--file", "playerbots.conf"]
    ),
    wow!(
        &["wow", "config", "raw-write"],
        "ConfigCmd::Write",
        &["config", "write"],
        pos = &["--file"],
        &["wow", "config", "raw-write", "--file", "playerbots.conf"]
    ),
    wow!(
        &["wow", "config", "tuning-list"],
        "TuningCmd::List",
        &["tuning", "list"],
        &["wow", "config", "tuning-list"]
    ),
    wow!(
        &["wow", "config", "tuning-set"],
        "TuningCmd::Set",
        &["tuning", "set"],
        pos = &["--key", "--value"],
        &["wow", "config", "tuning-set", "--key", "sitmeansrest.duration", "--value", "45"]
    ),
    wow!(
        &["wow", "accountwide", "get"],
        "AccountwideCmd::Get",
        &["accountwide", "get"],
        &["wow", "accountwide", "get"]
    ),
    wow!(
        &["wow", "accountwide", "set"],
        "AccountwideCmd::Set",
        &["accountwide", "set"],
        pos = &["--key", "--value"],
        &["wow", "accountwide", "set", "--key", "ENABLE_ACCOUNTWIDE_MOUNTS", "--value", "on", "--variant", "x"]
    ),
    wow!(
        &["wow", "players", "online"],
        "Cmd::PlayersOnline",
        &["players-online"],
        &["wow", "players", "online"]
    ),
    wow!(
        &["wow", "bots", "list"],
        "Cmd::Bots",
        &["bots"],
        &["wow", "bots", "list", "--name", "ab", "--class", "1", "--min-level", "1", "--max-level", "80", "--online", "--limit", "50", "--offset", "0"]
    ),
    wow!(
        &["wow", "party", "add"],
        "PartyCmd::Add",
        &["party", "add"],
        pos = &["--player", "--class"],
        &["wow", "party", "add", "--player", "Bob", "--class", "mage", "--gender", "female", "--spec", "frost pve"]
    ),
    wow!(
        &["wow", "party", "kick"],
        "PartyCmd::Kick",
        &["party", "kick"],
        pos = &["--player", "--bot"],
        &["wow", "party", "kick", "--player", "Bob", "--bot", "Zug"]
    ),
    wow!(
        &["wow", "party", "relogin"],
        "PartyCmd::Relogin",
        &["party", "relogin"],
        pos = &["--player", "--bot"],
        &["wow", "party", "relogin", "--player", "Bob", "--bot", "Zug"]
    ),
    wow!(
        &["wow", "party", "botcmd"],
        "PartyCmd::Botcmd",
        &["party", "botcmd"],
        pos = &["--player", "--bot", "--action"],
        &["wow", "party", "botcmd", "--player", "Bob", "--bot", "Zug", "--action", "gear", "--spec", "frost pve"]
    ),
    wow!(
        &["wow", "party", "preset-save"],
        "PartyCmd::PresetSave",
        &["party", "preset-save"],
        pos = &["--player", "--name"],
        &["wow", "party", "preset-save", "--player", "Bob", "--name", "raid"]
    ),
    wow!(
        &["wow", "party", "preset-list"],
        "PartyCmd::PresetList",
        &["party", "preset-list"],
        &["wow", "party", "preset-list"]
    ),
    wow!(
        &["wow", "party", "preset-delete"],
        "PartyCmd::PresetDelete",
        &["party", "preset-delete"],
        pos = &["--name"],
        &["wow", "party", "preset-delete", "--name", "raid"]
    ),
    wow_s!(
        &["wow", "party", "preset-load"],
        "PartyCmd::PresetLoad",
        &["party", "preset-load"],
        pos = &["--player", "--name"],
        &["wow", "party", "preset-load", "--player", "Bob", "--name", "raid"]
    ),
    wow!(
        &["wow", "gm", "level"],
        "GmCmd::Level",
        &["gm", "level"],
        pos = &["--player", "--level"],
        &["wow", "gm", "level", "--player", "Bob", "--level", "80"]
    ),
    wow!(
        &["wow", "gm", "gold"],
        "GmCmd::Gold",
        &["gm", "gold"],
        pos = &["--player", "--gold"],
        &["wow", "gm", "gold", "--player", "Bob", "--gold", "500"]
    ),
    wow!(
        &["wow", "gm", "heal"],
        "GmCmd::Heal",
        &["gm", "heal"],
        pos = &["--player"],
        &["wow", "gm", "heal", "--player", "Bob"]
    ),
    wow!(
        &["wow", "gm", "revive"],
        "GmCmd::Revive",
        &["gm", "revive"],
        pos = &["--player"],
        &["wow", "gm", "revive", "--player", "Bob"]
    ),
    wow!(
        &["wow", "gm", "summon"],
        "GmCmd::Summon",
        &["gm", "summon"],
        pos = &["--player", "--entry"],
        &["wow", "gm", "summon", "--player", "Bob", "--entry", "990000"]
    ),
    wow!(
        &["wow", "gm", "at-login"],
        "GmCmd::AtLogin",
        &["gm", "at-login"],
        pos = &["--player", "--flag"],
        &["wow", "gm", "at-login", "--player", "Bob", "--flag", "rename"]
    ),
    wow!(&["wow", "backup", "list"], "BackupCmd::List", &["backup", "list"], &["wow", "backup", "list"]),
    wow!(
        &["wow", "backup", "delete"],
        "BackupCmd::Delete",
        &["backup", "delete"],
        pos = &["--file"],
        &["wow", "backup", "delete", "--file", "dml-2026.sql.gz"]
    ),
    wow!(
        &["wow", "backup", "validate"],
        "BackupCmd::Validate",
        &["backup", "validate"],
        pos = &["--file"],
        &["wow", "backup", "validate", "--file", "dml-2026.sql.gz"]
    ),
    wow_s!(
        &["wow", "backup", "create"],
        "BackupCmd::Create",
        &["backup", "create"],
        &["wow", "backup", "create", "--include-world"]
    ),
    // `--yes`: `dml-wow` carries the destructive gate the bash CLI does not
    // (the launcher's gate is its own typed-confirm UI, already passed by the
    // time the argv is built). Without it every restore/clean earns a
    // CONFIRM_REQUIRED refusal instead of running.
    row!(
        &["wow", "backup", "restore"],
        Target::DmlWow,
        true,
        "BackupCmd::Restore",
        &["backup", "restore"],
        &["--file"],
        &["--yes"],
        Tail::Keep,
        &["wow", "backup", "restore", "--file", "dml-2026.sql.gz"]
    ),
    row!(
        &["wow", "docker-clean"],
        Target::DmlWow,
        true,
        "Cmd::DockerClean",
        &["docker-clean"],
        NONE,
        &["--yes"],
        Tail::Keep,
        &["wow", "docker-clean", "--level", "2"]
    ),
    wow_s!(
        &["wow", "bots", "flush"],
        "Cmd::BotsFlush",
        &["bots-flush"],
        &["wow", "bots", "flush", "--yes", "--ack", "flush"]
    ),
    wow_s!(&["wow", "update"], "Cmd::SelfUpdate", &["self-update"], &["wow", "update", "--no-backup"]),
    wow_s!(
        &["wow", "module", "install"],
        "ModuleCmd::Install",
        &["module", "install"],
        &["wow", "module", "install", "--family", "cpp", "--key", "mod-transmog", "--no-backup"]
    ),
    wow_s!(
        &["wow", "module", "remove"],
        "ModuleCmd::Remove",
        &["module", "remove"],
        &["wow", "module", "remove", "--family", "cpp", "--key", "mod-transmog", "--no-backup"]
    ),
    wow_s!(
        &["wow", "module", "rebuild"],
        "ModuleCmd::Rebuild",
        &["module", "rebuild"],
        &["wow", "module", "rebuild", "--no-backup"]
    ),
    wow_s!(
        &["wow", "module", "update"],
        "ModuleCmd::Update",
        &["module", "update"],
        &["wow", "module", "update", "--key", "mod-transmog"]
    ),
    wow!(&["wow", "cache-status"], "CacheCmd::Status", &["cache", "status"], &["wow", "cache-status"]),
    wow!(&["wow", "cache-clean"], "CacheCmd::Clean", &["cache", "clean"], &["wow", "cache-clean"]),
    row!(
        &["games", "start"],
        Target::DmlWow,
        true,
        "Cmd::Start",
        &["start"],
        NONE,
        NONE,
        Tail::IdFlag,
        &["games", "start", "wow-server-playerbots"]
    ),
    // `--no-stop-engine` is R2, and it is not cosmetic. `dml-wow stop` defaults
    // to stopping "the engine", which inside the distro means
    // `docker desktop stop` — a command that does not exist there today, but
    // one that would take down the distro's OWN dockerd the moment it did.
    row!(
        &["games", "stop"],
        Target::DmlWow,
        true,
        "Cmd::Stop",
        &["stop"],
        NONE,
        &["--no-stop-engine"],
        Tail::IdFlag,
        &["games", "stop", "wow-server-playerbots"]
    ),
    row!(
        &["games", "restart"],
        Target::DmlWow,
        true,
        "Cmd::Restart",
        &["restart"],
        NONE,
        NONE,
        Tail::IdFlag,
        &["games", "restart", "wow-server-playerbots", "--no-saveall"]
    ),
    wow_s!(
        &["games", "remove"],
        "Cmd::GamesRemove",
        &["games-remove"],
        &["games", "remove", "wow-server-playerbots", "--yes", "--keep-data", "--remove-images"]
    ),
    // `games list` / `games status` have no `dml-wow` arm YET — both are
    // filesystem questions about `$HOME/games` that `dml_wow::lifecycle` can
    // already answer. They ride the bash fallback until those arms land.
    bash!(&["games", "list"], &["games", "list"]),
    bash!(&["games", "status"], &["games", "status", "wow-server-playerbots"]),
    // -- group C: no `dml-wow` arm — the bash `dml` in the SAME distro ------
    //
    // Each of these is a verb `Backend::Wsl` drives today against the same
    // distro, the same dockerd and the same `$HOME/games`. Nothing is gated
    // off; every one has a working destination on day one.
    bash!(&["wow", "docker-usage"], &["wow", "docker-usage"]),
    bash!(&["wow", "update-check"], &["wow", "update-check"]),
    bash!(&["wow", "module", "update-check"], &["wow", "module", "update-check"]),
    bash!(&["wow", "module", "conf-activate"], &["wow", "module", "conf-activate", "--key", "mod-transmog"]),
    bash!(&["wow", "module", "tracking"], &["wow", "module", "tracking", "--key", "mod-transmog"]),
    bash!(&["wow", "module", "fixit"], &["wow", "module", "fixit", "mod-transmog"]),
    bash!(&["wow", "module", "place-npc"], &["wow", "module", "place-npc", "--key", "mod-transmog"]),
    bash!(stream &["wow", "module", "client-patch"], &["wow", "module", "client-patch", "--key", "mod-transmog"]),
    bash!(&["wow", "entity-info"], &["wow", "entity-info", "--kind", "creature", "--ids", "1,2"]),
    bash!(&["wow", "config", "pb-keys"], &["wow", "config", "pb-keys"]),
    bash!(&["wow", "config", "conf-keys"], &["wow", "config", "conf-keys", "--file", "playerbots.conf"]),
    bash!(&["wow", "config", "raw-reset"], &["wow", "config", "raw-reset", "--file", "playerbots.conf"]),
    bash!(&["wow", "party", "online"], &["wow", "party", "online"]),
    bash!(&["wow", "party", "specs"], &["wow", "party", "specs"]),
    bash!(&["wow", "party", "list"], &["wow", "party", "list", "--player", "Bob"]),
    bash!(&["wow", "party", "dismiss-all"], &["wow", "party", "dismiss-all", "--player", "Bob"]),
    bash!(&["wow", "party", "preset-show"], &["wow", "party", "preset-show", "--name", "raid"]),
    bash!(&["wow", "party", "preset-import"], &["wow", "party", "preset-import", "--name", "raid", "--classes", "mage"]),
    bash!(&["wow", "teleport-coords"], &["wow", "teleport-coords", "--char", "Bob", "--map", "0"]),
    bash!(&["wow", "gm", "return-home"], &["wow", "gm", "return-home", "--char", "Bob"]),
    bash!(stream &["wow", "world-restart"], &["wow", "world-restart", "--no-saveall"]),
    bash!(stream &["wow", "bridge-setup"], &["wow", "bridge-setup"]),
    bash!(stream &["wow", "party-setup"], &["wow", "party-setup"]),
    bash!(stream &["wow", "ahbot", "repair"], &["wow", "ahbot", "repair", "--char", "Bob"]),
    bash!(&["wow", "lan", "public-ip"], &["wow", "lan", "public-ip"]),
    // Linux-only by nature — bash is the RIGHT home for it inside the distro.
    bash!(&["wow", "tailscale"], &["wow", "tailscale", "status"]),
    bash!(&["wow", "port-check"], &["wow", "port-check"]),
    // `sudo -n systemctl restart docker` — correct on Arch.
    bash!(&["wow", "docker-restart"], &["wow", "docker-restart"]),
    bash!(&["doctor"], &["doctor"]),
    // `dml-wow lan` exists but is single-title and envelope-shaped; this caller
    // wants TEXT (`run_captured`). Contract mismatch, so it stays on bash.
    bash!(&["lan"], &["lan", "wow-server-playerbots", "status"]),
    // The title registry lives in `cli/src/80-titles.sh`, and
    // `_installers_supported` is TRUE inside the distro — bash is correct here.
    bash!(&["games", "catalog"], &["games", "catalog"]),
    // Interactive `.sh` installer. `dml-wow install <id>` exists, but this is
    // stdio passthrough and the installers are Linux scripts that work on Arch
    // precisely BECAUSE it is Linux.
    bash!(&["games", "install"], &["games", "install", "wow-server-playerbots"]),
    // `tool_install`'s closed TOOL_NAMES allowlist. `dml-wow unbound` exists but
    // emits NDJSON where this call site reads `chunk`/`exit` stdio.
    bash!(&["unbound"], &["unbound"]),
    bash!(&["unbound-remove"], &["unbound-remove"]),
    bash!(&["run"], &["run", "https://example.invalid/x"]),
];

/// The row whose [`Row::verb`] is the LONGEST prefix of `args`, if any.
fn lookup(args: &[&str]) -> Option<&'static Row> {
    let mut best: Option<&'static Row> = None;
    for row in TABLE {
        if row.verb.len() <= args.len() && args[..row.verb.len()] == *row.verb {
            if best.map_or(true, |b| b.verb.len() < row.verb.len()) {
                best = Some(row);
            }
        }
    }
    best
}

/// Does this verb have an EXPLICIT table row?
///
/// Deliberately NOT "does it reach a working destination" — everything does,
/// via the bash fallback. This answers the build-time question the launcher's
/// coverage test asks: has a human decided where this verb belongs?
pub fn is_classified(verb: &[&str]) -> bool {
    TABLE.iter().any(|r| r.verb == verb)
}

/// The bash-CLI argv the launcher sends → what to run inside `dml-arch`.
///
/// TOTAL: an unclassified verb returns [`Target::Bash`] with `argv` UNCHANGED,
/// which is exactly today's `Backend::Wsl` behaviour.
pub fn translate_for_arch(args: &[&str]) -> Translation {
    let passthrough = || Translation {
        target: Target::Bash,
        argv: args.iter().map(|s| s.to_string()).collect(),
    };
    let Some(row) = lookup(args) else { return passthrough() };
    if row.target == Target::Bash {
        return passthrough();
    }

    let rest = &args[row.verb.len()..];

    // 1. Promote the named flags to positionals, in the row's order. A flag
    //    that is absent (an optional one the launcher omitted) contributes
    //    nothing rather than an empty token.
    let mut taken: Vec<Option<String>> = vec![None; row.positionals.len()];
    let mut leftover: Vec<String> = Vec::new();
    let mut i = 0usize;
    while i < rest.len() {
        if let Some(k) = row.positionals.iter().position(|f| *f == rest[i]) {
            if i + 1 < rest.len() {
                taken[k] = Some(rest[i + 1].to_string());
                i += 2;
            } else {
                // A dangling value-less flag: drop it rather than emit a lone
                // `--flag` the positional-shaped arm would reject anyway.
                i += 1;
            }
            continue;
        }
        leftover.push(rest[i].to_string());
        i += 1;
    }

    let mut argv: Vec<String> = row.head.iter().map(|s| s.to_string()).collect();
    argv.extend(taken.into_iter().flatten());

    // 2. Everything the launcher sent that this row did not promote passes
    //    through in order — flags `dml-wow` spells identically, plus (for
    //    Tail::IdFlag) the bare title id, which `dml-wow` names with `--id`.
    match row.tail {
        Tail::Keep => argv.extend(leftover),
        Tail::IdFlag => {
            let mut used = false;
            for tok in leftover {
                if !used && !tok.starts_with('-') {
                    argv.push("--id".to_string());
                    used = true;
                }
                argv.push(tok);
            }
        }
    }

    argv.extend(row.append.iter().map(|s| s.to_string()));
    Translation { target: Target::DmlWow, argv }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tr(args: &[&str]) -> Translation {
        translate_for_arch(args)
    }

    /// THE fail-safe property. An unclassified verb is not an error, it is
    /// today's behaviour: bash, in the same distro, argv untouched.
    #[test]
    fn an_unclassified_verb_falls_back_to_bash_with_argv_unchanged() {
        let t = tr(&["wow", "brand-new-thing", "--flag", "v"]);
        assert_eq!(t.target, Target::Bash);
        assert_eq!(t.argv, vec!["wow", "brand-new-thing", "--flag", "v"]);
    }

    #[test]
    fn a_completely_unknown_namespace_falls_back_too() {
        let t = tr(&["totally", "unknown"]);
        assert_eq!(t.target, Target::Bash);
        assert_eq!(t.argv, vec!["totally", "unknown"]);
    }

    #[test]
    fn is_classified_is_about_the_table_not_about_reachability() {
        assert!(is_classified(&["wow", "server-detail"]));
        assert!(is_classified(&["wow", "docker-usage"])); // a Bash row IS classified
        assert!(!is_classified(&["wow", "brand-new-thing"]));
        // ...and the unclassified verb still WORKS:
        assert_eq!(tr(&["wow", "brand-new-thing"]).target, Target::Bash);
    }

    #[test]
    fn a_bash_row_is_passed_through_byte_for_byte() {
        let t = tr(&["wow", "tailscale", "up"]);
        assert_eq!(t.target, Target::Bash);
        assert_eq!(t.argv, vec!["wow", "tailscale", "up"]);
    }

    #[test]
    fn plain_verb_renames() {
        assert_eq!(tr(&["wow", "server-detail"]).argv, vec!["status"]);
        assert_eq!(tr(&["wow", "players", "online"]).argv, vec!["players-online"]);
        assert_eq!(tr(&["wow", "cache-status"]).argv, vec!["cache", "status"]);
        assert_eq!(tr(&["wow", "config", "tuning-list"]).argv, vec!["tuning", "list"]);
        assert_eq!(tr(&["version"]).argv, vec!["version"]);
    }

    #[test]
    fn flags_become_positionals_in_the_rows_order() {
        assert_eq!(
            tr(&["wow", "gm", "level", "--player", "Bob", "--level", "80"]).argv,
            vec!["gm", "level", "Bob", "80"]
        );
        assert_eq!(
            tr(&["wow", "account", "create", "--user", "bob", "--pass", "hunter2"]).argv,
            vec!["account", "create", "bob", "hunter2"]
        );
        assert_eq!(tr(&["wow", "paperdoll", "--char", "Bob"]).argv, vec!["paperdoll", "Bob"]);
    }

    /// The launcher builds argv in ITS order; `dml-wow`'s positionals have
    /// THEIRS. The row's list is the authority, not the order of arrival.
    #[test]
    fn positional_order_follows_the_row_not_the_input() {
        assert_eq!(
            tr(&["wow", "teleport", "--to", "stormwind", "--char", "Bob"]).argv,
            vec!["teleport", "Bob", "stormwind"]
        );
    }

    #[test]
    fn unpromoted_flags_pass_through_after_the_positionals() {
        assert_eq!(
            tr(&["wow", "party", "add", "--player", "Bob", "--class", "mage", "--gender", "female"]).argv,
            vec!["party", "add", "Bob", "mage", "--gender", "female"]
        );
        assert_eq!(
            tr(&["wow", "items", "search", "--name", "axe", "--quality", "4"]).argv,
            vec!["items-search", "--name", "axe", "--quality", "4"]
        );
    }

    #[test]
    fn an_absent_optional_flag_contributes_nothing() {
        // `--variant` omitted: no empty token, no placeholder.
        assert_eq!(
            tr(&["wow", "accountwide", "set", "--key", "K", "--value", "on"]).argv,
            vec!["accountwide", "set", "K", "on"]
        );
        assert_eq!(tr(&["wow", "teleport-list"]).argv, vec!["teleport-list"]);
    }

    /// R2. `dml-wow stop` defaults to stopping "the engine"; inside the distro
    /// that is `docker desktop stop`, which would take down the distro's own
    /// dockerd the day the plugin resolves there.
    #[test]
    fn stop_always_carries_no_stop_engine() {
        let t = tr(&["games", "stop", "wow-server-playerbots"]);
        assert_eq!(t.target, Target::DmlWow);
        assert_eq!(t.argv, vec!["stop", "--id", "wow-server-playerbots", "--no-stop-engine"]);
    }

    #[test]
    fn the_destructive_arms_get_the_yes_the_bash_cli_never_needed() {
        assert_eq!(
            tr(&["wow", "backup", "restore", "--file", "b.sql.gz"]).argv,
            vec!["backup", "restore", "b.sql.gz", "--yes"]
        );
        assert_eq!(
            tr(&["wow", "docker-clean", "--level", "2"]).argv,
            vec!["docker-clean", "--level", "2", "--yes"]
        );
    }

    #[test]
    fn a_bare_title_id_becomes_the_id_flag() {
        assert_eq!(
            tr(&["games", "start", "wow-server-playerbots"]).argv,
            vec!["start", "--id", "wow-server-playerbots"]
        );
        assert_eq!(
            tr(&["games", "restart", "wow-server-playerbots", "--no-saveall"]).argv,
            vec!["restart", "--id", "wow-server-playerbots", "--no-saveall"]
        );
    }

    /// `games remove` keeps its id POSITIONAL — `dml-wow games-remove` requires
    /// it that way on purpose (typing the id out is part of the confirmation).
    #[test]
    fn games_remove_keeps_the_id_positional_and_its_flags() {
        assert_eq!(
            tr(&["games", "remove", "wow-x", "--yes", "--keep-data"]).argv,
            vec!["games-remove", "wow-x", "--yes", "--keep-data"]
        );
    }

    #[test]
    fn console_send_collapses_to_the_raw_passthrough_arm() {
        assert_eq!(
            tr(&["wow", "console-send", "--command", "server info"]).argv,
            vec!["console", "server info"]
        );
    }

    #[test]
    fn the_raw_config_trio_is_renamed_not_reshaped() {
        assert_eq!(
            tr(&["wow", "config", "raw-read", "--file", "playerbots.conf"]).argv,
            vec!["config", "read", "playerbots.conf"]
        );
        assert_eq!(
            tr(&["wow", "config", "raw-write", "--file", "playerbots.conf"]).argv,
            vec!["config", "write", "playerbots.conf"]
        );
        // ...and raw-reset has no dml-wow arm at all, so it stays on bash.
        assert_eq!(tr(&["wow", "config", "raw-reset", "--file", "x"]).target, Target::Bash);
    }

    /// `wow bots list` and `wow bots flush` are different rows with different
    /// targets' shapes — longest-prefix lookup must not confuse them.
    #[test]
    fn lookup_takes_the_longest_matching_verb() {
        assert_eq!(tr(&["wow", "bots", "list", "--online"]).argv, vec!["bots", "--online"]);
        assert_eq!(
            tr(&["wow", "bots", "flush", "--yes", "--ack", "flush"]).argv,
            vec!["bots-flush", "--yes", "--ack", "flush"]
        );
    }

    #[test]
    fn a_dangling_value_less_flag_is_dropped_rather_than_emitted_alone() {
        // Never producible by the launcher, but a lone `--char` must not reach
        // a positional-shaped arm as a stray token.
        assert_eq!(tr(&["wow", "paperdoll", "--char"]).argv, vec!["paperdoll"]);
    }

    // -- table hygiene ---------------------------------------------------

    #[test]
    fn no_two_rows_share_a_verb() {
        let mut seen: Vec<&[&str]> = Vec::new();
        for r in TABLE {
            assert!(!seen.contains(&r.verb), "duplicate TABLE verb: {:?}", r.verb);
            seen.push(r.verb);
        }
    }

    #[test]
    fn every_row_classifies_its_own_sample_the_way_it_says() {
        for r in TABLE {
            let t = translate_for_arch(r.sample);
            assert_eq!(
                t.target, r.target,
                "row {:?} says {:?} but its own sample routes to {:?}",
                r.verb, r.target, t.target
            );
            assert!(is_classified(r.verb), "row {:?} is not is_classified", r.verb);
        }
    }

    #[test]
    fn a_dml_wow_row_never_leaves_a_bash_namespace_token_in_its_argv() {
        for r in TABLE {
            if r.target != Target::DmlWow {
                continue;
            }
            let t = translate_for_arch(r.sample);
            assert!(
                !matches!(t.argv.first().map(String::as_str), Some("wow") | Some("games")),
                "row {:?} still starts with a bash namespace: {:?}",
                r.verb,
                t.argv
            );
        }
    }

    /// Non-vacuity floor for the whole table — a TABLE that lost its rows would
    /// otherwise make every "for r in TABLE" test above pass by doing nothing.
    #[test]
    fn the_table_covers_the_launchers_surface() {
        let wow = TABLE.iter().filter(|r| r.target == Target::DmlWow).count();
        let bash = TABLE.iter().filter(|r| r.target == Target::Bash).count();
        // Raised to 70 when `games-list`/`games-status` land.
        assert!(wow >= 68, "expected >=68 dml-wow rows, got {wow}");
        assert!(bash >= 30, "expected >=30 bash rows, got {bash}");
        assert!(
            TABLE.iter().filter(|r| r.target == Target::DmlWow && r.streams).count() >= 12,
            "expected >=12 streaming dml-wow rows"
        );
    }
}
