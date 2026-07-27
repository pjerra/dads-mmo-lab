//! Dispatch a parsed [`crate::cli::Cmd`] to its `dml-wow` call and print the
//! resulting envelope via `out.rs`.
//!
//! Kept deliberately thin: one match arm per subcommand, each calling
//! exactly one `dml-wow`/`dml-core` function and handing the result to
//! [`crate::out::emit_ok`]/[`crate::out::emit_err`]. No business logic lives
//! here — if a subcommand needs more than "call one function, emit its
//! result", that logic belongs in `dml-wow`.
//!
//! Title/config resolution mirrors the launcher's native-mode Tauri commands
//! EXACTLY (`wow_server_info_read`/`wow_server_detail_read` in
//! `launcher/src-tauri/src/lib.rs`): the same `SoapConfig::load()`,
//! `DbConfig::from_env()`, `ConfigReader::from_env()`, and
//! `dml_core::engine::docker_program()` calls, so the CLI and the GUI agree
//! on where a title/its DB/its SOAP endpoint live without a second config
//! path to keep in sync.

use dml_core::engine::docker_program;
use dml_wow::config::ConfigReader;
use dml_wow::db::DbConfig;
use dml_wow::soap::SoapConfig;
use serde_json::json;

use crate::cli::Cmd;
use crate::out::{emit_err, emit_ok};

/// Run one parsed subcommand to completion, printing exactly one envelope on
/// stdout, and return the process exit code (0 ok / 1 error).
pub fn dispatch(command: Cmd) -> i32 {
    match command {
        Cmd::Version => emit_ok(json!({
            "version": env!("CARGO_PKG_VERSION"),
            "contract": "dml-json-v3",
            "backend": "native",
        })),

        Cmd::Status => {
            let program = docker_program();
            let soap_cfg = SoapConfig::load();
            let db_cfg = DbConfig::from_env();
            let mut reader = ConfigReader::from_env();
            let detail = dml_wow::status::read_server_detail(&program, &soap_cfg, &db_cfg, &mut reader);
            emit_ok(detail)
        }

        Cmd::ServerInfo => {
            let soap_cfg = SoapConfig::load();
            match dml_wow::status::read_server_info(&soap_cfg) {
                Ok(info) => emit_ok(info),
                // `read_server_info` returns `Err(())` for exactly one case:
                // a SOAP AUTH failure (HTTP 401 -- wrong `~/.dml/soap.env`
                // credentials). A down/unreachable server is NOT this arm --
                // it folds into `Ok(server_info_down())` (`{"online":false,
                // ...}` is itself the answer; see `dml_wow::status`'s and
                // `wow_server_info_read`'s doc comments). So this mirrors the
                // launcher's own mapping (`lib.rs::wow_server_info_read`)
                // exactly, code/message/hint included, rather than a
                // "server unreachable" message that would be actively wrong
                // here (the server can be perfectly up with bad creds).
                Err(()) => emit_err(
                    "SOAP_AUTH",
                    "SOAP authentication failed",
                    "Check ~/.dml/soap.env",
                ),
            }
        }

        Cmd::ConsoleTail { lines } => {
            let program = docker_program();
            let tail = dml_wow::status::read_console_tail(&program, lines);
            emit_ok(tail)
        }
    }
}
