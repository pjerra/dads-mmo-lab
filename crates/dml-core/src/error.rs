use serde::Serialize;

use crate::runner::RunnerError;

#[derive(Debug, Serialize)]
pub struct CmdError {
    pub code: String,
    pub message: String,
    pub hint: String,
}

impl From<RunnerError> for CmdError {
    fn from(e: RunnerError) -> Self {
        match e {
            RunnerError::Spawn(m) => CmdError {
                // Code stays WSL_SPAWN (frontend matches on it); the hint covers
                // both backends since this mapping has no runner context.
                code: "WSL_SPAWN".into(),
                message: m,
                hint: "Default mode: is WSL + the dml-arch distro present? (wsl -d dml-arch). Native mode (DML_BACKEND=native): are Git Bash and Docker Desktop installed and running?".into(),
            },
            RunnerError::BadOutput { raw } => CmdError {
                code: "CLI_BAD_OUTPUT".into(),
                message: raw,
                hint: "Is the dml CLI v3.0.0 installed? Run: powershell -File cli\\dev-install.ps1".into(),
            },
        }
    }
}

pub fn bad_arg(message: impl Into<String>) -> CmdError {
    CmdError { code: "BAD_ARG".into(), message: message.into(), hint: "Check the value and try again.".into() }
}

pub fn not_found_err(message: impl Into<String>, hint: impl Into<String>) -> CmdError {
    CmdError { code: "NOT_FOUND".into(), message: message.into(), hint: hint.into() }
}

pub fn io_internal_err(e: std::io::Error) -> CmdError {
    CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() }
}
