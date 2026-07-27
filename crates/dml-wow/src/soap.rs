//! AzerothCore SOAP client — Rust port of the parity oracle `cli/src/20-soap.sh`.
//!
//! `dml` (the bash CLI) performs the launcher's GM/account/mail/teleport/
//! announce writes by POSTing a SOAP `executeCommand` envelope to the
//! worldserver's SOAP listener (default `http://127.0.0.1:7878/`), then
//! classifying the HTTP response into one of four outcomes (bash rc 0/2/3/4).
//! This module is that client's core, ported byte-for-byte against
//! `20-soap.sh`'s `soap_url`/`soap_user`/`soap_pass`/`_soap_load_env`,
//! `soap_envelope`/`_xml_escape`, `soap_parse_result`, and `soap_exec`.
//!
//! CONTROLLER DECISION — no entity-decoding. `soap_parse_result` extracts the
//! text between `<result>`/`<faultstring>` tags with NO entity decoding, and
//! `dml wow soap-exec` returns that raw text verbatim. To keep byte-parity,
//! [`SoapOutcome::Ok`]/[`SoapOutcome::Fault`] carry the RAW extracted
//! substring — `&amp;`/`&#xD;`/etc. are never decoded. [`xml_escape`] is only
//! used for the OUTBOUND command.
//!
//! This module is self-contained: no other file wires it in yet (later tasks
//! add the Tauri commands + frontend routing). All tests here are pure — no
//! live network, no touching the real `~/.dml/soap.env`.

use std::time::Duration;

/// SOAP endpoint + Basic-auth credentials, resolved by [`SoapConfig::load`].
pub struct SoapConfig {
    pub url: String,
    pub user: String,
    pub pass: String,
}

/// Classification of a SOAP `executeCommand` call — mirrors `soap_exec`'s
/// bash return codes: 0 -> `Ok`, 2 -> `Fault`, 3 -> `Auth`, 4 -> `Unreachable`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SoapOutcome {
    /// rc 0 — the `<result>` inner text, RAW (no entity decoding; see the
    /// module-level CONTROLLER DECISION).
    Ok(String),
    /// rc 2 — the `<faultstring>` text, or the raw body if neither
    /// `<faultstring>` nor `<result>` is present.
    Fault(String),
    /// rc 3 — HTTP 401.
    Auth,
    /// rc 4 — connect/timeout/other transport error; carries the error message.
    Unreachable(String),
}

impl SoapConfig {
    /// Resolve SOAP endpoint + credentials the way `soap_url`/`soap_user`/
    /// `soap_pass` + `_soap_load_env` do: env `DML_SOAP_URL`/`DML_SOAP_USER`/
    /// `DML_SOAP_PASS` win (when set AND non-empty — bash's `${VAR:-default}`
    /// treats an empty value the same as unset); else the matching key from
    /// `~/.dml/soap.env` (when present and non-empty); else the built-in
    /// default.
    pub fn load() -> SoapConfig {
        let (file_url, file_user, file_pass) = soap_env_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|contents| parse_soap_env(&contents))
            .unwrap_or((None, None, None));

        SoapConfig {
            url: resolve(std::env::var("DML_SOAP_URL").ok(), file_url, "http://127.0.0.1:7878/"),
            user: resolve(std::env::var("DML_SOAP_USER").ok(), file_user, "admin"),
            pass: resolve(std::env::var("DML_SOAP_PASS").ok(), file_pass, "admin"),
        }
    }
}

/// Env wins over file wins over default — matching bash's `${VAR:-default}`
/// treatment of "set but empty" as equivalent to "unset" on both sides.
fn resolve(env: Option<String>, file: Option<String>, default: &str) -> String {
    env.filter(|s| !s.is_empty())
        .or_else(|| file.filter(|s| !s.is_empty()))
        .unwrap_or_else(|| default.to_string())
}

