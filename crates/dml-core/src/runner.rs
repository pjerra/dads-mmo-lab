use std::ffi::{OsStr, OsString};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::backend::Backend;
use crate::envelope::{decode_wsl_output, parse_envelope, Envelope};

#[derive(Debug)]
pub enum RunnerError {
    Spawn(String),
    BadOutput { raw: String },
}

impl std::fmt::Display for RunnerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // Backend-neutral: this error is shared by the WSL and native
            // runners; backend-specific wording comes from the runner's
            // host_label/host_hint at the sites that synthesize messages.
            RunnerError::Spawn(e) => write!(f, "failed to run dml: {e}"),
            RunnerError::BadOutput { raw } => write!(f, "dml produced unexpected output: {raw}"),
        }
    }
}

/// The WSL distro + Linux user the launcher talks to. Shared by the default
/// runner's invocation prefix and by anything else that needs to open a
/// shell into the same place (e.g. `open_shell` in lib.rs) -- keep these as
/// the single source of truth rather than re-hardcoding the strings.
pub const DISTRO: &str = "dml-arch";
pub const USER: &str = "dml";

pub struct DmlRunner {
    pub program: OsString,
    pub prefix_args: Vec<String>,
    /// A directory to prepend to the child's PATH, or None to inherit PATH
    /// unchanged. Set for the native (Docker Desktop) backend so the child
    /// `dml` finds `docker.exe` and its credential helpers, which live in the
    /// Docker Desktop bin dir that is NOT on the machine PATH for a per-user
    /// install. The WSL backend leaves this None (the distro has its own PATH).
    pub path_prepend: Option<OsString>,
    /// Short label of the host running dml, used in synthesized diagnostics
    /// ("wsl" vs "bash") — so native-mode errors never blame WSL
    /// (review finding, 2026-07-24).
    pub host_label: &'static str,
    /// Hint appended to synthesized crash diagnostics, matching the backend.
    pub host_hint: &'static str,
}

impl Default for DmlRunner {
    fn default() -> Self {
        DmlRunner {
            program: "wsl.exe".into(),
            prefix_args: ["-d", DISTRO, "-u", USER, "--", "dml"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            path_prepend: None,
            host_label: "wsl",
            host_hint: "Check WSL: wsl -d dml-arch",
        }
    }
}

/// Git Bash's `bash.exe`, for running the `dml` script natively on Windows.
/// `DML_BASH` overrides; otherwise the standard Git for Windows locations, then
/// a bare `bash` on PATH.
fn find_bash() -> OsString {
    if let Some(b) = std::env::var_os("DML_BASH") {
        if !b.is_empty() {
            return b;
        }
    }
    for c in [
        r"C:\Program Files\Git\bin\bash.exe",
        r"C:\Program Files\Git\usr\bin\bash.exe",
    ] {
        if Path::new(c).exists() {
            return OsString::from(c);
        }
    }
    OsString::from("bash")
}

/// The `dml` bash script to run in native mode. `DML_SCRIPT` points at it (the
/// repo's `cli/dml` in dev, a bundled copy in a release build); absent, we fall
/// back to a bare `dml` so the failure is an honest "not found" rather than a
/// silent wrong target.
fn find_dml_script() -> String {
    match std::env::var_os("DML_SCRIPT") {
        Some(s) if !s.is_empty() => s.to_string_lossy().into_owned(),
        _ => "dml".to_string(),
    }
}

/// The directory holding `docker.exe` (for PATH injection). None when docker is
/// only resolvable as a bare name on PATH — then the child already has it.
fn docker_bin_dir() -> Option<PathBuf> {
    let prog = crate::docker::docker_program();
    Path::new(&prog)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| p.to_path_buf())
}

/// PATH with `dir` prepended, or None when `dir` is empty. Pure, for testing.
fn prepend_path(dir: &OsStr, current: Option<OsString>) -> Option<OsString> {
    if dir.is_empty() {
        return None;
    }
    let mut paths = vec![PathBuf::from(dir)];
    if let Some(cur) = current {
        paths.extend(std::env::split_paths(&cur));
    }
    std::env::join_paths(paths).ok()
}

