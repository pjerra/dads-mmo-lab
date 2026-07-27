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
}