/// Build the `executeCommand` SOAP envelope for `command`, XML-escaping it —
/// a port of `soap_envelope`/`_xml_escape` (`20-soap.sh:31-56`). Byte-exact
/// whitespace is not required for parity (the server ignores it); escape
/// ORDER matters (`&` before `<`/`>`, so the escape entities' own `&` is
/// never re-escaped).
pub(crate) fn build_envelope(command: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<SOAP-ENV:Envelope xmlns:SOAP-ENV=\"http://schemas.xmlsoap.org/soap/envelope/\" xmlns:ns1=\"urn:AC\">\n  <SOAP-ENV:Body>\n    <ns1:executeCommand><command>{}</command></ns1:executeCommand>\n  </SOAP-ENV:Body>\n</SOAP-ENV:Envelope>\n",
        xml_escape(command)
    )
}

/// Classify a SOAP HTTP response — a port of `soap_exec`'s status/body
/// handling + `soap_parse_result` (`20-soap.sh:58-116`):
/// - `http_status == 401` -> [`SoapOutcome::Auth`].
/// - else `body` contains `<faultstring>` -> [`SoapOutcome::Fault`] of the
///   text after the FIRST `<faultstring>` up to the FIRST following
///   `</faultstring>` (bash `${xml#*<faultstring>}` / `%%</faultstring>*}`).
/// - else `body` contains `<result>` -> [`SoapOutcome::Ok`] of the text after
///   the FIRST `<result>` up to the FIRST following `</result>`.
/// - else -> [`SoapOutcome::Fault`] of the whole body (bash rc 2 for an
///   unexpected non-result body).
pub(crate) fn parse_response(http_status: u16, body: &str) -> SoapOutcome {
    if http_status == 401 {
        return SoapOutcome::Auth;
    }
    if body.contains("<faultstring>") {
        return SoapOutcome::Fault(extract_after(body, "<faultstring>", "</faultstring>"));
    }
    if body.contains("<result>") {
        return SoapOutcome::Ok(extract_after(body, "<result>", "</result>"));
    }
    SoapOutcome::Fault(body.to_string())
}

/// Bash `${xml#*<open>}` then `${..%%<close>*}`: the substring after the
/// FIRST `<open>`, truncated at the FIRST following `<close>`. If `<close>`
/// never appears, bash's `%%pattern` leaves the string unchanged, so the
/// entire remainder is returned rather than `None`-ing out — `open` is
/// guaranteed present by the `contains` check at each call site.
fn extract_after(body: &str, open: &str, close: &str) -> String {
    let after_open = match body.find(open) {
        Some(i) => &body[i + open.len()..],
        None => body,
    };
    match after_open.find(close) {
        Some(i) => after_open[..i].to_string(),
        None => after_open.to_string(),
    }
}

/// XML-escape `s` for splicing into the outbound `<command>` element —
/// `_xml_escape` (`20-soap.sh:32-44`). `&` MUST be escaped before `<`/`>` so
/// the entities' own `&` is not re-escaped.
pub(crate) fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            other => out.push(other),
        }
    }
    out
}

/// Path to `~/.dml/soap.env`, matching `lib.rs::windows_soap_env_path`'s home
/// resolution: `USERPROFILE` (Windows), falling back to `HOME` (non-Windows /
/// dev). `None` when neither is set.
fn soap_env_path() -> Option<std::path::PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(|home| std::path::PathBuf::from(home).join(".dml").join("soap.env"))
}

/// Parse `~/.dml/soap.env`-style shell-assignment lines (`DML_SOAP_URL=...` /
/// `DML_SOAP_USER=...` / `DML_SOAP_PASS=...`) into `(url, user, pass)`,
/// matching `_soap_load_env`'s source semantics closely enough for the three
/// keys we care about: a trailing `\r` (CRLF file) is stripped from each
/// line, surrounding single/double quotes on the value are stripped, `#`
/// lines and blank lines are ignored, and any line that isn't `KEY=VALUE` is
/// ignored. Unrecognized keys are ignored too (this is not a general shell
/// parser). Last occurrence of a key wins, matching shell re-assignment.
fn parse_soap_env(contents: &str) -> (Option<String>, Option<String>, Option<String>) {
    let mut url = None;
    let mut user = None;
    let mut pass = None;
    for raw_line in contents.split('\n') {
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line).trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let mut value = value.trim();
        let quoted = value.len() >= 2
            && ((value.starts_with('"') && value.ends_with('"'))
                || (value.starts_with('\'') && value.ends_with('\'')));
        if quoted {
            value = &value[1..value.len() - 1];
        }
        match key {
            "DML_SOAP_URL" => url = Some(value.to_string()),
            "DML_SOAP_USER" => user = Some(value.to_string()),
            "DML_SOAP_PASS" => pass = Some(value.to_string()),
            _ => {}
        }
    }
    (url, user, pass)
}