impl DmlRunner {
    /// Native (Docker Desktop) backend: run the `dml` bash script under Git Bash
    /// on Windows, with the Docker Desktop bin dir on PATH so its `docker` calls
    /// reach the engine. No `dml-arch` distro, no bash middleman inside WSL — the
    /// same `dml` program, just hosted on Windows against Docker Desktop. This is
    /// the "keep dml as the brain, drop the hand-built distro" path.
    pub fn native() -> Self {
        DmlRunner {
            program: find_bash(),
            prefix_args: vec![find_dml_script()],
            path_prepend: docker_bin_dir().map(|p| p.into_os_string()),
            host_label: "bash",
            host_hint: "Native mode: check Git Bash and Docker Desktop are installed and the engine is running",
        }
    }

    /// Construct the runner for the selected backend. Default stays WSL, so the
    /// app is unchanged until `DML_BACKEND=native` is set.
    pub fn for_backend(b: Backend) -> Self {
        match b {
            Backend::Wsl => Self::default(),
            Backend::Native => Self::native(),
        }
    }

    /// Apply the optional PATH prepend to a command about to be spawned.
    fn apply_env(&self, cmd: &mut Command) {
        if let Some(dir) = &self.path_prepend {
            if let Some(p) = prepend_path(dir, std::env::var_os("PATH")) {
                cmd.env("PATH", p);
            }
        }
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.prefix_args).args(args).arg("--json");
        self.apply_env(&mut cmd);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        cmd
    }

    fn command_raw(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.prefix_args).args(args);
        self.apply_env(&mut cmd);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        cmd
    }

    /// Interactive spawn for `games install`: raw text passthrough (no --json),
    /// stdin piped so the UI can answer installer prompts, stderr already
    /// merged by the CLI arm (2>&1).
    pub fn spawn_interactive(&self, args: &[&str]) -> Result<std::process::Child, RunnerError> {
        self.command_raw(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| RunnerError::Spawn(e.to_string()))
    }

    /// Non-interactive TEXT-mode capture (no `--json`): for CLI subcommands
    /// like `lan` and `doctor` that print plain, user-facing status lines
    /// (including their own error messages) straight to stdout/stderr rather
    /// than emitting a JSON envelope. Stdout and stderr are captured
    /// separately by the OS pipe and concatenated here (stdout first, then
    /// stderr if non-empty) -- true interleaving order isn't preserved, but
    /// these commands don't need it. Unlike `run_json`, a non-zero exit is
    /// NOT an error: the CLI's own failure text is the payload the caller
    /// wants to display, not a signal to synthesize a different message.
    ///
    /// SECURITY/ROBUSTNESS NOTE (review-mandated): there is no Rust-side
    /// timeout or output cap here. Callers MUST self-bound: only wire
    /// fixed, allowlisted CLI subcommands that are themselves fast and
    /// bounded (lan is quick; doctor's network probe self-caps at 5s). A
    /// future unbounded command wired through this seam would hang its IPC
    /// promise and grow memory without limit.
    pub fn run_captured(&self, args: &[&str]) -> Result<String, RunnerError> {
        let out = self
            .command_raw(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| RunnerError::Spawn(e.to_string()))?;
        let mut combined = decode_wsl_output(&out.stdout);
        let stderr = decode_wsl_output(&out.stderr);
        if !stderr.trim().is_empty() {
            if !combined.is_empty() && !combined.ends_with('\n') {
                combined.push('\n');
            }
            combined.push_str(&stderr);
        }
        Ok(combined)
    }

    pub fn run_json(&self, args: &[&str]) -> Result<Envelope, RunnerError> {
        let out = self
            .command(args)
            .output()
            .map_err(|e| RunnerError::Spawn(e.to_string()))?;
        self.finish_json(out)
    }

    fn finish_json(&self, out: std::process::Output) -> Result<Envelope, RunnerError> {
        let stdout = decode_wsl_output(&out.stdout);
        parse_envelope(&stdout).map_err(|parse_err| {
            if stdout.trim().is_empty() && !out.status.success() {
                let stderr = decode_wsl_output(&out.stderr);
                let stderr = stderr.trim();
                if stderr.is_empty() {
                    RunnerError::Spawn(format!(
                        "{} exited with code {} and no output ({})",
                        self.host_label,
                        out.status.code().unwrap_or(-1),
                        self.host_hint
                    ))
                } else {
                    RunnerError::Spawn(stderr.to_string())
                }
            } else {
                RunnerError::BadOutput { raw: parse_err }
            }
        })
    }

    /// NB: stdin is written to completion BEFORE the child's stdout is drained
    /// (`wait_with_output` starts reading only after `write_all` returns). Safe
    /// for children that consume all of stdin before emitting output (the dml
    /// raw-write contract), but a child that streams output while still reading
    /// a large stdin can deadlock both sides on full pipe buffers — if you add
    /// such a caller, move the write to a dedicated thread first.
    pub fn run_json_with_stdin(&self, args: &[&str], input: &str) -> Result<Envelope, RunnerError> {
        use std::io::Write;
        let mut child = self
            .command(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| RunnerError::Spawn(e.to_string()))?;
        {
            let mut stdin = child.stdin.take().expect("stdin piped above");
            stdin
                .write_all(input.as_bytes())
                .map_err(|e| RunnerError::Spawn(e.to_string()))?;
        } // dropping stdin closes it so the child sees EOF
        let out = child
            .wait_with_output()
            .map_err(|e| RunnerError::Spawn(e.to_string()))?;
        self.finish_json(out)
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
                    "hint": self.host_hint
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

    #[cfg(windows)]
    fn fixture_runner() -> DmlRunner {
        DmlRunner {
            program: "cmd.exe".into(),
            prefix_args: vec!["/C".into()],
            path_prepend: None,
            host_label: "wsl",
            host_hint: "Check WSL: wsl -d dml-arch",
        }
    }
    #[cfg(not(windows))]
    fn fixture_runner() -> DmlRunner {
        DmlRunner {
            program: "sh".into(),
            prefix_args: vec![],
            path_prepend: None,
            host_label: "wsl",
            host_hint: "Check WSL: wsl -d dml-arch",
        }
    }

    #[cfg(windows)]
    const FIXTURE_EXT: &str = "cmd";
    #[cfg(not(windows))]
    const FIXTURE_EXT: &str = "sh";

    fn fixture(name: &str) -> String {
        format!("{}/tests/fixtures/{}.{}", env!("CARGO_MANIFEST_DIR"), name, FIXTURE_EXT)
    }

    #[test]
    fn run_json_parses_ok_envelope() {
        let env = fixture_runner().run_json(&[&fixture("ok")]).unwrap();
        assert!(env.ok);
        assert_eq!(env.data["games"][0]["id"], "wow-server-playerbots");
    }

    #[test]
    fn run_json_returns_error_envelope_as_ok_false() {
        let env = fixture_runner().run_json(&[&fixture("err")]).unwrap();
        assert!(!env.ok);
        assert_eq!(env.error.unwrap().code, "NOT_FOUND");
    }

    #[test]
    fn run_json_garbage_is_bad_output() {
        match fixture_runner().run_json(&[&fixture("garbage")]) {
            Err(RunnerError::BadOutput { raw }) => assert!(raw.contains("not json")),
            other => panic!("expected BadOutput, got {other:?}"),
        }
    }

    #[test]
    fn run_json_empty_stdout_nonzero_exit_is_spawn_error() {
        match fixture_runner().run_json(&[&fixture("wsl_down")]) {
            Err(RunnerError::Spawn(msg)) => assert!(
                msg.contains("dml-arch"),
                "expected spawn message to mention dml-arch, got: {msg}"
            ),
            other => panic!("expected Spawn, got {other:?}"),
        }
    }

    #[test]
    fn run_json_bad_output_carries_parse_detail() {
        match fixture_runner().run_json(&[&fixture("garbage")]) {
            Err(RunnerError::BadOutput { raw }) => {
                assert!(raw.contains("not json"));
                assert!(raw.contains("unparseable"));
            }
            other => panic!("expected BadOutput, got {other:?}"),
        }
    }

    #[test]
    fn run_json_missing_program_is_spawn_error() {
        let r = DmlRunner { program: "definitely-not-a-real-exe-9f2.exe".into(), prefix_args: vec![], path_prepend: None, host_label: "wsl", host_hint: "" };
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
            .run_stream(&[&fixture("stream_ok")], |v| seen.push(v))
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
            .run_stream(&[&fixture("stream_crash")], |v| seen.push(v))
            .unwrap();
        assert_eq!(code, 3);
        let last = seen.last().unwrap();
        assert_eq!(last["event"], "error");
        assert_eq!(last["error"]["code"], "CLI_CRASH");
        assert!(last["error"]["message"].as_str().unwrap().contains("3"));
    }

    #[test]
    fn run_json_with_stdin_delivers_input_to_the_child() {
        let env = fixture_runner()
            .run_json_with_stdin(&[&fixture("stdin_echo")], "hello world")
            .unwrap();
        assert!(env.ok);
        assert_eq!(env.data["echo"], "hello world");
    }

    #[test]
    fn spawn_interactive_round_trips_stdin() {
        use std::io::{Read, Write};
        let r = fixture_runner();
        let mut child = r.spawn_interactive(&[&fixture("interactive_echo")]).unwrap();
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(b"hello\r\n").unwrap();
        drop(stdin);
        let mut out = String::new();
        child.stdout.take().unwrap().read_to_string(&mut out).unwrap();
        let status = child.wait().unwrap();
        assert!(out.contains("answer me:"));
        assert!(out.contains("you typed hello"));
        assert_eq!(status.code(), Some(0));
    }

    #[test]
    fn run_captured_combines_stdout_and_stderr_regardless_of_exit_code() {
        let text = fixture_runner()
            .run_captured(&[&fixture("captured_mixed")])
            .unwrap();
        assert!(text.contains("stdout line"), "missing stdout in: {text:?}");
        assert!(text.contains("stderr line"), "missing stderr in: {text:?}");
    }

    #[test]
    fn run_captured_missing_program_is_spawn_error() {
        let r = DmlRunner { program: "definitely-not-a-real-exe-9f2.exe".into(), prefix_args: vec![], path_prepend: None, host_label: "wsl", host_hint: "" };
        assert!(matches!(r.run_captured(&["x"]), Err(RunnerError::Spawn(_))));
    }

    #[test]
    fn default_runner_uses_distro_and_user_constants() {
        let r = DmlRunner::default();
        assert!(r.prefix_args.contains(&DISTRO.to_string()));
        assert!(r.prefix_args.contains(&USER.to_string()));
    }

    #[test]
    fn default_runner_has_no_path_prepend() {
        assert!(DmlRunner::default().path_prepend.is_none());
    }

    #[test]
    fn for_backend_wsl_is_the_default_wsl_runner() {
        let r = DmlRunner::for_backend(Backend::Wsl);
        assert_eq!(r.program, OsString::from("wsl.exe"));
        assert!(r.path_prepend.is_none());
    }

    #[test]
    fn native_runner_runs_dml_script_under_bash() {
        // Deterministic via the documented overrides.
        std::env::set_var("DML_BASH", r"C:\fake\bash.exe");
        std::env::set_var("DML_SCRIPT", "C:/repo/cli/dml");
        let r = DmlRunner::for_backend(Backend::Native);
        std::env::remove_var("DML_BASH");
        std::env::remove_var("DML_SCRIPT");
        assert_eq!(r.program, OsString::from(r"C:\fake\bash.exe"));
        assert_eq!(r.prefix_args, vec!["C:/repo/cli/dml".to_string()]);
    }

    #[test]
    fn prepend_path_puts_docker_dir_first() {
        let p = prepend_path(OsStr::new(r"C:\docker\bin"), Some(OsString::from(r"C:\win")))
            .unwrap();
        let s = p.to_string_lossy();
        assert!(s.starts_with(r"C:\docker\bin"));
        assert!(s.contains(r"C:\win"));
    }

    #[test]
    fn prepend_path_empty_dir_is_none() {
        assert!(prepend_path(OsStr::new(""), Some(OsString::from(r"C:\win"))).is_none());
    }

    #[test]
    fn run_stream_wraps_non_json_lines_as_warn() {
        // garbage.cmd prints a non-JSON line and exits 0 → wrapped line + CLI_CRASH-free
        let mut seen: Vec<serde_json::Value> = vec![];
        let code = fixture_runner()
            .run_stream(&[&fixture("garbage")], |v| seen.push(v))
            .unwrap();
        assert_eq!(code, 0);
        assert_eq!(seen[0]["event"], "line");
        assert_eq!(seen[0]["level"], "warn");
        assert!(seen[0]["text"].as_str().unwrap().contains("not json"));
    }
}
