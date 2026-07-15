use serde::{Deserialize, Serialize};

fn default_hint() -> String {
    String::new()
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ErrorInfo {
    pub code: String,
    pub message: String,
    #[serde(default = "default_hint")]
    pub hint: String,
}

#[derive(Debug, Deserialize)]
pub struct Envelope {
    pub ok: bool,
    #[serde(default)]
    pub data: serde_json::Value,
    #[serde(default)]
    pub error: Option<ErrorInfo>,
}

pub fn parse_envelope(s: &str) -> Result<Envelope, String> {
    serde_json::from_str(s.trim())
        .map_err(|e| format!("unparseable dml output ({e}): {}", s.trim()))
}

/// wsl.exe relays the guest's UTF-8 bytes, but its OWN messages (bad distro,
/// WSL not installed) are UTF-16LE. Detect the NUL pattern and decode.
pub fn decode_wsl_output(bytes: &[u8]) -> String {
    let looks_utf16 = bytes.len() >= 2
        && bytes.len() % 2 == 0
        && bytes.iter().skip(1).step_by(2).filter(|b| **b == 0).count() > bytes.len() / 4;
    if looks_utf16 {
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ok_envelope() {
        let env = parse_envelope(r#"{"ok":true,"data":{"version":"3.0.0"}}"#).unwrap();
        assert!(env.ok);
        assert_eq!(env.data["version"], "3.0.0");
        assert!(env.error.is_none());
    }

    #[test]
    fn parses_error_envelope_with_default_hint() {
        let env = parse_envelope(
            r#"{"ok":false,"error":{"code":"NOT_FOUND","message":"Title not found: nope"}}"#,
        )
        .unwrap();
        assert!(!env.ok);
        let e = env.error.unwrap();
        assert_eq!(e.code, "NOT_FOUND");
        assert_eq!(e.hint, "");
    }

    #[test]
    fn garbage_is_err_and_carries_raw_text() {
        let err = parse_envelope("wsl: unknown distro").unwrap_err();
        assert!(err.contains("wsl: unknown distro"));
    }

    #[test]
    fn decodes_plain_utf8() {
        assert_eq!(decode_wsl_output(b"dml v3.0.0\n"), "dml v3.0.0\n");
    }

    #[test]
    fn decodes_utf16le_from_wsl_exe() {
        // "hi" as UTF-16LE
        let bytes: &[u8] = &[b'h', 0, b'i', 0];
        assert_eq!(decode_wsl_output(bytes), "hi");
    }
}