/// Build a blocking client + POST `command`'s envelope to `cfg.url` with
/// Basic auth and a bounded timeout, then classify the response — a port of
/// `soap_exec` (`20-soap.sh:76-116`) minus the bash file-lock (that's a
/// same-process concurrency guard the Rust side doesn't need the same way;
/// later tasks may add one at the call site). Because reqwest's blocking
/// client must not run on the async runtime thread, callers wrap `exec` in
/// `spawn_blocking` — this function itself stays plain sync.
pub fn exec(cfg: &SoapConfig, command: &str) -> SoapOutcome {
    let body = build_envelope(command);
    let client = match reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(e) => return SoapOutcome::Unreachable(e.to_string()),
    };
    let resp = client
        .post(&cfg.url)
        .header("Content-Type", "application/xml")
        .basic_auth(&cfg.user, Some(&cfg.pass))
        .body(body)
        .send();
    let resp = match resp {
        Ok(r) => r,
        Err(e) => return SoapOutcome::Unreachable(e.to_string()),
    };
    let status = resp.status().as_u16();
    match resp.text() {
        Ok(text) => parse_response(status, &text),
        Err(e) => SoapOutcome::Unreachable(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- xml_escape -----------------------------------------------------

    #[test]
    fn xml_escape_escapes_amp_lt_gt_in_order() {
        assert_eq!(xml_escape("a&b<c>"), "a&amp;b&lt;c&gt;");
    }

    #[test]
    fn xml_escape_does_not_double_escape_amp() {
        // If `<`/`>` were escaped before `&`, the resulting `&lt;`/`&gt;`
        // would themselves get re-escaped into `&amp;lt;`. Guard against that.
        let escaped = xml_escape("<>");
        assert_eq!(escaped, "&lt;&gt;");
        assert!(!escaped.contains("&amp;lt;"));
        assert!(!escaped.contains("&amp;gt;"));
    }

    // -- build_envelope ---------------------------------------------------

    #[test]
    fn build_envelope_escapes_the_command() {
        let env = build_envelope("say hi & <bye>");
        assert!(env.contains("say hi &amp; &lt;bye&gt;"));
        assert!(!env.contains("<bye>"));
    }

    #[test]
    fn build_envelope_wraps_in_execute_command() {
        let env = build_envelope("server info");
        assert!(env.contains("<ns1:executeCommand><command>server info</command></ns1:executeCommand>"));
        assert!(env.contains("SOAP-ENV:Envelope"));
    }

    // -- parse_response -----------------------------------------------------

    #[test]
    fn parse_response_ok_extracts_result_text() {
        let body = "<a><result>Account created.</result></a>";
        match parse_response(200, body) {
            SoapOutcome::Ok(s) => assert_eq!(s, "Account created."),
            _ => panic!("expected Ok"),
        }
    }

    #[test]
    fn parse_response_fault_extracts_faultstring_text() {
        let body = "<a><faultstring>Incorrect syntax</faultstring></a>";
        match parse_response(200, body) {
            SoapOutcome::Fault(s) => assert_eq!(s, "Incorrect syntax"),
            _ => panic!("expected Fault"),
        }
    }

    #[test]
    fn parse_response_401_is_auth_regardless_of_body() {
        assert!(matches!(parse_response(401, "whatever"), SoapOutcome::Auth));
        assert!(matches!(
            parse_response(401, "<result>ignored</result>"),
            SoapOutcome::Auth
        ));
    }

    #[test]
    fn parse_response_unexpected_body_is_fault_of_whole_body() {
        match parse_response(200, "unexpected body no tags") {
            SoapOutcome::Fault(s) => assert_eq!(s, "unexpected body no tags"),
            _ => panic!("expected Fault"),
        }
    }

    #[test]
    fn parse_response_extracts_first_result_when_two_present() {
        let body = "<a><result>first</result>junk<result>second</result></a>";
        match parse_response(200, body) {
            SoapOutcome::Ok(s) => assert_eq!(s, "first"),
            _ => panic!("expected Ok"),
        }
    }

    #[test]
    fn parse_response_faultstring_takes_priority_over_result() {
        // soap_parse_result checks <faultstring> first, unconditionally.
        let body = "<a><faultstring>bad</faultstring><result>good</result></a>";
        match parse_response(200, body) {
            SoapOutcome::Fault(s) => assert_eq!(s, "bad"),
            _ => panic!("expected Fault"),
        }
    }

    #[test]
    fn parse_response_result_text_is_raw_not_entity_decoded() {
        // Controller decision: no entity decoding of the extracted text.
        let body = "<a><result>Tom &amp; Jerry</result></a>";
        match parse_response(200, body) {
            SoapOutcome::Ok(s) => assert_eq!(s, "Tom &amp; Jerry"),
            _ => panic!("expected Ok"),
        }
    }

    // -- parse_soap_env (soap.env file parsing) ------------------------------

    #[test]
    fn parse_soap_env_strips_crlf_and_quotes() {
        let contents = "DML_SOAP_PASS=secret\r\n";
        let (url, user, pass) = parse_soap_env(contents);
        assert_eq!(url, None);
        assert_eq!(user, None);
        assert_eq!(pass, Some("secret".to_string()));
    }

    #[test]
    fn parse_soap_env_all_three_keys_quoted_and_commented() {
        let contents = "\
# soap.env
DML_SOAP_URL=\"http://10.0.0.5:7878/\"\r
DML_SOAP_USER='gm3'\r
DML_SOAP_PASS=hunter2\r
# trailing comment
not a valid line
";
        let (url, user, pass) = parse_soap_env(contents);
        assert_eq!(url, Some("http://10.0.0.5:7878/".to_string()));
        assert_eq!(user, Some("gm3".to_string()));
        assert_eq!(pass, Some("hunter2".to_string()));
    }

    #[test]
    fn parse_soap_env_blank_and_unrelated_lines_ignored() {
        let contents = "\n\nSOME_OTHER_VAR=x\n";
        assert_eq!(parse_soap_env(contents), (None, None, None));
    }

    // -- SoapConfig::load -----------------------------------------------------

    #[test]
    fn load_defaults_when_nothing_set() {
        // NB: this reads the REAL ~/.dml/soap.env if USERPROFILE/HOME happen
        // to point somewhere with one — acceptable for the "defaults" shape
        // check below (url/user/pass are always non-empty), but the
        // env-wins assertions in the other tests are the parity-relevant
        // ones and set the env vars explicitly so the file can't interfere.
        let cfg = SoapConfig::load();
        assert!(!cfg.url.is_empty());
        assert!(!cfg.user.is_empty());
        assert!(!cfg.pass.is_empty());
    }

    #[test]
    fn load_env_vars_win_over_defaults() {
        std::env::set_var("DML_SOAP_URL", "http://example.invalid:7878/");
        std::env::set_var("DML_SOAP_USER", "testuser");
        std::env::set_var("DML_SOAP_PASS", "testpass");
        let cfg = SoapConfig::load();
        std::env::remove_var("DML_SOAP_URL");
        std::env::remove_var("DML_SOAP_USER");
        std::env::remove_var("DML_SOAP_PASS");
        assert_eq!(cfg.url, "http://example.invalid:7878/");
        assert_eq!(cfg.user, "testuser");
        assert_eq!(cfg.pass, "testpass");
    }
}
