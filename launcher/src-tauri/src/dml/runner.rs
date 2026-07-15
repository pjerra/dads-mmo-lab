use std::ffi::OsString;
use std::process::Command;

use super::envelope::{decode_wsl_output, parse_envelope, Envelope};

#[derive(Debug)]
pub enum RunnerError {
    Spawn(String),
    BadOutput { raw: String },
}

impl std::fmt::Display for RunnerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunnerError::Spawn(e) => write!(f, "failed to run dml via WSL: {e}"),
            RunnerError::BadOutput { raw } => write!(f, "dml produced unexpected output: {raw}"),
        }
    }
}

pub struct DmlRunner {
    pub program: OsString,
    pub prefix_args: Vec<String>,
}

impl Default for DmlRunner {
    fn default() -> Self {
        DmlRunner {
            program: "wsl.exe".into(),
            prefix_args: ["-d", "dml-arch", "-u", "dml", "--", "dml"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
        }
    }
}

impl DmlRunner {
    fn command(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.prefix_args).args(args).arg("--json");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        cmd
    }

    pub fn run_json(&self, args: &[&str]) -> Result<Envelope, RunnerError> {
        let out = self
            .command(args)
            .output()
            .map_err(|e| RunnerError::Spawn(e.to_string()))?;
        let stdout = decode_wsl_output(&out.stdout);
        parse_envelope(&stdout).map_err(|_| RunnerError::BadOutput { raw: stdout.clone() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_runner() -> DmlRunner {
        DmlRunner {
            program: "cmd.exe".into(),
            prefix_args: vec!["/C".into()],
        }
    }

    fn fixture(name: &str) -> String {
        format!("{}/tests/fixtures/{}", env!("CARGO_MANIFEST_DIR"), name)
    }

    #[test]
    fn run_json_parses_ok_envelope() {
        let env = fixture_runner().run_json(&[&fixture("ok.cmd")]).unwrap();
        assert!(env.ok);
        assert_eq!(env.data["games"][0]["id"], "wow-server-playerbots");
    }

    #[test]
    fn run_json_returns_error_envelope_as_ok_false() {
        let env = fixture_runner().run_json(&[&fixture("err.cmd")]).unwrap();
        assert!(!env.ok);
        assert_eq!(env.error.unwrap().code, "NOT_FOUND");
    }

    #[test]
    fn run_json_garbage_is_bad_output() {
        match fixture_runner().run_json(&[&fixture("garbage.cmd")]) {
            Err(RunnerError::BadOutput { raw }) => assert!(raw.contains("not json")),
            other => panic!("expected BadOutput, got {other:?}"),
        }
    }

    #[test]
    fn run_json_missing_program_is_spawn_error() {
        let r = DmlRunner { program: "definitely-not-a-real-exe-9f2.exe".into(), prefix_args: vec![] };
        assert!(matches!(r.run_json(&["x"]), Err(RunnerError::Spawn(_))));
    }

    #[test]
    fn default_runner_targets_wsl_dml() {
        let r = DmlRunner::default();
        assert_eq!(r.program, std::ffi::OsString::from("wsl.exe"));
        assert_eq!(r.prefix_args, vec!["-d", "dml-arch", "-u", "dml", "--", "dml"]);
    }
}
