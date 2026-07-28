//! Launcher-owned settings, persisted at `~/.dml/launcher.json`.
//!
//! This file exists because the launcher must know its backend BEFORE any
//! window exists: the tray shows server status while minimised, and every
//! Rust command needs the mode. Frontend `localStorage` — where every other
//! launcher preference lives — cannot answer that question at startup.
//!
//! Tolerance is deliberate and matches this directory's neighbours
//! (`soap.env`, `client-path`): a missing file is the normal first-run state,
//! and a corrupt one degrades to defaults rather than bricking startup.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

fn default_close_to_tray() -> bool {
    true
}

/// The persisted launcher settings. Every field is optional on disk; absent
/// means "work it out". `backend` is `auto` | `native` | `wsl` and records
/// INTENT, not a frozen answer — so a machine that gains Docker later
/// re-resolves correctly instead of going stale.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherConfig {
    #[serde(default)]
    pub backend: Option<String>,
    #[serde(default)]
    pub games_dir: Option<String>,
    #[serde(default)]
    pub dml_script: Option<String>,
    #[serde(default)]
    pub yq_bin: Option<String>,
    #[serde(default = "default_close_to_tray")]
    pub close_to_tray: bool,
    #[serde(default)]
    pub start_with_windows: bool,
    /// Which installed server the LIFECYCLE surfaces act on (Home's status
    /// card and its Start/Stop/Restart, the sidebar chip, the tray). Launcher
    /// state, not server state — which is why it lives here and NOT next to
    /// the server like its display name does: "which server am I looking at"
    /// is a per-user, per-install view preference, whereas the name belongs to
    /// the directory. `None` = the user has never chosen; callers resolve a
    /// fallback rather than rendering nothing.
    #[serde(default)]
    pub active_game: Option<String>,
}

impl Default for LauncherConfig {
    fn default() -> Self {
        Self {
            backend: None,
            games_dir: None,
            dml_script: None,
            yq_bin: None,
            close_to_tray: true,
            start_with_windows: false,
            active_game: None,
        }
    }
}

/// `<dml_home>/launcher.json`.
pub fn config_path(dml_home: &Path) -> PathBuf {
    dml_home.join("launcher.json")
}

/// Read the settings. NEVER fails: an unreadable or unparseable file yields
/// defaults, because a broken config must not stop the app from starting.
pub fn load(dml_home: &Path) -> LauncherConfig {
    let Ok(raw) = std::fs::read_to_string(config_path(dml_home)) else {
        return LauncherConfig::default();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

/// Write the settings via temp-file + rename, so a crash mid-write cannot
/// leave a truncated config behind.
pub fn save(dml_home: &Path, cfg: &LauncherConfig) -> std::io::Result<()> {
    std::fs::create_dir_all(dml_home)?;
    let path = config_path(dml_home);
    let tmp = path.with_extension("json.tmp");
    let body = serde_json::to_string_pretty(cfg).map_err(std::io::Error::other)?;
    std::fs::write(&tmp, body)?;
    std::fs::rename(&tmp, &path)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Each test MUST pass a distinct name literal: cargo runs tests as threads
    // of ONE process, so the pid alone does not make these unique and two
    // tests sharing a name would remove_dir_all each other's directory mid-run.
    fn tmp_dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("dml-core-lcfg-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn missing_file_is_all_defaults() {
        let d = tmp_dir("missing");
        let cfg = load(&d);
        assert_eq!(cfg, LauncherConfig::default());
        assert_eq!(cfg.backend, None);
        assert!(cfg.close_to_tray, "close-to-tray defaults ON");
        assert!(!cfg.start_with_windows);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn corrupt_file_degrades_to_defaults() {
        let d = tmp_dir("corrupt");
        std::fs::write(config_path(&d), "{ this is not json").unwrap();
        assert_eq!(load(&d), LauncherConfig::default());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn partial_file_keeps_known_fields_and_defaults_the_rest() {
        let d = tmp_dir("partial");
        std::fs::write(config_path(&d), "{\"backend\":\"native\",\"unknownKey\":123}").unwrap();
        let cfg = load(&d);
        assert_eq!(cfg.backend.as_deref(), Some("native"));
        assert!(cfg.close_to_tray, "an absent field takes its default, not false");
        assert_eq!(cfg.games_dir, None);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn active_game_defaults_to_none_and_round_trips_as_camel_case() {
        let d = tmp_dir("activegame");
        assert_eq!(load(&d).active_game, None, "no stored choice on first run");
        let cfg = LauncherConfig {
            active_game: Some("wow-server-playerbots".into()),
            ..LauncherConfig::default()
        };
        save(&d, &cfg).unwrap();
        let raw = std::fs::read_to_string(config_path(&d)).unwrap();
        assert!(raw.contains("activeGame"), "on-disk key is camelCase: {raw}");
        assert_eq!(load(&d).active_game.as_deref(), Some("wow-server-playerbots"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn active_game_survives_a_write_that_changes_another_field() {
        // The active server and the tray/autostart preferences share one file:
        // a settings save that dropped the active game (or vice versa) would
        // silently re-point the lifecycle buttons at another server.
        let d = tmp_dir("activegame-merge");
        save(
            &d,
            &LauncherConfig { active_game: Some("maplestory-server".into()), ..Default::default() },
        )
        .unwrap();
        let mut cfg = load(&d);
        cfg.close_to_tray = false;
        save(&d, &cfg).unwrap();
        let got = load(&d);
        assert_eq!(got.active_game.as_deref(), Some("maplestory-server"));
        assert!(!got.close_to_tray);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_config_written_before_active_game_existed_still_loads() {
        // Forward/backward compatibility: an older launcher.json has no
        // activeGame key at all, and must load as "no choice yet" rather than
        // failing the whole parse back to defaults.
        let d = tmp_dir("activegame-legacy");
        std::fs::write(
            config_path(&d),
            "{\"backend\":\"wsl\",\"closeToTray\":false,\"startWithWindows\":true}",
        )
        .unwrap();
        let cfg = load(&d);
        assert_eq!(cfg.active_game, None);
        assert_eq!(cfg.backend.as_deref(), Some("wsl"), "the rest still parsed");
        assert!(!cfg.close_to_tray);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn round_trips_through_save_with_camel_case_keys() {
        let d = tmp_dir("roundtrip");
        let cfg = LauncherConfig {
            backend: Some("wsl".into()),
            games_dir: Some("C:/games".into()),
            dml_script: None,
            yq_bin: None,
            close_to_tray: false,
            start_with_windows: true,
            active_game: Some("wow-server-playerbots".into()),
        };
        save(&d, &cfg).unwrap();
        let raw = std::fs::read_to_string(config_path(&d)).unwrap();
        assert!(raw.contains("gamesDir"), "on-disk keys are camelCase: {raw}");
        assert!(raw.contains("startWithWindows"), "on-disk keys are camelCase: {raw}");
        assert_eq!(load(&d), cfg);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn save_leaves_no_temp_file_behind() {
        let d = tmp_dir("atomic");
        save(&d, &LauncherConfig::default()).unwrap();
        let strays: Vec<String> = std::fs::read_dir(&d)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".tmp"))
            .collect();
        assert!(strays.is_empty(), "temp file survived the rename: {strays:?}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn save_creates_the_home_dir_when_absent() {
        let d = tmp_dir("mkdir");
        let nested = d.join("does-not-exist-yet");
        save(&nested, &LauncherConfig::default()).unwrap();
        assert!(config_path(&nested).is_file());
        let _ = std::fs::remove_dir_all(&d);
    }
}
