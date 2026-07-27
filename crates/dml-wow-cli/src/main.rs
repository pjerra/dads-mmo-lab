//! `dml-wow` — the DML per-game CLI for the WoW (AzerothCore + playerbots)
//! server. Every subcommand prints EXACTLY ONE JSON envelope (or an NDJSON
//! event stream ending in a terminal event) on stdout; see `out.rs` for the
//! contract and `run.rs` for the dispatch table.
//!
//! Env vars are the interface for title/DB/SOAP config (`DML_GAMES_DIR`,
//! `DML_SOAP_URL`/`DML_SOAP_USER`/`DML_SOAP_PASS`, `~/.dml/soap.env`, ...) --
//! deliberately no config file and no CLI flags for any of that here; see
//! `run.rs`'s module doc comment for exactly which `dml-wow` readers resolve
//! them.
//!
//! Exit codes: `0` on an ok envelope, `1` on an error envelope, `2` on a
//! clap usage error (bad flags/unknown subcommand).

mod cli;
mod out;
mod run;

use clap::error::ErrorKind;
use clap::Parser;

use cli::Cli;

fn main() {
    match Cli::try_parse() {
        Ok(parsed) => std::process::exit(run::dispatch(parsed.command)),
        Err(err) => std::process::exit(handle_parse_error(err)),
    }
}

/// clap failed to parse argv. `--help`/`--version` are not usage errors --
/// clap's own `Error::exit()` already prints them to the right stream (stdout)
/// and exits 0, so let it. Everything else (bad flags, unknown subcommand,
/// missing required arg, ...) is a real usage error: print clap's own
/// message to stderr for a human reading the terminal, AND a `BAD_ARGS`
/// error envelope to stdout for a machine reading the pipe, then exit 2.
fn handle_parse_error(err: clap::Error) -> i32 {
    if matches!(err.kind(), ErrorKind::DisplayHelp | ErrorKind::DisplayVersion) {
        err.exit();
    }
    eprintln!("{err}");
    let first_line = err.to_string().lines().next().unwrap_or("").to_string();
    let envelope = dml_core::envelope::error_envelope("BAD_ARGS", &first_line, "dml-wow --help");
    println!("{envelope}");
    2
}
