//! Docker Desktop's "open the dashboard on startup" switch.
//!
//! Starting the server with the engine down pops the Docker dashboard window
//! over whatever the user was doing. Switching from launching `Docker
//! Desktop.exe` to `docker desktop start -d` (see [`crate::engine::start_engine`])
//! did NOT fix that, because both routes start Docker Desktop and Docker Desktop
//! opens its dashboard by default. The only real control is Docker's own
//! setting, `OpenUIOnStartupDisabled`, in `%APPDATA%\Docker\settings-store.json`.
//!
//! The key name was verified against Docker Desktop's shipped binaries (19
//! occurrences), not recalled — a toggle that writes the wrong key is a toggle
//! that silently does nothing, which is worse than no toggle.
//!
//! # Why this edits someone else's config file, carefully
//!
//! This file belongs to Docker Desktop, not to us, and it affects every
//! container the user ever runs — not just DML. So:
//!
//! * every other key is preserved byte-for-byte in value (only the one key is
//!   added or changed),
//! * a missing or malformed file is REPORTED, never replaced with one of our
//!   own invention — Docker stores only non-default keys there, and fabricating
//!   a file risks dropping settings we never saw,
//! * the write is atomic (temp file + rename) so an interrupted write cannot
//!   leave the user with a truncated Docker config.
//!
//! Docker Desktop also rewrites this file itself while running, and reads it at
//! startup, so a change applies from the NEXT Docker Desktop start. The UI says
//! so rather than pretending it is instant.

use std::path::PathBuf;

/// The one Docker Desktop setting this module touches.
pub const OPEN_UI_KEY: &str = "OpenUIOnStartupDisabled";

/// Override for tests, so no test ever edits the developer's real Docker
/// config. (A test that can reach the real file is a test that will eventually
/// break someone's machine — that happened here once already, with a test that
/// stopped the real engine.)
pub const SETTINGS_ENV: &str = "DML_DOCKER_SETTINGS";

/// `%APPDATA%\Docker\settings-store.json`, or the override.
///
/// `None` when neither is resolvable (no `APPDATA`, e.g. a Linux CI box), which
/// the caller reports as "not supported here" rather than as a failure.
pub fn settings_path() -> Option<PathBuf> {
    settings_path_from(std::env::var_os(SETTINGS_ENV), std::env::var_os("APPDATA"))
}

/// The pure half, so the precedence rule is tested without mutating process
/// environment (which is racy under cargo's parallel test threads, and unsound
/// in recent Rust). An EMPTY override counts as unset — `${VAR:-default}`-style
/// blindness to empty values has bitten this repo before.
pub fn settings_path_from(
    override_var: Option<std::ffi::OsString>,
    appdata: Option<std::ffi::OsString>,
) -> Option<PathBuf> {
    if let Some(p) = override_var {
        if !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    let appdata = appdata?;
    if appdata.is_empty() {
        return None;
    }
    Some(PathBuf::from(appdata).join("Docker").join("settings-store.json"))
}

/// Read the current value out of the settings text.
///
/// `None` means the key is absent, which is Docker's way of saying "default" —
/// and the default is that the dashboard DOES open. The caller must therefore
/// treat absent as `false` (not disabled), never as unknown-so-assume-on.
pub fn read_open_ui_disabled(text: &str) -> Result<Option<bool>, String> {
    let v: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("Docker's settings file is not valid JSON: {e}"))?;
    let obj = v.as_object().ok_or_else(|| "Docker's settings file is not a JSON object".to_string())?;
    Ok(obj.get(OPEN_UI_KEY).and_then(|b| b.as_bool()))
}

