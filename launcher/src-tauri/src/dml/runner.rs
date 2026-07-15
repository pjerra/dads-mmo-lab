use std::ffi::OsString;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

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
        parse_envelope(&stdout).map_err(|parse_err| {
            if stdout.trim().is_empty() && !out.status.success() {
                let stderr = decode_wsl_output(&out.stderr);
                let stderr = stderr.trim();
                if stderr.is_empty() {
                    RunnerError::Spawn(format!(
                        "wsl exited with code {} and no output",
                        out.status.code().unwrap_or(-1)
                    ))
                } else {
                    RunnerError::Spawn(stderr.to_string())
                }
            } else {
                RunnerError::BadOutput { raw: parse_err }
            }
        })
    }

    pub fn run_stream(
        &self,
        args: &[&str],
        mut on_event: impl FnMut(serde_json::Value),
    ) -> Result<i32, RunnerError> {
        let mut child = self
            .command(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| RunnerError::Spawn(e.to_string()))?;

        let stdout = child.stdout.take().expect("stdout piped above");
        let mut saw_terminal = false;
        for line in BufReader::new(stdout).split(b'\n') {
            let bytes = line.map_err(|e| RunnerError::Spawn(e.to_string()))?;
            let text = decode_wsl_output(&bytes);
            let text = text.trim_end_matches('\r').trim();
            if text.is_empty() {
                continue;
            }
            let value: serde_json::Value = match serde_json::from_str(text) {
                Ok(v) => v,
                Err(_) => serde_json::json!({"event":"line","level":"warn","text": text}),
            };
            if is_terminal(&value) {
                saw_terminal = true;
            }
            on_event(value);
        }

        let status = child.wait().map_err(|e| RunnerError::Spawn(e.to_string()))?;
        let code = status.code().unwrap_or(-1);
        if code != 0 && !saw_terminal {
            on_event(serde_json::json!({
                "event": "error",
                "error": {
                    "code": "CLI_CRASH",
                    "message": format!("dml exited with code {code} before finishing"),
                    "hint": "Check WSL: wsl -d dml-arch"
                }
            }));
        }
        Ok(code)
    }
}

fn is_terminal(v: &serde_json::Value) -> bool {
    matches!(v["event"].as_str(), Some("done") | Some("error"))
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
    fn run_json_empty_stdout_nonzero_exit_is_spawn_error() {
        match fixture_runner().run_json(&[&fixture("wsl_down.cmd")]) {
            Err(RunnerError::Spawn(msg)) => assert!(
                msg.contains("dml-arch"),
                "expected spawn message to mention dml-arch, got: {msg}"
            ),
            other => panic!("expected Spawn, got {other:?}"),
        }
    }

    #[test]
    fn run_json_bad_output_carries_parse_detail() {
        match fixture_runner().run_json(&[&fixture("garbage.cmd")]) {
            Err(RunnerError::BadOutput { raw }) => {
                assert!(raw.contains("not json"));
                assert!(raw.contains("unparseable"));
            }
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

    #[test]
    fn run_stream_forwards_events_in_order() {
        let mut seen: Vec<serde_json::Value> = vec![];
        let code = fixture_runner()
            .run_stream(&[&fixture("stream_ok.cmd")], |v| seen.push(v))
            .unwrap();
        assert_eq!(code, 0);
        assert_eq!(seen.len(), 4);
        assert_eq!(seen[0]["event"], "section_start");
        assert_eq!(seen[3]["event"], "done");
        assert_eq!(seen[3]["data"]["state"], "running");
    }

    #[test]
    fn run_stream_synthesizes_error_on_silent_crash() {
        let mut seen: Vec<serde_json::Value> = vec![];
        let code = fixture_runner()
            .run_stream(&[&fixture("stream_crash.cmd")], |v| seen.push(v))
            .unwrap();
        assert_eq!(code, 3);
        let last = seen.last().unwrap();
        assert_eq!(last["event"], "error");
        assert_eq!(last["error"]["code"], "CLI_CRASH");
        assert!(last["error"]["message"].as_str().unwrap().contains("3"));
    }

    #[test]
    fn run_stream_wraps_non_json_lines_as_warn() {
        // garbage.cmd prints a non-JSON line and exits 0 → wrapped line + CLI_CRASH-free
        let mut seen: Vec<serde_json::Value> = vec![];
        let code = fixture_runner()
            .run_stream(&[&fixture("garbage.cmd")], |v| seen.push(v))
            .unwrap();
        assert_eq!(code, 0);
        assert_eq!(seen[0]["event"], "line");
        assert_eq!(seen[0]["level"], "warn");
        assert!(seen[0]["text"].as_str().unwrap().contains("not json"));
    }
}
