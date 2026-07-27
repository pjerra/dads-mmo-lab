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
        #[arg(long, default_value_t = 200)]
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
}