/// Return the settings text with the one key set, every other key preserved.
///
/// Deliberately NOT a string edit: parsing and re-serialising is what guarantees
/// we cannot corrupt the file, and serde_json's object preserves the other
/// entries exactly. The re-serialised file is pretty-printed, which is the shape
/// Docker Desktop itself writes.
pub fn with_open_ui_disabled(text: &str, disabled: bool) -> Result<String, String> {
    let mut v: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("Docker's settings file is not valid JSON: {e}"))?;
    let obj = v
        .as_object_mut()
        .ok_or_else(|| "Docker's settings file is not a JSON object".to_string())?;
    obj.insert(OPEN_UI_KEY.to_string(), serde_json::Value::Bool(disabled));
    serde_json::to_string_pretty(&v).map_err(|e| format!("Could not serialise Docker's settings: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const REAL: &str = r#"{
  "AutoStart": false,
  "DesktopTerminalEnabled": true,
  "LastContainerdSnapshotterEnable": 1784843094,
  "SettingsVersion": 45,
  "UseContainerdSnapshotter": true
}"#;

    #[test]
    fn an_absent_key_means_dockers_default_which_is_that_the_dashboard_opens() {
        assert_eq!(read_open_ui_disabled(REAL).unwrap(), None);
    }

    #[test]
    fn the_key_is_read_back_when_present() {
        let patched = with_open_ui_disabled(REAL, true).unwrap();
        assert_eq!(read_open_ui_disabled(&patched).unwrap(), Some(true));
        let patched = with_open_ui_disabled(&patched, false).unwrap();
        assert_eq!(read_open_ui_disabled(&patched).unwrap(), Some(false));
    }

    /// THE property that matters: this is the user's Docker config, shared by
    /// every container they run. Losing a key here is a real-world regression
    /// far worse than the popup this feature exists to suppress.
    #[test]
    fn every_other_setting_survives_the_patch_with_its_exact_value() {
        let before: serde_json::Value = serde_json::from_str(REAL).unwrap();
        let after: serde_json::Value =
            serde_json::from_str(&with_open_ui_disabled(REAL, true).unwrap()).unwrap();
        for (k, v) in before.as_object().unwrap() {
            assert_eq!(after.get(k), Some(v), "setting {k} was altered or dropped");
        }
        // ...and exactly one key was added.
        assert_eq!(after.as_object().unwrap().len(), before.as_object().unwrap().len() + 1);
    }

    #[test]
    fn an_existing_value_is_replaced_not_duplicated() {
        let once = with_open_ui_disabled(REAL, true).unwrap();
        let twice = with_open_ui_disabled(&once, false).unwrap();
        let v: serde_json::Value = serde_json::from_str(&twice).unwrap();
        assert_eq!(v.as_object().unwrap().get(OPEN_UI_KEY), Some(&serde_json::Value::Bool(false)));
        assert_eq!(v.as_object().unwrap().len(), 6);
    }

    /// Malformed input must be reported, never "fixed" by writing a fresh file:
    /// Docker stores only non-default keys, so replacing a file we cannot parse
    /// would silently reset settings we never saw.
    #[test]
    fn a_malformed_file_is_refused_by_both_the_reader_and_the_writer() {
        assert!(read_open_ui_disabled("{not json").is_err());
        assert!(with_open_ui_disabled("{not json", true).is_err());
        assert!(read_open_ui_disabled("[]").is_err(), "a JSON array is not a settings object");
        assert!(with_open_ui_disabled("[]", true).is_err());
    }

    fn os(s: &str) -> std::ffi::OsString {
        std::ffi::OsString::from(s)
    }

    #[test]
    fn the_override_wins_so_no_test_can_reach_the_real_docker_config() {
        assert_eq!(
            settings_path_from(Some(os("/tmp/fake-settings.json")), Some(os("/appdata"))),
            Some(PathBuf::from("/tmp/fake-settings.json"))
        );
    }

    #[test]
    fn an_empty_override_counts_as_unset_not_as_an_empty_path() {
        // `${VAR:-default}`-style blindness to empty values has produced a
        // vacuous-pass in this repo before; an empty override here would send a
        // WRITE to path "".
        assert_eq!(
            settings_path_from(Some(os("")), Some(os("/appdata"))),
            Some(PathBuf::from("/appdata").join("Docker").join("settings-store.json"))
        );
    }

    #[test]
    fn no_appdata_and_no_override_is_unsupported_rather_than_a_guess() {
        assert_eq!(settings_path_from(None, None), None);
        assert_eq!(settings_path_from(None, Some(os(""))), None);
    }
}

/// What the UI needs to render the toggle honestly.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DashboardSetting {
    /// False on a machine with no Docker Desktop settings file (a Linux box, or
    /// Docker Desktop never launched). The UI hides the toggle rather than
    /// offering a control that cannot do anything.
    pub supported: bool,
    /// True = Docker has been told NOT to open its dashboard on startup.
    /// An ABSENT key is Docker's default, which opens the dashboard, so absent
    /// reads as false here.
    pub disabled: bool,
    /// Shown in the UI so the user can see exactly whose file we are editing.
    pub path: Option<String>,
}

/// Read the current setting. A missing FILE is "not supported" (Docker Desktop
/// has never written one), which is different from a file we cannot parse —
/// that is an error the user should see, because it means we will refuse to
/// write.
pub fn get() -> Result<DashboardSetting, String> {
    let Some(path) = settings_path() else {
        return Ok(DashboardSetting { supported: false, disabled: false, path: None });
    };
    let shown = path.display().to_string();
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(DashboardSetting { supported: false, disabled: false, path: Some(shown) })
        }
        Err(e) => return Err(format!("Could not read Docker's settings file: {e}")),
    };
    Ok(DashboardSetting {
        supported: true,
        disabled: read_open_ui_disabled(&text)?.unwrap_or(false),
        path: Some(shown),
    })
}

/// Write the setting, preserving every other key.
///
/// Refuses rather than creating the file: Docker stores only NON-DEFAULT keys
/// there, so a file we invent could silently reset settings we never saw. The
/// write is atomic, so an interrupted write cannot truncate the user's Docker
/// config.
pub fn set(disabled: bool) -> Result<(), String> {
    let path = settings_path().ok_or_else(|| {
        "Docker Desktop's settings file could not be located on this machine.".to_string()
    })?;
    let text = std::fs::read_to_string(&path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "Docker Desktop has not written a settings file yet — start Docker Desktop once, then try again.".to_string()
        } else {
            format!("Could not read Docker's settings file: {e}")
        }
    })?;
    let patched = with_open_ui_disabled(&text, disabled)?;
    crate::conf::atomic_write(&path, &patched)
        .map_err(|e| format!("Could not write Docker's settings file: {e}"))
}
