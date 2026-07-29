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
            // SHIP-LIST 4.3. This hint used to read "Run: powershell -File
            // cli\dev-install.ps1" — a file that exists only inside a git
            // checkout of this repo. The people who hit this error are, almost
            // by definition, the ones who do NOT have a checkout: a freshly
            // created distro arrives with the old bootstrap CLI that
            // `Install-DML.ps1` embeds, which does not speak this envelope.
            // The launcher now installs the matching CLI itself, so the hint
            // names that button. Version comes from the contract constant so
            // this copy cannot drift away from what the probe chain and the
            // setup command actually compare against.
            RunnerError::BadOutput { raw } => CmdError {
                code: "CLI_BAD_OUTPUT".into(),
                message: raw,
                // Both backends, same reason the Spawn arm covers both: this
                // mapping has no runner context, and a native-mode user has no
                // distro and no setup button to press.
                //
                // BOTH labels, because both states land here and this mapping
                // cannot tell them apart: an outdated CLI answers in plain text
                // and an absent one answers with nothing that parses. The
                // start screen reads "Update backend" for the first (the common
                // one -- Install-DML.ps1 still bootstraps v2.6.0) and "Set up
                // backend" for the second, so naming only one of them sent most
                // readers looking for a button that is not on their screen.
                //
                // The test that reads both labels out of `first-run.ts` and
                // fails if this copy stops matching them lives in the LAUNCHER
                // crate (`the_cli_bad_output_hint_names_the_buttons_first_run_actually_renders`
                // in launcher/src-tauri/src/lib.rs). It cannot live here: this
                // crate is the game-agnostic bottom layer and must not know the
                // launcher's source tree exists -- an `include_str!` reaching up
                // into launcher/src would red `cargo test -p dml-core` (ubuntu
                // CI included) the day a SvelteKit file is renamed. The launcher
                // crate depends on this one and already reads both sides
                // (`provision.rs` does the same with `api.ts`), so that is where
                // the coupling is real.
                hint: format!(
                    "The DML backend answered with something this launcher does not understand - usually an older CLI than the v{} it speaks. Default mode: press \"Update backend\" on the launcher's start screen (the same button reads \"Set up backend\" when there is no backend in there at all) to install the copy that shipped with it. Native mode (DML_BACKEND=native): point DML_SCRIPT at a matching cli/dml.",
                    crate::setup::EXPECTED_CLI_VERSION
                ),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// SHIP-LIST 4.3. This is the hint a STRANGER actually hits: their distro
    /// answered with something that is not our envelope, which on a fresh
    /// machine means the old bootstrap CLI `Install-DML.ps1` embeds. It used to
    /// read "Run: powershell -File cli\dev-install.ps1" — a file that exists
    /// only inside a git checkout of this repo, which by definition they do not
    /// have, and which had one developer's repo path hardcoded in it anyway.
    #[test]
    fn the_bad_output_hint_points_at_the_in_app_setup_not_a_dev_script() {
        let err = CmdError::from(RunnerError::BadOutput { raw: "not json".into() });
        assert_eq!(err.code, "CLI_BAD_OUTPUT");
        assert!(
            !err.hint.contains("dev-install"),
            "the hint still sends the user to a dev script they do not have: {}",
            err.hint
        );
        assert!(
            !err.hint.to_lowercase().contains("powershell -file"),
            "the hint still tells the user to run a script from a repo: {}",
            err.hint
        );
    }

    // The hint has to name the button that is actually on the user's screen,
    // and `first-run.ts` has TWO setup buttons whose labels differ by state
    // ("Update backend" for `cli_outdated`, "Set up backend" for `no_cli`).
    // BOTH of those machines produce `CLI_BAD_OUTPUT`, so the copy names both.
    // That coupling is pinned by a test that reads the labels out of
    // `first-run.ts` itself -- in the LAUNCHER crate, which legitimately spans
    // both sides. See the comment on the `BadOutput` arm above for why it is
    // not here.

    /// The version in the copy must come from the contract constant, not a
    /// literal: `EXPECTED_CLI_VERSION` is what the probe chain and the setup
    /// command both compare against, and a hint quoting a different number is
    /// how a user ends up chasing the wrong version.
    #[test]
    fn the_bad_output_hint_quotes_the_contract_version() {
        let err = CmdError::from(RunnerError::BadOutput { raw: "x".into() });
        assert!(
            err.hint.contains(crate::setup::EXPECTED_CLI_VERSION),
            "{}",
            err.hint
        );
    }

    /// Native mode has no distro and no "Set up backend" button to press, so a
    /// hint that ONLY names the WSL route strands those users — the same
    /// two-backends care the `Spawn` arm above already takes.
    #[test]
    fn the_bad_output_hint_still_says_something_to_a_native_mode_user() {
        let err = CmdError::from(RunnerError::BadOutput { raw: "x".into() });
        assert!(err.hint.contains("DML_BACKEND=native"), "{}", err.hint);
    }

    #[test]
    fn the_bad_output_message_carries_the_raw_output_through_unchanged() {
        // The hint explains; the message is evidence. Losing the raw output
        // would leave a bug report with nothing in it.
        let err = CmdError::from(RunnerError::BadOutput { raw: "<html>502</html>".into() });
        assert_eq!(err.message, "<html>502</html>");
    }
}
